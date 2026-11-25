//! Browser file drops use File objects, never filesystem paths from the host.

use std::{cell::Cell, rc::Rc};

use crate::app::{DropImport, View, Workspace};
use gpui::{App, Context, Window, point, px};
use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen(module = "/src/web/drop_entries.js")]
extern "C" {
    #[wasm_bindgen(js_name = listenQuietFileDrop)]
    fn listen(destination: &js_sys::Function, max_entries: usize);
}

#[derive(Clone, Debug)]
pub struct BrowserDrop {
    pub(crate) count: usize,
    read: js_sys::Function,
    pending: Rc<Cell<bool>>,
}

impl BrowserDrop {
    pub(crate) fn import(self, workspace: &mut Workspace, parent: String, window: &mut Window, cx: &mut Context<Workspace>) {
        if self.pending.get() || workspace.store.top_of(&parent).is_none() { return; }
        let Ok(value) = self.read.call0(&JsValue::UNDEFINED) else { return };
        let Ok(files) = value.dyn_into::<js_sys::Promise>() else { return };
        self.pending.set(true);
        workspace.import_web_drop(JsFuture::from(files), parent, self.pending, window, cx);
    }
}

pub(super) fn install(cx: &App) {
    let cx = cx.to_async();
    let pending = Rc::new(Cell::new(false));
    let destination = Closure::<dyn FnMut(f32, f32, JsValue, usize) -> bool>::new(move |x, y, read: JsValue, count| {
        if pending.get() { return false; }
        cx.update(|cx| {
            let Some(handle) = cx.windows().into_iter().find_map(|handle| handle.downcast::<Workspace>()) else { return false };
            handle.update(cx, |w, window, cx| {
                if w.confirm.is_some() || w.store.loading() || w.store.file_task_busy()
                    || w.file_history.busy || matches!(w.view, View::Settings | View::Trash) { return false; }
                let at = point(px(x), px(y));
                if w.sidebar_open && at.x < px(w.sidebar_w(window)) {
                    let Some(target) = w.sidebar_target(at, false).filter(|hit| hit.slot.is_none() && hit.parent.is_some()) else { return false };
                    if let Ok(read) = read.dyn_into::<js_sys::Function>() {
                        w.ask_import(DropImport::Browser(BrowserDrop { count, read, pending: pending.clone() }), target, cx);
                    }
                } else {
                    let Some(parent) = w.browser_drop_parent(at) else { return false };
                    if let Ok(read) = read.dyn_into::<js_sys::Function>() {
                        BrowserDrop { count, read, pending: pending.clone() }.import(w, parent, window, cx);
                    }
                }
                true
            }).unwrap_or(false)
        })
    });
    // The single GPUI canvas and its listeners live for this page's lifetime.
    listen(destination.into_js_value().unchecked_ref(), 4 * crate::transfer::IMPORT_FOLDER_LIMIT);
}
