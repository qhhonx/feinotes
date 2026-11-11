// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Local read state for shared notes. The sidebar can observe remote records
//! without opening a note or advancing the editor's synchronization cursor.

use anyhow::Result;
use gpui::{App, Global};
use loro::{LoroDoc, VersionVector};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::{path::{Path, PathBuf}, time::Duration};
use super::{Bytes, Edit, Exchange, exchange, shared::{self, Access, Room}};

#[derive(PartialEq, Eq)]
pub(crate) struct SharedRow {
    pub room: Room,
    pub unread: bool,
}

#[derive(Default, PartialEq, Eq)]
pub(crate) struct ActivityView(pub Vec<SharedRow>);
impl Global for ActivityView {}

pub(super) fn schema(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS shared_activity (
        id TEXT PRIMARY KEY, projected INTEGER NOT NULL DEFAULT 0, seen INTEGER NOT NULL DEFAULT 0)")?;
    Ok(())
}

pub(super) fn observe(db: &Connection, note: &str, edits: &[Edit]) -> Result<()> {
    let mut projected_version = None;
    for edit in edits.iter().filter(|edit| edit.collaborator && edit.note == note) {
        let known: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM shared_activity WHERE id = ?1)",
            [&edit.id], |row| row.get(0))?;
        if known { continue; }
        // A newly shared note starts with one initial update. Only changes
        // that depend on earlier content represent collaborator activity.
        let update = LoroDoc::decode_import_blob_meta(&edit.data.0, false)?;
        if update.start_frontiers.is_empty() { continue; }
        if projected_version.is_none() {
            projected_version = Some(read_projected_version(db, note)?);
        }
        let projected = projected_version.as_ref().is_some_and(|version| version.includes_vv(&update.partial_end_vv));
        db.execute("INSERT OR IGNORE INTO shared_activity (id, projected) VALUES (?1, ?2)", params![edit.id, projected])?;
    }
    Ok(())
}

fn read_projected_version(db: &Connection, note: &str) -> Result<VersionVector> {
    // The watcher can replay history that was already merged by an earlier
    // build. An accepted note can also be listed before its journal is opened.
    let has_journal = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name = 'notes')",
        [], |row| row.get::<_, bool>(0))?;
    if !has_journal { return Ok(VersionVector::default()); }
    let snapshot: Option<Vec<u8>> = db.query_row("SELECT snapshot FROM notes WHERE id = ?1 AND held = 0",
        [note], |row| row.get(0)).optional()?;
    Ok(snapshot.map(|bytes| LoroDoc::decode_import_blob_meta(&bytes, false).map(|meta| meta.partial_end_vv))
        .transpose()?.unwrap_or_default())
}

