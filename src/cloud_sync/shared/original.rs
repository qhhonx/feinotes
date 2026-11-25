// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Keep the personal catalogue position while the shared library owns editing.
use super::*;
use crate::store::{Meta, SQLITE_FILE, Trashed};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::MetadataExt;

fn connect(root: &Path) -> Result<Connection> {
    let db = Connection::open_with_flags(root.join(SQLITE_FILE), rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(db)
}

fn entries(db: &Connection) -> Result<BTreeMap<String, Meta>> {
    let mut stmt = db.prepare("SELECT id,meta FROM library_entries")?;
    let mut rows = stmt.query([])?;
    let mut entries = BTreeMap::new();
    while let Some(row) = rows.next()? { entries.insert(row.get(0)?, serde_json::from_str(&row.get::<_, String>(1)?)?); }
    Ok(entries)
}

fn trash_entries(db: &Connection) -> Result<BTreeMap<String, Trashed>> {
    let mut stmt = db.prepare("SELECT id,meta FROM library_trash")?;
    let mut rows = stmt.query([])?;
    let mut entries = BTreeMap::new();
    while let Some(row) = rows.next()? { entries.insert(row.get(0)?, serde_json::from_str(&row.get::<_, String>(1)?)?); }
    Ok(entries)
}

fn subtree(all: &BTreeMap<String, Meta>, id: &str) -> Result<Vec<String>> {
    ensure!(all.contains_key(id), "The selected item no longer exists");
    let mut ids = vec![id.to_owned()];
    let mut seen = BTreeSet::from([id.to_owned()]);
    let mut index = 0;
    while index < ids.len() {
        let parent = ids[index].clone();
        if all[&parent].folder.is_some() {
            for (child, meta) in all.iter().filter(|(_, m)| m.parent.as_deref() == Some(&parent)) {
                ensure!(seen.insert(child.clone()), "Invalid folder cycle");
                ensure!(!meta.local(), "An external file cannot be shared");
                ids.push(child.clone());
            }
        }
        index += 1;
    }
    Ok(ids)
}

fn store_meta(db: &Connection, id: &str, meta: &Meta) -> Result<()> {
    db.execute("INSERT OR REPLACE INTO library_entries(id,meta) VALUES (?1,?2)", params![id, serde_json::to_string(meta)?])?;
    Ok(())
}

fn remove_purged_link(db: &Connection, id: &str) -> Result<()> {
    db.execute("DELETE FROM library_entries WHERE id = ?1", [id])?;
    db.execute("DELETE FROM library_trash WHERE id = ?1", [id])?;
    Store::remove_sync_body(db, id)
}

fn copy_entry(source: (&Connection, &Path), dest: (&Connection, &Path), id: &str, meta: &Meta) -> Result<()> {
    if meta.folder.is_none() {
        let (doc, _, front) = Store::read_sync_body(source.0, id, false)?;
        copy_assets(source.1, dest.1, &doc)?;
        Store::write_sync_body(dest.0, id, false, &doc, &front)?;
    }
    store_meta(dest.0, id, meta)
}

const NEW_PENDING: &str = "new-shared-pending";
const NEW_INVITED: &str = "new-shared-invited";

pub(crate) fn prepare_new(personal: &Path, id: String, title: String, folder: bool, public: Option<Access>) -> Result<Room> {
    Command::new(&account(personal)?, "checkAccess", None)?.send()?;
    let room = Room { scope: Scope { note: id, owner: None, folder }, title };
    stage_with(personal, &room, |staged| {
        let mut journal = Journal::open(staged, None)?;
        if folder {
            let db = connect(staged)?;
            let time = crate::time::now();
            let meta = Meta { folder: Some(room.title.clone()), created: time, modified: time, ..Default::default() };
            store_meta(&db, &room.scope.note, &meta)?;
            journal.merge(&Default::default())?;
        } else {
            let doc = Document { title: crate::model::RichText::plain(&room.title), ..Default::default() };
            journal.seed_shared(&doc, "")?;
        }
        fs::write(staged.join(NEW_PENDING), b"1")?;
        stage_public_access(staged, public)?;
        Ok(())
    })?;
    Ok(room)
}

pub(crate) fn invite_new(personal: &Path, room: &Room, email: &str, access: Access, revision: u64) -> Result<Reply> {
    ensure_created(personal, room)?;
    super::super::check_account_revision(revision)?;
    let reply = manage(personal, room, "invite", email, Some(access))?;
    register_new(personal, room, revision)?;
    Ok(reply)
}

pub(crate) fn register_new(personal: &Path, room: &Room, revision: u64) -> Result<()> {
    super::super::check_account_revision(revision)?;
    let root = room_root(personal, &room.scope.note)?;
    fs::write(root.join(NEW_INVITED), b"1")?;
    super::super::check_account_revision(revision)?;
    finish_new(personal, &root, room)
}

pub(crate) fn pending_new(root: &Path) -> bool { root.join(NEW_PENDING).is_file() }

pub(in crate::cloud_sync) fn new_pending(personal: &Path, root: &Path, room: &Room) -> Result<bool> {
    if !pending_new(root) { return Ok(false); }
    if !root.join(NEW_INVITED).is_file() { return Ok(true); }
    finish_new(personal, root, room)?;
    Ok(false)
}

fn finish_new(personal: &Path, root: &Path, room: &Room) -> Result<()> {
    ensure!(room.scope.owner.is_none() && account(personal)? == account(root)?, "Invalid new shared item");
    let mut db = connect(personal)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let id = &room.scope.note;
    let old: Option<String> = tx.query_row("SELECT meta FROM library_entries WHERE id=?1", [id], |row| row.get(0)).optional()?;
    if let Some(old) = old {
        ensure!(serde_json::from_str::<Meta>(&old)?.share_root.as_ref() == Some(id), "Conflicting personal item");
    } else {
        let cache = connect(root)?;
        let raw: String = cache.query_row("SELECT meta FROM library_entries WHERE id=?1", [id], |row| row.get(0))?;
        let mut meta = serde_json::from_str::<Meta>(&raw)?;
        ensure!(meta.folder.is_some() == room.scope.folder, "Invalid new shared item kind");
        meta.share_root = Some(id.clone());
        copy_entry((&cache, root), (&tx, personal), id, &meta)?;
    }
    tx.commit()?;
    match fs::remove_file(root.join(NEW_PENDING)) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn stage_public_access(staged: &Path, public: Option<Access>) -> Result<()> {
    if let Some(access) = public {
        let marker: &[u8] = match access { Access::ReadOnly => b"1", Access::ReadWrite => b"readWrite", _ => anyhow::bail!("Invalid public link permission") };
        fs::write(staged.join(LINK_PENDING), marker)?;
    }
    Ok(())
}

pub(crate) fn prepare(personal: &Path, id: &str, title: String, public_link: Option<Access>) -> Result<Room> {
    Command::new(&account(personal)?, "checkAccess", None)?.send()?;
    prepare_recovering(personal, id, title, public_link, |room| owned_zone_exists(personal, room))
}

#[cfg(any(test, feature = "devtools"))]
pub(in crate::cloud_sync) fn prepare_local(personal: &Path, id: &str, title: String, public_link: bool) -> Result<Room> {
    prepare_with_access(personal, id, &title, public_link.then_some(Access::ReadOnly), None)
}

#[derive(Debug)]
struct RetainedOwnerCache(Room);

impl std::fmt::Display for RetainedOwnerCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::settings::tr("这篇文档仍有共享记录。请刷新「共享文档」并核对成员与权限后再试。",
            "This document still has a sharing record. Refresh Shared Documents, check Members and Access, then try again."))
    }
}

