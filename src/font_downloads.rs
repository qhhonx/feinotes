//! Fonts fetched on demand (docs/FONTS.md). `packaging/fonts.json` lists
//! them: the board's CJK handwriting face Xiaolai (14 MB compressed, which
//! shipped builds no longer carry: build.rs, `embed-xiaolai`) and the faces
//! 设置 › 文档字体 offers to download. Nothing is fetched until it is needed:
//! Xiaolai when a board has CJK text Excalifont lacks (or copies a picture of
//! it), a document font when the reader picks it.
//!
//! Each file is fetched with `curl`, as updates are (`update::curl_command`:
//! HTTPS only, the system's proxy settings), from
//! `https://download.quiet.md/fonts/<name>`, a name that carries the file's
//! SHA-256, which the catalog (compiled in) holds in full: a file that
//! differs is thrown away. tools/fonts/stage.py builds the files from their
//! upstream releases and scripts/publish-downloads.sh uploads them before a
//! release. Verified files are kept in the app's data folder (not the
//! library), shared by the board and the document fonts, and checked again
//! when loaded; a copy that no longer verifies is fetched again (or, for a
//! document font, offered again). One fetch per font at a time; a failure
//! is silent and tried again only when needed after a growing pause (`Gate`).
//!
//! `Changed` says a font arrived (or failed); `Tick` repaints progress while
//! anything downloads. `QUIET_MD_FONT_URL` points at another folder (testing:
//! http and any host allowed, and a compiled-in Xiaolai is ignored),
//! `QUIET_MD_FONT_DIR` replaces the cache folder, `QUIET_MD_TRIAL_FONTS=1`
//! lists the trial fonts (as preview builds do).

use rustc_hash::{FxHashMap, FxHashSet};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::Duration;
use web_time::Instant;

const BASE: &str = "https://download.quiet.md/fonts/";
/// Whether release builds list the fonts marked `trial` (being tried out
/// before they are offered to everyone).
pub const TRIAL_FONTS: bool = false;
pub const XIAOLAI: &str = "xiaolai";

#[derive(Deserialize, Debug)]
pub struct Entry {
    pub id: String,
    /// The family name its files carry (what GPUI looks it up by).
    pub family: String,
    pub zh: String,
    pub en: String,
    #[serde(default)]
    pub group: Group,
    /// Where 设置's font list files it: hei, song, kai, yuan, mono, latin-serif, latin-sans.
    #[serde(default)]
    pub class: String,
    #[serde(default)]
    pub trial: bool,
    pub files: Vec<File>,
}

#[derive(Deserialize, Debug, Default, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Group { #[default] Cjk, Latin, Mono }

#[derive(Deserialize, Debug)]
pub struct File {
    pub name: String,
    pub sha256: String,
    pub size: u64,
    /// Stored deflated (Xiaolai).
    #[serde(default)]
    pub zlib: bool,
}

impl Entry {
    pub fn class(&self) -> crate::font_classes::Class {
        use crate::font_classes::Class;
        match self.class.as_str() { "hei" => Class::Hei, "song" => Class::Song, "kai" => Class::Kai, "yuan" => Class::Yuan, "mono" => Class::Mono, "latin-serif" => Class::LatinSerif, "latin-sans" => Class::LatinSans, _ => Class::Other }
    }
    pub fn size(&self) -> u64 { self.files.iter().map(|f| f.size).sum() }
    pub fn name(&self) -> &str { if matches!(crate::settings::lang(), crate::settings::Lang::Zh | crate::settings::Lang::ZhTw) { &self.zh } else { &self.en } }
}

pub fn parse(json: &str) -> Result<Vec<Entry>, serde_json::Error> {
    #[derive(Deserialize)]
    struct Catalog { fonts: Vec<Entry> }
    serde_json::from_str::<Catalog>(json).map(|c| c.fonts)
}

pub fn catalog() -> &'static [Entry] {
    static C: OnceLock<Vec<Entry>> = OnceLock::new();
    C.get_or_init(|| parse(include_str!("../packaging/fonts.json")).expect("packaging/fonts.json"))
}

pub fn entry(id: &str) -> Option<&'static Entry> { catalog().iter().find(|e| e.id == id) }

/// The catalog font a family name belongs to, ignoring case.
pub fn by_family(family: &str) -> Option<&'static Entry> {
    let family = family.trim();
    catalog().iter().find(|e| e.family.eq_ignore_ascii_case(family))
}

