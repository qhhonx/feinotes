//! Browser file drops use File objects, never filesystem paths from the host.

use std::{cell::Cell, rc::Rc};

use gpui::{App, point, px};
use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen(module = "/src/web/drop_entries.js")]
extern "C" {
    #[wasm_bindgen(js_name = listenQuietFileDrop)]
    fn listen(destination: &js_sys::Function, max_entries: usize);
}

pub(super) fn install(cx: &App) {
    use crate::app::Workspace;
    let cx = cx.to_async();
    let pending = Rc::new(Cell::new(false));
    let destination = Closure::<dyn FnMut(f32, f32, JsValue) -> bool>::new(move |x, y, files: JsValue| {
        if pending.get() { return false; }
        cx.update(|cx| {
            let Some(handle) = cx.windows().into_iter().find_map(|handle| handle.downcast::<Workspace>()) else { return false };
            handle.update(cx, |w, window, cx| {
                let Some(parent) = w.browser_drop_parent(point(px(x), px(y)), window) else { return false };
                if let Ok(files) = files.dyn_into::<js_sys::Promise>() {
                    pending.set(true);
                    w.import_web_drop(JsFuture::from(files), parent, pending.clone(), window, cx);
                }
                true
            }).unwrap_or(false)
        })
    });
    // The single GPUI canvas and its listeners live for this page's lifetime.
    listen(destination.into_js_value().unchecked_ref(), 4 * crate::transfer::IMPORT_FOLDER_LIMIT);
}
