//! Toolbars. shadcn has no `Toolbar`; ours is its parts put together: a popover's face holding
//! `Toggle`s (on / off: bold, italic …), ghost `Button`s (actions; a menu's trigger, `aria-expanded`
//! while it is open) and `ButtonGroupSeparator`s, with `ButtonGroup` joining a split button.
//!
//! ```ignore
//! Toolbar::new("tb")
//!     .child(Button::ghost("turn").sized(ButtonSize::Sm).editor_icon("type").icon_end("chevron-down").selected(open))
//!     .child(ButtonGroupSeparator::new())
//!     .child(Toggle::new("b").editor_icon("bold").pressed(bold))
//!     .overlay(dropdown)
//! ```
//!
//! Nothing in a toolbar casts a shadow or turns into a pill: pressed, open and picked are the
//! `muted` fill (one step up on a raised surface in the dark), at the buttons' radius.

use super::base::{element, icon, label, under};
use super::{color, geo, shadow};
use gpui::prelude::FluentBuilder;
use gpui::*;

/// The toolbar's padding (the popover's `p-1`) and the room between its items (`ToggleGroup
/// spacing={1}`, shadcn's text-formatting example).
pub const TOOLBAR_PAD: f32 = 4.;
pub const TOOLBAR_GAP: f32 = 4.;
/// Its items are `sm` (h-8), so the bar is 40 tall.
pub const TOOLBAR_H: f32 = 32. + 2. * TOOLBAR_PAD;
/// Widths of what goes in a toolbar, for callers that place a dropdown under an item.
pub mod w {
    /// An icon-only `Toggle` at sm: px-2.5 either side of a 16pt glyph.
    pub const TOGGLE: f32 = 10. + 16. + 10.;
    /// A ghost `icon-sm` button.
    pub const BUTTON: f32 = 32.;
    /// A ghost sm button with a glyph and a chevron: pl-1.5 + 16 + gap-1 + 16 + pr-1.5.
    pub const DROPDOWN: f32 = 6. + 16. + 4. + 16. + 6.;
    /// A split button: `ButtonGroup` of two `icon-sm` buttons.
    pub const SPLIT: f32 = 2. * BUTTON;
    pub const SEPARATOR: f32 = 1.;
}

/// Where each item starts (from the toolbar's left edge) and the toolbar's width, for items of
/// these widths.
pub fn toolbar_layout(widths: &[f32]) -> (Vec<f32>, f32) {
    let mut x = TOOLBAR_PAD;
    let mut at = Vec::with_capacity(widths.len());
    for (i, w) in widths.iter().enumerate() {
        if i > 0 { x += TOOLBAR_GAP }
        at.push(x);
        x += w;
    }
    (at, x + TOOLBAR_PAD)
}

/// A ghost's fills: (under the pointer, on). `muted` both (shadcn's Toggle; a ghost button is
/// `muted/50` under the pointer in the dark); on a raised surface in the dark, where `muted` is the
/// surface, a step up (DESIGN_SYSTEM.md, deviation C).
pub(crate) fn ghost_fills(toggle: bool) -> (Hsla, Hsla) {
    let dark = crate::theme::dark();
    if dark && super::base::raised() { (color::accent_hover(), color::accent()) }
    else { (if dark && !toggle { color::muted_half() } else { color::muted() }, color::muted()) }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToggleSize {
    /// h-9.
    Default,
    /// h-8: toolbars.
    Sm,
    /// h-10.
    Lg,
}

/// shadcn's `Toggle`: a two-state button. No fill until the pointer is over it or it is pressed
/// (`aria-pressed:bg-muted`); `outline` adds the field's 1px `input` border and shadow-xs.
#[derive(IntoElement)]
pub struct Toggle {
    base: Stateful<Div>,
    size: ToggleSize,
    outline: bool,
    pressed: bool,
    disabled: bool,
    icon: Option<(SharedString, bool)>,
    label: Option<SharedString>,
}
element!(Toggle, parent);

impl Toggle {
    /// A toolbar's toggle: sm.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Toggle { base: div().id(id), size: ToggleSize::Sm, outline: false, pressed: false, disabled: false, icon: None, label: None }
    }
    pub fn sized(mut self, s: ToggleSize) -> Self { self.size = s; self }
    pub fn outline(mut self) -> Self { self.outline = true; self }
    pub fn pressed(mut self, on: bool) -> Self { self.pressed = on; self }
    pub fn disabled(mut self, on: bool) -> Self { self.disabled = on; self }
    pub fn icon(mut self, name: impl Into<SharedString>) -> Self { self.icon = Some((name.into(), false)); self }
    /// The editor's glyph of that name (its set draws the marks, alignment, block types).
    pub fn editor_icon(mut self, name: impl Into<SharedString>) -> Self { self.icon = Some((name.into(), true)); self }
    pub fn label(mut self, s: impl Into<SharedString>) -> Self { self.label = Some(s.into()); self }
}

