//! UIKit presents the product sharing state; CloudKit requests stay in sharing.rs.
use super::*;

#[derive(Clone, serde::Serialize)]
pub(crate) struct SharingToast {
    pub(super) id: uuid::Uuid,
    kind: &'static str,
    message: String,
}

impl SharingToast {
    pub(super) fn new(id: uuid::Uuid, kind: ui::ToastKind, message: &str) -> Self {
        let kind = match kind {
            ui::ToastKind::Success => "success",
            ui::ToastKind::Error => "error",
            ui::ToastKind::Info => "info",
        };
        Self { id, kind, message: message.into() }
    }
}

#[derive(serde::Serialize)]
pub(crate) struct NativeSharedOpening {
    message: &'static str,
    loading: bool,
    retry: Option<&'static str>,
    back: &'static str,
}

#[derive(Clone, Copy)]
pub(super) enum AccessLookup {
    Idle,
    Pending,
    Verified { access: Access, revision: u64 },
    Failed,
}

impl Workspace {
    pub(crate) fn mobile_shared_opening(&self, cx: &App) -> Option<NativeSharedOpening> {
        self.sharing.opening.as_ref().filter(|_| self.view == View::Home)?;
        use cloud_sync::SyncPhase::*;
        let status = cloud_sync::status(self.store.root(), cx);
        let (message, loading, retry) = if shared::access(&self.store, cx) == Some(Access::Revoked) {
            (tr("共享访问已结束", "Access to this shared document has ended"), false, false)
        } else {
            match status.map(|status| status.phase) {
                Some(AccountChanged | SignedOut) => (tr("请在文档库设置中重新连接 iCloud。", "Reconnect to iCloud in Library settings."), false, false),
                Some(SubscriptionRequired) => (shared::sharing_unavailable(), false, true),
                Some(TestFlightVerification) => (tr("请确认 TestFlight 安装状态后重试。", "Check the TestFlight installation and try again."), false, true),
                Some(Offline) => (tr("无法连接 iCloud。请检查网络后重试。", "Could not connect to iCloud. Check your network and try again."), false, true),
                Some(Account) => (tr("暂时无法确认 iCloud 账户。请重试。", "Could not verify your iCloud account. Try again."), false, true),
                Some(Downloading | Applying | Uploading) if status.is_some_and(|status| status.stalled.is_some()) =>
                    (tr("接收共享文档停滞。请重试。", "Receiving the shared document has stalled. Try again."), false, true),
                Some(Connecting | AccountChecking | Downloading | Applying | Uploading) =>
                    (tr("正在接收共享文档", "Receiving shared document"), true, false),
                _ => (tr("暂时无法接收共享文档。请重试。", "Could not receive the shared document. Try again."), false, true),
            }
        };
        Some(NativeSharedOpening { message, loading, retry: retry.then(|| tr("重试", "Retry")),
            back: tr("返回列表", "Back to list") })
    }

    pub(crate) fn complete_shared_open(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(target) = self.sharing.opening.as_ref().filter(|_| self.view == View::Home).map(|opening| opening.target.clone()) else { return false; };
        if !matches!(shared::access(&self.store, cx), Some(Access::Owner | Access::ReadWrite | Access::ReadOnly))
            || shared::editing_paused(&self.store, cx) || !self.try_open(&target, window, cx) { return false; }
        self.sharing.opening = None;
        true
    }

