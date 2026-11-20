// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Local read state for shared notes. The sidebar can observe remote records
//! without opening a note or advancing the editor's synchronization cursor.

use anyhow::Result;
use gpui::{App, BorrowAppContext, Global};
use loro::{LoroDoc, VersionVector};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::{path::{Path, PathBuf}, time::Duration};
use super::{Bytes, Edit, Exchange, exchange, shared::{self, Access, Room}};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SharedRow {
    pub room: Room,
    pub unread: bool,
    pub summary: String,
    pub body: std::sync::Arc<str>,
    pub modified: u64,
    pub created: u64,
    pub opened: u64,
    pub icon: Option<String>,
    pub folder_items: Vec<FolderItem>,
    #[cfg(target_os = "macos")]
    pub access: Option<Access>,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct FolderItem {
    id: String,
    title: String,
    icon: Option<String>,
    folder: bool,
    parent: String,
    created: u64,
    modified: u64,
    order: u64,
    markdown: bool,
    kind: crate::store::DocKind,
}

impl FolderItem {
    pub(crate) fn from_listing(id: &str, title: &str, icon: Option<&str>, meta: &crate::store::Meta, scope: &shared::Scope) -> Option<Self> {
        if id == scope.note || !scope.permits(id) || meta.local() { return None; }
        Some(Self { id: id.into(), title: title.into(), icon: icon.map(str::to_owned), folder: meta.folder.is_some(),
            parent: meta.parent.clone()?, created: meta.created, modified: meta.modified, order: meta.order, markdown: meta.markdown, kind: meta.kind })
    }

    pub(crate) fn entry(&self, scope: &shared::Scope) -> crate::store::Entry {
        crate::store::Entry { id: self.id.clone(), title: self.title.clone(), icon: self.icon.clone(),
            meta: crate::store::Meta { parent: Some(self.parent.clone()), folder: self.folder.then(|| self.title.clone()),
                share_root: Some(scope.note.clone()), created: self.created, modified: self.modified, order: self.order, markdown: self.markdown,
                kind: self.kind, ..Default::default() }, body: Default::default(), reminders: vec![], links: vec![] }
    }
}

// A background read that began before a local write must not roll the sidebar back.
#[derive(Default)]
struct LocalActivity {
    sequence: u64,
    rooms: std::collections::HashMap<String, u64>,
}
impl Global for LocalActivity {}

fn updated_locally(note: &str, cx: &mut App) {
    if !cx.has_global::<LocalActivity>() { cx.set_global(LocalActivity::default()); }
    let local = cx.global_mut::<LocalActivity>();
    local.sequence += 1;
    local.rooms.insert(note.to_owned(), local.sequence);
}

pub(crate) fn remove_room(note: &str, cx: &mut App) {
    updated_locally(note, cx);
    cx.update_global::<ActivityView, _>(|view, _| view.0.retain(|row| row.room.scope.note != note));
}

pub(crate) fn update_folder_items(scope: &shared::Scope, entries: &[crate::store::Entry], cx: &mut App) {
    let Some(row) = cx.try_global::<ActivityView>().and_then(|view| view.0.iter().find(|row| row.room.scope == *scope)) else { return; };
    let items = folder_items(scope, entries);
    if row.folder_items == items { return; }
    updated_locally(&scope.note, cx);
    cx.update_global::<ActivityView, _>(|view, _| {
        if let Some(row) = view.0.iter_mut().find(|row| row.room.scope == *scope) { row.folder_items = items; }
    });
}

pub(crate) fn folder_items(scope: &shared::Scope, entries: &[crate::store::Entry]) -> Vec<FolderItem> {
    let mut items: Vec<_> = entries.iter().filter_map(|e| FolderItem::from_listing(&e.id, &e.title, e.icon.as_deref(), &e.meta, scope)).collect();
    items.sort_by(|a, b| a.id.cmp(&b.id));
    items
}

#[derive(Default, PartialEq, Eq)]
pub(crate) struct ActivityView(pub Vec<SharedRow>, pub bool);
impl Global for ActivityView {}

pub(crate) fn update_entry(scope: &shared::Scope, entry: &crate::store::Entry, cx: &mut App) {
    if !cx.has_global::<ActivityView>() { return; }
    if cx.global::<ActivityView>().0.iter().any(|row| row.room.scope == *scope && row.room.title == entry.title && row.icon == entry.icon && row.created == entry.meta.created && row.modified == entry.meta.modified && row.opened == entry.meta.opened && row.body == entry.body) { return; }
    updated_locally(&scope.note, cx);
    cx.update_global::<ActivityView, _>(|view, _| {
        if let Some(row) = view.0.iter_mut().find(|row| row.room.scope == *scope) {
            row.room.title = entry.title.clone(); row.icon = entry.icon.clone();
            row.created = entry.meta.created; row.modified = entry.meta.modified; row.opened = entry.meta.opened; row.body = entry.body.clone();
        } else {
            view.0.push(SharedRow { room: shared::Room { scope: scope.clone(), title: entry.title.clone() }, unread: false, summary: String::new(), body: entry.body.clone(), modified: entry.meta.modified, created: entry.meta.created, opened: entry.meta.opened, icon: entry.icon.clone(), folder_items: vec![],
                #[cfg(target_os = "macos")]
                access: None,
            });
        }
        sort_rows(&mut view.0);
    });
}

pub(crate) fn publish_row(row: SharedRow, cx: &mut App) {
    if !cx.has_global::<ActivityView>() { cx.set_global(ActivityView::default()); }
    updated_locally(&row.room.scope.note, cx);
    cx.update_global::<ActivityView, _>(|view, _| {
        view.0.retain(|previous| previous.room.scope.note != row.room.scope.note);
        view.0.push(row);
        sort_rows(&mut view.0);
    });
}

fn sort_rows(rows: &mut [SharedRow]) { rows.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.room.title.cmp(&b.room.title)).then_with(|| a.room.scope.note.cmp(&b.room.scope.note))); }

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
        let update = LoroDoc::decode_import_blob_meta(super::retention::unpack(&edit.data.0)?.1, false)?;
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
    let page = exchange(Exchange { downloads: vec![], maintenance: None, account: account.into(), channel: None, token, edits: vec![], legacy: false,
        room: Some(room.scope.clone()), activity_only: true,
        #[cfg(feature = "devtools")] expire_cursor_for_qa: false,
        #[cfg(feature = "devtools")] upload_fault_for_qa: None })?;
    let tx = db.transaction()?;
    observe(&tx, &room.scope.note, &page.edits)?;
    if let Some(token) = page.token { put(&tx, "shared-activity-token", &token.0)?; }
    else { tx.execute("DELETE FROM library_values WHERE key = 'shared-activity-token'", [])?; }
    if let Some(access) = page.room_access {
        remember_access(&tx, access)?;
        if access == Access::Revoked { super::journal::archive_owner_ended(&tx, &room.scope)?; }
    }
    super::check_account_revision(revision)?;
    tx.commit()?;
    Ok(page.more)
}

pub(crate) fn row(personal: &Path, mut room: Room) -> Result<Option<SharedRow>> {
    let root = shared::room_root(personal, &room.scope.note)?;
    if root.join("shared-note-ended").exists() || shared::original::awaiting_publication(personal, &room)
        || shared::original::new_pending(personal, &root, &room)? { return Ok(None); }
    let db = room_db(personal, &room)?;
    if value(&db, "shared-activity-revoked")?.is_some() { return Ok(None); }
    let has_activity: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name = 'shared_activity')",
        [], |row| row.get(0))?;
    let unread = has_activity && db.query_row("SELECT EXISTS(SELECT 1 FROM shared_activity WHERE seen = 0)",
        [], |row| row.get(0))?;
    if let Some(title) = db.query_row("SELECT title FROM library_documents WHERE id = ?1", [&room.scope.note],
        |row| row.get::<_, String>(0)).optional()? { room.title = title; }
    // Closed rooms may still have an older schema. Listing must not require a migration.
    let has_search: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name = 'library_search_blocks')", [], |row| row.get(0))?;
    let body: std::sync::Arc<str> = if has_search {
        db.prepare("SELECT fragment FROM library_search_blocks WHERE document = ?1 ORDER BY position")?
            .query_map([&room.scope.note], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?.join(" ").into()
    } else { "".into() };
    let summary = body.chars().take(160).collect();
    let meta: Option<String> = db.query_row("SELECT meta FROM library_entries WHERE id = ?1", [&room.scope.note], |row| row.get(0)).optional()?;
    let meta = meta.and_then(|meta| serde_json::from_str::<crate::store::Meta>(&meta).ok()).unwrap_or_else(|| {
        let registered = if room.scope.folder { 0 } else { shared::registered_at(&root) };
        crate::store::Meta { created: registered, modified: registered, ..Default::default() }
    });
    if let Some(title) = meta.folder.as_ref() { room.title = title.clone(); }
    let icon = db.query_row("SELECT icon FROM library_documents WHERE id = ?1", [&room.scope.note], |row| row.get::<_, Option<String>>(0)).optional().ok().flatten().flatten();
    let folder_items = if room.scope.folder { cached_folder_items(&db, &room.scope)? } else { vec![] };
    #[cfg(target_os = "macos")]
    let access = if room.scope.folder { shared::cached_access(&db) } else { None };
    Ok(Some(SharedRow { room, unread, summary, body, modified: meta.modified, created: meta.created, opened: meta.opened, icon, folder_items,
        #[cfg(target_os = "macos")]
        access,
    }))
}

