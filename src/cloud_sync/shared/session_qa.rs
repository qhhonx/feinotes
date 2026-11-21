// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Offline native multi-library fixtures and assertions.

use super::*;

pub(crate) fn prepare(root: &Path) -> Result<PathBuf> {
    let personal = root.join("a".repeat(64));
    ensure!(!personal.exists(), "Shared-session QA requires a fresh synthetic library");
    let source = personal.join(".restore-source");
    let mut file = Store::open_at(source.clone())?;
    let original = file.create_from(&Document { title: crate::model::RichText::plain("Personal fixture"),
        blocks: vec![crate::model::Block::text("private body")], ..Default::default() })?;
    fs::write(source.join(".cloud-restore-ready"), format!("{}\n", "a".repeat(64)))?;
    Store::initialize_sqlite(&personal)?;
    let mut rooms = vec![];
    for (title, received) in [("Shared owner fixture", false), ("Shared reader fixture", true)] {
        let seed = if received { root.join("synthetic-owner") } else { personal.clone() };
        fs::create_dir_all(&seed)?;
        if received { fs::write(seed.join(".cloud-restore-ready"), format!("{}\n", "b".repeat(64)))?; }
        let mut doc = sample(&seed)?;
        doc.title = crate::model::RichText::plain(title);
        if received { fs::write(seed.join("assets/files/shared-fixture.txt"), b"reader-only attachment fixture")?; }
        let mut room = Room { scope: Scope { note: uuid::Uuid::new_v4().to_string(), owner: None, folder: false }, title: title.into() };
        let path = stage(&seed, &room, Some((&doc, "")))?;
        if received {
            let owner = Journal::open(&path, None)?;
            let edits = owner.request()?.edits;
            room.scope.owner = Some("synthetic-remote-owner".into());
            let guest = stage(&personal, &room, None)?;
            let mut reader = Journal::open(&guest, None)?;
            reader.receive(crate::cloud_sync::Changes { retention: None, uploaded: vec![], edits,
                token: Some(crate::cloud_sync::Bytes(vec![1])), more: false, legacy: false,
                room_access: Some(Access::ReadOnly), cursor_replayed: false, upload_quota_exceeded: false })?;
            reader.merge(&Default::default())?;
        }
        rooms.push(room);
    }
    if std::env::var_os("QUIET_SYNC_QA_UNREAD_UI").is_some() {
        let room = original::prepare_local(&personal, &original, "Personal fixture".into(), false)?;
        let root = room_root(&personal, &room.scope.note)?;
        let db = rusqlite::Connection::open(root.join(crate::store::SQLITE_FILE))?;
        super::super::activity::schema(&db)?;
        db.execute("INSERT INTO shared_activity (id, projected) VALUES ('qa-unread', 1)", [])?;
        let order: Vec<&str> = rooms.iter().map(|r| r.scope.note.as_str()).chain(std::iter::once(&original[..])).collect();
        let db = rusqlite::Connection::open(personal.join(crate::store::SQLITE_FILE))?;
        db.execute("INSERT INTO library_values (key, value) VALUES ('shared-sidebar-order', ?1)", [serde_json::to_vec(&order)?])?;
    }
    if std::env::var_os("QUIET_SYNC_QA_TRASH_PURGE").is_some() {
        let mut store = Store::open_sqlite_at(personal.clone())?;
        let private = store.create_from(&crate::docxml::parse("<title>QA personal trash</title><p>throwaway</p>")?)?;
        store.delete(&private, None)?;
        let personal_folder = store.create_folder("QA personal folder", None)?;
        store.delete(&personal_folder, None)?;
        for title in ["QA ended standalone single", "QA ended standalone batch"] {
            let document = store.create_from(&crate::docxml::parse(&format!("<title>{title}</title><p>throwaway</p>"))?)?;
            original::prepare_local(&personal, &document, title.into(), false)?;
            store.delete(&document, None)?;
            let cache = room_root(&personal, &document)?;
            fs::write(cache.join("shared-note-ended"), b"Legacy ended standalone share")?;
        }
        for (title, legacy) in [("QA shared trash", false), ("QA stale shared trash", true)] {
            let folder = store.create_folder(&format!("{title} parent"), None)?;
            let removed = store.create_folder(title, Some(&folder))?;
            store.create_from_in(&crate::docxml::parse("<title>QA nested trash</title><p>throwaway</p>")?, Some(&removed))?;
            let removed_document = if legacy { None } else {
                Some(store.create_from_in(&crate::docxml::parse("<title>QA shared document</title><p>throwaway</p>")?, Some(&folder))?)
            };
            let room = original::prepare_local(&personal, &folder, title.into(), false)?;
            let cache = room_root(&personal, &folder)?;
            fs::write(cache.join("seed-published"), b"synthetic shared owner")?;
            let mut writer = Store::open_sqlite_at(cache.clone())?;
            writer.trash_sqlite_folder(&removed)?;
            if let Some(document) = removed_document { writer.delete(&document, None)?; }
            Journal::open(&cache, None)?.merge(&Default::default())?;
            original::refresh_owner(&cache, &room.scope, None)?;
            if legacy { writer.purge(&removed)?; }
        }
    }
    if std::env::var_os("QUIET_SYNC_QA_TRASH_ROOT_SHARE").is_some() {
        let mut store = Store::open_sqlite_at(personal.clone())?;
        let title = "QA shared root folder";
        let folder = store.create_folder(title, None)?;
        let room = original::prepare_local(&personal, &folder, title.into(), false)?;
        fs::write(room_root(&personal, &room.scope.note)?.join("seed-published"), b"synthetic shared owner")?;
        store.trash_sqlite_folder(&folder)?;
    }
    if std::env::var_os("QUIET_SYNC_QA_DOCUMENT_STATUS").is_some() {
        let mut store = Store::open_sqlite_at(personal.clone())?;
        let title = "长标题验收 · 从主页到搜索都应能看懂这个文档的共享与置顶状态 · 窄窗口里仍能打开更多操作 · Windows, Mac and Mobile document title layout · 第四季度工作记录";
        let document = store.create_from(&Document { title: crate::model::RichText::plain(title),
            blocks: vec![crate::model::Block::text("仅供测试的正文。")], ..Default::default() })?;
        store.update_meta(&document, |meta| { meta.pinned = true; meta.root_order = Some(0); })?;
        let room = original::prepare_local(&personal, &document, title.into(), false)?;
        fs::write(room_root(&personal, &room.scope.note)?.join("seed-published"), b"synthetic shared owner")?;
    }
    if std::env::var_os("QUIET_SYNC_QA_MOVE_MATRIX").is_some() {
        fn make_items(store: &mut Store, prefix: &str, parent: Option<&str>) -> Result<()> {
            let source = store.create_folder(&format!("{prefix} source"), parent)?;
            store.create_folder(&format!("{prefix} target"), parent)?;
            let folder = store.create_folder(&format!("{prefix} moving folder"), Some(&source))?;
            for (title, parent) in [(format!("{prefix} moving document"), &source), (format!("{prefix} child"), &folder)] {
                store.create_from_in(&Document { title: crate::model::RichText::plain(title),
                    blocks: vec![crate::model::Block::text("Synthetic movable content")], ..Default::default() }, Some(parent))?;
            }
            Ok(())
        }
        let mut store = Store::open_sqlite_at(personal.clone())?;
        make_items(&mut store, "Move personal", None)?;
        for title in ["Move shared A", "Move shared B"] {
            let folder = store.create_folder(title, None)?;
            make_items(&mut store, title, Some(&folder))?;
            let room = original::prepare_local(&personal, &folder, title.into(), false)?;
            fs::write(room_root(&personal, &room.scope.note)?.join("seed-published"), b"synthetic shared owner")?;
        }
    }
    if std::env::var_os("QUIET_SYNC_QA_SHARED_BROWSER_REFRESH").is_some() {
        let mut store = Store::open_sqlite_at(personal.clone())?;
        let folder = store.create_folder("QA collaboration folder", None)?;
        let title = "QA standalone collaboration";
        let document = store.create_from_in(&Document { title: crate::model::RichText::plain(title),
            blocks: vec![crate::model::Block::text("Original fixture paragraph")], ..Default::default() }, Some(&folder))?;
        let room = original::prepare_local(&personal, &document, title.into(), false)?;
        fs::write(room_root(&personal, &room.scope.note)?.join("seed-published"), b"synthetic shared owner")?;
    }
    fs::write(root.join("pause-cloud"), b"isolated local session regression")?;
    fs::write(root.join("qa-session-rooms.json"), serde_json::to_vec(&rooms)?)?;
    Ok(personal)
}

