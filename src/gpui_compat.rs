//! API differences between desktop GPUI and the iPad backend.
use gpui::{App, FocusHandle, Window};

pub fn focus(window: &mut Window, handle: &FocusHandle, _cx: &mut App) {
    window.focus(handle, _cx);
}


pub fn paint_line(line: &gpui::ShapedLine, origin: gpui::Point<gpui::Pixels>, height: gpui::Pixels,
    window: &mut Window, cx: &mut App) -> anyhow::Result<()> {
    line.paint(origin, height, gpui::TextAlign::Left, None, window, cx)
}

pub fn paint_svg(window: &mut Window, bounds: gpui::Bounds<gpui::Pixels>, path: gpui::SharedString,
    transform: gpui::TransformationMatrix, color: gpui::Hsla, cx: &App) -> anyhow::Result<()> {
    window.paint_svg(bounds, path, None, transform, color, cx)
}

pub fn paint_shadows(window: &mut Window, bounds: gpui::Bounds<gpui::Pixels>,
    corners: gpui::Corners<gpui::Pixels>, shadows: &[gpui::BoxShadow]) {
    window.paint_drop_shadows(bounds, corners, shadows);
}

pub fn scroll_max_y(list: &gpui::ScrollHandle) -> gpui::Pixels {
    list.max_offset().y
}

pub fn track_scroll(list: gpui::UniformList, scroll: &gpui::UniformListScrollHandle) -> gpui::UniformList {
    list.track_scroll(scroll)
}

pub fn grid_columns(grid: gpui::Div, widths: Vec<f32>) -> gpui::Div {
    #[cfg(not(target_os = "ios"))]
    use gpui::Styled;
    #[cfg(target_os = "ios")]
    {
        use gpui::*;
        grid.grid_cols_max_content(widths.len() as u16).children(widths.into_iter().enumerate().map(|(column,width)| {
            div().col_start(column as i16 + 1).row_start(1).w(px(width)).min_w(px(width)).max_w(px(width)).h_0()
        }))
    }
    #[cfg(not(target_os = "ios"))]
    { grid.grid_col_widths(widths) }
}

pub fn secondary(modifiers: gpui::Modifiers, _cx: &App) -> bool {
    modifiers.secondary(_cx)
}


#[cfg(feature = "cloudkit-sync")]
pub fn async_update<R>(cx: &gpui::AsyncApp, update: impl FnOnce(&mut App) -> R) -> anyhow::Result<R> {
    Ok(cx.update(update))
}

pub fn flex_shrink<T: gpui::Styled>(view: T) -> T {
    view.flex_shrink(1.)
}


pub fn max_scroll_x(handle: &gpui::ScrollHandle) -> gpui::Pixels {
    handle.max_offset().x
}


pub fn paint_image(window: &mut Window, bounds: gpui::Bounds<gpui::Pixels>, corners: gpui::Corners<gpui::Pixels>,
    image: std::sync::Arc<gpui::RenderImage>, frame: usize, grayscale: bool) -> anyhow::Result<()> {
    window.paint_image(bounds,bounds,corners,image,frame,grayscale)
}

pub fn hide_cursor(cx: &mut App) {
    #[cfg(not(target_os = "ios"))]
    cx.hide_cursor_until_mouse_moves();
    #[cfg(target_os = "ios")]
    let _ = cx;
}
