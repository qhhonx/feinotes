//! Mermaid diagrams (#23). A code block whose language is Mermaid shows its
//! diagram unless switched to its code (`show_code`, saved as `view="code"`).
//! The bundled mermaid.js (assets/mermaid, v11.17.2, stored deflated) runs in
//! an off-screen web view — WKWebView on macOS, WebView2 on Windows, as for
//! printing (printing.rs) — and returns SVG drawn without `<foreignObject>`
//! (`htmlLabels: false`), which `img()` shows through GPUI's resvg. Nothing
//! goes over the network. Results are kept by source and theme, in memory and
//! under the app's local data folder (`quiet.md/mermaid`, never the library).
//! Linux and the web build keep showing the code.

use gpui::*;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;

pub const SUPPORTED: bool = cfg!(any(target_os = "macos", target_os = "windows"));
/// The code block language (`lang::LANGS`) drawn as a diagram.
pub const LANG: &str = "Mermaid";
/// The bundled mermaid.js: part of the cache key, so an upgrade draws anew.
const VERSION: &str = "11.17.2";
/// Cached diagrams in memory before the oldest half is dropped, and files on disk.
const MEMORY: usize = 256;
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
const DISK: usize = 400;

/// A drawn diagram: the SVG at twice its size (so the 1× rasterization GPUI
/// gives an SVG `Image` is sharp on Retina) and its size in points.
#[derive(Clone)]
pub struct Diagram {
    pub image: Arc<Image>,
    pub size: (f32, f32),
}

/// What a block shows: the diagram, or while drawing / after an error, the
/// last diagram that block drew (if any).
#[derive(Clone)]
pub enum Look {
    Ready(Diagram),
    Pending(Option<Diagram>),
    Failed(String, Option<Diagram>),
}

/// The cache key of `source` drawn in the light or dark theme.
pub fn key(source: &str, dark: bool) -> u64 {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(VERSION.as_bytes());
    h.update([0, dark as u8]);
    h.update(source.as_bytes());
    u64::from_le_bytes(h.finalize()[..8].try_into().unwrap())
}

/// The size in points of an SVG made by `DRIVER` (its root's width and
/// height are twice that).
pub fn size_of(svg: &str) -> Option<(f32, f32)> {
    let start = svg.find("<svg")?;
    let root = &svg[start..start + svg[start..].find('>')?];
    let attr = |name: &str| -> Option<f32> {
        let i = root.find(&format!(" {name}=\""))? + name.len() + 3;
        root[i..i + root[i..].find('"')?].trim().trim_end_matches("px").parse().ok()
    };
    let (w, h) = (attr("width")? / 2., attr("height")? / 2.);
    (w.is_finite() && h.is_finite() && w > 0. && h > 0.).then_some((w, h))
}

enum Done {
    Ready(Diagram),
    Failed(String),
}

struct Job {
    block: u64,
    key: u64,
    source: String,
    dark: bool,
}

#[derive(Default)]
struct State {
    done: FxHashMap<u64, Done>,
    /// Insertion order of `done`, to drop the oldest.
    order: VecDeque<u64>,
    /// The last diagram each block drew, kept while a new one draws or fails.
    last: FxHashMap<u64, Diagram>,
    queue: VecDeque<Job>,
    /// The key being drawn now.
    drawing: Option<u64>,
    running: bool,
    /// The web view could not start (no WebView2 runtime): every block says so.
    broken: Option<String>,
    #[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
    pruned: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Changes whenever a drawing finishes: rows with diagrams measure again.
pub fn generation() -> u64 { GENERATION.load(std::sync::atomic::Ordering::Relaxed) }

fn finished(cx: &mut AsyncApp) {
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    cx.update(|cx| cx.refresh_windows());
}

impl State {
    fn insert(&mut self, key: u64, d: Done) {
        if self.done.insert(key, d).is_none() {
            self.order.push_back(key);
        }
        while self.order.len() > MEMORY {
            for k in self.order.drain(..MEMORY / 2) {
                self.done.remove(&k);
            }
        }
    }
}

fn diagram(svg: Vec<u8>) -> Option<Diagram> {
    let size = size_of(std::str::from_utf8(&svg).ok()?)?;
    Some(Diagram { image: Arc::new(Image::from_bytes(ImageFormat::Svg, svg)), size })
}

/// What block `block` (its id) shows for `source` in the current theme;
/// starts drawing it when nothing is known yet. Only when `SUPPORTED`.
pub fn look(block: u64, source: &str, cx: &mut App) -> Look {
    let dark = crate::theme::dark();
    let key = key(source, dark);
    let start = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let last = s.last.get(&block).cloned();
        if let Some(msg) = &s.broken {
            return Err(Look::Failed(msg.clone(), last));
        }
        match s.done.get(&key) {
            Some(Done::Ready(d)) => {
                let d = d.clone();
                s.last.insert(block, d.clone());
                return Err(Look::Ready(d));
            }
            Some(Done::Failed(m)) => return Err(Look::Failed(m.clone(), last)),
            None => {}
        }
        // Already on its way (asked again every frame until it arrives).
        if s.drawing == Some(key) || s.queue.iter().any(|j| j.key == key && j.block == block) {
            return Err(Look::Pending(last));
        }
        if let Some(d) = disk::read(key).and_then(diagram) {
            s.insert(key, Done::Ready(d.clone()));
            s.last.insert(block, d.clone());
            return Err(Look::Ready(d));
        }
        // Typing replaces the block's waiting source rather than queueing each keystroke.
        s.queue.retain(|j| j.block != block);
        s.queue.push_back(Job { block, key, source: source.to_string(), dark });
        Ok((!std::mem::replace(&mut s.running, true), last))
    });
    match start {
        Err(look) => look,
        Ok((spawn, last)) => {
            if spawn {
                cx.spawn(async move |cx| work(cx).await).detach();
            }
            Look::Pending(last)
        }
    }
}

