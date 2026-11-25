// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! The editor and sync journal share one SQLite database. The sync cursor,
//! received batches, CRDT snapshots and their library projection are durable.

use anyhow::{Context, Result, ensure};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use loro::{ExportMode, Frontiers, LoroDoc, VersionVector};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use rusqlite::types::{Value, ValueRef};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::{collections::{HashMap, HashSet}, fs::{File, OpenOptions}, io::{Read, Write}, path::{Path, PathBuf}, time::Duration};
use crate::{editor::core::{Core, Pos, keep_outside_history}, model::Document, shared_store::SharedStore, store::{Meta, Store, SQLITE_FILE}};
use super::{AttachmentIssue, Changes, Edit, Exchange, NoteChange, SyncIssue, activity, assets, catalogue, content};
use super::history::History;
use super::retention;
use super::shared::{self, Access};

pub(super) mod recovery;
pub(super) mod conflict;

#[cfg(panic = "abort")]
compile_error!("CloudKit journal recovery requires panic unwinding");

pub struct Journal {
    db: Connection,
    _lock: File,
    _session: Option<crate::store::SessionLease>,
    root: PathBuf,
    peer: u64,
    account: String,
    channel: Option<String>,
    room: Option<shared::Scope>,
    access: Option<Access>,
    next_collection: std::cell::Cell<Option<std::time::Instant>>,
    merged_data_version: Option<i64>,
    retry_catalogue: bool,
    pub(super) apply_wait: super::ApplyWait,
    history: HashMap<String, History>,
}

struct Note {
    assets: HashSet<String>,
    snapshot: Vec<u8>,
    seen: i64,
    held: bool,
}

fn note(db: &Connection, id: &str) -> Result<Option<Note>> {
    db.query_row(
        "SELECT assets, snapshot, seen, held FROM notes WHERE id = ?1", [id],
        |row| Ok((row.get::<_, String>(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).optional()?.map(|(assets, snapshot, seen, held)| -> Result<Note> {
        Ok(Note { assets: serde_json::from_str(&assets)?, snapshot, seen, held })
    }).transpose()
}

fn checkpoint_note(db: &Connection, id: &str, note: &Note) -> Result<()> {
    db.execute("INSERT OR REPLACE INTO notes VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, serde_json::to_string(&note.assets)?, note.snapshot, note.seen, note.held])?;
    db.execute("DELETE FROM note_local_updates WHERE note = ?1", [id])?;
    Ok(())
}

fn note_updates(db: &Connection, id: &str) -> Result<Vec<Vec<u8>>> {
    Ok(db.prepare("SELECT data FROM note_local_updates WHERE note = ?1 ORDER BY rowid")?
        .query_map([id], |row| row.get::<_, Vec<u8>>(0))?.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn import_note_updates(db: &Connection, id: &str, loro: &LoroDoc) -> Result<()> {
    let updates = note_updates(db, id)?;
    if !updates.is_empty() {
        ensure!(loro.import_batch(&updates)?.pending.is_none(), "Incomplete local iCloud edit history");
    }
    Ok(())
}

fn open_note(db: &Connection, id: &str, snapshot: &[u8]) -> Result<LoroDoc> {
    let loro = open_replay_snapshot(snapshot)?;
    let updates = note_updates(db, id)?;
    if updates.is_empty() { return Ok(loro); }
    let (loro, status) = with_replay_candidate(loro, |doc| doc.import_batch(&updates))?;
    ensure!(status.pending.is_none(), "Incomplete local iCloud edit history");
    Ok(loro)
}

const NOTE_CHECKPOINT_EDITS: i64 = 64;
const NOTE_CHECKPOINT_BYTES: usize = 256 * 1024;

fn needs_note_checkpoint(db: &Connection, id: &str, update_bytes: usize) -> Result<bool> {
    let (count, size): (i64, i64) = db.query_row(
        "SELECT count(*), coalesce(sum(length(data)), 0) FROM note_local_updates WHERE note = ?1",
        [id], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(count + 1 >= NOTE_CHECKPOINT_EDITS || size as usize + update_bytes >= NOTE_CHECKPOINT_BYTES)
}

pub(super) fn has_conflict(db: &Connection, id: &str) -> Result<bool> {
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM sync_document_conflicts WHERE note = ?1)", [id], |row| row.get(0))?)
}

fn block_document(db: &Connection, id: &str) -> Result<()> {
    db.execute("INSERT INTO sync_document_conflicts (note, retry) VALUES (?1, 0)
        ON CONFLICT(note) DO UPDATE SET retry = 0", [id])?;
    Ok(())
}

fn get_state(db: &Connection, key: &str) -> Result<Option<Vec<u8>>> {
    Ok(db.query_row("SELECT value FROM state WHERE key = ?1", [key], |row| row.get(0)).optional()?)
}

pub(super) fn stored_catalogue(db: &Connection) -> Result<LoroDoc> {
    Ok(open_replay_snapshot(&get_state(db, "catalogue")?.context("Missing shared catalogue")?)?)
}

pub(super) fn record_shared_trash_purge(db: &Connection, requested: &[String], removed: &[String]) -> Result<()> {
    let doc = stored_catalogue(db)?;
    let peer: i64 = db.query_row("SELECT value FROM state WHERE key='peer'", [], |row| row.get(0))?;
    doc.set_peer_id(peer as u64)?;
    let before = doc.oplog_vv();
    let mut seen: catalogue::Catalogue = serde_json::from_slice(&get_state(db, "catalogue_seen")?.context("Missing catalogue projection")?)?;
    let mut purged: std::collections::BTreeSet<_> = removed.iter().cloned().collect();
    for id in requested.iter().filter(|id| removed.binary_search(id).is_err()) {
        if catalogue::permanently_deleted(&doc, id) { continue; }
        // Older builds could erase a cached Trash group before publishing its
        // deletion. Recover only items recorded as trashed and still absent.
        let was_trashed = seen.get(id).and_then(|entry| entry.get("life"))
            .is_some_and(|life| life.get("Trash").is_some());
        ensure!(was_trashed, "{}", crate::settings::tr(
            "共享回收站尚未同步或该项目已恢复，请刷新后重试。",
            "The shared Trash has not synced or the item was restored. Refresh and try again."));
        for (entry_id, entry) in &seen {
            let root = entry.get("life").and_then(|life| life.get("Trash"))
                .and_then(|trash| trash.get("root")).and_then(serde_json::Value::as_str);
            if entry_id != id && root != Some(id.as_str()) { continue; }
            let live: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM library_entries WHERE id=?1)", [entry_id], |row| row.get(0))?;
            if entry_id == id {
                ensure!(!live, "{}", crate::settings::tr("该共享项目已恢复。", "The shared item was restored."));
            }
            if !live { purged.insert(entry_id.clone()); }
        }
    }
    for id in purged {
        catalogue::capture_purge(db, &doc, &id)?;
        seen.remove(&id);
    }
    doc.commit();
    if doc.oplog_vv() != before {
        enqueue(db, catalogue::SCOPE, doc.export(ExportMode::updates(&before))?)?;
        put_state(db, "catalogue", &doc.export(ExportMode::Snapshot)?)?;
        put_state(db, "catalogue_seen", &serde_json::to_vec(&seen)?)?;
    }
    Ok(())
}

fn put_state(db: &Connection, key: &str, data: &[u8]) -> Result<()> {
    db.execute("INSERT OR REPLACE INTO state VALUES (?1, ?2)", params![key, data])?;
    Ok(())
}

const CATALOGUE_PAUSED: &str = "catalogue_replay_paused";
const CATALOGUE_REPLAY_UPGRADE: &str = "catalogue_replay_attempted_loro_1_18";
const DOCUMENT_REPLAY_UPGRADE: &str = "table_alias_projection_v2";

#[derive(Debug)]
enum ReplayFailure { Rejected, Panicked }

impl std::fmt::Display for ReplayFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Rejected => "Loro rejected the sync replay",
            Self::Panicked => "Loro panicked during sync replay",
        })
    }
}

impl std::error::Error for ReplayFailure {}

fn with_replay_candidate<T>(candidate: LoroDoc, run: impl FnOnce(&LoroDoc) -> Result<T, loro::LoroError>)
    -> Result<(LoroDoc, T), ReplayFailure>
{
    let failure = match catch_unwind(AssertUnwindSafe(|| run(&candidate))) {
        Ok(Ok(value)) => return Ok((candidate, value)),
        Ok(Err(_)) => ReplayFailure::Rejected,
        Err(_) => ReplayFailure::Panicked,
    };
    // Loro 1.17 can panic again when disposing of the last reference to a
    // poisoned document. Drop it only after the import unwind has finished.
    if catch_unwind(AssertUnwindSafe(|| drop(candidate))).is_err() {
        return Err(ReplayFailure::Panicked);
    }
    Err(failure)
}

fn open_replay_snapshot(snapshot: &[u8]) -> Result<LoroDoc, ReplayFailure> {
    let doc = content::configured();
    if snapshot.is_empty() { return Ok(doc); }
    with_replay_candidate(doc, |doc| doc.import(snapshot)).map(|(doc, _)| doc)
}

fn stage_catalogue(db: &Connection, peer: u64, personal: bool, writable: bool, paused: bool,
    local: &catalogue::Catalogue, seen: &catalogue::Catalogue, awaiting_predecessor: &mut bool) -> Result<Option<(LoroDoc, VersionVector)>>
{
    let snapshot = get_state(db, "catalogue")?.context("Missing catalogue snapshot")?;
    let (catalogue, before) = with_replay_candidate(LoroDoc::new(), |doc| {
        doc.import(&snapshot)?;
        doc.set_peer_id(peer)?;
        Ok(doc.oplog_vv())
    })?;
    if personal { super::organizer::capture(db, &catalogue)?; }
    if writable {
        catalogue::capture(db, &catalogue, seen, local)?;
        if !personal { catalogue::capture_shared_order(db, &catalogue, seen, local)?; }
    }
    catalogue.commit();
    if catalogue.oplog_vv() != before {
        enqueue(db, catalogue::SCOPE, catalogue.export(ExportMode::updates(&before))?)?;
    }

    let (records, edits) = incoming(db, catalogue::SCOPE)?;
    if edits.is_empty() {
        return Ok(Some((catalogue, before)));
    }
    let snapshot = catalogue.export(ExportMode::Snapshot)?;
    let (candidate, pending) = with_replay_candidate(LoroDoc::new(), |doc| {
        doc.import(&snapshot)?;
        let pending = doc.import_batch(&edits)?.pending.is_some();
        doc.set_peer_id(peer)?;
        Ok(pending)
    })?;
    if pending {
        *awaiting_predecessor = true;
        if paused { return Ok(None); }
        // Keep records with missing history byte-for-byte. Persist the causally
        // complete changes from this batch with the catalogue projection so a
        // lone gap cannot prevent unrelated folders and lists from updating.
        let applied = candidate.oplog_vv();
        for (record, edit) in records.iter().zip(&edits) {
            let end = LoroDoc::decode_import_blob_meta(edit, false)?.partial_end_vv;
            if applied.includes_vv(&end) {
                db.execute("DELETE FROM inbox WHERE id = ?1", [record])?;
            }
        }
    } else {
        db.execute("DELETE FROM inbox WHERE note = ?1", [catalogue::SCOPE])?;
    }
    Ok(Some((candidate, before)))
}

pub fn enqueue(db: &Connection, scope: &str, update: Vec<u8>) -> Result<()> {
    ensure!(!update.is_empty() && update.len() <= 16 * 1024 * 1024, "An iCloud edit exceeds the record size limit");
    db.execute("INSERT INTO outbox VALUES (?1, ?2, ?3)",
        params![uuid::Uuid::new_v4().to_string(), scope, update])?;
    Ok(())
}

fn incoming(db: &Connection, scope: &str) -> Result<(Vec<String>, Vec<Vec<u8>>)> {
    let mut statement = db.prepare("SELECT id, data FROM inbox WHERE note = ?1 ORDER BY rowid")?;
    let mut rows = statement.query([scope])?;
    let mut records = Vec::new();
    let mut updates = Vec::new();
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let mut data: Vec<u8> = row.get(1)?;
        // Existing libraries can have complete checkpoint envelopes in the inbox.
        normalize_checkpoint(db, &id, scope, &mut data)?;
        records.push(id);
        updates.push(data);
    }
    Ok((records, updates))
}

fn front(loro: &LoroDoc) -> String {
    loro.get_map("markdown").get("front").and_then(|v| v.into_value().ok())
        .and_then(|v| v.as_string().map(|s| s.as_str().to_owned())).unwrap_or_default()
}

fn set_front(loro: &LoroDoc, text: &str) -> Result<()> {
    if front(loro) != text { loro.get_map("markdown").insert("front", text)?; }
    Ok(())
}

fn attachment_warnings(db: &Connection, id: &str, count: usize) -> Result<bool> {
    let previous: Option<i64> = db.query_row("SELECT count FROM sync_attachment_warnings WHERE note = ?1",
        [id], |row| row.get(0)).optional()?;
    let count = count as i64;
    if count == 0 {
        db.execute("DELETE FROM sync_attachment_warnings WHERE note = ?1", [id])?;
    } else if previous != Some(count) {
        db.execute("INSERT OR REPLACE INTO sync_attachment_warnings VALUES (?1, ?2)", params![id, count])?;
    }
    Ok(count > previous.unwrap_or(0))
}

pub(super) const RECENT_VERSIONS: i64 = 6;

pub(super) fn archive(db: &Connection, id: &str, doc: &Document, front: &str, recovery: bool) -> Result<i64> {
    let xml = crate::docxml::serialize(doc);
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(xml.as_bytes())?;
    let compressed = encoder.finish()?;
    let body = if compressed.len() < xml.len() { Value::Blob(compressed) } else { Value::Text(xml.clone()) };
    // The legacy XML row and a new compressed row represent the same version.
    if recovery {
        db.execute("UPDATE sync_history SET recovery = 1
            WHERE note = ?1 AND front = ?3 AND (body = ?2 OR body = ?4) AND recovery = 0",
            params![id, body, front, xml])?;
    }
    let inserted = db.execute("INSERT OR IGNORE INTO sync_history (note, body, front, recovery) SELECT ?1, ?2, ?3, ?5
        WHERE NOT EXISTS (SELECT 1 FROM sync_history WHERE note = ?1 AND body = ?4 AND front = ?3)",
        params![id, body, front, xml, recovery])?;
    if inserted != 0 { return Ok(db.last_insert_rowid()); }
    Ok(db.query_row("SELECT sequence FROM sync_history WHERE note = ?1 AND front = ?3 AND (body = ?2 OR body = ?4)
        ORDER BY sequence DESC LIMIT 1", params![id, body, front, xml], |row| row.get(0))?)
}

fn trim_recent_versions(db: &Connection, id: &str) -> Result<()> {
    db.execute("DELETE FROM sync_history WHERE note = ?1 AND recovery = 0 AND sequence IN (
        SELECT sequence FROM sync_history WHERE note = ?1 AND recovery = 0
        ORDER BY sequence DESC LIMIT -1 OFFSET ?2)", params![id, RECENT_VERSIONS])?;
    Ok(())
}

fn normalize_checkpoint(db: &Connection, record: &str, scope: &str, data: &mut Vec<u8>) -> Result<()> {
    let (index, body) = retention::unpack(data)?;
    let header_size = data.len() - body.len();
    if !index.versions.is_empty() {
        let doc = open_replay_snapshot(body)?;
        for anchor in index.versions {
            ensure!(doc.oplog_vv().includes_vv(&anchor.version), "Checkpoint is missing a protected version");
            let branch = doc.fork_at(&doc.vv_to_frontiers(&anchor.version))?;
            let archived = content::render(&branch)?;
            for path in content::local_assets(&archived) {
                db.execute("INSERT OR IGNORE INTO cloud_checkpoint_paths VALUES (?1,?2)", params![record,path])?;
            }
            archive(db, scope, &archived, &front(&branch), anchor.recovery)?;
        }
        trim_recent_versions(db, scope)?;
    }
    if header_size != 0 { data.drain(..header_size); }
    Ok(())
}

fn archive_frontiers(loro: &LoroDoc, before: &VersionVector, updates: &[Vec<u8>]) -> Result<(Vec<Frontiers>, bool)> {
    let mut tips = updates.iter().map(|update| {
        let version = LoroDoc::decode_import_blob_meta(update, false)?.partial_end_vv;
        let tips = loro.vv_to_frontiers(&version);
        let tips = loro.minimize_frontiers(&tips).map_err(|_| anyhow::anyhow!("Missing iCloud history"))?;
        let full = loro.frontiers_to_vv(&tips).context("Missing iCloud history")?;
        Ok((tips, full))
    }).collect::<Result<Vec<_>>>()?;
    // Classify the full causal versions, not the partial vectors in the
    // payloads. A single update can rely on operations from other peers.
    tips.retain(|(_, version)| !before.includes_vv(version));
    tips.sort_by_cached_key(|(_, version)| version.values().map(|counter| i64::from(*counter)).sum::<i64>());
    let has_fork = tips.windows(2).any(|pair| !pair[1].1.includes_vv(&pair[0].1))
        || tips.iter().any(|(_, version)| !version.includes_vv(before));
    if !has_fork {
        let latest = loro.oplog_frontiers();
        tips.retain(|(tips, _)| *tips != latest);
        tips.dedup_by(|a, b| a.0 == b.0);
        tips.drain(..tips.len().saturating_sub(RECENT_VERSIONS as usize));
    }
    Ok((tips.into_iter().map(|(tips, _)| tips).collect(), has_fork))
}

fn archived_document(body: ValueRef<'_>) -> Result<Document> {
    if let ValueRef::Blob(data) = body {
        let mut xml = String::new();
        ZlibDecoder::new(data).read_to_string(&mut xml)?;
        return crate::docxml::parse(&xml);
    }
    crate::docxml::parse(body.as_str()?)
}

fn archived_body(db: &Connection, id: &str, sequence: i64) -> Result<(Document, String)> {
    let mut stmt = db.prepare("SELECT body, front FROM sync_history WHERE note = ?1 AND sequence = ?2")?;
    let mut rows = stmt.query(params![id, sequence])?;
    let row = rows.next()?.context("The recovery copy is missing.")?;
    Ok((archived_document(row.get_ref(0)?)?, row.get(1)?))
}

fn write_recovery_copy(db: &Connection, id: &str, doc: &mut Document, front: &str, mut meta: Meta) -> Result<()> {
    let title = crate::settings::display_title(&doc.title.text);
    doc.title = crate::model::RichText::plain(crate::localized_format!("{} · 恢复副本", "{} · recovered", title));
    let now = crate::time::now();
    meta.created = now;
    meta.modified = now;
    meta.name = (meta.markdown || !meta.kind.is_doc()).then(|| doc.title.text.clone());
    Store::write_sync_body(db, id, false, doc, front)?;
    db.execute("INSERT INTO library_entries VALUES (?1, ?2)", params![id, serde_json::to_string(&meta)?])?;
    Ok(())
}

pub(super) fn note_version(db: &Connection, id: &str) -> Result<Option<VersionVector>> {
    note(db, id)?.filter(|note| !note.snapshot.is_empty())
        .map(|note| open_note(db, id, &note.snapshot).map(|doc| doc.oplog_vv())).transpose()
}

pub fn archive_body(db: &Connection, id: &str, deleted_version: Option<&VersionVector>, markdown: bool, shared_peer: Option<u64>) -> Result<()> {
    let trashed = db.query_row("SELECT trashed FROM library_documents WHERE id = ?1", [id],
        |row| row.get::<_, bool>(0)).optional()?;
    if let Some(trashed) = trashed {
        let (doc, _, front) = Store::read_sync_body(db, id, trashed)?;
        let sequence = archive(db, id, &doc, &front, true)?;
        let held: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM notes WHERE id = ?1 AND held <> 0)", [id], |row| row.get(0))?;
        let after_delete = match deleted_version {
            Some(deleted) => note_version(db, id)?.is_some_and(|local| match shared_peer {
                Some(peer) => local.get(&peer).copied().unwrap_or(0) > deleted.get(&peer).copied().unwrap_or(0),
                None => !deleted.includes_vv(&local),
            }),
            None => db.query_row("SELECT EXISTS(SELECT 1 FROM outbox WHERE note = ?1)", [id], |row| row.get(0))?,
        };
        if held || after_delete {
            db.execute("INSERT OR IGNORE INTO offline_recoveries VALUES (?1, ?2, ?3, ?4)", params![id, sequence, doc.title.text, markdown])?;
        }
    }
    Ok(())
}

pub(super) fn archive_owner_ended(db: &Connection, room: &shared::Scope) -> Result<()> {
    if room.owner.is_some() || room.folder { return Ok(()); }
    let initialized: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name = 'offline_recoveries')", [], |r| r.get(0))?;
    if !initialized { return Ok(()); }
    let front: Option<String> = db.query_row("SELECT front FROM library_documents WHERE id = ?1 AND (
        EXISTS(SELECT 1 FROM outbox WHERE note = ?1) OR EXISTS(SELECT 1 FROM notes WHERE id = ?1 AND held <> 0))",
        [&room.note], |r| r.get(0)).optional()?;
    if let Some(front) = front { archive_body(db, &room.note, None, !front.is_empty(), None)?; }
    Ok(())
}

