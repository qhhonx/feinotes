//! Excalidraw's UI design tokens, after `packages/excalidraw/css/theme.scss`
//! (`.excalidraw` and `.excalidraw.theme--dark`), `variables.module.scss` and
//! `styles.scss` @ 3a317b5. The board's chrome uses these, not our `Theme`:
//! one function per CSS variable, light or dark by the app's look (upstream's
//! UI isn't filtered in dark mode; only colors shown as scene colors, like
//! swatches, go through the canvas mapping). Sizes are px at upstream's 16px
//! rem (`--default-button-size: 2rem` = 32 …); the ≥1921px-wide variants are
//! left out.

use gpui::{point, px, BoxShadow, Hsla, Rgba};

fn dark() -> bool { crate::theme::dark() }
pub fn hex(c: u32) -> Hsla { Rgba { r: (c >> 16 & 0xff) as f32 / 255., g: (c >> 8 & 0xff) as f32 / 255., b: (c & 0xff) as f32 / 255., a: 1. }.into() }
pub fn hexa(c: u32, a: f32) -> Hsla { let mut h = hex(c); h.a = a; h }
fn pick(light: u32, dark_: u32) -> Hsla { hex(if dark() { dark_ } else { light }) }
fn hsl(h: f32, s: f32, l: f32) -> Hsla { gpui::hsla(h / 360., s / 100., l / 100., 1.) }

/// `--ui-font` (agent B registers the bundled Assistant under this name; GPUI
/// falls back to the system face until then).
pub const UI_FONT: &str = "Assistant";

// ---- sizes (`theme.scss`, `styles.scss`)
pub const SPACE: f32 = 4.; // --space-factor: 0.25rem
pub const DEFAULT_BUTTON: f32 = 32.; // --default-button-size
pub const LG_BUTTON: f32 = 36.; // --lg-button-size
pub const ICON: f32 = 16.; // --default-icon-size, --lg-icon-size
pub const RADIUS_LG: f32 = 8.; // --border-radius-lg
pub const RADIUS_MD: f32 = 6.; // --border-radius-md
pub const PADDING: f32 = 16.; // --editor-container-padding
/// Above the top row (menu, toolbar): ours, as the board sits under the
/// app's header, which already leaves room (upstream: `PADDING`).
pub const TOP_PADDING: f32 = 8.;
/// `.excalidraw button { font-size: 0.8333rem }`.
pub const BUTTON_TEXT: f32 = 13.333;
pub const LABEL_TEXT: f32 = 12.; // 0.75rem
/// CSS `line-height: normal` for Assistant (hhea 1021 / -287, no gap) as
/// Chrome computes it: ascent and descent rounded separately. GPUI's default
/// (1.618em) would make every text box taller than upstream's.
pub fn lh(size: f32) -> gpui::Pixels { px((size * 1.021).round() + (size * 0.287).round()) }

