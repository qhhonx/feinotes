//! What every component shares: it wraps one `Stateful<Div>` that the caller styles and wires up
//! (`.w()`, `.on_click()`, `.tip()` …) before it renders, and on render lays the component's own
//! look *under* the caller's, so the caller's sizes and colours win. The pointer's look (`hover`) is
//! the component's: ask for a state (`selected`, `disabled`) instead of restyling it.

use gpui::*;

/// `d` with `own` applied first and the caller's refinements (already on `d`) over it.
pub(crate) fn under(mut d: Stateful<Div>, own: impl FnOnce(Stateful<Div>) -> Stateful<Div>) -> Stateful<Div> {
    let caller = std::mem::take(d.style());
    let mut d = own(d);
    d.style().refine(&caller);
    d
}

/// Text in the UI face that keeps full-width punctuation at full width.
pub(crate) fn label(s: SharedString) -> crate::ui::UiText { crate::ui::ui_text(s) }

/// The traits that let a component take the caller's styling, handlers and tooltips.
macro_rules! element {
    ($t:ty) => {
        impl gpui::Styled for $t { fn style(&mut self) -> &mut gpui::StyleRefinement { gpui::Styled::style(&mut self.base) } }
        impl gpui::InteractiveElement for $t { fn interactivity(&mut self) -> &mut gpui::Interactivity { self.base.interactivity() } }
        impl gpui::StatefulInteractiveElement for $t {}
        impl crate::ui::TipExt for $t {
            fn tip_label(mut self, label: String, side: crate::ui::TipSide) -> Self { self.base = self.base.tip_label(label, side); self }
        }
    };
    ($t:ty, parent) => {
        element!($t);
        impl gpui::ParentElement for $t {
            fn extend(&mut self, elements: impl IntoIterator<Item = gpui::AnyElement>) { self.base.extend(elements) }
        }
    };
}
pub(crate) use element;

/// Glyphs the editor's icon set has and the shell's doesn't.
const EDITOR_ICONS: &[&str] = &["calendar", "columns-2", "table", "layout-grid", "grip-vertical", "maximize", "undo", "redo", "download", "upload", "link", "paperclip",
    "user", "trash-2", "list-tree", "history", "row-add", "column-add", "type", "image", "move", "wrap-text", "replace", "caret-down", "caret-up", "triangle-down",
    "triangle-right", "square-plus", "unlink", "todo", "circle-help", "view-card", "timer", "one-to-one", "swatch-none", "fit-page", "circle-check", "octagon-x"];

/// An icon from either set: the editor's `editor-icons/` for the names above and any the shell's
/// `icons/` lacks, else the shell's.
pub fn icon(name: &str, size: f32, color: Hsla) -> Svg {
    let editor = EDITOR_ICONS.contains(&name) || crate::assets::get(&format!("icons/{name}.svg")).is_none();
    if editor { crate::editor_ui::icon(name, size, color) } else { crate::ui::icon(name, size, color) }
}

thread_local! {
    /// Raised surfaces (a popover, a dialog, a card) being laid out around the current element.
    static RAISED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Whether the component being drawn sits on a raised surface: in the dark look that surface is
/// `muted` itself (#262626), so what shadcn fills with `muted` there steps up (`color::accent`).
pub(crate) fn raised() -> bool { RAISED.with(|r| r.get()) > 0 }

/// A raised surface's content: components laid out inside it know (`raised`).
pub(crate) struct Raised(pub AnyElement);
#[cfg(target_os = "ios")]
impl IntoElement for Raised {
    type Element = Self;
    fn into_element(self) -> Self { self }
}

impl Element for Raised {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> { None }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> { None }
    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        (in_raised(|| self.0.request_layout(window, cx)), ())
    }
    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) { self.0.prepaint(window, cx); }
    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) { self.0.paint(window, cx) }
}
pub(crate) fn in_raised<R>(f: impl FnOnce() -> R) -> R {
    RAISED.with(|r| r.set(r.get() + 1));
    let out = f();
    RAISED.with(|r| r.set(r.get() - 1));
    out
}
