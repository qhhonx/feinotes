//! 首次启动: a few seconds of dithered film with Quiet glowing in, then a
//! straight cut to two pages on a drifting gradient —
//! where the library lives and the quick-note shortcut, then notes to bring
//! along. ⌘⌥⇧O and Settings › About replay it without touching the library. Nothing is written until 开始使用;
//! closing the window before that quits without a library.

use crate::settings::tr;
use crate::theme::{self, Theme};
use crate::ui;
use gpui::*;
use gpui::prelude::FluentBuilder;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static PENDING: AtomicBool = AtomicBool::new(false);

/// First launch's window is open and no library is chosen yet.
pub fn pending() -> bool { PENDING.load(Ordering::Relaxed) }

/// A new library gets the pages; `QUIET_MD_ONBOARDING` shows them anyway (previews, screenshots).
pub fn wanted(existed: bool, scripted: bool) -> bool {
    std::env::var_os("QUIET_MD_ONBOARDING").is_some() || (!existed && !scripted)
}

// ---- the film

mod film {
use super::*;

/// Ordered-dither frames, each the XOR of the one before, stacked in one
/// greyscale PNG (palette index × 17) under a first row holding the palette
/// as RGB bytes. Made from a generated clip by the intro script.
const FILM: &str = "onboarding/intro.png";
const FILM_W: usize = 400;
const FILM_H: usize = 225;
pub const FILM_FPS: f32 = 12.;

pub struct Film { pub frames: Vec<Vec<u8>>, pub colors: Vec<Hsla> }

pub fn decode() -> Option<Film> {
    let img = image::load_from_memory_with_format(crate::assets::get(FILM)?, image::ImageFormat::Png).ok()?.into_luma8();
    if img.width() as usize != FILM_W { return None }
    let raw = img.as_raw();
    let colors = raw[..48].chunks(3).map(|c| Hsla::from(rgb(u32::from_be_bytes([0, c[0], c[1], c[2]])))).collect();
    let (px, n) = (&raw[FILM_W..], (img.height() as usize - 1) / FILM_H);
    let mut cur = vec![0u8; FILM_W * FILM_H];
    let mut frames = Vec::with_capacity(n);
    for f in 0..n {
        let delta = &px[f * FILM_W * FILM_H..(f + 1) * FILM_W * FILM_H];
        for (c, d) in cur.iter_mut().zip(delta) { *c ^= d / 17; }
        frames.push(cur.clone());
    }
    (!frames.is_empty()).then_some(Film { frames, colors })
}

/// How many texels each dot of the film gets in its texture. The GPU
/// samples images smoothly; at 4 the blend at a dot's edge is about a device
/// pixel wide, so the dots stay crisp.
const UP: usize = 4;

/// One frame as a texture (BGRA), each dot a 4×4 block. Built only when the
/// film moves on to another frame: drawing it is one image, where a quad per
/// run of dots came to some 45 000 quads a frame and held the window to 15 fps.
pub fn texture(frame: &[u8], colors: &[Hsla]) -> std::sync::Arc<RenderImage> {
    let bgra: Vec<[u8; 4]> = colors.iter().map(|&c| { let c = Rgba::from(c); [(c.b * 255.) as u8, (c.g * 255.) as u8, (c.r * 255.) as u8, 255] }).collect();
    let (w, h) = (FILM_W * UP, FILM_H * UP);
    let mut buf = vec![0u8; w * h * 4];
    for y in 0..FILM_H {
        let row = &frame[y * FILM_W..(y + 1) * FILM_W];
        let line = &mut buf[y * UP * w * 4..(y * UP + 1) * w * 4];
        for (x, &c) in row.iter().enumerate() {
            let px = bgra[(c as usize).min(bgra.len() - 1)];
            for k in 0..UP { line[(x * UP + k) * 4..(x * UP + k) * 4 + 4].copy_from_slice(&px); }
        }
        for k in 1..UP { buf.copy_within(y * UP * w * 4..(y * UP + 1) * w * 4, (y * UP + k) * w * 4); }
    }
    let img = image::RgbaImage::from_raw(w as u32, h as u32, buf).expect("film texture size");
    std::sync::Arc::new(RenderImage::new(vec![image::Frame::new(img)]))
}

/// The texture over `b`, scaled to cover it and centred; what spills over is clipped.
pub fn paint_texture(img: std::sync::Arc<RenderImage>, b: Bounds<Pixels>, window: &mut Window) {
    let (bw, bh) = (f32::from(b.size.width), f32::from(b.size.height));
    let s = (bw / FILM_W as f32).max(bh / FILM_H as f32);
    let (w, h) = (FILM_W as f32 * s, FILM_H as f32 * s);
    let at = point(b.left() + px((bw - w) / 2.), b.top() + px((bh - h) / 2.));
    window.with_content_mask(Some(ContentMask { bounds: b }), |window| {
        let _ = window.paint_image(Bounds::new(at, size(px(w), px(h))), Corners::default(), img, 0, false);
    });
}

/// The wordmark over the film: white Quiet with a glow that follows each
/// letter. GPUI has no text shadow, so faint copies of the word ring it,
/// a ring every point out to 18 with copies no more than 2.5 apart, each
/// ring turned half a step from the last, fading on a bell curve with a
/// long soft tail: their overlap blurs out from every stroke's edge
/// without steps you can see.
pub fn wordmark(alpha: f32) -> Div {
    let word = |a: f32| div().text_size(px(92.)).line_height(px(110.)).font_weight(FontWeight::MEDIUM).text_color(hsla(0., 0., 1., a)).child("Quiet");
    let mut halo = div().relative();
    for ring in 1..=18 {
        let r = ring as f32;
        let n = ((std::f32::consts::TAU * r / 2.5).round() as usize).max(10);
        let a = (0.05 * (-(r / 5.).powi(2)).exp() + 0.02 * (-(r / 13.).powi(2)).exp()) * 16. / n as f32;
        for i in 0..n {
            let th = std::f32::consts::TAU * (i as f32 + 0.5 * (ring % 2) as f32) / n as f32;
            halo = halo.child(word(a * alpha).absolute().left(px(r * th.cos())).top(px(r * th.sin())));
        }
    }
    div().absolute().inset_0().flex().items_center().justify_center().child(halo.child(word(alpha).relative()))
}
}

// ---- the backdrop

/// The film alone for a second, Quiet glowing in, then the pages straight
/// over the film, which keeps playing behind them.
const HOLD: f32 = 2.9;
const MARK_IN: (f32, f32) = (1.0, 1.8);
/// Found folders shown before 再显示.
const FOUND_SHOWN: usize = 4;

