//! Form fields (shadcn's `Input`, `SelectTrigger`, `Textarea`, `Label`): the box a text input or a
//! value sits in. The text editing itself stays with the caller's input (`crate::input`); these draw
//! its box.
//!
//! ```ignore
//! Field::new("title").focused(editing).child(input)
//! Field::select("type").sized(FieldSize::Sm).open(menu_open).child(value)
//! ```

use super::base::{element, icon, label, under};
use super::{color, geo, shadow};
use gpui::prelude::FluentBuilder;
use gpui::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FieldSize {
    /// h-9: forms and dialogs.
    Default,
    /// h-8: a search box in a menu, a value in a panel line.
    Sm,
}

/// A one-line field: `input` hairline with shadow-xs, `rounded-md`, px-2.5 (white in the light look,
/// `input/30` in the dark). With the keyboard in it (`focused`), a `ring` edge and a 3pt `ring/50`
/// halo; `invalid`, a `destructive` edge and halo; disabled, the whole box at 50%. A select
/// (`SelectTrigger`, `Field::select`: pl-2.5 pr-2, gap-1.5) adds a 16pt `muted-foreground` chevron and,
/// in the dark, the pointer's fill (`input/50`); open, it looks as it does shut (the list takes the
/// keyboard, so the trigger has no ring).
#[derive(IntoElement)]
pub struct Field { base: Stateful<Div>, size: FieldSize, focused: bool, select: bool, disabled: bool, invalid: bool, children: Vec<AnyElement> }
element!(Field);
impl ParentElement for Field {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) { self.children.extend(elements) }
}

impl Field {
    pub fn new(id: impl Into<ElementId>) -> Self { Field { base: div().id(id), size: FieldSize::Default, focused: false, select: false, disabled: false, invalid: false, children: vec![] } }
    /// A select's trigger: the field's box with a chevron at the end.
    pub fn select(id: impl Into<ElementId>) -> Self { let mut f = Self::new(id); f.select = true; f }
    /// A select showing `value` (the small one, as in a panel line).
    pub fn select_value(id: impl Into<ElementId>, value: impl Into<SharedString>) -> Self {
        Self::select(id).sized(FieldSize::Sm).child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(label(value.into())))
    }
    pub fn sized(mut self, s: FieldSize) -> Self { self.size = s; self }
    /// The keyboard is in it.
    pub fn focused(mut self, on: bool) -> Self { self.focused = on; self }
    /// A select whose list is open: as shadcn's trigger, no change to the box (kept so callers can say).
    pub fn open(self, _on: bool) -> Self { self }
    pub fn disabled(mut self, on: bool) -> Self { self.disabled = on; self }
    /// The value is wrong (`aria-invalid`).
    pub fn invalid(mut self, on: bool) -> Self { self.invalid = on; self }
}

impl RenderOnce for Field {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let g = geo();
        let h = match self.size { FieldSize::Default => g.field_h, FieldSize::Sm => g.field_sm_h };
        let r = match self.size { FieldSize::Default => g.field_r, FieldSize::Sm => g.field_sm_r };
        let (focused, select, off, bad, children) = (self.focused, self.select, self.disabled, self.invalid, self.children);
        under(self.base, |d| {
            let d = d.flex_none().h(px(h)).pl(px(g.field_px)).pr(px(if select { 8. } else { g.field_px })).flex().items_center().gap(px(6.)).rounded(px(r)).border_1()
                .text_size(px(g.text)).text_color(color::foreground()).bg(color::field()).when(off, |d| d.opacity(0.5));
            let d = edge(d, focused, bad);
            d.when(select && !off, |d| d.cursor_pointer().hover(|s| s.bg(color::field_hover())))
                .children(children)
                .when(select, |d| d.child(icon("chevron-down", 16., color::muted_foreground()).ml_auto()))
        })
    }
}

/// A field's edge: `ring` and a 3pt `ring/50` halo with the keyboard in it; `destructive` (`/50` in
/// the dark) and a `destructive/20` (`/40`) halo when invalid; else `input` with shadow-xs.
fn edge(d: Stateful<Div>, focused: bool, bad: bool) -> Stateful<Div> {
    let dark = crate::theme::dark();
    if bad {
        let halo = super::alpha(color::destructive(), if dark { 0.4 } else { 0.2 });
        let d = d.border_color(if dark { super::alpha(color::destructive(), 0.5) } else { color::destructive() });
        d.shadow(vec![shadow::ring(halo, 3.)])
    } else if focused { d.border_color(color::ring()).shadow(vec![shadow::ring(color::focus_ring(), 3.)]) } else { d.border_color(color::input()).shadow(shadow::xs()) }
}

/// A box that holds more than a line (`Textarea`: `rounded-md`, px-2.5 py-2, at least 64 tall).
#[derive(IntoElement)]
pub struct Textarea { base: Stateful<Div>, focused: bool, disabled: bool, invalid: bool }
element!(Textarea, parent);
impl Textarea {
    pub fn new(id: impl Into<ElementId>) -> Self { Textarea { base: div().id(id), focused: false, disabled: false, invalid: false } }
    pub fn focused(mut self, on: bool) -> Self { self.focused = on; self }
    pub fn disabled(mut self, on: bool) -> Self { self.disabled = on; self }
    pub fn invalid(mut self, on: bool) -> Self { self.invalid = on; self }
}
impl RenderOnce for Textarea {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (g, focused, off, bad) = (geo(), self.focused, self.disabled, self.invalid);
        under(self.base, |d| {
            let d = d.min_h(px(64.)).px(px(g.area_pad)).py(px(8.)).rounded(px(g.area_r)).bg(color::field()).border_1().text_size(px(g.text)).text_color(color::foreground())
                .when(off, |d| d.opacity(0.5));
            edge(d, focused, bad)
        })
    }
}

/// A field's label (`Label`: text-sm `leading-none`, `foreground`), `label_gap` above its control;
/// disabled, at 50%.
#[derive(IntoElement)]
pub struct Label { text: SharedString, muted: bool, disabled: bool }
impl Label {
    pub fn new(text: impl Into<SharedString>) -> Self { Label { text: text.into(), muted: false, disabled: false } }
    /// A caption rather than a label (`FieldDescription`: text-sm, `muted-foreground`, `leading-normal`).
    pub fn muted(mut self) -> Self { self.muted = true; self }
    pub fn disabled(mut self, on: bool) -> Self { self.disabled = on; self }
}
impl RenderOnce for Label {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let g = geo();
        div().flex_none().whitespace_nowrap().text_size(px(g.text)).when(self.disabled, |d| d.opacity(0.5))
            .when(!self.muted, |d| d.line_height(px(g.text)).text_color(color::foreground()))
            .when(self.muted, |d| d.line_height(px(g.text * 1.5)).text_color(color::muted_foreground())).child(label(self.text))
    }
}
