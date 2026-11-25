//! 数据表's look: the table-only parts of the design system (grid sizes, option colours, the
//! selection's marks) and short forwards to its components (`crate::design`), under the names the
//! table's code uses. Grid sizes the design system has no counterpart for (toolbar, record panel) stay
//! 飞书 多维表格's (docs/design/feishu-research/REPORT.md).

pub use crate::design::icon;
use crate::design::{Button, ButtonSize, Checkbox, Field, FieldSize, Label, MenuItem, MenuLabel, MenuSeparator, Segment, Segmented, Switch, Badge, chip, color, geo, shadow};
use crate::table::FieldType;
use fdl_core::table::RowHeight;
use gpui::prelude::FluentBuilder;
use gpui::*;

/// The 数据表's palette under the names its code uses (the app's `Theme` methods), drawn from the
/// design system's semantic colours.
pub struct Theme;
impl Theme {
    pub fn text() -> Hsla { color::foreground() }
    /// Captions, icons, header glyphs.
    pub fn text_caption() -> Hsla { color::muted_foreground() }
    /// Row numbers, placeholders, hints.
    pub fn text_placeholder() -> Hsla { color::placeholder() }
    pub fn text_disabled() -> Hsla { let mut h = color::muted_foreground(); h.a *= 0.5; h }
    pub fn icon() -> Hsla { color::muted_foreground() }
    /// Under the pointer (`accent`).
    pub fn fill_hover() -> Hsla { color::accent() }
    pub fn fill_hover_strong() -> Hsla { color::accent() }
    pub fn line_border() -> Hsla { color::border() }
    pub fn line_divider() -> Hsla { color::border() }
    /// The ground the table is drawn on: the note's page. Menus and popups are `popover`.
    pub fn bg_body() -> Hsla { crate::theme::Theme::bg_body() }
    pub fn primary() -> Hsla { color::primary() }
    /// A quiet fill for a picked or grouped thing (`muted`).
    pub fn primary_fill_1() -> Hsla { color::muted() }
    /// Links stay the app's blue: a URL in a cell reads as a link.
    pub fn link() -> Hsla { crate::theme::Theme::link() }
    pub fn danger() -> Hsla { color::destructive() }
}

/// The row's start (as 飞书's 多维表格): its number in the rows shown, a checkbox in its place while the
/// row is hovered or ticked, the handle before it and 展开 after it on hover.
pub const GUTTER_W: f32 = 56.;
/// Field header row (Vega's table head, h-10).
pub fn header_h() -> f32 { geo().table_head }
/// A row's height: the view's choice, the two short ones Vega's (36 / 44).
/// A row's height at `h` (适应内容: its one-line height, each row measured beyond that, `grid::fit_heights`).
pub fn row_px(h: RowHeight) -> f32 { match h { RowHeight::Short | RowHeight::Fit => geo().row_short, RowHeight::Medium => geo().row_medium, h => h.px() } }
/// Rows up to this tall hold one line, centred; taller ones wrap from the top.
pub fn one_line_h() -> f32 { geo().row_medium }
/// The trailing "+" (新增字段) column.
pub const ADD_FIELD_W: f32 = 145.;
/// Cell padding (Vega's p-2).
pub fn cell_pad_x() -> f32 { geo().cell_px }
/// The one header line of a table in a page (飞书 docx's embedded base: 52).
pub const INLINE_HEAD_H: f32 = 40.;
/// Rows a table in a page shows before it scrolls inside (飞书 grows without end; a
/// page holding thousands of rows would draw them all).
pub const INLINE_MAX_ROWS_H: f32 = 1120.;
/// The add-record line under the rows: one short line whatever the row height (it is not a record).
pub fn add_row_h() -> f32 { geo().add_row_h }
/// The pane's one line of chrome (views, tools).
pub const BAR_H: f32 = 44.;
/// 统计 bar under the grid.
/// A 分组 header line.
pub fn group_h() -> f32 { geo().group_h }
/// Indent per group level.
pub const GROUP_INDENT: f32 = 16.;
/// 记录详情 side panel.
pub const RECORD_W: f32 = 560.;
/// Body text: text-sm (14 / 20).
pub fn text_sm() -> f32 { geo().text }
/// Small text: text-xs (12 / 16).
pub fn text_xs() -> f32 { geo().text_xs }

