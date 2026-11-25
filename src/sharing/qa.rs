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
    if name.starts_with("folder-sidebar-") { return folder_sidebar_step(name, &root, primary, cx); }
    if let Some(action) = name.strip_prefix("shared-drop ") { return shared_drop(action, primary, cx); }
    if let Some(action) = name.strip_prefix("move-matrix-") { return move_matrix(action, primary, cx); }
    match name {
        "document-status-list" | "document-status-grid" | "document-status-search" | "document-status-menu-open" =>
            primary.update(cx, |ws,window,cx| -> Result<()> {
                let entry = ws.entries.iter().find(|entry| entry.title.starts_with("长标题验收"))
                    .context("Missing the long document-title fixture")?;
                ensure!(entry.meta.pinned && ws.document_has_shared_status(entry, cx), "Fixture must be pinned and shared");
                match name {
                    "document-status-search" => {
                        ws.open_search(window,cx);
                        ws.search.update(cx, |field,cx| field.set_text("长标题验收",cx));
                    }
                    "document-status-menu-open" => {
                        ensure!(ws.row_menu.as_ref().is_some_and(|menu| menu.id == entry.id),
                            "The separate ellipsis did not open the document menu");
                    }
                    _ => {
                        ws.sidebar_open = true;
                        ws.folded[1] = false;
                        ws.home.tab = crate::home::HomeTab::Recent;
                        ws.home.grid = name.ends_with("grid");
                        ws.show_view(View::Home,cx);
                    }
                }
                Ok(())
            })??,
        name if name.starts_with("personal-sync-") => primary.update(cx, |ws,window,cx| -> Result<()> {
            if let Some(phase) = name.strip_prefix("personal-sync-").and_then(cloud_sync::personal_status_fixture) {
                ensure!(root.join("pause-cloud").is_file(), "Expected a paused synthetic cloud session");
                cloud_sync::fixture_status(&personal,phase,cx);
            } else {
                ensure!(name == "personal-sync-page","Unknown personal sync fixture");
            }
            ws.open_cloud_settings(window,cx);
            Ok(())
        })??,
        "shared-sync-settings-panel" | "shared-sync-settings-open" | "shared-sync-settings-back" =>
            primary.update(cx, |ws,window,cx| -> Result<()> {
                ensure!(ws.is_shared(), "Expected the synthetic shared workspace");
                match name {
                    "shared-sync-settings-panel" => {
                        cloud_sync::fixture_status(ws.store.root(),cloud_sync::SyncPhase::SignedOut,cx);
                        ws.open_shared_sync(window,cx);
                    }
                    "shared-sync-settings-open" => {
                        ws.open_cloud_settings_from_share(window,cx);
                        ensure!(ws.view == View::Settings && ws.settings_page == settings::Page::Cloud
                            && ws.cloud_settings_from_share, "Expected contextual iCloud settings");
                    }
                    _ => {
                        ws.close_settings(cx);
                        ensure!(ws.view == View::Doc && ws.sharing.sync_panel_open, "Shared sync panel did not reopen");
                        eprintln!("SHARED_SYNC_SETTINGS_RETURN_OK");
                    }
                }
                Ok(())
            })??,
        "account-open-wait" | "account-open-denied" => primary.update(cx, |ws,window,cx| -> Result<()> {
            ensure!(!ws.is_shared(), "Expected a personal workspace");
            let rooms: Vec<Room> = serde_json::from_slice(&std::fs::read(root.join("qa-session-rooms.json"))?)?;
            cloud_sync::fixture_status(&personal,cloud_sync::SyncPhase::AccountChecking,cx);
            std::fs::write(personal.join("require-account-verification"),[])?;
            if name.ends_with("denied") { std::fs::write(root.join("qa-room-access-denied"),[])?; }
            let room = rooms[1].clone();
            ws.open_shared_item_interactive(room.clone(),room.scope.note.clone(),None,window,cx);
            ensure!(ws.sharing.account_open.is_some() && !ws.is_shared(), "The share opened before account verification");
            Ok(())
        })??,
        "account-open-notice" | "account-open-access-notice" => primary.update(cx, |ws,_,_| -> Result<()> {
            let message = if name == "account-open-notice" {
                tr("正在确认 iCloud 账户…", "Checking your iCloud account…")
            } else {
                tr("正在读取共享权限…", "Checking sharing access…")
            };
            ensure!(!ws.is_shared() && ws.sharing_status_notice() == Some(message), "Unexpected deferred share status");
            Ok(())
        })??,
        "account-open-delay-access" => std::fs::write(root.join("qa-delay-room-access-once"),[])?,
        "account-open-offline" => cloud_sync::fixture_status(&personal,cloud_sync::SyncPhase::Offline,cx),
        "account-open-offline-cleared" => primary.update(cx, |ws,_,_| -> Result<()> {
            ensure!(!ws.is_shared() && ws.sharing.account_open.is_none() && ws.sharing_status_notice().is_none()
                && ws.toast.as_ref().is_some_and(|(kind, text, _)| *kind == ui::ToastKind::Error && text.contains("iCloud")),
                "Offline account check kept a progress notice");
            eprintln!("ACCOUNT_OPEN_OFFLINE_OK");
            Ok(())
        })??,
        "account-open-verify" => shared::session_qa::confirm_account_check(cx)?,
        "account-open-cancel" => primary.update(cx, |ws,_,cx| ws.show_view(View::Trash,cx))?,
        "account-open-kept-route" => primary.update(cx, |ws,_,_| -> Result<()> {
            ensure!(!ws.is_shared() && ws.sharing.account_open.is_none(), "The deferred share replaced a newer route");
            eprintln!("ACCOUNT_OPEN_BLOCKED_OK");
            Ok(())
        })??,
        "account-open-resumed" => primary.update(cx, |ws,_,cx| -> Result<()> {
            let rooms: Vec<Room> = serde_json::from_slice(&std::fs::read(root.join("qa-session-rooms.json"))?)?;
            ensure!(ws.current.as_deref() == Some(rooms[1].scope.note.as_str()) && shared::scope(&ws.store,cx) == Some(&rooms[1].scope),
                "The original shared destination did not resume");
            ensure!(ws.editor.as_ref().is_some_and(|editor|editor.read(cx).read_only), "The shared document lost read-only access");
            eprintln!("ACCOUNT_OPEN_RESUMED_OK");
            Ok(())
        })??,
        "review-panel-closed" => primary.update(cx, |ws, _, _| -> Result<()> {
            ensure!(ws.sharing.sheet.is_none() && !ws.sharing.panel_open && !ws.sharing.sync_panel_open,
                "The sharing panel did not dismiss");
            eprintln!("SHARING_PANEL_DISMISSED");
            Ok(())
        })??,
        "review-english" => primary.update(cx, |ws,_,cx| {
            ws.set_pref(|p| { p.english = true; p.language = "en".into(); },cx);
        })?,
        "sync-badge-open" => primary.update(cx, |ws,window,cx| -> Result<()> {
            let [id,_,_]: [String;3] = serde_json::from_slice(&std::fs::read(root.join("qa-sidebar-original-ids"))?)?;
            ws.open(&id,window,cx);
            Ok(())
        })??,
        "sync-badge-light" | "sync-badge-dark" | "sync-badge-notes-light" | "sync-badge-notes-dark" =>
            primary.update(cx, |ws,_,cx| {
                let dark = name.ends_with("dark");
                let feishu = !name.contains("notes");
                let theme = if feishu { "feishu" } else { "notes" };
                crate::themes::load(theme,true,||cx.text_system().all_font_names());
                ws.set_pref(|p| { p.feishu = feishu; p.theme = theme.into(); p.interface_style = theme.into();
                    p.appearance = if dark { "dark" } else { "light" }.into(); },cx);
            })?,
        "sync-badge-ready" | "sync-badge-uploading" | "sync-badge-stalled" => primary.update(cx, |ws,_,cx| -> Result<()> {
            ensure!(ws.is_shared() && ws.view == View::Doc, "Synthetic shared editor expected");
            let phase = match name {
                "sync-badge-stalled" => cloud_sync::SyncPhase::Failed,
                "sync-badge-uploading" => cloud_sync::SyncPhase::Uploading,
                _ => cloud_sync::SyncPhase::Ready,
            };
            cloud_sync::fixture_status(ws.store.root(),phase,cx);
            let shown = settings::sync_status::shared(ws,cx).context("Shared status missing")?;
            ensure!(shown.attention == name.ends_with("stalled"),"Incorrect alert visibility for {name}");
            cx.notify();
            Ok(())
        })??,
        "sync-badge-position" | "sync-badge-check-position" => primary.update(cx, |ws,_,cx| -> Result<()> {
            let editor = ws.editor.as_ref().context("Synthetic editor missing")?;
            let view = editor.read(cx);
            let anchor = ws.sharing.share_bounds.get().context("Shared toolbar anchor missing")?;
            let position = format!("{:?} {:?} {:?} {:?}",editor.entity_id(),view.content_origin.get(),view.list.logical_scroll_top(),anchor);
            let saved = root.join("qa-sync-position");
            if name == "sync-badge-position" { std::fs::write(saved,&position)?; }
            else { ensure!(position == std::fs::read_to_string(saved)?,"Toolbar or body moved after a sync state change"); }
            eprintln!("SYNC_BADGE_POSITION_OK {name} {position}");
            Ok(())
        })??,
        "sidebar-entry-click-personal" | "sidebar-entry-click-shared" => {
            let [id, _, _]: [String; 3] = serde_json::from_slice(&std::fs::read(root.join("qa-sidebar-original-ids"))?)?;
            let at = primary.read(cx)?.qa_sidebar_point(name.ends_with("shared"), &id, false).context("Original row not rendered")?;
            cx.update_window(primary.into(), |_, window, cx| {
                let modifiers = Modifiers::default();
                window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position: at, pressed_button: None, modifiers }), cx);
                window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1, first_mouse: false }), cx);
                window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1 }), cx);
            })?;
        }
        "sidebar-entry-fold-personal" => primary.update(cx, |ws, _, cx| { ws.folded[1] = true; cx.notify(); })?,
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
        "sidebar-switch-setup" => primary.update(cx, |ws, window, cx| ws.navigate_home(window, cx))?,
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
                    DROP TABLE qa_delayed_entries;
                    DROP TABLE qa_delayed_documents;
                    DROP TABLE qa_delayed_blocks;
                    DROP TABLE qa_delayed_search_blocks;
                    COMMIT")?;
                crate::app::cloud_notes_changed(&room, &[], true, cx);
            }
        }
        "sidebar-switch-offline" | "sidebar-switch-reconnect" => {
            let rooms: Vec<Room> = serde_json::from_slice(&std::fs::read(root.join("qa-session-rooms.json"))?)?;
            let room = shared::room_root(&personal, &rooms[0].scope.note)?;
            let phase = if name.ends_with("offline") { cloud_sync::SyncPhase::Offline } else { cloud_sync::SyncPhase::Connecting };
            cloud_sync::fixture_status(&room, phase, cx);
        }
        "sidebar-switch-writer" | "sidebar-switch-responsive" | "sidebar-switch-a" | "sidebar-switch-b"
        | "sidebar-switch-waiting" | "sidebar-switch-offline-feedback" => primary.update(cx, |ws, _, cx| -> Result<()> {
            let rooms: Vec<Room> = serde_json::from_slice(&std::fs::read(root.join("qa-session-rooms.json"))?)?;
            let a = matches!(name, "sidebar-switch-responsive" | "sidebar-switch-a" | "sidebar-switch-waiting" | "sidebar-switch-offline-feedback");
            let scope = &rooms[usize::from(a)].scope;
            if matches!(name, "sidebar-switch-responsive" | "sidebar-switch-writer") {
                let released = root.join("qa-sidebar-db-lock-released").is_file();
                eprintln!("SIDEBAR_SWITCH responsive: lock_released={released}");
                ensure!(!released, "The UI event loop did not respond until the SQLite writer released its lock");
                if name == "sidebar-switch-writer" { return Ok(()); }
            }
            ensure!(shared::scope(&ws.store, cx) == Some(scope), "Wrong shared workspace after {name}");
            ensure!(ws.view == View::Doc && ws.current.as_ref() == Some(&scope.note), "Wrong editor after {name}");
            if name == "sidebar-switch-waiting" {
                ensure!(ws.sharing.open_notice.is_some() && !ws.sharing.busy, "Expected delayed, non-blocking feedback on the existing editor");
            } else if name == "sidebar-switch-offline-feedback" {
                ensure!(ws.sharing.open_notice.is_none() && ws.toast.as_ref().is_some_and(|(kind, text, _)| {
                    *kind == ui::ToastKind::Error && text.contains("iCloud")
                }), "Expected actionable offline feedback on the existing editor");
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
                ws.store.update_meta(id, |meta| meta.top_order = Some(i as u64 + 1))?;
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
        "original-organization" => primary.update(cx, |ws, _, cx| -> Result<()> {
            let [id, _, _]: [String; 3] = serde_json::from_slice(&std::fs::read(root.join("qa-sidebar-original-ids"))?)?;
            ensure!(!ws.is_shared(), "Expected the personal workspace");
            let actual = |ws: &Workspace, cx: &App| ws.shared_entries(cx).into_iter().find(|entry| entry.id == id).unwrap().meta;
            ws.toggle_star(&id, cx);
            ws.toggle_shared_organization(&id, true, cx);
            ensure!(actual(ws, cx).pinned && actual(ws, cx).starred > 0, "Shared list did not see the original's marks");
            ensure!(ws.favorite_entries(cx).iter().filter(|entry| entry.id == id).count() == 1, "All Favorites must contain one shortcut to the original");
            ws.toggle_shared_organization(&id, false, cx);
            ws.toggle_pin(&id, cx);
            ensure!(!actual(ws, cx).pinned && actual(ws, cx).starred == 0, "Removing marks from either route left a duplicate");
            ensure!(!ws.favorite_entries(cx).iter().any(|entry| entry.id == id), "Cleared favorite is still visible");
            let entry = ws.store.list().into_iter().find(|entry| entry.id == id).context("Missing personal original")?;
            ensure!(!entry.meta.pinned && entry.meta.starred == 0, "Original's marks were not cleared on disk");
            eprintln!("SHARED_ORIGINAL_ORGANIZATION_OK");
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
        "show-toolbar-panel" | "hide-toolbar-panel" => primary.update(cx, |ws, _, cx| -> Result<()> {
            let id = ws.current.as_deref().context("Missing presence fixture")?;
            let show = name == "show-toolbar-panel";
            cloud_sync::presence::qa_online(ws.store.root(), id, show, cx);
            ws.sharing.permissions_only = false;
            ws.sharing.panel_open = show;
            cx.notify();
            Ok(())
        })??,
        "check-offline-toolbar" => primary.update(cx, |ws, window, cx| -> Result<()> {
            ensure!(ws.online_collaborators(cx).is_empty(), "Expected nobody online");
            ws.sharing.permissions_only = false;
            ws.sharing.panel_open = true;
            ensure!(ws.render_sharing_toolbar_panel(window, cx).is_none() && !ws.sharing.panel_open,
                "The online panel should close when nobody is online");
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
            ensure!(!shared || ws.online_collaborators(cx).is_empty(), "Expected a sharing control without anyone else online");
            let style = if feishu { "feishu" } else { "notes" };
            ws.set_pref(|p| { p.appearance = "light".into(); p.feishu = feishu; p.theme = style.into(); p.interface_style = style.into(); }, cx);
            ws.reload_themes(cx);
            ensure!(settings::interface_feishu() == feishu, "Sharing toolbar style did not change");
            Ok(())
        })??,
        "check-shared-add" => primary.update(cx, |w, _, _| -> Result<()> {
            ensure!(matches!(w.create_menu, Some((crate::folders::CreateTarget::SharedSidebar, _))), "Sidebar plus did not open the shared creation menu"); Ok(())
        })??,
        "check-sidebar-invite" => primary.update(cx, |w, _, _| -> Result<()> {
            ensure!(w.accept_invitation_open() && w.sharing.accept_at == Some("accept-sidebar"), "Sidebar invitation button did not open the invitation form"); Ok(())
        })??,
        "home-menu-fixture" | "home-menu-personal" => primary.update(cx, |w, window, cx| -> Result<()> {
            ensure!(!w.is_shared(), "Expected to capture menus in the personal workspace");
            w.show_view(if name == "home-menu-fixture" { View::Shared } else { View::Home }, cx);
            if name == "home-menu-personal" { w.home.tab = crate::home::HomeTab::Recent; }
            w.sidebar_open = true;
            w.folded[1] = true;
            w.sharing.folded = true;
            w.set_pref(|p| { p.appearance = "light".into(); p.feishu = true; p.theme = "feishu".into(); p.interface_style = "feishu".into(); }, cx);
            w.reload_themes(cx);
            crate::gpui_compat::focus(window, &w.focus, cx);
            Ok(())
        })??,
        "check-sidebar-create-shared-document" => primary.update(cx, |w, window, cx| -> Result<()> {
            ensure!(matches!(w.sharing.sheet, Some(Sheet::CreateNew(false, None))), "Sidebar did not start a shared document");
            ensure!(w.home.tab == crate::home::HomeTab::Recent, "Sidebar creation unexpectedly changed the Home tab");
            w.close_sharing(window, cx);
            eprintln!("SIDEBAR_SHARED_CREATION_OK"); Ok(())
        })??,
        "cloud-version-row-fixture" => primary.update(cx, |w, _, cx| -> Result<()> {
            w.confirm = Some(crate::app::Confirm::CloudVersions {
                id: String::new(),
                versions: vec![(3,"2026-11-20 11:23".into(),String::new()), (2,"2026-11-20 11:22".into(),"Version preview fixture".into()), (1,"2026-11-20 11:21".into(),"   ".into())],
                selected: Some(3),
            });
            cx.notify();
            Ok(())
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
            for target in [CreateTarget::SharedSidebar, CreateTarget::Library(None), CreateTarget::Library(Some(parent.clone()))] {
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
            ws.sharing.expanded.insert(parent.clone()); ws.sharing.expanded.insert(child); ws.sharing.collection = Some(parent); ws.sharing.folded = false; cx.notify();
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
            ensure!(ws.store.list().len() == count && ws.folder_current.as_ref() == Some(&child), "A shared session accepted library creation without a valid shared parent");
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
            for (i, entry) in entries.iter().enumerate() { ws.store.update_meta(&entry.id, |meta| meta.top_order = Some(i as u64 + 1))?; }
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
            ws.store.update_meta(&folder, |meta| meta.top_order = Some(0))?;
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
            ws.set_pref(|p| { p.language="zh".into(); p.english=false; }, cx);
            let id = ws.current.clone().context("Missing personal fixture")?;
            ws.open(&id, window, cx);
            ws.create_shared_note(&id, window, cx);
            ensure!(ws.can_choose_sharing_audience(cx) && !ws.sharing.public_link, "Account invitation must be the default");
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
            ensure!(ws.sharing.public_link && !ws.can_choose_sharing_audience(cx), "Reopening lost the prepared link scope");
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
            ensure!(ws.public_link_retry_access(room, cx).is_some() && ws.sharing.reply.as_ref().is_some_and(|reply| reply.audience.can_invite()),
                "Missing explicit retry or account invitation");
            ensure!(ws.sharing.message.as_ref().is_some_and(|(error, message)| *error && message.contains("请刷新")
                && !message.contains("Conflicting share")) && ws.sharing.copied_text.is_none(), "The sharing conflict was not localized or copied an invitation");
            std::fs::write(root.join("fail-link-reply-once"), b"fault")?;
            ws.retry_public_link(window, cx);
            Ok(())
        })??,
        "public-link-retry-conflict" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let room = ws.sheet_room().context("Missing access management")?;
            let cache = shared::room_root(&personal, &room.scope.note)?;
            std::fs::remove_file(cache.join("public-link-pending"))?;
            ensure!(ws.public_link_retry_access(room, cx) == Some(Access::ReadOnly),
                "A draft from another owner device did not offer public activation");
            std::fs::write(root.join("fail-link-conflict-once"), b"fault")?;
            ws.retry_public_link(window, cx);
            Ok(())
        })??,
        "public-link-published" => primary.update(cx, |ws, _, cx| -> Result<()> {
            let id = std::fs::read_to_string(root.join("qa-public-note"))?;
            let room = ws.current_room(cx).context("Public share did not stay open")?;
            ensure!(ws.current.as_deref() == Some(id.as_str()) && room.scope.note == id && !shared::public_link_pending(&personal, &room), "Public share did not retain the original document");
            ensure!(ws.sharing.copied_text.as_deref() == Some("邀请你在飞记中协作《Personal fixture》\nhttps://feinotes.app/share/?v=2&u=aHR0cHM6Ly93d3cuaWNsb3VkLmNvbS9zaGFyZS9zeW50aGV0aWMtZmVpLXVp")
                && ws.sharing.message.is_none(), "Readback did not recover the lost acknowledgement");
            Ok(())
        })??,
        "public-link-recopy" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let room = ws.sheet_room().context("Missing published share")?.clone();
            ws.set_pref(|p| { p.language="en".into(); p.english=true; }, cx);
            ws.sharing.copied_text = None;
            ws.copy_shared_invitation(room, window, cx);
            Ok(())
        })??,
        "public-link-close" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let reply = ws.sharing.reply.as_ref().context("Missing public share state")?;
            ensure!(reply.audience == Audience::LinkReadOnly && reply.members.is_empty() && reply.url.is_some(), "Link must be read-only");
            ensure!(ws.sharing.copied_text.as_deref() == Some("Join me in Fei Notes: Personal fixture\nhttps://feinotes.app/share/?v=2&u=aHR0cHM6Ly93d3cuaWNsb3VkLmNvbS9zaGFyZS9zeW50aGV0aWMtZmVpLXVp"),
                "Copying an existing invitation did not include its English message");
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
            ws.copy_shared_invitation(room, window, cx);
            Ok(())
        })??,
        "public-link-copy-denied" => primary.update(cx, |ws, _, _| -> Result<()> {
            ensure!(!ws.sharing.busy && ws.sharing.copied_text.is_none(), "Copy returned an inactive link");
            Ok(())
        })??,
        "public-link-explicit-invite" => primary.update(cx, |_, _, _| -> Result<()> {
            let doc = crate::docxml::parse("<title>Private fallback</title><p>Synthetic document</p>")?;
            let id = crate::store::Store::open_sqlite_at(personal.clone())?.create_from(&doc)?;
            let room = shared::original::prepare(&personal, &id, doc.title.text.clone(), Some(Access::ReadOnly))?;
            shared::ensure_created(&personal, &room)?;
            std::fs::write(root.join("fail-link-write-once"), b"fault")?;
            let failed = shared::publish_link(&personal, &room, None);
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
        "public-link-stop-return" => primary.update(cx, |ws, window, cx| {
            ws.close_sharing(window, cx);
            ws.navigate_personal_home_tab(crate::home::HomeTab::Recent, window, cx);
        })?,
        "public-link-stop-open" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let id = std::fs::read_to_string(root.join("qa-public-note"))?;
            ensure!(!ws.is_shared() && !ws.home.shared() && ws.view == View::Home, "Expected My documents");
            ws.home_click(&id, window, cx);
            Ok(())
        })??,
        "public-link-stop-original" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let id = std::fs::read_to_string(root.join("qa-public-note"))?;
            let room = ws.current_room(cx).context("Missing original's owning share")?;
            ensure!(ws.current.as_ref() == Some(&id) && ws.stopping_original(&room, cx),
                "Stop did not start from the shared original");
            ws.open_end_shared(room, window, cx);
            ws.submit_sharing(window, cx);
            Ok(())
        })??,
        "public-link-original-stopped" => primary.update(cx, |ws, _, cx| -> Result<()> {
            let id = std::fs::read_to_string(root.join("qa-public-note"))?;
            ensure!(!ws.is_shared() && !ws.home.shared() && ws.current.as_ref() == Some(&id) && ws.view == View::Doc,
                "Stopping jumped away from the personal editor");
            ensure!(ws.entries.iter().any(|entry| entry.title == "Private fallback")
                && ws.entries.iter().any(|entry| entry.id == id && entry.meta.share_root.is_none()),
                "The personal list lost the original or another document");
            ensure!(ws.editor.as_ref().is_some_and(|editor| editor.read(cx).core.doc.plain_text().contains("private body")),
                "The restored editor lost the shared content");
            eprintln!("PUBLIC_LINK_STOP_ORIGINAL_OK");
            Ok(())
        })??,
        "public-link-open-reattached-original" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let id = std::fs::read_to_string(root.join("qa-public-note"))?;
            ws.store.update_meta(&id, |meta| meta.share_root = Some(id.clone()))?;
            ws.entries = ws.store.list();
            ws.show_view(View::Home, cx);
            ws.home_click(&id, window, cx);
            Ok(())
        })??,
        "public-link-repaired-original" => primary.update(cx, |ws, _, cx| -> Result<()> {
            let id = std::fs::read_to_string(root.join("qa-public-note"))?;
            ensure!(!ws.is_shared() && ws.view == View::Doc && ws.current.as_ref() == Some(&id)
                && ws.entries.iter().any(|entry| entry.id == id && entry.meta.share_root.is_none()),
                "Opening the stale entry did not recover the personal original");
            ensure!(ws.editor.as_ref().is_some_and(|editor| editor.read(cx).core.doc.plain_text().contains("private body")),
                "Recovery changed the original content");
            eprintln!("PUBLIC_LINK_STALE_ORIGINAL_RECOVERED");
            Ok(())
        })??,
        "public-link-new-editor" => primary.update(cx, |ws, window, cx| -> Result<()> {
            ws.close_sharing(window, cx);
            ws.open_new_shared_item(false, window, cx);
            ws.sharing.folder_name.update(cx, |field, cx| field.set_text("Public editing fixture", cx));
            ws.set_public_link(true, window, cx);
            ensure!(ws.sharing.permission == Access::ReadOnly, "A new public link must default to read-only");
            ws.set_public_link_permission(Access::ReadWrite, cx);
            ensure!(ws.sharing.permission == Access::ReadWrite, "Public editing requires an explicit choice");
            ws.submit_sharing(window, cx);
            Ok(())
        })??,
        "public-link-new-editor-published" => primary.update(cx, |ws, _, cx| -> Result<()> {
            let room = ws.current_room(cx).context("New public document did not open")?;
            let reply = shared::manage(&personal, &room, "members", "", None)?;
            ensure!(reply.audience == Audience::LinkReadWrite && reply.members.is_empty()
                && !shared::public_link_pending(&personal, &room)
                && ws.sharing.copied_text.as_deref() == Some("Join me in Fei Notes: Public editing fixture\nhttps://feinotes.app/share/?v=2&u=aHR0cHM6Ly93d3cuaWNsb3VkLmNvbS9zaGFyZS9zeW50aGV0aWMtZmVpLXVp"),
                "New public editing invitation did not match confirmed access or include its English message");
            eprintln!("PUBLIC_LINK_EDIT_UI_OK selection=explicit creation=new-document copy=confirmed");
            Ok(())
        })??,
        "public-link-stop-recover" => {
            let doc = crate::docxml::parse("<title>Stop recovery fixture</title><p>Keep the original</p>")?;
            let id = crate::store::Store::open_sqlite_at(personal.clone())?.create_from(&doc)?;
            let room = shared::original::prepare(&personal, &id, doc.title.text.clone(), Some(Access::ReadOnly))?;
            shared::ensure_created(&personal, &room)?;
            let cache = shared::room_root(&personal, &id)?;
            let stop = || shared::end(&personal, &room, Some(crate::cloud_restore::lock_library(&cache)?));
            std::fs::write(root.join("fail-end-reply-once"), b"fault")?;
            ensure!(stop().is_err() && !cache.join("shared-note-ended").exists(), "Unconfirmed stop was treated as finished");
            stop()?;
            let mut store = crate::store::Store::open_sqlite_at(personal.clone())?;
            ensure!(cache.join("shared-note-ended").is_file() && shared::original::owner_room(&store.list(), &id).is_none()
                && store.load(&id)?.plain_text() == doc.plain_text(), "Confirmed stop retry did not restore the original document");
            eprintln!("PUBLIC_LINK_STOP_RECOVERY_OK delete=lost-reply original=retained");
        }
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
        "failed-invite" | "retry-invite" | "narrow" | "wide" | "members-compact" | "members-overflow" | "members-scroll-bottom" | "members-restore" | "reopen-members" | "members" | "readonly" | "revoke" | "remove-confirm" | "revoked" | "collaboration" | "review-invite" | "review-back" => {
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
                    "review-invite" => {
                        ensure!(matches!(ws.sharing.sheet, Some(Sheet::Members(_))), "Expected member management");
                        let anchor = ws.sharing_sheet_anchor(ws.sharing.sheet.as_ref().unwrap(), cx);
                        ensure!(anchor.is_some(), "Member management must be under the Share control");
                        ws.open_invitation(room.clone(), window, cx);
                        ensure!(matches!(ws.sharing.sheet, Some(Sheet::Invite(_))), "Expected invitation form");
                        ensure!(ws.sharing_sheet_anchor(ws.sharing.sheet.as_ref().unwrap(), cx) == anchor, "The invitation changed the sharing anchor");
                    }
                    "review-back" => {
                        let anchor = ws.sharing_sheet_anchor(ws.sharing.sheet.as_ref().context("Missing invitation")?, cx);
                        ws.return_to_member_management(window, cx);
                        ensure!(anchor.is_some() && ws.sharing_sheet_anchor(ws.sharing.sheet.as_ref().context("Missing member management")?, cx) == anchor
                            && ws.sharing.reply.is_some() && !ws.sharing.busy, "Returning to members moved the panel or discarded the roster");
                    }
                    "members-compact" => {
                        ensure!(ws.sharing.scroll.max_offset().y <= px(2.), "A single invite requires scrolling the members sheet");
                    }
                    "members-overflow" => {
                        let reply = ws.sharing.reply.as_mut().context("Missing members")?;
                        let sample = reply.members.first().context("Missing test participant")?.clone();
                        reply.members.extend((0..20).map(|i| shared::Member {
                            id: format!("qa-member-{i}"), name: format!("Collaborator {} with a long account name", i + 2),
                            accepted: i % 2 == 0, ..sample.clone()
                        }));
                        cx.notify();
                    }
                    "members-scroll-bottom" => {
                        let overflow = ws.sharing.scroll.max_offset().y;
                        ensure!(overflow > px(100.), "The long members sheet did not scroll");
                        ws.sharing.scroll.set_offset(point(px(0.), -overflow)); cx.notify();
                    }
                    "members-restore" => {
                        ensure!(ws.sharing.scroll.offset().y < px(-100.), "Could not reach the bottom of the members sheet");
                        ws.sharing.reply.as_mut().context("Missing members")?.members.truncate(1);
                        ws.sharing.scroll.set_offset(point(px(0.), px(0.))); cx.notify();
                    }
                    "narrow" => {
                        ensure!(ws.sharing.copied_text.is_some(), "Invitation success did not keep the next step available");
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
                        ensure!(ws.sharing.copied_text.as_ref().is_some_and(|text| text.contains("https://feinotes.app/share/?v=2&u=") && text.lines().count() == 2 && !text.contains("Apple")), "Creation did not copy an invitation message");
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
                        ws.resolve_shared_removal(id, true, window, cx);
                    }
                    "revoked" => {
                        ensure!(ws.sharing.reply.as_ref().context("Missing members")?.members.is_empty(), "Revocation was not reflected");
                        ws.close_sharing(window, cx);
                        let id = ws.current.as_deref().context("Missing presence fixture")?;
                        cloud_sync::presence::qa_online(ws.store.root(), id, true, cx);
                        ws.sharing.permissions_only = false;
                        ws.sharing.panel_open = true;
                        cx.notify();
                    }
                    "collaboration" => {
                        ensure!(ws.render_sharing_toolbar_panel(window, cx).is_some(), "Contextual collaboration panel is missing");
                        let id = ws.current.as_deref().context("Missing presence fixture")?;
                        cloud_sync::presence::qa_online(ws.store.root(), id, false, cx);
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
                ws.sharing.permissions_only = true;
                ws.sharing.panel_open = true;
                ensure!(ws.sharing_toolbar(window, cx).is_some(), "Reader lost the sharing access entry");
                cx.notify();
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

fn shared_drop(args: &str, primary: WindowHandle<Workspace>, cx: &mut App) -> Result<()> {
    ensure!(std::env::var_os("QUIET_SYNC_QA_SHARED_DROP").is_some(), "Use isolated shared-drop fixtures");
    let mut args = args.split_whitespace();
    let action = args.next().context("Shared-drop action missing")?;
    let role = args.next().context("Shared-drop role missing")?;
    let (kind, target) = (args.next().unwrap_or("document"), args.next().unwrap_or("inner"));
    let ws = primary.read(cx)?;
    let rows = ws.shared_sidebar_entries(cx);
    let id = |kind: &str| -> Result<String> {
        let title = if kind == "root" { role.into() } else { format!("{role} {kind}") };
        rows.iter().find(|row| row.title == title).map(|row| row.id.clone()).with_context(|| format!("Fixture missing: {title}"))
    };
    let room = ws.shared_list_room(&id("root")?, cx).context("Shared-drop room missing")?;
    let surface = |kind: &str| -> Result<_> {
        let id = id(kind)?;
        ws.qa_home_card_point(&id).with_context(|| format!("Home item did not render: {kind}"))
    };
    let sidebar = |kind: &str| -> Result<_> {
        ws.qa_sidebar_point(true, &id(kind)?, false).with_context(|| format!("Sidebar item did not render: {kind}"))
    };
    if matches!(action, "list" | "grid" | "sidebar" | "personal") {
        let (root, source, folder) = (id("root")?, id("source")?, id("folder")?);
        return primary.update(cx, |ws, window, cx| {
            ws.sidebar_open = true;
            ws.folded[1] = true;
            ws.sharing.folded = false;
            ws.sharing.expanded.extend([root, source.clone(), folder]);
            match action {
                "list" | "grid" => {
                    ws.home.grid = action == "grid";
                    ws.navigate_shared_sidebar(room, crate::sidebar::SidebarEntry::new(crate::sidebar::SidebarSection::Shared, &source), window, cx);
                }
                "personal" => ws.navigate_personal_home_tab(crate::home::HomeTab::Recent, window, cx),
                _ => cx.notify(),
            }
        });
    }
    let parent = |target: &str| if target == "inner" { id("inner target") }
        else { id(target) };
    if matches!(action, "verify" | "observe") {
        let item = id(kind)?;
        let expected = parent(target)?;
        let moving = id("folder")?;
        let child = id("child")?;
        let descendant = id("descendant")?;
        let entries = shared::store_for(&room, cx)?.list();
        let actual = entries.iter().find(|entry| entry.id == item).and_then(|entry| entry.meta.parent.as_deref());
        if action == "verify" { ensure!(actual == Some(expected.as_str()), "{role} {kind} did not land in {target}"); }
        ensure!([child, descendant].iter().all(|id| entries.iter().any(|entry| entry.id == *id
            && entry.meta.parent.as_ref() == Some(&moving))), "Moved folder lost descendants");
        let title = entries.iter().find(|entry| Some(entry.id.as_str()) == actual).map(|entry| entry.title.as_str());
        eprintln!("SHARED_DROP {action} {role} {kind} parent={title:?}");
        return Ok(());
    }
    if action == "verify-open" {
        let expected = match role { "Owner" => Access::Owner, "Editor" => Access::ReadWrite, _ => Access::ReadOnly };
        ensure!(ws.view == View::Doc && ws.current.as_deref() == Some(&id(kind)?)
            && ws.sharing_access(cx) == Some(expected)
            && ws.editor.as_ref().is_some_and(|e| e.read(cx).read_only != expected.can_write()),
            "More menu changed document access or opened the wrong document");
        eprintln!("SHARED_DROP verified {role} {kind} open-access={expected:?}");
        return Ok(());
    }
    if action == "verify-pin" {
        ensure!(preferences::active(cx).get("pin", &id(kind)?) != 0, "Pin menu action did not persist");
        eprintln!("SHARED_DROP verified {role} {kind} pin");
        return Ok(());
    }
    if action == "verify-drag" {
        ensure!(ws.home_dragging(&id(kind)?) == (target == "yes"), "Unexpected drag start state");
        eprintln!("SHARED_DROP verified {role} {kind} home-drag={target}");
        return Ok(());
    }
    if action == "observe-menu" {
        eprintln!("SHARED_DROP {role} menu-shortcut={}", ws.more_bounds.borrow().contains_key(&format!("shared-{}", id(kind)?)));
        return Ok(());
    }
    let at = match action {
        "home-down" => surface(kind)?,
        "home-move" | "home-up" => surface(if target == "inner" { "inner target" } else { target })?,
        "sidebar-down" | "hover" => sidebar(kind)?,
        "sidebar-move" | "sidebar-up" => sidebar(if target == "inner" { "inner target" } else { target })?,
        "blank-move" | "blank-up" => point(px(450.), px(905.)),
        "cross-move" | "cross-up" => {
            let other = if role == "Owner" { "Editor" } else { "Owner" };
            let target = rows.iter().find(|row| row.title == other).context("Other share missing")?;
            ws.qa_sidebar_point(true, &target.id, false).context("Other share did not render")?
        }
        "menu" => ws.more_bounds.borrow().get(&format!("shared-{}", id(kind)?)).context("More control missing")?.center(),
        "pin" => ws.home.anchor("qa-shared-child-pin").context("Pin control missing")?.center(),
        "open" => ws.home.anchor("qa-shared-child-open").context("Open control missing")?.center(),
        _ => anyhow::bail!("Unknown shared-drop action: {action}"),
    };
    cx.update_window(primary.into(), |_, window, cx| {
        let modifiers = Modifiers::default();
        let move_to = |held| PlatformInput::MouseMove(MouseMoveEvent { position: at, pressed_button: held, modifiers });
        let down = PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1, first_mouse: false });
        let up = PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1 });
        if action.ends_with("-down") || matches!(action, "menu" | "pin" | "open" | "hover") {
            window.dispatch_event(move_to(None), cx);
            if action != "hover" { window.dispatch_event(down, cx); }
            if matches!(action, "menu" | "pin" | "open") { window.dispatch_event(up, cx); }
        } else if action.ends_with("-move") {
            window.dispatch_event(move_to(Some(MouseButton::Left)), cx);
        } else {
            window.dispatch_event(up, cx);
        }
    })?;
    Ok(())
}

