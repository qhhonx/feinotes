//! Left pane: document list with search. Rows, sections and footer, sunken:
//! no panel, border or shadow, straight on the window surface (only the note
//! floats). 30pt rows on a 31pt pitch, selection pill radius 9.

use crate::app::{Confirm, RowMenu, View, Workspace};
use crate::settings::display_title;
use crate::store::Entry;
use crate::theme::{self, Theme, craft};
use crate::ui::{TipExt, TitleBarExt, icon, popover, ui_text};
use gpui::{prelude::FluentBuilder, *};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SidebarSection { Personal, Shared, Pinned, Local }

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SidebarEntry { pub section: SidebarSection, pub id: String }

impl SidebarEntry {
    pub fn new(section: SidebarSection, id: &str) -> Self { Self { section, id: id.into() } }
}

/// Row "…" menu geometry: 32pt rows, 11pt dividers, 6pt padding inside
/// a 1pt border.
pub(crate) const ROW_MENU_W: f32 = 180.;
const ROW_ITEM: f32 = crate::ui::MENU_ROW_H;
const ROW_DIVIDER: f32 = 11.;
const ROW_ITEMS: usize = 9;
const ROW_DIVIDERS: usize = 2;
const DOWNLOAD_INDEX: usize = 6;
const DIVIDERS_BEFORE_DOWNLOAD: usize = 1;

const fn row_menu_height(items: usize, dividers: usize) -> f32 {
    2. + crate::ui::MENU_PAD_Y * 2. + items as f32 * ROW_ITEM + dividers as f32 * ROW_DIVIDER
}

const fn row_sub_top(menu_top: f32) -> f32 {
    menu_top + DOWNLOAD_INDEX as f32 * ROW_ITEM + DIVIDERS_BEFORE_DOWNLOAD as f32 * ROW_DIVIDER
}

const _: () = assert!(row_menu_height(ROW_ITEMS, ROW_DIVIDERS) == 324.);
const _: () = assert!(row_sub_top(0.) == 203.);

/// Sidebar geometry (measured from the window edge), shared by the
/// document sidebar and 设置's: selection pills start 14pt in, every row is
/// 30pt with radius 9 on a 31pt pitch, glyphs (18pt) centre at 37pt and
/// labels start at 54pt; groups are 16pt apart, a section header is 34pt
/// (6 above its 28pt label).
pub(crate) const SB_X: f32 = 14.;
pub(crate) const ROW_H: f32 = 30.;
const DOC_ROW_H: f32 = ROW_H;
pub(crate) const ROW_R: f32 = 9.;
pub(crate) const ROW_GAP: f32 = 1.;
pub(crate) const ROW_ICON: f32 = 18.;
pub(crate) const GROUP_GAP: f32 = 16.;
pub(crate) const SECTION_H: f32 = 34.;
pub(crate) const SECTION_TOP: f32 = 6.;

/// 批量操作 caps at 50 pages.
pub(crate) const BATCH_LIMIT: usize = 50;

pub(crate) fn surface() -> Option<Hsla> { crate::themes::interface().color("sidebar.bg") }
pub(crate) fn text_color() -> Hsla { crate::themes::interface().color("sidebar.text").unwrap_or_else(Theme::sidebar_text) }
pub(crate) fn section_color() -> Hsla { crate::themes::interface().color("sidebar.section").unwrap_or_else(Theme::sidebar_section) }
pub(crate) fn folder_color() -> Hsla { crate::themes::interface().color("sidebar.folder").unwrap_or_else(Theme::icon) }
pub(crate) fn hover() -> Hsla { crate::themes::interface().color("sidebar.hover").unwrap_or_else(Theme::sidebar_hover) }
pub(crate) fn selection_fill() -> Hsla { crate::themes::interface().color("sidebar.selected").unwrap_or_else(Theme::sidebar_selected) }

pub(crate) fn shared_document_badge() -> Stateful<Div> {
    div().id("shared-document-status").flex_none().flex().items_center()
        .child(icon("shared-document", 14., Theme::text_caption()))
        .tip(crate::settings::tr("已共享", "Shared"), None)
}

pub(crate) fn shared_unread_dot() -> Div { div().size(px(6.)).flex_none().rounded_full().bg(Theme::primary()) }

pub(crate) fn row_action_slot() -> Div {
    div().relative().size(px(22.)).mr(px(4.)).flex_none().flex().items_center().justify_center()
}

pub(crate) fn folder_glyph(entry: &Entry, open: bool, _mark_shared: bool, _cx: &App) -> Option<&'static str> {
    entry.meta.folder.as_ref()?;
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    if _mark_shared && (entry.meta.share_root.is_some()
        || _cx.try_global::<crate::cloud_sync::activity::ActivityView>().is_some_and(|view|
            view.0.iter().any(|row| row.room.scope.folder && row.room.scope.note == entry.id))) {
        return Some(if open { "folder-shared-open" } else { "folder-shared" });
    }
    Some(if open { "folder-open" } else { "folder" })
}

pub(crate) fn entry_icon(entry: &Entry, open: bool, mark_shared: bool, cx: &App) -> Div {
    let folder = folder_glyph(entry, open, mark_shared, cx);
    div().size(px(20.)).flex_none().flex().items_center().justify_center().relative().map(|d| match &entry.icon {
        _ if let Some(glyph) = folder => d.child(icon(glyph, ROW_ICON, folder_color())),
        _ if entry.meta.markdown => d.child(icon("doc-md", ROW_ICON, Theme::icon())),
        Some(emoji) => d.text_size(px(theme::ui_font())).line_height(px(theme::ui_font() + 7.)).child(emoji.clone()),
        None => d.child(icon("doc", ROW_ICON, Theme::icon())),
    })
}

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
        .text_color(text_color())
        .hover(|s| s.bg(hover()))
        .child(div().size(px(20.)).flex_none().flex().items_center().justify_center().mr(px(9.)).child(icon(glyph, ROW_ICON, Theme::icon())))
        .child(label)
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

