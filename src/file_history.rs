use crate::{app::{Workspace, View}, store::{Entry, FileState, FileDelta}};
use gpui::*;
use std::collections::{HashMap, VecDeque};

#[derive(Clone)]
struct Record { token: u64, label: &'static str, changes: Vec<FileDelta>, entries: Vec<(Option<Entry>, Option<Entry>)> }
#[derive(Default)]
pub(crate) struct FileHistory {
    undo: Vec<Record>, redo: Vec<Record>, next: u64,
    pub prefer_files: bool, pub busy: bool,
    requests: VecDeque<bool>,
}
impl Workspace {
    pub(crate) fn remember_files(&mut self, journal: crate::store::FileChanges, label: &'static str) -> u64 {
        self.remember_file_changes(journal.take(), label)
    }

    /// `remember_files` for changes written by a background store (导入文件夹).
    pub(crate) fn remember_file_changes(&mut self, changes: Vec<FileDelta>, label: &'static str) -> u64 {
        let entries = self.file_record_entries(&changes);
        self.push_file_record(changes, entries, label)
    }

    fn file_record_entries(&self, changes: &[FileDelta]) -> Vec<(Option<Entry>, Option<Entry>)> {
        changes.iter().map(|c| {
            let entry = self.entries.iter().find(|e| e.id == c.id).cloned();
            let make = |state: &FileState| {
                let mut e = entry.clone().unwrap_or_else(|| Entry { id: c.id.clone(), meta: Default::default(), title: String::new(), icon: None, body: Default::default(), reminders: vec![], links: vec![], note: None, top: String::new() });
                e.title = state.title.clone(); e.meta.folder = state.folder.then(|| state.title.clone());
                e.meta.parent = state.parent.clone(); e.meta.order = state.order; e.meta.top_order = state.top_order;
                e
            };
            (c.before.as_ref().map(make), c.after.as_ref().map(make))
        }).collect()
    }

    pub(crate) fn confirm_tree_history(&mut self, token: u64, changes: Vec<FileDelta>) {
        if changes.is_empty() { self.forget_file_change(token); return; }
        let entries = self.file_record_entries(&changes);
        if let Some(record) = self.file_history.undo.iter_mut().chain(self.file_history.redo.iter_mut()).find(|r| r.token == token) {
            record.changes = changes; record.entries = entries;
        }
    }

    /// `confirm_tree_history` for one of several moves under one record (移动到): what
    /// this one wrote replaces what the rows recorded for the same pages; the rest stands.
    pub(crate) fn join_tree_history(&mut self, token: u64, changes: Vec<FileDelta>) {
        let Some(mut merged) = self.file_history.undo.iter().chain(self.file_history.redo.iter()).find(|r| r.token == token).map(|r| r.changes.clone()) else { return };
        for delta in changes {
            match merged.iter_mut().find(|c| c.id == delta.id) { Some(c) => c.after = delta.after, None => merged.push(delta) }
        }
        let entries = self.file_record_entries(&merged);
        if let Some(record) = self.file_history.undo.iter_mut().chain(self.file_history.redo.iter_mut()).find(|r| r.token == token) { record.changes = merged; record.entries = entries; }
    }

    pub(crate) fn remember_tree_move(&mut self, before: Vec<Entry>, label: &'static str) -> u64 {
        let old: HashMap<_, _> = before.into_iter().map(|e| (e.id.clone(), e)).collect();
        let new: HashMap<_, _> = self.entries.iter().map(|e| (e.id.clone(), e.clone())).collect();
        let mut ids: Vec<_> = old.keys().chain(new.keys()).cloned().collect(); ids.sort(); ids.dedup();
        let mut changes = vec![]; let mut entries = vec![];
        for id in ids {
            let a = old.get(&id).map(FileState::of); let b = new.get(&id).map(FileState::of);
            if a == b { continue; }
            entries.push((old.get(&id).cloned(), new.get(&id).cloned()));
            changes.push(FileDelta { id, before: a, after: b });
        }
        self.push_file_record(changes, entries, label)
    }

    /// 重命名 shown before it is written (`rename_ahead`): the record as the rows have it,
    /// replaced by what was written once it lands (`confirm_tree_history`).
    pub(crate) fn remember_title(&mut self, old: Entry, new: Option<Entry>, label: &'static str) -> u64 {
        let change = FileDelta { id: old.id.clone(), before: Some(FileState::of(&old)), after: new.as_ref().map(FileState::of) };
        self.push_file_record(vec![change], vec![(Some(old), new)], label)
    }

    fn push_file_record(&mut self, changes: Vec<FileDelta>, entries: Vec<(Option<Entry>, Option<Entry>)>, label: &'static str) -> u64 {
        if changes.is_empty() { return 0; }
        self.file_history.next += 1;
        let token = self.file_history.next;
        self.file_history.undo.push(Record { token, label, changes, entries });
        if self.file_history.undo.len() > 100 { self.file_history.undo.remove(0); }
        self.file_history.redo.clear(); self.file_history.prefer_files = true;
        token
    }

    /// Fold `changes` into the newest record when it made one of `ids`: a
    /// drop places what it imported, and one undo takes both back.
    pub(crate) fn amend_file_record(&mut self, ids: &[String], changes: Vec<FileDelta>) {
        let Some(record) = self.file_history.undo.last_mut().filter(|r| r.changes.iter().any(|c| ids.contains(&c.id))) else { return };
        let mut merged = std::mem::take(&mut record.changes);
        for delta in changes {
            match merged.iter_mut().find(|c| c.id == delta.id) { Some(c) => c.after = delta.after, None => merged.push(delta) }
        }
        merged.retain(|c| c.before != c.after);
        let entries = self.file_record_entries(&merged);
        let record = self.file_history.undo.last_mut().unwrap();
        record.changes = merged; record.entries = entries;
    }