/// The backdrop's pace: slow drift, so 30 frames a second is smooth enough.
const FLOW_FPS: f32 = 30.;

/// Soft light on a near-white page, frosted with grain: sky blue above,
/// cloud and grass green below, each patch a blurred circle wandering on its
/// own slow loop. Colours measured from the website's hillside picture
/// (site/public/bg.png).
fn paint_backdrop(b: Bounds<Pixels>, t: f32, window: &mut Window) {
    use std::f32::consts::TAU;
    let (x, y, w, h) = (f32::from(b.left()), f32::from(b.top()), f32::from(b.size.width), f32::from(b.size.height));
    let dark = theme::dark();
    window.paint_quad(fill(b, if dark { Theme::bg_body() } else { rgb(0xfdfdfd).into() }));
    // Centre (shares of the window), diameter (share of the width), colour, strength, loop lengths in seconds.
    // Laid out like the film's hillside, which plays just before: sky over
    // the top half, a pale cloud at the left of the horizon, grass over the
    // bottom third. Light washes of those colours, so the page stays bright.
    const PATCHES: [(f32, f32, f32, u32, f32, f32, f32); 7] = [
        (0.12, 0.08, 0.48, 0x4f97ea, 0.5, 23., 31.),
        (0.50, 0.02, 0.46, 0x4793e6, 0.42, 37., 27.),
        (0.86, 0.10, 0.46, 0x4f97ea, 0.5, 33., 25.),
        (0.16, 0.60, 0.30, 0xc5dabd, 0.8, 41., 29.),
        (0.10, 0.98, 0.50, 0xb1c78d, 0.7, 29., 19.),
        (0.52, 1.04, 0.50, 0xb1c78d, 0.62, 31., 23.),
        (0.92, 0.96, 0.50, 0x95b676, 0.55, 27., 35.),
    ];
    for (i, &(cx, cy, d, color, k, lx, ly)) in PATCHES.iter().enumerate() {
        let ph = i as f32 * 1.7;
        let cx = x + w * (cx + 0.08 * (TAU * t / lx + ph).sin());
        let cy = y + h * (cy + 0.05 * (TAU * t / ly + ph).cos());
        let d = w * d * (1. + 0.10 * (TAU * t / (lx + ly) + ph).sin());
        let c = Hsla::from(rgb(color)).opacity(if dark { 0.16 } else { k });
        window.paint_shadows(Bounds::new(point(px(cx - d / 2.), px(cy - d / 2.)), size(px(d), px(d))), Corners::all(px(d / 2.)),
            &[BoxShadow { color: c, offset: point(px(0.), px(0.)), blur_radius: px(d * 0.45), spread_radius: px(0.) }]);
    }
    // Grain: one tile of noise repeated, a texel per point.
    let tile = GRAIN_N as f32;
    let (mut ty, img) = (y, GRAIN.clone());
    while ty < y + h {
        let mut tx = x;
        while tx < x + w {
            let _ = window.paint_image(Bounds::new(point(px(tx), px(ty)), size(px(tile), px(tile))), Corners::default(), img.clone(), 0, false);
            tx += tile;
        }
        ty += tile;
    }
}

const GRAIN_N: u32 = 320;
/// Specks mostly lighter than what's under them, barely any darker, so the
/// frost brightens the page instead of greying it (BGRA).
static GRAIN: std::sync::LazyLock<std::sync::Arc<RenderImage>> = std::sync::LazyLock::new(|| {
    let mut s: u32 = 0x9e37_79b9;
    let buf = image::RgbaImage::from_fn(GRAIN_N, GRAIN_N, |_, _| {
        s ^= s << 13; s ^= s >> 17; s ^= s << 5;
        let v = (s >> 24) as f32 / 255. * 2. - 1.;
        if v < 0. { image::Rgba([0, 0, 0, (-v * 9.) as u8]) } else { image::Rgba([255, 255, 255, (v * 70.) as u8]) }
    });
    std::sync::Arc::new(RenderImage::new(vec![image::Frame::new(buf)]))
});

/// Things settling in when a page appears: 0 → 1 over `dur` after `delay`, easing out.
fn arrive(since: f32, delay: f32, dur: f32) -> f32 {
    let p = ((since - delay) / dur).clamp(0., 1.);
    1. - (1. - p).powi(3)
}

// ---- where notes already are

#[derive(Clone)]
struct Found {
    app: &'static str,
    name: String,
    path: PathBuf,
    /// Markdown files inside.
    count: usize,
}

/// Markdown files under `dir`, without hidden folders, up to a budget of entries.
fn count_md(dir: &Path, depth: usize, budget: &mut usize) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    let mut n = 0;
    for e in rd.flatten() {
        if *budget == 0 { break }
        *budget -= 1;
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "node_modules" { continue }
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() && depth > 0 { n += count_md(&e.path(), depth - 1, budget); }
        else if ft.is_file() && crate::local_files::is_markdown_path(&e.path()) { n += 1; }
    }
    n
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir).map(|rd| rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir()) && !e.file_name().to_string_lossy().starts_with('.')).map(|e| e.path()).collect()).unwrap_or_default()
}

/// Notes other apps keep on this computer, read only and off the main
/// thread. Only an app's own places are looked at (never the user's other
/// folders), and an app shows only when its place really holds notes.
/// Only folders that already hold Markdown: apps that keep notes in their
/// own format (Bear, Ulysses…) are not offered.
fn discover(library: PathBuf) -> Vec<Found> {
    let home = dirs::home_dir().unwrap_or_default();
    let mobile = home.join("Library/Mobile Documents");
    let mut candidates: Vec<(&'static str, PathBuf)> = vec![];
    // Obsidian lists its vaults in obsidian.json (macOS, Windows and Linux alike), and keeps iCloud vaults in its container.
    if let Some(cfg) = dirs::config_dir().and_then(|d| std::fs::read(d.join("obsidian/obsidian.json")).ok())
        && let Ok(v) = serde_json::from_slice::<serde_json::Value>(&cfg)
        && let Some(vaults) = v.get("vaults").and_then(|v| v.as_object()) {
        for vault in vaults.values() {
            if let Some(p) = vault.get("path").and_then(|p| p.as_str()) { candidates.push(("Obsidian", PathBuf::from(p))); }
        }
    }
    for d in subdirs(&mobile.join("iCloud~md~obsidian/Documents")) { candidates.push(("Obsidian", d)); }
    // Logseq names each graph's folder in ~/.logseq/graphs: `logseq_local_++Users++me++notes.transit`.
    for g in std::fs::read_dir(home.join(".logseq/graphs")).into_iter().flatten().flatten() {
        let name = g.file_name().to_string_lossy().into_owned();
        if let Some(p) = name.strip_prefix("logseq_local_").and_then(|n| n.strip_suffix(".transit")) { candidates.push(("Logseq", PathBuf::from(p.replace("++", "/")))); }
    }
    for d in subdirs(&mobile.join("iCloud~com~logseq~logseq/Documents")) { candidates.push(("Logseq", d)); }
    candidates.push(("iA Writer", mobile.join("27N4MQEA55~pro~writer/Documents")));
    candidates.push(("Typora", mobile.join("iCloud~abnerworks~Typora/Documents")));
    let mut out: Vec<Found> = vec![];
    for (app, path) in candidates {
        if !path.is_dir() || path.starts_with(&library) || library.starts_with(&path) { continue }
        let key = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if out.iter().any(|f| std::fs::canonicalize(&f.path).unwrap_or_else(|_| f.path.clone()) == key) { continue }
        let mut budget = 20_000;
        let count = count_md(&path, 6, &mut budget);
        if count == 0 { continue }
        let name = found_name(app, &path, &mobile);
        out.push(Found { app, name, path, count });
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.count));
    out
}

