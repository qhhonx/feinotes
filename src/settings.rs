//! 设置: preferences (`.quiet/preferences.json` in the library) and the
//! settings view — a category list in the sidebar, one page per category,
//! each page a column of grouped rows. Only what a
//! user really needs to change lives here.

use crate::{app::{View, Workspace}, theme::{self, craft, Theme}, themes, ui};
use crate::ui::{TitleBarExt, TipExt};
use crate::sidebar as sb;
use gpui::{prelude::FluentBuilder, *};
#[cfg(target_os = "ios")]
use gpui::Anchor as Corner;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, LazyLock, Mutex, RwLock};

#[cfg(target_os = "ios")]
mod mobile;

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
pub(crate) mod sync_status;
#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
mod personal_sync;

#[cfg(target_os = "ios")]
use crate::cloud_sync::shared::{self, Access};
#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
use crate::cloud_subscription::{self, Action as SubscriptionAction, ActiveSubscription, Offer, Outcome as SubscriptionOutcome, Plan};

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
#[derive(Default)]
pub(crate) struct Subscription {
    offers: Vec<Offer>,
    active: Option<ActiveSubscription>,
    busy: bool,
    show_plans: bool,
    message: Option<String>,
    refresh_pending: bool,
    trial: Option<crate::cloud_trial::Trial>,
    trial_busy: bool,
    trial_message: Option<String>,
    enable_epoch: u64,
    enable_checking: bool,
    enable_result: Option<crate::cloud_restore::EnableCheck>,
    enabling_cloud: bool,
}

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
pub enum Lang { Zh, En, Ja, Ko, Fr, Es, Ar }

impl Lang {
    pub const ALL: [Lang; 7] = [Lang::Zh, Lang::En, Lang::Ja, Lang::Ko, Lang::Fr, Lang::Es, Lang::Ar];
    /// The `language` preference value.
    pub fn code(self) -> &'static str { match self { Lang::Zh => "zh", Lang::En => "en", Lang::Ja => "ja", Lang::Ko => "ko", Lang::Fr => "fr", Lang::Es => "es", Lang::Ar => "ar" } }
    /// The language's own name, as Settings lists it.
    pub fn name(self) -> &'static str { match self { Lang::Zh => "简体中文", Lang::En => "English", Lang::Ja => "日本語", Lang::Ko => "한국어", Lang::Fr => "Français", Lang::Es => "Español", Lang::Ar => "العربية" } }
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
            Path::new("/Applications").join(&app).exists() || user.as_ref().is_some_and(|u| u.join(&app).exists())
        })
        .take(BROWSERS_SHOWN)
        .collect()
}

/// The browser a 打开链接 segment stands for (`0`: the system default, "").
pub fn browser_at(i: usize) -> String {
    i.checked_sub(1).and_then(|i| browsers().get(i).copied()).unwrap_or_default().to_string()
}
pub(crate) fn product_about_description() -> &'static str {
    tr("飞记是一款原生 Markdown 文档应用。启用 iCloud 后，支持跨设备同步和多人实时协作。",
        "Fei Notes is a native Markdown document app. Enable iCloud to sync across devices and collaborate in real time.")
}

/// Whether ⌘N should make a Markdown file: the preferred 快捷新建 type, or the
/// last one created from the 新建 menu when no fixed type is set.
pub fn quick_new_markdown() -> bool {
    if crate::FEI_NOTES { return false; }
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
    /// 打开链接: the browser a link opens in ("Safari", "Microsoft Edge"…),
    /// empty for the system default.
    pub browser: String,
    /// 外观 › 窗口背景: paint the window opaque instead of letting the desktop
    /// show through the sidebar material.
    pub opaque_window: bool,
    /// 外观 › 界面字号: "small" or "large"; empty is the 13pt standard.
    pub ui_font: String,
    /// When 稍后 put off moving the library to real folders (Unix seconds; 0 never).
    pub migrate_later: u64,
}

/// A week between asks to move the library to real folders.
const MIGRATE_ASK_EVERY: u64 = 7 * 24 * 3600;
fn unix_now() -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) }

/// 稍后 on the move to real folders: remember when.
pub fn migrate_later() {
    let mut p = prefs();
    p.migrate_later = unix_now();
    let _ = save(&p);
}

/// Whether to ask about the move now: never put off, or a week since.
pub fn migrate_ask_due() -> bool { unix_now().saturating_sub(prefs().migrate_later) >= MIGRATE_ASK_EVERY }

/// Pick the first supported system language; saved preferences still win.
fn system_lang() -> Lang {
    sys_locale::get_locales().find_map(|locale| {
        let code = locale.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
        Lang::ALL.into_iter().find(|l| l.code() == code)
    }).unwrap_or(Lang::En)
}

impl Default for Preferences {
    fn default() -> Self {
        let lang = system_lang();
        Preferences { theme: String::new(), feishu: true, english: lang != Lang::Zh, language: lang.code().into(), remote_images: true, quick_hotkey: true, auto_update: true, appearance: String::new(), seen_version: String::new(), motion: String::new(), new_default: String::new(), last_new: String::new(), search_engine: String::new(), browser: String::new(), opaque_window: false, ui_font: String::new(), migrate_later: 0 }
    }
}

impl Preferences {
    pub fn theme_id(&self) -> String {
        let id = if !self.theme.is_empty() { self.theme.as_str() } else if cfg!(target_os = "ios") { "notes" } else if self.feishu { "feishu" } else { "craft" };
        crate::themes::supported_id(id).into()
    }
    pub fn lang(&self) -> Lang {
        Lang::ALL.into_iter().find(|l| l.code() == self.language).unwrap_or(if self.english { Lang::En } else { Lang::Zh })
    }
    pub fn set_lang(&mut self, lang: Lang) {
        self.language = lang.code().into();
        self.english = lang != Lang::Zh;
    }
}

fn path() -> std::path::PathBuf { crate::store::Store::internal_dir().join("preferences.json") }

/// Show the interface in `lang` without saving it (first launch, before the library exists).
pub(crate) fn show_lang(lang: Lang) {
    let mut p = prefs();
    p.set_lang(lang);
    apply(p);
}

pub fn load() {
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    let bytes = if crate::store::Store::sqlite_session() {
        crate::store::Store::sqlite_setting("preferences").ok().flatten().or({
            #[cfg(target_os = "ios")]
            { std::fs::read(path()).ok() }
            #[cfg(not(target_os = "ios"))]
            { None }
        })
    } else { std::fs::read(path()).ok() };
    #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
    let bytes = std::fs::read(path()).ok();
    let p = bytes.and_then(|b| serde_json::from_slice::<Preferences>(&b).ok()).unwrap_or_default();
    apply(p);
}

pub(crate) fn apply(p: Preferences) {
    FEISHU.store(p.feishu, Ordering::Relaxed);
    LANG.store(p.lang() as u8, Ordering::Relaxed);
    MOTION_HIGH.store(p.motion == "high", Ordering::Relaxed);
    // Before the themes are read (startup) the base comes from `feishu`.
    themes::select(&p.theme_id());
    *PREFS.write().unwrap() = Some(p);
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
        _ => {
            #[cfg(target_os = "ios")]
            { let _ = cx; unsafe extern "C" { fn fei_ios_system_dark() -> bool; } unsafe { fei_ios_system_dark() } }
            #[cfg(not(target_os = "ios"))]
            { matches!(cx.window_appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark) }
        },
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
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    if crate::store::Store::sqlite_session() {
        crate::store::Store::write_sqlite_setting("preferences", &serde_json::to_vec(p)?)?;
        apply(p.clone());
        return Ok(());
    }
    use std::io::Write;
    let root = crate::store::Store::internal_dir();
    std::fs::create_dir_all(&root)?;
    let tmp = root.join("preferences.json.tmp");
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(&serde_json::to_vec_pretty(p)?)?;
    f.sync_all()?;
    std::fs::rename(tmp, path())?;
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
    if crate::FEI_NOTES {
        cx.defer(|cx| {
            for handle in cx.windows().into_iter().filter_map(|window| window.downcast::<Workspace>()) {
                let _ = handle.update(cx, |ws, window, _| window.set_window_title(&crate::app_window_title(ws.quick_note)));
            }
        });
    }
    cx.refresh_windows();
}

/// A theme file in the user's editor; scripted runs only log it (they must
/// not start other apps).
fn open_in_editor(p: &std::path::Path, cx: &mut App) {
    if crate::devtools::scripted() { eprintln!("devtools: open {}", p.display()); return }
    #[cfg(target_os = "ios")]
    { let _ = cx; crate::ipad::present_theme_editor(p); }
    #[cfg(not(target_os = "ios"))]
    cx.open_url(&format!("file://{}", p.display()));
}

/// Theme files are watched: saving one in an editor redraws at once.
pub(crate) fn watch_themes(cx: &mut Context<Workspace>) {
    static LAST: Mutex<Option<Vec<(std::path::PathBuf, std::time::SystemTime, u64)>>> = Mutex::new(None);
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
pub enum Page {
    #[default] General, Appearance, Shortcuts, Library,
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    SharedSync,
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    Cloud,
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    CloudAccess,
    #[cfg(all(target_os = "ios", feature = "cloudkit-sync"))]
    Shared,
    About,
}

impl Page {
    pub(crate) fn title(self) -> &'static str {
        match self {
            Page::General => tr("通用", "General"),
            Page::Appearance => tr("外观", "Appearance"),
            Page::Shortcuts => tr("快捷键", "Shortcuts"),
            Page::Library => tr("文档库", "Library"),
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            Page::SharedSync => tr("共享同步状态", "Shared sync status"),
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            Page::Cloud => "iCloud",
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            Page::CloudAccess => tr("开通飞记云功能", "Fei Notes cloud features"),
            #[cfg(all(target_os = "ios", feature = "cloudkit-sync"))]
            Page::Shared => tr("共享文档", "Shared documents"),
            Page::About => tr("关于", "About"),
        }
    }
    pub(crate) fn icon(self) -> &'static str {
        match self {
            Page::General => "view-settings",
            Page::Appearance => "palette",
            Page::Shortcuts => "keyboard",
            Page::Library => "storage",
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            Page::SharedSync => "storage",
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            Page::Cloud | Page::CloudAccess => "cloud",
            #[cfg(all(target_os = "ios", feature = "cloudkit-sync"))]
            Page::Shared => "doc",
            Page::About => "info",
        }
    }
}

/// The settings sidebar. Trash is a view, represented by `None`.
const NAV: &[Option<Page>] = &[
    Some(Page::General), Some(Page::Appearance), Some(Page::Shortcuts), Some(Page::Library),
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    Some(Page::Cloud),
    None, Some(Page::About),
];

// ---- controls (switch on the accent, 30pt buttons, grouped rows)

const COLUMN_W: f32 = 640.;

/// A titled group: the caption above, then the rows in one rounded card,
/// split by hairlines.
pub(crate) fn group(title: Option<&'static str>, rows: Vec<AnyElement>) -> Div {
    div().w_full().flex().flex_col().gap(px(8.))
        .children(title.map(caption))
        .child(div().w_full().flex().flex_col().rounded(px(12.)).border_1().border_color(Theme::line_border()).bg(crate::theme::raised(Theme::bg_body())).overflow_hidden()
            .children(rows_with_dividers(rows)))
}

fn rows_with_dividers(rows: Vec<AnyElement>) -> impl Iterator<Item = Div> {
    let n = rows.len();
    rows.into_iter().enumerate().map(move |(i, row)| div().w_full().flex().flex_col()
        .when(i + 1 < n, |d| d.border_b_1().border_color(ui::list_line())).child(row))
}

pub(crate) fn caption(t: &'static str) -> Div {
    div().px(px(4.)).text_size(px(crate::mobile::ui_font())).line_height(px(crate::mobile::ui_font() + 5.)).font_weight(FontWeight::SEMIBOLD).text_color(Theme::sidebar_section()).child(t)
}

/// One setting: title and an optional description on the left, the control on the right.
fn row(title: impl Into<SharedString>, desc: Option<SharedString>, control: impl IntoElement) -> Div {
    div().w_full().min_h(px(48.)).px(px(16.)).py(px(10.)).flex().flex_wrap().items_center().justify_between().gap_x(px(24.)).gap_y(px(8.))
        .child(div().flex_1().min_w(px(144.)).max_w_full().flex().flex_col().gap(px(2.))
            .child(div().text_size(px(crate::mobile::ui_font())).line_height(px(crate::mobile::ui_font() + 7.)).text_color(Theme::text()).child(title.into()))
            .children(desc.map(|d| div().text_size(px(crate::mobile::ui_font_small())).line_height(px(crate::mobile::ui_font_small() + 5.)).text_color(Theme::text_caption()).child(d))))
        .child(div().flex_none().max_w_full().flex().items_center().gap(px(8.)).child(control))
}

fn desc(s: &'static str) -> Option<SharedString> { Some(s.into()) }

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
fn cloud_disable_detail() -> &'static str {
    tr("切换到本机文档库，保留 iCloud 文档和待上传改动。",
        "Switch to the local library and keep your iCloud documents and pending changes.")
}

