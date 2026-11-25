//! shadcn's ScrollArea scrollbar (Radix `type="hover"`), for any `overflow_y_scroll` box that tracks a
//! `ScrollHandle`: a trackless `border`-coloured thumb, 7 wide (the bar's `w-2.5` less its `p-px` and
//! clear `border-l`), 1 in from every edge; it shows while the pointer is over the box, while it
//! scrolls and while it is dragged, and fades out `HIDE` after (Radix's `scrollHideDelay`). The fade
//! is ours: shadcn switches it. Inside a rounded surface the bar stays flush to the side and its
//! round ends stay whole: it keeps from the top and bottom just what clears the corner (`ends_for`),
//! as shadcn's `p-px` does against its smaller corners. The pages' own scrollbar (the editor's virtualized list) takes
//! the same look from `THUMB`, `PAD`, `thumb_color` and `Fade`.

use super::color;
use gpui::*;
use std::cell::Cell;
use std::time::{Duration, Instant};

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

/// How far a flush thumb (`PAD` in from the side) keeps from the box's top and bottom so its round
/// ends stay whole inside a surface rounded `r`: shadcn's corners are small enough that `p-px` does
/// (8 against a 3.5 end), ours (12) need 4.5. No cut, no gap at the side.
pub fn ends_for(r: f32) -> f32 {
    let half = THUMB / 2.;
    let cx = PAD + half;
    if r <= cx { return PAD; }
    let reach = (r - half) * (r - half) - (r - cx) * (r - cx);
    if reach <= 0. { return PAD; }
    let i = r - half - reach.sqrt();
    ((i * 2.).ceil() / 2.).max(PAD)
}

/// Paints a thumb at `thumb` (round-ended, `THUMB` wide) cut to `clip` rounded `r`: where the thumb
/// passes the surface's corner it takes the corner's arc, as overflow-hidden on a rounded box cuts
/// shadcn's. A thumb clear of the corners is a plain capsule.
pub fn paint_thumb(window: &mut Window, thumb: Bounds<Pixels>, clip: Bounds<Pixels>, r: f32, color: Hsla) {
    let (top, bottom) = (f32::from(thumb.top()), f32::from(thumb.bottom()));
    let (ctop, cbottom, cright) = (f32::from(clip.top()), f32::from(clip.bottom()), f32::from(clip.right()));
    let half = f32::from(thumb.size.width) / 2.;
    if r <= 0. || (top >= ctop + r && bottom <= cbottom - r) {
        window.paint_quad(fill(thumb, color).corner_radii(px(half)));
        return;
    }
    let (x0, cx) = (f32::from(thumb.left()), f32::from(thumb.left()) + half);
    // the thumb's and the surface's extent across, at height y
    let across = |y: f32| -> Option<(f32, f32)> {
        let cap = if y < top + half { Some(top + half - y) } else if y > bottom - half { Some(y - (bottom - half)) } else { None };
        let (mut l, mut rt) = match cap { Some(d) if d >= half => return None, Some(d) => { let w = (half * half - d * d).sqrt(); (cx - w, cx + w) } None => (x0, x0 + 2. * half) };
        let edge = if y < ctop + r { Some(ctop + r - y) } else if y > cbottom - r { Some(y - (cbottom - r)) } else { None };
        if let Some(d) = edge { if d >= r { return None } rt = rt.min(cright - r + (r * r - d * d).sqrt()); }
        l = l.min(rt);
        (rt - l > 0.01).then_some((l, rt))
    };
    let (y0, y1) = (top.max(ctop), bottom.min(cbottom));
    let n = (((y1 - y0) * 4.).ceil() as usize).max(2);
    let rows: Vec<(f32, f32, f32)> = (0..=n).filter_map(|i| { let y = y0 + (y1 - y0) * i as f32 / n as f32; across(y).map(|(l, rt)| (y, l, rt)) }).collect();
    if rows.len() < 2 { return; }
    let mut path = PathBuilder::fill();
    path.move_to(point(px(rows[0].1), px(rows[0].0)));
    for &(y, l, _) in &rows[1..] { path.line_to(point(px(l), px(y))); }
    for &(y, _, rt) in rows.iter().rev() { path.line_to(point(px(rt), px(y))); }
    path.close();
    if let Ok(path) = path.build() { window.paint_path(path, color); }
}