/// The name a found folder shows and is imported under: its own, except an
/// app's iCloud folder, which is just "Documents" and goes by the app's name.
fn found_name(app: &str, path: &Path, mobile: &Path) -> String {
    if path.starts_with(mobile) && path.file_name().is_some_and(|n| n == "Documents") { return app.to_string() }
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

// ---- syncing

/// Whether macOS keeps `path` (or the nearest folder above it that exists)
/// in iCloud; this also covers Documents with 桌面与文稿 syncing on.
#[cfg(target_os = "macos")]
pub(crate) fn ubiquitous(path: &Path) -> bool {
    use objc::runtime::{BOOL, NO, Object};
    use objc::{class, msg_send, sel, sel_impl};
    #[link(name = "Foundation", kind = "framework")]
    unsafe extern "C" { static NSURLIsUbiquitousItemKey: *mut Object; }
    let mut p = path;
    while !p.exists() { match p.parent() { Some(q) => p = q, None => return false } }
    let s = p.to_string_lossy();
    unsafe {
        let ns: *mut Object = msg_send![class!(NSString), alloc];
        let ns: *mut Object = msg_send![ns, initWithBytes:s.as_ptr() length:s.len() encoding:4usize];
        let url: *mut Object = msg_send![class!(NSURL), fileURLWithPath:ns];
        let _: () = msg_send![ns, release];
        let mut value: *mut Object = std::ptr::null_mut();
        let ok: BOOL = msg_send![url, getResourceValue:&mut value forKey:NSURLIsUbiquitousItemKey error:std::ptr::null_mut::<*mut Object>()];
        if ok == NO || value.is_null() { return false }
        let on: BOOL = msg_send![value, boolValue];
        on != NO
    }
}

/// A path as people know it: 文稿 › Quiet.md, iCloud 云盘 › Notes, Dropbox › …;
/// deep ones keep their first and last two names.
/// Where a found folder lives, in a few words: its app's iCloud container
/// reads as iCloud 云盘, anything else as its short path.
fn where_short(p: &Path) -> String {
    let mobile = dirs::home_dir().unwrap_or_default().join("Library/Mobile Documents");
    if p.starts_with(&mobile) { tr("iCloud 云盘", "iCloud Drive").to_string() } else { short_path(p) }
}

fn short_path(p: &Path) -> String {
    let home = dirs::home_dir().unwrap_or_default();
    let places: [(PathBuf, &str); 5] = [
        (home.join("Library/Mobile Documents/com~apple~CloudDocs"), tr("iCloud 云盘", "iCloud Drive")),
        (dirs::document_dir().unwrap_or_else(|| home.join("Documents")), tr("文稿", "Documents")),
        (dirs::desktop_dir().unwrap_or_else(|| home.join("Desktop")), tr("桌面", "Desktop")),
        (dirs::download_dir().unwrap_or_else(|| home.join("Downloads")), tr("下载", "Downloads")),
        (home.join("Library/Mobile Documents"), "iCloud"),
    ];
    let names = |r: &Path| r.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>();
    let (base, rest): (String, Vec<String>) = if let Some((b, r)) = places.iter().find_map(|(b, n)| p.strip_prefix(b).ok().map(|r| (n.to_string(), names(r)))) { (b, r) }
        else if let Ok(r) = p.strip_prefix(home.join("Library/CloudStorage")) {
            let mut r = names(r);
            let provider = if r.is_empty() { String::new() } else { r.remove(0) };
            (provider.split('-').next().unwrap_or(&provider).to_string(), r)
        }
        else if let Ok(r) = p.strip_prefix(&home) { ("~".into(), names(r)) }
        else { (String::new(), names(p).into_iter().filter(|c| c != "/").collect()) };
    let rest = if rest.len() > 3 { vec![rest[0].clone(), "…".into(), rest[rest.len() - 2].clone(), rest[rest.len() - 1].clone()] } else { rest };
    let mut parts = vec![];
    if !base.is_empty() { parts.push(base) }
    parts.extend(rest);
    parts.join(" › ")
}

/// A picked folder as a library: an existing one or an empty folder is used
/// as it is; anything else gets a Quiet.md folder inside.
pub(crate) fn library_in(dir: PathBuf) -> PathBuf {
    let empty = std::fs::read_dir(&dir).map(|mut r| r.all(|e| e.is_ok_and(|e| e.file_name().to_string_lossy().starts_with('.')))).unwrap_or(true);
    if crate::store::Store::is_library(&dir) || empty { dir } else { dir.join("Quiet.md") }
}

// ---- the view

#[derive(Clone, Copy, PartialEq)]
enum Page { Setup, Import }

pub struct Onboarding {
    focus: FocusHandle,
    film: Option<film::Film>,
    /// The film's current frame as a texture, and which frame it is.
    film_tex: Option<(usize, std::sync::Arc<RenderImage>)>,
    opened: Instant,
    /// When the film cuts to the pages (sooner on a key or click).
    cut_at: Instant,
    /// Over a running app's window (⌘⌥⇧O, Settings): a look only, nothing is written.
    replay: bool,
    /// When the current page appeared; its parts settle in from there.
    page_at: Instant,
    page: Page,
    root: PathBuf,
    /// 界面语言: the system's at first, applied as it is picked.
    lang: crate::settings::Lang,
    /// The language menu is open, with this one under the keys.
    lang_menu: Option<crate::settings::Lang>,
    lang_anchor: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    hotkey: String,
    recording: bool,
    hotkey_error: Option<&'static str>,
    picked: Vec<PathBuf>,
    found: Option<Vec<Found>>,
    /// 再显示 was pressed: every found folder is listed.
    show_all: bool,
    chosen: Vec<bool>,
    error: Option<String>,
    scripted: bool,
    incoming: Option<crate::local_files::Incoming>,
    /// 导入并开始 is copying notes in (on a background thread); since when.
    importing: Option<Instant>,
    _ticker: Task<()>,
}

pub fn open(scripted: bool, incoming: crate::local_files::Incoming, cx: &mut App) {
    PENDING.store(true, Ordering::Relaxed);
    crate::shortcuts::init(cx);
    let options = crate::main_window_options(scripted, None, cx);
    cx.open_window(options, |window, cx| {
        crate::devtools::start(window, cx);
        cx.new(|cx| Onboarding::new(scripted, incoming, false, window, cx))
    }).expect("open window");
}

/// A replay's last page is done; the window it covers takes over again.
pub struct Done;
impl EventEmitter<Done> for Onboarding {}

impl Onboarding {
    /// ⌘⌥⇧O and Settings › About › Onboarding: the pages again, laid over a
    /// main window. Only the shortcut is applied; the library stays as it is.
    pub(crate) fn replay(window: &mut Window, cx: &mut Context<Self>) -> Self { Self::new(false, Default::default(), true, window, cx) }

    fn new(scripted: bool, incoming: crate::local_files::Incoming, replay: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus);
        let fps = FLOW_FPS;
        let ticker = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs_f32(1. / fps)).await;
            if this.update(cx, |_, cx| cx.notify()).is_err() { break }
        });
        let root = crate::store::Store::root_dir();
        let lib = root.clone();
        cx.spawn(async move |this, cx| {
            let found = cx.background_executor().spawn(async move { discover(lib) }).await;
            let _ = this.update(cx, |w, cx| { w.chosen = vec![true; found.len()]; w.found = Some(found); cx.notify(); });
        }).detach();
        let now = Instant::now();
        let cut_at = now + Duration::from_secs_f32(HOLD);
        Onboarding {
            focus,
            film: film::decode(), film_tex: None, opened: now, cut_at, replay, page_at: cut_at, page: Page::Setup, root,
            lang: crate::settings::prefs().lang(), lang_menu: None, lang_anchor: Default::default(),
            hotkey: crate::shortcuts::global_key(), recording: false, hotkey_error: None,
            picked: vec![], found: None, show_all: false, chosen: vec![], error: None, scripted, incoming: Some(incoming), importing: None, _ticker: ticker,
        }
    }

    /// A key or click while the film plays cuts to the pages now.
    fn cut(&mut self, cx: &mut Context<Self>) -> bool {
        let now = Instant::now();
        if now >= self.cut_at { return false }
        self.cut_at = now;
        self.page_at = now;
        cx.notify();
        true
    }

    fn go(&mut self, page: Page, cx: &mut Context<Self>) {
        self.stop_recording(cx);
        self.page = page;
        self.page_at = Instant::now();
        cx.notify();
    }

    /// 下一步 / 开始使用, from the button or ↩.
    fn advance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page == Page::Setup { self.go(Page::Import, cx) } else { self.finish(window, cx) }
    }

    /// 跳过: the defaults, nothing imported.
    fn skip(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.picked.clear();
        self.chosen.iter_mut().for_each(|c| *c = false);
        self.finish(window, cx);
    }

    fn pick_folder(&mut self, for_library: bool, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        let picked = crate::devtools::take_pick();
        let prompt = picked.is_none().then(|| cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some(tr("选择", "Choose").into()), extensions: None }));
        cx.spawn(async move |this, cx| {
            let paths = match prompt { Some(p) => p.await.ok().and_then(Result::ok).flatten(), None => picked };
            let Some(dir) = paths.and_then(|p| p.into_iter().next()) else { return };
            let _ = this.update(cx, |w, cx| {
                if for_library { w.root = library_in(dir); w.error = None; }
                else if !w.picked.contains(&dir) { w.picked.push(dir); }
                cx.notify();
            });
        }).detach();
    }

    fn start_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.recording = true;
        self.hotkey_error = None;
        window.focus(&self.focus);
        cx.clear_key_bindings();
        cx.notify();
    }

    fn stop_recording(&mut self, cx: &mut Context<Self>) {
        if !self.recording { return }
        self.recording = false;
        crate::shortcuts::rebuild(cx);
        cx.notify();
    }

    fn key_down(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(at) = self.lang_menu {
            use crate::settings::Lang;
            let i = Lang::ALL.iter().position(|l| *l == at).unwrap_or(0);
            match ev.keystroke.key.as_str() {
                "escape" => self.lang_menu = None,
                "up" => self.lang_menu = Some(Lang::ALL[i.saturating_sub(1)]),
                "down" => self.lang_menu = Some(Lang::ALL[(i + 1).min(Lang::ALL.len() - 1)]),
                "enter" | "space" => self.set_lang(at, cx),
                _ => {}
            }
            cx.stop_propagation();
            return cx.notify();
        }
        if !self.recording {
            if self.cut(cx) { return }
            if ev.keystroke.key == "enter" && ev.keystroke.modifiers == Modifiers::default() { self.advance(window, cx); }
            return
        }
        cx.stop_propagation();
        if ev.is_held { return }
        let k = &ev.keystroke;
        let plain = k.modifiers == Modifiers::default();
        if plain && k.key == "escape" { return self.stop_recording(cx) }
        if matches!(k.key.as_str(), "shift" | "alt" | "ctrl" | "cmd" | "fn" | "control" | "platform" | "function") { return }
        let m = k.modifiers;
        let key = k.unparse();
        if !(m.platform || m.control || m.alt) { self.hotkey_error = Some(tr("请包含 ⌘、⌃ 或 ⌥", "Include ⌘, ⌃ or ⌥")); }
        else if key == "cmd-alt-space" || key == "alt-cmd-space" { self.hotkey_error = Some(tr("⌘ ⌥ Space 用于打开上次的文档，请换一个", "⌘ ⌥ Space opens the last document; pick another")); }
        else { self.hotkey = key; self.hotkey_error = None; return self.stop_recording(cx) }
        cx.notify();
    }

    /// Show the pages in `lang` now. Saved with the library at the end; a
    /// replay saves it at once, since the library is already there.
    fn set_lang(&mut self, lang: crate::settings::Lang, cx: &mut Context<Self>) {
        self.lang_menu = None;
        self.lang = lang;
        if self.replay { save_lang(lang); } else { crate::settings::show_lang(lang); }
        cx.notify();
    }

    /// Each folder to import, and the name it takes in the library.
    fn imports(&self) -> Vec<(PathBuf, Option<String>)> {
        let mut out: Vec<_> = self.picked.iter().map(|p| (p.clone(), None)).collect();
        if let Some(found) = &self.found {
            out.extend(found.iter().zip(&self.chosen).filter(|(_, on)| **on).map(|(f, _)| (f.path.clone(), Some(f.name.clone()))));
        }
        out
    }

    /// 开始使用: settle the library, then hand over to the main window.
    fn finish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.importing.is_some() { return }
        self.stop_recording(cx);
        if self.replay {
            if cfg!(target_os = "macos") && self.hotkey != crate::shortcuts::global_key() { let _ = crate::shortcuts::set_global(&self.hotkey, cx); }
            self._ticker = Task::ready(());
            if let Some((_, t)) = self.film_tex.take() { let _ = window.drop_image(t); }
            return cx.emit(Done);
        }
        let root = self.root.clone();
        let existed = crate::guides::seeded(&root).unwrap_or(false);
        let set = crate::store::Store::set_root_dir(root.clone()).and_then(|_| if self.scripted { Ok(()) } else { crate::guides::initialize(&root) });
        if let Err(e) = set {
            self.error = Some(format!("{}{e}", tr("无法在这里创建文档库：", "Couldn't create the library here: ")));
            return cx.notify();
        }
        PENDING.store(false, Ordering::Relaxed);
        // The chosen library may already hold preferences (one synced from another Mac).
        crate::settings::load();
        save_lang(self.lang);
        crate::settings::apply_appearance(&crate::settings::prefs().appearance);
        crate::settings::sync_dark(cx);
        crate::themes::load(&crate::settings::prefs().theme_id(), false, || cx.text_system().all_font_names());
        let imports = self.imports();
        if imports.is_empty() { return self.hand_over(existed, None, window, cx); }
        // Copy the notes in before the main window opens, off the main thread,
        // so the button's spinner keeps moving.
        self.importing = Some(Instant::now());
        cx.notify();
        let work = cx.background_executor().spawn(async move { import_all(&imports) });
        cx.spawn_in(window, async move |this, cx| {
            let failed = work.await;
            let _ = this.update_in(cx, |w, window, cx| w.hand_over(existed, failed, window, cx));
        }).detach();
    }

    /// The notes take over this same window: no second window, nothing jumps.
    fn hand_over(&mut self, existed: bool, import_error: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self._ticker = Task::ready(());
        if let Some((_, t)) = self.film_tex.take() { let _ = window.drop_image(t); }
        let ws = crate::start(existed, self.scripted, Some(window), self.incoming.take().unwrap_or_default(), cx);
        let hotkey_error = (cfg!(target_os = "macos") && self.hotkey != crate::shortcuts::global_key())
            .then(|| crate::shortcuts::set_global(&self.hotkey, cx).err()).flatten().map(|e| e.to_string());
        if let Some(e) = hotkey_error.or(import_error) && let Some(ws) = ws {
            ws.update(cx, |w, cx| w.show_toast_kind(ui::ToastKind::Error, &e, cx));
        }
    }

    // ---- pages

    /// Where notes live and the quick-note shortcut: the title on the left,
    /// the two settings as cards floating in on the right.
    fn setup_page(&self, since: f32, cx: &mut Context<Self>) -> AnyElement {
        let anchor = self.lang_anchor.clone();
        let open = self.lang_menu.is_some();
        let current = self.lang;
        let lang = card(arrive(since, 0.24, 0.8))
            .child(card_icon("globe"))
            .child(card_text(tr("界面语言", "Language").into(), tr("默认跟随系统，之后可以在设置里更改", "Follows the system; you can change it in Settings").into(), false))
            .child(soft_button("onb-lang", current.name()).gap(px(6.)).when(open, |d| d.bg(Theme::fill_hover_strong()))
                .child(ui::icon("chevron-down", 12., Theme::icon()))
                .child(canvas(move |b, _, _| anchor.set(Some(b)), |_, _, _, _| {}).absolute().inset_0())
                .relative()
                .on_click(cx.listener(move |w, _, _, cx| { w.lang_menu = if open { None } else { Some(current) }; cx.notify(); })));
        let lib = card(arrive(since, 0.3, 0.8))
            .child(card_icon("folder"))
            .child(card_text(short_path(&self.root).into(), tr("笔记以文件保存在这里，在访达里也能直接打开", "Notes are saved here as files you can open in Finder").into(), false))
            .child(soft_button("onb-change", tr("更改…", "Change…")).on_click(cx.listener(|w, _, window, cx| w.pick_folder(true, window, cx))));
        let hotkey = cfg!(target_os = "macos").then(|| {
            let key = if self.recording { String::new() } else { self.hotkey.clone() };
            let (desc, error) = match (self.hotkey_error, self.recording) {
                (Some(e), _) => (e, true),
                (None, true) => (tr("按下新的组合键 · Esc 取消", "Press a new shortcut · Esc to cancel"), false),
                (None, false) => (tr("在任何应用里按下，打开一个小窗口记下想法", "Press it in any app to jot a thought in a small window"), false),
            };
            card(arrive(since, 0.42, 0.8))
                .child(card_icon("keyboard"))
                .child(card_text(tr("随手记", "Quick note").into(), desc.into(), error))
                .child(soft_button("onb-hotkey", "").min_w(px(96.)).when(self.recording, |d| d.bg(Theme::fill_hover_strong()).text_color(Theme::text_caption()))
                    .child(if self.recording { tr("按下组合键…", "Press keys…").to_string() } else { crate::shortcuts::pretty(&key) })
                    .on_click(cx.listener(|w, _, window, cx| w.start_recording(window, cx))))
        });
        // Top to bottom: the app's icon, its name, one line, then the settings.
        let over = self.film.is_some();
        let a = arrive(since, 0.05, 0.9);
        div().size_full().flex().flex_col().items_center().justify_center().px(px(48.)).pb(px(40.))
            .child(div().flex().flex_col().items_center().gap(px(12.)).opacity(a).relative().top(px(10. * (1. - a)))
                .child(div().size(px(64.)).rounded(px(15.)).overflow_hidden().shadow(theme::shadow_card()).child(img(crate::settings::about_icon()).size_full()))
                .child(lifted(over, 56., 64., true, false, "Quiet.md"))
                .child(lifted(over, 15., 23., false, true, tr("一个又快又安静的本地笔记应用。", "A fast, quiet, local notes app."))))
            .child(div().w_full().max_w(px(560.)).mt(px(40.)).flex().flex_col().gap(px(12.))
                .child(lang)
                .child(lib)
                .children(hotkey)
                .children(self.error.clone().map(|e| note_owned(e, true))))
            .children(self.lang_menu(cx))
            .into_any_element()
    }

    /// 界面语言's menu under its button, each language in its own name.
    fn lang_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::settings::Lang;
        let focused = self.lang_menu?;
        let b = self.lang_anchor.get()?;
        let rows = Lang::ALL.map(|l| {
            let g = SharedString::from(format!("onb-lang-{}", l.code()));
            ui::menu_row(g.clone(), g.clone())
                .when(l == focused, |d| d.bg(Theme::fill_hover()))
                .child(div().flex_1().min_w_0().overflow_hidden().child(ui::menu_label(l.name(), g.clone(), gpui::white())))
                .child(div().flex_none().w(px(14.)).when(l == self.lang, |d| d.child(ui::menu_glyph("check", 14., g.clone()))))
                .on_hover(cx.listener(move |w, hovered: &bool, _, cx| if *hovered && w.lang_menu != Some(l) { w.lang_menu = Some(l); cx.notify(); }))
                .on_click(cx.listener(move |w, _, _, cx| w.set_lang(l, cx)))
        });
        let anchor = self.lang_anchor.clone();
        let menu = ui::popover().id("onb-lang-menu").occlude().flex()
            .on_mouse_down_out(cx.listener(move |w, ev: &MouseDownEvent, _, cx| {
                if anchor.get().is_some_and(|b| b.contains(&ev.position)) { return }
                w.lang_menu = None;
                cx.notify();
            }))
            .child(div().w(px(160.)).py(px(ui::MENU_PAD_Y)).flex().flex_col().children(rows));
        let menu = anchored().anchor(Corner::TopRight).position(point(b.right(), b.bottom() + px(4.))).snap_to_window_with_margin(px(8.)).child(menu);
        // A replay is itself drawn deferred, over the notes, and deferred
        // drawing cannot nest: there the menu is simply the page's last child.
        Some(if self.replay { menu.into_any_element() } else { deferred(menu).with_priority(2).into_any_element() })
    }

    fn import_page(&self, since: f32, cx: &mut Context<Self>) -> AnyElement {
        let mut pick_rows = vec![
            row(tr("导入文件夹", "Import a folder"), Some(tr("Obsidian、Typora 或任何放着 Markdown 的文件夹", "Obsidian, Typora or any folder of Markdown").into()),
                soft_button("onb-pick", tr("选择…", "Choose…")).on_click(cx.listener(|w, _, window, cx| w.pick_folder(false, window, cx)))).into_any_element(),
        ];
        for (i, p) in self.picked.iter().enumerate() {
            pick_rows.push(row(short_path(p), None, soft_button(("onb-unpick", i), tr("移除", "Remove"))
                .on_click(cx.listener(move |w, _, _, cx| { if i < w.picked.len() { w.picked.remove(i); } cx.notify(); }))).into_any_element());
        }
        // Folders other apps keep notes in; nothing at all while looking or
        // when there are none. A long list shows its first few until 再显示.
        let over = self.film.is_some();
        let found = self.found.as_ref().filter(|f| !f.is_empty()).map(|found| {
            let shown = if self.show_all || found.len() <= FOUND_SHOWN + 1 { found.len() } else { FOUND_SHOWN };
            let mut rows: Vec<AnyElement> = found.iter().take(shown).enumerate().map(|(i, f)| {
                let on = self.chosen.get(i).copied().unwrap_or(false);
                let title = if f.app == "Markdown" || f.name == f.app { f.name.clone() } else { format!("{} · {}", f.app, f.name) };
                let desc = crate::localized_format!("{} · {} 篇 Markdown", "{} · {} Markdown files", where_short(&f.path), f.count);
                div().id(("onb-found", i)).cursor_pointer().hover(|s| s.bg(Theme::fill_hover()))
                    .on_click(cx.listener(move |w, _, _, cx| { if let Some(c) = w.chosen.get_mut(i) { *c = !*c; } cx.notify(); }))
                    .child(row(title, Some(desc.into()), tick(on))).into_any_element()
            }).collect();
            if shown < found.len() {
                let more = crate::localized_format!("再显示 {} 处", "Show {} more", found.len() - shown);
                rows.push(div().id("onb-more").h(px(40.)).px(px(16.)).flex().items_center().cursor_pointer().hover(|s| s.bg(Theme::fill_hover()))
                    .text_size(px(13.)).text_color(Theme::text_caption()).child(more)
                    .on_click(cx.listener(|w, _, _, cx| { w.show_all = true; cx.notify(); })).into_any_element());
            }
            // 全选 when more than one can be ticked.
            let can: Vec<usize> = (0..found.len()).collect();
            let all = !can.is_empty() && can.iter().all(|&i| self.chosen.get(i).copied().unwrap_or(false));
            let head = div().w_full().px(px(4.)).flex().items_center().justify_between()
                .child(lifted(over, 12., 16., false, true, crate::localized_format!("在这台电脑上找到 {} 处笔记", "Notes found on this computer ({})", found.len())))
                .when(can.len() > 1, |d| d.child(soft_button("onb-all", if all { tr("全不选", "Select none") } else { tr("全选", "Select all") })
                    .when(over, |b| b.bg(hsla(0., 0., 0., 0.28)).text_color(gpui::white()).hover(|s| s.bg(hsla(0., 0., 0., 0.4))))
                    .on_click(cx.listener(move |w, _, _, cx| { for &i in &can { if let Some(c) = w.chosen.get_mut(i) { *c = !all; } } cx.notify(); }))));
            (head, rows)
        });
        let a = arrive(since, 0., 0.7);
        // Auto margins centre it while it fits; taller, it starts at the top and scrolls.
        div().id("onb-scroll").size_full().overflow_y_scroll().flex().flex_col().items_center().px(px(48.)).pt(px(56.)).pb(px(88.))
            .child(div().my_auto().flex_none().w_full().max_w(px(560.)).flex().flex_col().gap(px(24.)).opacity(a).relative().top(px(10. * (1. - a)))
                .child(heading(over, tr("带上已有的笔记", "Bring your notes"), tr("导入的是副本，原来的文件不会被改动。也可以跳过，以后从菜单导入。", "Imports are copies; your files stay as they are. You can skip this and import later.")))
                .child(sheet(None, pick_rows))
                .children(found.map(|(head, rows)| sheet(Some(head.into_any_element()), rows)))
                .children(self.error.clone().map(|e| note_owned(e, true))))
            .into_any_element()
    }

    /// The frame every page shares: 跳过 top right, 上一步 bottom left, the
    /// step marks in the middle, the main button bottom right.
    fn chrome(&self, cx: &mut Context<Self>) -> Div {
        let n = self.imports().len();
        let label = match self.page { Page::Setup => tr("下一步", "Next"), Page::Import if n == 0 => tr("开始使用", "Get started"), Page::Import => tr("导入并开始", "Import and start") };
        let over = self.film.is_some();
        let mark = |on: bool| div().w(px(20.)).h(px(3.)).rounded(px(2.)).bg(match (over, on) {
            (true, true) => gpui::white(), (true, false) => hsla(0., 0., 1., 0.4), (false, true) => Theme::text(), (false, false) => Theme::fill_hover_strong(),
        });
        let soft = if over { hsla(0., 0., 1., 0.9) } else { Theme::text_caption() };
        div().absolute().inset_0()
            .when(self.importing.is_none(), |d| d.child(div().absolute().top(px(10.)).right(px(16.))
                .child(crate::settings::link_button("onb-skip", tr("跳过", "Skip")).text_color(soft).when(over, |d| d.bg(hsla(0., 0., 0., 0.28)).hover(|s| s.bg(hsla(0., 0., 0., 0.4)))).on_click(cx.listener(|w, _, window, cx| w.skip(window, cx))))))
            .child(div().absolute().left_0().right_0().bottom(px(40.)).flex().justify_center().gap(px(6.))
                .child(mark(self.page == Page::Setup)).child(mark(self.page == Page::Import)))
            .when(self.page == Page::Import && self.importing.is_none(), |d| d.child(div().absolute().left(px(24.)).bottom(px(30.))
                .child(crate::settings::link_button("onb-back", tr("‹ 上一步", "‹ Back")).text_size(px(13.)).text_color(soft).when(over, |d| d.bg(hsla(0., 0., 0., 0.28)).hover(|s| s.bg(hsla(0., 0., 0., 0.4))))
                    .on_click(cx.listener(|w, _, _, cx| w.go(Page::Setup, cx))))))
            .child(div().absolute().right(px(24.)).bottom(px(28.)).child(match self.importing {
                // The label stays (transparent) so the button keeps its width.
                Some(t) => pill("onb-done", label).cursor_default().text_color(transparent_white())
                    .child(div().absolute().inset_0().flex().items_center().justify_center().child(ui::spinner(t.elapsed().as_secs_f32(), Theme::bg_body()))),
                None => pill(if self.page == Page::Setup { "onb-next" } else { "onb-done" }, label).on_click(cx.listener(|w, _, window, cx| w.advance(window, cx))),
            }))
    }
}

