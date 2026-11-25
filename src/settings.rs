//! 设置: preferences (`.quiet/preferences.json` in the library) and the
//! settings view — a category list in the sidebar, one page per category,
//! each page a column of grouped rows. Only what a
//! user really needs to change lives here.

use fdl_core::vfs::PathExt as _;
use crate::{app::{View, Workspace}, theme::{self, Theme}, themes, ui};
use crate::design::{Button, ButtonSize, Card, Field, FieldSize, MenuItem, MenuLabel, Popover, Segment, Segmented, Separator, Switch, menu};
use crate::ui::{TitleBarExt, TipExt};
use crate::input::{TextField, TextFieldEvent};
use crate::sidebar as sb;
use gpui::{prelude::FluentBuilder, *};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, LazyLock, Mutex, RwLock};

// ---- preferences

static FEISHU: AtomicBool = AtomicBool::new(false);
static LANG: AtomicU8 = AtomicU8::new(Lang::En as u8);
static MOTION_HIGH: AtomicBool = AtomicBool::new(false);
static PREFS: RwLock<Option<Preferences>> = RwLock::new(None);

/// The active document theme draws blocks the 飞书 way (else Craft's).
pub fn feishu() -> bool { FEISHU.load(Ordering::Relaxed) }
pub(crate) fn set_base(feishu: bool) { FEISHU.store(feishu, Ordering::Relaxed); }
/// 界面语言, in the order Settings lists them. Chinese strings are the keys:
/// other languages look them up in `l10n` and fall back to English.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang { Zh, ZhTw, En, Ja, Ko, Fr, Es, Ar, Tr }

impl Lang {
    pub const ALL: [Lang; 9] = [Lang::Zh, Lang::ZhTw, Lang::En, Lang::Ja, Lang::Ko, Lang::Fr, Lang::Es, Lang::Ar, Lang::Tr];
    /// The `language` preference value.
    pub fn code(self) -> &'static str { match self { Lang::Zh => "zh", Lang::ZhTw => "zh-TW", Lang::En => "en", Lang::Ja => "ja", Lang::Ko => "ko", Lang::Fr => "fr", Lang::Es => "es", Lang::Ar => "ar", Lang::Tr => "tr" } }
    /// The language's own name, as Settings lists it.
    pub fn name(self) -> &'static str { match self { Lang::Zh => "简体中文", Lang::ZhTw => "繁體中文", Lang::En => "English", Lang::Ja => "日本語", Lang::Ko => "한국어", Lang::Fr => "Français", Lang::Es => "Español", Lang::Ar => "العربية", Lang::Tr => "Türkçe" } }
}

pub fn lang() -> Lang { Lang::ALL.get(LANG.load(Ordering::Relaxed) as usize).copied().unwrap_or(Lang::En) }
/// Not Chinese: English, or a language that shows English where it has no string yet.
pub fn english() -> bool { lang() != Lang::Zh }
/// 外观 › 动画 is 丰富: folding, the selection and hover animate too (else they answer at once).
pub fn motion_high() -> bool { MOTION_HIGH.load(Ordering::Relaxed) }
pub fn tr(zh: &'static str, en: &'static str) -> &'static str { match lang() { Lang::Zh => zh, Lang::En => en, l => crate::l10n::get(l, zh).unwrap_or(en) } }
/// `localized_format!` for a given language: `zh` / `en` build the two
/// originals; other languages refill their translation of `template`.
pub fn localized(lang: Lang, template: &str, zh: impl FnOnce() -> String, en: impl FnOnce() -> String) -> String {
    match lang { Lang::Zh => zh(), Lang::En => en(), l => crate::l10n::refill(l, template, &zh()).unwrap_or_else(en) }
}

/// Browsers 打开链接 can offer, by the name `open -a` takes. Only the ones
/// this Mac actually has are listed; anything else is the system default.
const BROWSERS: [&str; 8] = ["Safari", "Google Chrome", "Microsoft Edge", "Firefox", "Arc", "Brave Browser", "Vivaldi", "Opera"];

/// At most this many besides 系统默认, so the row stays one line.
const BROWSERS_SHOWN: usize = 3;

/// The browsers installed here, in [`BROWSERS`] order (empty off macOS).
pub fn browsers() -> Vec<&'static str> {
    if !cfg!(target_os = "macos") {
        return vec![];
    }
    let user = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Applications"));
    BROWSERS
        .into_iter()
        .filter(|name| {
            let app = format!("{name}.app");
            Path::new("/Applications").join(&app).fs_exists() || user.as_ref().is_some_and(|u| u.join(&app).fs_exists())
        })
        .take(BROWSERS_SHOWN)
        .collect()
}

/// The browser a 打开链接 segment stands for (`0`: the system default, "").
pub fn browser_at(i: usize) -> String {
    i.checked_sub(1).and_then(|i| browsers().get(i).copied()).unwrap_or_default().to_string()
}
/// Whether ⌘N should make a Markdown file: the preferred 快捷新建 type, or the
/// last one created from the 新建 menu when no fixed type is set.
pub fn quick_new_markdown() -> bool {
    let p = prefs();
    match p.new_default.as_str() { "markdown" => true, "doc" => false, _ => p.last_new == "markdown" }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// Document theme id (themes.rs); empty in files from before themes, see `feishu`.
    pub theme: String,
    /// The old two-way switch, still written so older builds keep the base style.
    pub feishu: bool,
    /// Still written (true unless Chinese) so older builds show English for a language they lack.
    pub english: bool,
    /// `Lang::code`; empty in files from before it, where `english` decides.
    #[serde(default)]
    pub language: String,
    /// Fetch `http(s)` images referenced by documents.
    pub remote_images: bool,
    /// The system-wide quick note hotkey is registered.
    pub quick_hotkey: bool,
    /// Look for a new version now and then (update.rs).
    pub auto_update: bool,
    /// Light or dark look: "light", "dark", or empty to follow the system.
    pub appearance: String,
    /// The last version that opened this library (changelog.rs).
    pub seen_version: String,
    /// 动画: "high" animates folding, the selection and hover; empty keeps them instant.
    pub motion: String,
    /// 快捷新建 (⌘N) 的默认类型: "doc", "markdown", or empty to repeat the last one.
    pub new_default: String,
    /// The type last created from the 新建 menu: "doc" or "markdown" (empty before any).
    pub last_new: String,
    /// 网页搜索 engine: "baidu", "bing", "duckduckgo"; empty is Google.
    pub search_engine: String,
    /// 启动时打开: true shows the home page; false returns to the
    /// document read last (#234).
    pub start_home: bool,
    /// 打开链接: the browser a link opens in ("Safari", "Microsoft Edge"…),
    /// empty for the system default.
    pub browser: String,
    /// 外观 › 窗口背景: paint the window opaque instead of letting the desktop
    /// show through the sidebar material.
    pub opaque_window: bool,
    /// 外观 › 界面字号: "small" or "large"; empty is the 13pt standard.
    pub ui_font: String,
    /// 外观 › 界面字体: the family the sidebar, menus and settings are drawn in
    /// (a system family name); empty is the platform's own sans.
    pub ui_family: String,
    /// 外观 › 文档字体: the family the note's body text and headings are drawn
    /// in (a system family name); empty follows the document theme.
    pub doc_family: String,
    /// 外观 › 正文字号: "small", "large" or "xlarge"; empty follows the
    /// document theme. Headings, tables and code keep the theme's ratio (themes::doc_len).
    #[serde(default)]
    pub doc_size: String,
    /// When 稍后 put off moving the library to real folders (Unix seconds; 0 never).
    pub migrate_later: u64,
    /// 查找 › `.*`: the find box takes regular expressions.
    pub find_regex: bool,
    /// 退格键回到标题: Backspace at the start of the body merges the first
    /// paragraph into the title (the default); off keeps the caret in the body.
    pub backspace_to_title: bool,
    /// 中英文之间自动加空格 (#293): typing puts a space between 汉字 and Latin
    /// letters or digits. Off by default.
    #[serde(default)]
    pub cjk_spacing: bool,
    /// 外观's font lists: 收藏 (both lists share it) and the last families
    /// picked for each, newest first.
    #[serde(default)]
    pub font_favorites: Vec<String>,
    #[serde(default)]
    pub recent_doc_fonts: Vec<String>,
    #[serde(default)]
    pub recent_ui_fonts: Vec<String>,
    /// 文档库 › 快速笔记保存到: the folder key quick notes go to; empty,
    /// or a top-level folder that is gone, is the first open top-level folder (2.8).
    #[serde(default, alias = "quick_note_root")]
    pub quick_note_folder: String,
}

/// A week between asks to move the library to real folders.
const MIGRATE_ASK_EVERY: u64 = 7 * 24 * 3600;
fn unix_now() -> u64 { web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) }

/// 稍后 on the move to real folders: remember when.
pub fn migrate_later() {
    let mut p = prefs();
    p.migrate_later = unix_now();
    let _ = save(&p);
}

/// Whether to ask about the move now: never put off, or a week since.
pub fn migrate_ask_due() -> bool { unix_now().saturating_sub(prefs().migrate_later) >= MIGRATE_ASK_EVERY }

/// Map a BCP 47 / POSIX locale tag to a supported interface language.
fn lang_for_locale(locale: &str) -> Option<Lang> {
    let mut parts = locale.split(['.', '@']).next().unwrap_or("").split(['-', '_']);
    let base = parts.next()?.to_ascii_lowercase();
    if base.is_empty() { return None; }
    if base == "zh" {
        // A script subtag decides (zh-Hans-HK is Simplified); otherwise the region does.
        let rest: Vec<String> = parts.map(str::to_ascii_lowercase).collect();
        let trad = rest.iter().find(|p| *p == "hans" || *p == "hant").map_or_else(|| rest.iter().any(|p| matches!(p.as_str(), "tw" | "hk" | "mo")), |s| s == "hant");
        return Some(if trad { Lang::ZhTw } else { Lang::Zh });
    }
    Lang::ALL.into_iter().find(|l| l.code() == base)
}

/// Pick the first supported system language; saved preferences still win.
fn system_lang() -> Lang {
    sys_locale::get_locales().find_map(|l| lang_for_locale(&l)).unwrap_or(Lang::En)
}

impl Default for Preferences {
    fn default() -> Self {
        let lang = system_lang();
        Preferences { theme: String::new(), feishu: true, english: lang != Lang::Zh, language: lang.code().into(), remote_images: true, quick_hotkey: true, auto_update: true, appearance: String::new(), seen_version: String::new(), motion: String::new(), new_default: String::new(), last_new: String::new(), search_engine: String::new(), start_home: false, browser: String::new(), opaque_window: false, ui_font: String::new(), ui_family: String::new(), doc_family: String::new(), doc_size: String::new(), migrate_later: 0, find_regex: false, backspace_to_title: true, cjk_spacing: false, font_favorites: vec![], recent_doc_fonts: vec![], recent_ui_fonts: vec![], quick_note_folder: String::new() }
    }
}

impl Preferences {
    pub fn theme_id(&self) -> String {
        if !self.theme.is_empty() { self.theme.clone() } else if self.feishu { "feishu".into() } else { "craft".into() }
    }
    pub fn lang(&self) -> Lang {
        if matches!(self.language.as_str(), "zh-TW" | "zh-Hant") { return Lang::ZhTw; }
        Lang::ALL.into_iter().find(|l| l.code() == self.language).unwrap_or(if self.english { Lang::En } else { Lang::Zh })
    }
    pub fn set_lang(&mut self, lang: Lang) {
        self.language = lang.code().into();
        self.english = lang != Lang::Zh;
    }
}

fn path() -> std::path::PathBuf { crate::store::Store::settings_dir().join("preferences.json") }

/// Show the interface in `lang` without saving it (first launch, before the library exists).
pub(crate) fn show_lang(lang: Lang) {
    let mut p = prefs();
    p.set_lang(lang);
    apply(p);
}

/// The preferences: this computer's (应用级), the document theme, font and
/// 正文字号 the top-level folder in front's own (W21: they go with its folder).
pub fn load() {
    let mut p = fdl_core::vfs::read(path()).ok().and_then(|b| serde_json::from_slice::<Preferences>(&b).ok()).unwrap_or_default();
    if let Some(root) = look_dir() {
        let look = crate::store::Store::look_at(&root);
        let d = Preferences::default();
        p.theme = look.theme.unwrap_or(d.theme);
        p.feishu = look.feishu.unwrap_or(d.feishu);
        p.doc_family = look.doc_family.unwrap_or(d.doc_family);
        p.doc_size = look.doc_size.unwrap_or(d.doc_size);
    }
    // Tests read the interface in English, whatever the machine's language or
    // the preferences a `QUIET_MD_HOME` holds; one that needs another language says so.
    #[cfg(test)]
    p.set_lang(Lang::En);
    apply(p);
}

/// The top-level folder whose look is in use: the one in front, when a top-level folder is open.
fn look_dir() -> Option<std::path::PathBuf> {
    let root = crate::store::Store::active_dir();
    (has_folders() && root.join(crate::store::INTERNAL).exists()).then_some(root)
}

/// Whether 主页 has a folder (none before first launch picks one).
fn has_folders() -> bool { HAS_FOLDERS.load(Ordering::Relaxed) }
static HAS_FOLDERS: AtomicBool = AtomicBool::new(false);
pub(crate) fn set_has_folders(on: bool) { HAS_FOLDERS.store(on, Ordering::Relaxed); }

/// Another top-level folder came to the front: its own look (theme, document font,
/// themes folder) takes over in every window. Changing look between top-level folders is
/// meant (W21).
pub(crate) fn top_switched(cx: &mut App) {
    let before = prefs();
    load();
    let now = prefs();
    if (now.theme_id(), &now.doc_family) == (before.theme_id(), &before.doc_family) && themes::dir() == *THEMES_DIR.lock().unwrap() { return }
    *THEMES_DIR.lock().unwrap() = themes::dir();
    themes::load(&now.theme_id(), themes::all_loaded(), || cx.text_system().all_font_names());
    check_fonts(cx);
    for w in cx.windows() {
        if let Some(h) = w.downcast::<Workspace>() { let _ = h.update(cx, |ws, _, cx| { ws.history_limit = ws.store.history_limit(); refresh_all(ws, cx) }); }
    }
}
static THEMES_DIR: std::sync::Mutex<std::path::PathBuf> = std::sync::Mutex::new(std::path::PathBuf::new());

fn apply(p: Preferences) {
    FEISHU.store(p.feishu, Ordering::Relaxed);
    LANG.store(p.lang() as u8, Ordering::Relaxed);
    crate::model::set_untitled(translate(crate::model::UNTITLED, p.lang()));
    MOTION_HIGH.store(p.motion == "high", Ordering::Relaxed);
    // Before the themes are read (startup) the base comes from `feishu`.
    themes::select(&p.theme_id());
    themes::set_doc_font(&p.doc_family);
    themes::set_doc_size(&p.doc_size);
    theme::set_ui_family(&p.ui_family);
    *PREFS.write().unwrap() = Some(p);
}

/// Startup and each new window: with a font chosen, read the installed
/// families once so one that has been uninstalled since falls back to the
/// default face (`fonts::store`) instead of GPUI's last resort, and note which
/// punctuation it draws itself (`fonts::probe`). Without one, fonts stay
/// unenumerated; only whether the Chinese punctuation face is here is
/// looked up (`fonts::check_cjk_punct`).
pub fn check_fonts(cx: &App) {
    crate::fonts::check_cjk_punct(cx);
    let p = prefs();
    if p.ui_family.trim().is_empty() && p.doc_family.trim().is_empty() { return }
    crate::fonts::families(cx);
    load_downloaded(&p.doc_family, cx);
    crate::fonts::probe(&p.doc_family, cx);
    crate::fonts::probe(&p.ui_family, cx);
    themes::set_doc_font(&p.doc_family);
    theme::set_ui_family(&p.ui_family);
}

/// A document font from 设置's download list: registered if this run has
/// it, else read from the cache (`Mode::Cache`: a deleted or damaged copy is
/// not fetched behind the reader's back; the font row offers it again and
/// the default face stands in, as for an uninstalled font).
fn load_downloaded(family: &str, cx: &App) {
    use crate::font_downloads as fd;
    let Some(e) = fd::by_family(family) else { return };
    if fd::register(&e.id, cx.text_system()) { crate::fonts::added(&e.family); return }
    if fd::on_disk(&e.id) { fd::want(&e.id, fd::Mode::Cache); }
}

/// The download list's font picked while it was still on its way (its id):
/// it becomes the document font when it arrives.
static PENDING_DOC: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// A downloadable font arrived or failed (`font_downloads::Changed`, every
/// window): a pending pick takes effect or is dropped with a toast (the font
/// in use stays); the document font in use starts drawing in it.
pub fn font_changed(cx: &mut App) {
    use crate::font_downloads as fd;
    let c = cx.global::<fd::Changed>().clone();
    let Some(e) = fd::entry(&c.id) else { return };
    let pending = { let mut p = PENDING_DOC.lock().unwrap(); if p.as_deref() == Some(c.id.as_str()) { p.take() } else { None } };
    let uses = prefs().doc_family.eq_ignore_ascii_case(&e.family);
    let ws = cx.active_window().or_else(|| cx.windows().first().copied()).and_then(|w| w.downcast::<Workspace>());
    // Picked from a cache that turned out damaged: download it after all.
    if !c.ok && c.error.is_none() && pending.is_some() {
        *PENDING_DOC.lock().unwrap() = pending;
        fd::want(&c.id, fd::Mode::Now);
        return;
    }
    if !c.ok {
        if pending.is_some() && let Some(w) = ws {
            let msg = crate::localized_format!("{} 下载失败，仍用原来的字体：{}", "Could not download {}; the font in use stays: {}", e.name(), c.error.clone().unwrap_or_default());
            let _ = w.update(cx, |ws, _, cx| ws.show_toast_kind(ui::ToastKind::Error, &msg, cx));
        }
        return;
    }
    if !(uses || pending.is_some()) { return }
    // Xiaolai registers when a board first needs it; as a document font, now.
    fd::register(&e.id, cx.text_system());
    crate::fonts::added(&e.family);
    let family = e.family.clone();
    match (pending, ws) {
        (Some(_), Some(w)) => { let _ = w.update(cx, |ws, _, cx| ws.set_pref(|p| p.doc_family = family, cx)); }
        _ => {
            crate::fonts::probe(&family, cx);
            themes::set_doc_font(&family);
            for w in cx.windows() {
                if let Some(h) = w.downcast::<Workspace>() { let _ = h.update(cx, |ws, _, cx| refresh_all(ws, cx)); }
            }
        }
    }
}

/// Tell the OS which look the app wants (macOS: the window material, menus
/// and dialogs follow it); empty follows the system.
pub fn apply_appearance(pref: &str) {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc::{class, msg_send, sel, sel_impl};
        use objc::runtime::Object;
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let name = match pref { "light" => Some(c"NSAppearanceNameAqua"), "dark" => Some(c"NSAppearanceNameDarkAqua"), _ => None };
        let look: *mut Object = match name {
            Some(n) => { let s: *mut Object = msg_send![class!(NSString), stringWithUTF8String: n.as_ptr()]; msg_send![class!(NSAppearance), appearanceNamed: s] }
            None => std::ptr::null_mut(),
        };
        let _: () = msg_send![app, setAppearance: look];
    }
    #[cfg(not(target_os = "macos"))]
    let _ = pref;
}

/// Draw light or dark as the preference and the system say; true when the look changed.
pub fn sync_dark(cx: &App) -> bool {
    let on = match prefs().appearance.as_str() {
        "light" => false,
        "dark" => true,
        _ => matches!(cx.window_appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark),
    };
    let changed = theme::set_dark(on);
    if changed { themes::touch(); }
    changed
}

/// The current preferences (loaded on first use).
pub fn prefs() -> Preferences {
    if PREFS.read().unwrap().is_none() { load(); }
    PREFS.read().unwrap().clone().unwrap_or_default()
}

pub(crate) fn save(p: &Preferences) -> anyhow::Result<()> {
    use std::io::Write;
    let root = crate::store::Store::settings_dir();
    fdl_core::vfs::create_dir_all(&root)?;
    let tmp = root.join("preferences.json.tmp");
    let mut f = fdl_core::vfs::File::create(&tmp)?;
    f.write_all(&serde_json::to_vec_pretty(p)?)?;
    f.sync_all()?;
    fdl_core::vfs::rename(tmp, path())?;
    // The document theme, font and 正文字号 are the top-level folder in front's (W21).
    if let Some(root) = look_dir() {
        let look = crate::store::FolderLook { theme: Some(p.theme.clone()), feishu: Some(p.feishu), doc_family: Some(p.doc_family.clone()), doc_size: Some(p.doc_size.clone()) };
        if crate::store::Store::look_at(&root) != look { crate::store::Store::save_look_at(&root, &look)?; }
    }
    apply(p.clone());
    Ok(())
}

