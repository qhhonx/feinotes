//! Quiet.md's design system (settled 2026-11-22, docs/design/DESIGN_SYSTEM.md): shadcn/ui (MIT),
//! **Nova's colours** — the Neutral theme, fields on a hairline, `accent` under the pointer, `muted`
//! for what is selected — on **Vega's geometry**: 36pt controls at 8pt corners, 8pt menus over 6pt
//! rows of 32, popovers padded 16, a 40pt table head over 8pt cell padding; all corners hang off one
//! base radius, `R`. Popovers and cards take **affectUI's edge** (MIT, github.com/flohoeller/affectui;
//! `shadow::popover`, `shadow::frame`): one 0.5pt ring and a soft drop, not shadcn's 1px ring over a
//! shadow.
//!
//! Tokens live here; the components built from them in the submodules (docs/design/COMPONENTS.md),
//! all of them live in the 组件库 panel (`crate::gallery`, `quiet_md --components`). Every token reads `theme::dark()` when called.
//! shadcn's colours are OKLCH; the hex here is their sRGB.

// A token set: the whole of it is kept, ahead of the components that will use each token.
#![allow(dead_code)]

mod base;
pub mod badge;
pub mod button;
pub mod card;
pub mod control;
pub mod dialog;
pub mod field;
pub mod menu;
pub mod scroll;
pub mod toolbar;

// The library's surface, whether or not every piece has a caller yet.
#[allow(unused_imports)]
pub use base::icon;
#[allow(unused_imports)]
pub use badge::{Badge, Kbd, KbdGroup};
#[allow(unused_imports)]
pub use button::{Button, ButtonSize, ButtonVariant};
#[allow(unused_imports)]
pub use card::{Card, Item, Separator, Toast, ToastTone};
#[allow(unused_imports)]
pub use control::{Checkbox, Radio, Segment, Segmented, Switch};
#[allow(unused_imports)]
pub use dialog::{Dialog, DialogFocus, scrim};
#[allow(unused_imports)]
pub use field::{Field, FieldSize, Label, Textarea};
#[allow(unused_imports)]
pub use scroll::ScrollBar;
pub use menu::{Align, MenuItem, MenuLabel, MenuSeparator, Popover, hang, room};
pub use toolbar::{ButtonGroup, ButtonGroupSeparator, Toggle, ToggleSize, Toolbar};

use crate::theme::{dark, rgbaf};
use gpui::{BoxShadow, Hsla, point, px, rgb};

/// The neutral ramp (Tailwind's neutral, which shadcn's Neutral theme is built from).
pub mod neutral {
    pub const N0: u32 = 0xffffff;
    /// oklch(0.985 0 0)
    pub const N50: u32 = 0xfafafa;
    /// oklch(0.97 0 0)
    pub const N100: u32 = 0xf5f5f5;
    /// oklch(0.922 0 0)
    pub const N200: u32 = 0xe5e5e5;
    pub const N300: u32 = 0xd4d4d4;
    /// oklch(0.708 0 0)
    pub const N400: u32 = 0xa1a1a1;
    /// oklch(0.556 0 0)
    pub const N500: u32 = 0x737373;
    /// oklch(0.439 0 0)
    pub const N600: u32 = 0x525252;
    /// oklch(0.371 0 0)
    pub const N700: u32 = 0x404040;
    /// oklch(0.269 0 0)
    pub const N800: u32 = 0x262626;
    /// oklch(0.205 0 0)
    pub const N900: u32 = 0x171717;
    /// oklch(0.145 0 0)
    pub const N950: u32 = 0x0a0a0a;
}

fn hex(v: u32) -> Hsla { rgb(v).into() }
/// `light` in the light look, `dark` in the dark one.
fn pick(light: u32, dark_v: u32) -> Hsla { hex(if dark() { dark_v } else { light }) }
/// `c / a` in Tailwind: the colour at an opacity.
pub fn alpha(mut h: Hsla, a: f32) -> Hsla { h.a *= a; h }

/// Semantic colours: shadcn's theme variables (Neutral).
///
/// One change for shadcn's dark look: its page is 0.145 and its popovers 0.205, but Quiet.md's dark
/// page is lighter (#1c1c1c), so popovers and cards go one step up (0.269) and `accent`, drawn on
/// them, with them (0.371, shadcn's own dark accent on its site).
pub mod color {
    use super::{alpha, hex, neutral::*, pick};
    use crate::theme::dark;
    use gpui::Hsla;