/// Whether the trial fonts are listed: `TRIAL_FONTS`, `QUIET_MD_TRIAL_FONTS=1`,
/// or a preview build (scripts/preview.sh's `Quiet.md Preview….app`).
pub fn trials_shown() -> bool {
    static T: OnceLock<bool> = OnceLock::new();
    *T.get_or_init(|| TRIAL_FONTS || std::env::var("QUIET_MD_TRIAL_FONTS").is_ok_and(|v| v == "1")
        || std::env::current_exe().is_ok_and(|p| p.to_string_lossy().contains("Quiet.md Preview")))
}

/// The document fonts 设置 offers, in catalog order.
pub fn listed() -> impl Iterator<Item = &'static Entry> { catalog().iter().filter(|e| !e.trial || trials_shown()) }

/// The file's bytes are this file: size and SHA-256 match.
pub fn verified(f: &File, bytes: &[u8]) -> bool { bytes.len() as u64 == f.size && hex(&Sha256::digest(bytes)) == f.sha256 }

fn hex(b: &[u8]) -> String { b.iter().map(|b| format!("{b:02x}")).collect() }

/// When a failed fetch may be tried again: 15 s, doubling to an hour.
#[derive(Default)]
pub struct Gate { failures: u32, next: Option<Instant> }
impl Gate {
    pub fn open(&self, now: Instant) -> bool { self.next.is_none_or(|t| now >= t) }
    pub fn failed(&mut self, now: Instant) {
        self.failures += 1;
        self.next = Some(now + Duration::from_secs(15u64 << (self.failures - 1).min(8)).min(Duration::from_secs(3600)));
    }
}

#[derive(Default)]
struct Slot { busy: bool, gate: Gate, fonts: Option<Vec<&'static [u8]>>, on_disk: Option<bool> }
static SLOTS: LazyLock<Mutex<FxHashMap<String, Slot>>> = LazyLock::new(Mutex::default);
static REGISTERED: LazyLock<Mutex<FxHashSet<String>>> = LazyLock::new(Mutex::default);

fn with<R>(id: &str, f: impl FnOnce(&mut Slot) -> R) -> R { f(SLOTS.lock().unwrap().entry(id.to_string()).or_default()) }

thread_local! {
    /// Set by `init` on the main thread: where a fetch reports back.
    static APP: std::cell::RefCell<Option<gpui::AsyncApp>> = const { std::cell::RefCell::new(None) };
}

/// Lets `want` report back to the app (main thread, at launch).
pub fn init(cx: &gpui::App) { APP.with(|a| *a.borrow_mut() = Some(cx.to_async())); }

fn override_url() -> Option<&'static str> {
    static U: OnceLock<Option<String>> = OnceLock::new();
    U.get_or_init(|| std::env::var("QUIET_MD_FONT_URL").ok().filter(|u| !u.is_empty())).as_deref()
}

/// Xiaolai compiled in (`embed-xiaolai`), unless a test points elsewhere.
fn embedded_xiaolai() -> Option<&'static [u8]> {
    if override_url().is_some() { return None; }
    #[cfg(test)]
    { Some(include_bytes!("../assets/fonts/excalidraw/Xiaolai-Regular.ttf.zlib")) }
    #[cfg(not(test))]
    { crate::assets::FILES.iter().find(|(p, _)| *p == "fonts/excalidraw/Xiaolai-Regular.ttf.zlib").map(|(_, b)| *b) }
}

fn inflate(z: &[u8]) -> Option<&'static [u8]> {
    miniz_oxide::inflate::decompress_to_vec_zlib(z).ok().map(|v| &*Box::leak(v.into_boxed_slice()))
}

/// A font's files (TrueType / OpenType), once it is here.
pub fn fonts(id: &str) -> Option<Vec<&'static [u8]>> {
    if let Some(f) = with(id, |s| s.fonts.clone()) { return Some(f); }
    if id != XIAOLAI { return None; }
    static EMBEDDED: OnceLock<Option<&'static [u8]>> = OnceLock::new();
    let f = (*EMBEDDED.get_or_init(|| embedded_xiaolai().and_then(inflate)))?;
    with(id, |s| s.fonts = Some(vec![f]));
    Some(vec![f])
}

/// A one-file font's bytes (Xiaolai).
pub fn font(id: &str) -> Option<&'static [u8]> { fonts(id).and_then(|f| f.first().copied()) }

/// Whether a fetch is under way.
pub fn pending(id: &str) -> bool { with(id, |s| s.busy) }

/// Whether its verified files are in the cache (checked by size once, then
/// kept up to date as they arrive or are deleted).
pub fn on_disk(id: &str) -> bool {
    if let Some(d) = with(id, |s| s.on_disk) { return d; }
    let d = entry(id).is_some_and(|e| e.files.iter().all(|f| std::fs::metadata(cache_dir().join(&f.name)).is_ok_and(|m| m.len() == f.size)));
    with(id, |s| s.on_disk = Some(d));
    d
}

