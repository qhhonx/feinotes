//! shadcn's ScrollArea scrollbar (Radix `type="hover"`), for any `overflow_y_scroll` box that tracks a
//! `ScrollHandle`: a trackless `border`-coloured thumb, 7 wide (the bar's `w-2.5` less its `p-px` and
//! clear `border-l`), 1 in from every edge; it shows while the pointer is over the box, while it
//! scrolls and while it is dragged, and fades out `HIDE` after (Radix's `scrollHideDelay`). The fade
//! is ours: shadcn switches it. Two rules shadcn's overlay doesn't keep (docs/design/SHADCN_AUDIT.md,
//! 滚动条): the thumb never lies over anything that takes the pointer, so a box that overflows makes
//! room for it on the right (`ScrollBar::room`, `room`), whether it shows or not; and inside a rounded
//! surface it runs only along the straight part of the edge, the corner and `PAD` clear of either
//! end (`ends_for`). The pages' own scrollbar (the editor's virtualized list) takes the same look from
//! `THUMB`, `PAD`, `thumb_color` and `Fade`.

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

/// How far the thumb keeps from the box's top and bottom inside a surface rounded `r`: the whole
/// corner, then the same `PAD` it keeps from the side, so it runs only along the straight part of the
/// edge and never into an arc, even at either end of its travel (as AppKit's `scrollerInsets` and
/// GPUI Kit's inset pull it in; shadcn's runs flush and the corner clips it). Square: just `PAD`.
pub fn ends_for(r: f32) -> f32 { if r > 0. { r + PAD } else { PAD } }

/// How far the window's own corner curves in from its edges, for a box that runs into it (the page, a
/// settings page, the home lists, a side panel): macOS draws a continuous corner whose curve starts 18
/// from the corner (measured on macOS 27 with `deskshot`, 2026-11-25); Windows 11 rounds 8.
pub const WINDOW_R: f32 = if cfg!(target_os = "macos") { 18. } else if cfg!(windows) { 8. } else { 0. };

/// The room the thumb takes in from the box's right edge: `PAD`, then the thumb.
pub const LANE: f32 = PAD + THUMB;
/// What the box's content keeps clear of the thumb: a menu's `p-1`, as its rows keep from the edge.
pub const GAP: f32 = 4.;

/// How much more right padding a box needs, beyond `inset` its content already keeps from the right
/// edge, for the content to stay `GAP` clear of the thumb's lane: nothing interactive (a row's hover
/// or selected fill, a button, a field) under the thumb. A box that has the room already (the
/// sidebar's 14) needs none.
pub fn room_for(inset: f32) -> f32 { (LANE + GAP - inset).max(0.) }

/// Makes `room` for the thumb on a box styled `style`: a width set in points grows by it (the lane goes
/// outside the content, which keeps its own width), and so does a right padding set in points. Says
/// whether the box's own right padding (none set) should take the room instead.
pub fn widen(style: &mut StyleRefinement, room: f32) -> bool {
    if room <= 0. { return false; }
    let grow = |l: &mut Length| if let Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(w))) = l { *l = px(f32::from(*w) + room).into(); };
    if let Some(w) = style.size.width.as_mut() { grow(w); }
    if let Some(w) = style.min_size.width.as_mut() { grow(w); }
    if let Some(w) = style.max_size.width.as_mut() { grow(w); }
    match style.padding.right.as_mut() {
        Some(DefiniteLength::Absolute(AbsoluteLength::Pixels(p))) => { *p = px(f32::from(*p) + room); false }
        Some(_) => false,
        None => true,
    }
}

/// Paints the thumb: a capsule (`ends_for` keeps it out of the corners).
pub fn paint_thumb(window: &mut Window, thumb: Bounds<Pixels>, color: Hsla) {
    window.paint_quad(fill(thumb, color).corner_radii(thumb.size.width / 2.));
}

thread_local! {
    /// A scrollbar found its box overflowing when it was laid out as not (or the other way): the
    /// box's room is a frame behind, and the popover around it skips this frame (`unsettled`).
    static UNSETTLED: Cell<bool> = const { Cell::new(false) };
    /// Whether a popover's box overflowed when last open, by the popover's id, so it opens with its
    /// room already made.
    static ROOMY: std::cell::RefCell<std::collections::HashMap<ElementId, bool>> = Default::default();
}

/// Runs `f` (a popover's prepaint) and says whether a scrollbar inside it found its room a frame
/// behind: the popover then skips painting this frame and shows next frame, laid out right, rather
/// than showing its rows wide for a frame and then narrowing them.
pub fn unsettled(f: impl FnOnce()) -> bool {
    let prev = UNSETTLED.with(|u| u.replace(false));
    f();
    UNSETTLED.with(|u| u.replace(prev))
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
    /// The radii of the corners by the box's top right and bottom right (0: square), which the thumb's
    /// ends keep clear of (`ends_for`).
    radius: Cell<(f32, f32)>,
    /// The pointer was over the box last paint.
    over: Cell<bool>,
    /// The pointer was on the thumb last paint.
    on_thumb: Cell<bool>,
    tone: Tone,
    /// The box overflowed when last laid out: its content makes room for the thumb (`room`).
    wide: Cell<bool>,
    /// The popover whose box this is, to remember `wide` by when it closes (`ROOMY`).
    memo: std::cell::RefCell<Option<ElementId>>,
}