/// Copy each folder into the library (one write for all of them); what went
/// wrong, for a toast in the main window.
fn import_all(dirs: &[(PathBuf, Option<String>)]) -> Option<String> {
    let mut store = match crate::store::Store::open() { Ok(s) => s, Err(e) => return Some(e.to_string()) };
    let (mut failed, batch) = (0, store.begin_batch().is_ok());
    for (dir, name) in dirs {
        match crate::transfer::scan_folder(dir, 0) {
            Some(t) if t.files > crate::transfer::IMPORT_FOLDER_LIMIT => failed += t.files,
            Some(mut t) => {
                if let Some(n) = name { t.name = n.clone(); }
                crate::transfer::import_tree(&mut store, &t, None, &mut failed);
            }
            None => {}
        }
    }
    let saved = if batch { store.end_batch() } else { Ok(()) };
    match saved {
        Err(e) => Some(crate::localized_format!("导入未能完成：{}", "Import could not finish: {}", crate::settings::diagnostic(&e.to_string()))),
        Ok(()) => (failed > 0).then(|| crate::localized_format!("有 {failed} 个文件未能导入", "{failed} files could not be imported")),
    }
}

fn heading(over: bool, title: &'static str, sub: &'static str) -> Div {
    div().flex().flex_col().items_center().gap(px(8.))
        .child(lifted(over, 34., 42., true, false, title))
        .child(lifted(over, 14., 21., false, true, sub))
}

