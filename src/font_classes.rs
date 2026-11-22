//! What kind of face each installed family is, for 设置's font lists
//! (`settings::font_choices`): 黑体 / 宋体 / 楷体·手写 / 圆体 / 等宽 / 西文衬线 /
//! 西文无衬线 / 其他, which scripts it covers (simplified Chinese against
//! 通用规范汉字表's 3500 first-level characters, as tools/fonts/coverage.py),
//! and whether it is a symbol face. Read from the font files themselves
//! (fontdb's scan of the system's fonts, once per run, off the main thread):
//! monospace from `post.isFixedPitch` or equal ASCII advances, style from
//! OS/2 `sFamilyClass` and PANOSE, else from the family's names (宋 / Ming /
//! Serif …). Common system families those get wrong, and the ones fontdb
//! cannot see (macOS's downloadable fonts such as 楷体-简), are named in `KNOWN`.

use rustc_hash::FxHashMap;
use std::sync::{Arc, OnceLock, RwLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class { Hei, Song, Kai, Yuan, Mono, LatinSerif, LatinSans, Other }

impl Class {
    pub fn label(self) -> &'static str {
        use crate::settings::tr;
        match self {
            Class::Hei => tr("黑体", "Chinese sans"),
            Class::Song => tr("宋体", "Chinese serif"),
            Class::Kai => tr("楷体·手写", "Kai & handwriting"),
            Class::Yuan => tr("圆体", "Rounded"),
            Class::Mono => tr("等宽", "Monospace"),
            Class::LatinSerif => tr("西文衬线", "Serif"),
            Class::LatinSans => tr("西文无衬线", "Sans serif"),
            Class::Other => tr("其他", "Other"),
        }
    }
    pub fn cjk(self) -> bool { matches!(self, Class::Hei | Class::Song | Class::Kai | Class::Yuan) }
}

/// What a family covers and what it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Info {
    pub class: Class,
    pub latin: bool,
    /// Share of the 3500 first-level characters it has.
    pub han: f32,
    /// Has traditional forms (國 這 說).
    pub trad: bool,
    pub kana: bool,
    pub hangul: bool,
    pub arabic: bool,
    pub symbol: bool,
    /// Made for another script (Thai, Devanagari, Arabic …), Latin letters
    /// or not: listed with the symbols, folded, unless it is the interface's.
    pub other_lang: bool,
}

impl Info {
    /// Covers simplified Chinese (≥ 99% of the first-level characters).
    pub fn simplified(&self) -> bool { self.han >= 0.99 }
    /// Some Chinese, not enough for simplified text.
    pub fn partial_han(&self) -> bool { self.han >= 0.2 && !self.simplified() }
}

fn level1() -> &'static [char] {
    static L: OnceLock<Vec<char>> = OnceLock::new();
    L.get_or_init(|| include_str!("../tools/fonts/data/tongyong-level1.txt").lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.trim().chars().next()).collect())
}