    /// Body text, labels, header names.
    pub fn foreground() -> Hsla { pick(N950, N50) }
    /// The page under everything: the note's own ground.
    pub fn background() -> Hsla { crate::theme::Theme::bg_body() }
    pub fn popover() -> Hsla { pick(N0, N800) }
    pub fn card() -> Hsla { popover() }
    pub fn primary() -> Hsla { pick(N900, N200) }
    pub fn primary_foreground() -> Hsla { pick(N50, N900) }
    pub fn secondary() -> Hsla { pick(N100, N800) }
    pub fn secondary_foreground() -> Hsla { pick(N900, N50) }
    /// What is selected.
    pub fn muted() -> Hsla { pick(N100, N800) }
    /// Captions, icons, row numbers.
    pub fn muted_foreground() -> Hsla { pick(N500, N400) }
    /// Placeholders: shadcn's `muted-foreground`.
    pub fn placeholder() -> Hsla { muted_foreground() }
    /// Under the pointer; a ghost button that is on.
    pub fn accent() -> Hsla { pick(N100, N700) }
    /// A ghost button under the pointer on a raised surface in the dark (white 6%, ours: there
    /// shadcn's `muted/50` would be the surface itself, and `accent`, #404040, is its "on"); `accent`
    /// in the light.
    pub fn accent_hover() -> Hsla { if dark() { alpha(hex(N0), 0.06) } else { accent() } }
    pub fn accent_foreground() -> Hsla { pick(N900, N50) }
    /// oklch(0.577 0.245 27.325) / oklch(0.704 0.191 22.216).
    pub fn destructive() -> Hsla { pick(0xe7000b, 0xff6467) }
    /// A destructive row or button: `destructive/10`, `/20` in the dark.
    pub fn destructive_soft() -> Hsla { alpha(destructive(), if dark() { 0.2 } else { 0.1 }) }
    /// A destructive button under the pointer: `destructive/20`, `/30` in the dark.
    pub fn destructive_hover() -> Hsla { alpha(destructive(), if dark() { 0.3 } else { 0.2 }) }
    pub fn border() -> Hsla { if dark() { alpha(hex(N0), 0.10) } else { hex(N200) } }
    pub fn input() -> Hsla { if dark() { alpha(hex(N0), 0.15) } else { hex(N200) } }
    /// The keyboard's and the selection's mark.
    pub fn ring() -> Hsla { pick(N400, N500) }

    /// A field's fill: `input/30` in the dark; in the light shadcn's is transparent over a white
    /// ground, drawn here as that white (GPUI paints a shadow under the box as well as round it,
    /// so a transparent field with shadow-xs would show it through).
    pub fn field() -> Hsla { if dark() { alpha(input(), 0.30) } else { hex(N0) } }
    /// A field under the pointer: `input/50` in the dark, nothing otherwise.
    pub fn field_hover() -> Hsla { if dark() { alpha(input(), 0.50) } else { field() } }
    /// `muted/50`: a table row under the pointer.
    pub fn muted_half() -> Hsla { alpha(muted(), 0.5) }
    /// `primary/80`: the primary button under the pointer.
    pub fn primary_hover() -> Hsla { alpha(primary(), 0.8) }
    /// The secondary button under the pointer: `secondary` mixed 5% toward `foreground`.
    pub fn secondary_hover() -> Hsla { let mut h = secondary(); h.l = h.l * 0.95 + foreground().l * 0.05; h }
    /// `ring/50`: the focus halo round a field with the keyboard in it.
    pub fn focus_ring() -> Hsla { alpha(ring(), 0.5) }
    /// An off switch's track: `input`, `input/80` in the dark.
    pub fn switch_off() -> Hsla { if dark() { alpha(input(), 0.8) } else { input() } }
    /// A disabled control's text.
    pub fn disabled() -> Hsla { alpha(muted_foreground(), 0.5) }
    /// shadcn's overlay under a dialog: `black/10`.
    pub fn overlay() -> Hsla { crate::theme::rgbaf(0, 0, 0, 0.1) }
}

