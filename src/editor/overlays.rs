//! Floating UI over the page: block handle, slash / "+" menu, block menu,
//! text toolbar with its dropdowns, color panels, link editor, code
//! language picker and the callout emoji picker (design-spec §4).

use super::core::{Pos, TITLE};
use super::menus::{self, SLASH_ITEMS, TURN_ITEMS};
use super::table::CellSelKind;
use super::view::{BlockAct, BlockSub, CtxKind, Editor, Mark, Menu};
use crate::model::*;
use crate::feishu_theme::{self as theme, Theme};
use crate::editor_ui::{TipExt, icon};
use crate::design::{Button, ButtonSize, Field, FieldSize, MenuItem, MenuLabel, MenuSeparator, Popover, Textarea, color};
use crate::design::menu::{LABEL_H, PAD, ROW_H, SEP_H};
use crate::design::toolbar::{TOOLBAR_H, TOOLBAR_PAD, toolbar_layout};
use crate::design::{ButtonGroup, ButtonGroupSeparator, Toggle, Toolbar};
use gpui::{prelude::FluentBuilder, *};

const SLASH_W: f32 = 247.;
const SLASH_MAX_H: f32 = 351.;
pub(super) const MENU_W: f32 = 247.;
/// Table handle: 44.5 left of the grid, centred on the column-dot row 16.5 above it.
const TABLE_HANDLE_DX: f32 = 44.5;
const TABLE_HANDLE_DY: f32 = 16.5 + 13.;
/// The selection toolbar's items (design::toolbar): T▾, |, ≡▾, |, six marks and 转为公式, A▾, |, 更多; Markdown
/// documents drop ≡▾ and A▾ (and the rule after ≡▾).
fn toolbar_widths(md: bool) -> Vec<f32> {
    use crate::design::toolbar::w;
    let mut v = vec![w::DROPDOWN, w::SEPARATOR];
    if !md { v.extend([w::DROPDOWN, w::SEPARATOR]) }
    v.extend([w::TOGGLE; 7]);
    if !md { v.push(w::SPLIT) }
    v.extend([w::SEPARATOR, w::BUTTON]);
    v
}
/// Callout hover bar: a toolbar with the palette button; it floats 9px
/// above the callout, centred on it.
const CALLOUT_BAR_W: f32 = TOOLBAR_PAD + crate::design::toolbar::w::BUTTON + TOOLBAR_PAD;
const CALLOUT_BAR_H: f32 = TOOLBAR_H;
const CALLOUT_PANEL_W: f32 = 230.;

/// Size picker: 24px cells 6 apart in 12px insets, inside a 1px box with 12px
/// margins, capped at `max-width: 312px` (cells then shrink).
fn picker_width(cols: usize) -> f32 {
    (12. + cols as f32 * 24. + (cols - 1) as f32 * 6. + 12.).min(312.)
}

/// Header 12 + 20 + 12, the grid (no box round it), 12 below.
fn picker_height(kind: menus::SlashKind) -> f32 {
    let rows = if kind == menus::SlashKind::Table { 9. * 24. + 8. * 6. } else { 80. };
    44. + rows + 12.
}

/// Place `el` at `rel` (content-column coordinates), above everything else.
pub(super) fn at(rel: Point<Pixels>, corner: Anchor, el: impl IntoElement) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .child(
            deferred(
                anchored()
                    .position_mode(AnchoredPositionMode::Local)
                    .position(rel)
                    .anchor(corner)
                    .snap_to_window_with_margin(px(8.))
                    .child(el),
            )
            .with_priority(1),
        )
        .into_any_element()
}

pub fn kind_icon(k: &BlockKind) -> &'static str {
    match k {
        BlockKind::Text => "type",
        BlockKind::Heading { level, .. } => match level {
            1 => "heading-1",
            2 => "heading-2",
            3 => "heading-3",
            4 => "heading-4",
            5 => "heading-5",
            6 => "heading-6",
            _ => "heading-6",
        },
        BlockKind::Bullet => "list",
        BlockKind::Ordered { .. } => "list-ordered",
        BlockKind::Todo { .. } => "todo",
        BlockKind::Code { .. } => "braces",
        BlockKind::Quote => "quote",
        BlockKind::Callout { .. } => "callout",
        BlockKind::Divider => "divider",
        BlockKind::Image { .. } => "image",
        BlockKind::Table(_) => "table",
        BlockKind::Grid | BlockKind::Column { .. } => "columns-2",
        BlockKind::Raw { .. } => "file-text",
        BlockKind::SubPageList => "list-tree",
        BlockKind::Countdown(_) => "timer",
        BlockKind::Board(_) => "board",
        BlockKind::DataTable(_) => "doc-table",
        BlockKind::File(_) => "paperclip",
    }
}

/// Handle icon tint, same per-family colours as the slash menu.
pub fn kind_color(k: &BlockKind) -> Hsla {
    theme::c(match k {
        BlockKind::Bullet | BlockKind::Ordered { .. } | BlockKind::Todo { .. } | BlockKind::Grid | BlockKind::Column { .. } => menus::INDIGO,
        BlockKind::Code { .. } => menus::GREEN,
        BlockKind::Divider | BlockKind::Callout { .. } => menus::ORANGE,
        BlockKind::Image { .. } => menus::YELLOW,
        BlockKind::Table(_) | BlockKind::DataTable(_) => menus::TEAL,
        BlockKind::Raw { .. } => menus::NEUTRAL,
        BlockKind::Countdown(_) => menus::ORANGE,
        BlockKind::Board(_) => menus::GREEN,
        BlockKind::File(_) => menus::WATHET,
        _ => menus::BLUE,
    })
}

/// A menu row: the editor's glyph (most aren't in the shell's set), the
/// label, a chevron when it opens a submenu (lit while that is open).
pub(super) fn row(id: impl Into<ElementId>, icon_name: &str, label: impl Into<SharedString>, sub: bool, open: bool) -> MenuItem {
    MenuItem::new(id).editor_icon(icon_name.to_string()).label(crate::settings::label(label.into())).submenu(sub).open(open)
}

/// A row that wouldn't apply: greyed, no hover.
pub(super) fn row_off(id: impl Into<ElementId>, icon_name: &str, label: impl Into<SharedString>) -> MenuItem {
    MenuItem::new(id).editor_icon(icon_name.to_string()).label(crate::settings::label(label.into())).disabled(true)
}

/// A 24pt tool with an 18pt editor glyph: the menus' icon grids (the
/// toolbars use design::toolbar). The glyph is `foreground` (shadcn's
/// ghost and Toggle glyphs take the text's colour); on shows only in the
/// fill: they sit on a popover, whose ground hides ghost's `muted` pressed
/// fill in the dark, so a pressed or keyboard-picked tool takes `accent`.
fn tool(id: impl Into<ElementId>, ic: &str, on: bool) -> Button {
    Button::ghost(id).sized(ButtonSize::Xs).selected(on).when(on, |d| d.bg(color::accent()))
        .child(icon(ic, 18., color::foreground()))
}

/// Color-panel swatch: 26 outer with a 2px ring (blue when selected), 22 inner.
/// Font swatches and the "none" swatch carry a hairline; filled ones don't.
fn swatch(id: impl Into<ElementId>, sel: bool, hairline: bool, inner: Div) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(26.))
        .rounded(px(4.))
        .border_2()
        .border_color(if sel { Theme::primary_500() } else { transparent_black() })
        .cursor_pointer()
        .child(inner.size(px(22.)).rounded(px(2.)).when(hairline && !sel, |d| d.border_1().border_color(Theme::swatch_border())))
}

fn font_swatch(hex: u32) -> Div {
    div().flex().items_center().justify_center().text_size(px(14.)).text_color(theme::c(hex)).child("A")
}

/// White square crossed by a grey diagonal: "no color".
fn none_swatch() -> Div {
    div().flex().items_center().justify_center().bg(crate::theme::raised(Theme::bg_body())).child(icon("swatch-none", 20., Theme::block_placeholder()))
}

fn color_title(t: &'static str) -> Div {
    div().mt(px(7.)).mb(px(4.)).text_size(px(12.)).line_height(px(20.)).text_color(Theme::text()).child(crate::settings::text(t))
}