    pub(crate) fn retry_shared_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mobile_shared_opening(cx).is_some_and(|opening| opening.retry.is_some())
            && !self.complete_shared_open(window, cx) {
            cloud_sync::retry(self.store.root(), cx);
        }
    }

    pub(crate) fn mobile_share_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.current.clone() else { return; };
        if self.is_shared() { return self.mobile_open_shared_access(window, cx); }
        if !self.flush(cx) { return; }
        gpui_mobile::ios::ffi::gpui_ios_hide_keyboard(gpui_mobile::ios::ffi::gpui_ios_get_window());
        if self.store.local_path(&id).is_some() {
            self.show_toast_kind(ui::ToastKind::Info,tr("请先将此文件导入文档库。","Import this file into your library to share it."),cx);
        } else if !self.sharing_allowed(cx) {
            self.mobile_present_sharing(cx);
            self.sharing.message = Some((true,shared::sharing_unavailable().into()));
        } else {
            self.create_shared_note(&id,window,cx);
        }
    }

    pub(crate) fn mobile_open_shared_access(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_shared() || !self.save_before(cx, |w, window, cx| w.mobile_open_shared_access(window, cx)) { return; }
        gpui_mobile::ios::ffi::gpui_ios_hide_keyboard(gpui_mobile::ios::ffi::gpui_ios_get_window());
        if let Some(room) = self.current_room(cx) {
            self.mobile_open_room_access(room,window,cx);
        }
    }

    pub(crate) fn mobile_open_room_access(&mut self, room: Room, window: &mut Window, cx: &mut Context<Self>) {
        if room.scope.owner.is_none() { self.open_member_management(room,window,cx); }
        else if !self.sharing.busy {
            let needs_lookup = shared::access_summary(&room,cx).1.is_none();
            self.sharing_sheet(Sheet::Access(room),window,cx);
            if needs_lookup { self.mobile_refresh_room_access(window,cx); }
        }
    }

    pub(crate) fn mobile_refresh_room_access(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Sheet::Access(room)) = self.sharing.sheet.clone() else { return; };
        if matches!(self.sharing.mobile_access_lookup, AccessLookup::Pending) || room.scope.owner.is_none() { return; }
        let personal = crate::store::Store::root_dir();
        let revision = cloud_sync::account_revision();
        let epoch = self.sharing.epoch;
        self.sharing.mobile_access_lookup = AccessLookup::Pending;
        self.sharing.message = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let target = room.clone();
            let result = cx.background_executor().spawn(async move { shared::inspect_library_room_access(&personal, &target, revision) }).await;
            let _ = this.update_in(cx, |w, _, cx| {
                if w.sharing.epoch != epoch || !matches!(&w.sharing.sheet, Some(Sheet::Access(current)) if current.scope == room.scope) { return; }
                match cloud_sync::check_account_revision(revision).and(result) {
                    Ok(access) => w.sharing.mobile_access_lookup = AccessLookup::Verified { access, revision },
                    Err(error) => {
                        w.sharing.mobile_access_lookup = AccessLookup::Failed;
                        w.sharing.message = Some((true,settings::diagnostic(&error.to_string())));
                    }
                }
                cx.notify();
            });
        }).detach();
    }

    pub(crate) fn mobile_room_access_summary(&self, room: &Room, cx: &App) -> (Option<SharedString>, Option<Access>, bool) {
        let (owner,cached) = shared::access_summary(room,cx);
        let access = match self.sharing.mobile_access_lookup {
            AccessLookup::Verified { access, revision } if revision == cloud_sync::account_revision() => Some(access),
            _ => cached,
        };
        (owner,access,matches!(self.sharing.mobile_access_lookup, AccessLookup::Failed))
    }

    pub(crate) fn mobile_share_personal_item(&mut self,id: &str,window: &mut Window,cx: &mut Context<Self>) {
        let entries = self.personal_navigation_entries(cx);
        if !entries.iter().any(|entry|entry.id == id && !entry.meta.local()) { return; }
        if let Some(room) = shared::original::owner_room(&entries,id) {
            if !shared::original::awaiting_publication(&crate::store::Store::root_dir(),&room) {
                self.open_member_management(room,window,cx);
            } else if (!self.is_shared() || self.mobile_personal_browser(cx)) && self.flush(cx) {
                self.resume_original_sharing(room,window,cx);
            }
            return;
        }
        if !self.is_shared() { self.create_shared_note(id,window,cx); }
        else if self.mobile_personal_browser(cx) && self.flush(cx) {
            self.sharing_sheet(super::Sheet::Create(id.to_owned()),window,cx);
        }
    }

    pub(crate) fn mobile_sharing_title(&self,cx: &App) -> Option<&'static str> {
        if !self.mobile_contextual_sharing() { return None; }
        if self.sharing.mobile_invitation_result && matches!(self.sharing.sheet, Some(Sheet::Members(_))) {
            return Some(tr("邀请已创建","Invitation created"));
        }
        Some(match self.sharing.sheet.as_ref() {
            Some(super::Sheet::Create(id)) => {
                let entries = self.personal_navigation_entries(cx);
                match entries.iter().find(|entry| &entry.id == id) {
                    Some(entry) if entry.meta.folder.is_some() => tr("共享文件夹","Share folder"),
                    Some(entry) if entry.meta.kind == crate::store::DocKind::Board => tr("共享画板","Share board"),
                    _ => tr("共享文档","Share document"),
                }
            }
            Some(super::Sheet::Invite(_)) => tr("邀请参与者","Invite participant"),
            Some(super::Sheet::CreateNew(kind, _)) => super::new_shared_title(*kind),
            Some(super::Sheet::Accept) => tr("接受邀请","Accept invitation"),
            Some(super::Sheet::End(room)) => self.shared_end_label(room,cx),
            Some(super::Sheet::EndMany(..)) => tr("移除共享文档","Remove shared documents"),
            Some(super::Sheet::MoveInto(..)) => tr("移入共享文件夹","Move into shared folder"),
            Some(super::Sheet::RenameCollection(_)) => tr("重命名个人文件夹","Rename personal folder"),
            _ => super::shared_access_title(self.sheet_room().or(self.current_room(cx).as_ref()).is_some_and(|room|room.scope.folder)),
        })
    }

    pub(crate) fn mobile_present_sharing(&mut self, cx: &mut Context<Self>) {
        if self.view != View::Settings { self.mobile_settings_return = self.view.clone(); }
        self.mobile_settings_root = false;
        self.mobile_settings_parent = None;
        self.open_settings(settings::Page::Shared,cx);
    }
    pub(crate) fn mobile_shared_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(room) = self.current_room(cx).filter(|room| room.scope.owner.is_none()) {
            self.open_member_management(room, window, cx);
        }
        cx.notify();
    }
}

