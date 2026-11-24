//! User-confirmed removal retains a recovery copy before changing remote access.
use super::*;

#[derive(Clone, Copy)]
pub(crate) enum SharedTrashAction {
    #[cfg(target_os = "macos")]
    DeleteFolder,
    #[cfg(target_os = "macos")]
    DeleteDocument,
    Restore,
    Purge,
}

impl SharedTrashAction {
    fn apply(self, writer: &mut crate::store::Store, ids: &[String]) -> anyhow::Result<()> {
        let id = ids.first().ok_or_else(|| anyhow::anyhow!("No Trash item selected"))?;
        match self {
            #[cfg(target_os = "macos")]
            Self::DeleteFolder => writer.trash_sqlite_folder(id),
            #[cfg(target_os = "macos")]
            Self::DeleteDocument => writer.delete(id, None),
            Self::Restore => {
                let item = writer.list_trash().into_iter().find(|entry| &entry.id == id)
                    .ok_or_else(|| anyhow::anyhow!("The item is no longer in Trash"))?;
                anyhow::ensure!(item.parent.is_some_and(|parent| writer.list().iter().any(|entry| entry.id == parent)),
                    "{}", tr("请先恢复上级文件夹。", "Restore the parent folder first."));
                writer.restore(id).map(|_| ())
            }
            Self::Purge => shared::purge_folder_trash(writer, ids),
        }
    }

    fn label(self) -> &'static str {
        match self {
            #[cfg(target_os = "macos")]
            Self::DeleteFolder | Self::DeleteDocument => tr("删除成功", "Moved to Trash"),
            Self::Restore => tr("恢复成功", "Restored"),
            Self::Purge => tr("已彻底删除", "Permanently deleted"),
        }
    }
}