/// The thumb's colour at `alpha` (shadcn `bg-border`).
pub fn thumb_color(alpha: f32) -> Hsla { thumb_tone(0., alpha) }

/// The thumb under the pointer: half way from `border` to `ring`.
pub const HOVER: f32 = 0.5;
/// The thumb held for a drag: `ring`, kept until the button goes up wherever the pointer is.
pub const HELD: f32 = 1.;

/// The thumb's colour at `tone` (0 rest, `HOVER`, `HELD`) and `alpha`: hover and a drag darken it, as a
/// browser's thumb does (the user's call; shadcn's stays `bg-border`): `border` at rest toward `ring`
/// in the light, the foreground from 10% to 35% in the dark.
pub fn thumb_tone(tone: f32, alpha: f32) -> Hsla {
    // dark: `border` is white at 10%, and mixing toward an opaque `ring` reaches it half way once laid on
    // the surface, so the steps are the foreground at 10% (border), 22% and 35% instead
    if super::dark() { return super::alpha(color::foreground(), (0.10 + 0.25 * tone.clamp(0., 1.)) * alpha); }
    let (a, b): (Rgba, Rgba) = (color::border().into(), color::ring().into());
    let t = tone.clamp(0., 1.);
    let c: Hsla = Rgba { r: a.r + (b.r - a.r) * t, g: a.g + (b.g - a.g) * t, b: a.b + (b.b - a.b) * t, a: a.a + (b.a - a.a) * t }.into();
    super::alpha(c, alpha)
}

/// Draws the window again after `wait` (a paint can't ask for the next one: `refresh` is ignored
/// while drawing).
fn wake(window: &mut Window, cx: &mut App, wait: Duration) {
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(wait).await;
        let _ = handle.update(cx, |_, window, _| window.refresh());
    }).detach();
}

/// The thumb's tone easing to its target over `FADE` (`step` each paint).
#[derive(Default)]
pub struct Tone { at: Cell<Option<(f32, Instant)>> }

impl Tone {
    pub fn step(&self, target: f32, window: &mut Window, cx: &mut App) -> f32 {
        let now = Instant::now();
        let (v, t) = self.at.get().unwrap_or((target, now));
        let d = now.duration_since(t).as_secs_f32() / FADE.as_secs_f32();
        let v = if target > v { (v + d).min(target) } else if target < v { (v - d).max(target) } else { v };
        self.at.set(Some((v, now)));
        if v != target { wake(window, cx, Duration::from_millis(16)); }
        v
    }
}

/// Shown or hidden, eased over `FADE`: call `step` each paint with whether it should show; it
/// returns the opacity and has the window drawn again while it moves or when `HIDE` runs out.
#[derive(Default)]
pub struct Fade {
    /// The opacity and when it was last stepped.
    at: Cell<Option<(f32, Instant)>>,
    /// The last scroll offset seen and when it changed.
    moved: Cell<Option<(f32, Instant)>>,
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
            None => self.moved.set(Some((offset, now - HIDE))),
            _ => self.moved.set(Some((offset, now))),
        }
        let scrolled = self.moved.get().is_some_and(|(_, t)| now.duration_since(t) < HIDE);
        if active || scrolled { self.seen.set(Some(now)); }
        let lingering = self.seen.get().is_some_and(|t| now.duration_since(t) < HIDE);
        let target = if active || scrolled || lingering { 1. } else { 0. };
        let (a, t) = self.at.get().unwrap_or((0., now));
        let step = now.duration_since(t).as_secs_f32() / FADE.as_secs_f32();
        // at the target it stays: stepping down from 1 while shown dimmed it every frame, then the next
        // frame brought it back (the flicker over a resting pointer)
        let a = if target > a { (a + step).min(1.) } else if target < a { (a - step).max(0.) } else { a };
        self.at.set(Some((a, now)));
        // A paint can't ask for the next one (`refresh` is ignored while drawing): a timer does, a
        // frame on while it fades, or when the hold runs out.
        let wait = if a != target { Some(Duration::from_millis(16)) }
            else if lingering && !active { self.seen.get().map(|t| HIDE.saturating_sub(now.duration_since(t)) + Duration::from_millis(1)) }
            else { None };
        if let Some(wait) = wait { wake(window, cx, wait); }
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
    /// The radius of the surface the box fills (0: square), whose corners the thumb's ends keep clear of.
    radius: Cell<f32>,
    /// The pointer was over the box last paint.
    over: Cell<bool>,
    /// The pointer was on the thumb last paint.
    on_thumb: Cell<bool>,
    tone: Tone,
}

