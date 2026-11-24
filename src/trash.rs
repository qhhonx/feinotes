//! 回收站 page and the confirmation dialogs (删除, 恢复, 彻底删除).
//! Deleted pages wait in the library's `trash/` for 30 days; each row is a
//! deleted page together with the child pages that went with it.

use crate::app::{Confirm, ConfirmCancel, ConfirmOk, Workspace};
use crate::settings::display_title;
use crate::search::doc_icon;
use crate::store::{TRASH_DAYS, TrashEntry};
pub(crate) use crate::ui::{ToastKind, list_header_text, list_line, row_hover};
use crate::theme::{self, Theme};
use crate::ui::{ROW_RADIUS, TipExt, TitleBarExt, icon, ui_para, ui_text};
use crate::design::{Button, ButtonSize, Checkbox, Dialog};
use gpui::{prelude::FluentBuilder, *};

pub const TRASH_MENU_W: f32 = 160.;

struct ListedTrash { lists: Vec<(String, Vec<TrashEntry>)>, warnings: Vec<String> }
struct PurgedTrash { errors: Vec<String> }

/// 剩余时间: whole days left before the automatic 彻底删除.
pub(crate) fn remaining(deleted: u64) -> String {
    let end = deleted + TRASH_DAYS * 86_400;
    let left = end.saturating_sub(crate::time::now());
    if left < 86_400 { crate::settings::text("即将彻底删除").into() } else { crate::localized_count!(left.div_ceil(86_400), "{} 天", "{} day", "{} days", left.div_ceil(86_400)) }
}