    pub(crate) fn forget_file_change(&mut self, token: u64) {
        self.file_history.undo.retain(|r| r.token != token);
        self.file_history.redo.retain(|r| r.token != token);
    }

    pub(crate) fn capture_file_history(&mut self, redo: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.rename.is_some() || self.search_open || self.move_dlg.is_some()
            || (!self.file_history.prefer_files && self.view == View::Doc) { return; }
        cx.stop_propagation();
        self.file_history.requests.push_back(redo);
        self.run_file_history(window, cx);
    }

    fn run_file_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_history.busy || self.file_history.requests.is_empty() { return; }
        if self.tree_writes_pending() {
            self.file_history.busy = true;
            cx.spawn_in(window, async move |this, cx| {
                loop {
                    cx.background_executor().timer(std::time::Duration::from_millis(12)).await;
                    let ready = this.update(cx, |w, _| !w.tree_writes_pending());
                    match ready { Ok(false) => continue, Err(_) => return, Ok(true) => break }
                }
                let _ = this.update_in(cx, |w, window, cx| { w.file_history.busy = false; w.run_file_history(window, cx); cx.notify(); });
            }).detach();
            cx.notify(); return;
        }
        let redo = self.file_history.requests.pop_front().unwrap();
        let record = if redo { self.file_history.redo.last() } else { self.file_history.undo.last() }.cloned();
        let Some(record) = record else { self.file_history.requests.clear(); return; };
        if !self.flush(cx) { self.file_history.requests.clear(); return; }
        for c in &record.changes {
            let content = c.before.is_none() || c.after.is_none() || c.before.as_ref().zip(c.after.as_ref()).is_some_and(|(a,b)| a.title != b.title);
            if content && !self.document_peers(&c.id, window, cx).is_empty() {
                self.show_toast_kind(crate::ui::ToastKind::Error, crate::settings::tr("请先关闭正在编辑这些文档的其他窗口", "Close other windows editing these documents first"), cx);
                self.file_history.requests.clear(); return;
            }
        }
        self.file_history.busy = true;
        let previous = self.entries.to_vec();
        for (a, b) in &record.entries {
            let target = if redo { b } else { a };
            let id = a.as_ref().or(b.as_ref()).unwrap().id.clone();
            if let Some(target) = target {
                if let Some(entry) = self.entries.edit().iter_mut().find(|e| e.id == id) {
                    let delta = record.changes.iter().find(|c| c.id == id).unwrap();
                    let fields_changed = delta.before.as_ref().zip(delta.after.as_ref());
                    if fields_changed.is_some_and(|(a,b)| a.title != b.title) { entry.title = target.title.clone(); entry.meta.folder = target.meta.folder.clone(); }
                    if fields_changed.is_some_and(|(a,b)| (a.parent.clone(),a.order,a.top_order) != (b.parent.clone(),b.order,b.top_order)) {
                        entry.meta.parent = target.meta.parent.clone(); entry.meta.order = target.meta.order; entry.meta.top_order = target.meta.top_order;
                    }
                } else { self.entries.edit().push(target.clone()); }
                if let Some(parent) = &target.meta.parent { self.expanded.insert(parent.clone()); }
            } else { self.entries.edit().retain(|e| e.id != id); }
        }
        // What one step changed is in one top-level folder (nothing moves between them).
        let key = record.changes.iter().find_map(|c| self.store.top_of(&c.id).or_else(|| self.store.trash_top(&c.id)));
        let Some((key, mut writer)) = key.and_then(|k| Some((k.clone(), self.store.top_writer(&k)?))) else { self.file_history.busy = false; self.entries.set(previous); self.file_history.requests.clear(); return; };
        let changes = record.changes.clone();
        let task = cx.background_executor().spawn(async move {
            let result = writer.apply_file_history(&changes, redo);
            let entries = writer.list();
            (result, entries)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (result, entries) = task.await;
            let _ = this.update_in(cx, |w, window, cx| {
                let entries = w.store.merge_entries(&key, entries);
                w.file_history.busy = false;
                match result {
                    Ok(()) => {
                        w.entries.set(entries);
                        if redo { w.file_history.redo.pop(); w.file_history.undo.push(record.clone()); }
                        else { w.file_history.undo.pop(); w.file_history.redo.push(record.clone()); }
                        if let Some(id) = w.current.clone() {
                            if let Some(entry) = w.entries.iter().find(|e| e.id == id) {
                                if let Some(ed) = &w.editor { let title = entry.title.clone(); let _ = w.store.accept_file_title(&id, &title); ed.update(cx, |e, cx| e.receive_file_title(&title, cx)); }
                            } else { w.editor = None; w.editor_sub = None; w.current = None; w.view = View::Home; window.focus(&w.focus, cx); }
                        }
                        if w.folder_current.as_ref().is_some_and(|id| !w.entries.iter().any(|e| &e.id == id)) { w.folder_current = None; w.view = View::Home; }
                        w.trash = w.store.list_trash();
                        w.batch = None; w.row_menu = None;
                        w.sync_sub_pages(cx);
                        w.show_toast(&format!("{}{}", if redo { crate::settings::tr("已重做：", "Redone: ") } else { crate::settings::tr("已撤销：", "Undone: ") }, record.label), cx);
                    }
                    Err(e) => {
                        w.entries.set(if entries.is_empty() && !previous.is_empty() { previous } else { entries });
                        w.file_history.requests.clear();
                        w.show_toast_kind(crate::ui::ToastKind::Error, &e.to_string(), cx);
                    }
                }
                w.file_history.prefer_files = true; w.sort();
                w.run_file_history(window, cx); cx.notify();
            });
        }).detach();
        cx.notify();
    }
}
