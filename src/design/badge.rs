//! Small marks (shadcn's `Badge`, `Kbd`): an option's tag, a key cap.

use super::base::{icon, label};
use super::{chip, color, geo};
use gpui::prelude::FluentBuilder;
use gpui::*;

/// A badge (h-5, a pill, px-2, gap-1, text-xs): an option's colour (`chip::colors`, ours), or one
/// of shadcn's variants: `secondary` (`Badge::new`), `primary`, `outline` (a `border` edge),
/// `destructive` (`destructive/10`, `/20` in the dark).
#[derive(IntoElement)]
pub struct Badge { base: Div, text: SharedString, colors: (Hsla, Hsla), outline: bool, close: bool }
impl Styled for Badge { fn style(&mut self) -> &mut StyleRefinement { self.base.style() } }
impl Badge {
    /// A grey badge.
    pub fn new(text: impl Into<SharedString>) -> Self { Badge { base: div(), text: text.into(), colors: (color::secondary(), color::secondary_foreground()), outline: false, close: false } }
    pub fn primary(text: impl Into<SharedString>) -> Self { Self::colored(text, (color::primary(), color::primary_foreground())) }
    pub fn outline(text: impl Into<SharedString>) -> Self { Badge { outline: true, ..Self::colored(text, (gpui::transparent_black(), color::foreground())) } }
    pub fn destructive(text: impl Into<SharedString>) -> Self { Self::colored(text, (color::destructive_soft(), color::destructive())) }
    /// A badge in (fill, text).
    pub fn colored(text: impl Into<SharedString>, colors: (Hsla, Hsla)) -> Self { Badge { colors, ..Self::new(text) } }
    /// The `i`th option colour (0–8; 9 the grey one).
    pub fn chip(text: impl Into<SharedString>, i: usize) -> Self { Self::colored(text, if i < 9 { chip::colors(i) } else { chip::neutral() }) }
    /// A ✕ at the end (a filter chip: `data-icon=inline-end`, pr-1.5, the glyph 12 in the text's colour).
    pub fn closable(mut self) -> Self { self.close = true; self }
}
impl RenderOnce for Badge {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let g = geo();
        let (bg, fg) = self.colors;
        let mut d = self.base;
        let caller = std::mem::take(d.style());
        let mut d = d.flex_none().h(px(g.badge_h)).max_w(px(180.)).px(px(g.badge_px)).rounded(px(g.badge_r)).bg(bg).border_1().border_color(if self.outline { color::border() } else { gpui::transparent_black() })
            .flex().items_center().gap(px(4.)).text_size(px(g.text_xs)).text_color(fg).overflow_hidden().whitespace_nowrap().text_ellipsis()
            .child(label(self.text))
            .when(self.close, |d| d.pr(px(6.)).child(icon("x", 12., fg).flex_none()));
        d.style().refine(&caller);
        d
    }
}

/// A key cap in a shortcut hint (`Kbd`: h-5, min-w-5, `rounded-sm`, `muted` under `muted-foreground`).
#[derive(IntoElement)]
pub struct Kbd { key: SharedString }
impl Kbd { pub fn new(key: impl Into<SharedString>) -> Self { Kbd { key: key.into() } } }
impl RenderOnce for Kbd {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let g = geo();
        div().flex_none().h(px(20.)).min_w(px(20.)).px(px(4.)).rounded(px(g.menu_row_r)).bg(color::muted()).flex().items_center().justify_center()
            .text_size(px(g.text_xs)).text_color(color::muted_foreground()).child(self.key)
    }
}

/// A shortcut as key caps (`KbdGroup`: gap-1): each stroke's keys side by side, strokes apart with a
/// `muted-foreground` → between them (a chord such as ⌘K → ⌘S).
#[derive(IntoElement)]
pub struct KbdGroup { strokes: Vec<Vec<String>> }
impl KbdGroup { pub fn new(strokes: Vec<Vec<String>>) -> Self { KbdGroup { strokes } } }
impl RenderOnce for KbdGroup {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let g = geo();
        div().flex_none().flex().items_center().gap(px(4.)).children(self.strokes.into_iter().enumerate().flat_map(|(i, keys)| {
            (i > 0).then(|| div().text_size(px(g.text_xs)).text_color(color::muted_foreground()).child("→").into_any_element()).into_iter()
                .chain(keys.into_iter().map(|k| Kbd::new(k).into_any_element()))
        }))
    }
}