pub(super) fn mark_projected(db: &Connection, note: &str) -> Result<()> {
    db.execute("UPDATE shared_activity SET projected = 1
        WHERE projected = 0 AND id IN (SELECT id FROM inbox WHERE note = ?1)", [note])?;
    Ok(())
}

pub(super) fn displayable(db: &Connection) -> Result<Vec<String>> {
    Ok(db.prepare("SELECT id FROM shared_activity WHERE projected = 1 AND seen = 0")?
        .query_map([], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?)
}

pub(super) fn mark_read(db: &mut Connection, displayed: &[String]) -> Result<()> {
    let tx = db.transaction()?;
    {
        let mut stmt = tx.prepare("UPDATE shared_activity SET seen = 1 WHERE id = ?1 AND projected = 1")?;
        for id in displayed { stmt.execute([id])?; }
    }
    tx.commit()?;
    Ok(())
}

fn value(db: &Connection, key: &str) -> Result<Option<Vec<u8>>> {
    Ok(db.query_row("SELECT value FROM library_values WHERE key = ?1", [key], |row| row.get(0)).optional()?)
}

fn put(db: &Connection, key: &str, bytes: &[u8]) -> Result<()> {
    db.execute("INSERT OR REPLACE INTO library_values (key, value) VALUES (?1, ?2)", params![key, bytes])?;
    Ok(())
}

pub(super) fn remember_access(db: &Connection, access: Access) -> Result<()> {
    if access == Access::Revoked { put(db, "shared-activity-revoked", b"1")?; }
    else { db.execute("DELETE FROM library_values WHERE key = 'shared-activity-revoked'", [])?; }
    Ok(())
}

fn room_db(personal: &Path, room: &Room) -> Result<Connection> {
    let path = shared::room_root(personal, &room.scope.note)?.join(crate::store::SQLITE_FILE);
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    db.busy_timeout(Duration::from_secs(2))?;
    Ok(db)
}

fn poll(personal: &Path, account: &str, room: &Room) -> Result<bool> {
    let mut db = room_db(personal, room)?;
    schema(&db)?;
    let token = value(&db, "shared-activity-token")?.map(Bytes);
    let revision = super::account_revision();
    let page = exchange(Exchange { account: account.into(), channel: None, token, edits: vec![], legacy: false,
        room: Some(room.scope.clone()), activity_only: true,
        #[cfg(feature = "devtools")] expire_cursor_for_qa: false,
        #[cfg(feature = "devtools")] upload_fault_for_qa: None })?;
    let tx = db.transaction()?;
    observe(&tx, &room.scope.note, &page.edits)?;
    if let Some(token) = page.token { put(&tx, "shared-activity-token", &token.0)?; }
    else { tx.execute("DELETE FROM library_values WHERE key = 'shared-activity-token'", [])?; }
    if let Some(access) = page.room_access { remember_access(&tx, access)?; }
    super::check_account_revision(revision)?;
    tx.commit()?;
    Ok(page.more)
}

fn row(personal: &Path, mut room: Room) -> Result<Option<SharedRow>> {
    let db = room_db(personal, &room)?;
    if value(&db, "shared-activity-revoked")?.is_some() { return Ok(None); }
    let has_activity: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name = 'shared_activity')",
        [], |row| row.get(0))?;
    let unread = has_activity && db.query_row("SELECT EXISTS(SELECT 1 FROM shared_activity WHERE seen = 0)",
        [], |row| row.get(0))?;
    if let Some(title) = db.query_row("SELECT title FROM library_documents WHERE id = ?1", [&room.scope.note],
        |row| row.get::<_, String>(0)).optional()? { room.title = title; }
    Ok(Some(SharedRow { room, unread }))
}

pub(super) fn start(cx: &mut App, personal: PathBuf, account: String, pause: Option<PathBuf>) {
    cx.set_global(ActivityView::default());
    cx.spawn(async move |cx| {
        let mut index = 0usize;
        let mut delay = Duration::ZERO;
        let mut last_error = None;
        loop {
            cx.background_executor().timer(delay).await;
            let (root, key, pause) = (personal.clone(), account.clone(), pause.clone());
            let (view, next, more, error) = cx.background_executor().spawn(async move {
                let rooms = shared::rooms(&root).unwrap_or_default();
                let current = if rooms.is_empty() { 0 } else { index % rooms.len() };
                let result = if pause.as_ref().is_some_and(|path| path.exists()) { Ok(false) }
                    else { rooms.get(current).map_or(Ok(false), |room| poll(&root, &key, room)) };
                let view = rooms.into_iter().filter_map(|room| row(&root, room).ok().flatten()).collect::<Vec<_>>();
                (ActivityView(view), current + 1, result.as_ref().is_ok_and(|more| *more), result.err())
            }).await;
            if let Some(error) = error {
                let error = format!("{error:#}");
                if last_error.as_ref() != Some(&error) { crate::logging::write(&format!("Shared note activity: {error}")); }
                last_error = Some(error);
            } else { last_error = None; }
            index = if more { next - 1 } else { next };
            delay = if more { Duration::from_secs(1) } else if view.0.is_empty() { Duration::from_secs(8) }
                else { Duration::from_secs(4) };
            if cx.update(|cx| {
                if cx.global::<ActivityView>() != &view { cx.set_global(view); }
            }).is_err() { break; }
        }
    }).detach();
}
