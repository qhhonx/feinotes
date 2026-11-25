//! Left pane: document list with search. Rows, sections and footer, sunken:
//! no panel, border or shadow, straight on the window surface (only the note
//! floats). 30pt rows 1pt apart (`ROW_GAP`), selection pill radius 9.

use crate::app::{Confirm, RowMenu, View, Workspace};
use crate::settings::display_title;
use crate::store::Entry;
use crate::theme::{self, Theme};
use crate::design::{Button, ButtonSize, MenuItem, MenuLabel, MenuSeparator, Popover};
use crate::ui::{TipExt, TitleBarExt, icon, ui_text};
use gpui::{prelude::FluentBuilder, *};

/// Row "…" menu geometry (`design::menu`): rows, separators, padding.
const ROW_MENU_W: f32 = 180.;
const ROW_ITEM: f32 = crate::design::menu::ROW_H;
const ROW_DIVIDER: f32 = crate::design::menu::SEP_H;
const ROW_ITEMS: usize = 9;
const ROW_DIVIDERS: usize = 2;
const DOWNLOAD_INDEX: usize = 5;
const DIVIDERS_BEFORE_DOWNLOAD: usize = 1;

const fn row_menu_height(items: usize, dividers: usize) -> f32 {
    crate::design::menu::PAD * 2. + items as f32 * ROW_ITEM + dividers as f32 * ROW_DIVIDER
}

const fn row_sub_top(menu_top: f32) -> f32 {
    menu_top + DOWNLOAD_INDEX as f32 * ROW_ITEM + DIVIDERS_BEFORE_DOWNLOAD as f32 * ROW_DIVIDER
}

const _: () = assert!(row_menu_height(ROW_ITEMS, ROW_DIVIDERS) == 313.);
const _: () = assert!(row_sub_top(0.) == 168.5);

/// Sidebar geometry (measured from the window edge), shared by the
/// document sidebar and 设置's: selection pills start 14pt in, every row is
/// 30pt with radius 9, `ROW_GAP` (1) apart, glyphs (18pt) centre at 37pt and
/// labels start at 54pt; groups are 16pt apart, a section header is 34pt
/// (6 above its 28pt label).
pub(crate) const SB_X: f32 = 14.;
pub(crate) const ROW_H: f32 = 30.;

const DOC_ROW_H: f32 = ROW_H;
pub(crate) const ROW_R: f32 = 9.;
pub(crate) const ROW_GAP: f32 = crate::design::G.sidebar_gap;
pub(crate) const ROW_ICON: f32 = 18.;
pub(crate) const GROUP_GAP: f32 = 16.;
pub(crate) const SECTION_H: f32 = 34.;
pub(crate) const SECTION_TOP: f32 = 6.;
/// 我的文档's heading is a row like 主页's: a `ROW_H` wash, `ROW_GAP` above the first row.
pub(crate) const LIBRARY_H: f32 = SECTION_TOP + ROW_H + ROW_GAP;

/// 批量操作 caps at 50 pages.
pub(crate) const BATCH_LIMIT: usize = 50;

/// The footer row: like 主页 (same pill, glyph and label columns) under
/// the footer line, so the bottom corner mirrors the top.
pub(crate) fn footer_row(id: &'static str, glyph: &'static str, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(ROW_H))
        .pl(px(6.))
        .pr(px(4.))
        .flex()
        .items_center()
        .rounded(px(ROW_R))
        .cursor_pointer()
        .text_size(px(theme::ui_font()))
        .line_height(px(20.))
        .text_color(Theme::sidebar_text())
        .hover(|s| s.bg(Theme::sidebar_hover()))
        .child(div().size(px(20.)).flex_none().flex().items_center().justify_center().mr(px(9.)).child(icon(glyph, ROW_ICON, Theme::icon())))
        .child(label)
        .relative().children(crate::devtools::row_probe(label))
}

