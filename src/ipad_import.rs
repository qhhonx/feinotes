//! Scoped native folder access around the existing legacy-library importer.
use crate::{app::Workspace, settings::tr, store::Store, ui};
use gpui::*;
use std::{ffi::{CStr, CString, c_char, c_void}, path::PathBuf, sync::{Arc, Mutex}, time::Duration};

struct StagedLibrary(PathBuf);
impl Drop for StagedLibrary {
    fn drop(&mut self) { if let Some(root) = self.0.parent() { let _ = std::fs::remove_dir_all(root); } }
}
impl StagedLibrary {
    fn from_native(path: PathBuf) -> Result<Self, String> {
        let path = path.canonicalize().map_err(|e|e.to_string())?;
        let base = std::env::temp_dir().join("FeiNotesImports").canonicalize().map_err(|e|e.to_string())?;
        let parent = path.parent().ok_or("Invalid import staging directory")?;
        if path.file_name().and_then(|n|n.to_str()) != Some("library") || parent.parent() != Some(base.as_path())
            || parent.file_name().and_then(|n|n.to_str()).and_then(|n|uuid::Uuid::parse_str(n).ok()).is_none() {
            return Err("Invalid import staging directory".into());
        }
        Ok(Self(path))
    }
}
type Pending = Mutex<Option<Result<Option<StagedLibrary>, String>>>;
unsafe extern "C" {
    fn fei_ios_pick_library(title: *const c_char, completion: extern "C" fn(i32, *const c_char, *const c_void), context: *const c_void);
}
extern "C" fn completed(status: i32, message: *const c_char, context: *const c_void) {
    let pending = unsafe { Arc::<Pending>::from_raw(context.cast()) };
    let text = if message.is_null() { String::new() } else { unsafe { CStr::from_ptr(message) }.to_string_lossy().into_owned() };
    let result = match status { 1 => StagedLibrary::from_native(PathBuf::from(text)).map(Some), 0 => Ok(None), _ => Err(text) };
    *pending.lock().expect("library picker completion lock") = Some(result);
}
async fn pick(cx: &mut AsyncApp) -> Result<Option<StagedLibrary>, String> {
    let title = CString::new(tr("选择 Quiet.md 文档库", "Choose a Quiet.md library")).map_err(|e|e.to_string())?;
    let pending = Arc::new(Pending::new(None));
    unsafe { fei_ios_pick_library(title.as_ptr(),completed,Arc::into_raw(pending.clone()).cast()); }
    loop {
        if let Some(result) = pending.lock().expect("library picker completion lock").take() { return result; }
        cx.background_executor().timer(Duration::from_millis(50)).await;
    }
}
impl Workspace {
    pub(crate) fn import_legacy_library(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.library_switching || self.store.file_task_busy() || crate::in_shared_note() || !self.flush(cx) { return; }
        let root = Store::root_dir();
        let lease = Store::session_lease(&root);
        self.library_switching = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let current = root.clone();
            let result = match pick(cx).await {
                Ok(Some(source)) => cx.background_executor().spawn(async move {
                    let _lease = lease;
                    Store::import_legacy_copy(&root, &source.0).map(Some).map_err(|e|e.to_string())
                }).await,
                Ok(None) => Ok(None), Err(error) => Err(error),
            };
            let _ = this.update_in(cx, |w, _, cx| {
                w.library_switching = false;
                if Store::root_dir() == current { match result {
                    Ok(Some(count)) => { w.entries = w.store.list(); w.sort(); w.sync_links(cx); crate::cloud_sync::retry(w.store.root(),cx); w.show_toast(&crate::localized_format!("已导入 {} 项", "Imported {} items", count), cx); }
                    Ok(None) => {},
                    Err(error) => w.show_toast_kind(ui::ToastKind::Error, &crate::localized_format!("导入失败 · {}", "Import failed · {}", error), cx),
                } }
                cx.notify();
            });
        }).detach();
    }
}