impl ScrollBar {
    /// A scrollbar in a box that fills a surface rounded `r` (0 for a square box): flush to the side, its
    /// ends clear of the corners (`ends_for`).
    pub fn rounded(r: f32) -> Self { let s = Self::default(); s.radius.set(r); s }

    pub fn render(self: &std::rc::Rc<Self>, handle: &ScrollHandle) -> impl IntoElement + use<> {
        let (list, list2) = (handle.clone(), handle.clone());
        let (sb, sb2) = (self.clone(), self.clone());
        canvas(
            move |_, window, _| {
                let area = list.bounds();
                let inset = px(PAD);
                // flush to the side; at the ends just far enough that the round end clears the corner
                let ends = px(ends_for(sb.radius.get()));
                let view = area.size.height;
                let content = view + list.max_offset().y;
                if content <= view + px(1.) { return None; }
                let track = (view - ends * 2.).max(px(1.));
                let h = match sb.grab.get() { Some((_, _, h)) => h, None => (track * (view / content)).max(px(MIN)).min(track) };
                let content = sb.grab.get().map_or(content, |(_, c, _)| c);
                let offset = -list.offset().y;
                let top = area.top() + ends + (track - h) * (offset / (content - view)).clamp(0., 1.);
                let zone = Bounds::new(point(area.right() - px(BAR), area.top()), size(px(BAR), view));
                Some((area, top, h, track, inset, content, offset, window.insert_hitbox(zone, HitboxBehavior::BlockMouseExceptScroll)))
            },
            move |_, state, window, cx| {
                let sb = sb2;
                let Some((area, top, h, track, inset, content, offset, hitbox)) = state else { sb.grab.set(None); return };
                window.set_cursor_style(CursorStyle::Arrow, &hitbox);
                let held = sb.grab.get().is_some();
                let mouse = window.mouse_position();
                let over = area.contains(&mouse);
                sb.over.set(over);
                let on_thumb = hitbox.is_hovered(window) && mouse.y >= top && mouse.y <= top + h;
                sb.on_thumb.set(on_thumb);
                let a = sb.fade.step(over || held, f32::from(offset), window, cx);
                let tone = sb.tone.step(if held { HELD } else if on_thumb { HOVER } else { 0. }, window, cx);
                if a > 0. {
                    let thumb = Bounds::new(point(area.right() - inset - px(THUMB), top), size(px(THUMB), h));
                    paint_thumb(window, thumb, area, sb.radius.get(), thumb_tone(tone, a));
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
                                let f = ((ev.position.y - dy - area.top() - px(ends_for(sb.radius.get()))) / travel).clamp(0., 1.);
                                // At the end, the real bottom: rows there may not be measured yet.
                                list.set_offset(point(list.offset().x, -list.max_offset().y * f));
                                window.refresh();
                            }
                            // The button went up somewhere we didn't see.
                            Some(_) => { sb.grab.set(None); window.refresh(); }
                            // Entering or leaving the box shows or hides the thumb.
                            None if area.contains(&ev.position) != sb.over.get() => window.refresh(),
                            // onto or off the thumb: its tone moves
                            None if (ev.position.x >= area.right() - px(BAR) && ev.position.x <= area.right() && ev.position.y >= top && ev.position.y <= top + h) != sb.on_thumb.get() => window.refresh(),
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