/// Words straight on the page, in the app's own font: dark on the plain
/// backdrop; white on the film, titles glowing a little like the Quiet
/// before them, the smallest words on a dark chip so the grass can't swallow them.
fn lifted(over: bool, size: f32, line: f32, title: bool, soft: bool, text: impl Into<SharedString>) -> Div {
    let text: SharedString = text.into();
    let word = |c: Hsla| div().text_size(px(size)).line_height(px(line)).whitespace_nowrap().when(title, |d| d.font_weight(FontWeight::MEDIUM)).text_color(c).child(text.clone());
    if !over { return word(if soft { Theme::text_caption() } else { Theme::text() }) }
    if size < 14. { return div().px(px(8.)).py(px(2.)).rounded(px(6.)).bg(hsla(0., 0., 0., 0.28)).child(word(hsla(0., 0., 1., 0.95))) }
    if !title { return word(hsla(0., 0., 1., if soft { 0.95 } else { 1. })) }
    let mut d = div().relative();
    for (r, a) in [(1.2, 0.12), (2.5, 0.05)] {
        for i in 0..12 {
            let th = std::f32::consts::TAU * i as f32 / 12.;
            d = d.child(word(hsla(0., 0., 1., a)).absolute().left(px(r * th.cos())).top(px(r * th.sin())));
        }
    }
    d.child(word(gpui::white()).relative())
}