impl std::error::Error for RetainedOwnerCache {}

fn prepare_recovering(personal: &Path, id: &str, title: String, public_link: Option<Access>,
    zone_exists: impl FnOnce(&Room) -> Result<bool>) -> Result<Room> {
    let revision = super::super::account_revision();
    let conflict = match prepare_with_access(personal, id, &title, public_link, None) {
        Err(error) if error.is::<RetainedOwnerCache>() => error,
        outcome => return outcome,
    };
    let room = &conflict.downcast_ref::<RetainedOwnerCache>().expect("retained owner cache").0;
    crate::logging::write("Sharing preparation found a retained owner cache");
    let root = room_root(personal, id)?;
    let identity = fs::metadata(&root)?;
    let lock = crate::cloud_restore::lock_library(&root).map_err(|_| anyhow::anyhow!("{}", crate::settings::tr(
        "请先关闭这篇共享文档，再从原位置重试。", "Close this shared document, then try again from its original location.")))?;
    if zone_exists(room)? { return Err(conflict); }
    let journal = Journal::open_locked(&root, None, lock.try_clone()?)?;
    ensure!(journal.counts()? == (0, 0, 0, 0), "{}", crate::settings::tr(
        "这台设备还有待恢复的共享改动。请先保留这些改动。", "This device has shared changes to recover. Keep those changes first."));
    drop(journal);
    super::super::check_account_revision(revision)?;
    prepare_with_access(personal, id, &title, public_link, Some(&identity))
}

fn prepare_with_access(personal: &Path, id: &str, title: &str, public_link: Option<Access>, retired_cache: Option<&fs::Metadata>) -> Result<Room> {
    let mut db = connect(personal)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let all = entries(&tx)?;
    let ids = subtree(&all, id)?;
    let root_meta = &all[id];
    ensure!(!root_meta.local(), "External files cannot be shared in place");
    let room = Room { scope: Scope { note: id.into(), owner: None, folder: root_meta.folder.is_some() }, title: title.into() };
    let root = room_root(personal, id)?;
    if root_meta.share_root.as_deref() == Some(id) {
        ensure!(descriptor(&root)?.is_some_and(|r| r.scope == room.scope), "Shared library unavailable");
        return Ok(room);
    }
    let access_changed = || crate::settings::tr("所选文档的共享状态已变化。请刷新文档库后重试。",
        "Sharing for the selected item has changed. Refresh the library and try again.");
    ensure!(ids.iter().all(|id| all[id].share_root.is_none()), "{}", access_changed());
    // A crash before the catalogue transaction commits leaves a private, unpublished stage.
    if root.exists() {
        ensure!(descriptor(&root)?.is_some_and(|r| r.scope == room.scope) && account(&root)? == account(personal)?, "{}", access_changed());
        let was_published = root.join("seed-published").is_file();
        let ended = root.join("shared-note-ended").is_file();
        let recoverable = !room.scope.folder && !ended && was_published && !pending_new(&root);
        let verified = recoverable && retired_cache.is_some_and(|checked| fs::metadata(&root)
            .is_ok_and(|now| now.dev() == checked.dev() && now.ino() == checked.ino()));
        if recoverable && !verified { return Err(RetainedOwnerCache(room).into()); }
        ensure!(ended || root.join("original-stage").is_file() && !was_published || verified, "{}", access_changed());
        let _lock = if verified { None } else { Some(crate::cloud_restore::lock_library(&root)?) };
        let retired = personal.join("retired-shared-notes");
        fs::create_dir_all(&retired)?;
        fs::rename(&root, retired.join(format!("{id}-{}", uuid::Uuid::new_v4())))?;
    }
    stage_with(personal, &room, |staged| {
        let mut journal = Journal::open(staged, None)?;
        let mut dest = connect(staged)?;
        let dest = dest.transaction()?;
        for key in &ids {
            let source = &all[key];
            let meta = Meta { folder: source.folder.clone(), kind: source.kind, markdown: source.markdown, name: source.name.clone(),
                parent: if key == id { None } else { source.parent.clone() }, order: source.order, created: source.created,
                modified: source.modified, ..Default::default() };
            copy_entry((&tx, personal), (&dest, staged), key, &meta)?;
        }
        dest.commit()?;
        journal.merge(&Default::default())?;
        let (uploads, _, held, warnings) = journal.counts()?;
        ensure!(uploads > 0 && held == 0 && warnings == 0, "Some selected content or attachments are unavailable");
        fs::write(staged.join("original-stage"), b"1")?;
        stage_public_access(staged, public_link)?;
        Ok(())
    })?;
    for key in ids {
        let mut meta = all[&key].clone();
        meta.share_root = Some(id.into());
        store_meta(&tx, &key, &meta)?;
    }
    tx.commit()?;
    Ok(room)
}

pub(crate) fn owner_room(entries: &[crate::store::Entry], id: &str) -> Option<Room> {
    let root = entries.iter().find(|e| e.id == id)?.meta.share_root.as_deref()?;
    let entry = entries.iter().find(|e| e.id == root)?;
    Some(Room { scope: Scope { note: root.into(), owner: None, folder: entry.meta.folder.is_some() }, title: entry.title.clone() })
}

