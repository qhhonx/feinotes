//! Native row reordering delegates to the shared tree writer and history.
use crate::{app::{View,Workspace},home::HomeTab,settings::{display_title,tr},store::Store};
use gpui::*;
use std::{path::PathBuf,sync::atomic::{AtomicU64,Ordering}};

pub(crate) struct ReorderSession { serial:u64, response:u64, root:PathBuf, view:View, parent:Option<String>, pub error:Option<String> }
#[derive(serde::Serialize)]
pub(crate) struct ReorderRequest { serial:u64, response:u64, title:&'static str, prompt:String, done:&'static str, saving:bool, items:Vec<Item> }
#[derive(serde::Serialize)]
struct Item { id:String, title:String, folder:bool, pinned:bool }
static SERIAL: AtomicU64 = AtomicU64::new(1);

impl Workspace {
    fn reorder_scope(&self, cx: &App) -> bool {
        #[cfg(feature = "cloudkit-sync")]
        if self.library_switching { return false; }
        !crate::in_shared_note() && self.query(cx).is_empty() && self.confirm.is_none()
            && (self.view == View::Folder || (self.view == View::Home || self.view == View::Doc && !unsafe { crate::ipad::fei_ios_is_phone() }) && self.home.tab == HomeTab::Recent)
    }
    pub(crate) fn mobile_can_reorder(&self, cx: &App) -> bool { self.reorder_scope(cx) && !self.tree_writes_pending() }
    fn reorder_matches(&self, serial:u64, cx: &App) -> bool {
        self.reorder_scope(cx) && self.mobile_manual_sort() && self.mobile_reorder.as_ref().is_some_and(|s|s.serial == serial && s.root == Store::root_dir() && s.view == self.view && s.parent == self.creation_parent())
    }
    pub(crate) fn mobile_move_row(&mut self, id:&str, before:Option<&str>, cx:&mut Context<Self>) {
        if !self.mobile_can_reorder(cx) { return; }
        let before=before.filter(|v|!v.is_empty());
        let parent=self.folder_current.clone();
        let Some(entry)=self.entries.iter().find(|e|e.id==id && e.meta.parent==parent && !e.meta.local()) else { return; };
        let pinned=entry.meta.pinned;
        if before.is_some_and(|next|next==id || !self.entries.iter().any(|e|e.id==next && e.meta.parent==parent && e.meta.pinned==pinned && !e.meta.local())) { return; }
        if !self.mobile_sort("manual",cx) { return; }
        self.queue_tree_move(id,parent,before.map(str::to_owned),cx);
    }
    pub(crate) fn open_mobile_reorder(&mut self, cx: &mut Context<Self>) {
        if !self.mobile_can_reorder(cx) || !self.mobile_sort("manual",cx) { return; }
        self.mobile_batch = None; self.batch = None;
        self.mobile_reorder = Some(ReorderSession { serial:SERIAL.fetch_add(1,Ordering::Relaxed),response:0,root:Store::root_dir(),view:self.view,parent:self.creation_parent(),error:None });
        cx.notify();
    }
    pub(crate) fn mobile_reorder_request(&self, cx: &App) -> Option<ReorderRequest> {
        let session = self.mobile_reorder.as_ref()?;
        if !self.reorder_matches(session.serial,cx) { return None; }
        let saving = self.tree_writes_pending();
        let prompt = if saving { tr("正在保存…","Saving…").into() } else { session.error.clone().unwrap_or_else(||tr("拖动右侧手柄调整顺序。","Drag the handles to change the order.").into()) };
        let items = self.mobile_visible_entries(cx).into_iter().map(|entry|Item { id:entry.id,title:display_title(&entry.title),folder:entry.meta.folder.is_some(),pinned:entry.meta.pinned }).collect();
        Some(ReorderRequest { serial:session.serial,response:session.response,title:tr("调整顺序","Reorder"),prompt,done:tr("完成","Done"),saving,items })
    }
    pub(crate) fn mobile_reorder_command(&mut self, serial:u64, action:&str, id:Option<&str>, before:Option<&str>, cx:&mut Context<Self>) {
        if action == "reorder-close" {
            if self.mobile_reorder.as_ref().is_some_and(|session|session.serial == serial) { self.mobile_reorder = None; cx.notify(); }
            return;
        }
        if action != "reorder-move" || !self.reorder_matches(serial,cx) { return; }
        let session = self.mobile_reorder.as_mut().unwrap();
        session.response += 1;
        session.error = None;
        let parent = session.parent.clone();
        let before = before.filter(|id|!id.is_empty());
        let valid = id.is_some_and(|id|self.entries.iter().any(|entry|entry.id == id && entry.meta.parent == parent && !entry.meta.local()))
            && before.is_none_or(|before|Some(before)!=id && self.entries.iter().any(|entry|entry.id == before && entry.meta.parent == parent));
        if self.tree_writes_pending() || !valid {
            self.mobile_reorder.as_mut().unwrap().error = Some(tr("列表已更新，请重试。","The list changed. Try again.").into()); cx.notify(); return;
        }
        self.queue_tree_move(id.unwrap(),parent,before.map(str::to_owned),cx);
    }
}
