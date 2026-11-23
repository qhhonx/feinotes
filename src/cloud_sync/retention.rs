// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
// Cloud record IDs are immutable. The zone fence in the cursor serializes cleanup
// with participant uploads, and a lost cursor rebuilds this index from CloudKit.
use anyhow::{Result, ensure};
use loro::{ExportMode, LoroDoc, VersionVector};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use super::{Bytes, Edit, assets, catalogue, content};

const MAGIC: &[u8] = b"FeiNotes-Checkpoint-1\0";
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Anchor { pub version: VersionVector, pub recovery: bool }

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Index { pub versions: Vec<Anchor>, pub records: Vec<String> }

pub(super) fn unpack(bytes: &[u8]) -> Result<(Index, &[u8])> {
    let Some(data) = bytes.strip_prefix(MAGIC) else { return Ok((Index::default(),bytes)); };
    let size = u32::from_le_bytes(data.get(..4).ok_or_else(|| anyhow::anyhow!("Incomplete checkpoint"))?.try_into()?) as usize;
    let (history,body) = data[4..].split_at_checked(size).ok_or_else(|| anyhow::anyhow!("Invalid checkpoint size"))?;
    Ok((serde_json::from_slice(history)?,body))
}

pub(super) fn pack(index: &Index, snapshot: &[u8]) -> Result<Vec<u8>> {
    let history=serde_json::to_vec(index)?;
    let mut bytes=Vec::with_capacity(MAGIC.len()+4+history.len()+snapshot.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&u32::try_from(history.len())?.to_le_bytes());
    bytes.extend_from_slice(&history);
    bytes.extend_from_slice(snapshot);
    Ok(bytes)
}

#[derive(Serialize)]
pub(super) struct Maintenance { pub snapshot: Option<Edit>, pub remove: Vec<String> }