pub fn rebuild_body(db: &Connection, id: &str, trashed: bool) -> Result<bool> {
    let Some(old) = note(db, id)?.filter(|note| !note.held && !note.snapshot.is_empty()) else { return Ok(false); };
    let loro = open_note(db, id, &old.snapshot)?;
    let seen = Store::write_sync_body(db, id, trashed, &content::render(&loro)?, &front(&loro))?;
    db.execute("UPDATE notes SET seen = ?2 WHERE id = ?1", params![id, seen])?;
    Ok(true)
}

struct Capture<'a> {
    old: Note,
    loro: &'a LoroDoc,
    before: VersionVector,
}

fn finish_capture(db: &Connection, root: &Path, id: &str, doc: &Document,
    revision: (i64, &str), capture: Capture<'_>) -> Result<(bool, bool)> {
    let Capture { mut old, loro, before } = capture;
    let previously_held = old.held;
    let paths = content::local_assets(doc);
    old.seen = revision.0;
    loro.set_next_commit_origin("sync-metadata");
    let local_only = loro.get_map("local-only-attachments");
    let mut available = old.assets.clone();
    let mut skipped = 0;
    for source in paths.difference(&old.assets) {
        if let Some(problem) = assets::queue(db, root, source)? {
            skipped += 1;
            // A device that never received the file cannot replace the
            // diagnosis from the Mac that still has the original.
            if problem != assets::AssetProblem::Missing || local_only.get(source).is_none() {
                content::put(&local_only, source, problem.code())?;
            }
        } else {
            available.insert(source.clone());
            if local_only.get(source).is_some() { local_only.delete(source)?; }
        }
    }
    set_front(loro, revision.1)?;
    loro.commit();
    old.held = false;
    let mut checkpoint = old.snapshot.is_empty();
    if loro.oplog_vv() != before {
        let update = loro.export(ExportMode::updates(&before))?;
        if update.len() <= 16 * 1024 * 1024 {
            checkpoint = checkpoint || needs_note_checkpoint(db, id, update.len())?;
            if checkpoint {
                old.snapshot = loro.export(ExportMode::Snapshot)?;
            } else {
                // The outbox may be acknowledged before the next local checkpoint.
                db.execute("INSERT INTO note_local_updates (note, data) VALUES (?1, ?2)", params![id, update])?;
            }
            enqueue(db, id, update)?;
            old.assets = available.intersection(&paths).cloned().collect();
        } else { old.held = true; }
    }
    if checkpoint { checkpoint_note(db, id, &old)?; }
    else {
        db.execute("UPDATE notes SET assets = ?2, seen = ?3, held = ?4 WHERE id = ?1",
            params![id, serde_json::to_string(&old.assets)?, old.seen, old.held])?;
    }
    if old.held != previously_held { put_state(db, "catalogue_dirty", b"1")?; }
    Ok((attachment_warnings(db, id, skipped)? || (old.held && !previously_held), old.held))
}

fn capture_note(db: &Connection, root: &Path, peer: u64, id: &str, doc: &Document,
    seen: i64, markdown: &str) -> Result<bool> {
    let old = note(db, id)?.unwrap_or_else(|| Note { assets: HashSet::new(), snapshot: vec![], seen: 0, held: false });
    let loro = open_note(db, id, &old.snapshot)?;
    let before = loro.oplog_vv();
    content::edit(&loro, peer, doc)?;
    Ok(finish_capture(db, root, id, doc, (seen, markdown), Capture { old, loro: &loro, before })?.0)
}

enum DocumentApply {
    Applied(Option<NoteChange>),
    Held,
    AwaitingPredecessor,
    AwaitingAttachment,
    Deleted,
}

fn apply_document(db: &Connection, root: &Path, id: &str, shared: bool, positions: &[Pos]) -> Result<DocumentApply> {
    let old = note(db, id)?;
    if old.as_ref().is_some_and(|note| note.held) { return Ok(DocumentApply::Held); }
    let (_, updates) = incoming(db, id)?;
    let retry = has_conflict(db, id)?;
    if updates.is_empty() && !retry { return Ok(DocumentApply::Applied(None)); }
    if old.is_none() && updates.is_empty() { return Err(content::ConflictingBlockShapes.into()); }
    let loro = open_note(db, id, old.as_ref().map_or(&[], |note| note.snapshot.as_slice()))?;
    let before = loro.oplog_vv();
    let anchors: Vec<_> = positions.iter().filter_map(|&pos| content::anchor(&loro, pos).map(|anchor| (pos, anchor))).collect();
    let (loro, status) = with_replay_candidate(loro, |doc| doc.import_batch(&updates))?;
    if status.pending.is_some() { return Ok(DocumentApply::AwaitingPredecessor); }
    let changed = if loro.oplog_vv() != before || retry {
        let (doc, repaired) = content::project(&loro).map_err(|error| {
            crate::logging::write(&format!("Could not project synced document: {error:#}"));
            content::ConflictingBlockShapes
        })?;
        let merged_front = front(&loro);
        let mut paths = content::local_assets(&doc);
        if shared {
            for path in &paths { assets::path(root, path)?; }
        }
        let local_only = loro.get_map("local-only-attachments");
        let total = paths.len();
        paths.retain(|source| local_only.get(source).is_none());
        if !db.query_row("SELECT EXISTS(SELECT 1 FROM cloud_inventory_state)", [], |r| r.get::<_,bool>(0))?
            && paths.iter().any(|source| !assets::present(root, source)) { return Ok(DocumentApply::AwaitingAttachment); }
        attachment_warnings(db, id, total - paths.len())?;
        let (tips, has_fork) = archive_frontiers(&loro, &before, &updates)?;
        let protect_versions = has_fork || repaired && (retry || {
            let previous = loro.fork_at(&loro.vv_to_frontiers(&before))?;
            !content::project(&previous).is_ok_and(|(_, had_aliases)| had_aliases)
        });
        let trashed = db.query_row("SELECT trashed FROM library_documents WHERE id = ?1", [id],
            |row| row.get::<_, bool>(0)).optional()?;
        if let Some(trashed) = trashed {
            let (current, _, front) = Store::read_sync_body(db, id, trashed)?;
            if current != doc || front != merged_front { archive(db, id, &current, &front, protect_versions)?; }
        }
        // Preserve the sender's branch as well as our previous view.
        // A split or deleted block can hide text in the merged view.
        for tips in tips {
            if tips == loro.oplog_frontiers() { continue; }
            let branch = loro.fork_at(&tips)?;
            let candidate = content::render(&branch)?;
            let candidate_front = front(&branch);
            if candidate != doc || candidate_front != merged_front {
                archive(db, id, &candidate, &candidate_front, protect_versions)?;
            }
        }
        trim_recent_versions(db, id)?;
        let seen = Store::write_sync_body(db, id, trashed.unwrap_or(false), &doc, &merged_front)
            .context("Save merged cloud document")?;
        checkpoint_note(db, id, &Note { assets: paths, snapshot: loro.export(ExportMode::Snapshot)?, seen, held: false })?;
        let positions = anchors.iter().filter_map(|(pos, anchor)|
            content::resolve(&loro, pos.block, anchor).map(|resolved| (*pos, resolved))).collect();
        Some(NoteChange { id: id.into(), document: doc, positions })
    } else { None };
    if shared { activity::mark_projected(db, id)?; }
    db.execute("DELETE FROM inbox WHERE note = ?1", [id])?;
    Ok(DocumentApply::Applied(changed))
}

fn revision(db: &Connection, id: &str) -> Result<(i64, String)> {
    Ok(db.query_row("SELECT revision, front FROM library_documents WHERE id = ?1",
        [id], |row| Ok((row.get(0)?, row.get(1)?)))?)
}

