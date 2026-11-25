//! The families the system has installed — 设置 › 界面字体 / 文档字体 pick from
//! these. `all_font_names` walks every descriptor in the collection, so the
//! list is read once per run (only once a family is chosen or the picker
//! opens) and shared by the picker and its "not installed" check.

use gpui::App;
use std::sync::{Arc, RwLock};

static FAMILIES: RwLock<Option<Arc<Vec<String>>>> = RwLock::new(None);

/// Every installed family, sorted ignoring case and without repeats (Windows
/// lists its system and custom collections one after the other, unsorted),
/// leaving out the private `.SystemUIFont`-style faces that only the platform
/// default does better.
pub fn families(cx: &App) -> Arc<Vec<String>> {
    if let Some(list) = FAMILIES.read().unwrap().as_ref() { return list.clone() }
    let list = Arc::new(tidy(cx.text_system().all_font_names()));
    *FAMILIES.write().unwrap() = Some(list.clone());
    list
}

fn tidy(mut list: Vec<String>) -> Vec<String> {
    list.retain(|name| !name.trim().is_empty() && !name.starts_with('.'));
    list.sort_by_cached_key(|f| f.to_lowercase());
    list.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    list
}

/// A downloaded family was registered (`font_downloads`): it counts as
/// installed from now on, and what punctuation it draws is probed again (a
/// probe before it arrived saw a fallback face).
pub fn added(family: &str) {
    if let Some(list) = FAMILIES.write().unwrap().as_mut() && !list.iter().any(|f| f.eq_ignore_ascii_case(family)) {
        let mut l = (**list).clone();
        l.push(family.to_string());
        *list = Arc::new(tidy(l));
    }
    OWN_GLYPHS.write().unwrap().retain(|(f, _)| !f.eq_ignore_ascii_case(family));
}

/// Whether `family` is installed, ignoring case (the platform's spelling of a
/// family need not match the one saved). On Windows also by its name in
/// another language (`other_name`).
pub fn installed(family: &str, cx: &App) -> bool {
    families(cx).iter().any(|f| f.eq_ignore_ascii_case(family)) || other_name(family, cx)
}

/// Known to be missing: the list has been read and lacks it. Before the list
/// is read nothing is known, and the family is used as saved.
fn missing(family: &str) -> bool {
    FAMILIES.read().unwrap().as_ref().is_some_and(|l| !l.iter().any(|f| f.eq_ignore_ascii_case(family)))
        && !OTHER_NAMES.read().unwrap().iter().any(|f| f.eq_ignore_ascii_case(family))
}

/// Families installed under a name the list does not show (`other_name`).
static OTHER_NAMES: RwLock<Vec<String>> = RwLock::new(Vec::new());

/// Windows lists each family by its name in the system's language: on a
/// Chinese one 微软雅黑, not Microsoft YaHei, which a library synced from a
/// Mac or an English system has saved. Such a family is still installed
/// when the text system finds it by that name (`resolves`); it is noted, so
/// `store` keeps it. Elsewhere the list is the answer, as before.
fn other_name(family: &str, cx: &App) -> bool {
    if !cfg!(target_os = "windows") || family.trim().is_empty() { return false }
    if OTHER_NAMES.read().unwrap().iter().any(|f| f.eq_ignore_ascii_case(family)) { return true }
    if families(cx).iter().any(|f| f.eq_ignore_ascii_case(family)) || !resolves(family, cx) { return false }
    OTHER_NAMES.write().unwrap().push(family.to_string());
    true
}

/// Whether the text system finds `family` itself: a family it does not have
/// resolves to the same fallback face as any other it does not have.
fn resolves(family: &str, cx: &App) -> bool {
    let ts = cx.text_system();
    ts.resolve_font(&gpui::font(family.to_string())) != ts.resolve_font(&gpui::font("Quiet.md: no such family"))
}