/// 14px checkbox; an unchecked box turns blue when `group` is hovered.
pub(crate) fn checkbox(state: Check, disabled: bool, group: &'static str) -> Div {
    let white = Theme::bg_body();
    div()
        .size(px(14.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .when(state != Check::Off, |d| d.bg(Theme::primary()))
        .when(state == Check::Off, |d| {
            d.border_1()
                .when(disabled, |d| d.border_color(Theme::line_border()).bg(theme::c(craft::N100)))
                .when(!disabled, |d| d.border_color(theme::c(craft::N500)).bg(white).group_hover(group, |s| s.border_color(Theme::primary())))
        })
        .when(state == Check::On, |d| d.child(icon("todo-check", 12., white)))
        .when(state == Check::Partial, |d| d.child(div().w(px(8.)).h(px(2.)).rounded(px(1.)).bg(white)))
}

pub(crate) fn batch_check_slot() -> Div {
    div().size(px(20.)).flex_none().flex().items_center().justify_center()
}

impl Workspace {
    pub(crate) fn sidebar_selection(&self, folder: bool) -> Hsla {
        let key = if folder { "sidebar.folder.focused" } else { "sidebar.focused" };
        if self.sidebar_focused && self.sidebar_window_active {
            crate::themes::interface().color(key).unwrap_or_else(selection_fill)
        } else { selection_fill() }
    }

    fn sidebar_selection_text(&self, folder: bool) -> Hsla {
        if folder && self.sidebar_focused && self.sidebar_window_active {
            crate::themes::interface().color("sidebar.folder.focused.text").unwrap_or_else(Theme::text)
        } else { Theme::text() }
    }

    /// Sidebar width for the current window: the dragged width, kept between
    /// 268 and min(800, window − 540).
    pub(crate) fn sidebar_w(&self, window: &Window) -> f32 {
        let vw = f32::from(window.viewport_size().width);
        let max = theme::SIDEBAR_MAX.min(vw - theme::SIDEBAR_MIN_CONTENT).max(theme::SIDEBAR_MIN);
        self.sidebar_width.clamp(theme::SIDEBAR_MIN, max)
    }

    pub(crate) fn render_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let entries = self.personal_navigation_entries(cx);
        self.folder_drag.personal_rows = (self.is_shared() || cfg!(all(target_os = "macos", feature = "cloudkit-sync"))).then(|| entries.clone());
        let mut pinned: Vec<Entry> = entries.iter().filter(|entry| entry.meta.pinned).cloned().collect();
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        pinned.extend(self.shared_sidebar_entries(cx).into_iter().filter(|entry| entry.meta.pinned));
        let mut seen = std::collections::HashSet::new();
        pinned.retain(|entry| seen.insert(entry.id.clone()));
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let shared_rows = (!crate::cloud_restore::local_only() && !self.sharing.folded).then(|| self.shared_sidebar_entries(cx));
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        let shared_rows: Option<Vec<Entry>> = None;
        self.sidebar_selection_row = self.resolve_sidebar_entry(&entries, shared_rows.as_deref(), &pinned, cx);
        self.more_bounds.borrow_mut().clear();
        self.settle_sidebar_drop(cx);

        let header = div()
            .h(px(crate::app::HEADER_H))
            .flex_none()
            .flex()
            .items_center()
            .pl(px(theme::TRAFFIC_LIGHTS_W))
            .pr(px(12.))
            .title_bar()
            .child(
                crate::ui::toolbar_control(div()
                    .id("collapse-sidebar")
                    .no_drag()
                    .flex_none()
                    .ml(px(4.))
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer(), 8., hover, None)
                    .tip(crate::settings::tr("收起侧栏", "Hide sidebar"), Some(crate::shortcuts::hint("workspace::ToggleSidebar", "⌘ + ]")))
                    .child(crate::ui::toolbar_icon("panel-left-close", 17.))
                    .on_click(cx.listener(|w, _, _, cx| {
                        w.sidebar_open = false;
                        cx.notify();
                    })),
            )
            .child(div().flex_1())
            .children(crate::settings::update_pill(cx));

        // The icon column is shared with 搜索 and the doc rows.
        let home_on = self.view == View::Home && !self.is_shared();
        let nav = div().flex_none().pt(px(4.)).pb(px(4.)).px(px(SB_X)).child(
            div()
                .id("nav-home")
                .h(px(ROW_H))
                .pl(px(6.))
                .pr(px(4.))
                .flex()
                .items_center()
                .rounded(px(ROW_R))
                .cursor_pointer()
                .text_size(px(theme::ui_font()))
                .line_height(px(20.))
                .when(home_on, |d| d.bg(selection_fill()).text_color(Theme::text()))
                .when(!home_on, |d| d.text_color(text_color()).hover(|s| s.bg(hover())))
                .child(
                    // Selected or not, the same outline: the row's fill marks the selection.
                    div().size(px(20.)).flex_none().flex().items_center().justify_center().mr(px(9.)).child(icon(
                        "home",
                        ROW_ICON,
                        Theme::icon(),
                    )),
                )
                .child(crate::settings::tr("主页", "Home"))
                .on_click(cx.listener(|w, _, window, cx| w.navigate_home(window, cx))),
        );

        let mut list = div().flex().flex_col().px(px(SB_X)).pb(px(8.));
        // Pinned docs are shortcuts: they stay in 我的文档 too.
        if !pinned.is_empty() {
            list = list.child(self.section(0, crate::settings::text("置顶"), cx));
            if !self.folded[0] {
                for e in pinned {
                    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                    if self.shared_list_room(&e.id, cx).is_some() || e.meta.share_root.as_deref().is_some_and(|root| root != e.id) { list = list.child(self.shared_shortcut_row(&e, cx)); continue; }
                    list = list.child(self.doc_row(&e, "pinned", 0, false, cx));
                }
            }
            list = list.child(div().h(px(GROUP_GAP - SECTION_TOP)));
        }
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if !crate::cloud_restore::local_only() { list = list.child(self.shared_sidebar(shared_rows, window, cx)).child(div().h(px(GROUP_GAP - SECTION_TOP))); }
        // Dropped from outside with no tree shown to open a slot in: the heading lights.
        // 工作区: one root is headed by its folder's name (W5); several show
        // as rows of the tree under 文件夹; none, a way to open one.
        let roots = self.store.roots();
        if roots.is_empty() {
            list = list.child(self.empty_roots(cx));
        } else {
            let title = if crate::FEI_NOTES { crate::settings::tr("我的文档", "My documents") }
                else if roots.len() == 1 { &roots[0].1 } else { crate::settings::tr("文件夹", "Folders") };
            list = list.child(self.section(1, title, cx).rounded(px(ROW_R)).when(self.sidebar_drop_top(), |d| d.bg(Theme::sidebar_selected())));
            if !self.folded[1] {
                list = list.child(self.render_folder_rows(window, cx));
            }
        }
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
        // The 批量操作 toolbar keeps the footer glyphs: bare on the sidebar, a rounded fill on hover.
        let footer_btn = |id: &'static str, name: &'static str, tip: &'static str, enabled: bool, on: bool| {
            let group = SharedString::from(format!("{id}-slot"));
            div()
                .id(id)
                .flex_none()
                .w(px(36.))
                .h(px(32.))
                .group(group.clone())
                .when(enabled, |d| d.cursor_pointer())
                .tip(tip, None)
                .child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(8.))
                        .when(on, |d| d.bg(selection_fill()))
                        .when(enabled && !on, |d| d.group_hover(group, |s| s.bg(hover())))
                        .child(match (on, enabled) {
                            (true, _) => icon(&format!("{name}-filled"), 17., Theme::icon()),
                            (false, true) => icon(name, 17., Theme::icon()),
                            (false, false) => icon(name, 17., Theme::text_disabled()),
                        }),
                )
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
                        .on_click(cx.listener(|w, _, window, cx| w.open_global_settings(window, cx))),
                ))
            })
            // 批量操作 toolbar: 创建副本 | 移动到 | 删除, minus the cloud ones.
            .when(self.batch.is_some(), |d| {
                d.child(
                    div().h(px(46.)).mx(px(SB_X + 6. + 10. - 18.)).flex().items_center().gap(px(4.))
                        .child(footer_btn("bt-copy", "copy", crate::settings::text("创建副本"), some, false).on_click(cx.listener(|w, _, _, cx| w.batch_copy(cx))))
                        .when(!self.batch_contains_shared(cx), |d| d.child(footer_btn("bt-move", "folder-input", crate::settings::text("移动到"), some, false).on_click(cx.listener(|w, _, window, cx| w.batch_move(window, cx)))))
                        .child(footer_btn("bt-delete", "bin", crate::settings::text("删除"), some, false).on_click(cx.listener(|w, _, window, cx| w.ask_batch_delete(window, cx)))),
                )
            });

        div()
            .w(px(self.sidebar_w(window)))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .when_some(surface(), |d, c| d.bg(c))
            .child(header)
            .child(nav)
            .child(self.sidebar_drop_zone(div().id("doc-list").flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.list_scroll).child(list), cx))
            .children(crate::changelog::card(cx.listener(|_, _, _, cx| cx.notify())))
            .child(footer)
            .into_any_element()
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    pub(crate) fn shared_sidebar(&mut self, entries: Option<Vec<Entry>>, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let folded = self.sharing.folded;
        let rows = cx.try_global::<crate::cloud_sync::activity::ActivityView>().map(|view| view.0.clone()).unwrap_or_default();
        let header = if self.shared_batch() {
            self.batch_section(true, rows.iter().filter(|row| !row.room.scope.folder).take(BATCH_LIMIT).map(|row| row.room.scope.note.clone()).collect(), cx)
        } else {
            self.section_heading(None, crate::settings::tr("共享文档", "Shared documents"), folded, folded && rows.iter().any(|row| row.unread), cx)
                .child(self.section_invitation_button(cx)).child(self.section_batch_button(true, cx)).child(self.section_add_button(true, cx))
        };
        let mut section = div().flex().flex_col().child(header);
        if let Some(entries) = entries { section = section.child(self.render_shared_sidebar_rows(entries, window, cx)); }
        section
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

    /// Section header: the title folds the list (an
    /// arrow appears on hover); 我的文档 has "+" and 批量操作 on the right,
    /// laid out so the last one lines up with the rows' "…".
    pub(crate) fn section(&self, idx: usize, title: impl AsRef<str>, cx: &mut Context<Self>) -> Div {
        if idx == 1 && self.batch.is_some() && !self.shared_batch() {
            let ids = self.personal_navigation_entries(cx).into_iter().take(BATCH_LIMIT).map(|entry| entry.id).collect();
            return self.batch_section(false, ids, cx);
        }
        self.section_heading(Some(idx), title, self.folded[idx], false, cx)
            .when(idx == 1, |row| row.child(self.section_batch_button(false, cx)).child(self.section_add_button(false, cx)))
    }

    fn section_heading(&self, idx: Option<usize>, title: impl AsRef<str>, folded: bool, unread: bool, cx: &mut Context<Self>) -> Div {
        let slot = idx.map_or_else(|| "shared".to_string(), |idx| idx.to_string());
        let group = SharedString::from(format!("section-{slot}"));
        let row = div().h(px(SECTION_H)).pl(px(4.)).pt(px(SECTION_TOP)).flex().items_center().group(group.clone());
        row.child(
            div()
                // Keyed by state so the tooltip is rebuilt after a toggle.
                .id(SharedString::from(format!("section-title-{slot}-{folded}")))
                .px(px(4.))
                .h(px(22.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|s| s.bg(hover()))
                .tip(if folded { crate::settings::text("点击展开") } else { crate::settings::text("点击折叠") }, None)
                .child(div().pr(px(4.)).text_size(px(theme::ui_font())).line_height(px(theme::ui_font() + 7.)).font_weight(FontWeight::SEMIBOLD).text_color(section_color()).child(crate::settings::label(title)))
                .child(
                    div()
                        .when(!folded, |d| d.opacity(0.).group_hover(group, |s| s.opacity(1.)))
                        .child(icon(if folded { "chevron-right" } else { "chevron-down" }, 10., Theme::icon())),
                )
                .when(unread, |d| d.child(shared_unread_dot().ml(px(6.))))
                .on_click(cx.listener(move |w, _, _, cx| {
                    if let Some(idx) = idx {
                        w.folded[idx] = !w.folded[idx];
                        if idx == 1 { w.folder_drag.reset(); }
                    }
                    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                    if idx.is_none() { w.sharing.folded = !w.sharing.folded; if !w.sharing.folded { w.sharing.sidebar.reset(); } }
                    cx.notify();
                })),
        )
        .child(div().flex_1())
    }

    fn section_trigger(id: &'static str, glyph: &'static str) -> Stateful<Div> {
        div().id(id).flex_none().size(px(22.)).p(px(3.)).rounded(px(6.)).cursor_pointer()
            .hover(|s| s.bg(hover())).child(icon(glyph, 16., Theme::icon()))
    }

    fn section_batch_button(&self, shared: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        Self::section_trigger(if shared { "shared-section-batch" } else { "section-batch" }, "batch")
            .when(shared, |d| d.ml(px(2.)))
            .tip(crate::settings::text("批量操作"), None).on_click(cx.listener(move |w, _, window, cx| {
                #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                if shared {
                    w.batch = Some(vec![]); w.sharing.batch = true; w.sharing.folded = false;
                    if w.view == View::Home { w.home.tab = crate::home::HomeTab::Shared; }
                    w.row_menu = None; cx.notify(); return;
                }
                w.personal_action(window, cx, |w, _, cx| {
                    w.begin_personal_batch(); w.folded[1] = false;
                    if w.view == View::Home { w.home.tab = crate::home::HomeTab::Recent; }
                    w.row_menu = None; cx.notify();
                })
            }))
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn section_invitation_button(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        Self::section_trigger("shared-section-accept", "mail-check").ml(px(2.))
            .child(self.home.track("accept-sidebar"))
            .when(self.accept_invitation_open(), |d| d.bg(selection_fill()))
            .tip(crate::settings::tr("接受邀请", "Accept invitation"), None)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|w, _, window, cx| w.open_accept_invitation_from("accept-sidebar", window, cx)))
    }

    fn section_add_button(&self, shared: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        use crate::folders::CreateTarget;
        let key = if shared { "create-shared-sidebar" } else { "create-sidebar" };
        let target = CreateTarget::PersonalSidebar(None);
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let target = if shared { CreateTarget::SharedSidebar } else { target };
        let open = self.create_menu.as_ref().is_some_and(|(item, _)| *item == target);
        Self::section_trigger(if shared { "shared-section-add" } else { "section-add" }, "plus")
            .ml(px(2.)).mr(px(4.)).child(self.home.track(key)).when(open, |d| d.bg(selection_fill()))
            .tip(if shared { crate::settings::tr("新建并共享", "Create and share") } else { crate::settings::tr("新建", "New") }, None)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |w, ev: &ClickEvent, _window, cx| {
                let pos = w.home.anchor(key).map_or(ev.position(), |b| point(b.left(), b.bottom() + px(4.)));
                #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                if shared && w.accept_invitation_open() { w.close_sharing(_window, cx); }
                w.create_menu = (!open).then(|| (target.clone(), pos));
                cx.notify();
            }))
    }

    /// Check or uncheck a 我的文档 row (entering 批量操作 if needed); false
    /// when the 50-page limit blocks it.
    pub(crate) fn toggle_check(&mut self, id: &str) -> bool {
        if crate::shared_store::root_of_row(id).is_some() { return false; }
        if !self.shared_batch() { self.folded[1] = false; }
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
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let shared_ids = self.batch.as_ref().into_iter().flatten().filter(|id| self.shared_list_room(id, cx).is_some()).cloned().collect::<Vec<_>>();
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if self.shared_batch() { self.copy_shared_documents(shared_ids, cx); return; }
        let file_before = self.store.track_file_changes();
        let ids = self.checked();
        if ids.is_empty() {
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            self.copy_shared_documents(shared_ids, cx);
            return;
        }
        for id in &ids {
            self.copy_doc(id, cx);
        }
        self.entries = self.store.list();
        self.sort();
        self.batch = None;
        self.row_menu = None;
        self.remember_files(file_before, crate::settings::tr("批量创建副本", "Duplicate selection"));
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if !shared_ids.is_empty() { self.copy_shared_documents(shared_ids, cx); }
        cx.notify();
    }

    pub(crate) fn batch_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.checked();
        if !ids.is_empty() {
            self.open_move(ids, window, cx);
        }
    }

    pub(crate) fn ask_batch_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if self.batch_contains_shared(cx) { self.confirm_shared_batch_removal(window, cx); return; }
        let _ = window;
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
        let stable_menu = cfg!(target_os = "macos") && e.meta.folder.is_some();
        let in_lib = key == "doc";
        div()
            .id(SharedString::from(format!("{key}-{}", e.id)))
            .group("doc-row")
            .mb(px(ROW_GAP))
            .rounded(px(ROW_R))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |w, ev: &MouseDownEvent, window, cx| {
                    if !in_lib { w.row_menu = None; cx.notify(); return; }
                    if w.rename.as_ref().is_some_and(|r| r.id == id) { return; }
                    w.row_menu = None;
                    // In 批量操作 a click checks the row; Shift-click enters it.
                    if w.batch.is_some() || ev.modifiers.shift {
                        let id = id.clone();
                        w.personal_action(window, cx, move |w, _, cx| {
                            if w.shared_batch() { w.begin_personal_batch(); }
                            w.toggle_check(&id);
                            cx.notify();
                        });
                    } else {
                        // The projected personal rows can be pressed while a shared editor stays open.
                        w.arm_folder_drag(&id, ev.position, cx);
                    }
                }),
            )
            .when(key == "pinned", |d| { let id = e.id.clone(); d.on_click(cx.listener(move |w, _, window, cx| { w.open_personal_sidebar_item(&id, SidebarSection::Pinned, window, cx); })) })
            // A 本机文件 row opens on click, and can be carried up into the tree to import it.
            .when(key == "local", |d| {
                let (open, drag) = (e.id.clone(), crate::sidebar_drop::LocalRowDrag { id: e.id.clone(), title: crate::settings::display_title(&e.title), folder: e.meta.folder.is_some() });
                d.on_click(cx.listener(move |w, _, window, cx| { w.open_personal_sidebar_item(&open, SidebarSection::Local, window, cx); }))
                    .on_drag(drag, |d, _, _, cx| cx.new(|_| d.clone()))
            })
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |w, ev: &MouseDownEvent, window, cx| {
                    let id2 = id2.clone(); let pos = ev.position;
                    if stable_menu {
                        w.row_menu = Some(RowMenu { id: id2, pos, corner: Anchor::TopLeft, sub: false, batch: false });
                        w.create_menu = None;
                        cx.notify();
                        return;
                    }
                    w.personal_action(window, cx, move |w, _, cx| {
                    let batch = in_lib && w.batch.is_some();
                    if batch && !w.batch.as_ref().is_some_and(|b| b.contains(&id2)) && !w.toggle_check(&id2) {
                        return;
                    }
                    w.row_menu = Some(RowMenu { id: id2.clone(), pos, corner: Anchor::TopLeft, sub: false, batch });
                    cx.notify();
                    });
                }),
            )
    }

    pub(crate) fn more_button(&self, e: &Entry, key: &str, cx: &mut Context<Self>) -> Stateful<Div> {
        self.more_button_with_status(e, key, None, cx)
    }

    pub(crate) fn document_has_shared_status(&self, entry: &Entry, cx: &App) -> bool {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        { self.published_document_share(entry, cx) }
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        { let _ = (entry, cx); false }
    }

    pub(crate) fn more_button_with_status(&self, e: &Entry, key: &str, status: Option<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = if key.starts_with("shared") { format!("shared:{}", e.id) } else { e.id.clone() };
        let stable_menu = cfg!(target_os = "macos") && e.meta.folder.is_some();
        let shared = key.starts_with("shared");
        let slot = format!("{key}-{}", e.id);
        let slot2 = slot.clone();
        let record = self.more_bounds.clone();
        let lookup = self.more_bounds.clone();
        let open = self.row_menu.as_ref().is_some_and(|m| m.id == id && m.pos.x < px(0.));
        let status = status.filter(|_| !open);
        let glyph = icon("ellipsis", 15., Theme::icon());
        let action = if status.is_some() {
            div().flex().items_center().opacity(0.).group_hover("doc-row", |s| s.opacity(1.)).child(glyph).into_any_element()
        } else { glyph.into_any_element() };
        // 我的文档 rows fade their buttons in with the hover wash.
        let reveal = (key == "doc").then(|| self.folder_drag.reveal(&e.id));
        row_action_slot()
            .id(SharedString::from(format!("more-{slot}")))
            .rounded(px(6.))
            .when(!open && status.is_none(), |d| match reveal { Some(a) => d.opacity(a), None => d.opacity(0.).group_hover("doc-row", |s| s.opacity(1.)) })
            .when(open, |d| d.bg(selection_fill()))
            .hover(|s| s.bg(selection_fill()))
            .child(action)
            .when_some(status, |d, status| d.child(div().absolute().inset_0().flex().items_center().justify_center()
                .group_hover("doc-row", |s| s.opacity(0.)).child(status)))
            .child(canvas(move |b, _, _| record.borrow_mut().insert(slot.clone(), b), |_, _, _, _| {}).absolute().inset_0())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |w, ev: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    // The row menu's outside-click handler has already closed it on this press, so
                    // a press on its own button closes it rather than reopening it (see `open`).
                    if open { w.row_menu = None; cx.notify(); return; }
                    // Hang the menu from the button; a negative x marks "from the button".
                    let b = lookup.borrow().get(&slot2).copied();
                    let pos = b.map_or(ev.position, |b| point(-b.left() - px(1.), b.top()));
                    let id = id.clone();
                    if shared || stable_menu || id.starts_with("collection-") {
                        w.row_menu = Some(RowMenu { id, pos, corner: Anchor::TopLeft, sub: false, batch: false });
                        w.create_menu = None;
                        cx.notify();
                    }
                    else { w.personal_action(window, cx, move |w, _, cx| { w.row_menu = Some(RowMenu { id, pos, corner: Anchor::TopLeft, sub: false, batch: false }); cx.notify(); }); }
                }),
            )
    }

    /// Creation inside a folder: the folder button makes one directly; the plus
    /// opens the same 新建 menu as 我的文档's, hung from the button.
    pub(crate) fn add_button(&self, e: &Entry, key: &str, folder: bool, target: crate::folders::CreateTarget, cx: &mut Context<Self>) -> Stateful<Div> {
        let slot = format!("add-{key}-{folder}-{}", e.id);
        let (record, lookup) = (self.more_bounds.clone(), self.more_bounds.clone());
        let anchor = |b: Bounds<Pixels>| point(b.left(), b.bottom() + px(4.));
        let open = !folder && self.create_menu.as_ref().is_some_and(|(item, _)| item == &target);
        div()
            .id(SharedString::from(format!("add-{key}-{folder}-{}", e.id)))
            .relative()
            .size(px(22.))
            .p(px(3.))
            .flex_none()
            .rounded(px(6.))
            .when(!open, |d| if key == "shared-folder" {
                d.opacity(0.).group_hover("doc-row", |s| s.opacity(1.))
            } else { d.opacity(self.folder_drag.reveal(&e.id)) })
            .when(open, |d| d.bg(selection_fill()))
            .hover(|s| s.bg(selection_fill()))
            .child(icon(if folder { "folder-plus" } else { "plus" }, 16., Theme::icon()))
            .child({ let slot = slot.clone(); canvas(move |b, _, _| { record.borrow_mut().insert(slot.clone(), b); }, |_, _, _, _| {}).absolute().inset_0() })
            .tip(crate::settings::tr(if folder { "新建文件夹" } else { "新建" }, if folder { "New folder" } else { "New" }), None)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |w, ev: &ClickEvent, window, cx| {
                cx.stop_propagation();
                w.row_menu = None;
                let pos = lookup.borrow().get(&slot).copied().map_or(ev.position(), anchor);
                if folder { w.new_folder(target.clone(), None, window, cx); }
                else { w.create_menu = (!open).then(|| (target.clone(), pos)); cx.notify(); }
            }))
    }

    /// Hover "+" on a document (新建子文档, #357): a new page under it, straight away.
    fn child_button(&self, e: &Entry, key: &str, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = e.id.clone();
        div()
            .id(SharedString::from(format!("child-{key}-{}", e.id)))
            .size(px(22.))
            .p(px(3.))
            .flex_none()
            .rounded(px(6.))
            .opacity(self.folder_drag.reveal(&e.id))
            .hover(|s| s.bg(Theme::sidebar_selected()))
            .child(icon("plus", 16., Theme::icon()))
            .tip(crate::settings::tr("新建子文档", "New child page"), None)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |w, _, window, cx| {
                cx.stop_propagation();
                w.row_menu = None;
                w.new_child(&id, window, cx);
            }))
    }

    pub(crate) fn folder_row_actions(&self, entry: &Entry, key: &str, target: Option<crate::folders::CreateTarget>, cx: &mut Context<Self>) -> Div {
        div().absolute().top_0().bottom_0().right_0().flex().items_center()
            .child(self.more_button(entry, key, cx))
            .when_some(target, |d, target| d.child(self.add_button(entry, key, true, target.clone(), cx).ml(px(2.)))
                .child(self.add_button(entry, key, false, target, cx).ml(px(2.)).mr(px(4.))))
    }

    pub(crate) fn sidebar_rename(&self, id: &str, section: SidebarSection) -> Option<&crate::app::Rename> {
        self.rename.as_ref().filter(|r| r.id == id && r.sidebar_section.is_none_or(|origin| origin == section))
    }

    pub(crate) fn sidebar_title(&self, e: &Entry, section: SidebarSection, selected: bool, drop_target: bool, cx: &App) -> AnyElement {
        let renaming = self.sidebar_rename(&e.id, section).map(|r| r.field.clone());
        let title = display_title(&e.title);
        match renaming {
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
                .flex()
                .items_center()
                .text_size(px(theme::ui_font()))
                .line_height(px(DOC_ROW_H))
                .text_color(if selected { self.sidebar_selection_text(e.meta.folder.is_some()) } else if drop_target { Theme::text() } else { text_color() })
                .when(selected, |d| d.when_some(crate::themes::interface().num_override("sidebar.selected.weight"), |d, w| d.font_weight(FontWeight(w))))
                .child(crate::ui::document_title(&title).child(ui_text(title)))
                .when(matches!(section, SidebarSection::Personal | SidebarSection::Pinned) && self.document_has_shared_status(e, cx), |d| {
                    d.child(shared_document_badge().ml(px(6.)))
                })
                .into_any_element(),
        }
    }

    pub(crate) fn doc_row(&self, e: &Entry, key: &str, depth: usize, _kids: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let drop_target = self.folder_drop_target(&e.id);
        let section = match key { "pinned" => SidebarSection::Pinned, "local" => SidebarSection::Local, _ => SidebarSection::Personal };
        let note = e.note.as_deref();
        let selected = self.sidebar_row_selected(section, note.unwrap_or(&e.id));
        let folder = e.meta.folder.is_some() && e.note.is_none();
        let child = key == "doc" && self.takes_children(&e.id);
        let lib_batch = self.batch.is_some() && key == "doc";
        let renaming = self.sidebar_rename(&e.id, section).is_some();
        let float = key == "doc" && !lib_batch && !renaming;
        let target = (float && folder).then(|| {
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            { self.sidebar_create_target(e) }
            #[cfg(not(all(target_os = "macos", feature = "cloudkit-sync")))]
            { crate::folders::CreateTarget::PersonalSidebar(Some(e.id.clone())) }
        });
        let menu_open = self.row_menu.as_ref().is_some_and(|m| m.id == e.id && !m.batch && m.pos.x < px(0.))
            || self.create_menu.as_ref().is_some_and(|(item, _)| target.as_ref() == Some(item));
        let give = if float { (if folder { 74. } else if child { 52. } else { 28. }) * if menu_open { 1. } else { self.folder_drag.reveal(&e.id) } } else { 0. };
        let title = div().flex_1().min_w_0().flex().mr(px(give)).child(self.sidebar_title(e, section, selected, drop_target, cx));
        // 批量操作: 我的文档 rows get a checkbox and lose "…"; checked rows are tinted.
        let batch = self.batch.as_ref().filter(|_| key == "doc" && !self.shared_batch());
        let checked = batch.is_some_and(|b| b.contains(&e.id));
        let blocked = batch.is_some_and(|b| !checked && b.len() >= BATCH_LIMIT);
        self.row_base(e, key, cx)
            .h(px(DOC_ROW_H))
            .w_full()
            .relative()
            .flex()
            .items_center()
            // In 我的文档 the selection pill and the hover wash are drawn under the row (`sidebar_drag.rs`).
            .when(checked || (selected && key != "doc"), |d| d.bg(if checked { selection_fill() } else { self.sidebar_selection(folder) }))
            .when(!selected && !checked && !drop_target && key != "doc", |d| d.hover(|s| s.bg(hover())))
            .pl(px(6. + 18. * depth as f32))
            .when(batch.is_some(), |d| {
                d.child(
                    batch_check_slot()
                        .id(SharedString::from(format!("check-{}", e.id)))
                        .mr(px(4.))
                        .when(blocked, |d| d.tip(crate::settings::text("无法选中，选择的页面数已到达上限"), None))
                        .child(checkbox(if checked { Check::On } else { Check::Off }, blocked, "doc-row")),
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
                    .when(toggle, |d| d.rounded(px(5.)).hover(|s| s.bg(hover()))
                        .on_mouse_down(MouseButton::Left, cx.listener(|w, ev: &MouseDownEvent, _, _| {
                            w.folder_drag.icon_press = true;
                            w.folder_drag.icon_alt = ev.modifiers.alt;
                        })))
                    .map(|d| match &e.icon {
                    _ if folder => {
                        let s = if key == "doc" { self.folder_drag.pop(&e.id) } else { 1. };
                        d.child(div().absolute().inset_0().flex().items_center().justify_center().opacity(1. - chevron)
                            .child(icon(if drop_target { "folder-input" } else { folder_glyph(e, open, true, cx).unwrap_or("folder") }, ROW_ICON, if selected && self.sidebar_focused && self.sidebar_window_active { crate::themes::interface().color("sidebar.folder.focused.text").unwrap_or_else(folder_color) } else { folder_color() })
                                .with_transformation(Transformation::scale(size(s, s)))))
                        .when(chevron > 0.005, |d| d.child(div().absolute().inset_0().flex().items_center().justify_center().opacity(chevron)
                            .child(icon("disclosure", ROW_ICON, if selected { self.sidebar_selection_text(true) } else { Theme::icon() })
                                .with_transformation(Transformation::rotate(radians(if open { std::f32::consts::FRAC_PI_2 } else { 0. }))))))
                    }
                    // A document with children: its own icon, turning into the chevron on hover like a folder's.
                    _ if note.is_some() && !drop_target => {
                        let face = div().absolute().inset_0().flex().items_center().justify_center().opacity(1. - chevron);
                        d.child(match &e.icon {
                            Some(em) if !e.meta.markdown => face.text_size(px(theme::ui_font())).line_height(px(theme::ui_font() + 7.)).child(em.clone()),
                            _ => face.child(icon(crate::search::file_glyph(&e.meta), ROW_ICON, Theme::icon())),
                        })
                        .when(chevron > 0.005, |d| d.child(div().absolute().inset_0().flex().items_center().justify_center().opacity(chevron)
                            .child(icon("disclosure", ROW_ICON, Theme::icon())
                                .with_transformation(Transformation::rotate(radians(if open { std::f32::consts::FRAC_PI_2 } else { 0. }))))))
                    }
                    _ if note.is_some() => d.child(icon("folder-input", ROW_ICON, Theme::icon())),
                    _ if e.meta.named_by_file() => d.child(icon(crate::search::file_glyph(&e.meta), ROW_ICON, Theme::icon())),
                    Some(em) => d.text_size(px(theme::ui_font())).line_height(px(theme::ui_font() + 7.)).child(em.clone()),
                    None => d.child(icon("doc", ROW_ICON, Theme::icon())),
                })
            })
            .child(title)
            // A folder's "…", "new folder" and "+" sit in the same columns as the header's
            // 批量操作, 新建文件夹 and 新建文档.
            .when(float && folder, |d| d.child(self.folder_row_actions(e, key, target.filter(|target| self.can_create_folder(target, cx)), cx)))
            .when(float && !folder, |d| d.child(div().absolute().top_0().bottom_0().right_0().flex().items_center()
                .child(self.more_button(e, key, cx))
                .when(child, |d| d.child(self.child_button(e, key, cx).ml(px(2.)).mr(px(4.))))))
            .when(!renaming && key != "doc" && batch.is_none(), |d| d.child(self.more_button(e, key, cx)))
    }

    pub(crate) fn render_row_menu(&self, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(m) = self.row_menu.clone() else { return vec![] };
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        {
            let shared_id = m.id.strip_prefix("shared:").unwrap_or(&m.id);
            #[cfg(target_os = "macos")]
            if !m.batch && let Some((room, _)) = self.shared_folder_context(shared_id, cx)
                && shared_id != room.scope.note {
                let section = if m.id.starts_with("shared:") || room.scope.owner.is_some() { SidebarSection::Shared } else { SidebarSection::Personal };
                return self.shared_nested_folder_menu(&m, room, SidebarEntry::new(section, shared_id), cx);
            }
            if self.shared_list_room(shared_id, cx).is_some() {
                let mut menu = m.clone();
                menu.id = shared_id.to_owned();
                let section = if m.id.starts_with("shared:") || self.home.shared() { SidebarSection::Shared } else { SidebarSection::Personal };
                return self.shared_row_menu(menu, section, window, cx);
            }
            if (m.id.starts_with("shared:") || !self.is_shared()) && let Some(room) = self.shared_child_room(shared_id,cx) {
                return self.shared_child_row_menu(&m,shared_id,room,cx);
            }
        }
        if let Some(key) = crate::shared_store::root_of_row(&m.id).map(str::to_string) { return self.root_row_menu(&key, m, cx); }
        if !m.batch && self.entries.iter().any(|e| e.id == m.id && e.meta.folder.is_some() && e.note.is_none()) { return self.folder_row_menu(m, cx); }
        if !m.batch && self.entries.iter().any(|e| e.id == m.id && e.meta.local()) { return self.local_row_menu(m, cx); }
        // A document with children (a folder with a 正文, #357): the document's menu, its
        // file for what concerns the page, the folder for 移动到, 重命名 and 删除.
        let doc = self.note_or(&m.id);
        let child = !m.batch && self.takes_children(&m.id);
        // The context menu: rows on the quiet hover fill.
        let item_c = |id: &'static str, ic: &'static str, label: &'static str, color: Hsla, icon_color: Hsla| {
            let g = SharedString::from(id);
            crate::ui::menu_row(id, g.clone())
                .mx(px(0.))
                .text_color(color)
                .child(icon(ic, 16., icon_color))
                .child(div().flex_1().min_w_0().child(crate::ui::menu_label(label, g, gpui::white())))
        };
        let item = |id, ic, label| item_c(id, ic, label, Theme::text(), Theme::icon());
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if m.id.starts_with("collection-") {
            let rename = m.id.clone(); let remove = rename.clone();
            let pos = if m.pos.x < px(0.) { point(-m.pos.x, m.pos.y + px(22.)) } else { m.pos };
            return vec![deferred(anchored().anchor(m.corner).position(pos).snap_to_window_with_margin(px(8.)).child(popover().id("private-folder-menu").occlude().w(px(180.))
                .on_mouse_down_out(cx.listener(|w, _, _, cx| { w.row_menu = None; cx.notify(); }))
                .child(item("private-folder-rename", "square-pen", crate::settings::tr("重命名", "Rename")).on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.rename_collection(rename.clone(), window, cx); })))
                .child(item("private-folder-remove", "folder", crate::settings::tr("移除分类并保留文档", "Remove folder, keep items")).on_click(cx.listener(move |w, _, _, cx| w.remove_collection(&remove, cx))))))
                .with_priority(2).into_any_element()];
        }
        if !m.batch && self.entries.iter().chain(self.folder_drag.personal_rows.iter().flatten())
            .any(|e| e.id == m.id && e.meta.folder.is_some()) { return self.folder_row_menu(m, cx); }
        if !m.batch && self.entries.iter().any(|e| e.id == m.id && e.meta.local()) { return self.local_row_menu(m, cx); }
        let Some((pinned, starred)) = self.entries.iter().find(|e| e.id == m.id).map(|e| (e.meta.pinned, e.meta.starred > 0)) else { return vec![] };
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let (pinned, starred) = self.shared_child_marks(&m.id,cx).map_or((pinned,starred), |(pin,star)| (pin,star>0));
        let divider = || div().mx(px(8.)).my(px(5.)).h(px(1.)).bg(Theme::line_border());
        if m.batch {
            // Right-click in 批量操作: the batch menu minus 添加快捷方式到, 转移所有权.
            let n = self.batch.as_ref().map_or(0, |b| b.len());
            let menu = popover()
                .id("batch-menu")
                .occlude()
                .min_w(px(ROW_MENU_W))
                .px(px(5.))
                .py(px(crate::ui::MENU_PAD_Y))
                .on_mouse_down_out(cx.listener(|w, _, _, cx| {
                    w.row_menu = None;
                    cx.notify();
                }))
                .child(div().h(px(ROW_ITEM)).px(px(8.)).flex().items_center().text_size(px(theme::ui_font_small())).text_color(Theme::text_placeholder()).child(crate::localized_format!("已选 {n} 项", "Selected: {n}")))
                .child(item("bm-copy", "copy", crate::settings::text("创建副本")).on_click(cx.listener(|w, _, _, cx| w.batch_copy(cx))))
                .when(!self.batch_contains_shared(cx), |d| d.child(item("bm-move", "folder-input", crate::settings::text("移动到")).on_click(cx.listener(|w, _, window, cx| w.batch_move(window, cx)))))
                .child(item("bm-delete", "bin", crate::settings::text("删除")).on_click(cx.listener(|w, _, window, cx| w.ask_batch_delete(window, cx))));
            return vec![
                deferred(anchored().anchor(m.corner).position(m.pos).snap_to_window_with_margin(px(8.)).child(menu))
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
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let share_row = !self.is_shared() && !crate::cloud_restore::local_only();
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        let share_row = false;
        // The menu follows its longest translation too (#225); its rendered
        // width (from the previous frame — a submenu only opens after one)
        // places the 下载 submenu beside it.
        let menu_w_record = self.row_menu_w.clone();
        let menu = popover()
            .id("row-menu")
            .occlude()
            .min_w(px(ROW_MENU_W))
            .child(canvas(move |b, _, _| menu_w_record.set(Some(f32::from(b.size.width))), |_, _, _, _| {}).absolute().inset_0())
            .px(px(5.))
            .py(px(crate::ui::MENU_PAD_Y))
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
                item("rm-pin", if pinned { "pin-off" } else { "pin" }, if pinned { crate::settings::text("从“置顶”移除") } else { crate::settings::text("添加到“置顶”") })
                    .on_hover(set_sub(false))
                    .on_click(cx.listener(move |w, _, _, cx| w.toggle_pin(&pin, cx))),
            )
            .child(
                item("rm-star", if starred { "star-off" } else { "star" }, if starred { crate::settings::text("取消收藏") } else { crate::settings::tr("收藏", "Add to favorites") })
                    .on_hover(set_sub(false))
                    .on_click(cx.listener(move |w, _, _, cx| w.toggle_star(&star, cx))),
            )
            .child(
                item("rm-rename", "square-pen", crate::settings::text("重命名"))
                    .on_hover(set_sub(false))
                    .on_click(cx.listener(move |w, _, window, cx| w.start_rename(&ren, window, cx))),
            )
            .child(
                item("rm-move", "folder-input", crate::settings::text("移动到"))
                    .on_hover(set_sub(false))
                    .on_click(cx.listener(move |w, _, window, cx| w.open_move(vec![mv.clone()], window, cx))),
            )
            .child(
                item("rm-copy", "copy", crate::settings::text("创建副本"))
                    .on_hover(set_sub(false))
                    .on_click(cx.listener(move |w, _, window, cx| w.duplicate_doc(&dup, window, cx))),
            )
            .child(
                item("rm-path", "copy", crate::settings::text("复制文件地址"))
                    .on_hover(set_sub(false))
                    .on_click(cx.listener(move |w, _, _, cx| {
                        w.row_menu = None;
                        if let Some(path) = w.store.file_path(&path) {
                            cx.write_to_clipboard(ClipboardItem::new_string(path.display().to_string()));
                            w.show_toast(crate::settings::text("已复制文件地址"), cx);
                        }
                    })),
            )
            .child(divider())
            .child(
                item("rm-download", "export", crate::settings::text("下载"))
                    .when(m.sub, |d| d.bg(Theme::fill_hover()))
                    .child(icon("chevron-right", 10., Theme::icon()))
                    .on_hover(set_sub(true)),
            )
            .map(|d| {
                #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                {
                    let target = m.id.clone();
                    d.when(share_row, |d| d.child(divider()).child(item("rm-share", "share", crate::settings::tr("共享", "Share"))
                        .on_hover(set_sub(false)).on_click(cx.listener(move |w, _, window, cx| {
                            w.row_menu = None;
                            w.create_shared_note(&target,window,cx);
                        }))))
                }
                #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
                { d }
            })
            .children({
                #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                {
                    let target = doc.to_owned();
                    self.object_link_available(cx).then(|| item("rm-copy-object-link", "copy", crate::settings::tr("复制链接", "Copy Link"))
                        .on_hover(set_sub(false))
                        .on_click(cx.listener(move |w, _, _, cx| w.copy_object_link(&target, None, !w.is_shared(), cx))))
                }
                #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
                { None::<Stateful<Div>> }
            })
            .child(divider())
            .child(
                item_c("rm-del", "bin", crate::settings::text("删除"), Theme::danger(), Theme::danger())
                    .on_hover(set_sub(false))
                    .on_click(cx.listener(move |w, _, _, cx| w.ask_delete(&del, cx))),
            );
        let vh = f32::from(window.viewport_size().height);
        let extra = usize::from(child);
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let extra = extra + usize::from(self.object_link_available(cx));
        let shared_rows = usize::from(share_row);
        let menu_h = row_menu_height(ROW_ITEMS + shared_rows + extra, ROW_DIVIDERS + shared_rows);
        // From the "…" button: hang 2px below it, or sit 2px above it when
        // there's no room below. From a right-click: at the pointer.
        let (origin, corner) = if m.pos.x < px(0.) {
            let (left, top) = (-m.pos.x, f32::from(m.pos.y));
            if top + 20. + 2. + menu_h > vh - 8. {
                (point(left, px(top - 2.)), Anchor::BottomLeft)
            } else {
                (point(left, px(top + 22.)), Anchor::TopLeft)
            }
        } else {
            (m.pos, m.corner)
        };
        let menu_top = match corner {
            Anchor::BottomLeft | Anchor::BottomRight => f32::from(origin.y) - menu_h,
            _ => f32::from(origin.y),
        };
        let mut out = vec![
            deferred(anchored().anchor(corner).position(origin).snap_to_window_with_margin(px(8.)).child(menu))
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
                        popover()
                            .id("rm-download-menu")
                            .occlude()
                            .min_w(px(130.))
                            .px(px(5.))
                            .py(px(crate::ui::MENU_PAD_Y))
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

// Sidebar actions target the personal library even while a shared note is open.
type PersonalAction = Box<dyn FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>)>;
impl Workspace {
    fn resolve_sidebar_entry(&self, personal: &[Entry], shared_rows: Option<&[Entry]>, pinned: &[Entry], cx: &App) -> Option<SidebarEntry> {
        let shared_tree = shared_rows.unwrap_or_default();
        let in_tree = |rows: &[Entry], expanded: &std::collections::HashSet<String>, id: &str| {
            crate::tree::Tree::new(rows).rows(|id| expanded.contains(id)).iter().any(|(row, _)| row.id == id)
        };
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let shared_expanded = &self.sharing.expanded;
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        let shared_expanded = &self.expanded;
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let scope = crate::cloud_sync::shared::scope(&self.store, cx);
        let id = match self.view {
            View::Doc => self.current.as_deref(),
            View::Folder => self.folder_current.as_deref(),
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            View::Home if self.is_shared() => self.sharing.sidebar_waiting.as_deref().or_else(|| scope.map(|scope| scope.note.as_str())),
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            View::Home if self.home.tab == crate::home::HomeTab::Shared => {
                let id = self.sharing.collection.as_deref()?;
                return in_tree(shared_tree, shared_expanded, id).then(|| SidebarEntry::new(SidebarSection::Shared, id));
            }
            _ => None,
        }?;
        let mut personal_item = !self.is_shared();
        let mut shared_item = false;
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if let Some(scope) = scope {
            if crate::cloud_sync::shared::access(&self.store, cx) == Some(crate::cloud_sync::shared::Access::Revoked) { return None; }
            shared_item = scope.note == id || (scope.folder && shared_tree.iter().any(|row|
                row.id == id && row.meta.share_root.as_ref() == Some(&scope.note)));
            personal_item = crate::cloud_sync::shared::original::owner_room(personal, id).is_some_and(|room| room.scope == *scope);
        }
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        let _ = (cx, &mut personal_item, &mut shared_item);
        let visible = |section| match section {
            SidebarSection::Personal => personal_item && !self.folded[1] && in_tree(personal, &self.expanded, id),
            SidebarSection::Shared => shared_item && in_tree(shared_tree, shared_expanded, id),
            SidebarSection::Pinned => (personal_item || shared_item) && !self.folded[0] && pinned.iter().any(|row| row.id == id),
            SidebarSection::Local => personal_item && !self.folded[2] && personal.iter().any(|row| row.id == id && row.meta.local()),
        };
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        if let Some(entry) = self.sidebar_entry.as_ref().filter(|entry| entry.id == id
            && matches!(entry.section, SidebarSection::Personal | SidebarSection::Shared)) {
            // Folding the originating tree hides its selection without selecting its duplicate.
            return visible(entry.section).then(|| entry.clone());
        }
        self.sidebar_entry.as_ref().filter(|entry| entry.id == id && visible(entry.section)).cloned()
            .or_else(|| [SidebarSection::Shared, SidebarSection::Personal, SidebarSection::Local, SidebarSection::Pinned]
                .into_iter().find(|section| visible(*section)).map(|section| SidebarEntry::new(section, id)))
    }

