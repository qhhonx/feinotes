//! Native selection uses the existing workspace batch commands.
use crate::{app::{View,Workspace}, home::HomeTab, store::Store};
use gpui::*;
use serde_json::{Value,json};
use std::{path::PathBuf,sync::atomic::{AtomicU64,Ordering}};

pub(crate) struct BatchSession {
    serial: u64, root: PathBuf, view: View, folder: Option<String>, tab: HomeTab, query: String,
    candidates: Vec<String>, moving: Option<Vec<String>>,
}
static SERIAL: AtomicU64 = AtomicU64::new(1);
unsafe extern "C" {
    fn fei_ios_present_batch_move(json: *const std::ffi::c_char);
}

impl Workspace {
    pub(crate) fn open_mobile_batch(&mut self, cx: &mut Context<Self>) {
        if !(matches!(self.view,View::Home | View::Folder) || self.view==View::Doc && !unsafe { crate::ipad::fei_ios_is_phone() }) || self.is_shared() { return; }
        #[cfg(feature = "cloudkit-sync")]
        if self.library_switching { return; }
        let candidates = self.mobile_visible_entries(cx).into_iter().map(|entry|entry.id).collect();
        self.mobile_batch = Some(BatchSession { serial: SERIAL.fetch_add(1,Ordering::Relaxed),root: Store::root_dir(),view: self.view,folder: self.folder_current.clone(),tab: self.home.tab,query: self.query(cx),candidates,moving: None });
        self.begin_personal_batch();
        self.sharing.batch = self.home.tab == HomeTab::Shared;
        cx.notify();
    }

    fn mobile_batch_matches(&self, serial: u64, cx: &App) -> bool {
        #[cfg(feature = "cloudkit-sync")]
        if self.library_switching { return false; }
        !self.is_shared() && self.mobile_batch.as_ref().is_some_and(|session|session.serial == serial && session.root == Store::root_dir()
            && session.view == self.view && session.folder == self.folder_current && session.tab == self.home.tab && session.query == self.query(cx))
    }

    pub(crate) fn mobile_batch_state(&self,cx:&App) -> Option<Value> {
        let session=self.mobile_batch.as_ref()?;
        if !self.mobile_batch_matches(session.serial,cx) { return None; }
        let visible=self.mobile_visible_entries(cx);
        let ids=session.candidates.iter().filter(|id|visible.iter().any(|entry|&entry.id==*id)).cloned().collect::<Vec<_>>();
        let selected=self.batch.as_deref().unwrap_or_default().iter().filter(|id|ids.contains(id)).cloned().collect::<Vec<_>>();
        Some(json!({"serial":session.serial,"ids":ids,"selected":selected,"limit":crate::sidebar::BATCH_LIMIT,"can_move":!self.batch_contains_shared(cx),"moving":session.moving.is_some()}))
    }

    pub(crate) fn mobile_batch_command(&mut self, serial: u64, command: &str, value: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.mobile_batch_matches(serial,cx) { return; }
        let moving = self.mobile_batch.as_ref().is_some_and(|session|session.moving.is_some());
        if command == "batch-move-cancel" {
            if let Some(ids)=self.mobile_batch.as_mut().and_then(|s|s.moving.take()) { self.batch=Some(ids); cx.notify(); }
            return;
        }
        if command == "batch-close" {
            if !moving { self.mobile_batch = None; self.batch = None; cx.notify(); }
            return;
        }
        if command == "batch-move" {
            let Some(ids) = self.mobile_batch.as_ref().and_then(|session|session.moving.clone()) else { return };
            let target = value.filter(|id|!id.is_empty());
            if target.is_some_and(|id|!self.entries.iter().any(|entry|entry.id == id && entry.meta.folder.is_some())) { return; }
            let ids: Vec<_> = ids.into_iter().filter(|id|self.entries.iter().any(|entry|&entry.id == id)).collect();
            self.mobile_batch = None;
            self.move_docs(&ids,target.map(str::to_owned),cx);
            return;
        }
        if command != "batch-action" || moving { return; }
        let visible = self.mobile_visible_entries(cx);
        let valid: Vec<_> = self.mobile_batch.as_ref().unwrap().candidates.iter().filter(|id|visible.iter().any(|entry|&entry.id == *id)).cloned().collect();
        if let Some(selected) = &mut self.batch { selected.retain(|id|valid.contains(id)); }
        match value {
            Some("all")=>{
                let all = self.batch.as_ref().is_some_and(|selected|!valid.is_empty() && valid.iter().take(crate::sidebar::BATCH_LIMIT).all(|id|selected.contains(id)));
                self.batch = Some(if all { vec![] } else { valid.into_iter().take(crate::sidebar::BATCH_LIMIT).collect() });
            }
            Some(key) if key.starts_with("select:")=>{
                let id = &key[7..];
                if valid.iter().any(|candidate|candidate == id) { self.toggle_check(id); }
            }
            Some("copy") if self.batch.as_ref().is_some_and(|v|!v.is_empty())=>{ self.batch_copy(cx); self.mobile_batch = None; return; }
            Some("delete") if self.batch.as_ref().is_some_and(|v|!v.is_empty())=>{ self.ask_batch_delete(window,cx); self.batch = None; self.mobile_batch = None; return; }
            Some("move")=>{
                if self.batch_contains_shared(cx) { return; }
                let ids = self.batch.take().unwrap_or_default();
                if ids.is_empty() { self.mobile_batch = None; return; }
                self.mobile_batch.as_mut().unwrap().moving = Some(ids.clone());
                let request = json!({"serial":serial,"ids":ids});
                if let Ok(json) = std::ffi::CString::new(request.to_string()) { unsafe { fei_ios_present_batch_move(json.as_ptr()); } }
                cx.notify(); return;
            }
            _=>{}
        }
        cx.notify();
    }
}