/// `KNOWN`: families whose files mislead (or that fontdb doesn't find), by
/// name, ignoring case: their class, and whether they are symbol faces.
const KNOWN: &[(&str, Class)] = &[
    ("PingFang SC", Class::Hei), ("PingFang TC", Class::Hei), ("PingFang HK", Class::Hei), ("PingFang MO", Class::Hei),
    ("Heiti SC", Class::Hei), ("Heiti TC", Class::Hei), ("STHeiti", Class::Hei), ("Hiragino Sans GB", Class::Hei),
    ("Lantinghei SC", Class::Hei), ("Lantinghei TC", Class::Hei), ("Apple LiGothic", Class::Hei), ("Microsoft YaHei", Class::Hei),
    ("Microsoft YaHei UI", Class::Hei), ("Microsoft JhengHei", Class::Hei), ("DengXian", Class::Hei), ("SimHei", Class::Hei),
    ("Songti SC", Class::Song), ("Songti TC", Class::Song), ("STSong", Class::Song), ("Apple LiSung", Class::Song), ("LiSong Pro", Class::Song),
    ("SimSun", Class::Song), ("NSimSun", Class::Song), ("FangSong", Class::Song), ("STFangsong", Class::Song), ("MingLiU", Class::Song), ("PMingLiU", Class::Song),
    ("Kaiti SC", Class::Kai), ("Kaiti TC", Class::Kai), ("STKaiti", Class::Kai), ("KaiTi", Class::Kai), ("BiauKai", Class::Kai), ("DFKai-SB", Class::Kai),
    ("HanziPen SC", Class::Kai), ("HanziPen TC", Class::Kai), ("Hannotate SC", Class::Kai), ("Hannotate TC", Class::Kai), ("Libian SC", Class::Kai),
    ("Weibei SC", Class::Kai), ("Weibei TC", Class::Kai), ("Xingkai SC", Class::Kai), ("Xingkai TC", Class::Kai), ("Baoli SC", Class::Kai), ("Wawati SC", Class::Kai), ("Wawati TC", Class::Kai),
    ("Yuanti SC", Class::Yuan), ("Yuanti TC", Class::Yuan), ("Yuppy SC", Class::Yuan), ("Yuppy TC", Class::Yuan),
    ("New York", Class::LatinSerif), ("Charter", Class::LatinSerif), ("Iowan Old Style", Class::LatinSerif), ("Georgia", Class::LatinSerif),
    ("STIX Two Text", Class::LatinSerif), ("Times New Roman", Class::LatinSerif), ("Times", Class::LatinSerif), ("Palatino", Class::LatinSerif), ("Baskerville", Class::LatinSerif), ("Hoefler Text", Class::LatinSerif),
    ("Helvetica", Class::LatinSans), ("Helvetica Neue", Class::LatinSans), ("Arial", Class::LatinSans), ("Avenir", Class::LatinSans), ("Avenir Next", Class::LatinSans),
    ("SF Pro", Class::LatinSans), ("SF Pro Text", Class::LatinSans), ("Gill Sans", Class::LatinSans), ("Verdana", Class::LatinSans), ("Segoe UI", Class::LatinSans),
    ("Menlo", Class::Mono), ("Monaco", Class::Mono), ("Courier", Class::Mono), ("Courier New", Class::Mono), ("SF Mono", Class::Mono), ("Andale Mono", Class::Mono), ("Consolas", Class::Mono),
];

/// Symbol and emoji faces: never a choice for text, listed last and folded.
const SYMBOLS: &[&str] = &["Webdings", "Wingdings", "Wingdings 2", "Wingdings 3", "Apple Symbols", "LastResort", "Last Resort", "Zapf Dingbats", "Symbol",
    "Bodoni Ornaments", "Apple Color Emoji", "Noto Color Emoji", "Segoe UI Emoji", "Segoe UI Symbol", "Marlett", "MT Extra", "Noto Sans Symbols", "Noto Sans Symbols 2"];

/// Words in the names of faces made for one other script.
const SCRIPT_WORDS: &[&str] = &["bangla", "bengali", "devanagari", "gujarati", "gurmukhi", "kannada", "malayalam", "oriya", "odia", "sinhala", "tamil", "telugu",
    "thai", "lao", "khmer", "myanmar", "tibetan", "arabic", "hebrew", "armenian", "georgian", "ethiopic", "mongolian", "cherokee", "kohinoor", "thonburi",
    "sukhumvit", "ayuthaya", "silom", "krungthep", "sathu", "mukta", "al nile", "al tarikh", "baghdad", "damascus", "farah", "farisi", "kufi", "nadeem", "waseem",
    "diwan", "geeza", "raanana", "corsiva hebrew", "mishafi", "muna", "sana", "beirut", "naskh", "nastaliq", "euphemia", "inaimathi", "khmer", "lao sangam",
    "shree devanagari", "devanagari sangam", "bangla sangam", "gurmukhi", "malayalam sangam", "tamil sangam", "telugu sangam", "kannada sangam", "oriya sangam",
    "sinhala sangam", "myanmar sangam", "noto sans", "noto serif"];

/// Made for another script, judging by its names (`Noto Sans …` only when it
/// also lacks Latin: Noto Sans itself is a Latin face).
fn other_script_name(names: &[String], latin: bool) -> bool {
    names.iter().any(|n| {
        let n = n.to_lowercase();
        SCRIPT_WORDS.iter().any(|w| if w.starts_with("noto") { !latin && n.starts_with(w) } else { n.contains(w) })
    })
}

/// Where a family stands in `KNOWN` (the common faces lead their section).
pub fn known_rank(family: &str) -> usize { KNOWN.iter().position(|(f, _)| f.eq_ignore_ascii_case(family)).unwrap_or(usize::MAX) }

pub fn known(family: &str) -> Option<Class> { KNOWN.iter().find(|(f, _)| f.eq_ignore_ascii_case(family)).map(|k| k.1) }
pub fn symbol_name(family: &str) -> bool { SYMBOLS.iter().any(|f| f.eq_ignore_ascii_case(family)) }