impl ScrollBar {
    /// A scrollbar in a box that fills a surface rounded `r` (0 for a square box): flush to the side, its
    /// ends clear of the corners (`ends_for`).
    pub fn rounded(r: f32) -> Self { Self::corners(r, r) }
    /// A scrollbar whose box's top right corner is rounded `top` and its bottom right `bottom` (a box
    /// that runs down to the window's corner: `WINDOW_R`).
    pub fn corners(top: f32, bottom: f32) -> Self { let s = Self::default(); s.radius.set((top, bottom)); s }

    /// The scrollbar of popover `id`'s box: it opens with the room it needed when last open.
    pub fn remembered(self, id: &ElementId) -> Self {
        self.wide.set(ROOMY.with(|m| m.borrow().get(id).copied().unwrap_or(false)));
        *self.memo.borrow_mut() = Some(id.clone());
        self
    }

    /// The right padding the box adds, beyond the `inset` its content keeps from the right edge, so
    /// its content stays clear of the thumb (`room_for`): only while it overflows, and then whether
    /// the thumb shows or not, so nothing moves as it fades in and out (`scrollbar-gutter: stable`
    /// with `overflow: auto`, as a classic scrollbar takes its room).
    pub fn room(&self, inset: f32) -> f32 { if self.wide.get() { room_for(inset) } else { 0. } }

    /// The box `el`, given its `room` (`widen`): last in the box's styling, after its own padding and width.
    pub fn make_room<E: Styled>(&self, mut el: E, inset: f32) -> E {
        let room = self.room(inset);
        if widen(el.style(), room) { el = el.pr(px(room)); }
        el
    }

    /// Notes whether the box overflows; a change has it laid out again (`room`), the popover around
    /// it skipping this frame (`unsettled`).
    fn overflows(&self, yes: bool, window: &mut Window) {
        if self.wide.get() == yes { return; }
        self.wide.set(yes);
        if let Some(id) = self.memo.borrow().clone() { ROOMY.with(|m| m.borrow_mut().insert(id, yes)); }
        UNSETTLED.with(|u| u.set(true));
        window.request_animation_frame();
    }

    pub fn render(self: &std::rc::Rc<Self>, handle: &ScrollHandle) -> impl IntoElement + use<> {
        let (list, list2) = (handle.clone(), handle.clone());
        let (sb, sb2) = (self.clone(), self.clone());
        canvas(
            move |_, window, _| {
                let area = list.bounds();
                let inset = px(PAD);
                // flush to the side; at either end clear of the corner there
                let (r_top, r_bottom) = sb.radius.get();
                let (ends, ends_b) = (px(ends_for(r_top)), px(ends_for(r_bottom)));
                let view = area.size.height;
                let content = view + list.max_offset().y;
                sb.overflows(content > view + px(1.), window);
                if content <= view + px(1.) { return None; }
                let track = (view - ends - ends_b).max(px(1.));
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
                    paint_thumb(window, thumb, thumb_tone(tone, a));
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
                                let f = ((ev.position.y - dy - area.top() - px(ends_for(sb.radius.get().0))) / travel).clamp(0., 1.);
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
        // it paints from the box's bounds, not its own, and sits among the rows: the box counts its
        // children's extent as its content, and a point at its top, above the top padding, had a menu
        // that fits scroll by that padding
        .absolute()
        .top(relative(0.5))
        .left(relative(0.5))
        .size_0()
    }
}

thread_local! {
    /// `handle`'s scroll handles, by the key their box gives.
    static HANDLES: std::cell::RefCell<std::collections::HashMap<&'static str, ScrollHandle>> = Default::default();
    /// `bar`'s scrollbars, by the key their box gives.
    static BARS: std::cell::RefCell<std::collections::HashMap<&'static str, std::rc::Rc<ScrollBar>>> = Default::default();
}

fn keyed(key: &'static str, r: f32) -> std::rc::Rc<ScrollBar> { keyed_corners(key, r, r) }
fn keyed_corners(key: &'static str, top: f32, bottom: f32) -> std::rc::Rc<ScrollBar> {
    let b = BARS.with(|m| m.borrow_mut().entry(key).or_default().clone());
    b.radius.set((top, bottom));
    b
}

/// `bar` for a box whose top right corner is rounded `top` and bottom right `bottom` (`ScrollBar::corners`).
pub fn bar_corners(key: &'static str, handle: &ScrollHandle, top: f32, bottom: f32) -> impl IntoElement + use<> { keyed_corners(key, top, bottom).render(handle) }

/// The scrollbar of the box `handle` tracks, kept under `key`, in a surface rounded `r` (0: square):
/// the box's last child (it paints over the box wherever the box is). Its room comes from `room`.
pub fn bar(key: &'static str, handle: &ScrollHandle, r: f32) -> impl IntoElement + use<> { keyed(key, r).render(handle) }

/// The right padding `key`'s box adds while it overflows, beyond the `inset` its content keeps from
/// the right edge (`ScrollBar::room`).
pub fn room(key: &'static str, inset: f32) -> Pixels { px(of(key).room(inset)) }

/// `key`'s box `el`, given its room (`ScrollBar::make_room`); `inset` as `room` takes it.
pub fn make_room<E: Styled>(el: E, key: &'static str, inset: f32) -> E { of(key).make_room(el, inset) }

/// `key`'s scrollbar, as `bar` keeps it.
fn of(key: &'static str) -> std::rc::Rc<ScrollBar> { BARS.with(|m| m.borrow_mut().entry(key).or_default().clone()) }

/// A scroll handle kept under `key`, for a box with none of its own to `track_scroll` and give `bar`.
pub fn handle(key: &'static str) -> ScrollHandle {
    HANDLES.with(|m| m.borrow_mut().entry(key).or_insert_with(ScrollHandle::new).clone())
}