/// 在下方添加 ›: hovering opens the insert panel next to the handle menu.
fn add_below_row(open: bool, cx: &Context<Editor>) -> MenuItem {
    row("bm-add", "square-plus", "在下方添加", true, open)
        .on_mouse_move(cx.listener(|e: &mut Editor, _: &MouseMoveEvent, _, cx| {
            if let Some(Menu::Block { sub, .. }) = &mut e.menu
                && !matches!(sub, Some(BlockSub::Add(_)))
            {
                *sub = Some(BlockSub::Add(usize::MAX));
                e.size_pick = (0, 0);
                e.slash_scroll.set_offset(Point::default());
                cx.notify();
            }
        }))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

/// Split the basic grid into rows of 6, a plain 6-column grid, so items
/// just flow with no forced breaks.
fn grid_rows<T>(cells: Vec<(menus::SlashKind, T)>) -> Vec<Vec<T>> {
    let mut rows: Vec<Vec<T>> = vec![];
    for (_, c) in cells {
        match rows.last_mut() {
            Some(r) if r.len() < menus::GRID_COLS => r.push(c),
            _ => rows.push(vec![c]),
        }
    }
    rows
}

/// A menu separator that can take the pointer (leaving a submenu's row).
pub(super) fn separator_h() -> Div {
    div().flex_none().child(MenuSeparator::new())
}

fn group_title(t: &str) -> MenuLabel {
    MenuLabel::new(crate::settings::label(t))
}

/// A layout lookup an overlay depends on; re-checked after paint so an
/// overlay placed from a stale frame is redrawn at the fresh position.
#[derive(Clone, Copy, PartialEq)]
pub enum Probe {
    Point(Pos),
    Block(BlockId),
    /// A table bar segment: (table, column bar?, index).
    Bar(BlockId, bool, usize),
    /// An image's box.
    Image(BlockId),
    File(BlockId),
    /// A 按钮's box.
    Button(Pos),
}

impl Editor {
    pub(super) fn probe(&self, p: Probe) -> Option<Bounds<Pixels>> {
        match p {
            Probe::Point(pos) => {
                let cache = self.cache.borrow();
                let l = cache.get(&pos.block)?;
                Some(Bounds::new(l.position_for(pos.offset), size(px(0.), l.line_height)))
            }
            Probe::Block(id) => self.block_bounds.borrow().get(&id).copied(),
            Probe::Bar(t, cols, i) => self.tables.geo.borrow().get(&(t, cols, i)).copied(),
            Probe::Image(id) => self.img.boxes.borrow().get(&id).copied(),
            Probe::File(id) => self.file.boxes.borrow().get(&id).copied(),
            Probe::Button(p) => self.button_rect(p),
        }
    }

    pub(super) fn probed(&self, p: Probe) -> Option<Bounds<Pixels>> {
        let v = self.probe(p);
        self.probes.borrow_mut().push((p, v));
        v
    }

    pub(super) fn rel(&self, p: Point<Pixels>) -> Point<Pixels> {
        let o = self.content_origin.get();
        point(p.x - o.x, p.y - o.y)
    }

    /// Caret top-left and line height for a position (last frame).
    pub(super) fn point_of(&self, p: Pos) -> Option<(Point<Pixels>, Pixels)> {
        self.probed(Probe::Point(p)).map(|b| (b.origin, b.size.height))
    }

    pub(super) fn block_rect(&self, id: BlockId) -> Option<Bounds<Pixels>> {
        self.probed(Probe::Block(id))
    }

    /// Handle bounds (window coords) for a block: centered on its first line.
    pub(super) fn handle_rect(&self, id: BlockId, add: bool) -> Option<Bounds<Pixels>> {
        let b = self.block_rect(id)?;
        let (top, lh) = {
            let first = self.probed(Probe::Point(Pos::new(id, 0)));
            match (self.core.kind_of(id), first) {
                (Some(BlockKind::Code { .. }), _) => (b.top() + px(3.), px(26.)),
                (_, Some(l)) => (l.top(), l.size.height),
                _ => (b.top(), px(26.).min(b.size.height)),
            }
        };
        let w = if add { px(28.) } else { px(42.) };
        if matches!(self.core.kind_of(id), Some(BlockKind::Table(_))) {
            return Some(Bounds::new(point(b.left() - px(TABLE_HANDLE_DX), b.top() - px(TABLE_HANDLE_DY)), size(w, px(26.))));
        }
        Some(Bounds::new(point(b.left() + px(self.handle_dx(id, add)), top + (lh - px(26.)) / 2.), size(w, px(26.))))
    }

    /// The handle's left edge relative to the block (tables aside).
    fn handle_dx(&self, id: BlockId, add: bool) -> f32 {
        let mut dx = if add { -34. } else { -48. };
        // Foldable blocks leave room for the fold arrow between handle and text.
        if self.core.can_fold(id) {
            dx -= 14.;
        }
        // A quote's own children put theirs where the quote's would be.
        if matches!(self.core.parent_of(id).map(|p| &p.kind), Some(BlockKind::Quote)) {
            dx -= 14.;
        }
        dx
    }

    fn is_empty_line(&self, id: BlockId) -> bool {
        self.core.block(id).is_some_and(|b| matches!(b.kind, BlockKind::Text) && b.text.is_empty() && b.children.is_empty())
    }

    pub(super) fn render_overlays(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        self.probes.borrow_mut().clear();
        self.callout_zone.borrow_mut().clear();
        let mut out = vec![];
        if self.read_only {
            // Only the right-click menu's 复制 / 全选.
            if let Some(Menu::Ctx { pos, kind: kind @ CtxKind::ReadOnly { .. }, sub }) = self.menu.clone() {
                out.extend(self.ctx_menu(pos, kind, sub, cx));
            }
            return out;
        }
        let menu_el = match self.menu.clone() {
            Some(Menu::Ctx { pos, kind, sub }) => self.ctx_menu(pos, kind, sub, cx),
            Some(Menu::OrderedNumber { block, pos, edit, depth }) => self.ordered_number_menu(block, pos, edit, depth, cx),
            Some(Menu::Block { block, sub }) => self.block_menu(block, sub, cx),
            Some(Menu::Link { block, range, only_link }) => self.link_panel(block, range, only_link, window, cx),
            Some(Menu::Equation { block, offset }) => self.equation_editor(block, offset, window, cx),
            Some(Menu::Lang { block, selected }) => self.lang_menu(block, selected, window, cx),
            Some(Menu::Emoji { target, selected }) => self.emoji_picker(target, selected, window, cx),
            Some(Menu::CoverEdit) => self.cover_menu(cx),
            Some(Menu::CoverPick { upload }) => self.cover_panel(upload, window, cx),
            Some(Menu::Button { block, offset }) => self.button_panel(block, offset, window, cx),
            Some(Menu::Reminder { block, offset }) => self.reminder_popover(block, offset, window, cx),
            _ => None,
        };
        out.extend(menu_el);
        // After the handle menu: 在下方添加's panel sits on top of it.
        out.extend(self.slash_menu(window, cx));
        out.extend(self.mention_panel(window, cx));
        out.extend(self.callout_bar(cx));
        if let Some(t) = self.toolbar(window, cx) {
            out.push(t);
        }
        out.extend(self.table_toolbar(window, cx));
        out.extend(self.image_toolbar(window, cx));
        out.extend(self.file_toolbar(window, cx));
        out.extend(self.link_card(window, cx));
        out.extend(self.button_layers(window, cx));
        out.extend(self.reminder_layers(window, cx));
        out.extend(self.countdown_overlays(window, cx));
        out
    }

    // ---- handle

    /// The block that shows the handle this frame.
    pub(super) fn handle_block(&self) -> Option<BlockId> {
        if self.read_only || self.dragging || self.slash.is_some() || self.block_drag.as_ref().is_some_and(|d| d.started) {
            return None;
        }
        match &self.menu {
            // A right-click menu brings no handle: the pointer is not there
            // to drag anything.
            Some(Menu::Ctx { .. }) => None,
            Some(Menu::Block { block, .. }) | Some(Menu::Add { block, .. }) => Some(*block),
            _ => self.hover.filter(|_| !self.hover_clipped && !self.typing),
        }
    }

    /// The handle, laid out together with its block (not from last frame's
    /// bounds) so it never lags behind when the block changes height.
    /// `first_line` is the height of the block's first line box.
    pub(super) fn handle(&self, id: BlockId, first_line: f32, cx: &mut Context<Self>) -> Option<AnyElement> {
        let kind = self.core.kind_of(id)?.clone();
        let add = self.is_empty_line(id);
        let (w, left) = match (add, &kind) {
            (false, BlockKind::Table(_)) => (42., -TABLE_HANDLE_DX),
            _ => (if add { 28. } else { 42. }, self.handle_dx(id, add)),
        };
        // Tables put the handle on the row of column dots above the grid.
        let top = if matches!(kind, BlockKind::Table(_)) { -TABLE_HANDLE_DY } else { (first_line - 26.) / 2. };
        let active = matches!(&self.menu, Some(Menu::Block { block, .. }) | Some(Menu::Add { block, .. }) if *block == id);
        // design::toolbar::block_handle: an outline ButtonGroup
        let glyph = (!add).then(|| icon(kind_icon(&kind), 12., kind_color(&kind)));
        let el = crate::design::toolbar::block_handle(id.0, glyph, w, active)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |e, ev: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    if !add {
                        e.press_handle(id, ev.position);
                    }
                    let open = match &e.menu {
                        Some(Menu::Block { block, .. }) | Some(Menu::Add { block, .. }) => *block == id,
                        _ => false,
                    };
                    if open {
                        e.menu = None;
                        e.core.clear_block_sel();
                        cx.notify();
                    } else {
                        e.open_block_menu(id, add, window, cx);
                    }
                }),
            );
        Some(at(point(px(left), px(top)), Anchor::TopLeft, el))
    }

    /// The handle as the drag card carries it, white as the card, under the pointer
    /// that grabbed it (the closed hand stays over its open-hand segments).
    pub(super) fn handle_face(&self, id: BlockId) -> Option<AnyElement> {
        let kind = self.core.kind_of(id)?;
        let glyph = icon(kind_icon(kind), 12., kind_color(kind));
        let face = crate::design::toolbar::block_handle(id.0, Some(glyph), 42., false);
        Some(div().relative().child(face).child(div().absolute().inset_0().cursor(CursorStyle::ClosedHand)).into_any_element())
    }

    // ---- slash / add menu

    /// The menu, plus the size picker when one is open.
    fn slash_menu(&self, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out = vec![];
        self.slash_menu_into(window, cx, &mut out);
        out
    }

    fn slash_menu_into(&self, window: &Window, cx: &mut Context<Self>, out: &mut Vec<AnyElement>) -> Option<()> {
        // `beside`: the handle menu's 在下方添加 row (its top-left in window
        // coordinates); the panel opens right next to the menu.
        let (anchor_top, anchor_bottom, selected, beside) = match (&self.slash, &self.menu) {
            (Some(s), _) => {
                let (p, lh) = self.point_of(Pos::new(s.block, s.trigger))?;
                (p, point(p.x, p.y + lh), s.selected, None)
            }
            (None, Some(Menu::Add { block, selected })) => {
                let r = self.handle_rect(*block, true)?;
                (point(r.right() + px(4.), r.top()), point(r.right() + px(4.), r.top()), *selected, None)
            }
            (None, Some(Menu::Block { block, sub: Some(BlockSub::Add(selected)) })) => {
                let r = self.handle_rect(*block, false)?;
                let p = point(r.left() + px(MENU_W), r.bottom() + px(4. + self.block_menu_add_y(*block)));
                (p, p, *selected, Some(p))
            }
            _ => return None,
        };
        let results = self.slash_results();
        let grid_basic = self.slash_grid();
        let mut list = div()
            .id("slash-list")
            .w(px(SLASH_W))
            .max_h(px(SLASH_MAX_H))
            .overflow_y_scroll()
            .track_scroll(&self.slash_scroll)
            .py(px(PAD))
            .flex()
            .flex_col();
        let hover_sel = |n: usize| {
            cx.listener(move |e: &mut Editor, _: &MouseMoveEvent, _, cx| {
                let cur = match (&mut e.slash, &mut e.menu) {
                    (Some(s), _) => &mut s.selected,
                    (None, Some(Menu::Add { selected, .. })) => selected,
                    (None, Some(Menu::Block { sub: Some(BlockSub::Add(selected)), .. })) => selected,
                    _ => return,
                };
                if *cur != n {
                    *cur = n;
                    e.size_pick = (0, 0);
                    cx.notify();
                }
            })
        };
        // In the "+" panel 表格 and 分栏 open a size picker instead of inserting.
        let add_panel = self.slash.is_none();
        // 按钮 opens its type menu in both.
        let has_picker = |k: menus::SlashKind| {
            k == menus::SlashKind::Button || add_panel && matches!(k, menus::SlashKind::Table | menus::SlashKind::Grid)
        };
        let mut picker_at: Option<(menus::SlashKind, f32)> = None;
        let run = |kind: menus::SlashKind| {
            cx.listener(move |e: &mut Editor, _: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                e.run_slash(kind, window, cx);
            })
        };
        let mut last_group = usize::MAX;
        let mut content_h: f32 = 2. * PAD;
        // Basic group as one 6-per-row grid of 24px icon buttons
        // (spread across the 214px row, 12px between rows, 8px under the grid).
        let mut grid: Option<Vec<(menus::SlashKind, AnyElement)>> = None;
        // Rows hold 6 and 分隔线 always starts a new one; short rows are padded
        // so every row keeps the same columns.
        let grid_el = |cells: Vec<(menus::SlashKind, AnyElement)>| {
            let mut rows = div().flex_none().mb(px(8.)).px(px(PAD + 4.)).flex().flex_col().gap(px(12.));
            for r in grid_rows(cells) {
                let pad = 6 - r.len();
                rows = rows.child(div().flex().justify_between().children(r).children((0..pad).map(|_| div().size(px(24.)))));
            }
            rows
        };
        let grid_h = |kinds: &[menus::SlashKind]| {
            let n = grid_rows(kinds.iter().map(|k| (*k, ())).collect()).len() as f32;
            24. * n + 12. * (n - 1.).max(0.) + 8.
        };
        for (n, i) in results.iter().enumerate() {
            let it = &SLASH_ITEMS[*i];
            if it.group != last_group {
                last_group = it.group;
                if let Some(g) = grid.take() {
                    list = list.child(grid_el(g));
                }
                list = list.child(group_title(menus::GROUPS[it.group].title));
                content_h += LABEL_H;
                if grid_basic && it.group == 0 {
                    grid = Some(vec![]);
                    let kinds: Vec<menus::SlashKind> =
                        results.iter().map(|i| &SLASH_ITEMS[*i]).filter(|it| it.group == 0).map(|it| it.kind).collect();
                    content_h += grid_h(&kinds);
                }
            }
            let kind = it.kind;
            if let Some(g) = &mut grid {
                let (keys, md) = menus::basic_tip(kind);
                g.push((
                    kind,
                    tool(("slash", n), it.icon, n == selected)
                        .tip_md(it.label, keys, md)
                        .on_mouse_move(hover_sel(n))
                        .on_mouse_down(MouseButton::Left, run(kind))
                        .into_any_element(),
                ));
                continue;
            }
            let sub = has_picker(kind);
            if sub && n == selected {
                picker_at = Some((kind, content_h - PAD));
            }
            content_h += ROW_H;
            list = list.child(
                MenuItem::new(("slash", n))
                    .leading(icon(it.icon, 16., crate::feishu_theme::c(it.color)))
                    .label(crate::settings::label(it.label))
                    .submenu(sub)
                    .selected(n == selected)
                    .open(sub && n == selected)
                    .on_mouse_move(hover_sel(n))
                    .when(!sub, |d| d.on_mouse_down(MouseButton::Left, run(kind)))
                    .when(sub, |d| d.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())),
            );
        }
        if let Some(g) = grid.take() {
            list = list.child(grid_el(g));
        }
        if results.is_empty() {
            content_h += ROW_H;
            list = list.child(
                div().flex_none().h(px(ROW_H)).px(px(PAD + 8.)).flex().items_center().text_color(color::muted_foreground()).child(crate::settings::text("未能找到相关结果")),
            );
        }
        let view_h = content_h.min(SLASH_MAX_H);
        let vh = f32::from(window.viewport_size().height);
        let (pos, corner) = if let Some(p) = beside {
            // The panel's padding lines up with the row's top, inside the window.
            let top = (f32::from(p.y) - PAD).min(vh - 8. - view_h).max(8.);
            (point(p.x, px(top)), Anchor::TopLeft)
        } else if vh - f32::from(anchor_bottom.y) < 240. {
            (anchor_top, Anchor::BottomLeft)
        } else {
            // The menu hangs right off the line box, no gap.
            (anchor_bottom, Anchor::TopLeft)
        };
        // The shared scrollbar (shadcn's ScrollArea): shows while the pointer is over the menu or it scrolls.
        let thumb = self.slash_bar.render(&self.slash_scroll);
        let el = Popover::new("slash-menu")
            .relative()
            .w(px(SLASH_W))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
            .child(list)
            .child(thumb);
        out.push(at(self.rel(pos), corner, el));
        let (kind, row_top) = picker_at?;
        // The picker hangs 4pt off the menu's right edge (as submenus do), centred on its row and
        // kept inside the window.
        let (picker, ph) = if kind == menus::SlashKind::Button {
            (self.button_type_menu(cx).into_any_element(), super::button::TYPE_MENU_H)
        } else {
            (self.size_picker(kind, cx).into_any_element(), picker_height(kind))
        };
        let menu_top = if corner == Anchor::TopLeft { pos.y } else { pos.y - px(view_h) };
        let off = -f32::from(self.slash_scroll.offset().y);
        let top = (f32::from(menu_top) + row_top - off + ROW_H / 2. - ph / 2.).min(vh - 8. - ph).max(8.);
        let left = pos.x + px(SLASH_W + 4.);
        out.push(at(self.rel(point(left, px(top))), Anchor::TopLeft, picker));
        Some(())
    }

    /// The insert-size picker: 9×9 for 表格, 1×5 for 分栏.
    /// Hovering a cell picks rows × cols; leaving the grid resets to 0 × 0;
    /// a click inserts unless the pick is within the default counts.
    fn size_picker(&self, kind: menus::SlashKind, cx: &mut Context<Self>) -> Popover {
        let table = kind == menus::SlashKind::Table;
        let (title, rows, cols, def) = if table { (crate::settings::text("插入支持富文本的表格"), 9, 9, (0, 0)) } else { (crate::settings::text("选择栏数"), 1, 5, (1, 1)) };
        let (r, c) = self.size_pick;
        let disabled = r <= def.0 && c <= def.1;
        let cell_h = if rows == 1 { 80. } else { 24. };
        let mut grid = div()
            .id("size-grid")
            .mx(px(12.))
            .mb(px(12.))
            .cursor_pointer()
            .on_hover(cx.listener(|e, hovered: &bool, _, cx| {
                if !*hovered && e.size_pick != (0, 0) {
                    e.size_pick = (0, 0);
                    cx.notify();
                }
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |e, _, window, cx| {
                let (r, c) = e.size_pick;
                if r > def.0 || c > def.1 {
                    e.run_slash_sized(kind, Some((r, c)), window, cx);
                }
            }));
        for i in 0..rows {
            let mut line = div().flex().h(px(cell_h)).when(i > 0, |d| d.mt(px(6.)));
            for j in 0..cols {
                let (ri, cj) = (i + 1, j + 1);
                let (bg, border) = if ri <= def.0 && cj <= def.1 {
                    (Theme::size_cell_disabled(), Theme::size_cell_disabled())
                } else if !disabled && ri <= r && cj <= c {
                    (Theme::size_cell_active(), Theme::size_cell_active())
                } else {
                    (Theme::size_cell(), Theme::size_cell_border())
                };
                line = line.child(
                    div()
                        .id(("size-cell", i * cols + j))
                        .flex_1()
                        .h(px(cell_h))
                        .when(j + 1 < cols, |d| d.mr(px(6.)))
                        .bg(bg)
                        .border_2()
                        .border_color(border)
                        .when(ri <= def.0 && cj <= def.1, |d| d.cursor(CursorStyle::OperationNotAllowed))
                        .on_mouse_move(cx.listener(move |e, _: &MouseMoveEvent, _, cx| {
                            if e.size_pick != (ri, cj) {
                                e.size_pick = (ri, cj);
                                cx.notify();
                            }
                        })),
                );
            }
            grid = grid.child(line);
        }
        let count = if table { format!("{c} x {r}") } else { c.to_string() };
        Popover::new("size-picker")
            .w(px(picker_width(cols)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .my(px(12.))
                    .px(px(12.))
                    .h(px(20.))
                    .flex()
                    .justify_between()
                    .text_size(px(12.))
                    .line_height(px(20.))
                    .text_color(color::muted_foreground())
                    .child(title)
                    .child(div().flex_none().w(px(36.)).ml(px(8.)).text_color(color::foreground()).when(disabled, |d| d.opacity(0.)).child(count)),
            )
            .child(grid)
    }

    // ---- block menu

    fn block_menu(&self, block: BlockId, sub: Option<BlockSub>, cx: &mut Context<Self>) -> Option<AnyElement> {
        let r = self.handle_rect(block, false)?;
        let b = self.core.block(block)?;
        let kind = b.kind.clone();
        if let BlockKind::Table(t) = &kind {
            return self.table_block_menu(block, (t.header_rows > 0, t.header_col), sub, r, cx);
        }
        let shape = menus::MenuShape::of(&kind);
        let act = |a: BlockAct| {
            cx.listener(move |e: &mut Editor, _: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                e.block_menu_action(block, a.clone(), window, cx);
            })
        };
        let set_sub = |s: Option<BlockSub>| {
            cx.listener(move |e: &mut Editor, _: &MouseMoveEvent, _, cx| {
                if let Some(Menu::Block { sub, .. }) = &mut e.menu
                    && *sub != s
                {
                    *sub = s;
                    cx.notify();
                }
            })
        };
        let mut m = Popover::new("block-menu").menu().w(px(MENU_W));
        let mut turn = shape.turn_items();
        if self.markdown { turn.retain(|t| !matches!((t.kind)(), BlockKind::Callout { .. })); }
        if !turn.is_empty() {
            // A callout's grid shows what its content is, next to 高亮块 itself.
            let current = |k: &BlockKind| match &kind {
                BlockKind::Callout { .. } if !matches!(k, BlockKind::Callout { .. }) => {
                    !b.children.is_empty() && b.children.iter().all(|c| menus::same_kind(&c.kind, k))
                }
                _ => menus::same_kind(&kind, k),
            };
            let mut grid = div().flex().flex_wrap().gap_x(px(12.5)).gap_y(px(12.)).px(px(12.)).py(px(4.));
            for (i, it) in turn.iter().enumerate() {
                let k = (it.kind)();
                let active = current(&k);
                let (tip, keys, md) = (it.tip, it.keys.map(|k| crate::shortcuts::hint(it.action, k)), it.md);
                grid = grid.child(
                    tool(("turn", i), it.icon, active)
                        .tip_md(tip, keys, md)
                        .on_mouse_down(MouseButton::Left, act(BlockAct::Turn(k))),
                );
            }
            m = m.child(grid).child(separator_h().on_mouse_move(set_sub(None)));
        }
        // Grid: 2 rows of 24 + 12 gap + 4+4 padding, then a separator.
        let indent_y = PAD + if turn.is_empty() { 0. } else { 4. + 24. * 2. + 12. + 4. + SEP_H };
        // Text blocks get 缩进和对齐 and 颜色; the rest only indent.
        let (indent_icon, indent_label) = if shape == menus::MenuShape::Text && !self.markdown { ("indent-align", crate::settings::text("缩进和对齐")) } else { ("indent-increase", crate::settings::text("缩进")) };
        m = m.child(
            row("bm-indent", indent_icon, indent_label, true, sub == Some(BlockSub::Indent))
                .on_mouse_move(set_sub(Some(BlockSub::Indent))),
        );
        let colorable = shape == menus::MenuShape::Text && !self.markdown;
        if colorable {
            m = m.child(row("bm-color", "palette", crate::settings::text("颜色"), true, sub == Some(BlockSub::Color)).on_mouse_move(set_sub(Some(BlockSub::Color))));
        }
        let adding = matches!(sub, Some(BlockSub::Add(_)));
        m = m
            .child(separator_h().on_mouse_move(set_sub(None)))
            .child(row("bm-cut", "scissors", crate::settings::text("剪切"), false, false).on_mouse_move(set_sub(None)).on_mouse_down(MouseButton::Left, act(BlockAct::Cut)))
            .child(row("bm-copy", "copy", crate::settings::text("复制"), false, false).on_mouse_move(set_sub(None)).on_mouse_down(MouseButton::Left, act(BlockAct::Copy)))
            .child(row("bm-del", "trash-2", crate::settings::text("删除"), false, false).on_mouse_move(set_sub(None)).on_mouse_down(MouseButton::Left, act(BlockAct::Delete)))
            .child(separator_h().on_mouse_move(set_sub(None)))
            .child(add_below_row(adding, cx));
        let color_y = indent_y + ROW_H;
        let sub_el: Option<AnyElement> = match sub {
            Some(BlockSub::Indent) => {
                // Order: the three alignments, a divider, then indent /
                // outdent. Shortcuts show as tooltips on the right; indent items
                // are greyed out when they wouldn't apply.
                let align = b.align;
                let can_in = self.core.can_indent(block);
                let can_out = self.core.can_outdent(block);
                let align_row = |id: &'static str, ic: &'static str, label: &'static str, keys: &'static str, a: Align| {
                    row(id, ic, label, false, false)
                        .checked(align == a)
                        .tip_right(label, Some(keys))
                        .on_mouse_down(MouseButton::Left, act(BlockAct::Align(a)))
                };
                let indent_row = |id: &'static str, ic: &'static str, label: &'static str, keys: Option<&'static str>, enabled: bool, a: BlockAct| {
                    if enabled {
                        let r = row(id, ic, label, false, false).on_mouse_down(MouseButton::Left, act(a));
                        match keys {
                            Some(k) => r.tip_right(label, Some(k)),
                            None => r,
                        }
                    } else {
                        row_off(id, ic, label).on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    }
                };
                Some(
                    Popover::new("bm-indent-sub")
                        .menu()
                        .absolute()
                        .left(px(MENU_W + 4.))
                        .top(px(indent_y - PAD))
                        .w(px(MENU_W))
                        .when(shape == menus::MenuShape::Text && !self.markdown, |d| {
                            d.child(align_row("al-l", "align-left", crate::settings::text("左对齐"), crate::shortcuts::hint("editor::AlignLeft", "⌘ + Shift + L"), Align::Left))
                                .child(align_row("al-c", "align-center", crate::settings::text("居中对齐"), crate::shortcuts::hint("editor::AlignCenter", "⌘ + Shift + E"), Align::Center))
                                .child(align_row("al-r", "align-right", crate::settings::text("右对齐"), crate::shortcuts::hint("editor::AlignRight", "⌘ + Shift + R"), Align::Right))
                                .child(separator_h())
                        })
                        .child(indent_row("in-inc", "indent-increase", crate::settings::text("增加缩进"), None, can_in, BlockAct::Indent))
                        .child(indent_row("in-dec", "indent-decrease", crate::settings::text("减少缩进"), Some("Shift + Tab"), can_out, BlockAct::Outdent))
                        .into_any_element(),
                )
            }
            Some(BlockSub::Color) if colorable => {
                let p = if matches!(kind, BlockKind::Callout { .. }) {
                    self.callout_panel(block, cx).into_any_element()
                } else {
                    self.color_panel(Some(block), cx).into_any_element()
                };
                Some(div().absolute().left(px(MENU_W + 4.)).top(px(color_y - PAD)).child(p).into_any_element())
            }
            _ => None,
        };
        let el = div()
            .relative()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(m)
            .children(sub_el);
        Some(at(self.rel(point(r.left(), r.bottom() + px(4.))), Anchor::TopLeft, el))
    }

    /// Top of the 在下方添加 row inside the handle menu (mirrors its layout).
    fn block_menu_add_y(&self, block: BlockId) -> f32 {
        let Some(b) = self.core.block(block) else { return 0. };
        // 剪切 / 复制 / 删除 sit between two separators.
        let tail = SEP_H + ROW_H * 3. + SEP_H;
        if matches!(b.kind, BlockKind::Table(_)) {
            return PAD + ROW_H + tail + ROW_H * 3. + SEP_H;
        }
        let shape = menus::MenuShape::of(&b.kind);
        let grid = if shape.turn_items().is_empty() { 0. } else { 4. + 24. * 2. + 12. + 4. + SEP_H };
        let color = if shape == menus::MenuShape::Text { ROW_H } else { 0. };
        PAD + grid + ROW_H + color + tail
    }

    // ---- color panels

    /// Text / background color panel (230×202) for the selection or selected
    /// blocks. From the handle menu (`block`) the background row is the
    /// block palette instead: pale swatches that fill the whole block.
    fn color_panel(&self, block: Option<BlockId>, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let (cur_t, mut cur_b) = self.current_colors();
        if let Some(id) = block {
            cur_b = self.core.block(id).and_then(|b| b.bg.clone());
        }
        let mut fonts = div().flex().justify_between();
        for (i, (name, hex)) in theme::FONT_COLORS.iter().enumerate() {
            let sel = cur_t.as_ref().map(|c| c.0.as_str()) == *name;
            let value = name.map(|n| ColorName(n.to_string()));
            let toggled = if sel && value.is_some() { None } else { value };
            fonts = fonts.child(swatch(("fc", i), sel, true, font_swatch(*hex)).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |e, _, window, cx| {
                    cx.stop_propagation();
                    e.apply_color(Some(toggled.clone()), None, window, cx);
                }),
            ));
        }
        let mut bgs = div().flex().flex_wrap().justify_between().gap_y(px(4.));
        let (names, fill): (&[Option<&str>], fn(&str) -> Option<Hsla>) =
            if block.is_some() { (&theme::CELL_BG_COLORS, theme::bg_swatch) } else { (&theme::BG_COLORS, theme::bg_color) };
        for (i, name) in names.iter().enumerate() {
            let sel = cur_b.as_ref().map(|c| theme::bg_canon(&c.0)) == *name;
            let value = name.map(|n| ColorName(n.to_string()));
            let toggled = if sel && value.is_some() { None } else { value };
            let inner = match name.and_then(fill) {
                Some(fill) => div().bg(fill),
                None => none_swatch(),
            };
            bgs = bgs.child(swatch(("bc", i), sel, name.is_none(), inner).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |e, _, window, cx| {
                    cx.stop_propagation();
                    match block {
                        Some(id) => e.set_block_bg(id, toggled.clone(), window, cx),
                        None => e.apply_color(None, Some(toggled.clone()), window, cx),
                    }
                }),
            ));
        }
        let title = color_title;
        Popover::new("color-panel")
            .w(px(CALLOUT_PANEL_W))
            .pt(px(4.))
            .px(px(10.))
            .pb(px(12.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(title(crate::settings::text("字体颜色")))
            .child(fonts)
            .child(title(crate::settings::text("背景颜色")))
            .child(bgs)
            .child(
                Button::outline("color-reset")
                    .sized(ButtonSize::Xs)
                    .mt(px(12.))
                    .w_full()
                    .label(crate::settings::text("恢复默认"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |e, _, window, cx| {
                            cx.stop_propagation();
                            match block {
                                Some(id) => {
                                    e.apply_color(Some(None), None, window, cx);
                                    e.set_block_bg(id, None, window, cx);
                                }
                                None => e.apply_color(Some(None), Some(None), window, cx),
                            }
                        }),
                    ),
            )
    }

    /// Records an overlay's painted bounds into the callout hover zone.
    fn zone_probe(&self) -> impl IntoElement {
        let zone = self.callout_zone.clone();
        canvas(move |b, _, _| zone.borrow_mut().push(b), |_, _, _, _| {}).absolute().top_0().left_0().size_full()
    }

    /// The callout hover bar: the palette, whose colour panel opens on
    /// hover, centred below it.
    fn callout_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (block, open) = match &self.menu {
            Some(Menu::CalloutColor { block, .. }) => (*block, true),
            None => (self.handle_block()?, false),
            _ => return None,
        };
        if !matches!(self.core.kind_of(block), Some(BlockKind::Callout { .. })) {
            return None;
        }
        let b = self.block_rect(block)?;
        let pos = point(b.center().x - px(CALLOUT_BAR_W / 2.), b.top() - px(9. + CALLOUT_BAR_H));
        let palette = Button::ghost("callout-palette").sized(ButtonSize::Sm).editor_icon("palette").selected(open)
            .on_hover(cx.listener(move |e, hovered: &bool, _, cx| {
                if *hovered && e.menu.is_none() {
                    e.menu = Some(Menu::CalloutColor { block, section: 0 });
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |e, _, _, cx| {
                    cx.stop_propagation();
                    e.menu = if open { None } else { Some(Menu::CalloutColor { block, section: 0 }) };
                    cx.notify();
                }),
            );
        let drop = open.then(|| {
            div()
                .absolute()
                .top(px(CALLOUT_BAR_H + 4.))
                .left(px((CALLOUT_BAR_W - CALLOUT_PANEL_W) / 2.))
                .child(self.callout_panel(block, cx).relative().child(self.zone_probe()))
        });
        let el = Toolbar::new("callout-bar")
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(palette)
            .overlay(self.zone_probe())
            .overlays(drop);
        Some(at(self.rel(pos), Anchor::TopLeft, el))
    }

    /// Callout panel: 字体颜色 8 / 边框颜色 8 / 填充颜色 16, 随机, 恢复默认.
    fn callout_panel(&self, block: BlockId, cx: &mut Context<Self>) -> Popover {
        let (cur_bg, cur_border, cur_text) = match self.core.kind_of(block) {
            Some(BlockKind::Callout { bg, border, text_color, .. }) => (bg.clone(), border.clone(), text_color.clone()),
            _ => (None, None, None),
        };
        let name_eq = |c: &Option<ColorName>, n: Option<&str>| c.as_ref().map(|c| c.0.as_str()) == n;
        let mut fonts = div().flex().justify_between();
        for (i, (name, hex)) in theme::FONT_COLORS.iter().enumerate() {
            let sel = name_eq(&cur_text, *name);
            let v = if sel { None } else { name.map(|n| ColorName(n.to_string())) };
            fonts = fonts.child(
                swatch(("cf", i), sel, true, font_swatch(*hex))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |e, _, window, cx| {
                        cx.stop_propagation();
                        let v = v.clone();
                        e.set_callout(block, |_, _, t, _| *t = v, window, cx);
                    }),
                ),
            );
        }
        let mut borders = div().flex().justify_between();
        let border_names: [Option<&str>; 8] = [None, Some("gray"), Some("red"), Some("orange"), Some("yellow"), Some("green"), Some("blue"), Some("purple")];
        for (i, name) in border_names.iter().enumerate() {
            let sel = name_eq(&cur_border, *name);
            let v = if sel { None } else { name.map(|n| ColorName(n.to_string())) };
            let inner = match name.and_then(theme::callout_border_swatch) {
                Some(col) => div().rounded(px(2.)).border_2().border_color(col),
                None => none_swatch(),
            };
            borders = borders.child(
                swatch(("cb", i), sel, name.is_none(), inner).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |e, _, window, cx| {
                        cx.stop_propagation();
                        let v = v.clone();
                        e.set_callout(block, |_, b, _, _| *b = v, window, cx);
                    }),
                ),
            );
        }
        let mut fills = div().flex().flex_wrap().justify_between().gap_y(px(4.));
        for (i, name) in theme::CALLOUT_BG_COLORS.iter().enumerate() {
            let sel = name_eq(&cur_bg, *name);
            let v = if sel { None } else { name.map(|n| ColorName(n.to_string())) };
            let inner = match name.and_then(theme::callout_bg) {
                Some(col) => div().bg(col),
                None => none_swatch(),
            };
            fills = fills.child(swatch(("cg", i), sel, name.is_none(), inner).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |e, _, window, cx| {
                    cx.stop_propagation();
                    let v = v.clone();
                    e.set_callout(block, |bg, _, _, _| *bg = v, window, cx);
                }),
            ));
        }
        let title = color_title;
        let button = |id: &'static str, label: &'static str| Button::outline(id).sized(ButtonSize::Xs).flex_1().label(crate::settings::label(label));
        Popover::new("callout-panel")
            .w(px(CALLOUT_PANEL_W))
            .pt(px(4.))
            .px(px(10.))
            .pb(px(12.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(title("字体颜色"))
            .child(fonts)
            .child(title("边框颜色"))
            .child(borders)
            .child(title("填充颜色"))
            .child(fills)
            .child(
                div()
                    .mt(px(12.))
                    .flex()
                    .gap(px(8.))
                    .child(button("callout-random", "随机").on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |e, _, window, cx| {
                            cx.stop_propagation();
                            let hue = theme::HUES[menus::random_index(theme::HUES.len())];
                            e.set_callout(
                                block,
                                |bg, b, _, _| {
                                    *bg = Some(ColorName(format!("light-{hue}")));
                                    *b = Some(ColorName(hue.to_string()));
                                },
                                window,
                                cx,
                            );
                        }),
                    ))
                    .child(button("callout-reset", crate::settings::text("恢复默认")).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |e, _, window, cx| {
                            cx.stop_propagation();
                            e.set_callout(
                                block,
                                |bg, b, t, _| {
                                    *bg = Some(ColorName("light-orange".into()));
                                    *b = Some(ColorName("orange".into()));
                                    *t = None;
                                },
                                window,
                                cx,
                            );
                        }),
                    )),
            )
    }

    /// The LaTeX editor under an equation.
    fn equation_editor(&self, block: BlockId, offset: usize, window: &mut Window, cx: &App) -> Option<AnyElement> {
        let Some(b) = self.cache.borrow().get(&block).and_then(|l| l.equation_bounds(offset)) else {
            // Not laid out yet (just inserted): place it next frame.
            window.request_animation_frame();
            return None;
        };
        let focused = self.eq_field.read(cx).is_focused(window);
        let el = Popover::new("equation-editor")
            .w(px(400.))
            .pt(px(16.))
            .px(px(16.))
            .pb(px(8.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                Textarea::new("equation-input")
                    .focused(focused)
                    .max_h(px(500.))
                    .overflow_hidden()
                    .flex()
                    .child(self.eq_field.clone()),
            )
            .child(
                div()
                    .mt(px(8.))
                    .text_size(px(12.))
                    .line_height(px(20.))
                    .text_color(color::muted_foreground())
                    .child(crate::settings::text("按 ESC 完成输入")),
            );
        Some(at(self.rel(point(b.left(), b.bottom() + px(4.))), Anchor::TopLeft, el))
    }

    // ---- code language picker

    fn lang_menu(&self, block: BlockId, selected: usize, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let b = self.block_rect(block)?;
        let current = match self.core.kind_of(block) {
            Some(BlockKind::Code { lang, .. }) => lang.clone(),
            _ => String::new(),
        };
        let q = self.lang_field.read(cx).text.clone();
        let langs = menus::filter_languages(&q);
        let lang_scroll = crate::design::scroll::handle("lang-list");
        let mut list = div().id("lang-list").max_h(px(288.)).overflow_y_scroll().track_scroll(&lang_scroll).flex().flex_col();
        for (i, l) in langs.iter().enumerate() {
            let l: &'static str = l;
            let is_cur = current.eq_ignore_ascii_case(l);
            list = list.child(
                MenuItem::new(("lang", i))
                    .label(l)
                    .checked(is_cur)
                    .selected(i == selected)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |e, _, window, cx| {
                            cx.stop_propagation();
                            e.set_lang(block, l, window, cx);
                        }),
                    ),
            );
        }
        if langs.is_empty() {
            list = list.child(div().h(px(ROW_H)).px(px(PAD + 8.)).flex().items_center().text_color(color::muted_foreground()).child(crate::settings::text("无匹配结果")));
        }
        let el = Popover::new("lang-menu")
            .menu()
            .w(px(240.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                Field::new("lang-search")
                    .sized(FieldSize::Sm)
                    .mx(px(PAD))
                    .mb(px(6.))
                    .child(icon("search", 14., color::muted_foreground()))
                    .child(self.lang_field.clone()),
            )
            .child(list.child(crate::design::scroll::bar("lang-list", &lang_scroll, 0.)));
        // Under the language button, which sits ~267px left of the block's right
        // edge; above it when the list wouldn't fit below.
        let h = px(2. * PAD + 38. + (langs.len().max(1) as f32 * ROW_H).min(288.));
        let x = b.right() - px(267.);
        let below = b.top() + px(34.);
        let (pos, corner) = if below + h > window.viewport_size().height - px(8.) && b.top() + px(4.) - h >= px(8.) {
            (point(x, b.top() + px(4.)), Anchor::BottomLeft)
        } else {
            (point(x, below), Anchor::TopLeft)
        };
        Some(at(self.rel(pos), corner, el))
    }

    // ---- floating text toolbar

    fn toolbar(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dropdown = match &self.menu {
            None => None,
            Some(m @ (Menu::TurnInto | Menu::Align | Menu::Color | Menu::TbMore)) => Some(m.clone()),
            _ => return None,
        };
        let sel = self.core.sel;
        if self.core.cell_sel.is_some() || self.toolbar_hidden || matches!(self.menu, Some(Menu::Equation { .. })) || self.dragging || sel.is_collapsed() || self.core.in_block_mode() || self.slash.is_some() {
            return None;
        }
        if !self.focus.is_focused(window) {
            return None;
        }
        let (s, e) = self.core.ordered();
        if s.block == TITLE {
            return None;
        }
        let (sp, _) = self.point_of(s)?;
        let (ep, elh) = self.point_of(e)?;
        let (hp, _) = self.point_of(sel.head)?;
        // The bar is centred on the selection focus and floats 5px
        // above the first line box (below the last one when there is no room).
        let viewport = self.list.viewport_bounds();
        let view_top = viewport.top();
        let pane_left = self.rel(point(viewport.left() + px(8.), px(0.))).x;
        let pane_right = self.rel(point(viewport.right() - px(8.), px(0.))).x;
        // Keep the bar inside the pane on both sides: a selection near the right
        // edge used to push it, and its dropdowns, past the window (#95).
        let (at_x, tw) = toolbar_layout(&toolbar_widths(self.markdown));
        let x = (hp.x - px(tw / 2.)).max(pane_left).min((pane_right - px(tw)).max(pane_left));
        let above = sp.y - px(45.) >= view_top;
        let pos = if above { point(x, sp.y - px(45.)) } else { point(x, ep.y + elh + px(5.)) };

        let head_kind = self.core.kind_of(s.block).cloned().unwrap_or(BlockKind::Text);
        let align = self.current_align();
        let mark_btn = |id: &'static str, ic: &'static str, tip: &'static str, keys: &'static str, md: Option<&'static str>, mark: Mark, get: fn(&Marks) -> bool| {
            let active = self.mark_active(get);
            Toggle::new(id).editor_icon(ic).pressed(active)
                .tip_md(tip, Some(keys), md)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |e, _, window, cx| {
                        cx.stop_propagation();
                        e.menu = None;
                        e.toggle_mark(mark, window, cx);
                    }),
                )
        };
        // Hover opens the dropdowns; a press keeps them open.
        let open = |m: Menu| {
            cx.listener(move |e: &mut Editor, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                e.menu = Some(m.clone());
                cx.notify();
            })
        };
        let hover = |m: Menu| cx.listener(move |e: &mut Editor, h: &bool, _, cx| e.tb_drop_over(m.clone(), 0, *h, cx));
        let turn_below = (window.viewport_size().height - pos.y - px(52.)).max(px(0.));
        let turn_above = (pos.y - px(8.)).max(px(0.));
        let turn_up = turn_below < px(338.) && turn_above > turn_below;
        let turn_height = if turn_up { turn_above } else { turn_below }.min(px(338.));
        let more_height = px(2. * PAD + 2. * ROW_H);
        let more_above = (pos.y - px(10.)).max(px(0.));
        let more_below = (window.viewport_size().height - pos.y - px(52.)).max(px(0.));
        // Down unless it doesn't fit there, like the other dropdowns.
        let more_up = more_below < more_height && more_above > more_below;
        let drop = |m: Menu, x: f32| {
            div()
                .id("tb-drop")
                .absolute()
                .when(matches!(m, Menu::TurnInto) && turn_up, |d| d.bottom(px(TOOLBAR_H + 4.)))
                .when(!matches!(m, Menu::TurnInto) || !turn_up, |d| d.top(px(TOOLBAR_H + 4.)))
                .left(px(x))
                .on_hover(cx.listener(move |e: &mut Editor, h: &bool, _, cx| e.tb_drop_over(m.clone(), 1, *h, cx)))
        };
        // A menu's trigger: its glyph and a chevron, `muted` while the menu is open.
        let trigger = |id: &'static str, ic: &str, m: Menu| {
            Button::ghost(id).sized(ButtonSize::Sm).editor_icon(ic.to_string()).icon_end("chevron-down").selected(dropdown.as_ref() == Some(&m))
        };
        let align_icon = match align {
            Align::Left => "align-left",
            Align::Center => "align-center",
            Align::Right => "align-right",
        };

        // Where the dropdowns' buttons start; the colour panel is wider than the
        // bar's right-hand section, so it is right-aligned inside the bar
        // instead of hanging past its edge (#95).
        let x_turn = at_x[0];
        let x_align = at_x[2];
        let x_color = f32::min(at_x[11], tw - CALLOUT_PANEL_W).max(0.);
        let sep = ButtonGroupSeparator::new;
        let bar = Toolbar::new("toolbar")
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(trigger("tb-turn", kind_icon(&head_kind), Menu::TurnInto).on_hover(hover(Menu::TurnInto)).on_mouse_down(MouseButton::Left, open(Menu::TurnInto)))
            .child(sep())
            .when(!self.markdown, |d| d
                .child(trigger("tb-align", align_icon, Menu::Align).on_hover(hover(Menu::Align)).on_mouse_down(MouseButton::Left, open(Menu::Align)))
                .child(sep()))
            .child(mark_btn("tb-b", "bold", crate::settings::text("粗体"), crate::shortcuts::hint("editor::Bold", "⌘ + B"), Some(crate::settings::text("**文本** 空格")), Mark::Bold, |m| m.bold))
            .child(mark_btn("tb-s", "strikethrough", crate::settings::text("删除线"), crate::shortcuts::hint("editor::Strike", "⌘ + Shift + X"), Some(crate::settings::text("~~文本~~ 空格")), Mark::Strike, |m| m.strike))
            .child(mark_btn("tb-i", "italic", crate::settings::text("斜体"), crate::shortcuts::hint("editor::Italic", "⌘ + I"), Some(crate::settings::text("*文本* 空格")), Mark::Italic, |m| m.italic))
            .child(mark_btn("tb-u", "underline", crate::settings::text("下划线"), crate::shortcuts::hint("editor::Underline", "⌘ + U"), Some(crate::settings::text("~文本~ 空格")), Mark::Underline, |m| m.underline))
            .child(
                // 链接 makes one, so it is never pressed; a toggle for its size
                Toggle::new("tb-link")
                    .editor_icon("link")
                    .tip_above(crate::settings::text("链接"), Some(crate::shortcuts::hint("editor::Link", "⌘ + K")))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|e, _, window, cx| {
                            cx.stop_propagation();
                            e.link_from_selection(window, cx);
                        }),
                    ),
            )
            .child(mark_btn("tb-code", "code", crate::settings::text("代码"), crate::shortcuts::hint("editor::InlineCode", "⌘ + Control + C"), Some(crate::settings::text("`代码`")), Mark::Code, |m| m.code))
            .child(
                // 转为公式 acts, it is never pressed; a toggle for its size, as 链接
                Toggle::new("tb-equation")
                    .editor_icon("sigma")
                    .tip_md(crate::settings::text("转为公式"), None, Some(crate::settings::text("$$文字$$")))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|e, _, window, cx| {
                            cx.stop_propagation();
                            e.menu = None;
                            e.selection_to_equation(window, cx);
                        }),
                    ),
            )
            .when(!self.markdown, |d| d.child(self.color_split("tb-color", matches!(dropdown, Some(Menu::Color)), cx)))
            .child(sep())
            .child(
                Button::ghost("tb-more")
                    .sized(ButtonSize::Sm)
                    .editor_icon("layout-grid")
                    .selected(matches!(dropdown, Some(Menu::TbMore)))
                    .on_hover(hover(Menu::TbMore))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|e, _, _, cx| {
                            cx.stop_propagation();
                            e.menu = Some(Menu::TbMore);
                            cx.notify();
                        }),
                    ),
            );

        let drop_el: Option<AnyElement> = match dropdown {
            Some(Menu::TurnInto) => {
                let mut list = Popover::new("tb-turn-menu").menu().min_w(px(200.)).fit(f32::from(turn_height)).on_scroll(|_, _, cx| cx.stop_propagation());
                for (i, it) in TURN_ITEMS.iter().enumerate() {
                    let k = (it.kind)();
                    if self.markdown && matches!(k, BlockKind::Callout { .. }) {
                        continue;
                    }
                    let active = menus::same_kind(&head_kind, &k);
                    list = list.child(
                        row(("tt", i), it.icon, it.label, false, false)
                            .checked(active)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |e, _, window, cx| {
                                    cx.stop_propagation();
                                    e.menu = None;
                                    e.turn(k.clone(), false, window, cx);
                                }),
                            ),
                    );
                }
                Some(drop(Menu::TurnInto, x_turn).child(list).into_any_element())
            }
            Some(Menu::Align) => {
                let item = |id: &'static str, ic: &'static str, label: &'static str, keys: &'static str, a: Align| {
                    row(id, ic, label, false, false).checked(align == a).tip_above(label, Some(keys)).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |e, _, window, cx| {
                            cx.stop_propagation();
                            e.menu = None;
                            e.align(a, window, cx);
                        }),
                    )
                };
                let list = Popover::new("tb-align-menu")
                    .menu()
                    .w(px(180.))
                    .child(item("ta-l", "align-left", crate::settings::text("左对齐"), crate::shortcuts::hint("editor::AlignLeft", "⌘ + Shift + L"), Align::Left))
                    .child(item("ta-c", "align-center", crate::settings::text("居中对齐"), crate::shortcuts::hint("editor::AlignCenter", "⌘ + Shift + E"), Align::Center))
                    .child(item("ta-r", "align-right", crate::settings::text("右对齐"), crate::shortcuts::hint("editor::AlignRight", "⌘ + Shift + R"), Align::Right));
                Some(drop(Menu::Align, x_align).child(list).into_any_element())
            }
            Some(Menu::Color) => Some(drop(Menu::Color, x_color).child(self.color_panel(None, cx)).into_any_element()),
            Some(Menu::TbMore) => Some(div().absolute().when(more_up, |d| d.bottom(px(TOOLBAR_H + 4.))).when(!more_up, |d| d.top(px(TOOLBAR_H + 4.))).right_0().child(self.tb_more_menu(cx)).into_any_element()),
            _ => None,
        };
        Some(at(self.rel(pos), Anchor::TopLeft, bar.overlays(drop_el)))
    }
}