/// Redraw every window's document after the theme or language changed.
pub(crate) fn refresh_all(this: &mut Workspace, cx: &mut Context<Workspace>) {
    fn refresh(ws: &mut Workspace, cx: &mut Context<Workspace>) {
        ws.settings_filter.update(cx, |f, cx| { f.placeholder_color = Theme::text_disabled(); f.set_placeholder(tr("搜索快捷键", "Search shortcuts"), cx) });
        ws.search.update(cx, |f, _| f.placeholder_color = Theme::text_disabled());
        if let Some(editor) = &ws.editor {
            editor.update(cx, |e, cx| { e.cache.borrow_mut().clear(); e.refresh_theme(); cx.notify(); });
        }
        cx.notify();
    }
    refresh(this, cx);
    // The other windows. Skip this workspace, and never read a window that is
    // mid-update (the one a click came from): GPUI panics on that, so ask
    // `is_active` first, which is None for a window on the update stack.
    let me = cx.entity_id();
    for w in cx.windows() {
        if let Some(h) = w.downcast::<Workspace>()
            && h.is_active(cx).is_some()
            && h.entity(cx).is_ok_and(|e| e.entity_id() != me)
        {
            let _ = h.update(cx, |ws, _, cx| refresh(ws, cx));
        }
    }
    crate::app::set_menus(cx);
    cx.refresh_windows();
}

/// A theme file in the user's editor; scripted runs only log it (they must
/// not start other apps).
fn open_in_editor(p: &std::path::Path, cx: &mut App) {
    if crate::devtools::scripted() { eprintln!("devtools: open {}", p.display()); return }
    cx.open_url(&format!("file://{}", p.display()));
}

/// Theme files are watched: saving one in an editor redraws at once.
pub(crate) fn watch_themes(cx: &mut Context<Workspace>) {
    static LAST: Mutex<Option<Vec<(std::path::PathBuf, web_time::SystemTime, u64)>>> = Mutex::new(None);
    *LAST.lock().unwrap() = Some(themes::stamp());
    cx.spawn(async |this, cx| loop {
        cx.background_executor().timer(std::time::Duration::from_millis(800)).await;
        let now = cx.background_executor().spawn(async { themes::stamp() }).await;
        let changed = { let mut l = LAST.lock().unwrap(); let c = l.as_ref() != Some(&now); *l = Some(now); c };
        if changed && this.update(cx, |w, cx| w.reload_themes(cx)).is_err() { break }
        if this.upgrade().is_none() { break }
    }).detach();
}

// ---- the view

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page { #[default] General, Appearance, Shortcuts, Library, About }

impl Page {
    fn title(self) -> &'static str {
        match self {
            Page::General => tr("通用", "General"),
            Page::Appearance => tr("外观", "Appearance"),
            Page::Shortcuts => tr("快捷键", "Shortcuts"),
            Page::Library => tr("文档库", "Documents"),
            Page::About => tr("关于", "About"),
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Page::General => "view-settings",
            Page::Appearance => "palette",
            Page::Shortcuts => "keyboard",
            Page::Library => "storage",
            Page::About => "info",
        }
    }
}

/// The sidebar list, one evenly spaced run (six rows need no unlabelled
/// groups); `None` is 回收站 (its own view, shown with this sidebar).
const NAV: [Option<Page>; 6] = [Some(Page::General), Some(Page::Appearance), Some(Page::Shortcuts), Some(Page::Library), None, Some(Page::About)];

// ---- controls (switch on the accent, 30pt buttons, grouped rows)

const COLUMN_W: f32 = 640.;

/// A titled group: the caption above, then the rows in one card, split by
/// `Separator`s.
pub(crate) fn group(id: impl Into<ElementId>, title: Option<&'static str>, rows: Vec<AnyElement>) -> Div {
    let n = rows.len();
    let mut card = Card::new(id).w_full().overflow_hidden();
    for (i, r) in rows.into_iter().enumerate() {
        card = card.child(div().w_full().flex().flex_col().child(r));
        if i + 1 < n { card = card.child(Separator::new()); }
    }
    div().w_full().flex().flex_col().gap(px(8.)).children(title.map(caption)).child(card)
}

pub(crate) fn caption(t: &'static str) -> Div {
    div().px(px(4.)).text_size(px(theme::ui_font())).line_height(px(theme::ui_font() + 5.)).font_weight(FontWeight::SEMIBOLD).text_color(Theme::sidebar_section()).child(t)
}

/// One setting: `design::Item`, its control on the right.
fn row(title: impl Into<SharedString>, desc: Option<SharedString>, control: impl IntoElement) -> Div {
    div().w_full().child(crate::design::Item::new(title).description(desc).child(control))
}

fn desc(s: &'static str) -> Option<SharedString> { Some(s.into()) }

/// A `Switch` that takes the click (the switch itself only draws).
pub(crate) fn switch(id: impl Into<ElementId>, on: bool) -> Stateful<Div> {
    let id = id.into();
    div().id(id.clone()).flex_none().flex().items_center().cursor_pointer().child(Switch::new(on).keyed(id))
}

/// 外观 › 主题 CSS (duplicate, edit, restore a theme's stylesheet): hidden in
/// this release while editing themes by hand settles.
const THEME_CSS: bool = false;

/// An open dropdown on 通用 (see `Workspace::select`): its choices, which one
/// is picked and which is highlighted, and what picking one does.
pub struct PrefMenu {
    id: &'static str,
    labels: Vec<SharedString>,
    selected: usize,
    focused: usize,
    on: std::rc::Rc<dyn Fn(&mut Workspace, usize, &mut Context<Workspace>)>,
}

/// Which family a font list picks: the note's text or the interface's.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum FontTarget { Doc, Ui }

impl FontTarget {
    fn key(self) -> &'static str { match self { FontTarget::Doc => "doc", FontTarget::Ui => "ui" } }
    /// The family in use ("" follows the theme / the platform).
    fn current(self) -> String { match self { FontTarget::Doc => prefs().doc_family, FontTarget::Ui => prefs().ui_family } }
    /// Its 最近使用.
    fn recent(self, p: &Preferences) -> &Vec<String> { match self { FontTarget::Doc => &p.recent_doc_fonts, FontTarget::Ui => &p.recent_ui_fonts } }
    /// The first row of the list: put the setting back to the default.
    fn reset_label(self) -> &'static str {
        match self { FontTarget::Doc => tr("跟随主题", "Theme default"), FontTarget::Ui => tr("跟随系统字体", "System font") }
    }
}

/// 外观 › 文档字体 / 界面字体: the open font list. Its search field filters the
/// system's families (`crate::fonts`, read once per run); ↑ / ↓ walk the
/// list, ↩ picks.
pub struct FontMenu {
    pub field: Entity<TextField>,
    target: FontTarget,
    /// The row the arrow keys or the pointer are on (a font can be listed
    /// twice: in 收藏 and in its section). Reset when the rows change.
    focused: Option<usize>,
    /// 其他语言和符号 is open.
    others: bool,
    /// The family the page shows for now (hover / arrows), until the list
    /// closes: picked, it stays; else the saved one comes back.
    previewing: Option<String>,
    preview_task: Option<Task<()>>,
    /// The list is virtual (hundreds of families, each shaped in its own
    /// face) and follows the arrow keys.
    scroll: UniformListScrollHandle,
    _sub: Subscription,
}