/// The class a family's names suggest (any of them: 宋体-简 as well as Songti SC).
fn by_names(names: &[String], mono: bool, cjk: bool) -> Class {
    let has = |keys: &[&str]| names.iter().any(|n| { let n = n.to_lowercase(); keys.iter().any(|k| n.contains(k)) });
    if mono || has(&["mono", "code", "typewriter", "courier", "等宽"]) { return Class::Mono; }
    if cjk {
        if has(&["楷", "kai", "手写", "hand", "行书", "xingkai", "隶", "libian", "魏碑", "weibei", "xiaolai", "klee", "翩翩", "手札", "hannotate", "hanzipen"]) { return Class::Kai; }
        if has(&["圆", "圓", "yuan", "round", "maru", "huninn", "粉圆"]) { return Class::Yuan; }
        if has(&["宋", "明", "song", "ming", "serif", "mincho", "myeongjo", "batang", "仿宋", "fang"]) { return Class::Song; }
        return Class::Hei;
    }
    if has(&["serif", "times", "roman", "garamond", "book", "old style", "caslon", "didot", "bodoni", "minion"]) && !has(&["sans"]) { return Class::LatinSerif; }
    Class::LatinSans
}

/// A face's coverage and class, from its file.
pub fn analyze(data: &[u8], index: u32, names: &[String]) -> Option<Info> {
    let face = ttf_parser::Face::parse(data, index).ok()?;
    let has = |c: char| face.glyph_index(c).is_some_and(|g| g.0 != 0);
    let latin = "aeosAEOS".chars().all(has);
    let l1 = level1();
    let han = l1.iter().filter(|&&c| has(c)).count() as f32 / l1.len() as f32;
    let symbol_cmap = face.tables().cmap.is_some_and(|c| c.subtables.into_iter().any(|s| s.platform_id == ttf_parser::PlatformId::Windows && s.encoding_id == 0));
    let os2 = face.raw_face().table(ttf_parser::Tag::from_bytes(b"OS/2"));
    let (family_class, panose) = os2.filter(|t| t.len() >= 42).map_or((0, [0u8; 2]), |t| (t[30], [t[32], t[33]]));
    // monospace: post says so, or every printable ASCII advance is the same
    let advance = |c: char| face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g));
    let ascii: Vec<u16> = ('!'..='~').filter_map(advance).collect();
    let mono = latin && (face.is_monospaced() || ascii.len() > 80 && ascii.iter().all(|&a| a == ascii[0]));
    let symbol = symbol_cmap || family_class == 12 || panose[0] == 5;
    let cjk = han >= 0.2 || "あア가".chars().any(has) && !latin;
    let class = names.iter().find_map(|n| known(n)).unwrap_or_else(|| {
        if !latin && !cjk { return Class::Other; }
        if !cjk && !mono {
            // sFamilyClass 1–5, 7: serif; 8: sans; PANOSE serif style 2–10 serif, 11+ sans
            if matches!(family_class, 1..=5 | 7) || panose[0] == 2 && (2..=10).contains(&panose[1]) { return Class::LatinSerif; }
            if family_class == 8 || panose[0] == 2 && panose[1] >= 11 { return Class::LatinSans; }
        }
        by_names(names, mono, cjk)
    });
    let (kana, hangul) = ("あア".chars().all(has), "가한".chars().all(has));
    // Korean or Japanese faces with little Chinese count as another script too
    let other_lang = han < 0.2 && (!latin || kana || hangul || other_script_name(names, latin));
    Some(Info { class, latin, han, trad: "國這說".chars().all(has), kana, hangul, arabic: "بت".chars().all(has), symbol, other_lang })
}

static INFOS: RwLock<Option<Arc<FxHashMap<String, Info>>>> = RwLock::new(None);
static SCANNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A family's info once the scan is done (by any of its names, ignoring
/// case); None before, or for a family fontdb doesn't find.
pub fn info(family: &str) -> Option<Info> {
    let infos = INFOS.read().unwrap().clone()?;
    infos.get(&family.to_lowercase()).copied()
}

pub fn scanned() -> bool { INFOS.read().unwrap().is_some() }

/// Scans the system's fonts once, off the main thread, then repaints.
pub fn scan(cx: &mut gpui::App) {
    if scanned() || SCANNING.swap(true, std::sync::atomic::Ordering::Relaxed) { return; }
    let task = cx.background_executor().spawn(async { scan_system() });
    cx.spawn(async move |cx| {
        let map = task.await;
        *INFOS.write().unwrap() = Some(Arc::new(map));
        cx.update(|cx| cx.refresh_windows());
    }).detach();
}