pub(super) fn has_materialized_item(db: &Connection, id: &str) -> Result<bool> {
    let row = db.query_row("SELECT meta,
        EXISTS(SELECT 1 FROM library_documents WHERE id = ?1 AND trashed = 0)
        FROM library_entries WHERE id = ?1", [id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))).optional()?;
    let Some((meta, has_body)) = row else { return Ok(false); };
    Ok(has_body || serde_json::from_str::<crate::store::Meta>(&meta)?.folder.is_some())
}

impl Journal {
    pub fn open(root: &Path, channel: Option<String>) -> Result<Self> {
        let lock = OpenOptions::new().write(true).create(true).truncate(false).open(root.join("sync.lock"))?;
        lock.try_lock().context("This document library is already open")?;
        Self::open_locked(root, channel, lock)
    }

    pub(super) fn open_locked(root: &Path, channel: Option<String>, lock: std::fs::File) -> Result<Self> {
        let session = Store::session_lease(root);
        let marker = std::fs::read_to_string(root.join(".cloud-restore-ready"))?;
        let account = marker.lines().next().filter(|a| a.len() == 64).context("Invalid library account")?.to_string();
        let room = shared::descriptor(root)?.map(|room| room.scope);
        ensure!(room.is_some() == (marker.lines().nth(1) == Some("shared")) && (room.is_none() || channel.is_none()),
            "Shared document and personal library cannot use the same sync journal");
        let mut db = Connection::open(root.join(SQLITE_FILE))?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.execute_batch("
            PRAGMA synchronous = FULL;
            CREATE TABLE IF NOT EXISTS state (key TEXT PRIMARY KEY, value BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS notes (id TEXT PRIMARY KEY, assets TEXT NOT NULL, snapshot BLOB NOT NULL,
                seen INTEGER NOT NULL, held INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS note_local_updates (note TEXT NOT NULL, data BLOB NOT NULL);
            CREATE INDEX IF NOT EXISTS note_local_updates_by_note ON note_local_updates (note);
            CREATE TABLE IF NOT EXISTS outbox (id TEXT PRIMARY KEY, note TEXT NOT NULL, data BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS inbox (id TEXT PRIMARY KEY, note TEXT NOT NULL, data BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS sync_document_conflicts (note TEXT PRIMARY KEY, retry INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS sync_assets (path TEXT PRIMARY KEY, digest TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS sync_asset_repairs (path TEXT PRIMARY KEY);
            CREATE TABLE IF NOT EXISTS sync_attachment_warnings (note TEXT PRIMARY KEY, count INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS sync_asset_chunks (path TEXT NOT NULL, digest TEXT NOT NULL,
                part INTEGER NOT NULL, size INTEGER NOT NULL, data BLOB NOT NULL, PRIMARY KEY (path, digest, part));
            CREATE TABLE IF NOT EXISTS sync_history (sequence INTEGER PRIMARY KEY, note TEXT NOT NULL,
                body TEXT NOT NULL, front TEXT NOT NULL, time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                recovery INTEGER NOT NULL DEFAULT 1, UNIQUE(note, body, front));
            CREATE INDEX IF NOT EXISTS sync_history_by_note ON sync_history (note, sequence);
            CREATE TABLE IF NOT EXISTS offline_recoveries (
                note TEXT PRIMARY KEY, sequence INTEGER NOT NULL, title TEXT NOT NULL, markdown INTEGER NOT NULL);
        ")?;
        if room.is_none() { db.execute_batch("CREATE TABLE IF NOT EXISTS shared_recovery_resolutions (id TEXT PRIMARY KEY, copy TEXT)")?; }
        retention::schema(&db)?;
        let has_recovery = db.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('sync_history') WHERE name = 'recovery')",
            [], |row| row.get::<_, bool>(0))?;
        // Preexisting versions may include conflict copies. Protect all of
        // them when opening a library from a build that did not classify them.
        if !has_recovery { db.execute("ALTER TABLE sync_history ADD COLUMN recovery INTEGER NOT NULL DEFAULT 1", [])?; }
        if room.is_some() { activity::schema(&db)?; }
        let tx = db.transaction()?;
        let scope = room.as_ref().map(serde_json::to_vec).transpose()?;
        match get_state(&tx, "shared_scope")? {
            Some(saved) => ensure!(scope.as_ref() == Some(&saved), "Shared document scope changed"),
            None => if let Some(scope) = &scope { put_state(&tx, "shared_scope", scope)?; },
        }
        let access = if let Some(room) = &room {
            Some(get_state(&tx, "shared_access")?.map(|raw| serde_json::from_slice(&raw)).transpose()?
                .unwrap_or(if room.owner.is_none() { Access::Owner } else { Access::ReadOnly }))
        } else { None };
        match get_state(&tx, "protocol")?.as_deref() {
            Some(b"4") => {}
            None => {
                tx.execute("UPDATE state SET key = 'legacy_token' WHERE key = 'token'", [])?;
                put_state(&tx, "protocol", b"4")?;
            }
            Some(_) => anyhow::bail!("This library uses a newer sync protocol"),
        }
        let existing = tx.query_row("SELECT value FROM state WHERE key = 'peer'", [], |row| row.get::<_, i64>(0)).optional()?;
        let peer = if let Some(peer) = existing {
            let saved: String = tx.query_row("SELECT value FROM state WHERE key = 'channel'", [], |row| row.get(0))?;
            ensure!(saved == channel.as_deref().unwrap_or(""), "The sync test channel for this local library changed");
            peer
        } else {
            let peer = (uuid::Uuid::new_v4().as_u128() as i64 & i64::MAX).max(2);
            let ids = tx.prepare("SELECT id, trashed FROM library_documents ORDER BY id")?
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for (id, trashed) in ids {
                let (doc, seen, markdown) = Store::read_sync_body(&tx, &id, trashed)?;
                let loro = content::open(&content::seed(&doc)?)?;
                loro.set_peer_id(1)?;
                set_front(&loro, &markdown)?;
                ensure!(crate::docxml::serialize(&content::render(&loro)?) == crate::docxml::serialize(&doc),
                    "A library note could not round-trip through Loro");
                checkpoint_note(&tx, &id, &Note { assets: content::local_assets(&doc), snapshot: loro.export(ExportMode::Snapshot)?, seen, held: false })?;
            }
            let initial = catalogue::local(&tx)?;
            let loro = LoroDoc::new();
            loro.set_peer_id(1)?;
            catalogue::capture(&tx, &loro, &Default::default(), &initial)?;
            put_state(&tx, "catalogue", &loro.export(ExportMode::Snapshot)?)?;
            put_state(&tx, "catalogue_seen", &serde_json::to_vec(&initial)?)?;
            tx.execute("INSERT INTO state VALUES ('peer', ?1)", [peer])?;
            tx.execute("INSERT INTO state VALUES ('channel', ?1)", [channel.as_deref().unwrap_or("")])?;
            peer
        };
        tx.commit()?;
        let mut journal = Self { db, _lock: lock, _session: session, root: root.to_path_buf(), peer: peer as u64, account, channel, room, access,
            next_collection: Default::default(), merged_data_version: None, retry_catalogue: false, apply_wait: Default::default(), history: HashMap::new() };
        if let Err(error) = journal.collect_local() { crate::logging::write(&format!("Deferred local attachment cleanup: {error:#}")); }
        Ok(journal)
    }

    pub fn room(&self) -> Option<&shared::Scope> { self.room.as_ref() }

    pub fn access(&self) -> Option<Access> { self.access }

    pub(super) fn restrict_to_read_only(&mut self) -> Result<bool> {
        if !self.access.is_some_and(Access::can_write) { return Ok(false); }
        let tx = self.db.transaction()?;
        put_state(&tx, "shared_access", &serde_json::to_vec(&Access::ReadOnly)?)?;
        tx.commit()?;
        self.access = Some(Access::ReadOnly);
        Ok(true)
    }

    pub fn seed_shared(&mut self, doc: &Document, front: &str) -> Result<()> {
        let room = self.room.as_ref().context("Not a shared document")?;
        ensure!(self.access == Some(Access::Owner), "Only the owner can seed a shared document");
        let tx = self.db.transaction()?;
        ensure!(note(&tx, &room.note)?.is_none(), "Shared document is already initialized");
        let seen = Store::write_sync_body(&tx, &room.note, false, doc, front)?;
        Store::show_shared_note(&tx, &room.note, front, shared::registered_at(&self.root))?;
        ensure!(!capture_note(&tx, &self.root, self.peer, &room.note, doc, seen, front)?,
            "The shared document or one of its attachments is unavailable or too large");
        tx.commit()?;
        Ok(())
    }

    fn ensure_writable(&self, id: &str) -> Result<()> {
        if let Some(room) = &self.room {
            ensure!((id == room.note || room.folder && room.permits(id)) && self.access.is_some_and(Access::can_write), "This shared document is read-only");
        }
        Ok(())
    }

    fn history(&mut self, id: &str) -> Result<Option<(Note, &mut History)>> {
        if note(&self.db, id)?.is_none() {
            let tx = self.db.transaction()?;
            let (body, seen, front) = Store::read_sync_body(&tx, id, false)?;
            capture_note(&tx, &self.root, self.peer, id, &body, seen, &front)?;
            tx.commit()?;
        }
        let Some(old) = note(&self.db, id)?.filter(|note| !note.held) else { return Ok(None) };
        let history = match self.history.entry(id.into()) {
            std::collections::hash_map::Entry::Occupied(entry) => {
                let history = entry.into_mut();
                history.doc.import(&old.snapshot)?;
                import_note_updates(&self.db, id, &history.doc)?;
                history
            }
            std::collections::hash_map::Entry::Vacant(entry) =>
                entry.insert(History::new(open_note(&self.db, id, &old.snapshot)?, self.peer)?),
        };
        Ok(Some((old, history)))
    }

    pub fn save(&mut self, store: &mut SharedStore, id: &str, core: &Core) -> Result<(Meta, bool)> {
        self.ensure_writable(id)?;
        let doc = &core.doc;
        if self.room.is_some() {
            for path in content::local_assets(doc) { assets::path(&self.root, &path)?; }
        }
        let in_library: bool = self.db.query_row("SELECT EXISTS(SELECT 1 FROM library_documents WHERE id = ?1 AND trashed = 0)",
            [id], |row| row.get(0))?;
        if !in_library { return Ok((store.save(id, doc)?, false)); }
        if has_conflict(&self.db, id)? {
            self.history.remove(id);
            return store.save(id, doc).map(|meta| (meta, false));
        }
        let root = self.root.clone();
        let peer = self.peer;
        let active = self.history(id)?;
        let result = store.save_sqlite_with(id, doc, |tx, _| {
            let (seen, front) = revision(tx, id)?;
            if let Some((old, history)) = active {
                let before = history.doc.oplog_vv();
                history.save(core, peer)?;
                finish_capture(tx, &root, id, doc, (seen, &front), Capture { old, loro: &history.doc, before })
            } else {
                Ok((capture_note(tx, &root, peer, id, doc, seen, &front)?, true))
            }
        });
        if !matches!(&result, Ok((_, (_, false)))) { self.history.remove(id); }
        match result {
            Err(error) if error.is::<content::ConflictingBlockShapes>() => {
                let (meta, ()) = store.save_sqlite_with(id, doc, |tx, _| block_document(tx, id))?;
                Ok((meta, true))
            }
            result => result.map(|(meta, (warning, _))| (meta, warning)),
        }
    }

    pub fn undo(&mut self, store: &mut SharedStore, id: &str, core: &Core, peers: &[Pos], redo: bool) -> Result<Option<(NoteChange, Option<crate::editor::core::Selection>)>> {
        self.ensure_writable(id)?;
        if has_conflict(&self.db, id)? { return Ok(None); }
        let root = self.root.clone();
        let peer = self.peer;
        let Some((old, history)) = self.history(id)? else { return Ok(None) };
        let before = history.doc.oplog_vv();
        let anchors = [core.sel.anchor, core.sel.head].into_iter().chain(peers.iter().copied())
            .filter_map(|pos| content::anchor(&history.doc, pos).map(|cursor| (pos, cursor))).collect::<Vec<_>>();
        let result = (|| {
            history.checkpoint_selection(Some(core.sel))?;
            if !history.apply(redo)? { return Ok(None); }
            let mut document = content::render(&history.doc)?;
            keep_outside_history(&core.doc, &mut document);
            history.doc.set_next_commit_origin("sync-metadata");
            content::edit(&history.doc, peer, &document)?;
            let positions = anchors.iter().filter_map(|(pos, anchor)|
                content::resolve(&history.doc, pos.block, anchor).map(|resolved| (*pos, resolved))).collect();
            let (_, (_, held)) = store.save_sqlite_with(id, &document, |tx, _| {
                let (seen, front) = revision(tx, id)?;
                finish_capture(tx, &root, id, &document, (seen, &front), Capture { old, loro: &history.doc, before })
            })?;
            Ok(Some(((NoteChange { id: id.into(), document, positions }, history.restored_selection()), held)))
        })();
        if matches!(&result, Ok(Some((_, true))) | Err(_)) { self.history.remove(id); }
        result.map(|change| change.map(|(note, _)| note))
    }

    pub fn versions(&self, id: &str) -> Result<Vec<(i64, String, String)>> {
        let mut stmt = self.db.prepare("SELECT sequence, coalesce(strftime('%Y-%m-%d %H:%M', time, 'localtime'), time), body
            FROM sync_history WHERE note = ?1 ORDER BY sequence DESC LIMIT ?2")?;
        let mut rows = stmt.query(params![id, RECENT_VERSIONS])?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            let doc = archived_document(row.get_ref(2)?)?;
            let snippet = doc.plain_text().split_whitespace().collect::<Vec<_>>().join(" ").chars().take(45).collect();
            result.push((row.get(0)?, row.get(1)?, snippet));
        }
        Ok(result)
    }

    pub fn version(&self, id: &str, sequence: i64) -> Result<(Document, String)> {
        let mut stmt = self.db.prepare("SELECT body, front FROM sync_history WHERE note = ?1 AND sequence = ?2")?;
        let mut rows = stmt.query(params![id, sequence])?;
        let row = rows.next()?.context("No such iCloud version")?;
        Ok((archived_document(row.get_ref(0)?)?, row.get(1)?))
    }

    pub fn offline_recoveries(&self) -> Result<Vec<(String, String)>> {
        if self.room.is_some() { return Ok(vec![]); }
        Ok(self.db.prepare("SELECT note, title FROM offline_recoveries ORDER BY sequence DESC")?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<rusqlite::Result<_>>()?)
    }

    pub fn resolve_offline_recovery(&mut self, id: &str, keep: bool) -> Result<Option<String>> {
        ensure!(self.room.is_none(), "Recovery copies belong to the personal library.");
        let tx = self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (sequence, markdown): (i64, bool) = tx.query_row(
            "SELECT sequence, markdown FROM offline_recoveries WHERE note = ?1", [id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let copy = if keep {
            let (mut doc, front) = self::archived_body(&tx, id, sequence)?;
            let new = uuid::Uuid::new_v4().to_string();
            write_recovery_copy(&tx, &new, &mut doc, &front, Meta { markdown, ..Default::default() })?;
            Some(new)
        } else { None };
        tx.execute("DELETE FROM offline_recoveries WHERE note = ?1", [id])?;
        tx.execute("DELETE FROM sync_history WHERE note = ?1 AND sequence = ?2", params![id, sequence])?;
        tx.commit()?;
        Ok(copy)
    }

    pub fn request(&self) -> Result<Exchange> {
        self.request_with_uploads(true)
    }

    fn retained_paths(&self) -> Result<HashSet<String>> {
        let mut paths = HashSet::new();
        let db = &self.db;
        let mut stmt = db.prepare("SELECT id,trashed FROM library_documents")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let (doc, _, _) = Store::read_sync_body(db, row.get_ref(0)?.as_str()?,row.get(1)?)?;
            paths.extend(content::local_assets(&doc));
        }
        let mut stmt = db.prepare("SELECT h.body FROM sync_history h WHERE EXISTS (
            SELECT 1 FROM library_documents d WHERE d.id=h.note) OR EXISTS (
            SELECT 1 FROM offline_recoveries r WHERE r.sequence=h.sequence)")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? { paths.extend(content::local_assets(&archived_document(row.get_ref(0)?)?)); }
        retention::disk_roots(&self.root, &mut paths)?;
        Ok(paths)
    }

    // A new session has no editor undo stack. Only consider immutable bytes
    // previously acknowledged by the cloud, under this specific library root.
    fn collect_local(&mut self) -> Result<()> {
        if self.catalogue_paused()? || !retention::ready(&self.db)? || !self.db.query_row("SELECT EXISTS(SELECT 1 FROM cloud_release_candidates)
            AND NOT EXISTS(SELECT 1 FROM inbox) AND NOT EXISTS(SELECT 1 FROM outbox)
            AND NOT EXISTS(SELECT 1 FROM sync_document_conflicts)", [], |r| r.get::<_,bool>(0))?
        { return Ok(()); }
        let retained = self.retained_paths()?;
        let tx = self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let candidates = tx.prepare("SELECT path,digest FROM cloud_release_candidates r WHERE NOT EXISTS (
            SELECT 1 FROM cloud_inventory c WHERE c.path=r.path)")?.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (source, digest) in candidates {
            if retained.contains(&source) { continue; }
            let path = assets::path(&self.root, &source)?;
            if path.try_exists()? {
                use sha2::{Digest, Sha256};
                let data = crate::store::read_bounded(&path, assets::LIMIT as u64)?;
                if format!("{:x}", Sha256::digest(&data)) != digest { continue; }
                std::fs::remove_file(&path)?;
            }
            tx.execute("DELETE FROM sync_assets WHERE path=?1 AND digest=?2", params![source,digest])?;
            tx.execute("DELETE FROM sync_asset_chunks WHERE path=?1 AND digest=?2", params![source,digest])?;
            tx.execute("DELETE FROM cloud_release_candidates WHERE path=?1", [&source])?;
        }
        tx.commit()?;
        Ok(())
    }

    fn maintenance(&self) -> Result<Option<retention::Maintenance>> {
        if self.access.is_some_and(|a| a != Access::Owner) || self.catalogue_paused()? || !retention::ready(&self.db)? ||
            self.db.query_row("SELECT EXISTS(SELECT 1 FROM inbox) OR EXISTS(SELECT 1 FROM outbox)
                OR EXISTS(SELECT 1 FROM notes WHERE held<>0) OR EXISTS(SELECT 1 FROM sync_document_conflicts)", [], |r| r.get::<_,bool>(0))? { return Ok(None); }
        if let Some(plan)=retention::verified_cleanup(&self.db)? { return Ok(Some(plan)); }
        if self.room.is_none() {
            let missing = self.db.query_row("SELECT count(*) FROM cloud_inventory c WHERE vv IS NOT NULL AND scope<>?1
                AND NOT EXISTS(SELECT 1 FROM library_documents d WHERE d.id=c.scope)", [catalogue::SCOPE], |r| r.get::<_,i64>(0))?;
            if missing>0 {
                let cat=content::open(&get_state(&self.db,"catalogue")?.context("Missing catalogue")?)?;
                let mut stmt=self.db.prepare("SELECT id,scope FROM cloud_inventory c WHERE vv IS NOT NULL AND scope<>?1
                    AND NOT EXISTS(SELECT 1 FROM library_documents d WHERE d.id=c.scope)")?;
                let mut rows=stmt.query([catalogue::SCOPE])?;
                let mut remove=vec![];
                while let Some(row)=rows.next()? {
                    if catalogue::permanently_deleted(&cat,row.get_ref(1)?.as_str()?) { remove.push(row.get(0)?); }
                    if remove.len()==160 { break; }
                }
                if !remove.is_empty() { return Ok(Some(retention::Maintenance {snapshot:None,remove})); }
            }
        }
        for id in retention::candidates(&self.db,32)? {
            let doc = if id == catalogue::SCOPE { get_state(&self.db, "catalogue")?.as_deref().map(content::open).transpose()? }
                else { self.text_state(&id)? };
            if let Some(doc) = doc && let Some(plan) = retention::compact(&self.db, &id, &doc)? { return Ok(Some(plan)); }
        }
        let now = std::time::Instant::now();
        if self.next_collection.get().is_some_and(|next| next>now) { return Ok(None); }
        self.next_collection.set(Some(now + Duration::from_secs(60)));
        if !self.db.query_row("SELECT EXISTS(SELECT 1 FROM cloud_inventory WHERE path IS NOT NULL AND observed<=unixepoch()-3600)", [],
            |r| r.get::<_,bool>(0))? { return Ok(None); }
        let paths=self.retained_paths()?;
        let Some(plan)=retention::stale_assets(&self.db,&paths)? else { return Ok(None); };
        // Publish the recoverable version set before reclaiming files it may
        // reference. In small, low-traffic notes this need not wait for 32 edits.
        for id in retention::candidates(&self.db,2)? {
            if id==catalogue::SCOPE { continue; }
            if let Some(doc)=self.text_state(&id)? {
                return retention::compact(&self.db,&id,&doc);
            }
        }
        let pins=self.db.prepare("SELECT DISTINCT p.path FROM cloud_checkpoint_paths p JOIN cloud_inventory c USING(id)
            JOIN library_documents d ON d.id=c.scope")?.query_map([], |r| r.get::<_,String>(0))?
            .collect::<rusqlite::Result<HashSet<_>>>()?;
        if pins.is_empty() { return Ok(Some(plan)); }
        let mut protected=paths;
        protected.extend(pins);
        retention::stale_assets(&self.db,&protected)
    }

    fn ensure_dependencies(&self, edits: &mut Vec<Edit>) -> Result<Option<bool>> {
        let tx = self.db.unchecked_transaction()?;
        let mut paths_by_note = HashMap::<&str, HashSet<String>>::new();
        let mut conflicts = HashSet::new();
        for edit in edits.iter().filter(|e| e.note != catalogue::SCOPE && e.note != assets::SCOPE) {
            if conflicts.contains(&edit.note) { continue; }
            let state = self.text_state(&edit.note)?.context("Missing local history for pending edit")?;
            let end = LoroDoc::decode_import_blob_meta(&edit.data.0, true)?.partial_end_vv;
            let branch = state.fork_at(&state.vv_to_frontiers(&end))?;
            let doc = match content::render(&branch) {
                Ok(doc) => doc,
                Err(error) if error.is::<content::ConflictingBlockShapes>() => {
                    block_document(&tx, &edit.note)?;
                    paths_by_note.remove(edit.note.as_str());
                    conflicts.insert(edit.note.clone());
                    continue;
                }
                Err(error) => return Err(error),
            };
            let excluded = branch.get_map("local-only-attachments");
            paths_by_note.entry(&edit.note).or_default()
                .extend(content::local_assets(&doc).into_iter().filter(|path| excluded.get(path).is_none()));
        }
        let paths: HashSet<_> = paths_by_note.into_values().flatten().collect();
        edits.retain(|edit| !conflicts.contains(&edit.note));
        let mut added = false;
        for path in paths {
            match assets::ensure_upload(&tx, &self.root,&path) {
                Ok(queued) => added |= queued,
                Err(e) if e.chain().any(|e| e.downcast_ref::<std::io::Error>().is_some_and(|e| e.kind()==std::io::ErrorKind::NotFound)) => {
                    tx.execute("INSERT OR IGNORE INTO sync_asset_repairs VALUES (?1)",[&path])?;
                    tx.commit()?;
                    return Ok(None);
                }
                Err(error) => return Err(error),
            }
        }
        tx.commit()?;
        Ok(Some(added))
    }

    pub fn request_with_uploads(&self, allow_upload: bool) -> Result<Exchange> {
        let allow_upload = allow_upload && self.access.is_none_or(Access::can_write);
        let mut stmt = self.db.prepare("SELECT id, note, length(data), data FROM outbox WHERE ?1
            AND note NOT IN (SELECT note FROM sync_document_conflicts)
            AND (note <> ?3 OR NOT ?4)
            ORDER BY (note=?2) DESC,rowid LIMIT 20")?;
        let mut rows = stmt.query(params![allow_upload, assets::SCOPE, catalogue::SCOPE, self.catalogue_paused()?])?;
        let mut edits = vec![];
        let mut bytes = 0i64;
        while let Some(row) = rows.next()? {
            let note: String = row.get(1)?;
            ensure!(self.room.as_ref().is_none_or(|room| room.permits(&note)), "Unrelated content was queued in a shared document");
            let size: i64 = row.get(2)?;
            // Allow a large record by itself, while batching small note edits.
            if !edits.is_empty() && bytes + size > 8 * 1024 * 1024 { break; }
            edits.push(Edit { id: row.get(0)?, note, data: super::Bytes(row.get(3)?), collaborator: false });
            bytes += size;
        }
        drop(rows);
        drop(stmt);
        if allow_upload && retention::ready(&self.db)? && !edits.is_empty() {
            if edits[0].note == assets::SCOPE { edits.retain(|e| e.note == assets::SCOPE); }
            else { match self.ensure_dependencies(&mut edits)? {
                Some(true) => return self.request_with_uploads(true),
                Some(false) => {},
                None => edits.clear(),
            } }
        }
        let maintenance = if allow_upload && edits.is_empty() { self.maintenance()? } else { None };
        Ok(Exchange { downloads: if self.room.as_ref().is_some_and(|room| room.folder) { vec![] }
            else { retention::downloads(&self.db, &self.root, None)? }, maintenance, account: self.account.clone(), channel: self.channel.clone(),
            token: get_state(&self.db, "token")?.map(super::Bytes), edits, legacy: false, room: self.room.clone(), activity_only: false,
            #[cfg(feature = "devtools")]
            expire_cursor_for_qa: false,
            #[cfg(feature = "devtools")]
            upload_fault_for_qa: None })
    }

    pub fn legacy_request(&self) -> Result<Exchange> {
        ensure!(self.room.is_none(), "A shared document cannot access the personal sync zone");
        Ok(Exchange { downloads: vec![], maintenance: None, account: self.account.clone(), channel: self.channel.clone(),
            token: get_state(&self.db, "legacy_token")?.map(super::Bytes), edits: vec![], legacy: true, room: None, activity_only: false,
            #[cfg(feature = "devtools")]
            expire_cursor_for_qa: false,
            #[cfg(feature = "devtools")]
            upload_fault_for_qa: None })
    }

    pub(super) fn inbox_count(&self) -> Result<u32> {
        Ok(self.db.query_row("SELECT count(*) FROM inbox", [], |row| row.get(0))?)
    }

    pub(super) fn awaiting_attachment(&self, source: &str, inventory_confirmed: bool) -> Result<bool> {
        retention::awaiting_attachment(&self.db, source, inventory_confirmed)
    }

    pub(super) fn attachment_downloads(&self, open_notes: &[String]) -> Result<Vec<String>> {
        retention::downloads(&self.db, &self.root, self.room.as_ref().is_some_and(|room| room.folder).then_some(open_notes))
    }

    fn success_key(&self) -> Result<String> {
        // The account and scope also protect metadata when a library is copied.
        Ok(format!("sync_success:{}", serde_json::to_string(&(&self.account, &self.channel, &self.room))?))
    }

    pub(super) fn last_success(&self) -> Result<Option<u64>> {
        Ok(get_state(&self.db, &self.success_key()?)?.map(|value| serde_json::from_slice(&value)).transpose()?)
    }

    pub(super) fn record_success(&self, time: u64) -> Result<()> {
        put_state(&self.db, &self.success_key()?, &serde_json::to_vec(&time)?)
    }

    pub fn counts(&self) -> Result<(u32, u32, u32, u32)> {
        Ok(self.db.query_row("SELECT
            (SELECT count(*) FROM outbox),
            (SELECT count(*) FROM inbox),
            (SELECT count(*) FROM notes JOIN library_documents USING (id) WHERE held),
            (SELECT coalesce(sum(count), 0) FROM sync_attachment_warnings JOIN library_documents ON note = id)
                + (SELECT count(*) FROM sync_asset_repairs)",
            [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?)
    }

    pub(super) fn catalogue_paused(&self) -> Result<bool> {
        Ok(get_state(&self.db, CATALOGUE_PAUSED)?.is_some())
    }

    pub(super) fn has_materialized_item(&self, id: &str) -> Result<bool> {
        has_materialized_item(&self.db, id)
    }

    pub(super) fn has_queued_catalogue(&self) -> Result<bool> {
        Ok(self.db.query_row("SELECT EXISTS(SELECT 1 FROM outbox WHERE note=?1)", [catalogue::SCOPE], |row| row.get(0))?)
    }

    pub(super) fn pending_work(&self) -> Result<(u32, u32, u32)> {
        Ok(self.db.query_row("SELECT
            (SELECT count(*) FROM outbox WHERE note NOT IN (SELECT note FROM sync_document_conflicts)
                AND (note <> ?2 OR NOT ?1)),
            (SELECT count(*) FROM inbox i WHERE note NOT IN (SELECT note FROM sync_document_conflicts)
                AND (NOT ?1 OR note = ?3 OR (note <> ?2 AND (
                    EXISTS(SELECT 1 FROM library_entries WHERE id = i.note)
                    OR EXISTS(SELECT 1 FROM library_trash WHERE id = i.note))))),
            (SELECT count(*) FROM sync_document_conflicts)",
            params![self.catalogue_paused()?, catalogue::SCOPE, assets::SCOPE],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?)
    }

    pub(super) fn replay_pending_apply(&self) -> Result<bool> {
        if self.pending_work()?.1 == 0 { return Ok(false); }
        // Ask CloudKit for predecessors again. Preserve all downloaded edits,
        // local snapshots, versions and the outbox while the cursor is rebuilt.
        Ok(self.db.execute("DELETE FROM state WHERE key IN ('token', 'legacy_token')", [])? != 0)
    }

    pub(super) fn retry_conflicts(&mut self) -> Result<()> {
        self.db.execute("UPDATE sync_document_conflicts SET retry = 1 WHERE retry = 0", [])?;
        self.retry_catalogue = true;
        Ok(())
    }

    pub fn issues(&self) -> Result<Vec<SyncIssue>> {
        let repairs: HashSet<String> = self.db.prepare("SELECT path FROM sync_asset_repairs")?
            .query_map([], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
        let mut stmt = self.db.prepare("SELECT d.id, d.title, d.trashed, coalesce(n.held, 0), coalesce(w.count, 0), c.note IS NOT NULL
            FROM library_documents d LEFT JOIN notes n ON n.id = d.id
            LEFT JOIN sync_attachment_warnings w ON w.note = d.id
            LEFT JOIN sync_document_conflicts c ON c.note = d.id
            WHERE c.note IS NOT NULL OR n.held <> 0 OR w.count > 0 OR EXISTS (
                SELECT 1 FROM json_each(n.assets) ref JOIN sync_asset_repairs r ON r.path = ref.value)
            UNION ALL SELECT c.note, NULL, 0, 0, 0, 1 FROM sync_document_conflicts c
                WHERE NOT EXISTS(SELECT 1 FROM library_documents d WHERE d.id = c.note)
            ORDER BY 6 DESC, 3, 2, 1 LIMIT 30")?;
        let mut rows = stmt.query([])?;
        let mut issues = Vec::new();
        while let Some(row) = rows.next()? {
            let (id, title, trashed, held, warnings, conflict): (String, Option<String>, bool, bool, u32, bool) =
                (row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?);
            let mut attachments = Vec::new();
            if title.is_some() && (warnings != 0 || !repairs.is_empty()) {
                let (doc, _, _) = Store::read_sync_body(&self.db, &id, trashed)?;
                let loro = note(&self.db, &id)?.filter(|note| !note.snapshot.is_empty())
                    .map(|note| open_note(&self.db, &id, &note.snapshot)).transpose()?;
                let local_only = loro.as_ref().map(|loro| loro.get_map("local-only-attachments"));
                let mut sources: Vec<_> = content::local_assets(&doc).into_iter().collect();
                sources.sort();
                for source in sources {
                    let current = assets::source_file(&self.root, &source).err();
                    let recorded = local_only.as_ref().filter(|map| map.get(&source).is_some());
                    let recovering = repairs.contains(&source);
                    if !recovering && recorded.is_none() && (warnings == 0 || current.is_none()) { continue; }
                    let reason = recorded.and_then(|map| content::value(map, &source))
                        .and_then(|reason| assets::AssetProblem::from_code(&reason)).or(current);
                    let stored_here = assets::present(&self.root, &source);
                    attachments.push(AttachmentIssue { source, reason, stored_here, recovering });
                }
            }
            issues.push(SyncIssue { id, title, trashed, held, conflict, attachments });
        }
        Ok(issues)
    }

    pub fn identity(&self) -> (&str, Option<&str>, u64) {
        (&self.account, self.channel.as_deref(), self.peer)
    }

    pub fn account_verification_marker(&self) -> Result<PathBuf> {
        let personal = if self.room.is_some() {
            self.root.parent().and_then(Path::parent).context("Missing personal library for shared document")?
        } else { &self.root };
        ensure!(personal.file_name().and_then(|name| name.to_str()) == Some(self.account.as_str()), "Unexpected iCloud library directory");
        Ok(personal.join("require-account-verification"))
    }

    pub fn displayable_updates(&self, rendered: &Document) -> Result<Vec<String>> {
        let Some(room) = &self.room else { return Ok(vec![]); };
        if self.access == Some(Access::Revoked) { return Ok(vec![]); }
        let updates = activity::displayable(&self.db)?;
        if updates.is_empty() { return Ok(updates); }
        // An open editor can still hold the previous projection.
        let (projected, _, _) = Store::read_sync_body(&self.db, &room.note, false)?;
        if projected != *rendered { return Ok(vec![]); }
        Ok(updates)
    }

    pub fn mark_updates_read(&mut self, displayed: &[String]) -> Result<()> {
        if self.room.is_some() && self.access != Some(Access::Revoked) {
            activity::mark_read(&mut self.db, displayed)?;
        }
        Ok(())
    }

    pub fn text_state(&self, id: &str) -> Result<Option<LoroDoc>> {
        note(&self.db, id)?.filter(|note| !note.held)
            .map(|note| open_note(&self.db, id, &note.snapshot)).transpose()
    }

    pub fn retry_attachments(&mut self) -> Result<bool> {
        let tx = self.db.transaction()?;
        tx.execute("UPDATE notes SET seen = -1 WHERE id IN (SELECT note FROM sync_attachment_warnings)", [])?;
        let paths: Vec<String> = tx.prepare("SELECT DISTINCT a.path FROM notes n
            JOIN library_documents d ON d.id = n.id JOIN json_each(n.assets) ref
            JOIN sync_assets a ON a.path = ref.value")?
            .query_map([], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
        tx.execute("DELETE FROM sync_asset_repairs", [])?;
        let mut replay = false;
        for path in paths {
            if matches!(assets::source_file(&self.root, &path), Err(assets::AssetProblem::Missing)) {
                tx.execute("INSERT INTO sync_asset_repairs VALUES (?1)", [&path])?;
                replay = true;
            }
        }
        // A missing cache can be recovered from the immutable edit records.
        // Keep the outbox and any partially downloaded chunks intact.
        if replay { tx.execute("DELETE FROM state WHERE key IN ('token', 'legacy_token')", [])?; }
        tx.commit()?;
        self.merged_data_version = None;
        Ok(replay)
    }

    pub fn receive(&mut self, page: Changes) -> Result<bool> {
        self.receive_page(page).map(|(access_changed, _)| access_changed)
    }

    pub(super) fn receive_page(&mut self, page: Changes) -> Result<(bool, bool)> {
        let tx = self.db.transaction()?;
        let cursor = if page.legacy { "legacy_token" } else { "token" };
        ensure!(!page.legacy || page.uploaded.is_empty(), "The previous sync zone is read-only");
        ensure!(page.room_access.is_some() == self.room.is_some(), "Incorrect CloudKit sharing scope");
        if let (Some(room), Some(access)) = (&self.room, page.room_access) {
            ensure!(page.edits.iter().all(|edit| room.permits(&edit.note))
                && (access.can_write() || page.uploaded.is_empty()), "Invalid shared document changes");
            put_state(&tx, "shared_access", &serde_json::to_vec(&access)?)?;
            activity::observe(&tx, &room.note, &page.edits)?;
            activity::remember_access(&tx, access)?;
            if access == Access::Revoked {
                ensure!(page.edits.is_empty() && page.token.is_none(), "A revoked share returned note content");
                archive_owner_ended(&tx, room)?;
                tx.execute("DELETE FROM library_entries", [])?;
            }
        }
        if let Some(receipt) = &page.retention {
            ensure!(!page.legacy && page.room_access != Some(Access::Revoked), "Incorrect inventory scope");
            retention::received(&tx, receipt, &page.edits)?;
        }
        let mut uploaded = false;
        for id in page.uploaded { uploaded |= tx.execute("DELETE FROM outbox WHERE id = ?1", [id])? != 0; }
        let mut catalogue_received = false;
        for edit in page.edits {
            // Save the full CloudKit payload before advancing the cursor. A
            // checkpoint may contain an unrenderable protected version.
            if tx.execute("INSERT OR IGNORE INTO inbox VALUES (?1, ?2, ?3)", params![edit.id, edit.note, edit.data.0])? != 0 {
                tx.execute("UPDATE sync_document_conflicts SET retry = 1 WHERE note = ?1", [&edit.note])?;
                catalogue_received |= edit.note == catalogue::SCOPE;
            }
        }
        if let Some(token) = page.token { put_state(&tx, cursor, &token.0)?; }
        else {
            ensure!((page.legacy || page.room_access == Some(Access::Revoked)) && !page.more,
                "CloudKit did not return a sync cursor");
            tx.execute("DELETE FROM state WHERE key = ?1", [cursor])?;
        }
        tx.commit()?;
        self.retry_catalogue |= catalogue_received;
        let changed = self.access != page.room_access;
        self.access = page.room_access;
        if changed {
            self.merged_data_version = None;
            if self.access.is_some_and(|access| !access.can_write()) { self.history.clear(); }
        }
        Ok((changed, uploaded))
    }

    pub fn merge(&mut self, open_positions: &HashMap<String, Vec<Pos>>) -> Result<(bool, Vec<NoteChange>, bool)> {
        self.apply_wait = Default::default();
        if self.access == Some(Access::Revoked) { return Ok((false, vec![], false)); }
        self.history.retain(|id, _| open_positions.contains_key(id));
        // Store writes through a separate connection. A quiet poll needn't
        // decode the entire library index again just to fetch a CloudKit page.
        let version: i64 = self.db.pragma_query_value(None, "data_version", |row| row.get(0))?;
        let paused = self.catalogue_paused()?;
        // Retry an existing pause once after the Loro upgrade. Keep the
        // durable pause if its original records still cannot be applied.
        let upgraded = paused && get_state(&self.db, CATALOGUE_REPLAY_UPGRADE)?.is_none();
        let document_recovery_available = get_state(&self.db, DOCUMENT_REPLAY_UPGRADE)?.is_none();
        let retry = self.retry_catalogue || upgraded;
        if self.merged_data_version == Some(version) && !document_recovery_available && !(paused && retry)
            && self.pending_work()?.1 == 0
            && !self.db.query_row("SELECT EXISTS(SELECT 1 FROM sync_document_conflicts WHERE retry = 1)",
                [], |row| row.get::<_, bool>(0))?
        { return Ok((false, vec![], false)); }
        let mut tx = self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let has_catalogue = self.room.as_ref().is_none_or(|room| room.folder);
        let writable = self.access.is_none_or(Access::can_write);
        let local = if has_catalogue { catalogue::local(&tx)? } else { Default::default() };
        let seen = serde_json::from_slice(&get_state(&tx, "catalogue_seen")?.context("Missing catalogue projection")?)?;
        let mut wait = super::ApplyWait::default();
        let catalogue = if !paused || retry {
            let mut candidate = tx.savepoint()?;
            match stage_catalogue(&candidate, self.peer, self.room.is_none(), writable, paused, &local, &seen, &mut wait.catalogue_predecessor) {
                Ok(Some(ready)) => {
                    candidate.execute("DELETE FROM state WHERE key = ?1", [CATALOGUE_PAUSED])?;
                    candidate.commit()?;
                    Some(ready)
                }
                Ok(None) => None,
                Err(error) if error.is::<ReplayFailure>() => {
                    candidate.rollback()?;
                    candidate.commit()?;
                    if !paused { crate::logging::write(&format!("CloudKit sync: {error}; catalogue paused")); }
                    put_state(&tx, CATALOGUE_PAUSED, error.to_string().as_bytes())?;
                    None
                }
                Err(error) => return Err(error),
            }
        } else { None };
        if upgraded { put_state(&tx, CATALOGUE_REPLAY_UPGRADE, b"1")?; }
        let catalogue_paused = has_catalogue && catalogue.is_none();
        let recover_documents = document_recovery_available && !catalogue_paused;
        // Revisit conflicts recorded by older clients once, even offline.
        if recover_documents {
            tx.execute("UPDATE sync_document_conflicts SET retry = 1", [])?;
            put_state(&tx, DOCUMENT_REPLAY_UPGRADE, b"1")?;
        }

        let mut work: std::collections::BTreeMap<String, Option<bool>> = tx.prepare(
            "SELECT DISTINCT note FROM inbox WHERE note NOT IN (?1, ?2)
                AND note NOT IN (SELECT note FROM sync_document_conflicts WHERE retry = 0)
             UNION SELECT note FROM sync_document_conflicts WHERE retry = 1")?
            .query_map([catalogue::SCOPE, assets::SCOPE], |row| Ok((row.get(0)?, None)))?
            .collect::<rusqlite::Result<_>>()?;
        let local_edits = tx.prepare("SELECT d.id, d.trashed FROM library_documents d LEFT JOIN notes n ON n.id = d.id
            WHERE (n.id IS NULL OR d.revision <> n.seen OR n.held <> 0)
                AND d.id NOT IN (SELECT note FROM sync_document_conflicts WHERE retry = 0)")?
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let local_bodies_changed = !local_edits.is_empty() && has_catalogue;
        for (id, trashed) in local_edits.into_iter().filter(|(id, _)| writable && local.contains_key(id)
            || self.room.as_ref().is_some_and(|room| room.note == *id && writable)) {
            work.insert(id, Some(trashed));
        }

        let attachments_ready = assets::ingest(&tx, &self.root)?;
        let mut changed = vec![];
        for (id, trashed) in work {
            // Keep bodies for unknown entries pending until their catalogue
            // metadata can be applied. Never project a rejected tombstone.
            if catalogue_paused && !local.contains_key(&id) {
                wait.documents_awaiting_catalogue += 1;
                continue;
            }
            let mut candidate = tx.savepoint()?;
            let result = (|| {
                if !writable && has_conflict(&candidate, &id)?
                    && candidate.query_row("SELECT EXISTS(SELECT 1 FROM library_documents d JOIN notes n USING(id)
                        WHERE d.id = ?1 AND d.revision <> n.seen)", [&id], |row| row.get::<_, bool>(0))?
                { return Err(content::ConflictingBlockShapes.into()); }
                if let Some(trashed) = trashed {
                    let (doc, seen, front) = Store::read_sync_body(&candidate, &id, trashed)?;
                    if has_conflict(&candidate, &id)? && let Some(old) = note(&candidate, &id)?
                        && let Ok((previous, true)) = content::project(&open_note(&candidate, &id, &old.snapshot)?)
                    {
                        // An older local projection may predate the aliases.
                        // Do not turn missing tables in it into CRDT deletes.
                        let mut ids = HashSet::new();
                        doc.for_each_block(|block| { ids.insert(block.id); });
                        let mut retains_tables = true;
                        previous.for_each_block(|block| {
                            if matches!(block.kind, crate::model::BlockKind::Table(_)) { retains_tables &= ids.contains(&block.id); }
                        });
                        ensure!(retains_tables, content::ConflictingBlockShapes);
                    }
                    capture_note(&candidate, &self.root, self.peer, &id, &doc, seen, &front)
                        .context("Capture local document for sync")?;
                }
                if catalogue.as_ref().is_some_and(|(doc, _)| catalogue::permanently_deleted(doc, &id)) {
                    // Keep the quarantined data available for recovery even if
                    // the catalogue carries a permanent deletion.
                    if has_conflict(&candidate, &id)? {
                        candidate.execute("UPDATE sync_document_conflicts SET retry = 0 WHERE note = ?1", [&id])?;
                    } else {
                        candidate.execute("DELETE FROM inbox WHERE note = ?1", [&id])?;
                    }
                    return Ok(DocumentApply::Deleted);
                }
                apply_document(&candidate, &self.root, &id, self.room.is_some(),
                    open_positions.get(&id).map_or(&[], Vec::as_slice))
            })();
            match result {
                Ok(result) => {
                    match result {
                        DocumentApply::Applied(change) => {
                            candidate.execute("DELETE FROM sync_document_conflicts WHERE note = ?1", [&id])?;
                            changed.extend(change);
                        }
                        DocumentApply::Held => wait.documents_held += 1,
                        DocumentApply::AwaitingPredecessor => wait.documents_awaiting_predecessor += 1,
                        DocumentApply::AwaitingAttachment => wait.documents_awaiting_attachment += 1,
                        DocumentApply::Deleted => {}
                    }
                    candidate.commit()?;
                }
                Err(error) if error.is::<content::ConflictingBlockShapes>() || error.is::<ReplayFailure>() => {
                    candidate.rollback()?;
                    candidate.commit()?;
                    block_document(&tx, &id)?;
                    if error.is::<ReplayFailure>() { self.history.remove(&id); }
                }
                Err(error) => return Err(error),
            }
        }

        let library_changed = if let Some(room) = self.room.as_ref().filter(|room| !room.folder) {
            let front: Option<String> = tx.query_row("SELECT front FROM library_documents WHERE id = ?1",
                [&room.note], |row| row.get(0)).optional()?;
            let front = front.unwrap_or_default();
            Store::show_shared_note(&tx, &room.note, &front, shared::registered_at(&self.root))?
        } else if let Some((catalogue_doc, before)) = catalogue {
            let before_projection = catalogue_doc.oplog_vv();
            let catalogue_changed = before_projection != before;
            let dirty = tx.execute("DELETE FROM state WHERE key = 'catalogue_dirty'", [])? != 0;
            let projected = if catalogue_changed || local_bodies_changed || dirty || !changed.is_empty() || !writable && local != seen {
                if self.room.is_none() { super::organizer::project(&tx, &catalogue_doc)?; }
                let result = catalogue::project(&tx, &catalogue_doc, self.room.as_ref().is_some_and(|room| room.folder))? || catalogue_changed;
                put_state(&tx, "catalogue_seen", &serde_json::to_vec(&catalogue::local(&tx)?)?)?;
                result
            } else { false };
            catalogue_doc.commit();
            if writable && catalogue_doc.oplog_vv() != before_projection {
                enqueue(&tx, catalogue::SCOPE, catalogue_doc.export(ExportMode::updates(&before_projection))?)?;
            }
            if catalogue_doc.oplog_vv() != before { put_state(&tx, "catalogue", &catalogue_doc.export(ExportMode::Snapshot)?)?; }
            projected
        } else { false };
        tx.commit()?;
        self.retry_catalogue = false;
        self.apply_wait = wait;
        self.merged_data_version = Some(version);
        Ok((library_changed, changed, attachments_ready))
    }
}

#[cfg(test)]
struct HistoryFixture(PathBuf);

#[cfg(test)]
impl HistoryFixture {
    fn new() -> Result<Self> {
        Self::at(std::env::temp_dir().join(format!("quiet-history-test-{}", uuid::Uuid::new_v4())))
    }

    fn at(path: PathBuf) -> Result<Self> {
        let fixture = Self(path);
        let source = fixture.0.join(".restore-source");
        Store::open_at(source.clone())?;
        std::fs::write(source.join(".cloud-restore-ready"), format!("{}\n", "a".repeat(64)))?;
        Store::initialize_sqlite(&fixture.0)?;
        Ok(fixture)
    }
}

#[cfg(test)]
impl Drop for HistoryFixture {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}

#[cfg(test)]
#[test]
fn sync_success_reopens_with_only_the_matching_account_and_scope() -> Result<()> {
    let fixture = HistoryFixture::new()?;
    let marker = fixture.0.join(".cloud-restore-ready");
    let account_a = std::fs::read_to_string(&marker)?;
    let account_b = "b".repeat(64);
    let personal = Journal::open(&fixture.0, None)?;
    assert_eq!(personal.last_success()?, None);
    personal.record_success(123)?;
    drop(personal);
    assert_eq!(Journal::open(&fixture.0, None)?.last_success()?, Some(123));

    // Even a copy of the original journal must not expose another account's timestamp.
    std::fs::write(&marker, &account_b)?;
    let other = Journal::open(&fixture.0, None)?;
    assert_eq!(other.last_success()?, None);
    other.record_success(456)?;
    drop(other);
    std::fs::write(&marker, &account_a)?;
    assert_eq!(Journal::open(&fixture.0, None)?.last_success()?, Some(123));

    let room_fixture = HistoryFixture::new()?;
    let room = shared::Room { scope: shared::Scope { note: uuid::Uuid::new_v4().to_string(),
        owner: Some("synthetic-owner".into()), folder: false }, title: "Synthetic share".into() };
    std::fs::write(room_fixture.0.join(".cloud-restore-ready"), format!("{account_a}shared\n"))?;
    std::fs::write(room_fixture.0.join(shared::MARKER), serde_json::to_vec(&room)?)?;
    let shared = Journal::open(&room_fixture.0, None)?;
    assert_eq!(shared.last_success()?, None);
    shared.record_success(789)?;
    drop(shared);
    assert_eq!(Journal::open(&room_fixture.0, None)?.last_success()?, Some(789));
    assert_eq!(Journal::open(&fixture.0, None)?.last_success()?, Some(123));
    Ok(())
}

#[cfg(test)]
#[test]
fn sync_progress_uses_durable_receipts_even_when_the_outbox_length_stays_fixed() -> Result<()> {
    use super::{Bytes, SyncPhase, progress::{Monitor, Round}};
    let fixture = HistoryFixture::new()?;
    let mut journal = Journal::open(&fixture.0, None)?;
    let mut raw = Store::open_sqlite_at(fixture.0.clone())?;
    let id = raw.create_from(&crate::docxml::parse("<title>Sync status</title><p>Initial text</p>")?)?;
    let mut store = SharedStore::for_test(raw);
    journal.merge(&Default::default())?;
    let acknowledge = |uploaded| Changes { retention: None, uploaded, edits: vec![], token: Some(Bytes(vec![1])),
        more: false, legacy: false, room_access: None, upload_quota_exceeded: false,
        #[cfg(feature = "devtools")] cursor_replayed: false };
    let baseline = journal.request_with_uploads(true)?.edits.into_iter().map(|edit| edit.id).collect();
    assert!(journal.receive_page(acknowledge(baseline))?.1);

    let mut doc = store.load_for_edit(&id, false)?;
    doc.blocks[0].text = crate::model::RichText::plain("Changed text");
    journal.db.execute_batch("CREATE TRIGGER reject_test_outbox BEFORE INSERT ON outbox
        BEGIN SELECT RAISE(ABORT, 'injected durable queue failure'); END;")?;
    assert!(journal.save(&mut store, &id, &Core::new(doc.clone())).is_err());
    assert_ne!(store.load(&id)?, doc);
    assert_eq!(journal.counts()?.0, 0);
    assert_eq!(journal.last_success()?, None);
    journal.db.execute_batch("DROP TRIGGER reject_test_outbox;")?;

    journal.save(&mut store, &id, &Core::new(doc.clone()))?;
    let start = std::time::Instant::now();
    let mut monitor = Monitor::default();
    let queue = journal.counts()?.0;
    assert!(queue > 0);
    for seconds in [0, 35, 70, 105] {
        let batch = journal.request_with_uploads(true)?.edits.into_iter().map(|edit| edit.id).collect::<Vec<_>>();
        assert!(journal.receive_page(acknowledge(batch.clone()))?.1);
        assert!(!journal.receive_page(acknowledge(batch))?.1);
        doc.blocks[0].text = crate::model::RichText::plain(format!("Edit at {seconds}"));
        journal.save(&mut store, &id, &Core::new(doc.clone()))?;
        assert_eq!(journal.counts()?.0, queue);
        monitor.completed(start + Duration::from_secs(seconds), Round { uploading: true, uploaded: true, ..Round::default() });
        assert_eq!(monitor.stalled(), None);
    }
    for seconds in [135, 165, 195] {
        let (_, uploaded) = journal.receive_page(acknowledge(vec![]))?;
        monitor.completed(start + Duration::from_secs(seconds), Round { uploading: true, uploaded, ..Round::default() });
    }
    assert_eq!(monitor.stalled(), Some(SyncPhase::Uploading));
    let batch = journal.request_with_uploads(true)?.edits.into_iter().map(|edit| edit.id).collect();
    let (_, uploaded) = journal.receive_page(acknowledge(batch))?;
    monitor.completed(start + Duration::from_secs(196), Round { uploaded, ..Round::default() });
    assert_eq!(monitor.stalled(), None);
    assert_eq!(journal.counts()?.0, 0);
    assert_eq!(store.load(&id)?, doc);
    Ok(())
}

#[cfg(test)]
#[test]
fn merge_recovers_a_checkpoint_envelope_already_in_the_inbox() -> Result<()> {
    let fixture = HistoryFixture::new()?;
    let mut store = Store::open_sqlite_at(fixture.0.clone())?;
    let mut journal = Journal::open(&fixture.0, None)?;
    let id = store.create_from(&crate::docxml::parse("<title>Unsynced local document</title><p>Keep me</p>")?)?;
    journal.merge(&Default::default())?;
    let document = store.load(&id)?;
    let outbox = journal.request_with_uploads(true)?.edits;
    assert!(!outbox.is_empty());
    let catalogue = content::open(&get_state(&journal.db, "catalogue")?.context("Missing catalogue snapshot")?)?;
    let checkpoint = retention::pack(&retention::Index { versions: vec![], records: vec![outbox[0].id.clone()] },
        &catalogue.export(ExportMode::Snapshot)?)?;
    let cursor = b"saved cloud cursor";
    put_state(&journal.db, "token", cursor)?;
    journal.db.execute("INSERT INTO inbox VALUES (?1, ?2, ?3)",
        params![uuid::Uuid::new_v4().to_string(), catalogue::SCOPE, checkpoint])?;
    drop(journal);

    let mut reopened = Journal::open(&fixture.0, None)?;
    reopened.merge(&Default::default())?;
    reopened.merge(&Default::default())?;
    assert_eq!(reopened.counts()?.1, 0);
    assert_eq!(get_state(&reopened.db, "token")?.as_deref(), Some(cursor.as_slice()));
    assert_eq!(reopened.request_with_uploads(true)?.edits.iter().map(|edit| &edit.id).collect::<Vec<_>>(),
        outbox.iter().map(|edit| &edit.id).collect::<Vec<_>>());
    assert_eq!(store.load(&id)?, document);
    Ok(())
}

#[cfg(test)]
#[test]
fn equivalent_cloud_block_aliases_preserve_descendants_and_future_edits() -> Result<()> {
    let fixture = HistoryFixture::new()?;
    let mut journal = Journal::open(&fixture.0, None)?;
    let mut store = Store::open_sqlite_at(fixture.0.clone())?;
    let body = crate::docxml::parse("<title>Block identity</title><p>first</p><p>second</p>")?;
    let id = store.create_from(&body)?;
    journal.merge(&Default::default())?;
    let stored = store.load_for_edit(&id, false)?;
    let remote = journal.text_state(&id)?.context("Missing document snapshot")?;
    remote.set_peer_id(journal.peer + 1)?;
    let before = remote.oplog_vv();
    let tree = remote.get_tree("blocks");
    let tree_id = |value: crate::model::BlockId| -> Result<loro::TreeID> {
        let key = value.0.to_string();
        tree.roots().into_iter()
            .find(|&node| tree.get_meta(node).ok().and_then(|meta| content::value(&meta, "block")).as_deref() == Some(&key))
            .context("Missing test block")
    };
    let original = tree_id(stored.blocks[0].id)?;
    let child = tree_id(stored.blocks[1].id)?;
    let duplicate = tree.create(None)?;
    let meta = tree.get_meta(duplicate)?;
    let original_meta = tree.get_meta(original)?;
    for key in ["block", "shape"] {
        content::put(&meta, key, &content::value(&original_meta, key).context("Missing test metadata")?)?;
    }
    tree.mov(child, Some(duplicate))?;
    let data = remote.export(ExportMode::updates(&before))?;
    journal.receive(Changes { retention: None, uploaded: vec![], edits: vec![Edit { id: "duplicate-block".into(),
        note: id.clone(), data: super::Bytes(data), collaborator: false }], token: Some(super::Bytes(vec![1])),
        more: false, legacy: false, room_access: None, #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false })?;
    journal.merge(&Default::default())?;
    let mut merged = store.load_for_edit(&id, false)?;
    assert_eq!(merged, stored);
    merged.blocks[1].text = crate::model::RichText::plain("second, edited locally");
    store.save(&id, &merged)?;
    journal.merge(&Default::default())?;
    assert_eq!(content::render(&journal.text_state(&id)?.context("Missing edited snapshot")?)?, merged);
    Ok(())
}

#[cfg(test)]
#[test]
fn permanent_delete_wins_over_late_restore_and_archives_offline_body() -> Result<()> {
    let fixture = HistoryFixture::new()?;
    let mut journal = Journal::open(&fixture.0, None)?;
    let mut store = Store::open_sqlite_at(fixture.0.clone())?;
    let asset = "assets/files/离线附件.txt";
    std::fs::create_dir_all(fixture.0.join("assets/files"))?;
    std::fs::write(fixture.0.join(asset), b"offline recovery attachment")?;
    let body = crate::docxml::parse(&format!("<title>离线测试</title><p>共同正文</p><figure view-type=\"Card\"><source path=\"{asset}\" name=\"离线附件.txt\"/></figure>"))?;
    let id = store.create_markdown(&body, "---\nstatus: draft\n---\n")?;
    let unchanged = store.create_from(&crate::docxml::parse("<title>旧缓存</title><p>没有新增内容</p>")?)?;
    journal.merge(&Default::default())?;
    store.delete(&id, None)?;
    store.delete(&unchanged, None)?;
    journal.merge(&Default::default())?;
    let snapshot = get_state(&journal.db, "catalogue")?.context("Missing trash catalogue")?;
    let trash = catalogue::local(&journal.db)?;
    let deletion = LoroDoc::new();
    deletion.import(&snapshot)?;
    deletion.set_peer_id(journal.peer + 1)?;
    let base = deletion.oplog_vv();
    catalogue::capture(&journal.db, &deletion, &trash, &Default::default())?;
    let purge = deletion.export(ExportMode::updates(&base))?;

    let original = journal.text_state(&id)?.context("Missing original note")?;
    let mut late_body = content::render(&original)?;
    late_body.blocks[0].text = crate::model::RichText::plain("另一台迟到的改动");
    let body_update = content::edit(&original, journal.peer + 2, &late_body)?.context("Missing late body update")?;

    let stale = LoroDoc::new();
    stale.import(&snapshot)?;
    stale.set_peer_id(journal.peer + 2)?;
    for index in 0..8 { stale.get_map("test-clock").insert("tick", index)?; }
    let entry = format!("entry-{id}");
    stale.get_map(entry.as_str()).insert("life", "\"Live\"")?;
    let stale_restore = stale.export(ExportMode::updates(&base))?;
    deletion.import(&stale_restore)?;
    assert_eq!(content::value(&deletion.get_map(entry.as_str()), "life").as_deref(), Some("\"Live\""));

    store.restore(&id)?;
    let mut edited = store.load_for_edit(&id, false)?;
    edited.blocks[0].text = crate::model::RichText::plain("共同正文和离线新增文字");
    store.save(&id, &edited)?;
    journal.merge(&Default::default())?;
    // CloudKit may acknowledge the offline edit on the same page as the deletion.
    let uploaded: Vec<_> = journal.request()?.edits.into_iter().map(|edit| edit.id).collect();
    let edits = [("purge", catalogue::SCOPE, purge), ("late-restore", catalogue::SCOPE, stale_restore),
        ("late-body", id.as_str(), body_update)].map(|(id, note, data)| Edit {
            id: id.into(), note: note.into(), data: super::Bytes(data), collaborator: false,
        });
    for _ in 0..2 {
        journal.receive(Changes { retention: None, uploaded: uploaded.clone(), edits: edits.to_vec(), token: Some(super::Bytes(vec![1])),
            more: false, legacy: false, room_access: None, #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false })?;
        journal.merge(&Default::default())?;
        assert!(store.list().is_empty());
        assert!(store.list_trash().is_empty());
        assert!(store.load(&id).is_err());
        assert!(journal.request()?.edits.iter().all(|edit| edit.note != id));
        assert_eq!(journal.counts()?.1, 0);
        assert_eq!(journal.offline_recoveries()?, [(id.clone(), "离线测试".to_string())]);
    }
    let versions = journal.versions(&id)?;
    assert!(versions.iter().any(|(sequence, _, _)| journal.version(&id, *sequence)
        .is_ok_and(|(doc, _)| doc.plain_text() == edited.plain_text())));
    drop(journal);
    let mut journal = Journal::open(&fixture.0, None)?;
    journal.merge(&Default::default())?;
    assert!(store.list().is_empty());
    assert_eq!(journal.versions(&id)?, versions);

    let restore = HistoryFixture::new()?;
    let target = HistoryFixture::at(restore.0.join("empty"))?;
    drop(Journal::open(&target.0, None)?);
    let (backup, _) = super::backup::create(&fixture.0, &restore.0)?;
    super::backup::prepare(&target.0, &backup)?.schedule(&target.0)?;
    let old_lock = crate::cloud_restore::lock_library(&target.0)?;
    let restored_lock = super::backup::install(&target.0)?.context("Restore did not run")?;
    drop((old_lock, restored_lock));
    let mut restored = Journal::open(&target.0, None)?;
    assert_eq!(restored.offline_recoveries()?, journal.offline_recoveries()?);
    let restored_copy = restored.resolve_offline_recovery(&id, true)?.context("Missing recovery copy after backup restore")?;
    let restored_store = Store::open_sqlite_at(target.0.clone())?;
    assert!(restored_store.load(&id).is_err());
    assert_eq!(restored_store.load(&restored_copy)?.asset_sources(), [asset]);
    assert_eq!(restored_store.load(&restored_copy)?.plain_text(), edited.plain_text());
    assert_eq!(std::fs::read(target.0.join(asset))?, b"offline recovery attachment");

    let copy = journal.resolve_offline_recovery(&id, true)?.context("Missing recovery copy")?;
    assert_ne!(copy, id);
    assert!(journal.offline_recoveries()?.is_empty());
    assert!(store.load(&id).is_err());
    assert!(store.list().iter().any(|entry| entry.id == copy && entry.meta.markdown));
    assert_eq!(store.load(&copy)?.plain_text(), edited.plain_text());
    assert!(store.is_markdown(&copy));
    assert!(store.front_matter_of(&copy).contains("status: draft"));
    assert!(journal.resolve_offline_recovery(&id, true).is_err());
    Ok(())
}

#[cfg(test)]
#[test]
fn acknowledged_local_updates_survive_reopen_and_compact_after_merge() -> Result<()> {
    let fixture = HistoryFixture::new()?;
    let mut journal = Journal::open(&fixture.0, None)?;
    let mut store = Store::open_sqlite_at(fixture.0.clone())?;
    let id = store.create_from(&crate::docxml::parse("<title>快照验收</title><p>原文</p>")?)?;
    journal.merge(&Default::default())?;
    let base = note(&journal.db, &id)?.context("Missing initial snapshot")?.snapshot;
    let update_count = |journal: &Journal| -> Result<i64> {
        Ok(journal.db.query_row("SELECT count(*) FROM note_local_updates WHERE note = ?1", [&id], |row| row.get(0))?)
    };
    let mut doc = store.load_for_edit(&id, false)?;
    let mut remote_doc = doc.clone();
    let at = doc.blocks[0].text.text.len();
    doc.blocks[0].text.replace(at..at, &crate::model::RichText::plain("本机"));
    store.save(&id, &doc)?;
    journal.merge(&Default::default())?;
    assert_eq!(update_count(&journal)?, 1);
    assert_eq!(note(&journal.db, &id)?.unwrap().snapshot, base);
    let uploaded = journal.request()?.edits.into_iter().map(|edit| edit.id).collect();
    journal.receive(Changes { retention: None, uploaded, edits: vec![], token: Some(super::Bytes(vec![1])),
        more: false, legacy: false, room_access: None, #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false })?;
    assert_eq!(journal.counts()?.0, 0);
    drop(journal);
    let mut journal = Journal::open(&fixture.0, None)?;
    assert_eq!(update_count(&journal)?, 1);
    assert_eq!(content::render(&journal.text_state(&id)?.unwrap())?, doc);

    let remote = content::open(&base)?;
    remote_doc.blocks[0].text.replace(0..0, &crate::model::RichText::plain("远端"));
    let data = content::edit(&remote, journal.peer + 1, &remote_doc)?.context("Missing concurrent update")?;
    journal.receive(Changes { retention: None, uploaded: vec![], edits: vec![Edit { id: "remote".into(), note: id.clone(),
        data: super::Bytes(data), collaborator: false }], token: Some(super::Bytes(vec![2])),
        more: false, legacy: false, room_access: None, #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false })?;
    journal.merge(&Default::default())?;
    assert_eq!(store.load_for_edit(&id, false)?.plain_text(), "远端原文本机");
    assert_eq!(update_count(&journal)?, 0);
    assert_eq!(content::render(&journal.text_state(&id)?.unwrap())?.plain_text(), "远端原文本机");

    let mut doc = store.load_for_edit(&id, false)?;
    for step in 1..=NOTE_CHECKPOINT_EDITS {
        doc.blocks[0].text = crate::model::RichText::plain(format!("离线保存 {step}"));
        store.save(&id, &doc)?;
        journal.merge(&Default::default())?;
        assert_eq!(update_count(&journal)?, step % NOTE_CHECKPOINT_EDITS);
    }
    assert_eq!(content::render(&journal.text_state(&id)?.unwrap())?, doc);
    assert_eq!(journal.db.query_row("SELECT count(*) FROM outbox WHERE note = ?1", [&id], |row| row.get::<_, i64>(0))?,
        NOTE_CHECKPOINT_EDITS);
    Ok(())
}

#[cfg(test)]
#[test]
fn attachment_issues_identify_files_and_clear_after_retry() -> Result<()> {
    let fixture = HistoryFixture::new()?;
    let root = &fixture.0;
    let large = root.join("assets/too-large.bin");
    std::fs::File::create(&large)?.set_len(assets::LIMIT as u64 + 1)?;
    let mut journal = Journal::open(root, None)?;
    let mut store = Store::open_sqlite_at(root.clone())?;
    let id = store.create_from(&crate::docxml::parse(
        "<title>附件检查</title><p>普通正文</p><figure><source path=\"assets/too-large.bin\" name=\"too-large.bin\"/></figure><figure><source path=\"assets/missing.bin\" name=\"missing.bin\"/></figure>")?)?;
    journal.merge(&Default::default())?;
    let issues = journal.issues()?;
    let [issue] = issues.as_slice() else { anyhow::bail!("Expected one affected note") };
    assert_eq!(issue.id, id);
    assert_eq!(issue.title.as_deref(), Some("附件检查"));
    assert!(!issue.held);
    assert_eq!(issue.attachments, [
        AttachmentIssue { source: "assets/missing.bin".into(), reason: Some(assets::AssetProblem::Missing), stored_here: false, recovering: false },
        AttachmentIssue { source: "assets/too-large.bin".into(), reason: Some(assets::AssetProblem::Oversized), stored_here: true, recovering: false },
    ]);
    assert!(store.load(&id)?.plain_text().contains("普通正文"));

    std::fs::write(root.join("assets/missing.bin"), b"replacement for missing file")?;
    std::fs::write(large, b"smaller file")?;
    journal.retry_attachments()?;
    journal.merge(&Default::default())?;
    assert!(journal.issues()?.is_empty());
    assert_eq!(journal.counts()?.3, 0);
    Ok(())
}

#[cfg(test)]
#[test]
fn stalled_apply_refetches_a_missing_predecessor_without_discarding_queues() -> Result<()> {
    let fixture = HistoryFixture::new()?;
    let mut journal = Journal::open(&fixture.0, None)?;
    let mut store = Store::open_sqlite_at(fixture.0.clone())?;
    let id = store.create_from(&crate::docxml::parse("<title>Existing document</title><p>Local text</p>")?)?;
    journal.merge(&Default::default())?;
    let page = |edits, uploaded, token| Changes {
        retention: None, uploaded, edits, token: Some(super::Bytes(vec![token])),
        more: false, legacy: false, room_access: None, upload_quota_exceeded: false,
        #[cfg(feature = "devtools")] cursor_replayed: false,
    };
    let ids = journal.request()?.edits.into_iter().map(|edit| edit.id).collect();
    journal.receive(page(vec![], ids, 1))?;
    assert!(!journal.replay_pending_apply()?);

    let state = journal.text_state(&id)?.context("Missing local state")?;
    state.set_peer_id(journal.peer.wrapping_add(1))?;
    let start = state.oplog_vv();
    set_front(&state, "predecessor")?;
    state.commit();
    let predecessor = Edit { id: "missing".into(), note: id.clone(), collaborator: false,
        data: super::Bytes(state.export(ExportMode::updates(&start))?) };
    let after_first = state.oplog_vv();
    set_front(&state, "received later")?;
    state.commit();
    let later = Edit { id: "later".into(), note: id.clone(), collaborator: false,
        data: super::Bytes(state.export(ExportMode::updates(&after_first))?) };
    let new_local = store.create_from(&crate::docxml::parse("<title>Local outbox</title>")?)?;
    journal.receive(page(vec![later.clone()], vec![], 2))?;
    journal.merge(&Default::default())?;
    let prior = journal.counts()?;
    assert!(prior.0 > 0);
    assert_eq!(prior.1, 1);
    assert_eq!(journal.apply_wait.documents_awaiting_predecessor, 1);
    assert_ne!(journal.text_state(&id)?.unwrap().oplog_vv(), state.oplog_vv());

    assert!(journal.replay_pending_apply()?);
    assert!(!journal.replay_pending_apply()?);
    assert_eq!(journal.counts()?, prior);
    assert!(journal.request()?.token.is_none());
    journal.receive(page(vec![predecessor, later], vec![], 3))?;
    journal.merge(&Default::default())?;
    assert_eq!(journal.inbox_count()?, 0);
    assert_eq!(journal.apply_wait, Default::default());
    assert_eq!(journal.counts()?.0, prior.0);
    assert_eq!(journal.text_state(&id)?.unwrap().oplog_vv(), state.oplog_vv());
    assert!(store.list().iter().any(|entry| entry.id == new_local));
    Ok(())
}

#[cfg(test)]
#[test]
fn catalogue_continues_applying_independent_folders_while_a_predecessor_is_missing() -> Result<()> {
    let fixture = HistoryFixture::new()?;
    let remote = HistoryFixture::new()?;
    let mut local_store = Store::open_sqlite_at(fixture.0.clone())?;
    let mut remote_store = Store::open_sqlite_at(remote.0.clone())?;
    let mut journal = Journal::open(&fixture.0, None)?;
    let remote_journal = Journal::open(&remote.0, None)?;

    let local_folder = local_store.create_folder("Pending local folder", None)?;
    journal.merge(&Default::default())?;
    let outbox = journal.counts()?.0;
    assert!(outbox > 0);

    let first_folder = remote_store.create_folder("Folder missing its initial record", None)?;
    let independent = remote_store.create_folder("Independent folder", None)?;
    let catalogue = catalogue::local(&remote_journal.db)?;
    let page = |edits, token| Changes {
        retention: None, uploaded: vec![], edits, token: Some(super::Bytes(vec![token])),
        more: false, legacy: false, room_access: None, upload_quota_exceeded: false,
        #[cfg(feature = "devtools")] cursor_replayed: false,
    };
    let edit = |name: &str, doc: &LoroDoc, since| -> Result<Edit> {
        Ok(Edit { id: name.into(), note: catalogue::SCOPE.into(), collaborator: false,
            data: super::Bytes(doc.export(ExportMode::updates(&since))?) })
    };
    let delayed = LoroDoc::new();
    delayed.set_peer_id(50)?;
    let first: catalogue::Catalogue = [(first_folder.clone(), catalogue[&first_folder].clone())].into();
    catalogue::capture(&remote_journal.db, &delayed, &Default::default(), &first)?;
    let predecessor = edit("predecessor", &delayed, VersionVector::default())?;
    let before = delayed.oplog_vv();
    let mut renamed: catalogue::Catalogue = first.clone();
    renamed.get_mut(&first_folder).unwrap().insert("folder".into(), serde_json::json!("Recovered folder"));
    catalogue::capture(&remote_journal.db, &delayed, &first, &renamed)?;
    let pending = edit("pending", &delayed, before)?;
    let ready = LoroDoc::new();
    ready.set_peer_id(51)?;
    catalogue::capture(&remote_journal.db, &ready, &Default::default(),
        &[(independent.clone(), catalogue[&independent].clone())].into())?;

    journal.receive(page(vec![pending.clone(), edit("ready", &ready, Default::default())?], 1))?;
    journal.merge(&Default::default())?;
    assert!(journal.apply_wait.catalogue_predecessor);
    assert_eq!(journal.inbox_count()?, 1);
    let remaining: Vec<u8> = journal.db.query_row("SELECT data FROM inbox WHERE id='pending'", [], |row| row.get(0))?;
    assert_eq!(remaining, pending.data.0);
    let visible = catalogue::local(&journal.db)?;
    assert!(visible.contains_key(&independent));
    assert!(visible.contains_key(&local_folder));
    assert!(!visible.contains_key(&first_folder));
    assert_eq!(journal.counts()?.0, outbox);

    drop(journal);
    let mut reopened = Journal::open(&fixture.0, None)?;
    reopened.receive(page(vec![predecessor], 2))?;
    reopened.merge(&Default::default())?;
    assert_eq!(reopened.inbox_count()?, 0);
    assert_eq!(reopened.counts()?.0, outbox);
    let visible = catalogue::local(&reopened.db)?;
    assert_eq!(visible[&first_folder]["folder"], serde_json::json!("Recovered folder"));
    assert!(visible.contains_key(&independent) && visible.contains_key(&local_folder));
    Ok(())
}

#[cfg(test)]
#[test]
fn missing_cached_attachment_replays_without_dropping_pending_edits() -> Result<()> {
    let fixture = HistoryFixture::new()?;
    let root = &fixture.0;
    let path = root.join("assets/cached.bin");
    std::fs::write(&path, b"previously synced bytes")?;
    let mut journal = Journal::open(root, None)?;
    let mut store = Store::open_sqlite_at(root.clone())?;
    let id = store.create_from(&crate::docxml::parse(
        "<title>缓存附件</title><p>已同步正文</p><figure><source path=\"assets/cached.bin\" name=\"cached.bin\"/></figure>")?)?;
    journal.merge(&Default::default())?;
    let saved = journal.request()?.edits;
    let asset = saved.iter().find(|edit| edit.note == assets::SCOPE).context("Missing queued attachment")?.clone();
    journal.receive(Changes { retention: None, uploaded: saved.into_iter().map(|edit| edit.id).collect(), edits: vec![],
        token: Some(super::Bytes(vec![1])), more: false, legacy: false, room_access: None,
        #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false })?;
    let mut doc = store.load(&id)?;
    doc.blocks[0].text = crate::model::RichText::plain("仍待上传的正文");
    store.save(&id, &doc)?;
    journal.merge(&Default::default())?;
    let pending = journal.request()?.edits;
    assert!(!pending.is_empty());

    std::fs::remove_file(&path)?;
    assert!(journal.retry_attachments()?);
    drop(journal);
    let mut journal = Journal::open(root, None)?;
    assert!(journal.request()?.token.is_none());
    assert!(journal.issues()?[0].attachments[0].recovering);
    journal.receive(Changes { retention: None, uploaded: vec![], edits: vec![asset], token: Some(super::Bytes(vec![2])),
        more: false, legacy: false, room_access: None, #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false })?;
    assert!(journal.merge(&Default::default())?.2, "A repaired attachment must refresh open editors");
    assert_eq!(std::fs::read(path)?, b"previously synced bytes");
    assert!(journal.issues()?.is_empty());
    assert_eq!(journal.request()?.edits.iter().map(|edit| &edit.id).collect::<Vec<_>>(),
        pending.iter().map(|edit| &edit.id).collect::<Vec<_>>());
    assert!(store.load(&id)?.plain_text().contains("仍待上传的正文"));
    Ok(())
}

#[cfg(test)]
#[test]
fn restores_existing_xml_and_compressed_versions_from_the_same_library() -> Result<()> {
    let fixture = HistoryFixture::new()?;
    let id = uuid::Uuid::new_v4().to_string();
    let old = crate::docxml::parse("<title>旧存档</title><p>包含中文和格式 <b>旧</b></p>")?;
    let mut new = old.clone();
    new.blocks.push(crate::model::Block::text(&"云同步版本".repeat(800)));
    let front = "---\nstatus: draft\n---\n";
    {
        let journal = Journal::open(&fixture.0, None)?;
        journal.db.pragma_update(None, "user_version", 3)?;
        journal.db.execute("ALTER TABLE sync_history DROP COLUMN recovery", [])?;
        journal.db.execute("INSERT INTO sync_history (note, body, front) VALUES (?1, ?2, ?3)",
            params![id, crate::docxml::serialize(&old), front])?;
    }
    Store::open_sqlite_at(fixture.0.clone())?;
    let journal = Journal::open(&fixture.0, None)?;
    assert_eq!(journal.db.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?, 7);
    assert!(journal.db.query_row("SELECT recovery FROM sync_history WHERE note = ?1", [&id], |row| row.get::<_, bool>(0))?);
    archive(&journal.db, &id, &old, front, true)?;
    archive(&journal.db, &id, &new, front, true)?;
    archive(&journal.db, &id, &new, front, true)?;
    let types: Vec<String> = journal.db.prepare("SELECT typeof(body) FROM sync_history ORDER BY sequence")?
        .query_map([], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
    assert_eq!(types, ["text", "blob"]);
    let versions = journal.versions(&id)?;
    assert_eq!(versions.len(), 2);
    for (version, expected) in versions.into_iter().zip([new, old]) {
        let (doc, metadata) = journal.version(&id, version.0)?;
        assert_eq!(crate::docxml::serialize(&doc), crate::docxml::serialize(&expected));
        assert_eq!(metadata, front);
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn linear_history_is_bounded_without_expiring_conflict_recovery() -> Result<()> {
    use crate::model::{Block, RichText};
    let fixture = HistoryFixture::new()?;
    let mut journal = Journal::open(&fixture.0, None)?;
    let id = uuid::Uuid::new_v4().to_string();
    let mut document = Document { blocks: vec![Block::text("起点")], ..Default::default() };
    let source = content::open(&content::seed(&document)?)?;
    let deliver = |journal: &mut Journal, payloads: Vec<Vec<u8>>| -> Result<()> {
        let edits = payloads.into_iter().map(|bytes| Edit { id: uuid::Uuid::new_v4().to_string(), note: id.clone(),
            data: super::Bytes(bytes), collaborator: false }).collect();
        journal.receive(Changes { retention: None, uploaded: vec![], edits, token: Some(super::Bytes(vec![1])),
            more: false, legacy: false, room_access: None, #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false })?;
        journal.merge(&Default::default())?;
        Ok(())
    };
    let edit = |loro: &LoroDoc, peer, doc: &mut Document, text| -> Result<Vec<u8>> {
        doc.blocks[0].text = RichText::plain(text);
        content::edit(loro, peer, doc)?.context("Missing test change")
    };
    let count = |journal: &Journal, recovery| -> Result<i64> {
        Ok(journal.db.query_row("SELECT count(*) FROM sync_history WHERE note = ?1 AND recovery = ?2",
            params![id, recovery], |row| row.get(0))?)
    };
    deliver(&mut journal, vec![source.export(ExportMode::Snapshot)?])?;
    let mut linear = (1..=14).map(|n| edit(&source, 10, &mut document, format!("顺序 {n}")))
        .collect::<Result<Vec<_>>>()?;
    linear.reverse();
    deliver(&mut journal, linear)?;
    assert_eq!(count(&journal, false)?, RECENT_VERSIONS);
    assert_eq!(count(&journal, true)?, 0);
    let latest = journal.versions(&id)?[0].0;
    assert_eq!(journal.version(&id, latest)?.0.blocks[0].text.text, "顺序 13");

    let seed = source.export(ExportMode::Snapshot)?;
    let left = content::open(&seed)?;
    let right = content::open(&seed)?;
    deliver(&mut journal, vec![
        edit(&left, 11, &mut document.clone(), "并发甲".to_owned())?,
        edit(&right, 12, &mut document, "并发乙".to_owned())?,
    ])?;
    let protected = count(&journal, true)?;
    assert!(protected >= 2);
    let versions = journal.db.prepare("SELECT body FROM sync_history WHERE note = ?1 AND recovery = 1")?
        .query_map([&id], |row| {
            let doc = archived_document(row.get_ref(0)?)
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))?;
            Ok(doc.blocks[0].text.text.clone())
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
    assert!(versions.iter().any(|text| text == "并发甲"));
    assert!(versions.iter().any(|text| text == "并发乙"));
    let source = journal.text_state(&id)?.context("Missing merged test note")?;
    let mut document = content::render(&source)?;
    let linear = (15..=29).map(|n| edit(&source, 10, &mut document, format!("顺序 {n}")))
        .collect::<Result<Vec<_>>>()?;
    deliver(&mut journal, linear)?;
    assert_eq!(count(&journal, false)?, RECENT_VERSIONS);
    assert_eq!(count(&journal, true)?, protected);
    Ok(())
}

#[cfg(test)]
#[test]
#[ignore = "measures local database growth across hundreds of sync records"]
fn measure_history_growth() -> Result<()> {
    let fixture = HistoryFixture::new()?;
    let db = Connection::open(fixture.0.join(SQLITE_FILE))?;
    let id = uuid::Uuid::new_v4().to_string();
    let mut document = Document {
        blocks: (0..40).map(|i| crate::model::Block::text(&format!("第{i}段 {}", "海盐橙子".repeat(90)))).collect(),
        ..Default::default()
    };
    let source = content::open(&content::seed(&document)?)?;
    let mut journal = Journal::open(&fixture.0, None)?;
    let mut batch = vec![Edit { id: uuid::Uuid::new_v4().to_string(), note: id.clone(),
        data: super::Bytes(source.export(ExportMode::Snapshot)?), collaborator: false }];
    let start = std::time::Instant::now();
    let mut record_bytes = 0;
    for index in 0..=600u64 {
        if index > 0 {
            let text = &mut document.blocks[0].text;
            text.replace(0.."甲".len().min(text.text.len()), &crate::model::RichText::plain(if index % 2 == 0 { "甲" } else { "乙" }));
            let at = (index as usize) % document.blocks.len();
            let text = &mut document.blocks[at].text;
            text.replace(0..0, &crate::model::RichText::plain(format!("{index:03} ")));
            let update = content::edit(&source, 2, &document)?.context("Missing benchmark edit")?;
            record_bytes += update.len();
            batch.push(Edit { id: uuid::Uuid::new_v4().to_string(), note: id.clone(), data: super::Bytes(update), collaborator: false });
        }
        if index % 20 == 0 {
            journal.receive(Changes { retention: None, uploaded: vec![], edits: std::mem::take(&mut batch), token: Some(super::Bytes(vec![1])),
                more: false, legacy: false, room_access: None, #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false })?;
            journal.merge(&Default::default())?;
        }
        if matches!(index, 0 | 200 | 400 | 600) {
            db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
            let (versions, archive): (i64, i64) = db.query_row("SELECT COUNT(*), coalesce(sum(length(CAST(body AS BLOB))),0) FROM sync_history", [],
                |row| Ok((row.get(0)?, row.get(1)?)))?;
            let bytes = std::fs::metadata(fixture.0.join(SQLITE_FILE))?.len();
            let snapshot: i64 = db.query_row("SELECT length(snapshot) FROM notes WHERE id = ?1", [&id], |r| r.get(0))?;
            println!("HISTORY edits={index} versions={versions} archive={archive} db={bytes} snapshot={snapshot} cloud_updates={record_bytes} seconds={:.2}", start.elapsed().as_secs_f64());
        }
    }
    assert_eq!(content::render(&journal.text_state(&id)?.unwrap())?, document);
    assert_eq!(journal.counts()?, (0, 0, 0, 0));
    Ok(())
}


#[cfg(test)]
#[test]
fn local_version_restore_commits_cloud_state_atomically_and_obeys_shared_access() -> Result<()> {
    let fixture = HistoryFixture::new()?;
    let mut journal = Journal::open(&fixture.0, None)?;
    let mut raw = Store::open_sqlite_at(fixture.0.clone())?;
    let old = crate::docxml::parse("<title>历史恢复</title><p>第一版</p><table><tr><td><p>单元格</p></td></tr></table>")?;
    let id = raw.create_from(&old)?;
    let mut store = SharedStore::for_test(raw);
    journal.merge(&Default::default())?;
    let mut edited = store.load_for_edit(&id, false)?;
    edited.blocks[0].text = crate::model::RichText::plain("第二版");
    journal.save(&mut store, &id, &Core::new(edited.clone()))?;
    let version = store.versions(&id)[0].name.clone();
    let restored = store.restore_version(&id, &version)?;
    assert_eq!(restored.blocks[0].text.text, "第一版");
    let queue = journal.counts()?.0;
    let before = store.load(&id)?;
    journal.db.execute_batch("CREATE TRIGGER reject_test_outbox BEFORE INSERT ON outbox BEGIN SELECT RAISE(ABORT, 'injected durable queue failure'); END;")?;
    assert!(journal.save(&mut store, &id, &Core::new(restored.clone())).is_err());
    assert_eq!(store.load(&id)?, before);
    assert_eq!(journal.counts()?.0, queue);
    journal.db.execute_batch("DROP TRIGGER reject_test_outbox;")?;
    journal.save(&mut store, &id, &Core::new(restored.clone()))?;
    assert!(journal.counts()?.0 > queue);
    assert_eq!(store.load(&id)?, restored);
    let state = journal.text_state(&id)?.context("Missing restored CRDT")?;
    assert_eq!(content::render(&state)?.blocks[0].text.text, "第一版");
    journal.room = Some(shared::Scope { note: id.clone(), owner: Some("synthetic-owner".into()), folder: false });
    for access in [Access::ReadOnly, Access::Revoked] {
        journal.access = Some(access);
        let queue = journal.counts()?.0;
        assert!(journal.save(&mut store, &id, &Core::new(edited.clone())).is_err());
        assert_eq!(store.load(&id)?, restored);
        assert_eq!(journal.counts()?.0, queue);
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn legacy_reuse_after_cloud_collection_does_not_resend_file() -> Result<()> {
    let library = HistoryFixture::new()?;
    let mut journal = Journal::open(&library.0, None)?;
    let mut store = Store::open_sqlite_at(library.0.clone())?;
    let name = "assets/kept-offline.bin";
    std::fs::write(library.0.join(name), vec![7u8; 40_000])?;
    let with_image = crate::docxml::parse(&format!("<title>Test</title><p>Text</p><figure><source path=\"{name}\" name=\"kept.bin\"/></figure>"))?;
    let without_image = crate::docxml::parse("<title>Test</title><p>Text</p>")?;
    let id = store.create_from(&with_image)?;
    journal.merge(&Default::default())?;
    let uploaded = journal.request()?.edits;
    assert!(uploaded.iter().any(|edit| edit.note == assets::SCOPE));
    let ack = |records: Vec<Edit>| Changes { retention: None, uploaded: records.into_iter().map(|edit| edit.id).collect(),
        edits: vec![], token: Some(super::Bytes(vec![1])), more: false, legacy: false, room_access: None,
        #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false };
    journal.receive(ack(uploaded))?;
    store.save(&id, &without_image)?;
    journal.merge(&Default::default())?;
    journal.receive(ack(journal.request()?.edits))?;
    // An old device cannot observe a generation retired by another client.
    store.save(&id, &with_image)?;
    journal.merge(&Default::default())?;
    let pending = journal.request()?.edits;
    assert!(pending.iter().any(|edit| edit.note == id));
    assert!(!pending.iter().any(|edit| edit.note == assets::SCOPE));
    assert_eq!(std::fs::metadata(library.0.join(name))?.len(), 40_000);
    Ok(())
}

#[cfg(test)]
#[test]
fn cloud_checkpoint_restores_and_accepts_an_offline_branch() -> Result<()> {
    let first = HistoryFixture::new()?;
    let mut j = Journal::open(&first.0, None)?;
    let mut store = Store::open_sqlite_at(first.0.clone())?;
    let id = store.create_from(&crate::docxml::parse("<title>Recovery</title><p>initial</p>")?)?;
    j.merge(&Default::default())?;
    let old = j.text_state(&id)?.unwrap();
    old.set_peer_id(j.peer.wrapping_add(1))?;
    let old_base = old.oplog_vv();
    let mut doc = store.load(&id)?;
    for n in 0..40 {
        doc.blocks[0].text = crate::model::RichText::plain(format!("local edit {n}"));
        store.save(&id,&doc)?;
        j.merge(&Default::default())?;
    }
    let receipt = |edits: Vec<Edit>, deleted| Changes { retention: Some(retention::Receipt {
        reset: false, ready: true, deleted, chunks: vec![] }), uploaded: edits.iter().map(|e| e.id.clone()).collect(),
        edits, token: Some(super::Bytes(vec![1])), more: false, legacy: false, room_access: None,
        #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false };
    let mut catalogue = vec![];
    loop {
        let edits = j.request()?.edits;
        if edits.is_empty() { break; }
        catalogue.extend(edits.iter().filter(|e| e.note == super::catalogue::SCOPE).cloned());
        j.receive(receipt(edits, vec![]))?;
        j.merge(&Default::default())?;
    }
    let plan = j.request()?.maintenance.context("No cloud checkpoint")?;
    assert!(plan.remove.is_empty());
    let checkpoint = plan.snapshot.unwrap();
    j.receive(receipt(vec![checkpoint.clone()],vec![]))?;
    j.merge(&Default::default())?;
    let cleanup = j.request()?.maintenance.context("Missing verified cleanup")?;
    assert!(cleanup.snapshot.is_none() && cleanup.remove.len()>=40);
    let second = HistoryFixture::new()?;
    let mut restored = Journal::open(&second.0,None)?;
    catalogue.push(checkpoint);
    restored.receive(receipt(catalogue,vec![]))?;
    restored.merge(&Default::default())?;
    assert_eq!(content::render(&restored.text_state(&id)?.unwrap())?, doc);
    old.get_map("extra").insert("offline","kept")?;
    let late = Edit { id: uuid::Uuid::new_v4().to_string(), note: id.clone(),
        data: super::Bytes(old.export(ExportMode::updates(&old_base))?), collaborator: false };
    j.receive(receipt(vec![late.clone()], cleanup.remove))?;
    restored.receive(receipt(vec![late],vec![]))?;
    for local in [&mut j,&mut restored] { local.merge(&Default::default())?; }
    assert_eq!(j.text_state(&id)?.unwrap().oplog_vv(), restored.text_state(&id)?.unwrap().oplog_vv());
    assert_eq!(content::value(&restored.text_state(&id)?.unwrap().get_map("extra"),"offline").as_deref(),Some("kept"));
    Ok(())
}

#[cfg(test)]
#[test]
fn clean_asset_is_not_downloaded_and_reuse_uploads_bytes_first() -> Result<()> {
    let f = HistoryFixture::new()?;
    let mut j = Journal::open(&f.0,None)?;
    let mut store = Store::open_sqlite_at(f.0.clone())?;
    let name = "assets/data.bin";
    std::fs::write(f.0.join(name), b"offline test bytes")?;
    let with_file = crate::docxml::parse(&format!("<title>Test</title><p>x</p><figure><source path=\"{name}\" name=\"x\"/></figure>"))?;
    let id = store.create_from(&with_file)?;
    j.merge(&Default::default())?;
    let mut file_id=String::new();
    let mut uploaded = Vec::new();
    let page = |edits: Vec<Edit>, deleted: Vec<String>| Changes { retention: Some(retention::Receipt {
        reset:false, ready:true, deleted, chunks: vec![] }), uploaded: edits.iter().map(|e| e.id.clone()).collect(),
        edits, token: Some(super::Bytes(vec![1])), more:false, legacy:false, room_access:None,
        #[cfg(feature="devtools")] cursor_replayed:false, upload_quota_exceeded:false };
    loop {
        let edits=j.request()?.edits;
        if edits.is_empty() { break; }
        if let Some(e)=edits.iter().find(|e| e.note==assets::SCOPE) { file_id=e.id.clone(); }
        uploaded.extend(edits.iter().cloned());
        j.receive(page(edits,vec![]))?;
        j.merge(&Default::default())?;
    }
    assert!(!file_id.is_empty());
    // A reinstalled client sees two attachment headers, but only the file
    // referenced by the projected note should get a byte-range request.
    let fresh_library = HistoryFixture::new()?;
    let mut fresh = Journal::open(&fresh_library.0, None)?;
    let expected = uploaded.iter().find(|e| e.note == assets::SCOPE).unwrap().clone();
    let (header, _) = assets::decode(&expected.data.0)?;
    uploaded.retain(|e| e.note != assets::SCOPE);
    let mut metadata = page(uploaded, vec![]);
    metadata.retention.as_mut().unwrap().chunks = vec![
        retention::RemoteChunk { id: file_id.clone(), path: name.into(), digest: header.digest.clone(), part: 0, parts: 1 },
        retention::RemoteChunk { id: uuid::Uuid::new_v4().to_string(), path: "assets/retired.bin".into(), digest: header.digest, part: 0, parts: 1 },
    ];
    fresh.receive(metadata)?;
    fresh.merge(&Default::default())?;
    assert_eq!(fresh.request()?.downloads, [file_id.clone()]);
    fresh.receive(page(vec![expected], vec![]))?;
    fresh.merge(&Default::default())?;
    assert_eq!(std::fs::read(fresh_library.0.join(name))?, b"offline test bytes");
    assert!(!fresh_library.0.join("assets/retired.bin").exists());
    assert!(fresh.request()?.downloads.is_empty());
    j.db.execute("UPDATE cloud_inventory SET observed=0",[])?;
    assert!(retention::stale_assets(&j.db,&j.retained_paths()?)?.is_none());
    let plain=crate::docxml::parse("<title>Test</title><p>plain</p>")?;
    store.save(&id,&plain)?;
    j.merge(&Default::default())?;
    let pending=j.request()?.edits;
    j.receive(page(pending,vec![]))?;
    j.merge(&Default::default())?;
    let versions = store.versions(&id);
    assert!(!versions.is_empty());
    assert!(j.retained_paths()?.contains(name));
    for version in versions { store.delete_version(&id,&version.name)?; }
    let stale=retention::stale_assets(&j.db,&j.retained_paths()?)?.context("Unreferenced attachment not collectable")?;
    assert_eq!(stale.remove,[file_id.clone()]);
    assert!(j.request()?.downloads.is_empty());
    store.save(&id,&with_file)?;
    j.merge(&Default::default())?;
    j.receive(page(vec![],vec![file_id]))?;
    let reupload=j.request()?.edits;
    assert!(!reupload.is_empty() && reupload.iter().all(|e| e.note==assets::SCOPE));
    assert_eq!(assets::decode(&reupload[0].data.0)?.1,b"offline test bytes");
    j.receive(page(reupload,vec![]))?;
    assert!(j.request()?.edits.iter().any(|e| e.note==id));
    Ok(())
}

#[cfg(test)]
#[test]
fn cloud_checkpoint_cursor_reset_preserves_retention_metadata() -> Result<()> {
    let first = HistoryFixture::new()?;
    let mut j = Journal::open(&first.0, None)?;
    let mut store = Store::open_sqlite_at(first.0.clone())?;
    std::fs::write(first.0.join("assets/audit.bin"), b"synthetic audit attachment")?;
    let id = store.create_from(&crate::docxml::parse("<title>Recovery</title><p>initial</p><figure><source path=\"assets/audit.bin\" name=\"audit\"/></figure>")?)?;
    j.merge(&Default::default())?;
    let mut doc = store.load(&id)?;
    for n in 0..40 {
        if n == 39 { doc.blocks.truncate(1); }
        doc.blocks[0].text = crate::model::RichText::plain(format!("local edit {n}"));
        store.save(&id,&doc)?;
        j.merge(&Default::default())?;
    }
    let receipt = |edits: Vec<Edit>, deleted| Changes { retention: Some(retention::Receipt {
        reset: false, ready: true, deleted, chunks: vec![] }), uploaded: edits.iter().map(|e| e.id.clone()).collect(),
        edits, token: Some(super::Bytes(vec![1])), more: false, legacy: false, room_access: None,
        #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false };
    let mut catalogue = vec![];
    loop {
        let edits = j.request()?.edits;
        if edits.is_empty() { break; }
        catalogue.extend(edits.iter().filter(|e| e.note == super::catalogue::SCOPE).cloned());
        j.receive(receipt(edits, vec![]))?;
        j.merge(&Default::default())?;
    }
    let plan = j.request()?.maintenance.context("No cloud checkpoint")?;
    assert!(plan.remove.is_empty());
    let checkpoint = plan.snapshot.unwrap();
    j.receive(receipt(vec![checkpoint.clone()],vec![]))?;
    j.merge(&Default::default())?;
    let cleanup = j.request()?.maintenance.context("Missing verified cleanup")?;
    assert!(cleanup.snapshot.is_none() && cleanup.remove.len()>=40);
    let mut results = Vec::new();
    for reset in [false, true] {
        let second = HistoryFixture::new()?;
        let mut restored = Journal::open(&second.0, None)?;
        let mut edits = catalogue.clone(); edits.push(checkpoint.clone());
        let mut page = receipt(edits, vec![]);
        page.retention.as_mut().unwrap().reset = reset;
        restored.receive(page)?;
        restored.merge(&Default::default())?;
        assert_eq!(content::render(&restored.text_state(&id)?.unwrap())?, doc);
        let counts: (i64,i64,i64) = restored.db.query_row(
            "SELECT (SELECT count(*) FROM cloud_covered), (SELECT count(*) FROM cloud_checkpoint_anchors), (SELECT count(*) FROM cloud_checkpoint_paths)",
            [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        eprintln!("AUDIT reset={reset} covered={} anchors={} paths={} body_restored=true", counts.0,counts.1,counts.2);
        results.push(counts);
    }
    assert!(results[0].0 > 0 && results[0].1 > 0 && results[0].2 > 0);
    assert_eq!(results[0], results[1], "Cursor reset must retain the checkpoint's verified coverage, version anchors and attachment pins");
    Ok(())
}

#[cfg(test)]
#[test]
fn shared_personal_preferences_sync_only_through_the_personal_catalogue() -> Result<()> {
    use crate::sharing::preferences::{KEY, Preferences};
    let device_a = HistoryFixture::new()?;
    let device_b = HistoryFixture::new()?;
    let other_account = HistoryFixture::new()?;
    std::fs::write(other_account.0.join(".cloud-restore-ready"), format!("{}\n", "b".repeat(64)))?;
    let shared_cache = HistoryFixture::new()?;
    let id = uuid::Uuid::new_v4().to_string();
    let other_doc = uuid::Uuid::new_v4().to_string();
    let room = shared::Room { scope: shared::Scope { note: id.clone(), owner: None, folder: false }, title: "Synthetic share".into() };
    std::fs::write(shared_cache.0.join(shared::MARKER), serde_json::to_vec(&room)?)?;
    std::fs::write(shared_cache.0.join(".cloud-restore-ready"), format!("{}\nshared\n", "a".repeat(64)))?;
    let read = |db: &Connection| -> Result<Preferences> {
        let raw: Vec<u8> = db.query_row("SELECT value FROM library_values WHERE key=?1", [KEY], |row| row.get(0))?;
        Ok(serde_json::from_slice(&raw)?)
    };
    let write = |db: &Connection, prefs: &Preferences| -> Result<()> {
        db.execute("INSERT OR REPLACE INTO library_values VALUES (?1,?2)", params![KEY, serde_json::to_vec(prefs)?])?;
        Ok(())
    };
    let page = |edits: Vec<Edit>, uploaded| Changes {
        retention: None, uploaded, edits, token: Some(super::Bytes(vec![1])), more: false, legacy: false,
        room_access: None, upload_quota_exceeded: false, #[cfg(feature = "devtools")] cursor_replayed: false,
    };
    let a_db = Connection::open(device_a.0.join(crate::store::SQLITE_FILE))?;
    let b_db = Connection::open(device_b.0.join(crate::store::SQLITE_FILE))?;
    let legacy = serde_json::json!({"pinned":[&id],"starred":{(id.clone()): 123}});
    a_db.execute("INSERT INTO library_values VALUES ('shared-organization', ?1)", [serde_json::to_vec(&legacy)?])?;
    a_db.execute("INSERT INTO library_values VALUES ('shared-sidebar-order', ?1)", [serde_json::to_vec(&[&id, &other_doc])?])?;
    let mut a = Journal::open(&device_a.0, None)?;
    let mut b = Journal::open(&device_b.0, None)?;
    a.merge(&Default::default())?;
    let first = a.request()?.edits;
    assert!(first.iter().all(|edit| edit.note == catalogue::SCOPE));
    assert!(!first.is_empty());
    a.receive(page(vec![], first.iter().map(|edit| edit.id.clone()).collect()))?;
    b.receive(page(first, vec![]))?;
    b.merge(&Default::default())?;
    let mut left = read(&a_db)?;
    let mut right = read(&b_db)?;
    assert!(left == right);
    assert_eq!((right.get("pin", &id), right.get("star", &id)), (1, 123));
    assert!(right.get("rank", &id) < right.get("rank", &other_doc));

    // Concurrent changes to different fields survive in the same account.
    left.toggle(&id, true);
    left.toggle(&id, false);
    right.move_item(vec![id.as_str(), other_doc.as_str()], &other_doc, Some(&id));
    write(&a_db, &left)?;
    write(&b_db, &right)?;
    a.merge(&Default::default())?;
    b.merge(&Default::default())?;
    let from_a = a.request()?.edits;
    let from_b = b.request()?.edits;
    a.receive(page(from_b, vec![]))?;
    b.receive(page(from_a, vec![]))?;
    a.merge(&Default::default())?;
    b.merge(&Default::default())?;
    let left = read(&a_db)?;
    let right = read(&b_db)?;
    assert!(left == right);
    assert_eq!((right.get("pin", &id), right.get("star", &id)), (0, 0));
    assert!(right.get("rank", &other_doc) < right.get("rank", &id));

    let mut unrelated = Journal::open(&other_account.0, None)?;
    unrelated.merge(&Default::default())?;
    assert!(read(&unrelated.db).is_err());
    let mut shared_journal = Journal::open(&shared_cache.0, None)?;
    write(&shared_journal.db, &right)?;
    shared_journal.merge(&Default::default())?;
    assert!(shared_journal.request()?.edits.is_empty());
    Ok(())
}

#[cfg(test)]
mod conflict_tests;
