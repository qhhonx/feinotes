//! The board's fonts: Excalidraw's own (assets/fonts/excalidraw, licenses in
//! THIRD_PARTY.md), registered with GPUI, and the family → font mapping of
//! Excalidraw `packages/common/src/utils.ts` `getFontFamilyString` /
//! `constants.ts` `getFontFamilyFallbacks` and `excalidraw/fonts/Fonts.ts`
//! @ 3a317b5 (which font faces back which family).
//!
//! Upstream lets CSS fall back per character: `Excalifont, Xiaolai,
//! sans-serif, Segoe UI Emoji`, the others `<family>, sans-serif |
//! monospace, Segoe UI Emoji`. Here a text is cut into runs instead: a
//! character the family's font lacks goes to Xiaolai (Excalifont only) when
//! Xiaolai has it, else stays with the family and GPUI's fallback list (the
//! system's generic font, CJK font and emoji font) draws it. GPUI's own
//! fallback lists only reach system fonts, hence the runs.
//!
//! Differences: Helvetica (2) is the system's Helvetica on macOS, Arial on
//! Windows (as browsers resolve it); Liberation Sans (9, not user-pickable
//! upstream) isn't bundled (the copy upstream ships is the GPL-licensed 1.x)
//! and draws with Arial, which it is metric-compatible with. Xiaolai (14 MB
//! compressed) is unpacked and registered the first time a text needs it.

use fdl_core::board::consts::font_family as ff;
use fdl_core::board::font_subset::mapped_chars;
use fdl_core::board::text_measure::{XIAOLAI_FALLBACK, set_font_source};
use gpui::{App, Font, FontFallbacks, FontFeatures, FontStyle, FontWeight, TextSystem};
use rustc_hash::FxHashSet;
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::sync::{Arc, OnceLock};

/// Bundled faces: family number, family name (as Excalidraw's CSS names
/// it, and as the font's name table says), file under assets/.
const BUNDLED: [(u32, &str, &str); 8] = [
    (ff::EXCALIFONT, "Excalifont", "fonts/excalidraw/Excalifont-Regular.ttf"),
    (ff::VIRGIL, "Virgil", "fonts/excalidraw/Virgil-Regular.ttf"),
    (ff::NUNITO, "Nunito", "fonts/excalidraw/Nunito-Medium.ttf"),
    (ff::LILITA_ONE, "Lilita One", "fonts/excalidraw/LilitaOne-Regular.ttf"),
    (ff::COMIC_SHANNS, "Comic Shanns", "fonts/excalidraw/ComicShanns-Regular.ttf"),
    (ff::CASCADIA, "Cascadia", "fonts/excalidraw/CascadiaCode-Regular.ttf"),
    (ff::ASSISTANT, "Assistant", "fonts/excalidraw/Assistant-Regular.ttf"),
    (XIAOLAI_FALLBACK, "Xiaolai", XIAOLAI_FILE),
];
const XIAOLAI_FILE: &str = "fonts/excalidraw/Xiaolai-Regular.ttf.zlib";
/// Assistant's other weights (upstream's UI font, `fonts.css`).
const ASSISTANT_WEIGHTS: [&str; 3] = ["fonts/excalidraw/Assistant-Medium.ttf", "fonts/excalidraw/Assistant-SemiBold.ttf", "fonts/excalidraw/Assistant-Bold.ttf"];

fn asset(path: &str) -> Option<&'static [u8]> { crate::assets::FILES.iter().find(|(p, _)| *p == path).map(|(_, b)| *b) }

fn index(family: u32) -> Option<usize> { BUNDLED.iter().position(|b| b.0 == family) }

/// Xiaolai's TrueType bytes, unpacked once.
fn xiaolai() -> Option<&'static [u8]> {
    static X: OnceLock<Option<&'static [u8]>> = OnceLock::new();
    *X.get_or_init(|| {
        let z = asset(XIAOLAI_FILE)?;
        miniz_oxide::inflate::decompress_to_vec_zlib(z).ok().map(|v| &*Box::leak(v.into_boxed_slice()))
    })
}

/// The font file of a family or fallback number (core's `FontSource`).
pub fn bytes(family: u32) -> Option<&'static [u8]> {
    if family == XIAOLAI_FALLBACK { return xiaolai(); }
    asset(BUNDLED[index(family)?].2)
}