/// How much of a fetch has arrived, 0–1 (read off the cache folder).
pub fn progress(id: &str) -> f32 {
    let Some(e) = entry(id) else { return 0. };
    let dir = cache_dir();
    let got: u64 = e.files.iter().map(|f| {
        std::fs::metadata(dir.join(&f.name)).map(|m| m.len()).ok().filter(|&n| n == f.size)
            .unwrap_or_else(|| std::fs::metadata(part_path(&dir.join(&f.name))).map_or(0, |m| m.len()))
    }).sum();
    if e.size() == 0 { 0. } else { (got as f32 / e.size() as f32).clamp(0., 1.) }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    /// Only what the cache holds (a document font at launch: a missing one is
    /// offered again, not fetched behind the reader's back).
    Cache,
    /// The cache, else the download site, unless a recent attempt failed.
    Fetch,
    /// `Fetch` for something the user asked for: a recent failure doesn't hold it back.
    Now,
}

/// A font arrived (`ok`), or could not be had (`error`: a failed fetch;
/// None: not in the cache, `Mode::Cache`). `n` counts the events.
#[derive(Clone, Debug)]
pub struct Changed { pub id: String, pub ok: bool, pub error: Option<String>, pub n: u32 }
impl gpui::Global for Changed {}

/// Set every 250 ms while a font downloads (progress shown).
pub struct Tick;
impl gpui::Global for Tick {}

/// Get font `id`'s files unless they are here, on their way, or (`Fetch`) a
/// recent attempt failed. Main thread only (elsewhere a no-op).
pub fn want(id: &str, mode: Mode) {
    if cfg!(target_family = "wasm") || fonts(id).is_some() || entry(id).is_none() { return; }
    let Some(app) = APP.with(|a| a.borrow().clone()) else { return };
    let go = with(id, |s| {
        if s.busy || mode == Mode::Fetch && !s.gate.open(Instant::now()) { return false; }
        s.busy = true;
        true
    });
    if !go { return; }
    use gpui::AppContext as _;
    let id = id.to_string();
    let fetch = mode != Mode::Cache;
    app.spawn(async move |cx| {
        let task = { let id = id.clone(); cx.background_spawn(async move { obtain(&id, fetch) }) };
        let got = task.await;
        cx.update(|cx| arrived(&id, got, fetch, cx));
    }).detach();
    if fetch { tick(&app); }
}

/// Repaints progress (`Tick`) while anything downloads.
fn tick(app: &gpui::AsyncApp) {
    static TICKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if TICKING.swap(true, std::sync::atomic::Ordering::Relaxed) { return; }
    app.spawn(async move |cx| {
        while SLOTS.lock().unwrap().values().any(|s| s.busy) {
            cx.background_executor().timer(Duration::from_millis(250)).await;
            cx.update(|cx| cx.set_global(Tick));
        }
        TICKING.store(false, std::sync::atomic::Ordering::Relaxed);
    }).detach();
}

/// Back on the main thread: publish the font, or note the failure.
fn arrived(id: &str, got: Result<Vec<&'static [u8]>, Option<String>>, fetch: bool, cx: &mut gpui::App) {
    let ok = got.is_ok();
    let error = with(id, |s| {
        s.busy = false;
        match got {
            Ok(f) => { s.fonts = Some(f); s.on_disk = Some(true); None }
            Err(e) => {
                if e.is_some() && fetch { s.gate.failed(Instant::now()); }
                if e.is_none() { s.on_disk = Some(false); }
                e
            }
        }
    });
    match &error { Some(e) => log::info!("font {id}: {e}"), None if ok => log::info!("font {id} ready"), None => {} }
    if ok {
        if id == XIAOLAI { fdl_core::board::text_measure::reset_char_width_cache(); }
        // Document fonts register now (Xiaolai when a board first needs it).
        if id != XIAOLAI { register(id, cx.text_system()); }
    }
    let n = cx.try_global::<Changed>().map_or(0, |c| c.n) + 1;
    cx.set_global(Changed { id: id.into(), ok, error, n });
    cx.refresh_windows();
}

/// Adds font `id` to a text system, once; false while it isn't here.
pub fn register(id: &str, ts: &gpui::TextSystem) -> bool {
    if REGISTERED.lock().unwrap().contains(id) { return true; }
    let Some(f) = fonts(id) else { return false };
    if let Err(e) = ts.add_fonts(f.into_iter().map(Cow::Borrowed).collect()) { log::warn!("font {id}: {e}"); return false; }
    REGISTERED.lock().unwrap().insert(id.to_string());
    true
}