/// A tick box in the pages' one dark colour, no outline.
fn tick(on: bool) -> Div {
    div().size(px(18.)).flex_none().flex().items_center().justify_center().rounded(px(5.))
        .bg(if on { Theme::text() } else { Theme::fill_hover_strong() })
        .when(on, |d| d.child(ui::icon("todo-check", 12., Theme::bg_body())))
}

/// One row of a group: title and description on the left, a control on the right.
/// Keep `lang` in the library's preferences (a replay's, or the new one's).
fn save_lang(lang: crate::settings::Lang) {
    let mut p = crate::settings::prefs();
    if p.lang() == lang { return }
    p.set_lang(lang);
    if let Err(e) = crate::settings::save(&p) { log::warn!("onboarding: language not saved: {e:#}"); }
}

fn row(title: impl Into<SharedString>, desc: Option<SharedString>, control: impl IntoElement) -> Div {
    div().w_full().min_h(px(48.)).px(px(16.)).py(px(10.)).flex().items_center().justify_between().gap(px(16.))
        .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.))
            .child(div().text_size(px(13.)).line_height(px(20.)).text_color(Theme::text()).truncate().child(title.into()))
            .children(desc.map(|d| div().text_size(px(12.)).line_height(px(17.)).text_color(Theme::text_caption()).child(d))))
        .child(div().flex_none().flex().items_center().child(control))
}

