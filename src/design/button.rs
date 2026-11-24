//! Buttons (shadcn's `Button`): five variants in four sizes, a label and / or a glyph.
//!
//! ```ignore
//! Button::primary("ok").label("确定").on_click(…)
//! Button::ghost("undo").icon("undo").sized(ButtonSize::Tool).tip("撤销", None)
//! ```

use super::base::{element, icon, label, under};
use super::{color, geo, shadow};
use gpui::prelude::FluentBuilder;
use gpui::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ButtonVariant {
    /// `primary` under `primary-foreground`; `primary/80` under the pointer.
    Primary,
    /// A `border` hairline with shadow-xs on the page (`input` on `input/30` in the dark); `muted`
    /// under the pointer.
    Outline,
    /// `secondary`, a shade deeper under the pointer.
    Secondary,
    /// Text only: `muted` under the pointer (`muted/50` in the dark) and when on (`selected`, as
    /// shadcn's `Toggle` and an `aria-expanded` trigger). On a raised surface in the dark, where
    /// `muted` is the surface, a step up: `accent_hover` under the pointer, `accent` when on.
    Ghost,
    /// `destructive` text on `destructive/10`.
    Destructive,
    /// `primary` text, underlined under the pointer.
    Link,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ButtonSize {
    /// h-9: dialogs and forms.
    Default,
    /// h-8: panels.
    Sm,
    /// 28: toolbars, a panel's tools (ours: shadcn has 24 and 32, see DESIGN_SYSTEM.md).
    Tool,
    /// h-6 (`xs` / `icon-xs`): the small ✕ and arrows inside a field or a bar; text-xs, 12pt glyphs.
    Xs,
    /// h-10 (`lg` / `icon-lg`).
    Lg,
}

impl ButtonSize {
    fn h(self) -> f32 { let g = geo(); match self { ButtonSize::Default => g.button_h, ButtonSize::Sm => g.button_sm_h, ButtonSize::Tool => g.tool_h, ButtonSize::Xs => 24., ButtonSize::Lg => 40. } }
    /// `[&_svg]:size-4`, `size-3` at xs.
    fn icon(self) -> f32 { match self { ButtonSize::Xs => 12., _ => 16. } }
    /// px-2.5, px-2 at xs.
    fn px(self) -> f32 { match self { ButtonSize::Default | ButtonSize::Sm | ButtonSize::Lg => geo().button_px, ButtonSize::Tool | ButtonSize::Xs => 8. } }
    /// The side a glyph is on (`has-data-[icon=inline-start]`): pl-2, pl-1.5 at sm and xs.
    fn px_icon(self) -> f32 { match self { ButtonSize::Default | ButtonSize::Lg => 8., _ => 6. } }
    /// gap-1.5, gap-1 at sm and xs.
    fn gap(self) -> f32 { match self { ButtonSize::Default | ButtonSize::Lg => 6., _ => 4. } }
}

#[derive(IntoElement)]
pub struct Button {
    base: Stateful<Div>,
    variant: ButtonVariant,
    size: ButtonSize,
    icon: Option<SharedString>,
    editor: bool,
    icon_end: Option<SharedString>,
    label: Option<SharedString>,
    selected: bool,
    disabled: bool,
}
element!(Button, parent);

impl Button {
    pub fn new(id: impl Into<ElementId>, variant: ButtonVariant) -> Self {
        Button { base: div().id(id), variant, size: ButtonSize::Default, icon: None, editor: false, icon_end: None, label: None, selected: false, disabled: false }
    }
    pub fn primary(id: impl Into<ElementId>) -> Self { Self::new(id, ButtonVariant::Primary) }
    pub fn outline(id: impl Into<ElementId>) -> Self { Self::new(id, ButtonVariant::Outline) }
    pub fn secondary(id: impl Into<ElementId>) -> Self { Self::new(id, ButtonVariant::Secondary) }
    /// A quiet button; its size is a toolbar's (28) unless asked otherwise.
    pub fn ghost(id: impl Into<ElementId>) -> Self { Self::new(id, ButtonVariant::Ghost).sized(ButtonSize::Tool) }
    pub fn destructive(id: impl Into<ElementId>) -> Self { Self::new(id, ButtonVariant::Destructive) }
    pub fn link(id: impl Into<ElementId>) -> Self { Self::new(id, ButtonVariant::Link) }

    pub fn sized(mut self, s: ButtonSize) -> Self { self.size = s; self }
    /// Another look for a button built elsewhere (a shared ⋯ button floating on a tile).
    pub fn variant(mut self, v: ButtonVariant) -> Self { self.variant = v; self }
    pub fn label(mut self, s: impl Into<SharedString>) -> Self { self.label = Some(s.into()); self }
    /// A glyph before the label; alone, the button is square.
    pub fn icon(mut self, name: impl Into<SharedString>) -> Self { let n = name.into(); if !n.is_empty() { self.icon = Some(n) } self }
    /// The editor's glyph of that name before the label (its set draws the marks and block types).
    pub fn editor_icon(mut self, name: impl Into<SharedString>) -> Self { self.editor = true; self.icon(name) }
    /// A glyph after the label (a chevron); with `icon` and no label, a menu's trigger (not square).
    pub fn icon_end(mut self, name: impl Into<SharedString>) -> Self { self.icon_end = Some(name.into()); self }
    /// Pressed or in use (a toolbar tool with a filter on, a menu's trigger while it is open:
    /// `aria-expanded` / `aria-pressed`): `muted` (one step up on a raised surface in the dark).
    pub fn selected(mut self, on: bool) -> Self { self.selected = on; self }
    /// Not available: the whole button at 50% (`disabled:opacity-50`), no pointer.
    pub fn disabled(mut self, on: bool) -> Self { self.disabled = on; self }
}

impl RenderOnce for Button {
    fn render(mut self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (g, dark, v, sz) = (geo(), crate::theme::dark(), self.variant, self.size);
        // the press nudge moves a relative box; one the caller placed absolutely keeps still
        let placed = self.base.style().position == Some(Position::Absolute) || self.base.style().inset.top.is_some();
        let square = self.label.is_none() && self.icon_end.is_none();
        // the glyph takes the text's colour (`[&_svg]` inherits `currentColor`)
        let fg = match v {
            ButtonVariant::Primary => color::primary_foreground(),
            ButtonVariant::Destructive => color::destructive(),
            ButtonVariant::Link => color::primary(),
            _ => color::foreground(),
        };
        // `rounded-md`; `rounded-[min(var(--radius-md),8px)]` at xs and `…10px` at sm: 8 either way at R = 10
        let r = g.button_r.min(if sz == ButtonSize::Xs { 8. } else { 10. });
        let (icon_l, icon_r, label_s, disabled, selected, editor) = (self.icon, self.icon_end, self.label, self.disabled, self.selected, self.editor);
        // a ghost's fills: `muted` (`muted/50` under the pointer in the dark); on a raised surface in the dark, a step up
        let (ghost_hover, ghost_on) = super::toolbar::ghost_fills(false);
        under(self.base, |d| {
            let d = d.flex_none().h(px(sz.h())).flex().items_center().justify_center().gap(px(sz.gap())).rounded(px(r))
                .text_size(px(if sz == ButtonSize::Xs { g.text_xs } else { g.text })).text_color(fg).whitespace_nowrap()
                .when(square, |d| d.w(px(sz.h())))
                .when(!square, |d| d.pl(px(if icon_l.is_some() { sz.px_icon() } else { sz.px() })).pr(px(if icon_r.is_some() { sz.px_icon() } else { sz.px() })))
                .when(!disabled, |d| d.cursor_pointer()).when(disabled, |d| d.opacity(0.5));
            let d = match v {
                ButtonVariant::Primary => d.bg(color::primary()).when(!disabled, |d| d.hover(|s| s.bg(color::primary_hover()))),
                ButtonVariant::Outline => d.border_1().border_color(if dark { color::input() } else { color::border() })
                    .bg(if selected { color::muted() } else if dark { color::field() } else { color::background() }).shadow(shadow::xs())
                    .when(!disabled, |d| d.hover(|s| s.bg(if dark { color::field_hover() } else { color::muted() }))),
                ButtonVariant::Secondary => d.bg(color::secondary()).when(!disabled, |d| d.hover(|s| s.bg(color::secondary_hover()))),
                ButtonVariant::Ghost => d.when(selected, |d| d.bg(ghost_on))
                    .when(!disabled, |d| d.hover(move |s| s.bg(if selected { ghost_on } else { ghost_hover }))),
                ButtonVariant::Destructive => d.bg(color::destructive_soft()).when(!disabled, |d| d.hover(|s| s.bg(color::destructive_hover()))),
                ButtonVariant::Link => d.when(!disabled, |d| d.hover(|s| s.underline())),
            };
            // `active:translate-y-px`, not on a menu's trigger (`aria-haspopup`): those come `selected` while open
            let d = d.when(!disabled && !selected && !placed, |d| d.relative().active(|s| s.top(px(1.))));
            d.when_some(icon_l, |d, n| d.child(if editor { crate::editor_ui::icon(&n, sz.icon(), fg) } else { icon(&n, sz.icon(), fg) }))
                .when_some(label_s, |d, s| d.child(label(s)))
                .when_some(icon_r, |d, n| d.child(icon(&n, sz.icon(), fg)))
        })
    }
}
