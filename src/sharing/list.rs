//! Shared entries reuse the personal document list and its contextual controls.
use super::*;
use crate::app::RowMenu;

#[cfg(target_os = "macos")]
use crate::design::{MenuItem, MenuSeparator, Popover};

#[cfg(target_os = "macos")]
fn shared_menu_item(id: &'static str, glyph: &'static str, label: &'static str) -> MenuItem {
    MenuItem::new(id).icon(glyph).label(label)
}

#[cfg(target_os = "macos")]
fn present_shared_menu(menu: &RowMenu, body: Popover) -> AnyElement {
    let pos = if menu.pos.x < px(0.) { point(-menu.pos.x, menu.pos.y + px(22.)) } else { menu.pos };
    deferred(anchored().anchor(menu.corner).position(pos).snap_to_window_with_margin(px(8.)).child(body)).with_priority(2).into_any_element()
}

#[cfg(target_os = "macos")]
fn pin_label(on: bool) -> &'static str { settings::text(if on { "从“置顶”移除" } else { "添加到“置顶”" }) }

#[cfg(target_os = "macos")]
fn star_label(on: bool) -> &'static str { if on { settings::text("取消收藏") } else { tr("收藏", "Add to favorites") } }

impl Workspace {
    pub(crate) fn shared_list_room(&self, id: &str, cx: &App) -> Option<Room> {
        cx.try_global::<cloud_sync::activity::ActivityView>()?.0.iter().find(|row| row.room.scope.note == id).map(|row| row.room.clone())
    }

    pub(crate) fn published_document_share(&self, entry: &crate::store::Entry, cx: &App) -> bool {
        if entry.meta.folder.is_some() { return false; }
        if let Some(root) = &entry.meta.share_root {
            let room = Room { scope: shared::Scope { note: root.clone(), owner: None, folder: false }, title: String::new() };
            return !shared::original::awaiting_publication(&crate::store::Store::root_dir(), &room);
        }
        self.shared_list_room(&entry.id, cx).is_some()
    }

    #[cfg(target_os = "ios")]
    pub(crate) fn shared_document_unread(&self, id: &str, cx: &App) -> bool {
        cx.try_global::<crate::cloud_sync::activity::ActivityView>().is_some_and(|view|
            view.0.iter().any(|row| row.room.scope.note == id && row.unread))
    }

    pub(crate) fn render_shared_documents(&self, entries: &[crate::store::Entry], window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let tree = cx.global::<organizer::Organizer>();
        let folder = self.sharing.collection.as_deref().filter(|id| tree.is_folder(id));
        let error = cx.try_global::<cloud_sync::activity::ActivityView>().is_some_and(|view| view.1);
        let mut body = div().flex_1().min_h_0().flex().flex_col();
        if let Some(id) = folder {
            let placement = &tree.0[id]; let up = placement.parent.clone();
            body = body.child(div().flex().items_center().gap(px(10.)).px(px(24.)).py(px(6.))
                .child(action("collection-up", tr("上一级", "Up one level"), true)
                .on_click(cx.listener(move |ws, _, _, cx| { ws.sharing.collection = up.clone(); cx.notify(); })))
                .child(caption(placement.title.clone().unwrap_or_default())));
        }
        if !self.sharing_allowed(cx) {
            body = body.child(div().px(px(24.)).pt(px(12.)).child(caption(tr("协作已暂停。请在文档库设置中检查 iCloud。", "Collaboration is paused. Check iCloud in Documents settings."))));
        }
        if error {
            body = body.child(div().px(px(24.)).pt(px(12.)).child(caption(tr(
                "共享列表暂未更新。飞记会自动重试。", "The shared list could not update. Fei Notes will retry."))));
        }
        body.child(if self.home.grid { self.render_home_grid(&std::rc::Rc::new(entries.to_vec()), false, window, cx) } else { self.render_home_list(entries, false, cx) }).into_any_element()
    }