pub(crate) fn awaiting_publication(personal: &Path, room: &Room) -> bool {
    room.scope.owner.is_none() && room_root(personal, &room.scope.note).is_ok_and(|root|
        root.join("original-stage").exists() && !root.join("shared-note-ended").exists() && !root.join("seed-published").is_file())
}

fn standalone_trash(db: &Connection, id: &str) -> Result<Option<Trashed>> {
    let raw: Option<String> = db.query_row("SELECT meta FROM library_trash WHERE id=?1", [id], |row| row.get(0)).optional()?;
    Ok(raw.map(|raw| serde_json::from_str::<Trashed>(&raw)).transpose()?
        .filter(|trash| trash.meta.folder.is_none()))
}

pub(super) fn is_original(personal: &Path, room: &Room) -> Result<bool> {
    if room.scope.owner.is_some() { return Ok(false); }
    let id = &room.scope.note;
    let db = connect(personal)?;
    let meta: Option<String> = db.query_row("SELECT meta FROM library_entries WHERE id=?1", [id], |r| r.get(0)).optional()?;
    if let Some(meta) = meta { return Ok(serde_json::from_str::<Meta>(&meta)?.share_root.as_ref() == Some(id)); }
    Ok(!room.scope.folder && standalone_trash(&db, id)?.is_some_and(|trash| trash.meta.share_root.as_ref() == Some(id)))
}

pub(super) fn detach_ended_standalone_trash(personal: &Path, id: &str) -> Result<bool> {
    let mut db = connect(personal)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let Some(mut trash) = standalone_trash(&tx, id)? else { return Ok(false); };
    match trash.meta.share_root.as_deref() {
        None => return Ok(true),
        Some(root) if root == id => {}
        _ => return Ok(false),
    }
    trash.meta.share_root = None;
    tx.execute("UPDATE library_trash SET meta=?2 WHERE id=?1", params![id, serde_json::to_string(&trash)?])?;
    tx.commit()?;
    Ok(true)
}

/// Restore an ended standalone document's personal catalogue entry in place.
fn detach_standalone(personal: &Path, id: &str, verify: impl FnOnce() -> Result<()>) -> Result<()> {
    let mut db = connect(personal)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let raw: String = tx.query_row("SELECT meta FROM library_entries WHERE id=?1", [id], |row| row.get(0))?;
    let mut meta: Meta = serde_json::from_str(&raw)?;
    let body = tx.query_row("SELECT EXISTS(SELECT 1 FROM library_documents WHERE id=?1 AND trashed=0)", [id], |row| row.get::<_, bool>(0))?;
    ensure!(meta.folder.is_none() && body && meta.share_root.as_deref().is_none_or(|root| root == id), "{}",
        crate::settings::tr("本机还没有可恢复的个人文档。请等待同步后再试。", "The personal document is not available on this device yet. Try again after syncing."));
    verify()?;
    if meta.share_root.take().is_some() { store_meta(&tx, id, &meta)?; }
    tx.commit()?;
    Ok(())
}

pub(crate) fn needs_standalone_check(personal: &Path, room: &Room) -> bool {
    room.scope.owner.is_none() && !room.scope.folder && room_root(personal, &room.scope.note)
        .is_ok_and(|root| !root.exists() || root.join("shared-note-ended").is_file())
}

// A local end marker belongs to an older share. Confirm the current zone before
// using it to recover the private entry or retiring the old cache.
pub(crate) fn reconcile_standalone(personal: &Path, room: &Room) -> Result<bool> {
    reconcile_standalone_with(personal, room, || owned_zone_exists(personal, room))
}

fn reconcile_standalone_with(personal: &Path, room: &Room, zone_exists: impl FnOnce() -> Result<bool>) -> Result<bool> {
    ensure!(needs_standalone_check(personal, room), "Expected an ended or uncached standalone share");
    ensure!(is_original(personal, room)?, "The original document has changed");
    let revision = super::super::account_revision();
    let key = account(personal)?;
    let root = room_root(personal, &room.scope.note)?;
    let ended = root.join("shared-note-ended");
    let lock = if root.exists() {
        let lock = crate::cloud_restore::lock_library(&root)?;
        ensure!(ended.is_file() && descriptor(&root)?.is_some_and(|saved| saved.scope == room.scope)
            && account(&root)? == key, "The shared document has changed");
        Some(lock)
    } else { None };
    let active = zone_exists()?;
    let verify = || -> Result<()> {
        super::super::check_account_revision(revision)?;
        ensure!(account(personal)? == key && !personal.join("require-account-verification").exists()
            && (if lock.is_some() { ended.is_file() } else { !root.exists() }), "The shared document has changed");
        Ok(())
    };
    if !active {
        detach_standalone(personal, &room.scope.note, verify)?;
    } else {
        verify()?;
        if lock.is_some() {
            let retired = personal.join("retired-shared-notes");
            fs::create_dir_all(&retired)?;
            fs::rename(&root, retired.join(format!("{}-{}", room.scope.note, uuid::Uuid::new_v4())))?;
        }
    }
    Ok(!active)
}

