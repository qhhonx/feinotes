//! The web build (wasm32): the site's live demo. A sample library is fetched
//! as one bundle and kept in memory (`fdl_core::vfs`), so edits last until the
//! page reloads. The browser has no system fonts: the UI font and the
//! monospace font are fetched too, and CJK text falls back to the browser's
//! canvas text (gpui_web).

use crate::*;
use std::borrow::Cow;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen::prelude::wasm_bindgen(inline_js = r#"
export function pickQuietDocuments(accept) {
  return new Promise((resolve, reject) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = accept;
    input.multiple = true;
    input.hidden = true;
    const settle = files => { input.remove(); resolve(files); };
    input.onchange = () => settle(Array.from(input.files));
    input.oncancel = () => settle([]);
    document.body.append(input);
    try { if (input.showPicker) input.showPicker(); else input.click(); }
    catch (error) { input.remove(); reject(error); }
  });
}
"#)]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = pickQuietDocuments)]
    fn pick_documents(accept: &str) -> js_sys::Promise;
}

pub(crate) struct ImportStaging(std::path::PathBuf);

impl Drop for ImportStaging {
    fn drop(&mut self) { let _ = fdl_core::vfs::remove_dir_all(&self.0); }
}

/// Open the browser picker while the click still has user activation.
pub(crate) fn select_documents(accept: &str) -> JsFuture {
    JsFuture::from(pick_documents(accept))
}

/// Copy selected files into an isolated in-memory directory for the regular importer.
pub(crate) async fn stage_documents(files: JsFuture, max_bytes: u64)
    -> Result<Option<(std::sync::Arc<ImportStaging>, Vec<std::path::PathBuf>)>, wasm_bindgen::JsValue> {
    use std::path::{Path, PathBuf};
    use wasm_bindgen::JsValue;
    let files = files.await?.dyn_into::<js_sys::Array>()?;
    if files.length() == 0 { return Ok(None) }
    let staging = std::sync::Arc::new(ImportStaging(PathBuf::from("/.quiet-web-import").join(uuid::Uuid::new_v4().to_string())));
    let mut paths = Vec::new();
    let mut total = 0.;
    for (index, item) in files.iter().enumerate() {
        let file = item.dyn_into::<web_sys::File>()?;
        let name = file.name();
        let component = Path::new(&name).file_name().and_then(|name| name.to_str());
        if component != Some(name.as_str()) || name.contains('\\') || file.size() > max_bytes as f64 {
            return Err(JsValue::from_str("invalid or oversized document file"));
        }
        total += file.size();
        if total > (max_bytes * 2) as f64 { return Err(JsValue::from_str("document selection is too large")); }
        let buffer = JsFuture::from(file.array_buffer()).await?;
        let folder = staging.0.join(index.to_string());
        let path = folder.join(name);
        fdl_core::vfs::create_dir_all(folder).and_then(|_| fdl_core::vfs::write(&path, js_sys::Uint8Array::new(&buffer).to_vec()))
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        paths.push(path);
    }
    Ok(Some((staging, paths)))
}

/// Fonts next to the page, in `fonts/`: what the first screen draws with
/// (web/index.html preloads these, alongside the wasm)…
const FONTS: &[&str] = &["Inter-Regular.ttf", "Inter-Medium.ttf", "Inter-SemiBold.ttf", "Inter-Bold.ttf", "JetBrainsMono-Regular.ttf"];
/// …and the rest, fetched once the window is up (text in them is laid out again as they arrive).
const LATER_FONTS: &[&str] = &["Inter-Italic.ttf", "Inter-MediumItalic.ttf", "Inter-SemiBoldItalic.ttf", "Inter-BoldItalic.ttf", "JetBrainsMono-Italic.ttf", "JetBrainsMono-Bold.ttf"];

async fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let window = web_sys::window().ok_or("no window")?;
    let resp: web_sys::Response = JsFuture::from(window.fetch_with_str(url)).await
        .map_err(|e| format!("{url}: {e:?}"))?.dyn_into().map_err(|_| format!("{url}: not a response"))?;
    if !resp.ok() { return Err(format!("{url}: HTTP {}", resp.status())) }
    let buf = JsFuture::from(resp.array_buffer().map_err(|e| format!("{e:?}"))?).await.map_err(|e| format!("{url}: {e:?}"))?;
    Ok(js_sys::Uint8Array::new(&buf).to_vec())
}

/// `?lib=<name>` picks `libraries/<name>.qmdb`; the page can also set
/// `window.QUIET_MD_LIBRARY`.
fn library_url() -> String {
    let window = web_sys::window();
    let from_global = window.as_ref().and_then(|w| js_sys::Reflect::get(w, &"QUIET_MD_LIBRARY".into()).ok()).and_then(|v| v.as_string());
    let from_query = window.and_then(|w| w.location().search().ok())
        .and_then(|q| web_sys::UrlSearchParams::new_with_str(&q).ok()).and_then(|p| p.get("lib"))
        .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    let name = from_query.or(from_global).unwrap_or_else(|| "han-meimei".into());
    format!("libraries/{name}.qmdb")
}