fn move_matrix(action: &str, primary: WindowHandle<Workspace>, cx: &mut App) -> Result<()> {
    use crate::home::HomeTab;
    if matches!(action, "document-move-click" | "document-target-click" | "document-confirm-click") {
        let ws = primary.read(cx)?;
        let anchor = match action {
            "document-move-click" => "qa-document-move".to_owned(),
            "document-target-click" => "qa-move-first-hit".to_owned(),
            _ => "qa-move-confirm".to_owned(),
        };
        let at = ws.home.anchor(&anchor).context(format!("Move control did not render: {anchor}"))?.center();
        cx.update_window(primary.into(), |_, window, cx| {
            let modifiers = Modifiers::default();
            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1, first_mouse: false }), cx);
            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1 }), cx);
        })?;
        return Ok(());
    }
    primary.update(cx, |ws,window,cx| -> Result<()> {
        ensure!(std::env::var_os("QUIET_SYNC_QA_MOVE_MATRIX").is_some(), "Move matrix requires isolated fixtures");
        fn id(entries: &[crate::store::Entry], title: &str) -> Result<String> {
            Ok(entries.iter().find(|entry| entry.title == title).with_context(|| format!("Missing fixture {title}"))?.id.clone())
        }
        fn pair(entries: &[crate::store::Entry], prefix: &str) -> Result<Vec<String>> {
            ["document","folder"].into_iter().map(|kind| id(entries,&format!("{prefix} moving {kind}"))).collect()
        }
        let shared_entries = ws.shared_sidebar_entries(cx);
        let personal_entries = ws.personal_navigation_entries(cx);
        match action {
            "setup" => {
                ws.sidebar_open = true;
                ws.folded[1] = false;
                ws.sharing.folded = false;
                ws.expanded.insert(id(&personal_entries,"Move personal source")?);
                for title in ["Move shared A","Move shared A source"] {
                    ws.expanded.insert(id(&personal_entries,title)?);
                    ws.sharing.expanded.insert(id(&shared_entries,title)?);
                }
                ws.home.tab = HomeTab::Recent;
                ws.navigate_home(window,cx);
            }
            "drag-personal-document" | "drag-personal-folder" | "drag-shared-document" | "drag-shared-folder" => {
                let shared = action.starts_with("drag-shared");
                let entries = if shared { &shared_entries } else { &personal_entries };
                let prefix = if shared { "Move shared A" } else { "Move personal" };
                let kind = action.rsplit('-').next().unwrap();
                let source = id(entries,&format!("{prefix} moving {kind}"))?;
                let target = id(entries,&format!("{prefix} target"))?;
                let from = ws.qa_sidebar_point(shared,&source,false).context("Source row must be visible")?;
                let to = ws.qa_sidebar_point(shared,&target,false).context("Destination row must be visible")?;
                ws.with_sidebar(shared, |ws| {
                    ws.arm_folder_drag(&source,from,cx);
                    ws.move_folder_drag(to,window,cx);
                    ws.end_folder_drag(false,window,cx);
                });
            }
            "verify-personal-drag" | "verify-personal-root" => {
                ensure!(!ws.is_shared(),"Expected personal library");
                let entries = ws.store.list();
                let target = id(&entries,"Move personal target")?;
                let expected = (action == "verify-personal-drag").then_some(&target);
                for item in pair(&entries,"Move personal")? {
                    ensure!(entries.iter().any(|entry| entry.id == item && entry.meta.parent.as_ref() == expected),
                        "Wrong personal destination after {action}");
                }
                eprintln!("MOVE_MATRIX_OK {action}");
            }
            "personal-root" => ws.move_docs(&pair(&personal_entries,"Move personal")?,None,cx),
            "personal-into-share" => {
                ws.move_docs(&pair(&personal_entries,"Move personal")?,Some(id(&personal_entries,"Move shared B target")?),cx);
                ensure!(matches!(ws.sharing.sheet,Some(Sheet::MoveInto(..))),"Moving into a share must first confirm who gains access");
                ws.submit_sharing(window,cx);
            }
            "verify-personal-into-share" => {
                let entries = ws.store.list();
                let scope = id(&entries,"Move shared B")?;
                let target = id(&entries,"Move shared B target")?;
                let room = ws.shared_list_room(&scope,cx).context("Missing destination share")?;
                let shared = shared::store_for(&room,cx)?.list();
                let folder = id(&entries,"Move personal moving folder")?;
                let child = id(&entries,"Move personal child")?;
                for item in pair(&entries,"Move personal")? {
                    ensure!(entries.iter().any(|entry| entry.id == item && entry.meta.parent.as_ref() == Some(&target) && entry.meta.share_root.as_ref() == Some(&scope))
                        && shared.iter().any(|entry| entry.id == item && entry.meta.parent.as_ref() == Some(&target)), "Real move missed the shared catalogue");
                }
                ensure!(shared.iter().any(|entry| entry.id == child && entry.meta.parent.as_ref() == Some(&folder)), "Shared move split the subtree");
                eprintln!("MOVE_MATRIX_OK {action}");
            }
            "drag-projected-to-personal" | "drag-projected-to-personal-folder" => {
                let kind = if action.ends_with("folder") { "folder" } else { "document" };
                let source = id(&personal_entries, &format!("Move shared A moving {kind}"))?;
                let target = id(&personal_entries, "Move personal target")?;
                let from = ws.qa_sidebar_point(false, &source, false).context("Projected row must be visible")?;
                let to = ws.qa_sidebar_point(false, &target, false).context("Personal destination must be visible")?;
                ws.arm_folder_drag(&source, from, cx);
                ws.move_folder_drag(to, window, cx);
                ws.end_folder_drag(false, window, cx);
            }
            "observe-projected-drop" => {
                let entries = ws.store.list();
                let source = id(&entries, "Move shared A source")?;
                for item in pair(&entries, "Move shared A")? {
                    let entry = entries.iter().find(|entry| entry.id == item).unwrap();
                    ensure!(entry.meta.parent.as_ref() == Some(&source),
                        "Cross-library drop changed the personal projection of {}", entry.title);
                    let personal_target = id(&entries, "Move personal target")?;
                    ensure!(ws.store.set_parent(&item, Some(&personal_target)).is_err(),
                        "Personal store accepted a move for a shared child");
                }
                eprintln!("MOVE_MATRIX_OK {action}");
            }
            "observe-shared-drag" | "open-share" => {
                let scope = id(&shared_entries,"Move shared A")?;
                let room = ws.shared_list_room(&scope,cx).context("Missing source share")?;
                if action == "open-share" {
                    ws.open_shared_note(&room,window,cx);
                } else {
                    let entries = shared::store_for(&room,cx)?.list();
                    for kind in ["document","folder"] {
                        let title = format!("Move shared A moving {kind}");
                        let parent = entries.iter().find(|entry| entry.title == title).and_then(|entry|entry.meta.parent.as_ref()).context("Drag lost the original")?;
                        let location = entries.iter().find(|entry| &entry.id == parent).context("Drag left an invalid parent")?;
                        eprintln!("MOVE_MATRIX_OBSERVED shared-sidebar-{kind} {}", location.title);
                    }
                }
            }
            "open-shared-document" => {
                let room = ws.current_room(cx).context("Missing shared library")?;
                let document = id(&ws.entries, "Move shared A moving document")?;
                ws.open_shared_item_interactive(room, document, None, window, cx);
            }
            "open-document-menu" => {
                ensure!(ws.view == View::Doc && ws.is_shared(), "Shared editor did not open");
                ws.doc_menu = true;
                cx.notify();
            }
            "verify-document-menu-move" => {
                let entries = ws.store.list();
                let document = id(&entries, "Move shared A moving document")?;
                let target = id(&entries, "Move shared A target")?;
                ensure!(entries.iter().any(|entry| entry.id == document && entry.meta.parent.as_ref() == Some(&target)),
                    "The document menu did not move the shared document");
                eprintln!("MOVE_MATRIX_OK {action}");
            }
            "shared-to-target" | "shared-to-root" | "verify-shared-target" | "verify-shared-root" => {
                ensure!(ws.is_shared() && ws.view == View::Folder, "Expected a shared folder");
                let entries = ws.store.list();
                let items = pair(&entries,"Move shared A")?;
                let root = id(&entries,"Move shared A")?;
                let target = id(&entries,"Move shared A target")?;
                match action {
                    "shared-to-target" => ws.move_docs(&items,Some(target),cx),
                    "shared-to-root" => ws.move_docs(&items,None,cx),
                    _ => {
                        let expected = if action.ends_with("target") { &target } else { &root };
                        for item in items {
                            ensure!(entries.iter().any(|entry| entry.id == item && entry.meta.parent.as_ref() == Some(expected)),
                                "Wrong shared destination after {action}");
                        }
                        let child = id(&entries,"Move shared A child")?;
                        let folder = id(&entries,"Move shared A moving folder")?;
                        ensure!(entries.iter().any(|entry| entry.id == child && entry.meta.parent.as_ref() == Some(&folder)),
                            "Shared move split the subtree");
                        eprintln!("MOVE_MATRIX_OK {action}");
                    }
                }
            }
            _ => anyhow::bail!("Unknown move-matrix check"),
        }
        Ok(())
    })?
}