/// A plain white card floating over the backdrop, no outline; it rises into
/// place as `a` goes 0 → 1.
fn card(a: f32) -> Div {
    div().w_full().relative().top(px(16. * (1. - a))).opacity(a)
        .px(px(16.)).py(px(14.)).flex().items_center().gap(px(14.))
        .rounded(px(16.)).bg(theme::raised(Theme::bg_body())).shadow(theme::shadow_card())
}

/// Rows on one borderless card, divided by hairlines (the import page).
fn sheet(head: Option<AnyElement>, rows: Vec<AnyElement>) -> Div {
    let n = rows.len();
    div().w_full().flex().flex_col().gap(px(8.))
        .children(head)
        .child(div().w_full().flex().flex_col().rounded(px(16.)).overflow_hidden().bg(theme::raised(Theme::bg_body())).shadow(theme::shadow_card())
            .children(rows.into_iter().enumerate().map(move |(i, r)| div().w_full().when(i + 1 < n, |d| d.border_b_1().border_color(ui::list_line())).child(r))))
}

fn card_icon(name: &str) -> Div {
    div().flex_none().size(px(36.)).rounded(px(10.)).bg(Theme::fill_hover()).flex().items_center().justify_center()
        .child(ui::icon(name, 18., Theme::text_caption()))
}