    #[cfg(target_os = "ios")]
    pub(crate) fn shared_child_row_menu(&self, menu: &RowMenu, id: &str, room: Room, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let prefs = preferences::active(cx);
        let pin = if prefs.get("pin", id) != 0 { tr("取消置顶", "Unpin") } else { tr("置顶", "Pin") };
        let star = if prefs.get("star", id) != 0 { tr("取消收藏", "Remove from Favorites") } else { tr("收藏", "Favorite") };
        let open_room = room.clone();
        let (link_room, link_target) = (room.clone(), id.to_owned());
        let (open, pin_id, star_id) = (id.to_owned(), id.to_owned(), id.to_owned());
        let row = |id, icon, label| ui::menu_row(id, SharedString::from(id)).child(ui::icon(icon,16.,Theme::icon())).child(label);
        let divider = || div().mx(px(8.)).my(px(5.)).h(px(1.)).bg(Theme::line_border());
        let body = ui::popover().id("shared-child-row-menu").occlude().w(px(crate::sidebar::ROW_MENU_W))
            .py(px(ui::MENU_PAD_Y)).on_mouse_down_out(cx.listener(|w, _, _, cx| { w.row_menu = None; cx.notify(); }))
            .child(row("shared-child-open", "doc", tr("打开", "Open"))
                .when(cfg!(feature = "devtools"), |row| row.relative().child(self.home.track("qa-shared-child-open")))
                .on_click(cx.listener(move |w, _, window, cx| {
                    w.row_menu = None;
                    w.open_shared_item_interactive(open_room.clone(), open.clone(), None, window, cx);
                })))
            .child(divider())
            .child(row("shared-child-pin", "pin", pin).when(cfg!(feature = "devtools"), |row| row.relative().child(self.home.track("qa-shared-child-pin")))
                .on_click(cx.listener(move |w, _, _, cx| w.toggle_pin(&pin_id,cx))))
            .child(row("shared-child-star", "star", star).on_click(cx.listener(move |w, _, _, cx| w.toggle_star(&star_id,cx))))
            .child(divider())
            .when(self.object_link_available(cx), |d| d.child(row("shared-child-copy-object-link", "copy", tr("复制链接", "Copy Link"))
                .on_click(cx.listener(move |w, _, _, cx| w.copy_object_link(&link_target, Some(link_room.clone()), false, cx)))))
            .child(row("shared-child-access", "shared-document", shared_access_title(true))
                .on_click(cx.listener(move |w, _, window, cx| {
                    w.row_menu = None;
                    if room.scope.owner.is_none() { w.open_member_management(room.clone(),window,cx); }
                    else { w.sharing_sheet(Sheet::Access(room.clone()),window,cx); }
                })));
        let pos = if menu.pos.x < px(0.) { point(-menu.pos.x, menu.pos.y + px(22.)) } else { menu.pos };
        vec![deferred(anchored().anchor(menu.corner).position(pos).snap_to_window_with_margin(px(8.)).child(body)).with_priority(2).into_any_element()]
    }