/// The characters a bundled font maps.
fn coverage(family: u32) -> Option<&'static FxHashSet<char>> {
    static C: [OnceLock<FxHashSet<char>>; 8] = [const { OnceLock::new() }; 8];
    let i = index(family)?;
    Some(C[i].get_or_init(|| bytes(family).map(|b| mapped_chars(b).into_iter().collect()).unwrap_or_default()))
}

thread_local! {
    static TEXT_SYSTEM: RefCell<Option<Arc<TextSystem>>> = const { RefCell::new(None) };
    static XIAOLAI_ADDED: Cell<Option<bool>> = const { Cell::new(None) };
}

/// Registers the Latin faces (Xiaolai waits for its first character) and
/// hands core the font files for SVG export.
pub fn load(cx: &App) {
    let fonts = BUNDLED.iter().filter(|b| b.0 != XIAOLAI_FALLBACK).map(|b| b.2).chain(ASSISTANT_WEIGHTS).filter_map(asset).map(Cow::Borrowed).collect();
    if let Err(e) = cx.text_system().add_fonts(fonts) { log::warn!("board fonts: {e}"); }
    TEXT_SYSTEM.with(|t| *t.borrow_mut() = Some(cx.text_system().clone()));
    set_font_source(bytes);
}

/// Adds the bundled fonts to a usvg font database (copy as PNG rasterizes
/// the exported SVG; usvg skips its inlined `@font-face`s). Xiaolai only
/// with `cjk`.
pub fn load_into(db: &mut resvg::usvg::fontdb::Database, cjk: bool) {
    for (f, ..) in BUNDLED {
        if f == XIAOLAI_FALLBACK && !cjk { continue; }
        if let Some(b) = bytes(f) { db.load_font_source(resvg::usvg::fontdb::Source::Binary(Arc::new(b))); }
    }
}

/// Registers Xiaolai with GPUI, once; false when it can't be.
fn xiaolai_added() -> bool {
    XIAOLAI_ADDED.with(|a| {
        if let Some(v) = a.get() { return v; }
        let ok = TEXT_SYSTEM.with(|t| t.borrow().clone()).zip(xiaolai()).is_some_and(|(ts, b)| ts.add_fonts(vec![Cow::Borrowed(b)]).is_ok());
        a.set(Some(ok));
        ok
    })
}

/// The family name GPUI looks a family up by.
pub fn family_name(family: u32) -> &'static str {
    let arial = if cfg!(target_os = "linux") { "Liberation Sans" } else { "Arial" };
    match family {
        ff::HELVETICA => if cfg!(target_os = "macos") { "Helvetica" } else { arial },
        ff::LIBERATION_SANS => arial,
        f => index(f).map_or("Excalifont", |i| BUNDLED[i].1),
    }
}

/// The system fonts behind a family: `sans-serif` or `monospace`, then CJK
/// and emoji (`Segoe UI Emoji` upstream).
fn system_fallbacks(family: u32) -> Vec<String> {
    let mono = ff::is_monospace(family);
    let v: &[&str] = if cfg!(target_os = "macos") {
        if mono { &["Menlo", "PingFang SC", "Apple Color Emoji"] } else { &["Helvetica", "PingFang SC", "Apple Color Emoji"] }
    } else if cfg!(target_os = "windows") {
        if mono { &["Consolas", "Microsoft YaHei", "Segoe UI Emoji"] } else { &["Arial", "Microsoft YaHei", "Segoe UI Emoji"] }
    } else { &[] };
    v.iter().map(|s| s.to_string()).collect()
}

fn font_named(name: &'static str, family: u32) -> Font {
    Font { family: name.into(), features: FontFeatures::default(), fallbacks: Some(FontFallbacks::from_fonts(system_fallbacks(family))), weight: FontWeight::NORMAL, style: FontStyle::Normal }
}

/// A family's font (its characters only; `runs` adds Xiaolai).
pub fn font(family: u32) -> Font { font_named(family_name(family), family) }

/// Characters that stay with the one before (combining marks, joiners,
/// variation selectors, emoji modifiers).
fn attaches(c: char) -> bool {
    matches!(c as u32, 0x300..=0x36F | 0x200C | 0x200D | 0x20D0..=0x20FF | 0xFE00..=0xFE0F | 0x1F3FB..=0x1F3FF | 0xE0020..=0xE007F | 0xE0100..=0xE01EF)
}

