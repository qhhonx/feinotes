// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! The editor and sync journal share one SQLite database. The sync cursor,
//! received batches, CRDT snapshots and their library projection are durable.

use anyhow::{Context, Result, ensure};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use loro::{ExportMode, Frontiers, LoroDoc, VersionVector};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use rusqlite::types::{Value, ValueRef};
use std::{collections::{HashMap, HashSet}, fs::{File, OpenOptions}, io::{Read, Write}, path::{Path, PathBuf}, time::Duration};
use crate::{editor::core::{Core, Pos, keep_outside_history}, model::Document, shared_store::SharedStore, store::{Meta, Store, SQLITE_FILE}};
use super::{AttachmentIssue, Changes, Edit, Exchange, NoteChange, SyncIssue, activity, assets, catalogue, content};
use super::history::History;
use super::shared::{self, Access};

pub struct Journal {
    db: Connection,
    _lock: File,
    root: PathBuf,
    peer: u64,
    account: String,
    channel: Option<String>,
    room: Option<shared::Scope>,
    access: Option<Access>,
    merged_data_version: Option<i64>,
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

fn import_note_updates(db: &Connection, id: &str, loro: &LoroDoc) -> Result<()> {
    let updates = db.prepare("SELECT data FROM note_local_updates WHERE note = ?1 ORDER BY rowid")?
        .query_map([id], |row| row.get::<_, Vec<u8>>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    if !updates.is_empty() {
        ensure!(loro.import_batch(&updates)?.pending.is_none(), "Incomplete local iCloud edit history");
    }
    Ok(())
}

fn open_note(db: &Connection, id: &str, snapshot: &[u8]) -> Result<LoroDoc> {
    let loro = if snapshot.is_empty() { content::configured() } else { content::open(snapshot)? };
    import_note_updates(db, id, &loro)?;
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

fn get_state(db: &Connection, key: &str) -> Result<Option<Vec<u8>>> {
    Ok(db.query_row("SELECT value FROM state WHERE key = ?1", [key], |row| row.get(0)).optional()?)
}

fn put_state(db: &Connection, key: &str, data: &[u8]) -> Result<()> {
    db.execute("INSERT OR REPLACE INTO state VALUES (?1, ?2)", params![key, data])?;
    Ok(())
}

pub fn enqueue(db: &Connection, scope: &str, update: Vec<u8>) -> Result<()> {
    ensure!(!update.is_empty() && update.len() <= 16 * 1024 * 1024, "An iCloud edit exceeds the record size limit");
    db.execute("INSERT INTO outbox VALUES (?1, ?2, ?3)",
        params![uuid::Uuid::new_v4().to_string(), scope, update])?;
    Ok(())
}

fn incoming(db: &Connection, scope: &str) -> Result<Vec<Vec<u8>>> {
    Ok(db.prepare("SELECT data FROM inbox WHERE note = ?1 ORDER BY rowid")?
        .query_map([scope], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?)
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

const RECENT_VERSIONS: i64 = 6;

fn archive(db: &Connection, id: &str, doc: &Document, front: &str, recovery: bool) -> Result<i64> {
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

pub(super) fn note_version(db: &Connection, id: &str) -> Result<Option<VersionVector>> {
    note(db, id)?.filter(|note| !note.snapshot.is_empty())
        .map(|note| open_note(db, id, &note.snapshot).map(|doc| doc.oplog_vv())).transpose()
}

pub fn archive_body(db: &Connection, id: &str, deleted_version: Option<&VersionVector>, markdown: bool) -> Result<()> {
    let trashed = db.query_row("SELECT trashed FROM library_documents WHERE id = ?1", [id],
        |row| row.get::<_, bool>(0)).optional()?;
    if let Some(trashed) = trashed {
        let (doc, _, front) = Store::read_sync_body(db, id, trashed)?;
        let sequence = archive(db, id, &doc, &front, true)?;
        let held: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM notes WHERE id = ?1 AND held <> 0)", [id], |row| row.get(0))?;
        let after_delete = match deleted_version {
            Some(deleted) => note_version(db, id)?.is_some_and(|local| !deleted.includes_vv(&local)),
            None => db.query_row("SELECT EXISTS(SELECT 1 FROM outbox WHERE note = ?1)", [id], |row| row.get(0))?,
        };
        if held || after_delete {
            db.execute("INSERT OR IGNORE INTO offline_recoveries VALUES (?1, ?2, ?3, ?4)", params![id, sequence, doc.title.text, markdown])?;
        }
    }
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

fn revision(db: &Connection, id: &str) -> Result<(i64, String)> {
    Ok(db.query_row("SELECT revision, front FROM library_documents WHERE id = ?1",
        [id], |row| Ok((row.get(0)?, row.get(1)?)))?)
}

impl Journal {
    pub fn open(root: &Path, channel: Option<String>) -> Result<Self> {
        let lock = OpenOptions::new().write(true).create(true).truncate(false).open(root.join("sync.lock"))?;
        lock.try_lock().context("This document library is already open")?;
        let marker = std::fs::read_to_string(root.join(".cloud-restore-ready"))?;
        let account = marker.lines().next().filter(|a| a.len() == 64).context("Invalid library account")?.to_string();
        let room = shared::descriptor(root)?.map(|room| room.scope);
        ensure!(room.is_some() == (marker.lines().nth(1) == Some("shared")) && (room.is_none() || channel.is_none()),
            "Shared note and personal library cannot use the same sync journal");
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
        let has_recovery = db.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('sync_history') WHERE name = 'recovery')",
            [], |row| row.get::<_, bool>(0))?;
        // Preexisting versions may include conflict copies. Protect all of
        // them when opening a library from a build that did not classify them.
        if !has_recovery { db.execute("ALTER TABLE sync_history ADD COLUMN recovery INTEGER NOT NULL DEFAULT 1", [])?; }
        if room.is_some() { activity::schema(&db)?; }
        let tx = db.transaction()?;
        let scope = room.as_ref().map(serde_json::to_vec).transpose()?;
        match get_state(&tx, "shared_scope")? {
            Some(saved) => ensure!(scope.as_ref() == Some(&saved), "Shared note scope changed"),
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
        Ok(Self { db, _lock: lock, root: root.to_path_buf(), peer: peer as u64, account, channel, room, access,
            merged_data_version: None, history: HashMap::new() })
    }

    pub fn room(&self) -> Option<&shared::Scope> { self.room.as_ref() }

    pub fn access(&self) -> Option<Access> { self.access }

    pub fn seed_shared(&mut self, doc: &Document, front: &str) -> Result<()> {
        let room = self.room.as_ref().context("Not a shared note")?;
        ensure!(self.access == Some(Access::Owner), "Only the owner can seed a shared note");
        let tx = self.db.transaction()?;
        ensure!(note(&tx, &room.note)?.is_none(), "Shared note is already initialized");
        let seen = Store::write_sync_body(&tx, &room.note, false, doc, front)?;
        Store::show_shared_note(&tx, &room.note, front)?;
        ensure!(!capture_note(&tx, &self.root, self.peer, &room.note, doc, seen, front)?,
            "The shared note or one of its attachments is unavailable or too large");
        tx.commit()?;
        Ok(())
    }

    fn ensure_writable(&self, id: &str) -> Result<()> {
        if let Some(room) = &self.room {
            ensure!(id == room.note && self.access.is_some_and(Access::can_write), "This shared note is read-only");
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
        result.map(|(meta, (warning, _))| (meta, warning))
    }

    pub fn undo(&mut self, store: &mut SharedStore, id: &str, core: &Core, redo: bool) -> Result<Option<NoteChange>> {
        self.ensure_writable(id)?;
        let root = self.root.clone();
        let peer = self.peer;
        let Some((old, history)) = self.history(id)? else { return Ok(None) };
        let before = history.doc.oplog_vv();
        let anchors = [core.sel.anchor, core.sel.head].into_iter()
            .filter_map(|pos| content::anchor(&history.doc, pos).map(|cursor| (pos, cursor))).collect::<Vec<_>>();
        let result = (|| {
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
            Ok(Some((NoteChange { id: id.into(), document, positions }, held)))
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
            let (mut doc, front): (Document, String) = {
                let mut stmt = tx.prepare("SELECT body, front FROM sync_history WHERE note = ?1 AND sequence = ?2")?;
                let mut rows = stmt.query(params![id, sequence])?;
                let row = rows.next()?.context("The recovery copy is missing.")?;
                (archived_document(row.get_ref(0)?)?, row.get(1)?)
            };
            let title = crate::settings::display_title(&doc.title.text);
            doc.title = crate::model::RichText::plain(crate::localized_format!("{} · 恢复副本", "{} · recovered", title));
            let new = uuid::Uuid::new_v4().to_string();
            let now = crate::time::now();
            let name = markdown.then(|| doc.title.text.clone());
            let meta = Meta { created: now, modified: now, markdown, name, ..Default::default() };
            Store::write_sync_body(&tx, &new, false, &doc, &front)?;
            tx.execute("INSERT INTO library_entries VALUES (?1, ?2)", params![new, serde_json::to_string(&meta)?])?;
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

    pub fn request_with_uploads(&self, allow_upload: bool) -> Result<Exchange> {
        let allow_upload = allow_upload && self.access.is_none_or(Access::can_write);
        let mut stmt = self.db.prepare("SELECT id, note, length(data), data FROM outbox WHERE ?1 ORDER BY rowid LIMIT 20")?;
        let mut rows = stmt.query([allow_upload])?;
        let mut edits = vec![];
        let mut bytes = 0i64;
        while let Some(row) = rows.next()? {
            let note: String = row.get(1)?;
            ensure!(self.room.as_ref().is_none_or(|room| room.permits(&note)), "Unrelated content was queued in a shared note");
            let size: i64 = row.get(2)?;
            // Allow a large record by itself, while batching small note edits.
            if !edits.is_empty() && bytes + size > 8 * 1024 * 1024 { break; }
            edits.push(Edit { id: row.get(0)?, note, data: super::Bytes(row.get(3)?), collaborator: false });
            bytes += size;
        }
        Ok(Exchange { account: self.account.clone(), channel: self.channel.clone(),
            token: get_state(&self.db, "token")?.map(super::Bytes), edits, legacy: false, room: self.room.clone(), activity_only: false,
            #[cfg(feature = "devtools")]
            expire_cursor_for_qa: false,
            #[cfg(feature = "devtools")]
            upload_fault_for_qa: None })
    }

    pub fn legacy_request(&self) -> Result<Exchange> {
        ensure!(self.room.is_none(), "A shared note cannot access the personal sync zone");
        Ok(Exchange { account: self.account.clone(), channel: self.channel.clone(),
            token: get_state(&self.db, "legacy_token")?.map(super::Bytes), edits: vec![], legacy: true, room: None, activity_only: false,
            #[cfg(feature = "devtools")]
            expire_cursor_for_qa: false,
            #[cfg(feature = "devtools")]
            upload_fault_for_qa: None })
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

    pub fn issues(&self) -> Result<Vec<SyncIssue>> {
        let repairs: HashSet<String> = self.db.prepare("SELECT path FROM sync_asset_repairs")?
            .query_map([], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
        let mut stmt = self.db.prepare("SELECT d.id, d.title, d.trashed, coalesce(n.held, 0), coalesce(w.count, 0)
            FROM library_documents d LEFT JOIN notes n ON n.id = d.id
            LEFT JOIN sync_attachment_warnings w ON w.note = d.id
            WHERE n.held <> 0 OR w.count > 0 OR EXISTS (
                SELECT 1 FROM json_each(n.assets) ref JOIN sync_asset_repairs r ON r.path = ref.value)
            ORDER BY d.trashed, d.title, d.id LIMIT 30")?;
        let mut rows = stmt.query([])?;
        let mut issues = Vec::new();
        while let Some(row) = rows.next()? {
            let (id, title, trashed, held, warnings): (String, String, bool, bool, u32) =
                (row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?);
            let mut attachments = Vec::new();
            if warnings != 0 || !repairs.is_empty() {
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
            issues.push(SyncIssue { id, title, trashed, held, attachments });
        }
        Ok(issues)
    }

    pub fn identity(&self) -> (&str, Option<&str>, u64) {
        (&self.account, self.channel.as_deref(), self.peer)
    }

    pub fn account_verification_marker(&self) -> Result<PathBuf> {
        let personal = if self.room.is_some() {
            self.root.parent().and_then(Path::parent).context("Missing personal library for shared note")?
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
        let tx = self.db.transaction()?;
        let cursor = if page.legacy { "legacy_token" } else { "token" };
        ensure!(!page.legacy || page.uploaded.is_empty(), "The previous sync zone is read-only");
        ensure!(page.room_access.is_some() == self.room.is_some(), "Incorrect CloudKit sharing scope");
        if let (Some(room), Some(access)) = (&self.room, page.room_access) {
            ensure!(page.edits.iter().all(|edit| room.permits(&edit.note))
                && (access.can_write() || page.uploaded.is_empty()), "Invalid shared note changes");
            put_state(&tx, "shared_access", &serde_json::to_vec(&access)?)?;
            activity::observe(&tx, &room.note, &page.edits)?;
            activity::remember_access(&tx, access)?;
            if access == Access::Revoked {
                ensure!(page.edits.is_empty() && page.token.is_none(), "A revoked share returned note content");
                tx.execute("DELETE FROM library_entries WHERE id = ?1", [&room.note])?;
            }
        }
        for id in page.uploaded { tx.execute("DELETE FROM outbox WHERE id = ?1", [id])?; }
        for edit in page.edits {
            tx.execute("INSERT OR IGNORE INTO inbox VALUES (?1, ?2, ?3)", params![edit.id, edit.note, edit.data.0])?;
        }
        if let Some(token) = page.token { put_state(&tx, cursor, &token.0)?; }
        else {
            ensure!((page.legacy || page.room_access == Some(Access::Revoked)) && !page.more,
                "CloudKit did not return a sync cursor");
            tx.execute("DELETE FROM state WHERE key = ?1", [cursor])?;
        }
        tx.commit()?;
        let changed = self.access != page.room_access;
        self.access = page.room_access;
        if changed {
            self.merged_data_version = None;
            if self.access.is_some_and(|access| !access.can_write()) { self.history.clear(); }
        }
        Ok(changed)
    }

    pub fn merge(&mut self, open_positions: &HashMap<String, Vec<Pos>>) -> Result<(bool, Vec<NoteChange>)> {
        if self.access == Some(Access::Revoked) { return Ok((false, vec![])); }
        self.history.retain(|id, _| open_positions.contains_key(id));
        // Store writes through a separate connection. A quiet poll needn't
        // decode the entire library index again just to fetch a CloudKit page.
        let version: i64 = self.db.pragma_query_value(None, "data_version", |row| row.get(0))?;
        if self.merged_data_version == Some(version)
            && !self.db.query_row("SELECT EXISTS(SELECT 1 FROM inbox)", [], |row| row.get::<_, bool>(0))?
        { return Ok((false, vec![])); }
        let tx = self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let local = if self.room.is_none() { catalogue::local(&tx)? } else { Default::default() };
        let catalogue_doc = LoroDoc::new();
        catalogue_doc.import(&get_state(&tx, "catalogue")?.context("Missing catalogue snapshot")?)?;
        catalogue_doc.set_peer_id(self.peer)?;
        let before = catalogue_doc.oplog_vv();
        let seen = serde_json::from_slice(&get_state(&tx, "catalogue_seen")?.context("Missing catalogue projection")?)?;
        catalogue::capture(&tx, &catalogue_doc, &seen, &local)?;
        catalogue_doc.commit();
        if catalogue_doc.oplog_vv() != before {
            enqueue(&tx, catalogue::SCOPE, catalogue_doc.export(ExportMode::updates(&before))?)?;
        }

        let ids = tx.prepare("SELECT d.id, d.trashed FROM library_documents d LEFT JOIN notes n ON n.id = d.id
            WHERE n.id IS NULL OR d.revision <> n.seen OR n.held <> 0")?
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let local_bodies_changed = !ids.is_empty() && self.room.is_none();
        for (id, trashed) in ids.into_iter().filter(|(id, _)| local.contains_key(id)
            || self.room.as_ref().is_some_and(|room| room.note == *id && self.access.is_some_and(Access::can_write))) {
            let (doc, seen, front) = Store::read_sync_body(&tx, &id, trashed)?;
            capture_note(&tx, &self.root, self.peer, &id, &doc, seen, &front)?;
        }

        let edits = incoming(&tx, catalogue::SCOPE)?;
        let catalogue_doc = if edits.is_empty() { catalogue_doc } else {
            let candidate = LoroDoc::new();
            candidate.import(&catalogue_doc.export(ExportMode::Snapshot)?)?;
            if candidate.import_batch(&edits)?.pending.is_some() { catalogue_doc } else {
                tx.execute("DELETE FROM inbox WHERE note = ?1", [catalogue::SCOPE])?;
                candidate.set_peer_id(self.peer)?;
                candidate
            }
        };

        assets::ingest(&tx, &self.root)?;
        let ids: Vec<String> = tx.prepare("SELECT DISTINCT note FROM inbox WHERE note <> ?1 AND note <> ?2")?
            .query_map([catalogue::SCOPE, assets::SCOPE], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
        let mut changed = vec![];
        for id in ids {
            if self.room.is_none() && catalogue::permanently_deleted(&catalogue_doc, &id) {
                tx.execute("DELETE FROM inbox WHERE note = ?1", [&id])?;
                continue;
            }
            let old = note(&tx, &id)?;
            if old.as_ref().is_some_and(|note| note.held) { continue; }
            let loro = open_note(&tx, &id, old.as_ref().map_or(&[], |note| note.snapshot.as_slice()))?;
            let before = loro.oplog_vv();
            let anchors: Vec<_> = open_positions.get(&id).into_iter().flatten()
                .filter_map(|&pos| content::anchor(&loro, pos).map(|anchor| (pos, anchor))).collect();
            let updates = incoming(&tx, &id)?;
            let status = loro.import_batch(&updates)?;
            if status.pending.is_some() { continue; }
            if loro.oplog_vv() != before {
                let doc = content::render(&loro)?;
                let merged_front = front(&loro);
                let mut paths = content::local_assets(&doc);
                if self.room.is_some() {
                    for path in &paths { assets::path(&self.root, path)?; }
                }
                let local_only = loro.get_map("local-only-attachments");
                let total = paths.len();
                paths.retain(|source| local_only.get(source).is_none());
                if paths.iter().any(|source| !assets::present(&self.root, source)) { continue; }
                attachment_warnings(&tx, &id, total - paths.len())?;
                let (tips, has_fork) = archive_frontiers(&loro, &before, &updates)?;
                let trashed = tx.query_row("SELECT trashed FROM library_documents WHERE id = ?1", [&id],
                    |row| row.get::<_, bool>(0)).optional()?;
                if let Some(trashed) = trashed {
                    let (current, _, front) = Store::read_sync_body(&tx, &id, trashed)?;
                    if current != doc || front != merged_front { archive(&tx, &id, &current, &front, has_fork)?; }
                }
                // Preserve the sender's branch as well as our previous view.
                // A split or deleted block can hide text in the merged view.
                for tips in tips {
                    if tips == loro.oplog_frontiers() { continue; }
                    let branch = loro.fork_at(&tips)?;
                    let candidate = content::render(&branch)?;
                    let candidate_front = front(&branch);
                    if candidate != doc || candidate_front != merged_front {
                        archive(&tx, &id, &candidate, &candidate_front, has_fork)?;
                    }
                }
                trim_recent_versions(&tx, &id)?;
                let seen = Store::write_sync_body(&tx, &id, trashed.unwrap_or(false), &doc, &merged_front)?;
                checkpoint_note(&tx, &id, &Note { assets: paths, snapshot: loro.export(ExportMode::Snapshot)?, seen, held: false })?;
                let positions = anchors.iter().filter_map(|(pos, anchor)|
                    content::resolve(&loro, pos.block, anchor).map(|resolved| (*pos, resolved))).collect();
                changed.push(NoteChange { id: id.clone(), document: doc, positions });
            }
            if self.room.is_some() { activity::mark_projected(&tx, &id)?; }
            tx.execute("DELETE FROM inbox WHERE note = ?1", [&id])?;
        }

        let before_projection = catalogue_doc.oplog_vv();
        let catalogue_changed = before_projection != before;
        let dirty = tx.execute("DELETE FROM state WHERE key = 'catalogue_dirty'", [])? != 0;
        let library_changed = if let Some(room) = &self.room {
            let front: Option<String> = tx.query_row("SELECT front FROM library_documents WHERE id = ?1",
                [&room.note], |row| row.get(0)).optional()?;
            let front = front.unwrap_or_default();
            Store::show_shared_note(&tx, &room.note, &front)?
        } else if catalogue_changed || local_bodies_changed || dirty || !changed.is_empty() {
            let changed = catalogue::project(&tx, &catalogue_doc)?;
            put_state(&tx, "catalogue_seen", &serde_json::to_vec(&catalogue::local(&tx)?)?)?;
            changed
        } else { false };
        catalogue_doc.commit();
        if catalogue_doc.oplog_vv() != before_projection {
            enqueue(&tx, catalogue::SCOPE, catalogue_doc.export(ExportMode::updates(&before_projection))?)?;
        }
        if catalogue_doc.oplog_vv() != before { put_state(&tx, "catalogue", &catalogue_doc.export(ExportMode::Snapshot)?)?; }
        tx.commit()?;
        self.merged_data_version = Some(version);
        Ok((library_changed, changed))
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
        journal.receive(Changes { uploaded: uploaded.clone(), edits: edits.to_vec(), token: Some(super::Bytes(vec![1])),
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
    journal.receive(Changes { uploaded, edits: vec![], token: Some(super::Bytes(vec![1])),
        more: false, legacy: false, room_access: None, #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false })?;
    assert_eq!(journal.counts()?.0, 0);
    drop(journal);
    let mut journal = Journal::open(&fixture.0, None)?;
    assert_eq!(update_count(&journal)?, 1);
    assert_eq!(content::render(&journal.text_state(&id)?.unwrap())?, doc);

    let remote = content::open(&base)?;
    remote_doc.blocks[0].text.replace(0..0, &crate::model::RichText::plain("远端"));
    let data = content::edit(&remote, journal.peer + 1, &remote_doc)?.context("Missing concurrent update")?;
    journal.receive(Changes { uploaded: vec![], edits: vec![Edit { id: "remote".into(), note: id.clone(),
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
    assert_eq!(issue.title, "附件检查");
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
    journal.receive(Changes { uploaded: saved.into_iter().map(|edit| edit.id).collect(), edits: vec![],
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
    journal.receive(Changes { uploaded: vec![], edits: vec![asset], token: Some(super::Bytes(vec![2])),
        more: false, legacy: false, room_access: None, #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false })?;
    journal.merge(&Default::default())?;
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
        journal.receive(Changes { uploaded: vec![], edits, token: Some(super::Bytes(vec![1])),
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
            journal.receive(Changes { uploaded: vec![], edits: std::mem::take(&mut batch), token: Some(super::Bytes(vec![1])),
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
