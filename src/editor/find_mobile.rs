//! UIKit owns the find navigator; the shared editor owns matches and replacement.
use super::view::Editor;
use gpui::*;
use serde_json::{Value,json};
use std::sync::atomic::{AtomicU64,Ordering};

static SERIAL: AtomicU64 = AtomicU64::new(1);
pub(super) fn next_serial() -> u64 { SERIAL.fetch_add(1,Ordering::Relaxed) }
pub(super) fn dismiss(serial: u64) { unsafe { fei_ios_dismiss_find(serial); } }

impl Editor {
    fn native_find_state(&self, cx: &App) -> Option<Value> {
        let find = self.find.as_ref()?;
        Some(json!({"serial":find.native_serial,"generation":self.core.generation,"query":self.find_field.read(cx).text,
            "replacement":self.replace_field.read(cx).text,"replace":find.replace,"count":find.matches.len(),"current":find.current,
            "can_replace":!self.read_only && crate::cloud_sync::shared::can_edit_in(&self.library_root,cx)}))
    }

    pub(super) fn present_native_find(&mut self, cx: &App) {
        let Some(state) = self.native_find_state(cx) else { return };
        self.dismiss_touch_menu();
        self.invalidate_table_controls();
        if let Ok(json) = std::ffi::CString::new(state.to_string()) { unsafe { fei_ios_present_find(json.as_ptr()); } }
    }

    pub(super) fn publish_native_find(&mut self, cx: &App) {
        let Some(state) = self.native_find_state(cx) else { return };
        let Ok(bytes) = serde_json::to_vec(&state) else { return };
        let Some(find) = &mut self.find else { return };
        if find.native_published == bytes { return; }
        find.native_published = bytes.clone();
        if let Ok(json) = std::ffi::CString::new(bytes) { unsafe { fei_ios_update_find(json.as_ptr()); } }
    }

    pub(crate) fn mobile_find_command(&mut self, serial: u64, action: &str, value: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.find.as_ref().is_some_and(|find|find.native_serial == serial) { return; }
        if action == "find-close" { self.close_find(window,cx); return; }
        if matches!(action,"find-mode-search" | "find-mode-replace") {
            let replace = action == "find-mode-replace" && !self.read_only && crate::cloud_sync::shared::can_edit_in(&self.library_root,cx);
            if let Some(find) = &mut self.find { find.replace = replace; }
            self.present_native_find(cx);
            self.scroll_to_current(window);
            cx.notify();
            return;
        }
        #[derive(serde::Deserialize)] struct Fields { query: String, replacement: String, generation: u64 }
        let Some(fields) = value.and_then(|value|serde_json::from_str::<Fields>(value).ok()) else { return };
        if action == "find-query" {
            if fields.query != self.find_field.read(cx).text {
                self.find_field.update(cx,|field,cx|field.set_text(&fields.query,cx));
                self.refresh_find(true,cx);
            }
            self.scroll_to_current(window);
            cx.notify();
        } else if fields.query == self.find_field.read(cx).text {
            match action {
                "find-next"=>self.step_find(true,window,cx),
                "find-previous"=>self.step_find(false,window,cx),
                "find-replace" | "find-replace-all" if fields.generation == self.core.generation && !self.read_only && crate::cloud_sync::shared::can_edit_in(&self.library_root,cx)=>{
                    self.replace_field.update(cx,|field,cx|field.set_text(&fields.replacement,cx));
                    if action == "find-replace" { self.replace_one(window,cx); } else { self.replace_all(window,cx); }
                }
                _=>{}
            }
        }
        self.publish_native_find(cx);
    }
}

unsafe extern "C" {
    fn fei_ios_present_find(json: *const std::ffi::c_char);
    fn fei_ios_update_find(json: *const std::ffi::c_char);
    fn fei_ios_dismiss_find(serial: u64);
}
