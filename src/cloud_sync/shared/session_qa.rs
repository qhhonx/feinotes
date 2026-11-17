// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Offline native multi-library fixtures and assertions.

use super::*;

pub(crate) fn prepare(root: &Path) -> Result<PathBuf> {
    let personal = root.join("a".repeat(64));
    ensure!(!personal.exists(), "Shared-session QA requires a fresh synthetic library");
    let source = personal.join(".restore-source");
    let mut file = Store::open_at(source.clone())?;
    file.create_from(&Document { title: crate::model::RichText::plain("Personal fixture"),
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
            reader.receive(crate::cloud_sync::Changes { uploaded: vec![], edits,
                token: Some(crate::cloud_sync::Bytes(vec![1])), more: false, legacy: false,
                room_access: Some(Access::ReadOnly), cursor_replayed: false, upload_quota_exceeded: false })?;
            reader.merge(&Default::default())?;
        }
        rooms.push(room);
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