fn card_text(title: SharedString, desc: SharedString, error: bool) -> Div {
    div().flex_1().min_w_0().flex().flex_col().gap(px(2.))
        .child(div().text_size(px(13.)).line_height(px(19.)).font_weight(FontWeight::MEDIUM).text_color(Theme::text()).truncate().child(title))
        .child(div().text_size(px(12.)).line_height(px(17.)).text_color(if error { Theme::danger() } else { Theme::text_caption() }).child(desc))
}

/// Every lesser action on these pages: no outline, a grey tint that deepens
/// under the pointer. The one main action is the dark pill.
fn soft_button(id: impl Into<ElementId>, label: &'static str) -> Stateful<Div> {
    div().id(id).flex_none().h(px(28.)).px(px(12.)).flex().items_center().justify_center().rounded(px(8.)).cursor_pointer()
        .bg(Theme::fill_hover()).hover(|s| s.bg(Theme::fill_hover_strong()))
        .text_size(px(13.)).line_height(px(18.)).text_color(Theme::text()).child(crate::settings::label(label))
}

/// The main button: a dark pill with the ↩ that also presses it.
fn pill(id: &'static str, label: &'static str) -> Stateful<Div> {
    div().id(id).relative().flex_none().h(px(32.)).pl(px(14.)).pr(px(12.)).flex().items_center().gap(px(8.)).rounded(px(9.)).cursor_pointer()
        .bg(Theme::text()).hover(|s| s.opacity(0.88)).shadow(theme::shadow_button())
        .text_size(px(13.)).line_height(px(20.)).font_weight(FontWeight::MEDIUM).text_color(Theme::bg_body())
        .child(crate::settings::label(label))
        .child(div().text_size(px(12.)).opacity(0.55).child("↩"))
}

fn note_owned(text: String, error: bool) -> Div {
    div().px(px(4.)).text_size(px(12.)).line_height(px(18.)).text_color(if error { Theme::danger() } else { Theme::text_caption() }).child(text)
}

impl Render for Onboarding {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let since = now.saturating_duration_since(self.page_at).as_secs_f32();
        let t = now.duration_since(self.opened).as_secs_f32();
        // Before the cut the film is alone; after it, the pages sit on it.
        let film_up = now < self.cut_at && self.film.is_some();
        if film_up || since < 1.3 || self.importing.is_some() { window.request_animation_frame(); }
        // Without the film (it failed to decode) the pages sit on the drifting gradient.
        let backdrop = canvas(|b, _, _| b, move |_, b, window, _| paint_backdrop(b, t, window)).absolute().inset_0();
        let page = if self.page == Page::Setup { self.setup_page(since, cx) } else { self.import_page(since, cx) };
        // A new texture only when the film reaches another frame; the old one leaves the GPU.
        if let Some(f) = &self.film {
            let n = f.frames.len();
            let k = (t * film::FILM_FPS) as usize % (2 * n - 2).max(1);
            let k = if k < n { k } else { 2 * n - 2 - k };
            if self.film_tex.as_ref().is_none_or(|(i, _)| *i != k) {
                let tex = film::texture(&f.frames[k], &f.colors);
                if let Some((_, old)) = self.film_tex.replace((k, tex)) { let _ = window.drop_image(old); }
            }
        }
        let frame = self.film_tex.as_ref().map(|(_, t)| t.clone());
        // Quiet glows in as the film ends.
        let mark = if film_up { ((t - MARK_IN.0) / (MARK_IN.1 - MARK_IN.0)).clamp(0., 1.) } else { 0. };
        div().id("onboarding").size_full().relative().overflow_hidden()
            .font_family(theme::UI_FONT).text_color(Theme::text())
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|w, ev: &KeyDownEvent, window, cx| w.key_down(ev, window, cx)))
            .when(frame.is_none(), |d| d.child(backdrop))
            .children(frame.map(|tex| canvas(|b, _, _| b, move |_, b, window, _| film::paint_texture(tex, b, window)).absolute().inset_0()))
            .when(!film_up, |d| d.child(div().absolute().inset_0().child(page)).child(self.chrome(cx)))
            // The film alone: a click anywhere brings the pages now.
            .when(film_up, |d| d.child(div().id("onb-cut").absolute().inset_0().occlude().on_mouse_down(MouseButton::Left, cx.listener(|w, _, _, cx| { w.cut(cx); }))))
            .when(mark > 0., |d| d.child(film::wordmark(mark)))
    }
}

#[cfg(test)]
mod tests {
    use super::found_name;
    use std::path::Path;

    #[test]
    fn icloud_app_folders_go_by_the_app() {
        let m = Path::new("/Users/me/Library/Mobile Documents");
        assert_eq!(found_name("iA Writer", &m.join("27N4MQEA55~pro~writer/Documents"), m), "iA Writer");
        assert_eq!(found_name("Typora", &m.join("iCloud~abnerworks~Typora/Documents"), m), "Typora");
        assert_eq!(found_name("Obsidian", &m.join("iCloud~md~obsidian/Documents/Thesis"), m), "Thesis");
        assert_eq!(found_name("Obsidian", Path::new("/Users/me/Documents"), m), "Documents");
    }
}
