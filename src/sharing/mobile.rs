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
                Some(SubscriptionRequired) => (crate::cloud_trial::access_required(), false, true),
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
        if !self.flush(cx) { return; }
        gpui_mobile::ios::ffi::gpui_ios_hide_keyboard(gpui_mobile::ios::ffi::gpui_ios_get_window());
        if self.is_shared() {
            self.mobile_present_sharing(cx);
            self.mobile_shared_settings(window,cx);
        } else if self.store.local_path(&id).is_some() {
            self.show_toast_kind(ui::ToastKind::Info,tr("请先将此文件导入文档库。","Import this file into your library to share it."),cx);
        } else if !self.sharing_allowed(cx) {
            self.mobile_present_sharing(cx);
            self.sharing.message = Some((true,tr("共享暂不可用，请在文档库设置中检查 iCloud。","Sharing is unavailable. Check iCloud in Library settings.").into()));
        } else {
            self.create_shared_note(&id,window,cx);
        }
    }

    pub(crate) fn mobile_present_sharing(&mut self, cx: &mut Context<Self>) {
        if self.view != View::Settings { self.mobile_settings_return = self.view.clone(); }
        self.mobile_settings_root = false;
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
    pub(crate) fn mobile_move_shortcut(&mut self, id: &str, parent: Option<String>, cx: &mut Context<Self>) -> bool {
        if self.is_shared() || self.home.tab != crate::home::HomeTab::Shared { return false; }
        if self.shared_list_room(id,cx).is_none() && !cx.global::<organizer::Organizer>().0.contains_key(id) { return false; }
        if !self.is_shared() { self.update_organizer(cx,|tree|tree.place(id,parent.as_deref())); }
        true
    }

}
