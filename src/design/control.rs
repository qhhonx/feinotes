//! Choice controls (shadcn's `Checkbox`, `Switch`, `Tabs` as a segmented control). The checkbox and
//! the switch only draw; the caller's row or button takes the click.
//!
//! ```ignore
//! Checkbox::new(done)
//! Switch::new(on).disabled(!can)
//! Segmented::new().child(Segment::new("asc", "A → Z").selected(asc).on_click(…)).child(…)
//! ```

use super::base::{element, icon, label, under};
use super::{color, geo, shadow};
use gpui::prelude::FluentBuilder;
use gpui::*;

/// The checkbox: 16pt, corners 4, on an `input` hairline with shadow-xs; on, `primary` with a 14pt
/// `primary-foreground` tick (`[&>svg]:size-3.5`).
#[derive(IntoElement)]
pub struct Checkbox { base: Div, on: bool, partial: bool, disabled: bool }
impl Styled for Checkbox { fn style(&mut self) -> &mut StyleRefinement { self.base.style() } }
impl Checkbox {
    pub fn new(on: bool) -> Self { Checkbox { base: div(), on, partial: false, disabled: false } }
    /// Some of a set checked (`data-state=indeterminate`): filled like on, a dash for the tick.
    pub fn partial(mut self, on: bool) -> Self { self.partial = on; self }
    pub fn disabled(mut self, on: bool) -> Self { self.disabled = on; self }
}
impl RenderOnce for Checkbox {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let g = geo();
        let mut d = self.base;
        let caller = std::mem::take(d.style());
        let d = d.size(px(g.checkbox)).flex_none().flex().items_center().justify_center().rounded(px(g.checkbox_r)).when(self.disabled, |d| d.opacity(0.5));
        let mut d = if self.on { d.bg(color::primary()).child(icon("todo-check", 14., color::primary_foreground())) }
            else if self.partial { d.bg(color::primary()).child(div().w(px(8.)).h(px(2.)).rounded(px(1.)).bg(color::primary_foreground())) }
            else { d.border_1().border_color(color::input()).bg(color::field()).shadow(shadow::xs()) };
        d.style().refine(&caller);
        d
    }
}

/// A switch, 32×18.4 (a 1pt clear border round a 16pt thumb) with shadow-xs: `primary` on, `input`
/// off (`input/80` in the dark); the thumb `background`, in the dark `foreground` off and
/// `primary-foreground` on.
#[derive(IntoElement)]
pub struct Switch { base: Div, on: bool, disabled: bool }
impl Styled for Switch { fn style(&mut self) -> &mut StyleRefinement { self.base.style() } }
impl Switch {
    pub fn new(on: bool) -> Self { Switch { base: div(), on, disabled: false } }
    pub fn disabled(mut self, on: bool) -> Self { self.disabled = on; self }
}
impl RenderOnce for Switch {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (g, dark, on) = (geo(), crate::theme::dark(), self.on);
        let thumb = if !dark { color::popover() } else if on { color::primary_foreground() } else { color::foreground() };
        let mut d = self.base;
        let caller = std::mem::take(d.style());
        let mut d = d.flex_none().w(px(g.switch_w)).h(px(g.switch_h)).px(px(1.)).rounded_full().flex().items_center()
            .bg(if on { color::primary() } else { color::switch_off() }).shadow(shadow::xs()).when(on, |d| d.justify_end()).when(self.disabled, |d| d.opacity(0.5))
            .child(div().size(px(g.thumb)).rounded_full().bg(thumb));
        d.style().refine(&caller);
        d
    }
}