// Save the latest materialized collaboration before removing anyone's access.
pub(super) fn restore(personal: &Path, room: &Room, revoke: impl FnOnce() -> Result<()>) -> Result<()> {
    let root = room_root(personal, &room.scope.note)?;
    let mut cache = connect(&root)?;
    let cache = cache.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut db = connect(personal)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let all = entries(&cache)?;
    let old = entries(&tx)?;
    let id = &room.scope.note;
    if !room.scope.folder && !old.contains_key(id)
        && let Some(mut trash) = standalone_trash(&tx, id)?
        && trash.meta.share_root.as_ref() == Some(id) {
        let (doc, _, front) = Store::read_sync_body(&cache, id, false)?;
        copy_assets(&root, personal, &doc)?;
        Store::write_sync_body(&tx, id, true, &doc, &front)?;
        trash.meta.share_root = None;
        tx.execute("UPDATE library_trash SET meta=?2 WHERE id=?1", params![id, serde_json::to_string(&trash)?])?;
        revoke()?;
        tx.commit()?;
        return Ok(());
    }
    let catalogue = room.scope.folder.then(|| super::super::journal::stored_catalogue(&cache)).transpose()?;
    let ids = subtree(&all, &room.scope.note)?;
    for key in &ids {
        let source = &all[key];
        let mut meta = old.get(key).cloned().unwrap_or_else(|| source.clone());
        ensure!(old.get(key).is_none_or(|m| m.share_root.as_ref() == Some(&room.scope.note)), "Conflicting personal item");
        meta.share_root = None;
        meta.folder = source.folder.clone();
        if key != &room.scope.note { meta.parent = source.parent.clone(); meta.order = source.order; }
        copy_entry((&cache, &root), (&tx, personal), key, &meta)?;
    }
    for (key, mut meta) in old {
        if meta.share_root.as_ref() == Some(&room.scope.note) && !all.contains_key(&key) {
            if catalogue.as_ref().is_some_and(|doc| super::super::catalogue::permanently_deleted(doc, &key)) {
                remove_purged_link(&tx, &key)?;
                continue;
            }
            meta.share_root = None;
            let deleted: Option<String> = cache.query_row("SELECT meta FROM library_trash WHERE id=?1", [&key], |row| row.get(0)).optional()?;
            let mut trash = deleted.map(|raw| serde_json::from_str::<crate::store::Trashed>(&raw)).transpose()?
                .unwrap_or_else(|| crate::store::Trashed { meta: meta.clone(), deleted: crate::time::now(), root: None });
            trash.meta = meta;
            tx.execute("INSERT OR REPLACE INTO library_trash(id,meta) VALUES (?1,?2)", params![key, serde_json::to_string(&trash)?])?;
            tx.execute("UPDATE library_documents SET trashed=1 WHERE id=?1", [&key])?;
            tx.execute("DELETE FROM library_entries WHERE id=?1", [&key])?;
        }
    }
    for (id, mut trash) in trash_entries(&tx)? {
        if trash.meta.share_root.as_ref() == Some(&room.scope.note) {
            if all.contains_key(&id) { tx.execute("DELETE FROM library_trash WHERE id=?1", [&id])?; }
            else {
                trash.meta.share_root = None;
                tx.execute("UPDATE library_trash SET meta=?2 WHERE id=?1", params![id, serde_json::to_string(&trash)?])?;
            }
        }
    }
    revoke()?;
    tx.commit()?;
    Ok(())
}

// Refresh the owner's private index and recovery body after a shared projection.
// A linked private body can never be edited by this client.
pub(crate) fn refresh_owner(cache: &Path, scope: &Scope, document: Option<&str>) -> Result<()> {
    if scope.owner.is_some() { return Ok(()); }
    let Some(personal) = cache.parent().and_then(Path::parent) else { return Ok(()); };
    if !personal.join(SQLITE_FILE).exists() { return Ok(()); }
    let mut source = connect(cache)?;
    let source = source.transaction()?;
    let all = entries(&source)?;
    if !all.contains_key(&scope.note) { return Ok(()); }
    let mut dest = connect(personal)?;
    let dest = dest.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let old = entries(&dest)?;
    let old_trash = trash_entries(&dest)?;
    if !old.get(&scope.note).is_some_and(|m| m.share_root.as_ref() == Some(&scope.note)) { return Ok(()); }
    let linked = |meta: &Meta| meta.share_root.as_ref() == Some(&scope.note);
    if document.is_none() && scope.folder {
        let catalogue = super::super::journal::stored_catalogue(&source)?;
        let source_trash = trash_entries(&source)?;
        for (id, meta) in old.iter().chain(old_trash.iter().map(|(id, trash)| (id, &trash.meta))) {
            if linked(meta) && !all.contains_key(id)
                && super::super::catalogue::permanently_deleted(&catalogue, id) {
                remove_purged_link(&dest, id)?;
            }
        }
        for (id, mut trash) in source_trash {
            if !scope.permits(&id) || all.contains_key(&id) { continue; }
            ensure!(old.get(&id).is_none_or(linked) && old_trash.get(&id).is_none_or(|old| linked(&old.meta)), "Conflicting personal trash item");
            trash.meta.share_root = Some(scope.note.clone());
            if trash.meta.folder.is_none() {
                // A pending remote document can have catalogue metadata before its body arrives.
                let body_exists: bool = source.query_row("SELECT EXISTS(SELECT 1 FROM library_documents WHERE id=?1 AND trashed=1)", [&id], |row| row.get(0))?;
                if body_exists {
                    let (doc, _, front) = Store::read_sync_body(&source, &id, true)?;
                    if Store::read_sync_body(&dest, &id, true).ok().is_none_or(|(previous, _, previous_front)| doc != previous || front != previous_front) {
                        copy_assets(cache, personal, &doc)?;
                        Store::write_sync_body(&dest, &id, true, &doc, &front)?;
                    }
                } else if old.contains_key(&id) || old_trash.contains_key(&id) {
                    dest.execute("UPDATE library_documents SET trashed=1 WHERE id=?1", [&id])?;
                } else { continue; }
            }
            let raw = serde_json::to_string(&trash)?;
            if old_trash.get(&id).map(serde_json::to_string).transpose()?.as_deref() != Some(&raw) {
                dest.execute("INSERT OR REPLACE INTO library_trash VALUES (?1,?2)", params![id, raw])?;
            }
            if old.contains_key(&id) { dest.execute("DELETE FROM library_entries WHERE id=?1", [&id])?; }
        }
    }
    let has_history: bool = dest.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='sync_history')", [], |r| r.get(0))?;
    let ids = match document { Some(id) if all.contains_key(id) => vec![id.to_owned()], Some(_) => vec![], None => subtree(&all, &scope.note)? };
    for id in ids {
        let src = &all[&id];
        let previous_meta = old.get(&id).or_else(|| old_trash.get(&id).map(|trash| &trash.meta));
        let mut meta = previous_meta.cloned().unwrap_or_else(|| src.clone());
        ensure!(previous_meta.is_none_or(linked), "Conflicting personal document");
        meta.share_root = Some(scope.note.clone());
        meta.folder = src.folder.clone(); meta.name = src.name.clone();
        if id != scope.note { meta.parent = src.parent.clone(); meta.order = src.order; }
        if src.folder.is_none() {
            let (doc, _, front) = Store::read_sync_body(&source, &id, false)?;
            if old.contains_key(&id) {
                let (previous, _, previous_front) = Store::read_sync_body(&dest, &id, false)?;
                if doc != previous || front != previous_front {
                    if has_history { super::super::journal::archive(&dest, &id, &previous, &previous_front, true)?; }
                    copy_assets(cache, personal, &doc)?;
                    Store::write_sync_body(&dest, &id, false, &doc, &front)?;
                }
            } else {
                copy_assets(cache, personal, &doc)?;
                Store::write_sync_body(&dest, &id, false, &doc, &front)?;
            }
        }
        if old_trash.contains_key(&id) { dest.execute("DELETE FROM library_trash WHERE id=?1", [&id])?; }
        meta.modified = meta.modified.max(src.modified);
        if old.get(&id).map(serde_json::to_string).transpose()? != Some(serde_json::to_string(&meta)?) { store_meta(&dest, &id, &meta)?; }
    }
    dest.commit()?;
    Ok(())
}

