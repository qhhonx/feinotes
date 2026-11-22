//! 数据表's metrics and colours: 飞书 多维表格's, measured on the web app
//! (docs/design/feishu-research/REPORT.md) and named from `feishu_theme`.

use crate::feishu_theme::{Theme, c, ud};
use crate::table::FieldType;
use gpui::prelude::FluentBuilder;
use gpui::*;

/// Glyphs the editor's icon set has and the shell's doesn't.
const EDITOR_ICONS: &[&str] = &["calendar", "columns-2", "table", "layout-grid", "grip-vertical", "maximize", "undo", "redo", "download", "upload", "link", "paperclip",
    "user", "trash-2", "list-tree", "history", "row-add", "column-add", "type", "image", "move", "wrap-text", "replace", "caret-down", "caret-up", "triangle-down",
    "triangle-right", "square-plus", "unlink", "todo", "circle-help", "view-card", "timer", "one-to-one", "swatch-none", "fit-page"];

/// An icon from either set.
pub fn icon(name: &str, size: f32, color: Hsla) -> Svg {
    if EDITOR_ICONS.contains(&name) { crate::editor_ui::icon(name, size, color) } else { crate::ui::icon(name, size, color) }
}

/// Row-number / checkbox column.
pub const GUTTER_W: f32 = 56.;
/// Field header row; rows are `RowHeight::px`.
pub const HEADER_H: f32 = 36.;
/// The trailing "+" (新增字段) column.
pub const ADD_FIELD_W: f32 = 145.;
pub const CELL_PAD_X: f32 = 8.;
/// The one header line of a table in a page (飞书 docx's embedded base: 52).
pub const INLINE_HEAD_H: f32 = 40.;
/// Rows a table in a page shows before it scrolls inside (飞书 grows without end; a
/// page holding thousands of rows would draw them all).
pub const INLINE_MAX_ROWS_H: f32 = 1120.;
/// The add-record line under the rows: one short line whatever the row height (it is not a record).
pub const ADD_ROW_H: f32 = 32.;
/// View tab bar and toolbar.
/// The pane's one line of chrome (views, tools).
pub const BAR_H: f32 = 44.;
/// 统计 bar under the grid.
pub const FOOTER_H: f32 = 36.;
/// A 分组 header line.
pub const GROUP_H: f32 = 40.;
/// Indent per group level.
pub const GROUP_INDENT: f32 = 16.;
/// 记录详情 side panel.
pub const RECORD_W: f32 = 560.;
pub const TEXT: f32 = 13.;

/// Grid lines (飞书 #DEE0E3).
pub fn line() -> Hsla { c(ud::N300) }
/// The frozen columns' edge: darker than the grid lines (#BCBFC4).
pub fn frozen_line() -> Hsla { c(ud::N400) }
/// Selected column / row and the row number of the selected cell (#E9EEFC… a 10% primary on white).
pub fn sel_bg() -> Hsla { c(0xe9eefc) }
/// The row of the selected cell (飞书 #E7EEFD, measured in its embedded base, EMBED.md §3).
pub fn sel_row() -> Hsla { c(0xe7eefd) }
/// A range selection's fill over the cells.
pub fn range_bg() -> Hsla { crate::theme::adapt(Rgba { r: 20. / 255., g: 86. / 255., b: 240. / 255., a: 0.08 }.into()) }
pub fn row_hover() -> Hsla { c(ud::N50) }
pub fn star() -> Hsla { c(ud::O350) }
pub fn found_bg() -> Hsla { c(0xfff5c2) }
pub fn found_current() -> Hsla { c(0xffd666) }

/// The tag palette, (fill, text): 飞书's option colours, light then their
/// deeper second row.
pub const PALETTE: &[(u32, u32)] = &[
    (0xe1eaff, 0x1456f0), // blue
    (0xd9f5d6, 0x237b19), // green
    (0xfdf4bf, 0x8f6e00), // yellow
    (0xfeead2, 0xb26206), // orange
    (0xfde2e2, 0xc02a26), // red
    (0xfdddef, 0xb82879), // pink
    (0xece2fe, 0x6425d0), // purple
    (0xd5f6f2, 0x0f7b6c), // teal
    (0xd9f3fd, 0x0a6c96), // sky
    (0xeff0f1, 0x51565d), // grey
    (0xbacefd, 0x0c296e), // deeper blue
    (0xb7edb1, 0x124b0c), // deeper green
    (0xf8e6ab, 0x5c3a00), // deeper yellow
    (0xfed4a4, 0x6f3604), // deeper orange
    (0xfbbfbc, 0x7a1512), // deeper red
    (0xf9aed9, 0x6c0d47), // deeper pink
    (0xcdb2fa, 0x3a1b85), // deeper purple
    (0xa9efe6, 0x06443c), // deeper teal
    (0xb1e8fc, 0x0a4563), // deeper sky
    (0xdee0e3, 0x1f2329), // deeper grey
];

pub fn tag_colors(slot: u8) -> (Hsla, Hsla) {
    let (bg, fg) = PALETTE[slot as usize % PALETTE.len()];
    (c(bg), c(fg))
}