/// Whether this run added font `id` to GPUI.
pub fn registered(id: &str) -> bool { REGISTERED.lock().unwrap().contains(id) }

/// Deletes font `id`'s cached files. GPUI cannot drop a font, so what this
/// run registered keeps drawing until it quits; a document font is fetched
/// again when picked again, Xiaolai stays loaded for the open boards.
pub fn delete(id: &str) -> std::io::Result<()> {
    let Some(e) = entry(id) else { return Ok(()) };
    let dir = cache_dir();
    for f in &e.files {
        match std::fs::remove_file(dir.join(&f.name)) { Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e), _ => {} }
    }
    with(id, |s| { s.on_disk = Some(false); if id != XIAOLAI { s.fonts = None; } });
    Ok(())
}

/// Where the verified files are kept.
pub fn cache_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("QUIET_MD_FONT_DIR") { return PathBuf::from(d); }
    dirs::data_local_dir().unwrap_or_else(fdl_core::vfs::temp_dir).join("quiet.md").join("fonts")
}

fn part_path(path: &Path) -> PathBuf { path.with_extension(format!("{}.part", fdl_core::vfs::process_id())) }

/// The cached file if it still verifies (else it is deleted).
pub fn cached(f: &File, path: &Path) -> Option<Vec<u8>> {
    let b = std::fs::read(path).ok()?;
    if verified(f, &b) { return Some(b); }
    let _ = std::fs::remove_file(path);
    None
}

/// Every file of font `id`: the cache, else (`fetch`) the download site;
/// verified and unpacked (off the main thread). Err(None): not cached.
fn obtain(id: &str, fetch: bool) -> Result<Vec<&'static [u8]>, Option<String>> {
    let e = entry(id).ok_or(None)?;
    let dir = cache_dir();
    let mut out = vec![];
    for f in &e.files {
        let path = dir.join(&f.name);
        let b = match cached(f, &path) {
            Some(b) => b,
            None if !fetch => return Err(None),
            None => {
                std::fs::create_dir_all(&dir).map_err(|e| Some(e.to_string()))?;
                let base = override_url().unwrap_or(BASE);
                download(f, &format!("{}/{}", base.trim_end_matches('/'), f.name), &path, override_url().is_some()).map_err(Some)?
            }
        };
        out.push(if f.zlib { inflate(&b).ok_or_else(|| Some("inflate".to_string()))? } else { &*Box::leak(b.into_boxed_slice()) });
    }
    Ok(out)
}