/// A row of a font list.
#[derive(Clone, Debug, PartialEq)]
enum Choice {
    /// 跟随主题 / 跟随系统字体.
    Reset,
    /// A section's caption (收藏, 最近使用, a class).
    Header(SharedString),
    /// An installed family, or (`dl`) one of `font_downloads`'s fonts.
    /// `flat`: outside its class's section (收藏, 最近使用, a search), so it
    /// says what it is.
    Font { family: String, dl: Option<&'static str>, flat: bool },
    /// 其他语言和符号 (how many), open or not.
    Others(usize, bool),
}

impl Choice {
    /// What focus and picking go by: "" for the reset, the family, or
    /// `DOWNLOAD` and the font's id.
    fn key(&self) -> Option<String> {
        match self {
            Choice::Reset => Some(String::new()),
            Choice::Font { dl: Some(id), .. } => Some(format!("{DOWNLOAD}{id}")),
            Choice::Font { family, .. } => Some(family.clone()),
            _ => None,
        }
    }
}

/// What marks a download row's key.
const DOWNLOAD: char = '\u{1}';

/// The family a key draws in, if it can be shown now ("" for the reset; a
/// download that isn't here yet: None).
fn key_family(key: &str, cx: &App) -> Option<String> {
    let Some(id) = key.strip_prefix(DOWNLOAD) else { return Some(key.to_string()) };
    let e = crate::font_downloads::entry(id)?;
    (crate::font_downloads::fonts(id).is_some() || crate::fonts::installed(&e.family, cx)).then(|| e.family.clone())
}

/// The sections in the interface language's order: the CJK classes first
/// for Chinese, Japanese and Korean, the Latin ones first otherwise.
fn class_order(lang: Lang) -> [crate::font_classes::Class; 8] {
    use crate::font_classes::Class::*;
    if matches!(lang, Lang::Zh | Lang::ZhTw | Lang::Ja | Lang::Ko) { [Hei, Song, Kai, Yuan, Mono, LatinSans, LatinSerif, Other] } else { [LatinSans, LatinSerif, Mono, Hei, Song, Kai, Yuan, Other] }
}

/// What a row's font is (a download's from the catalog).
fn font_info(family: &str, dl: Option<&crate::font_downloads::Entry>) -> crate::font_classes::Info {
    use crate::font_classes::{guess, info, Class, Info};
    if let Some(e) = dl {
        let cjk = e.group == crate::font_downloads::Group::Cjk || e.id == "maple-mono-cn";
        // a Chinese font in the catalog covers simplified Chinese (tools/fonts/stage.py checks)
        let han = if cjk { 1. } else { 0. };
        return Info { class: e.class(), latin: true, han, trad: cjk, kana: false, hangul: false, arabic: false, symbol: false, other_lang: false };
    }
    let mut i = info(family).unwrap_or_else(|| guess(family));
    if let Some(c) = crate::font_classes::known(family) { i.class = c; }
    if i.class == Class::Other && i.latin && !i.symbol { i.class = Class::LatinSans; }
    i
}

/// Folded under 其他语言和符号: symbol faces, and faces for another script
/// than the interface's (Arabic faces stay out for the Arabic interface).
fn folded(i: &crate::font_classes::Info, lang: Lang) -> bool {
    let own = match lang { Lang::Ar => i.arabic, Lang::Ko => i.hangul, Lang::Ja => i.kana, _ => false };
    i.symbol || i.other_lang && !own
}

/// Within a section, what suits the interface language first.
fn rank(i: &crate::font_classes::Info, lang: Lang) -> u8 {
    match lang {
        Lang::Zh => if i.simplified() { 0 } else if i.han > 0.2 { 1 } else { 2 },
        Lang::ZhTw => if i.trad { 0 } else { 1 },
        Lang::Ja => if i.kana { 0 } else { 1 },
        Lang::Ko => if i.hangul { 0 } else { 1 },
        _ => if i.latin { 0 } else { 1 },
    }
}

/// 外观's font list. Without a search: the reset, 收藏, 最近使用 (5 at most),
/// then a section per class (`font_classes`) in the interface language's
/// order, the installed families and (文档字体) the downloadable fonts, then
/// 其他语言和符号 folded. A search lists every match, flat.
fn font_choices(term: &str, target: FontTarget, others: bool, cx: &App) -> Vec<Choice> {
    let downloads: Vec<&'static crate::font_downloads::Entry> = if target == FontTarget::Doc { crate::font_downloads::listed().collect() } else { vec![] };
    let p = prefs();
    arrange(term, others, &crate::fonts::families(cx), &downloads, &p.font_favorites, target.recent(&p), lang())
}

/// `font_choices` from its inputs.
fn arrange(term: &str, others: bool, families: &[String], downloads: &[&'static crate::font_downloads::Entry], favorites: &[String], recent: &[String], lang: Lang) -> Vec<Choice> {
    use crate::font_downloads as fd;
    // installed families, minus those the download list holds
    let mut all: Vec<(String, Option<&'static fd::Entry>)> = families.iter().filter(|f| !downloads.iter().any(|d| d.family.eq_ignore_ascii_case(f))).map(|f| (f.clone(), None)).collect();
    all.extend(downloads.iter().map(|d| (d.family.clone(), Some(*d))));
    let name = |f: &str, d: Option<&fd::Entry>| d.map_or_else(|| f.to_string(), |d| d.name().to_string());
    let font = |f: &str, d: Option<&'static fd::Entry>, flat: bool| Choice::Font { family: f.to_string(), dl: d.map(|d| d.id.as_str()), flat };
    let order = class_order(lang);
    let sort_key = |(f, d): &(String, Option<&'static fd::Entry>)| { let i = font_info(f, *d); (order.iter().position(|c| *c == i.class), rank(&i, lang), d.is_some(), crate::font_classes::known_rank(f), name(f, *d).to_lowercase()) };
    let term = term.trim().to_lowercase();
    if !term.is_empty() {
        let mut hits: Vec<_> = all.into_iter().filter(|(f, d)| f.to_lowercase().contains(&term) || d.is_some_and(|d| d.zh.to_lowercase().contains(&term) || d.en.to_lowercase().contains(&term))).collect();
        hits.sort_by_cached_key(sort_key);
        return hits.iter().map(|(f, d)| font(f, *d, true)).collect();
    }
    let find = |f: &String| all.iter().find(|(a, _)| a.eq_ignore_ascii_case(f)).cloned();
    let mut list = vec![Choice::Reset];
    for (label, picks) in [(tr("收藏", "Favorites"), favorites), (tr("最近使用", "Recent"), recent)] {
        let rows: Vec<_> = picks.iter().filter_map(find).take(5).collect();
        if rows.is_empty() { continue }
        list.push(Choice::Header(label.into()));
        list.extend(rows.iter().map(|(f, d)| font(f, *d, true)));
    }
    let (mut shown, mut hidden): (Vec<_>, Vec<_>) = all.into_iter().partition(|(f, d)| !folded(&font_info(f, *d), lang));
    shown.sort_by_cached_key(sort_key);
    hidden.sort_by_cached_key(|(f, _)| f.to_lowercase());
    let mut class = None;
    for (f, d) in &shown {
        let c = font_info(f, *d).class;
        if class != Some(c) { list.push(Choice::Header(c.label().into())); class = Some(c); }
        list.push(font(f, *d, false));
    }
    if !hidden.is_empty() {
        list.push(Choice::Others(hidden.len(), others));
        if others { list.extend(hidden.iter().map(|(f, d)| font(f, *d, true))); }
    }
    list
}

/// A row's sample, in the interface language.
fn font_sample() -> &'static str {
    match lang() {
        Lang::Zh | Lang::ZhTw => "永和九年 The quick brown fox",
        Lang::Ja => "いろはにほへと The quick brown fox",
        Lang::Ko => "다람쥐 헌 쳇바퀴 The quick brown fox",
        Lang::Ar => "أبجد هوز The quick brown fox",
        _ => "The quick brown fox jumps",
    }
}

/// A font row: its name (the interface's face), a sample in its own face
/// (下载后可预览 for a download not here yet), what it is (西文, 繁体为主 …,
/// 试用), then a check when in use, a download's size or progress, its
/// delete button, and the star that keeps it in 收藏.
/// `key` / `row` / `target`: what hovering previews and a click picks.
#[allow(clippy::too_many_arguments)]
fn font_row(family: &str, dl: Option<&'static str>, flat: bool, g: SharedString, focused: bool, current: &str, (key, row, target): (String, usize, FontTarget), cx: &mut Context<Workspace>) -> Div {
    use crate::font_downloads as fd;
    let e = dl.and_then(fd::entry);
    let zh = matches!(lang(), Lang::Zh | Lang::ZhTw);
    let info = font_info(family, e);
    let small = |t: String| div().flex_none().text_size(px(crate::design::geo().text_xs)).text_color(crate::design::color::muted_foreground()).child(t);
    let tag = |t: &str| crate::design::Badge::new(t.to_string());
    let here = e.is_none_or(|e| fd::on_disk(&e.id) || crate::fonts::installed(&e.family, cx) && !fd::registered(&e.id));
    let drawable = e.is_none_or(|e| crate::fonts::installed(&e.family, cx) || fd::fonts(&e.id).is_some() && fd::register(&e.id, cx.text_system()));
    let mut tags: Vec<&str> = vec![];
    if zh && !info.class.cjk() && info.han < 0.2 && !folded(&info, lang()) && (flat || info.class == crate::font_classes::Class::Mono) { tags.push(tr("西文", "Latin")); }
    if lang() == Lang::Zh && info.partial_han() { tags.push(if info.trad { tr("繁体为主", "Traditional") } else { tr("部分中文", "Partial Chinese") }); }
    if e.is_some_and(|e| e.trial) { tags.push(tr("试用", "Trial")); }
    let picked = family.eq_ignore_ascii_case(current);
    let status = match e {
        Some(e) if fd::pending(&e.id) => small(format!("{:.0}%", fd::progress(&e.id) * 100.)).into_any_element(),
        Some(e) if !here => {
            let mb = e.size() as f64 / 1e6;
            div().flex_none().flex().items_center().gap(px(4.)).child(small(if mb < 10. { format!("{mb:.1} MB") } else { format!("{mb:.0} MB") }))
                .child(crate::design::icon("download", 14., crate::design::color::muted_foreground())).into_any_element()
        }
        _ => div().flex_none().w(px(16.)).flex().justify_center().when(picked, |d| d.child(crate::design::icon("check", 14., crate::design::color::foreground()))).into_any_element(),
    };
    let fav = prefs().font_favorites.iter().any(|f| f.eq_ignore_ascii_case(family));
    let label: SharedString = e.map_or_else(|| family.to_string(), |e| e.name().to_string()).into();
    let sample = if drawable { div().font_family(SharedString::from(family.to_string())).text_color(crate::design::color::muted_foreground()).child(font_sample()) }
        else { div().text_color(crate::design::color::disabled()).child(tr("下载后可预览", "Preview after download")) };
    let (del, star) = (dl.map(str::to_string), family.to_string());
    let hover_key = key.clone();
    div().w_full().flex().flex_col().relative().children(crate::devtools::row_probe(label.clone())).child(MenuItem::new(g.clone()).selected(focused)
        .on_hover(cx.listener(move |w, hovered: &bool, _, cx| if *hovered { w.preview_font(hover_key.clone(), row, cx) }))
        .on_click(cx.listener(move |w, _, window, cx| w.pick_font(key.clone(), target, window, cx)))
        .child(div().flex_none().w(px(150.)).overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui::ui_text(label)))
        .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_size(px(theme::ui_font())).child(sample))
        .children(tags.into_iter().map(tag))
        .child(status)
        .when(e.is_some_and(|e| fd::on_disk(&e.id) && !fd::pending(&e.id)), |d| d.child(div().relative().children(crate::devtools::anchor_probe(format!("font-del-{}", del.clone().unwrap_or_default())))
            .child(Button::ghost(SharedString::from(format!("font-del-{}", del.clone().unwrap_or_default()))).sized(ButtonSize::Xs).icon("bin")
            .on_click(cx.listener(move |w, _, _, cx| { cx.stop_propagation(); if let Some(id) = &del { w.delete_download(id, cx) } })))))
        .child(Button::ghost(SharedString::from(format!("{g}-star"))).sized(ButtonSize::Xs)
            .child(crate::design::icon("star", 14., if fav { crate::design::color::primary() } else { crate::design::color::disabled() }))
            .on_click(cx.listener(move |w, _, _, cx| { cx.stop_propagation(); w.toggle_favorite(&star, cx) }))))
}

thread_local! {
    /// 正文字号's 自定义 was picked this session: its stepper shows though the size may be a preset's.
    static DOC_CUSTOM: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Bumped at each step of 正文字号's stepper, so only the last of a burst is written.
    static DOC_SIZE_GEN: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// A segmented control, each segment `(id, i)`.
fn segmented(id: &'static str, labels: &[&'static str], selected: usize, on: impl Fn(&mut Workspace, usize, &mut Context<Workspace>) + Clone + 'static, cx: &mut Context<Workspace>) -> Segmented {
    Segmented::new().children(labels.iter().enumerate().map(|(i, label)| {
        let on = on.clone();
        Segment::new((id, i), *label).selected(i == selected).on_click(cx.listener(move |w, _, _, cx| on(w, i, cx)))
    }))
}

/// A keyboard shortcut: its keys as `Kbd`s in a field, which a click puts in
/// recording (`recording`: focused, waiting for keys).
pub(crate) fn keycap(id: impl Into<ElementId>, key: &str, recording: bool) -> Field { keys_field(id, key, recording, tr("按下新的组合键…", "Press keys…")) }

/// `keycap` with its own words while waiting for keys.
pub(crate) fn keys_field(id: impl Into<ElementId>, key: &str, recording: bool, waiting: &'static str) -> Field {
    let muted = |t: &str| div().text_color(crate::design::color::muted_foreground()).child(t.to_string()).into_any_element();
    let body = if key.is_empty() { muted(if recording { waiting } else { tr("未设置", "Not set") }) } else { crate::design::KbdGroup::new(crate::shortcuts::strokes(key)).into_any_element() };
    Field::new(id).sized(FieldSize::Sm).focused(recording).min_w(px(144.)).justify_center().cursor_pointer().child(body)
}

/// A small text button (恢复默认 …): a ghost button.
pub(crate) fn link_button(id: impl Into<ElementId>, label: &'static str) -> Button {
    Button::ghost(id).label(label)
}

/// A row's button: an outline one at the panel size.
pub(crate) fn button(id: impl Into<ElementId>, label: &'static str) -> Button {
    Button::outline(id).sized(ButtonSize::Sm).label(label)
}

// ---- theme preview

/// Preview scale against the real document. GPUI clips to rectangles (its
/// masks have no radius), so the page rounds its own bottom corners to the
/// group's (`SAMPLE_RADIUS`) instead of being clipped.
const PAGE_S: f32 = 0.9;
/// The settings group's radius (the card's).
const SAMPLE_RADIUS: f32 = crate::design::G.card_r;

/// 外观's sample page: a document with most of what a theme styles (headings,
/// long paragraphs with bold and italics, mixed Chinese and Latin text, lists,
/// a task, quote, callout, table, divider, highlighted code), at scale `s`.
fn sample_page(t: &themes::Theme, s: f32) -> Div {
    let st = t.style;
    let sc = |v: f32| px((v * s).max(0.5));
    let text = st.color("page.color").unwrap_or_else(|| st.token("text"));
    // 正文字号 too, by the same ratio as the note (themes::doc_len).
    let dl = |n: f32| themes::doc_len_in(st, n);
    let (body, line) = (dl(st.num("page.size")), dl(st.num("page.line")));
    // 文档字体 follows into the sample: what the page shows is what the note looks like.
    let (font, mono): (SharedString, SharedString) = (themes::doc_font_in(st, "page.font").into(), st.font("pre.font").into());
    let gap = sc(st.num("p.gap")).clamp(px(4.), px(9.));
    let p = |c: Div| c.text_size(sc(body)).line_height(sc(line));
    let heading = |l: u8, s: &'static str| {
        let h = format!("h{l}");
        div().text_size(sc(dl(st.num(&format!("{h}.size"))))).line_height(sc(dl(st.num(&format!("{h}.line"))))).font_weight(FontWeight(st.num(&format!("{h}.weight"))))
            .text_color(st.color(&format!("{h}.color")).unwrap_or(text)).child(s)
    };
    let link = st.color("a.color").unwrap_or(text);
    let chip = |s: &'static str| div().px(sc(st.num("code.pad")).max(px(2.))).rounded(sc(st.num("code.radius"))).bg(st.color("code.bg").unwrap_or_else(|| st.token("code_chip")))
        .border_1().border_color(st.color("code.border").unwrap_or_else(|| st.token("code_chip_border"))).font_family(mono.clone()).text_color(st.color("code.color").unwrap_or(text)).child(s);
    let para = p(div().flex().flex_wrap().items_center())
        .child(tr("Quiet.md 是一款", "Quiet.md is a "))
        .child(div().font_weight(FontWeight(st.num("strong.weight"))).text_color(st.color("strong.color").unwrap_or(text)).child(tr("安静的本地笔记应用", "quiet, local notes app")))
        .child(tr("，", ", "))
        .child(tr("文档就是电脑上的文件。", "your notes are files. "))
        .child(tr("输入 ", "Type "))
        .child(chip("/"))
        .child(tr(" 插入内容，", " to insert, "))
        .child(tr("详见", "see "))
        // The full stop rides with the link, so it never wraps alone.
        .child(div().flex().child(div().text_color(link).when(st.flag("a.underline"), |d| d.underline()).child(tr("写作基础", "Writing basics"))).child(tr("。", ".")));
    // Long paragraphs wrap as one text (pieces in a flex row would break only
    // between pieces); `strong` and `em` are spans of it.
    let strong_c = st.color("strong.color").unwrap_or(text);
    let prose = |t: &'static str, strong: &'static str, em: &'static str| {
        let mut marks = vec![];
        if let Some(i) = t.find(strong).filter(|_| !strong.is_empty()) { marks.push((i..i + strong.len(), HighlightStyle { font_weight: Some(FontWeight(st.num("strong.weight"))), color: Some(strong_c), ..Default::default() })); }
        if let Some(i) = t.find(em).filter(|_| !em.is_empty()) { marks.push((i..i + em.len(), HighlightStyle { font_style: Some(FontStyle::Italic), ..Default::default() })); }
        marks.sort_by_key(|m| m.0.start);
        p(div()).child(StyledText::new(t).with_highlights(marks))
    };
    let long = prose(tr("好的阅读字体让长文不累眼：字距匀称，标点端正，笔画在小字号下依然清楚。正文用文档字体排版，标题跟着一起变；代码块仍然用主题的等宽字体，不受影响。写得久了你会发现，安静的页面最适合思考，也最适合把一件事慢慢想清楚。",
        "A good reading face keeps long text easy on the eyes: even spacing, upright punctuation, strokes that stay clear at small sizes. Body text uses the document font and headings follow; code blocks keep the theme's monospace face. Write for a while and you'll find a quiet page is the best place to think a thing through."),
        tr("文档字体", "document font"), tr("代码块仍然用主题的等宽字体", "code blocks keep the theme's monospace face"));
    let mixed = prose(tr("笔记存成 Markdown（.md）和 DocxXML 文件，同步交给 iCloud、Git 或 Syncthing。中文与 English 混排时，引号写作“这样”，书名写作《瓦尔登湖》（Walden, 1854），数字 0123456789 也排得整齐。",
        "Notes are stored as Markdown (.md) and DocxXML files; syncing is left to iCloud, Git or Syncthing. Mixed text keeps its own punctuation — “quotes”, (parentheses), Walden (1854) — and the figures 0123456789 line up."),
        "", tr("Walden, 1854", "Walden"));
    let indent = sc(st.num("list.indent")).max(px(10.));
    let marker = |m: Div| div().flex_none().w(indent).flex().justify_center().child(m);
    let bullet_c = st.color("bullet.color").unwrap_or(text);
    let bullet = |s: &'static str| div().flex().items_center().child(marker(div().size(sc(6.).max(px(3.))).rounded_full().bg(bullet_c))).child(p(div()).child(s));
    let num_c = st.color("number.color").unwrap_or(text);
    let number = |n: &'static str, s: &'static str| div().flex().items_center().child(marker(p(div()).text_color(num_c).child(n))).child(p(div()).child(s));
    let task_size = st.num("task.size");
    let task = |done: bool, label: &'static str| div().flex().items_center().child(marker(
            div().size(sc(task_size)).rounded(sc(st.num("task.radius"))).flex().items_center().justify_center()
                .when(done, |d| d.bg(st.color("task.checked").unwrap_or_else(|| st.token("primary")))
                    .child(ui::icon("todo-check", (task_size * s * 0.75).round(), st.color("task.check").unwrap_or_else(gpui::white))))
                .when(!done, |d| d.border_1().border_color(st.color("task.border").unwrap_or_else(|| st.token("todo_box"))))))
        .child(p(div()).child(label).when(done, |d| d.text_color(st.color("done.color").unwrap_or_else(|| st.token("todo_done"))).when(st.flag("done.strike"), |d| d.line_through())));
    let quote = div().flex().gap(sc((st.num("quote.indent") - st.num("quote.bar")).max(4.)))
        .child(div().flex_none().w(sc(st.num("quote.bar")).max(px(1.))).rounded(px(1.)).bg(st.color("quote.bar-color").unwrap_or_else(|| st.token("quote_bar"))))
        .child(p(div()).text_color(st.color("quote.color").unwrap_or(text)).when(st.flag("quote.italic"), |d| d.italic()).child(tr("每一篇指南都是用 Quiet.md 写成的，放心修改。", "Every guide is written in Quiet.md. Edit them freely.")));
    let callout = div().px(sc(st.num("callout.pad-x"))).py(sc(st.num("callout.pad-y"))).rounded(sc(st.num("callout.radius")))
        .bg(st.named("callout", "light-yellow").unwrap_or_else(|| st.token("code_chip")))
        .when(st.num("callout.border") > 0., |d| d.border(sc(st.num("callout.border")).max(px(1.))).border_color(st.named("border", "yellow").unwrap_or_else(|| st.token("code_chip_border"))))
        .child(p(div().flex().gap(px(6.))).child("💡").child(tr("一边读一边动手试，记得最快。", "Try things as you read; it sticks faster.")));
    let line_c = st.color("table.line").unwrap_or_else(|| st.token("table_line"));
    let cell = |s: &'static str, head: bool| div().flex_1().px(px(6.)).py(px(3.)).when(head, |d| d.bg(st.color("th.bg").unwrap_or_else(|| st.token("table_header"))).font_weight(FontWeight::SEMIBOLD)).child(s);
    let row = |a: &'static str, b: &'static str, head: bool, last: bool| div().flex().when(!last, |d| d.border_b_1().border_color(line_c))
        .child(cell(a, head).border_r_1().border_color(line_c)).child(cell(b, head));
    let table = div().w_full().rounded(sc(st.num("table.radius"))).border_1().border_color(line_c).overflow_hidden()
        .text_size(sc(dl(st.num("table.size")))).line_height(sc(dl(st.num("table.line-height"))))
        .child(row(tr("按键", "Keys"), tr("作用", "Does"), true, false)).child(row(crate::shortcuts::keys("⌘ N"), tr("新建文档", "New document"), false, false)).child(row(crate::shortcuts::keys("⌘ J"), tr("搜索所有文档", "Search documents"), false, true));
    let (hr_c, hr_w) = (st.color("hr.color").unwrap_or_else(|| st.token("separator_dots")), (st.num("hr.width") * s).max(0.6));
    // Painted to the column's width: a row of fixed-size dots would set the
    // column's minimum width and squeeze its neighbour.
    let hr_style = st.line("hr.style");
    let hr = canvas(|_, _, _| {}, move |b, _, window, _| {
        let (step, dash) = match hr_style { themes::LineStyle::Solid => return window.paint_quad(fill(Bounds::new(b.origin, size(b.size.width, px(hr_w))), hr_c)), themes::LineStyle::Dashed => (8., 4.), themes::LineStyle::Dotted => (hr_w + 2.5, hr_w) };
        let mut x = 0.;
        while x + dash <= f32::from(b.size.width) {
            window.paint_quad(fill(Bounds::new(point(b.origin.x + px(x), b.origin.y), size(px(dash), px(hr_w))), hr_c).corner_radii(px(if hr_style == themes::LineStyle::Dotted { hr_w / 2. } else { 0. })));
            x += step;
        }
    }).w_full().h(px(hr_w));
    let syn = |k: &str| st.color(&format!("syntax.{k}")).unwrap_or(text);
    let tok = |s: &'static str, c: Hsla| div().text_color(c).child(s);
    let code_line = |n: &'static str, parts: Vec<Div>| div().flex()
        .when(st.flag("pre.numbers"), |d| d.child(div().flex_none().w(px(16.)).mr(px(8.)).text_color(st.color("pre.number-color").unwrap_or_else(|| st.token("code_line_number"))).child(n)))
        .children(parts);
    let code = div().w_full().overflow_hidden().px(px(10.)).py(px(7.)).rounded(sc(st.num("pre.radius"))).bg(st.color("pre.bg").unwrap_or_else(|| st.token("code_bg")))
        .border_1().border_color(st.color("pre.border").unwrap_or_else(|| st.token("code_border")))
        .font_family(mono.clone()).text_size(sc(dl(st.num("pre.size")))).line_height(sc(dl(st.num("pre.line")))).text_color(st.color("pre.color").unwrap_or_else(|| st.token("code_text")))
        .child(code_line("1", vec![tok("# ", syn("comment")), tok(tr("备份文档库", "back up your documents"), syn("comment"))]))
        .child(code_line("2", vec![tok("cd ", syn("function")), tok("~/Documents ", text), tok("&& ", syn("keyword")), tok("tar ", syn("function")), tok("-czf ", syn("attr")), tok("\"Quiet.tar.gz\" ", syn("string")), tok("Quiet.md", text)]))
        .child(code_line("3", vec![tok("echo ", syn("function")), tok("\"done\"", syn("string"))]));
    // One column, like a page: a centred text column (the theme's width, scaled,
    // up to 880 so a wide frame isn't mostly margin) whose bottom fades into
    // the paper. Blocks come in the order the 开始指南 itself uses.
    let paper = st.color("page.bg").unwrap_or_else(|| st.token("bg_body"));
    // Its own bottom corners too: nothing clips it to the page's radius.
    let fade = div().absolute().left_0().right_0().bottom_0().h(px(56.)).rounded_b(px(SAMPLE_RADIUS)).bg(linear_gradient(180., linear_color_stop(paper.opacity(0.), 0.), linear_color_stop(paper, 1.)));
    // items_start: a column stretched to the frame would shrink its blocks to fit.
    div().relative().size_full().flex().justify_center().items_start().bg(paper).rounded_b(px(SAMPLE_RADIUS)).font_family(font).text_color(text).overflow_hidden()
        // min_w_0: the column follows the frame, not its widest unbreakable line.
        .child(div().w_full().min_w_0().overflow_hidden().max_w((sc(st.num("page.width")) * 1.5).min(px(880.))).px(px(40.)).pt(px(28.)).flex().flex_col().gap(gap)
            .child(div().text_size(sc(dl(st.num("title.size")))).line_height(sc(dl(st.num("title.size")) * 1.25)).font_weight(FontWeight(st.num("title.weight")))
                .font_family(SharedString::from(themes::doc_font_in(st, "title.font"))).text_color(st.color("title.color").unwrap_or(text)).child(tr("开始指南", "Getting started")))
            .child(para)
            .child(long)
            .child(callout)
            .child(heading(1, tr("五分钟上手", "Five minutes")))
            .child(div().flex().flex_col().child(task(true, tr("打开这份指南", "Open this guide"))).child(task(false, crate::shortcuts::keys(tr("按 ⌘ N 新建一篇文档", "Press ⌘ N for a new document")))))
            .child(heading(2, tr("三种输入方式", "Three ways to write")))
            .child(div().flex().flex_col().child(bullet(tr("斜杠菜单：输入 / 插入任意内容块", "Slash menu: type / to insert any block"))).child(bullet(tr("Markdown：# 加空格变标题", "Markdown: # and a space make a heading"))))
            .child(div().flex().flex_col().child(number("1.", tr("先读「写作基础」", "Read Writing basics first"))).child(number("2.", tr("再看「内容块」", "Then Content blocks"))))
            .child(quote)
            .child(heading(2, tr("中英混排", "Mixed scripts")))
            .child(mixed)
            .child(heading(3, tr("最常用的快捷键", "Shortcuts to know")))
            .child(table)
            .child(div().w_full().py(px(2.)).child(hr))
            .child(code))
        .child(fade)
}

fn badge(t: &themes::Theme) -> Option<&'static str> {
    if t.customized() { Some(tr("已修改", "Edited")) } else if t.bundled.is_none() { Some(tr("自定义", "Custom")) } else { None }
}

/// A pop-up button: a select showing the current choice, open while its menu is.
fn popup_button(id: impl Into<ElementId>, label: impl Into<SharedString>, open: bool) -> Field {
    Field::select(id).sized(FieldSize::Sm).max_w(px(220.)).open(open).child(div().min_w_0().truncate().child(ui::ui_text(label.into())))
}

fn apply_theme(w: &mut Workspace, id: String, cx: &mut Context<Workspace>) {
    let feishu = themes::themes().iter().find(|t| t.id == id).is_some_and(|t| t.style.feishu);
    w.theme_menu = None;
    w.set_pref(move |p| { p.theme = id; p.feishu = feishu; }, cx)
}