// Read only the catalogue fields needed for the sidebar. Closed rooms stay closed.
fn cached_folder_items(db: &Connection, scope: &shared::Scope) -> Result<Vec<FolderItem>> {
    let mut stmt = db.prepare("SELECT e.id, e.meta, d.title, d.icon FROM library_entries e
        LEFT JOIN library_documents d ON d.id = e.id AND d.trashed = 0 WHERE e.id != ?1 ORDER BY e.id")?;
    let mut rows = stmt.query([&scope.note])?;
    let mut items = vec![];
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let Ok(meta) = serde_json::from_str::<crate::store::Meta>(row.get_ref(1)?.as_str()?) else { continue; };
        let title: Option<String> = row.get(2)?;
        let icon: Option<String> = row.get(3)?;
        if let Some(title) = meta.folder.as_deref().or(title.as_deref())
            && let Some(item) = FolderItem::from_listing(&id, title, icon.as_deref(), &meta, scope) { items.push(item); }
    }
    Ok(items)
}

pub(super) fn start(cx: &mut App, personal: PathBuf, account: String, pause: Option<PathBuf>, generation: uuid::Uuid, discover_owned: bool) {
    cx.set_global(ActivityView::default());
    if discover_owned {
        let root = personal.clone();
        let key = account.clone();
        cx.spawn(async move |cx| {
            let mut last_error = None;
            loop {
                if !crate::gpui_compat::async_update(cx, |cx| super::current(&root, generation, cx)).unwrap_or(false) { break; }
                let (root, key) = (root.clone(), key.clone());
                let lease = crate::store::Store::session_lease(&root);
                let error = cx.background_executor().spawn(async move {
                    let _lease = lease;
                    shared::discover_owned(&root, &key).err().map(|error| format!("{error:#}"))
                }).await;
                if let Some(error) = &error && last_error.as_ref() != Some(error) {
                    crate::logging::write(&format!("Discover owned shares: {error}"));
                }
                last_error = error;
                cx.background_executor().timer(Duration::from_secs(30)).await;
            }
        }).detach();
    }
    cx.spawn(async move |cx| {
        let mut index = 0usize;
        // Show the local catalogue before the first CloudKit activity request.
        let mut loading_cache = true;
        let mut delay = Duration::ZERO;
        let mut last_error = None;
        loop {
            cx.background_executor().timer(delay).await;
            if !crate::gpui_compat::async_update(cx, |cx| super::current(&personal, generation, cx)).unwrap_or(false) { break; }
            let revision = crate::gpui_compat::async_update(cx, |cx|
                cx.try_global::<LocalActivity>().map_or(0, |local| local.sequence)).unwrap_or(0);
            let (root, key, pause) = (personal.clone(), account.clone(), pause.clone());
            let lease = crate::store::Store::session_lease(&personal);
            let cache_only = std::mem::take(&mut loading_cache);
            let (view, next, more, error) = cx.background_executor().spawn(async move {
                let _lease = lease;
                let rooms = shared::rooms(&root).unwrap_or_default();
                let current = if rooms.is_empty() { 0 } else { index % rooms.len() };
                let result = if cache_only || pause.as_ref().is_some_and(|path| path.exists()) { Ok(false) }
                    else { rooms.get(current).map_or(Ok(false), |room| {
                        let cache = shared::room_root(&root, &room.scope.note)?;
                        if shared::original::pending_new(&cache) { Ok(false) } else { poll(&root, &key, room) }
                    }) };
                let mut view = rooms.into_iter().filter_map(|room| row(&root, room).ok().flatten()).collect::<Vec<_>>();
                sort_rows(&mut view);
                (view, current + 1, result.as_ref().is_ok_and(|more| *more), result.err())
            }).await;
            let list_error = error.is_some();
            if let Some(error) = error {
                let error = format!("{error:#}");
                if last_error.as_ref() != Some(&error) { crate::logging::write(&format!("Shared note activity: {error}")); }
                last_error = Some(error);
            } else { last_error = None; }
            if !cache_only { index = if more { next - 1 } else { next }; }
            delay = if cache_only { Duration::ZERO } else if more { Duration::from_secs(1) } else if view.is_empty() { Duration::from_secs(8) }
                else { Duration::from_secs(4) };
            if !crate::gpui_compat::async_update(cx, |cx| {
                if !super::current(&personal, generation, cx) { return false; }
                let mut view = ActivityView(view, list_error);
                if let Some(local) = cx.try_global::<LocalActivity>() {
                    let recent = |row: &SharedRow| local.rooms.get(&row.room.scope.note).is_some_and(|seq| *seq > revision);
                    view.0.retain(|row| !recent(row));
                    view.0.extend(cx.global::<ActivityView>().0.iter().filter(|row| recent(row)).cloned());
                }
                // A poll can read metadata just before the editor records an open.
                let opened: std::collections::HashMap<_, _> = cx.global::<ActivityView>().0.iter()
                    .map(|row| (&row.room.scope.note, row.opened)).collect();
                for row in &mut view.0 {
                    if let Some(&previous) = opened.get(&row.room.scope.note) { row.opened = row.opened.max(previous); }
                }
                // A disk poll must not put an older title back over an unsaved edit.
                for window in cx.windows().into_iter().filter_map(|window| window.downcast::<crate::app::Workspace>()) {
                    if let Ok(ws) = window.read(cx)
                        && ws.is_shared() && ws.preview.is_none()
                        && let (Some(id), Some(editor)) = (&ws.current, &ws.editor)
                        && let Some(row) = view.0.iter_mut().find(|row| &row.room.scope.note == id)
                    {
                        let doc = &editor.read(cx).core.doc;
                        row.room.title = doc.title.text.clone(); row.icon = doc.icon.clone(); row.body = doc.plain_text().into();
                        if let Some(entry) = ws.entries.iter().find(|entry| &entry.id == id) { row.modified = entry.meta.modified; }
                    }
                }
                sort_rows(&mut view.0);
                if cx.global::<ActivityView>() != &view { cx.set_global(view); }
                true
            }).unwrap_or(false) { break; }
        }
    }).detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_list_keeps_older_caches_visible_and_bounds_summaries() -> Result<()> {
        struct Fixture(PathBuf);
        impl Drop for Fixture { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
        let temp = Fixture(std::env::temp_dir().join(format!("quiet-shared-list-{}", uuid::Uuid::new_v4())));
        let room = Room { scope: shared::Scope { note: uuid::Uuid::new_v4().to_string(), owner: None, folder: false }, title: "Old title".into() };
        let root = shared::room_root(&temp.0, &room.scope.note)?;
        std::fs::create_dir_all(&root)?;
        let db = Connection::open(root.join(crate::store::SQLITE_FILE))?;
        db.execute_batch("CREATE TABLE library_values (key TEXT PRIMARY KEY, value BLOB);
            CREATE TABLE library_documents (id TEXT PRIMARY KEY, title TEXT);
            CREATE TABLE library_entries (id TEXT PRIMARY KEY, meta TEXT);")?;
        schema(&db)?;
        db.execute("INSERT INTO library_documents VALUES (?1, 'Updated title')", [&room.scope.note])?;
        let meta = crate::store::Meta { modified: 1234, ..Default::default() };
        db.execute("INSERT INTO library_entries VALUES (?1, ?2)", params![room.scope.note, serde_json::to_string(&meta)?])?;
        db.execute("INSERT INTO shared_activity (id) VALUES ('remote-update')", [])?;
        let cached = row(&temp.0, room.clone())?.expect("old cache must remain navigable");
        assert_eq!(cached.room.title, "Updated title");
        assert_eq!(cached.modified, 1234);
        assert!(cached.unread && cached.summary.is_empty());
        db.execute_batch("CREATE TABLE library_search_blocks (document TEXT, position INTEGER, fragment TEXT)")?;
        db.execute("INSERT INTO library_search_blocks VALUES (?1, 0, ?2)", params![room.scope.note, "共享正文".repeat(1000)])?;
        let indexed = row(&temp.0, room.clone())?.unwrap();
        assert_eq!(indexed.summary.chars().count(), 160);
        assert_eq!(indexed.body.chars().count(), 4000);
        remember_access(&db, Access::Revoked)?;
        assert!(row(&temp.0, room)?.is_none());
        Ok(())
    }
}
