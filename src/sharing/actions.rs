//! Organization and explicit cross-library actions for shared navigation.
use super::*;

type SharedAction = Box<dyn FnOnce(&mut Workspace, &str, &mut Window, &mut Context<Workspace>)>;

impl Workspace {
    pub(crate) fn with_shared_document(&mut self, room: Room, window: &mut Window, cx: &mut Context<Self>, action: impl FnOnce(&mut Self, &str, &mut Window, &mut Context<Self>) + 'static) {
        self.shared_action_boxed(room, window, cx, Box::new(action));
    }

    fn shared_action_boxed(&mut self, room: Room, window: &mut Window, cx: &mut Context<Self>, action: SharedAction) {
        if self.sharing.busy { return; }
        let id = room.scope.note.clone(); let window_id = window.window_handle().window_id();
        let held = std::rc::Rc::new(std::cell::RefCell::new(Some(action))); let retry = held.clone(); let retry_room = room.clone();
        if !self.save_before(cx, move |w, window, cx| {
            if let Some(action) = retry.borrow_mut().take() { w.shared_action_boxed(retry_room, window, cx, action); }
        }) { return; }
        let action = held.borrow_mut().take().unwrap();
        self.navigate_shared(room, None, window, cx);
        cx.defer(move |cx| {
            if let Some(handle) = cx.windows().into_iter().find(|handle| handle.window_id() == window_id).and_then(|handle| handle.downcast::<Workspace>()) {
                let _ = handle.update(cx, |w, window, cx| { if w.is_shared() && (w.current.as_ref() == Some(&id) || w.folder_current.as_ref() == Some(&id)) { action(w, &id, window, cx); } });
            }
        });
    }

    pub(crate) fn open_shared_window(&mut self, room: Room, window: &Window, cx: &mut Context<Self>) {
        #[cfg(target_os = "ios")]
        { let _ = window; shared::launch(&room,cx).detach(); self.row_menu = None; }
        #[cfg(target_os = "macos")]
        {
        let window_id = window.window_handle().window_id(); self.row_menu = None;
        cx.defer(move |cx| {
            if let Err(error) = shared::open_window(&room, cx) {
                let message = settings::diagnostic(&error.to_string());
                if let Some(handle) = cx.windows().into_iter().find(|handle| handle.window_id() == window_id).and_then(|handle| handle.downcast::<Workspace>()) {
                    let _ = handle.update(cx, |w, _, cx| w.show_toast_kind(ui::ToastKind::Error, &message, cx));
                }
            }
        });
        }
    }

    pub(super) fn copy_link_to_clipboard(&mut self, link: String, public_link: bool, cx: &mut Context<Self>) {
        if !crate::devtools::scripted() { cx.write_to_clipboard(ClipboardItem::new_string(link.clone())); }
        self.sharing.copied_text = Some(link);
        self.show_toast(if public_link { tr("已复制公开链接", "Public link copied") }
            else { tr("已复制邀请链接", "Invitation link copied") }, cx);
    }

    pub(crate) fn copy_shared_link(&mut self, room: Room, window: &mut Window, cx: &mut Context<Self>) {
        if room.scope.owner.is_some() || self.sharing.busy || !self.sharing_account_ready(cx) { return; }
        let epoch = self.begin_sharing_request(cx);
        let personal = crate::store::Store::root_dir();
        cx.spawn_in(window, async move |this, cx| {
            let target = room.clone();
            let result = cx.background_executor().spawn(async move { shared::manage(&personal, &target, "members", "", None) }).await;
            let _ = this.update_in(cx, |w, _, cx| {
                if !w.finish_sharing_request(epoch, None, cx) { return; }
                match result {
                    Ok(reply) => {
                        let public_link = reply.audience.link_active();
                        let link = reply.url.as_deref().and_then(|url| links::branded(url).ok());
                        let closed = reply.audience == Audience::LinkClosed;
                        if w.sheet_room().is_some_and(|open| open.scope == room.scope) { w.sharing.reply = Some(reply); }
                        if let Some(link) = link { w.copy_link_to_clipboard(link, public_link, cx); }
                        else {
                            w.show_toast_kind(ui::ToastKind::Error, if closed { link_audience(Audience::LinkClosed) }
                                else { tr("共享链接暂不可用，请刷新后重试。", "Share link unavailable. Refresh and try again.") }, cx);
                        }
                    }
                    Err(error) => w.show_toast_kind(ui::ToastKind::Error, &settings::diagnostic(&error.to_string()), cx),
                }
            });
        }).detach();
    }

    pub(crate) fn copy_shared_documents(&mut self, ids: Vec<String>, cx: &mut Context<Self>) {
        if self.sharing.busy || ids.is_empty() { return; }
        if !self.flush(cx) { self.show_toast_kind(ui::ToastKind::Error, tr("请先保存当前文档，再创建副本", "Save the current document before copying"), cx); return; }
        let personal = crate::store::Store::root_dir();
        let mut sources = vec![];
        for id in ids {
            let Some(room) = self.shared_list_room(&id, cx) else { continue };
            match shared::store_for(&room, cx) {
                Ok(store) if shared::access(&store, cx).is_some_and(|access| access != Access::Revoked) => sources.push((id, store.root().to_path_buf(), crate::localized_format!("{} 副本", "{} Copy", display_title(&room.title)))),
                Ok(_) => { self.show_toast_kind(ui::ToastKind::Error, tr("文档暂不可用，请稍后再试", "Document unavailable. Try again later"), cx); return; }
                Err(error) => { self.show_toast_kind(ui::ToastKind::Error, &settings::diagnostic(&error.to_string()), cx); return; }
            }
        }
        if sources.is_empty() { return; }
        let Some(guard) = cx.global::<crate::shared_store::SharedStore>().begin_file_task() else { return };
        self.sharing.busy = true; self.row_menu = None; cx.notify();
        cx.spawn(async move |this, cx| {
            let results = cx.background_executor().spawn(async move {
                sources.into_iter().map(|(id, source, title)| { let result = crate::store::Store::copy_note_to_personal(&personal, &source, &id, &title); (id, result) }).collect::<Vec<_>>()
            }).await;
            drop(guard);
            let _ = this.update(cx, |w, cx| {
                w.sharing.busy = false; w.batch = None;
                let errors = results.iter().filter_map(|(_, result)| result.as_ref().err()).map(|error| settings::diagnostic(&error.to_string())).collect::<Vec<_>>();
                if !w.is_shared() { w.entries = w.store.list(); w.sort(); }
                if errors.is_empty() { w.show_toast(tr("副本已保存到“我的文档”", "Copies saved to My documents"), cx); }
                else {
                    w.batch = Some(results.iter().filter(|(_, result)| result.is_err()).map(|(id, _)| id.clone()).collect()); w.sharing.batch = true;
                    w.show_toast_kind(ui::ToastKind::Error, &errors.join("；"), cx);
                }
                sidebar::notify(cx); cx.notify();
            });
        }).detach();
    }

    pub(crate) fn confirm_shared_batch_removal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rooms = self.batch.as_ref().into_iter().flatten().filter_map(|id| self.shared_list_room(id, cx)).collect::<Vec<_>>();
        let entries = self.personal_navigation_entries(cx);
        let personal = self.batch.as_ref().into_iter().flatten().filter(|id| entries.iter().any(|entry| &entry.id == *id) && self.shared_list_room(id, cx).is_none()).cloned().collect();
        if !rooms.is_empty() { self.sharing_sheet(Sheet::EndMany(rooms, personal), window, cx); }
    }
}