#[derive(Default, Deserialize)]
pub(super) struct Receipt { pub reset: bool, pub ready: bool, pub deleted: Vec<String>, #[serde(default)] pub chunks: Vec<RemoteChunk> }

pub(super) fn schema(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS cloud_inventory (
        id TEXT PRIMARY KEY, scope TEXT NOT NULL, path TEXT, digest TEXT, part INTEGER, parts INTEGER, vv BLOB,
        observed INTEGER NOT NULL DEFAULT (unixepoch()));
        CREATE INDEX IF NOT EXISTS cloud_inventory_scope ON cloud_inventory(scope);
        CREATE INDEX IF NOT EXISTS cloud_inventory_asset ON cloud_inventory(path,digest,part);
        CREATE TABLE IF NOT EXISTS cloud_covered(witness TEXT NOT NULL, edit TEXT NOT NULL, PRIMARY KEY(witness,edit));
        CREATE TABLE IF NOT EXISTS cloud_checkpoint_paths(id TEXT NOT NULL, path TEXT NOT NULL, PRIMARY KEY(id,path));
        CREATE TABLE IF NOT EXISTS cloud_checkpoint_anchors(id TEXT PRIMARY KEY, anchors BLOB NOT NULL);
        CREATE TABLE IF NOT EXISTS cloud_release_candidates (path TEXT PRIMARY KEY, digest TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS cloud_inventory_state (ready INTEGER NOT NULL);
        ")?;
    Ok(())
}

pub(super) fn ready(db: &Connection) -> Result<bool> {
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM cloud_inventory_state WHERE ready=1)", [], |r| r.get(0))?)
}

pub(super) fn received(db: &Connection, receipt: &Receipt, edits: &[Edit]) -> Result<()> {
    if receipt.reset {
        db.execute("DELETE FROM cloud_covered", [])?;
        db.execute("DELETE FROM cloud_checkpoint_paths", [])?;
        db.execute("DELETE FROM cloud_checkpoint_anchors", [])?;
        db.execute("INSERT OR IGNORE INTO cloud_release_candidates SELECT path,digest FROM cloud_inventory WHERE path IS NOT NULL", [])?;
        db.execute("DELETE FROM cloud_inventory", [])?;
    }
    db.execute("UPDATE cloud_inventory_state SET ready=?1 WHERE ready<>?1",[receipt.ready])?;
    db.execute("INSERT INTO cloud_inventory_state SELECT ?1 WHERE NOT EXISTS(SELECT 1 FROM cloud_inventory_state)", [receipt.ready])?;
    for id in &receipt.deleted {
        db.execute("INSERT OR IGNORE INTO cloud_release_candidates SELECT path,digest FROM cloud_inventory WHERE id=?1 AND path IS NOT NULL",[id])?;
        db.execute("DELETE FROM cloud_covered WHERE witness=?1 OR edit=?1",[id])?;
        db.execute("DELETE FROM cloud_checkpoint_paths WHERE id=?1",[id])?;
        db.execute("DELETE FROM cloud_checkpoint_anchors WHERE id=?1",[id])?;
        db.execute("DELETE FROM cloud_inventory WHERE id=?1", [id])?;
    }
    for chunk in &receipt.chunks {
        ensure!(uuid::Uuid::parse_str(&chunk.id).is_ok() && (1..=64).contains(&chunk.parts) &&
            (0..chunk.parts).contains(&chunk.part) && chunk.digest.len()==64 &&
            chunk.digest.bytes().all(|b| b.is_ascii_hexdigit()), "Invalid cloud attachment index");
        db.execute("INSERT OR IGNORE INTO cloud_inventory(id,scope,path,digest,part,parts) VALUES (?1,?2,?3,?4,?5,?6)",
            params![chunk.id,assets::SCOPE,chunk.path,chunk.digest,chunk.part,chunk.parts])?;
    }
    for edit in edits {
        if edit.note == assets::SCOPE {
            let (chunk, _) = assets::decode(&edit.data.0)?;
            db.execute("INSERT OR IGNORE INTO cloud_inventory(id,scope,path,digest,part,parts) VALUES (?1,?2,?3,?4,?5,?6)",
                params![edit.id,edit.note,chunk.path,chunk.digest,chunk.part as i64,chunk.count() as i64])?;
        } else {
            let (index, body) = unpack(&edit.data.0)?;
            let meta = LoroDoc::decode_import_blob_meta(body, true)?;
            if body.len()!=edit.data.0.len() {
                let verified=content::configured();
                ensure!(verified.import(body)?.pending.is_none() && verified.oplog_vv()==meta.partial_end_vv,
                    "Unverifiable cloud checkpoint");
                ensure!(index.records.len()<=160, "Checkpoint coverage limit exceeded");
                for id in &index.records {
                    ensure!(id!=&edit.id && uuid::Uuid::parse_str(id).is_ok(), "Invalid covered record");
                    db.execute("INSERT OR IGNORE INTO cloud_covered VALUES (?1,?2)",params![edit.id,id])?;
                }
            }
            if !index.versions.is_empty() {
                db.execute("INSERT OR IGNORE INTO cloud_checkpoint_anchors VALUES (?1,?2)",params![edit.id,serde_json::to_vec(&index.versions)?])?;
            }
            db.execute("INSERT OR IGNORE INTO cloud_inventory(id,scope,vv) VALUES (?1,?2,?3)",
                params![edit.id,edit.note,serde_json::to_vec(&meta.partial_end_vv)?])?;
        }
    }
    Ok(())
}

// The witness has been fetched back from CloudKit, verified and durably
// imported. Delete only recorded IDs from this scope with covered causal ends.
pub(super) fn verified_cleanup(db: &Connection) -> Result<Option<Maintenance>> {
    let mut stmt=db.prepare("SELECT target.id,target.vv,base.vv,base.id FROM cloud_covered x
        JOIN cloud_inventory base ON base.id=x.witness JOIN cloud_inventory target ON target.id=x.edit
        WHERE base.scope=target.scope AND target.vv IS NOT NULL AND base.vv IS NOT NULL ORDER BY base.rowid DESC")?;
    let mut rows=stmt.query([])?;
    let mut remove=vec![];
    let mut witness=None;
    while let Some(r)=rows.next()? {
        let id=r.get::<_,String>(3)?;
        if witness.as_ref().is_some_and(|w| *w!=id) { break; }
        witness=Some(id);
        let covered: VersionVector=serde_json::from_slice(r.get_ref(1)?.as_blob()?)?;
        let version: VersionVector=serde_json::from_slice(r.get_ref(2)?.as_blob()?)?;
        if version.includes_vv(&covered) { remove.push(r.get(0)?); }
        if remove.len()==160 { break; }
    }
    Ok((!remove.is_empty()).then_some(Maintenance {snapshot:None,remove}))
}

pub(super) fn candidates(db: &Connection, threshold: i64) -> Result<Vec<String>> {
    Ok(db.prepare("SELECT scope FROM cloud_inventory WHERE vv IS NOT NULL GROUP BY scope
        HAVING count(*) >= ?1 ORDER BY count(*) DESC")?.query_map([threshold], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
}

pub(super) fn compact(db: &Connection, scope: &str, doc: &LoroDoc) -> Result<Option<Maintenance>> {
    let version = doc.oplog_vv();
    let mut remove = vec![];
    let mut history: Vec<Anchor> = vec![];
    let mut stmt = db.prepare("SELECT c.id,c.vv,a.anchors FROM cloud_inventory c LEFT JOIN cloud_checkpoint_anchors a USING(id)
        WHERE scope=?1 ORDER BY c.rowid LIMIT 160")?;
    let mut rows = stmt.query([scope])?;
    while let Some(r) = rows.next()? {
        let end: VersionVector = serde_json::from_slice(r.get_ref(1)?.as_blob()?)?;
        if !version.includes_vv(&end) { continue; }
        remove.push(r.get(0)?);
        if scope==catalogue::SCOPE { continue; }
        if let Some(raw)=r.get::<_,Option<Vec<u8>>>(2)? {
            history.extend(serde_json::from_slice::<Vec<Anchor>>(&raw)?);
        }
        let full=doc.frontiers_to_vv(&doc.vv_to_frontiers(&end)).ok_or_else(|| anyhow::anyhow!("Missing checkpoint history"))?;
        if full!=version { history.push(Anchor {version:full,recovery:false}); }
    }
    if remove.len() < 2 { return Ok(None); }
    let bytes = doc.export(ExportMode::Snapshot)?;
    if bytes.len() > 16*1024*1024 { return Ok(None); }
    let check = if scope == catalogue::SCOPE { LoroDoc::new() } else { content::configured() };
    ensure!(check.import(&bytes)?.pending.is_none() && check.oplog_vv() == version
        && check.get_deep_value() == doc.get_deep_value(), "Incomplete cloud checkpoint");
    let mut merged: Vec<Anchor> = vec![];
    for item in history {
        if let Some(previous) = merged.iter_mut().find(|s| s.version==item.version) { previous.recovery |= item.recovery; }
        else { merged.push(item); }
    }
    // A parallel branch remains recoverable. Keep the same six most recent
    // linear versions as the journal. Carry these causal points across compactions.
    for i in 0..merged.len() {
        for j in i+1..merged.len() {
            if !merged[i].version.includes_vv(&merged[j].version) && !merged[j].version.includes_vv(&merged[i].version) {
                merged[i].recovery=true; merged[j].recovery=true;
            }
        }
    }
    merged.sort_by_cached_key(|a| a.version.values().map(|n| i64::from(*n)).sum::<i64>());
    let mut drop_linear=merged.iter().filter(|a| !a.recovery).count().saturating_sub(super::journal::RECENT_VERSIONS as usize);
    merged.retain(|a| {
        if !a.recovery && drop_linear>0 { drop_linear-=1; false } else { true }
    });
    let bytes=pack(&Index { versions:merged, records:remove },&bytes)?;
    if bytes.len()>16*1024*1024 { return Ok(None); }
    Ok(Some(Maintenance { snapshot: Some(Edit { id: uuid::Uuid::new_v4().to_string(), note: scope.into(),
        data: Bytes(bytes), collaborator: false }), remove: vec![] }))
}

pub(super) fn stale_assets(db: &Connection, retained: &HashSet<String>) -> Result<Option<Maintenance>> {
    // Give partially uploaded files a chance to finish. Every writer still checks
    // complete cloud chunks before publishing a body, even after this interval.
    let mut stmt = db.prepare("SELECT id,path FROM cloud_inventory WHERE path IS NOT NULL AND observed<=unixepoch()-3600 ORDER BY rowid")?;
    let mut rows = stmt.query([])?;
    let mut remove = vec![];
    while let Some(row) = rows.next()? {
        if !retained.contains(row.get_ref(1)?.as_str()?) { remove.push(row.get(0)?); }
        if remove.len() == 160 { break; }
    }
    Ok((!remove.is_empty()).then_some(Maintenance { snapshot: None, remove }))
}

#[derive(Deserialize)]
pub(super) struct RemoteChunk { pub id: String, pub path: String, pub digest: String, pub part: i64, pub parts: i64 }

// Local absence is conclusive only after this session has checked the
// cloud inventory. Indexed bytes may still be queued for another exchange.
pub(super) fn awaiting_attachment(db: &Connection, source: &str, inventory_confirmed: bool) -> Result<bool> {
    if !inventory_confirmed { return Ok(true); }
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM cloud_inventory WHERE path = ?1)", [source], |r| r.get(0))?)
}

pub(super) fn downloads(db: &Connection, root: &std::path::Path, open_notes: Option<&[String]>) -> Result<Vec<String>> {
    if !ready(db)? { return Ok(vec![]); }
    let mut ids = vec![];
    let mut stmt = db.prepare("SELECT c.id,c.path FROM cloud_inventory c WHERE c.path IS NOT NULL AND EXISTS (
        SELECT 1 FROM notes n JOIN library_documents d USING(id), json_each(n.assets) a WHERE a.value=c.path
            AND (?1 IS NULL OR n.id IN (SELECT value FROM json_each(?1))
                OR EXISTS(SELECT 1 FROM sync_asset_repairs m WHERE m.path=c.path)))
        AND (NOT EXISTS(SELECT 1 FROM sync_assets a WHERE a.path=c.path AND a.digest=c.digest)
            OR EXISTS(SELECT 1 FROM sync_asset_repairs m WHERE m.path=c.path))
        AND NOT EXISTS(SELECT 1 FROM sync_asset_chunks s WHERE s.path=c.path AND s.digest=c.digest AND s.part=c.part)
        ORDER BY c.path,c.part")?;
    let scope = open_notes.map(serde_json::to_string).transpose()?;
    let mut rows = stmt.query([scope])?;
    while let Some(r) = rows.next()? {
        if !assets::present(root, r.get_ref(1)?.as_str()?) { ids.push(r.get(0)?); }
        if ids.len() == 8 { break; }
    }
    Ok(ids)
}

// Local named versions and crash recovery live outside the SQLite journal.
// Abort a sweep on an unreadable source; a partial scan cannot prove absence.
pub(super) fn disk_roots(root: &std::path::Path, paths: &mut HashSet<String>) -> Result<()> {
    fn read(dir: &std::path::Path) -> Result<Vec<std::fs::DirEntry>> {
        match std::fs::read_dir(dir) {
            Ok(entries) => Ok(entries.collect::<std::io::Result<_>>()?),
            Err(e) if e.kind()==std::io::ErrorKind::NotFound => Ok(vec![]),
            Err(e) => Err(e.into())
        }
    }
    fn documents(dir: &std::path::Path, paths: &mut HashSet<String>) -> Result<()> {
        for entry in read(dir)? {
            let path = entry.path();
            let ext = path.extension().and_then(|s| s.to_str());
            if !matches!(ext, Some("xml" | "md")) { continue; }
            ensure!(entry.file_type()?.is_file(), "Unexpected history file type");
            let raw = std::fs::read_to_string(&path)?;
            let doc = if ext == Some("xml") { crate::docxml::parse(&raw)? }
                else { crate::markdown::parse_file(&raw,"",Some(dir)) };
            paths.extend(content::local_assets(&doc));
        }
        Ok(())
    }
    for history in ["history","history-trash"] {
        for entry in read(&root.join(crate::store::INTERNAL).join(history))? {
            if entry.file_type()?.is_dir() { documents(&entry.path(), paths)?; }
        }
    }
    documents(&crate::store::Store::recovery_directory(root), paths)
}

#[test]
fn shared_attachment_stays_pending_until_cloud_absence_is_confirmed() -> Result<()> {
    let db = Connection::open_in_memory()?;
    schema(&db)?;
    let source = "assets/files/shared.txt";
    let id = uuid::Uuid::new_v4().to_string();
    assert!(awaiting_attachment(&db, source, false)?);

    received(&db, &Receipt { ready: true, chunks: vec![RemoteChunk {
        id: id.clone(), path: source.into(), digest: "a".repeat(64), part: 0, parts: 1,
    }], ..Default::default() }, &[])?;
    assert!(awaiting_attachment(&db, source, true)?);
    assert!(!awaiting_attachment(&db, "assets/files/unknown.txt", true)?);

    received(&db, &Receipt { ready: true, deleted: vec![id], ..Default::default() }, &[])?;
    assert!(!awaiting_attachment(&db, source, true)?);
    assert!(awaiting_attachment(&db, source, false)?);
    Ok(())
}