impl Workspace {
    pub(crate) fn mobile_collection_exists(&self,id:&str,cx:&App) -> bool {
        cx.global::<organizer::Organizer>().0.get(id).is_some_and(|p|p.title.is_some())
    }
    pub(crate) fn mobile_collection(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        if !self.mobile_collection_exists(id,cx) { return false; }
        self.sharing.collection = Some(id.into()); self.home.tab = crate::home::HomeTab::Shared;
        self.show_view(View::Home, cx); true
    }
    pub(crate) fn mobile_collection_up(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(id) = self.sharing.collection.take() else { return false; };
        self.sharing.collection = cx.global::<organizer::Organizer>().0.get(&id).and_then(|p|p.parent.clone());
        cx.notify(); true
    }
    pub(crate) fn mobile_collection_entries(&self, cx: &App) -> Vec<crate::store::Entry> { cx.global::<organizer::Organizer>().folders().collect() }
    pub(crate) fn mobile_collection_path(&self, cx: &App) -> Vec<(String,String)> {
        let tree=cx.global::<organizer::Organizer>(); let mut current=self.sharing.collection.clone(); let mut out=vec![];
        while let Some(id)=current { if out.iter().any(|(seen,_)|seen==&id) { break; } let Some(row)=tree.0.get(&id) else {break;};
            out.push((id,row.title.clone().unwrap_or_default())); current=row.parent.clone(); }
        out.reverse();out
    }
    pub(crate) fn mobile_place_shared_entry(&mut self,id:&str,parent:Option<&str>,cx:&mut Context<Self>) {
        if self.is_shared() && !self.mobile_personal_browser(cx) { return; }
        if self.shared_list_room(id,cx).is_some() || self.mobile_collection_exists(id,cx) {
            self.update_organizer(cx,|tree|tree.place(id,parent));
        }
    }

}

