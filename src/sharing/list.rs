//! Shared entries reuse the personal document list and its contextual controls.
use super::*;
use crate::app::RowMenu;

impl Workspace {
    pub(crate) fn shared_list_room(&self, id: &str, cx: &App) -> Option<Room> {
        cx.try_global::<cloud_sync::activity::ActivityView>()?.0.iter().find(|row| row.room.scope.note == id).map(|row| row.room.clone())
    }

    pub(crate) fn render_shared_documents(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let entries = self.sorted_entries(self.shared_entries(cx));
        let error = cx.try_global::<cloud_sync::activity::ActivityView>().and_then(|view| view.1.clone());
        let mut body = div().flex_1().min_h_0().flex().flex_col();
        if !self.sharing_allowed(cx) {
            body = body.child(div().px(px(24.)).pt(px(12.)).child(caption(tr("协作已暂停。请在文档库设置中检查 iCloud。", "Collaboration is paused. Check iCloud in Documents settings."))));
        }
        if let Some(error) = error {
            body = body.child(div().px(px(24.)).pt(px(12.)).child(caption(format!("{} {}", tr("共享列表暂未更新。", "The shared list could not update."), settings::diagnostic(&error)))));
        }
        body.child(if self.home.grid { self.render_home_grid(&entries, false, window, cx) } else { self.render_home_list(&entries, false, cx) }).into_any_element()
    }

    pub(crate) fn shared_row_menu(&self, menu: RowMenu, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(room) = self.shared_list_room(&menu.id, cx) else { return vec![]; };
        let entry = self.shared_entries(cx).into_iter().find(|entry| entry.id == menu.id).unwrap();
        let owner = room.scope.owner.is_none();
        let editable = owner || shared::store_for(&room, cx).ok().is_some_and(|store| shared::access(&store, cx) == Some(Access::ReadWrite));
        let (open, new_window, manage, rename, md, pdf) = (room.clone(), room.clone(), room.clone(), room.clone(), room.clone(), room.clone());
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
                .child(item("shared-row-copy", "copy", tr("创建副本", "Create copy"))
                    .on_click(cx.listener(move |w, _, _, cx| w.copy_shared_documents(vec![copy.clone()], cx))))
                .child(item("shared-row-pin", if entry.meta.pinned { "pin-off" } else { "pin" }, if entry.meta.pinned { tr("从“置顶”移除", "Remove from pinned") } else { tr("添加到“置顶”", "Pin document") })
                    .on_click(cx.listener(move |w, _, _, cx| w.toggle_shared_organization(&pin, true, cx))))
                .child(item("shared-row-star", if entry.meta.starred > 0 { "star-off" } else { "star" }, if entry.meta.starred > 0 { tr("取消收藏", "Remove favorite") } else { tr("收藏", "Add to favorites") })
                    .on_click(cx.listener(move |w, _, _, cx| w.toggle_shared_organization(&star, false, cx))))
                .child(item("shared-row-link", "copy", tr("复制邀请链接", "Copy invitation link"))
                    .on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.copy_shared_link(link.clone(), window, cx); })))
                .child(div().mx(px(8.)).my(px(5.)).h(px(1.)).bg(Theme::line_border()))
                .child(item("shared-row-markdown", "export", tr("下载为 Markdown…", "Download Markdown…"))
                    .on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.with_shared_document(md.clone(), window, cx, |w, id, _, cx| w.open_export_md(id, cx)); })))
                .child(item("shared-row-pdf", "export", tr("下载为 PDF…", "Download PDF…"))
                    .on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.with_shared_document(pdf.clone(), window, cx, |w, id, window, cx| w.export_pdf(id, window, cx)); })))
                .child(item("shared-row-rename", "square-pen", tr("重命名", "Rename"))
                    .when(!editable, |d| d.opacity(0.4).cursor_default())
                    .when(editable, |d| d.on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.with_shared_document(rename.clone(), window, cx, |w, id, window, cx| { if matches!(w.sharing_access(cx), Some(Access::Owner | Access::ReadWrite)) { w.start_rename(id, window, cx); } }); }))))
                .when(owner, |d| d.child(item("shared-row-manage", "share", tr("成员与权限…", "Members and access…"))
                    .on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.open_member_management(manage.clone(), window, cx); }))))
                .child(div().mx(px(8.)).my(px(5.)).h(px(1.)).bg(Theme::line_border()))
                .child(item("shared-row-remove", "bin", if owner { tr("删除共享文档…", "Delete shared document…") } else { tr("离开共享…", "Leave share…") })
                    .text_color(Theme::danger()).on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.open_end_shared(room.clone(), window, cx); })));
        }
        let position = if menu.pos.x < px(0.) { point(-menu.pos.x, menu.pos.y + px(22.)) } else { menu.pos };
        vec![deferred(anchored().anchor(menu.corner).position(position).snap_to_window_with_margin(px(8.)).child(popover.max_h(window.viewport_size().height - px(16.)).overflow_y_scroll())).with_priority(2).into_any_element()]
    }
}