#[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
fn release_bundle_string(key: &std::ffi::CStr) -> Option<String> {
    use objc::{class, msg_send, sel, sel_impl, runtime::{BOOL, Object, YES}};
    unsafe {
        let bundle: *mut Object = msg_send![class!(NSBundle), mainBundle];
        if bundle.is_null() { return None; }
        let key: *mut Object = msg_send![class!(NSString), stringWithUTF8String: key.as_ptr()];
        if key.is_null() { return None; }
        let value: *mut Object = msg_send![bundle, objectForInfoDictionaryKey: key];
        // Missing Info.plist keys return nil, which is not a valid objc message receiver in Rust.
        if value.is_null() { return None; }
        let is_string: BOOL = msg_send![value, isKindOfClass: class!(NSString)];
        if is_string != YES { return None; }
        let text: *const std::ffi::c_char = msg_send![value, UTF8String];
        if text.is_null() { return None; }
        std::ffi::CStr::from_ptr(text).to_str().ok().map(str::to_owned)
    }
}

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
fn release_url(key: &std::ffi::CStr) -> Option<String> {
    release_bundle_string(key).filter(|url| url.starts_with("https://"))
}

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
fn about_public_links() -> [(&'static str, &'static str, &'static str); 2] {
    [("about-website", tr("官方网站", "Official Website"), "https://feinotes.app/"),
     ("about-feedback", tr("Telegram 反馈群", "Telegram Feedback Group"), "https://t.me/+eKIYrdIFufxmYzJl")]
}

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
fn setting_status(label: &'static str) -> Div {
    let (font_size, line_height) = if cfg!(target_os = "ios") { (14., 21.) } else { (12., 17.) };
    div().text_size(px(font_size)).line_height(px(line_height)).text_color(Theme::text_caption()).child(label)
}

/// The switch: the accent when on, black 16% when off, a white knob.
pub(crate) fn switch(id: impl Into<ElementId>, on: bool) -> Stateful<Div> {
    div().id(id).flex_none().w(px(if cfg!(target_os = "ios") { 51. } else { 32. })).h(px(if cfg!(target_os = "ios") { 31. } else { 19. })).p(px(2.)).rounded_full().cursor_pointer().flex().items_center()
        .when(on, |d| d.bg(Theme::primary()).justify_end())
        .when(!on, |d| d.bg(theme::ca(craft::N1000, 0.16)).justify_start())
        .child(div().size(px(if cfg!(target_os = "ios") { 27. } else { 15. })).rounded_full().bg(gpui::white()).shadow(theme::shadow_button()))
}

/// 外观 › 主题 CSS (duplicate, edit, restore a theme's stylesheet): hidden in
/// this release while editing themes by hand settles.
const THEME_CSS: bool = false;

/// An open dropdown on 通用 (see `Workspace::select`): its choices, which one
/// is picked and which is highlighted, and what picking one does.
pub struct PrefMenu {
    id: &'static str,
    labels: Vec<&'static str>,
    selected: usize,
    focused: usize,
    on: std::rc::Rc<dyn Fn(&mut Workspace, usize, &mut Context<Workspace>)>,
}

/// A segmented control: a grey groove and a white selected pill.
fn segmented(id: &'static str, labels: &[&'static str], selected: usize, on: impl Fn(&mut Workspace, usize, &mut Context<Workspace>) + Clone + 'static, cx: &mut Context<Workspace>) -> Div {
    div().flex_none().h(px(crate::mobile::control(28.))).p(px(2.)).flex().gap(px(2.)).rounded(px(9.)).bg(Theme::fill_hover_strong())
        .children(labels.iter().enumerate().map(|(i, label)| {
            let on = on.clone();
            div().id((id, i)).h_full().px(px(12.)).flex().items_center().justify_center().rounded(px(7.)).cursor_pointer()
                .text_size(px(crate::mobile::ui_font())).line_height(px(crate::mobile::ui_font() + 5.))
                // In dark mode the picked segment is lighter than the groove (macOS: #636366).
                .when(i == selected, |d| d.bg(if theme::dark() { theme::rgbaf(0x63, 0x63, 0x66, 1.) } else { Theme::bg_body() }).shadow(theme::shadow_button()).text_color(Theme::text()))
                .when(i != selected, |d| d.text_color(Theme::text_caption()).hover(|s| s.text_color(Theme::text())))
                .child(*label)
                .on_click(cx.listener(move |w, _, _, cx| on(w, i, cx)))
        }))
}

/// A keyboard shortcut chip; `recording` shows it waiting for keys.
pub(crate) fn keycap(id: impl Into<ElementId>, key: &str, recording: bool) -> Stateful<Div> {
    div().id(id).flex_none().min_w(px(144.)).min_h(px(28.)).px(px(14.)).py(px(4.)).flex().items_center().justify_center().rounded(px(7.)).cursor_pointer()
        .text_size(px(crate::mobile::ui_font_small())).line_height(px(crate::mobile::ui_font_small() + 4.))
        .when(recording, |d| d.bg(crate::theme::raised(Theme::bg_body())).border_1().border_color(Theme::primary()).text_color(Theme::primary()).child(if key.is_empty() { tr("按下新的组合键…", "Press keys…").to_string() } else { crate::shortcuts::pretty(key) }))
        .when(!recording, |d| d.bg(Theme::fill_hover()).border_1().border_color(transparent_black()).text_color(Theme::text()).hover(|s| s.bg(Theme::fill_hover_strong())).child(crate::shortcuts::pretty(key)))
}

/// A small text button (恢复默认 …).
pub(crate) fn link_button(id: impl Into<ElementId>, label: &'static str) -> Stateful<Div> {
    div().id(id).flex_none().h(px(26.)).px(px(10.)).flex().items_center().rounded(px(7.)).cursor_pointer()
        .text_size(px(crate::mobile::ui_font_small())).line_height(px(crate::mobile::ui_font_small() + 4.)).text_color(Theme::primary()).hover(|s| s.bg(Theme::fill_hover()))
        .child(label)
}

pub(crate) fn button(id: impl Into<ElementId>, label: &'static str) -> Stateful<Div> {
    setting_button(id,label,true)
}

fn setting_button(id: impl Into<ElementId>, label: &'static str, enabled: bool) -> Stateful<Div> {
    div().id(id).flex_none().h(px(crate::mobile::control(28.))).px(px(12.)).flex().items_center().justify_center().rounded(px(8.))
        .when(cfg!(target_os = "ios"), |d| d.min_w(px(44.)))
        .bg(crate::theme::raised(Theme::bg_body())).border_1().border_color(Theme::button_border())
        .when(enabled, |d| d.cursor_pointer().shadow(theme::shadow_button())
            .hover(|s| s.bg(theme::c(craft::N100))).active(|s| s.bg(theme::c(craft::N200))))
        .text_size(px(crate::mobile::ui_font())).line_height(px(crate::mobile::ui_font() + 5.))
        .text_color(if enabled { Theme::text() } else { Theme::text_disabled() }).child(label)
}

// ---- theme preview

struct ThemeSample {
    title: &'static str,
    body: &'static str,
    highlight: &'static str,
    heading: &'static str,
    completed: &'static str,
    pending: &'static str,
}

fn theme_sample() -> ThemeSample {
    ThemeSample {
        title: tr("海边小书展", "Seaside Book Fair"),
        body: tr("去旧码头的路上没信号，先把雨天动线写下来。", "No signal on the way to the old pier. I’ll sketch the rain route here."),
        highlight: tr("周五下午一起走场。", "Walk through the space together on Friday afternoon."),
        heading: tr("走场前", "Before the walkthrough"),
        completed: tr("回到 Mac，接着补上摊位图", "Pick up on the Mac and add the stall map"),
        pending: tr("邀请陈屿一起改入口说明", "Invite Chen to revise the entrance directions together"),
    }
}

fn branded_sample_page(st: &themes::Style) -> Div {
    use crate::editor_theme::{body_line_for, body_size_for, heading_metrics_for, title_for};

    let sample = theme_sample();
    let ink = st.color("page.color").unwrap_or_else(|| st.token("text"));
    let paper = st.color("page.bg").unwrap_or_else(|| st.token("bg_body"));
    let family = SharedString::from(st.font("page.font"));
    let size = body_size_for(st);
    let line = body_line_for(st);
    let title = title_for(st);
    let heading = heading_metrics_for(st, 2);
    let strong = st.color("strong.color").unwrap_or(ink);
    let mut mark_font = font(family.clone());
    mark_font.weight = FontWeight(st.num("strong.weight"));
    let fill = st.named("highlight", "light-yellow");
    let highlight = StyledText::new(sample.highlight).with_runs(vec![TextRun {
        len: sample.highlight.len(), font: mark_font,
        color: if fill.is_some() { theme::on_mark(strong) } else { strong },
        background_color: fill, underline: None, strikethrough: None,
    }]);

    let feishu = st.feishu;
    let box_size = if feishu { 16. } else { st.num("task.size") };
    let radius = if feishu { 4. } else { st.num("task.radius") };
    let circular = radius >= box_size / 2. - 0.1;
    let fill = st.color("task.checked").unwrap_or_else(|| st.token("primary"));
    let check = if feishu { crate::feishu_theme::Theme::primary_500() } else { st.color("task.check").unwrap_or_else(gpui::white) };
    let border = if feishu { crate::feishu_theme::Theme::text() } else { st.token("todo_box") };
    let empty = theme::raised(st.token("bg_body"));
    let done_ink = st.token("todo_done");
    let task = |done, label| {
        let filled = done && !feishu;
        let icon_size = if feishu { 14. } else if circular { box_size } else { (box_size * 13. / 17.).round() };
        div().flex().gap(px(8.)).items_start()
            .child(div().flex_none().size(px(box_size)).mt(px(((line - box_size) / 2.).max(0.))).rounded(px(radius))
                .border_1().border_color(if filled { fill } else { border }).bg(if filled { fill } else { empty })
                .flex().items_center().justify_center()
                .when(done, |d| d.child(ui::icon(if circular { "notes-todo-check" } else { "todo-check" }, icon_size, check))))
            .child(div().min_w_0().flex_1().when(done, |d| d.text_color(done_ink).when(feishu || st.flag("done.strike"), |d| d.line_through())).child(label))
    };
    div().w_full().min_w_0().flex().justify_center().bg(paper).font_family(family)
        .text_color(ink).text_size(px(size)).line_height(px(line))
        .child(div().w_full().min_w_0().max_w(px(st.num("page.width").clamp(240., 760.))).px(px(24.)).pt(px(22.)).pb(px(24.)).flex().flex_col()
            .child(div().font_family(SharedString::from(st.font("title.font"))).text_size(px(title.size)).line_height(px(title.line_height))
                .font_weight(FontWeight(st.num("title.weight"))).text_color(st.color("title.color").unwrap_or(ink))
                .mb(px(st.num("title.gap"))).child(sample.title))
            .child(div().mb(px(st.num("p.gap"))).child(sample.body))
            .child(div().child(highlight))
            .child(div().mt(px(heading.mt)).mb(px(heading.mb)).font_family(SharedString::from(st.font("h2.font")))
                .text_size(px(heading.size)).line_height(px(heading.line_height)).font_weight(FontWeight(st.num("h2.weight")))
                .text_color(st.color("h2.color").unwrap_or(ink)).child(sample.heading))
            .child(div().flex().flex_col().gap(px(st.todo_gap.unwrap_or_else(|| st.list_gap())))
                .child(task(true, sample.completed)).child(task(false, sample.pending))))
}

/// Preview scale against the real document. Its frame clips to a rectangle
/// (GPUI masks have no radius), so the page is rounded inside the frame.
const PAGE_S: f32 = 0.78;