pub(crate) fn retention_note() -> String {
    crate::localized_format!("删除的文档保留 {} 天后自动清除。", "Deleted documents are automatically removed after {} days.", crate::store::TRASH_DAYS)
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
                Button::ghost("open-sidebar")
                    .no_drag()
                    .ml(px(4.))
                    .mr(px(12.))
                    .icon("panel-left-open")
                    .tip(crate::settings::tr("展开侧栏", "Show sidebar"), Some(crate::shortcuts::hint("workspace::ToggleSidebar", "⌘ + ]")))
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

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
pub(crate) fn backup_confirmation(prepared: &crate::cloud_sync::backup::Prepared) -> (String,String,&'static str,bool) {
    let info = &prepared.summary;
    (crate::settings::tr("从整库文件恢复？", "Restore from this library file?").into(),
        crate::localized_format!(
            "整库文件包含 {} 篇文档、{} 个文件夹、回收站 {} 项、共享文档 {} 篇和 {} 条待上传改动。恢复前的文档库会保留。",
            "The library file contains {} documents, {} folders, {} items in Trash, {} shared documents and {} pending uploads. The library before restoration will be kept.",
            info.notes,info.folders,info.trash,info.shared,info.pending),crate::settings::tr("恢复","Restore"),false)
}

impl Workspace {
    /// Where a trashed page goes back to: its parent if that page is live.
    pub(crate) fn trash_location(&self, t: &TrashEntry) -> String {
        t.parent
            .as_ref()
            .and_then(|p| self.entries.iter().find(|e| &e.id == p))
            .map_or_else(|| crate::settings::label("我的文档"), |e| display_title(&e.title))
    }

    pub(crate) fn render_trash(&mut self, cx: &mut Context<Self>) -> AnyElement {
        // 清空回收站: a labelled button, only when there is something to empty.
        let empty = (!self.trash.is_empty()).then(|| Button::ghost("trash-empty").no_drag().mr(px(8.)).icon("bin").label(crate::settings::text("清空回收站"))
            .on_click(cx.listener(|w, _, _, cx| { w.trash_menu = None; w.confirm = Some(Confirm::EmptyTrash); cx.notify(); })));
        let header = page_header(self, "回收站", 26., cx).children(empty).child(
            crate::ui::header_icon("trash-search", "search")
                .tip_label(crate::shortcuts::keys(crate::settings::text("搜索(⌘ + J)")).into(), crate::ui::TipSide::Below)
                .on_click(cx.listener(|_, _, window, cx| window.dispatch_action(Box::new(crate::app::FocusSearch), cx))),
        );
        let mut page = div().flex_1().min_w_0().h_full().flex().flex_col().child(header)
            .when(cfg!(all(target_os = "macos", feature = "cloudkit-sync")), |d| d.child(div().mx(px(26.)).pb(px(10.))
                .text_size(px(13.)).line_height(px(19.)).text_color(Theme::text_caption()).child(ui_para(retention_note()))));
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let recoveries = self.render_offline_recoveries(cx);
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
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
            let more = Button::ghost(SharedString::from(format!("trash-more-{}", t.id)))
                .sized(ButtonSize::Xs)
                .icon("ellipsis")
                .selected(menu_open)
                .on_click(cx.listener(move |w, ev: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    let p = ev.position();
                    w.trash_menu = Some((id.clone(), point(p.x + px(12.), p.y + px(14.)), Anchor::TopRight));
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
                            w.trash_menu = Some((id2.clone(), ev.position, Anchor::TopLeft));
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
                            .child(if t.folder {
                                let glyph = if t.share_root.is_some() { "folder-shared" } else { "folder" };
                                div().size(px(24.)).mr(px(10.)).flex_none().flex().items_center().justify_center()
                                    .child(icon(glyph, 18., Theme::icon()))
                            } else {
                                match &t.icon {
                                    Some(e) => div().size(px(24.)).mr(px(10.)).flex_none().flex().items_center().justify_center().text_size(px(18.)).child(e.clone()),
                                    None => doc_icon(),
                                }
                            })
                            .child(crate::ui::document_title(&title).text_size(px(15.)).line_height(px(22.)).child(ui_text(title)))
                            .when(!t.folder && t.share_root.is_some(), |d| {
                                d.child(crate::sidebar::shared_document_badge().ml(px(6.)))
                            }),
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

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn render_offline_recoveries(&self, cx: &mut Context<Self>) -> Option<Div> {
        use crate::settings::{button, tr};
        let recoveries = crate::cloud_sync::offline_recoveries(self.store.root(), cx).ok()?;
        if recoveries.is_empty() { return None; }
        Some(div().mt(px(20.)).px(px(20.)).py(px(12.)).rounded(px(ROW_RADIUS))
            .border_1().border_color(list_line())
            .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(tr("离线恢复副本", "Offline Recovery Copies")))
            .child(div().mt(px(4.)).text_size(px(13.)).text_color(Theme::text_caption()).child(ui_para(tr(
                "可以将这台设备保留的离线改动另存为个人文档。",
                "Save offline changes kept on this device as a personal document."))))
            .children(recoveries.into_iter().map(|(id, title)| {
                let discard = id.clone();
                div().mt(px(10.)).flex().flex_wrap().items_center().gap(px(8.))
                    .child(div().flex_1().min_w(px(120.)).max_w_full().text_size(px(14.)).child(ui_para(display_title(&title))))
                    .child(button(SharedString::from(format!("keep-recovery-{id}")), tr("另存为新文档", "Save as New Document"))
                        .on_click(cx.listener(move |w, _, window, cx| {
                            w.resolve_offline_recovery(&id, true, window, cx);
                        })))
                    .child(button(SharedString::from(format!("discard-recovery-{discard}")), tr("移除恢复项", "Remove Recovery Item"))
                        .on_click(cx.listener(move |w, _, _, cx| {
                            w.confirm = Some(Confirm::DiscardOfflineRecovery(discard.clone()));
                            cx.notify();
                        })))
            })))
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    pub(crate) fn resolve_offline_recovery(&mut self, id: &str, keep: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_shared() || self.library_switching || self.store.file_task_busy() { return; }
        match crate::cloud_sync::resolve_offline_recovery(self.store.root(), id, keep, cx) {
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
        let item = |key: &'static str, ic: &'static str, label: &'static str, danger: bool| crate::design::MenuItem::new(key).icon(ic).label(crate::settings::label(label)).destructive(danger);
        let (a, b) = (id.clone(), id);
        let menu = crate::design::Popover::new("trash-menu")
            .menu()
            // At least TRASH_MENU_W, wider when a label needs it (English
            // "Delete permanently" ran into the edge).
            .min_w(px(TRASH_MENU_W))
            .on_mouse_down_out(cx.listener(|w, _, _, cx| {
                w.trash_menu = None;
                cx.notify();
            }))
            .child(item("tm-restore", "restore", crate::settings::text("恢复"), false).on_click(cx.listener(move |w, _, _, cx| {
                w.trash_menu = None;
                w.confirm = Some(Confirm::Restore(a.clone()));
                cx.notify();
            })))
            .child(item("tm-purge", "bin", crate::settings::text("彻底删除"), true).on_click(cx.listener(move |w, _, _, cx| {
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

    pub(crate) fn restore_doc(&mut self, id: &str, cx: &mut Context<Self>) {
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
        let success = crate::settings::text("已彻底删除");
        self.purge_trash(vec![id.to_string()], success, cx);
    }

    /// Purge personal Trash and route active linked items to their authoritative folders.
    fn empty_trash(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let (personal, linked): (Vec<_>, Vec<_>) = self.trash.iter().partition(|item| {
            if item.share_root.is_none() { return true; }
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            { self.detach_ended_trash_document(item) }
            #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
            { false }
        });
        let ids = personal.into_iter().map(|item| item.id.clone()).collect();
        let success = if !linked.is_empty() { crate::settings::tr("已清空个人回收站", "Personal Trash cleared") }
            else { crate::settings::text("已清空回收站") };
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        {
            let mut groups = std::collections::BTreeMap::<String, Vec<String>>::new();
            for item in linked {
                if let Some(scope) = &item.share_root { groups.entry(scope.clone()).or_default().push(item.id.clone()); }
            }
            for (scope, ids) in groups {
                if let Some(room) = self.linked_trash_room(&scope, cx) {
                    self.change_shared_trash_items(room, ids, crate::sharing::SharedTrashAction::Purge, _window, cx);
                } else {
                    self.show_toast_kind(crate::ui::ToastKind::Error, crate::settings::tr(
                        "部分共享项目当前无法删除，请确认其共享状态后重试。", "Some shared items cannot be deleted. Check their sharing status and try again."), cx);
                }
            }
        }
        self.purge_trash(ids, success, cx);
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn handle_linked_trash(&mut self, id: &str, action: crate::sharing::SharedTrashAction, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(item) = self.trash.iter().find(|item| item.id == id) else { return false; };
        let Some(scope) = item.share_root.as_ref() else { return false; };
        if self.detach_ended_trash_document(item) { return false; }
        if let Some(room) = self.linked_trash_room(scope, cx) { self.change_shared_trash(room, id.into(), action, window, cx); }
        else {
            let message = if scope == id { crate::settings::tr(
                "请确认此项目已停止共享，再操作回收站。", "Confirm that sharing has ended before changing this Trash item.") }
            else { crate::settings::tr("共享文件夹暂不可用，请稍后重试。", "The shared folder is unavailable. Try again shortly.") };
            self.show_toast_kind(crate::ui::ToastKind::Error, message, cx);
        }
        true
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn detach_ended_trash_document(&self, item: &TrashEntry) -> bool {
        item.share_root.as_deref() == Some(item.id.as_str())
            && crate::cloud_sync::shared::detach_ended_standalone_trash(self.store.root(), &item.id)
                .inspect_err(|error| crate::logging::write(&format!("Detach ended Trash document: {error:#}"))).unwrap_or(false)
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn linked_trash_room(&self, scope: &str, cx: &App) -> Option<crate::cloud_sync::shared::Room> {
        crate::cloud_sync::shared::owned_trash_room(self.store.root(), scope)
            .inspect_err(|error| crate::logging::write(&format!("Find shared Trash folder: {error:#}"))).ok().flatten()
            .or_else(|| self.shared_list_room(scope, cx).filter(|room| room.scope.folder && room.scope.owner.is_none()))
    }

    fn purge_trash(&mut self, ids: Vec<String>, success: &'static str, cx: &mut Context<Self>) {
        if ids.is_empty() { return; }
        if self.file_history.busy || self.tree_writes_pending() || self.store.file_task_busy() {
            self.show_toast_kind(crate::ui::ToastKind::Info, crate::settings::tr(
                "文档库正在处理文件，请稍后再试。", "The library is processing files. Try again shortly."), cx);
            return;
        }
        let Some(task_guard) = self.store.begin_file_task() else { return };
        // Each root's 回收站 is its own: one writer, and one write, per root.
        let mut writers: Vec<(String, crate::store::Store, Vec<String>)> = vec![];
        for id in &ids {
            let Some(key) = self.store.trash_root(id) else { continue };
            match writers.iter_mut().find(|(k, _, _)| *k == key) {
                Some((_, _, v)) => v.push(id.clone()),
                None => if let Some(w) = self.store.root_writer(&key) { writers.push((key, w, vec![id.clone()])); },
            }
        }
        let task = cx.background_executor().spawn(async move {
            let (mut errors, mut lists, mut warnings) = (vec![], vec![], vec![]);
            for (key, mut writer, gone) in writers {
                // The 回收站's index is written once, not once a page.
                errors.extend(writer.purge_many(&gone).err().map(|e| e.to_string()));
                warnings.extend(writer.take_warnings());
                lists.push((key, writer.list_trash()));
            }
            (PurgedTrash { errors }, ListedTrash { lists, warnings })
        });
        // The rows go now; the 回收站 read back afterwards replaces this.
        let ids: std::collections::HashSet<String> = ids.into_iter().collect();
        self.trash.retain(|t| !ids.contains(&t.id));
        self.trash_menu = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let (done, listed) = task.await;
            drop(task_guard);
            let _ = this.update(cx, |w, cx| {
                for (key, trash) in listed.lists { w.trash = w.store.merge_trash(&key, trash); }
                let mut errors = done.errors;
                errors.extend(listed.warnings.iter().map(|s| crate::settings::diagnostic(s)));
                #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                crate::cloud_sync::recheck_account(w.store.root(), cx);
                if errors.is_empty() { w.show_toast(success, cx); }
                else {
                    let msg = errors.iter().map(|e| crate::localized_format!("删除失败：{}", "Could not delete document: {}", crate::settings::diagnostic(e))).collect::<Vec<_>>().join("；");
                    w.show_toast_kind(crate::ui::ToastKind::Error, &msg, cx);
                    w.status = crate::app::SaveStatus::Failed(msg);
                }
                cx.notify();
            });
        }).detach();
    }

    /// What the unused-asset sweep (#237) reads besides the library: the
    /// sources every open editor holds — this window's and the others',
    /// quick notes included — that the disk does not (unsaved text, the
    /// snapshots Undo can still bring back), and the 本机文件 and 本机文件夹,
    /// whose pasted pictures are copied into the library's assets too.
    pub(crate) fn sweep_inputs(&self, window: &Window, cx: &Context<Self>) -> (Vec<String>, Vec<std::path::PathBuf>) {
        let mut live: Vec<String> = self.editor.as_ref().map(|e| e.read(cx).core.asset_refs()).unwrap_or_default();
        // This window is being updated: reading it again would panic.
        let me = window.window_handle().window_id();
        for w in cx.windows().into_iter().filter(|w| w.window_id() != me).filter_map(|w| w.downcast::<Workspace>()) {
            let Ok(ws) = w.entity(cx) else { continue };
            if let Some(e) = &ws.read(cx).editor { live.extend(e.read(cx).core.asset_refs()); }
        }
        let outside = self.entries.iter().filter_map(|e| e.meta.path.as_ref().map(std::path::PathBuf::from)).collect();
        (live, outside)
    }

    pub(crate) fn sweep_failed(&mut self, e: &anyhow::Error, cx: &mut Context<Self>) {
        log::warn!("unused assets: {e:#}");
        self.show_toast_kind(ToastKind::Error, &crate::localized_format!("无法读取文档库，未清理：{}", "Could not read the library; nothing was removed: {}", e), cx);
    }

    /// 设置 › 文档库 › 未使用资源 (#237): take away the listed files that are
    /// still unused. The sweep runs again first, off the UI thread: a page
    /// restored or a picture brought back by Undo since the list was made
    /// keeps its file.
    pub(crate) fn remove_unused_assets(&mut self, names: Vec<(String, u64)>, window: &Window, cx: &mut Context<Self>) {
        if crate::FEI_NOTES || self.is_shared() { return; }
        let root = crate::store::Store::root_dir();
        let (live, outside) = self.sweep_inputs(window, cx);
        cx.spawn(async move |this, cx| {
            let gone = cx.background_executor().spawn(async move {
                let still = crate::store::Store::unused_assets_at(root.clone(), live, outside)?;
                let names: Vec<String> = names.into_iter().map(|(n, _)| n).filter(|n| still.iter().any(|(s, _)| s == n)).collect();
                anyhow::Ok(crate::store::Store::remove_unused_assets(&root, &names))
            }).await;
            let _ = this.update(cx, |w, cx| match gone {
                Ok(0) => w.show_toast(crate::settings::text("没有可清理的资源"), cx),
                Ok(n) => w.show_toast_kind(ToastKind::Success, &crate::localized_format!("已移出 {} 个未使用资源", "Moved {} unused assets out of the asset folder", n), cx),
                Err(e) => w.sweep_failed(&e, cx),
            });
        }).detach();
    }

    fn show_store_warnings(&mut self, cx: &mut Context<Self>) {
        self.show_store_warnings_from(&self.store.take_warnings(), cx);
    }

    pub(crate) fn show_store_warnings_from(&mut self, warnings: &[String], cx: &mut Context<Self>) {
        if !warnings.is_empty() {
            let msg = warnings.iter().map(|s| crate::settings::diagnostic(s)).collect::<Vec<_>>().join(crate::settings::tr("；", "; "));
            self.show_toast_kind(crate::ui::ToastKind::Error, &msg, cx);
            self.status = crate::app::SaveStatus::Failed(msg);
        }
    }

    /// A dialog takes focus as it opens, so ↩ can't reach the document
    /// beneath, and gives it back where it was once it closes (unless what
    /// the dialog did moved it on).
    pub(crate) fn dismiss_confirm(&mut self, cx: &mut Context<Self>) {
        self.confirm = None;
        self.after_save = None;
        cx.notify();
    }

    #[cfg(not(target_os = "ios"))]
    pub(crate) fn sync_confirm_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match (self.confirm.is_some(), self.confirm_return.is_some()) {
            (true, false) => {
                self.confirm_return = Some(window.focused(cx).map(|f| f.downgrade()));
                window.focus(&self.confirm_focus, cx);
            }
            (false, true) => {
                let back = self.confirm_return.take().flatten().and_then(|f| f.upgrade());
                if !self.confirm_focus.is_focused(window) { return }
                match back.or_else(|| self.editor.as_ref().map(|e| e.read(cx).focus.clone())) {
                    Some(f) => window.focus(&f, cx),
                    None => window.focus(&self.focus, cx),
                }
            }
            _ => {}
        }
    }

    pub(crate) fn confirm_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = self.confirm.take() else { return };
        match c {
            Confirm::Delete { ids, batch } => {
                if batch {
                    self.batch = None;
                }
                if self.is_shared() && ids.iter().all(|id| !self.entries.iter().any(|e| &e.id == id)) {
                    self.personal_action(window, cx, move |w, window, cx| w.delete_docs(&ids, window, cx));
                } else { self.delete_docs(&ids, window, cx); }
            }
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            Confirm::DeleteSharedFolder { room, id, .. } => self.delete_shared_folder(room, id, window, cx),
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            Confirm::DeleteSharedDocument { room, id, .. } =>
                self.change_shared_trash(room, id, crate::sharing::SharedTrashAction::DeleteDocument, window, cx),
            Confirm::Restore(id) => {
                #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                if self.handle_linked_trash(&id, crate::sharing::SharedTrashAction::Restore, window, cx) { return; }
                self.restore_doc(&id, cx);
            }
            Confirm::Purge(id) => {
                #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                if self.handle_linked_trash(&id, crate::sharing::SharedTrashAction::Purge, window, cx) { return; }
                self.purge_doc(&id, cx);
            }
            Confirm::EmptyTrash => self.empty_trash(window, cx),
            Confirm::RemoveUnused(files) => self.remove_unused_assets(files, window, cx),
            Confirm::Migrate(_) => self.migrate_library(window,cx),
            Confirm::RemoveRoot(key, _) => self.remove_root(&key, window, cx),
            Confirm::Import { what,target } => self.import_dropped(what,target,window,cx),
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            Confirm::DiscardOfflineRecovery(id) => self.resolve_offline_recovery(&id, false, window, cx),
            Confirm::Refs { show, no_ask } => {
                if no_ask {
                    let mut s = self.store.settings();
                    s.refs_hide_no_ask = true;
                    let _ = self.store.save_settings(&s);
                }
                self.set_refs_hidden(!show, cx);
            }
            Confirm::SaveFailed => self.resolve_save_failed(true, window, cx),
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            Confirm::StartCloudTrial => self.trial_action(true, window, cx),
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            Confirm::SwitchCloudLibrary { cloud, copy_local } => self.select_library(match (cloud, copy_local) {
                (true, true) => crate::cloud_restore::LibrarySelection::CopyToCloud,
                (true, false) => crate::cloud_restore::LibrarySelection::Cloud,
                (false, _) => crate::cloud_restore::LibrarySelection::Local,
            }, cx),
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            Confirm::RestoreLibraryBackup(prepared) => self.restore_library_backup(*prepared, cx),
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            Confirm::CloudVersions { id, selected, .. } => {
                if let Some(sequence) = selected { self.restore_cloud_version(&id, sequence, window, cx); }
            }
        }
        cx.notify();
    }

    pub(crate) fn confirmation_text(&self) -> Option<(String,String,&'static str,bool)> {
        let c = self.confirm.as_ref()?;
        let title_of = |id: &str| {
            self.entries
                .iter()
                .chain(self.folder_drag.personal_rows.iter().flatten())
                .find(|e| e.id == id)
                .map(|e| display_title(&e.title))
                .or_else(|| self.trash.iter().find(|t| t.id == id).map(|t| display_title(&t.title)))
                .unwrap_or_default()
        };
        Some(match c {
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            Confirm::DeleteSharedFolder { room, id, title } => (
                crate::localized_format!("是否删除： {}？", "Delete {}?", display_title(title)),
                if id == &room.scope.note {
                    crate::settings::tr("删除会结束此文件夹的共享，内容将进入回收站。", "Deleting ends sharing for this folder and moves its contents to Trash.")
                } else {
                    crate::settings::tr("此文件夹及其内容将从协作成员的列表移除，并进入回收站。", "The folder and its contents will be removed for collaborators and moved to Trash.")
                }.into(),
                crate::settings::text("删除"),
                true,
            ),
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            Confirm::DeleteSharedDocument { title, .. } => (
                crate::localized_format!("是否删除： {}？", "Delete {}?", display_title(title)),
                crate::settings::tr("此文档将从协作成员的列表移除，并进入回收站。", "The document will be removed for collaborators and moved to Trash.").into(),
                crate::settings::text("删除"),
                true,
            ),
            Confirm::Delete { ids, batch } => (
                if *batch || ids.len() > 1 { crate::localized_count!(ids.len(), "是否删除所选的 {} 项？", "Delete the {} selected item?", "Delete the {} selected items?", ids.len()) } else { crate::localized_format!("是否删除： {}？", "Delete {}?", title_of(&ids[0])) },
                crate::settings::text("删除的内容将进入回收站，30 天后自动彻底删除").to_string(),
                crate::settings::text("删除"),
                true,
            ),
            Confirm::Restore(id) if self.trash.iter().any(|item| item.id == *id && item.folder) => (
                crate::settings::tr("恢复文件夹？", "Restore folder?").into(),
                crate::settings::tr("将恢复此文件夹和其中的项目。", "The folder and its contents will be restored.").into(),
                crate::settings::text("恢复"), false),
            Confirm::Restore(_) => (crate::settings::text("确认恢复？").to_string(), crate::settings::text("确认后将恢复至原位置").to_string(), crate::settings::text("恢复"), false),
            Confirm::EmptyTrash => (
                crate::settings::text("确认清空回收站？").to_string(),
                crate::localized_count!(self.trash.len(), "回收站中的 {} 项将被彻底删除，无法恢复", "The {} item in the Trash will be deleted permanently. This cannot be undone.", "The {} items in the Trash will be deleted permanently. This cannot be undone.", self.trash.len()),
                crate::settings::text("清空"),
                true,
            ),
            Confirm::Purge(id) if self.trash.iter().any(|item| item.id == *id && item.folder) => (
                crate::settings::tr("彻底删除文件夹？", "Permanently delete folder?").into(),
                crate::settings::tr("此文件夹及其中的项目将无法恢复。", "The folder and its contents cannot be recovered.").into(),
                crate::settings::text("删除"), true),
            Confirm::Purge(_) => (crate::settings::text("确认彻底删除？").to_string(), crate::settings::text("彻底删除后，该页面及其子页面将无法恢复").to_string(), crate::settings::text("删除"), true),
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            Confirm::DiscardOfflineRecovery(_) => (
                crate::settings::tr("移除离线恢复项？", "Remove Offline Recovery Item?").into(),
                crate::settings::tr("此恢复项将从这台设备的回收站移除。", "Remove this recovery item from Trash on this device.").into(),
                crate::settings::tr("移除", "Remove"), true,
            ),
            Confirm::RemoveUnused(files) => (
                crate::settings::text("清理未使用资源").to_string(),
                crate::localized_format!("{} 个文件（{}）将移到文档库的 .quiet/removed-assets 文件夹，{} 天后自动删除；在此之前可以从那里拖回 .assets。", "{} files ({}) will be moved to .quiet/removed-assets in the library and deleted after {} days; until then they can be dragged back to .assets.",
                    files.len(), crate::docinfo::format_size(files.iter().map(|(_, n)| n).sum()), crate::store::TRASH_DAYS),
                crate::settings::text("移出"),
                true,
            ),
            Confirm::Refs { show: false, .. } => (
                crate::settings::text("是否隐藏引用列表？").to_string(),
                crate::settings::text("选择隐藏后，协作者将无法查看本文的引用列表。如需重新显示，可点击文档右上角的更多按钮“...”，打开“显示引用列表”开关。").to_string(),
                crate::settings::text("隐藏"),
                false,
            ),
            Confirm::Refs { show: true, .. } => (crate::settings::text("是否显示引用列表？").to_string(), crate::settings::text("选择显示后，协作者可再次查看本文的引用列表。").to_string(), crate::settings::text("显示"), false),
            Confirm::Migrate(reason) => {
                use crate::store::{ConsentReason, DeviceHint, SyncService};
                let why = match reason {
                    ConsentReason::Synced(SyncService::Provider(p)) => crate::localized_format!("文档库在 {p} 同步的文件夹里。", "The library is in a folder synced by {p}. "),
                    ConsentReason::Synced(_) => crate::settings::tr("文档库在 iCloud 云盘里。", "The library is in iCloud Drive. ").to_string(),
                    ConsentReason::OtherDevice(DeviceHint::MissingFile(p)) => crate::localized_format!("这个文档库可能也在别的电脑上使用（打开过的本机文件 {p} 不在这台电脑上）。", "This library may also be used on another computer (a local file it opened, {p}, isn't on this one). "),
                    ConsentReason::OtherDevice(DeviceHint::SyncCopy(n)) => crate::localized_format!("这个文档库可能也在别的电脑上使用（同步留下了 {n}）。", "This library may also be used on another computer (sync left {n} behind). "),
                    ConsentReason::Restored => crate::settings::tr("你之前把文档库恢复成了原来的存放方式。", "You went back to the old layout before. ").to_string(),
                };
                (
                    crate::settings::tr("把文档库改为真实文件夹？", "Move the library to real folders?").to_string(),
                    why + if cfg!(target_os = "macos") { crate::settings::tr(
                        "改完后，在访达里看到的文件夹和文件就是侧栏里的样子，文件以标题命名。别的电脑上的旧版 Quiet.md 打不开改过的文档库，请先把它们更新到最新版本。原来的文件会在文档库的 .quiet 文件夹里保留 30 天。",
                        "Afterwards, Finder shows the same folders and files as the sidebar, each file named by its title. Older versions of Quiet.md on other computers can't open the moved library, so update them first. The old files are kept in the library's .quiet folder for 30 days.") }
                    else if cfg!(target_os = "windows") { crate::settings::tr(
                        "改完后，在文件资源管理器里看到的文件夹和文件就是侧栏里的样子，文件以标题命名。别的电脑上的旧版 Quiet.md 打不开改过的文档库，请先把它们更新到最新版本。原来的文件会在文档库的 .quiet 文件夹里保留 30 天。",
                        "Afterwards, File Explorer shows the same folders and files as the sidebar, each file named by its title. Older versions of Quiet.md on other computers can't open the moved library, so update them first. The old files are kept in the library's .quiet folder for 30 days.") }
                    else { crate::settings::tr(
                        "改完后，在文件管理器里看到的文件夹和文件就是侧栏里的样子，文件以标题命名。别的电脑上的旧版 Quiet.md 打不开改过的文档库，请先把它们更新到最新版本。原来的文件会在文档库的 .quiet 文件夹里保留 30 天。",
                        "Afterwards, your file manager shows the same folders and files as the sidebar, each file named by its title. Older versions of Quiet.md on other computers can't open the moved library, so update them first. The old files are kept in the library's .quiet folder for 30 days.") },
                    crate::settings::tr("改用文件夹", "Move"),
                    false,
                )
            }
            Confirm::RemoveRoot(_, name) => (
                crate::localized_format!("从侧栏移除「{}」？", "Remove “{}” from the sidebar?", name),
                crate::settings::tr("只是从侧栏去掉，文件夹里的文件不会删除。以后重新打开这个文件夹，置顶、历史版本和回收站都还在。", "It only leaves the sidebar; nothing in the folder is deleted. Open the folder again later and its pins, history and Trash are still there.").to_string(),
                crate::settings::tr("移除", "Remove"),
                false,
            ),
            Confirm::Import { what, target } => {
                let place = self.drop_place(target);
                let what_s = match what {
                    crate::app::DropImport::Local(id) => crate::localized_format!("「{}」", "“{}”", title_of(id)),
                    crate::app::DropImport::Files(p) if p.len() == 1 => crate::localized_format!("「{}」", "“{}”", p[0].file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
                    crate::app::DropImport::Files(p) => crate::localized_format!("这 {} 项", "these {} items", p.len()),
                };
                (
                    crate::localized_format!("是否将{}导入到「{}」？", "Import {} into “{}”?", what_s, place),
                    crate::settings::tr("会复制一份到文档库里，原文件不会改动。", "A copy goes into the library; the original file is left as it is.").to_string(),
                    crate::settings::tr("导入", "Import"),
                    false,
                )
            }
            Confirm::SaveFailed => (
                crate::settings::text("文档保存失败").to_string(),
                crate::localized_format!("{}。可以重试或另存副本；重新载入将放弃未保存的修改并载入磁盘内容。", "{}. Retry or save a copy. Reload discards unsaved changes and loads the document from disk.", self.save_error()),
                crate::settings::text("重试"),
                false,
            ),
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            Confirm::StartCloudTrial => (
                crate::settings::tr("开始 7 天试用？", "Start a 7-day trial?").into(),
                crate::settings::tr("确认后开始计时。同一 iCloud 账户的设备共用这 7 天。试用不会自动续费，本机文档库始终免费使用。",
                    "The trial starts when you confirm and is shared across devices on this iCloud account. It won’t renew automatically. The local library stays free to use.").into(),
                crate::settings::tr("开始试用", "Start Trial"),
                false,
            ),
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            Confirm::SwitchCloudLibrary { cloud: true, .. } => (
                crate::settings::tr("开启 iCloud 同步？", "Enable iCloud sync?").into(),
                if cfg!(target_os = "ios") { crate::settings::tr("飞记将打开 iCloud 文档库，本机文档会保留。",
                    "Fei Notes will open your iCloud library and keep your local documents.").into() }
                else { crate::settings::tr("飞记将自动重启并打开 iCloud 文档库。本机文档会保留。",
                    "Fei Notes will restart and open your iCloud library. Your local documents will be kept on this Mac.").into() },
                crate::settings::tr("开启同步", "Enable Sync"),
                false,
            ),
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            Confirm::SwitchCloudLibrary { cloud: false, .. } => (
                crate::settings::tr("关闭 iCloud 同步？", "Turn off iCloud sync?").into(),
                if cfg!(target_os = "ios") { crate::settings::tr("飞记将打开本机文档库，iCloud 文档和待上传的改动会保留。",
                    "Fei Notes will open your local library and keep your iCloud documents and pending changes.").into() }
                else { crate::settings::tr("飞记将自动重启并打开本机文档库。iCloud 文档和待上传的改动会保留。",
                    "Fei Notes will restart and open your local library. Your iCloud documents and pending changes are kept.").into() },
                crate::settings::tr("切换到本机", "Switch to Local"),
                false,
            ),
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            Confirm::RestoreLibraryBackup(prepared) => backup_confirmation(prepared),
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            Confirm::CloudVersions { .. } => (
                crate::settings::tr("iCloud 历史版本", "iCloud Version History").into(),
                crate::settings::tr("选择一个版本，恢复为一篇新的文档。", "Choose a version to restore as a new document.").into(),
                crate::settings::tr("恢复副本", "Restore a Copy"),
                false,
            ),
        })
    }

    /// The confirmation dialog: a modal (white, radius 16), a 15pt
    /// semibold title, one line of 13pt body, 取消 and the action button on
    /// the right.
    pub(crate) fn render_confirm(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let c = self.confirm.as_ref()?;
        let (title, body, action, danger) = self.confirmation_text()?;
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let copy_choice = if let Confirm::SwitchCloudLibrary { cloud: true, copy_local } = c {
            Some(div().id("confirm-copy-local").mt(px(14.)).flex().items_center().gap(px(8.))
                .cursor_pointer().text_size(px(13.)).line_height(px(20.))
                .child(crate::ui::checkbox(*copy_local))
                .child(ui_para(crate::settings::tr("同时复制本机文档到 iCloud", "Also copy local documents to iCloud")))
                .on_click(cx.listener(|w, _, _, cx| {
                    if let Some(Confirm::SwitchCloudLibrary { copy_local, .. }) = &mut w.confirm { *copy_local = !*copy_local; }
                    cx.notify();
                })))
        } else { None };
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        let copy_choice = None::<Stateful<Div>>;
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let (version_list, ready) = if let Confirm::CloudVersions { versions, selected, .. } = c {
            let list = div().id("cloud-version-list").mt(px(12.)).max_h(px(220.)).overflow_y_scroll()
                .children(versions.iter().map(|(sequence, time, preview)| {
                    let sequence = *sequence;
                    let active = *selected == Some(sequence);
                    div().id(SharedString::from(format!("cloud-version-{sequence}"))).flex().items_center().gap(px(8.))
                        .px(px(8.)).py(px(6.)).rounded(px(8.)).cursor_pointer().hover(row_hover)
                        .child(crate::transfer::radio(active))
                        .child(div().flex_1().min_w_0().flex().flex_col().text_size(px(13.)).line_height(px(20.))
                            .child(time.clone())
                            .when(!preview.trim().is_empty(), |column| column
                                .child(div().truncate().text_size(px(12.)).text_color(Theme::text_caption()).child(preview.clone()))))
                        .on_click(cx.listener(move |w, _, _, cx| {
                            if let Some(Confirm::CloudVersions { selected, .. }) = &mut w.confirm { *selected = Some(sequence); cx.notify(); }
                        }))
                }));
            (Some(list), selected.is_some())
        } else { (None, true) };
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        let (version_list, ready) = (None::<Stateful<Div>>, true);
        let attached = copy_choice.is_some() && cfg!(target_os = "macos");
        let no_ask = match c {
            Confirm::Refs { show: false, no_ask } => Some(*no_ask),
            _ => None,
        };
        let ok = if danger { Button::destructive("confirm-ok") } else { Button::primary("confirm-ok") }.label(action)
            .when(cfg!(feature = "devtools"), |button| button.relative().child(self.home.track("qa-confirm-ok")));
        let max_width = px(if matches!(c, Confirm::SaveFailed) || version_list.is_some() { 520. } else { 400. });
        let width = (window.viewport_size().width - px(32.)).min(max_width);
        // The dialog holds focus while it is up (`sync_confirm_focus`): ↩ and Esc are its own keys.
        let dialog = Dialog::alert("confirm-dialog")
            .key_context("Confirm")
            .track_focus(&self.confirm_focus)
            .on_action(cx.listener(move |w, _: &ConfirmOk, window, cx| { if ready { w.confirm_current(window, cx); } }))
            .on_action(cx.listener(|w, _: &ConfirmCancel, _, cx| w.dismiss_confirm(cx)))
            .w(width)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .title(title)
            .description(body)
            .footer(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().flex_1().children(no_ask.map(|on| {
                        div()
                            .id("confirm-no-ask")
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .cursor_pointer()
                            .on_click(cx.listener(|w, _, _, cx| {
                                if let Some(Confirm::Refs { no_ask, .. }) = &mut w.confirm {
                                    *no_ask = !*no_ask;
                                }
                                cx.notify();
                            }))
                            .child(Checkbox::new(on))
                            .child(ui_text(crate::settings::text("不再提示")))
                    })))
                    .child(Button::outline("confirm-cancel").label(if matches!(c, Confirm::Migrate(_)) { crate::settings::tr("稍后", "Later") } else { crate::settings::text("取消") }).on_click(cx.listener(|w, _, _, cx| {
                        // 稍后 asks again in a week, not at every launch.
                        if matches!(w.confirm, Some(Confirm::Migrate(_))) { crate::settings::migrate_later(); }
                        w.dismiss_confirm(cx);
                    })))
                    .when(matches!(c, Confirm::SaveFailed), |d| {
                        d.child(Button::outline("confirm-save-copy").label(crate::settings::tr("另存副本", "Save a copy")).on_click(cx.listener(|w, _, window, cx| {
                            w.save_failed_copy(window, cx);
                        })))
                        .child(Button::destructive("confirm-discard").label(crate::settings::tr("重新载入", "Reload")).on_click(cx.listener(|w, _, window, cx| {
                            w.confirm = None;
                            w.resolve_save_failed(false, window, cx);
                        })))
                    })
                    .child(ok.when(!ready, |b| b.opacity(0.45).cursor_default()).when(ready, |b| b.on_click(cx.listener(|w, _, window, cx| {
                        w.confirm_current(window,cx);
                    })))),
            );
        Some(
            deferred(
                crate::design::scrim()
                    .id("confirm-mask")
                    .when(attached, |d| d.items_start().pt(px(44.)))
                    .when(!attached, |d| d.items_center())
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(dialog),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }
}
