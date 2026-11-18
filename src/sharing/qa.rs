//! Native controller assertions used only by the isolated, offline sharing harness.
use super::*;
use anyhow::{Result, Context, ensure};

pub(crate) fn step(name: &str, cx: &mut App) -> Result<()> {
    ensure!(std::env::var_os("QUIET_SYNC_QA_SHARING_UI").is_some() && crate::devtools::scripted(), "Use isolated sharing UI QA");
    if name.starts_with("parity-") { return super::qa_parity::step(name, cx); }
    let root = cloud_sync::test_root()?.context("Missing QA root")?;
    let personal = crate::store::Store::root_dir();
    let handles: Vec<_> = cx.windows().into_iter().filter_map(|h| h.downcast::<Workspace>()).collect();
    ensure!(handles.len() == 1, "Sharing navigation created a second window");
    let primary = handles[0];
    primary.update(cx, |_, window, _| crate::devtools::select_script_window(window))?;
    match name {
        "sidebar-entry-personal" | "sidebar-entry-shared" | "sidebar-entry-none" => primary.update(cx, |ws, _, cx| -> Result<()> {
            let [id, _, _]: [String; 3] = serde_json::from_slice(&std::fs::read(root.join("qa-sidebar-original-ids"))?)?;
            let personal = ws.qa_sidebar_pick(false);
            let shared_pick = ws.qa_sidebar_pick(true);
            eprintln!("SIDEBAR_ENTRY {name}: personal={personal:?} shared={shared_pick:?}");
            let (expect_personal, expect_shared) = match name {
                "sidebar-entry-personal" => (Some(id.as_str()), None),
                "sidebar-entry-shared" => (None, Some(id.as_str())),
                _ => (None, None),
            };
            ensure!((personal, shared_pick) == (expect_personal, expect_shared), "Wrong sidebar entry for the current document");
            if name != "sidebar-entry-none" {
                ensure!(shared::scope(&ws.store, cx).is_some_and(|scope| scope.note == id && scope.owner.is_none()) && ws.current.as_ref() == Some(&id), "Sidebar selection changed the owned document identity");
            }
            Ok(())
        })??,
        "sidebar-entry-fold-all" => primary.update(cx, |ws, _, cx| {
            ws.sharing.folded = true; ws.folded[1] = true; cx.notify();
        })?,
        "sidebar-entry-home" => primary.update(cx, |ws, _, _| -> Result<()> {
            ensure!(ws.view == View::Home && !ws.is_shared() && ws.qa_sidebar_pick(false).is_none() && ws.qa_sidebar_pick(true).is_none(), "Home retained a document selection");
            Ok(())
        })??,
        "sidebar-entry-open-without-row" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let [id, _, _]: [String; 3] = serde_json::from_slice(&std::fs::read(root.join("qa-sidebar-original-ids"))?)?;
            ws.sharing.folded = false; ws.folded[1] = false;
            ws.open(&id, window, cx);
            Ok(())
        })??,
        "sidebar-switch-lock" | "sidebar-switch-existing-db-lock" | "sidebar-switch-missing" | "sidebar-switch-deliver" => {
            let rooms: Vec<Room> = serde_json::from_slice(&std::fs::read(root.join("qa-session-rooms.json"))?)?;
            let id = &rooms[0].scope.note;
            let room = shared::room_root(&personal, id)?;
            let mut db = rusqlite::Connection::open(room.join(crate::store::SQLITE_FILE))?;
            if matches!(name, "sidebar-switch-lock" | "sidebar-switch-existing-db-lock") {
                if name == "sidebar-switch-lock" { db.execute_batch("DROP TABLE IF EXISTS cloud_inventory_state")?; }
                db.execute_batch("BEGIN IMMEDIATE")?;
                let marker = root.join("qa-sidebar-db-lock-released");
                if marker.exists() { std::fs::remove_file(&marker)?; }
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1600));
                    db.execute_batch("ROLLBACK").unwrap();
                    std::fs::write(marker, b"released").unwrap();
                });
            } else if name == "sidebar-switch-missing" {
                let tx = db.transaction()?;
                // Hold the content aside while preserving the share descriptor and sidebar title.
                tx.execute_batch("CREATE TABLE qa_delayed_entries AS SELECT * FROM library_entries;
                    CREATE TABLE qa_delayed_documents AS SELECT * FROM library_documents;
                    CREATE TABLE qa_delayed_blocks AS SELECT * FROM library_blocks;
                    CREATE TABLE qa_delayed_search_blocks AS SELECT * FROM library_search_blocks")?;
                crate::store::Store::remove_sync_body(&tx, id)?;
                tx.execute("DELETE FROM library_entries WHERE id = ?1", [id])?;
                tx.commit()?;
            } else {
                db.execute_batch("BEGIN;
                    INSERT INTO library_entries SELECT * FROM qa_delayed_entries;
                    INSERT INTO library_documents SELECT * FROM qa_delayed_documents;
                    INSERT INTO library_blocks SELECT * FROM qa_delayed_blocks;
                    INSERT INTO library_search_blocks SELECT * FROM qa_delayed_search_blocks;
                    COMMIT")?;
                crate::app::cloud_notes_changed(&room, &[], true, cx);
            }
        }
        "sidebar-switch-writer" | "sidebar-switch-responsive" | "sidebar-switch-a" | "sidebar-switch-b" | "sidebar-switch-waiting" => primary.update(cx, |ws, _, cx| -> Result<()> {
            let rooms: Vec<Room> = serde_json::from_slice(&std::fs::read(root.join("qa-session-rooms.json"))?)?;
            let a = name == "sidebar-switch-responsive" || name == "sidebar-switch-a";
            let scope = &rooms[usize::from(a)].scope;
            if matches!(name, "sidebar-switch-responsive" | "sidebar-switch-writer") {
                let released = root.join("qa-sidebar-db-lock-released").is_file();
                eprintln!("SIDEBAR_SWITCH responsive: lock_released={released}");
                ensure!(!released, "The UI event loop did not respond until the SQLite writer released its lock");
                if name == "sidebar-switch-writer" { return Ok(()); }
            }
            ensure!(shared::scope(&ws.store, cx) == Some(scope), "Wrong shared workspace after {name}");
            if name == "sidebar-switch-waiting" {
                ensure!(ws.view == View::Home && ws.current.is_none(), "Expected the waiting view while the body is missing");
            } else {
                ensure!(ws.view == View::Doc && ws.current.as_ref() == Some(&scope.note), "Wrong editor after {name}");
            }
            ensure!(ws.qa_sidebar_pick(true) == Some(scope.note.as_str()) && ws.qa_sidebar_pick(false).is_none(), "Wrong sidebar selection after {name}");
            eprintln!("SIDEBAR_SWITCH {name}: selected={:?}", ws.qa_sidebar_pick(true));
            Ok(())
        })??,
        "sidebar-original-setup" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let original = ws.entries.iter().find(|entry| entry.meta.share_root.as_ref() == Some(&entry.id)).context("Missing personal original")?.id.clone();
            let room = shared::original::owner_room(&ws.entries, &original).context("Missing owner room")?;
            shared::ensure_created(&personal, &room)?;
            let mut shared_store = shared::store_for(&room, cx)?;
            let mut doc = shared_store.load(&original)?;
            doc.blocks = vec![crate::model::Block::text("Shared body fixture")];
            shared_store.save(&original, &doc)?;
            let previous = ws.store.create_from(&crate::model::Document { title: crate::model::RichText::plain("Previous personal note"), ..Default::default() })?;
            let same_title = ws.store.create_from(&crate::model::Document { title: doc.title.clone(), ..Default::default() })?;
            for (i, id) in [&original, &previous, &same_title].into_iter().enumerate() {
                ws.store.update_meta(id, |meta| meta.root_order = Some(i as u64 + 1))?;
            }
            std::fs::write(root.join("qa-sidebar-original-ids"), serde_json::to_vec(&[original, previous.clone(), same_title])?)?;
            ws.entries = ws.store.list(); ws.sort();
            ws.sidebar_open = true; ws.folded[1] = false; ws.sharing.folded = false;
            ws.open(&previous, window, cx);
            Ok(())
        })??,
        "sidebar-original-opened" | "sidebar-original-held" | "sidebar-original-released" => primary.update(cx, |ws, _, cx| -> Result<()> {
            let [id, _, _]: [String; 3] = serde_json::from_slice(&std::fs::read(root.join("qa-sidebar-original-ids"))?)?;
            let editor = ws.editor.as_ref().context("Editor not open")?;
            let scope = shared::scope(&ws.store, cx);
            eprintln!("SIDEBAR_ORIGINAL {name}: room={:?}, title={:?}", scope.map(|scope| &scope.note), editor.read(cx).core.doc.title.text);
            ensure!(scope.is_some_and(|scope| scope.note == id && scope.owner.is_none()) && ws.current.as_ref() == Some(&id), "Personal row navigated away from the owner's shared document");
            ensure!(editor.read(cx).core.doc.plain_text().contains("Shared body fixture"), "Personal row opened the stale local body");
            ensure!(ws.qa_rendered_sidebar_rows(false).first() == Some(&(id.clone(), 0)) && ws.qa_rendered_sidebar_rows(true).iter().any(|(row, _)| row == &id), "Owner original disappeared from a sidebar");
            let entity = format!("{:?}", editor.entity_id());
            if name == "sidebar-original-opened" { std::fs::write(root.join("qa-sidebar-original-editor"), entity)?; }
            else { ensure!(entity == std::fs::read_to_string(root.join("qa-sidebar-original-editor"))?, "Clicking the displayed original replaced its editor"); }
            Ok(())
        })??,
        "sidebar-original-other-room" | "sidebar-original-newer-route" => primary.update(cx, |ws, _, cx| -> Result<()> {
            let rooms: Vec<Room> = serde_json::from_slice(&std::fs::read(root.join("qa-session-rooms.json"))?)?;
            ensure!(shared::scope(&ws.store, cx) == Some(&rooms[0].scope) && ws.current.as_ref() == Some(&rooms[0].scope.note), "The other shared document did not stay open");
            Ok(())
        })??,
        "sidebar-original-deferred" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let [original, _, _]: [String; 3] = serde_json::from_slice(&std::fs::read(root.join("qa-sidebar-original-ids"))?)?;
            let rooms: Vec<Room> = serde_json::from_slice(&std::fs::read(root.join("qa-session-rooms.json"))?)?;
            ensure!(!ws.is_shared(), "Expected the personal workspace");
            ws.open(&original, window, cx);
            ensure!(ws.sharing.pending_open.as_ref() == Some(&original), "Expected a deferred owner-original open");
            ws.navigate_shared(rooms[0].clone(), None, window, cx);
            Ok(())
        })??,
        "sidebar-original-personal-reorder" | "sidebar-original-same-title" => primary.update(cx, |ws, _, cx| -> Result<()> {
            let [original, previous, same_title]: [String; 3] = serde_json::from_slice(&std::fs::read(root.join("qa-sidebar-original-ids"))?)?;
            ensure!(!ws.is_shared(), "Expected the personal workspace");
            let order = ws.qa_rendered_sidebar_rows(false).into_iter().map(|(id, _)| id).collect::<Vec<_>>();
            ensure!(order == [previous, same_title.clone(), original.clone()], "Personal reorder was not applied to the personal sidebar: {order:?}");
            let saved = crate::tree::Tree::new(&ws.store.list()).children(None).iter().map(|entry| entry.id.clone()).collect::<Vec<_>>();
            ensure!(saved == order, "Personal reorder was not persisted");
            let rooms: Vec<Room> = serde_json::from_slice(&std::fs::read(root.join("qa-session-rooms.json"))?)?;
            let shared_order = ws.qa_rendered_sidebar_rows(true).into_iter().map(|(id, _)| id).collect::<Vec<_>>();
            ensure!(shared_order == [rooms[0].scope.note.clone(), rooms[1].scope.note.clone(), original], "Personal reorder changed the shared sidebar");
            if name == "sidebar-original-same-title" {
                ensure!(ws.current.as_ref() == Some(&same_title), "Same-title personal item opened the wrong document");
                eprintln!("SIDEBAR_ORIGINAL_OK");
            }
            let _ = cx;
            Ok(())
        })??,
        "unread-menu-open" | "unread-menu-closed" => primary.update(cx, |ws, _, _| -> Result<()> {
            let entry = ws.entries.iter().find(|entry| entry.title == "Personal fixture").context("Missing original document")?;
            let anchored = ws.row_menu.as_ref().is_some_and(|menu| menu.id == format!("shared:{}", entry.id) && menu.pos.x < px(0.));
            ensure!(anchored == (name == "unread-menu-open"), "Shared row menu did not follow the ellipsis action");
            Ok(())
        })??,
        "unread-collapsed" | "unread-expanded" | "unread-cleared" => primary.update(cx, |ws, _, cx| -> Result<()> {
            ensure!(!ws.is_shared(), "Unread fixture must also be visible in My Documents");
            let entry = ws.entries.iter().find(|entry| entry.title == "Personal fixture").context("Missing original document")?;
            ensure!(entry.meta.share_root.as_ref() == Some(&entry.id), "Expected the same original in both sections");
            let unread = name != "unread-cleared";
            let room = cx.global_mut::<cloud_sync::activity::ActivityView>().0.iter_mut().find(|row| row.room.scope.note == entry.id).context("Missing shared activity row")?;
            if unread { ensure!(room.unread, "Expected unread collaborator activity"); }
            else {
                let db = rusqlite::Connection::open(shared::room_root(&personal, &entry.id)?.join(crate::store::SQLITE_FILE))?;
                ensure!(db.execute("UPDATE shared_activity SET seen = 1 WHERE id = 'qa-unread'", [])? == 1, "Missing unread fixture event");
                room.unread = false;
            }
            ws.folded[1] = false;
            ws.sharing.folded = name == "unread-collapsed";
            ws.show_view(View::Home, cx);
            Ok(())
        })??,
        "check-toolbar-panel" => primary.update(cx, |ws, window, cx| -> Result<()> {
            ensure!(ws.sharing.panel_open && ws.render_sharing_toolbar_panel(window, cx).is_some(), "The participants control did not open the panel");
            Ok(())
        })??,
        "personal-feishu" | "personal-notes" | "shared-feishu" | "shared-notes" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let shared = name.starts_with("shared");
            let feishu = name.ends_with("feishu");
            ensure!(ws.is_shared() == shared && ws.view == View::Doc && ws.sharing_toolbar(window, cx).is_some(), "Missing document toolbar fixture");
            ensure!(!shared || ws.online_collaborators(cx).is_empty(), "Expected the participants fallback without anyone else online");
            ws.set_pref(|p| { p.appearance = "light".into(); p.feishu = feishu; p.theme = if feishu { "feishu" } else { "notes" }.into(); }, cx);
            ws.reload_themes(cx);
            ensure!(settings::feishu() == feishu, "Sharing toolbar theme did not change");
            Ok(())
        })??,
        "check-shared-add" => primary.update(cx, |w, _, _| -> Result<()> {
            ensure!(matches!(w.create_menu, Some((crate::folders::CreateTarget::Shared, _))), "Sidebar plus did not open the shared menu"); Ok(())
        })??,
        "check-sidebar-invite" => primary.update(cx, |w, _, _| -> Result<()> {
            ensure!(w.accept_invitation_open() && w.sharing.accept_at == Some("accept-sidebar"), "Sidebar menu did not open the invitation form"); Ok(())
        })??,
        "collections" => primary.update(cx, |ws, window, cx| -> Result<()> {
            use crate::folders::CreateTarget;
            let parent = "collection-existing".to_string();
            let child = "collection-existing-nested".to_string();
            let room = shared::rooms(&personal)?.into_iter().next().context("Missing room")?;
            // Reproduce the persisted organizer from an earlier build.
            ensure!(ws.update_organizer(cx, |tree| {
                tree.0.insert(parent.clone(), organizer::Placement { title: Some("客户资料".into()), parent: None });
                tree.0.insert(child.clone(), organizer::Placement { title: Some("十一月".into()), parent: Some(parent.clone()) });
                tree.place(&room.scope.note, Some(&child))
            }), "Could not seed existing personal folders");
            ws.show_view(View::Shared, cx);
            let count = ws.store.list().len();
            let before = organizer::Organizer::load()?;
            for target in [CreateTarget::Shared, CreateTarget::Library(None), CreateTarget::Library(Some(parent.clone()))] {
                ws.new_folder(target, Some("Unexpected folder".into()), window, cx);
            }
            ws.rename_collection(room.scope.note.clone(), window, cx);
            ensure!(ws.sharing.sheet.is_none(), "A shared document was treated as an existing personal folder");
            ensure!(ws.store.list().len() == count && organizer::Organizer::load()? == before, "Shared overview wrote a new folder");
            ws.sharing.collection = Some(child.clone());
            ws.rename_collection(child.clone(), window, cx);
            ws.sharing.folder_name.update(cx, |field, cx| field.set_text("十一月归档", cx));
            ws.submit_sharing(window, cx);
            let tree = organizer::Organizer::load()?;
            ensure!(tree.0.len() == before.0.len() && tree.0[&child].title.as_deref() == Some("十一月归档") && tree.0[&child].parent.as_ref() == Some(&parent)
                && tree.0[&room.scope.note].parent.as_ref() == Some(&child) && ws.sharing.collection.as_ref() == Some(&child), "Existing folder or placements changed during rename");
            ws.new_folder(CreateTarget::PersonalSidebar(None), Some("共同项目".into()), window, cx);
            let folder = ws.folder_current.clone().context("Missing new My Documents folder")?;
            ws.new_folder(CreateTarget::Library(Some(folder.clone())), Some("会议".into()), window, cx);
            let nested = ws.folder_current.clone().context("Missing new My Documents subfolder")?;
            ensure!(ws.entries.iter().any(|entry| entry.id == nested && entry.title == "会议" && entry.meta.parent.as_ref() == Some(&folder)), "My Documents folder creation failed");
            let doc = ws.store.create_from(&crate::model::Document { title: crate::model::RichText::plain("议程"), ..Default::default() })?;
            ws.store.set_parent(&doc, Some(&nested))?; ws.entries = ws.store.list();
            ws.show_view(View::Shared, cx);
            ws.expanded.insert(parent.clone()); ws.expanded.insert(child); ws.sharing.collection = Some(parent); ws.sharing.folded = false; cx.notify();
            std::fs::write(root.join("qa-team-folder"), &folder)?;
            eprintln!("SHARED_PERSONAL_FOLDER_REMOVAL_OK");
            Ok(())
        })??,
        "folder-invite" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let id = std::fs::read_to_string(root.join("qa-team-folder"))?;
            ws.create_shared_note(&id, window, cx); ws.sharing.email.update(cx, |f,cx| f.set_text("person@example.test",cx));
            ws.submit_sharing(window, cx); Ok(())
        })??,
        "folder-open" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let id = std::fs::read_to_string(root.join("qa-team-folder"))?;
            ensure!(ws.is_shared() && ws.folder_current.as_ref() == Some(&id) && matches!(ws.sharing.sheet, Some(Sheet::Members(_))), "Folder invitation did not open the shared root");
            ws.close_sharing(window, cx);
            let child = ws.entries.iter().find(|e| e.meta.parent.as_ref() == Some(&id)).context("Missing nested shared folder")?.id.clone();
            ws.open(&child, window, cx);
            let count = ws.store.list().len();
            ws.new_folder(crate::folders::CreateTarget::Library(None), Some("Invalid shared root folder".into()), window, cx);
            ws.new_folder(crate::folders::CreateTarget::PersonalSidebar(None), Some("Invalid personal folder".into()), window, cx);
            ensure!(ws.store.list().len() == count && ws.folder_current.as_ref() == Some(&child), "A shared session accepted creation without a valid shared parent");
            ws.create_doc(false, Some(child.clone()), window, cx);
            ensure!(ws.current.as_ref().is_some_and(|id| ws.entries.iter().any(|e| &e.id == id && e.meta.parent.as_ref() == Some(&child))), "New document missed its shared parent");
            ws.open(&child, window, cx);
            eprintln!("SHARED_FOLDERS_UI_OK"); Ok(())
        })??,
        "sidebar-setup" | "sidebar-scope-setup" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let original = ws.entries.iter().find(|entry| entry.title == "Personal fixture").context("Missing personal fixture")?.id.clone();
            std::fs::write(root.join("qa-personal-original"), &original)?;
            for title in ["排序验证甲", "排序验证乙"] { ws.store.create_from(&crate::model::Document { title: crate::model::RichText::plain(title), icon: Some("😀".into()), ..Default::default() })?; }
            let folder = ws.store.create_folder("展开的文件夹", None)?;
            let child = ws.store.create_from(&crate::model::Document::default())?;
            ws.store.set_parent(&child, Some(&folder))?; ws.expanded.insert(folder);
            let mut entries = ws.store.list(); entries.sort_by(|a, b| b.id.cmp(&a.id));
            for (i, entry) in entries.iter().enumerate() { ws.store.update_meta(&entry.id, |meta| meta.root_order = Some(i as u64 + 1))?; }
            ws.entries = ws.store.list(); ws.sort();
            let rows = crate::tree::Tree::new(&ws.entries).rows(|id| ws.expanded.contains(id)).into_iter().map(|(entry, depth)| (entry.id.clone(), depth)).collect::<Vec<_>>();
            std::fs::write(root.join("qa-personal-sidebar.json"), serde_json::to_vec(&rows)?)?;
            if name == "sidebar-scope-setup" {
                ws.sidebar_open = true; ws.folded[1] = false; ws.sharing.folded = false;
                ws.show_view(View::Home, cx); return Ok(());
            }
            let rooms: Vec<Room> = serde_json::from_slice(&std::fs::read(root.join("qa-session-rooms.json"))?)?;
            ws.navigate_shared(rooms[0].clone(), None, window, cx); Ok(())
        })??,
        "sidebar-scope-before" | "sidebar-scope-opened" | "sidebar-scope-switched" | "sidebar-scope-returned" | "sidebar-scope-typed" | "sidebar-scope-created" | "sidebar-scope-personal-reordered" => primary.update(cx, |ws, _, cx| -> Result<()> {
            let session = shared::scope(&ws.store, cx).map(|scope| scope.note.clone());
            match name {
                "sidebar-scope-opened" => { std::fs::write(root.join("qa-clicked-share"), session.as_deref().context("Shared sidebar click did not open a session")?)?; }
                "sidebar-scope-switched" => {
                    ensure!(session.context("Second shared sidebar click left the session")? != std::fs::read_to_string(root.join("qa-clicked-share"))?, "Second shared sidebar click did not switch sessions");
                }
                _ => ensure!(session.is_none(), "Expected the personal workspace"),
            }
            let mut expected: Vec<(String, usize)> = serde_json::from_slice(&std::fs::read(root.join("qa-personal-sidebar.json"))?)?;
            if matches!(name, "sidebar-scope-typed" | "sidebar-scope-created" | "sidebar-scope-personal-reordered") {
                let created = ws.entries.iter().find(|entry| entry.title == "New sidebar fixture").context("New personal draft was not saved")?;
                if name == "sidebar-scope-typed" {
                    ensure!(ws.view == View::Doc && ws.current.as_ref() == Some(&created.id), "New personal document was not open in the editor");
                } else {
                    ensure!(ws.view == View::Home && ws.home.tab == crate::home::HomeTab::Recent && ws.home_entries(cx).iter().any(|entry| entry.id == created.id), "New personal document did not appear in Home");
                }
                if name == "sidebar-scope-personal-reordered" {
                    expected.push((created.id.clone(), 0));
                    let saved = crate::tree::Tree::new(&ws.store.list()).rows(|id| ws.expanded.contains(id)).into_iter().map(|(entry, depth)| (entry.id.clone(), depth)).collect::<Vec<_>>();
                    ensure!(saved == expected, "Personal reorder was not persisted");
                } else { expected.insert(0, (created.id.clone(), 0)); }
            }
            let personal = ws.qa_rendered_sidebar_rows(false);
            let shared = ws.qa_rendered_sidebar_rows(true);
            let expected_shared = crate::tree::Tree::new(&ws.organized_shared_entries(cx)).rows(|id| ws.expanded.contains(id)).into_iter().map(|(entry, depth)| (entry.id.clone(), depth)).collect::<Vec<_>>();
            eprintln!("SIDEBAR_SCOPE {name}: personal={personal:?} shared={shared:?}");
            ensure!(personal == expected, "My Documents rendered unexpected rows: expected={expected:?}, actual={personal:?}, shared={shared:?}");
            ensure!(shared == expected_shared, "Shared Documents rendered unexpected rows");
            if name == "sidebar-scope-personal-reordered" { eprintln!("SIDEBAR_PERSONAL_REORDER_OK"); }
            Ok(())
        })??,
        "sidebar-shared" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let expected: Vec<(String, usize)> = serde_json::from_slice(&std::fs::read(root.join("qa-personal-sidebar.json"))?)?;
            ensure!(ws.is_shared(), "Sidebar test did not enter a shared document");
            let actual = ws.personal_sidebar_rows(cx).into_iter().map(|(entry, depth)| (entry.id, depth)).collect::<Vec<_>>();
            ensure!(actual == expected, "Opening a shared document reordered the personal sidebar or lost expanded children");
            ws.navigate_personal(View::Doc, Some(std::fs::read_to_string(root.join("qa-personal-original"))?), window, cx); Ok(())
        })??,
        "sidebar-returned" => primary.update(cx, |ws, _, _| -> Result<()> {
            let expected: Vec<(String, usize)> = serde_json::from_slice(&std::fs::read(root.join("qa-personal-sidebar.json"))?)?;
            let actual = crate::tree::Tree::new(&ws.entries).rows(|id| ws.expanded.contains(id)).into_iter().map(|(entry, depth)| (entry.id.clone(), depth)).collect::<Vec<_>>();
            ensure!(!ws.is_shared() && actual == expected, "Returning to personal documents reordered the sidebar"); Ok(())
        })??,
        "shared-reorder" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let before = ws.store.list();
            ws.qa_shared_reorder(window, cx)?;
            std::fs::write(root.join("qa-reordered-sidebar"), serde_json::to_vec(&ws.qa_shared_root_order(cx))?)?;
            let after = ws.store.list();
            ensure!(serde_json::to_vec(&before.iter().map(|e| (&e.id, &e.meta)).collect::<Vec<_>>())? == serde_json::to_vec(&after.iter().map(|e| (&e.id, &e.meta)).collect::<Vec<_>>())?, "Shared sorting modified personal metadata");
            Ok(())
        })??,
        "shared-gesture" => primary.update(cx, |ws, _, cx| -> Result<()> {
            let mut expected: Vec<String> = serde_json::from_slice(&std::fs::read(root.join("qa-reordered-sidebar"))?)?; expected.reverse();
            let actual = ws.qa_shared_root_order(cx);
            ensure!(actual == expected && ws.sharing.sidebar.active.is_none(), "Shared pointer gesture did not reorder or release the row: actual={actual:?}, expected={expected:?}, active={}", ws.sharing.sidebar.active.is_some()); Ok(())
        })??,
        "personal-delete" => primary.update(cx, |ws, window, cx| -> Result<()> {
            ensure!(!ws.is_shared(), "Use personal context for deletion regression");
            let folder = ws.store.create_folder("保持收起", None)?;
            let child = ws.store.create_from(&crate::model::Document::default())?;
            ws.store.set_parent(&child, Some(&folder))?;
            let disposable = ws.store.create_from(&crate::model::Document::default())?;
            ws.store.update_meta(&folder, |meta| meta.root_order = Some(0))?;
            ws.entries = ws.store.list(); ws.sort(); ws.open(&disposable, window, cx);
            ws.expanded.remove(&folder);
            std::fs::write(root.join("qa-folded-folder"), &folder)?;
            ws.delete_docs(&[disposable], window, cx); Ok(())
        })??,
        "personal-deleted" => primary.update(cx, |ws, _, _| -> Result<()> {
            let folder = std::fs::read_to_string(root.join("qa-folded-folder"))?;
            ensure!(!ws.expanded.contains(&folder), "Deleting a personal document expanded a collapsed folder");
            ensure!(ws.view != View::Doc || ws.current.is_some(), "Deletion left an empty document view");
            eprintln!("SHARING_INTERACTIONS_OK pointer=reordered order=persisted personal=unchanged cancel=preserved folders=collapsed"); Ok(())
        })??,
        "original-review" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let id = ws.sharing.original.clone().context("Missing original")?; ws.close_sharing(window, cx); ws.navigate_personal(View::Doc, Some(id), window, cx); Ok(())
        })??,
        "original-cancel" => primary.update(cx, |ws, window, cx| -> Result<()> {
            ensure!(ws.is_shared() && ws.confirm.is_none(), "Original cleanup did not use personal deletion confirmation");
            ws.confirm = None;
            let room: Room = serde_json::from_slice(&std::fs::read(root.join("qa-created-room.json"))?)?;
            ws.open_shared_note(&room, window, cx); Ok(())
        })??,
        "rename-shared" => primary.update(cx, |ws, _, cx| -> Result<()> {
            let editor = ws.editor.clone().context("Missing shared editor")?;
            ensure!(ws.is_shared(), "Rename regression must use a shared document");
            editor.update(cx, |editor, cx| editor.set_title("即时更新的共享标题", cx)); Ok(())
        })??,
        "renamed-shared" => primary.update(cx, |ws, _, cx| -> Result<()> {
            ensure!(ws.unsaved, "Title regression must run before the save timer");
            let row = cx.global::<cloud_sync::activity::ActivityView>().0.iter().find(|row| Some(&row.room.scope.note) == ws.current.as_ref()).context("Missing live shared row")?;
            ensure!(row.modified == ws.entries.iter().find(|entry| Some(&entry.id) == ws.current.as_ref()).unwrap().meta.modified, "Sidebar timestamp lagged behind editing");
            ensure!(row.room.title == "即时更新的共享标题", "Shared sidebar/list title waited for background polling"); Ok(())
        })??,
        "list" => primary.update(cx, |ws, _, cx| -> Result<()> { ensure!(ws.sharing.folded, "Shared sidebar must start collapsed"); ws.set_pref(|p| { p.language="zh".into(); p.english=false; }, cx); ws.show_view(View::Shared, cx); Ok(()) })??,
        "accept-home" => primary.update(cx, |ws, window, cx| -> Result<()> { ws.open_accept_invitation_from("accept-home", window, cx); ensure!(ws.accept_invitation_open(), "Home invitation trigger did not open"); Ok(()) })??,
        "accept-sidebar-toggle" => primary.update(cx, |ws, window, cx| -> Result<()> { ensure!(ws.accept_invitation_open(), "Invitation triggers lost shared state"); ws.open_accept_invitation_from("accept-sidebar", window, cx); ensure!(!ws.accept_invitation_open(), "Sidebar trigger did not close the same invitation"); Ok(()) })??,
        "public-link-prepare" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let id = ws.current.clone().context("Missing personal fixture")?;
            ws.open(&id, window, cx);
            ws.create_shared_note(&id, window, cx);
            ensure!(ws.can_choose_link(&id) && !ws.sharing.public_link, "Account invitation must be the default");
            ws.set_public_link(true, window, cx);
            ensure!(ws.sharing.public_link && ws.sharing.email.read(cx).text.is_empty(), "Public link should not require a recipient");
            std::fs::write(root.join("qa-public-note"), &id)?;
            std::fs::write(root.join("fail-publish-once"), b"fault")?;
            std::fs::write(root.join("fail-link-write-once"), b"fault")?;
            ws.submit_sharing(window, cx);
            Ok(())
        })??,
        "public-link-retry-seed" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let id = std::fs::read_to_string(root.join("qa-public-note"))?;
            ensure!(!ws.sharing.busy && ws.sharing.copied_text.is_none() && ws.sharing.message.as_ref().is_some_and(|m| m.0), "Failed request copied a link or lost its error");
            let room = ws.sharing.prepared.clone().context("Failed public link lost its prepared room")?;
            ensure!(room.scope.note == id && shared::public_link_pending(&personal, &room), "Public intent or original document ID changed");
            ws.close_sharing(window, cx);
            ws.entries = ws.store.list();
            ws.create_shared_note(&id, window, cx);
            ensure!(ws.sharing.public_link && !ws.can_choose_link(&id), "Reopening lost the prepared link scope");
            ws.submit_sharing(window, cx);
            Ok(())
        })??,
        "public-link-rejected" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let id = std::fs::read_to_string(root.join("qa-public-note"))?;
            let room = ws.sheet_room().context("Rejection did not open access management")?.clone();
            let reply = ws.sharing.reply.as_ref().context("Rejection did not read back the actual access mode")?;
            ensure!(matches!(ws.sharing.sheet, Some(Sheet::Members(_))) && reply.audience == Audience::LinkDraft && reply.url.is_none()
                && reply.members.is_empty() && ws.sharing.copied_text.is_none(), "A rejected write changed sharing or copied a link");
            ensure!(ws.sharing.message.as_ref().is_some_and(|(error, message)| *error && (message.contains("regional restriction") || message.contains("地区限制"))
                && !message.contains("Cannot perform")), "The rejection did not have a localized explanation");
            ensure!(room.scope.note == id && shared::public_link_pending(&personal, &room)
                && !shared::original::awaiting_publication(&personal, &room), "The pending access change still blocks the uploaded document");
            ensure!(shared::rooms(&personal)?.iter().filter(|r| r.scope.note == id).count() == 1, "Retry created another shared entry");
            ws.close_sharing(window, cx);
            ws.show_view(View::Shared, cx);
            ws.open_shared_note(&room, window, cx);
            Ok(())
        })??,
        "public-link-opened" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let id = std::fs::read_to_string(root.join("qa-public-note"))?;
            let room = ws.current_room(cx).context("Shared list reopened creation instead of the document")?;
            ensure!(ws.view == View::Doc && ws.current.as_deref() == Some(id.as_str()) && ws.sharing.sheet.is_none(),
                "Opening the rejected share did not show the original document");
            ws.open_member_management(room, window, cx);
            Ok(())
        })??,
        "public-link-retry-reply" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let room = ws.sheet_room().context("Missing access management")?;
            ensure!(ws.can_retry_public_link(room, cx) && ws.sharing.reply.as_ref().is_some_and(|reply| reply.audience.can_invite()),
                "Missing explicit retry or account invitation");
            std::fs::write(root.join("fail-link-reply-once"), b"fault")?;
            ws.retry_public_link(window, cx);
            Ok(())
        })??,
        "public-link-published" => primary.update(cx, |ws, _, cx| -> Result<()> {
            let id = std::fs::read_to_string(root.join("qa-public-note"))?;
            let room = ws.current_room(cx).context("Public share did not stay open")?;
            ensure!(ws.current.as_deref() == Some(id.as_str()) && room.scope.note == id && !shared::public_link_pending(&personal, &room), "Public share did not retain the original document");
            ensure!(ws.sharing.copied_text.as_deref() == Some("https://feinotes.app/share/?g=synthetic-fei-ui")
                && ws.sharing.message.is_none(), "Readback did not recover the lost acknowledgement");
            Ok(())
        })??,
        "public-link-close" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let reply = ws.sharing.reply.as_ref().context("Missing public share state")?;
            ensure!(reply.audience == Audience::LinkReadOnly && reply.members.is_empty() && reply.url.is_some(), "Link must be read-only");
            ws.confirm_close_link(window, cx);
            ensure!(!ws.sharing.busy && ws.sharing.close_link_confirm, "Closing must ask for confirmation");
            ws.confirm_close_link(window, cx);
            Ok(())
        })??,
        "public-link-closed" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let room = ws.current_room(cx).context("Closure removed the owner's document")?;
            let reply = ws.sharing.reply.as_ref().context("Missing reloaded share state")?;
            ensure!(reply.audience == Audience::LinkClosed && reply.url.is_none() && !ws.can_close_link(&room, cx), "Closed link stayed available");
            ensure!(shared::manage(&personal, &room, "publishLink", "", None)?.audience == Audience::LinkClosed, "Retry reopened the closed link");
            ensure!(shared::manage(&personal, &room, "invite", "person@example.test", Some(Access::ReadOnly)).is_err(), "Closed link unexpectedly changed to account invitations");
            ws.sharing.copied_text = None;
            ws.copy_shared_link(room, window, cx);
            Ok(())
        })??,
        "public-link-copy-denied" => primary.update(cx, |ws, _, _| -> Result<()> {
            ensure!(!ws.sharing.busy && ws.sharing.copied_text.is_none(), "Copy returned an inactive link");
            Ok(())
        })??,
        "public-link-explicit-invite" => primary.update(cx, |_, _, _| -> Result<()> {
            let doc = crate::docxml::parse("<title>Private fallback</title><p>Synthetic document</p>")?;
            let id = crate::store::Store::open_sqlite_at(personal.clone())?.create_from(&doc)?;
            let room = shared::original::prepare(&personal, &id, doc.title.text.clone(), true)?;
            shared::ensure_created(&personal, &room)?;
            std::fs::write(root.join("fail-link-write-once"), b"fault")?;
            let failed = shared::publish_link(&personal, &room);
            ensure!(failed.result.is_err() && failed.reply.is_some_and(|reply| reply.members.is_empty() && reply.audience == Audience::LinkDraft),
                "Failed public sharing granted automatic access");
            let invite = shared::manage(&personal, &room, "invite", "owner-approved@example.test", Some(Access::ReadOnly))?;
            ensure!(invite.audience == Audience::Invited && invite.members.len() == 1 && !shared::public_link_pending(&personal, &room),
                "Explicit account invitation did not keep the same document private");
            ensure!(shared::manage(&personal, &room, "publishLink", "", None).is_err()
                && shared::manage(&personal, &room, "members", "", None)?.members.len() == 1,
                "A public-link retry changed an existing private invitation");
            eprintln!("PUBLIC_LINK_UI_OK default=invited rejection=private open=original lost-reply=confirmed closure=owner-retained copy=denied invite=explicit");
            Ok(())
        })??,
        "create" => primary.update(cx, |ws, window, cx| -> Result<()> {
            ensure!(ws.view == View::Home && ws.home.tab == crate::home::HomeTab::Shared, "Shared documents did not use the Home tab");
            let id = ws.current.clone().context("Missing personal fixture")?;
            ws.open(&id, window, cx);
            ws.create_shared_note(&id, window, cx);
            ensure!(matches!(ws.sharing.sheet, Some(Sheet::Create(_))) && ws.view == View::Doc, "Creation left the note");
            std::fs::write(root.join("fail-publish-once"), b"fault")?;
            std::fs::write(root.join("fail-invite-once"), b"fault")?;
            Ok(())
        })??,
        "retry-publish" => primary.update(cx, |ws, window, cx| -> Result<()> {
            ensure!(!ws.sharing.busy && ws.sharing.message.as_ref().is_some_and(|m| m.0), "Publish failure was not visible");
            let room = ws.sharing.prepared.clone().context("Failed publication lost its copy")?;
            std::fs::write(root.join("qa-created-room.json"), serde_json::to_vec(&room)?)?;
            ensure!(shared::rooms(&personal)?.len() == 3, "Initial create made duplicate copies");
            ws.submit_sharing(window, cx);
            Ok(())
        })??,
        "failed-invite" | "retry-invite" | "narrow" | "wide" | "reopen-members" | "members" | "readonly" | "revoke" | "remove-confirm" | "revoked" | "collaboration" => {
            let room: Room = serde_json::from_slice(&std::fs::read(root.join("qa-created-room.json"))?)?;
            let shared = handles.into_iter().find(|h| h.read(cx).is_ok_and(|ws| shared::scope(&ws.store, cx) == Some(&room.scope))).context("Missing newly created shared window")?;
            shared.update(cx, |ws, window, cx| -> Result<()> {
                crate::devtools::select_script_window(window);
                ensure!(!ws.sharing.busy, "Request did not complete");
                match name {
                    "failed-invite" | "retry-invite" => {
                        ensure!(matches!(ws.sharing.sheet, Some(Sheet::Invite(_))) && ws.sharing.message.as_ref().is_some_and(|m| m.0), "Invitation failure lost its retry form");
                        ensure!(ws.sharing.email.read(cx).text == "person@example.test", "Invitation failure lost its recipient");
                        ensure!(shared::rooms(&personal)?.len() == 3, "Invite failure created another note");
                        if name == "retry-invite" { ws.submit_sharing(window, cx); }
                    }
                    "narrow" => {
                        ws.set_pref(|p| { p.appearance="dark".into(); }, cx);
                        window.resize(size(px(480.), px(600.)));
                    }
                    "wide" => {
                        ws.set_pref(|p| { p.appearance="light".into(); }, cx);
                        window.resize(size(px(1050.), px(760.)));
                    }
                    "reopen-members" => {
                        ensure!(ws.sharing.sheet.is_none(), "Escape did not close management after invitation success");
                        ws.open_member_management(room.clone(), window, cx);
                    }
                    "members" => {
                        ensure!(ws.sharing.copied_text.as_ref().is_some_and(|text| text.contains("https://feinotes.app/share/?g=") && text.lines().count() == 2 && !text.contains("Apple")), "Creation did not copy an invitation message");
                        ensure!(ws.sharing.message.as_ref().is_none_or(|(error, _)| *error), "Success text still occupies sheet feedback");
                        ensure!(ws.sharing.original.is_some(), "Creation lost original cleanup option");
                        ensure!(matches!(ws.sharing.sheet, Some(Sheet::Members(_))), "Invitation did not open contextual management");
                        let member = &ws.sharing.reply.as_ref().context("Missing members")?.members[0];
                        ensure!(member.permission == Access::ReadOnly && !member.accepted, "Default invitation access or pending state is wrong");
                        ws.sharing.member = Some(member.id.clone());
                        ws.manage_shared_note("permission", member.id.clone(), Some(Access::ReadWrite), window, cx);
                    }
                    "readonly" => {
                        let member = &ws.sharing.reply.as_ref().context("Missing members")?.members[0];
                        ensure!(member.permission == Access::ReadWrite, "Permission change was not reflected");
                        ws.manage_shared_note("permission", member.id.clone(), Some(Access::ReadOnly), window, cx);
                    }
                    "revoke" => {
                        let member = &ws.sharing.reply.as_ref().context("Missing members")?.members[0];
                        ensure!(member.permission == Access::ReadOnly, "Read-only permission change was not reflected");
                        let id = member.id.clone();
                        ws.sharing.member = Some(id.clone());
                        ws.confirm_shared_removal(id, window, cx);
                        ensure!(ws.sharing.revoke_confirm && !ws.sharing.busy, "First removal click did not request confirmation");
                    }
                    "remove-confirm" => {
                        ensure!(ws.sharing.revoke_confirm, "Removal confirmation was lost");
                        let id = ws.sharing.reply.as_ref().context("Missing members")?.members[0].id.clone();
                        ws.confirm_shared_removal(id, window, cx);
                    }
                    "revoked" => {
                        ensure!(ws.sharing.reply.as_ref().context("Missing members")?.members.is_empty(), "Revocation was not reflected");
                        ws.close_sharing(window, cx);
                        ws.sharing.panel_open = true;
                    }
                    "collaboration" => {
                        ensure!(ws.render_sharing_toolbar_panel(window, cx).is_some(), "Contextual collaboration panel is missing");
                        ws.sharing.panel_open = false;
                    }
                    _ => unreachable!(),
                }
                Ok(())
            })??;
        }
        "return-list" => primary.update(cx, |ws, window, cx| ws.navigate_personal(View::Shared, None, window, cx))?,
        "fold" => primary.update(cx, |ws, _, cx| { ws.sharing.folded = !ws.sharing.folded; cx.notify(); })?,
        "leave" | "delete-document" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let room = ws.current_room(cx).context("Missing shared context")?;
            if name == "delete-document" { std::fs::write(root.join("fail-end-once"), b"fault")?; }
            ws.sharing.panel_open = false;
            ws.open_end_shared(room, window, cx);
            Ok(())
        })??,
        "confirm-end" => primary.update(cx, |ws, window, cx| ws.submit_sharing(window, cx))?,
        "retry-end" => primary.update(cx, |ws, window, cx| -> Result<()> {
            ensure!(matches!(ws.sharing.sheet, Some(Sheet::End(_))) && ws.sharing.message.as_ref().is_some_and(|message| message.0), "Removal failure lost its confirmation");
            ensure!(shared::rooms(&personal)?.len() == 2, "Failed deletion hid a live share");
            ws.submit_sharing(window, cx); Ok(())
        })??,
        "left" => primary.update(cx, |ws, window, cx| -> Result<()> {
            ensure!(!ws.is_shared() && ws.home.tab == crate::home::HomeTab::Shared, "Leaving did not return to the shared overview");
            ensure!(shared::rooms(&personal)?.len() == 2, "Leaving removed another room");
            let room: Room = serde_json::from_slice(&std::fs::read(root.join("qa-created-room.json"))?)?;
            ws.open_shared_note(&room, window, cx); Ok(())
        })??,
        "accept" => primary.update(cx, |ws, window, cx| {
            ws.open_accept_invitation_from("accept-home", window, cx);
        })?,
        "accepted" => {
            let reader = handles.iter().find(|h| h.read(cx).is_ok_and(|ws| ws.is_shared() && shared::scope(&ws.store, cx).is_some_and(|room| room.owner.is_some()))).context("Missing received note")?;
            reader.update(cx, |ws, window, cx| -> Result<()> {
                crate::devtools::select_script_window(window);
                ensure!(ws.sharing_access(cx) == Some(Access::ReadOnly) && ws.editor.as_ref().is_some_and(|e| e.read(cx).read_only), "Accepted note lost read-only access");
                ws.sharing.panel_open = true;
                ensure!(ws.sharing_toolbar(window, cx).is_some(), "Reader lost the collaboration entry");
                Ok(())
            })??;
        }
        "done" => {
            primary.update(cx, |ws, _, _| -> Result<()> {
                ensure!(!ws.is_shared(), "Deletion did not return to the personal context");
                ensure!(shared::rooms(&personal)?.len() == 1, "Deleted shared document remains listed");
                ensure!(ws.store.list_trash().len() == 1, "Removal retry duplicated or lost recovery copies");
                let id = std::fs::read_to_string(root.join("qa-personal-original"))?;
                ensure!(ws.store.load(&id)?.title.text == "即时更新的共享标题" && ws.store.list().iter().any(|entry| entry.id == id && entry.meta.share_root.is_none()), "Stopping did not restore the edited original");
                Ok(())
            })??;
            eprintln!("SHARING_UI_OK sidebar=stable title=live list=main create=contextual publish=retry-same-copy invite=retry-in-current-window members=isolated permission=updated revoke=confirmed accept=readonly original=kept window=same leave=isolated delete=recoverable");
        }
        _ => anyhow::bail!("Unknown sharing UI QA step"),
    }
    Ok(())
}