/// Fetches `url` into `path` through a part file of this process's own (two
/// running copies may fetch at once; the rename is atomic).
fn download(f: &File, url: &str, path: &Path, testing: bool) -> Result<Vec<u8>, String> {
    let part = part_path(path);
    let keep = (|| {
        #[cfg(target_os = "ios")]
        if !crate::editor::image::download_ios(url, &part, !testing) { return Err("URLSession download failed".into()); }
        #[cfg(not(target_os = "ios"))]
        {
            // curl gets no path: see update::fetch_package.
            let out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
            let result = crate::update::curl_command(testing).args(["--max-time", "1800", url]).stdin(std::process::Stdio::null())
                .stdout(out).stderr(std::process::Stdio::piped()).output().map_err(|e| format!("curl: {e}"))?;
            if !result.status.success() { return Err(String::from_utf8_lossy(&result.stderr).trim().to_string()); }
        }
        let b = std::fs::read(&part).map_err(|e| e.to_string())?;
        if !verified(f, &b) { return Err(format!("{url}: content does not match")); }
        std::fs::rename(&part, path).map_err(|e| e.to_string())?;
        Ok(b)
    })();
    if keep.is_err() { let _ = std::fs::remove_file(&part); }
    keep
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xiaolai_file() -> &'static [u8] { include_bytes!("../assets/fonts/excalidraw/Xiaolai-Regular.ttf.zlib") }
    fn xiaolai() -> &'static File { &entry(XIAOLAI).unwrap().files[0] }

    /// Every entry is well formed: ids and families unique, names carry the
    /// first 12 hex digits of their hash, Xiaolai is the assets file.
    #[test]
    fn the_catalog_parses_and_names_its_files_by_hash() {
        let c = catalog();
        assert!(c.len() >= 3);
        let ids: FxHashSet<&str> = c.iter().map(|e| e.id.as_str()).collect();
        let fams: FxHashSet<String> = c.iter().map(|e| e.family.to_lowercase()).collect();
        assert_eq!((ids.len(), fams.len()), (c.len(), c.len()));
        for e in c {
            assert!(!e.files.is_empty() && !e.zh.is_empty() && !e.en.is_empty(), "{}", e.id);
            assert_ne!(e.class(), crate::font_classes::Class::Other, "{} has a class", e.id);
            for f in &e.files {
                assert_eq!(f.sha256.len(), 64, "{}", f.name);
                assert!(f.name.contains(&format!("-{}.", &f.sha256[..12])), "{} carries its hash", f.name);
                assert!(f.size > 0);
            }
        }
        assert!(verified(xiaolai(), xiaolai_file()), "update packaging/fonts.json after changing the font");
        assert!(xiaolai().zlib && inflate(xiaolai_file()).is_some_and(|f| f.starts_with(&[0, 1, 0, 0])));
        // the release lists only the fonts no longer on trial
        const { assert!(!TRIAL_FONTS) };
        assert!(["xiaolai", "lxgw-wenkai", "zhuque-fangsong"].iter().all(|id| entry(id).is_some_and(|e| !e.trial)));
        assert!(parse(r#"{"fonts":[{"id":"a","family":"A","zh":"甲","en":"A","files":[]}]}"#).is_ok());
        assert!(parse(r#"{"fonts":[{"id":"a"}]}"#).is_err());
    }

    #[test]
    fn altered_or_cut_files_fail_verification() {
        let f = xiaolai();
        let mut bad = xiaolai_file().to_vec();
        bad[f.size as usize / 2] ^= 1;
        assert!(!verified(f, &bad));
        assert!(!verified(f, &xiaolai_file()[..xiaolai_file().len() - 1]));
        assert!(!verified(f, &[]));
        let mut longer = xiaolai_file().to_vec();
        longer.push(0);
        assert!(!verified(f, &longer));
    }

    #[test]
    fn the_cache_is_used_while_it_verifies() {
        let dir = std::env::temp_dir().join(format!("quiet-md-font-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (f, path) = (xiaolai(), dir.join(&xiaolai().name));
        std::fs::write(&path, xiaolai_file()).unwrap();
        assert_eq!(cached(f, &path).as_deref(), Some(xiaolai_file()));
        // a damaged copy is dropped, so the next need fetches it again
        let mut bad = xiaolai_file().to_vec();
        bad[10] ^= 0xff;
        std::fs::write(&path, &bad).unwrap();
        assert!(cached(f, &path).is_none() && !path.exists());
        std::fs::write(&path, &xiaolai_file()[..1000]).unwrap();
        assert!(cached(f, &path).is_none() && !path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn failures_back_off() {
        let t0 = Instant::now();
        let mut g = Gate::default();
        assert!(g.open(t0));
        g.failed(t0);
        assert!(!g.open(t0 + Duration::from_secs(14)) && g.open(t0 + Duration::from_secs(15)));
        g.failed(t0);
        assert!(!g.open(t0 + Duration::from_secs(29)) && g.open(t0 + Duration::from_secs(30)));
        for _ in 0..20 { g.failed(t0); }
        assert!(!g.open(t0 + Duration::from_secs(3599)) && g.open(t0 + Duration::from_secs(3600)));
    }

    /// A served file is fetched, verified and kept; a wrong one is refused
    /// and leaves nothing behind; no server fails quietly.
    #[test]
    #[cfg(unix)]
    fn downloads_verify_before_they_are_kept() {
        use std::io::{Read, Write};
        let dir = std::env::temp_dir().join(format!("quiet-md-font-dl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let serve = |body: Vec<u8>| {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = l.local_addr().unwrap().port();
            std::thread::spawn(move || {
                let (mut s, _) = l.accept().unwrap();
                let mut req = [0u8; 1024];
                let _ = s.read(&mut req);
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = s.write_all(&body);
            });
            format!("http://127.0.0.1:{port}/x")
        };
        let (f, path) = (xiaolai(), dir.join(&xiaolai().name));
        let mut bad = xiaolai_file().to_vec();
        bad[0] ^= 1;
        assert!(download(f, &serve(bad), &path, true).is_err());
        assert!(std::fs::read_dir(&dir).unwrap().next().is_none(), "nothing kept");
        assert_eq!(download(f, &serve(xiaolai_file().to_vec()), &path, true).unwrap(), xiaolai_file());
        assert_eq!(cached(f, &path).as_deref(), Some(xiaolai_file()));
        let closed = { let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() };
        assert!(download(f, &format!("http://127.0.0.1:{closed}/x"), &dir.join("other"), true).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