pub(crate) fn open(cx: &mut gpui::App) -> Result<()> {
    ensure!(crate::devtools::scripted() && std::env::var_os("QUIET_SYNC_QA_SHARED_SESSIONS").is_some(), "Use isolated shared-session QA");
    let rooms: Vec<Room> = serde_json::from_slice(&fs::read(crate::cloud_sync::test_root()?.context("Missing QA root")?.join("qa-session-rooms.json"))?)?;
    for room in &rooms { open_window(room, cx)?; }
    ensure!(cx.windows().len() == 3, "Expected personal and two shared windows in one process");
    open_window(&rooms[0], cx)?;
    ensure!(cx.windows().len() == 3, "Opening the same room created another window");
    Ok(())
}

pub(crate) fn verify(cx: &mut gpui::App) -> Result<()> {
    use crate::{app::Workspace, editor::core::Core};
    let personal = Store::root_dir();
    let root = crate::cloud_sync::test_root()?.context("Missing QA root")?;
    let rooms: Vec<Room> = serde_json::from_slice(&fs::read(root.join("qa-session-rooms.json"))?)?;
    let room_root = room_root(&personal, &rooms[0].scope.note)?;
    let reader_root = self::room_root(&personal, &rooms[1].scope.note)?;
    let find = |root: &Path, cx: &gpui::App| cx.windows().into_iter().filter_map(|w| w.downcast::<Workspace>())
        .find(|w| w.read(cx).is_ok_and(|ws| ws.store.root() == root)).expect("library window");
    let primary = find(&personal, cx);
    let owner = find(&room_root, cx);
    let reader = find(&reader_root, cx);
    let original = primary.update(cx, |ws, _, _| { let id = ws.entries[0].id.clone(); ws.store.load(&id).unwrap() })?;
    verify_library_settings(cx, &primary, &owner, &personal, &room_root)?;
    let before_owner = journal_count(&room_root, cx)?;
    let before_reader = journal_count(&reader_root, cx)?;
    reader.update(cx, |ws, _, cx| -> Result<()> {
        let editor = ws.editor.as_ref().context("Reader editor not ready")?;
        ensure!(editor.read(cx).read_only && editor.read(cx).library_root == reader_root, "Reader lost its permission or asset root");
        let mut core = Core::new(editor.read(cx).core.doc.clone());
        core.insert_text("forbidden reader edit");
        ensure!(crate::cloud_sync::save(&mut ws.store, &rooms[1].scope.note, &core, cx).is_err(), "Read-only room queued an edit");
        Ok(())
    })??;
    ensure!(journal_count(&reader_root, cx)? == before_reader, "Reader queue changed");
    owner.update(cx, |ws, _, cx| -> Result<()> {
        let editor = ws.editor.clone().context("Owner editor not ready")?;
        ensure!(!editor.read(cx).read_only && editor.read(cx).library_root == room_root, "Owner lost its library context");
        editor.update(cx, |ed, _| ed.core.insert_text("owner session edit"));
        ws.unsaved = true;
        ensure!(ws.flush(cx), "Shared owner save failed");
        Ok(())
    })??;
    ensure!(journal_count(&room_root, cx)? > before_owner, "Owner edit missed its durable queue");
    primary.update(cx, |ws, _, _| {
        let id = ws.entries[0].id.clone();
        ensure!(ws.store.load(&id)? == original, "Shared save changed the personal note");
        Ok::<_, anyhow::Error>(())
    })??;
    let reader_doc = reader.update(cx, |ws, _, cx| ws.editor.as_ref().unwrap().read(cx).core.doc.clone())?;
    ensure!(!reader_doc.plain_text().contains("owner session edit"), "A different shared editor received the owner update");
    ensure!(fs::read(room_root.join("assets/files/shared-fixture.txt"))? != fs::read(reader_root.join("assets/files/shared-fixture.txt"))?, "Attachment fixtures must differ");
    // An outstanding reply from the closed session must not reach the reopened one.
    let generation = crate::cloud_sync::session(&room_root, cx).unwrap().generation;
    owner.update(cx, |_, window, _| window.remove_window())?;
    fs::write(root.join("qa-closed-session"), format!("{generation}\n{before_owner}\n"))?;
    Ok(())
}