impl Workspace {
    pub(crate) fn mobile_menu_room_writable(&self, room: &Room, cx: &App) -> bool {
        if let Some(writable) = shared::live_room_writable(room, cx) { return writable; }
        cx.try_global::<cloud_sync::activity::ActivityView>().and_then(|view| view.0.iter().find(|row| row.room.scope == room.scope))
            .is_some_and(|row| row.access.map_or(room.scope.owner.is_none(), Access::can_write))
    }

    fn mobile_item_room(&self, entry: &crate::store::Entry, cx: &App) -> Option<Room> {
        self.shared_list_room(&entry.id, cx).or_else(|| {
            self.current_room(cx).filter(|room| (room.scope.folder || room.scope.note == entry.id)
                && self.entries.iter().any(|row| row.id == entry.id))
        }).or_else(|| {
            let root = entry.meta.share_root.clone().or_else(|| self.shared_sidebar_entries(cx).into_iter()
                .find(|row| row.id == entry.id).and_then(|row| row.meta.share_root))?;
            self.shared_list_room(&root, cx).filter(|room| room.scope.folder || room.scope.note == entry.id)
        })
    }

    pub(crate) fn mobile_item_menu(&mut self, id: &str, command: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.mobile_visible_entries(cx).into_iter().find(|row| row.id == id) else { return; };
        if id.starts_with("collection-") || entry.meta.local() { return; }
        let (verb, value) = command.split_once(':').unwrap_or((command, ""));
        let folder = entry.meta.folder.is_some();
        let item = id.to_owned();
        let value = value.trim().to_owned();
        let Some(room) = self.mobile_item_room(&entry, cx) else {
            if !self.personal_navigation_entries(cx).iter().any(|row| row.id == id) { return; }
            if verb == "share" {
                if self.personal_row_can_share(id, cx) { self.mobile_share_personal_item(id, window, cx); }
                return;
            }
            if entry.meta.share_root.is_some() {
                if verb == "delete" && !folder && entry.meta.share_root.as_deref() == Some(id)
                    && let Some(room) = shared::original::owner_room(&self.personal_navigation_entries(cx), id)
                    && shared::original::needs_standalone_check(&crate::store::Store::root_dir(), &room) {
                    self.reconcile_standalone_original(room, window, cx, move |ws, restored, window, cx| {
                        if restored {
                            ws.personal_action(window, cx, move |w, _, cx| w.ask_delete(&item, cx));
                        } else {
                            ws.show_sharing_toast(ui::ToastKind::Info, tr("此文档仍在共享。请先停止共享，再删除。",
                                "This document is still shared. Stop sharing before deleting it."), cx);
                        }
                    });
                }
                return;
            }
            if matches!(verb, "pin" | "star") && !folder {
                self.toggle_personal_organization(id, verb == "pin", false, cx);
                return;
            }
            let verb = verb.to_owned();
            self.personal_action(window, cx, move |w, window, cx| {
                if !w.entries.iter().any(|row| row.id == item && row.meta.share_root.is_none()) { return; }
                if folder && verb == "duplicate" {
                    let tree = crate::tree::Tree::new(&w.entries);
                    let subtree = tree.subtree(&item);
                    if w.entries.iter().any(|row| subtree.contains(&row.id) && (row.meta.share_root.is_some()
                        || row.meta.kind == crate::store::DocKind::Table || row.meta.local())) { return; }
                }
                w.apply_mobile_menu_item(&item, folder, &verb, &value, None, window, cx);
            });
            return;
        };
        let root = room.scope.note == id;
        match verb {
            "access" => self.mobile_open_room_access(room, window, cx),
            "end" if root => self.open_end_shared(room, window, cx),
            "copy" if root && !folder => self.copy_shared_documents(vec![item], cx),
            "link" => self.copy_object_link(id, Some(room), false, cx),
            "pin" | "star" if !folder => {
                if root { self.toggle_shared_organization(id, verb == "pin", cx); }
                else { self.toggle_shared_child_organization(id, verb == "pin", cx); }
            }
            "delete" if root && folder && self.stopping_original(&room, cx) && self.mobile_menu_room_writable(&room, cx) => {
                self.confirm = Some(crate::app::Confirm::DeleteSharedFolder { title: room.title.clone(), room, id: item });
                cx.notify();
            }
            "delete" if !root => self.ask_delete_shared_descendant(&room, id, folder, cx),
            "export" | "pdf" | "print" | "rename" | "duplicate" | "move" | "new-document" | "new-board" | "new-folder" => {
                let write = !matches!(verb,"export" | "pdf" | "print");
                if write && !self.mobile_menu_room_writable(&room, cx) { return; }
                let scope = room.scope.clone();
                let verb = verb.to_owned();
                let already_open = shared::scope(&self.store, cx) == Some(&scope);
                let action = move |w: &mut Workspace, id: &str, window: &mut Window, cx: &mut Context<Workspace>| {
                    if shared::scope(&w.store, cx) != Some(&scope) || (write && shared::editing_paused(&w.store, cx))
                        || !w.sharing_access(cx).is_some_and(|access| if write { access.can_write() } else { access != Access::Revoked })
                        || !w.entries.iter().any(|entry| entry.id == id && entry.meta.folder.is_some() == folder) { return; }
                    w.apply_mobile_menu_item(id, folder, &verb, &value, Some(&scope), window, cx);
                };
                if already_open { action(self, &item, window, cx); }
                else {
                    let callback: super::actions::SharedAction = std::rc::Rc::new(std::cell::RefCell::new(Some(Box::new(action))));
                    self.open_shared_item_for_action(room, item, None, callback, window, cx);
                }
            }
            _ => {}
        }
    }

