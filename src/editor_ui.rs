pub use crate::ui::*;
use gpui::*;
use crate::feishu_theme as theme;
use crate::editor_theme::Theme;
pub fn icon(name: &str, size: f32, color: Hsla) -> Svg { svg().path(SharedString::from(format!("editor-icons/{name}.svg"))).size(px(size)).flex_none().text_color(color) }
pub fn popover() -> Div {
    div()
        .bg(crate::theme::raised(Theme::bg_body()))
        .border_1()
        .border_color(Theme::line_border())
        .rounded(px(6.))
        .shadow(theme::shadow_menu())
        .font_family(theme::UI_FONT)
        .text_color(Theme::text())
}
pub fn panel() -> Div {
    div()
        .bg(crate::theme::raised(Theme::bg_body()))
        .border_1()
        .border_color(Theme::line_border())
        .rounded(px(6.))
        .shadow(theme::shadow_panel())
        .font_family(theme::UI_FONT)
        .text_color(Theme::text())
}
pub const MENU_RADIUS: f32 = 6.;
pub const ROW_RADIUS: f32 = 4.;
pub const MENU_ROW_H: f32 = crate::ui::MENU_ROW_H;
/// Dropdown menus look like the app's (`ui::menu`): 6 above and below the
/// rows, each row 5 in from the sides, 13pt, radius 12 / 6, a quiet hover.
pub const MENU_PAD_Y: f32 = crate::ui::MENU_PAD_Y;
pub const MENU_INSET: f32 = crate::ui::MENU_INSET;

/// A dropdown menu's panel: the app's menu shell.
pub fn menu() -> Div { crate::ui::popover() }

pub fn menu_row(id: impl Into<ElementId>, group: SharedString) -> Stateful<Div> { crate::ui::menu_row(id, group) }
pub fn menu_label(label: impl Into<SharedString>, _group: SharedString, _hover: Hsla) -> Div {
    div().whitespace_nowrap().child(crate::settings::label(label.into()))
}
pub fn menu_glyph(name: &str, size: f32, _group: SharedString) -> Svg { icon(name, size, Theme::icon()) }