static CJK_PUNCT_MISSING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Find out once whether this system has `CJK_PUNCT_FONT` (a Mac always
/// does). Until then it is taken to.
pub fn check_cjk_punct(cx: &App) {
    use std::sync::atomic::Ordering;
    static CHECKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if cfg!(any(target_os = "macos", target_os = "ios")) || CHECKED.swap(true, Ordering::Relaxed) { return }
    CJK_PUNCT_MISSING.store(!resolves(crate::editor_ui::CJK_PUNCT_FONT, cx), Ordering::Relaxed);
}

/// Whether `CJK_PUNCT_FONT` is here to draw (`check_cjk_punct`).
pub fn cjk_punct_installed() -> bool { !CJK_PUNCT_MISSING.load(std::sync::atomic::Ordering::Relaxed) }

/// Put `family` in one of the font settings' slots (`theme::ui_family`,
/// `themes::doc_font`): empty, or a family that has been uninstalled since it
/// was picked, leaves the slot empty so the default face draws instead of
/// GPUI's last-resort fallback. The preference itself keeps the name, so
/// reinstalling the font brings it back.
pub fn store(slot: &RwLock<Option<&'static str>>, family: &str) {
    let family = family.trim();
    let family = if missing(family) { "" } else { family };
    let mut cur = slot.write().unwrap();
    let same = match *cur { Some(f) => f == family, None => family.is_empty() };
    // Preferences are applied on every save: only a real change costs the leak.
    if same { return }
    *cur = if family.is_empty() { None } else { Some(&*Box::leak(family.to_string().into_boxed_str())) };
}

/// The chosen families that have their own full-width punctuation and
/// Latin-width curly quotes: the text code swaps those characters to PingFang SC / Helvetica
/// Neue because the platform sans squeezes them, which would be wrong for a
/// family picked for its own (a CJK face's 「，」). Filled by `probe`; a
/// family not probed (a theme's, the system's) keeps the swap.
static OWN_GLYPHS: RwLock<Vec<(String, Glyphs)>> = RwLock::new(Vec::new());

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Glyphs { pub cjk_punct: bool, pub quotes: bool }

/// Record which of those characters `family` draws itself.
pub fn probe(family: &str, cx: &App) {
    let family = family.trim();
    if family.is_empty() || OWN_GLYPHS.read().unwrap().iter().any(|(f, _)| f == family) { return }
    other_name(family, cx);
    // a downloaded font still loading: probed once it is registered (`added`)
    if crate::font_downloads::by_family(family).is_some_and(|e| !crate::font_downloads::registered(&e.id) && !installed(family, cx)) { return }
    let ts = cx.text_system();
    let id = ts.resolve_font(&gpui::font(family.to_string()));
    let width = |ch| ts.advance(id, gpui::px(16.), ch).ok().map(|s| s.width);
    let has = |ch| width(ch).is_some();
    // Curly quotes stay Latin width (text.rs): a CJK face's full-width ones
    // still give way to QUOTE_FONT.
    let narrow = |ch| width(ch).is_some_and(|w| w < gpui::px(16. * 0.7));
    // A missing family resolves to a fallback face, which must not count.
    let g = if missing(family) { Glyphs::default() } else { Glyphs { cjk_punct: has('，') && has('（'), quotes: narrow('“') && narrow('’') } };
    OWN_GLYPHS.write().unwrap().push((family.to_string(), g));
}

/// What `family` was found to draw itself (nothing when never probed).
pub fn own_glyphs(family: &str) -> Glyphs {
    OWN_GLYPHS.read().unwrap().iter().find(|(f, _)| f == family).map(|(_, g)| *g).unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn set_families_for_test(list: Option<Vec<String>>) {
    *FAMILIES.write().unwrap() = list.map(|l| Arc::new(tidy(l)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tidy_sorts_dedups_and_drops_private_faces() {
        let l = tidy(["b", ".SystemUIFont", "A", "", "a", "C", "B"].map(String::from).to_vec());
        assert_eq!(l, ["A", "b", "C"]);
    }

    /// The only test that touches the family list: an uninstalled family
    /// leaves the slot empty (the default face), an installed one fills it,
    /// and before the list is read the family is taken as saved.
    #[test]
    fn store_falls_back_when_uninstalled() {
        static SLOT: RwLock<Option<&'static str>> = RwLock::new(None);
        set_families_for_test(None);
        store(&SLOT, "Gone Sans");
        assert_eq!(*SLOT.read().unwrap(), Some("Gone Sans"));
        set_families_for_test(Some(vec!["Georgia".into(), "LXGW WenKai".into()]));
        store(&SLOT, "Gone Sans");
        assert_eq!(*SLOT.read().unwrap(), None);
        store(&SLOT, " LXGW WenKai ");
        assert_eq!(*SLOT.read().unwrap(), Some("LXGW WenKai"));
        store(&SLOT, "");
        assert_eq!(*SLOT.read().unwrap(), None);
        set_families_for_test(None);
    }

    #[test]
    fn unprobed_families_keep_the_swaps() {
        assert_eq!(own_glyphs(".SystemUIFont"), Glyphs::default());
    }
}