impl RenderOnce for Toggle {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let g = geo();
        let h = match self.size { ToggleSize::Default => 36., ToggleSize::Sm => 32., ToggleSize::Lg => 40. };
        let (hover, on) = ghost_fills(true);
        let (pressed, disabled, outline, ic, lab) = (self.pressed, self.disabled, self.outline, self.icon, self.label);
        // px-2.5; the glyph's side pl-2 (pl-1.5 at sm) beside a label
        let pl = if ic.is_some() && lab.is_some() { if self.size == ToggleSize::Sm { 6. } else { 8. } } else { 10. };
        let fg = color::foreground();
        under(self.base, |d| {
            d.flex_none().h(px(h)).min_w(px(h)).pl(px(pl)).pr(px(10.)).flex().items_center().justify_center().gap(px(4.))
                .rounded(px(g.button_r)).text_size(px(g.text)).text_color(fg).whitespace_nowrap()
                // bg-transparent, but GPUI draws shadow-xs under the box: on `background` in the light
                // (the dark has no shadow-xs), as the fields (DESIGN_SYSTEM.md, deviation C)
                .when(outline, |d| d.border_1().border_color(color::input()).shadow(shadow::xs()).when(!crate::theme::dark(), |d| d.bg(color::background())))
                .when(pressed, |d| d.bg(on))
                .when(!disabled, |d| d.cursor_pointer().hover(move |s| s.bg(if pressed { on } else { hover })))
                .when(disabled, |d| d.opacity(0.5))
                .when_some(ic, |d, (n, ed)| d.child(if ed { crate::editor_ui::icon(&n, 16., fg) } else { icon(&n, 16., fg) }))
                .when_some(lab, |d, s| d.child(label(s)))
        })
    }
}

/// shadcn's `ButtonGroup`: buttons side by side with no room between them; the inner corners are
/// square and (for outline buttons) the inner borders shared.
#[derive(IntoElement)]
pub struct ButtonGroup { base: Div, items: Vec<Box<dyn FnOnce(bool, bool) -> AnyElement>> }

impl Styled for ButtonGroup { fn style(&mut self) -> &mut StyleRefinement { self.base.style() } }

impl ButtonGroup {
    pub fn new() -> Self { ButtonGroup { base: div(), items: vec![] } }
    /// A button (or anything styled like one) in the group.
    pub fn child<E: Styled + IntoElement + 'static>(mut self, e: E) -> Self {
        self.items.push(Box::new(move |first, last| {
            let mut e = e;
            if !first { e = e.rounded_l(px(0.)); e.style().border_widths.left = Some(px(0.).into()); }
            if !last { e = e.rounded_r(px(0.)); }
            e.into_any_element()
        }));
        self
    }
    /// A `ButtonGroupSeparator` (or any piece the corners leave alone).
    pub fn separator(mut self, s: ButtonGroupSeparator) -> Self { self.items.push(Box::new(move |_, _| s.into_any_element())); self }
}

impl RenderOnce for ButtonGroup {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let n = self.items.len();
        let mut d = self.base;
        let caller = std::mem::take(d.style());
        let mut d = d.flex().flex_none().items_stretch();
        d.style().refine(&caller);
        d.children(self.items.into_iter().enumerate().map(|(i, f)| f(i == 0, i + 1 == n)))
    }
}

