//! UIKit presents the product sharing state; CloudKit requests stay in sharing.rs.
use super::*;

#[derive(serde::Serialize)]
pub(crate) struct NativeSharedOpening {
    message: &'static str,
    loading: bool,
    retry: Option<&'static str>,
    back: &'static str,
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
        else { self.sharing_sheet(Sheet::Access(room),window,cx); }
    }

    pub(crate) fn mobile_share_personal_item(&mut self,id: &str,window: &mut Window,cx: &mut Context<Self>) {
        let entries = self.personal_navigation_entries(cx);
        if !entries.iter().any(|entry|entry.id == id && !entry.meta.local()) { return; }
        if let Some(room) = shared::original::owner_room(&entries,id) {
            if !shared::original::awaiting_publication(&crate::store::Store::root_dir(),&room) {
                self.open_member_management(room,window,cx);
                return;
            }
        }
        if !self.is_shared() { self.create_shared_note(id,window,cx); }
        else if self.mobile_personal_browser(cx) && self.flush(cx) {
            self.sharing_sheet(super::Sheet::Create(id.to_owned()),window,cx);
        }
    }

    pub(crate) fn mobile_sharing_title(&self,cx: &App) -> Option<&'static str> {
        if !self.mobile_contextual_sharing() { return None; }
        Some(match self.sharing.sheet.as_ref() {
            Some(super::Sheet::Create(id)) if self.personal_navigation_entries(cx).iter().any(|e| &e.id == id && e.meta.folder.is_some()) => tr("共享文件夹","Share folder"),
            Some(super::Sheet::Create(_)) => tr("共享文档","Share document"),
            Some(super::Sheet::Invite(_)) => tr("邀请参与者","Invite participant"),
            Some(super::Sheet::CreateNew(folder, _)) => super::new_shared_title(*folder),
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