/// A segmented control's track (`TabsList`: `muted`, `rounded-lg`, p-[3px], h-9); its segments are
/// `Segment`s.
#[derive(IntoElement)]
pub struct Segmented { base: Div }
impl Styled for Segmented { fn style(&mut self) -> &mut StyleRefinement { self.base.style() } }
impl ParentElement for Segmented { fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) { self.base.extend(elements) } }
impl Default for Segmented { fn default() -> Self { Self::new() } }
impl Segmented {
    pub fn new() -> Self { Segmented { base: div() } }
    /// A track of `opts` with `picked` raised, each segment `(id, i)`.
    pub fn of(id: &'static str, opts: &[&str], picked: usize) -> Self {
        let mut t = Self::new();
        for (i, o) in opts.iter().enumerate() { t = t.child(Segment::new((id, i), o.to_string()).selected(i == picked)); }
        t
    }
}
impl RenderOnce for Segmented {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let g = geo();
        let mut d = self.base;
        let caller = std::mem::take(d.style());
        let mut d = d.flex_none().h(px(g.seg_h)).p(px(g.seg_pad)).rounded(px(g.seg_r)).bg(color::muted()).flex().items_center();
        d.style().refine(&caller);
        d
    }
}

/// One segment (`TabsTrigger`: the track's height less 1, `rounded-md`, px-2, gap-1.5, a clear 1pt
/// border; `foreground/60`, `muted-foreground` in the dark, `foreground` under the pointer). Picked:
/// `background` with shadow-sm and `foreground`; in the dark `input/30` on an `input` border.
#[derive(IntoElement)]
pub struct Segment { base: Stateful<Div>, label: SharedString, icon: Option<SharedString>, selected: bool, disabled: bool }
element!(Segment);
impl Segment {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self { Segment { base: div().id(id), label: label.into(), icon: None, selected: false, disabled: false } }
    pub fn disabled(mut self, on: bool) -> Self { self.disabled = on; self }
    /// A glyph before the label; with an empty label, an icon-only segment (a view switch).
    pub fn icon(mut self, name: impl Into<SharedString>) -> Self { self.icon = Some(name.into()); self }
    pub fn selected(mut self, on: bool) -> Self { self.selected = on; self }
}
impl RenderOnce for Segment {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (g, on, t, ic, off, dark) = (geo(), self.selected, self.label, self.icon, self.disabled, crate::theme::dark());
        let fg = if on { color::foreground() } else if dark { color::muted_foreground() } else { super::alpha(color::foreground(), 0.6) };
        under(self.base, |d| {
            let d = d.flex_1().h(px(g.seg_h - 2. * g.seg_pad - 1.)).px(px(8.)).flex().items_center().justify_center().gap(px(6.)).rounded(px(g.button_r)).border_1().border_color(gpui::transparent_black())
                .whitespace_nowrap().text_size(px(g.text)).text_color(fg).when(off, |d| d.opacity(0.5)).when(!off, |d| d.cursor_pointer())
                .when_some(ic, |d, n| d.child(icon(&n, 16., fg))).when(!t.is_empty(), |d| d.child(label(t)));
            if !on { return d.when(!off, |d| d.hover(|s| s.text_color(color::foreground()))) }
            if dark { d.bg(super::alpha(color::input(), 0.3)).border_color(color::input()) } else { d.bg(color::background()).shadow(shadow::sm()) }
        })
    }
}

/// A radio (`RadioGroupItem`: a 16pt disc on an `input` hairline, `input/30` in the dark; picked,
/// `primary` with an 8pt `primary-foreground` dot). The caller's row takes the click.
#[derive(IntoElement)]
pub struct Radio { base: Div, on: bool, disabled: bool }
impl Styled for Radio { fn style(&mut self) -> &mut StyleRefinement { self.base.style() } }
impl Radio {
    pub fn new(on: bool) -> Self { Radio { base: div(), on, disabled: false } }
    pub fn disabled(mut self, on: bool) -> Self { self.disabled = on; self }
}
impl RenderOnce for Radio {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let g = geo();
        let mut d = self.base;
        let caller = std::mem::take(d.style());
        let mut d = d.size(px(g.checkbox)).flex_none().flex().items_center().justify_center().rounded_full().border_1()
            .bg(if self.on { color::primary() } else { color::field() }).border_color(if self.on { color::primary() } else { color::input() }).when(self.disabled, |d| d.opacity(0.5))
            .when(self.on, |d| d.child(div().size(px(8.)).rounded_full().bg(color::primary_foreground())));
        d.style().refine(&caller);
        d
    }
}