/// Draws the queue in one web view, closed again after a quiet spell.
async fn work(cx: &mut AsyncApp) {
    let mut view: Option<backend::View> = None;
    let mut n = 0;
    loop {
        let Some(job) = STATE.with(|s| s.borrow_mut().queue.pop_front()) else {
            if view.is_none() {
                STATE.with(|s| s.borrow_mut().running = false);
                return;
            }
            // Idle: new work wakes it within 50ms; after 30s the view closes.
            let idle = web_time::Instant::now();
            while STATE.with(|s| s.borrow().queue.is_empty()) && idle.elapsed() < std::time::Duration::from_secs(30) {
                cx.background_executor().timer(std::time::Duration::from_millis(50)).await;
            }
            if STATE.with(|s| s.borrow().queue.is_empty()) {
                view = None;
            }
            continue;
        };
        if view.is_none() {
            match backend::View::open(cx).await {
                Ok(v) => view = Some(v),
                Err(msg) => {
                    log::warn!("mermaid: {msg}");
                    STATE.with(|s| {
                        let mut s = s.borrow_mut();
                        s.broken = Some(msg);
                        s.queue.clear();
                        s.running = false;
                    });
                    finished(cx);
                    return;
                }
            }
        }
        let Some(v) = view.as_ref() else { continue };
        n += 1;
        STATE.with(|s| s.borrow_mut().drawing = Some(job.key));
        let done = match ask(v, n, &job.source, job.dark, cx).await {
            Ok(svg) => match diagram(svg.clone().into_bytes()) {
                Some(d) => {
                    disk::write(job.key, &d);
                    Done::Ready(d)
                }
                None => {
                    log::warn!("mermaid: no size in {}", &svg[..svg.len().min(300)]);
                    Done::Failed(crate::settings::tr("图表没有尺寸", "The diagram has no size").into())
                }
            },
            Err(msg) => Done::Failed(msg),
        };
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.drawing = None;
            if let Done::Ready(d) = &done {
                s.last.insert(job.block, d.clone());
            }
            s.insert(job.key, done);
        });
        finished(cx);
    }
}

/// Inflates mermaid.js into the page the web view loads.
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
fn page() -> Result<String, String> {
    let z = crate::assets::get("mermaid/mermaid.min.js.zlib").ok_or("mermaid.js is not bundled")?;
    let js = miniz_oxide::inflate::decompress_to_vec_zlib(z).map_err(|e| format!("mermaid.js: {e:?}"))?;
    let js = String::from_utf8(js).map_err(|e| e.to_string())?;
    Ok(format!("<!doctype html><html><head><meta charset=\"utf-8\"></head><body><script>{js}</script><script>{DRIVER}</script></body></html>"))
}