/// 外观's sample page: a short document with most of what a theme styles
/// (headings, inline styles, lists, a task, quote, callout, table, divider,
/// highlighted code), at scale `s`, in two columns so it reads at a glance.
fn sample_page(t: &themes::Theme, s: f32) -> Div {
    let st = t.style;
    let sc = |v: f32| px((v * s).max(0.5));
    let text = st.color("page.color").unwrap_or_else(|| st.token("text"));
    let (body, line) = (st.num("page.size"), st.num("page.line"));
    let (font, mono): (SharedString, SharedString) = (st.font("page.font").into(), st.font("pre.font").into());
    let gap = sc(st.num("p.gap")).clamp(px(4.), px(9.));
    let p = |c: Div| c.text_size(sc(body)).line_height(sc(line));
    let heading = |l: u8, s: &'static str| {
        let h = format!("h{l}");
        div().text_size(sc(st.num(&format!("{h}.size")))).line_height(sc(st.num(&format!("{h}.line")))).font_weight(FontWeight(st.num(&format!("{h}.weight"))))
            .text_color(st.color(&format!("{h}.color")).unwrap_or(text)).child(s)
    };
    let link = st.color("a.color").unwrap_or(text);
    let chip = |s: &'static str| div().px(sc(st.num("code.pad")).max(px(2.))).rounded(sc(st.num("code.radius"))).bg(st.color("code.bg").unwrap_or_else(|| st.token("code_chip")))
        .border_1().border_color(st.color("code.border").unwrap_or_else(|| st.token("code_chip_border"))).font_family(mono.clone()).text_color(st.color("code.color").unwrap_or(text)).child(s);
    let para = p(div().flex().flex_wrap().items_center())
        .child(format!("{}{}", crate::app_name(), tr(" 是一款", " is a ")))
        .child(div().font_weight(FontWeight(st.num("strong.weight"))).text_color(st.color("strong.color").unwrap_or(text)).child(tr("安静的本地文档应用", "quiet, local document app")))
        .child(tr("，", ", "))
        .child(if crate::FEI_NOTES { tr("离线也能写作。", "write offline. ") }
            else { tr("文档就是电脑上的文件。", "your documents are files. ") })
        .child(tr("输入 ", "Type "))
        .child(chip("/"))
        .child(tr(" 插入内容，", " to insert, "))
        .child(tr("详见", "see "))
        // The full stop rides with the link, so it never wraps alone.
        .child(div().flex().child(div().text_color(link).when(st.flag("a.underline"), |d| d.underline()).child(tr("写作基础", "Writing basics"))).child(tr("。", ".")));
    let indent = sc(st.num("list.indent")).max(px(10.));
    let marker = |m: Div| div().flex_none().w(indent).flex().justify_center().child(m);
    let bullet_c = st.color("bullet.color").unwrap_or(text);
    let bullet = |s: &'static str| {
        let mark = div().w(sc(6.).max(px(3.))).h(if st.flag("list.dash") { sc(1.) } else { sc(6.).max(px(3.)) }).rounded_full().bg(bullet_c);
        div().flex().items_center().child(marker(mark)).child(p(div()).child(s))
    };
    let num_c = st.color("number.color").unwrap_or(text);
    let number = |n: &'static str, s: &'static str| div().flex().items_center().child(marker(p(div()).text_color(num_c).child(n))).child(p(div()).child(s));
    let task_size = st.num("task.size");
    let task = |done: bool, label: &'static str| div().flex().items_center().child(marker(
            div().size(sc(task_size)).rounded(sc(st.num("task.radius"))).flex().items_center().justify_center()
                .when(done, |d| d.bg(st.color("task.checked").unwrap_or_else(|| st.token("primary")))
                    .child(ui::icon(if st.num("task.radius") >= task_size / 2. - 0.1 { "notes-todo-check" } else { "todo-check" }, if st.num("task.radius") >= task_size / 2. - 0.1 { task_size * s } else { (task_size * s * 0.75).round() }, st.color("task.check").unwrap_or_else(gpui::white))))
                .when(!done, |d| d.border_1().border_color(st.color("task.border").unwrap_or_else(|| st.token("todo_box"))))))
        .child(p(div()).child(label).when(done, |d| d.text_color(st.color("done.color").unwrap_or_else(|| st.token("todo_done"))).when(st.flag("done.strike"), |d| d.line_through())));
    let quote = div().flex().gap(sc((st.num("quote.indent") - st.num("quote.bar")).max(4.)))
        .child(div().flex_none().w(sc(st.num("quote.bar")).max(px(1.))).rounded(px(1.)).bg(st.color("quote.bar-color").unwrap_or_else(|| st.token("quote_bar"))))
        .child(p(div()).text_color(st.color("quote.color").unwrap_or(text)).when(st.flag("quote.italic"), |d| d.italic()).child(if crate::FEI_NOTES {
            tr("这是一篇示例文档，可以自由修改。", "This is a sample document. Edit it freely.")
        } else { tr("每一篇指南都是用 Quiet.md 写成的，放心修改。", "Every guide is written in Quiet.md. Edit them freely.") }));
    let callout = div().px(sc(st.num("callout.pad-x"))).py(sc(st.num("callout.pad-y"))).rounded(sc(st.num("callout.radius")))
        .bg(st.named("callout", "light-yellow").unwrap_or_else(|| st.token("code_chip")))
        .when(st.num("callout.border") > 0., |d| d.border(sc(st.num("callout.border")).max(px(1.))).border_color(st.named("border", "yellow").unwrap_or_else(|| st.token("code_chip_border"))))
        .child(p(div().flex().gap(px(6.))).child("💡").child(tr("一边读一边动手试，记得最快。", "Try things as you read; it sticks faster.")));
    let line_c = st.color("table.line").unwrap_or_else(|| st.token("table_line"));
    let (cell_x, cell_y) = (px(st.num("table.pad-x") * 0.6), px(st.num("table.pad-y") / 3.));
    let cell = |s: &'static str, head: bool| div().flex_1().px(cell_x).py(cell_y).when(head, |d| d.bg(st.color("th.bg").unwrap_or_else(|| st.token("table_header"))).font_weight(FontWeight::SEMIBOLD)).child(s);
    let row = |a: &'static str, b: &'static str, head: bool, last: bool| div().flex().when(!last, |d| d.border_b_1().border_color(line_c))
        .child(cell(a, head).border_r_1().border_color(line_c)).child(cell(b, head));
    let table = div().w_full().rounded(sc(st.num("table.radius"))).border_1().border_color(line_c).overflow_hidden()
        .text_size(sc(st.num("table.size"))).line_height(sc(st.num("table.line-height")))
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
        .font_family(mono.clone()).text_size(sc(st.num("pre.size"))).line_height(sc(st.num("pre.line"))).text_color(st.color("pre.color").unwrap_or_else(|| st.token("code_text")))
        .child(code_line("1", vec![tok("# ", syn("comment")), tok(tr("备份文档库", "back up your documents"), syn("comment"))]))
        .child(code_line("2", vec![tok("cd ", syn("function")), tok("~/Documents ", text), tok("&& ", syn("keyword")), tok("tar ", syn("function")), tok("-czf ", syn("attr")), tok("\"Quiet.tar.gz\" ", syn("string")), tok("Quiet.md", text)]))
        .child(code_line("3", vec![tok("echo ", syn("function")), tok("\"done\"", syn("string"))]));
    // One column, like a page: a centred text column (the theme's width, scaled,
    // up to 760 so a wide frame isn't mostly margin) whose bottom fades into
    // the paper. Blocks come in the order the 开始指南 itself uses.
    let paper = st.color("page.bg").unwrap_or_else(|| st.token("bg_body"));
    // Its own bottom corners too: nothing clips it to the page's radius.
    let fade = div().absolute().left_0().right_0().bottom_0().h(px(56.)).rounded_b(px(7.)).bg(linear_gradient(180., linear_color_stop(paper.opacity(0.), 0.), linear_color_stop(paper, 1.)));
    // items_start: a column stretched to the frame would shrink its blocks to fit.
    div().relative().size_full().flex().justify_center().items_start().bg(paper).font_family(font).text_color(text).overflow_hidden()
        // min_w_0: the column follows the frame, not its widest unbreakable line.
        .child(div().w_full().min_w_0().overflow_hidden().max_w((sc(st.num("page.width")) * 1.35).min(px(760.))).px(px(32.)).pt(px(22.)).flex().flex_col().gap(gap)
            .child(div().text_size(sc(st.num("title.size"))).line_height(sc(st.num("title.size") * 1.25)).font_weight(FontWeight(st.num("title.weight")))
                .font_family(SharedString::from(st.font("title.font"))).text_color(st.color("title.color").unwrap_or(text)).child(tr("开始指南", "Getting started")))
            .child(para)
            .child(callout)
            .child(heading(1, tr("五分钟上手", "Five minutes")))
            .child(div().flex().flex_col().child(task(true, tr("打开这份指南", "Open this guide"))).child(task(false, crate::shortcuts::keys(tr("按 ⌘ N 新建一篇文档", "Press ⌘ N for a new document")))))
            .child(heading(2, tr("三种输入方式", "Three ways to write")))
            .child(div().flex().flex_col().child(bullet(tr("斜杠菜单：输入 / 插入任意内容块", "Slash menu: type / to insert any block"))).child(bullet(tr("Markdown：# 加空格变标题", "Markdown: # and a space make a heading"))))
            .child(div().flex().flex_col().child(number("1.", tr("先读「写作基础」", "Read Writing basics first"))).child(number("2.", tr("再看「内容块」", "Then Content blocks"))))
            .child(quote)
            .child(heading(3, tr("最常用的快捷键", "Shortcuts to know")))
            .child(table)
            .child(div().w_full().py(px(2.)).child(hr))
            .child(code))
        .child(fade)
}

fn badge(t: &themes::Theme) -> Option<&'static str> {
    if t.customized() { Some(tr("已修改", "Edited")) } else if t.bundled.is_none() { Some(tr("自定义", "Custom")) } else { None }
}

