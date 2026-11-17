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
        "check-shared-add" => primary.update(cx, |w, _, _| -> Result<()> {
            ensure!(matches!(w.create_menu, Some((crate::folders::CreateTarget::Shared, _))), "Sidebar plus did not open the shared menu"); Ok(())
        })??,
        "check-sidebar-invite" => primary.update(cx, |w, _, _| -> Result<()> {
            ensure!(w.accept_invitation_open() && w.sharing.accept_at == Some("accept-sidebar"), "Sidebar menu did not open the invitation form"); Ok(())
        })??,
        "collections" => primary.update(cx, |ws, window, cx| -> Result<()> {
            ws.show_view(View::Shared, cx);
            ws.personal_collection(None, window, cx);
            ws.sharing.folder_name.update(cx, |field, cx| field.set_text("客户资料", cx));
            ws.submit_sharing(window, cx);
            let parent = organizer::Organizer::load()?.folders().find(|e| e.title == "客户资料").context("Missing personal collection")?.id;
            ws.personal_collection(Some(parent.clone()), window, cx);
            ws.sharing.folder_name.update(cx, |field, cx| field.set_text("十一月", cx));
            ws.submit_sharing(window, cx);
            let child = organizer::Organizer::load()?.folders().find(|e| e.meta.parent == Some(parent.clone())).context("Missing nested collection")?.id;
            let room = shared::rooms(&personal)?.into_iter().next().context("Missing room")?;
            ensure!(ws.update_organizer(cx, |tree| tree.place(&room.scope.note, Some(&child))), "Could not file the shared document");
            ws.expanded.insert(parent.clone()); ws.expanded.insert(child); ws.sharing.collection = Some(parent); ws.sharing.folded = false; cx.notify();
            let folder = ws.store.create_folder("共同项目", None)?;
            let nested = ws.store.create_folder("会议", Some(&folder))?;
            let doc = ws.store.create_from(&crate::model::Document { title: crate::model::RichText::plain("议程"), ..Default::default() })?;
            ws.store.set_parent(&doc, Some(&nested))?; ws.entries = ws.store.list();
            std::fs::write(root.join("qa-team-folder"), &folder)?;
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
            ws.create_doc(false, Some(child.clone()), window, cx);
            ensure!(ws.current.as_ref().is_some_and(|id| ws.entries.iter().any(|e| &e.id == id && e.meta.parent.as_ref() == Some(&child))), "New document missed its shared parent");
            ws.open(&child, window, cx);
            eprintln!("SHARED_FOLDERS_UI_OK"); Ok(())
        })??,
        "sidebar-setup" => primary.update(cx, |ws, window, cx| -> Result<()> {
            let original = ws.current.clone().context("Missing personal fixture")?;
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
            let rooms: Vec<Room> = serde_json::from_slice(&std::fs::read(root.join("qa-session-rooms.json"))?)?;
            ws.navigate_shared(rooms[0].clone(), None, window, cx); Ok(())
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
            std::fs::write(root.join("qa-reordered-sidebar"), crate::store::Store::sqlite_setting("shared-sidebar-order")?.context("Missing shared order")?)?;
            let after = ws.store.list();
            ensure!(serde_json::to_vec(&before.iter().map(|e| (&e.id, &e.meta)).collect::<Vec<_>>())? == serde_json::to_vec(&after.iter().map(|e| (&e.id, &e.meta)).collect::<Vec<_>>())?, "Shared sorting modified personal metadata");
            Ok(())
        })??,
        "shared-gesture" => primary.update(cx, |ws, _, _| -> Result<()> {
            let mut expected: Vec<String> = serde_json::from_slice(&std::fs::read(root.join("qa-reordered-sidebar"))?)?; expected.reverse();
            let actual: Vec<String> = serde_json::from_slice(&crate::store::Store::sqlite_setting("shared-sidebar-order")?.context("Missing order after pointer gesture")?)?;
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