/// Runs in the page: `qmdRender(n, source, dark)` posts `ok:n:<SVG>` or
/// `err:n:<message>` to the app (`INBOX`), as does `ready` once loaded.
/// Fonts both WebKit (which measures the labels) and resvg (which draws them)
/// find; the root is sized at 2× its viewBox.
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
const DRIVER: &str = r#"
const FONT = '"Helvetica Neue", Helvetica, Arial, "PingFang SC", "Microsoft YaHei", sans-serif';
function send(s) {
  if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.qmd) window.webkit.messageHandlers.qmd.postMessage(s);
  else if (window.chrome && window.chrome.webview) window.chrome.webview.postMessage(s);
}
async function qmdRender(n, src, dark) {
  let out;
  try {
    mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', theme: dark ? 'dark' : 'default', fontFamily: FONT,
      htmlLabels: false, flowchart: { htmlLabels: false }, suppressErrorRendering: true });
    const { svg } = await mermaid.render('qmd' + n, src);
    const root = new DOMParser().parseFromString(svg, 'image/svg+xml').documentElement;
    if (root.querySelector('foreignObject')) throw new Error('This diagram type needs HTML labels, which cannot be drawn here.');
    const vb = (root.getAttribute('viewBox') || '').trim().split(/[\s,]+/).map(Number);
    const w = vb[2] || parseFloat(root.getAttribute('width')), h = vb[3] || parseFloat(root.getAttribute('height'));
    root.setAttribute('width', 2 * w);
    root.setAttribute('height', 2 * h);
    root.removeAttribute('style');
    out = 'ok:' + n + ':' + new XMLSerializer().serializeToString(root);
  } catch (e) {
    out = 'err:' + n + ':' + String((e && e.message) || e);
  }
  for (const id of ['dqmd' + n, 'qmd' + n]) document.getElementById(id)?.remove();
  send(out);
}
send('ready');
"#;

thread_local! {
    /// What the page posted, in order (delivered on the main thread).
    static INBOX: RefCell<VecDeque<String>> = RefCell::default();
}

#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
fn post(message: String) { INBOX.with(|i| i.borrow_mut().push_back(message)); }

/// Waits up to `limit` for a message `f` accepts; others are stale and dropped.
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
async fn receive<T>(limit: std::time::Duration, cx: &mut AsyncApp, mut f: impl FnMut(&str) -> Option<T>) -> Option<T> {
    let started = web_time::Instant::now();
    while started.elapsed() < limit {
        while let Some(m) = INBOX.with(|i| i.borrow_mut().pop_front()) {
            if let Some(v) = f(&m) { return Some(v); }
        }
        cx.background_executor().timer(std::time::Duration::from_millis(10)).await;
    }
    None
}

/// Reads the page's answer to request `n`.
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
fn answer(message: &str, n: u64) -> Option<Result<String, String>> {
    if let Some(svg) = message.strip_prefix(&format!("ok:{n}:")) {
        return Some(Ok(svg.to_string()));
    }
    message.strip_prefix(&format!("err:{n}:")).map(|m| Err(m.trim().to_string()))
}

/// Sends request `n` and waits for its answer.
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
async fn ask(view: &backend::View, n: u64, source: &str, dark: bool, cx: &mut AsyncApp) -> Result<String, String> {
    view.run(&format!("qmdRender({n}, {}, {dark})", serde_json::to_string(source).unwrap_or_default()));
    receive(std::time::Duration::from_secs(15), cx, |m| answer(m, n)).await
        .unwrap_or_else(|| Err(crate::settings::tr("绘制图表超时", "Drawing the diagram timed out").into()))
}

/// The diagrams on disk, under the app's local data folder.
mod disk {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn dir() -> std::path::PathBuf {
        dirs::data_local_dir().unwrap_or_else(fdl_core::vfs::temp_dir).join("quiet.md").join("mermaid")
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub fn read(key: u64) -> Option<Vec<u8>> { std::fs::read(dir().join(format!("{key:016x}.svg"))).ok() }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub fn read(_: u64) -> Option<Vec<u8>> { None }

    /// Best effort; the first write of a session trims the folder to the newest `DISK`.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub fn write(key: u64, d: &super::Diagram) {
        let dir = dir();
        if std::fs::create_dir_all(&dir).is_err() { return; }
        let tmp = dir.join(format!("{key:016x}.{}.part", fdl_core::vfs::process_id()));
        if std::fs::write(&tmp, &d.image.bytes).is_ok() { let _ = std::fs::rename(&tmp, dir.join(format!("{key:016x}.svg"))); }
        if super::STATE.with(|s| std::mem::replace(&mut s.borrow_mut().pruned, true)) { return; }
        let Ok(entries) = std::fs::read_dir(&dir) else { return };
        let mut files: Vec<_> = entries.flatten().filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).collect();
        if files.len() <= super::DISK { return; }
        files.sort();
        for (_, p) in &files[..files.len() - super::DISK * 3 / 4] { let _ = std::fs::remove_file(p); }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub fn write(_: u64, _: &super::Diagram) {}
}