    pub(crate) fn sidebar_row_selected(&self, section: SidebarSection, id: &str) -> bool {
        self.sidebar_selection_row.as_ref().is_some_and(|entry| entry.section == section && entry.id == id)
    }

    pub(crate) fn personal_navigation_entries(&self, cx: &App) -> Vec<Entry> {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let entries = if self.is_shared() { cx.global::<crate::shared_store::SharedStore>().clone().list() } else { self.entries.clone() };
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        let entries = self.entries.clone();
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        let entries = {
            let mut entries = entries;
            self.project_owned_sidebar_folders(&mut entries, cx);
            entries
        };
        let _ = cx;
        entries
    }

    pub(crate) fn open_personal_sidebar_item(&mut self, id: &str, section: SidebarSection, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let origin = SidebarEntry::new(section, id);
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        {
            use crate::cloud_sync::shared::original;
            let personal;
            let entries = if self.is_shared() || cfg!(target_os = "macos") { personal = self.personal_navigation_entries(cx); &personal } else { &self.entries };
            if !entries.iter().any(|entry| entry.id == id) { return false; }
            if let Some(room) = original::owner_room(entries, id)
                && !original::awaiting_publication(&crate::store::Store::root_dir(), &room) {
                if !cfg!(target_os = "macos") && room.scope.folder && entries.iter().any(|entry| entry.id == id && entry.meta.folder.is_some()) {
                    self.expanded.insert(id.to_owned());
                }
                self.navigate_shared_sidebar(room, origin, window, cx);
                return true;
            }
            if self.is_shared() {
                self.navigate_personal_from(View::Doc, Some(id.to_owned()), Some(origin), window, cx);
                return true;
            }
        }
        if !self.try_open(id, window, cx) { return false; }
        self.sidebar_entry = Some(origin);
        cx.notify();
        true
    }