pub(crate) fn move_into(personal: &Path, room: &Room, selection: &[String], destination: &str) -> Result<()> {
    ensure!(room.scope.folder && room.scope.owner.is_none(), "Invalid destination share");
    let cache = room_root(personal, &room.scope.note)?;
    ensure!(!cache.join("shared-note-ended").exists() && descriptor(&cache)?.is_some_and(|saved| saved.scope == room.scope)
        && account(personal)? == account(&cache)?, "Shared folder unavailable");
    let mut source = connect(personal)?;
    let source = source.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut dest = connect(&cache)?;
    let dest = dest.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let all = entries(&source)?;
    let shared = entries(&dest)?;
    ensure!(shared.get(destination).is_some_and(|m| m.folder.is_some()), "Destination folder unavailable");
    let mut adding = BTreeSet::new();
    for id in selection { adding.extend(subtree(&all, id)?); }
    ensure!(!adding.contains(destination) && adding.iter().all(|key| !all[key].local() && all[key].share_root.is_none()), "This selection contains a shared or external item");
    dest.execute_batch("CREATE TABLE IF NOT EXISTS owner_imports(id TEXT PRIMARY KEY, revision INTEGER, parent TEXT)")?;
    for key in &adding {
        let original = &all[key];
        let parent = original.parent.as_ref().filter(|parent| adding.contains(*parent)).map_or(destination, String::as_str);
        let revision: i64 = source.query_row("SELECT coalesce((SELECT revision FROM library_documents WHERE id=?1),0)", [key], |r| r.get(0))?;
        if shared.contains_key(key) {
            let retry: bool = dest.query_row("SELECT EXISTS(SELECT 1 FROM owner_imports WHERE id=?1 AND revision=?2 AND parent=?3)", params![key, revision, parent], |r| r.get(0))?;
            ensure!(retry, "The shared folder already contains this document");
        } else {
            let meta = Meta { folder: original.folder.clone(), name: original.name.clone(), kind: original.kind, markdown: original.markdown,
                created: original.created, modified: original.modified, parent: Some(parent.into()), ..Default::default() };
            copy_entry((&source, personal), (&dest, &cache), key, &meta)?;
            dest.execute("INSERT OR REPLACE INTO owner_imports VALUES(?1,?2,?3)", params![key, revision, parent])?;
        }
        let mut link = original.clone();
        link.parent = Some(parent.into());
        link.share_root = Some(room.scope.note.clone());
        store_meta(&source, key, &link)?;
    }
    dest.commit()?;
    source.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud_sync::{Bytes, Changes};

    #[test]
    fn ended_standalone_link_recovers_only_after_a_confirmed_zone_deletion() -> Result<()> {
        struct Fixture(PathBuf);
        impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
        let fixture = Fixture(std::env::temp_dir().join(format!("fei-stale-original-{}", uuid::Uuid::new_v4())));
        let personal = &fixture.0;
        Store::initialize_local_sqlite(personal)?;
        fs::write(personal.join(".cloud-restore-ready"), format!("{}\n", "a".repeat(64)))?;
        let doc = crate::docxml::parse("<title>Personal preserved</title><p>Keep the original</p>")?;
        let id = Store::open_sqlite_at(personal.clone())?.create_from(&doc)?;
        let room = prepare_local(personal, &id, doc.title.text.clone(), true)?;
        let cache = room_root(personal, &id)?;
        fs::write(cache.join("shared-note-ended"), b"Ended on this device")?;
        assert!(reconcile_standalone_with(personal, &room, || anyhow::bail!("offline")).is_err());
        assert!(cache.join("shared-note-ended").is_file() && is_original(personal,&room)?);

        assert!(!reconcile_standalone_with(personal, &room, || Ok(true))?);
        assert!(!cache.exists() && is_original(personal,&room)?);
        assert_eq!(fs::read_dir(personal.join("retired-shared-notes"))?.count(), 1);

        assert!(reconcile_standalone_with(personal, &room, || Ok(false))?);
        assert!(!is_original(personal, &room)?);
        let mut store = Store::open_sqlite_at(personal.clone())?;
        assert_eq!(store.load(&id)?.plain_text(), doc.plain_text());
        assert!(store.list().iter().any(|entry| entry.id == id && entry.meta.share_root.is_none()));
        Ok(())
    }

    #[test]
    fn stopped_share_on_one_owner_device_can_be_shared_again_on_another() -> Result<()> {
        struct Fixture(PathBuf);
        impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
        let temp = Fixture(std::env::temp_dir().join(format!("fei-stop-republish-{}", uuid::Uuid::new_v4())));
        let first = temp.0.join("first");
        let second = temp.0.join("second");
        for path in [&first, &second] {
            Store::initialize_local_sqlite(path)?;
            fs::write(path.join(".cloud-restore-ready"), format!("{}\n", "a".repeat(64)))?;
        }
        let doc = crate::docxml::parse("<title>Synthetic reuse</title><p>Original body</p>")?;
        let id = Store::open_sqlite_at(first.clone())?.create_from(&doc)?;
        let room = prepare_local(&first, &id, doc.title.text.clone(), true)?;
        fs::write(room_root(&first, &id)?.join("seed-published"), b"1")?;
        let mirror_catalogue = || -> Result<()> {
            let source = connect(&first)?;
            let target = connect(&second)?;
            copy_entry((&source, &first), (&target, &second), &id, &entries(&source)?[&id])
        };
        mirror_catalogue()?;
        let stale_root = room_root(&second, &id)?;
        prepare_room(&second, &stale_root, &room)?;
        let other = Store::open_sqlite_at(second.clone())?.create_from(&Document::default())?;
        let other_room = prepare_local(&second, &other, "Unrelated named share".into(), false)?;
        let other_cache = room_root(&second, &other)?;
        fs::write(other_cache.join("seed-published"), b"1")?;

        let remote_present = std::cell::Cell::new(true);
        restore(&first, &room, || { remote_present.set(false); Ok(()) })?;
        mirror_catalogue()?;
        assert!(!is_original(&second, &room)?);
        assert!(stale_root.join("seed-published").is_file());
        assert!(prepare_local(&second, &id, room.title.clone(), true)
            .is_err_and(|error| error.is::<RetainedOwnerCache>()));
        assert!(prepare_recovering(&second, &id, room.title.clone(), Some(Access::ReadOnly), |_| Ok(true))
            .is_err_and(|error| error.is::<RetainedOwnerCache>()));
        assert!(stale_root.join("seed-published").is_file());

        let recreated = prepare_recovering(&second, &id, room.title.clone(), Some(Access::ReadWrite), |requested| {
            assert_eq!(requested.scope, room.scope);
            Ok(remote_present.get())
        })?;
        assert_eq!(recreated.scope, room.scope);
        assert!(is_original(&second, &room)?);
        assert!(!stale_root.join("seed-published").exists());
        assert_eq!(pending_public_access(&second, &recreated), Some(Access::ReadWrite));
        assert_eq!(Store::open_sqlite_at(stale_root)?.load(&id)?.plain_text(), doc.plain_text());
        assert_eq!(fs::read_dir(second.join("retired-shared-notes"))?.count(), 1);
        assert!(other_cache.join("seed-published").is_file() && is_original(&second, &other_room)?);
        Ok(())
    }

    #[test]
    fn public_folder_prepares_the_entire_tree_before_publishing() -> Result<()> {
        struct Fixture(PathBuf);
        impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
        let fixture = Fixture(std::env::temp_dir().join(format!("fei-public-folder-{}", uuid::Uuid::new_v4())));
        let personal = fixture.0.as_path();
        Store::initialize_local_sqlite(personal)?;
        fs::write(personal.join(".cloud-restore-ready"), format!("{}\n", "a".repeat(64)))?;
        let mut store = Store::open_sqlite_at(personal.to_path_buf())?;
        let folder = store.create_folder("Public folder", None)?;
        let nested = store.create_folder("Nested", Some(&folder))?;
        let doc = crate::docxml::parse("<title>Nested note</title><p>Shared content</p>")?;
        let note = store.create_from_in(&doc, Some(&nested))?;
        let private = store.create_from(&Document::default())?;

        let room = prepare_with_access(personal, &folder, "Public folder", Some(Access::ReadWrite), None)?;
        let cache = room_root(personal, &folder)?;
        assert!(room.scope.folder && awaiting_publication(personal, &room));
        assert_eq!(pending_public_access(personal, &room), Some(Access::ReadWrite));
        let mut shared = Store::open_sqlite_at(cache)?;
        let entries = shared.list();
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().any(|entry| entry.id == note && entry.meta.parent.as_deref() == Some(nested.as_str())));
        assert_eq!(shared.load(&note)?.plain_text(), doc.plain_text());
        assert!(owner_room(&store.list(), &private).is_none());
        Ok(())
    }

    #[test]
    fn owner_folder_trash_and_restore_follow_the_shared_catalogue() -> Result<()> {
        struct Fixture(PathBuf);
        impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
        let temp = Fixture(std::env::temp_dir().join(format!("fei-owner-folder-trash-{}", uuid::Uuid::new_v4())));
        let personal = temp.0.as_path();
        Store::initialize_local_sqlite(personal)?;
        fs::write(personal.join(".cloud-restore-ready"), format!("{}\n", "a".repeat(64)))?;
        let mut original = Store::open_sqlite_at(personal.to_path_buf())?;
        let root = original.create_folder("Shared", None)?;
        let folder = original.create_folder("Nested", Some(&root))?;
        let document = original.create_from_in(&Document::default(), Some(&folder))?;
        let room = prepare_local(personal, &root, "Shared".into(), false)?;
        let cache = room_root(personal, &root)?;
        let mut writer = Store::open_sqlite_at(cache.clone())?;

        // Build 119 could write a linked deletion to the personal copy alone.
        original.delete(&folder, None)?;
        refresh_owner(&cache, &room.scope, None)?;
        assert!(original.list().iter().any(|e| e.id == folder));
        assert!(original.list_trash().is_empty());

        writer.trash_sqlite_folder(&folder)?;
        refresh_owner(&cache, &room.scope, None)?;
        assert!(original.list().iter().all(|e| e.id != folder && e.id != document));
        let trash = original.list_trash();
        assert_eq!(trash.len(), 1);
        assert_eq!(trash[0].id, folder);
        assert_eq!(trash[0].share_root.as_deref(), Some(root.as_str()));

        writer.restore(&folder)?;
        refresh_owner(&cache, &room.scope, None)?;
        assert!(original.list().iter().any(|e| e.id == document && e.meta.parent.as_ref() == Some(&folder)));
        assert!(original.list_trash().is_empty());
        Ok(())
    }

    #[test]
    fn ending_a_trashed_standalone_original_keeps_one_personal_trash_item() -> Result<()> {
        struct Fixture(PathBuf);
        impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
        let temp = Fixture(std::env::temp_dir().join(format!("fei-ended-standalone-trash-{}", uuid::Uuid::new_v4())));
        let personal = temp.0.as_path();
        Store::initialize_local_sqlite(personal)?;
        fs::write(personal.join(".cloud-restore-ready"), format!("{}\n", "a".repeat(64)))?;
        let mut store = Store::open_sqlite_at(personal.to_path_buf())?;
        let document = store.create_from(&crate::docxml::parse("<title>Original</title><p>before sharing</p>")?)?;
        let room = prepare_local(personal, &document, "Original".into(), false)?;
        let cache = room_root(personal, &document)?;
        let latest = crate::docxml::parse("<title>Latest</title><p>shared edit</p>")?;
        Store::write_sync_body(&connect(&cache)?, &document, false, &latest, "")?;

        store.delete(&document, None)?;
        let deleted = store.list_trash()[0].deleted;
        assert!(is_original(personal, &room)?);
        assert!(!super::super::detach_ended_standalone_trash(personal, &document)?);
        assert!(restore(personal, &room, || anyhow::bail!("simulated revoke failure")).is_err());
        assert_eq!(store.list_trash()[0].title, "Original");
        assert_eq!(store.list_trash()[0].share_root.as_deref(), Some(document.as_str()));

        restore(personal, &room, || Ok(()))?;
        let items = store.list_trash();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, document);
        assert_eq!(items[0].title, "Latest");
        assert_eq!(items[0].deleted, deleted);
        assert_eq!(items[0].parent, None);
        assert_eq!(items[0].share_root, None);
        assert!(store.list().is_empty());
        store.purge(&document)?;
        assert!(store.list_trash().is_empty());
        Ok(())
    }

    #[test]
    fn legacy_ended_standalone_trash_can_be_purged_as_personal() -> Result<()> {
        struct Fixture(PathBuf);
        impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
        let temp = Fixture(std::env::temp_dir().join(format!("fei-legacy-standalone-trash-{}", uuid::Uuid::new_v4())));
        let personal = temp.0.as_path();
        Store::initialize_local_sqlite(personal)?;
        fs::write(personal.join(".cloud-restore-ready"), format!("{}\n", "a".repeat(64)))?;
        let mut store = Store::open_sqlite_at(personal.to_path_buf())?;
        let document = store.create_from(&Document::default())?;
        prepare_local(personal, &document, "Shared".into(), false)?;
        let cache = room_root(personal, &document)?;
        store.delete(&document, None)?;
        fs::write(cache.join("shared-note-ended"), b"Ended")?;

        fs::write(cache.join(".cloud-restore-ready"), format!("{}\n", "b".repeat(64)))?;
        assert!(!super::super::detach_ended_standalone_trash(personal, &document)?);
        fs::write(cache.join(".cloud-restore-ready"), format!("{}\n", "a".repeat(64)))?;
        assert!(super::super::detach_ended_standalone_trash(personal, &document)?);
        assert!(super::super::detach_ended_standalone_trash(personal, &document)?);
        assert_eq!(store.list_trash()[0].share_root, None);
        store.purge(&document)?;
        assert!(store.list_trash().is_empty());
        Ok(())
    }

    #[test]
    fn cached_owned_trash_purge_records_deletion_before_removing_private_projection() -> Result<()> {
        struct Fixture(PathBuf);
        impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
        let temp = Fixture(std::env::temp_dir().join(format!("fei-owned-trash-purge-{}", uuid::Uuid::new_v4())));
        let personal = temp.0.as_path();
        Store::initialize_local_sqlite(personal)?;
        fs::write(personal.join(".cloud-restore-ready"), format!("{}\n", "a".repeat(64)))?;
        let mut private = Store::open_sqlite_at(temp.0.clone())?;
        let folder = private.create_folder("Shared", None)?;
        let live = private.create_folder("Keep", Some(&folder))?;
        let legacy = private.create_from_in(&Document::default(), Some(&folder))?;
        let current = private.create_folder("Trash group", Some(&folder))?;
        let child = private.create_from_in(&Document::default(), Some(&current))?;
        let room = prepare_local(personal, &folder, "Shared".into(), false)?;
        assert_eq!(owned_trash_room(personal, &folder)?, Some(room.clone()));
        let cache = room_root(personal, &folder)?;
        let mut shared = Store::open_sqlite_at(cache.clone())?;
        shared.delete(&legacy, None)?;
        shared.trash_sqlite_folder(&current)?;
        Journal::open(&cache, None)?.merge(&Default::default())?;
        refresh_owner(&cache, &room.scope, None)?;
        assert_eq!(private.list_trash().len(), 2);

        shared.purge(&legacy)?;
        assert!(purge_folder_trash(&mut shared, &[current.clone(), live.clone()]).is_err());
        assert_eq!(shared.list_trash().len(), 1);
        let before: i64 = connect(&cache)?.query_row("SELECT count(*) FROM outbox", [], |r| r.get(0))?;
        let selected = [legacy.clone(), current.clone()];
        purge_folder_trash(&mut shared, &selected)?;
        purge_folder_trash(&mut shared, &selected)?;
        let db = connect(&cache)?;
        let stored = crate::cloud_sync::journal::stored_catalogue(&db)?;
        assert!([&legacy, &current, &child].into_iter().all(|id| crate::cloud_sync::catalogue::permanently_deleted(&stored, id)));
        let after: i64 = db.query_row("SELECT count(*) FROM outbox", [], |r| r.get(0))?;
        assert_eq!(after, before + 1);
        refresh_owner(&cache, &room.scope, None)?;
        assert!(private.list_trash().is_empty());
        assert!(shared.list_trash().is_empty());
        assert!(shared.list().iter().any(|entry| entry.id == live));
        Ok(())
    }

    #[test]
    fn folder_checkpoints_restore_nested_content_without_granting_cleanup_to_readers() -> Result<()> {
        use super::super::super::retention;
        let temp = std::env::temp_dir().join(format!("fei-folder-checkpoint-{}", uuid::Uuid::new_v4()));
        let personal = temp.join("owner"); let guest = temp.join("guest");
        for (dir, key) in [(&personal, 'a'), (&guest, 'b')] {
            Store::initialize_local_sqlite(dir)?;
            fs::write(dir.join(".cloud-restore-ready"), format!("{}\n", key.to_string().repeat(64)))?;
        }
        let mut private = Store::open_sqlite_at(personal.clone())?;
        let folder = private.create_folder("Shared", None)?;
        let nested = private.create_folder("Nested", Some(&folder))?;
        let child = private.create_from(&crate::docxml::parse("<title>Child</title><p>initial</p>")?)?;
        private.set_parent(&child, Some(&nested))?;
        let unrelated = private.create_from(&crate::docxml::parse("<title>Private</title><p>secret</p>")?)?;
        let room = prepare_local(&personal, &folder, "Shared".into(), false)?;
        let root = room_root(&personal, &folder)?;
        let mut owner = Journal::open(&root, None)?;
        let mut store = Store::open_sqlite_at(root.clone())?;
        let mut doc = store.load(&child)?;
        let mut cloud = std::collections::HashMap::new();
        let receipt = |edits: Vec<crate::cloud_sync::Edit>, deleted, access, acknowledge: bool| Changes {
            retention: Some(retention::Receipt { reset: false, ready: true, deleted, chunks: vec![] }),
            uploaded: if acknowledge { edits.iter().map(|e| e.id.clone()).collect() } else { vec![] },
            edits, token: Some(Bytes(vec![1])), more: false, legacy: false, room_access: Some(access),
            #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false };
        for n in 0..40 {
            doc.blocks[0].text = crate::model::RichText::plain(format!("edit {n}"));
            store.save(&child, &doc)?;
            store.rename(&nested, &format!("Nested {n}"))?;
            owner.merge(&Default::default())?;
            let edits = owner.request()?.edits;
            for e in &edits { cloud.insert(e.id.clone(), e.clone()); }
            owner.receive(receipt(edits, vec![], Access::Owner, true))?;
            owner.merge(&Default::default())?;
        }
        let mut checkpoints = 0;
        for _ in 0..12 {
            let Some(plan) = owner.request()?.maintenance else { break; };
            if let Some(snapshot) = plan.snapshot {
                assert!(plan.remove.is_empty());
                cloud.insert(snapshot.id.clone(), snapshot.clone()); checkpoints += 1;
                owner.receive(receipt(vec![snapshot], vec![], Access::Owner, false))?;
            } else {
                for id in &plan.remove { cloud.remove(id); }
                owner.receive(receipt(vec![], plan.remove, Access::Owner, false))?;
            }
            owner.merge(&Default::default())?;
        }
        assert!(checkpoints >= 2, "Both the folder catalogue and body need checkpoints");
        let remote = Room { scope: Scope { owner: Some("synthetic-owner".into()), ..room.scope }, title: room.title };
        let restored_root = stage(&guest, &remote, None)?;
        let mut reader = Journal::open(&restored_root, None)?;
        reader.receive(receipt(cloud.into_values().collect(), vec![], Access::ReadOnly, false))?;
        reader.merge(&Default::default())?;
        let restored = Store::open_sqlite_at(restored_root.clone())?;
        assert_eq!(restored.load(&child)?, doc);
        let entries = entries(&connect(&restored_root)?)?;
        assert_eq!(entries[&nested].folder.as_deref(), Some("Nested 39"));
        assert_eq!(entries[&child].parent.as_deref(), Some(nested.as_str()));
        assert!(!entries.contains_key(&unrelated));
        let request = reader.request()?;
        assert!(request.edits.is_empty() && request.maintenance.is_none());
        drop((owner, reader, store, restored, private)); fs::remove_dir_all(temp)?;
        Ok(())
    }

    #[test]
    fn folder_round_trip_preserves_identity_and_limits_the_invitation_to_its_subtree() -> Result<()> {
        let temp = std::env::temp_dir().join(format!("fei-folder-share-{}", uuid::Uuid::new_v4()));
        let personal = temp.join("personal");
        let guest = temp.join("guest");
        for (dir, key) in [(&personal, 'a'), (&guest, 'b')] {
            Store::initialize_local_sqlite(dir)?;
            fs::write(dir.join(".cloud-restore-ready"), format!("{}\n", key.to_string().repeat(64)))?;
        }
        let mut owner_store = Store::open_sqlite_at(personal.clone())?;
        let outer = owner_store.create_folder("private parent", None)?;
        let root = owner_store.create_folder("team", Some(&outer))?;
        let sub = owner_store.create_folder("week one", Some(&root))?;
        let (private, _) = owner_store.create()?;
        let mut doc = Document::default();
        doc.title = crate::model::RichText::plain("Agenda");
        let child = owner_store.create_from_in(&doc, Some(&sub))?;
        let room = prepare_local(&personal, &root, "team".into(), false)?;
        assert_eq!(prepare_local(&personal, &root, "team".into(), false)?, room);
        let mut remote = room.clone();
        remote.scope.owner = Some("synthetic-owner".into());
        let remote_root = stage(&guest, &remote, None)?;
        let owner_root = room_root(&personal, &root)?;
        let owner = Journal::open(&owner_root, None)?;
        let mut reader = Journal::open(&remote_root, None)?;
        let page = |edits, access| Changes { retention: None, uploaded: vec![], edits, token: Some(Bytes(vec![1])), more: false, legacy: false,
            room_access: Some(access), #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false };
        reader.receive(page(owner.request()?.edits, Access::ReadOnly))?;
        reader.merge(&Default::default())?;
        let projected = entries(&connect(&remote_root)?)?;
        assert_eq!(projected.len(), 3);
        assert_eq!(projected[&child].parent.as_ref(), Some(&sub));
        assert!(projected[&root].parent.is_none());
        assert!(!projected.contains_key(&private));
        assert!(!projected.contains_key(&outer));
        let mut guest_store = Store::open_sqlite_at(remote_root)?;
        assert!(guest_store.create_folder("forbidden", Some(&sub)).is_err());
        assert!(guest_store.rename(&sub, "forbidden").is_err());
        assert_eq!(guest_store.load(&child)?, doc);
        assert!(reader.request()?.edits.is_empty());
        assert!(owner_store.save(&child, &doc).is_err());
        let later = owner_store.create_from(&doc)?;
        move_into(&personal, &room, std::slice::from_ref(&later), &sub)?;
        let mut owner = owner;
        owner.merge(&Default::default())?;
        reader.receive(page(owner.request()?.edits, Access::ReadWrite))?;
        reader.merge(&Default::default())?;
        assert!(guest_store.list().iter().any(|entry| entry.id == later && entry.meta.parent.as_ref() == Some(&sub)));
        let guest_child = guest_store.create_folder("participant subfolder", Some(&sub))?;
        reader.merge(&Default::default())?;
        owner.receive(page(reader.request()?.edits, Access::Owner))?;
        owner.merge(&Default::default())?;
        let mut shared_store = Store::open_sqlite_at(owner_root)?;
        assert!(shared_store.list().iter().any(|entry| entry.id == guest_child));
        let mut edited = shared_store.load(&child)?;
        edited.title = crate::model::RichText::plain("Latest edits");
        shared_store.save(&child, &edited)?;
        shared_store.delete(&later, None)?;
        restore(&personal, &room, || Ok(()))?;
        assert!(!entries(&connect(&personal)?)?.contains_key(&later));
        assert!(connect(&personal)?.query_row("SELECT trashed FROM library_documents WHERE id=?1", [&later], |r| r.get::<_, bool>(0))?);
        assert_eq!(owner_store.load(&child)?, edited);
        let restored = entries(&connect(&personal)?)?;
        assert_eq!(restored[&root].parent.as_ref(), Some(&outer));
        assert!(restored[&child].share_root.is_none());
        drop((owner, reader));
        fs::remove_dir_all(temp)?;
        Ok(())
    }
}