fn verify_library_settings(cx: &mut gpui::App, primary: &gpui::WindowHandle<crate::app::Workspace>,
    owner: &gpui::WindowHandle<crate::app::Workspace>, personal: &Path, shared: &Path) -> Result<()> {
    use crate::{app::View, settings::Page};
    let (id, personal_limit) = primary.update(cx, |ws, window, cx| -> Result<_> {
        let id = ws.entries[0].id.clone();
        ensure!(ws.try_open(&id, window, cx), "Could not open the personal fixture");
        ws.open_settings(Page::Library, cx);
        ensure!(ws.settings_sync_root(cx).as_deref() == Some(personal), "Wrong scope in personal settings");
        let limit = ws.personal_history_limit(cx);
        ws.close_settings(cx);
        ensure!(ws.view == View::Doc && ws.current.as_deref() == Some(&id), "Personal settings lost their return document");
        Ok((id, limit))
    })??;
    owner.update(cx, |ws, window, cx| -> Result<()> {
        let note = ws.current.clone();
        let shared_limit = ws.store.settings().history_limit();
        let next = if personal_limit < 30 { personal_limit + 1 } else { personal_limit - 1 };
        ws.open_settings(Page::Library, cx);
        ensure!(ws.settings_sync_root(cx).as_deref() == Some(personal) && ws.personal_history_limit(cx) == personal_limit,
            "Global settings opened against the active share");
        ws.change_history_limit(next as i32 - personal_limit as i32, window, cx);
        ensure!(ws.personal_history_limit(cx) == next && ws.store.settings().history_limit() == shared_limit,
            "The global history change affected the shared store");
        ws.change_history_limit(personal_limit as i32 - next as i32, window, cx);
        ws.close_settings(cx);
        ensure!(ws.view == View::Doc && ws.current == note && ws.store.root() == shared, "Global settings displaced the shared document");
        ws.open_settings(Page::SharedSync, cx);
        ensure!(ws.settings_sync_root(cx).as_deref() == Some(shared), "Share sync used the personal library");
        ws.close_settings(cx);
        ensure!(ws.view == View::Doc && ws.current == note, "Share sync displaced the shared document");
        ws.open_global_settings(window, cx);
        ensure!(ws.settings_page == Page::Library && ws.settings_sync_root(cx).as_deref() == Some(personal),
            "The global Settings entry reopened the share sync page");
        ws.close_settings(cx);
        Ok(())
    })??;
    primary.update(cx, |ws, _, _| -> Result<()> {
        ensure!(ws.current.as_deref() == Some(&id) && ws.history_limit == personal_limit, "Personal window settings cache did not recover");
        Ok(())
    })??;
    eprintln!("LIBRARY_SETTINGS_SCOPE_OK personal=matched share=isolated return=preserved history=personal");
    Ok(())
}

