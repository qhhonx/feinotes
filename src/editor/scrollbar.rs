//! The page scrollbar: a thin trackless thumb on the right edge, drawn over
//! the virtualized list after it lays out, so it never lags a frame behind.

use super::view::Editor;
use crate::feishu_theme::{self as theme, Theme, craft};
use gpui::*;
use std::cell::Cell;

const W: f32 = 4.;
const W_HOT: f32 = 6.;
const INSET: f32 = 2.;
const MIN_H: f32 = 32.;
/// The grab zone is wider than the thumb.
const HIT_W: f32 = 12.;

#[derive(Default)]
pub struct Scrollbar {
    /// The pointer was over the thumb last frame.
    hot: Cell<bool>,
    /// While dragging: pointer offset into the thumb, and the content and
    /// thumb heights at the press (held still so the thumb stays under the
    /// pointer as newly measured blocks refine the estimate).
    grab: Cell<Option<(Pixels, Pixels, Pixels)>>,
}

impl Editor {
    pub(super) fn page_scrollbar(&self) -> impl IntoElement {
        let list = self.list.clone();
        let sb = self.scrollbar.clone();
        let sb2 = sb.clone();
        let list2 = list.clone();
        canvas(
            move |bounds, window, _| {
                let (offset, content, view) = list.estimated_scroll_metrics();
                let (content, h) = match sb.grab.get() {
                    Some((_, content, h)) => (content, h),
                    None => (content, (view * (view / content)).max(px(MIN_H)).min(bounds.size.height)),
                };
                if content <= view + px(1.) {
                    return None;
                }
                let travel = bounds.size.height - h;
                let top = bounds.top() + travel * (offset / (content - view)).clamp(0., 1.);
                let zone = Bounds::new(point(bounds.right() - px(HIT_W), top), size(px(HIT_W), h));
                Some((top, h, content, view, window.insert_hitbox(zone, HitboxBehavior::BlockMouseExceptScroll)))
            },
            move |bounds, state, window, _| {
                let sb = sb2;
                let Some((top, h, content, view, hitbox)) = state else {
                    sb.grab.set(None);
                    return;
                };
                window.set_cursor_style(CursorStyle::Arrow, &hitbox);
                let hovered = hitbox.is_hovered(window);
                sb.hot.set(hovered);
                let dragging = sb.grab.get().is_some();
                let w = if hovered || dragging { W_HOT } else { W };
                // Black at 22%, darker under the pointer and while held.
                let color = if dragging {
                    theme::ca(craft::N1000, 0.4)
                } else if hovered {
                    theme::ca(craft::N1000, 0.32)
                } else {
                    Theme::scrollbar_thumb()
                };
                let thumb = Bounds::new(point(bounds.right() - px(INSET + w), top), size(px(w), h));
                window.paint_quad(fill(thumb, color).corner_radii(px(w / 2.)));

                window.on_mouse_event({
                    let (sb, hitbox) = (sb.clone(), hitbox.clone());
                    move |ev: &MouseDownEvent, phase, window, cx| {
                        if phase == DispatchPhase::Bubble && ev.button == MouseButton::Left && hitbox.is_hovered(window) {
                            sb.grab.set(Some((ev.position.y - top, content, h)));
                            cx.stop_propagation();
                            window.refresh();
                        }
                    }
                });
                window.on_mouse_event({
                    let (sb, hitbox, list) = (sb.clone(), hitbox.clone(), list2.clone());
                    move |ev: &MouseMoveEvent, phase, window, _| {
                        if phase != DispatchPhase::Bubble {
                            return;
                        }
                        match sb.grab.get() {
                            Some((dy, _, h)) if ev.pressed_button == Some(MouseButton::Left) => {
                                let travel = (bounds.size.height - h).max(px(1.));
                                let f = ((ev.position.y - dy - bounds.top()) / travel).clamp(0., 1.);
                                // At the end, go to the real bottom: the rows there may not
                                // be measured yet, so the estimate can stop short.
                                list.scroll_to_estimated_offset(if f >= 1. { px(f32::MAX) } else { (content - view) * f });
                                window.refresh();
                            }
                            // The button went up somewhere we didn't see.
                            Some(_) => {
                                sb.grab.set(None);
                                window.refresh();
                            }
                            None if hitbox.is_hovered(window) != sb.hot.get() => window.refresh(),
                            None => {}
                        }
                    }
                });
                window.on_mouse_event({
                    let sb = sb.clone();
                    move |ev: &MouseUpEvent, phase, window, _| {
                        if phase == DispatchPhase::Bubble && ev.button == MouseButton::Left && sb.grab.take().is_some() {
                            window.refresh();
                        }
                    }
                });
            },
        )
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .w(px(HIT_W))
    }
}