/// A pop-up button (macOS): the current choice and a chevron on `button`'s white pill.
fn popup_button(id: impl Into<ElementId>, label: impl Into<SharedString>, open: bool) -> Stateful<Div> {
    div().id(id).flex_none().max_w(px(220.)).h(px(crate::mobile::control(28.))).pl(px(12.)).pr(px(8.)).flex().items_center().gap(px(6.)).rounded(px(8.)).cursor_pointer()
        .bg(if open { theme::c(craft::N100) } else { Theme::bg_body() }).border_1().border_color(Theme::button_border()).shadow(theme::shadow_button())
        .hover(|s| s.bg(theme::c(craft::N100))).active(|s| s.bg(theme::c(craft::N200)))
        .text_size(px(crate::mobile::ui_font())).line_height(px(crate::mobile::ui_font() + 5.)).text_color(Theme::text())
        .child(div().min_w_0().truncate().child(label.into()))
        .child(ui::icon("chevron-down", 12., Theme::icon()))
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
            ui::menu_row(g.clone(), g.clone())
                .when(&t.id == focused, |d| d.bg(Theme::fill_hover()))
                .child(div().flex_1().min_w_0().overflow_hidden().child(ui::menu_label(t.name().to_string(), g.clone(), gpui::white())))
                .children(badge(t).map(|b| div().flex_none().text_size(px(11.)).text_color(Theme::text_caption()).child(ui::menu_label(b, g.clone(), gpui::white()))))
                .child(div().flex_none().w(px(14.)).when(t.id == current, |d| d.child(ui::menu_glyph("check", 14., g.clone()))))
                .on_hover(cx.listener(move |w, hovered: &bool, _, cx| if *hovered && w.theme_menu.as_ref() != Some(&hover_id) { w.theme_menu = Some(hover_id.clone()); cx.notify(); }))
                .on_click(cx.listener(move |w, _, _, cx| apply_theme(w, id.clone(), cx)))
                .into_any_element()
        }).collect();
        drop(list);
        let anchor = self.theme_anchor.clone();
        let menu = ui::popover().id("theme-menu").occlude().flex()
            .on_mouse_down_out(cx.listener(move |w, ev: &MouseDownEvent, _, cx| {
                // A press on the button itself: its click closes the menu.
                if anchor.get().is_some_and(|b| b.contains(&ev.position)) { return }
                w.theme_menu = None;
                cx.notify();
            }))
            .child(div().id("theme-menu-list").w(px(200.)).max_h(px(302.)).overflow_y_scroll().py(px(ui::MENU_PAD_Y)).flex().flex_col().children(rows));
        Some(deferred(anchored().anchor(Corner::TopRight).position(point(b.right(), b.bottom() + px(4.))).snap_to_window_with_margin(px(8.)).child(menu)).with_priority(2).into_any_element())
    }

    /// A choice as a dropdown button, like 界面语言's: the picked label, a menu of the rest.
    fn select(&self, id: &'static str, labels: Vec<&'static str>, selected: usize, on: impl Fn(&mut Workspace, usize, &mut Context<Workspace>) + 'static, cx: &mut Context<Self>) -> Stateful<Div> {
        let open = self.pref_menu.as_ref().is_some_and(|m| m.id == id);
        let anchors = self.pref_anchors.clone();
        let on: std::rc::Rc<dyn Fn(&mut Workspace, usize, &mut Context<Workspace>)> = std::rc::Rc::new(on);
        popup_button(id, labels.get(selected).copied().unwrap_or_default(), open)
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
        if self.view != View::Settings || self.settings_page != Page::General { return None }
        let b = *self.pref_anchors.borrow().get(m.id)?;
        let rows = m.labels.iter().enumerate().map(|(i, label)| {
            let g = SharedString::from(format!("{}-opt-{i}", m.id));
            ui::menu_row(g.clone(), g.clone())
                .h(px(crate::mobile::control(ui::MENU_ROW_H)))
                .when(i == m.focused, |d| d.bg(Theme::fill_hover()))
                .child(div().flex_1().min_w_0().overflow_hidden().child(ui::menu_label(*label, g.clone(), gpui::white())))
                .child(div().flex_none().w(px(14.)).when(i == m.selected, |d| d.child(ui::menu_glyph("check", 14., g.clone()))))
                .on_hover(cx.listener(move |w, hovered: &bool, _, cx| if *hovered && let Some(m) = &mut w.pref_menu && m.focused != i { m.focused = i; cx.notify(); }))
                .on_click(cx.listener(move |w, _, _, cx| w.pick_pref(i, cx)))
        }).collect::<Vec<_>>();
        let anchors = self.pref_anchors.clone();
        let id = m.id;
        let menu = ui::popover().id("pref-menu").occlude().flex()
            .on_mouse_down_out(cx.listener(move |w, ev: &MouseDownEvent, _, cx| {
                if anchors.borrow().get(id).is_some_and(|b| b.contains(&ev.position)) { return }
                w.pref_menu = None;
                cx.notify();
            }))
            .child(div().min_w(px(160.)).py(px(ui::MENU_PAD_Y)).flex().flex_col().children(rows));
        Some(deferred(anchored().anchor(Corner::TopRight).position(point(b.right(), b.bottom() + px(4.))).snap_to_window_with_margin(px(8.)).child(menu)).with_priority(2).into_any_element())
    }

    fn pick_pref(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(m) = self.pref_menu.take() else { return };
        (m.on)(self, i, cx);
        cx.notify();
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
            ui::menu_row(g.clone(), g.clone())
                .h(px(crate::mobile::control(ui::MENU_ROW_H)))
                .when(l == focused, |d| d.bg(Theme::fill_hover()))
                .child(div().flex_1().min_w_0().overflow_hidden().child(ui::menu_label(l.name(), g.clone(), gpui::white())))
                .child(div().flex_none().w(px(14.)).when(l == current, |d| d.child(ui::menu_glyph("check", 14., g.clone()))))
                .on_hover(cx.listener(move |w, hovered: &bool, _, cx| if *hovered && w.lang_menu != Some(l) { w.lang_menu = Some(l); cx.notify(); }))
                .on_click(cx.listener(move |w, _, _, cx| { w.lang_menu = None; w.set_pref(|p| p.set_lang(l), cx) }))
        });
        let anchor = self.lang_anchor.clone();
        let menu = ui::popover().id("lang-menu").occlude().flex()
            .on_mouse_down_out(cx.listener(move |w, ev: &MouseDownEvent, _, cx| {
                if anchor.get().is_some_and(|b| b.contains(&ev.position)) { return }
                w.lang_menu = None;
                cx.notify();
            }))
            .child(div().id("lang-menu-list").w(px(160.)).py(px(ui::MENU_PAD_Y)).flex().flex_col()
                .when(cfg!(target_os = "ios"), |d| d.max_h(px(360.)).overflow_y_scroll()).children(rows));
        Some(deferred(anchored().anchor(Corner::TopRight).position(point(b.right(), b.bottom() + px(4.))).snap_to_window_with_margin(px(8.)).child(menu)).with_priority(2).into_any_element())
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
        #[cfg(target_os = "macos")]
        if p.appearance != old.appearance {
            // AppKit can notify each window synchronously. Run this outside
            // the GPUI input update so those callbacks can refresh the windows.
            cx.spawn(async |_, _| apply_appearance(&prefs().appearance)).detach();
        }
        if p.theme_id() != old.theme_id() || p.lang() != old.lang() || sync_dark(cx) {
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

    pub(crate) fn open_global_settings(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(target_os = "ios")]
        self.mobile_open_settings(cx);
        #[cfg(not(target_os = "ios"))]
        {
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            if self.settings_page == Page::SharedSync {
                self.open_settings(Page::Library, cx);
                self.refresh_library_cloud(_window, cx);
                return;
            }
            self.show_view(View::Settings, cx);
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            self.refresh_library_cloud(_window, cx);
        }
    }

    pub(crate) fn open_settings(&mut self, page: Page, cx: &mut Context<Self>) {
        self.cancel_shortcut_recording(cx);
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if page != self.settings_page {
            self.sync_issues = None;
            if !matches!(page, Page::Cloud | Page::CloudAccess) { self.cancel_cloud_enable(); }
        }
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
        let current = (self.view == View::Settings).then_some(self.settings_page);
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let current = if current == Some(Page::CloudAccess) { Some(Page::Cloud) } else { current };
        let in_trash = self.view == View::Trash;
        let nav_row = |page: Option<Page>, cx: &mut Context<Self>| {
            let on = if page.is_some() { page == current } else { in_trash };
            let (title, icon) = page.map_or((tr("回收站", "Trash"), "bin"), |p| (p.title(), p.icon()));
            div().id(SharedString::from(format!("settings-nav-{page:?}"))).h(px(sb::ROW_H)).flex().items_center().pl(px(6.)).pr(px(4.)).rounded(px(sb::ROW_R))
                .text_size(px(crate::mobile::ui_font())).line_height(px(crate::mobile::ui_font() + 7.)).cursor_pointer()
                .when(on, |d| d.bg(sb::selection_fill()).text_color(Theme::text()))
                .when(!on, |d| d.text_color(sb::text_color()).hover(|s| s.bg(sb::hover())))
                .child(div().size(px(20.)).flex_none().flex().items_center().justify_center().mr(px(9.)).child(ui::icon(icon, sb::ROW_ICON, Theme::icon())))
                .child(title)
                .on_click(cx.listener(move |w, _, _window, cx| match page {
                    Some(p) => {
                        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                        { w.cloud_settings_from_share = false; }
                        w.open_settings(p, cx);
                        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                        w.refresh_library_cloud(_window, cx);
                    }
                    None => w.open_settings_trash(cx),
                }))
        };
        div().w(px(self.sidebar_w(window))).h_full().flex_none().flex().flex_col().text_color(Theme::text()).font_family(theme::UI_FONT)
            .when_some(sb::surface(), |d, c| d.bg(c))
            .child(div().h(px(crate::app::HEADER_H)).flex_none().flex().items_center().pl(px(theme::TRAFFIC_LIGHTS_W)).pr(px(12.)).title_bar().child(self.header_navigation(cx))
                .child(div().flex_1()).children(update_pill(cx)))
            // The document sidebar's rhythm: 主页 sits 8 under the bar, a section
            // header like 我的文档's, rows on a 31 pitch. 设置's line sits on
            // 主页's, not a section header's: the 34pt box keeps the rows below.
            .child(div().pt(px(8.)).px(px(sb::SB_X)).flex().flex_col()
                .child(div().h(px(sb::SECTION_H)).pl(px(4.)).pt(px((sb::SECTION_TOP + sb::SECTION_H) / 2. - sb::ROW_H / 2.))
                    .child(div().px(px(4.)).text_size(px(crate::mobile::ui_font())).line_height(px(crate::mobile::ui_font() + 7.)).font_weight(FontWeight::SEMIBOLD).text_color(sb::section_color()).child(tr("设置", "Settings"))))
                .child(div().flex().flex_col().gap(px(sb::ROW_GAP)).children(NAV.iter().map(|p| nav_row(*p, cx)).collect::<Vec<_>>())))
            // Where 设置 was: 返回, back to the page 设置 was opened from.
            .child(div().flex_1())
            .child(sb::footer_slot(sb::footer_row("ft-back", "arrow-left", tr("返回", "Back")).tip(tr("返回", "Back"), Some("Esc"))
                .on_click(cx.listener(|w, _, _, cx| w.close_settings(cx)))))
            .into_any_element()
    }

    /// 收起 / 展开侧栏 and 主页, in the very box the document sidebar's
    /// 收起侧栏 uses (28pt, 4pt past the traffic lights, glyph 17), so the
    /// buttons don't move when switching between the document list and 设置.
    pub(crate) fn header_navigation(&self, cx: &mut Context<Self>) -> AnyElement {
        let open = self.sidebar_open;
        let bar_button = |id: &'static str, glyph: &'static str| {
            ui::toolbar_control(div().id(id).no_drag().flex_none().size(px(28.)).flex().items_center().justify_center().cursor_pointer(),
                8., if open { sb::hover } else { Theme::fill_hover_strong }, None)
                .child(ui::toolbar_icon(glyph, 17.))
        };
        div().flex().items_center().ml(px(4.)).gap(px(4.))
            .child(bar_button("settings-collapse", if open { "panel-left-close" } else { "panel-left-open" })
                .tip(if open { tr("收起侧栏", "Hide sidebar") } else { tr("展开侧栏", "Show sidebar") }, Some(crate::shortcuts::hint("workspace::ToggleSidebar", "⌘ + ]")))
                .on_click(cx.listener(|w, _, _, cx| { w.sidebar_open = !w.sidebar_open; cx.notify(); })))
            .child(bar_button("settings-home", "home").tip(tr("返回主页", "Home"), Some(crate::shortcuts::hint("workspace::GoHome", "⌘ + ⌥ + Home")))
                .on_click(cx.listener(|w, _, window, cx| w.navigate_home(window, cx))))
            .into_any_element()
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    pub(crate) fn cancel_cloud_enable(&mut self) {
        self.subscription.enable_epoch += 1;
        self.subscription.enable_checking = false;
        self.subscription.enable_result = None;
        self.subscription.enabling_cloud = false;
    }

    /// Back to where 设置 was opened from.
    pub(crate) fn close_settings(&mut self, cx: &mut Context<Self>) {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if self.settings_page == Page::CloudAccess {
            self.cancel_cloud_enable();
            #[cfg(target_os = "ios")]
            { self.mobile_settings_parent = None; }
            return self.open_settings(Page::Cloud, cx);
        }
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let restore_shared_sync = self.take_cloud_settings_return();
        #[cfg(not(target_os = "ios"))]
        let back = self.settings_return;
        #[cfg(target_os = "ios")]
        let back = self.mobile_settings_return;
        self.show_view(back, cx);
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        self.restore_shared_sync(restore_shared_sync,cx);
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    pub(crate) fn take_cloud_settings_return(&mut self) -> bool {
        if matches!(self.settings_page,Page::Cloud | Page::CloudAccess) { self.cancel_cloud_enable(); }
        std::mem::take(&mut self.cloud_settings_from_share)
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    pub(crate) fn restore_shared_sync(&mut self, restore: bool, cx: &mut Context<Self>) {
        if restore && matches!(self.view,View::Doc | View::Folder) && self.current_room(cx).is_some() {
            self.sharing.sync_panel_open = true;
            cx.notify();
        }
    }

    pub(crate) fn render_settings(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let page = self.settings_page;
        let body = match page {
            Page::General => self.general_page(cx),
            Page::Appearance => self.appearance_page(cx),
            Page::Shortcuts => self.shortcut_settings(cx),
            Page::Library => self.library_page(cx),
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            Page::SharedSync => {
                let mut rows = vec![cloud_sync_row(self, self.store.root(), cx)];
                rows.extend(self.sync_issues_rows(self.store.root(), cx));
                group(None, rows).into_any_element()
            }
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            Page::Cloud => div().flex().flex_col().gap(px(8.))
                .when(self.cloud_settings_from_share,|d|d.child(self.cloud_back_row(tr("‹ 共享同步状态","‹ Shared sync status"),cx)))
                .child(self.cloud_rows(cx)).into_any_element(),
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            Page::CloudAccess => div().flex().flex_col().gap(px(8.)).child(self.cloud_back_row("‹ iCloud",cx))
                .child(group(None, if cloud_subscription::available() { self.subscription_rows(cx) }
                    else { vec![self.trial_row(cx).into_any_element()] })).into_any_element(),
            #[cfg(all(target_os = "ios", feature = "cloudkit-sync"))]
            Page::Shared => div().into_any_element(),
            Page::About => self.about_page(cx),
        };
        let content_width = f32::from(window.viewport_size().width)
            - if self.sidebar_open { self.sidebar_w(window) } else { 0. };
        let margin = if content_width < 400. { 16. } else { 32. };
        let compact_title = cfg!(all(target_os = "macos", feature = "cloudkit-sync")) && page == Page::Library;
        div().flex_1().min_w_0().h_full().flex().flex_col().bg(Theme::bg_body()).text_color(Theme::text()).font_family(theme::UI_FONT)
            // The bar only drags the window; the page title heads the centred column.
            .child(div().h(px(crate::app::HEADER_H)).flex_none().flex().items_center().title_bar()
                .when_some(ui::toolbar_color("toolbar.bg"), |d, c| d.bg(c))
                .when(!self.sidebar_open, |d| d.pl(px(theme::TRAFFIC_LIGHTS_W)).child(self.header_navigation(cx))))
            .child(div().id("settings-content").flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.settings_scroll)
                .child(div().w_full().flex().flex_col().items_center().px(px(margin)).pb(px(64.))
                    .child(div().w_full().max_w(px(if page == Page::Shortcuts { 760. } else { COLUMN_W })).flex().flex_col().gap(px(28.))
                        .when(compact_title, |d| d.gap(px(16.)))
                        .child(div().flex().items_center().gap(px(16.))
                            .child(div().flex_1().min_w_0().px(px(4.)).text_size(px(if compact_title { 17. } else { 22. }))
                                .line_height(px(if compact_title { 24. } else { 28. })).font_weight(FontWeight::SEMIBOLD).child(page.title()))
                            .when(page == Page::Shortcuts, |d| d.child(self.shortcut_actions(cx))))
                        .child(body))))
            .into_any_element()
    }

    fn general_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = prefs();
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
        let mut rows = vec![
                row(tr("界面语言", "Interface language"), None, {
                    let anchor = self.lang_anchor.clone();
                    let current = p.lang();
                    popup_button("lang-picker", current.name(), self.lang_menu.is_some())
                        .child(canvas(move |b, _, _| anchor.set(Some(b)), |_, _, _, _| {}).absolute().inset_0())
                        .relative()
                        .on_click(cx.listener(move |w, _, _, cx| { w.pref_menu = None; w.lang_menu = if w.lang_menu.is_some() { None } else { Some(current) }; cx.notify(); }))
                }).into_any_element(),
                row(tr("自动下载网络图片", "Download web images"), desc(if crate::FEI_NOTES {
                    tr("关闭后，网络图片显示为占位，可点击单独加载。", "When off, web images show a placeholder you can click to load.")
                } else { tr("关闭后不联网，网络图片显示为占位，点击单独加载。", "When off the app stays offline; web images show a placeholder you can click to load.") }),
                    switch("pref-remote", p.remote_images).on_click(cx.listener(|w, _, _, cx| w.set_pref(|p| p.remote_images = !p.remote_images, cx)))).into_any_element(),
                // 网页搜索 opens the selection in the browser; which site is the
                // user's habit, so it is a preference rather than a constant.
                row(tr("网页搜索", "Web search"), desc(tr("选中文字后「网页搜索」用哪个网站。", "Which site “Web search” opens the selected text in.")), {
                    let sel = match p.search_engine.as_str() { "baidu" => 1, "bing" => 2, "duckduckgo" => 3, _ => 0 };
                    self.select("search-engine", vec!["Google", tr("百度", "Baidu"), "Bing", "DuckDuckGo"], sel,
                        |w, i, cx| w.set_pref(|p| p.search_engine = ["", "baidu", "bing", "duckduckgo"][i].into(), cx), cx)
                }).into_any_element(),
        ];
        #[cfg(target_os = "ios")]
        rows.push(row(tr("日期提醒通知","Reminder notifications"),desc(tr("查看通知权限和已安排的提醒。","View notification permission and scheduled reminders.")),button("reminder-notifications",tr("设置","Settings")).on_click(|_,_,_|crate::ipad_notifications::present_settings())).into_any_element());
        if !crate::FEI_NOTES {
            rows.insert(2, row(tr("快捷新建默认类型", "Quick new (⌘N)"), desc(tr("⌘N 新建时创建的类型；「记住上次」沿用你上次从新建菜单选的类型。", "What ⌘N creates; “Last used” repeats the type you last picked from the New menu.")), {
                    let sel = match p.new_default.as_str() { "doc" => 1, "markdown" => 2, _ => 0 };
                    self.select("quick-new", vec![tr("记住上次", "Last used"), tr("文档", "Document"), "Markdown"], sel,
                        |w, i, cx| w.set_pref(|p| p.new_default = ["", "doc", "markdown"][i].into(), cx), cx)
                }).into_any_element());
            rows.push(row(tr("自动检查更新", "Check for updates automatically"), desc(tr("只读取一份版本信息，不发送任何数据；新版本由你决定何时下载安装。", "Reads a version file and sends nothing about you; you decide when to download and install.")),
                switch("pref-update", p.auto_update).on_click(cx.listener(|w, _, _, cx| w.set_pref(|p| p.auto_update = !p.auto_update, cx)))).into_any_element());
        }
        rows.extend(links);
        div().w_full().flex().flex_col().gap(px(28.))
            .child(group(None, rows))
            .into_any_element()
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
            Some(p) => p.strip_prefix(crate::store::Store::root_dir()).unwrap_or(p).display().to_string().into(),
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
        let frame = |d: Div| d.w_full().min_w_0().rounded(px(8.)).overflow_hidden()
            .border_1().border_color(Theme::line_border()).shadow(theme::shadow_button());
        let list = themes::themes();
        let selected = list.iter().find(|t| t.id == current).or_else(|| list.first());
        let shown = if crate::FEI_NOTES {
            selected.map(|t| frame(branded_sample_page(t.style)))
        } else {
            Some(frame(div().h(px(360.)).children(selected.map(|t| sample_page(t, PAGE_S).rounded(px(7.))))))
        };
        drop(list);
        let sample = div().w_full().px(px(16.)).py(px(16.)).flex().justify_center().bg(Theme::fill_hover()).children(shown);
        let mut rows = vec![
            row(tr("外观", "Appearance"), None, {
                let at = match prefs().appearance.as_str() { "light" => 1, "dark" => 2, _ => 0 };
                segmented("appearance", &[tr("跟随系统", "System"), tr("浅色", "Light"), tr("深色", "Dark")], at,
                    |w, i, cx| w.set_pref(|p| p.appearance = ["", "light", "dark"][i].into(), cx), cx)
            }).into_any_element(),
            row(tr("动画", "Animation"), Some(tr("克制：切换即时，只有拖拽带动画。丰富：展开收起、选中和悬停也带动画。", "Low: switching is instant, only dragging animates. High: folding, selection and hover animate too.").into()),
                segmented("motion", &[tr("克制", "Low"), tr("丰富", "High")], usize::from(prefs().motion == "high"),
                    |w, i, cx| w.set_pref(|p| p.motion = ["", "high"][i].into(), cx), cx)).into_any_element(),
        ];
        #[cfg(not(target_os = "ios"))]
        rows.push(
            // Desktop material has no equivalent in the iOS app.
            // 窗口背景: the sidebar sits on the desktop material by
            // default; a busy wallpaper behind it can be distracting.
            row(tr("窗口背景", "Window background"), Some(tr("默认让侧栏透出桌面的毛玻璃。开启后整个窗口不透明，背景里动的东西不会透进来。", "By default the sidebar shows the desktop through a frosted surface. Turn this on for an opaque window.").into()),
                switch("pref-opaque", prefs().opaque_window).on_click(cx.listener(|w, _, window, cx| {
                    w.set_pref(|p| p.opaque_window = !p.opaque_window, cx);
                    window.refresh();
                }))).into_any_element()
        );
        rows.extend([
            // 界面字号: the sidebar and this page together, so the pairs
            // (label, description) stay in proportion.
            row(tr("界面字号", "Interface size"), None, {
                let sel = match prefs().ui_font.as_str() { "small" => 0, "large" => 2, _ => 1 };
                segmented("ui-font", &[tr("小", "Small"), tr("标准", "Default"), tr("大", "Large")], sel,
                    |w, i, cx| w.set_pref(|p| p.ui_font = ["small", "", "large"][i].into(), cx), cx)
            }).into_any_element(),
            row(tr("文档主题", "Document theme"), None, picker).into_any_element(),
            sample.into_any_element(),
        ]);
        if THEME_CSS { rows.push(row(tr("主题 CSS", "Theme CSS"), Some(where_),
                div().flex().flex_wrap().items_center().gap(px(8.))
                    .when(customized || own, |d| d.child(link_button("theme-remove", if customized { tr("恢复默认", "Restore") } else { tr("删除", "Delete") })
                        .on_click(cx.listener(move |w, _, _, cx| w.remove_theme(&rm_id, cx)))))
                    .child(button("theme-duplicate", tr("复制为新主题", "Duplicate")).on_click(cx.listener(move |w, _, _, cx| w.duplicate_theme(&dup_id, cx))))
                    .child(button("theme-edit", tr("编辑 CSS", "Edit CSS")).on_click(cx.listener(move |w, _, _, cx| w.edit_theme(&edit_id, cx))))).into_any_element()); }
        div().w_full().flex().flex_col().gap(px(28.))
            .child(div().w_full().flex().flex_col().gap(px(8.))
                .child(group(None, rows))
                .children(errors.into_iter().chain(warnings.into_iter().take(8).map(|w| format!("{}：{w}", tr("未生效", "Ignored")))).map(|e| div().px(px(4.)).text_size(px(crate::mobile::ui_font_small())).line_height(px(crate::mobile::ui_font_small() + 6.)).text_color(Theme::danger()).child(e)))
                )
            .into_any_element()
    }

    pub(crate) fn personal_settings_store(&self, cx: &App) -> Option<crate::shared_store::SharedStore> {
        let store = cx.global::<crate::shared_store::SharedStore>();
        (!store.is_shared() && store.root() == crate::store::Store::root_dir()).then(|| store.clone())
    }

    pub(crate) fn personal_history_limit(&self, cx: &App) -> u32 {
        if self.is_shared() { self.personal_settings_store(cx).map_or(self.history_limit, |store| store.settings().history_limit()) }
        else { self.history_limit }
    }

    /// 版本历史: the retention count is stored in the library, so every
    /// window sees it; the cached copy only avoids reading a file per frame.
    pub(crate) fn change_history_limit(&mut self, delta: i32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(store) = self.personal_settings_store(cx) else { return; };
        let mut s = store.settings();
        let next = (s.history_limit() as i32 + delta).clamp(1, 30) as u32;
        s.set_history_limit(next);
        if store.save_settings(&s).is_err() { return; }
        if self.store.root() == store.root() { self.history_limit = next; }
        // Other windows cache the number for their settings page; saving
        // always reads the library, so this is display-only.
        for handle in cx.windows().into_iter()
            .filter(|w| w.window_id() != window.window_handle().window_id())
            .filter_map(|w| w.downcast::<Workspace>()) {
            let _ = handle.update(cx, |ws, _, cx| {
                if ws.store.root() == store.root() { ws.history_limit = next; }
                cx.notify();
            });
        }
        cx.notify();
    }



    fn library_page(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.personal_settings_store(cx).is_none() {
            return div().child(tr("请在飞记主窗口打开个人文档库设置。", "Open your personal library settings in the main Fei Notes window.")).into_any_element();
        }
        let mut rows = Vec::new();
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        rows.extend(self.library_mode_rows(cx));
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        {
            let root = crate::store::Store::root_dir();
            let location = if cfg!(target_os = "windows") { root.display().to_string() } else {
                dirs::home_dir().and_then(|h| root.strip_prefix(h).ok().map(|r| format!("~/{}", r.display()))).unwrap_or_else(|| root.display().to_string())
            };
            let reveal = if cfg!(target_os = "macos") { tr("在访达中显示", "Show in Finder") } else if cfg!(target_os = "windows") {
                tr("在文件资源管理器中显示", "Show in File Explorer")
            } else { tr("在文件管理器中显示", "Show in File Manager") };
            rows.extend([
                row(tr("文档库文件夹", "Documents folder"), Some(location.into()), div().flex_none().flex().gap(px(8.))
                    .child(button("reveal-library", reveal).on_click(|_, _, cx| cx.reveal_path(&crate::store::Store::root_dir())))
                    // A test run's library (`QUIET_MD_HOME`) is fixed: a restart would lose it.
                    .when(!cfg!(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-restore")) && std::env::var_os("QUIET_MD_HOME").is_none(), |d| d.child(button("change-library", tr("更改…", "Change…")).on_click(cx.listener(|w, _, _, cx| w.pick_library(cx)))))).into_any_element(),
                row(tr("回收站", "Trash"), Some(crate::trash::retention_note().into()),
                    button("open-trash", tr("打开", "Open")).on_click(cx.listener(|w, _, _, cx| w.open_settings_trash(cx)))).into_any_element(),
            ]);
        }
        rows.push(row(tr("版本历史", "Version history"),
            desc(tr("每篇文档保留的历史版本数（1–30 份）", "Snapshots kept for each document (1–30)")),
            div().flex_none().flex().items_center().gap(px(12.))
                .child(button("history-minus", "−").on_click(cx.listener(|w, _, window, cx| w.change_history_limit(-1, window, cx))))
                .child(div().w(px(40.)).flex().items_center().justify_center().text_size(px(theme::ui_font())).text_color(Theme::text()).child(self.personal_history_limit(cx).to_string()))
                .child(button("history-plus", "+").on_click(cx.listener(|w, _, window, cx| w.change_history_limit(1, window, cx))))).into_any_element());
        div().w_full().flex().flex_col().gap(px(8.))
            .child(group(None,rows))
            .children({
                #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                { Some(self.product_library_groups(cx)) }
                #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
                { None::<Div> }
            })
            .into_any_element()
    }

    pub(crate) fn open_settings_trash(&mut self, cx: &mut Context<Self>) {
        if self.personal_settings_store(cx).is_none() { return; }
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        if self.is_shared() {
            cx.defer(|cx| {
                let options = crate::main_window_options(crate::devtools::scripted(), None, cx);
                let _ = cx.open_window(options, |window, cx| cx.new(|cx| {
                    let mut ws = Workspace::new(window, cx);
                    ws.show_view(View::Trash, cx);
                    ws
                }));
            });
            return;
        }
        if !self.is_shared() { self.show_view(View::Trash, cx); }
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    pub(crate) fn settings_sync_root(&self, cx: &App) -> Option<std::path::PathBuf> {
        if self.settings_page == Page::SharedSync { return Some(self.store.root().to_path_buf()); }
        self.personal_settings_store(cx).map(|store| store.root().to_path_buf())
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn product_library_groups(&self, cx: &mut Context<Self>) -> Div {
        let mut files = vec![row(tr("从 Quiet.md 迁移", "Migrate from Quiet.md"),
            desc(tr("将文档复制到当前文档库。", "Copy documents into your current library.")),
            button("import-legacy-library", tr("选择文件夹…", "Choose Folder…"))
                .on_click(cx.listener(|w, _, window, cx| w.import_legacy_library(window, cx)))).into_any_element()];
        files.extend(self.backup_rows(cx));
        group(Some(tr("导入与导出","Import and Export")),files).pt(px(8.))
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn cloud_back_row(&self, label: &'static str, cx: &mut Context<Self>) -> Div {
        div().flex().child(link_button("cloud-back", label)
            .on_click(cx.listener(|w, _, _, cx| w.close_settings(cx))))
    }

    #[cfg(all(target_os = "ios", feature = "cloudkit-sync"))]
    fn cloud_summary(&self, cx: &App) -> &'static str {
        use crate::cloud_sync::SyncPhase::*;
        if crate::cloud_restore::local_only() { return tr("未开启","Off"); }
        match self.personal_settings_store(cx).and_then(|store|crate::cloud_sync::status(store.root(),cx)).map(|s|s.phase) {
            Some(SignedOut) => tr("已退出登录","Signed out"),
            Some(SubscriptionRequired) => tr("同步暂停","Sync paused"),
            Some(Offline | Failed) => tr("等待重试","Waiting to retry"),
            Some(AccountChanged) => tr("账户已切换","Account changed"),
            Some(AccountChecking | Account) => tr("正在确认账户","Checking account"),
            _ => tr("已开启","On"),
        }
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    pub(crate) fn open_cloud_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.personal_settings_store(cx).is_none() { return; }
        #[cfg(target_os = "ios")]
        {
            if self.view != View::Settings {
                if !self.save_before(cx,|w,window,cx| w.open_cloud_settings(window,cx)) { return; }
                self.mobile_settings_return = self.view;
            }
            self.mobile_settings_root = false;
            self.mobile_settings_parent = None;
        }
        self.open_settings(Page::Cloud,cx);
        self.refresh_library_cloud(window,cx);
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    pub(crate) fn open_cloud_settings_from_share(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.view,View::Doc | View::Folder) || self.current_room(cx).is_none()
            || self.personal_settings_store(cx).is_none() { return; }
        if !self.save_before(cx,|w,window,cx| w.open_cloud_settings_from_share(window,cx)) { return; }
        self.open_cloud_settings(window,cx);
        self.cloud_settings_from_share = self.view == View::Settings;
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn open_cloud_access(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(target_os = "ios")]
        { self.mobile_settings_parent = Some(Page::Cloud); }
        self.open_settings(Page::CloudAccess,cx);
        self.subscription.show_plans = true;
        if cloud_subscription::available() { self.subscription_action(SubscriptionAction::Load,window,cx); }
        if crate::cloud_trial::available() { self.trial_action(false,window,cx); }
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn begin_cloud_enable(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::cloud_restore::EnableCheck;
        if self.library_switching || self.subscription.enable_checking || !crate::cloud_restore::local_only()
            || self.view != View::Settings || !matches!(self.settings_page,Page::Cloud | Page::CloudAccess) { return; }
        self.subscription.enabling_cloud = true;
        self.subscription.enable_checking = true;
        self.subscription.enable_result = None;
        let epoch = self.subscription.enable_epoch;
        cx.notify();
        cx.spawn_in(window, async move |this,cx| {
            let result = cx.background_executor().spawn(async { crate::cloud_restore::check_enable() }).await;
            let _ = this.update_in(cx, |w,window,cx| {
                if w.subscription.enable_epoch != epoch { return; }
                w.subscription.enable_checking = false;
                if w.view != View::Settings || !matches!(w.settings_page,Page::Cloud | Page::CloudAccess)
                    || !crate::cloud_restore::local_only() {
                    w.subscription.enabling_cloud = false;
                    return;
                }
                w.subscription.enable_result = Some(result);
                match result {
                    EnableCheck::Ready => {
                        w.subscription.enabling_cloud = false;
                        w.subscription.enable_result = None;
                        w.open_cloud_settings(window,cx);
                        w.confirm = Some(crate::app::Confirm::SwitchCloudLibrary { cloud: true, copy_local: false });
                    }
                    EnableCheck::AccessRequired if !crate::cloud_trial::testflight() &&
                        (crate::cloud_trial::available() || cloud_subscription::available()) => w.open_cloud_access(window,cx),
                    _ => w.subscription.enabling_cloud = false,
                }
                cx.notify();
            });
        }).detach();
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn enable_cloud_message(&self) -> &'static str {
        use crate::cloud_restore::EnableCheck;
        if self.subscription.enable_checking { return tr("正在核对 iCloud 账户和云功能权益。","Checking iCloud account and cloud access."); }
        match self.subscription.enable_result {
            Some(EnableCheck::SignedOut) => tr("请在系统设置中登录 iCloud，再回来重试。","Sign in to iCloud in system settings, then try again."),
            Some(EnableCheck::Unavailable) => tr("暂时无法确认 iCloud。连接恢复后可重试。","Could not verify iCloud. Retry when the connection returns."),
            Some(EnableCheck::AccessRequired) => crate::cloud_trial::access_required(),
            _ => crate::cloud_restore::startup_fallback().unwrap_or(tr("当前使用本机文档库。开启后可跨设备同步和多人实时协作。",
                "You’re using your local library. Turn on iCloud to sync between devices and collaborate in real time.")),
        }
    }

    /// 更改…: a folder picked as the next library; `library_in` decides
    /// whether it is used as it is or gets a Quiet.md folder inside.
    #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
    fn pick_library(&mut self, cx: &mut Context<Self>) {
        if cfg!(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-restore")) { return; }
        let picked = crate::devtools::take_pick();
        let prompt = picked.is_none().then(|| cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some(tr("选择", "Choose").into()), #[cfg(not(target_os = "ios"))] extensions: None }));
        cx.spawn(async move |this, cx| {
            let paths = match prompt { Some(p) => p.await.ok().and_then(Result::ok).flatten(), None => picked };
            let Some(dir) = paths.and_then(|p| p.into_iter().next()) else { return };
            let root = crate::onboarding::library_in(dir);
            if root == crate::store::Store::root_dir() { return }
            let _ = this.update(cx, |w, cx| { w.confirm = Some(crate::app::Confirm::SwitchLibrary(root)); cx.notify(); });
        }).detach();
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn cloud_rows(&self, cx: &mut Context<Self>) -> Div {
        use personal_sync::Action;
        if crate::cloud_restore::local_only() {
            let control = if self.subscription.enable_checking { setting_status(tr("正在检查","Checking")).into_any_element() }
            else { button("switch-library",tr("开启 iCloud 同步…","Enable iCloud Sync…"))
                .on_click(cx.listener(|w, _, window, cx| w.begin_cloud_enable(window,cx))).into_any_element() };
            let mut rows = vec![row(tr("同步未开启","Sync is off"),desc(self.enable_cloud_message()),control).into_any_element()];
            if matches!(self.subscription.enable_result, Some(crate::cloud_restore::EnableCheck::SignedOut)) {
                rows.push(row(tr("iCloud 账户","iCloud account"),None,account_settings_control()).into_any_element());
            }
            return group(None, rows);
        }
        let Some(store) = self.personal_settings_store(cx) else { return group(None, vec![]); };
        let root = store.root();
        let sync = personal_sync::read(root,cx);
        let control = match sync.action {
            Action::Sync => button("retry-cloud-sync",tr("立即同步","Sync Now"))
                .on_click({ let root = root.to_path_buf(); move |_, _, cx| crate::cloud_sync::retry(&root,cx) }).into_any_element(),
            Action::AutoRetry => setting_button("retry-cloud-sync",tr("立即同步","Sync Now"),false).into_any_element(),
            Action::VerifyAccount => button("verify-cloud-account",tr("重新验证 iCloud","Verify iCloud Again"))
                .on_click({ let root = root.to_path_buf(); move |_, _, cx| crate::cloud_sync::recheck_account(&root,cx) }).into_any_element(),
            Action::SignIn | Action::ChangedAccount => account_settings_control(),
            Action::Plans if crate::cloud_trial::available() || cloud_subscription::available() =>
                button("cloud-subscription-load",tr("查看订阅方案","View Plans"))
                    .on_click(cx.listener(|w, _, window, cx| w.open_cloud_access(window,cx))).into_any_element(),
            _ => div().into_any_element(),
        };
        let mut rows = vec![row(sync.title,sync.detail.map(Into::into),control).into_any_element()];
        rows.extend(sync.rows.into_iter().map(|(_,label,value)|row(label,None,div()
            .text_size(px(crate::mobile::ui_font())).line_height(px(crate::mobile::ui_font() + 7.))
            .text_color(Theme::text_caption()).child(value)).into_any_element()));
        rows.extend(self.sync_issues_rows(root,cx));
        if sync.action != Action::Plans && !crate::cloud_trial::testflight() && cloud_subscription::available() {
            rows.push(row(tr("订阅","Subscription"),None,button("cloud-subscription-load",tr("查看订阅方案","View Plans"))
                .on_click(cx.listener(|w, _, window, cx| w.open_cloud_access(window,cx)))).into_any_element());
        }
        let disable = if self.library_switching { setting_status(tr("正在准备文档库", "Preparing library")).into_any_element() }
        else { button("cloud-disable",tr("关闭…","Turn Off…"))
            .on_click(cx.listener(|w, _, _, cx| w.begin_cloud_disable(cx))).into_any_element() };
        div().flex().flex_col().gap(px(16.))
            .child(group(Some(tr("同步状态","Sync status")), rows))
            .child(group(None,vec![row(tr("关闭 iCloud 同步","Turn Off iCloud Sync"),
                desc(cloud_disable_detail()),disable).into_any_element()]))
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn begin_cloud_disable(&mut self, cx: &mut Context<Self>) {
        if self.view != View::Settings || self.settings_page != Page::Cloud || self.library_switching
            || self.confirm.is_some() || crate::cloud_restore::local_only() || self.personal_settings_store(cx).is_none() { return; }
        self.confirm = Some(crate::app::Confirm::SwitchCloudLibrary { cloud: false, copy_local: false });
        cx.notify();
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn subscription_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let state = &self.subscription;
        let active = state.active.and_then(ActiveSubscription::plan_now);
        let summary = state.message.as_deref().unwrap_or(match active {
            Some(Plan::Annual) => tr("已订阅年付方案。", "Your annual subscription is active."),
            Some(Plan::Monthly) => tr("已订阅月付方案。", "Your monthly subscription is active."),
            None => tr("本机文档库免费使用。订阅包含 iCloud 同步和协作。",
                "The local library is free to use. A subscription includes iCloud sync and collaboration."),
        });
        let load = if state.busy { setting_status(tr("正在查询 App Store", "Checking the App Store")).into_any_element() }
            else { button("cloud-subscription-load", if state.show_plans { tr("收起方案", "Hide Plans") } else { tr("查看方案", "View Plans") })
                .on_click(cx.listener(|w, _, window, cx| {
                    w.subscription.show_plans = !w.subscription.show_plans;
                    if w.subscription.show_plans { w.subscription_action(SubscriptionAction::Load, window, cx); }
                    else { cx.notify(); }
                })).into_any_element() };
        let mut rows = vec![row(tr("订阅", "Subscription"), Some(summary.to_owned().into()), load).into_any_element()];
        if active.is_none() { rows.push(self.trial_row(cx).into_any_element()); }
        if !state.show_plans { return rows; }
        for offer in &state.offers {
            let (title, id, mut detail) = match offer.plan {
                Plan::Annual => (tr("年订阅", "Annual"), "subscribe-annual", crate::localized_format!("每年支付 {}", "{} billed yearly", offer.price)),
                Plan::Monthly => (tr("月订阅", "Monthly"), "subscribe-monthly", crate::localized_format!("每月支付 {}", "{} billed monthly", offer.price)),
            };
            if let Some(price) = &offer.equivalent_monthly {
                detail.push_str(&crate::localized_format!(" · 折合每月 {}", " · Equivalent to {} per month", price));
            }
            if let Some(saving) = offer.saving_percent {
                detail.push_str(&crate::localized_format!(" · 比连续月付省 {}%", " · Save {}% compared with monthly billing", saving));
            }
            let plan = offer.plan;
            let control = if active == Some(plan) { setting_status(tr("当前订阅", "Current Plan")).into_any_element() }
                else if state.busy { div().into_any_element() }
                else { button(id, tr("订阅…", "Subscribe…")).on_click(cx.listener(move |w, _, window, cx| {
                    w.subscription_action(SubscriptionAction::Purchase(plan), window, cx);
                })).into_any_element() };
            rows.push(row(title, Some(detail.into()), control).into_any_element());
        }
        rows.push(row(tr("已有订阅", "Already Subscribed?"), None, div().flex().flex_wrap().gap(px(8.))
            .when(!state.busy, |d| d.child(button("cloud-subscription-restore", tr("恢复购买", "Restore Purchases"))
                .on_click(cx.listener(|w, _, window, cx| w.subscription_action(SubscriptionAction::Restore, window, cx)))))
            .child(button("cloud-subscription-manage", tr("管理订阅", "Manage Subscriptions"))
                .on_click(|_, _, cx| cx.open_url("https://apps.apple.com/account/subscriptions")))).into_any_element());
        let links = [(c"FeiPrivacyURL", "subscription-privacy", tr("隐私政策", "Privacy Policy")),
            (c"FeiTermsURL", "subscription-terms", tr("使用条款", "Terms of Use"))];
        rows.push(row(tr("订阅说明", "Subscription Details"),
            desc(tr("订阅会自动续订，可在 App Store 管理或取消。", "Subscriptions renew automatically and can be managed or canceled in the App Store.")),
            div().flex().flex_wrap().gap(px(8.)).children(links.into_iter().filter_map(|(key, id, label)| {
                let url = release_url(key)?;
                Some(link_button(id, label).on_click(move |_, _, cx| cx.open_url(&url)))
            }))).into_any_element());
        rows
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn trial_row(&self, cx: &mut Context<Self>) -> Div {
        let state = &self.subscription;
        let detail = if let Some(message) = &state.trial_message { message.clone() }
            else if let Some(trial) = state.trial {
                let (y, m, d, h, min) = crate::time::civil(trial.expires_at / 1_000);
                let date = format!("{y}-{m:02}-{d:02} {h:02}:{min:02}");
                if trial.active_at(crate::cloud_trial::now_ms()) {
                    crate::localized_format!("试用截止于 {}。", "Trial ends at {}.", date)
                } else { crate::localized_format!("试用已于 {} 结束。", "Trial ended at {}.", date) }
            } else { tr("同一 iCloud 账户的设备共用 7 天。", "Devices on the same iCloud account share one seven-day trial.").into() };
        let control = if state.trial_busy { setting_status(tr("正在查询 iCloud", "Checking iCloud")).into_any_element() }
            else if state.trial.is_none() { button("cloud-trial-start", tr("开始试用…", "Start Trial…"))
                .on_click(cx.listener(|w, _, _, cx| {
                    w.confirm = Some(crate::app::Confirm::StartCloudTrial);
                    cx.notify();
                })).into_any_element() }
            else { div().into_any_element() };
        row(tr("iCloud 试用", "iCloud Trial"), Some(detail.into()), control)
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    pub(crate) fn trial_action(&mut self, start: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.subscription.trial_busy { return; }
        self.subscription.trial_busy = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_executor().spawn(async move { crate::cloud_trial::request(start) }).await;
            let _ = this.update_in(cx, |w, window, cx| {
                let state = &mut w.subscription;
                state.trial_busy = false;
                let mut granted = false;
                match result {
                    Ok(trial) => {
                        state.trial = trial;
                        state.trial_message = None;
                        if start {
                            crate::cloud_sync::access_changed(cx);
                            granted = trial.is_some_and(|t|t.active_at(crate::cloud_trial::now_ms()));
                        }
                    }
                    Err(error) => state.trial_message = Some(crate::localized_format!("无法查询 iCloud 试用。{}", "Couldn’t check the iCloud trial. {}", error)),
                }
                if granted && w.subscription.enabling_cloud { w.begin_cloud_enable(window,cx); }
                cx.notify();
            });
        }).detach();
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    pub(crate) fn refresh_library_cloud(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.view != View::Settings || !matches!(self.settings_page,Page::Cloud | Page::CloudAccess) || self.personal_settings_store(cx).is_none() { return; }
        if cloud_subscription::available() { self.subscription_updated(window, cx); }
        if !crate::cloud_trial::testflight() && crate::cloud_trial::available() { self.trial_action(false, window, cx); }
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    pub(crate) fn subscription_updated(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.view != View::Settings || !matches!(self.settings_page,Page::Cloud | Page::CloudAccess) || self.personal_settings_store(cx).is_none() { return; }
        if self.subscription.busy { self.subscription.refresh_pending = true; }
        else { self.subscription_action(SubscriptionAction::Status, window, cx); }
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn subscription_action(&mut self, action: SubscriptionAction, window: &mut Window, cx: &mut Context<Self>) {
        if self.subscription.busy { return; }
        self.subscription.busy = true;
        if !matches!(action, SubscriptionAction::Status) { self.subscription.message = None; }
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let reply = cloud_subscription::request(action).await;
            let _ = this.update_in(cx, |w, window, cx| {
                let state = &mut w.subscription;
                let before = state.active;
                state.busy = false;
                match reply {
                    Ok(reply) if matches!(action, SubscriptionAction::Status) => {
                        if reply.error.is_none() {
                            if state.active.and_then(ActiveSubscription::plan_now) != reply.active.and_then(ActiveSubscription::plan_now) {
                                state.message = None;
                            }
                            state.active = reply.active;
                        }
                    }
                    Ok(reply) => {
                        if matches!(reply.outcome, Some(SubscriptionOutcome::Purchased | SubscriptionOutcome::Restored)) {
                            crate::cloud_sync::access_changed(cx);
                        }
                        if reply.error.is_none() || matches!(action, SubscriptionAction::Load) { state.active = reply.active; }
                        if matches!(action, SubscriptionAction::Load) { state.offers = reply.offers.unwrap_or_default(); }
                        state.message = reply.error.or_else(|| match reply.outcome {
                            Some(SubscriptionOutcome::Loaded) if state.offers.is_empty() => Some(tr("App Store 暂未提供订阅方案。", "The App Store has no plans available right now.")),
                            Some(SubscriptionOutcome::Purchased) => Some(tr("购买已确认。", "Purchase confirmed.")),
                            Some(SubscriptionOutcome::Pending) => Some(tr("购买等待批准。批准后可刷新状态。", "Purchase pending approval. Refresh after approval.")),
                            Some(SubscriptionOutcome::Restored) if state.active.and_then(ActiveSubscription::plan_now).is_some() => Some(tr("已恢复有效订阅。", "Active subscription restored.")),
                            Some(SubscriptionOutcome::Restored) => Some(tr("当前 App Store 账户没有有效订阅。", "No active subscription for this App Store account.")),
                            _ => None,
                        }.map(str::to_owned));
                    }
                    Err(error) if !matches!(action, SubscriptionAction::Status) => {
                        if matches!(action, SubscriptionAction::Load) { state.offers.clear(); }
                        state.message = Some(error.to_string());
                    }
                    Err(_) => {}
                }
                if state.active != before && let Some(active) = state.active && let Some(remaining) = active.remaining() {
                    cx.spawn_in(window, async move |this, cx| {
                        cx.background_executor().timer(remaining).await;
                        let _ = this.update_in(cx, |w, window, cx| {
                            if w.subscription.active == Some(active) { w.subscription_updated(window, cx); }
                        });
                    }).detach();
                }
                let granted = state.active.and_then(ActiveSubscription::plan_now).is_some() && state.enabling_cloud;
                if std::mem::take(&mut state.refresh_pending) { w.subscription_updated(window, cx); }
                if granted { w.begin_cloud_enable(window,cx); }
                cx.notify();
            });
        }).detach();
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn backup_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        #[cfg(target_os = "macos")]
        let error = crate::cloud_restore::BACKUP_RESTORE_ERROR.get();
        #[cfg(target_os = "ios")]
        let error: Option<&String> = None;
        let scheduled = crate::cloud_sync::backup::scheduled(&crate::store::Store::root_dir());
        let detail = error.map(|error| diagnostic(error)).unwrap_or_else(|| if scheduled {
            tr("整库文件已校验。重启后继续恢复。", "The library file was verified. Restart to continue restoring it.").into()
        } else {
            tr("飞记会校验所选文件，并保留恢复前的文档库。", "Fei Notes verifies the selected file and keeps your current library.").into()
        });
        let action = if scheduled {
            button("retry-library-restore", tr("重试恢复", "Retry Restore"))
                .on_click(cx.listener(|w, _, _, cx| {
                    if !w.library_switching { cx.defer(crate::app::restart); }
                }))
        } else {
            button("restore-library-backup", tr("选择文件…", "Choose File…"))
                .on_click(cx.listener(|w, _, window, cx| w.library_backup(true, window, cx)))
        };
        vec![
            row(tr("导出整库文件", "Export Library File"),
                Some(tr("包含文档、文件夹、回收站、附件和待同步改动。",
                    "Includes documents, folders, Trash, attachments, and pending changes.").into()),
                button("create-library-backup", tr("选择保存文件夹…", "Choose Save Folder…"))
                    .on_click(cx.listener(|w, _, window, cx| w.library_backup(false, window, cx)))).into_any_element(),
            row(if error.is_some() || scheduled { tr("尚未完成恢复", "Restore Incomplete") }
                else { tr("从整库文件恢复", "Restore from Library File") }, Some(detail.into()), action).into_any_element(),
        ]
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn library_backup(&mut self, restore: bool, window: &mut Window, cx: &mut Context<Self>) {
        use crate::cloud_sync::backup;
        let Some(store) = self.personal_settings_store(cx) else { return; };
        let root = store.root().to_path_buf();
        if self.library_switching { return; }
        let picked = crate::devtools::take_pick();
        let prompt = picked.is_none().then(|| cx.prompt_for_paths(PathPromptOptions {
            files: restore, directories: !restore, multiple: false,
            prompt: Some(if restore { tr("选择飞记整库文件", "Choose a Fei Notes Library File") }
                else { tr("选择保存文件夹", "Choose a Save Folder") }.into()),
            #[cfg(not(target_os = "ios"))] extensions: restore.then(|| vec!["feinotesbackup".into()]),
        }));
        self.library_switching = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let paths = match prompt {
                Some(prompt) => prompt.await.ok().and_then(Result::ok).flatten(),
                None => picked,
            };
            let result: anyhow::Result<Option<String>> = async {
                let Some(path) = paths.and_then(|paths| paths.into_iter().next()) else { return Ok(None); };
                if restore {
                    let target = root.clone();
                    let prepared = cx.background_executor().spawn(async move { backup::prepare(&target, &path) }).await?;
                    this.update_in(cx, |w, _, cx| {
                        w.confirm = Some(crate::app::Confirm::RestoreLibraryBackup(Box::new(prepared)));
                        cx.notify();
                    })?;
                    Ok(None)
                } else {
                    let saved = this.update_in(cx, |w, _, cx| w.flush(cx))?;
                    let current = cx.window_handle().window_id();
                    let others_saved = cx.update(|_, cx| cx.windows().into_iter().filter(|window| window.window_id() != current)
                        .filter_map(|window| window.downcast::<Workspace>())
                        .map(|window| window.update(cx, |w, _, cx| w.flush(cx)).unwrap_or(false))
                        .fold(true, |saved, next| saved & next))?;
                    anyhow::ensure!(saved && others_saved, "{}", tr("请先处理窗口里的保存失败，再导出整库文件。",
                        "Resolve any save errors in open windows before exporting the library file."));
                    let (output, _) = cx.background_executor().spawn(async move { backup::create(&root, &path) }).await?;
                    Ok(Some(crate::localized_format!("已导出整库文件 · {}", "Library file exported · {}", output.file_name().unwrap_or_default().to_string_lossy())))
                }
            }.await;
            let _ = this.update_in(cx, |w, _, cx| {
                w.library_switching = false;
                match result {
                    Ok(Some(message)) => w.show_toast(&message, cx),
                    Err(error) => w.show_toast_kind(ui::ToastKind::Error, &crate::localized_format!(
                        "导出或恢复失败 · {}", "Export or restore failed · {}", error), cx),
                    Ok(None) => {}
                }
                cx.notify();
            });
        }).detach();
    }

    #[cfg(all(target_os = "ios", feature = "cloudkit-sync"))]
    pub(crate) fn restore_library_backup(&mut self, prepared: crate::cloud_sync::backup::Prepared, cx: &mut Context<Self>) {
        if let Some(handle) = cx.windows().into_iter().find_map(|window| window.downcast::<Workspace>()) {
            cx.defer(move |cx| {
                let _ = handle.update(cx,|workspace,window,cx| crate::ipad_session::restore(workspace,prepared,window,cx));
            });
        }
    }

    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    pub(crate) fn restore_library_backup(&mut self, prepared: crate::cloud_sync::backup::Prepared, cx: &mut Context<Self>) {
        let Some(store) = self.personal_settings_store(cx) else { return; };
        let root = store.root().to_path_buf();
        if self.library_switching { return; }
        self.library_switching = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            if !cx.update(|cx| crate::app::save_windows_before(cx, |_| {})).unwrap_or(false) {
                let _ = this.update(cx, |w, cx| { w.library_switching = false; cx.notify(); });
                return;
            }
            let _ = cx.update(crate::cloud_sync::release_unused);
            let result = cx.background_executor().spawn(async move { prepared.schedule(&root) }).await;
            match result {
                Ok(()) => { let _ = cx.update(crate::app::restart); }
                Err(error) => {
                    let _ = this.update(cx, |w, cx| {
                        w.library_switching = false;
                        w.show_toast_kind(ui::ToastKind::Error, &crate::localized_format!(
                            "恢复整库文件失败。{}", "Couldn’t restore the library file. {}", error), cx);
                        cx.notify();
                    });
                }
            }
        }).detach();
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn sync_issues_rows(&self, root: &std::path::Path, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(status) = crate::cloud_sync::status(root, cx) else { return vec![]; };
        let scoped_to_share = self.settings_page == Page::SharedSync;
        let expanded = self.sync_issues.is_some();
        if !expanded && status.held == 0 && status.attachments == 0 && status.conflicts == 0 { return vec![]; }
        let action = if expanded { tr("收起", "Hide Details") } else { tr("查看详情", "Show Details") };
        let mut rows = vec![row(tr("未同步的内容", "Items That Couldn’t Sync"), None,
            button("cloud-issues", action).on_click(cx.listener(|w, _, _, cx| {
                if w.sync_issues.is_some() { w.sync_issues = None; }
                else {
                    let Some(root) = w.settings_sync_root(cx) else { return; };
                    match crate::cloud_sync::issues(&root, cx) {
                        Ok(issues) => w.sync_issues = Some(issues),
                        Err(error) => w.show_toast_kind(ui::ToastKind::Error, &error.to_string(), cx),
                    }
                }
                cx.notify();
            }))).into_any_element()];
        if let Some(issues) = &self.sync_issues {
            if issues.is_empty() {
                rows.push(row(tr("没有待处理的项目", "No Items Need Attention"), None, div()).into_any_element());
            }
            for issue in issues {
                let id = issue.id.clone();
                let trashed = issue.trashed;
                let detail = sync_issue_detail(issue);
                let Some(title) = issue.title.as_deref() else {
                    rows.push(row(tr("尚未载入的文档", "Document not yet loaded"), Some(detail.into()), div()).into_any_element());
                    continue;
                };
                let title = crate::model::display_title(title);
                let action = (!scoped_to_share || !trashed).then(|| button(SharedString::from(format!("open-sync-issue-{}", issue.id)),
                    if trashed { tr("打开回收站", "Open Trash") } else { tr("打开文档", "Open Document") })
                    .on_click(cx.listener(move |w, _, window, cx| {
                        if trashed { w.open_settings_trash(cx); }
                        else if !scoped_to_share && w.is_shared() { w.navigate_personal(View::Home, Some(id.clone()), window, cx); }
                        else { w.open(&id, window, cx); }
                    })));
                rows.push(row(title, Some(detail.into()), div().children(action)).into_any_element());
            }
            if issues.len() == 30 {
                rows.push(row(tr("显示前 30 篇待处理文档", "Showing the First 30 Affected Documents"), None, div()).into_any_element());
            }
        }
        rows
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn cloud_library_label(&self,cx: &App) -> &'static str {
        use crate::cloud_sync::SyncPhase;
        if crate::cloud_restore::local_only() { tr("本机","On this device") }
        else if self.personal_settings_store(cx).and_then(|store|crate::cloud_sync::status(store.root(),cx))
            .is_some_and(|status|matches!(status.phase,SyncPhase::SignedOut | SyncPhase::AccountChanged)) {
            tr("原 iCloud 文档库","Previous iCloud library")
        } else { "iCloud" }
    }

    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    fn library_mode_rows(&self, cx: &App) -> [AnyElement; 2] {
        [
            row(tr("当前文档库","Current library"),None,
                div().text_size(px(crate::mobile::ui_font())).line_height(px(crate::mobile::ui_font() + 7.))
                    .text_color(Theme::text_caption()).child(self.cloud_library_label(cx))).into_any_element(),
            row(tr("本机数据文件夹","Local data folder"),None,
                button("reveal-library",tr("在访达中显示","Show in Finder"))
                    .on_click(|_, _, cx| cx.reveal_path(&crate::store::Store::root_dir()))).into_any_element(),
        ]
    }

    #[cfg(all(target_os = "ios", feature = "cloudkit-sync"))]
    pub(crate) fn select_library(&mut self, selection: crate::cloud_restore::LibrarySelection, cx: &mut Context<Self>) {
        crate::ipad_session::request(selection,cx);
    }

    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    pub(crate) fn select_library(&mut self, selection: crate::cloud_restore::LibrarySelection, cx: &mut Context<Self>) {
        if self.personal_settings_store(cx).is_none() { return; }
        if self.library_switching { return; }
        self.library_switching = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            if !cx.update(|cx| crate::app::save_windows_before(cx, |_| {})).unwrap_or(false) {
                let _ = this.update(cx, |w, cx| { w.library_switching = false; cx.notify(); });
                return;
            }
            match cx.background_executor().spawn(async move { crate::cloud_restore::select_library(selection) }).await {
                Ok(_) => { let _ = cx.update(crate::app::restart); }
                Err(error) => {
                    let _ = this.update(cx, |w, cx| {
                        w.library_switching = false;
                        w.show_toast_kind(ui::ToastKind::Error, &error.to_string(), cx);
                        cx.notify();
                    });
                }
            }
        }).detach();
    }

    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    fn import_legacy_library(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(store) = self.personal_settings_store(cx) else { return; };
        let root = store.root().to_path_buf();
        let picked = crate::devtools::take_pick();
        let prompt = picked.is_none().then(|| cx.prompt_for_paths(PathPromptOptions {
            files: false, directories: true, multiple: false,
            prompt: Some(tr("选择 Quiet.md 文档库", "Choose a Quiet.md library").into()), extensions: None,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let paths = match prompt {
                Some(prompt) => prompt.await.ok().and_then(Result::ok).flatten(),
                None => picked,
            };
            let Some(source) = paths.and_then(|paths| paths.into_iter().next()) else { return; };
            let target = root.clone();
            let result = cx.background_executor().spawn(async move {
                crate::store::Store::import_legacy_copy(&target, &source)
            }).await;
            let _ = this.update_in(cx, |w, _, cx| match result {
                Ok(count) => {
                    if w.store.root() == root {
                        w.entries = w.store.list();
                        w.sort();
                        w.sync_links(cx);
                    }
                    crate::cloud_sync::retry(&root, cx);
                    w.show_toast(&crate::localized_format!("已导入 {} 项", "Imported {} items", count), cx);
                }
                Err(error) => w.show_toast_kind(crate::ui::ToastKind::Error,
                    &crate::localized_format!("导入失败 · {}", "Import failed · {}", error), cx),
            });
        }).detach();
    }

    fn about_page(&self, cx: &mut Context<Self>) -> AnyElement {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let release_rows = {
            let mut rows: Vec<AnyElement> = about_public_links().into_iter().map(|(id, title, url)| {
                row(title, None, button(id, tr("打开", "Open")).on_click(move |_, _, cx| cx.open_url(url))).into_any_element()
            }).collect();
            rows.extend([(c"FeiSupportURL", "about-support", tr("支持", "Support")),
                (c"FeiPrivacyURL", "about-privacy", tr("隐私政策", "Privacy Policy")),
                (c"FeiTermsURL", "about-terms", tr("使用条款", "Terms of Use"))].into_iter().filter_map(|(key, id, title)| {
                let url = release_url(key)?;
                Some(row(title, None, button(id, tr("打开", "Open")).on_click(move |_, _, cx| cx.open_url(&url))).into_any_element())
            }));
            if let Some(notices) = std::env::current_exe().ok().and_then(|path| {
                #[cfg(target_os = "macos")]
                { Some(path.parent()?.parent()?.join("Resources/Open Source Licenses.txt")) }
                #[cfg(target_os = "ios")]
                { Some(path.parent()?.join("Open Source Licenses.json")) }
            }).filter(|path| path.is_file()) {
                rows.push(row(tr("开源许可与第三方声明", "Open Source Licenses and Notices"), None,
                    button("about-licenses", tr("打开", "Open")).on_click(move |_, _, cx| {
                        #[cfg(target_os = "macos")]
                        cx.open_with_system(&notices);
                        #[cfg(target_os = "ios")]
                        {
                            let _ = cx;
                            unsafe extern "C" { fn fei_ios_present_notices(path: *const std::ffi::c_char, title: *const std::ffi::c_char); }
                            if let (Ok(path), Ok(title)) = (std::ffi::CString::new(notices.to_string_lossy().as_bytes()), std::ffi::CString::new(tr("开源许可与第三方声明", "Open Source Licenses and Notices"))) {
                                unsafe { fei_ios_present_notices(path.as_ptr(), title.as_ptr()); }
                            }
                        }
                    })).into_any_element());
            }
            Some(group(None, rows))
        };
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        let release_rows: Option<Div> = None;
        let version = env!("CARGO_PKG_VERSION");
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let version = release_bundle_string(c"CFBundleShortVersionString").unwrap_or_else(|| version.into());
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let version = release_bundle_string(c"CFBundleVersion").filter(|build| !build.is_empty()).map(|build| format!("{version} ({build})")).unwrap_or(version);
        let footer_size = crate::mobile::ui_font_small();
        div().w_full().flex().flex_col().gap(px(28.)).child(div().w_full().pt(px(24.)).flex().flex_col().items_center().gap(px(10.))
            .child(img(about_icon()).size(px(88.)))
            .child(div().text_size(px(20.)).line_height(px(26.)).font_weight(FontWeight::SEMIBOLD).child(crate::app_name()))
            .child(div().text_size(px(crate::mobile::ui_font())).line_height(px(crate::mobile::ui_font() + 5.)).text_color(Theme::text_caption()).child(format!("{} {}", tr("版本", "Version"), version)))
            .child(div().text_size(px(crate::mobile::ui_font())).line_height(px(crate::mobile::ui_font() + 7.)).text_color(Theme::text_caption()).child(if crate::FEI_NOTES {
                product_about_description()
            } else { tr("本地优先的文档应用：无需账号，文档就是你电脑上的文件。", "A local-first document app. Your documents are stored as files on your Mac, with no account required.") })))
            .children(release_rows)
            .when(crate::FEI_NOTES, |d| d.child(div().w_full().px(px(4.)).text_size(px(footer_size))
                .line_height(px(footer_size + 5.)).text_color(Theme::text_caption())
                .child(tr("基于 Quiet.md 开发", "Based on Quiet.md"))))
            .when(!crate::FEI_NOTES, |d| d.child(group(None, vec![update_row(cx).into_any_element()])))
            .into_any_element()
    }
}

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
fn sync_issue_detail(issue: &crate::cloud_sync::SyncIssue) -> String {
    use crate::cloud_sync::AssetProblem;
                let mut detail = Vec::new();
                if issue.conflict {
                    detail.push(tr("这篇文档的改动需要恢复。已保留本机内容和收到的 iCloud 改动。",
                        "Changes to this document need recovery. Local content and downloaded iCloud changes are preserved.").to_string());
                }
                if issue.held {
                    detail.push(tr("改动已保存在本机。将较长的正文拆成几篇文档后重试。",
                        "Changes are saved on this Mac. Split a large document into smaller documents and try again.").to_string());
                }
                for attachment in &issue.attachments {
                    let name = attachment.source.rsplit('/').next().unwrap_or(&attachment.source);
                    let message = if attachment.recovering {
                        tr("本机副本缺失，正在尝试从 iCloud 恢复。", "The local copy is missing. Recovering it from iCloud.")
                    } else { match attachment.reason {
                        Some(AssetProblem::External) => tr("在文档库外。请重新添加文件。", "Outside the library. Add the file again."),
                        Some(AssetProblem::Missing) => tr("找不到文件。请在原 Mac 重新添加。", "File missing. Add it again on the original Mac."),
                        Some(AssetProblem::Oversized) => tr("超过 64 MiB。请在原 Mac 换成较小的文件。", "Over 64 MiB. Replace it with a smaller file on the original Mac."),
                        Some(AssetProblem::Unreadable) => tr("无法读取。请在原 Mac 检查权限或重新添加。", "Unreadable. Check permissions or add it again on the original Mac."),
                        None => tr("等待原 Mac 上传。请检查文件后重试。", "Waiting for the original Mac. Check the file and try again."),
                    } };
                    let location = if attachment.stored_here { tr("文件保存在这台 Mac。", "The file is on this Mac. ") } else { "" };
                    detail.push(format!("{name} · {location}{message}"));
                }
                if detail.is_empty() {
                    detail.push(tr("请在原 Mac 检查附件并重试。", "Check the attachments on the original Mac and try again.").into());
                }
    detail.join("\n")
}

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
fn cloud_sync_detail(workspace: &Workspace, root: &std::path::Path, cx: &App) -> Option<String> {
    use crate::{app::SaveStatus, cloud_sync::SyncPhase};
    let status = crate::cloud_sync::status(root, cx)?;
    let current_document = workspace.store.root() == root && workspace.current.is_some();
    let account_check = matches!(status.phase, SyncPhase::AccountChecking | SyncPhase::AccountChanged | SyncPhase::SignedOut);
    let phase = if status.access_paused && !account_check { SyncPhase::SubscriptionRequired } else { status.phase };
    let stalled = status.stalled;
    let actionable = matches!(phase, SyncPhase::AccountChanged | SyncPhase::SignedOut | SyncPhase::Quota
        | SyncPhase::SubscriptionRequired | SyncPhase::ReadOnly | SyncPhase::Revoked);
    let phase = if actionable || account_check { phase } else { stalled.unwrap_or(phase) };
    let description = match phase {
        SyncPhase::Connecting if stalled.is_some() => tr("连接 iCloud 停滞。请检查网络，再点「立即同步」。",
            "Connecting to iCloud has stalled. Check your connection, then choose Sync Now."),
        SyncPhase::Uploading if stalled.is_some() => tr("上传停滞。请检查网络和 iCloud 空间，再点「立即同步」。",
            "Uploads have stalled. Check your connection and iCloud storage, then choose Sync Now."),
        SyncPhase::Downloading if stalled.is_some() => tr("下载停滞。请检查网络，再点「立即同步」。",
            "Downloads have stalled. Check your connection, then choose Sync Now."),
        SyncPhase::Applying if stalled.is_some() => tr("应用已下载的改动时停滞。请检查本机可用空间，再点「立即同步」。",
            "Applying downloaded changes has stalled. Check available device storage, then choose Sync Now."),
        SyncPhase::Failed if stalled.is_some() => tr("同步停滞。请点「立即同步」重试。", "Sync has stalled. Choose Sync Now to retry."),
        SyncPhase::Offline if stalled.is_some() => tr("一直连不上 iCloud。请检查网络，恢复连接后会自动重试。",
            "iCloud is still unreachable. Check your connection. Sync retries when the connection returns."),
        SyncPhase::Account if stalled.is_some() => tr("无法验证 iCloud 账户。请检查系统设置中的账户，再点「立即同步」。",
            "Could not verify the iCloud account. Check your account in Settings, then choose Sync Now."),
        SyncPhase::TestFlightVerification if stalled.is_some() => tr("无法验证 TestFlight 安装。请联网后重试，或从 TestFlight 重新打开飞记。",
            "Could not verify the TestFlight installation. Retry online or reopen Fei Notes from TestFlight."),
        SyncPhase::Connecting => tr("正在连接 iCloud", "Connecting to iCloud"),
        SyncPhase::Uploading => tr("正在上传改动", "Uploading changes"),
        SyncPhase::Downloading => tr("正在从 iCloud 下载改动", "Downloading changes from iCloud"),
        SyncPhase::Applying => tr("正在应用已下载的改动", "Applying downloaded changes"),
        SyncPhase::Ready if status.catalogue_paused => tr("文档目录同步已暂停", "Document list sync is paused"),
        SyncPhase::Ready if status.conflicts > 0 => tr("部分文档需要恢复", "Some documents need recovery"),
        SyncPhase::Ready if status.downloads > 0 => tr("正在应用已下载的改动", "Applying downloaded changes"),
        SyncPhase::Ready if status.uploads > 0 => tr("正在等待上传", "Waiting to upload"),
        SyncPhase::Ready if status.held > 0 || status.attachments > 0 => tr("部分内容未同步", "Some items could not sync"),
        SyncPhase::Ready if current_document && workspace.unsaved =>
            tr("等待本机改动保存后同步", "Waiting for changes to be saved on this device"),
        SyncPhase::Ready => tr("已同步", "Up to date"),
        SyncPhase::Offline => tr("暂时连不上 iCloud，恢复连接后会自动重试。", "iCloud is temporarily unreachable. Sync will retry automatically."),
        SyncPhase::AccountChecking | SyncPhase::Account => tr("正在确认 iCloud 账户", "Checking the iCloud account"),
        SyncPhase::AccountChanged => tr("iCloud 账户已切换。请在「文档库 › iCloud」查看。",
            "The iCloud account changed. See Library › iCloud."),
        SyncPhase::SignedOut => tr("iCloud 已退出登录。请在系统设置中登录原账户。",
            "You’re signed out of iCloud. Sign in to your previous account in System Settings."),
        SyncPhase::Quota => tr("iCloud 空间不足。释放空间后可点「立即同步」。",
            "iCloud storage is full. Free up space, then choose Sync Now."),
        SyncPhase::SubscriptionRequired => tr("同步已暂停。恢复云功能后会继续同步。",
            "Sync is paused. It resumes when cloud access is restored."),
        SyncPhase::TestFlightVerification => tr("正在验证 TestFlight 安装", "Verifying the TestFlight installation"),
        SyncPhase::ReadOnly => tr("共享权限已改为只读。请联系所有者恢复编辑权限。",
            "The share is now read-only. Ask the owner to restore editing access."),
        SyncPhase::Revoked => tr("共享访问已结束。请联系所有者。", "Access to this share has ended. Contact the owner."),
        SyncPhase::Failed => tr("正在等待重试", "Waiting to retry"),
    };
    let local = if current_document {
        match &workspace.status {
            SaveStatus::Failed(_) => Some(tr("本机保存失败", "Could not save on this device")),
            SaveStatus::Saving => Some(tr("正在保存到本机", "Saving on this device")),
            SaveStatus::Saved if !workspace.unsaved => Some(tr("当前文档的改动已保存在本机", "Changes to this document are saved on this device")),
            SaveStatus::Idle if !workspace.unsaved && workspace.editor.is_some() =>
                Some(tr("当前文档已保存在本机", "This document is saved on this device")),
            _ => None,
        }
    } else { None };
    let mut detail = String::new();
    if let Some(local) = local.or_else(|| (status.uploads > 0).then(|| tr(
        "待上传的改动已保存在本机", "Pending uploads are saved on this device"))) {
        detail.push_str(local);
        detail.push('\n');
    }
    detail.push_str(description);
    if status.uploads > 0 || status.downloads > 0 {
        detail.push_str(&crate::localized_format!(" · 待上传 {} 项，待应用 {} 项", " · {} uploads and {} items to apply",
            status.uploads, status.downloads));
    }
    if let Some(time) = crate::cloud_sync::last_success(root, cx) {
        let (year, month, day, hour, minute) = crate::time::civil(time);
        let date = format!("{year}-{month:02}-{day:02} {hour:02}:{minute:02}");
        detail.push_str(&crate::localized_format!("\n上次同步成功：{}", "\nLast successful sync: {}", date));
    } else if !actionable && !account_check && phase != SyncPhase::Account {
        detail.push_str(tr("\n尚无同步成功记录", "\nNo successful sync recorded yet"));
    }
    if status.catalogue_paused {
        detail.push_str(tr("\n已保留目录改动，已有文档的正文仍可同步。", "\nDocument list changes are preserved. Existing document contents can still sync."));
    }
    if status.conflicts > 0 {
        detail.push_str(&crate::localized_format!(" · {} 篇文档的改动需要恢复", " · {} documents have changes that need recovery", status.conflicts));
    }
    if status.held > 0 {
        detail.push_str(&crate::localized_format!(" · {} 篇文档有未能上传的改动", " · {} documents have changes that could not upload", status.held));
    }
    if status.attachments > 0 {
        detail.push_str(&crate::localized_format!(" · {} 个附件超限、缺失或不可读", " · {} attachments are oversized, missing or unreadable", status.attachments));
    }
    Some(detail)
}

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
fn cloud_sync_row(workspace: &Workspace, root: &std::path::Path, cx: &App) -> AnyElement {
    use crate::cloud_sync::SyncPhase;
    let Some(status) = crate::cloud_sync::status(root, cx) else {
        return row(tr("iCloud 同步", "iCloud sync"), None, div().child(tr("仅保存在本机", "Saved on this device only"))).into_any_element();
    };
    let account_check = matches!(status.phase, SyncPhase::AccountChecking | SyncPhase::AccountChanged | SyncPhase::SignedOut);
    let detail = cloud_sync_detail(workspace, root, cx).unwrap_or_default();
    let action = match status.phase {
        _ if !status.access_paused && !account_check && status.phase != SyncPhase::Revoked => Some(button("retry-cloud-sync", tr("立即同步", "Sync now"))
            .on_click({ let root = root.to_path_buf(); move |_, _, cx| crate::cloud_sync::retry(&root, cx) })),
        _ => None,
    };
    row(tr("iCloud 同步", "iCloud sync"), Some(detail.into()), div().children(action)).into_any_element()
}

#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
fn account_settings_control() -> AnyElement {
    #[cfg(target_os = "macos")]
    return button("open-apple-account",tr("打开系统设置…","Open System Settings…"))
        .on_click(|_,_,cx|cx.open_url("x-apple.systempreferences:com.apple.systempreferences.AppleIDSettings")).into_any_element();
    #[cfg(target_os = "ios")]
    setting_status(tr("设置 › Apple 账户","Settings › Apple Account")).into_any_element()
}

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
    Some(div().id("update-pill").no_drag().map(crate::gpui_compat::flex_shrink).min_w_0().h(px(24.)).px(px(10.)).relative().overflow_hidden().flex().items_center().justify_center().rounded_full()
        .text_size(px(crate::mobile::ui_font_small())).line_height(px(crate::mobile::ui_font_small() + 4.)).font_weight(FontWeight::MEDIUM)
        .when(!busy, |d| d.cursor_pointer().bg(Theme::primary()).text_color(gpui::white()).hover(|s| s.opacity(0.9)))
        .when(busy, |d| d.bg(Theme::sidebar_hover()).text_color(Theme::text_caption()))
        // Downloading: a fixed width (no jitter as the digits change), filled left
        // to right behind a moving wave.
        .when_some(fill, |d, f| d.w(px(56.)).child(ui::wave_fill(f, Theme::primary().opacity(0.18))))
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
        "粘贴 iCloud 邀请链接" => "Paste iCloud invitation link",
        "对方的 Apple 账户邮箱" => "Apple Account email",
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
        "* 空格" => "* Space",
        "- 空格" => "- Space",
        "短划线列表" => "Dashed list",
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
        "--- 或 ***" => "--- or ***",
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
        "复制文件地址" => "Copy file path",
        "已复制文件地址" => "File path copied",
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
        "创建时间（最新优先）" => "Created (newest first)",
        "标题（升序）" => "Title (A–Z)",
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
        "Quiet.md · 快速文档" => "Quiet.md · Quick document",
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
pub fn display_title(title: &str) -> String {
    let title = title.trim();
    if title.is_empty() { text(crate::model::UNTITLED).to_string() } else { title.to_string() }
}

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
        ("恢复文档 {} 失败，已恢复的文档保留在文档库", "Could not restore document {}. Documents already restored remain in Documents."),
        ("文档已恢复，回收站索引清理失败：{}", "Document restored, but the Trash index could not be cleaned up: {}"),
        ("上一次恢复请求已失效。请重新选择备份。", "The previous restore request is no longer usable. Choose the backup again."),
        ("请关闭打开的共享文档窗口，再重试恢复。", "Close any open shared document windows, then retry the restore."),
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
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() { walk(&path, pattern); }
                else if path.extension().is_some_and(|ext| ext == "rs") {
                    let source = std::fs::read_to_string(path).unwrap();
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
    fn theme_preferences_preserve_supported_choices_and_retire_product_themes() {
        assert_eq!(Preferences::default().theme_id(), "feishu");
        for (json, expected) in [
            (r#"{}"#, "feishu"),
            (r#"{"feishu":false}"#, "craft"),
            (r#"{"feishu":true}"#, "feishu"),
            (r#"{"theme":"notion","feishu":false}"#, "notion"),
            (r#"{"theme":"craft","feishu":true}"#, "craft"),
            (r#"{"theme":"my-theme"}"#, "my-theme"),
            (r#"{"theme":"notes"}"#, "notes"),
            (r#"{"theme":"obsidian"}"#, "obsidian"),
        ] {
            let expected = if crate::FEI_NOTES && !matches!(expected, "feishu" | "notes") { "notes" } else { expected };
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
            (r#"{"english":true,"language":"ko"}"#, Lang::Ko),
            (r#"{"english":true,"language":"ar"}"#, Lang::Ar),
            (r#"{"english":true,"language":"xx"}"#, Lang::En),
        ] {
            let saved: Preferences = serde_json::from_str(json).unwrap();
            assert_eq!(saved.lang(), expected, "{json}");
        }
        let mut p: Preferences = serde_json::from_str(r#"{"english":false}"#).unwrap();
        p.set_lang(Lang::Ko);
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!((json["language"].as_str(), json["english"].as_bool()), (Some("ko"), Some(true)), "older builds read English");
        p.set_lang(Lang::Zh);
        assert!(!p.english);
    }
}

#[cfg(target_os = "ios")]
fn release_bundle_string(key: &std::ffi::CStr) -> Option<String> {
    unsafe extern "C" { fn fei_ios_product_value(key: *const std::ffi::c_char) -> *mut std::ffi::c_char; fn free(value: *mut std::ffi::c_void); }
    let value = unsafe { fei_ios_product_value(key.as_ptr()) };
    if value.is_null() { return None; }
    let text = unsafe { std::ffi::CStr::from_ptr(value) }.to_str().ok().map(str::to_owned);
    unsafe { free(value.cast()); }
    text
}