/// 填色's row tints: the palette's fills, lighter.
pub fn row_tint(slot: u8) -> Hsla {
    let (bg, _) = PALETTE[slot as usize % 10];
    let mut h = c(bg);
    h.a = 0.55;
    h
}

/// A select option's tag.
pub fn tag(name: &str, slot: u8) -> Div {
    let (bg, fg) = tag_colors(slot);
    div().flex_none().h(px(20.)).max_w(px(160.)).px(px(8.)).rounded(px(10.)).bg(bg).flex().items_center()
        .text_size(px(12.)).text_color(fg).overflow_hidden().whitespace_nowrap().text_ellipsis()
        .child(crate::ui::ui_text(name.to_string()))
}

/// A 人员 chip: initial in a circle, then the name.
pub fn person(name: &str, slot: u8) -> Div {
    let (bg, fg) = tag_colors(slot);
    let initial: String = name.chars().next().map(|ch| ch.to_string()).unwrap_or_default();
    div().flex_none().h(px(22.)).pl(px(2.)).pr(px(8.)).rounded(px(11.)).bg(c(ud::N100)).flex().items_center().gap(px(4.))
        .child(div().size(px(18.)).rounded_full().bg(bg).flex().items_center().justify_center().text_size(px(10.)).text_color(fg).child(initial))
        .child(div().text_size(px(12.)).text_color(Theme::text()).whitespace_nowrap().child(crate::ui::ui_text(name.to_string())))
}

/// A 关联 chip: the linked record's title.
pub fn link_chip(title: &str) -> Div {
    div().flex_none().h(px(22.)).max_w(px(180.)).px(px(6.)).rounded(px(4.)).bg(c(ud::N100)).flex().items_center()
        .text_size(px(12.)).text_color(Theme::text()).overflow_hidden().whitespace_nowrap().text_ellipsis()
        .child(crate::ui::ui_text(title.to_string()))
}

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
    }
}

/// 飞书's checkbox: a 14px box, blue with a tick when on.
pub fn checkbox(on: bool) -> Div {
    div().size(px(14.)).flex_none().flex().items_center().justify_center().rounded(px(3.))
        .when(on, |d| d.bg(Theme::primary()).child(icon("todo-check", 11., Theme::bg_body())))
        .when(!on, |d| d.border_1().border_color(c(ud::N500)).bg(Theme::bg_body()))
}

/// A thin rule between menu sections.
pub fn menu_rule() -> Div { div().flex_none().h(px(1.)).my(px(4.)).bg(Theme::line_divider()) }

/// A menu line: glyph, label, optional trailing element.
pub fn menu_item(id: impl Into<ElementId>, glyph: &str, label: impl Into<SharedString>, danger: bool) -> Stateful<Div> {
    let color = if danger { Theme::danger() } else { Theme::text() };
    div().id(id).flex_none().flex().items_center().gap(px(8.)).h(px(32.)).mx(px(4.)).px(px(8.)).rounded(px(6.))
        .cursor_pointer().hover(|s| s.bg(Theme::fill_hover()))
        .when(!glyph.is_empty(), |d| d.child(icon(glyph, 15., if danger { Theme::danger() } else { Theme::icon() })))
        .child(div().flex_1().min_w_0().text_size(px(TEXT)).text_color(color).overflow_hidden().whitespace_nowrap().text_ellipsis().child(label.into()))
}

/// 飞书's primary button (32 high, radius 6).
pub fn primary_btn(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div().id(id).flex_none().h(px(32.)).px(px(16.)).flex().items_center().justify_center().rounded(px(6.)).cursor_pointer()
        .bg(Theme::primary()).hover(|s| s.bg(c(ud::B500))).text_size(px(14.)).text_color(Theme::bg_body()).child(label.into())
}

pub fn plain_btn(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div().id(id).flex_none().h(px(32.)).px(px(16.)).flex().items_center().justify_center().rounded(px(6.)).cursor_pointer()
        .border_1().border_color(c(ud::N400)).bg(Theme::bg_body()).hover(|s| s.bg(c(ud::N100))).text_size(px(14.)).text_color(Theme::text()).child(label.into())
}

/// The box an input sits in (32 high, hairline, radius 6).
pub fn input_box() -> Div {
    div().h(px(32.)).px(px(10.)).flex().items_center().rounded(px(6.)).border_1().border_color(c(ud::N400)).bg(Theme::bg_body())
}

/// A select-like box showing a value and a chevron.
pub fn select_box(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div().id(id).h(px(32.)).px(px(10.)).flex().items_center().gap(px(6.)).rounded(px(6.)).border_1().border_color(c(ud::N400)).bg(Theme::bg_body())
        .cursor_pointer().hover(|s| s.border_color(Theme::primary()))
        .child(div().flex_1().min_w_0().text_size(px(TEXT)).text_color(Theme::text()).overflow_hidden().whitespace_nowrap().text_ellipsis().child(label.into()))
        .child(icon("chevron-down", 13., Theme::text_caption()))
}