/// The option colours of 单选 / 多选 (shadcn has none; ours): Tailwind's 200 fill under its 800
/// text, its 950 at 60% under the 400 / 500 in the dark.
pub mod chip {
    use super::{alpha, dark, hex};
    use gpui::Hsla;

    /// (light fill, light text, dark fill, dark fill opacity, dark text)
    pub struct Chip(pub u32, pub u32, pub u32, pub f32, pub u32);
    pub const BLUE: Chip = Chip(0xbedbff, 0x193cb8, 0x162456, 0.6, 0x8ec5ff);
    pub const LIME: Chip = Chip(0xd8f999, 0x3c6300, 0x192e03, 0.6, 0x7ccf00);
    pub const YELLOW: Chip = Chip(0xfff085, 0x894b00, 0x432004, 0.6, 0xf0b100);
    pub const ORANGE: Chip = Chip(0xffd6a7, 0x9f2d00, 0x441306, 0.6, 0xff6900);
    // rose-400 in the dark, not the 500 (#ff2056 read 4.4:1 on its fill), the same hue
    pub const ROSE: Chip = Chip(0xffccd3, 0xa50036, 0x4d0218, 0.6, 0xff637e);
    pub const PINK: Chip = Chip(0xfccee8, 0xa3004c, 0x510424, 0.6, 0xfb64b6);
    pub const PURPLE: Chip = Chip(0xe9d4ff, 0x6e11b0, 0x3c0366, 0.6, 0xc27aff);
    pub const CYAN: Chip = Chip(0xa2f4fd, 0x005f78, 0x053345, 0.6, 0x00d3f2);
    pub const SKY: Chip = Chip(0xb8e6fe, 0x00598a, 0x052f4a, 0.6, 0x00bcff);
    pub const ALL: [&Chip; 9] = [&BLUE, &LIME, &YELLOW, &ORANGE, &ROSE, &PINK, &PURPLE, &CYAN, &SKY];

    /// The `i`th colour (0–8), (fill, text).
    pub fn colors(i: usize) -> (Hsla, Hsla) {
        let c = ALL[i];
        if dark() { (alpha(hex(c.2), c.3), hex(c.4)) } else { (hex(c.0), hex(c.1)) }
    }
    /// The grey option: `secondary` under `muted-foreground`.
    /// In the light its text a step darker than `muted-foreground` (#737373 read 4.35:1 on `secondary`).
    pub fn neutral() -> (Hsla, Hsla) { (super::color::secondary(), if dark() { super::color::muted_foreground() } else { hex(0x6b6b6b) }) }
}

/// Corner radii past the `R` steps.
pub mod radius {
    /// Badges and switches (`rounded-4xl` / `rounded-full`).
    pub const PILL: f32 = 999.;
}

/// Type sizes (Tailwind's text-sm / text-xs).
pub mod text {
    pub const SM: f32 = 14.;
    pub const SM_LINE: f32 = 20.;
    pub const XS: f32 = 12.;
    pub const XS_LINE: f32 = 16.;
}

