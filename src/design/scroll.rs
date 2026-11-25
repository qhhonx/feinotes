//! shadcn's ScrollArea scrollbar (Radix `type="hover"`), for any `overflow_y_scroll` box that tracks a
//! `ScrollHandle`: a trackless `border`-coloured thumb, 7 wide (the bar's `w-2.5` less its `p-px` and
//! clear `border-l`), 1 in from every edge; it shows while the pointer is over the box, while it
//! scrolls and while it is dragged, and fades out `HIDE` after (Radix's `scrollHideDelay`). The fade
//! is ours: shadcn switches it. Inside a rounded surface the thumb keeps one inset all round, as much
//! as clears the corner (`inset_for`). The pages' own scrollbar (the editor's virtualized list) takes
//! the same look from `THUMB`, `PAD`, `thumb_color` and `Fade`.

use super::color;
use gpui::*;
use std::cell::Cell;
use std::time::Duration;
use web_time::Instant;

/// The thumb's width (shadcn: `w-2.5` 10, less `p-px` both sides and the clear 1pt `border-l`).
pub const THUMB: f32 = 7.;
/// The thumb's inset from the box's edges (`p-px`).
pub const PAD: f32 = 1.;
/// The bar, which takes the pointer: `w-2.5`.
pub const BAR: f32 = 10.;
/// The shortest thumb.
pub const MIN: f32 = 24.;
/// How long the thumb stays after the pointer leaves or the scrolling stops (Radix `scrollHideDelay`).
pub const HIDE: Duration = Duration::from_millis(600);
/// Fade in and out.
pub const FADE: Duration = Duration::from_millis(150);

/// One inset all round that keeps a `THUMB`-wide thumb with round ends inside a surface rounded
/// `r`: the end's centre sits `r` from the corner's centre less its own radius. 2.5 for 12.
pub fn inset_for(r: f32) -> f32 {
    let half = THUMB / 2.;
    let i = (r - half) * (1. - std::f32::consts::FRAC_1_SQRT_2);
    ((i * 2.).ceil() / 2.).max(PAD)
}

/// The thumb's colour at `alpha` (shadcn `bg-border`).
pub fn thumb_color(alpha: f32) -> Hsla { super::alpha(color::border(), alpha) }

/// Shown or hidden, eased over `FADE`: call `step` each paint with whether it should show; it
/// returns the opacity and has the window drawn again while it moves or when `HIDE` runs out.
#[derive(Default)]
pub struct Fade {
    /// The opacity and when it was last stepped.
    at: Cell<Option<(f32, Instant)>>,
    /// The last scroll offset seen and when it changed.
    moved: Cell<Option<(f32, Option<Instant>)>>,
    /// The last time the thumb had cause to show.
    seen: Cell<Option<Instant>>,
}

impl Fade {
    /// `active`: the pointer is over the box or the thumb is held; `offset`: the scroll position.
    pub fn step(&self, active: bool, offset: f32, window: &mut Window, cx: &mut App) -> f32 {
        let now = Instant::now();
        match self.moved.get() {
            Some((o, _)) if (o - offset).abs() < 0.5 => {}
            // The first frame is not a scroll.
            None => self.moved.set(Some((offset, None))),
            _ => self.moved.set(Some((offset, Some(now)))),
        }
        let scrolled = self.moved.get().and_then(|(_, t)| t).is_some_and(|t| now.duration_since(t) < HIDE);
        if active || scrolled { self.seen.set(Some(now)); }
        let lingering = self.seen.get().is_some_and(|t| now.duration_since(t) < HIDE);
        let target = if active || scrolled || lingering { 1. } else { 0. };
        let (a, t) = self.at.get().unwrap_or((0., now));
        let step = now.duration_since(t).as_secs_f32() / FADE.as_secs_f32();
        let a = if target > a { (a + step).min(1.) } else { (a - step).max(0.) };
        self.at.set(Some((a, now)));
        // A paint can't ask for the next one (`refresh` is ignored while drawing): a timer does, a
        // frame on while it fades, or when the hold runs out.
        let wait = if a != target { Some(Duration::from_millis(16)) }
            else if lingering && !active { self.seen.get().map(|t| HIDE.saturating_sub(now.duration_since(t)) + Duration::from_millis(1)) }
            else { None };
        if let Some(wait) = wait {
            let handle = window.window_handle();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(wait).await;
                let _ = handle.update(cx, |_, window, _| window.refresh());
            }).detach();
        }
        a
    }
}

/// The scrollbar of the box `handle` tracks. Put `render` beside the box in a `relative` parent
/// (it lays itself on the parent's right edge), or as the box's last child.
#[derive(Default)]
pub struct ScrollBar {
    /// While dragging: pointer offset into the thumb, and the content and thumb heights at the press
    /// (held still so the thumb stays under the pointer as newly measured rows refine the estimate).
    grab: Cell<Option<(Pixels, Pixels, Pixels)>>,
    fade: Fade,
    /// The inset all round: `PAD`, or `inset_for` the surface's radius.
    inset: Cell<f32>,
    /// The pointer was over the box last paint.
    over: Cell<bool>,
}