/// Every family the system has: its upright regular face analysed, filed
/// under each of its names.
fn scan_system() -> FxHashMap<String, Info> {
    use resvg::usvg::fontdb;
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    // the face nearest to regular, per family
    let mut best: FxHashMap<String, (i32, fontdb::ID, Vec<String>)> = FxHashMap::default();
    for f in db.faces() {
        let Some((name, _)) = f.families.first() else { continue };
        let score = (f.weight.0 as i32 - 400).abs() + if f.style == fontdb::Style::Normal { 0 } else { 1000 };
        let names: Vec<String> = f.families.iter().map(|(n, _)| n.clone()).collect();
        let e = best.entry(name.to_lowercase()).or_insert((i32::MAX, f.id, names.clone()));
        if score < e.0 { *e = (score, f.id, names); }
    }
    let mut out = FxHashMap::default();
    for (_, (_, id, names)) in best {
        let Some(Some(mut info)) = db.with_face_data(id, |data, index| analyze(data, index, &names)) else { continue };
        if names.iter().any(|n| symbol_name(n)) { info.symbol = true; }
        for n in &names { out.insert(n.to_lowercase(), info); }
    }
    out
}

/// A family the scan didn't see: what its name says.
pub fn guess(family: &str) -> Info {
    let class = known(family).unwrap_or_else(|| by_names(&[family.to_string()], false, family.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)) || ["SC", "TC", "HK"].iter().any(|s| family.ends_with(s))));
    let cjk = class.cjk();
    Info { class, latin: true, han: if cjk { 1. } else { 0. }, trad: cjk, kana: false, hangul: false, arabic: false, symbol: symbol_name(family), other_lang: !cjk && other_script_name(&[family.to_string()], true) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(p: &str) -> Vec<u8> {
        let b = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(p)).unwrap();
        if p.ends_with(".zlib") { miniz_oxide::inflate::decompress_to_vec_zlib(&b).unwrap() } else { b }
    }
    fn of(p: &str, name: &str) -> Info { analyze(&read(p), 0, &[name.to_string()]).unwrap() }

    /// Known faces from the repository land where they belong.
    #[test]
    fn faces_are_classified_from_their_files() {
        let cascadia = of("assets/fonts/excalidraw/CascadiaCode-Regular.ttf", "Cascadia Code");
        assert_eq!((cascadia.class, cascadia.latin, cascadia.simplified()), (Class::Mono, true, false));
        // no PANOSE or class: monospace from its advances
        assert_eq!(of("assets/fonts/katex/KaTeX_Typewriter-Regular.ttf", "KaTeX_Typewriter").class, Class::Mono);
        assert_eq!(of("assets/fonts/excalidraw/Nunito-Medium.ttf", "Nunito").class, Class::LatinSans);
        assert_eq!(of("assets/fonts/katex/KaTeX_Main-Regular.ttf", "KaTeX_Main").class, Class::LatinSans);
        let x = of("assets/fonts/excalidraw/Xiaolai-Regular.ttf.zlib", "Xiaolai");
        assert!(x.simplified() && x.trad && x.hangul && !x.symbol, "{x:?}");
        assert_eq!(x.class, Class::Kai);
        // the name table wins over what the file says
        assert_eq!(analyze(&read("assets/fonts/excalidraw/Nunito-Medium.ttf"), 0, &["Georgia".into()]).unwrap().class, Class::LatinSerif);
    }

    #[test]
    fn names_fill_in() {
        assert_eq!(known("pingfang sc"), Some(Class::Hei));
        assert_eq!(known("Kaiti SC"), Some(Class::Kai));
        assert!(symbol_name("Wingdings") && !symbol_name("Georgia"));
        assert_eq!(by_names(&["宋体-简".into()], false, true), Class::Song);
        assert_eq!(by_names(&["Hiragino Mincho ProN".into()], false, true), Class::Song);
        assert_eq!(by_names(&["Source Serif 4".into()], false, false), Class::LatinSerif);
        assert_eq!(by_names(&["PT Sans".into()], false, false), Class::LatinSans);
        assert_eq!(guess("Yuanti SC").class, Class::Yuan);
        assert!(guess("Songti SC").simplified() && guess("Webdings").symbol);
        assert_eq!(level1().len(), 3500);
        assert!(other_script_name(&["Kohinoor Devanagari".into()], true) && other_script_name(&["Noto Sans Javanese".into()], false));
        assert!(!other_script_name(&["Noto Sans".into()], true) && !other_script_name(&["Georgia".into()], true));
    }
}