/// Grid lines (`border`).
pub fn line() -> Hsla { color::border() }
/// The header row's fill: a step off the page (neutral-50; white at 3% in the dark), so the
/// names read as a header and not as a first row (as Supabase's grid head).
pub fn header_bg() -> Hsla { if crate::theme::dark() { crate::design::alpha(rgb(crate::design::neutral::N0).into(), 0.03) } else { rgb(crate::design::neutral::N50).into() } }
/// The line under the header: a step darker than the grid's (neutral-300; white at 16%).
pub fn header_line() -> Hsla { if crate::theme::dark() { crate::design::alpha(rgb(crate::design::neutral::N0).into(), 0.16) } else { rgb(crate::design::neutral::N300).into() } }
/// A field's name in the header: `foreground`.
pub fn header_text() -> Hsla { color::foreground() }
/// A table in a page: its frame's corners (Vega's card, rounded-xl).
pub fn frame_r() -> f32 { geo().frame_r }
/// The frozen columns' edge.
pub fn frozen_line() -> Hsla { color::border() }
/// Picked rows, a picked column, the selected cells (`muted`, as shadcn's selected table row).
pub fn sel_bg() -> Hsla { color::muted() }
/// A picked column's header.
pub fn sel_header() -> Hsla { color::muted() }
/// The picked cell's frame (`grid::RING_W`): the app's blue (craft B500), as Supabase and 飞书 frame
/// it; shadcn's `ring` grey reads as nothing picked over the grid lines.
pub fn ring() -> Hsla { crate::theme::Theme::primary() }
/// A range selection's fill over the cells.
pub fn range_bg() -> Hsla { color::muted() }
pub fn star() -> Hsla { crate::theme::c(crate::theme::craft::O350) }
// 查找's hits and the current one: 飞书多维表格's two yellows, as the first 数据表 took them (4cf23b9).
// The design system has no highlight token yet; these stay until it does.
pub fn found_bg() -> Hsla { crate::theme::c(0xfff5c2) }
pub fn found_current() -> Hsla { crate::theme::c(0xffd666) }

/// Slots 10–19: 飞书's deeper row, (fill, text): 飞书多维表格's option palette as files store its
/// slots (4cf23b9). The design system's chips cover the first ten only, so these keep their values.
const DEEPER: [(u32, u32); 10] = [(0xbacefd, 0x0c296e), (0xb7edb1, 0x124b0c), (0xf8e6ab, 0x5c3a00), (0xfed4a4, 0x6f3604), (0xfbbfbc, 0x7a1512),
    (0xf9aed9, 0x6c0d47), (0xcdb2fa, 0x3a1b85), (0xa9efe6, 0x06443c), (0xb1e8fc, 0x0a4563), (0xdee0e3, 0x1f2329)];
/// How many option colours there are.
pub const SLOTS: usize = 20;

/// An option colour, (fill, text): the design system's chips (blue, lime, yellow, orange, rose, pink,
/// purple, cyan, sky, neutral) in the order of 飞书's slots that files store.
pub fn tag_colors(slot: u8) -> (Hsla, Hsla) {
    match slot as usize % SLOTS {
        s @ 0..9 => chip::colors(s),
        9 => chip::neutral(),
        s => { let (bg, fg) = DEEPER[s - 10]; (crate::theme::c(bg), crate::theme::c(fg)) }
    }
}

/// 填色's row tints: the option fills, lighter.
pub fn row_tint(slot: u8) -> Hsla {
    let (mut h, _) = tag_colors(slot % 10);
    h.a *= 0.55;
    h
}

/// A select option's tag (`design::Badge` in the slot's colours).
pub fn tag(name: &str, slot: u8) -> Badge {
    // the design system's ten chips; the deeper row files may also hold keeps 飞书's fills
    match slot as usize % SLOTS { s @ 0..10 => Badge::chip(name.to_string(), s), _ => Badge::colored(name.to_string(), tag_colors(slot)) }
}

/// A small square tool: a glyph on a ghost button (`icon-xs`, 24).
pub fn mini_btn(id: impl Into<ElementId>, glyph: &str) -> Button { Button::ghost(id).icon(glyph.to_string()).sized(ButtonSize::Xs) }

/// A 人员 chip: initial in a circle, then the name, on a `secondary` pill.
pub fn person(name: &str, slot: u8) -> Div {
    let (bg, fg) = tag_colors(slot);
    let initial: String = name.chars().next().map(|ch| ch.to_string()).unwrap_or_default();
    div().flex_none().h(px(22.)).pl(px(2.)).pr(px(8.)).rounded_full().bg(color::secondary()).flex().items_center().gap(px(4.))
        .child(div().size(px(18.)).rounded_full().bg(bg).flex().items_center().justify_center().text_size(px(10.)).text_color(fg).child(initial))
        .child(div().text_size(px(text_xs())).text_color(Theme::text()).whitespace_nowrap().child(crate::ui::ui_text(name.to_string())))
}

/// A 关联 chip: the linked record's title, a grey badge.
pub fn link_chip(title: &str) -> Badge { Badge::new(title.to_string()) }