/// The table toolbar's items: merge, |, palette, |, T▾, |, ≡▾, |, five marks,
/// A▾; + |, delete for whole rows / columns. Markdown drops the palette and A▾.
fn table_toolbar_widths(md: bool, whole: bool) -> Vec<f32> {
    use crate::design::toolbar::w;
    let mut v = vec![w::BUTTON, w::SEPARATOR];
    if !md { v.extend([w::BUTTON, w::SEPARATOR]) }
    v.extend([w::DROPDOWN, w::SEPARATOR, w::DROPDOWN, w::SEPARATOR]);
    v.extend([w::TOGGLE; 5]);
    if !md { v.push(w::SPLIT) }
    if whole { v.extend([w::SEPARATOR, w::BUTTON]) }
    v
}

impl Editor {
    /// The toolbars' A▾: a split button (`ButtonGroup` of two ghost
    /// `icon-sm` buttons). The A applies the last used colors at once (by
    /// default the yellow highlight); the chevron opens the panel on hover.
    /// The A's colours are content and stay.
    fn color_split(&self, id: &'static str, open: bool, cx: &Context<Self>) -> ButtonGroup {
        let (lt, lb) = self.last_color.clone();
        ButtonGroup::new()
            .child(
                Button::ghost((id, 0usize))
                    .sized(ButtonSize::Sm)
                    .child(
                        div()
                            .size(px(20.))
                            .rounded(px(2.))
                            .when_some(lb.as_ref().and_then(|c| theme::bg_color(&c.0)), |d, c| d.bg(c))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(16.))
                            .text_color({
                                let c = lt.as_ref().and_then(|c| theme::text_color(&c.0)).unwrap_or(Theme::text());
                                if lb.is_some() { crate::theme::on_mark(c) } else { c }
                            })
                            .child("A"),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|e, _, window, cx| {
                            cx.stop_propagation();
                            e.menu = None;
                            let (t, b) = e.last_color.clone();
                            e.apply_color(Some(t), Some(b), window, cx);
                        }),
                    ),
            )
            .child(
                Button::ghost((id, 1usize))
                    .sized(ButtonSize::Sm)
                    .icon("chevron-down")
                    .selected(open)
                    .on_hover(cx.listener(|e, h: &bool, _, cx| e.tb_drop_over(Menu::Color, 0, *h, cx)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|e, _, _, cx| {
                            cx.stop_propagation();
                            e.menu = Some(Menu::Color);
                            cx.notify();
                        }),
                    ),
            )
    }

    /// Window bounds of the selected cells and of the table's top edge.
    fn cell_sel_rect(&self) -> Option<(Bounds<Pixels>, Pixels)> {
        let (id, r) = self.core.cell_range()?;
        let l = self.probed(Probe::Bar(id, true, r.left))?;
        let rr = self.probed(Probe::Bar(id, true, r.right))?;
        let t = self.probed(Probe::Bar(id, false, r.top))?;
        let b = self.probed(Probe::Bar(id, false, r.bottom))?;
        let table_top = self.probed(Probe::Bar(id, true, 0))?.bottom();
        Some((Bounds::from_corners(point(l.left(), t.top()), point(rr.right(), b.bottom())), table_top))
    }

    /// The toolbar over selected table cells: no text toolbar and no
    /// context menu, so this bar carries the cell operations.
    fn table_toolbar(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let s = self.core.cell_sel?;
        let dropdown = match &self.menu {
            None => None,
            Some(m @ (Menu::TurnInto | Menu::Align | Menu::Color | Menu::CellBg)) => Some(m.clone()),
            _ => return None,
        };
        if self.toolbar_hidden || self.dragging || self.tables.drag.is_some() || !self.focus.is_focused(window) {
            return None;
        }
        let (range_top, whole) = {
            let (_, r) = self.core.cell_range()?;
            (r.top, s.kind != CellSelKind::Range)
        };
        let (rect, table_top) = self.cell_sel_rect()?;
        // A Markdown file keeps only a column's alignment, no colours or
        // vertical alignment, so those are left out (as on the text toolbar).
        let md = self.markdown;
        let (at_x, w) = toolbar_layout(&table_toolbar_widths(md, whole));
        let viewport = self.list.viewport_bounds();
        let pane_left = viewport.left() + px(12.);
        let pane_right = viewport.right() - px(12.);
        // Same two-sided clamp as the text toolbar (#95).
        let x = (rect.center().x - px(w / 2.)).max(pane_left).min((pane_right - px(w)).max(pane_left));
        let bottom = if range_top == 0 { table_top - px(22.) } else { rect.top() - px(15.) };
        let y = if bottom - px(TOOLBAR_H) >= viewport.top() { bottom - px(TOOLBAR_H) } else { rect.bottom() + px(15.) };

        let cell = self.core.first_selected_cell();
        let valign = cell.and_then(|c| c.valign.clone());
        let cur_bg = cell.and_then(|c| c.bg.clone());
        let n_cells = self.core.selected_cells().len();
        let merged = self.core.merged_selected().is_some();
        let head_kind = self
            .core
            .cell_text_blocks()
            .first()
            .and_then(|id| self.core.kind_of(*id).cloned())
            .unwrap_or(BlockKind::Text);
        let align = self.current_align();

        let sep = ButtonGroupSeparator::new;
        let toggle = |m: Menu| {
            cx.listener(move |e: &mut Editor, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                e.menu = if e.menu.as_ref() == Some(&m) { None } else { Some(m.clone()) };
                cx.notify();
            })
        };
        // Hover opens the dropdowns; a press keeps them open.
        let open = |m: Menu| {
            cx.listener(move |e: &mut Editor, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                e.menu = Some(m.clone());
                cx.notify();
            })
        };
        let hover = |m: Menu| cx.listener(move |e: &mut Editor, h: &bool, _, cx| e.tb_drop_over(m.clone(), 0, *h, cx));
        let turn_below = (window.viewport_size().height - y - px(52.)).max(px(0.));
        let turn_above = (y - px(8.)).max(px(0.));
        let turn_up = turn_below < px(338.) && turn_above > turn_below;
        let turn_height = if turn_up { turn_above } else { turn_below }.min(px(338.));
        let drop = |m: Menu, x: f32| {
            div()
                .id("ttb-drop")
                .absolute()
                .when(matches!(m, Menu::TurnInto) && turn_up, |d| d.bottom(px(TOOLBAR_H + 4.)))
                .when(!matches!(m, Menu::TurnInto) || !turn_up, |d| d.top(px(TOOLBAR_H + 4.)))
                .left(px(x))
                .on_hover(cx.listener(move |e: &mut Editor, h: &bool, _, cx| e.tb_drop_over(m.clone(), 1, *h, cx)))
        };
        // A menu's trigger: its glyph and a chevron, `muted` while the menu is open.
        let trigger = |id: &'static str, ic: &str, m: Menu| {
            Button::ghost(id).sized(ButtonSize::Sm).editor_icon(ic.to_string()).icon_end("chevron-down").selected(dropdown.as_ref() == Some(&m))
        };
        let mark_btn = |id: &'static str, ic: &'static str, tip: &'static str, keys: &'static str, md: Option<&'static str>, mark: Mark, get: fn(&Marks) -> bool| {
            let active = self.mark_active(get);
            Toggle::new(id).editor_icon(ic).pressed(active)
                .tip_md(tip, Some(keys), md)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |e, _, window, cx| {
                        cx.stop_propagation();
                        e.menu = None;
                        e.toggle_mark(mark, window, cx);
                    }),
                )
        };

        let merge_btn = if merged {
            Button::ghost("ttb-merge").sized(ButtonSize::Sm).editor_icon("split-cells")
                .tip_above(crate::settings::text("拆分单元格"), None)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|e, _, window, cx| {
                        cx.stop_propagation();
                        e.menu = None;
                        e.core.table_unmerge();
                        e.changed(window, cx);
                    }),
                )
        } else if n_cells >= 2 {
            Button::ghost("ttb-merge").sized(ButtonSize::Sm).editor_icon("merge-cells")
                .tip_above(crate::settings::text("合并单元格"), None)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|e, _, window, cx| {
                        cx.stop_propagation();
                        e.menu = None;
                        e.core.table_merge();
                        e.changed(window, cx);
                    }),
                )
        } else {
            Button::ghost("ttb-merge").sized(ButtonSize::Sm).editor_icon("merge-cells").disabled(true)
        };
        let align_icon = match align {
            Align::Left => "align-left",
            Align::Center => "align-center",
            Align::Right => "align-right",
        };
        let mut bar = Toolbar::new("table-toolbar")
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(merge_btn)
            .when(!md, |d| d.child(sep()).child(
                Button::ghost("ttb-bg").sized(ButtonSize::Sm).editor_icon("palette").selected(matches!(dropdown, Some(Menu::CellBg)))
                    .when(!matches!(dropdown, Some(Menu::CellBg)), |d| d.tip_above(crate::settings::text("单元格背景颜色"), None))
                    .on_mouse_down(MouseButton::Left, toggle(Menu::CellBg)),
            ))
            .child(sep())
            .child(
                trigger("ttb-turn", kind_icon(&head_kind), Menu::TurnInto)
                    .on_hover(hover(Menu::TurnInto))
                    .on_mouse_down(MouseButton::Left, open(Menu::TurnInto)),
            )
            .child(sep())
            .child(
                trigger("ttb-align", align_icon, Menu::Align)
                    .on_hover(hover(Menu::Align))
                    .on_mouse_down(MouseButton::Left, open(Menu::Align)),
            )
            .child(sep())
            .child(mark_btn("ttb-b", "bold", crate::settings::text("粗体"), crate::shortcuts::hint("editor::Bold", "⌘ + B"), Some(crate::settings::text("**文本** 空格")), Mark::Bold, |m| m.bold))
            .child(mark_btn("ttb-s", "strikethrough", crate::settings::text("删除线"), crate::shortcuts::hint("editor::Strike", "⌘ + Shift + X"), Some(crate::settings::text("~~文本~~ 空格")), Mark::Strike, |m| m.strike))
            .child(mark_btn("ttb-i", "italic", crate::settings::text("斜体"), crate::shortcuts::hint("editor::Italic", "⌘ + I"), Some(crate::settings::text("*文本* 空格")), Mark::Italic, |m| m.italic))
            .child(mark_btn("ttb-u", "underline", crate::settings::text("下划线"), crate::shortcuts::hint("editor::Underline", "⌘ + U"), Some(crate::settings::text("~文本~ 空格")), Mark::Underline, |m| m.underline))
            .child(mark_btn("ttb-code", "code", crate::settings::text("代码"), crate::shortcuts::hint("editor::InlineCode", "⌘ + Control + C"), Some(crate::settings::text("`代码`")), Mark::Code, |m| m.code))
            .when(!md, |d| d.child(self.color_split("ttb-color", matches!(dropdown, Some(Menu::Color)), cx)));
        if whole {
            let label = if s.kind == CellSelKind::Cols { crate::settings::text("删除列") } else { crate::settings::text("删除行") };
            bar = bar.child(sep()).child(
                Button::ghost("ttb-del")
                    .sized(ButtonSize::Sm)
                    .tip_above(label, None)
                    .child(icon("bin", 16., if self.tables.danger { color::destructive() } else { color::foreground() }))
                    .on_hover(cx.listener(|e, hovered: &bool, _, cx| {
                        if e.tables.danger != *hovered {
                            e.tables.danger = *hovered;
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|e, _, window, cx| {
                            cx.stop_propagation();
                            e.menu = None;
                            e.tables.danger = false;
                            e.core.table_delete_selected();
                            e.changed(window, cx);
                        }),
                    ),
            );
        }

        // merge, |, (palette, |,) T▾, |, ≡▾, |, five marks, (A▾)
        let x_bg = at_x[2];
        let x_turn = if md { at_x[2] } else { at_x[4] };
        let x_align = if md { at_x[4] } else { at_x[6] };
        let x_color = if md { 0. } else { f32::min(at_x[13], w - CALLOUT_PANEL_W).max(0.) };
        let drop_el: Option<AnyElement> = match dropdown {
            Some(Menu::TurnInto) => {
                let mut list = Popover::new("ttb-turn-menu").menu().min_w(px(200.)).fit(f32::from(turn_height)).on_scroll(|_, _, cx| cx.stop_propagation());
                for (i, it) in TURN_ITEMS.iter().enumerate() {
                    let k = (it.kind)();
                    if matches!(k, BlockKind::Callout { .. }) {
                        continue;
                    }
                    let active = menus::same_kind(&head_kind, &k);
                    list = list.child(
                        row(("ttt", i), it.icon, it.label, false, false)
                            .checked(active)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |e, _, window, cx| {
                                    cx.stop_propagation();
                                    e.menu = None;
                                    e.turn(k.clone(), false, window, cx);
                                }),
                            ),
                    );
                }
                Some(drop(Menu::TurnInto, x_turn).child(list).into_any_element())
            }
            Some(Menu::Align) => {
                let item = |id: &'static str, ic: &'static str, label: &'static str, active: bool, keys: Option<&'static str>| {
                    let r = row(id, ic, label, false, false).checked(active);
                    match keys {
                        Some(k) => r.tip_right(label, Some(k)),
                        None => r,
                    }
                };
                let h = |id, ic, label, keys, a: Align| {
                    item(id, ic, label, align == a, Some(keys)).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |e, _, window, cx| {
                            cx.stop_propagation();
                            e.menu = None;
                            e.align(a, window, cx);
                        }),
                    )
                };
                let v = |id, ic, label, value: Option<&'static str>| {
                    item(id, ic, label, valign.as_deref() == value, None).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |e, _, window, cx| {
                            cx.stop_propagation();
                            e.menu = None;
                            e.core.table_set_valign(value);
                            e.changed_preserving_viewport(window, cx);
                        }),
                    )
                };
                let list = Popover::new("ttb-align-menu")
                    .menu()
                    .w(px(232.))
                    .child(h("tta-l", "align-left", crate::settings::text("左对齐"), crate::shortcuts::hint("editor::AlignLeft", "⌘ + Shift + L"), Align::Left))
                    .child(h("tta-c", "align-center", crate::settings::text("居中对齐"), crate::shortcuts::hint("editor::AlignCenter", "⌘ + Shift + E"), Align::Center))
                    .child(h("tta-r", "align-right", crate::settings::text("右对齐"), crate::shortcuts::hint("editor::AlignRight", "⌘ + Shift + R"), Align::Right))
                    .when(!md, |d| d
                        .child(separator_h())
                        .child(v("tta-t", "valign-top", crate::settings::text("顶部对齐"), None))
                        .child(v("tta-m", "valign-middle", crate::settings::text("垂直居中"), Some("middle")))
                        .child(v("tta-b", "valign-bottom", crate::settings::text("底部对齐"), Some("bottom"))));
                Some(drop(Menu::Align, x_align).child(list).into_any_element())
            }
            Some(Menu::Color) => Some(drop(Menu::Color, x_color).child(self.color_panel(None, cx)).into_any_element()),
            Some(Menu::CellBg) => Some(div().absolute().top(px(TOOLBAR_H + 4.)).left(px(x_bg)).child(self.cell_bg_panel(cur_bg, cx)).into_any_element()),
            _ => None,
        };
        Some(at(self.rel(point(x, y)), Anchor::TopLeft, bar.overlays(drop_el)))
    }

    /// 单元格背景颜色 (230 × 142): 2 × 8 swatches and 恢复默认.
    fn cell_bg_panel(&self, cur: Option<ColorName>, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let set = |v: Option<ColorName>| {
            cx.listener(move |e: &mut Editor, _: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                e.menu = None;
                e.core.table_set_bg(v.clone());
                e.changed_preserving_viewport(window, cx);
            })
        };
        let mut grid = div().flex().flex_wrap().justify_between().gap_y(px(4.));
        for (i, name) in theme::CELL_BG_COLORS.iter().enumerate() {
            let sel = cur.as_ref().map(|c| c.0.as_str()) == *name;
            let inner = match name.and_then(theme::bg_swatch) {
                Some(fill) => div().bg(fill),
                None => none_swatch(),
            };
            grid = grid.child(
                swatch(("cbg", i), sel, name.is_none(), inner).on_mouse_down(MouseButton::Left, set(name.map(|n| ColorName(n.to_string())))),
            );
        }
        Popover::new("cell-bg-panel")
            .w(px(CALLOUT_PANEL_W))
            .pt(px(4.))
            .px(px(10.))
            .pb(px(12.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(color_title(crate::settings::text("单元格背景颜色")))
            .child(grid)
            .child(
                Button::outline("cell-bg-reset")
                    .sized(ButtonSize::Xs)
                    .mt(px(12.))
                    .w_full()
                    .label(crate::settings::text("恢复默认"))
                    .on_mouse_down(MouseButton::Left, set(None)),
            )
    }

    /// The handle menu of a table: 缩进›, 剪切 / 复制 / 删除, the 标题行 /
    /// 标题列 switches, 均分列宽 and 在下方添加.
    fn table_block_menu(&self, block: BlockId, (hrow, hcol): (bool, bool), sub: Option<BlockSub>, r: Bounds<Pixels>, cx: &mut Context<Self>) -> Option<AnyElement> {
        let act = |a: BlockAct| {
            cx.listener(move |e: &mut Editor, _: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                e.block_menu_action(block, a.clone(), window, cx);
            })
        };
        let set_sub = |s: Option<BlockSub>| {
            cx.listener(move |e: &mut Editor, _: &MouseMoveEvent, _, cx| {
                if let Some(Menu::Block { sub, .. }) = &mut e.menu
                    && *sub != s
                {
                    *sub = s;
                    cx.notify();
                }
            })
        };
        let plain = |id: &'static str, ic: &'static str, label: &'static str, a: BlockAct| {
            row(id, ic, label, false, false).on_mouse_move(set_sub(None)).on_mouse_down(MouseButton::Left, act(a))
        };
        let toggle = |id: &'static str, ic: &'static str, label: &'static str, on: bool, a: BlockAct| plain(id, ic, label, a).switch(on);
        let m = Popover::new("block-menu")
            .menu()
            .w(px(MENU_W))
            .child(row("bm-indent", "indent-align", crate::settings::text("缩进"), true, sub == Some(BlockSub::Indent)).on_mouse_move(set_sub(Some(BlockSub::Indent))))
            .child(separator_h().on_mouse_move(set_sub(None)))
            .child(plain("bm-cut", "scissors", crate::settings::text("剪切"), BlockAct::Cut))
            .child(plain("bm-copy", "copy", crate::settings::text("复制"), BlockAct::Copy))
            .child(plain("bm-del", "trash-2", crate::settings::text("删除"), BlockAct::Delete))
            .child(separator_h().on_mouse_move(set_sub(None)))
            .child(toggle("bm-hrow", "header-row", crate::settings::text("标题行"), hrow, BlockAct::HeaderRow))
            .child(toggle("bm-hcol", "header-col", crate::settings::text("标题列"), hcol, BlockAct::HeaderCol))
            .child(plain("bm-dist", "distribute-cols", crate::settings::text("均分列宽"), BlockAct::DistributeCols))
            .child(separator_h().on_mouse_move(set_sub(None)))
            .child(add_below_row(matches!(sub, Some(BlockSub::Add(_))), cx));
        let sub_el = (sub == Some(BlockSub::Indent)).then(|| {
            let indent_row = |id: &'static str, ic: &'static str, label: &'static str, keys: Option<&'static str>, enabled: bool, a: BlockAct| {
                if enabled {
                    let r = row(id, ic, label, false, false).on_mouse_down(MouseButton::Left, act(a));
                    match keys {
                        Some(k) => r.tip_right(label, Some(k)),
                        None => r,
                    }
                } else {
                    row_off(id, ic, label).on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                }
            };
            Popover::new("bm-indent-sub")
                .menu()
                .absolute()
                .left(px(MENU_W + 4.))
                .top_0()
                .w(px(MENU_W))
                .child(indent_row("in-inc", "indent-increase", crate::settings::text("增加缩进"), None, self.core.can_indent(block), BlockAct::Indent))
                .child(indent_row("in-dec", "indent-decrease", crate::settings::text("减少缩进"), Some("Shift + Tab"), self.core.can_outdent(block), BlockAct::Outdent))
                .into_any_element()
        });
        let el = div()
            .relative()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(m)
            .children(sub_el);
        Some(at(self.rel(point(r.left(), r.bottom() + px(4.))), Anchor::TopLeft, el))
    }
}