/// A wide table's scrollbar: the page thumb turned sideways, along the top
/// of its box (`HIT_W` tall), for the scroller that `handle` tracks.
pub(super) fn table_scrollbar(handle: ScrollHandle, sb: std::rc::Rc<Scrollbar>) -> Canvas<Option<(Pixels, Pixels, Hitbox)>> {
    let sb2 = sb.clone();
    let list = handle.clone();
    canvas(
        move |bounds, window, _| {
            let max = list.max_offset().width;
            if max <= px(1.) {
                return None;
            }
            let view = bounds.size.width;
            let content = view + max;
            let w = match sb.grab.get() {
                Some((_, _, w)) => w,
                None => (view * (view / content)).max(px(MIN_H)).min(view),
            };
            let left = bounds.left() + (view - w) * (-list.offset().x / max).clamp(0., 1.);
            let zone = Bounds::new(point(left, bounds.top()), size(w, px(HIT_W)));
            Some((left, w, window.insert_hitbox(zone, HitboxBehavior::BlockMouseExceptScroll)))
        },
        move |bounds, state, window, _| {
            let sb = sb2;
            let Some((left, w, hitbox)) = state else {
                sb.grab.set(None);
                return;
            };
            window.set_cursor_style(CursorStyle::Arrow, &hitbox);
            let hovered = hitbox.is_hovered(window);
            sb.hot.set(hovered);
            let dragging = sb.grab.get().is_some();
            let h = if hovered || dragging { W_HOT } else { W };
            let color = if dragging {
                theme::ca(craft::N1000, 0.4)
            } else if hovered {
                theme::ca(craft::N1000, 0.32)
            } else {
                Theme::scrollbar_thumb()
            };
            let thumb = Bounds::new(point(left, bounds.top() + px(INSET)), size(w, px(h)));
            window.paint_quad(fill(thumb, color).corner_radii(px(h / 2.)));

            window.on_mouse_event({
                let (sb, hitbox) = (sb.clone(), hitbox.clone());
                move |ev: &MouseDownEvent, phase, window, cx| {
                    if phase == DispatchPhase::Bubble && ev.button == MouseButton::Left && hitbox.is_hovered(window) {
                        sb.grab.set(Some((ev.position.x - left, px(0.), w)));
                        cx.stop_propagation();
                        window.refresh();
                    }
                }
            });
            window.on_mouse_event({
                let (sb, hitbox, list) = (sb.clone(), hitbox.clone(), handle.clone());
                move |ev: &MouseMoveEvent, phase, window, _| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    match sb.grab.get() {
                        Some((dx, _, w)) if ev.pressed_button == Some(MouseButton::Left) => {
                            let travel = (bounds.size.width - w).max(px(1.));
                            let f = ((ev.position.x - dx - bounds.left()) / travel).clamp(0., 1.);
                            list.set_offset(point(-list.max_offset().width * f, list.offset().y));
                            window.refresh();
                        }
                        Some(_) => {
                            sb.grab.set(None);
                            window.refresh();
                        }
                        None if hitbox.is_hovered(window) != sb.hot.get() => window.refresh(),
                        None => {}
                    }
                }
            });
            window.on_mouse_event({
                let sb = sb.clone();
                move |ev: &MouseUpEvent, phase, window, _| {
                    if phase == DispatchPhase::Bubble && ev.button == MouseButton::Left && sb.grab.take().is_some() {
                        window.refresh();
                    }
                }
            });
        },
    )
}