fn folder_sidebar_step(name: &str, root: &std::path::Path, primary: WindowHandle<Workspace>, cx: &mut App) -> Result<()> {
    let ids = root.join("qa-folder-sidebar-ids");
    if name == "folder-sidebar-setup" {
        primary.update(cx, |ws, window, cx| -> Result<()> {
            let owner = ws.store.create_folder("项目共享", None)?;
            let nested = ws.store.create_folder("会议资料", Some(&owner))?;
            let document = ws.store.create_from_in(&crate::model::Document { title: crate::model::RichText::plain("已有的议程"), ..Default::default() }, Some(&nested))?;
            let (received, readonly, empty) = shared::session_qa::readonly_folder(root, &crate::store::Store::root_dir())?;
            std::fs::write(&ids, serde_json::to_vec(&[owner.clone(), nested.clone(), document, received, readonly, empty])?)?;
            ws.entries = ws.store.list(); ws.sort();
            ws.sidebar_open = true; ws.sharing.folded = false; ws.folded[1] = false;
            ws.expanded.extend([owner.clone(), nested]);
            ws.create_shared_note(&owner, window, cx);
            ws.sharing.email.update(cx, |field, cx| field.set_text("participant@example.test", cx));
            ws.submit_sharing(window, cx);
            Ok(())
        })??;
        return Ok(());
    }
    let [owner, nested, document, received, readonly, empty]: [String; 6] = serde_json::from_slice(&std::fs::read(ids)?)?;
    #[cfg(target_os = "macos")]
    if let Some(step) = name.strip_prefix("folder-sidebar-create-click-") {
        let ws = primary.read(cx)?;
        let bounds = match step {
            "personal-header" => ws.home.anchor("create-sidebar"),
            "personal-shortcut" => ws.more_bounds.borrow().get(&format!("add-doc-false-{owner}")).copied(),
            "shared-shortcut" => ws.more_bounds.borrow().get(&format!("add-shared-folder-false-{owner}")).copied(),
            "nested-shortcut" => ws.more_bounds.borrow().get(&format!("add-shared-folder-false-{nested}")).copied(),
            "personal-folder-choice" => ws.create_menu.as_ref().map(|(_, pos)| Bounds::new(
                point(pos.x + px(12.), pos.y + px(ui::MENU_PAD_Y + ui::MENU_ROW_H * 2.)), size(px(130.), px(ui::MENU_ROW_H)))),
            _ => anyhow::bail!("Unknown sidebar creation control"),
        }.context("Sidebar creation control was not rendered")?;
        let at = bounds.center();
        cx.update_window(primary.into(), |_, window, cx| {
            let modifiers = Modifiers::default();
            window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position: at, pressed_button: None, modifiers }), cx);
            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1, first_mouse: false }), cx);
            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1 }), cx);
        })?;
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    if let Some(step) = name.strip_prefix("folder-sidebar-consistency-click-") {
        let ws = primary.read(cx)?;
        let target = std::fs::read_to_string(root.join("qa-consistency-folder")).unwrap_or_else(|_| nested.clone());
        let bounds = match step {
            "personal-more" => ws.more_bounds.borrow().get(&format!("doc-{target}")).copied(),
            "shared-more" => ws.more_bounds.borrow().get(&format!("shared-folder-{target}")).copied(),
            "root-more" => ws.more_bounds.borrow().get(&format!("shared-folder-{owner}")).copied(),
            "shared-create" => ws.more_bounds.borrow().get(&format!("add-shared-folder-false-{target}")).copied(),
            "delete" => ws.home.anchor("qa-shared-folder-delete"),
            "confirm" => ws.home.anchor("qa-confirm-ok"),
            _ => anyhow::bail!("Unknown consistency control"),
        }.context(format!("Sidebar control {step} was not rendered"))?;
        let at = bounds.center();
        cx.update_window(primary.into(), |_, window, cx| {
            let modifiers = Modifiers::default();
            window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position: at, pressed_button: None, modifiers }), cx);
            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1, first_mouse: false }), cx);
            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1 }), cx);
        })?;
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    if let Some(kind) = name.strip_prefix("folder-sidebar-origin-click-home-") {
        let id = if kind == "document" { std::fs::read_to_string(root.join("qa-new-original-id"))? } else { owner.clone() };
        let at = primary.read(cx)?.qa_home_card_point(&id).context("Expected a rendered Home card")?;
        cx.update_window(primary.into(), |_, window, cx| {
            let modifiers = Modifiers::default();
            window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position: at, pressed_button: None, modifiers }), cx);
            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1, first_mouse: false }), cx);
            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1 }), cx);
        })?;
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    if name == "folder-sidebar-origin-click-breadcrumb" {
        let at = primary.read(cx)?.home.anchor("folder-breadcrumb-root").context("Folder breadcrumb did not render")?.center();
        cx.update_window(primary.into(), |_, window, cx| {
            let modifiers = Modifiers::default();
            window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position: at, pressed_button: None, modifiers }), cx);
            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1, first_mouse: false }), cx);
            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1 }), cx);
        })?;
        return Ok(());
    }
    if let Some(row) = name.strip_prefix("folder-sidebar-click-") {
        let (in_shared, id, icon) = match row {
            "owner" => (true, &owner, false),
            "owner-chevron" => (true, &owner, true),
            "nested" => (true, &nested, false),
            "document" => (true, &document, false),
            "personal-document" => (false, &document, false),
            "received" => (true, &received, false),
            "readonly" => (true, &readonly, false),
            "empty" => (true, &empty, false),
            _ => anyhow::bail!("Unknown folder sidebar row"),
        };
        let at = primary.read(cx)?.qa_sidebar_point(in_shared, id, icon).context("Sidebar row did not render")?;
        cx.update_window(primary.into(), |_, window, cx| {
            let modifiers = Modifiers::default();
            window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position: at, pressed_button: None, modifiers }), cx);
            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1, first_mouse: false }), cx);
            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1 }), cx);
        })?;
        return Ok(());
    }
    if matches!(name,"folder-sidebar-toolbar-click" | "folder-sidebar-document-more") {
        let ws = primary.read(cx)?;
        let bounds = if name.ends_with("document-more") { ws.more_btn_bounds.get() } else { ws.sharing.share_bounds.get() }
            .context("Sharing toolbar anchor missing")?;
        let at = point(bounds.left()+bounds.size.width/2.,bounds.top()+bounds.size.height/2.);
        cx.update_window(primary.into(), |_, window, cx| {
            let modifiers = Modifiers::default();
            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1, first_mouse: false }), cx);
            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1 }), cx);
        })?;
        return Ok(());
    }
    primary.update(cx, |ws, window, cx| -> Result<()> {
        let shared_rows = ws.qa_rendered_sidebar_rows(true);
        let personal_rows = ws.qa_rendered_sidebar_rows(false);
        let exactly_once = |rows: &[(String, usize)], id: &str, depth| {
            rows.iter().filter(|(key, _)| key == id).count() == 1 && rows.iter().any(|(key, d)| key == id && *d == depth)
        };
        let scope = shared::scope(&ws.store, cx);
        match name {
            #[cfg(target_os = "macos")]
            "folder-sidebar-create-capture" => {
                ensure!(ws.current.as_ref() == Some(&document), "Expected the shared document before opening the personal menu");
                let editor = ws.editor.as_ref().context("Missing editor")?;
                std::fs::write(root.join("qa-sidebar-editor"), format!("{:?}", editor.entity_id()))?;
                std::fs::write(root.join("qa-sidebar-count"), ws.personal_navigation_entries(cx).len().to_string())?;
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-create-menu" | "folder-sidebar-create-dismissed" => {
                use crate::folders::CreateTarget;
                let editor = ws.editor.as_ref().context("Menu closed the editor")?;
                ensure!(ws.current.as_ref() == Some(&document) && scope.is_some_and(|scope| scope.note == owner)
                    && format!("{:?}", editor.entity_id()) == std::fs::read_to_string(root.join("qa-sidebar-editor"))?,
                    "Toggling the new menu changed the current document or editor");
                ensure!(ws.personal_navigation_entries(cx).len().to_string() == std::fs::read_to_string(root.join("qa-sidebar-count"))?,
                    "Toggling the create menu wrote to the personal library");
                if name.ends_with("-dismissed") {
                    ensure!(ws.create_menu.is_none(), "Second click left the menu open");
                    eprintln!("SIDEBAR_CREATE_MENU_STABLE");
                } else {
                    ensure!(matches!(ws.create_menu.as_ref(), Some((CreateTarget::PersonalSidebar(None), _)))
                        && ws.can_create_folder(&CreateTarget::PersonalSidebar(None), cx), "The personal menu was missing its root destination");
                }
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-create-personal-made" | "folder-sidebar-create-shared-made" | "folder-sidebar-create-nested-made" => {
                use crate::sidebar::SidebarSection;
                let section = if name.ends_with("-personal-made") { SidebarSection::Personal } else { SidebarSection::Shared };
                let parent = if name.ends_with("-nested-made") { &nested } else { &owner };
                let new = ws.folder_current.as_ref().context("New folder did not open")?;
                ensure!(scope.is_some_and(|scope| scope.note == owner) && ws.entries.iter().any(|e| &e.id == new
                    && e.meta.folder.is_some() && e.meta.parent.as_ref() == Some(parent)),
                    "New folder missed its collaborative parent");
                let other = if section == SidebarSection::Shared { SidebarSection::Personal } else { SidebarSection::Shared };
                ensure!(ws.sidebar_rename(new, section).is_some() && ws.sidebar_rename(new, other).is_none()
                    && ws.sidebar_entry.as_ref().is_some_and(|entry| &entry.id == new && entry.section == section),
                    "New folder did not confine the name field to its invoking sidebar");
                let rows = if section == SidebarSection::Shared { &shared_rows } else { &personal_rows };
                ensure!(rows.iter().any(|(id, _)| id == new), "New folder did not appear in the invoking sidebar");
                eprintln!("SIDEBAR_SHARED_CHILD_CREATED {name}");
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-create-readonly-absent" => {
                let bounds = ws.more_bounds.borrow();
                for id in [&received, &empty] {
                    ensure!(!bounds.contains_key(&format!("add-shared-folder-false-{id}")), "Read-only folder offered a creation shortcut");
                }
                eprintln!("SIDEBAR_CREATE_READONLY_PROTECTED");
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-create-personal-root-made" => {
                let new = ws.folder_current.as_ref().context("New personal folder did not open")?;
                ensure!(!ws.is_shared() && ws.entries.iter().any(|e| &e.id == new
                    && e.meta.folder.is_some() && e.meta.parent.is_none() && e.meta.share_root.is_none()),
                    "Personal root creation wrote to the shared workspace");
                eprintln!("SIDEBAR_CREATE_PERSONAL_ROOT");
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-consistency-fixture" => {
                let made = ws.folder_current.clone().context("Expected the new folder")?;
                ws.finish_rename(false, window, cx);
                let child = ws.store.create_from_in(&crate::model::Document { title: crate::model::RichText::plain("恢复用测试文档"), ..Default::default() }, Some(&made))?;
                std::fs::write(root.join("qa-consistency-folder"), &made)?;
                std::fs::write(root.join("qa-consistency-child"), &child)?;
                ws.entries = ws.store.list();
                ws.publish_shared_folder_change(cx);
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-consistency-presentation" => {
                let folder = std::fs::read_to_string(root.join("qa-consistency-folder"))?;
                let child = std::fs::read_to_string(root.join("qa-consistency-child"))?;
                for rows in [&shared_rows, &personal_rows] {
                    ensure!(exactly_once(rows, &folder, 1) && exactly_once(rows, &child, 2), "Folder and its contents did not appear in both sections");
                }
                let slot_bounds = ws.more_bounds.borrow();
                for (personal, shared) in [(format!("doc-{folder}"), format!("shared-folder-{folder}")),
                    (format!("add-doc-false-{folder}"), format!("add-shared-folder-false-{folder}"))] {
                    let x = slot_bounds.get(&personal).with_context(|| format!("Missing My Documents shortcut: {personal}"))?.center().x;
                    let y = slot_bounds.get(&shared).with_context(|| format!("Missing Shared Documents shortcut: {shared}"))?.center().x;
                    ensure!((f32::from(x) - f32::from(y)).abs() <= 0.5, "Folder controls use different columns: {x:?}, {y:?}");
                }
                eprintln!("SIDEBAR_FOLDER_CONTROLS_ALIGNED");
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-consistency-expand" => {
                let folder = std::fs::read_to_string(root.join("qa-consistency-folder"))?;
                ws.expanded.insert(folder.clone());
                ws.sharing.expanded.extend([owner.clone(), folder]);
                cx.notify();
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-consistency-menu-stable" => {
                let editor = ws.editor.as_ref().context("Opening the folder menu released the editor")?;
                ensure!(ws.current.as_ref() == Some(&document) && scope.is_some_and(|scope| scope.note == owner)
                    && format!("{:?}", editor.entity_id()) == std::fs::read_to_string(root.join("qa-sidebar-editor"))?,
                    "Opening the folder menu switched to another workspace");
                ensure!(ws.row_menu.is_some() || ws.create_menu.is_some(), "Expected a sidebar popover");
                eprintln!("SIDEBAR_FOLDER_MENU_STABLE");
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-consistency-deleted" => {
                let folder = std::fs::read_to_string(root.join("qa-consistency-folder"))?;
                let child = std::fs::read_to_string(root.join("qa-consistency-child"))?;
                for rows in [&shared_rows, &personal_rows] {
                    ensure!(!rows.iter().any(|(id, _)| id == &folder || id == &child), "The deleted folder survived in a sidebar");
                }
                ensure!(ws.current.as_ref() == Some(&document) && scope.is_some_and(|scope| scope.note == owner),
                    "Deleting a sibling changed the open document");
                let personal = cx.global::<crate::shared_store::SharedStore>().clone().list_trash();
                ensure!(personal.iter().any(|item| item.id == folder && item.share_root.as_ref() == Some(&owner)), "Recovery item missed the personal Trash");
                ensure!(ws.store.list_trash().iter().any(|item| item.id == folder), "Canonical folder missed the shared Trash");
                eprintln!("SIDEBAR_FOLDER_DELETE_BOTH");
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-consistency-restore" => {
                let folder = std::fs::read_to_string(root.join("qa-consistency-folder"))?;
                ws.change_shared_trash(ws.current_room(cx).context("Expected the owner room")?, folder,
                    crate::sharing::SharedTrashAction::Restore, window, cx);
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-consistency-root-deleted" => {
                ensure!(!ws.is_shared(), "Deleting the sharing root did not leave its workspace");
                ensure!(!shared_rows.iter().any(|(id, _)| id == &owner) && !personal_rows.iter().any(|(id, _)| id == &owner),
                    "Deleted root survived in a sidebar");
                ensure!(ws.store.list_trash().iter().any(|item| item.id == owner && item.share_root.is_none()),
                    "Deleted root did not become a locally restorable item");
                eprintln!("SIDEBAR_FOLDER_ROOT_DELETED");
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-origin-personal-home-folder" | "folder-sidebar-origin-personal-home-document" |
            "folder-sidebar-origin-shared-home-folder" => {
                let shared = name.contains("shared-home");
                ensure!(!ws.is_shared(), "Expected the personal workspace before clicking Home");
                ws.show_view(if shared { View::Shared } else { View::Home }, cx);
                ws.home.tab = if shared { crate::home::HomeTab::Shared } else { crate::home::HomeTab::Recent };
                ws.sharing.folded = false;
                ws.folded[1] = false;
                ws.home.grid = true;
                cx.notify();
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-origin-check-personal-folder" | "folder-sidebar-origin-check-personal-document" |
            "folder-sidebar-origin-check-shared-folder" => {
                let shared = name.contains("check-shared");
                let made = name.ends_with("document").then(|| std::fs::read_to_string(root.join("qa-new-original-id"))).transpose()?;
                let id = made.as_ref().unwrap_or(&owner);
                ensure!(scope.is_some_and(|scope| &scope.note == id)
                    && (ws.folder_current.as_ref() == Some(id) || ws.current.as_ref() == Some(id)), "Home opened the wrong shared item");
                let picks = if shared { (None, Some(id.as_str())) } else { (Some(id.as_str()), None) };
                ensure!((ws.qa_sidebar_pick(false), ws.qa_sidebar_pick(true)) == picks, "Home highlighted the wrong sidebar section");
                let label = if shared { tr("共享文档", "Shared documents") } else { tr("我的文档", "My documents") };
                ensure!(ws.breadcrumb_library_title() == label, "The breadcrumb ignored the entry's sidebar section");
                eprintln!("SIDEBAR_HOME_ORIGIN_OK {name}");
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-origin-return-personal" | "folder-sidebar-origin-return-shared" => {
                let tab = if name.ends_with("shared") { crate::home::HomeTab::Shared } else { crate::home::HomeTab::Recent };
                ensure!(!ws.is_shared() && ws.view == View::Home && ws.home.tab == tab, "The breadcrumb returned to the wrong Home tab");
                eprintln!("FOLDER_BREADCRUMB_RETURN_OK {name}");
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-origin-fold-personal" => {
                ws.folded[1] = true;
                cx.notify();
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-origin-no-jump" => {
                ensure!(ws.qa_sidebar_pick(false).is_none() && ws.qa_sidebar_pick(true).is_none(), "Folding the selected section moved selection to its duplicate");
                eprintln!("SIDEBAR_COLLAPSE_SELECTION_OK");
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-origin-make-personal-folder" | "folder-sidebar-origin-make-shared-folder" |
            "folder-sidebar-origin-make-personal-document" | "folder-sidebar-origin-make-shared-document" => {
                use crate::sidebar::SidebarSection;
                let personal = name.contains("make-personal");
                let folder = name.ends_with("folder");
                let section = if personal { SidebarSection::Personal } else { SidebarSection::Shared };
                let kind = if folder { crate::folders::SharedFolderCreation::Folder(None) } else { crate::folders::SharedFolderCreation::Document };
                ws.expanded.remove(&nested);
                ws.sharing.expanded.remove(&nested);
                // Test collapsed groups for folders and collapsed branches in an open group for documents.
                ws.folded[1] = !personal && folder;
                ws.sharing.folded = personal && folder;
                let state = if personal { (ws.sharing.folded, &ws.sharing.expanded) } else { (ws.folded[1], &ws.expanded) };
                let saved = (personal, folder, state);
                std::fs::write(root.join("qa-sidebar-origin-state"), serde_json::to_vec(&saved)?)?;
                let room = ws.shared_list_room(&owner, cx).context("Shared owner is missing")?;
                ws.create_in_shared_sidebar(room, nested.clone(), kind, section, window, cx);
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-origin-check-creation" => {
                let (personal, folder, (folded, expanded)): (bool, bool, (bool, std::collections::HashSet<String>)) =
                    serde_json::from_slice(&std::fs::read(root.join("qa-sidebar-origin-state"))?)?;
                let actual = if personal { (ws.sharing.folded, &ws.sharing.expanded) } else { (ws.folded[1], &ws.expanded) };
                ensure!(actual == (folded, &expanded), "Creation changed the other sidebar's fold state");
                let id = if folder { ws.folder_current.as_ref() } else { ws.current.as_ref() }.context("The new item did not open")?;
                ensure!(ws.entries.iter().any(|entry| &entry.id == id && entry.meta.parent.as_ref() == Some(&nested)),
                    "The new item did not land in the intended folder");
                let picks = if personal { (Some(id.as_str()), None) } else { (None, Some(id.as_str())) };
                ensure!((ws.qa_sidebar_pick(false), ws.qa_sidebar_pick(true)) == picks, "Creation selected the wrong section");
                if folder { ws.finish_rename(false, window, cx); }
                eprintln!("SIDEBAR_CREATE_ORIGIN_OK personal={personal} folder={folder}");
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-origin-new-document" | "folder-sidebar-origin-new-folder" => {
                use crate::folders::CreateTarget;
                let folder = name.ends_with("new-folder");
                ensure!(!ws.is_shared(), "Expected Home before starting a top-level creation");
                let count = ws.store.list().len();
                std::fs::write(root.join("qa-new-original-count"), count.to_string())?;
                ws.show_view(View::Shared, cx);
                ws.folded[1] = true;
                ws.sharing.folded = false;
                ws.open_new_shared_item(folder, window, cx);
                let title = if folder { "验收共享目录" } else { "验收共享文档" };
                ws.sharing.folder_name.update(cx, |field, cx| field.set_text(title, cx));
                ws.sharing.email.update(cx, |field, cx| field.set_text("participant@example.test", cx));
                std::fs::write(root.join("fail-invite-once"), b"fixture")?;
                std::fs::write(root.join("delay-invite-once"), b"fixture")?;
                ws.submit_sharing(window, cx);
                ensure!(matches!(ws.sharing.sheet, Some(Sheet::CreateNew(_, Some(_)))) && ws.sharing.busy && ws.sharing.busy_feedback.is_none()
                    && ws.store.list().len() == count && !ws.can_create_folder(&CreateTarget::Library(None), cx),
                    "Submitting the invitation exposed a personal item");
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-origin-form-keeps-editor" => {
                ws.open(&document, window, cx);
                let before = ws.editor.as_ref().context("Shared editor is missing")?.entity_id();
                ws.open_new_shared_item(false, window, cx);
                ensure!(matches!(ws.sharing.sheet, Some(Sheet::CreateNew(false, None))) && ws.sharing_allowed(cx)
                    && ws.editor.as_ref().is_some_and(|editor| editor.entity_id() == before),
                    "Opening the shared creation form navigated away");
                ws.close_sharing(window, cx);
                eprintln!("SIDEBAR_NEW_FORM_KEEPS_EDITOR");
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-origin-invite-pending" => {
                let Some(Sheet::CreateNew(folder, Some(_))) = &ws.sharing.sheet else { anyhow::bail!("Creating a share left the original form"); };
                let expected = if *folder { tr("正在新建共享文件夹…", "Creating shared folder…") }
                    else { tr("正在新建共享文档…", "Creating shared document…") };
                ensure!(ws.sharing.busy && ws.sharing.busy_feedback == Some(expected), "Expected feedback for this creation action");
                ensure!(ws.store.list().len().to_string() == std::fs::read_to_string(root.join("qa-new-original-count"))?,
                    "The new share appeared before the invitation succeeded");
                eprintln!("SIDEBAR_INVITE_PROGRESS_IN_PLACE");
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-origin-invite-failed" => {
                let Some(Sheet::CreateNew(_, Some(id))) = &ws.sharing.sheet else { anyhow::bail!("The failure closed the creation form"); };
                ensure!(!ws.sharing.busy && ws.sharing.message.as_ref().is_some_and(|(error, _)| *error), "Expected an editable failed invitation");
                ensure!(!ws.sharing.email.read(cx).text.is_empty() && !ws.sharing.folder_name.read(cx).text.is_empty(), "The failed form lost its input");
                let personal = crate::store::Store::root_dir();
                let room = shared::rooms(&personal)?.into_iter().find(|room| room.scope.note == *id).context("Missing staged share")?;
                ensure!(cloud_sync::activity::row(&personal, room)?.is_none() && ws.shared_list_room(id, cx).is_none()
                    && ws.store.list().len().to_string() == std::fs::read_to_string(root.join("qa-new-original-count"))?,
                    "Failed invitation left a visible blank item");
                std::fs::write(root.join("qa-new-original-id"), id)?;
                eprintln!("SIDEBAR_INVITE_FAILURE_HIDDEN");
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-origin-invite-retry" => {
                ws.sharing.email.update(cx, |field, cx| field.set_text("corrected@example.test", cx));
                ws.submit_sharing(window, cx);
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-origin-invite-success" => {
                let id = std::fs::read_to_string(root.join("qa-new-original-id"))?;
                ensure!(scope.is_some_and(|scope| scope.note == id) && matches!(ws.sharing.sheet, Some(Sheet::Members(_))),
                    "Retry did not open the original staged share");
                let before: usize = std::fs::read_to_string(root.join("qa-new-original-count"))?.parse()?;
                let personal = ws.personal_navigation_entries(cx);
                ensure!(personal.len() == before + 1 && personal.iter().filter(|entry| entry.id == id).count() == 1
                    && ws.shared_list_room(&id, cx).is_some(), "Retry created duplicate library items");
                ensure!(ws.folded[1] && ws.qa_sidebar_pick(false).is_none() && ws.qa_sidebar_pick(true) == Some(id.as_str()),
                    "Submitting the invitation unfolded or selected My Documents");
                ws.close_sharing(window, cx);
                eprintln!("SIDEBAR_INVITE_RETRY_ONE_ITEM");
            }
            #[cfg(target_os = "macos")]
            "folder-sidebar-origin-invite-cancel" => {
                let id = std::fs::read_to_string(root.join("qa-new-original-id"))?;
                ws.close_sharing(window, cx);
                ensure!(ws.sharing.sheet.is_none() && ws.shared_list_room(&id, cx).is_none()
                    && ws.store.list().iter().all(|entry| entry.id != id) && ws.store.list_trash().iter().all(|entry| entry.id != id),
                    "Cancel left a library or Trash item");
                eprintln!("SIDEBAR_INVITE_CANCEL_NO_ITEM");
            }
            "folder-sidebar-ready" => {
                ensure!(scope.is_some_and(|scope| scope.note == owner && scope.owner.is_none()), "Expected the newly shared owner folder");
                ws.close_sharing(window, cx);
                ws.navigate_home(window, cx);
            }
            "folder-sidebar-expanded" => {
                ensure!(ws.folder_current.as_ref() == Some(&owner) && exactly_once(&shared_rows, &nested, 1), "Opening the owner folder did not expand its children");
                ensure!(exactly_once(&personal_rows, &document, 2), "Owner's personal subtree disappeared");
                let personal = ws.personal_navigation_entries(cx);
                let nested = personal.iter().find(|entry| entry.id == nested).context("Missing the owned nested folder")?;
                use crate::sidebar::folder_glyph;
                ensure!(folder_glyph(nested, false, true, cx) == Some("folder-shared") && folder_glyph(nested, true, true, cx) == Some("folder-shared-open"), "My Documents did not identify the folder's inherited share");
                ensure!(folder_glyph(nested, false, false, cx) == Some("folder") && folder_glyph(nested, true, false, cx) == Some("folder-open"), "Shared Documents duplicated the sharing mark");
            }
            "folder-sidebar-opened" | "folder-sidebar-personal-opened" => {
                ensure!(scope.is_some_and(|scope| scope.note == owner) && ws.current.as_ref() == Some(&document), "Click opened the wrong document or library");
                ensure!(!ws.editor.as_ref().context("Missing owner editor")?.read(cx).read_only, "Owner editor is unexpectedly read-only");
                ensure!(exactly_once(&shared_rows, &document, 2) && exactly_once(&personal_rows, &document, 2), "The original was duplicated or missing");
                let (shared_pick, personal_pick) = if name == "folder-sidebar-opened" { (Some(document.as_str()), None) } else { (None, Some(document.as_str())) };
                ensure!(ws.qa_sidebar_pick(true) == shared_pick && ws.qa_sidebar_pick(false) == personal_pick, "Shared and personal selection leaked");
            }
            "folder-sidebar-collapsed" => {
                ensure!(!shared_rows.iter().any(|(id, _)| id == &nested || id == &document), "Shared collapse left descendants visible");
                ensure!(exactly_once(&personal_rows, &document, 2) && ws.current.as_ref() == Some(&document), "Shared collapse affected the personal tree or current document");
            }
            "folder-sidebar-create" => {
                #[cfg(target_os = "macos")]
                ws.create_in_shared_sidebar(ws.current_room(cx).context("Expected owner room")?, nested.clone(),
                    crate::folders::SharedFolderCreation::Document, crate::sidebar::SidebarSection::Shared, window, cx);
                #[cfg(target_os = "ios")]
                ws.create_doc(false, Some(nested.clone()), window, cx);
                let id = ws.current.as_ref().context("New shared document did not open")?;
                ensure!(id != &document && ws.entries.iter().any(|entry| &entry.id == id && entry.meta.parent.as_ref() == Some(&nested)), "New document missed the shared folder");
                std::fs::write(root.join("qa-new-shared-child"), id)?;
            }
            "folder-sidebar-created" => {
                let id = std::fs::read_to_string(root.join("qa-new-shared-child"))?;
                ensure!(exactly_once(&shared_rows, &id, 2), "New shared document did not appear immediately");
            }
            "folder-sidebar-received-open" => {
                ensure!(ws.folder_current.as_ref() == Some(&received) && exactly_once(&shared_rows, &readonly, 1), "Received folder cache did not populate the sidebar");
                ensure!(exactly_once(&shared_rows, &empty, 1) && !ws.qa_sidebar_has_children(&empty), "Empty folder received a chevron");
                ensure!(!personal_rows.iter().any(|(id, _)| id == &received || id == &readonly || id == &empty), "Received folder leaked into My Documents");
            }
            "folder-sidebar-empty" => {
                ensure!(scope.is_some_and(|scope| scope.note == received) && ws.folder_current.as_ref() == Some(&empty), "Empty child did not open in its own room");
                ensure!(!ws.entries.iter().any(|entry| entry.meta.parent.as_ref() == Some(&empty)), "Empty child has unexpected contents");
            }
            "folder-sidebar-overview" => {
                if ws.is_shared() { ws.navigate_personal(View::Shared,None,window,cx); }
                else { ws.show_view(View::Shared,cx); }
            }
            "folder-sidebar-compare-glyphs" => {
                ensure!(!ws.is_shared(), "Expected the personal workspace for folder comparisons");
                let ordinary = ws.store.create_folder("Personal folder fixture", None)?;
                ws.store.create_from_in(&crate::model::Document::default(), Some(&ordinary))?;
                std::fs::write(root.join("qa-ordinary-folder"), &ordinary)?;
                ws.entries = ws.store.list(); ws.sort();
                ws.expanded.remove(&owner);
                ws.sharing.expanded.remove(&owner);
                ws.home.tab = crate::home::HomeTab::Recent;
                ws.home.grid = false;
                ws.set_pref(|p| { p.appearance = "light".into(); p.feishu = true; p.theme = "feishu".into(); p.interface_style = "feishu".into(); }, cx);
                ws.reload_themes(cx); cx.notify();
            }
            "folder-sidebar-open-glyphs" => {
                let ordinary = std::fs::read_to_string(root.join("qa-ordinary-folder"))?;
                ws.expanded.extend([owner.clone(),ordinary]);
                cx.notify();
            }
            "folder-sidebar-shared-batch" => {
                ensure!(ws.shared_batch(), "Shared sidebar did not enter batch selection");
                ws.home.grid = true;
                cx.notify();
            }
            "folder-sidebar-batch-only-documents" => {
                let selected = ws.batch.as_ref().context("Missing shared batch selection")?;
                ensure!(!selected.is_empty() && selected.iter().all(|id| ws.shared_list_room(id, cx).is_some_and(|room| !room.scope.folder)),
                    "Header selected a folder with no visible checkbox");
                eprintln!("SHARED_BATCH_CHECKBOX_SCOPE_OK");
            }
            "folder-sidebar-personal-batch" => {
                ws.begin_personal_batch();
                ws.home.tab = crate::home::HomeTab::Recent;
                cx.notify();
            }
            "folder-sidebar-new-form" | "folder-sidebar-new-folder-form" => {
                let folder = name.contains("folder-form");
                let count = ws.store.list().len();
                ws.new_and_share(folder,window,cx);
                ensure!(matches!(ws.sharing.sheet,Some(Sheet::CreateNew(kind,None)) if kind == folder), "Shared overview form did not open");
                ensure!(ws.store.list().len() == count, "Showing the form prematurely created a personal item");
                let title = if folder { "新共享文件夹" } else { "新共享文档" };
                ws.sharing.folder_name.update(cx, |field,cx| field.set_text(title,cx));
                ws.sharing.email.update(cx, |field,cx| field.set_text("participant@example.test",cx));
                ws.sharing.permission = Access::ReadWrite;
            }
            "folder-sidebar-new-submit" => ws.submit_sharing(window,cx),
            "folder-sidebar-new-document-published" | "folder-sidebar-new-folder-published" => {
                let folder = name.contains("folder-published");
                let room = ws.current_room(cx).context("New item did not open as shared")?;
                ensure!(room.scope.folder == folder && room.scope.owner.is_none(), "New shared item opened with a different role or kind");
                ensure!(ws.personal_navigation_entries(cx).iter().any(|entry| entry.id == room.scope.note), "Shared creation lost the original identity");
                eprintln!("SHARED_NEW_PUBLISHED {folder}");
            }
            "folder-sidebar-accept-form" => {
                ws.open_accept_invitation(window,cx);
                ensure!(ws.accept_invitation_open(), "Expected the independent invitation form");
            }
            "folder-sidebar-toolbar-ready" | "folder-sidebar-toolbar-warning" => {
                ensure!(ws.is_shared() && ws.view == View::Folder, "Expected a shared folder page");
                let warning = name.ends_with("warning");
                cloud_sync::fixture_status(ws.store.root(),if warning { cloud_sync::SyncPhase::Failed } else { cloud_sync::SyncPhase::Ready },cx);
                ensure!(settings::sync_status::needs_attention(ws,cx) == warning, "Wrong shared folder warning state");
                cx.notify();
            }
            "folder-sidebar-document-menu" => {
                ensure!(ws.is_shared() && ws.view == View::Doc && ws.doc_menu && !ws.render_doc_menu(window,cx).is_empty(),
                    "Shared document toolbar did not open the document menu");
                eprintln!("SHARED_DOCUMENT_MENU_OK {:?}",ws.sharing_access(cx));
            }
            "folder-sidebar-toolbar-panel" => {
                ensure!(ws.view == View::Folder && ws.sharing.panel_open && ws.sharing.permissions_only, "Folder sharing shortcut did not open the permissions panel");
                ensure!(ws.render_sharing_toolbar_panel(window,cx).is_some(), "Folder permissions panel was not anchored");
                eprintln!("SHARED_FOLDER_TOOLBAR_PANEL_OK");
            }
            "folder-sidebar-toolbar-sync" => {
                ws.open_shared_sync(window,cx);
                ensure!(ws.sharing.sync_panel_open && ws.render_sharing_toolbar_panel(window,cx).is_some(), "Folder sync details did not use the sharing anchor");
                eprintln!("SHARED_FOLDER_TOOLBAR_SYNC_OK");
            }
            "folder-sidebar-favorite" => {
                ensure!(ws.current.as_ref() == Some(&readonly),"Expected the received child document");
                ws.toggle_star(&readonly,cx);
                ensure!(ws.favorite_entries(cx).iter().filter(|entry| entry.id == readonly).count() == 1,"Received child missing from All Favorites");
                ws.toggle_star(&readonly,cx);
                ensure!(!ws.favorite_entries(cx).iter().any(|entry| entry.id == readonly),"Removed favorite is still shown");
                eprintln!("SHARED_CHILD_FAVORITES_OK");
            }
            "folder-sidebar-dark" => {
                ws.set_pref(|p| p.appearance = "dark".into(), cx);
                ws.reload_themes(cx);
            }
            "folder-sidebar-readonly" => {
                ensure!(scope.is_some_and(|scope| scope.note == received && scope.owner.is_some()) && ws.current.as_ref() == Some(&readonly), "Received child opened outside its room");
                ensure!(ws.editor.as_ref().context("Missing read-only editor")?.read(cx).read_only && !ws.can_create_in_shared_folder(Some(&received), cx), "Received child elevated permissions");
                ensure!(ws.qa_sidebar_pick(true) == Some(readonly.as_str()) && ws.qa_sidebar_pick(false).is_none(), "Received child did not select the correct row");
                eprintln!("SHARED_FOLDER_SIDEBAR_OK nesting=2 immediate=visible owner=editable received=readonly empty=no-chevron selection=isolated");
            }
            _ => anyhow::bail!("Unknown shared folder sidebar step"),
        }
        Ok(())
    })??;
    Ok(())
}
