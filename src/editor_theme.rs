pub use crate::theme::{craft, c, ca, rgbaf, ta, TITLE, UI_FONT, MONO_FONT, BlockMetrics};
/// The active theme's resolved style (themes.rs).
pub fn style() -> &'static crate::themes::Style { crate::themes::active() }
#[cfg(target_os = "ios")]
unsafe extern "C" { fn fei_ios_type_scale() -> f32; }
fn mobile_metric(key: &str, value: f32, floor: f32) -> f32 {
    #[cfg(target_os = "ios")]
    { style().num_override(key).unwrap_or(value.max(floor)) * (unsafe { fei_ios_type_scale() }).clamp(0.8, 3.5) }
    #[cfg(not(target_os = "ios"))]
    { let _ = (key,floor); value }
}
pub fn body_size() -> f32 { mobile_metric("mobile.page.size",style().num("page.size"),17.) }
pub fn table_size() -> f32 { mobile_metric("mobile.table.size",style().num("table.size"),16.) }
pub fn table_line() -> f32 { mobile_metric("mobile.table.line",style().num("table.line-height"),24.) }
pub fn code_size() -> f32 { mobile_metric("mobile.code.size",style().num("pre.size"),15.) }
pub fn code_line() -> f32 { mobile_metric("mobile.code.line",style().num("pre.line"),22.) }
pub fn body_line() -> f32 { mobile_metric("mobile.page.line",style().num("page.line"),26.) }
pub fn block_gap() -> f32 { style().num("p.gap") }
fn h(level: u8) -> u8 { level.clamp(1, 9) }
pub fn heading_metrics(level: u8) -> BlockMetrics {
    let (s, l) = (style(), h(level));
    BlockMetrics { size: mobile_metric("",s.num(&format!("h{l}.size")),if l == 1 { 22. } else if l == 2 { 20. } else { 17. }), line_height: mobile_metric("",s.num(&format!("h{l}.line")),if l <= 2 { 28. } else { 24. }), mt: s.num(&format!("h{l}.mt")), mb: s.num(&format!("h{l}.mb")) }
}
pub fn heading_weight(level: u8) -> gpui::FontWeight { gpui::FontWeight(style().num(&format!("h{}.weight", h(level)))) }
pub fn heading_font(level: u8) -> &'static str { style().font(&format!("h{}.font", h(level))) }
pub fn heading_color(level: u8) -> Option<gpui::Hsla> { style().color(&format!("h{}.color", h(level))) }
/// The page title.
pub fn title() -> BlockMetrics { BlockMetrics { size: mobile_metric("mobile.title.size",style().num("title.size"),26.), line_height: mobile_metric("mobile.title.line",style().num("title.line"),32.), mt: 0., mb: 0. } }
/// Document text's family (UI fonts stay the system's).
pub fn body_font() -> &'static str { style().font("page.font") }
/// Document colour tokens: a theme file may override any of them by name
/// (docs/THEMES.md); otherwise they come from the base style.
macro_rules! tokens {
    ($($n:ident),* $(,)?) => {
        #[allow(non_camel_case_types)]
        #[derive(Clone, Copy)]
        enum Tok { $($n),* }
        /// Names a theme file uses for the colour tokens, in index order.
        pub const TOKENS: &[&str] = &[$(stringify!($n)),*];
        /// The base style's value of token `i`.
        pub fn base_color(i: usize, feishu: bool) -> gpui::Hsla {
            const ALL: &[(fn() -> gpui::Hsla, fn() -> gpui::Hsla)] = &[$((crate::feishu_theme::Theme::$n, crate::theme::Theme::$n)),*];
            if feishu { (ALL[i].0)() } else { (ALL[i].1)() }
        }
        pub struct Theme;
        impl Theme {
            $(pub fn $n() -> gpui::Hsla { crate::themes::color(Tok::$n as usize).unwrap_or_else(|| base_color(Tok::$n as usize, crate::settings::feishu())) })*
        }
    };
}
tokens!(
    text, text_caption, text_placeholder, block_placeholder, text_disabled, text_code,
    link, link_hover, primary, primary_500, primary_fill_1, bg_body,
    bg_overlay, line_divider, line_border, fill_hover, fill_hover_strong, fill_pressed,
    fill_turninto_active, selection, block_hover, block_selected, handle_active, size_cell,
    size_cell_border, size_cell_active, size_cell_disabled, swatch_border, block_bg_area, todo_done,
    scrollbar_thumb, shadow, code_hover_ring, quote_bar, caret, pinned,
    table_header, table_line, table_bar_hover, table_bar_selected, danger_bar, danger_cell,
    danger, window, sidebar_text, sidebar_hover, sidebar_selected, sidebar_section,
    button_border, code_chip, code_chip_border, code_bg, code_gutter, code_gutter_line,
    code_border, code_line_number, caption, menu_highlight, popover_bg, separator_dots,
    todo_box, code_text,
);
fn hue(name: &str) -> &str { name.strip_prefix("light-").or_else(|| name.strip_prefix("medium-")).unwrap_or(name) }
pub fn text_color(name: &str) -> Option<gpui::Hsla> { style().palette("color", name).or_else(|| if crate::settings::feishu() { crate::feishu_theme::text_color(name) } else { crate::theme::text_color(name) }) }
pub fn bg_color(name: &str) -> Option<gpui::Hsla> { style().palette("highlight", name).or_else(|| if crate::settings::feishu() { crate::feishu_theme::bg_color(name) } else { crate::theme::bg_color(name) }) }