    fn apply_mobile_menu_item(&mut self, id: &str, folder: bool, verb: &str, value: &str, scope: Option<&shared::Scope>,
        window: &mut Window, cx: &mut Context<Self>) {
        match verb {
            "rename" if !value.is_empty() => {
                self.start_rename(id, window, cx);
                if let Some(rename) = &self.rename {
                    if rename.id != id { return; }
                    rename.field.update(cx, |field,cx| field.set_text(value,cx));
                    self.finish_rename(true,window,cx);
                }
            }
            "new-document" if folder => self.create_doc(false, Some(id.into()), window, cx),
            "new-board" if folder => self.create_board(Some(id.into()), window, cx),
            "new-folder" if folder && !value.is_empty() => self.new_folder(
                crate::folders::CreateTarget::Library(Some(id.into())), Some(value.into()), window, cx),
            "duplicate" if scope.is_none_or(|scope| scope.folder && !folder && scope.note != id) => self.duplicate_doc(id, window, cx),
            "move" => {
                let mut destination = (!value.is_empty()).then(||value.to_owned());
                if let Some(scope) = scope {
                    let tree = crate::tree::Tree::new(&self.entries);
                    let members = tree.subtree(&scope.note);
                    let target = destination.get_or_insert_with(|| scope.note.clone());
                    if !scope.folder || scope.note == id || !members.iter().any(|id_in_room| id_in_room == id)
                        || !members.contains(target) || tree.subtree(id).contains(target)
                        || !self.entries.iter().any(|entry| &entry.id == target && entry.meta.folder.is_some()) { return; }
                }
                self.move_docs(&[id.into()], destination, cx);
            }
            "delete" if scope.is_none() => self.ask_delete(id, cx),
            "pin" if !folder => self.toggle_pin(id, cx),
            "star" if !folder => self.toggle_star(id, cx),
            "link" if scope.is_none() => self.copy_object_link(id, None, true, cx),
            "export" if !folder => self.mobile_export(id, value == "all", cx),
            "pdf" if !folder => self.export_pdf(id, window, cx),
            "print" if !folder => self.print_doc(id, window, cx),
            _ => {}
        }
    }
}