impl Workspace {
    /// A compact theme list; applying a choice updates the page preview below.
    pub(crate) fn render_theme_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let focused = self.theme_menu.as_ref()?;
        if self.view != View::Settings || self.settings_page != Page::Appearance { return None }
        let b = self.theme_anchor.get()?;
        let current = prefs().theme_id();
        let list = themes::themes();
        let rows: Vec<AnyElement> = list.iter().map(|t| {
            let g = SharedString::from(format!("theme-opt-{}", t.id));
            let (id, hover_id) = (t.id.clone(), t.id.clone());
            MenuItem::new(g).label(t.name().to_string()).selected(&t.id == focused)
                .children(badge(t).map(|b| crate::design::Badge::new(label(b))))
                .checked(t.id == current)
                .on_hover(cx.listener(move |w, hovered: &bool, _, cx| if *hovered && w.theme_menu.as_ref() != Some(&hover_id) { w.theme_menu = Some(hover_id.clone()); cx.notify(); }))
                .on_click(cx.listener(move |w, _, _, cx| apply_theme(w, id.clone(), cx)))
                .into_any_element()
        }).collect();
        drop(list);
        let anchor = self.theme_anchor.clone();
        let menu = Popover::new("theme-menu")
            .on_mouse_down_out(cx.listener(move |w, ev: &MouseDownEvent, _, cx| {
                // A press on the button itself: its click closes the menu.
                if anchor.get().is_some_and(|b| b.contains(&ev.position)) { return }
                w.theme_menu = None;
                cx.notify();
            }))
            .child({ let h = crate::design::scroll::handle("theme-menu-list"); div().id("theme-menu-list").w(px(220.)).max_h(px(302.)).overflow_y_scroll().track_scroll(&h).py(px(menu::PAD)).flex().flex_col().children(rows).child(crate::design::scroll::bar("theme-menu-list", &h, crate::design::geo().menu_r)) });
        Some(deferred({ let (pos, corner) = crate::design::hang(b, crate::design::Align::End, None); anchored().anchor(corner).position(pos) }.snap_to_window_with_margin(px(8.)).child(menu)).with_priority(2).into_any_element())
    }

    /// A choice as a dropdown button, like 界面语言's: the picked label, a menu of the rest.
    fn select<L: Into<SharedString>>(&self, id: &'static str, labels: Vec<L>, selected: usize, on: impl Fn(&mut Workspace, usize, &mut Context<Workspace>) + 'static, cx: &mut Context<Self>) -> Field {
        let labels: Vec<SharedString> = labels.into_iter().map(Into::into).collect();
        let open = self.pref_menu.as_ref().is_some_and(|m| m.id == id);
        let anchors = self.pref_anchors.clone();
        let on: std::rc::Rc<dyn Fn(&mut Workspace, usize, &mut Context<Workspace>)> = std::rc::Rc::new(on);
        popup_button(id, labels.get(selected).cloned().unwrap_or_default(), open)
            .child(canvas(move |b, _, _| { anchors.borrow_mut().insert(id, b); }, |_, _, _, _| {}).absolute().inset_0())
            .relative()
            .on_click(cx.listener(move |w, _, _, cx| {
                w.lang_menu = None;
                w.pref_menu = (!open).then(|| PrefMenu { id, labels: labels.clone(), selected, focused: selected, on: on.clone() });
                cx.notify();
            }))
    }

    pub(crate) fn render_pref_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let m = self.pref_menu.as_ref()?;
        if self.view != View::Settings || !matches!(self.settings_page, Page::General | Page::Library) { return None }
        let b = *self.pref_anchors.borrow().get(m.id)?;
        let rows = m.labels.iter().enumerate().map(|(i, label)| {
            let g = SharedString::from(format!("{}-opt-{i}", m.id));
            MenuItem::new(g).label(self::label(label)).selected(i == m.focused).checked(i == m.selected)
                .on_hover(cx.listener(move |w, hovered: &bool, _, cx| if *hovered && let Some(m) = &mut w.pref_menu && m.focused != i { m.focused = i; cx.notify(); }))
                .on_click(cx.listener(move |w, _, _, cx| w.pick_pref(i, cx)))
        }).collect::<Vec<_>>();
        let anchors = self.pref_anchors.clone();
        let id = m.id;
        let menu = Popover::new("pref-menu").menu().min_w(px(160.))
            .on_mouse_down_out(cx.listener(move |w, ev: &MouseDownEvent, _, cx| {
                if anchors.borrow().get(id).is_some_and(|b| b.contains(&ev.position)) { return }
                w.pref_menu = None;
                cx.notify();
            }))
            .children(rows);
        { let (pos, corner) = crate::design::hang(b, crate::design::Align::End, None); Some(deferred(anchored().anchor(corner).position(pos).snap_to_window_with_margin(px(8.)).child(menu.fit(crate::design::room(pos, corner)))).with_priority(2).into_any_element()) }
    }

    fn pick_pref(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(m) = self.pref_menu.take() else { return };
        (m.on)(self, i, cx);
        cx.notify();
    }

    /// 外观 › 文档字体 / 界面字体: open (or close) the font list, its search
    /// field focused.
    pub(crate) fn toggle_font_menu(&mut self, target: FontTarget, window: &mut Window, cx: &mut Context<Self>) {
        if self.font_menu.is_some() {
            self.close_font_menu(cx);
            window.focus(&self.focus, cx);
            return;
        }
        crate::font_classes::scan(cx);
        // downloaded fonts this run hasn't loaded: their samples need them
        if target == FontTarget::Doc {
            for e in crate::font_downloads::listed() {
                if crate::font_downloads::on_disk(&e.id) && crate::font_downloads::fonts(&e.id).is_none() { crate::font_downloads::want(&e.id, crate::font_downloads::Mode::Cache); }
            }
        }
        let field = cx.new(|cx| {
            let mut f = TextField::new(tr("搜索字体", "Search fonts"), cx);
            f.placeholder_color = Theme::text_disabled();
            f
        });
        // Typing re-filters the list; the keys walk it (the field stops the
        // window's own key handling, so the events come from here).
        let sub = cx.subscribe_in(&field, window, |w, _, ev: &TextFieldEvent, window, cx| {
            match ev {
                TextFieldEvent::Changed => if let Some(m) = &mut w.font_menu { m.focused = None; m.scroll.scroll_to_item(0, ScrollStrategy::Top); },
                TextFieldEvent::Down | TextFieldEvent::Up => w.move_font_focus(*ev == TextFieldEvent::Down, cx),
                TextFieldEvent::Submit => w.pick_focused_font(window, cx),
                TextFieldEvent::Cancel => { w.close_font_menu(cx); window.focus(&w.focus, cx); }
                _ => {}
            }
            cx.notify();
        });
        let focus = field.focus_handle(cx);
        // Open on the family in use.
        let scroll = UniformListScrollHandle::new();
        let current = target.current();
        if let Some(i) = font_choices("", target, false, cx).iter().position(|c| matches!(c, Choice::Font { family, flat: false, .. } if family.eq_ignore_ascii_case(&current))) { scroll.scroll_to_item(i, ScrollStrategy::Center); }
        self.font_menu = Some(FontMenu { field, target, focused: None, others: false, previewing: None, preview_task: None, scroll, _sub: sub });
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Closes the font list without picking: a font shown for the moment
    /// gives way to the saved one again.
    pub(crate) fn close_font_menu(&mut self, cx: &mut Context<Self>) {
        let Some(m) = self.font_menu.take() else { return };
        if m.previewing.is_some() { self.show_font(m.target, None, cx); }
        cx.notify();
    }

    /// The page in `family` for now (None: the saved choice again), without
    /// saving it.
    fn show_font(&mut self, target: FontTarget, family: Option<&str>, cx: &mut Context<Self>) {
        let p = prefs();
        let family = family.unwrap_or(match target { FontTarget::Doc => &p.doc_family, FontTarget::Ui => &p.ui_family });
        crate::fonts::probe(family, cx);
        match target { FontTarget::Doc => themes::set_doc_font(family), FontTarget::Ui => theme::set_ui_family(family) }
        refresh_all(self, cx);
    }

    /// The pointer or the arrows on a row: the page takes that font after a
    /// short pause (walking the list doesn't relayout the note at every row);
    /// a download not here yet isn't previewed.
    fn preview_font(&mut self, key: String, row: usize, cx: &mut Context<Self>) {
        let Some(m) = &mut self.font_menu else { return };
        if m.focused == Some(row) && m.preview_task.is_some() { return }
        m.focused = Some(row);
        let target = m.target;
        m.preview_task = Some(cx.spawn(async move |w, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(140)).await;
            let _ = w.update(cx, |w, cx| {
                let Some(family) = key_family(&key, cx) else { return };
                let Some(m) = &mut w.font_menu else { return };
                if m.previewing.as_deref() == Some(family.as_str()) { return }
                m.previewing = Some(family.clone());
                w.show_font(target, Some(&family), cx);
            });
        }));
        cx.notify();
    }

    /// The font list under its button: a search field, then the rows
    /// (`font_choices`), each font drawing a sample in its own face.
    pub(crate) fn render_font_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.font_menu.as_ref()?;
        if self.view != View::Settings || self.settings_page != Page::Appearance { return None }
        let b = *self.font_anchors.borrow().get(menu.target.key())?;
        let target = menu.target;
        let current = target.current();
        let choices = std::rc::Rc::new(font_choices(&menu.field.read(cx).text, target, menu.others, cx));
        let n = choices.len();
        let focused = menu.focused;
        let list = if n == 0 {
            div().px(px(10.)).py(px(6.)).text_size(px(theme::ui_font_small())).line_height(px(theme::ui_font_small() + 5.))
                .text_color(Theme::text_caption()).child(tr("没有匹配的字体", "No matching font")).into_any_element()
        } else {
            uniform_list("font-list", n, cx.processor(move |_, range: std::ops::Range<usize>, _, cx| {
                range.map(|i| {
                    let g = SharedString::from(format!("font-{}-{i}", target.key()));
                    match &choices[i] {
                        // a virtual list's rows are all one height
                        Choice::Header(label) => MenuLabel::new(label.clone()).h(px(menu::ROW_H)).into_any_element(),
                        Choice::Others(count, open) => {
                            let open = *open;
                            div().w_full().flex().flex_col().child(MenuItem::new(g.clone()).icon(if open { "chevron-down" } else { "chevron-right" })
                                .child(div().flex_1().text_color(crate::design::color::muted_foreground()).child(crate::localized_format!("其他语言和符号（{}）", "Other scripts and symbols ({})", count)))
                                .on_click(cx.listener(move |w, _, _, cx| { if let Some(m) = &mut w.font_menu { m.others = !open; m.focused = None; } cx.notify(); }))).into_any_element()
                        }
                        Choice::Reset => {
                            let picked = current.is_empty();
                            // A virtual list's rows don't stretch on their own.
                            div().w_full().flex().flex_col().child(MenuItem::new(g.clone()).selected(focused == Some(i)).label(target.reset_label()).checked(picked)
                                .on_hover(cx.listener(move |w, hovered: &bool, _, cx| if *hovered { w.preview_font(String::new(), i, cx) }))
                                .on_click(cx.listener(move |w, _, window, cx| w.pick_font(String::new(), target, window, cx)))).into_any_element()
                        }
                        c @ Choice::Font { family, dl, flat } => font_row(family, *dl, *flat, g, focused == Some(i), &current, (c.key().unwrap_or_default(), i, target), cx).into_any_element(),
                    }
                }).collect()
            }))
            .track_scroll(&menu.scroll)
            .h(px((n as f32 * menu::ROW_H).min(420.)))
            .into_any_element()
        };
        let anchors = self.font_anchors.clone();
        let key = target.key();
        let popup = Popover::new("font-menu").flex()
            .on_mouse_down_out(cx.listener(move |w, ev: &MouseDownEvent, window, cx| {
                // A press on the button itself: its click closes the list.
                if anchors.borrow().get(key).is_some_and(|b| b.contains(&ev.position)) { return }
                w.close_font_menu(cx);
                window.focus(&w.focus, cx);
            }))
            .child(div().w(px(460.)).flex().flex_col()
                .child(div().p(px(menu::PAD))
                    .child(Field::new("font-search").sized(FieldSize::Sm)
                        .child(crate::design::icon("search", 14., crate::design::color::muted_foreground()))
                        .child(div().flex_1().min_w_0().child(menu.field.clone()))))
                .child(div().pb(px(menu::PAD)).child(list)));
        Some(deferred({ let (pos, corner) = crate::design::hang(b, crate::design::Align::End, None); anchored().anchor(corner).position(pos) }.snap_to_window_with_margin(px(8.)).child(popup)).with_priority(2).into_any_element())
    }

    /// A row picked (its key): saved, and kept in 最近使用.
    fn pick_font(&mut self, family: String, target: FontTarget, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = family.strip_prefix(DOWNLOAD) { return self.pick_download(id, window, cx) }
        let previewed = self.font_menu.take().is_some_and(|m| m.previewing.is_some());
        window.focus(&self.focus, cx);
        let recent = family.clone();
        self.set_pref(move |p| {
            let list = match target { FontTarget::Doc => &mut p.recent_doc_fonts, FontTarget::Ui => &mut p.recent_ui_fonts };
            if !recent.is_empty() {
                list.retain(|f| !f.eq_ignore_ascii_case(&recent));
                list.insert(0, recent.clone());
                list.truncate(5);
            }
            match target {
                FontTarget::Doc => p.doc_family = family,
                FontTarget::Ui => p.ui_family = family,
            }
        }, cx);
        // a preview of another family than the one saved now
        if previewed { self.show_font(target, None, cx); }
    }

    /// A row's star: in or out of 收藏 (both lists share it).
    fn toggle_favorite(&mut self, family: &str, cx: &mut Context<Self>) {
        let family = family.to_string();
        self.set_pref(move |p| {
            if p.font_favorites.iter().any(|f| f.eq_ignore_ascii_case(&family)) { p.font_favorites.retain(|f| !f.eq_ignore_ascii_case(&family)); }
            else { p.font_favorites.insert(0, family); }
        }, cx);
    }

    /// ↑ / ↓ in the font list: from the row in focus (first press: the
    /// family in use) to the next row that is a choice; the page previews it.
    fn move_font_focus(&mut self, down: bool, cx: &mut Context<Self>) {
        let Some(menu) = &self.font_menu else { return };
        let choices = font_choices(&menu.field.read(cx).text, menu.target, menu.others, cx);
        let rows: Vec<(usize, String)> = choices.iter().enumerate().filter_map(|(i, c)| c.key().map(|k| (i, k))).collect();
        if rows.is_empty() { return }
        let current = menu.target.current();
        let at = match menu.focused {
            Some(r) => rows.iter().position(|(i, _)| *i == r),
            None => rows.iter().position(|(i, k)| !k.is_empty() && k.eq_ignore_ascii_case(&current) && matches!(choices[*i], Choice::Font { flat: false, .. })),
        };
        let next = match (at, down) {
            (None, true) => 0,
            (None, false) => rows.len() - 1,
            (Some(i), true) => (i + 1).min(rows.len() - 1),
            (Some(i), false) => i.saturating_sub(1),
        };
        let (row, key) = rows[next].clone();
        if let Some(m) = &mut self.font_menu { m.scroll.scroll_to_item(row, ScrollStrategy::Nearest); }
        self.preview_font(key, row, cx);
    }

    /// ↩ in the font list: the row in focus, else the first choice.
    fn pick_focused_font(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = self.font_menu.as_ref().and_then(|m| {
            let choices = font_choices(&m.field.read(cx).text, m.target, m.others, cx);
            let key = match m.focused { Some(r) => choices.get(r).and_then(Choice::key), None => choices.iter().find_map(Choice::key) };
            key.map(|k| (k, m.target))
        });
        if let Some((key, target)) = picked { self.pick_font(key, target, window, cx); }
    }

    /// A download row picked: a font this run has (or the system's own copy)
    /// becomes the document font at once; else it is read from the cache or
    /// downloaded, the list staying open on its progress, and becomes the
    /// document font when it arrives (`font_changed`).
    fn pick_download(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        use crate::font_downloads as fd;
        let Some(e) = fd::entry(id) else { return };
        if fd::fonts(id).is_some() && fd::register(id, cx.text_system()) || crate::fonts::installed(&e.family, cx) && !fd::registered(id) {
            crate::fonts::added(&e.family);
            let family = e.family.clone();
            return self.pick_font(family, FontTarget::Doc, window, cx);
        }
        *PENDING_DOC.lock().unwrap() = Some(id.to_string());
        fd::want(id, if fd::on_disk(id) { fd::Mode::Cache } else { fd::Mode::Now });
        cx.notify();
    }

    /// The trash button of a downloaded font: its files go; the document
    /// font goes back to 跟随主题 if it was this one. Xiaolai stays loaded
    /// for the boards open now and is fetched again when a board next needs it.
    fn delete_download(&mut self, id: &str, cx: &mut Context<Self>) {
        use crate::font_downloads as fd;
        let Some(e) = fd::entry(id) else { return };
        if let Err(err) = fd::delete(id) {
            return self.show_toast_kind(ui::ToastKind::Error, &format!("{}: {err}", tr("删除失败", "Could not delete")), cx);
        }
        if prefs().doc_family.eq_ignore_ascii_case(&e.family) { self.set_pref(|p| p.doc_family.clear(), cx); }
        let msg = if id == fd::XIAOLAI { tr("已删除。画板需要中文手写字体时会重新下载。", "Deleted. Boards download the handwriting font again when they need it.").to_string() }
            else { crate::localized_format!("已删除 {}", "Deleted {}", e.name()) };
        self.show_toast_kind(ui::ToastKind::Success, &msg, cx);
    }

    /// Keys while a 通用 dropdown is open; true when used.
    pub(crate) fn pref_menu_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let Some(m) = &mut self.pref_menu else { return false };
        match key {
            "escape" => self.pref_menu = None,
            "up" => m.focused = m.focused.saturating_sub(1),
            "down" => m.focused = (m.focused + 1).min(m.labels.len().saturating_sub(1)),
            "enter" | "space" => { let i = m.focused; self.pick_pref(i, cx); return true }
            _ => return false,
        }
        cx.notify();
        true
    }

    /// 界面语言's dropdown, each language in its own name.
    pub(crate) fn render_lang_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let focused = self.lang_menu?;
        if self.view != View::Settings || self.settings_page != Page::General { return None }
        let b = self.lang_anchor.get()?;
        let current = prefs().lang();
        let rows = Lang::ALL.map(|l| {
            let g = SharedString::from(format!("lang-opt-{}", l.code()));
            MenuItem::new(g).label(l.name()).selected(l == focused).checked(l == current)
                .on_hover(cx.listener(move |w, hovered: &bool, _, cx| if *hovered && w.lang_menu != Some(l) { w.lang_menu = Some(l); cx.notify(); }))
                .on_click(cx.listener(move |w, _, _, cx| { w.lang_menu = None; w.set_pref(|p| p.set_lang(l), cx) }))
        });
        let anchor = self.lang_anchor.clone();
        let menu = Popover::new("lang-menu").menu().w(px(160.))
            .on_mouse_down_out(cx.listener(move |w, ev: &MouseDownEvent, _, cx| {
                if anchor.get().is_some_and(|b| b.contains(&ev.position)) { return }
                w.lang_menu = None;
                cx.notify();
            }))
            .children(rows);
        { let (pos, corner) = crate::design::hang(b, crate::design::Align::End, None); Some(deferred(anchored().anchor(corner).position(pos).snap_to_window_with_margin(px(8.)).child(menu.fit(crate::design::room(pos, corner)))).with_priority(2).into_any_element()) }
    }

    /// Keys while the language dropdown is open; true when used.
    pub(crate) fn lang_menu_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let at = self.lang_menu.map_or(0, |l| l as usize);
        match key {
            "escape" => self.lang_menu = None,
            "up" => self.lang_menu = Some(Lang::ALL[at.saturating_sub(1)]),
            "down" => self.lang_menu = Some(Lang::ALL[(at + 1).min(Lang::ALL.len() - 1)]),
            "enter" | "space" => { self.lang_menu = None; self.set_pref(|p| p.set_lang(Lang::ALL[at]), cx); return true }
            _ => return false,
        }
        cx.notify();
        true
    }

    /// Keys while the theme dropdown is open; true when used.
    pub(crate) fn theme_menu_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let ids: Vec<String> = themes::themes().iter().map(|t| t.id.clone()).collect();
        let at = self.theme_menu.as_ref().and_then(|p| ids.iter().position(|id| id == p)).unwrap_or(0);
        match key {
            "escape" => self.theme_menu = None,
            "up" => self.theme_menu = Some(ids[at.saturating_sub(1)].clone()),
            "down" => self.theme_menu = Some(ids[(at + 1).min(ids.len().saturating_sub(1))].clone()),
            "enter" | "space" => { apply_theme(self, ids[at].clone(), cx); return true }
            _ => return false,
        }
        cx.notify();
        true
    }
}

