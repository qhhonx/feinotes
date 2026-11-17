//! API differences between desktop GPUI and the iPad backend.
use gpui::{App, FocusHandle, Window};

pub fn focus(window: &mut Window, handle: &FocusHandle, _cx: &mut App) {
    #[cfg(target_os = "ios")]
    window.focus(handle, _cx);
    #[cfg(not(target_os = "ios"))]
    window.focus(handle);
}

#[cfg(target_os = "ios")]
pub const NO_CURSOR: gpui::CursorStyle = gpui::CursorStyle::Arrow;
#[cfg(not(target_os = "ios"))]
pub const NO_CURSOR: gpui::CursorStyle = gpui::CursorStyle::None;

pub fn paint_line(line: &gpui::ShapedLine, origin: gpui::Point<gpui::Pixels>, height: gpui::Pixels,
    window: &mut Window, cx: &mut App) -> anyhow::Result<()> {
    #[cfg(target_os = "ios")]
    return line.paint(origin, height, gpui::TextAlign::Left, None, window, cx);
    #[cfg(not(target_os = "ios"))]
    line.paint(origin, height, window, cx)
}

pub fn paint_svg(window: &mut Window, bounds: gpui::Bounds<gpui::Pixels>, path: gpui::SharedString,
    transform: gpui::TransformationMatrix, color: gpui::Hsla, cx: &App) -> anyhow::Result<()> {
    #[cfg(target_os = "ios")]
    return window.paint_svg(bounds, path, None, transform, color, cx);
    #[cfg(not(target_os = "ios"))]
    window.paint_svg(bounds, path, transform, color, cx)
}

pub fn paint_shadows(window: &mut Window, bounds: gpui::Bounds<gpui::Pixels>,
    corners: gpui::Corners<gpui::Pixels>, shadows: &[gpui::BoxShadow]) {
    #[cfg(target_os = "ios")]
    window.paint_drop_shadows(bounds, corners, shadows);
    #[cfg(not(target_os = "ios"))]
    window.paint_shadows(bounds, corners, shadows);
}

pub fn scroll_max_y(list: &gpui::ScrollHandle) -> gpui::Pixels {
    #[cfg(target_os = "ios")]
    return list.max_offset().y;
    #[cfg(not(target_os = "ios"))]
    list.max_offset().height
}

pub fn track_scroll(list: gpui::UniformList, scroll: &gpui::UniformListScrollHandle) -> gpui::UniformList {
    #[cfg(target_os = "ios")]
    return list.track_scroll(scroll);
    #[cfg(not(target_os = "ios"))]
    list.track_scroll(scroll.clone())
}

pub fn grid_columns(grid: gpui::Div, widths: Vec<f32>) -> gpui::Div {
    use gpui::Styled;
    #[cfg(target_os = "ios")]
    {
        use gpui::*;
        return grid.grid_cols_max_content(widths.len() as u16).children(widths.into_iter().enumerate().map(|(column,width)| {
            div().col_start(column as i16 + 1).row_start(1).w(px(width)).min_w(px(width)).max_w(px(width)).h_0()
        }));
    }
    #[cfg(not(target_os = "ios"))]
    grid.grid_col_widths(widths)
}

pub fn secondary(modifiers: gpui::Modifiers, _cx: &App) -> bool {
    #[cfg(target_os = "ios")]
    return modifiers.secondary(_cx);
    #[cfg(not(target_os = "ios"))]
    modifiers.secondary()
}


#[cfg(feature = "cloudkit-sync")]
pub fn async_update<R>(cx: &gpui::AsyncApp, update: impl FnOnce(&mut App) -> R) -> anyhow::Result<R> {
    #[cfg(target_os = "ios")]
    return Ok(cx.update(update));
    #[cfg(not(target_os = "ios"))]
    cx.update(update)
}

pub fn flex_shrink<T: gpui::Styled>(view: T) -> T {
    #[cfg(target_os = "ios")]
    { view.flex_shrink(1.) }
    #[cfg(not(target_os = "ios"))]
    { view.flex_shrink() }
}


pub fn max_scroll_x(handle: &gpui::ScrollHandle) -> gpui::Pixels {
    #[cfg(target_os = "ios")]
    return handle.max_offset().x;
    #[cfg(not(target_os = "ios"))]
    handle.max_offset().width
}


pub fn paint_image(window: &mut Window, bounds: gpui::Bounds<gpui::Pixels>, corners: gpui::Corners<gpui::Pixels>,
    image: std::sync::Arc<gpui::RenderImage>, frame: usize, grayscale: bool) -> anyhow::Result<()> {
    #[cfg(target_os = "ios")]
    return window.paint_image(bounds,bounds,corners,image,frame,grayscale);
    #[cfg(not(target_os = "ios"))]
    window.paint_image(bounds,corners,image,frame,grayscale)
}
