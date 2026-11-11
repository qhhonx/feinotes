//! 回收站 page and the confirmation dialogs (删除, 恢复, 彻底删除).
//! Deleted pages wait in the library's `trash/` for 30 days; each row is a
//! deleted page together with the child pages that went with it.

use crate::app::{Confirm, Workspace};
use crate::settings::display_title;
use crate::search::doc_icon;
use crate::store::{TRASH_DAYS, TrashEntry};
pub(crate) use crate::ui::{close_button, dialog, list_header_text, list_line, menu_label, primary_button, row_hover, scrim, secondary_button};
use crate::theme::{self, Theme, craft};
use crate::ui::{ROW_RADIUS, TipExt, TitleBarExt, icon, popover, ui_para, ui_text};
use gpui::{prelude::FluentBuilder, *};

pub const TRASH_MENU_W: f32 = 160.;

/// 剩余时间: whole days left before the automatic 彻底删除.
fn remaining(deleted: u64) -> String {
    let end = deleted + TRASH_DAYS * 86_400;
    let left = end.saturating_sub(crate::time::now());
    if left < 86_400 { crate::settings::text("即将彻底删除").into() } else { crate::localized_format!("{} 天", "{} days", left.div_ceil(86_400)) }
}

/// The page header, as tall as the document's so the header pills sit in
/// the same place: a 17pt semibold view title, and the open-sidebar
/// button when the sidebar is hidden.
pub(crate) fn page_header(ws: &Workspace, title: &'static str, left: f32, cx: &mut Context<Workspace>) -> Stateful<Div> {
    page_header_with(ws, div().text_size(px(17.)).line_height(px(24.)).font_weight(FontWeight::SEMIBOLD).child(crate::settings::label(title)), left, cx)
}

/// `page_header` with any title element (a folder page puts its breadcrumbs there).
pub(crate) fn page_header_with(ws: &Workspace, title: impl IntoElement, left: f32, cx: &mut Context<Workspace>) -> Stateful<Div> {
    div()
        .id("page-header")
        .h(px(crate::app::HEADER_H))
        .flex_none()
        .flex()
        .items_center()
        // Clear of the traffic lights, like the document header.
        .pl(if ws.sidebar_open { px(left) } else { px(theme::TRAFFIC_LIGHTS_W) })
        .pr(px(crate::ui::header_right_inset()))
        .title_bar()
        .when_some(crate::ui::toolbar_color("toolbar.bg"), |d, c| d.bg(c))
        .when(!ws.sidebar_open && cfg!(target_os = "windows"), |d| d.child(div().mr(px(12.)).child(ws.header_navigation(cx))))
        .when(!ws.sidebar_open && !cfg!(target_os = "windows"), |d| {
            d.child(
                crate::ui::toolbar_control(div()
                    .id("open-sidebar")
                    .no_drag()
                    .flex_none()
                    .ml(px(4.))
                    .mr(px(12.))
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer(), 8., Theme::fill_hover_strong, None)
                    .tip(crate::settings::tr("展开侧栏", "Show sidebar"), Some(crate::shortcuts::hint("workspace::ToggleSidebar", "⌘ + ]")))
                    .child(crate::ui::toolbar_icon("panel-left-open", 17.))
                    .on_click(cx.listener(|w, _, _, cx| {
                        w.sidebar_open = true;
                        cx.notify();
                    })),
            )
        })
        .child(title)
        .child(div().flex_1())
}

/// Centred empty-state illustration and caption.
pub(crate) fn empty_state(illus: &'static str, text: &'static str) -> Div {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .pb(px(64.))
        .child(img(illus).size(px(120.)))
        .child(div().mt(px(16.)).text_size(px(13.)).line_height(px(20.)).text_color(Theme::text_caption()).child(text))
}

impl Workspace {
    /// Where a trashed page goes back to: its parent if that page is live.
    fn trash_location(&self, t: &TrashEntry) -> String {
        t.parent
            .as_ref()
            .and_then(|p| self.entries.iter().find(|e| &e.id == p))
            .map_or_else(|| crate::settings::label("我的文档"), |e| display_title(&e.title))
    }