impl Workspace {
    /// Change preferences and refresh what depends on them.
    /// 正文字号's stepper moved to `pref`: the note and the sample redraw at once; the preferences and
    /// the folder's look are written when the steps stop for half a second, not at every step.
    pub(crate) fn set_doc_size_soon(&mut self, pref: String, cx: &mut Context<Self>) {
        let mut p = prefs();
        p.doc_size = pref.clone();
        apply(p);
        refresh_all(self, cx);
        cx.notify();
        let turn = DOC_SIZE_GEN.with(|g| { g.set(g.get() + 1); g.get() });
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(500)).await;
            let _ = this.update(cx, |w, cx| if DOC_SIZE_GEN.with(|g| g.get()) == turn { w.set_pref(|p| p.doc_size = pref, cx) });
        }).detach();
    }

    pub(crate) fn set_pref(&mut self, f: impl FnOnce(&mut Preferences), cx: &mut Context<Self>) {
        let old = prefs();
        let mut p = old.clone();
        f(&mut p);
        if let Err(e) = save(&p) {
            self.show_toast_kind(ui::ToastKind::Error, &format!("{}: {e}", tr("设置保存失败", "Could not save settings")), cx);
            return;
        }
        if p.quick_hotkey != old.quick_hotkey {
            if p.quick_hotkey { crate::quick_note::register(cx); } else { crate::quick_note::suspend_hotkey(); }
        }
        if p.appearance != old.appearance { apply_appearance(&p.appearance); }
        if p.doc_family != old.doc_family { crate::fonts::probe(&p.doc_family, cx); }
        if p.ui_family != old.ui_family { crate::fonts::probe(&p.ui_family, cx); }
        if p.theme_id() != old.theme_id() || p.lang() != old.lang() || p.doc_family != old.doc_family || p.doc_size != old.doc_size || p.ui_family != old.ui_family || sync_dark(cx) {
            refresh_all(self, cx);
        }
        cx.notify();
    }

    pub(crate) fn reload_themes(&mut self, cx: &mut Context<Self>) {
        themes::load(&prefs().theme_id(), themes::all_loaded(), || cx.text_system().all_font_names());
        refresh_all(self, cx);
    }

    /// Open the theme's stylesheet in the user's editor; a built-in theme is
    /// first copied in full into the themes folder (the copy replaces it).
    fn edit_theme(&mut self, id: &str, cx: &mut Context<Self>) {
        match themes::editable_file(id) {
            Ok(p) => {
                self.reload_themes(cx);
                open_in_editor(&p, cx);
            }
            Err(e) => self.show_toast_kind(ui::ToastKind::Error, &e.to_string(), cx),
        }
    }

    fn duplicate_theme(&mut self, id: &str, cx: &mut Context<Self>) {
        match themes::duplicate(id) {
            Ok((new_id, p)) => {
                themes::load(&new_id, themes::all_loaded(), || cx.text_system().all_font_names());
                let feishu = themes::themes().iter().find(|t| t.id == new_id).is_some_and(|t| t.style.feishu);
                self.set_pref(move |pr| { pr.theme = new_id; pr.feishu = feishu; }, cx);
                open_in_editor(&p, cx);
            }
            Err(e) => self.show_toast_kind(ui::ToastKind::Error, &e.to_string(), cx),
        }
    }

    fn remove_theme(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Err(e) = themes::remove(id) { return self.show_toast_kind(ui::ToastKind::Error, &e.to_string(), cx) }
        self.reload_themes(cx);
        // A deleted theme of the user's own: back to Craft.
        if !themes::themes().iter().any(|t| t.id == id) { self.set_pref(|p| { p.theme = "craft".into(); p.feishu = false; }, cx); }
    }

    pub(crate) fn open_settings(&mut self, page: Page, cx: &mut Context<Self>) {
        self.cancel_shortcut_recording(cx);
        self.settings_page = page;
        self.theme_menu = None;
        self.lang_menu = None;
        self.pref_menu = None;
        self.settings_scroll.set_offset(point(px(0.), px(0.)));
        self.shortcut_error = None;
        if self.view != View::Settings { self.show_view(View::Settings, cx); }
        cx.notify();
    }

    pub(crate) fn render_settings_sidebar(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        crate::devtools::rows_begin(window);
        let current = (self.view == View::Settings).then_some(self.settings_page);
        let in_trash = self.view == View::Trash;
        let nav_row = |page: Option<Page>, cx: &mut Context<Self>| {
            let on = if page.is_some() { page == current } else { in_trash };
            let (title, icon) = page.map_or((tr("回收站", "Trash"), "bin"), |p| (p.title(), p.icon()));
            div().id(SharedString::from(format!("settings-nav-{page:?}"))).h(px(sb::ROW_H)).flex().items_center().pl(px(6.)).pr(px(4.)).rounded(px(sb::ROW_R))
                .relative().children(crate::devtools::row_probe(title))
                .text_size(px(theme::ui_font())).line_height(px(theme::ui_font() + 7.)).cursor_pointer()
                .when(on, |d| d.bg(Theme::sidebar_selected()).text_color(Theme::text()))
                .when(!on, |d| d.text_color(Theme::sidebar_text()).hover(|s| s.bg(Theme::sidebar_hover())))
                .child(div().size(px(20.)).flex_none().flex().items_center().justify_center().mr(px(9.)).child(ui::icon(icon, sb::ROW_ICON, Theme::icon())))
                .child(title)
                .on_click(cx.listener(move |w, _, _, cx| match page { Some(p) => w.open_settings(p, cx), None => w.show_view(View::Trash, cx) }))
        };
        div().w(px(self.sidebar_w(window))).h_full().flex_none().flex().flex_col().text_color(Theme::text()).font_family(theme::ui_family())
            .child(div().h(px(crate::app::HEADER_H)).flex_none().flex().items_center().pl(px(theme::TRAFFIC_LIGHTS_W)).pr(px(12.)).title_bar().child(self.header_navigation(cx))
                .child(div().flex_1()).children(update_pill(cx)))
            // The document sidebar's rhythm: 主页 sits 8 under the bar, a section
            // header like 我的文档's, rows `ROW_GAP` apart. 设置's line sits on
            // 主页's, not a section header's: the 34pt box keeps the rows below.
            .child(div().pt(px(8.)).px(px(sb::SB_X)).flex().flex_col()
                .child(div().h(px(sb::SECTION_H)).pl(px(4.)).pt(px((sb::SECTION_TOP + sb::SECTION_H) / 2. - sb::ROW_H / 2.))
                    .child(div().px(px(4.)).text_size(px(theme::ui_font())).line_height(px(theme::ui_font() + 7.)).font_weight(FontWeight::SEMIBOLD).text_color(Theme::sidebar_section()).child(tr("设置", "Settings"))))
                .child(div().flex().flex_col().gap(px(sb::ROW_GAP)).children(NAV.iter().map(|p| nav_row(*p, cx)).collect::<Vec<_>>())))
            // Where 设置 was: 返回, back to the page 设置 was opened from.
            .child(div().flex_1())
            .child(sb::footer_slot(sb::footer_row("ft-back", "arrow-left", tr("返回", "Back")).tip(tr("返回", "Back"), Some("Esc"))
                .on_click(cx.listener(|w, _, _, cx| w.close_settings(cx)))))
            .into_any_element()
    }

    /// 收起 / 展开侧栏 and 返回, in the very box the document sidebar's
    /// 收起侧栏 uses (28pt, 4pt past the traffic lights, glyph 17), so the
    /// buttons don't move when switching between the document list and 设置.
    pub(crate) fn header_navigation(&self, cx: &mut Context<Self>) -> AnyElement {
        let open = self.sidebar_open;
        let bar_button = |id: &'static str, glyph: &'static str| Button::ghost(id).icon(glyph).no_drag();
        div().flex().items_center().ml(px(4.)).gap(px(4.))
            .child(bar_button("settings-collapse", if open { "panel-left-close" } else { "panel-left-open" })
                .tip(if open { tr("收起侧栏", "Hide sidebar") } else { tr("展开侧栏", "Show sidebar") }, Some(crate::shortcuts::hint("workspace::ToggleSidebar", "⌘ + ]")))
                .on_click(cx.listener(|w, _, _, cx| { w.sidebar_open = !w.sidebar_open; cx.notify(); })))
            // 返回, where navigation goes back (HIG): to the page 设置 was opened from, as the footer's 返回 does.
            .child(bar_button("settings-back", "arrow-left").tip(tr("返回", "Back"), Some("Esc"))
                .on_click(cx.listener(|w, _, _, cx| w.close_settings(cx))))
            .into_any_element()
    }

    /// Back to where 设置 was opened from: the document, a folder's page or
    /// 主页 (主页 when what it was is gone).
    pub(crate) fn close_settings(&mut self, cx: &mut Context<Self>) {
        let back = match self.settings_from {
            View::Doc if self.editor.is_some() && self.current.is_some() => View::Doc,
            View::Folder if self.folder_current.is_some() => View::Folder,
            _ => View::Home,
        };
        self.show_view(back, cx);
    }

    pub(crate) fn render_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let page = self.settings_page;
        let body = match page {
            Page::General => self.general_page(cx),
            Page::Appearance => self.appearance_page(cx),
            Page::Shortcuts => self.shortcut_settings(cx),
            Page::Library => self.library_page(cx),
            Page::About => self.about_page(cx),
        };
        div().flex_1().min_w_0().h_full().flex().flex_col().bg(Theme::bg_body()).text_color(Theme::text()).font_family(theme::ui_family())
            // The bar only drags the window; the page title heads the centred column.
            .child(div().h(px(crate::app::HEADER_H)).flex_none().flex().items_center().title_bar()
                .when(!self.sidebar_open, |d| d.pl(px(theme::TRAFFIC_LIGHTS_W)).child(self.header_navigation(cx))))
            .child(div().id("settings-content").flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.settings_scroll).child(crate::design::scroll::bar("settings-content", &self.settings_scroll, 0.))
                .child(div().w_full().flex().flex_col().items_center().px(px(32.)).pb(px(64.))
                    .child(div().w_full().max_w(px(if page == Page::Shortcuts { 760. } else { COLUMN_W })).flex().flex_col().gap(px(28.))
                        .child(div().flex().items_center().gap(px(16.))
                            .child(div().flex_1().min_w_0().px(px(4.)).text_size(px(22.)).line_height(px(28.)).font_weight(FontWeight::SEMIBOLD).child(page.title()))
                            .when(page == Page::Shortcuts, |d| d.child(self.shortcut_actions(cx))))
                        .child(body))))
            .into_any_element()
    }

    fn general_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = prefs();
        let new_key = crate::shortcuts::hint("workspace::NewDoc", "⌘N");
        // 打开链接: which browser a clicked link (and 网页搜索) opens in. The
        // list is what this Mac has, so the row is only there when there is a
        // choice to make.
        let browsers = browsers();
        let links = if browsers.is_empty() {
            vec![]
        } else {
            let sel = browsers.iter().position(|b| *b == p.browser).map_or(0, |i| i + 1);
            let labels: Vec<&'static str> = std::iter::once(tr("系统默认", "Default")).chain(browsers.iter().copied()).collect();
            vec![row(tr("打开链接", "Open links"), desc(tr("点击链接时用哪个浏览器打开；网页搜索也用它。", "Which browser a link opens in; “Web search” uses it too.")), {
                self.select("browser", labels, sel, |w, i, cx| {
                    let app = crate::settings::browser_at(i);
                    w.set_pref(|p| p.browser = app.clone(), cx)
                }, cx)
            }).into_any_element()]
        };
        div().w_full().flex().flex_col().gap(px(28.))
            .child(group("group-general", None, vec![
                row(tr("界面语言", "Interface language"), None, {
                    let anchor = self.lang_anchor.clone();
                    let current = p.lang();
                    popup_button("lang-picker", current.name(), self.lang_menu.is_some())
                        .child(canvas(move |b, _, _| anchor.set(Some(b)), |_, _, _, _| {}).absolute().inset_0())
                        .relative()
                        .on_click(cx.listener(move |w, _, _, cx| { w.pref_menu = None; w.lang_menu = if w.lang_menu.is_some() { None } else { Some(current) }; cx.notify(); }))
                }).into_any_element(),
                row(tr("启动时打开", "On launch"), desc(tr("上次阅读的文档，或主页。", "The document read last, or the home page.")),
                    segmented("pref-start", &[tr("上次文档", "Last document"), tr("主页", "Home page")], p.start_home as usize,
                        |w, i, cx| w.set_pref(|p| p.start_home = i == 1, cx), cx)).into_any_element(),
                row(tr("自动下载网络图片", "Download web images"), desc(tr("关闭后不联网，网络图片显示为占位，点击单独加载。", "When off the app stays offline; web images show a placeholder you can click to load.")),
                    switch("pref-remote", p.remote_images).on_click(cx.listener(|w, _, _, cx| w.set_pref(|p| p.remote_images = !p.remote_images, cx)))).into_any_element(),
                row(tr("自动检查更新", "Check for updates automatically"), desc(tr("只读取一份版本信息，不发送任何数据；新版本由你决定何时下载安装。", "Reads a version file and sends nothing about you; you decide when to download and install.")),
                    switch("pref-update", p.auto_update).on_click(cx.listener(|w, _, _, cx| w.set_pref(|p| p.auto_update = !p.auto_update, cx)))).into_any_element(),
                row(tr("退格键回到标题", "Backspace returns to title"), desc(tr("在正文开头按退格键时回到标题，第一段文字并入标题。关闭后光标留在正文。", "Backspace at the start of the body moves to the title, joining the first paragraph to it. When off the caret stays in the body.")),
                    switch("pref-backspace-title", p.backspace_to_title).on_click(cx.listener(|w, _, _, cx| w.set_pref(|p| p.backspace_to_title = !p.backspace_to_title, cx)))).into_any_element(),
                row(tr("中英文之间自动加空格", "Space between Chinese and English"), desc(tr("输入时在汉字和英文字母、数字之间自动加一个空格，代码、链接和公式里不加。已有的文字可用「编辑 › 整理本文空格」。", "While typing, adds a space between Chinese characters and Latin letters or digits, except in code, links and formulas. For existing text use Edit › Add Spaces Between Chinese and English.")),
                    switch("pref-cjk-spacing", p.cjk_spacing).on_click(cx.listener(|w, _, _, cx| w.set_pref(|p| p.cjk_spacing = !p.cjk_spacing, cx)))).into_any_element(),
                // The key as bound (Ctrl+N off macOS, or the user's own), in every language's text.
                row(tr("快捷新建默认类型", "Quick new (⌘N)").replace("⌘N", new_key), Some(tr("⌘N 新建时创建的类型；「记住上次」沿用你上次从新建菜单选的类型。", "What ⌘N creates; “Last used” repeats the type you last picked from the New menu.").replace("⌘N", new_key).into()), {
                    let sel = match p.new_default.as_str() { "doc" => 1, "markdown" => 2, _ => 0 };
                    self.select("quick-new", vec![tr("记住上次", "Last used"), tr("文档", "Document"), "Markdown"], sel,
                        |w, i, cx| w.set_pref(|p| p.new_default = ["", "doc", "markdown"][i].into(), cx), cx)
                }).into_any_element(),
                // 网页搜索 opens the selection in the browser; which site is the
                // user's habit, so it is a preference rather than a constant.
                row(tr("网页搜索", "Web search"), desc(tr("选中文字后「网页搜索」用哪个网站。", "Which site “Web search” opens the selected text in.")), {
                    let sel = match p.search_engine.as_str() { "baidu" => 1, "bing" => 2, "duckduckgo" => 3, _ => 0 };
                    self.select("search-engine", vec!["Google", tr("百度", "Baidu"), "Bing", "DuckDuckGo"], sel,
                        |w, i, cx| w.set_pref(|p| p.search_engine = ["", "baidu", "bing", "duckduckgo"][i].into(), cx), cx)
                }).into_any_element(),
            ].into_iter().chain(links).collect()))
            .into_any_element()
    }

    /// 外观's two font rows (界面字体, 文档字体): the family in use and a list of
    /// every family the system has. `about` describes the row; a family that
    /// was uninstalled since it was picked says so instead.
    fn font_row(&self, target: FontTarget, title: &'static str, about: &'static str, cx: &mut Context<Self>) -> Div {
        let family = target.current();
        let description = if family.is_empty() || crate::fonts::installed(&family, cx) {
            desc(about)
        } else if let Some(e) = crate::font_downloads::by_family(&family) {
            Some(if crate::font_downloads::pending(&e.id) { tr("正在载入这个字体…", "Loading that font…").into() }
                else { tr("这个字体还没下载（或已被删除），暂时用默认字体显示。打开列表可以重新下载。", "That font isn't downloaded (or was deleted); the default face stands in. Open the list to download it again.").into() })
        } else {
            Some(tr("这个字体没有安装，暂时用默认字体显示。", "That font isn't installed; the default face stands in for now.").into())
        };
        row(title, description, {
            let anchors = self.font_anchors.clone();
            let key = target.key();
            let label: SharedString = if family.is_empty() { target.reset_label().into() }
                else { crate::font_downloads::by_family(&family).map_or_else(|| family.clone(), |e| e.name().to_string()).into() };
            let open = self.font_menu.as_ref().is_some_and(|m| m.target == target);
            popup_button(format!("{}-font-picker", target.key()), label, open)
                .child(canvas(move |b, _, _| { anchors.borrow_mut().insert(key, b); }, |_, _, _, _| {}).absolute().inset_0())
                .children(crate::devtools::anchor_probe(format!("{}-font-picker", target.key())))
                .relative()
                .on_click(cx.listener(move |w, _, window, cx| w.toggle_font_menu(target, window, cx)))
        })
    }

    fn appearance_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = prefs().theme_id();
        let list = themes::themes();
        let sel = list.iter().find(|t| t.id == current).or_else(|| list.first());
        let (sel_id, sel_name, sel_path, customized, own, warnings) = match sel {
            Some(t) => (t.id.clone(), t.name().to_string(), t.path.clone(), t.customized(), t.bundled.is_none(), t.warnings.clone()),
            None => ("craft".into(), "Craft".into(), None, false, false, vec![]),
        };
        drop(list);
        let errors = themes::errors();
        let where_: SharedString = match &sel_path {
            // Inside the library: the short path (the full one wraps on narrow windows).
            Some(p) => p.strip_prefix(crate::store::Store::active_dir()).unwrap_or(p).display().to_string().into(),
            None => tr("内置主题", "Built in").into(),
        };
        let (edit_id, dup_id, rm_id, open_id) = (sel_id.clone(), sel_id.clone(), sel_id.clone(), sel_id);
        let anchor = self.theme_anchor.clone();
        let picker = popup_button("theme-picker", sel_name, self.theme_menu.is_some())
            .child(canvas(move |b, _, _| anchor.set(Some(b)), |_, _, _, _| {}).absolute().inset_0())
            .relative()
            .on_click(cx.listener(move |w, _, _, cx| {
                if w.theme_menu.is_none() && !themes::all_loaded() {
                    themes::load(&prefs().theme_id(), true, || cx.text_system().all_font_names());
                }
                w.theme_menu = if w.theme_menu.is_some() { None } else { Some(open_id.clone()) };
                cx.notify();
            }));
        // One group, top to bottom: the choice, how it looks, its stylesheet.
        // The preview is a page of the theme on a quiet band, like a sheet on a desk.
        // The page itself, the group's last row: no frame around it.
        let shown = { let list = themes::themes(); list.iter().find(|t| t.id == current).or_else(|| list.first()).map(|t| sample_page(t, PAGE_S)) };
        let sample = div().w_full().h(px(640.)).children(shown);
        div().w_full().flex().flex_col().gap(px(28.))
            .child(div().w_full().flex().flex_col().gap(px(8.))
                .child(group("group-appearance", None, vec![
                    row(tr("外观", "Appearance"), None, {
                        let at = match prefs().appearance.as_str() { "light" => 1, "dark" => 2, _ => 0 };
                        segmented("appearance", &[tr("跟随系统", "System"), tr("浅色", "Light"), tr("深色", "Dark")], at,
                            |w, i, cx| w.set_pref(|p| p.appearance = ["", "light", "dark"][i].into(), cx), cx)
                    }).into_any_element(),
                    row(tr("动画", "Animation"), Some(tr("克制：切换即时，只有拖拽带动画。丰富：展开收起、选中和悬停也带动画。", "Low: switching is instant, only dragging animates. High: folding, selection and hover animate too.").into()),
                        segmented("motion", &[tr("克制", "Low"), tr("丰富", "High")], usize::from(prefs().motion == "high"),
                            |w, i, cx| w.set_pref(|p| p.motion = ["", "high"][i].into(), cx), cx)).into_any_element(),
                    // 窗口背景: the sidebar sits on the desktop material by
                    // default; a busy wallpaper behind it can be distracting.
                    row(tr("窗口背景", "Window background"), Some(tr("默认让侧栏透出桌面的毛玻璃。开启后整个窗口不透明，背景里动的东西不会透进来。", "By default the sidebar shows the desktop through a frosted surface. Turn this on for an opaque window.").into()),
                        switch("pref-opaque", prefs().opaque_window).on_click(cx.listener(|w, _, window, cx| {
                            w.set_pref(|p| p.opaque_window = !p.opaque_window, cx);
                            window.refresh();
                        }))).into_any_element(),
                ]))
                // 界面, then 文档: each font before its size, and the document theme last, right above
                // the sample it shapes (the user's order, 2026-11-25)
                .child(group("group-interface", Some(tr("界面", "Interface")), vec![
                    // 界面字体: the chrome — sidebar, menus, this page.
                    self.font_row(FontTarget::Ui, tr("界面字体", "Interface font"),
                        tr("侧栏、菜单和设置页用这个字体；文档用下面的「文档字体」。", "The sidebar, menus and settings use it; the note has its own below."), cx).into_any_element(),
                    // 界面字号: the sidebar and this page together, so the pairs
                    // (label, description) stay in proportion.
                    row(tr("界面字号", "Interface size"), None, {
                        let sel = match prefs().ui_font.as_str() { "small" => 0, "large" => 2, _ => 1 };
                        segmented("ui-font", &[tr("小", "Small"), tr("标准", "Default"), tr("大", "Large")], sel,
                            |w, i, cx| w.set_pref(|p| p.ui_font = ["small", "", "large"][i].into(), cx), cx)
                    }).into_any_element(),
                ]))
                .child(group("group-document", Some(tr("文档", "Document")), vec![
                    // 文档字体: every family the system has, for the note's
                    // body text and headings (code keeps the theme's).
                    self.font_row(FontTarget::Doc, tr("文档字体", "Document font"),
                        tr("正文和标题用这个字体，代码块仍用主题的等宽字体。", "Body text and headings use it; code keeps the theme's monospace face."), cx).into_any_element(),
                    // 正文字号: the note's text, headings and code together (themes::doc_len).
                    // Presets and 自定义: a custom size (12–24pt) steps by 1 under the presets; a size that
                    // is a preset's lights that preset unless 自定义 was picked this time.
                    row(tr("正文字号", "Text size"), None, {
                        let body = themes::active().num("page.size");
                        let pref = prefs().doc_size.clone();
                        let pt = themes::doc_size_pt(&pref, body);
                        let preset = themes::DOC_PRESETS.iter().position(|n| themes::doc_size_pt(n, body) == pt);
                        let custom = DOC_CUSTOM.with(|c| c.get()) || preset.is_none();
                        let sel = if custom { 4 } else { preset.unwrap_or(1) };
                        let step = move |d: f32| move |w: &mut Workspace, _: &ClickEvent, _: &mut Window, cx: &mut Context<Workspace>| {
                            let next = (pt + d).clamp(themes::DOC_MIN, themes::DOC_MAX);
                            if next != pt { w.set_doc_size_soon(format!("{next}"), cx) }
                        };
                        div().flex_none().flex().flex_col().items_end().gap(px(10.))
                            .child(segmented("doc-size", &[tr("小", "Small"), tr("标准", "Default"), tr("大", "Large"), tr("特大", "Extra large"), tr("自定义", "Custom")], sel,
                                move |w, i, cx| {
                                    DOC_CUSTOM.with(|c| c.set(i == 4));
                                    // 自定义 starts at the size in force; a preset brings its own
                                    w.set_pref(|p| p.doc_size = if i == 4 { format!("{pt}") } else { themes::DOC_PRESETS[i].into() }, cx)
                                }, cx))
                            .when(custom, |d| d.child(div().flex_none().flex().items_center().gap(px(12.))
                                .child(button("doc-size-minus", "−").on_click(cx.listener(step(-1.))))
                                .child(div().w(px(56.)).flex().items_center().justify_center().text_size(px(theme::ui_font())).text_color(Theme::text()).child(format!("{pt} pt")))
                                .child(button("doc-size-plus", "+").on_click(cx.listener(step(1.))))))
                    }).into_any_element(),
                    row(tr("文档主题", "Document theme"), None, picker).into_any_element(),
                    sample.into_any_element(),
                ].into_iter().chain(THEME_CSS.then(|| {
                    row(tr("主题 CSS", "Theme CSS"), Some(where_),
                        div().flex().items_center().gap(px(8.))
                            .when(customized || own, |d| d.child(link_button("theme-remove", if customized { tr("恢复默认", "Restore") } else { tr("删除", "Delete") })
                                .on_click(cx.listener(move |w, _, _, cx| w.remove_theme(&rm_id, cx)))))
                            .child(button("theme-duplicate", tr("复制为新主题", "Duplicate")).on_click(cx.listener(move |w, _, _, cx| w.duplicate_theme(&dup_id, cx))))
                            .child(button("theme-edit", tr("编辑 CSS", "Edit CSS")).on_click(cx.listener(move |w, _, _, cx| w.edit_theme(&edit_id, cx))))).into_any_element()
                })).collect()))
                .children(errors.into_iter().chain(warnings.into_iter().take(8).map(|w| format!("{}：{w}", tr("未生效", "Ignored")))).map(|e| div().px(px(4.)).text_size(px(theme::ui_font_small())).line_height(px(theme::ui_font_small() + 6.)).text_color(Theme::danger()).child(e)))
                )
            .into_any_element()
    }

    /// 版本历史: the retention count is stored in the library, so every
    /// window sees it; the cached copy only avoids reading a file per frame.
    pub(crate) fn change_history_limit(&mut self, delta: i32, window: &mut Window, cx: &mut Context<Self>) {
        let next = (self.store.history_limit() as i32 + delta).clamp(1, 30) as u32;
        if self.store.set_history_limit(next).is_err() { return }
        self.history_limit = next;
        // Other windows cache the number for their settings page; saving
        // always reads the library, so this is display-only.
        for handle in cx.windows().into_iter()
            .filter(|w| w.window_id() != window.window_handle().window_id())
            .filter_map(|w| w.downcast::<Workspace>()) {
            let _ = handle.update(cx, |ws, _, cx| { ws.history_limit = next; cx.notify() });
        }
        cx.notify();
    }

    fn library_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let reveal = crate::local_folders::show_in_files();
        let short = |root: &std::path::Path| if cfg!(target_os = "windows") { root.display().to_string() } else {
            dirs::home_dir().and_then(|h| root.strip_prefix(h).ok().map(|r| format!("~/{}", r.display()))).unwrap_or_else(|| root.display().to_string())
        };
        // 主页: every top-level folder alike, each with its own 回收站 and history.
        let mut folders: Vec<AnyElement> = self.store.top_folders().into_iter().map(|(key, name, path, error)| {
            let (k, n, p) = (key.clone(), name.clone(), path.clone());
            let sub: SharedString = match error { Some(e) => format!("{} · {}", short(&path), diagnostic(&e)).into(), None => short(&path).into() };
            row(name, Some(sub), div().flex_none().flex().gap(px(8.))
                .child(button(SharedString::from(format!("reveal-{key}")), reveal).on_click(move |_, _, cx| cx.reveal_path(&p)))
                // A test run's one top-level folder (`QUIET_MD_HOME`) is fixed.
                .when(std::env::var_os("QUIET_MD_HOME").is_none() || std::env::var_os("QUIET_MD_FOLDERS").is_some(), |d| d.child(button(SharedString::from(format!("remove-{key}")), tr("移除", "Remove")).on_click(cx.listener(move |w, _, _, cx| { w.confirm = Some(crate::app::Confirm::RemoveFolder(k.clone(), n.clone())); cx.notify(); }))))).into_any_element()
        }).collect();
        // 快速笔记保存到 (2.8): a choice once there is one to make.
        let open: Vec<(String, String)> = self.store.top_folders().into_iter().filter(|r| r.3.is_none()).map(|r| (r.0, r.1)).collect();
        if open.len() > 1 {
            let at = self.store.quick_note_folder().and_then(|k| open.iter().position(|r| r.0 == k)).unwrap_or(0);
            let keys: Vec<String> = open.iter().map(|r| r.0.clone()).collect();
            folders.push(row(tr("快速笔记保存到", "Quick notes go to"), desc(tr("「新建笔记窗口」和全局快捷键记下的笔记放在这个文件夹里。", "Notes from New Note Window and the global shortcut are saved in this folder.")),
                self.select("quick-note-folder", open.into_iter().map(|r| r.1).collect(), at, move |w, i, cx| { let k = keys[i].clone(); w.set_pref(|p| p.quick_note_folder = k, cx) }, cx)).into_any_element());
        }
        // A new folder is made in the system's own open panel (its 新建文件夹 button), not here.
        folders.push(row(tr("打开文件夹", "Open a folder"), Some(tr("打开电脑上的文件夹，它会显示在侧栏里；要新的文件夹，在打开窗口里新建。", "Open a folder on your computer and it shows in the sidebar; for a new one, make it in the open window.").into()),
            button("open-folder", tr("打开…", "Open…")).on_click(cx.listener(|w, _, window, cx| w.open_folder_dialog(window, cx)))).into_any_element());
        div().w_full().flex().flex_col().gap(px(8.))
            .child(group("group-folders", Some(tr("文件夹", "Folders")), folders))
            .child(group("group-library", None, vec![
                row(tr("回收站", "Trash"), Some(crate::localized_format!("删除的文档保留 {} 天后自动清除。", "Deleted documents are kept for {} days.", crate::store::TRASH_DAYS).into()),
                    button("open-trash", tr("打开", "Open")).on_click(cx.listener(|w, _, _, cx| w.show_view(View::Trash, cx)))).into_any_element(),
                // #237: files no document (the Trash included) mentions.
                row(tr("未使用资源", "Unused assets"), Some(tr("资源文件夹里没有任何文档（含回收站、版本历史和本机文件）引用的图片和文件。检查后确认才会移出。", "Images and files in the asset folder that no document mentions, including the Trash, version history and local files. Nothing is moved until you confirm.").into()),
                    button("sweep-assets", tr("检查", "Check")).on_click(cx.listener(|w, _, window, cx| {
                        let root = crate::store::Store::active_dir();
                        let (live, outside) = w.sweep_inputs(window, cx);
                        cx.spawn(async move |this, cx| {
                            let sweep = cx.background_executor().spawn(async move { crate::store::Store::unused_assets_at(root, live, outside) }).await;
                            let _ = this.update(cx, |w, cx| match sweep {
                                Ok(files) if files.is_empty() => w.show_toast(crate::settings::text("没有未使用资源"), cx),
                                Ok(files) => { w.confirm = Some(crate::app::Confirm::RemoveUnused(files)); cx.notify(); }
                                Err(e) => w.sweep_failed(&e, cx),
                            });
                        }).detach();
                    }))).into_any_element(),
                row(tr("版本历史", "Version history"),
                    desc(tr("每篇文档保留的历史版本数（1–30 份）", "Snapshots kept for each document (1–30)")),
                    div().flex_none().flex().items_center().gap(px(12.))
                        .child(button("history-minus", "−").on_click(cx.listener(|w, _, window, cx| w.change_history_limit(-1, window, cx))))
                        .child(div().w(px(40.)).flex().items_center().justify_center().text_size(px(theme::ui_font())).text_color(Theme::text()).child(self.history_limit.to_string()))
                        .child(button("history-plus", "+").on_click(cx.listener(|w, _, window, cx| w.change_history_limit(1, window, cx))))).into_any_element(),
            ]))
            .into_any_element()
    }

    fn about_page(&self, cx: &mut Context<Self>) -> AnyElement {
        div().w_full().flex().flex_col().gap(px(28.)).child(div().w_full().pt(px(24.)).flex().flex_col().items_center().gap(px(10.))
            .child(img(about_icon()).size(px(88.)))
            .child(div().text_size(px(20.)).line_height(px(26.)).font_weight(FontWeight::SEMIBOLD).child("Quiet.md"))
            .child(div().text_size(px(theme::ui_font())).line_height(px(theme::ui_font() + 5.)).text_color(Theme::text_caption()).child(format!("{} {}", tr("版本", "Version"), env!("CARGO_PKG_VERSION"))))
            .child(div().text_size(px(theme::ui_font())).line_height(px(theme::ui_font() + 7.)).text_color(Theme::text_caption()).child(if cfg!(target_os = "macos") { tr("本地优先的笔记应用：无需账号，文档就是你电脑上的文件。", "Local-first notes: no account, your documents are files on your Mac.") }
                else { tr("本地优先的笔记应用：无需账号，文档就是你电脑上的文件。", "Local-first notes: no account, your documents are files on your computer.") })))
            .child(group("group-update", None, vec![update_row(cx).into_any_element()]))
            .child(group("group-repo", None, vec![row(tr("项目主页", "Project page"), Some(REPO.trim_start_matches("https://").into()),
                button("open-repo", tr("打开", "Open")).on_click(|_, _, cx| crate::editor::open_link(cx, REPO))).into_any_element()]))
            .child(group("group-onboarding", None, vec![row("Onboarding", Some(tr("在这个窗口上再走一遍首次启动的欢迎页（⌘⌥⇧O）；只会应用快捷键，不改动文档库", "Go through the first-launch pages again over this window (⌘⌥⇧O); only the shortcut is applied, the library stays as it is").into()),
                button("replay-onboarding", tr("打开", "Open")).on_click(cx.listener(|w, _, window, cx| w.replay_onboarding(window, cx)))).into_any_element()]))
            .into_any_element()
    }
}