pub fn field_glyph(ty: FieldType) -> &'static str {
    use FieldType::*;
    match ty {
        Text | Barcode | Location => "ft-text",
        SingleSelect => "ft-select",
        MultiSelect => "ft-multi",
        Checkbox => "ft-check",
        Number | Currency | AutoNumber | Progress => "ft-number",
        Formula => "sigma",
        Rating => "star",
        Date | CreatedTime | ModifiedTime => "ft-date",
        Url => "ft-link",
        Link => "link",
        Lookup => "search",
        Attachment => "ft-attach",
        Person => "ft-person",
        Phone | Email => "ft-text",
        Note => "doc",
    }
}

/// The checkbox (`design::Checkbox`).
pub fn checkbox(on: bool) -> Checkbox { Checkbox::new(on) }

/// A rule between menu sections (`design::MenuSeparator`).
pub fn menu_rule() -> MenuSeparator { MenuSeparator::new() }

/// A small heading over a group of menu rows (`design::MenuLabel`).
pub fn menu_title(s: impl Into<SharedString>) -> MenuLabel { MenuLabel::new(s) }

/// A row of a menu or a picking list, filled by the caller (`design::MenuItem` without a label).
pub fn menu_row(id: impl Into<ElementId>) -> MenuItem { MenuItem::new(id) }
pub fn list_row(id: impl Into<ElementId>) -> MenuItem { MenuItem::new(id) }

/// A menu line: glyph, label (`design::MenuItem`).
pub fn menu_item(id: impl Into<ElementId>, glyph: &str, label: impl Into<SharedString>, danger: bool) -> MenuItem {
    MenuItem::new(id).icon(glyph.to_string()).label(label).destructive(danger)
}

pub fn primary_btn(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button { Button::primary(id).label(label) }
pub fn plain_btn(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button { Button::outline(id).label(label) }
/// A quiet button: glyph and text, toolbar height.
pub fn ghost_btn(id: impl Into<ElementId>, glyph: &str, label: impl Into<SharedString>) -> Button { Button::ghost(id).icon(glyph.to_string()).label(label) }

/// A toolbar tool (字段配置, 筛选…): its glyph; in use, its count beside it on `muted`.
pub fn tool_btn(id: impl Into<ElementId>, glyph: &str, n: usize) -> Button {
    let b = Button::ghost(id).icon(glyph.to_string()).selected(n > 0);
    if n > 0 { b.label(format!("{n}")).px(px(6.)) } else { b }
}

/// A small field in a menu or panel (a search box, a value): h-8.
pub fn input_box(id: impl Into<ElementId>) -> Field { Field::new(id).sized(FieldSize::Sm) }
/// A form field: h-9.
pub fn field_box(id: impl Into<ElementId>) -> Field { Field::new(id) }
/// A form field's label.
pub fn field_label(s: impl Into<SharedString>) -> Label { Label::new(s) }

/// A list of rows inside a form or panel: a menu's surface without its float, on a 0.5pt `border` ring.
pub fn inset_list(id: impl Into<ElementId>) -> Stateful<Div> {
    let g = geo();
    div().id(id).py(px(g.menu_pad)).rounded(px(g.menu_r)).bg(color::popover()).flex().flex_col().shadow(vec![shadow::ring(color::border(), 0.5)])
}

pub fn switch(on: bool, enabled: bool) -> Switch { Switch::new(on).disabled(!enabled) }

/// A select-like box around anything (a glyph and a name, tags), with a chevron.
pub fn pick_box(id: impl Into<ElementId>, body: Div) -> Field {
    Field::select(id).sized(FieldSize::Sm).child(body.flex_1().min_w_0().overflow_hidden())
}
/// A `pick_box` showing a value.
pub fn select_box(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Field { Field::select_value(id, label) }

/// A popover's heading (筛选, 排序, 填色).
pub fn panel_title(s: impl Into<SharedString>) -> Label { Label::new(s) }

/// The ✕ that drops a line from a panel.
pub fn close_btn(id: impl Into<ElementId>) -> Button { Button::ghost(id).icon("x").sized(ButtonSize::Xs) }

/// A segmented control's track and its segments (`design::Segmented`, `design::Segment`).
pub fn seg_track() -> Segmented { Segmented::new() }
pub fn segment(id: impl Into<ElementId>, on: bool, label: impl Into<SharedString>) -> Segment { Segment::new(id, label).selected(on) }

/// A colour swatch to pick (the option colours, 填色), `size` square: picked, a `ring` round it.
pub fn swatch(id: impl Into<ElementId>, fill: Hsla, size: f32, picked: bool) -> Stateful<Div> {
    let r = geo().menu_row_r;
    div().id(id).flex_none().size(px(size)).rounded(px(r)).bg(fill).cursor_pointer()
        // borders, not rings in the shadow: GPUI paints a shadow under the box too, and the fills are translucent
        .when(picked, |d| d.border_2().border_color(color::ring()))
        .when(!picked, |d| d.border_1().border_color(color::border()))
}