    pub(crate) fn shared_batch(&self) -> bool {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        { self.batch.is_some() && self.sharing.batch }
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        { false }
    }

    pub(crate) fn batch_contains_shared(&self, cx: &App) -> bool {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        { self.shared_batch() || self.batch.as_ref().is_some_and(|ids| ids.iter().any(|id| self.shared_list_room(id, cx).is_some())) }
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        { let _ = cx; false }
    }

    pub(crate) fn begin_personal_batch(&mut self) {
        self.batch = Some(vec![]);
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        { self.sharing.batch = false; }
    }

    pub(crate) fn personal_action(&mut self, window: &mut Window, cx: &mut Context<Self>, action: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static) {
        self.personal_action_boxed(window, cx, Box::new(action));
    }

    fn personal_action_boxed(&mut self, window: &mut Window, cx: &mut Context<Self>, action: PersonalAction) {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if self.is_shared() {
            if self.sharing_busy() { return; }
            let held = std::rc::Rc::new(std::cell::RefCell::new(Some(action))); let retry = held.clone();
            if !self.save_before(cx, move |w, window, cx| { if let Some(action) = retry.borrow_mut().take() { w.personal_action_boxed(window, cx, action); } }) { return; }
            let action = held.borrow_mut().take().unwrap();
            let window_id = window.window_handle().window_id();
            self.navigate_personal(View::Doc, None, window, cx);
            cx.defer(move |cx| {
                if let Some(handle) = cx.windows().into_iter().find(|handle| handle.window_id() == window_id).and_then(|handle| handle.downcast::<Workspace>()) {
                    let _ = handle.update(cx, |ws, window, cx| { if !ws.is_shared() { action(ws, window, cx); } });
                }
            });
            return;
        }
        action(self, window, cx);
    }
}

