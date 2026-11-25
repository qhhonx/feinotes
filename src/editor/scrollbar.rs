//! The page scrollbar: a thin trackless thumb on the right edge, drawn over
//! the virtualized list after it lays out, so it never lags a frame behind.

use super::view::Editor;

use gpui::*;
use std::cell::Cell;
use std::time::{Duration, Instant};

// The shared look (`design::scroll`, shadcn's ScrollArea): a 7pt `border` thumb 1pt in, shown while
// the pointer is on the bar, while the page scrolls and while held, fading out after.
use crate::design::scroll::{HELD, HOVER, PAD as INSET, THUMB as W, WINDOW_R, ends_for, thumb_tone};
const MIN_H: f32 = 32.;
/// The grab zone is wider than the thumb.
const HIT_W: f32 = 12.;

#[derive(Default)]
pub struct Scrollbar {
    /// The pointer was over the thumb last frame.
    hot: Cell<bool>,
    /// The page thumb while dragged.
    held: Cell<Option<Grab>>,
    /// A wide table's thumb while dragged: pointer offset into the thumb, and
    /// its width (the middle is unused).
    grab: Cell<Option<(Pixels, Pixels, Pixels)>>,
    fade: crate::design::scroll::Fade,
    tone: crate::design::scroll::Tone,
    /// The pointer was over the table last paint (a wide table's bar).
    over: Cell<bool>,
}

/// A thumb being dragged. It stays under the pointer, and the page follows
/// it: unmeasured rows count at the average height, so the estimated length
/// changes as they come on screen, and the page is aimed again until it sits
/// where the thumb says. (Holding the length from the press instead, a first
/// screen of tall blocks put the end of the page a third down the track, and
/// the thumb would go no further.) Each move already aims by the latest
/// estimate; aiming again lays out rows nobody sees, so it waits until the
/// pointer rests, or the button is let go (the thumb is held where it is
/// until the page is there).
#[derive(Clone, Copy)]
struct Grab {
    /// Pointer offset into the thumb.
    dy: Pixels,
    /// Thumb height at the press.
    h: Pixels,
    /// Where the thumb is, 0 (top) to 1 (end).
    f: f32,
    /// Re-aims left before the estimate is taken as it is.
    tries: u8,
    /// The last move.
    moved: Instant,
    /// The button is still down (else the page is settling after it).
    down: bool,
}

const TRIES: u8 = 8;
/// How far beyond the window the page lays out rows (`ListState::new`): none
/// while the thumb is held, where every frame lands somewhere new and rows
/// nobody sees were most of the frame.
pub(super) const OVERDRAW: f32 = 600.;
/// How long the pointer rests before the page is aimed again.
const REST: Duration = Duration::from_millis(80);

/// The thumb's top within the track and its height, for the scroll `offset`
/// in `content` seen through `view` on a `track` this tall; `None` when it
/// all fits. A held thumb is where it was dragged to.
fn thumb(offset: Pixels, content: Pixels, view: Pixels, track: Pixels, grab: Option<Grab>) -> Option<(Pixels, Pixels)> {
    if content <= view + px(1.) {
        return None;
    }
    let h = grab.map_or_else(|| (view * (view / content)).max(px(MIN_H)).min(track), |g| g.h);
    let f = grab.map_or_else(|| (offset / (content - view)).clamp(0., 1.), |g| g.f);
    Some(((track - h) * f, h))
}

/// Where a thumb at `f` puts the page in `content`: the end is the real end,
/// which the estimate may put short of it.
fn aim(f: f32, content: Pixels, view: Pixels) -> Pixels {
    if f >= 1. { px(f32::MAX) } else { (content - view) * f }
}

/// Whether the page has drifted from a held thumb (the estimate changed).
fn drifted(g: &Grab, offset: Pixels, content: Pixels, view: Pixels) -> bool {
    g.f < 1. && g.tries > 0 && (offset - aim(g.f, content, view)).abs() > px(0.5)
}

