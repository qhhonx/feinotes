//! Offline parity regressions using disposable personal and shared libraries.
use super::*;
use anyhow::{Context as _, Result, ensure};

pub(crate) fn step(name: &str, cx: &mut App) -> Result<()> {
    ensure!(crate::devtools::scripted() && std::env::var_os("QUIET_SYNC_QA_SHARING_UI").is_some(), "Use isolated sharing fixtures");
    let root = cloud_sync::test_root()?.context("Missing temporary root")?;
    let rooms: Vec<Room> = serde_json::from_slice(&std::fs::read(root.join("qa-session-rooms.json"))?)?;
    let windows = cx.windows().into_iter().filter_map(|handle| handle.downcast::<Workspace>()).collect::<Vec<_>>();
    if name == "parity-window-close" {
        ensure!(windows.len() == 2, "Explicit shared new-window action failed");
        let secondary = windows.iter().find(|handle| handle.read(cx).is_ok_and(|ws| ws.is_shared())).context("Missing shared secondary window")?;
        secondary.update(cx, |ws, window, cx| -> Result<()> {
            ensure!(ws.sharing_access(cx) == Some(Access::ReadOnly), "New shared window lost read-only access");
            ensure!(ws.personal_navigation_entries(cx).iter().any(|entry| entry.meta.pinned), "New shared window lost personal navigation");
            window.remove_window(); Ok(())
        })??;
        return Ok(());
    }
    ensure!(windows.len() == 1, "Navigation unexpectedly opened another window");
    let primary = windows[0];
    primary.update(cx, |_, window, _| crate::devtools::select_script_window(window))?;
    primary.update(cx, |ws, window, cx| -> Result<()> {
        let target = || std::fs::read_to_string(root.join("qa-parity-pinned"));
        match name {
            "parity-setup" => {
                let doc = crate::model::Document { title: crate::model::RichText::plain("PINNED_PERSONAL_TARGET"), blocks: vec![crate::model::Block::text("PERSONAL_ONLY_SEARCH")], ..Default::default() };
                let id = ws.store.create_from(&doc)?; ws.store.update_meta(&id, |meta| meta.pinned = true)?;
                std::fs::write(root.join("qa-parity-pinned"), &id)?;
                let mut store = shared::store_for(&rooms[0], cx)?;
                let mut doc = store.load(&rooms[0].scope.note)?;
                doc.blocks.push(crate::model::Block::text(&"padding ".repeat(100)));
                doc.blocks.push(crate::model::Block::text("SHARED_BODY_TAIL_NEEDLE"));
                store.save(&rooms[0].scope.note, &doc)?;
                let entry = store.list().into_iter().find(|entry| entry.id == rooms[0].scope.note).context("Missing shared fixture")?;
                cloud_sync::activity::update_entry(&rooms[0].scope, &entry, cx);
                ws.entries = ws.store.list(); ws.sort(); ws.folded[0] = false; ws.folded[1] = false; ws.sharing.folded = false;
                ws.home.grid = false; ws.navigate_home(window, cx);
            }
            "parity-pinned-opened" | "parity-pinned-returned" => {
                ensure!(!ws.is_shared() && ws.current.as_deref() == Some(target()?.as_str()), "Pinned click did not open its personal note in the current window");
                ws.open_shared_note(&rooms[0], window, cx);
            }
            "parity-import" => {
                ensure!(ws.is_shared(), "Import routing test must start in a shared document");
                let file = root.join("qa-personal-import.md"); std::fs::write(&file, "# Synthetic personal import")?;
                ws.sidebar_drop = Some(crate::sidebar_drop::SidebarDropAt::top());
                ws.sidebar_files_dropped(vec![file], window, cx);
            }
            "parity-import-cancel" => {
                ensure!(!ws.is_shared() && matches!(ws.confirm, Some(crate::app::Confirm::Import { .. })), "Sidebar import did not resolve the personal library");
                ws.confirm = None; ws.open_shared_note(&rooms[0], window, cx);
            }
            "parity-search" => {
                ensure!(ws.is_shared() && ws.personal_navigation_entries(cx).iter().any(|entry| entry.id == target().unwrap() && entry.meta.pinned), "Personal pinned navigation disappeared in a shared note");
                ws.search.update(cx, |field, cx| field.set_text("SHARED_BODY_TAIL_NEEDLE", cx));
                ensure!(ws.search_results(cx).iter().any(|entry| entry.id == rooms[0].scope.note), "Shared full-body search missed content beyond the summary");
                ws.search.update(cx, |field, cx| field.set_text("PERSONAL_ONLY_SEARCH", cx));
                ensure!(ws.search_results(cx).iter().any(|entry| entry.id == target().unwrap()), "Shared context lost personal search results");
                ws.pick_search(0, window, cx);
            }
            "parity-search-personal" => {
                ensure!(!ws.is_shared() && ws.current.as_deref() == Some(target()?.as_str()), "Search opened the result in the wrong library");
                ws.search.update(cx, |field, cx| field.set_text("SHARED_BODY_TAIL_NEEDLE", cx));
                ensure!(ws.search_results(cx).iter().any(|entry| entry.id == rooms[0].scope.note), "Personal context lost shared search results");
                ws.pick_search(0, window, cx);
            }
            "parity-search-shared" => {
                ensure!(ws.is_shared() && ws.current.as_deref() == Some(&rooms[0].scope.note), "Shared search result used the personal store");
                ws.start_rename(&rooms[0].scope.note, window, cx);
                ws.rename.as_ref().context("Rename field did not open")?.field.update(cx, |field, cx| field.set_text("SHARED_RENAMED_QA", cx));
            }
            "parity-renamed" => {
                ensure!(ws.rename.is_none() && ws.shared_entries(cx).iter().any(|entry| entry.id == rooms[0].scope.note && entry.title == "SHARED_RENAMED_QA"), "Shared rename was not visible immediately");
                ensure!(ws.store.load(&rooms[0].scope.note)?.title.text == "SHARED_RENAMED_QA", "Shared rename was not persisted");
                ws.open_shared_note(&rooms[1], window, cx);
            }
            "parity-readonly" => {
                ws.start_rename(&rooms[1].scope.note, window, cx);
                ensure!(ws.rename.is_none(), "Read-only shared note allowed renaming");
                ws.navigate_personal(View::Shared, None, window, cx);
            }
            "parity-menu" => {
                ensure!(ws.row_menu.as_ref().is_some_and(|menu| menu.id == format!("shared:{}", rooms[0].scope.note)), "Sidebar ellipsis did not open the shared menu");
                ensure!(!ws.render_row_menu(window, cx).is_empty(), "Shared context menu is empty");
            }
            "parity-menu-close" => { ws.row_menu = None; cx.notify(); }
            "parity-window" => {
                shared::open_window(&rooms[1], cx)?;
                crate::devtools::select_script_window(window);
            }
            "parity-list" => {
                ensure!(!ws.is_shared() && ws.home.tab == crate::home::HomeTab::Shared, "Shared overview context changed");
                ensure!(ws.home_entries(cx).len() == 2, "Shared overview has no canonical entries");
                ws.home.grid = false; ws.sharing.folded = false; cx.notify();
            }
            "parity-grid" => { ensure!(ws.home.grid, "Shared overview has no working grid switch"); }
            "parity-organization" => {
                ws.toggle_shared_organization(&rooms[0].scope.note, true, cx); ws.toggle_shared_organization(&rooms[0].scope.note, false, cx);
                let entry = ws.shared_entries(cx).into_iter().find(|entry| entry.id == rooms[0].scope.note).unwrap();
                ensure!(entry.meta.pinned && entry.meta.starred > 0, "Shared organization was not reflected");
                let store = shared::store_for(&rooms[0], cx)?;
                ensure!(!store.clone().list().iter().find(|entry| entry.id == rooms[0].scope.note).unwrap().meta.pinned, "Local pin mutated shared metadata");
                ws.home.tab = crate::home::HomeTab::Starred;
                ensure!(ws.home_entries(cx).iter().any(|entry| entry.id == rooms[0].scope.note), "Favorites omitted shared documents");
                ws.batch = Some(vec![target()?, rooms[0].scope.note.clone()]); ws.sharing.batch = false;
                ws.ask_batch_delete(window, cx);
                ensure!(matches!(&ws.sharing.sheet, Some(Sheet::EndMany(selected, personal)) if selected.len() == 1 && personal.len() == 1), "Mixed Favorites removal did not distinguish personal and shared effects");
                ws.close_sharing(window, cx);
                ws.home.tab = crate::home::HomeTab::Shared;
                ws.home.grid = false; ws.batch = Some(vec![]); ws.sharing.batch = true;
                ws.toggle_check(&rooms[0].scope.note); ws.toggle_check(&rooms[1].scope.note);
                ws.ask_batch_delete(window, cx);
                ensure!(matches!(&ws.sharing.sheet, Some(Sheet::EndMany(selected, personal)) if selected.len() == 2 && personal.is_empty()), "Batch removal lost its role-aware confirmation");
            }
            "parity-cancel-batch" => {
                ws.close_sharing(window, cx);
                ensure!(shared::rooms(&crate::store::Store::root_dir())?.len() == 2, "Cancelling batch removal changed membership");
                ws.batch_copy(cx);
            }
            "parity-copied" => {
                ensure!(!ws.sharing.busy, "Shared copy did not finish");
                let copies = ws.store.list().into_iter().filter(|entry| entry.title.contains("副本") || entry.title.contains("Copy")).collect::<Vec<_>>();
                ensure!(copies.len() == 2 && copies.iter().all(|entry| rooms.iter().all(|room| room.scope.note != entry.id)), "Shared copies were missing or reused source identities");
                ensure!(shared::rooms(&crate::store::Store::root_dir())?.len() == 2, "Copy changed membership");
                ws.batch = Some(vec![rooms[0].scope.note.clone(), rooms[1].scope.note.clone()]); ws.sharing.batch = true;
                ws.ask_batch_delete(window, cx);
                std::fs::write(root.join("fail-end-once"), b"synthetic partial failure")?;
                ws.submit_sharing(window, cx);
            }
            "parity-partial" => {
                ensure!(!ws.sharing.busy && shared::rooms(&crate::store::Store::root_dir())?.len() == 1, "Partial batch removal did not preserve only failed membership");
                ensure!(matches!(&ws.sharing.sheet, Some(Sheet::EndMany(selected, _)) if selected.len() == 1), "Retry still includes already removed documents");
                ensure!(ws.batch.as_ref().is_some_and(|ids| ids.len() == 1), "Failed batch selection was discarded");
                ws.submit_sharing(window, cx);
            }
            "parity-done" => {
                ensure!(!ws.sharing.busy && !ws.is_shared() && shared::rooms(&crate::store::Store::root_dir())?.is_empty(), "Batch removal did not finish");
                ensure!(ws.store.list_trash().len() == 2, "Batch removal did not preserve both recovery copies");
                ensure!(ws.store.load(&target()?)?.plain_text().contains("PERSONAL_ONLY_SEARCH"), "Shared operations changed a personal note");
                eprintln!("SHARING_PARITY_OK pinned=opens-in-both-contexts sidebar=unified search=all-libraries-full-body grid=working organization=local batch=role-aware copy=personal-with-new-ids recovery=retained");
            }
            _ => anyhow::bail!("Unknown parity step"),
        }
        Ok(())
    })??;
    Ok(())
}