/// The geometry every component sizes itself by.
pub struct Geo {
    /// Body text and its small size.
    pub text: f32,
    pub text_xs: f32,
    /// Buttons: height, corners, side padding; the small (h-8) one.
    pub button_h: f32,
    pub button_r: f32,
    pub button_px: f32,
    pub button_sm_h: f32,
    /// Form fields and select triggers; the small ones (search boxes, panel values).
    pub field_h: f32,
    pub field_r: f32,
    pub field_sm_h: f32,
    pub field_sm_r: f32,
    pub field_px: f32,
    /// Multi-line boxes.
    pub area_r: f32,
    pub area_pad: f32,
    /// Menus: corners, padding; rows: height, corners, side padding, icon gap; a rule's inset.
    pub menu_r: f32,
    pub menu_pad: f32,
    pub menu_row_h: f32,
    pub menu_row_r: f32,
    pub menu_row_px: f32,
    pub menu_gap: f32,
    pub rule_inset: f32,
    /// Sidebar rows: the gap between them (the tree's, and the nav's above it), so a selected row
    /// and a hovered one beside it never touch.
    pub sidebar_gap: f32,
    /// Popovers holding a form: padding, gap between fields, a label over its field.
    pub popover_pad: f32,
    pub popover_gap: f32,
    /// Between the lines of a filter / sort panel.
    pub panel_gap: f32,
    pub label_gap: f32,
    /// Between a settings page's sections (a caption and its card): shadcn's settings forms, `gap-8`.
    pub section_gap: f32,
    /// Dialogs: corners, padding, the gap between header, body and footer.
    pub dialog_r: f32,
    pub dialog_pad: f32,
    pub dialog_gap: f32,
    /// Badges (option tags).
    pub badge_h: f32,
    pub badge_r: f32,
    pub badge_px: f32,
    pub checkbox: f32,
    pub checkbox_r: f32,
    pub switch_w: f32,
    pub switch_h: f32,
    pub thumb: f32,
    /// Toolbar buttons and icon buttons; their corners.
    pub tool_h: f32,
    pub tool_r: f32,
    /// A segmented control: height, inner padding, corners.
    pub seg_h: f32,
    pub seg_pad: f32,
    pub seg_r: f32,
    /// The grid: head, the two short row heights, cell padding, a table-in-a-page's corners.
    pub table_head: f32,
    pub row_short: f32,
    pub row_medium: f32,
    pub cell_px: f32,
    pub frame_r: f32,
    /// Cards in a page; a line in one (`Item`): least height, padding.
    pub card_r: f32,
    pub item_h: f32,
    pub item_px: f32,
    pub item_py: f32,
    /// Toasts: corners and padding; tooltips: corners and padding.
    pub toast_r: f32,
    pub toast_px: f32,
    pub toast_py: f32,
    pub tip_r: f32,
    pub tip_px: f32,
    pub tip_py: f32,
    /// The add-record line, the 统计 bar, a 分组 header.
    pub add_row_h: f32,
    pub footer_h: f32,
    pub group_h: f32,
}

/// The base radius (`--radius`, 0.625rem). Vega's corners all hang off it — `rounded-sm` R−4,
/// `rounded-md` R−2, `rounded-lg` R, `rounded-xl` R+4. To round the look (wanted in time), move a
/// component up a step, as Maia does (`R - 2.` → `R + 2.`), keeping inner = outer − padding; raising
/// `R` only scales every corner alike.
// Experiment 2026-11-23 (maintainer): R 12 with continuous corners (GPUI patch 0047); 14 tried first.
pub const R: f32 = 12.;

/// Vega's sizes (style-vega.css, measured on ui.shadcn.com/create): 36pt controls at `rounded-md`,
/// px-2.5; menus `rounded-md` p-1 over `rounded-sm` rows of px-2 py-1.5; popovers p-4 gap-4; dialogs
/// `rounded-xl` p-6 gap-6; tables h-10 heads over p-2 cells; cards `rounded-xl`.
pub const G: Geo = Geo {
    text: text::SM, text_xs: text::XS,
    button_h: 36., button_r: R - 2., button_px: 10., button_sm_h: 32.,
    field_h: 36., field_r: R - 2., field_sm_h: 32., field_sm_r: R - 2., field_px: 10.,
    area_r: R - 2., area_pad: 10.,
    menu_r: R - 2., menu_pad: 4., menu_row_h: 32., menu_row_r: R - 4., menu_row_px: 8., menu_gap: 8., rule_inset: 12., sidebar_gap: 1.,
    popover_pad: 16., popover_gap: 16., panel_gap: 8., label_gap: 8., section_gap: 32.,
    dialog_r: R + 4., dialog_pad: 24., dialog_gap: 24.,
    badge_h: 20., badge_r: radius::PILL, badge_px: 8.,
    checkbox: 16., checkbox_r: 4., switch_w: 32., switch_h: 18.4, thumb: 16.,
    tool_h: 28., tool_r: R - 2.,
    seg_h: 36., seg_pad: 3., seg_r: R,
    table_head: 40., row_short: 36., row_medium: 44., cell_px: 8., frame_r: R + 4., card_r: R + 4., item_h: 48., item_px: 16., item_py: 14.,
    toast_r: R + 8., toast_px: 16., toast_py: 16., tip_r: R - 2., tip_px: 12., tip_py: 6.,
    add_row_h: 36., footer_h: 40., group_h: 36.,
};