    #[cfg(target_os = "macos")]
    fn shared_menu_panel(&self, id: &'static str, window: &Window, cx: &Context<Self>) -> Popover {
        Popover::new(id).menu().min_w(px(crate::sidebar::ROW_MENU_W))
            .max_h(window.viewport_size().height - px(16.)).overflow_y_scroll()
            .on_mouse_down_out(cx.listener(Self::dismiss_row_menu))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn shared_child_row_menu(&self, menu: &RowMenu, room: Room, entry: crate::sidebar::SidebarEntry, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let writable = self.shared_sidebar_folder_writable(&room, cx);
        let row = |id, glyph, label| shared_menu_item(id, glyph, label).on_hover(cx.listener(Self::close_row_download));
        let marks = preferences::active(cx);
        let pinned = marks.get("pin", &entry.id) != 0;
        let starred = marks.get("star", &entry.id) != 0;
        let (new_window, window_room, copy, copy_room) = (entry.id.clone(), room.clone(), entry.clone(), room.clone());
        let (moving, move_room, rename, rename_room) = (entry.clone(), room.clone(), entry.clone(), room.clone());
        let (pin, star) = (entry.id.clone(), entry.id.clone());
        let (link_room, link_target) = (room.clone(), entry.id.clone());
        let (access, delete_room, delete) = (room.clone(), room.clone(), entry.id.clone());
        let body = self.shared_menu_panel("shared-child-row-menu", window, cx)
            .child(row("shared-child-window", "copy", tr("在新窗口中打开", "Open in new window"))
                .on_click(cx.listener(move |w, _, window, cx| w.open_shared_child_window(window_room.clone(), new_window.clone(), window, cx))))
            .when(writable, |d| d.child(row("shared-child-copy", "copy", tr("创建副本", "Duplicate"))
                .on_click(cx.listener(move |w, _, window, cx|
                    w.with_shared_sidebar_document(copy_room.clone(), copy.clone(), true, window, cx, Self::duplicate_doc)))))
            .when(writable, |d| d.child(row("shared-child-move", "folder-input", tr("移动到", "Move to"))
                .on_click(cx.listener(move |w, _, window, cx| {
                    let target = moving.clone();
                    w.with_shared_sidebar_document(move_room.clone(), target, true, window, cx, |w, id, window, cx|
                        w.open_move(vec![id.to_owned()], window, cx));
                }))))
            .child(row("shared-child-pin", if pinned { "pin-off" } else { "pin" }, pin_label(pinned))
                .when(cfg!(feature = "devtools"), |row| row.relative().child(self.home.track("qa-shared-child-pin")))
                .on_click(cx.listener(move |w, _, _, cx| { w.toggle_shared_child_organization(&pin, true, cx); })))
            .child(row("shared-child-star", if starred { "star-off" } else { "star" }, star_label(starred))
                .on_click(cx.listener(move |w, _, _, cx| { w.toggle_shared_child_organization(&star, false, cx); })))
            .child(MenuSeparator::new())
            .child(self.row_download_item(shared_menu_item("shared-child-download", "export", crate::settings::text("下载")), menu.sub, cx))
            .when(writable, |d| d.child(row("shared-child-rename", "square-pen", tr("重命名", "Rename"))
                .on_click(cx.listener(move |w, _, window, cx| {
                    let section = rename.section;
                    w.with_shared_sidebar_document(rename_room.clone(), rename.clone(), true, window, cx, move |w, id, window, cx|
                        w.start_rename_in_sidebar(id, Some(section), window, cx));
                }))))
            .child(MenuSeparator::new())
            .child(row("shared-child-access", "shared-document", shared_access_title(true))
                .on_click(cx.listener(move |w, _, window, cx| w.open_row_sharing_access(access.clone(), window, cx))))
            .when(self.object_link_available(cx), |d| d.child(row("shared-child-copy-object-link", "copy", tr("复制链接", "Copy Link"))
                .on_click(cx.listener(move |w, _, _, cx| w.copy_object_link(&link_target, Some(link_room.clone()), false, cx)))))
            .when(writable, |d| d.child(MenuSeparator::new())
                .child(row("shared-child-delete", "bin", tr("删除文档", "Delete document")).destructive(true)
                    .on_click(cx.listener(move |w, _, _, cx| w.ask_delete_shared_descendant(&delete_room, &delete, false, cx)))));
        let mut out = vec![present_shared_menu(menu, body)];
        out.extend(self.row_download_submenu(menu.sub, window, cx, move |w, format, window, cx| {
            w.with_shared_sidebar_document(room.clone(), entry.clone(), false, window, cx, move |w, id, window, cx| format.export(w, id, window, cx));
        }));
        out
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn shared_nested_folder_menu(&self, menu: &RowMenu, room: Room, entry: crate::sidebar::SidebarEntry, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let writable = self.shared_sidebar_folder_writable(&room, cx);
        let (link_room, link_target) = (room.clone(), entry.id.clone());
        let access = room.clone();
        let body = self.shared_menu_panel("shared-folder-row-menu", window, cx)
            .when(writable, |body| {
                let rename = entry.clone(); let rename_room = room.clone();
                let moving = entry.clone(); let move_room = room.clone();
                body.when(menu.pos.x >= px(0.), |d| d.children(self.shared_folder_create_rows(&room, &entry, cx)))
                    .child(shared_menu_item("shared-folder-rename", "square-pen", tr("重命名", "Rename"))
                        .on_click(cx.listener(move |w, _, window, cx|
                            w.edit_shared_folder(rename_room.clone(), rename.clone(), SharedFolderEdit::Rename, window, cx))))
                    .child(shared_menu_item("shared-folder-move", "folder-input", tr("移动到", "Move to"))
                        .on_click(cx.listener(move |w, _, window, cx|
                            w.edit_shared_folder(move_room.clone(), moving.clone(), SharedFolderEdit::Move, window, cx))))
                    .child(MenuSeparator::new())
            })
            .child(shared_menu_item("shared-folder-access", "shared-document", shared_access_title(true))
                .on_click(cx.listener(move |w, _, window, cx| w.open_row_sharing_access(access.clone(), window, cx))))
            .when(self.object_link_available(cx), |d| d.child(shared_menu_item("shared-folder-copy-object-link", "copy", tr("复制链接", "Copy Link"))
                .on_click(cx.listener(move |w, _, _, cx| w.copy_object_link(&link_target, Some(link_room.clone()), false, cx)))))
            .when(writable, |d| d.child(MenuSeparator::new())
                .child(shared_menu_item("shared-folder-delete", "bin", tr("删除子文件夹", "Delete subfolder")).destructive(true)
                    .when(cfg!(feature = "devtools"), |row| row.relative().child(self.home.track("qa-shared-folder-delete")))
                    .on_click(cx.listener(move |w, _, _, cx| w.ask_delete_shared_descendant(&room, &entry.id, true, cx)))));
        vec![present_shared_menu(menu, body)]
    }

    #[cfg(target_os = "macos")]
    fn shared_folder_create_rows(&self, room: &Room, entry: &crate::sidebar::SidebarEntry, cx: &Context<Self>) -> [MenuItem; 2] {
        use crate::folders::{CreateTarget, SharedFolderCreation};
        let (document, document_room) = (entry.clone(), room.clone());
        let folder = CreateTarget::SharedFolder { room: room.clone(), parent: entry.id.clone(), section: entry.section };
        [
            shared_menu_item("shared-folder-new-document", "doc", tr("新建文档", "New document"))
                .on_click(cx.listener(move |w, _, window, cx|
                    w.create_in_shared_sidebar(document_room.clone(), document.id.clone(), SharedFolderCreation::Document, document.section, window, cx))),
            shared_menu_item("shared-folder-new-folder", "folder-plus", tr("新建文件夹", "New folder"))
                .on_click(cx.listener(move |w, _, window, cx| {
                    w.row_menu = None;
                    w.new_folder(folder.clone(), None, window, cx);
                })),
        ]
    }

    #[cfg(target_os = "macos")]
    fn open_row_sharing_access(&mut self, room: Room, window: &mut Window, cx: &mut Context<Self>) {
        self.row_menu = None;
        if room.scope.owner.is_none() { self.open_member_management(room, window, cx); }
        else { self.sharing_sheet(Sheet::Access(room), window, cx); }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn shared_row_menu(&self, menu: RowMenu, section: crate::sidebar::SidebarSection, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(room) = self.shared_list_room(&menu.id, cx) else { return vec![]; };
        let Some(entry) = self.shared_entries(cx).into_iter().find(|entry| entry.id == menu.id) else { return vec![]; };
        let mut body = self.shared_menu_panel("shared-row-menu", window, cx);
        if menu.batch {
            body = body.child(shared_menu_item("shared-batch-copy-menu", "copy", tr("创建副本", "Create copies"))
                .on_click(cx.listener(|w, _, _, cx| w.batch_copy(cx))))
                .child(shared_menu_item("shared-batch-remove-menu", "bin", tr("移除共享文档…", "Remove shared documents…"))
                    .destructive(true).on_click(cx.listener(|w, _, window, cx| w.confirm_shared_batch_removal(window, cx))));
            return vec![present_shared_menu(&menu, body)];
        }
        let owner = room.scope.owner.is_none();
        let folder = room.scope.folder;
        let writable = if folder { self.shared_sidebar_folder_writable(&room, cx) }
            else { shared::live_room_writable(&room, cx).unwrap_or_else(|| cx.try_global::<cloud_sync::activity::ActivityView>()
                .and_then(|view| view.0.iter().find(|row| row.room.scope == room.scope))
                .is_some_and(|row| row.access.map_or(owner, Access::can_write))) };
        let target = crate::sidebar::SidebarEntry::new(section, &entry.id);
        let row = |id, glyph, label| shared_menu_item(id, glyph, label).on_hover(cx.listener(Self::close_row_download));
        let (new_window, manage, rename, invite, ending) = (room.clone(), room.clone(), room.clone(), room.clone(), room.clone());
        let (copy, pin, star, link_target, delete) = (entry.id.clone(), entry.id.clone(), entry.id.clone(), entry.id.clone(), entry.id.clone());
        let link_room = room.clone();
        let rename_target = target.clone();
        let rename = row("shared-row-rename", "square-pen", tr("重命名", "Rename"))
            .on_click(cx.listener(move |w, _, window, cx| {
                if rename.scope.folder {
                    w.edit_shared_folder(rename.clone(), rename_target.clone(), SharedFolderEdit::Rename, window, cx);
                } else {
                    let section = rename_target.section;
                    w.with_shared_sidebar_document(rename.clone(), rename_target.clone(), true, window, cx, move |w, id, window, cx|
                        w.start_rename_in_sidebar(id, Some(section), window, cx));
                }
            }));
        if !folder {
            body = body.child(row("shared-row-window", "copy", tr("在新窗口中打开", "Open in new window"))
                    .on_click(cx.listener(move |w, _, window, cx| w.open_shared_window(new_window.clone(), window, cx))))
                .child(row("shared-row-copy", "copy", tr("复制到我的文档", "Copy to My Documents"))
                    .on_click(cx.listener(move |w, _, _, cx| w.copy_shared_documents(vec![copy.clone()], cx))));
        }
        let rename = if folder && writable {
            body = body.when(menu.pos.x >= px(0.), |d| d.children(self.shared_folder_create_rows(&room, &target, cx)))
                .child(rename).child(MenuSeparator::new());
            None
        } else { (!folder && writable).then_some(rename) };
        body = body.child(row("shared-row-pin", if entry.meta.pinned { "pin-off" } else { "pin" }, pin_label(entry.meta.pinned))
                .on_click(cx.listener(move |w, _, _, cx| w.toggle_shared_organization(&pin, true, cx))))
            .child(row("shared-row-star", if entry.meta.starred > 0 { "star-off" } else { "star" }, star_label(entry.meta.starred > 0))
                .on_click(cx.listener(move |w, _, _, cx| w.toggle_shared_organization(&star, false, cx))))
            .child(MenuSeparator::new())
            .when(!folder, |d| d.child(self.row_download_item(shared_menu_item("shared-row-download", "export", crate::settings::text("下载")), menu.sub, cx))
                .children(rename).child(MenuSeparator::new()))
            .child(row("shared-row-manage", "shared-document", if owner { tr("成员与权限…", "Members and access…") } else { tr("查看共享权限…", "View sharing access…") })
                .on_click(cx.listener(move |w, _, window, cx| w.open_row_sharing_access(manage.clone(), window, cx))))
            .when(self.object_link_available(cx), |d| d.child(row("shared-row-copy-object-link", "copy", tr("复制链接", "Copy Link"))
                .on_click(cx.listener(move |w, _, _, cx| w.copy_object_link(&link_target, Some(link_room.clone()), false, cx)))))
            .when(owner, |d| d.child(row("shared-row-link", "copy", copy_invitation_label(false))
                .on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.copy_shared_invitation(invite.clone(), window, cx); }))))
            .child(MenuSeparator::new())
            .when(folder && self.stopping_original(&room, cx), |d| d.child(row("shared-root-delete", "bin", tr("删除文件夹", "Delete folder"))
                .destructive(true).when(cfg!(feature = "devtools"), |row| row.relative().child(self.home.track("qa-shared-folder-delete")))
                .on_click(cx.listener(move |w, _, _, cx| w.ask_delete(&delete, cx)))))
            .child(row("shared-row-remove", "bin", self.shared_end_label(&room, cx))
                .destructive(true).on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.open_end_shared(ending.clone(), window, cx); })));
        let mut out = vec![present_shared_menu(&menu, body)];
        out.extend(self.row_download_submenu(!folder && menu.sub, window, cx, move |w, format, window, cx| {
            w.with_shared_sidebar_document(room.clone(), target.clone(), false, window, cx, move |w, id, window, cx| format.export(w, id, window, cx));
        }));
        out
    }

    #[cfg(target_os = "ios")]
    pub(crate) fn shared_row_menu(&self, menu: RowMenu, section: crate::sidebar::SidebarSection, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(room) = self.shared_list_room(&menu.id, cx) else { return vec![]; };
        let entry = self.shared_entries(cx).into_iter().find(|entry| entry.id == menu.id).unwrap();
        let owner = room.scope.owner.is_none();
        let _ = section;
        let editable = owner || shared::store_for(&room, cx).ok().is_some_and(|store| shared::access(&store, cx) == Some(Access::ReadWrite));
        let (open, new_window, manage, rename, md, pdf) = (room.clone(), room.clone(), room.clone(), room.clone(), room.clone(), room.clone());
        let folder_writable = room.scope.folder && editable;
        let folder_actions = room.clone();
        let (link_room, link_target) = (room.clone(), room.scope.note.clone());
        let (copy, pin, star, link) = (menu.id.clone(), menu.id.clone(), menu.id.clone(), room.clone());
        let item = |id: &'static str, glyph: &'static str, label: &'static str| {
            ui::menu_row(id, SharedString::from(id)).child(ui::icon(glyph, 16., Theme::icon())).child(label)
        };
        let mut popover = ui::popover().id("shared-row-menu").occlude().w(px(crate::sidebar::ROW_MENU_W))
            .px(px(5.)).py(px(ui::MENU_PAD_Y))
            .on_mouse_down_out(cx.listener(|w, _, _, cx| { w.row_menu = None; cx.notify(); }));
        if self.shared_batch() || menu.batch {
            popover = popover.child(item("shared-batch-copy-menu", "copy", tr("创建副本", "Create copies"))
                .on_click(cx.listener(|w, _, _, cx| w.batch_copy(cx))))
                .child(item("shared-batch-remove-menu", "bin", tr("移除共享文档…", "Remove shared documents…"))
                    .text_color(Theme::danger()).on_click(cx.listener(|w, _, window, cx| w.confirm_shared_batch_removal(window, cx))));
        } else {
            popover = popover
                .child(item("shared-row-open", "doc", tr("打开", "Open"))
                    .on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.open_shared_note(&open, window, cx); })))
                .child(item("shared-row-window", "copy", tr("在新窗口中打开", "Open in new window"))
                    .on_click(cx.listener(move |w, _, window, cx| w.open_shared_window(new_window.clone(), window, cx))))
                .child(div().mx(px(8.)).my(px(5.)).h(px(1.)).bg(Theme::line_border()))
                .child(item("shared-row-rename", "square-pen", tr("重命名", "Rename"))
                    .when(!editable, |d| d.opacity(0.4).cursor_default())
                    .when(editable, |d| d.on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None;
                        w.with_shared_document(rename.clone(), window, cx, |w, id, window, cx| { if matches!(w.sharing_access(cx), Some(Access::Owner | Access::ReadWrite)) { w.start_rename(id, window, cx); } }); }))))
                .child(item("shared-row-pin", if entry.meta.pinned { "pin-off" } else { "pin" }, if entry.meta.pinned { tr("从“置顶”移除", "Remove from pinned") } else { tr("添加到“置顶”", "Pin document") })
                    .on_click(cx.listener(move |w, _, _, cx| w.toggle_shared_organization(&pin, true, cx))))
                .child(item("shared-row-star", if entry.meta.starred > 0 { "star-off" } else { "star" }, if entry.meta.starred > 0 { tr("取消收藏", "Remove favorite") } else { tr("收藏", "Add to favorites") })
                    .on_click(cx.listener(move |w, _, _, cx| w.toggle_shared_organization(&star, false, cx))))
                .when(!room.scope.folder, |d| d.child(item("shared-row-copy", "copy", tr("复制到我的文档", "Copy to My Documents"))
                    .on_click(cx.listener(move |w, _, _, cx| w.copy_shared_documents(vec![copy.clone()], cx)))))
                .when(folder_writable, |d| d.child(item("shared-folder-new", "plus", tr("新建…", "New…"))
                    .on_click(cx.listener(move |w, event: &ClickEvent, _window, cx| {
                        w.row_menu = None; let pos = event.position();
                        w.with_shared_document(folder_actions.clone(), _window, cx, move |w, id, _, cx| {
                            if w.sharing_access(cx).is_some_and(|access| access.can_write()) {
                                w.create_menu = Some((crate::folders::CreateTarget::Library(Some(id.into())), pos)); cx.notify();
                            }
                        });
                    }))))
                .child(div().mx(px(8.)).my(px(5.)).h(px(1.)).bg(Theme::line_border()))
                .child(item("shared-row-manage", "share", if owner { tr("成员与权限…", "Members and access…") } else { tr("查看共享权限…", "View sharing access…") })
                    .on_click(cx.listener(move |w, _, window, cx| {
                        w.row_menu = None;
                        if owner { w.open_member_management(manage.clone(),window,cx); }
                        else { w.sharing_sheet(Sheet::Access(manage.clone()),window,cx); }
                    })))
                .when(self.object_link_available(cx), |d| d.child(item("shared-row-copy-object-link", "copy", tr("复制链接", "Copy Link"))
                    .on_click(cx.listener(move |w, _, _, cx| w.copy_object_link(&link_target, Some(link_room.clone()), false, cx)))))
                .when(owner, |d| d.child(item("shared-row-link", "copy", copy_invitation_label(false))
                    .on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.copy_shared_invitation(link.clone(), window, cx); }))))
                .when(!room.scope.folder, |d| d.child(item("shared-row-markdown", "export", tr("下载为 Markdown…", "Download Markdown…"))
                    .on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.with_shared_document(md.clone(), window, cx, |w, id, _, cx| w.open_export_md(id, cx)); }))))
                .when(!room.scope.folder, |d| d.child(item("shared-row-pdf", "export", tr("下载为 PDF…", "Download PDF…"))
                    .on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.with_shared_document(pdf.clone(), window, cx, |w, id, window, cx| w.export_pdf(id, window, cx)); }))))
                .child(div().mx(px(8.)).my(px(5.)).h(px(1.)).bg(Theme::line_border()))
                .child(item("shared-row-remove", "bin", self.shared_end_label(&room, cx))
                    .text_color(Theme::danger()).on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.open_end_shared(room.clone(), window, cx); })));
        }
        let position = if menu.pos.x < px(0.) { point(-menu.pos.x, menu.pos.y + px(22.)) } else { menu.pos };
        vec![deferred(anchored().anchor(menu.corner).position(position).snap_to_window_with_margin(px(8.)).child(popover.max_h(window.viewport_size().height - px(16.)).overflow_y_scroll())).with_priority(2).into_any_element()]
    }
}