/// 关于 › 项目主页: the source repository, opened in the 打开链接 browser.
const REPO: &str = "https://github.com/s1dashu/quiet-md";

/// The sidebar bar's 更新 pill, only while a new version is on offer:
/// 更新 downloads it (the pill then fills with its progress), 重启更新
/// installs it. Labels stay short: the bar keeps 72pt for it at the
/// narrowest sidebar, and a longer one ellipsizes rather than clips.
pub(crate) fn update_pill(cx: &mut Context<Workspace>) -> Option<AnyElement> {
    use crate::update::State;
    let state = crate::update::state(cx);
    let (label, tip) = match &state {
        State::Available(m) => (tr("更新", "Update").to_string(), crate::localized_format!("下载 Quiet.md {}", "Download Quiet.md {}", m.version)),
        State::Downloading(m) => (format!("{:.0}%", crate::update::progress(m) * 100.), crate::localized_format!("正在下载 {}…", "Downloading {}…", m.version)),
        State::Ready(m, _) => (tr("重启更新", "Restart").to_string(), crate::localized_format!("重启后更新到 {}，打开的文档会先保存", "Restart into {}; open documents are saved first", m.version)),
        State::Installing(m) => (tr("即将更新", "Updating").to_string(), crate::localized_format!("Quiet.md 退出后安装 {}", "{} installs when Quiet.md quits", m.version)),
        _ => return None,
    };
    let fill = if let State::Downloading(m) = &state { Some(crate::update::progress(m)) } else { None };
    let busy = matches!(state, State::Downloading(_) | State::Installing(_));
    Some(div().id("update-pill").no_drag().flex_shrink_1().min_w_0().h(px(24.)).px(px(10.)).relative().overflow_hidden().flex().items_center().justify_center().rounded_full()
        .text_size(px(theme::ui_font_small())).line_height(px(theme::ui_font_small() + 4.)).font_weight(FontWeight::MEDIUM)
        .when(!busy, |d| d.cursor_pointer().bg(crate::design::color::primary()).text_color(crate::design::color::primary_foreground()).hover(|s| s.opacity(0.9)))
        .when(busy, |d| d.bg(Theme::sidebar_hover()).text_color(Theme::text_caption()))
        // Downloading: a fixed width (no jitter as the digits change), filled left
        // to right behind a moving wave.
        .when_some(fill, |d, f| d.w(px(56.)).child(ui::wave_fill(f, crate::design::color::primary().opacity(0.18))))
        .child(div().relative().min_w_0().truncate().child(label))
        .tip(&tip, None)
        .on_click(move |_, _, cx| match &state { State::Available(_) => crate::update::download(cx), State::Ready(..) => crate::update::install(cx), _ => {} })
        .into_any_element())
}

/// 软件更新: where the updater stands and the one next step.
fn update_row(cx: &mut Context<Workspace>) -> Div {
    use crate::update::State;
    let state = crate::update::state(cx);
    let (title, detail): (SharedString, Option<SharedString>) = match &state {
        State::Idle => (tr("软件更新", "Software update").into(), None),
        State::Unsupported => (tr("软件更新", "Software update").into(), desc(tr("这个版本不能自动更新，请从网站下载新版本。", "This copy cannot update itself. Download new versions from the website."))),
        State::Checking => (tr("正在检查更新…", "Checking for updates…").into(), None),
        State::Current => (tr("已是最新版本", "Quiet.md is up to date").into(), None),
        State::Available(m) => (crate::localized_format!("新版本 {} 可用", "Version {} is available", m.version).into(), None),
        State::Downloading(m) => (crate::localized_format!("正在下载 {}… {:.0}%", "Downloading {}… {:.0}%", m.version, crate::update::progress(m) * 100.).into(), None),
        State::Ready(m, _) => (crate::localized_format!("{} 已准备好", "{} is ready to install", m.version).into(), desc(tr("重启后完成更新，打开的文档会先保存。", "Quiet.md restarts to finish; open documents are saved first."))),
        State::Installing(m) => (crate::localized_format!("{} 将在退出后安装", "{} installs when Quiet.md quits", m.version).into(), None),
        State::Failed(e) => (tr("更新失败", "Update failed").into(), Some(e.clone().into())),
    };
    let control = match &state {
        State::Checking | State::Downloading(_) | State::Installing(_) => None,
        State::Available(_) => Some(button("update-download", tr("下载更新", "Download")).on_click(|_, _, cx| crate::update::download(cx))),
        State::Ready(..) => Some(button("update-install", tr("重启以更新", "Restart to Update")).on_click(|_, _, cx| crate::update::install(cx))),
        _ => Some(button("update-check", tr("检查更新", "Check Now")).on_click(|_, _, cx| crate::update::check(true, cx))),
    };
    row(title, detail, div().children(control))
}

/// The app icon resampled once to 176px, its size at 88pt on a 2x screen.
/// GPUI samples images bilinearly without mipmaps, so the 1024px file shrunk
/// 5.8× on the GPU skips most of its pixels and the edges come out jagged.
/// On a 1x screen the 2:1 step is still an exact box filter.
pub(crate) fn about_icon() -> ImageSource {
    const PATH: &str = "branding/app-icon.png";
    static ICON: LazyLock<Option<Arc<RenderImage>>> = LazyLock::new(|| {
        let bytes = crate::assets::get(PATH)?;
        let mut buf = image::imageops::resize(&image::load_from_memory(bytes).ok()?.into_rgba8(), 176, 176, image::imageops::FilterType::Lanczos3);
        for px in buf.pixels_mut() {
            px.0.swap(0, 2);
        }
        Some(Arc::new(RenderImage::new(vec![image::Frame::new(buf)])))
    });
    match &*ICON { Some(icon) => icon.clone().into(), None => PATH.into() }
}

pub fn label(text: impl AsRef<str>) -> String { translate(text.as_ref(), lang()).to_string() }

pub fn text(text: &str) -> &str { translate(text, lang()) }

/// `localize` for any language: its own string, else the English one.
pub(crate) fn translate(text: &str, lang: Lang) -> &str {
    match lang { Lang::Zh | Lang::En => localize(text, lang == Lang::En), l => crate::l10n::get(l, text).unwrap_or_else(|| localize(text, true)) }
}