/// Imported highlights can themselves be dark. Keep their foreground when
/// legible instead of applying the named palette's light-paper assumption.
pub fn marked_text(color: gpui::Hsla, marks: &crate::model::Marks, background: gpui::Hsla) -> gpui::Hsla {
    if marks.bg_color.as_ref().and_then(|c| crate::palette::literal(&c.0)).is_none() { return crate::theme::on_mark(color); }
    let foreground = marks.text_color.as_ref().and_then(|c| crate::palette::literal(&c.0))
        .map(|c| crate::theme::rgbaf(c.r, c.g, c.b, c.a)).unwrap_or_else(|| crate::theme::on_mark(color));
    let blend = |top: gpui::Rgba, bottom: gpui::Rgba| gpui::Rgba { r: top.r * top.a + bottom.r * (1. - top.a), g: top.g * top.a + bottom.g * (1. - top.a), b: top.b * top.a + bottom.b * (1. - top.a), a: 1. };
    let bg = blend(background.into(), Theme::bg_body().into());
    let luminance = |c: gpui::Rgba| { let f = |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }; 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b) };
    let (b, f) = (luminance(bg), luminance(blend(foreground.into(), bg)));
    if (b.max(f) + 0.05) / (b.min(f) + 0.05) >= 4.5 { foreground }
    else if (1.05 / (b + 0.05)) > ((b + 0.05) / 0.05) { gpui::white() } else { gpui::black() }
}
pub fn block_bg(name: &str) -> Option<gpui::Hsla> { style().palette("background", name).or_else(|| if crate::settings::feishu() { crate::feishu_theme::block_bg(name) } else { crate::theme::block_bg(name) }) }
pub fn bg_swatch(name: &str) -> Option<gpui::Hsla> { style().palette("background", name).or_else(|| if crate::settings::feishu() { crate::feishu_theme::bg_swatch(name) } else { crate::theme::bg_swatch(name) }) }
pub fn callout_bg(name: &str) -> Option<gpui::Hsla> { style().palette("callout", name).or_else(|| if crate::settings::feishu() { crate::feishu_theme::callout_bg(name) } else { crate::theme::callout_bg(name) }) }
pub fn callout_border(name: &str) -> Option<gpui::Hsla> { style().palette("border", hue(name)).or_else(|| if crate::settings::feishu() { crate::feishu_theme::callout_border(name) } else { crate::theme::callout_border(name) }) }
pub fn callout_border_swatch(name: &str) -> Option<gpui::Hsla> { style().palette("border", hue(name)).or_else(|| if crate::settings::feishu() { crate::feishu_theme::callout_border_swatch(name) } else { crate::theme::callout_border_swatch(name) }) }
pub use crate::theme::{shadow_button, shadow_menu, shadow_panel, shadow_dialog};

pub fn mono_font() -> &'static str { style().font("pre.font") }

impl Theme {
    /// Keep labels readable on both dark accents and the Notes gold fill.
    pub fn on_primary() -> gpui::Hsla {
        let c = gpui::Rgba::from(Self::primary());
        let linear = |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
        if 0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b) > 0.179 { gpui::black() } else { gpui::white() }
    }
    pub fn icon() -> gpui::Hsla { if crate::theme::dark() { Self::text_code() } else { crate::theme::Theme::icon() } }
}