/// Where to aim the page again at `now`, if anywhere: not while the pointer
/// is moving (its moves aim the page).
fn reaim(g: &Grab, offset: Pixels, content: Pixels, view: Pixels, now: Instant) -> Option<Pixels> {
    (drifted(g, offset, content, view) && (!g.down || now.duration_since(g.moved) >= REST)).then(|| aim(g.f, content, view))
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
                let mut grab = sb.held.get();
                if let Some(g) = grab {
                    if let Some(to) = reaim(&g, offset, content, view, Instant::now()) {
                        sb.held.set(Some(Grab { tries: g.tries - 1, ..g }));
                        list.scroll_to_estimated_offset(to);
                        window.refresh();
                    } else if !g.down {
                        // Let go and settled: the estimate has the thumb again.
                        sb.held.set(None);
                        list.set_overdraw(px(OVERDRAW));
                        grab = None;
                    }
                }
                // under the top bar a straight edge; at the bottom the page runs into the window's corner
                let (ends, ends_b) = (px(INSET), px(ends_for(WINDOW_R)));
                let track = (bounds.size.height - ends - ends_b).max(px(1.));
                let (y, h) = thumb(offset, content, view, track, grab)?;
                let top = bounds.top() + ends + y;
                let f = if track > h { y / (track - h) } else { 0. };
                let zone = Bounds::new(point(bounds.right() - px(HIT_W), top), size(px(HIT_W), h));
                Some((top, h, f, window.insert_hitbox(zone, HitboxBehavior::BlockMouseExceptScroll)))
            },
            move |bounds, state, window, cx| {
                let sb = sb2;
                let Some((top, h, f, hitbox)) = state else {
                    if sb.held.take().is_some() {
                        list2.set_overdraw(px(OVERDRAW));
                    }
                    return;
                };
                window.set_cursor_style(CursorStyle::Arrow, &hitbox);
                let hovered = hitbox.is_hovered(window);
                sb.hot.set(hovered);
                let dragging = sb.held.get().is_some_and(|g| g.down);
                let a = sb.fade.step(hovered || dragging, f32::from(top), window, cx);
                let tone = sb.tone.step(if dragging { HELD } else if hovered { HOVER } else { 0. }, window, cx);
                let thumb = Bounds::new(point(bounds.right() - px(INSET + W), top), size(px(W), h));
                if a > 0. { window.paint_quad(fill(thumb, thumb_tone(tone, a)).corner_radii(px(W / 2.))); }

                window.on_mouse_event({
                    let (sb, hitbox, list) = (sb.clone(), hitbox.clone(), list2.clone());
                    move |ev: &MouseDownEvent, phase, window, cx| {
                        if phase == DispatchPhase::Bubble && ev.button == MouseButton::Left && hitbox.is_hovered(window) {
                            sb.held.set(Some(Grab { dy: ev.position.y - top, h, f, tries: TRIES, moved: Instant::now(), down: true }));
                            list.set_overdraw(px(0.));
                            cx.stop_propagation();
                            window.refresh();
                        }
                    }
                });
                window.on_mouse_event({
                    let (sb, hitbox, list) = (sb.clone(), hitbox.clone(), list2.clone());
                    move |ev: &MouseMoveEvent, phase, window, cx| {
                        if phase != DispatchPhase::Bubble {
                            return;
                        }
                        match sb.held.get().filter(|g| g.down) {
                            Some(g) if ev.pressed_button == Some(MouseButton::Left) => {
                                let travel = (bounds.size.height - px(INSET + ends_for(WINDOW_R)) - g.h).max(px(1.));
                                let f = ((ev.position.y - g.dy - bounds.top() - px(INSET)) / travel).clamp(0., 1.);
                                sb.held.set(Some(Grab { f, tries: TRIES, moved: Instant::now(), ..g }));
                                let (_, content, view) = list.estimated_scroll_metrics();
                                list.scroll_to_estimated_offset(aim(f, content, view));
                                window.refresh();
                                // A frame once the pointer rests, to aim the page again.
                                window.spawn(cx, async move |cx| {
                                    cx.background_executor().timer(REST).await;
                                    let _ = cx.update(|window, _| window.refresh());
                                })
                                .detach();
                            }
                            // The button went up somewhere we didn't see.
                            Some(g) => {
                                sb.held.set(Some(Grab { down: false, ..g }));
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
                        if phase == DispatchPhase::Bubble && ev.button == MouseButton::Left && let Some(g) = sb.held.get().filter(|g| g.down) {
                            sb.held.set(Some(Grab { down: false, ..g }));
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
    let area = handle.clone();
    canvas(
        move |bounds, window, _| {
            let max = list.max_offset().x;
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
        move |bounds, state, window, cx| {
            let sb = sb2;
            let Some((left, w, hitbox)) = state else {
                sb.grab.set(None);
                return;
            };
            window.set_cursor_style(CursorStyle::Arrow, &hitbox);
            let hovered = hitbox.is_hovered(window);
            sb.hot.set(hovered);
            let dragging = sb.grab.get().is_some();
            // shown while the pointer is over the table, as shadcn's ScrollArea is over its box
            let over = area.bounds().contains(&window.mouse_position()) || bounds.contains(&window.mouse_position());
            if over != sb.over.get() { sb.over.set(over); }
            window.on_mouse_event({
                let (sb, area) = (sb.clone(), area.clone());
                move |ev: &MouseMoveEvent, phase, window, _| {
                    if phase == DispatchPhase::Bubble && (area.bounds().contains(&ev.position) || bounds.contains(&ev.position)) != sb.over.get() { window.refresh(); }
                }
            });
            let a = sb.fade.step(over || hovered || dragging, f32::from(left), window, cx);
            let tone = sb.tone.step(if dragging { HELD } else if hovered { HOVER } else { 0. }, window, cx);
            let thumb = Bounds::new(point(left, bounds.top() + px(INSET)), size(w, px(W)));
            if a > 0. { window.paint_quad(fill(thumb, thumb_tone(tone, a)).corner_radii(px(W / 2.))); }

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
                            list.set_offset(point(-list.max_offset().x * f, list.offset().y));
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

#[cfg(test)]
mod tests {
    use super::{Grab, REST, TRIES, aim, drifted, reaim, thumb};
    use gpui::px;
    use std::time::Instant;

    fn held(f: f32) -> Option<Grab> { Some(Grab { dy: px(0.), h: px(40.), f, tries: TRIES, moved: Instant::now(), down: true }) }

    #[test]
    fn the_thumb_reaches_the_end_with_the_page() {
        let (view, track) = (px(600.), px(600.));
        // At the end of the page the thumb is at the end of the track, whatever the length.
        for content in [px(1200.), px(24_000.), px(250_000.)] {
            let (y, h) = thumb(content - view, content, view, track, None).unwrap();
            assert_eq!(y + h, track, "{content:?}");
            assert_eq!(thumb(px(0.), content, view, track, None).unwrap().0, px(0.));
        }
        assert!(thumb(px(0.), px(600.5), view, track, None).is_none(), "it all fits");
        // Held, it is where the pointer put it, not where a changing estimate says.
        let (y, h) = thumb(px(5_000.), px(250_000.), view, track, held(1.)).unwrap();
        assert_eq!((y + h, h), (track, px(40.)));
        assert_eq!(thumb(px(0.), px(9_000.), view, track, held(0.5)).unwrap().0, px(280.));
    }

    #[test]
    fn a_held_thumb_takes_the_page_with_it() {
        let view = px(600.);
        // The end of the track is the real end, which the estimate may put short of it.
        assert_eq!(aim(1., px(10_000.), view), px(f32::MAX));
        assert_eq!(aim(0.5, px(10_600.), view), px(5_000.));
        // The estimate shrank as rows were measured: the page is aimed again …
        let g = held(0.5).unwrap();
        assert!(drifted(&g, px(5_000.), px(4_600.), view));
        assert!(!drifted(&g, px(2_000.), px(4_600.), view));
        // … but not past the end (the real end is the end), nor for ever.
        assert!(!drifted(&held(1.).unwrap(), px(0.), px(4_600.), view));
        assert!(!drifted(&Grab { tries: 0, ..g }, px(5_000.), px(4_600.), view));
    }

    /// Aiming again lays out rows nobody sees: a moving pointer's own moves
    /// aim the page, so only a resting pointer, or a thumb let go, aims again.
    #[test]
    fn the_page_is_aimed_again_only_at_rest() {
        let view = px(600.);
        let g = held(0.5).unwrap();
        let (offset, content) = (px(5_000.), px(4_600.));
        assert_eq!(reaim(&g, offset, content, view, g.moved), None, "moving");
        assert_eq!(reaim(&g, offset, content, view, g.moved + REST), Some(px(2_000.)), "resting");
        assert_eq!(reaim(&Grab { down: false, ..g }, offset, content, view, g.moved), Some(px(2_000.)), "let go");
        assert_eq!(reaim(&g, px(2_000.), content, view, g.moved + REST), None, "already there");
    }
}