/// The footer's divider and padding around `row`.
pub(crate) fn footer_slot(row: impl IntoElement) -> Div {
    div().pt(px(6.)).pb(px(10.)).px(px(SB_X)).child(row)
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Check {
    Off,
    Partial,
    On,
}

/// The batch checkbox: `design::Checkbox`, with 部分选中 as its indeterminate state.
pub(crate) fn checkbox(state: Check, disabled: bool) -> crate::design::Checkbox {
    crate::design::Checkbox::new(state == Check::On).partial(state == Check::Partial).disabled(disabled)
}

impl Workspace {
    /// Sidebar width for the current window: the dragged width, kept between
    /// 268 and min(800, window − 540).
    pub(crate) fn sidebar_w(&self, window: &Window) -> f32 {
        let vw = f32::from(window.viewport_size().width);
        let max = theme::SIDEBAR_MAX.min(vw - theme::SIDEBAR_MIN_CONTENT).max(theme::SIDEBAR_MIN);
        self.sidebar_width.clamp(theme::SIDEBAR_MIN, max)
    }

    pub(crate) fn render_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.more_bounds.borrow_mut().clear();
        self.settle_sidebar_drop(cx);
        crate::devtools::rows_begin(window);

        let header = div()
            .h(px(crate::app::HEADER_H))
            .flex_none()
            .flex()
            .items_center()
            .pl(px(theme::TRAFFIC_LIGHTS_W))
            .pr(px(12.))
            .title_bar()
            .child(
                Button::ghost("collapse-sidebar")
                    .no_drag()
                    .ml(px(4.))
                    .icon("panel-left-close")
                    .tip(crate::settings::tr("收起侧栏", "Hide sidebar"), Some(crate::shortcuts::hint("workspace::ToggleSidebar", "⌘ + ]")))
                    .on_click(cx.listener(|w, _, _, cx| {
                        w.sidebar_open = false;
                        cx.notify();
                    })),
            )
            .child(div().flex_1())
            .children(crate::settings::update_pill(cx));

        // The nav: 主页, then 打开文件夹 (another folder beside the default one, as
        // 文件 › 打开文件夹… does). 36px items, a tint when selected. Their icon
        // column (x 17) is shared with 搜索 and the doc rows.
        let home_on = self.view == View::Home;
        let item = |id: &'static str, glyph: &'static str, label: &'static str, on: bool| div()
            .id(id)
            .h(px(ROW_H))
            .pl(px(6.))
            .pr(px(4.))
            .flex()
            .items_center()
            .rounded(px(ROW_R))
            .cursor_pointer()
            .text_size(px(theme::ui_font()))
            .line_height(px(20.))
            .when(on, |d| d.bg(Theme::sidebar_selected()).text_color(Theme::text()))
            .when(!on, |d| d.text_color(Theme::sidebar_text()).hover(|s| s.bg(Theme::sidebar_hover())))
            // Selected or not, the same outline: the row's fill marks the selection.
            .child(div().size(px(20.)).flex_none().flex().items_center().justify_center().mr(px(9.)).child(icon(glyph, ROW_ICON, Theme::icon())))
            .child(label)
            .relative().children(crate::devtools::row_probe(label));
        let nav = div().flex_none().pt(px(4.)).pb(px(4.)).px(px(SB_X)).flex().flex_col().gap(px(ROW_GAP))
            .child(item("nav-home", "home", crate::settings::tr("主页", "Home"), home_on).on_click(cx.listener(|w, _, _, cx| w.show_view(View::Home, cx))));

        // 置顶 and 我的文档's heading stand still above the list, as 我的文档's own header (the
        // maintainer, 2026-11-25): the rows scroll under it, and so does the scrollbar's track.
        let mut fixed = div().flex_none().flex().flex_col().px(px(SB_X));
        let mut list = div().flex().flex_col().px(px(SB_X)).pb(px(8.));
        // Pinned docs are shortcuts: they stay in 我的文档 too.
        // 置顶 follows the same order as the lists below it. Only the pinned are
        // sorted: copying and sorting every entry each frame cost 3 ms in a big library.
        let order = self.home.order();
        let mut pinned: Vec<&Entry> = self.entries.iter().filter(|e| e.meta.pinned).collect();
        pinned.sort_by(|a, b| order.cmp(a, b));
        if !pinned.is_empty() {
            fixed = fixed.child(self.section(0, crate::settings::text("置顶"), cx));
            if !self.folded[0] {
                for e in pinned {
                    fixed = fixed.child(self.doc_row(e, "pinned", 0, false, cx));
                }
            }
            fixed = fixed.child(div().h(px(GROUP_GAP - SECTION_TOP)));
        }
        // 主页: one top-level folder shows its contents under 我的文档; several show as rows
        // of the tree there; none, a way to open one.
        let tops = self.store.shown_tops();
        if tops.is_empty() {
            list = list.child(self.empty_home(cx));
        } else {
            // One top-level folder or several, the heading is 我的文档 (#421); it never folds.
            fixed = fixed.child(self.section(1, crate::settings::text("我的文档"), cx));
            list = list.child(self.render_folder_rows(window, cx));
        }
        // The first listing still running: grey rows in place of the tree, fading out over it once it is here.
        let load = if crate::loading::adding(cx) { crate::loading::Phase::Gone } else { crate::loading::phase(cx) };
        if load.loading() { list = div().flex().flex_col().px(px(SB_X)).pb(px(8.)).child(crate::loading::sidebar_rows(load, crate::loading::t(cx))); }
        // The sidebar sits on the window's glass: nothing can cover the rows, so they fade in as the grey ones fade out.
        else if load.moving() { list = div().relative().child(list.opacity(1. - load.cover())).child(crate::loading::sidebar_rows(load, crate::loading::t(cx)).absolute().top_0().left(px(SB_X)).right(px(SB_X))); }
        // 本机文件: Markdown files and folders edited where they are, outside the
        // library. Files or folders from the Finder let go here open the same way.
        let local = self.local_rows();
        if !local.is_empty() {
            let mut section = div().id("local-section").flex().flex_col().rounded(px(ROW_R))
                .drag_over::<ExternalPaths>(|s, _, _, _| s.bg(Theme::sidebar_hover()))
                .on_drop(cx.listener(|w, paths: &ExternalPaths, window, cx| {
                    cx.stop_propagation();
                    w.sidebar_drop = None;
                    w.open_paths(paths.paths(), window, cx);
                }))
                .child(self.section(2, crate::settings::tr("本机文件", "Files on this computer"), cx));
            if !self.folded[2] {
                for (e, depth) in local {
                    section = section.child(self.doc_row(e, "local", depth, false, cx));
                }
            }
            list = list.child(div().h(px(GROUP_GAP - SECTION_TOP))).child(section);
        }

        // The footer holds one row: 设置 (存储管理 and 回收站 live in 设置).
        // The 批量操作 toolbar: ghost icon buttons.
        let footer_btn = |id: &'static str, name: &'static str, tip: &'static str, enabled: bool, on: bool| {
            Button::ghost(id).w(px(36.)).h(px(32.)).icon(if on { format!("{name}-filled") } else { name.to_string() }).selected(on).disabled(!enabled).tip(tip, None)
        };
        let some = self.batch.as_ref().is_some_and(|b| !b.is_empty());
        // 设置; in 设置 the same row reads 返回 (`settings.rs`), so one spot toggles.
        let footer = div()
            .flex_none()
            .flex()
            .flex_col()
            .when(self.batch.is_none(), |d| {
                d.child(footer_slot(
                    footer_row("ft-settings", "settings-gear", crate::settings::tr("设置", "Settings"))
                        .on_click(cx.listener(|w, _, _, cx| w.show_view(View::Settings, cx))),
                ))
            })
            // 批量操作 toolbar: 创建副本 | 移动到 | 删除, minus the cloud ones.
            .when(self.batch.is_some(), |d| {
                d.child(
                    div().h(px(46.)).mx(px(SB_X + 6. + 10. - 18.)).flex().items_center().gap(px(4.))
                        .child(footer_btn("bt-copy", "copy", crate::settings::text("创建副本"), some, false).on_click(cx.listener(|w, _, _, cx| w.batch_copy(cx))))
                        .child(footer_btn("bt-move", "folder-input", crate::settings::text("移动到"), some, false).on_click(cx.listener(|w, _, window, cx| w.batch_move(window, cx))))
                        .child(footer_btn("bt-delete", "bin", crate::settings::text("删除"), some, false).on_click(cx.listener(|w, _, _, cx| w.ask_batch_delete(cx)))),
                )
            });

        div()
            .w(px(self.sidebar_w(window)))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .child(header)
            .child(nav)
            .child(fixed)
            .child(self.sidebar_drop_zone(div().id("doc-list").flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.list_scroll).child(list).child(crate::design::scroll::bar("doc-list", &self.list_scroll, 0.)), cx))
            .children(crate::changelog::card(cx.listener(|_, _, _, cx| cx.notify())))
            .child(footer)
            .into_any_element()
    }

    /// Width handle: a 15px hit strip straddling the sidebar edge (6px inside,
    /// 9px over the note's margin). Borderless like the sidebar: only the
    /// cursor changes, and a faint line shows while dragging.
    pub(crate) fn render_resize_handle(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let w = self.sidebar_w(window);
        let dragging = self.sidebar_drag;
        div()
            .id("sidebar-resize")
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(w - 6.))
            .w(px(15.))
            .group("sidebar-resize")
            .cursor(CursorStyle::ResizeLeftRight)
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(5.))
                    .w(px(2.))
                    .rounded(px(1.))
                    .when(dragging, |d| d.bg(Theme::primary()))
                    .group_hover("sidebar-resize", |s| s.bg(Theme::primary())),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, window, cx| {
                    cx.stop_propagation();
                    ws.sidebar_width = ws.sidebar_w(window);
                    ws.sidebar_drag = true;
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    /// While dragging, a transparent full-window layer keeps the resize cursor
    /// and follows the mouse.
    pub(crate) fn render_resize_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.sidebar_drag {
            return None;
        }
        let end = |ws: &mut Workspace, cx: &mut Context<Workspace>| {
            ws.sidebar_drag = false;
            let mut s = ws.store.settings();
            s.sidebar_width = Some(ws.sidebar_width);
            let _ = ws.store.save_settings(&s);
            cx.notify();
        };
        Some(
            deferred(
                div()
                    .id("sidebar-resize-overlay")
                    .absolute()
                    .inset_0()
                    .occlude()
                    .cursor(CursorStyle::ResizeLeftRight)
                    .on_mouse_move(cx.listener(move |ws, ev: &MouseMoveEvent, window, cx| {
                        // The release can land before this layer exists.
                        if ev.pressed_button != Some(MouseButton::Left) {
                            return end(ws, cx);
                        }
                        ws.sidebar_width = f32::from(ev.position.x);
                        ws.sidebar_width = ws.sidebar_w(window);
                        cx.notify();
                    }))
                    .on_mouse_up(MouseButton::Left, cx.listener(move |ws, _, _, cx| end(ws, cx)))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(move |ws, _, _, cx| end(ws, cx))),
            )
            .with_priority(4)
            .into_any_element(),
        )
    }

    /// Section header: the title folds the list (an arrow appears on hover);
    /// 我的文档's is a row that opens its page, with 批量操作 and "+" on the
    /// right, laid out so the last one lines up with the rows' "…".
    fn section(&self, idx: usize, title: impl AsRef<str>, cx: &mut Context<Self>) -> Div {
        let title = crate::settings::label(title);
        let group = SharedString::from(format!("section-{idx}"));
        let folded = self.folded[idx];
        let trigger = |id: &'static str, name: &'static str| Button::ghost(id).sized(ButtonSize::Xs).on_wash().icon(name);
        let create_open = matches!(self.create_menu, Some((None, _)));
        let row = div().h(px(if idx == 1 { LIBRARY_H } else { SECTION_H })).pl(px(4.)).pt(px(SECTION_TOP)).flex().items_center().group(group.clone());
        if let (1, Some(sel)) = (idx, self.batch.as_ref()) {
            let n = sel.len();
            let all = n > 0 && n >= self.entries.len().min(BATCH_LIMIT);
            return row
                .child(
                    div()
                        .id("batch-all")
                        .ml(px(6.))
                        .size(px(14.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .group("batch-all")
                        .child(checkbox(if all { Check::On } else if n > 0 { Check::Partial } else { Check::Off }, false))
                        .on_click(cx.listener(move |w, _, _, cx| {
                            let ids = if all { vec![] } else { w.entries.iter().take(BATCH_LIMIT).map(|e| e.id.clone()).collect() };
                            w.batch = Some(ids);
                            cx.notify();
                        })),
                )
                .child(div().flex_1().min_w_0().ml(px(8.)).text_size(px(theme::ui_font())).line_height(px(theme::ui_font() + 7.)).text_color(Theme::text()).child(crate::localized_format!("已选 {n} 项", "Selected: {n}")))
                .child(
                    trigger("batch-exit", "x").ml(px(8.))
                        .tip(crate::settings::text("退出批量操作"), None)
                        .on_click(cx.listener(|w, _, _, cx| {
                            w.batch = None;
                            w.row_menu = None;
                            cx.notify();
                        })),
                )
                .child(trigger("section-add", "plus").ml(px(2.)).mr(px(4.)).child(self.home.track("create-sidebar")).selected(create_open).tip(crate::settings::tr("新建", "New"), None)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|w, _: &ClickEvent, _, cx| { let Some(b) = w.home.anchor("create-sidebar") else { return }; let pos = crate::design::hang(b, crate::design::Align::Start, None).0; w.create_menu = if w.create_menu.is_some() { None } else { Some((None, pos)) }; cx.notify(); })));
        }
        // 我的文档 (#421) does not fold: the heading is a row of its own, like a folder's, that
        // opens 主页 on its 我的文档 tab, which lists every top-level folder (主页 is 我的文档).
        // One page, so one active row: 主页 lights, the heading only answers the pointer. Its
        // 批量操作 and "+" keep their clicks to themselves. Its wash stops `ROW_GAP` above the
        // first row, as rows do.
        if idx == 1 {
            return div().h(px(LIBRARY_H)).pt(px(SECTION_TOP)).flex().child(
                div().id("section-library").flex_1().h(px(ROW_H)).pl(px(8.)).flex().items_center().rounded(px(ROW_R)).cursor_pointer()
                    .relative().children(crate::devtools::row_probe(title.clone()))
                    .hover(|s| s.bg(Theme::sidebar_hover())).when(self.sidebar_drop_top(), |d| d.bg(Theme::sidebar_selected()))
                    .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(px(theme::ui_font())).line_height(px(theme::ui_font() + 7.))
                        .font_weight(FontWeight::SEMIBOLD).text_color(Theme::sidebar_section()).child(title))
                    .child(trigger("section-batch", "batch").tip(crate::settings::text("批量操作"), None)
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(|w, _, _, cx| {
                            cx.stop_propagation();
                            w.batch = Some(vec![]);
                            w.row_menu = None;
                            cx.notify();
                        })))
                    .child(trigger("section-add", "plus").ml(px(2.)).mr(px(4.)).child(self.home.track("create-sidebar")).selected(create_open).tip(crate::settings::tr("新建", "New"), None)
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(|w, _: &ClickEvent, _, cx| { cx.stop_propagation(); let Some(b) = w.home.anchor("create-sidebar") else { return }; let pos = crate::design::hang(b, crate::design::Align::Start, None).0; w.create_menu = if w.create_menu.is_some() { None } else { Some((None, pos)) }; cx.notify(); })))
                    .on_click(cx.listener(|w, _, _, cx| { w.home.tab = crate::home::HomeTab::Recent; w.show_view(View::Home, cx); })),
            );
        }
        row.child(
            div()
                // Keyed by state so the tooltip is rebuilt after a toggle.
                .id(SharedString::from(format!("section-title-{idx}-{folded}")))
                .relative().children(crate::devtools::row_probe(title.clone()))
                .px(px(4.))
                .h(px(22.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|s| s.bg(Theme::sidebar_hover()))
                .tip(if folded { crate::settings::text("点击展开") } else { crate::settings::text("点击折叠") }, None)
                .child(div().pr(px(4.)).text_size(px(theme::ui_font())).line_height(px(theme::ui_font() + 7.)).font_weight(FontWeight::SEMIBOLD).text_color(Theme::sidebar_section()).child(title))
                .child(
                    div()
                        .when(!folded, |d| d.opacity(0.).group_hover(group, |s| s.opacity(1.)))
                        .child(icon(if folded { "chevron-right" } else { "chevron-down" }, 10., Theme::icon())),
                )
                .on_click(cx.listener(move |w, _, _, cx| {
                    w.folded[idx] = !w.folded[idx];
                    cx.notify();
                })),
        )
    }

    /// Check or uncheck a 我的文档 row (entering 批量操作 if needed); false
    /// when the 50-page limit blocks it.
    pub(crate) fn toggle_check(&mut self, id: &str) -> bool {
        if crate::shared_store::top_of_row(id).is_some() { return false; }
        let sel = self.batch.get_or_insert_with(Vec::new);
        if let Some(i) = sel.iter().position(|x| x == id) {
            sel.remove(i);
        } else if sel.len() < BATCH_LIMIT {
            sel.push(id.to_string());
        } else {
            return false;
        }
        true
    }

    /// The checked docs, in list order.
    fn checked(&self) -> Vec<String> {
        let Some(sel) = &self.batch else { return vec![] };
        self.entries.iter().filter(|e| sel.contains(&e.id)).map(|e| e.id.clone()).collect()
    }

    /// 批量创建副本: a "… 副本" of each checked doc; leaves 批量操作.
    pub(crate) fn batch_copy(&mut self, cx: &mut Context<Self>) {
        let file_before = self.store.track_file_changes();
        let ids = self.checked();
        if ids.is_empty() {
            return;
        }
        for id in &ids {
            self.copy_doc(id, cx);
        }
        self.entries.set(self.store.list());
        self.sort();
        self.batch = None;
        self.row_menu = None;
        self.remember_files(file_before, crate::settings::tr("批量创建副本", "Duplicate selection"));
        cx.notify();
    }

    pub(crate) fn batch_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.checked();
        if !ids.is_empty() {
            self.open_move(ids, window, cx);
        }
    }

    pub(crate) fn ask_batch_delete(&mut self, cx: &mut Context<Self>) {
        let ids = self.checked();
        if !ids.is_empty() {
            self.confirm = Some(Confirm::Delete { ids, batch: true });
            self.row_menu = None;
            cx.notify();
        }
    }

    fn row_base(&self, e: &Entry, key: &str, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = e.id.clone();
        let id2 = e.id.clone();
        let (in_lib, pinned) = (key == "doc", key == "pinned");
        div()
            .id(SharedString::from(format!("{key}-{}", e.id)))
            .group("doc-row")
            .mb(px(ROW_GAP))
            .rounded(px(ROW_R))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |w, ev: &MouseDownEvent, window, cx| {
                    if w.rename.as_ref().is_some_and(|r| r.id == id) {
                        return;
                    }
                    w.row_menu = None;
                    // In 批量操作 a click checks the row; Shift-click enters it.
                    if in_lib && (w.batch.is_some() || ev.modifiers.shift) {
                        w.toggle_check(&id);
                        cx.notify();
                        return;
                    }
                    if in_lib { w.arm_folder_drag(&id, ev.position, cx); }
                    // A 置顶 row opens on press, as it did before 本机文件 rows moved to click (#272).
                    else if pinned { w.open(&id, window, cx); }
                }),
            )
            // A 本机文件 row opens on click, and can be carried up into the tree to import it.
            .when(key == "local", |d| {
                let (open, drag) = (e.id.clone(), crate::sidebar_drop::LocalRowDrag { id: e.id.clone(), title: crate::settings::display_title(&e.title), folder: e.meta.folder.is_some() });
                d.on_click(cx.listener(move |w, _, window, cx| w.open(&open, window, cx)))
                    .on_drag(drag, |d, _, _, cx| cx.new(|_| d.clone()))
            })
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |w, ev: &MouseDownEvent, _, cx| {
                    let batch = in_lib && w.batch.is_some();
                    if batch && !w.batch.as_ref().is_some_and(|b| b.contains(&id2)) && !w.toggle_check(&id2) {
                        return;
                    }
                    w.row_menu = Some(RowMenu { id: id2.clone(), pos: ev.position, corner: Anchor::TopLeft, sub: false, batch, from: None });
                    cx.notify();
                }),
            )
    }

    fn more_button(&self, e: &Entry, key: &str, cx: &mut Context<Self>) -> Button {
        let id = e.id.clone();
        let slot = format!("{key}-{}", e.id);
        let slot2 = slot.clone();
        let record = self.more_bounds.clone();
        let lookup = self.more_bounds.clone();
        let from = format!("sidebar:{slot}");
        let open = self.row_menu.as_ref().is_some_and(|m| m.id == e.id && m.from.as_ref() == Some(&from));
        // 我的文档 rows fade their buttons in with the hover wash.
        let reveal = (key == "doc").then(|| self.folder_drag.reveal(&e.id));
        Button::ghost(SharedString::from(format!("more-{slot}")))
            .sized(ButtonSize::Xs)
            .on_wash()
            .relative()
            .mr(px(4.))
            .icon("ellipsis")
            .when(!open, |d| match reveal { Some(a) => d.opacity(a), None => d.opacity(0.).group_hover("doc-row", |s| s.opacity(1.)) })
            .selected(open)
            .child(canvas(move |b, _, _| record.borrow_mut().insert(slot.clone(), b), |_, _, _, _| {}).absolute().inset_0())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |w, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    // The row menu's outside-click handler has already closed it on this press, so
                    // a press on its own button closes it rather than reopening it (see `open`).
                    if open { w.row_menu = None; cx.notify(); return; }
                    // Hung from the measured button, never the pointer; above it when there's no room below.
                    let Some(b) = lookup.borrow().get(&slot2).copied() else { return };
                    let menu_h = row_menu_height(ROW_ITEMS + usize::from(w.takes_children(&id)), ROW_DIVIDERS);
                    let (pos, corner) = crate::design::hang(b, crate::design::Align::Start, Some(menu_h));
                    w.row_menu = Some(RowMenu { id: id.clone(), pos, corner, sub: false, batch: false, from: Some(from.clone()) });
                    cx.notify();
                }),
            )
    }

    /// Creation inside a folder: "+" opens the same 新建 menu as 我的文档's, hung
    /// from the button; 文件夹 is its last entry, so a folder needs no own button.
    fn add_button(&self, e: &Entry, key: &str, cx: &mut Context<Self>) -> Button {
        let id = e.id.clone();
        let slot = format!("add-{key}-{}", e.id);
        let (record, lookup) = (self.more_bounds.clone(), self.more_bounds.clone());
        let anchor = |b: Bounds<Pixels>| crate::design::hang(b, crate::design::Align::Start, None).0;
        let open = self.create_menu.as_ref().is_some_and(|(p, pos)| p.as_deref() == Some(e.id.as_str()) && self.more_bounds.borrow().get(&slot).is_some_and(|b| anchor(*b) == *pos));
        Button::ghost(SharedString::from(format!("add-{key}-{}", e.id)))
            .sized(ButtonSize::Xs)
            .on_wash()
            .relative()
            .opacity(if open { 1. } else { self.folder_drag.reveal(&e.id) })
            .selected(open)
            .icon("plus")
            .child({ let slot = slot.clone(); canvas(move |b, _, _| { record.borrow_mut().insert(slot.clone(), b); }, |_, _, _, _| {}).absolute().inset_0() })
            .tip(crate::settings::tr("新建", "New"), None)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |w, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                w.row_menu = None;
                let Some(pos) = lookup.borrow().get(&slot).copied().map(anchor) else { return };
                w.create_menu = if open { None } else { Some((Some(id.clone()), pos)) };
                cx.notify();
            }))
    }

    pub(crate) fn doc_row(&self, e: &Entry, key: &str, depth: usize, _kids: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let drop_target = self.folder_drop_target(&e.id);
        // 我的文档 rows float their buttons over the row's right end instead of reserving
        // the room: at rest the name runs to the edge, and it gives way as they fade in.
        // A folder with a 正文 is that document with child pages (#357): a page's face and buttons.
        // A document row has no "+": 新建子文档 is only in its "…" menu, so child pages stay a
        // side feature rather than the habit for making pages.
        let note = e.note.as_deref();
        let folder = e.meta.folder.is_some() && note.is_none();
        let lib_batch = self.batch.is_some() && key == "doc";
        let float = key == "doc" && !lib_batch && self.rename.as_ref().is_none_or(|r| r.id != e.id);
        let menu_open = self.row_menu.as_ref().is_some_and(|m| m.id == e.id && !m.batch && m.from.as_deref().is_some_and(|f| f.starts_with("sidebar:"))) || (folder && self.create_menu.as_ref().is_some_and(|(p, _)| p.as_deref() == Some(e.id.as_str())));
        let give = if float { (if folder { 52. } else { 28. }) * if menu_open { 1. } else { self.folder_drag.reveal(&e.id) } } else { 0. };
        let selected = (self.view == View::Doc && (self.current.as_deref() == Some(e.id.as_str()) || (note.is_some() && self.current.as_deref() == note))) || (self.view == View::Folder && self.folder_current.as_deref() == Some(e.id.as_str()));
        let renaming = self.rename.as_ref().filter(|r| r.id == e.id).map(|r| r.field.clone());
        let title = match renaming {
            // The field opens with the text exactly where the name was (2 + 1 + 6 = the
            // label's 9): a white well with a thin edge, wrapped in a soft focus ring.
            // The ring is its own 3px band, not a box-shadow, so its corner is drawn at
            // the well's radius + 3 — inner 圆角 + padding = outer 圆角, corners parallel.
            // Both layers overflow without taking layout space, and fade in together.
            Some(field) => {
                let a = self.folder_drag.rename_in(&e.id);
                let r = ROW_R - 2.;
                div()
                    .ml(px(2.))
                    .mr(px(4.))
                    .flex_1()
                    .min_w_0()
                    .h(px(26.))
                    .relative()
                    .child(div().absolute().inset(px(-3.)).rounded(px(r + 3.)).bg(Theme::sidebar_field_ring().opacity(a)))
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .px(px(6.))
                            .flex()
                            .items_center()
                            .rounded(px(r))
                            .bg(crate::theme::raised(Theme::bg_body()).opacity(a))
                            .border_1()
                            .border_color(Theme::sidebar_field_edge().opacity(a))
                            .text_size(px(theme::ui_font()))
                            .child(field),
                    )
                    .into_any_element()
            }
            None => div()
                .pl(px(9.))
                .flex_1()
                .min_w_0()
                .text_size(px(theme::ui_font()))
                .line_height(px(DOC_ROW_H))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .mr(px(give))
                .text_color(if drop_target || selected { Theme::text() } else { Theme::sidebar_text() })
                .child(ui_text(display_title(&e.title)))
                .into_any_element(),
        };
        let renaming = self.rename.as_ref().is_some_and(|r| r.id == e.id);
        // 批量操作: 我的文档 rows get a checkbox and lose "…"; checked rows are tinted.
        let batch = self.batch.as_ref().filter(|_| key == "doc");
        let checked = batch.is_some_and(|b| b.contains(&e.id));
        let blocked = batch.is_some_and(|b| !checked && b.len() >= BATCH_LIMIT);
        self.row_base(e, key, cx)
            .h(px(DOC_ROW_H))
            .w_full()
            .relative()
            .children(crate::devtools::row_probe(display_title(&e.title)))
            .flex()
            .items_center()
            // In 我的文档 the selection pill and the hover wash are drawn under the row (`sidebar_drag.rs`).
            .when(checked || (selected && key != "doc"), |d| d.bg(Theme::sidebar_selected()))
            .when(!selected && !checked && !drop_target && key != "doc", |d| d.hover(|s| s.bg(Theme::sidebar_hover())))
            .pl(px(6. + 18. * depth as f32))
            .when(batch.is_some(), |d| {
                d.child(
                    div()
                        .id(SharedString::from(format!("check-{}", e.id)))
                        .size(px(20.))
                        .mr(px(4.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(blocked, |d| d.tip(crate::settings::text("无法选中，选择的页面数已到达上限"), None))
                        .child(checkbox(if checked { Check::On } else { Check::Off }, blocked)),
                )
            })
            .child({
                // A folder with something in it folds from its icon, the way Notion's
                // pages do: hovering the row turns the icon into a chevron (pointing
                // down when open), and a press on it folds instead of opening the page.
                let toggle = e.meta.folder.is_some() && key == "doc" && !drop_target && self.folder_drag.has_children(&e.id);
                let chevron = if toggle { self.folder_drag.reveal(&e.id) } else { 0. };
                let open = self.expanded.contains(&e.id);
                div().size(px(20.)).flex_none().flex().items_center().justify_center().relative()
                    .when(toggle, |d| d.rounded(px(5.)).hover(|s| s.bg(Theme::sidebar_hover()))
                        .on_mouse_down(MouseButton::Left, cx.listener(|w, ev: &MouseDownEvent, _, _| {
                            w.folder_drag.icon_press = true;
                            w.folder_drag.icon_alt = ev.modifiers.alt;
                        })))
                    .map(|d| match &e.icon {
                    _ if folder => {
                        let s = if key == "doc" { self.folder_drag.pop(&e.id) } else { 1. };
                        d.child(div().absolute().inset_0().flex().items_center().justify_center().opacity(1. - chevron)
                            .child(icon(if drop_target { "folder-input" } else { "folder" }, ROW_ICON, Theme::icon())
                                .with_transformation(Transformation::scale(size(s, s)))))
                        .when(chevron > 0.005, |d| d.child(div().absolute().inset_0().flex().items_center().justify_center().opacity(chevron)
                            .child(icon("disclosure", ROW_ICON, Theme::icon())
                                .with_transformation(Transformation::rotate(radians(if open { std::f32::consts::FRAC_PI_2 } else { 0. }))))))
                    }
                    // A document with children: its own icon, turning into the chevron on hover like a folder's.
                    _ if note.is_some() => {
                        let face = div().absolute().inset_0().flex().items_center().justify_center().opacity(1. - chevron);
                        d.child(match &e.icon {
                            Some(em) if !e.meta.markdown => face.text_size(px(theme::ui_font())).line_height(px(theme::ui_font() + 7.)).child(em.clone()),
                            _ => face.child(icon(crate::search::file_glyph(&e.meta), ROW_ICON, Theme::icon())),
                        })
                        .when(chevron > 0.005, |d| d.child(div().absolute().inset_0().flex().items_center().justify_center().opacity(chevron)
                            .child(icon("disclosure", ROW_ICON, Theme::icon())
                                .with_transformation(Transformation::rotate(radians(if open { std::f32::consts::FRAC_PI_2 } else { 0. }))))))
                    }
                    _ if e.meta.named_by_file() => d.child(icon(crate::search::file_glyph(&e.meta), ROW_ICON, Theme::icon())),
                    Some(em) => d.text_size(px(theme::ui_font())).line_height(px(theme::ui_font() + 7.)).child(em.clone()),
                    None => d.child(icon("doc", ROW_ICON, Theme::icon())),
                })
            })
            .child(title)
            // A folder's "…" and "+" sit in the same columns as the header's 批量操作
            // and 新建: "…" manages the folder, "+" makes things in it (文件夹 is the
            // menu's last entry, so a separate new-folder button would be a third entry).
            .when(float, |d| {
                let end = div().absolute().top_0().bottom_0().right_0().flex().items_center();
                d.child(if folder { end.child(self.more_button(e, key, cx).mr(px(0.))).child(self.add_button(e, key, cx).ml(px(2.)).mr(px(4.))) }
                    else { end.child(self.more_button(e, key, cx)) })
            })
            .when(!renaming && key != "doc" && batch.is_none(), |d| d.child(self.more_button(e, key, cx)))
    }

    pub(crate) fn render_row_menu(&self, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(m) = self.row_menu.clone() else { return vec![] };
        if let Some(key) = crate::shared_store::top_of_row(&m.id).map(str::to_string) { return self.top_row_menu(&key, m, cx); }
        if !m.batch && self.entries.iter().any(|e| e.id == m.id && e.meta.folder.is_some() && e.note.is_none()) { return self.folder_row_menu(m, cx); }
        if !m.batch && self.entries.iter().any(|e| e.id == m.id && e.meta.local()) { return self.local_row_menu(m, cx); }
        // A document with children (a folder with a 正文, #357): the document's menu, its
        // file for what concerns the page, the folder for 移动到, 重命名 and 删除.
        let doc = self.note_or(&m.id);
        let child = !m.batch && self.takes_children(&m.id);
        let Some((pinned, starred)) = self.entries.iter().find(|e| e.id == doc).map(|e| (e.meta.pinned, e.meta.starred > 0)) else { return vec![] };
        let item = |id: &'static str, ic: &'static str, label: &'static str| MenuItem::new(id).icon(ic).label(crate::settings::label(label));
        let divider = MenuSeparator::new;
        if m.batch {
            // Right-click in 批量操作: the batch menu minus 添加快捷方式到, 转移所有权.
            let n = self.batch.as_ref().map_or(0, |b| b.len());
            let menu = Popover::new("batch-menu")
                .menu()
                .min_w(px(ROW_MENU_W))
                .on_mouse_down_out(cx.listener(|w, _, _, cx| {
                    w.row_menu = None;
                    cx.notify();
                }))
                .child(MenuLabel::new(crate::localized_format!("已选 {n} 项", "Selected: {n}")))
                .child(item("bm-copy", "copy", crate::settings::text("创建副本")).on_click(cx.listener(|w, _, _, cx| w.batch_copy(cx))))
                .child(item("bm-move", "folder-input", crate::settings::text("移动到")).on_click(cx.listener(|w, _, window, cx| w.batch_move(window, cx))))
                .child(item("bm-delete", "bin", crate::settings::text("删除")).on_click(cx.listener(|w, _, _, cx| w.ask_batch_delete(cx))));
            return vec![
                deferred(anchored().anchor(m.corner).position(m.pos).snap_to_window_with_margin(px(8.)).child(menu.fit(crate::design::room(m.pos, m.corner))))
                    .with_priority(2)
                    .into_any_element(),
            ];
        }
        // Hovering 下载 opens its submenu; hovering any other item closes it.
        let set_sub = |open: bool| {
            cx.listener(move |w: &mut Workspace, hovered: &bool, _, cx| {
                if let Some(m) = w.row_menu.as_mut().filter(|m| *hovered && m.sub != open) {
                    m.sub = open;
                    cx.notify();
                }
            })
        };
        let (dup, mv, path, pin, star, dl, ren, del) = (doc.clone(), m.id.clone(), doc.clone(), doc.clone(), doc.clone(), doc.clone(), m.id.clone(), m.id.clone());
        let pdf = doc.clone();
        let new_window = doc.clone();
        let sub_doc = m.id.clone();
        let sub_bounds = self.row_sub_bounds.clone();
        // The menu follows its longest translation too (#225); its rendered
        // width (from the previous frame — a submenu only opens after one)
        // places the 下载 submenu beside it.
        let menu_w_record = self.row_menu_w.clone();
        // The row menu leaves out the cloud entries 在新标签页打开, 分享,
        // 复制链接, 添加快捷方式到, 转移所有权, 设置密级.
        let menu = Popover::new("row-menu")
            .menu()
            .min_w(px(ROW_MENU_W))
            .child(canvas(move |b, _, _| menu_w_record.set(Some(f32::from(b.size.width))), |_, _, _, _| {}).absolute().inset_0())
            .on_mouse_down_out(cx.listener(move |w, ev: &MouseDownEvent, _, cx| {
                if sub_bounds.get().is_none_or(|b| !b.contains(&ev.position)) {
                    w.row_menu = None;
                    cx.notify();
                }
            }))
            .when(child, |d| d.child(
                item("rm-child", "plus", crate::settings::tr("新建子文档", "New child page"))
                    .on_hover(set_sub(false))
                    .on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.new_child(&sub_doc, window, cx); })),
            ))
            .child(
                item("rm-window", "copy", crate::settings::text("在新窗口中打开"))
                    .on_hover(set_sub(false))
                    .on_click(cx.listener(move |w, _, _, cx| {
                        w.row_menu = None;
                        let id = new_window.clone();
                        cx.defer(move |cx| crate::quick_note::open_document(Some(id), cx));
                        cx.notify();
                    })),
            )
            .child(
                item("rm-copy", "copy", crate::settings::text("创建副本"))
                    .on_hover(set_sub(false))
                    .on_click(cx.listener(move |w, _, window, cx| w.duplicate_doc(&dup, window, cx))),
            )
            .child(
                item("rm-move", "folder-input", crate::settings::text("移动到"))
                    .on_hover(set_sub(false))
                    .on_click(cx.listener(move |w, _, window, cx| w.open_move(vec![mv.clone()], window, cx))),
            )
            .child(
                // the file (or folder) picked out in Finder, as a 本机文件夹's row offers
                item("rm-reveal", "folder", crate::local_folders::show_in_files())
                    .on_hover(set_sub(false))
                    .on_click(cx.listener(move |w, _, _, cx| {
                        w.row_menu = None;
                        if let Some(path) = w.store.file_path(&path) { cx.reveal_path(&path); }
                        cx.notify();
                    })),
            )
            .child(
                item("rm-pin", if pinned { "pin-off" } else { "pin" }, if pinned { crate::settings::text("从“置顶”移除") } else { crate::settings::text("添加到“置顶”") })
                    .on_hover(set_sub(false))
                    .on_click(cx.listener(move |w, _, _, cx| w.toggle_pin(&pin, cx))),
            )
            .child(
                item("rm-star", if starred { "star-off" } else { "star" }, if starred { crate::settings::text("取消收藏") } else { crate::settings::tr("收藏", "Add to favorites") })
                    .on_hover(set_sub(false))
                    .on_click(cx.listener(move |w, _, _, cx| w.toggle_star(&star, cx))),
            )
            .child(divider())
            .child(
                item("rm-download", "export", crate::settings::text("下载"))
                    .submenu(true)
                    .open(m.sub)
                    .on_hover(set_sub(true)),
            )
            .child(
                item("rm-rename", "square-pen", crate::settings::text("重命名"))
                    .on_hover(set_sub(false))
                    .on_click(cx.listener(move |w, _, window, cx| w.start_rename(&ren, window, cx))),
            )
            .child(divider())
            .child(
                item("rm-del", "bin", crate::settings::text("删除")).destructive(true)
                    .on_hover(set_sub(false))
                    .on_click(cx.listener(move |w, _, _, cx| w.ask_delete(&del, cx))),
            );
        let extra = usize::from(child);
        let menu_h = row_menu_height(ROW_ITEMS + extra, ROW_DIVIDERS);
        // From the "…" button: where `design::hang` put it (under the button, or above when there's no
        // room below). From a right-click: at the pointer.
        let (origin, corner) = (m.pos, m.corner);
        // capped to the room there (`room`), scrolled under the shared scrollbar: the submenu levels
        // with 下载 where it is now
        let room = crate::design::room(origin, corner);
        let scrolled = window.use_keyed_state("row-menu-scroll", cx, |_, _| ScrollHandle::new()).read(cx).clone();
        let menu_top = match corner {
            Anchor::BottomLeft | Anchor::BottomRight => f32::from(origin.y) - menu_h.min(room),
            _ => f32::from(origin.y),
        } + f32::from(scrolled.offset().y);
        let mut out = vec![
            deferred(anchored().anchor(corner).position(origin).snap_to_window_with_margin(px(8.)).child(menu.fit(room).scroll(&scrolled)))
                .with_priority(2)
                .into_any_element(),
        ];
        if m.sub {
            let record = self.row_sub_bounds.clone();
            record.set(None);
            // Level the submenu's first item with 下载.
            let top = row_sub_top(menu_top + extra as f32 * ROW_ITEM);
            let menu_w = self.row_menu_w.get().unwrap_or(ROW_MENU_W);
            let left = match corner {
                Anchor::TopRight | Anchor::BottomRight => origin.x - px(menu_w),
                _ => origin.x,
            };
            let x = left + px(menu_w + 4.);
            out.push(
                deferred(
                    anchored().anchor(Anchor::TopLeft).position(point(x, px(top))).snap_to_window_with_margin(px(8.)).child(
                        Popover::new("rm-download-menu")
                            .menu()
                            .min_w(px(130.))
                            .child(canvas(move |b, _, _| record.set(Some(b)), |_, _, _, _| {}).absolute().inset_0())
                            .when(crate::printing::SUPPORTED, |d| d.child(crate::docmenu::sub_item("rm-pdf", "PDF").on_click(cx.listener(move |w, _, window, cx| w.export_pdf(&pdf, window, cx)))))
                            .child(crate::docmenu::sub_item("rm-md", "Markdown").on_click(cx.listener(move |w, _, _, cx| w.open_export_md(&dl, cx)))),
                    ),
                )
                .with_priority(2)
                .into_any_element(),
            );
        } else {
            self.row_sub_bounds.set(None);
        }
        out
    }
}