pub(crate) fn localize(text: &str, english: bool) -> &str {
    if !english { return text; }
    match text {
        "主页" => "Home",
        "我的文档" => "My documents",
        "置顶" => "Pinned",
        "收藏" => "Favorites",
        "设置" => "Settings",
        "新建" => "New",
        "新建文档" => "New document",
        "搜索" => "Search",
        "搜索文档…" => "Search documents…",
        "搜索文档..." => "Search documents...",
        "最近访问" => "Recently visited",
        "最近访问时间" => "Last visited",
        "最近修改时间" => "Last modified",
        "创建时间" => "Date created",
        "修改时间" => "Date modified",
        "文档名称" => "Document name",
        "名称" => "Name",
        "回收站" => "Trash",
        "存储管理" => "Storage",
        "更多" => "More",
        "返回" => "Back",
        "取消" => "Cancel",
        "确定" => "OK",
        "删除" => "Delete",
        "移到回收站" => "Move to trash",
        "恢复" => "Restore",
        "清空回收站" => "Empty trash",
        "永久删除" => "Delete permanently",
        "彻底删除" => "Delete permanently",
        "重命名" => "Rename",
        "创建副本" => "Duplicate",
        "复制" => "Copy",
        "粘贴" => "Paste",
        "剪切" => "Cut",
        "全选" => "Select all",
        "导入" => "Import",
        "导出" => "Export",
        "下载" => "Download",
        "移动到" => "Move to",
        "添加到收藏" => "Add to favorites",
        "取消收藏" => "Remove from favorites",
        "取消置顶" => "Unpin",
        "文档信息" => "Document information",
        "字数统计" => "Word count",
        "快捷键" => "Keyboard shortcuts",
        "最近浏览" => "Recently viewed",
        "所有文档" => "All documents",
        "按名称" => "By name",
        "上次浏览" => "Last viewed",
        "创建" => "Created",
        "修改" => "Modified",
        "访问" => "Visited",
        "收藏时间" => "Date favorited",
        "文档标题" => "Document title",
        "暂无文档" => "No documents yet",
        "没有找到相关文档" => "No documents found",
        "暂无收藏文档" => "No favorites yet",
        "收藏的文档会显示在这里" => "Your favorites will appear here",
        "左对齐" => "Align left",
        "居中对齐" => "Align center",
        "右对齐" => "Align right",
        "正文" => "Text",
        "一级标题" => "Heading 1",
        "二级标题" => "Heading 2",
        "三级标题" => "Heading 3",
        "四级标题" => "Heading 4",
        "五级标题" => "Heading 5",
        "六级标题" => "Heading 6",
        "加粗" => "Bold",
        "斜体" => "Italic",
        "下划线" => "Underline",
        "删除线" => "Strikethrough",
        "行内代码" => "Inline code",
        "链接" => "Link",
        "代码块" => "Code block",
        "无序列表" => "Bulleted list",
        "有序列表" => "Numbered list",
        "任务" => "Task",
        "待办事项" => "To-do list",
        "引用" => "Quote",
        "分割线" => "Divider",
        "高亮块" => "Callout",
        "图片" => "Image",
        "表格" => "Table",
        "分栏" => "Columns",
        "基础" => "Basic",
        "常用" => "Common",
        "更多类型" => "More blocks",
        "文字颜色" => "Text color",
        "背景颜色" => "Background color",
        "边框颜色" => "Border color",
        "添加图标" => "Add icon",
        "添加封面" => "Add cover",
        "编辑封面" => "Edit cover",
        "移除封面" => "Remove cover",
        "自动换行" => "Wrap lines",
        "取消换行" => "Unwrap lines",
        "文档库内搜索" => "Search documents",
        "网页搜索" => "Search the web",
        "展开" => "Expand",
        "收起" => "Collapse",
        "选择语言" => "Choose language",
        "视频或文件" => "Video or file",
        "清除格式" => "Clear formatting",
        "转换为" => "Turn into",
        "向上插入" => "Insert above",
        "向下插入" => "Insert below",
        "保存" => "Save",
        "关闭" => "Close",
        "缩进和对齐" => "Indent and align",
        "缩进" => "Indent",
        "颜色" => "Color",
        "在下方添加" => "Add below",
        "分隔线" => "Divider",
        "团队协作" => "Collaboration",
        "小应用" => "Widgets",
        "按钮" => "Button",
        "子页面列表" => "Child pages",
        "日期提醒" => "Date reminder",
        "倒计时" => "Countdown",
        "公式" => "Equation",
        "Craft for macOS 的文档样式：SF Pro 15 / 22，虚点分隔线，蓝色强调。" => "Craft-style typography: SF Pro 15 / 22, dotted dividers and blue accents.",
        "飞书文档的正文样式：16 / 26，标题 Medium，蓝色链接。" => "Feishu-style typography: 16 / 26 body text, medium headings and blue links.",
        "Notion 的默认浅色页面：暖灰正文、紧凑的块、灰底红字的行内代码、深色引用竖线。" => "Warm gray text, compact blocks, red inline code and dark quote borders.",
        "Obsidian 默认主题的浅色模式：紫色强调、浅灰列表符号、更大的标题与段落间距。" => "Purple accents, subtle list markers and generous heading and paragraph spacing.",
        "阅读模式切回编辑" => "Switch from reading to editing",
        "复制 / 剪切 / 粘贴" => "Copy / Cut / Paste",
        "逐字扩选" => "Extend selection by character",
        "逐行扩选" => "Extend selection by line",
        "选到行首 / 行尾" => "Select to start / end of line",
        "选到文档开头 / 结尾" => "Select to start / end of document",
        "# 空格" => "# Space",
        "## 空格" => "## Space",
        "### 空格" => "### Space",
        "1. 空格" => "1. Space",
        "- 空格 或 * 空格" => "- Space or * Space",
        "任务列表" => "To-do list",
        "[] 空格" => "[] Space",
        "增加缩进" => "Increase indent",
        "减少缩进" => "Decrease indent",
        "> 空格" => "> Space",
        "代码" => "Code",
        "`代码`" => "`code`",
        "``` 空格 或 ``` 代码语言 空格" => "``` Space or ``` language Space",
        "**文字** 空格" => "**text** Space",
        "*文字* 空格" => "*text* Space",
        "~文字~ 空格" => "~text~ Space",
        "~~文字~~ 空格" => "~~text~~ Space",
        "$$文字$$" => "$$text$$",
        "转为公式" => "Convert to equation",
        "只有同一段里的文字能转为公式" => "Only text within one paragraph can become an equation",
        "--- 或 ***" => "--- or ***",
        "阅读模式" => "Reading mode",
        "已切换为阅读模式" => "Switched to reading mode",
        "已切换为编辑模式" => "Switched to editing mode",
        "编辑" => "Edit",
        "可编辑文档" => "Edit this document",
        "阅读" => "Read",
        "仅查看或评论文档" => "View this document",
        "键盘快捷键" => "Keyboard shortcuts",
        "Markdown 是一种快速输入带格式文本的方式，让你书写文档时行云流水，一气呵成，例如输入#空格，即可输入一级标题。" => "Markdown lets you format text as you type. For example, type # followed by Space to create a heading.",
        "页宽设置" => "Page width",
        "查找和替换" => "Find and replace",
        "从“置顶”移除" => "Unpin",
        "添加到“置顶”" => "Pin",
        "下载为" => "Download as",
        "打印" => "Print",
        "显示引用列表" => "Show references",
        "复制为 Markdown" => "Copy as Markdown",
        "粘贴为纯文本" => "Paste as plain text",
        "插入链接" => "Insert link",
        "插入" => "Insert",
        "对齐" => "Align",
        "上移块" => "Move block up",
        "下移块" => "Move block down",
        "复制文件地址" => "Copy file path",
        "已复制文件地址" => "File path copied",
        "已复制图片" => "Image copied",
        "已复制为 Markdown" => "Copied as Markdown",
        "为当前窗口选择合适页宽" => "Choose a page width for this window",
        "默认" => "Default",
        "较宽" => "Wide",
        "全宽" => "Full width",
        "几秒前" => "a few seconds ago",
        "今天" => "Today",
        "昨天" => "Yesterday",
        "前天" => "Two days ago",
        "天" => "d",
        "时" => "h",
        "分" => "min",
        "秒" => "s",
        "无效公式" => "Invalid equation",
        "目录下暂无子页面" => "No child pages yet",
        "切换为宫格视图" => "Switch to grid view",
        "切换为列表视图" => "Switch to list view",
        "当前列表暂无内容" => "Nothing here yet",
        "标题" => "Title",
        "空白文档" => "Untitled",
        "退出" => "Exit",
        "退出批量操作" => "Exit selection mode",
        "点击展开" => "Click to expand",
        "点击折叠" => "Click to collapse",
        "批量操作" => "Select documents",
        "批量管理" => "Manage",
        "无法选中，选择的页面数已到达上限" => "Selection limit reached",
        "在新窗口中打开" => "Open in new window",
        "搜索目标位置" => "Search destinations",
        "移动成功" => "Moved successfully",
        "无搜索结果" => "No results",
        "从 我的文档 移动到" => "Move from My documents to",
        "信息与数据" => "Information and data",
        "概览" => "Overview",
        "最近修改" => "Last modified",
        "文档统计" => "Document statistics",
        "总字数" => "Words",
        "总字符数" => "Characters",
        "大小" => "Size",
        "当前系统不支持" => "Not supported on this platform",
        "打印超时，请稍后重试" => "Printing timed out. Please try again.",
        "打印失败，请重试" => "Printing failed. Please try again.",
        "需要 Microsoft Edge WebView2 运行时" => "Requires the Microsoft Edge WebView2 Runtime",
        "没有找到匹配的结果，尝试其他关键词进行搜索" => "No results. Try another search term.",
        "请先关闭正在编辑这些文档的其他窗口，再删除" => "Close other windows editing these documents before deleting them.",
        "删除成功" => "Deleted successfully",
        "已添加到“置顶”" => "Pinned",
        "已从“置顶”移除" => "Unpinned",
        "已添加到“收藏”" => "Added to favorites",
        "已从“收藏”中移除" => "Removed from favorites",
        "保存失败" => "Save failed",
        "返回主页" => "Back to Home",
        "搜索(⌘ + J)" => "Search (⌘ + J)",
        "导出失败，请重试" => "Export failed. Please try again.",
        "正在导出文档" => "Exporting document",
        "文档打印中" => "Printing document",
        "文件格式不支持" => "Unsupported file format",
        "导入失败，请检查文件格式" => "Import failed. Check the file format.",
        "导出 Markdown 设置" => "Export Markdown",
        "内容范围" => "Content",
        "仅文本（不含图片和附件）" => "Text only (no images or attachments)",
        "所有内容" => "All content",
        "即将彻底删除" => "Deleting soon",
        "回收站暂无内容" => "Trash is empty",
        "位置" => "Location",
        "剩余时间" => "Time remaining",
        "恢复成功。部分文档由于原位置不存在，已恢复到“我的文档”" => "Restored. Some documents were moved to My documents because their original location no longer exists.",
        "恢复成功" => "Restored successfully",
        "已彻底删除" => "Permanently deleted",
        "清空" => "Empty",
        "已清空回收站" => "Trash emptied",
        "清理未使用资源" => "Remove unused assets",
        "没有未使用资源" => "No unused assets",
        "没有可清理的资源" => "Nothing to remove",
        "移出" => "Move out",
        "确认清空回收站？" => "Empty the Trash?",
        "删除的内容将进入回收站，30 天后自动彻底删除" => "Deleted items will be moved to Trash and permanently deleted after 30 days.",
        "确认恢复？" => "Restore this document?",
        "确认后将恢复至原位置" => "The document will be restored to its original location.",
        "确认彻底删除？" => "Delete permanently?",
        "彻底删除后，该页面及其子页面将无法恢复" => "This page and its child pages cannot be recovered after permanent deletion.",
        "是否隐藏引用列表？" => "Hide references?",
        "选择隐藏后，协作者将无法查看本文的引用列表。如需重新显示，可点击文档右上角的更多按钮“...”，打开“显示引用列表”开关。" => "The reference list will be hidden. To show it again, open the document’s More menu and turn on Show references.",
        "隐藏" => "Hide",
        "是否显示引用列表？" => "Show references?",
        "选择显示后，协作者可再次查看本文的引用列表。" => "The document’s reference list will be visible again.",
        "显示" => "Show",
        "文档保存失败" => "Could not save document",
        "重试" => "Retry",
        "不再提示" => "Do not ask again",
        "放弃修改" => "Discard changes",
        "Quiet.md · 快速笔记" => "Quiet.md · Quick note",
        "没有匹配结果，换个关键词试试吧" => "No results. Try another search term.",
        "本文被以下文档所引用" => "Referenced by these documents",
        "隐藏引用列表" => "Hide references",
        "反向引用" => "Backlinks",
        "本文引用" => "Links from this document",
        "此文档已被删除" => "This document was deleted",
        "本文暂未被其它文档引用" => "No backlinks yet",
        "本文暂未引用其它文档" => "No linked documents yet",
        "查看更多" => "Show more",
        "当前为阅读模式，请切换为编辑模式后重试" => "Switch to editing mode to make changes.",
        "表情符号" => "Emoji",
        "动物" => "Animals",
        "食物" => "Food",
        "活动" => "Activities",
        "地点" => "Places",
        "物件" => "Objects",
        "符号" => "Symbols",
        "最近" => "Recent",
        "随机" => "Random",
        "移除" => "Remove",
        "输入“/”快速插入内容" => "Type / to insert content",
        "点击更换图标" => "Click to change icon",
        "请输入标题" => "Enter a title",
        "请输入有效的链接。" => "Enter a valid link.",
        "确认" => "Confirm",
        "文本" => "Text",
        "复制原始网页链接" => "Copy original link",
        "链接已复制到剪贴板" => "Link copied to clipboard",
        "编辑链接" => "Edit link",
        "移除链接" => "Remove link",
        "渐变" => "Gradients",
        "极光" => "Aurora",
        "山峦" => "Mountains",
        "几何" => "Geometry",
        "上传图片" => "Upload image",
        "展示文档信息" => "Show document information",
        "更换图标" => "Change icon",
        "拖拽以调整图片的位置" => "Drag to reposition the image",
        "选择图片" => "Choose image",
        "调整图片位置" => "Reposition image",
        "官方图库" => "Gallery",
        "本地上传" => "Upload",
        "推荐使用尺寸为 2,000 × 400 的图片，显示效果最佳" => "For best results, use a 2,000 × 400 image.",
        "新增分栏" => "Add column",
        "图片格式仅支持 jpeg、jpg、png、gif、webp、bmp" => "Supported image formats: JPEG, JPG, PNG, GIF, WebP and BMP",
        "识别图片宽高失败" => "Could not read image dimensions",
        "上传图片大小不能超过 20 MB" => "Images must be 20 MB or smaller",
        "文件不存在" => "File not found",
        "文件夹不存在" => "Folder not found",
        "这个文件夹在文档库里，它的内容已经在我的文档中" => "This folder is inside the library; its notes are already in My documents",
        "这个文件夹里有文档库，不能作为本机文件夹打开" => "This folder holds the library, so it can't be opened as a folder on this computer",
        "本机文件夹和文档库之间只能复制，不能移动" => "Between folders on this computer and the library, notes can be copied but not moved",
        "加载失败，" => "Could not load. ",
        "点击重试" => "Retry",
        "图片过小，请放大后重试" => "Image too small. Enlarge it and try again.",
        "保存裁剪" => "Save crop",
        "裁剪" => "Crop",
        "重置图片" => "Reset image",
        "编辑描述" => "Edit caption",
        "添加描述" => "Add caption",
        "上一张" => "Previous image",
        "下一张" => "Next image",
        "缩小" => "Zoom out",
        "放大" => "Zoom in",
        "适应页面" => "Fit to page",
        "原始尺寸" => "Actual size",
        "旋转" => "Rotate",
        "红色" => "Red",
        "橙色" => "Orange",
        "黄色" => "Yellow",
        "绿色" => "Green",
        "蓝色" => "Blue",
        "紫色" => "Purple",
        "灰色" => "Gray",
        "倒计时已结束" => "Countdown finished",
        "设置的倒计时时间必须晚于当前 GMT+8 的时间" => "Choose a time later than the current time in GMT+8.",
        "输入倒计时时长" => "Enter duration",
        "选择具体日期" => "Choose date and time",
        "倒计时结束时显示气泡提醒" => "Show a notification when the countdown ends",
        "倒计时设置" => "Countdown settings",
        "一" => "Mon",
        "二" => "Tue",
        "三" => "Wed",
        "四" => "Thu",
        "五" => "Fri",
        "六" => "Sat",
        "日" => "Sun",
        "周一" => "Mon",
        "周二" => "Tue",
        "周三" => "Wed",
        "周四" => "Thu",
        "周五" => "Fri",
        "周六" => "Sat",
        "周日" => "Sun",
        "事件发生时" => "At time of event",
        "提前 5 分钟" => "5 minutes before",
        "提前 15 分钟" => "15 minutes before",
        "提前 30 分钟" => "30 minutes before",
        "提前 1 小时" => "1 hour before",
        "提前 2 小时" => "2 hours before",
        "提前 1 天" => "1 day before",
        "提前 2 天" => "2 days before",
        "提前 1 周" => "1 week before",
        "当天 (09:00)" => "Same day (09:00)",
        "提前 1 天 (09:00)" => "1 day before (09:00)",
        "提前 2 天 (09:00)" => "2 days before (09:00)",
        "提前 1 周 (09:00)" => "1 week before (09:00)",
        "当天 (18:00)" => "Same day (18:00)",
        "明天" => "Tomorrow",
        "该提醒已被删除。" => "This reminder was deleted.",
        "设置时间" => "Set time",
        "提醒" => "Reminder",
        "收起目录" => "Hide table of contents",
        "展开目录" => "Show table of contents",
        "#### 空格" => "#### Space",
        "##### 空格" => "##### Space",
        "列" => "columns",
        "行" => "rows",
        "插入列" => "Insert column",
        "插入行" => "Insert row",
        "添加 LaTeX 公式" => "Add LaTeX equation",
        "上一处" => "Previous match",
        "下一处" => "Next match",
        "查找" => "Find",
        "替换为" => "Replace with",
        "全部替换" => "Replace all",
        "正则表达式" => "Regular expression",
        "正则有误" => "Invalid regex",
        "替换" => "Replace",
        "请输入代码块名称" => "Enter code block name",
        "输入图片描述" => "Enter image caption",
        "新编号为" => "Start at",
        "继续之前的编号" => "Continue numbering",
        "开始新列表" => "Start new list",
        "修改编号值" => "Change list number",
        "未能找到相关结果" => "No results found",
        "插入支持富文本的表格" => "Insert a rich text table",
        "选择栏数" => "Choose number of columns",
        "字体颜色" => "Text color",
        "恢复默认" => "Reset to default",
        "填充颜色" => "Fill color",
        "按 ESC 完成输入" => "Press Esc to finish",
        "无匹配结果" => "No matches",
        "粗体" => "Bold",
        "**文本** 空格" => "**text** Space",
        "~~文本~~ 空格" => "~~text~~ Space",
        "*文本* 空格" => "*text* Space",
        "~文本~ 空格" => "~text~ Space",
        "拆分单元格" => "Split cells",
        "合并单元格" => "Merge cells",
        "单元格背景颜色" => "Cell background",
        "删除列" => "Delete columns",
        "删除行" => "Delete rows",
        "上方插入行" => "Insert row above",
        "下方插入行" => "Insert row below",
        "左侧插入列" => "Insert column left",
        "右侧插入列" => "Insert column right",
        "清空内容" => "Clear contents",
        "删除表格" => "Delete table",
        "顶部对齐" => "Align top",
        "垂直居中" => "Align middle",
        "底部对齐" => "Align bottom",
        "标题行" => "Header row",
        "标题列" => "Header column",
        "均分列宽" => "Distribute columns evenly",
        "粘贴或输入链接" => "Paste or enter a link",
        "输入文本" => "Enter text",
        "在文档内查找" => "Find in document",
        "请输入文字" => "Enter text",
        "编号" => "Number",
        "请输入公式" => "Enter an equation",
        "插入图片" => "Insert image",
        "请输入按钮" => "Enter button text",
        "请输入链接地址" => "Enter link URL",
        "按钮不能为空" => "Button text is required",
        "按钮文字不能超过 200 个字" => "Button text must be 200 characters or fewer",
        "超链接不合法，请重新输入" => "Invalid link. Please enter a valid URL.",
        "当前位置暂不支持该功能" => "This action is not available here.",
        "按钮文字" => "Button text",
        "按钮颜色" => "Button color",
        "执行操作" => "Action",
        "超链接地址" => "Link URL",
        "选择按钮类型" => "Choose button type",
        "无法播放" => "Cannot play",
        "暂停" => "Pause",
        "播放" => "Play",
        "取消静音" => "Unmute",
        "静音" => "Mute",
        "预览" => "Preview",
        "无法播放该音频" => "Cannot play this audio",
        "全屏" => "Full screen",
        "文本视图" => "Text view",
        "卡片视图" => "Card view",
        "预览视图" => "Preview view",
        "插入视频或文件" => "Insert video or file",
        "文件读取失败" => "Could not read file",
        "未命名文档" => "Untitled",
        "画板" => "Board",
        // 数据表
        "查看" => "Open",
        "退出全屏" => "Exit full screen",
        "上周" => "Last week",
        "上月" => "Last month",
        "下周" => "Next week",
        "下月" => "Next month",
        "不为空" => "Is not empty",
        "不包含" => "Does not contain",
        "不展示" => "None",
        "不等于" => "Is not",
        "业务" => "Business",
        "个隐藏字段" => "hidden fields",
        "中等" => "Medium",
        "为空" => "Is empty",
        "人员" => "Person",
        "人民币" => "Chinese yuan",
        "任一" => "Any",
        "位" => "places",
        "位小数" => "decimal places",
        "低" => "Short",
        "俄罗斯卢布" => "Russian ruble",
        "保留" => "Keep",
        "修改字段/列" => "Edit field",
        "倒序" => "Descending",
        "允许关联多条记录" => "Allow linking to multiple records",
        "全部展开" => "Expand all",
        "全部折叠" => "Collapse all",
        "全部显示" => "Show all",
        "全部隐藏" => "Hide all",
        "公式内容" => "Formula",
        "公式有效" => "Formula is valid",
        "关联" => "Link",
        "关联到" => "Link to",
        "关联字段" => "Link field",
        "关联的数据表" => "Linked table",
        "具体日期" => "Exact date",
        "冻结至此字段/列" => "Freeze up to this field",
        "函数" => "Functions",
        "分组" => "Group",
        "创建于" => "Created",
        "删除字段/列" => "Delete field",
        "个字段" => "fields",
        "删除数据表" => "Delete table",
        "删除视图" => "Delete view",
        "删除记录" => "Delete record",
        "删除选项" => "Delete option",
        "加拿大元" => "Canadian dollar",
        "包含" => "Contains",
        "千分位" => "Thousands separator",
        "千分位（小数点）" => "Thousands separator (decimal)",
        "单选" => "Single select",
        "印度卢比" => "Indian rupee",
        "原样引用" => "Original values",
        "去重" => "Unique values",
        "双向关联" => "Two-way link",
        "取消冻结" => "Unfreeze",
        "取消选择" => "Deselect",
        "合并文本" => "Join text",
        "向上插入 1 行" => "Insert row above",
        "向下插入 1 行" => "Insert row below",
        "向右插入字段/列" => "Insert field right",
        "向左插入字段/列" => "Insert field left",
        "唯一值" => "Unique",
        "地理位置" => "Location",
        "填色" => "Color",
        "复制为 Markdown 表格" => "Copy as Markdown table",
        "复制字段/列" => "Duplicate field",
        "复制视图" => "Duplicate view",
        "复选框" => "Checkbox",
        "多维表格" => "Base",
        "多选" => "Multiple select",
        "大于" => "Greater than",
        "大于或等于" => "Greater than or equal",
        "字段与函数" => "Fields and functions",
        "字段引用" => "Field references",
        "字段格式" => "Format",
        "字段类型" => "Field type",
        "字段配置" => "Fields",
        "导入 CSV 为新数据表" => "Import CSV as new table",
        "导出为 CSV" => "Export as CSV",
        "小于" => "Less than",
        "小于或等于" => "Less than or equal",
        "小数位数" => "Decimal places",
        "已勾选" => "Checked",
        "已填写" => "Filled",
        "已填写占比" => "Percent filled",
        "已选" => "Selected",
        "巴西雷亚尔" => "Brazilian real",
        "常规" => "General",
        "平均值" => "Average",
        "成员" => "Members",
        "所有" => "All",
        "按 A 到 Z 排序" => "Sort A to Z",
        "按 Z 到 A 排序" => "Sort Z to A",
        "按此字段分组" => "Group by this field",
        "按此字段筛选" => "Filter by this field",
        "排序" => "Sort",
        "搜索记录" => "Search records",
        "数字" => "Number",
        "数字格式" => "Number format",
        "数据表" => "Data table",
        "整数" => "Integer",
        "新增字段" => "Add field",
        "新增记录" => "Add record",
        "新建数据表" => "New table",
        "新建视图" => "New view",
        "新建选项" => "Create option",
        "新记录默认填入创建日期" => "Default to the creation date",
        "无法读取数据表" => "Could not read the data table",
        "无结果" => "No results",
        "日历" => "Calendar",
        "日历视图" => "Calendar view",
        "日期格式" => "Date format",
        "早于" => "Is before",
        "显示时间" => "Include time",
        "晚于" => "Is after",
        "最后更新于" => "Last modified",
        "最后更新时间" => "Last modified time",
        "最大值" => "Max",
        "最大分值" => "Maximum",
        "最小值" => "Min",
        "最早" => "Earliest",
        "最晚" => "Latest",
        "未分类" => "Uncategorized",
        "未勾选" => "Unchecked",
        "未命名数据表" => "Untitled data table",
        "未命名记录" => "Untitled record",
        "未填写" => "Empty",
        "未填写占比" => "Percent empty",
        "未来 30 天内" => "Next 30 days",
        "未来 7 天内" => "Next 7 days",
        "本周" => "This week",
        "本月" => "This month",
        "条件" => "conditions",
        "条码" => "Barcode",
        "条记录" => "records",
        "查找引用" => "Lookup",
        "查找或创建选项" => "Find or create an option",
        "查看详情" => "Open record",
        "打开链接" => "Open link",
        "⌘ + 点击" => "⌘ + click",
        "一起删除" => "Delete both",
        "保留笔记" => "Keep the notes",
        "找不到笔记" => "Note not found",
        "指向已有笔记" => "Point to an existing note",
        "搜索笔记" => "Search notes",
        "整页打开" => "Open as a page",
        "新建笔记" => "New note",
        "没有笔记" => "No notes",
        "独立的数据表才能加笔记列" => "Only a data table of its own can have a note column",
        "空笔记" => "Empty",
        "笔记" => "Note",
        "欧元" => "Euro",
        "求和" => "Sum",
        "没有符合筛选条件的记录" => "No records match the filter",
        "没有记录" => "No records",
        "没有选项" => "No options",
        "添加人员" => "Add person",
        "添加成员" => "Add member",
        "添加条件" => "Add condition",
        "添加记录" => "Add record",
        "添加选项" => "Add option",
        "添加附件" => "Add attachment",
        "港元" => "Hong Kong dollar",
        "澳大利亚元" => "Australian dollar",
        "瑞士法郎" => "Swiss franc",
        "甘特图" => "Gantt",
        "甘特视图" => "Gantt view",
        "电话号码" => "Phone",
        "百分比" => "Percent",
        "百分比（小数点）" => "Percent (decimal)",
        "看板" => "Kanban",
        "看板视图" => "Kanban view",
        "看板需要按单选字段分组，请先添加一个单选字段" => "A kanban is grouped by a single select field: add one first",
        "空" => "Empty",
        "符合以下" => "Match",
        "等于" => "Is",
        "筛选" => "Filter",
        "统计" => "Summary",
        "编号前缀" => "Prefix",
        "编号前缀，例如 NO." => "Prefix, e.g. NO.",
        "美元" => "US dollar",
        "自动编号" => "Auto number",
        "英镑" => "Pound sterling",
        "行高" => "Row height",
        "表单" => "Form",
        "表单视图" => "Form view",
        "表格视图" => "Grid view",
        "要引用的字段" => "Field to look up",
        "计数" => "Count",
        "计数（非空）" => "Count (non-empty)",
        "计算方式" => "Calculation",
        "记录总数" => "Count",
        "设置分组条件" => "Group by",
        "设置填色条件" => "Color rows",
        "设置排序条件" => "Sort by",
        "设置筛选条件" => "Filter",
        "设置行高" => "Row height",
        "评分" => "Rating",
        "请先添加关联字段" => "Add a link field first",
        "已删除的数据表" => "Deleted table",
        "是" => "Yes",
        "否" => "No",
        "已删除的字段" => "Deleted field",
        "这个字段来自有多张数据表的旧文件：它照常显示，设置不能再修改。" => "This field comes from an older file with several tables. It still shows its values, but its settings can no longer be changed.",
        "请输入" => "Enter",
        "请输入字段标题" => "Field name",
        "请输入选项" => "Option name",
        "请输入默认值" => "Default value",
        "请选择人员" => "Choose people",
        "请选择关联字段" => "Choose a link field",
        "请选择关联字段和要引用的字段" => "Choose a link field and the field to look up",
        "请选择日期" => "Choose a date",
        "请选择要引用的字段" => "Choose the field to look up",
        "请选择记录" => "Choose records",
        "请选择选项" => "Choose options",
        "货币" => "Currency",
        "超链接" => "URL",
        "超高" => "Extra tall",
        "适应内容" => "Fit to content",
        "输入名字后回车添加" => "Type a name and press Enter",
        "输入名称后回车新建选项" => "Type a name and press Enter to create an option",
        "过去 30 天内" => "Past 30 days",
        "过去 7 天内" => "Past 7 days",
        "还没有附件" => "No attachments yet",
        "进度" => "Progress",
        "选择一个单选或人员字段作为看板的分组依据" => "Choose a single select or person field to stack the kanban by",
        "选择字段" => "Choose a field",
        "选择条件" => "Add",
        "选项倒序" => "Options, last first",
        "选项内容" => "Options",
        "选项名称" => "Option name",
        "选项顺序" => "Options, first to last",
        "阿联酋迪拉姆" => "UAE dirham",
        "附件" => "Attachment",
        "隐藏字段/列" => "Hide field",
        "韩元" => "South Korean won",
        "顺序" => "Ascending",
        "高" => "Tall",
        "高级" => "Advanced",
        "默认值" => "Default value",
        "日期" => "Date",
        "添加" => "Add",
        "清除" => "Clear",
        "未命名画板" => "Untitled board",
        "退出画板" => "Exit board",
        "空画板" => "Empty board",
        "双击打开画板" => "Double-click to open the board",
        "重置缩放" => "Reset zoom",
        "缩放到全部内容" => "Zoom to fit all elements",
        "打开超链接" => "Open link",
        "关注文档更新" => "Follow document updates",
        "刚刚" => "just now",
        "文档不存在" => "Document not found",
        "文档文件已不在原处，可能已被移动或删除" => "The document's file is no longer where it was. It may have been moved or deleted.",
        "文档已被另一个实例修改，未覆盖磁盘内容。请另存副本或重新载入磁盘版本" => "Another instance modified this document. The file was not overwritten. Save a copy or reload the version on disk.",
        "这个文档库由更新版本的 Quiet.md 创建，部分内容可能无法正确显示，请升级后再编辑" => "These documents were created by a newer version of Quiet.md. Update the app before editing them.",
        _ => text,
    }
}