/// A bundle is a run of `[u32 LE path length][path][u32 LE data length][data]`,
/// paths relative to the library folder (see web/bundle.py); `QMZ1` and a zlib
/// stream of that run when it is packed (static hosts don't compress it).
fn unpack(bytes: &[u8], root: &std::path::Path) -> Result<usize, String> {
    let inflated;
    let bytes = match bytes.strip_prefix(b"QMZ1") {
        Some(z) => { inflated = miniz_oxide::inflate::decompress_to_vec_zlib(z).map_err(|e| format!("bundle: {e:?}"))?; &inflated[..] }
        None => bytes,
    };
    let (mut at, mut n) = (0usize, 0usize);
    let take = |len: usize, at: &mut usize| -> Result<std::ops::Range<usize>, String> {
        let r = *at..*at + len;
        if r.end > bytes.len() { return Err("truncated bundle".into()) }
        *at = r.end;
        Ok(r)
    };
    while at < bytes.len() {
        let len = |r: std::ops::Range<usize>| u32::from_le_bytes(bytes[r].try_into().unwrap()) as usize;
        let l = len(take(4, &mut at)?);
        let path = std::str::from_utf8(&bytes[take(l, &mut at)?]).map_err(|_| "bad path")?.to_string();
        let l = len(take(4, &mut at)?);
        let data = bytes[take(l, &mut at)?].to_vec();
        if path.split('/').any(|c| c == "..") { continue }
        fdl_core::vfs::seed(&root.join(&path), data);
        n += 1;
    }
    Ok(n)
}

/// Pictures stay on the server until shown (`vfs::add_lazy`): the bundle
/// lists them in `.web/lazy`, one library-relative path a line, served next to
/// the bundle under `libraries/<name>/`.
fn register_lazy(root: &std::path::Path, base: &str) {
    let Ok(list) = fdl_core::vfs::read_to_string(root.join(".web/lazy")) else { return };
    for rel in list.lines().filter(|l| !l.is_empty()) {
        let url = format!("{base}/{}", rel.split('/').map(|c| String::from(js_sys::encode_uri_component(c))).collect::<Vec<_>>().join("/"));
        fdl_core::vfs::add_lazy(&root.join(rel), url);
    }
    fdl_core::vfs::set_fetcher(|path, url| wasm_bindgen_futures::spawn_local(async move {
        match fetch(&url).await {
            Ok(bytes) => { fdl_core::vfs::seed(&path, bytes); ARRIVED.store(true, std::sync::atomic::Ordering::Relaxed); }
            Err(e) => log::warn!("{e}"),
        }
    }));
}

/// Set when a lazy file arrived: the windows redraw to show it.
static ARRIVED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn status(text: &str) {
    if let Some(el) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id("status")) { el.set_text_content(Some(text)); }
}

pub fn main() {
    gpui_platform::web_init();
    status("Loading…");
    wasm_bindgen_futures::spawn_local(async {
        let root = store::Store::active_dir();
        let url = library_url();
        let (library, fonts) = futures::join!(fetch(&url), futures::future::join_all(FONTS.iter().map(|f| fetch_font(f))));
        match library.and_then(|b| unpack(&b, &root)) {
            Ok(n) => log::info!("{n} files from {url}"),
            Err(e) => { status(&e); log::error!("{e}"); return }
        }
        register_lazy(&root, url.trim_end_matches(".qmdb"));
        let fonts: Vec<Cow<'static, [u8]>> = fonts.into_iter().flatten().map(Cow::Owned).collect();
        status("");
        run(fonts);
    });
}

async fn fetch_font(name: &str) -> Option<Vec<u8>> {
    fetch(&format!("fonts/{name}")).await.map_err(|e| log::warn!("{e}")).ok()
}

fn run(fonts: Vec<Cow<'static, [u8]>>) {
    // Single-threaded: no cross-origin isolation needed on the site. CJK
    // text borrows the browser's fonts through canvas.
    let platform = gpui_web::WebPlatform::new_with_system_font(false, gpui_web::WebBackendPreference::Auto, gpui_web::CanvasFontFallback::EmojiAndCjk, theme::SYSTEM_FONT);
    let app = Application::with_platform(std::rc::Rc::new(platform)).with_assets(assets::Assets);
    app.run(move |cx: &mut App| {
        if let Err(e) = cx.text_system().add_fonts(fonts) { log::warn!("fonts: {e}") }
        init_app(cx);
        start(true, false, None, local_files::Incoming::default(), cx);
        cx.spawn(async |cx| loop {
            cx.background_executor().timer(std::time::Duration::from_millis(150)).await;
            if ARRIVED.swap(false, std::sync::atomic::Ordering::Relaxed) { cx.update(|cx| cx.refresh_windows()) }
        }).detach();
        // italics and the rest of the monospace family: after the first frame
        cx.spawn(async |cx| {
            let fonts: Vec<Cow<'static, [u8]>> = futures::future::join_all(LATER_FONTS.iter().map(|f| fetch_font(f))).await.into_iter().flatten().map(Cow::Owned).collect();
            cx.update(|cx| { if let Err(e) = cx.text_system().add_fonts(fonts) { log::warn!("fonts: {e}") } cx.refresh_windows(); });
        }).detach();
    });
}
