//! 回收站 page and the confirmation dialogs (删除, 恢复, 彻底删除).
//! Deleted pages wait in the library's `trash/` for 30 days; each row is a
//! deleted page together with the child pages that went with it.

use crate::app::{Confirm, ConfirmCancel, ConfirmOk, Workspace};
use crate::settings::display_title;
use crate::search::doc_icon;
use crate::store::{TRASH_DAYS, TrashEntry};
pub(crate) use crate::ui::{ToastKind, list_header_text, row_hover, row_line};
use crate::design::{Button, ButtonSize, Checkbox, Dialog};
use crate::theme::{self, Theme};
use crate::ui::{ROW_RADIUS, TipExt, TitleBarExt, icon, ui_text};
use gpui::{prelude::FluentBuilder, *};

pub const TRASH_MENU_W: f32 = 160.;

struct ListedTrash { lists: Vec<(String, Vec<TrashEntry>)>, warnings: Vec<String> }
struct PurgedTrash { errors: Vec<String> }

/// 剩余时间: whole days left before the automatic 彻底删除.
fn remaining(deleted: u64) -> String {
    let end = deleted + TRASH_DAYS * 86_400;
    let left = end.saturating_sub(crate::time::now());
    if left < 86_400 { crate::settings::text("即将彻底删除").into() } else { crate::localized_count!(left.div_ceil(86_400), "{} 天", "{} day", "{} days", left.div_ceil(86_400)) }
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
/// An illustration in the app's look: `illus/dark/…` beside `illus/…` holds the dark one (light greys
/// mirrored to deep ones), so a white bin doesn't glow on a dark page.
pub(crate) fn illus_for(path: &'static str) -> SharedString {
    if crate::theme::dark() && let Some(name) = path.strip_prefix("illus/") {
        let dark = format!("illus/dark/{name}");
        if crate::assets::get(&dark).is_some() { return dark.into() }
    }
    path.into()
}

pub(crate) fn empty_state(illus: &'static str, text: &'static str) -> Div {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .pb(px(64.))
        .child(img(illus_for(illus)).size(px(120.)))
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
        let empty = (!self.trash.is_empty()).then(|| Button::ghost("trash-empty").no_drag().mr(px(8.)).icon("bin").label(crate::settings::text("清空回收站"))
            .on_click(cx.listener(|w, _, _, cx| { w.trash_menu = None; w.confirm = Some(Confirm::EmptyTrash); cx.notify(); })));
        let header = page_header(self, "回收站", 26., cx).children(empty).child(
            crate::ui::header_icon("trash-search", "search")
                .tip_label(crate::shortcuts::keys(crate::settings::text("搜索(⌘ + J)")).into(), crate::ui::TipSide::Below)
                .on_click(cx.listener(|_, _, window, cx| window.dispatch_action(Box::new(crate::app::FocusSearch), cx))),
        );
        let mut page = div().flex_1().min_w_0().h_full().flex().flex_col().child(header);
        if self.trash.is_empty() {
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
        // A row is lit while hovered or holding its menu; the lines touching it hide.
        let lit = |i: usize| self.trash.get(i).is_some_and(|t| {
            self.home.list_hover.as_ref().is_some_and(|h| *h == format!("trash-row-{}", t.id))
                || self.trash_menu.as_ref().is_some_and(|(m, _, _)| *m == t.id)
        });
        let table_header = div()
            .h(px(33.))
            .flex_none()
            .mx(px(24.))
            .flex()
            .items_center()
            .border_b_1()
            .border_color(row_line(lit(0)))
            .child(col(None, "标题"))
            .child(col(Some(168.), "位置"))
            .child(col(Some(168.), "剩余时间"))
            .child(div().w(px(48.)).flex_none());
        let mut list = div().id("trash-list").flex_1().min_h_0().overflow_y_scroll().px(px(24.)).pb(px(24.));
        for (i, t) in self.trash.iter().enumerate() {
            let (id, id2) = (t.id.clone(), t.id.clone());
            let menu_open = self.trash_menu.as_ref().is_some_and(|(m, _, _)| *m == t.id);
            let title = display_title(&t.title);
            let slot = format!("trash-more-{}", t.id);
            let (record, lookup, slot2) = (self.more_bounds.clone(), self.more_bounds.clone(), slot.clone());
            let more = Button::ghost(SharedString::from(slot.clone()))
                .sized(ButtonSize::Xs)
                .icon("ellipsis")
                .selected(menu_open)
                .relative()
                .child(canvas(move |b, _, _| { record.borrow_mut().insert(slot.clone(), b); }, |_, _, _, _| {}).absolute().inset_0())
                .on_click(cx.listener(move |w, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    // hung from the measured button, never the pointer
                    let Some(b) = lookup.borrow().get(&slot2).copied() else { return };
                    let (pos, corner) = crate::design::hang(b, crate::design::Align::End, None);
                    w.trash_menu = Some((id.clone(), pos, corner));
                    cx.notify();
                }));
            let key = SharedString::from(format!("trash-row-{}", t.id));
            list = list.child(
                div()
                    .id(key.clone())
                    .h(px(56.))
                    .flex()
                    .items_center()
                    .when(i > 0, |d| d.border_t_1().border_color(row_line(lit(i - 1) || lit(i))))
                    .hover(row_hover)
                    .on_hover(cx.listener(move |w, on: &bool, _, cx| if w.home.hover_row(&key, *on) { cx.notify() }))
                    .when(menu_open, |d| d.bg(Theme::fill_hover()).rounded(px(ROW_RADIUS)))
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
                            .child(icon("folder", 16., Theme::icon()))
                            .child(div().min_w_0().text_size(px(13.)).text_color(Theme::text_caption()).overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui_text(self.trash_location(t)))),
                    )
                    .child(div().w(px(168.)).flex_none().pl(px(20.)).text_size(px(13.)).text_color(Theme::text_caption()).child(remaining(t.deleted)))
                    .child(div().w(px(48.)).flex_none().flex().justify_end().pr(px(12.)).child(more)),
            );
        }
        page = page.child(table_header).child(list);
        page.into_any_element()
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
            deferred(anchored().anchor(corner).position(pos).snap_to_window_with_margin(px(8.)).child(menu.fit(crate::design::room(pos, corner))))
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
        self.entries.set(self.store.list());
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

    /// 清空回收站: purges every trashed page; stops at the first failure.
    fn empty_trash(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<String> = self.trash.iter().map(|t| t.id.clone()).collect();
        let success = crate::settings::text("已清空回收站");
        self.purge_trash(ids, success, cx);
    }

    fn purge_trash(&mut self, ids: Vec<String>, success: &'static str, cx: &mut Context<Self>) {
        if self.store.file_task_busy() || self.file_history.busy || ids.is_empty() { return; }
        let later = ids.clone();
        if self.after_tree_writes(move |w, _, cx| w.purge_trash(later, success, cx)) { return; }
        let Some(task_guard) = self.store.begin_file_task() else { return };
        // Each top-level folder's 回收站 is its own: one writer, and one write, per top-level folder.
        let mut writers: Vec<(String, crate::store::Store, Vec<String>)> = vec![];
        for id in &ids {
            let Some(key) = self.store.trash_top(id) else { continue };
            match writers.iter_mut().find(|(k, _, _)| *k == key) {
                Some((_, _, v)) => v.push(id.clone()),
                None => if let Some(w) = self.store.top_writer(&key) { writers.push((key, w, vec![id.clone()])); },
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
            let _task_guard = task_guard;
            let (done, listed) = task.await;
            let _ = this.update(cx, |w, cx| {
                for (key, trash) in listed.lists { w.trash = w.store.merge_trash(&key, trash); }
                let mut errors = done.errors;
                errors.extend(listed.warnings.iter().map(|s| crate::settings::diagnostic(s)));
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
        let root = crate::store::Store::active_dir();
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

    /// The action button, and ↩ while the dialog is up.
    pub(crate) fn confirm_ok(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = self.confirm.take() else { return };
        match c {
            Confirm::Delete { ids, batch } => {
                if batch {
                    self.batch = None;
                }
                self.delete_docs(&ids, window, cx);
            }
            Confirm::Restore(id) => self.restore_doc(&id, cx),
            Confirm::Purge(id) => self.purge_doc(&id, cx),
            Confirm::EmptyTrash => self.empty_trash(cx),
            Confirm::RemoveUnused(files) => self.remove_unused_assets(files, window, cx),
            Confirm::Refs { show, no_ask } => {
                if no_ask {
                    let mut s = self.store.settings();
                    s.refs_hide_no_ask = true;
                    let _ = self.store.save_settings(&s);
                }
                self.set_refs_hidden(!show, cx);
            }
            Confirm::SaveFailed => self.resolve_save_failed(true, window, cx),
            Confirm::Migrate(_) => self.migrate_library(window, cx),
            Confirm::RemoveFolder(key, _) => self.remove_folder(&key, window, cx),
            Confirm::Import { what, target } => self.import_dropped(what, target, window, cx),
            Confirm::AddFolder(ask) => self.add_asked_folder(ask, window, cx),
        }
        cx.notify();
    }

    /// 取消 and Esc: close the dialog, and drop what was waiting on a save.
    pub(crate) fn dismiss_confirm(&mut self, cx: &mut Context<Self>) {
        self.confirm = None;
        self.after_save = None;
        cx.notify();
    }

    /// A dialog takes focus as it opens, so ↩ can't reach the document
    /// beneath, and gives it back where it was once it closes (unless what
    /// the dialog did moved it on).
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

    /// The confirmation dialog: a `Dialog` with its title, one line of body,
    /// 取消 and the action button on the right.
    pub(crate) fn render_confirm(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
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
                if *batch || ids.len() > 1 { crate::localized_count!(ids.len(), "是否删除所选的 {} 项？", "Delete the {} selected item?", "Delete the {} selected items?", ids.len()) } else { crate::localized_format!("是否删除： {}？", "Delete {}?", title_of(&ids[0])) },
                crate::settings::text("删除的内容将进入回收站，30 天后自动彻底删除").to_string(),
                crate::settings::text("删除"),
                true,
            ),
            Confirm::Restore(_) => (crate::settings::text("确认恢复？").to_string(), crate::settings::text("确认后将恢复至原位置").to_string(), crate::settings::text("恢复"), false),
            Confirm::EmptyTrash => (
                crate::settings::text("确认清空回收站？").to_string(),
                crate::localized_count!(self.trash.len(), "回收站中的 {} 项将被彻底删除，无法恢复", "The {} item in the Trash will be deleted permanently. This cannot be undone.", "The {} items in the Trash will be deleted permanently. This cannot be undone.", self.trash.len()),
                crate::settings::text("清空"),
                true,
            ),
            Confirm::Purge(_) => (crate::settings::text("确认彻底删除？").to_string(), crate::settings::text("彻底删除后，该页面及其子页面将无法恢复").to_string(), crate::settings::text("删除"), true),
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
            Confirm::RemoveFolder(_, name) => (
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
                    #[cfg(target_family = "wasm")]
                    crate::app::DropImport::Browser(files) => crate::localized_count!(files.count, "这 {} 项", "{} item", "these {} items", files.count),
                };
                (
                    crate::localized_format!("是否将{}导入到「{}」？", "Import {} into “{}”?", what_s, place),
                    crate::settings::tr("会复制一份到文档库里，原文件不会改动。", "A copy goes into the library; the original file is left as it is.").to_string(),
                    crate::settings::tr("导入", "Import"),
                    false,
                )
            }
            Confirm::AddFolder(ask) => {
                let name = ask[0].0.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let why = match ask[0].1 {
                    fdl_core::shown::Survey::Big => crate::localized_format!("这个文件夹里有 {} 多个文件。", "This folder holds more than {} files. ", crate::loading::grouped(fdl_core::shown::SURVEY_LIMIT)),
                    _ => crate::settings::tr("这个文件夹看起来是代码项目。", "This folder looks like a code project. ").to_string(),
                };
                (
                    crate::localized_format!("添加「{}」？", "Add “{}”?", name),
                    why + crate::settings::tr("Quiet.md 只显示其中的 Markdown 文件和 Quiet.md 文档，不进入依赖、构建产物和 .gitignore 忽略的文件夹；添加后，文件夹里会多一个隐藏的 .quiet 文件夹。",
                        "Quiet.md shows only its Markdown files and Quiet.md documents, and skips dependencies, build output and what .gitignore leaves out. Adding it puts a hidden .quiet folder inside."),
                    crate::settings::tr("添加", "Add"),
                    false,
                )
            }
            Confirm::SaveFailed => (
                crate::settings::text("文档保存失败").to_string(),
                crate::localized_format!("{}。可以重试或另存副本；重新载入将放弃未保存的修改并载入磁盘内容。", "{}. Retry or save a copy. Reload discards unsaved changes and loads the document from disk.", self.save_error()),
                crate::settings::text("重试"),
                false,
            ),
        };
        let no_ask = match c {
            Confirm::Refs { show: false, no_ask } => Some(*no_ask),
            _ => None,
        };
        let ok = if danger { Button::destructive("confirm-ok") } else { Button::primary("confirm-ok") }.label(action);
        // The dialog holds focus while it is up (`sync_confirm_focus`): ↩ and Esc are its own keys.
        let dialog = Dialog::alert("confirm-dialog")
            .key_context("Confirm")
            .track_focus(&self.confirm_focus)
            .on_action(cx.listener(|w, _: &ConfirmOk, window, cx| w.confirm_ok(window, cx)))
            .on_action(cx.listener(|w, _: &ConfirmCancel, _, cx| w.dismiss_confirm(cx)))
            .w(px(if matches!(c, Confirm::SaveFailed | Confirm::Migrate(_)) { 520. } else { 400. }))
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
                    .child(ok.on_click(cx.listener(|w, _, window, cx| w.confirm_ok(window, cx)))),
            );
        Some(
            deferred(
                crate::design::scrim()
                    .id("confirm-mask")
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(dialog),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }
}