pub(crate) fn reopen(cx: &mut gpui::App) -> Result<()> {
    use crate::app::Workspace;
    let personal = Store::root_dir();
    let root = crate::cloud_sync::test_root()?.context("Missing QA root")?;
    let rooms: Vec<Room> = serde_json::from_slice(&fs::read(root.join("qa-session-rooms.json"))?)?;
    let room_root = room_root(&personal, &rooms[0].scope.note)?;
    let old = fs::read_to_string(root.join("qa-closed-session"))?;
    let mut old = old.lines();
    let generation = uuid::Uuid::parse_str(old.next().context("Missing old generation")?)?;
    let before_owner: u32 = old.next().context("Missing queue count")?.parse()?;
    crate::cloud_sync::release_unused(cx);
    ensure!(crate::cloud_sync::session(&room_root, cx).is_none(), "Closing the room retained its sync session");
    { let _lock = crate::cloud_restore::lock_library(&room_root)?; }
    open_window(&rooms[0], cx)?;
    ensure!(!crate::cloud_sync::current(&room_root, generation, cx), "A reopened room accepted an old session generation");
    ensure!(journal_count(&room_root, cx)? > before_owner, "Reopening lost queued edits");
    ensure!(cx.windows().len() == 3, "Closing or reopening one room affected another library");
    ensure!(cx.windows().into_iter().filter_map(|w| w.downcast::<Workspace>())
        .any(|w| w.read(cx).is_ok_and(|ws| ws.store.root() == personal)), "Personal window was lost");
    Ok(())
}

