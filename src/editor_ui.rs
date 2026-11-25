pub use crate::ui::*;
use gpui::*;
pub fn icon(name: &str, size: f32, color: Hsla) -> Svg { svg().path(SharedString::from(format!("editor-icons/{name}.svg"))).size(px(size)).flex_none().text_color(color) }
pub const ROW_RADIUS: f32 = 4.;
