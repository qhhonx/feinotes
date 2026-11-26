//! Native history presentation around the shared snapshot commands.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static TOKEN: AtomicU64 = AtomicU64::new(1);
pub(super) fn next_token() -> u64 { TOKEN.fetch_add(1,Ordering::Relaxed) }

#[derive(serde::Serialize)]
pub(crate) struct NativeVersion { pub name:String, pub title:String, pub detail:String, pub named:bool }
#[derive(serde::Serialize)]
pub(crate) struct NativeHistory {
    pub token:u64, pub scope:String, pub note:String, pub selected:Option<String>, pub previewing:bool,
    pub named_only:bool, pub marks:bool, pub can_restore:bool, pub can_copy:bool,
    pub versions:Vec<NativeVersion>, pub notice:Option<String>, pub empty:String, pub subtitle:String,
}
#[derive(serde::Deserialize)]
struct Command { op:String, name:Option<String>, text:Option<String> }

impl Workspace {
    pub(crate) fn native_history(&self, cx:&App) -> Option<NativeHistory> {
        if self.view != crate::app::View::Doc { return None; }
        let h = self.history.as_ref()?;
        let can_restore = !self.reading() && !crate::cloud_sync::shared::editing_paused(&self.store,cx) && crate::cloud_sync::shared::access(&self.store,cx).is_none_or(|a|a.can_write());
        Some(NativeHistory { token:h.token,scope:crate::ipad::editor_scope(self),note:h.id.clone(),selected:h.selected.clone(),previewing:self.preview.is_some(),named_only:h.named_only,marks:h.marks,can_restore,can_copy:!crate::in_shared_note(),
            versions:h.versions.iter().map(|v|NativeVersion { name:v.name.clone(),title:v.label.clone().unwrap_or_else(||format_time(v.ms/1000)),detail:format!("{} · {}",day_label(v.ms/1000),delta_text(&v.name,&h.delta)),named:v.label.is_some() }).collect(),
            notice:h.error.clone().or_else(||h.feedback.clone()),
            empty:crate::settings::tr("暂无历史版本","No versions yet").into(),subtitle:crate::settings::tr("本机保存的历史版本","Versions saved on this device").into() })
    }

    pub(crate) fn mobile_history_command(&mut self, token:Option<u64>, scope:Option<&str>, value:Option<&str>, window:&mut Window, cx:&mut Context<Self>) {
        if self.view != crate::app::View::Doc || self.library_switching { return; }
        let Some(h) = &self.history else { return };
        if token != Some(h.token) || self.current.as_deref() != Some(h.id.as_str()) || scope != Some(crate::ipad::editor_scope(self).as_str()) { return; }
        let Some(command) = value.and_then(|v|serde_json::from_str::<Command>(v).ok()) else { return };
        if command.name.as_ref().is_some_and(|name|!h.versions.iter().any(|v|&v.name == name)) { return; }
        let selected = h.selected.clone();
        if let Some(h) = &mut self.history { h.feedback = None; }
        match command.op.as_str() {
            "preview"=>if let Some(name) = command.name { self.preview_version(&name,window,cx); },
            "close"=>self.close_history(cx),
            "back-to-list" if self.preview.is_some()=>self.exit_preview(cx),
            "marks"=>self.toggle_history_marks(cx),
            "named"=>self.toggle_history_named(cx),
            "save" if self.preview.is_none()=>self.save_version_now(window,cx),
            "rename"=>if let (Some(name),Some(text)) = (command.name,command.text) { self.set_history_label(&name,&text,cx); },
            "delete" | "copy"=>if let Some(name) = command.name {
                self.history.as_mut().unwrap().selected = Some(name);
                if command.op == "delete" { self.delete_selected_version(window,cx); } else { self.version_as_copy(window,cx); }
            },
            "restore" if self.preview.is_some() && command.name == selected=>self.restore_selected(window,cx),
            _=>{},
        }
    }
}