/// shadcn's `ButtonGroupSeparator`: a 1px `input` rule as tall as the row, 1 short at each end.
#[derive(IntoElement)]
pub struct ButtonGroupSeparator { base: Div }
impl Styled for ButtonGroupSeparator { fn style(&mut self) -> &mut StyleRefinement { self.base.style() } }
impl ButtonGroupSeparator { pub fn new() -> Self { ButtonGroupSeparator { base: div() } } }
impl RenderOnce for ButtonGroupSeparator {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut d = self.base;
        let caller = std::mem::take(d.style());
        let mut d = d.flex_none().w(px(w::SEPARATOR)).self_stretch().my(px(1.)).bg(color::input());
        d.style().refine(&caller);
        d
    }
}

/// A floating toolbar: the popover's face and edge (`shadow::popover()`), p-1, its items 4 apart
/// in one row. `overlay` hangs dropdowns off it (absolutely placed, from its top-left corner).
#[derive(IntoElement)]
pub struct Toolbar { pop: super::Popover, items: Vec<AnyElement>, over: Vec<AnyElement> }

impl Toolbar {
    pub fn new(id: impl Into<ElementId>) -> Self { Toolbar { pop: super::Popover::new(id), items: vec![], over: vec![] } }
    pub fn overlay(mut self, e: impl IntoElement) -> Self { self.over.push(e.into_any_element()); self }
    pub fn overlays(mut self, e: impl IntoIterator<Item = impl IntoElement>) -> Self { self.over.extend(e.into_iter().map(|e| e.into_any_element())); self }
}
impl ParentElement for Toolbar { fn extend(&mut self, e: impl IntoIterator<Item = AnyElement>) { self.items.extend(e) } }
impl Styled for Toolbar { fn style(&mut self) -> &mut StyleRefinement { self.pop.style() } }
impl InteractiveElement for Toolbar { fn interactivity(&mut self) -> &mut Interactivity { self.pop.interactivity() } }
impl StatefulInteractiveElement for Toolbar {}

impl RenderOnce for Toolbar {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.pop.relative().p(px(TOOLBAR_PAD))
            .child(div().h(px(TOOLBAR_H - 2. * TOOLBAR_PAD)).flex().items_center().gap(px(TOOLBAR_GAP)).children(self.items))
            .children(self.over)
    }
}

/// The editor's block handle: shadcn's `ButtonGroup` of outline `icon-xs` buttons, the block's
/// type glyph (`glyph`, its tint is content) and the ⋮⋮ grip in `muted-foreground`; with no glyph,
/// the empty line's + alone. `open` while its menu is: `aria-expanded`, `muted`. 26 tall and
/// `w` (42 / 28) wide rather than icon-xs's 24 square: the editor places the handle by those sizes
/// (DESIGN_SYSTEM.md, deviation C). It is one control, so the pointer anywhere on it lights both
/// halves.
pub fn block_handle(key: u64, glyph: Option<Svg>, w: f32, open: bool) -> Stateful<Div> {
    let group = SharedString::from(format!("handle-{key}"));
    let add = glyph.is_none();
    let hover_bg = if crate::theme::dark() { color::field_hover() } else { color::muted() };
    let seg = |k: u64, w: f32| {
        let g = group.clone();
        super::Button::outline(("handle-seg", key * 2 + k))
            .sized(super::ButtonSize::Xs)
            .selected(open)
            .w(px(w))
            .h(px(HANDLE_H))
            // still when pressed (it is dragged from): a set top inset turns the press nudge off
            .top(px(0.))
            .cursor(if add { CursorStyle::PointingHand } else { CursorStyle::OpenHand })
            .group_hover(g, move |s| s.bg(hover_bg))
    };
    let d = div().id(("handle", key)).group(group.clone()).occlude().flex();
    match glyph {
        None => d.child(seg(0, w).child(icon("plus", 12., color::foreground()))),
        Some(g) => d.child(ButtonGroup::new()
            .child(seg(0, HANDLE_GLYPH_W).child(g))
            .child(seg(1, w - HANDLE_GLYPH_W).child(crate::editor_ui::icon("grip-vertical", 12., color::muted_foreground())))),
    }
}
/// The handle's height and its type glyph's half (the grip's half is the rest of its 42).
pub const HANDLE_H: f32 = 26.;
const HANDLE_GLYPH_W: f32 = 24.;