// ---- colors (`.excalidraw` / `.theme--dark`)
pub fn primary() -> Hsla { pick(0x6965db, 0xa8a5ff) }
pub fn primary_darker() -> Hsla { pick(0x5b57d1, 0xb2aeff) }
pub fn primary_darkest() -> Hsla { pick(0x4a47b1, 0xbeb9ff) }
pub fn primary_light() -> Hsla { pick(0xe3e2fe, 0x4f4d6f) }
pub fn island_bg() -> Hsla { pick(0xffffff, 0x232329) }
pub fn popup_bg() -> Hsla { island_bg() }
pub fn surface_high() -> Hsla { pick(0xf1f0ff, 0x2e2d39) }
pub fn surface_mid() -> Hsla { if dark() { hsl(240., 6., 10.) } else { hex(0xf6f6f9) } }
pub fn surface_low() -> Hsla { if dark() { hsl(240., 8., 15.) } else { hex(0xececf4) } }
pub fn surface_lowest() -> Hsla { if dark() { hsl(0., 0., 7.) } else { hex(0xffffff) } }
pub fn on_surface() -> Hsla { pick(0x1b1b1f, 0xe3e3e8) }
pub fn brand_active() -> Hsla { pick(0x4440bf, 0xd0ccff) }
pub fn on_primary_container() -> Hsla { pick(0x030064, 0xe0dfff) }
pub fn surface_primary_container() -> Hsla { pick(0xe0dfff, 0x403e6a) }
/// `--text-primary-color`, `--icon-fill-color`.
pub fn text() -> Hsla { on_surface() }
/// `--default-border-color`, `--button-hover-bg`, `--button-active-bg`.
pub fn border() -> Hsla { surface_high() }
pub fn button_hover_bg() -> Hsla { surface_high() }
/// `--button-active-border`.
pub fn button_active_border() -> Hsla { brand_active() }
pub fn disabled() -> Hsla { pick(GRAY_40, GRAY_70) }
pub fn keybinding() -> Hsla { pick(GRAY_40, GRAY_60) }
pub fn button_gray_2() -> Hsla { pick(0xced4da, 0x272727) }
pub fn button_gray_3() -> Hsla { pick(0xadb5bd, 0x222222) }
pub fn dialog_border() -> Hsla { pick(GRAY_20, GRAY_80) }
/// `--list-border-color` (`QuickSearch.scss`; dark draws no border there).
pub fn list_border() -> Hsla { if dark() { gpui::transparent_black() } else { hex(GRAY_20) } }
pub fn overlay_bg() -> Hsla { hexa(0x0c0c0e, if dark() { 0.65 } else { 0.35 }) }
pub fn input_label() -> Hsla { pick(0x495057, 0xe9ecef) }
pub fn slider_track() -> Hsla { if dark() { hsl(244., 23., 39.) } else { hsl(240., 100., 90.) } }
pub fn slider_thumb() -> Hsla { hex(GRAY_80) }
/// `ColorPicker.scss` `--color-outline`.
pub fn color_outline() -> Hsla { pick(GRAY_20, GRAY_80) }
/// The styles panel's buttons (`.App-menu__left`, `.shape-actions-theme-scope`).
pub fn panel_button_bg() -> Hsla { if dark() { surface_high() } else { surface_mid() } }
pub fn panel_button_hover_bg() -> Hsla { if dark() { hex(0x363541) } else { surface_high() } }
pub fn hint_text() -> Hsla { pick(GRAY_40, GRAY_60) }
/// `--color-gray-*`.
pub const GRAY_20: u32 = 0xebebeb;
pub const GRAY_40: u32 = 0xb8b8b8;
pub const GRAY_60: u32 = 0x7a7a7a;
pub const GRAY_70: u32 = 0x5c5c5c;
pub const GRAY_80: u32 = 0x3d3d3d;
/// `--color-selection`: the interactive canvas's selection color.
pub fn selection() -> Hsla { pick(0x6965db, 0xb4b0ff) }

// ---- shadows
fn sh(y: f32, blur: f32, a: f32) -> BoxShadow { BoxShadow { color: gpui::hsla(0., 0., 0., a), offset: point(px(0.), px(y)), blur_radius: px(blur), spread_radius: px(0.), inset: false } }
/// `--shadow-island`.
pub fn shadow_island() -> Vec<BoxShadow> { vec![sh(0., 1., 0.17), sh(0., 3., 0.08), sh(7., 14., 0.05)] }
/// `--shadow-island-stronger`.
pub fn shadow_island_stronger() -> Vec<BoxShadow> { vec![sh(0., 1., 0.17), sh(0., 3., 0.08), sh(7., 14., 0.18)] }
/// `--modal-shadow`.
pub fn shadow_modal() -> Vec<BoxShadow> {
    vec![sh(100., 80., 0.07), sh(41.78, 33.42, 0.0503), sh(22.34, 17.87, 0.0417), sh(12.52, 10.02, 0.035), sh(6.65, 5.32, 0.0283), sh(2.77, 2.21, 0.0197)]
}
/// A 1px ring (`box-shadow: 0 0 0 1px …`).
pub fn ring(c: Hsla) -> Vec<BoxShadow> { vec![BoxShadow { color: c, offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(1.), inset: false }] }