/// Where Xiaolai might have a glyph (what's worth unpacking it for).
fn maybe_xiaolai(c: char) -> bool { matches!(c as u32, 0x1100..=0x11FF | 0x2E80..=0xD7FF | 0xF900..=0xFFEF | 0x20000..=0x3FFFF) }

/// `text` cut into runs by font (byte lengths): the family's own, Xiaolai
/// for the CJK characters Excalifont lacks.
pub fn runs(text: &str, family: u32) -> Vec<(usize, Font)> {
    let own = if family == ff::EXCALIFONT { coverage(family) } else { None };
    let mut cut: Vec<(usize, bool)> = vec![];
    for c in text.chars() {
        let x = if attaches(c) { cut.last().is_some_and(|r| r.1) } else {
            own.is_some_and(|s| !s.contains(&c)) && maybe_xiaolai(c) && xiaolai_added() && coverage(XIAOLAI_FALLBACK).is_some_and(|s| s.contains(&c))
        };
        match cut.last_mut() { Some(r) if r.1 == x => r.0 += c.len_utf8(), _ => cut.push((c.len_utf8(), x)) }
    }
    let (main, cjk) = (font(family), font_named("Xiaolai", family));
    cut.into_iter().map(|(len, x)| (len, if x { cjk.clone() } else { main.clone() })).collect()
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn bundled_fonts_parse_and_cover() {
        for (f, name, _) in BUNDLED {
            let b = bytes(f).unwrap_or_else(|| panic!("{name} missing"));
            assert!(b.starts_with(&[0, 1, 0, 0]), "{name} is TrueType");
            assert!(!coverage(f).unwrap().is_empty(), "{name} maps characters");
        }
        assert!(coverage(ff::EXCALIFONT).unwrap().contains(&'A') && !coverage(ff::EXCALIFONT).unwrap().contains(&'中'));
        assert!(coverage(XIAOLAI_FALLBACK).unwrap().contains(&'中') && coverage(XIAOLAI_FALLBACK).unwrap().contains(&'한'));
    }

    /// Fonts register with GPUI and measure as the font files say,
    /// CJK takes Xiaolai's full-width glyphs.
    #[test]
    fn fonts_register_and_measure() {
        let ts = gpui::test_text_system();
        ts.add_fonts(BUNDLED.iter().map(|b| Cow::Borrowed(bytes(b.0).unwrap())).collect()).unwrap();
        XIAOLAI_ADDED.with(|a| a.set(Some(true)));
        let m = super::super::render::GpuiMeasure(ts.clone());
        use fdl_core::board::text_measure::{Font as BFont, Measure};
        let w = |s: &str, f: u32| m.line_width(s, BFont::new(20., f));
        // the fonts' hmtx advances sum to 219.54 and 223.16 (Excalifont kerns, Virgil has no GPOS)
        let (ex, vi) = (w("Sphinx of black quartz", ff::EXCALIFONT), w("Sphinx of black quartz", ff::VIRGIL));
        assert!((ex - 219.54).abs() < 1. && (vi - 223.16).abs() < 0.01, "Excalifont {ex}, Virgil {vi}");
        for (id, name) in [(ts.resolve_font(&font(ff::EXCALIFONT)), "Excalifont"), (ts.resolve_font(&font(ff::NUNITO)), "Nunito"), (ts.resolve_font(&font(ff::CASCADIA)), "Cascadia")] {
            assert_eq!(ts.get_font_for_id(id).map(|f| f.family.to_string()).as_deref(), Some(name));
        }
        let r = runs("ab中文", ff::EXCALIFONT);
        assert_eq!(r.iter().map(|r| (r.0, r.1.family.to_string())).collect::<Vec<_>>(), [(2, "Excalifont".into()), (6, "Xiaolai".into())]);
        // shaped runs keep their fonts (vendor/PATCHES.md #39)
        let t = "ab 中文 cd";
        let lay = ts.layout_line(t, gpui::px(20.), &super::super::render::runs(t, ff::EXCALIFONT, gpui::black()), None);
        let fams: Vec<_> = lay.runs.iter().map(|r| ts.get_font_for_id(r.font_id).map(|f| f.family.to_string())).collect();
        assert_eq!(fams, [Some("Excalifont".into()), Some("Xiaolai".into()), Some("Excalifont".into())]);
        let cjk = w("中文", ff::EXCALIFONT);
        assert!((cjk - 40.).abs() < 2., "Xiaolai is full width: {cjk}");
    }
}