#[macro_export]
macro_rules! localized_format {
    ($zh:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        // Other languages translate the Chinese template; the values are read back out of the Chinese text.
        $crate::settings::localized($crate::settings::lang(), $zh, || format!($zh $(, $arg)*), || format!($en $(, $arg)*))
    };
}

/// User titles are content, not translation keys.
pub fn display_title(title: &str) -> String { crate::model::display_title(title) }

/// Core diagnostics stay independent of UI language. Only known message templates
/// are translated here; captured paths, document IDs and OS errors are preserved.
pub fn diagnostic(message: &str) -> String {
    let l = lang();
    if l != Lang::Zh && l != Lang::En
        && let Some(s) = crate::l10n::get(l, message).map(str::to_string).or_else(|| crate::l10n::refill_any(l, message)) { return s; }
    diagnostic_for(message, english())
}

fn diagnostic_for(message: &str, english: bool) -> String {
    if !english { return message.to_string(); }
    const TEMPLATES: &[(&str, &str)] = &[
        ("无法读取文档索引：{}", "Could not read document index: {}"),
        ("无法读取回收站索引：{}", "Could not read Trash index: {}"),
        ("文档索引已损坏，已从备份恢复（原文件保留为 {}）", "Document index was damaged and restored from backup (original retained as {})."),
        ("回收站索引已损坏，已从备份恢复（原文件保留为 {}）", "Trash index was damaged and restored from backup (original retained as {})."),
        ("文档索引已损坏且没有可用备份，部分文档的置顶、层级等信息已重置（原文件保留为 {}）", "Document index was damaged with no usable backup. Some pins and document hierarchy were reset (original retained as {})."),
        ("回收站索引已损坏且没有可用备份，部分文档的置顶、层级等信息已重置（原文件保留为 {}）", "Trash index was damaged with no usable backup. Some metadata was reset (original retained as {})."),
        ("回收站已存在同名文档，未覆盖：{}", "A document with this ID already exists in Trash and was not overwritten: {}"),
        ("文档库已存在同名文档，未覆盖：{}", "A document with this ID already exists in Documents and was not overwritten: {}"),
        ("文档已移入回收站，文档索引清理失败：{}", "Document moved to Trash, but the document index could not be cleaned up: {}"),
        ("回收站目录读取不完整：{}", "Could not fully read the Trash directory: {}"),
        ("无法清理过期回收站文档 {}：{}", "Could not remove expired Trash document {}: {}"),
        ("回收站索引保存失败：{}", "Could not save Trash index: {}"),
        ("{}还在 iCloud 上没有下载，这次没有读取", "{} is still only in iCloud and wasn't read this time"),
        ("{}还在 iCloud 上没有下载，没有写入，以免覆盖其他设备的内容", "{} is still only in iCloud and wasn't written, so as not to overwrite another device's data"),
        ("这篇文档还在从 iCloud 下载，请稍后再打开", "This document is still downloading from iCloud. Open it again in a moment."),
        ("恢复文档 {} 失败，已恢复的文档保留在文档库", "Could not restore document {}. Documents already restored remain in Documents."),
        ("文档已恢复，回收站索引清理失败：{}", "Document restored, but the Trash index could not be cleaned up: {}"),
    ];
    for (source, target) in TEMPLATES {
        let parts: Vec<_> = source.split("{}").collect();
        let Some(mut rest) = message.strip_prefix(parts[0]) else { continue };
        let mut values = Vec::new();
        let mut matched = true;
        for (i, suffix) in parts.iter().enumerate().skip(1) {
            let split = if i == parts.len() - 1 { rest.strip_suffix(suffix).map(|v| (v, "")) } else { rest.split_once(suffix) };
            let Some((value, next)) = split else { matched = false; break };
            values.push(value);
            rest = next;
        }
        if matched && rest.is_empty() {
            let mut result = String::new();
            let mut fragments = target.split("{}");
            result.push_str(fragments.next().unwrap_or_default());
            for (value, fragment) in values.iter().zip(fragments) { result.push_str(value); result.push_str(fragment); }
            return result;
        }
    }
    localize(message, true).to_string()
}

#[cfg(test)]
mod localization_tests {
    use fdl_core::vfs::PathExt as _;
    use super::{localize, diagnostic_for, display_title};

    #[test]
    fn ui_keys_and_menu_tables_have_english_translations() {
        fn check(key: &str) {
            if key.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)) {
                assert_ne!(localize(key, true), key, "Missing English UI translation: {key}");
                assert_eq!(localize(key, false), key);
            }
        }
        fn walk(dir: &std::path::Path, pattern: &regex::Regex) {
            for entry in fdl_core::vfs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.fs_is_dir() { walk(&path, pattern); }
                else if path.extension().is_some_and(|ext| ext == "rs") {
                    let source = fdl_core::vfs::read_to_string(path).unwrap();
                    for hit in pattern.captures_iter(source.split("#[cfg(test)]").next().unwrap()) { check(&hit[1]); }
                }
            }
        }
        let pattern = regex::Regex::new(r#"settings::(?:text|label)\("([^"\\]*)""#).unwrap();
        walk(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &pattern);
        for item in crate::editor::menus::SLASH_ITEMS { check(item.label); }
        for item in crate::editor::menus::TURN_ITEMS { check(item.label); check(item.tip); if let Some(md) = item.md { check(md); } }
        for group in crate::editor::menus::GROUPS { check(group.title); }
    }

    #[test]
    fn diagnostics_preserve_paths_and_identifiers() {
        let source = "无法清理过期回收站文档 我的笔记.xml：Permission denied";
        assert_eq!(diagnostic_for(source, false), source);
        assert_eq!(diagnostic_for(source, true), "Could not remove expired Trash document 我的笔记.xml: Permission denied");
        assert_eq!(diagnostic_for("文档索引已损坏，已从备份恢复（原文件保留为 index.json.bad）", true), "Document index was damaged and restored from backup (original retained as index.json.bad).");
        assert_eq!(diagnostic_for("unknown OS error", true), "unknown OS error");
    }

    #[test]
    fn user_titles_are_never_translation_keys() {
        for title in ["主页", "收藏", "未命名文档", "My notes", "我的中文笔记"] { assert_eq!(display_title(title), title); }
    }
}

#[cfg(test)]
mod preference_tests {
    use super::Preferences;

    #[test]
    fn new_preferences_use_feishu_without_overriding_saved_themes() {
        assert_eq!(Preferences::default().theme_id(), "feishu");
        for (json, expected) in [
            (r#"{}"#, "feishu"),
            (r#"{"feishu":false}"#, "craft"),
            (r#"{"feishu":true}"#, "feishu"),
            (r#"{"theme":"notion","feishu":false}"#, "notion"),
            (r#"{"theme":"craft","feishu":true}"#, "craft"),
            (r#"{"theme":"my-theme"}"#, "my-theme"),
        ] {
            let saved: Preferences = serde_json::from_str(json).unwrap();
            assert_eq!(saved.theme_id(), expected);
            let round_trip: Preferences = serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
            assert_eq!(round_trip.theme_id(), expected);
        }
    }

    #[test]
    fn language_comes_from_older_files_english_switch() {
        use super::Lang;
        for (json, expected) in [
            (r#"{"english":false}"#, Lang::Zh),
            (r#"{"english":true}"#, Lang::En),
            (r#"{"english":true,"language":"zh-TW"}"#, Lang::ZhTw),
            (r#"{"english":true,"language":"zh-Hant"}"#, Lang::ZhTw),
            (r#"{"english":true,"language":"ko"}"#, Lang::Ko),
            (r#"{"english":true,"language":"ar"}"#, Lang::Ar),
            (r#"{"english":true,"language":"tr"}"#, Lang::Tr),
            (r#"{"english":true,"language":"xx"}"#, Lang::En),
        ] {
            let saved: Preferences = serde_json::from_str(json).unwrap();
            assert_eq!(saved.lang(), expected, "{json}");
        }
        let mut p: Preferences = serde_json::from_str(r#"{"english":false}"#).unwrap();
        p.set_lang(Lang::Ko);
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!((json["language"].as_str(), json["english"].as_bool()), (Some("ko"), Some(true)), "older builds read English");
        p.set_lang(Lang::ZhTw);
        assert_eq!((p.language.as_str(), p.english), ("zh-TW", true));
        p.set_lang(Lang::Zh);
        assert!(!p.english);
    }

    #[test]
    fn system_locale_distinguishes_traditional_and_simplified_chinese() {
        use super::{Lang, lang_for_locale};
        for tag in ["zh-TW", "zh_TW.UTF-8", "zh-Hant", "zh-Hant-TW", "zh-Hant-HK", "zh-HK", "zh-MO"] {
            assert_eq!(lang_for_locale(tag), Some(Lang::ZhTw), "{tag}");
        }
        for tag in ["zh", "zh-CN", "zh_CN.UTF-8", "zh-Hans", "zh-Hans-CN", "zh-SG", "zh-Hans-HK"] {
            assert_eq!(lang_for_locale(tag), Some(Lang::Zh), "{tag}");
        }
        assert_eq!(lang_for_locale("fr_FR.UTF-8"), Some(Lang::Fr));
        assert_eq!(lang_for_locale("tr-TR"), Some(Lang::Tr));
        assert_eq!(lang_for_locale("C.UTF-8"), None);
    }

    /// `lang()` reads the stored discriminant back by its position in `ALL`.
    #[test]
    fn languages_are_listed_in_declaration_order() {
        use super::Lang;
        for (i, l) in Lang::ALL.into_iter().enumerate() { assert_eq!(l as usize, i, "{l:?}"); }
    }
}

#[cfg(test)]
mod tests {
    use super::{arrange, Choice, Lang, Preferences};

    fn fonts(list: &[Choice]) -> Vec<String> { list.iter().filter_map(|c| match c { Choice::Font { .. } => c.key(), _ => None }).collect() }
    fn headers(list: &[Choice]) -> Vec<String> { list.iter().filter_map(|c| if let Choice::Header(h) = c { Some(h.to_string()) } else { None }).collect() }

    /// Sections by class in the interface language's order, the downloads in
    /// theirs (not listed twice), symbols folded, 收藏 / 最近使用 on top, and
    /// a search flat and without the reset.
    #[test]
    fn font_list_is_grouped_and_searched() {
        let all: Vec<String> = ["Georgia", "Helvetica", "Menlo", "PingFang SC", "Songti SC", "Kaiti SC", "Webdings", "LXGW WenKai"].map(String::from).to_vec();
        let dl: Vec<_> = ["lxgw-wenkai", "zhuque-fangsong"].iter().filter_map(|id| crate::font_downloads::entry(id)).collect();
        let zh = arrange("", false, &all, &dl, &[], &[], Lang::Zh);
        assert_eq!(zh[0], Choice::Reset);
        // English labels: tests read the interface in English (`load`)
        assert_eq!(headers(&zh), ["Chinese sans", "Chinese serif", "Kai & handwriting", "Monospace", "Sans serif", "Serif"]);
        assert_eq!(fonts(&zh), ["PingFang SC", "Songti SC", "\u{1}zhuque-fangsong", "Kaiti SC", "\u{1}lxgw-wenkai", "Menlo", "Helvetica", "Georgia"]);
        assert!(matches!(zh.last(), Some(Choice::Others(1, false))), "Webdings folded");
        assert!(fonts(&arrange("", true, &all, &dl, &[], &[], Lang::Zh)).contains(&"Webdings".to_string()));
        let en = arrange("", false, &all, &dl, &[], &[], Lang::En);
        assert_eq!(&fonts(&en)[..3], ["Helvetica", "Georgia", "Menlo"]);
        let top = arrange("", false, &all, &dl, &["Georgia".into()], &["Menlo".into(), "Gone".into()], Lang::Zh);
        assert_eq!(&headers(&top)[..2], ["Favorites", "Recent"]);
        assert_eq!(&fonts(&top)[..2], ["Georgia", "Menlo"]);
        let hits = arrange("song", false, &all, &dl, &[], &[], Lang::Zh);
        assert_eq!(fonts(&hits), ["Songti SC", "\u{1}zhuque-fangsong"], "Zhuque Fangsong matches too");
        assert!(!hits.contains(&Choice::Reset) && headers(&hits).is_empty());
        assert_eq!(fonts(&arrange("朱雀", false, &all, &dl, &[], &[], Lang::Zh)), ["\u{1}zhuque-fangsong"]);
    }

    /// A preferences.json from before the font settings reads with both
    /// empty: the theme's and the platform's faces.
    #[test]
    fn old_preferences_have_no_fonts() {
        let p: Preferences = serde_json::from_str(r#"{"theme":"craft","ui_font":"large"}"#).unwrap();
        assert_eq!((p.ui_family.as_str(), p.doc_family.as_str(), p.ui_font.as_str()), ("", "", "large"));
        // …and 正文字号 follows the theme.
        assert_eq!(p.doc_size, "");
    }
}