/// The geometry (a function, so a later density or step-up can choose).
pub fn geo() -> &'static Geo { &G }

/// Shadows (Tailwind's, and affectUI's for floating things), each heavier in the dark look so it
/// still reads.
pub mod shadow {
    use super::*;

    fn drop(a: f32, y: f32, blur: f32, spread: f32) -> BoxShadow {
        let a = if dark() { (a * 3.).min(0.6) } else { a };
        BoxShadow { color: rgbaf(0, 0, 0, a), offset: point(px(0.), px(y)), blur_radius: px(blur), spread_radius: px(spread), inset: false }
    }
    fn s(c: Hsla, y: f32, blur: f32, spread: f32) -> BoxShadow { BoxShadow { color: c, offset: point(px(0.), px(y)), blur_radius: px(blur), spread_radius: px(spread), inset: false } }
    /// affectUI's edge: a ring of `c`, `w` wide, drawn by the shadow.
    pub fn ring(c: Hsla, w: f32) -> BoxShadow { s(c, 0., 0., w) }
    /// shadow-xs: fields and outline buttons. None in the dark look, where those are translucent
    /// (`input/30`) and GPUI would paint the shadow through them.
    pub fn xs() -> Vec<BoxShadow> { if dark() { vec![] } else { vec![drop(0.05, 1., 2., 0.)] } }
    /// shadow-sm: the picked segment of a segmented control.
    pub fn sm() -> Vec<BoxShadow> { if dark() { vec![] } else { vec![drop(0.1, 1., 3., 0.), drop(0.1, 1., 2., -1.)] } }
    /// A card's edge (a table in a page): a 10% hairline 0.5pt wide (Vega's card ring,
    /// `ring-foreground/10`, drawn like affectUI's) and a 1–2pt drop. GPUI paints the shadow under the
    /// card, so the card needs its own opaque ground.
    pub fn frame() -> Vec<BoxShadow> {
        if dark() { return vec![ring(rgbaf(255, 255, 255, 0.12), 0.5), s(rgbaf(0, 0, 0, 0.4), 1., 2., 0.)]; }
        vec![ring(rgbaf(0, 0, 0, 0.10), 0.5), s(rgbaf(0, 0, 0, 0.1), 1., 1., -1.), s(rgbaf(0, 0, 0, 0.05), 1., 2., 0.)]
    }
    /// affectUI's `--aui-shadow-popover` (menus, popovers, dialogs): a 0.5pt ring and a soft drop
    /// read as one crisp edge, where Nova's `ring-foreground/10` border over shadow-md read as two.
    pub fn popover() -> Vec<BoxShadow> {
        if dark() { return vec![ring(rgbaf(255, 255, 255, 0.14), 0.5), s(rgbaf(0, 0, 0, 0.6), 12., 32., -8.)]; }
        vec![ring(rgbaf(0, 0, 0, 0.08), 0.5), s(rgbaf(0, 0, 0, 0.14), 8., 24., -6.), s(rgbaf(0, 0, 0, 0.06), 2., 6., -2.)]
    }
    /// A frozen column's edge over the columns scrolled under it (飞书's): shadow-sm turned to the
    /// right (`0 1px 3px / 0 1px 2px -1px`, black 10%), on an element at that edge.
    pub fn sticky() -> Vec<BoxShadow> {
        let side = |a: f32, x: f32, blur: f32, spread: f32| { let mut d = drop(a, 0., blur, spread); d.offset.x = px(x); d };
        vec![side(0.1, 1., 3., 0.), side(0.1, 1., 2., -1.)]
    }
    /// A dialog: the popover's edge over a deeper drop (it sits over a dimmed window).
    pub fn dialog() -> Vec<BoxShadow> {
        if dark() { return vec![ring(rgbaf(255, 255, 255, 0.14), 0.5), s(rgbaf(0, 0, 0, 0.6), 24., 48., -12.)]; }
        vec![ring(rgbaf(0, 0, 0, 0.08), 0.5), s(rgbaf(0, 0, 0, 0.18), 24., 48., -12.), s(rgbaf(0, 0, 0, 0.06), 4., 12., -4.)]
    }
}