impl Workspace {
    #[cfg(target_os = "macos")]
    pub(crate) fn delete_shared_folder(&mut self, room: Room, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if id == room.scope.note {
            if room.scope.owner.is_none() && self.stopping_original(&room, cx) {
                self.end_shared_documents(vec![room], vec![id], window, cx);
            }
        } else {
            self.change_shared_trash(room, id, SharedTrashAction::DeleteFolder, window, cx);
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn ask_delete_shared_descendant(&mut self, room: &Room, id: &str, folder: bool, cx: &mut Context<Self>) {
        self.row_menu = None;
        if id == room.scope.note || !self.shared_sidebar_folder_writable(room, cx) { cx.notify(); return; }
        let entries = if shared::scope(&self.store, cx) == Some(&room.scope) { self.entries.clone() }
            else { self.shared_sidebar_entries(cx) };
        let tree = crate::tree::Tree::new(&entries);
        let Some(entry) = entries.iter().find(|entry| entry.id == id && entry.meta.folder.is_some() == folder
            && tree.subtree(&room.scope.note).iter().any(|child| child == id)) else { cx.notify(); return; };
        let confirm = if folder { crate::app::Confirm::DeleteSharedFolder { room: room.clone(), id: id.into(), title: entry.title.clone() } }
            else { crate::app::Confirm::DeleteSharedDocument { room: room.clone(), id: id.into(), title: entry.title.clone() } };
        self.confirm = Some(confirm);
        cx.notify();
    }

    pub(crate) fn change_shared_trash(&mut self, room: Room, id: String, action: SharedTrashAction, window: &mut Window, cx: &mut Context<Self>) {
        self.change_shared_trash_items(room, vec![id], action, window, cx);
    }

    pub(crate) fn change_shared_trash_items(&mut self, room: Room, ids: Vec<String>, action: SharedTrashAction, window: &mut Window, cx: &mut Context<Self>) {
        if !room.scope.folder || ids.iter().any(|id| id == &room.scope.note) { return; }
        if self.sharing.busy {
            self.show_toast_kind(ui::ToastKind::Info, tr("正在处理共享项目，请稍后再试。", "A shared operation is in progress. Try again shortly."), cx);
            return;
        }
        let (retry_room, retry_ids) = (room.clone(), ids.clone());
        if !self.save_before(cx, move |w, window, cx| w.change_shared_trash_items(retry_room, retry_ids, action, window, cx)) { return; }
        let workspace = cx.entity_id();
        let window_id = window.window_handle().window_id();
        let handle = WindowHandle::<Self>::new(window_id);
        let requested = room.clone();
        let loaded = move |store: Result<crate::shared_store::SharedStore, String>, cx: &mut App| {
            let _ = handle.update(cx, |ws, _, cx| {
                if cx.entity_id() != workspace { return; }
                match store {
                    Ok(store) => ws.apply_shared_trash(requested, ids, action, store, window_id, cx),
                    Err(error) => ws.show_toast_kind(ui::ToastKind::Error, &settings::diagnostic(&error), cx),
                }
            });
        };
        #[cfg(target_os = "macos")]
        if let Err(error) = shared::load_sidebar_store(room, cx, Box::new(loaded)) {
            self.show_toast_kind(ui::ToastKind::Error, &settings::diagnostic(&error.to_string()), cx);
        }
        #[cfg(target_os = "ios")]
        {
            let store = shared::store_for(&room, cx).map_err(|error| error.to_string());
            cx.defer(move |cx| loaded(store, cx));
        }
    }

    fn apply_shared_trash(&mut self, room: Room, ids: Vec<String>, action: SharedTrashAction,
        store: crate::shared_store::SharedStore, _window_id: WindowId, cx: &mut Context<Self>) {
        let prepare = (|| -> anyhow::Result<_> {
            let store = shared::check_opened_store(store, &room, cx)?;
            anyhow::ensure!(shared::access(&store, cx).is_some_and(Access::can_write) && !shared::editing_paused(&store, cx),
                "{}", tr("此共享文件夹当前不可写入。", "You cannot write to this shared folder right now."));
            #[cfg(target_os = "macos")]
            if matches!(action, SharedTrashAction::DeleteFolder | SharedTrashAction::DeleteDocument) {
                let listing = store.clone().list();
                let tree = crate::tree::Tree::new(&listing);
                let folder = matches!(action, SharedTrashAction::DeleteFolder);
                let room_items = tree.subtree(&room.scope.note);
                let id = ids.first().filter(|id| room_items.contains(id) && listing.iter()
                    .any(|entry| &entry.id == *id && entry.meta.folder.is_some() == folder))
                    .ok_or_else(|| anyhow::anyhow!("The shared item no longer exists"))?;
                let affected = tree.subtree(id);
                if self.store.root() == store.root() && self.current.as_ref().is_some_and(|id| affected.contains(id)) {
                    anyhow::ensure!(self.flush(cx), "{}", tr("请先保存正在编辑的文档。", "Save the open document before deleting."));
                }
                anyhow::ensure!(!cx.windows().into_iter().filter(|handle| handle.window_id() != _window_id)
                    .filter_map(|handle| handle.downcast::<Workspace>())
                    .filter_map(|window| window.read(cx).ok()).any(|ws|
                        ws.store.root() == store.root() && ws.current.as_ref().is_some_and(|id| affected.contains(id))),
                    "{}", tr("请先关闭正在编辑这些文档的其他窗口，再删除。", "Close other windows editing these documents before deleting."));
            }
            let guard = store.begin_file_task().ok_or_else(|| anyhow::anyhow!(
                "{}", tr("正在处理文件，请稍后再试。", "Another file operation is in progress. Try again shortly.")))?;
            Ok((store, guard))
        })();
        let (store, guard) = match prepare {
            Ok(ready) => ready,
            Err(error) => { self.show_toast_kind(ui::ToastKind::Error, &settings::diagnostic(&error.to_string()), cx); return; }
        };
        let root = store.root().to_path_buf();
        let personal = crate::store::Store::root_dir();
        let Some((_, mut writer)) = store.tree_writer(None) else {
            self.show_toast_kind(ui::ToastKind::Error, tr("暂时无法打开共享文档库。", "Could not open the shared library."), cx);
            return;
        };
        let scope = room.scope;
        let written_scope = scope.clone();
        let written_root = root.clone();
        let revision = cloud_sync::account_revision();
        let task = cx.background_executor().spawn(async move {
            cloud_sync::check_account_revision(revision)?;
            action.apply(&mut writer, &ids)?;
            let projection = shared::original::refresh_owner(&written_root, &written_scope, None);
            Ok::<_, anyhow::Error>((writer.list(), projection))
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            drop(guard);
            if let Ok((listing, _)) = &result {
                let _ = crate::gpui_compat::async_update(cx, |cx| {
                    if personal != crate::store::Store::root_dir() || cloud_sync::account_revision() != revision { return; }
                    cloud_sync::activity::update_folder_items(&scope, listing, cx);
                    crate::app::cloud_notes_changed(&root, &[], false, cx);
                    if scope.owner.is_none() { crate::app::cloud_notes_changed(&personal, &[], false, cx); }
                    cloud_sync::recheck_account(&root, cx);
                });
            }
            let _ = this.update(cx, |w, cx| match result.and_then(|(_, projection)| projection) {
                Ok(()) => w.show_toast(action.label(), cx),
                Err(error) => w.show_toast_kind(ui::ToastKind::Error, &settings::diagnostic(&error.to_string()), cx),
            });
            drop(store);
        }).detach();
    }

    pub(crate) fn open_end_shared(&mut self, room: Room, window: &mut Window, cx: &mut Context<Self>) {
        self.sharing_sheet(Sheet::End(room), window, cx);
    }

    pub(super) fn end_shared_document(&mut self, room: Room, window: &mut Window, cx: &mut Context<Self>) {
        self.end_shared_documents(vec![room], vec![], window, cx);
    }

    pub(super) fn end_shared_documents(&mut self, rooms: Vec<Room>, personal_ids: Vec<String>, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy || rooms.is_empty() || !self.sharing_account_ready(cx) { return; }
        let retry = rooms.clone(); let retry_personal = personal_ids.clone();
        if !self.save_before(cx, move |w, window, cx| w.end_shared_documents(retry, retry_personal, window, cx)) { return; }
        let bulk = matches!(self.sharing.sheet, Some(Sheet::EndMany(..)));
        let epoch = self.begin_sharing_request(cx);
        let personal = crate::store::Store::root_dir();
        let rooms: Vec<_> = rooms.into_iter().map(|room| {
            let lock = shared::prepare_end(&personal, &room, cx); (room, lock)
        }).collect();
        cx.spawn_in(window, async move |this, cx| {
            let root = personal.clone();
            let results = cx.background_executor().spawn(async move {
                rooms.into_iter().map(|(room, lock)| { let result = lock.and_then(|lock| shared::end(&root, &room, lock)); (room, result) }).collect::<Vec<_>>()
            }).await;
            let _ = this.update_in(cx, |w, window, cx| {
                if !w.finish_sharing_request(epoch, None, cx) { return; }
                let mut failed = vec![]; let mut failed_rooms = vec![]; let mut errors = vec![]; let mut removed_current = false;
                for (room, result) in results {
                    match result {
                        Ok(()) => {
                            removed_current |= shared::room_root(&personal, &room.scope.note).is_ok_and(|root| w.store.root() == root);
                            if let Ok(root) = shared::room_root(&personal, &room.scope.note)
                                && let Err(error) = cloud_sync::shared_access_ended(&root, &room.scope.note, cx) {
                                crate::logging::write(&format!("Shared removal local access state: {error:#}"));
                            }
                        }
                        Err(error) => { let _ = shared::store_for(&room, cx); failed.push(room.scope.note.clone()); failed_rooms.push(room); errors.push(settings::diagnostic(&error.to_string())); }
                    }
                }
                if !errors.is_empty() {
                    if removed_current {
                        let message = errors.join("；"); w.close_sharing(window, cx);
                        w.personal_action(window, cx, move |w, window, cx| {
                            w.show_view(View::Shared, cx); w.batch = Some(failed); w.sharing.batch = true;
                            w.sharing_sheet(Sheet::EndMany(failed_rooms, personal_ids), window, cx); w.sharing.message = Some((true, message)); cx.notify();
                        });
                    } else {
                        if bulk { w.batch = Some(failed); w.sharing.batch = true; w.sharing.sheet = Some(Sheet::EndMany(failed_rooms, personal_ids)); }
                        w.sharing.message = Some((true, errors.join("；"))); cx.notify();
                    }
                    return;
                }
                w.batch = None; w.close_sharing(window, cx); cloud_sync::retry(&personal, cx);
                if personal_ids.is_empty() { w.navigate_personal(View::Shared, None, window, cx); }
                else { w.personal_action(window, cx, move |w, window, cx| {
                    w.entries = w.store.list();
                    w.delete_docs(&personal_ids, window, cx);
                }); }
            });
        }).detach();
    }
}