    pub(crate) fn render_trash(&mut self, cx: &mut Context<Self>) -> AnyElement {
        // 清空回收站: a labelled button, only when there is something to empty.
        let empty = (!self.trash.is_empty()).then(|| div().id("trash-empty").no_drag().flex_none().mr(px(8.)).h(px(28.)).px(px(10.)).flex().items_center().gap(px(6.))
            .rounded(px(9.)).cursor_pointer().hover(|s| s.bg(Theme::fill_hover())).active(|s| s.bg(Theme::fill_pressed()))
            .text_size(px(13.)).line_height(px(20.)).text_color(Theme::text())
            .child(icon("bin", 14., Theme::icon())).child(crate::settings::text("清空回收站"))
            .on_click(cx.listener(|w, _, _, cx| { w.trash_menu = None; w.confirm = Some(Confirm::EmptyTrash); cx.notify(); })));
        let header = page_header(self, "回收站", 26., cx).children(empty).child(
            crate::ui::header_icon("trash-search", "search")
                .tip_label(crate::shortcuts::keys(crate::settings::text("搜索(⌘ + J)")).into(), crate::ui::TipSide::Below)
                .on_click(cx.listener(|_, _, window, cx| window.dispatch_action(Box::new(crate::app::FocusSearch), cx))),
        );
        let mut page = div().flex_1().min_w_0().h_full().flex().flex_col().child(header);
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        let recoveries = self.render_offline_recoveries(cx);
        #[cfg(not(all(target_os = "macos", feature = "cloudkit-sync")))]
        let recoveries: Option<Div> = None;
        if self.trash.is_empty() && recoveries.is_none() {
            return page.child(empty_state("illus/empty-trash.svg", crate::settings::text("回收站暂无内容"))).into_any_element();
        }
        // The list: 13pt tertiary headers over a hairline,
        // 15pt titles, 13pt secondary cells.
        let col = |w: Option<f32>, label: &'static str| {
            div()
                .when_some(w, |d, w| d.w(px(w)).flex_none())
                .when(w.is_none(), |d| d.flex_1().min_w_0())
                .pl(px(20.))
                .text_size(px(13.))
                .line_height(px(24.))
                .text_color(list_header_text())
                .child(crate::settings::label(label))
        };
        let table_header = div()
            .h(px(33.))
            .flex_none()
            .mx(px(24.))
            .flex()
            .items_center()
            .border_b_1()
            .border_color(list_line())
            .child(col(None, "标题"))
            .child(col(Some(168.), "位置"))
            .child(col(Some(168.), "剩余时间"))
            .child(div().w(px(48.)).flex_none());
        let mut list = div().id("trash-list").flex_1().min_h_0().overflow_y_scroll().px(px(24.)).pb(px(24.));
        for (i, t) in self.trash.iter().enumerate() {
            let (id, id2) = (t.id.clone(), t.id.clone());
            let menu_open = self.trash_menu.as_ref().is_some_and(|(m, _, _)| *m == t.id);
            let title = display_title(&t.title);
            let more = div()
                .id(SharedString::from(format!("trash-more-{}", t.id)))
                .size(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|s| s.bg(Theme::fill_hover_strong()))
                .when(menu_open, |d| d.bg(Theme::fill_hover_strong()))
                .child(icon("ellipsis", 16., Theme::icon()))
                .on_click(cx.listener(move |w, ev: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    let p = ev.position();
                    w.trash_menu = Some((id.clone(), point(p.x + px(12.), p.y + px(14.)), Corner::TopRight));
                    cx.notify();
                }));
            list = list.child(
                div()
                    .id(SharedString::from(format!("trash-row-{}", t.id)))
                    .h(px(56.))
                    .flex()
                    .items_center()
                    .when(i > 0, |d| d.border_t_1().border_color(list_line()))
                    .hover(row_hover)
                    .when(menu_open, |d| d.bg(Theme::fill_hover()).border_color(transparent_black()).rounded(px(ROW_RADIUS)))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |w, ev: &MouseDownEvent, _, cx| {
                            w.trash_menu = Some((id2.clone(), ev.position, Corner::TopLeft));
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .pl(px(24.))
                            .flex()
                            .items_center()
                            .child(match &t.icon {
                                Some(e) => div().size(px(24.)).mr(px(10.)).flex_none().flex().items_center().justify_center().text_size(px(18.)).child(e.clone()),
                                None => doc_icon(),
                            })
                            .child(div().min_w_0().text_size(px(15.)).line_height(px(22.)).overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui_text(title))),
                    )
                    .child(
                        div()
                            .w(px(168.))
                            .flex_none()
                            .pl(px(16.))
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .child(icon("library", 16., Theme::icon()))
                            .child(div().min_w_0().text_size(px(13.)).text_color(Theme::text_caption()).overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui_text(self.trash_location(t)))),
                    )
                    .child(div().w(px(168.)).flex_none().pl(px(20.)).text_size(px(13.)).text_color(Theme::text_caption()).child(remaining(t.deleted)))
                    .child(div().w(px(48.)).flex_none().flex().justify_end().pr(px(12.)).child(more)),
            );
        }
        page = page.when(!self.trash.is_empty(), |page| page.child(table_header)).child(list.children(recoveries));
        page.into_any_element()
    }

    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    fn render_offline_recoveries(&self, cx: &mut Context<Self>) -> Option<Div> {
        use crate::settings::{button, tr};
        let recoveries = crate::cloud_sync::offline_recoveries(cx).ok()?;
        if recoveries.is_empty() { return None; }
        Some(div().mt(px(20.)).px(px(20.)).py(px(12.)).rounded(px(ROW_RADIUS))
            .border_1().border_color(list_line())
            .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(tr("离线恢复副本", "Offline Recovery Copies")))
            .child(div().mt(px(4.)).text_size(px(13.)).text_color(Theme::text_caption()).child(ui_para(tr(
                "原笔记已删除。这台 Mac 保留了删除时未包含的改动。",
                "The original notes were deleted. This Mac kept changes that were missing at deletion."))))
            .children(recoveries.into_iter().map(|(id, title)| {
                let discard = id.clone();
                div().mt(px(10.)).flex().flex_wrap().items_center().gap(px(8.))
                    .child(div().flex_1().min_w(px(120.)).max_w_full().text_size(px(14.)).child(ui_para(display_title(&title))))
                    .child(button(SharedString::from(format!("keep-recovery-{id}")), tr("另存为新笔记", "Save as New Note"))
                        .on_click(cx.listener(move |w, _, window, cx| {
                            w.resolve_offline_recovery(&id, true, window, cx);
                        })))
                    .child(button(SharedString::from(format!("discard-recovery-{discard}")), tr("删除副本", "Delete Copy"))
                        .on_click(cx.listener(move |w, _, _, cx| {
                            w.confirm = Some(Confirm::DiscardOfflineRecovery(discard.clone()));
                            cx.notify();
                        })))
            })))
    }

    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    fn resolve_offline_recovery(&mut self, id: &str, keep: bool, window: &mut Window, cx: &mut Context<Self>) {
        match crate::cloud_sync::resolve_offline_recovery(id, keep, cx) {
            Ok(Some(new)) => {
                self.entries = self.store.list();
                self.sort();
                self.open(&new, window, cx);
            }
            Ok(None) => cx.notify(),
            Err(error) => self.show_toast_kind(crate::ui::ToastKind::Error, &error.to_string(), cx),
        }
    }

    pub(crate) fn render_trash_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (id, pos, corner) = self.trash_menu.clone()?;
        // The context menu: 32pt rows on the quiet hover fill.
        let item = |key: &'static str, ic: &'static str, label: &'static str, color: Hsla| {
            let g = SharedString::from(key);
            crate::ui::menu_row(key, g.clone())
                .text_color(color)
                .child(icon(ic, 16., if color == Theme::icon() { Theme::icon() } else { color }))
                .child(menu_label(label, g, gpui::white()))
        };
        let (a, b) = (id.clone(), id);
        let menu = popover()
            .id("trash-menu")
            .occlude()
            // At least TRASH_MENU_W, wider when a label needs it (English
            // "Delete permanently" ran into the edge).
            .min_w(px(TRASH_MENU_W))
            .py(px(crate::ui::MENU_PAD_Y))
            .on_mouse_down_out(cx.listener(|w, _, _, cx| {
                w.trash_menu = None;
                cx.notify();
            }))
            .child(item("tm-restore", "restore", crate::settings::text("恢复"), Theme::text()).on_click(cx.listener(move |w, _, _, cx| {
                w.trash_menu = None;
                w.confirm = Some(Confirm::Restore(a.clone()));
                cx.notify();
            })))
            .child(item("tm-purge", "bin", crate::settings::text("彻底删除"), Theme::danger()).on_click(cx.listener(move |w, _, _, cx| {
                w.trash_menu = None;
                w.confirm = Some(Confirm::Purge(b.clone()));
                cx.notify();
            })));
        Some(
            deferred(anchored().anchor(corner).position(pos).snap_to_window_with_margin(px(8.)).child(menu))
                .with_priority(2)
                .into_any_element(),
        )
    }

    fn restore_doc(&mut self, id: &str, cx: &mut Context<Self>) {
        let file_before = self.store.track_file_changes();
        match self.store.restore(id) {
            Ok(moved) => {
                self.show_toast(if moved { crate::settings::text("恢复成功。部分文档由于原位置不存在，已恢复到“我的文档”") } else { crate::settings::text("恢复成功") }, cx);
            }
            Err(e) => {
                let detail = e.chain().map(|cause| crate::settings::diagnostic(&cause.to_string())).collect::<Vec<_>>().join(": ");
                let msg = crate::localized_format!("恢复失败：{}", "Could not restore document: {}", detail);
                self.show_toast_kind(crate::ui::ToastKind::Error, &msg, cx);
                self.status = crate::app::SaveStatus::Failed(msg);
            }
        }
        self.entries = self.store.list();
        self.sort();
        self.sync_sub_pages(cx);
        self.trash = self.store.list_trash();
        self.show_store_warnings(cx);
        self.remember_files(file_before, crate::settings::tr("恢复", "Restore"));
    }

    fn purge_doc(&mut self, id: &str, cx: &mut Context<Self>) {
        match self.store.purge(id) {
            Ok(()) => self.show_toast(crate::settings::text("已彻底删除"), cx),
            Err(e) => self.status = crate::app::SaveStatus::Failed(crate::localized_format!("删除失败：{}", "Could not delete document: {}", crate::settings::diagnostic(&e.to_string()))),
        }
        self.trash = self.store.list_trash();
        self.show_store_warnings(cx);
    }

    /// 清空回收站: purges every trashed page; stops at the first failure.
    fn empty_trash(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<String> = self.store.list_trash().into_iter().map(|t| t.id).collect();
        match ids.iter().try_for_each(|id| self.store.purge(id)) {
            Ok(()) => self.show_toast(crate::settings::text("已清空回收站"), cx),
            Err(e) => self.status = crate::app::SaveStatus::Failed(crate::localized_format!("删除失败：{}", "Could not delete document: {}", crate::settings::diagnostic(&e.to_string()))),
        }
        self.trash = self.store.list_trash();
        self.show_store_warnings(cx);
    }

    fn show_store_warnings(&mut self, cx: &mut Context<Self>) {
        let warnings = self.store.take_warnings();
        if !warnings.is_empty() {
            let msg = warnings.iter().map(|s| crate::settings::diagnostic(s)).collect::<Vec<_>>().join(crate::settings::tr("；", "; "));
            self.show_toast_kind(crate::ui::ToastKind::Error, &msg, cx);
            self.status = crate::app::SaveStatus::Failed(msg);
        }
    }

    /// The confirmation dialog: a modal (white, radius 16), a 15pt
    /// semibold title, one line of 13pt body, 取消 and the action button on
    /// the right.
    pub(crate) fn render_confirm(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let c = self.confirm.as_ref()?;
        let title_of = |id: &str| {
            self.entries
                .iter()
                .find(|e| e.id == id)
                .map(|e| display_title(&e.title))
                .or_else(|| self.trash.iter().find(|t| t.id == id).map(|t| display_title(&t.title)))
                .unwrap_or_default()
        };
        let (title, body, action, danger) = match c {
            Confirm::Delete { ids, batch } => (
                if *batch || ids.len() > 1 { crate::localized_format!("是否删除所选的 {} 项？", "Delete the {} selected items?", ids.len()) } else { crate::localized_format!("是否删除： {}？", "Delete {}?", title_of(&ids[0])) },
                crate::settings::text("删除的内容将进入回收站，30 天后自动彻底删除").to_string(),
                crate::settings::text("删除"),
                true,
            ),
            Confirm::Restore(_) => (crate::settings::text("确认恢复？").to_string(), crate::settings::text("确认后将恢复至原位置").to_string(), crate::settings::text("恢复"), false),
            Confirm::EmptyTrash => (
                crate::settings::text("确认清空回收站？").to_string(),
                crate::localized_format!("回收站中的 {} 项将被彻底删除，无法恢复", "The {} items in the Trash will be deleted permanently. This cannot be undone.", self.trash.len()),
                crate::settings::text("清空"),
                true,
            ),
            Confirm::Purge(_) => (crate::settings::text("确认彻底删除？").to_string(), crate::settings::text("彻底删除后，该页面及其子页面将无法恢复").to_string(), crate::settings::text("删除"), true),
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            Confirm::DiscardOfflineRecovery(_) => (
                crate::settings::tr("删除离线恢复副本？", "Delete Offline Recovery Copy?").into(),
                crate::settings::tr("此副本将从这台 Mac 移除。", "This copy will be removed from this Mac.").into(),
                crate::settings::text("删除"), true,
            ),
            Confirm::Refs { show: false, .. } => (
                crate::settings::text("是否隐藏引用列表？").to_string(),
                crate::settings::text("选择隐藏后，协作者将无法查看本文的引用列表。如需重新显示，可点击文档右上角的更多按钮“...”，打开“显示引用列表”开关。").to_string(),
                crate::settings::text("隐藏"),
                false,
            ),
            Confirm::Refs { show: true, .. } => (crate::settings::text("是否显示引用列表？").to_string(), crate::settings::text("选择显示后，协作者可再次查看本文的引用列表。").to_string(), crate::settings::text("显示"), false),
            Confirm::SaveFailed => (
                crate::settings::text("文档保存失败").to_string(),
                crate::localized_format!("{}。可以重试或另存副本；重新载入将放弃未保存的修改并载入磁盘内容。", "{}. Retry or save a copy. Reload discards unsaved changes and loads the document from disk.", self.save_error()),
                crate::settings::text("重试"),
                false,
            ),
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            Confirm::StartCloudTrial => (
                crate::settings::tr("开始 7 天试用？", "Start a 7-day trial?").into(),
                crate::settings::tr("确认后开始计时。同一 iCloud 账户的设备共用这 7 天。试用不会自动续费，本机笔记始终免费。",
                    "The trial starts when you confirm and is shared across devices on this iCloud account. It won’t renew automatically. Local notes stay free.").into(),
                crate::settings::tr("开始试用", "Start Trial"),
                false,
            ),
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            Confirm::SwitchCloudLibrary { cloud: true, .. } => (
                crate::settings::tr("开启 iCloud 同步？", "Enable iCloud sync?").into(),
                crate::settings::tr("飞记将自动重启并打开 iCloud 文档库。本机笔记会保留。",
                    "Fei Notes will restart and open your iCloud library. Your local notes will be kept on this Mac.").into(),
                crate::settings::tr("开启同步", "Enable Sync"),
                false,
            ),
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            Confirm::SwitchCloudLibrary { cloud: false, .. } => (
                crate::settings::tr("打开本机文档库？", "Open the local library?").into(),
                crate::settings::tr("飞记将自动重启并打开本机文档库。iCloud 笔记和待上传的改动会保留。",
                    "Fei Notes will restart and open your local library. Your iCloud notes and pending changes are kept.").into(),
                crate::settings::tr("切换到本机", "Switch to Local"),
                false,
            ),
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            Confirm::RestoreLibraryBackup(prepared) => {
                let info = &prepared.summary;
                (crate::settings::tr("恢复已校验的备份？", "Restore this verified backup?").into(),
                    crate::localized_format!(
                        "备份包含 {} 篇笔记、{} 个文件夹、回收站 {} 项、共享笔记 {} 篇和 {} 条待上传改动。飞记将自动重启并恢复备份，原文档库会保留。",
                        "The backup contains {} notes, {} folders, {} items in Trash, {} shared notes and {} pending uploads. Fei Notes will restart to restore the backup and keep your current library.",
                        info.notes, info.folders, info.trash, info.shared, info.pending),
                    crate::settings::tr("恢复", "Restore"), false)
            }
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            Confirm::CloudVersions { .. } => (
                crate::settings::tr("iCloud 历史版本", "iCloud Version History").into(),
                crate::settings::tr("选择一个版本，恢复为一篇新的笔记。", "Choose a version to restore as a new note.").into(),
                crate::settings::tr("恢复副本", "Restore a Copy"),
                false,
            ),
        };
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        let copy_choice = if let Confirm::SwitchCloudLibrary { cloud: true, copy_local } = c {
            Some(div().id("confirm-copy-local").mt(px(14.)).flex().items_center().gap(px(8.))
                .cursor_pointer().text_size(px(13.)).line_height(px(20.))
                .child(crate::ui::checkbox(*copy_local))
                .child(ui_para(crate::settings::tr("同时复制本机笔记到 iCloud", "Also copy local notes to iCloud")))
                .on_click(cx.listener(|w, _, _, cx| {
                    if let Some(Confirm::SwitchCloudLibrary { copy_local, .. }) = &mut w.confirm { *copy_local = !*copy_local; }
                    cx.notify();
                })))
        } else { None };
        #[cfg(not(all(target_os = "macos", feature = "cloudkit-sync")))]
        let copy_choice = None::<Stateful<Div>>;
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        let (version_list, ready) = if let Confirm::CloudVersions { versions, selected, .. } = c {
            let list = div().id("cloud-version-list").mt(px(12.)).max_h(px(220.)).overflow_y_scroll()
                .children(versions.iter().map(|(sequence, time, preview)| {
                    let sequence = *sequence;
                    let active = *selected == Some(sequence);
                    div().id(SharedString::from(format!("cloud-version-{sequence}"))).flex().items_center().gap(px(8.))
                        .px(px(8.)).py(px(6.)).rounded(px(8.)).cursor_pointer().hover(row_hover)
                        .child(crate::transfer::radio(active))
                        .child(div().flex_1().min_w_0().text_size(px(13.)).line_height(px(20.))
                            .child(time.clone())
                            .child(div().truncate().text_size(px(12.)).text_color(Theme::text_caption()).child(preview.clone())))
                        .on_click(cx.listener(move |w, _, _, cx| {
                            if let Some(Confirm::CloudVersions { selected, .. }) = &mut w.confirm { *selected = Some(sequence); cx.notify(); }
                        }))
                }));
            (Some(list), selected.is_some())
        } else { (None, true) };
        #[cfg(not(all(target_os = "macos", feature = "cloudkit-sync")))]
        let (version_list, ready) = (None::<Stateful<Div>>, true);
        let no_ask = match c {
            Confirm::Refs { show: false, no_ask } => Some(*no_ask),
            _ => None,
        };
        let ok = primary_button("confirm-ok", action);
        let ok = if danger { ok.bg(Theme::danger()).hover(|s| s.bg(theme::c(craft::R600))).active(|s| s.bg(theme::c(craft::R700))) } else { ok };
        let max_width = px(if matches!(c, Confirm::SaveFailed) || version_list.is_some() { 520. } else { 400. });
        let width = (window.viewport_size().width - px(32.)).min(max_width);
        let dialog = dialog("confirm-dialog")
            .w(width)
            .px(px(24.))
            .pt(px(20.))
            .pb(px(24.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(div().text_size(px(15.)).line_height(px(22.)).font_weight(FontWeight::SEMIBOLD).child(ui_para(title)))
            .child(div().mt(px(8.)).text_size(px(13.)).line_height(px(20.)).text_color(Theme::text()).child(ui_para(body)))
            .children(copy_choice)
            .children(version_list)
            .child(
                div()
                    .mt(px(24.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(div().flex_1().children(no_ask.map(|on| {
                        div()
                            .id("confirm-no-ask")
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .cursor_pointer()
                            .text_size(px(13.))
                            .on_click(cx.listener(|w, _, _, cx| {
                                if let Some(Confirm::Refs { no_ask, .. }) = &mut w.confirm {
                                    *no_ask = !*no_ask;
                                }
                                cx.notify();
                            }))
                            .child(crate::ui::checkbox(on))
                            .child(ui_text(crate::settings::text("不再提示")))
                    })))
                    .child(secondary_button("confirm-cancel", crate::settings::text("取消")).on_click(cx.listener(|w, _, _, cx| {
                        w.confirm = None;
                        w.after_save = None;
                        cx.notify();
                    })))
                    .when(matches!(c, Confirm::SaveFailed), |d| {
                        d.child(secondary_button("confirm-save-copy", crate::settings::tr("另存副本", "Save a copy")).on_click(cx.listener(|w, _, window, cx| {
                            w.save_failed_copy(window, cx);
                        })))
                        .child(secondary_button("confirm-discard", crate::settings::tr("重新载入", "Reload")).text_color(Theme::danger()).on_click(cx.listener(|w, _, window, cx| {
                            w.confirm = None;
                            w.resolve_save_failed(false, window, cx);
                        })))
                    })
                    .child(ok.when(!ready, |b| b.opacity(0.45).cursor_default()).when(ready, |b| b.on_click(cx.listener(|w, _, window, cx| {
                        let Some(c) = w.confirm.take() else { return };
                        match c {
                            Confirm::Delete { ids, batch } => {
                                if batch {
                                    w.batch = None;
                                }
                                w.delete_docs(&ids, window, cx);
                            }
                            Confirm::Restore(id) => w.restore_doc(&id, cx),
                            Confirm::Purge(id) => w.purge_doc(&id, cx),
                            Confirm::EmptyTrash => w.empty_trash(cx),
                            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
                            Confirm::DiscardOfflineRecovery(id) => w.resolve_offline_recovery(&id, false, window, cx),
                            Confirm::Refs { show, no_ask } => {
                                if no_ask {
                                    let mut s = w.store.settings();
                                    s.refs_hide_no_ask = true;
                                    let _ = w.store.save_settings(&s);
                                }
                                w.set_refs_hidden(!show, cx);
                            }
                            Confirm::SaveFailed => w.resolve_save_failed(true, window, cx),
                            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
                            Confirm::StartCloudTrial => w.trial_action(true, window, cx),
                            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
                            Confirm::SwitchCloudLibrary { cloud, copy_local } => w.select_library(match (cloud, copy_local) {
                                (true, true) => crate::cloud_restore::LibrarySelection::CopyToCloud,
                                (true, false) => crate::cloud_restore::LibrarySelection::Cloud,
                                (false, _) => crate::cloud_restore::LibrarySelection::Local,
                            }, cx),
                            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
                            Confirm::RestoreLibraryBackup(prepared) => w.restore_library_backup(*prepared, cx),
                            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
                            Confirm::CloudVersions { id, selected, .. } => {
                                if let Some(sequence) = selected { w.restore_cloud_version(&id, sequence, window, cx); }
                            }
                        }
                        cx.notify();
                    })))),
            );
        Some(
            deferred(
                div()
                    .id("confirm-mask")
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(scrim())
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(dialog),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }
}