#[cfg(target_os = "macos")]
mod backend {
    use crate::printing::native::{close, ns_string, offscreen};
    use gpui::AsyncApp;
    use objc::declare::ClassDecl;
    use objc::runtime::{Class, Object, Sel};
    use objc::{class, msg_send, sel, sel_impl};

    /// The page's `webkit.messageHandlers.qmd`.
    extern "C" fn received(_: &Object, _: Sel, _: *mut Object, message: *mut Object) {
        unsafe {
            let body: *mut Object = msg_send![message, body];
            if body.is_null() { return; }
            let is_string: bool = msg_send![body, isKindOfClass: class!(NSString)];
            if !is_string { return; }
            let p: *const std::os::raw::c_char = msg_send![body, UTF8String];
            if !p.is_null() { super::post(std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()); }
        }
    }

    fn handler_class() -> &'static Class {
        if let Some(c) = Class::get("FDLMermaidHandler") { return c; }
        let mut decl = ClassDecl::new("FDLMermaidHandler", class!(NSObject)).expect("FDLMermaidHandler");
        unsafe {
            decl.add_method(sel!(userContentController:didReceiveScriptMessage:), received as extern "C" fn(&Object, Sel, *mut Object, *mut Object));
        }
        decl.register()
    }

    pub struct View {
        holder: *mut Object,
        web: *mut Object,
    }

    impl View {
        pub async fn open(cx: &mut AsyncApp) -> Result<View, String> {
            let page = cx.background_executor().spawn(async { super::page() }).await?;
            super::INBOX.with(|i| i.borrow_mut().clear());
            let (holder, web) = offscreen(1200., &page, |conf| unsafe {
                let ucc: *mut Object = msg_send![conf, userContentController];
                let handler: *mut Object = msg_send![handler_class(), new];
                let _: () = msg_send![ucc, addScriptMessageHandler: handler name: ns_string("qmd")];
                let _: () = msg_send![handler, release];
            });
            let view = View { holder, web };
            match super::receive(std::time::Duration::from_secs(20), cx, |m| (m == "ready").then_some(())).await {
                Some(()) => Ok(view),
                None => Err("mermaid.js did not load".into()),
            }
        }

        pub fn run(&self, js: &str) {
            unsafe {
                let nil: *mut Object = std::ptr::null_mut();
                let _: () = msg_send![self.web, evaluateJavaScript: ns_string(js) completionHandler: nil];
            }
        }
    }

    impl Drop for View {
        fn drop(&mut self) {
            unsafe {
                // The controller holds the handler (and the page's view of it) until removed.
                let conf: *mut Object = msg_send![self.web, configuration];
                let ucc: *mut Object = msg_send![conf, userContentController];
                let _: () = msg_send![ucc, removeScriptMessageHandlerForName: ns_string("qmd")];
            }
            close(self.holder, self.web);
        }
    }
}