pub(crate) fn request_account_check(_cx: &mut gpui::App) { crate::cloud_sync::sync_resumed(3); }

pub(crate) fn confirm_account_check(cx: &mut gpui::App) -> Result<()> {
    let root = Store::root_dir();
    let marker = root.join("require-account-verification");
    ensure!(marker.is_file(), "No isolated account verification was pending");
    super::super::complete_account_check(&root, super::super::account_revision())?;
    super::super::fixture_status(&root, super::super::SyncPhase::Ready, cx);
    Ok(())
}

pub(crate) fn verify_account_check(cx: &mut gpui::App) -> Result<()> {
    for window in cx.windows().into_iter().filter_map(|w| w.downcast::<crate::app::Workspace>()) {
        window.update(cx, |ws, _, cx| -> Result<()> {
            ensure!(crate::cloud_sync::status(ws.store.root(), cx).is_some_and(|s| s.phase == crate::cloud_sync::SyncPhase::AccountChecking),
                "Account change did not reach every library session");
            if ws.is_shared() {
                ensure!(ws.editor.as_ref().is_some_and(|e| e.read(cx).read_only), "Shared editor did not pause on account change");
            }
            Ok(())
        })??;
    }
    ensure!(Store::root_dir().join("require-account-verification").is_file(), "Account change did not protect the personal library");
    eprintln!("SHARED_SESSIONS_OK pid={} windows=3 duplicate=reused permission=isolated assets=isolated queue=retained lock=released generation=checked account=broadcast", std::process::id());
    Ok(())
}

fn journal_count(root: &Path, cx: &gpui::App) -> Result<u32> { Ok(crate::cloud_sync::journal(root, cx).counts()?.0) }

pub(crate) fn readonly_folder(root: &Path, personal: &Path) -> Result<(String, String, String)> {
    let source = root.join("folder-sidebar-sender");
    fs::create_dir(&source)?;
    Store::initialize_shared_sqlite(&source, &"c".repeat(64))?;
    let mut store = Store::open_sqlite_at(source.clone())?;
    let id = store.create_folder("收到的共享文件夹", None)?;
    let doc = store.create_from_in(&Document { title: crate::model::RichText::plain("只读资料"), ..Default::default() }, Some(&id))?;
    let empty = store.create_folder("空文件夹", Some(&id))?;
    drop(store);
    let mut room = original::prepare_local(&source, &id, "收到的共享文件夹".into(), false)?;
    let edits = Journal::open(&room_root(&source, &id)?, None)?.request()?.edits;
    room.scope.owner = Some("synthetic-folder-owner".into());
    let guest = stage(personal, &room, None)?;
    let mut reader = Journal::open(&guest, None)?;
    reader.receive(crate::cloud_sync::Changes { retention: None, uploaded: vec![], edits,
        token: Some(crate::cloud_sync::Bytes(vec![1])), more: false, legacy: false,
        room_access: Some(Access::ReadOnly), cursor_replayed: false, upload_quota_exceeded: false })?;
    reader.merge(&Default::default())?;
    Ok((id, doc, empty))
}