impl Workspace {
    fn batch_section(&self, shared: bool, ids: Vec<String>, cx: &mut Context<Self>) -> Div {
        let n = self.batch.as_ref().map_or(0, Vec::len);
        let all = !ids.is_empty() && self.batch.as_ref().is_some_and(|selected| ids.iter().all(|id| selected.contains(id)));
        let id = if shared { "shared-batch-all" } else { "batch-all" };
        let row = div().h(px(SECTION_H)).pl(px(6.)).pt(px(SECTION_TOP)).flex().items_center()
            .child(batch_check_slot().id(id).cursor_pointer().group(id)
                .child(checkbox(if all { Check::On } else if n > 0 { Check::Partial } else { Check::Off }, false, id))
                .on_click(cx.listener(move |w, _, _, cx| { w.batch = Some(if all { vec![] } else { ids.clone() }); cx.notify(); })))
            .child(div().flex_1().min_w_0().ml(px(6.)).text_size(px(theme::ui_font())).text_color(Theme::text()).child(crate::localized_format!("已选 {n} 项", "Selected: {n}")));
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let row = row.when(shared, |row| row.child(self.section_invitation_button(cx)));
        row.child(Self::section_trigger(if shared { "shared-batch-exit" } else { "batch-exit" }, "x").ml(px(if shared { 2. } else { 8. }))
            .tip(crate::settings::text("退出批量操作"), None)
            .on_click(cx.listener(|w, _, _, cx| { w.batch = None; w.row_menu = None; cx.notify(); })))
            .child(self.section_add_button(shared, cx))
    }
}