#[cfg(target_os = "windows")]
mod backend {
    use crate::printing::webview2::{controller, file_url};
    use gpui::AsyncApp;
    use std::time::Duration;
    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    use webview2_com::{ExecuteScriptCompletedHandler, WebMessageReceivedEventHandler};
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP};
    use windows::core::{HSTRING, PCWSTR, PWSTR, w};

    pub struct View {
        host: HWND,
        controller: Option<ICoreWebView2Controller>,
        web: Option<ICoreWebView2>,
        page: std::path::PathBuf,
    }

    impl View {
        pub async fn open(cx: &mut AsyncApp) -> Result<View, String> {
            let page = cx.background_executor().spawn(async { super::page() }).await?;
            super::INBOX.with(|i| i.borrow_mut().clear());
            // NavigateToString stops at 2 MB; mermaid.js alone is more.
            let path = fdl_core::vfs::temp_dir().join(format!("quiet-md-mermaid-{}.html", fdl_core::vfs::process_id()));
            std::fs::write(&path, page).map_err(|e| e.to_string())?;
            let host = unsafe {
                CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, w!("STATIC"), PCWSTR::null(), WS_POPUP, -20000, -20000, 1200, 1000, None, None, None, None)
            };
            let Ok(host) = host else {
                let _ = std::fs::remove_file(&path);
                return Err("no window for WebView2".into());
            };
            // Dropped on any early return: closes what was made so far.
            let mut view = View { host, controller: None, web: None, page: path };
            let fail = || "WebView2 did not start".to_string();
            let (_env, controller) = controller(host, cx).await.map_err(|e| e.unwrap_or_else(fail))?;
            let web = unsafe { controller.CoreWebView2() }.map_err(|_| fail())?;
            view.controller = Some(controller.clone());
            view.web = Some(web.clone());
            let mut token = 0i64;
            unsafe {
                let _ = controller.SetBounds(RECT { left: 0, top: 0, right: 1200, bottom: 1000 });
                let _ = controller.SetIsVisible(true);
                web.add_WebMessageReceived(
                    &WebMessageReceivedEventHandler::create(Box::new(|_, args| {
                        if let Some(a) = args {
                            let mut m = PWSTR::null();
                            if a.TryGetWebMessageAsString(&mut m).is_ok() { super::post(webview2_com::take_pwstr(m)); }
                        }
                        Ok(())
                    })),
                    &mut token,
                )
                .map_err(|_| fail())?;
                web.Navigate(&HSTRING::from(file_url(&view.page))).map_err(|_| fail())?;
            }
            match super::receive(Duration::from_secs(20), cx, |m| (m == "ready").then_some(())).await {
                Some(()) => Ok(view),
                None => Err("mermaid.js did not load".into()),
            }
        }

        pub fn run(&self, js: &str) {
            if let Some(web) = &self.web {
                unsafe {
                    let _ = web.ExecuteScript(&HSTRING::from(js), &ExecuteScriptCompletedHandler::create(Box::new(|_, _| Ok(()))));
                }
            }
        }
    }

    impl Drop for View {
        fn drop(&mut self) {
            unsafe {
                if let Some(c) = &self.controller { let _ = c.Close(); }
                let _ = DestroyWindow(self.host);
            }
            let _ = std::fs::remove_file(&self.page);
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod backend {
    use gpui::AsyncApp;

    pub struct View;

    impl View {
        pub async fn open(_: &mut AsyncApp) -> Result<View, String> { Err("no web view on this system".into()) }
        pub fn run(&self, _: &str) {}
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: that brings in gpui's own `test` attribute.
    use super::{answer, key, size_of};

    #[test]
    fn keys_follow_source_and_theme() {
        let src = "flowchart LR\n  A --> B";
        assert_eq!(key(src, false), key(src, false));
        assert_ne!(key(src, false), key(src, true), "light and dark draw differently");
        assert_ne!(key(src, false), key("flowchart LR\n  A --> C", false));
        assert_ne!(key("ab", false), key("a b", false));
        // On disk across runs and builds: pinned, not a std hash.
        assert_eq!(format!("{:016x}", key("", false)), format!("{:016x}", key("", false)));
        assert_eq!(key("graph TD", true), {
            use sha2::{Digest, Sha256};
            let d = Sha256::new().chain_update(b"11.17.2").chain_update([0u8, 1]).chain_update(b"graph TD").finalize();
            u64::from_le_bytes(d[..8].try_into().unwrap())
        });
    }

    #[test]
    fn size_is_half_the_root() {
        let svg = r#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" aria-roledescription="flowchart-v2" width="361.5" height="174" viewBox="0 0 180.75 87"><rect width="5" height="5"/></svg>"#;
        assert_eq!(size_of(svg), Some((180.75, 87.)));
        assert_eq!(size_of(r#"<svg width="100%" viewBox="0 0 1 1"/>"#), None);
        assert_eq!(size_of("<svg/>"), None);
        assert_eq!(size_of("not svg"), None);
    }

    #[test]
    fn answers_match_their_request() {
        assert_eq!(answer("ok:3:<svg/>", 3), Some(Ok("<svg/>".into())));
        assert_eq!(answer("ok:3:<svg/>", 4), None, "a stale answer is not this one");
        assert_eq!(answer("ok:13:<svg/>", 1), None);
        assert_eq!(answer("err:4:Parse error 语法\n", 4), Some(Err("Parse error 语法".into())));
        assert_eq!(answer("ready", 1), None);
    }

    /// The bundled mermaid.js inflates, and the page loads it before the driver.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn page_bundles_mermaid() {
        let page = super::page().unwrap();
        assert!(page.len() > 3_000_000);
        assert!(page.find(r#"globalThis["mermaid"]"#).unwrap() < page.find("qmdRender").unwrap());
        assert_eq!(page.matches("</script>").count(), 2, "mermaid.js never closes its own script element");
    }
}