impl ScrollBar {
    /// A scrollbar inside a surface rounded `r` (0 for a square box).
    pub fn rounded(r: f32) -> Self { let s = Self::default(); s.inset.set(if r > 0. { inset_for(r) } else { PAD }); s }

    pub fn render(self: &std::rc::Rc<Self>, handle: &ScrollHandle) -> impl IntoElement + use<> {
        let (list, list2) = (handle.clone(), handle.clone());
        let (sb, sb2) = (self.clone(), self.clone());
        canvas(
            move |_, window, _| {
                let area = list.bounds();
                let inset = px(sb.inset.get().max(PAD));
                let view = area.size.height;
                let content = view + list.max_offset().y;
                if content <= view + px(1.) { return None; }
                let track = (view - inset * 2.).max(px(1.));
                let h = match sb.grab.get() { Some((_, _, h)) => h, None => (track * (view / content)).max(px(MIN)).min(track) };
                let content = sb.grab.get().map_or(content, |(_, c, _)| c);
                let offset = -list.offset().y;
                let top = area.top() + inset + (track - h) * (offset / (content - view)).clamp(0., 1.);
                let zone = Bounds::new(point(area.right() - px(BAR), area.top()), size(px(BAR), view));
                Some((area, top, h, track, inset, content, offset, window.insert_hitbox(zone, HitboxBehavior::BlockMouseExceptScroll)))
            },
            move |_, state, window, cx| {
                let sb = sb2;
                let Some((area, top, h, track, inset, content, offset, hitbox)) = state else { sb.grab.set(None); return };
                window.set_cursor_style(CursorStyle::Arrow, &hitbox);
                let held = sb.grab.get().is_some();
                let over = area.contains(&window.mouse_position());
                sb.over.set(over);
                let a = sb.fade.step(over || held, f32::from(offset), window, cx);
                if a > 0. {
                    let thumb = Bounds::new(point(area.right() - inset - px(THUMB), top), size(px(THUMB), h));
                    window.paint_quad(fill(thumb, thumb_color(a)).corner_radii(px(THUMB / 2.)));
                }
                window.on_mouse_event({
                    let (sb, hitbox) = (sb.clone(), hitbox.clone());
                    move |ev: &MouseDownEvent, phase, window, cx| {
                        if phase != DispatchPhase::Bubble || ev.button != MouseButton::Left || !hitbox.is_hovered(window) { return; }
                        // On the thumb: hold it where it was taken; on the bar: centre it there first.
                        let dy = if ev.position.y >= top && ev.position.y <= top + h { ev.position.y - top } else { h / 2. };
                        sb.grab.set(Some((dy, content, h)));
                        cx.stop_propagation();
                        window.refresh();
                    }
                });
                window.on_mouse_event({
                    let (sb, list) = (sb.clone(), list2.clone());
                    move |ev: &MouseMoveEvent, phase, window, _| {
                        if phase != DispatchPhase::Bubble { return; }
                        match sb.grab.get() {
                            Some((dy, _, h)) if ev.pressed_button == Some(MouseButton::Left) => {
                                let travel = (track - h).max(px(1.));
                                let f = ((ev.position.y - dy - area.top() - inset) / travel).clamp(0., 1.);
                                // At the end, the real bottom: rows there may not be measured yet.
                                list.set_offset(point(list.offset().x, -list.max_offset().y * f));
                                window.refresh();
                            }
                            // The button went up somewhere we didn't see.
                            Some(_) => { sb.grab.set(None); window.refresh(); }
                            // Entering or leaving the box shows or hides the thumb.
                            None if area.contains(&ev.position) != sb.over.get() => window.refresh(),
                            None => {}
                        }
                    }
                });
                window.on_mouse_event({
                    let sb = sb.clone();
                    move |ev: &MouseUpEvent, phase, window, _| {
                        if phase == DispatchPhase::Bubble && ev.button == MouseButton::Left && sb.grab.take().is_some() { window.refresh(); }
                    }
                });
            },
        )
        .absolute()
        .top_0()
        .right_0()
        .size_0()
    }
}

thread_local! {
    /// `handle`'s scroll handles, by the key their box gives.
    static HANDLES: std::cell::RefCell<std::collections::HashMap<&'static str, ScrollHandle>> = Default::default();
    /// `bar`'s scrollbars, by the key their box gives.
    static BARS: std::cell::RefCell<std::collections::HashMap<&'static str, std::rc::Rc<ScrollBar>>> = Default::default();
}

/// The scrollbar of the box `handle` tracks, kept under `key`, in a surface rounded `r` (0: square):
/// the box's last child (it paints over the box wherever the box is, and takes no room).
pub fn bar(key: &'static str, handle: &ScrollHandle, r: f32) -> impl IntoElement + use<> {
    BARS.with(|m| m.borrow_mut().entry(key).or_insert_with(|| std::rc::Rc::new(ScrollBar::rounded(r))).clone()).render(handle)
}

/// A scroll handle kept under `key`, for a box with none of its own to `track_scroll` and give `bar`.
pub fn handle(key: &'static str) -> ScrollHandle {
    HANDLES.with(|m| m.borrow_mut().entry(key).or_insert_with(ScrollHandle::new).clone())
}
