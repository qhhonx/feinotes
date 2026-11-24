//! Organization and explicit cross-library actions for shared navigation.
use super::*;

type Action = Box<dyn FnOnce(&mut Workspace, &str, &mut Window, &mut Context<Workspace>)>;
pub(super) type SharedAction = std::rc::Rc<std::cell::RefCell<Option<Action>>>;

impl Workspace {
    #[cfg(target_os = "ios")]
    pub(crate) fn with_shared_document(&mut self, room: Room, window: &mut Window, cx: &mut Context<Self>, action: impl FnOnce(&mut Self, &str, &mut Window, &mut Context<Self>) + 'static) {
        let action = std::rc::Rc::new(std::cell::RefCell::new(Some(Box::new(action) as Action)));
        self.open_shared_room_for_action(room, action, window, cx);
    }

    #[cfg(target_os = "macos")]
    pub(super) fn with_shared_sidebar_document(&mut self, room: Room, entry: crate::sidebar::SidebarEntry, write: bool,
        window: &mut Window, cx: &mut Context<Self>, action: impl FnOnce(&mut Self, &str, &mut Window, &mut Context<Self>) + 'static) {
        self.row_menu = None;
        let expected = room.scope.clone();
        let guarded: Action = Box::new(move |w, id, window, cx| {
            let access = w.sharing_access(cx);
            if shared::scope(&w.store, cx) != Some(&expected) || shared::editing_paused(&w.store, cx)
                || !access.is_some_and(|access| if write { access.can_write() } else { access != Access::Revoked })
                || !w.entries.iter().any(|e| e.id == id && e.meta.folder.is_none()) { return; }
            action(w, id, window, cx);
        });
        self.open_shared_item_for_action(room, entry.id.clone(), Some(entry), std::rc::Rc::new(std::cell::RefCell::new(Some(guarded))), window, cx);
    }

    pub(crate) fn open_shared_window(&mut self, room: Room, window: &Window, cx: &mut Context<Self>) {
        #[cfg(target_os = "ios")]
        { let _ = window; shared::launch(&room,cx).detach(); self.row_menu = None; }
        #[cfg(target_os = "macos")]
        self.open_shared_window_at(room, None, window, cx);
    }

    #[cfg(target_os = "macos")]
    pub(super) fn open_shared_child_window(&mut self, room: Room, target: String, window: &Window, cx: &mut Context<Self>) {
        self.open_shared_window_at(room, Some(target), window, cx);
    }

    #[cfg(target_os = "macos")]
    fn open_shared_window_at(&mut self, room: Room, target: Option<String>, window: &Window, cx: &mut Context<Self>) {
        let window_id = window.window_handle().window_id();
        self.row_menu = None;
        cx.notify();
        cx.defer(move |cx| {
            let result = match target {
                Some(id) => shared::open_child_window(&room, &id, cx),
                None => shared::open_window(&room, cx),
            };
            if let Err(error) = result {
                let message = settings::diagnostic(&error.to_string());
                if let Some(handle) = cx.windows().into_iter().find(|handle| handle.window_id() == window_id).and_then(|handle| handle.downcast::<Workspace>()) {
                    let _ = handle.update(cx, |w, _, cx| w.show_toast_kind(ui::ToastKind::Error, &message, cx));
                }
            }
        });
    }

    pub(super) fn write_invitation_to_clipboard(&mut self, text: String, cx: &mut Context<Self>) {
        if !crate::devtools::scripted() { cx.write_to_clipboard(ClipboardItem::new_string(text.clone())); }
        self.sharing.copied_text = Some(text);
    }

    pub(super) fn public_invitation_copied(&mut self, cx: &mut Context<Self>) {
        self.show_toast(tr("已复制公开邀请", "Public invitation copied"), cx);
    }

    pub(crate) fn copy_shared_invitation(&mut self, room: Room, window: &mut Window, cx: &mut Context<Self>) {
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
                        if let Some(link) = link {
                            w.write_invitation_to_clipboard(invitation_text(&room.title, &link), cx);
                            if public_link { w.public_invitation_copied(cx); }
                            else { w.show_toast(tr("已复制邀请", "Invitation copied"), cx); }
                        }
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
            let Some(room) = self.shared_list_room(&id, cx).filter(|room| !room.scope.folder) else { continue };
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
