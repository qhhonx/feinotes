//! Deliver the shared workspace reminder list through User Notifications.
use crate::{app::Workspace,settings::display_title,store::Store};
use gpui::*;
use base64::Engine;
use std::{cell::RefCell,ffi::{CString,c_char}};
thread_local! { static PUBLISHED: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) }; }

pub(crate) fn namespace() -> String {
    let root = Store::root_dir();
    let root = root.canonicalize().unwrap_or(root);
    #[cfg(feature = "cloudkit-sync")]
    let base = crate::ipad_session::base_path().ok().map(|base|base.canonicalize().unwrap_or(base));
    #[cfg(not(feature = "cloudkit-sync"))]
    let base: Option<std::path::PathBuf> = None;
    let logical = base.as_ref().and_then(|base|root.strip_prefix(base).ok()).unwrap_or(&root);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(logical.to_string_lossy().as_bytes())
}

pub(crate) fn publish(workspace: &Workspace, cx: &App) {
    if workspace.library_switching { return; }
    let namespace = namespace();
    let now = crate::time::now_ms() as i64;
    let mut requests = Vec::new();
    workspace.visit_reminders(cx,|entry,index,reminder| {
        if reminder.should_notify && reminder.notify > now {
            requests.push(serde_json::json!({"identifier":format!("fei.reminder.{namespace}.{}.{index}",entry.id),"document":entry.id,
                "title":display_title(&entry.title),"body":crate::editor::reminder::notification(reminder),"at":reminder.notify}));
        }
    });
    requests.sort_by(|a,b|a["at"].as_i64().cmp(&b["at"].as_i64()).then_with(||a["identifier"].as_str().cmp(&b["identifier"].as_str())));
    let total = requests.len();
    requests.truncate(60);
    let payload = serde_json::json!({"namespace":namespace,"english":crate::settings::english(),"total":total,"requests":requests});
    let bytes = payload.to_string().into_bytes();
    PUBLISHED.with(|previous| {
        if *previous.borrow() == bytes { return; }
        if let Ok(json) = CString::new(bytes.clone()) { unsafe { fei_ios_reminders_changed(json.as_ptr()); } *previous.borrow_mut() = bytes; }
    });
}

pub(crate) fn present_settings() { unsafe { fei_ios_present_reminder_settings(crate::settings::english()); } }
unsafe extern "C" {
    fn fei_ios_reminders_changed(json: *const c_char);
    fn fei_ios_present_reminder_settings(english: bool);
}
