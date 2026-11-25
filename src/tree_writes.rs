//! Writes to the tree the window shows before they are on disk: a drop in
//! the sidebar, 重命名, a file name following its document's title. The rows
//! change at once; the write is queued and made off the window's thread by a
//! copy of the library (`Store::tree_writer`), one at a time. Renaming a
//! folder rewrites every link into and out of it, each file synced to disk:
//! over a second for a few hundred pages, which Enter used to wait for.
//!
//! A write names what it changes by id, never by path, so one queued behind
//! a rename finds the page wherever it is by then. Until a write has landed
//! the window's own library still holds the old paths, so:
//! - a page it moves is opened, deleted or imported into once it has landed
//!   (`after_tree_writes`), and the open document's autosave waits for it;
//! - a save that cannot wait (⌘S, leaving the document, quitting) lands
//!   every queued write first, on the window's thread (`settle_tree_writes`);
//! - anything else that writes takes the library's lock, behind the write
//!   on its way, and reads the index again under it (`Store::begin_write`).

use crate::{app::Workspace, store::{Entry, FileDelta, Store}, ui::ToastKind};
use gpui::*;
use std::{collections::VecDeque, time::Duration};

#[derive(Clone)]
pub(crate) struct PendingMove { pub id: String, pub parent: Option<String>, pub before: Option<String>, pub history: u64,
    /// One of several pages moved together (移动到), under one record of the file history.
    pub joined: bool }

#[derive(Clone)]
pub(crate) struct PendingRename {
    /// The document or folder as its library knows it (a folder with a 正文: the 正文).
    pub id: String,
    pub title: String,
    /// 重命名 in the sidebar: what to go back to when it fails. `None`: a
    /// title typed in the editor and saved already, whose file name follows.
    pub gesture: Option<RenameGesture>,
}

#[derive(Clone)]
pub(crate) struct RenameGesture { pub row: String, pub was: String, pub history: u64 }

#[derive(Clone)]
pub(crate) enum TreeWrite { Move(PendingMove), Rename(PendingRename) }

impl TreeWrite {
    fn id(&self) -> &str { match self { TreeWrite::Move(m) => &m.id, TreeWrite::Rename(r) => &r.id } }
    fn history(&self) -> u64 { match self { TreeWrite::Move(m) => m.history, TreeWrite::Rename(r) => r.gesture.as_ref().map_or(0, |g| g.history) } }
}

type Later = Box<dyn FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

#[derive(Default)]
pub(crate) struct TreeWrites {
    /// The first is being written while `saving`.
    pending: VecDeque<TreeWrite>,
    saving: bool,
    /// Goes up when `settle_tree_writes` makes the writes itself: the answer
    /// of the one that was on its way is then dropped.
    epoch: u64,
    /// What waits for the queue to empty, in the order it was asked for.
    after: VecDeque<Later>,
}

/// What a writer brings back from the background.
struct Written { result: anyhow::Result<()>, entries: Option<Vec<Entry>>, changes: Vec<FileDelta>, writer: Store, warnings: Vec<String> }

fn move_failed(e: &anyhow::Error) -> String { crate::localized_format!("移动失败：{}", "Could not move document: {}", crate::settings::diagnostic(&e.to_string())) }
fn rename_failed(e: &anyhow::Error) -> String { crate::localized_format!("重命名失败：{}", "Could not rename: {}", crate::settings::diagnostic(&e.to_string())) }
fn name_not_followed(e: &anyhow::Error) -> String { crate::localized_format!("未能按标题重命名文件：{}", "Could not rename the file after its title: {}", crate::settings::diagnostic(&e.to_string())) }

impl Workspace {
    pub(crate) fn tree_writes_pending(&self) -> bool { self.tree_writes.saving || !self.tree_writes.pending.is_empty() }

    /// Whether a pending write moves `id`'s file: itself, or a folder above
    /// it. Where it is on disk is not known here until that write has landed.
    pub(crate) fn awaits_tree_write(&self, id: &str) -> bool {
        if self.tree_writes.pending.is_empty() { return false; }
        // A 正文 moves with its folder, and everything in it.
        let moved: Vec<String> = self.tree_writes.pending.iter().map(|w| self.row_of(w.id())).collect();
        let mut at = Some(self.row_of(id));
        while let Some(row) = at {
            if moved.contains(&row) { return true; }
            at = self.entries.iter().find(|e| e.id == row).and_then(|e| e.meta.parent.clone());
        }
        false
    }

    /// Run `then` once every pending write has landed; false (and nothing
    /// kept) when none is pending, for the caller to go on right away.
    pub(crate) fn after_tree_writes(&mut self, then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static) -> bool {
        if !self.tree_writes_pending() { return false; }
        self.tree_writes.after.push_back(Box::new(then));
        true
    }

    /// The next of `after`, with nothing pending: one at a time, as each may queue a write of its own.
    fn run_after_tree_writes(&mut self, cx: &mut Context<Self>) {
        if self.tree_writes_pending() || self.tree_writes.after.is_empty() { return; }
        let window = self.window;
        cx.spawn(async move |this, cx| {
            let _ = window.update(cx, |_, window, cx| {
                let _ = this.update(cx, |w, cx| {
                    if w.tree_writes_pending() { return; }
                    if let Some(then) = w.tree_writes.after.pop_front() { then(w, window, cx); }
                    w.run_after_tree_writes(cx);
                    cx.notify();
                });
            });
        }).detach();
    }

    pub(crate) fn queue_tree_write(&mut self, write: TreeWrite, cx: &mut Context<Self>) {
        // A title typed on: only the last name is worth following.
        if let TreeWrite::Rename(PendingRename { id, gesture: None, .. }) = &write {
            let first = usize::from(self.tree_writes.saving);
            let mut i = 0;
            self.tree_writes.pending.retain(|w| { i += 1; i <= first || !matches!(w, TreeWrite::Rename(r) if r.gesture.is_none() && r.id == *id) });
        }
        self.tree_writes.pending.push_back(write);
        self.save_next_tree_write(cx);
        // A 数据表 writes its file as it is edited, by path: the open one's move lands before the next cell does.
        if self.editor.as_ref().is_some_and(|e| e.read(cx).table_file) && self.current.clone().is_some_and(|c| self.awaits_tree_write(&c)) { self.settle_tree_writes(cx); }
    }

    /// 重命名 from the sidebar: the row, the pages showing it and the open
    /// document take `title` now, the library when the write lands. `row` is
    /// the row that was edited, `id` what it names in the library.
    pub(crate) fn rename_ahead(&mut self, row: &str, id: &str, title: &str, cx: &mut Context<Self>) {
        let Some(old) = self.entries.iter().find(|e| e.id == id).cloned() else { return };
        let mut rename = PendingRename { id: id.to_string(), title: title.to_string(), gesture: None };
        self.apply_tree_rename(&rename);
        let new = self.entries.iter().find(|e| e.id == id).cloned();
        let history = self.remember_title(old.clone(), new, crate::settings::tr("重命名", "Rename"));
        rename.gesture = Some(RenameGesture { row: row.to_string(), was: old.title.trim().to_string(), history });
        if self.current.as_deref() == Some(id) && let Some(ed) = &self.editor { ed.update(cx, |e, cx| e.receive_file_title(title, cx)); }
        self.queue_tree_write(TreeWrite::Rename(rename), cx);
        self.sort();
    }

    /// The pending writes over rows listed from the library, which does not have them yet.
    pub(crate) fn reapply_pending_tree_writes(&mut self) {
        for write in self.tree_writes.pending.clone() {
            match &write { TreeWrite::Move(m) => self.apply_tree_move(m), TreeWrite::Rename(r) => self.apply_tree_rename(r) }
        }
    }

    pub(crate) fn apply_tree_move(&mut self, movement: &PendingMove) {
        let tree = crate::tree::Tree::new(&self.entries);
        let mut siblings: Vec<String> = tree.children(movement.parent.as_deref()).iter()
            .filter(|e| e.id != movement.id).map(|e| e.id.clone()).collect();
        let pos = movement.before.as_ref().and_then(|id| siblings.iter().position(|key| key == id)).unwrap_or(siblings.len());
        siblings.insert(pos, movement.id.clone());
        for e in self.entries.edit().iter_mut() {
            if e.id == movement.id { e.meta.parent = movement.parent.clone(); }
            if let Some(i) = siblings.iter().position(|id| *id == e.id) {
                if movement.parent.is_none() { e.meta.top_order = Some(i as u64 + 1); }
                else { e.meta.order = i as u64 + 1; e.meta.top_order = None; }
            }
        }
    }

    fn apply_tree_rename(&mut self, r: &PendingRename) {
        // The open document's title is the editor's, typed on since this was queued.
        if r.gesture.is_none() && self.current.as_deref() == Some(r.id.as_str()) { return; }
        let Some(e) = self.entries.edit().iter_mut().find(|e| e.id == r.id) else { return };
        if e.title != r.title { e.meta.modified = e.meta.modified.max(crate::time::now()); }
        e.title = r.title.clone();
        if e.meta.folder.is_some() { e.meta.folder = Some(r.title.clone()); }
    }

    /// `write` made by `store`, a writer off the window's thread or the window's own (`settle_tree_writes`).
    fn write_tree(store: &mut Store, write: &TreeWrite, parent: Option<&str>, before: Option<&str>) -> anyhow::Result<()> {
        match write {
            TreeWrite::Move(m) => store.move_in_tree(&m.id, parent, before),
            TreeWrite::Rename(r) if r.gesture.is_some() => store.rename(&r.id, &r.title).map(|_| ()),
            TreeWrite::Rename(r) => store.follow_name(&r.id, &r.title),
        }
    }

    /// A move's folder and anchor as the library sees them, once it is known to stay inside one top-level folder (W7).
    fn tree_write_place(&self, write: &TreeWrite) -> anyhow::Result<(Option<String>, Option<String>)> {
        let TreeWrite::Move(m) = write else { return Ok((None, None)) };
        self.store.same_top(&m.id, m.parent.as_deref())?;
        Ok((self.store.real_parent(m.parent.as_deref()).map(str::to_string), self.store.real_parent(m.before.as_deref()).map(str::to_string)))
    }

    fn save_next_tree_write(&mut self, cx: &mut Context<Self>) {
        if self.tree_writes.saving { return; }
        let Some(write) = self.tree_writes.pending.front().cloned() else { return self.run_after_tree_writes(cx) };
        let Some((key, n, mut writer)) = self.store.tree_writer_at(write.id()) else { self.tree_writes.pending.pop_front(); return self.save_next_tree_write(cx); };
        self.tree_writes.saving = true;
        let (epoch, window, place) = (self.tree_writes.epoch, self.window, self.tree_write_place(&write));
        let job = write.clone();
        let task = cx.background_executor().spawn(async move {
            // Scripted runs hold the write back (`FDL_TREE_WRITE_DELAY`, ms) to act while it is on its way.
            #[cfg(feature = "devtools")]
            if let Some(ms) = std::env::var("FDL_TREE_WRITE_DELAY").ok().and_then(|v| v.parse().ok()) { std::thread::sleep(Duration::from_millis(ms)); }
            let journal = writer.track_file_changes();
            let at = |w: &Store| w.file_path(job.id()).or_else(|| w.folder_dir(Some(job.id())));
            let was = at(&writer);
            let result = place.and_then(|(parent, before)| Self::write_tree(&mut writer, &job, parent.as_deref(), before.as_deref()));
            // Rows from what the write left where files moved (paths, a name numbered because
            // it was taken, links rewritten), for the library to take over; a failed write's,
            // to put the rows back. A new place among the same siblings moves nothing.
            let entries = (result.is_err() || matches!(job, TreeWrite::Rename(_)) || at(&writer) != was).then(|| writer.list());
            let warnings = writer.take_warnings();
            Written { result, entries, changes: journal.take(), writer, warnings }
        });
        cx.spawn(async move |this, cx| {
            let done = task.await;
            let _ = window.update(cx, |_, window, cx| {
                let _ = this.update(cx, |w, cx| {
                    // Made here meanwhile (`settle_tree_writes`): the rows are that library's.
                    if w.tree_writes.epoch != epoch { return; }
                    w.tree_writes.pending.pop_front();
                    w.tree_writes.saving = false;
                    w.tree_write_done(&key, n, write, done, window, cx);
                    w.save_next_tree_write(cx);
                    cx.notify();
                });
            });
        }).detach();
    }

    fn tree_write_done(&mut self, key: &str, n: u64, write: TreeWrite, done: Written, window: &mut Window, cx: &mut Context<Self>) {
        let Written { result, entries, changes, writer, warnings } = done;
        for warning in warnings { crate::logging::write(&warning); }
        match (&write, result) {
            (TreeWrite::Move(m), Ok(())) => {
                self.tree_move_written(m, changes);
                // Into another folder: the library takes over the writer, so paths are right from here on.
                if let Some(entries) = entries { self.entries.set(self.store.adopt_written(key, n, writer, entries)); self.sort(); self.sync_sub_pages(cx); }
            }
            (TreeWrite::Move(m), Err(e)) => {
                self.forget_file_change(m.history);
                // Restore only hierarchy, preserving edits made while the write was pending.
                if let Some(restored) = entries.map(|r| self.store.merge_entries(key, r)) {
                    for entry in self.entries.edit().iter_mut() {
                        if let Some(saved) = restored.iter().find(|e| e.id == entry.id) {
                            entry.meta.parent = saved.meta.parent.clone();
                            entry.meta.order = saved.meta.order;
                            entry.meta.top_order = saved.meta.top_order;
                        }
                    }
                }
                self.reapply_pending_tree_writes();
                self.sync_sub_pages(cx);
                self.show_toast_kind(ToastKind::Error, &move_failed(&e), cx);
            }
            (TreeWrite::Rename(r), result) => {
                // The library takes over what the writer read and wrote, so paths are right from here on.
                if let Some(entries) = entries { self.entries.set(self.store.adopt_written(key, n, writer, entries)); }
                let open = self.current.as_deref() == Some(r.id.as_str());
                match (&r.gesture, result) {
                    (Some(g), Ok(())) => {
                        self.confirm_tree_history(g.history, changes);
                        // The open document's file has the new title now: its next save is against that.
                        if open { let _ = self.store.accept_file_title(&r.id, &r.title); }
                    }
                    (Some(g), Err(e)) => {
                        self.forget_file_change(g.history);
                        self.rename_undone(r, g, cx);
                        // Back to the field with what was typed, unless the row was renamed again since.
                        let again = self.tree_writes.pending.iter().any(|w| matches!(w, TreeWrite::Rename(next) if next.id == r.id && next.gesture.is_some()));
                        if !again && self.rename.is_none() && self.entries.iter().any(|e| e.id == g.row) {
                            self.start_rename(&g.row, window, cx);
                            if let Some(field) = self.rename.as_ref().map(|n| n.field.clone()) { field.update(cx, |f, cx| { f.set_text(&r.title, cx); f.select_all_text(cx); }); }
                        }
                        self.show_toast_kind(ToastKind::Error, &rename_failed(&e), cx);
                    }
                    (None, Ok(())) => {}
                    (None, Err(e)) => self.show_toast_kind(ToastKind::Error, &name_not_followed(&e), cx),
                }
                self.sort();
                self.sync_sub_pages(cx);
            }
        }
    }

    /// What a move wrote takes the place of what the rows recorded for it.
    fn tree_move_written(&mut self, m: &PendingMove, changes: Vec<FileDelta>) {
        if m.joined { self.join_tree_history(m.history, changes) } else { self.confirm_tree_history(m.history, changes) }
    }

    /// A 重命名 that failed: the open document shows its old title again (the rows are the library's already).
    fn rename_undone(&mut self, r: &PendingRename, g: &RenameGesture, cx: &mut Context<Self>) {
        if self.current.as_deref() == Some(r.id.as_str()) && let Some(ed) = &self.editor { ed.update(cx, |e, cx| e.receive_file_title(&g.was, cx)); }
    }

    /// Make every pending write now, on the window's thread, for what cannot
    /// wait behind them. The one on its way is waited for at the library's
    /// lock and made again, which changes nothing: writes go by id, and a
    /// page already named or placed so stays as it is.
    pub(crate) fn settle_tree_writes(&mut self, cx: &mut Context<Self>) {
        if !self.tree_writes_pending() { return; }
        self.tree_writes.epoch += 1;
        self.tree_writes.saving = false;
        for write in std::mem::take(&mut self.tree_writes.pending) {
            let journal = self.store.track_file_changes();
            let result = self.tree_write_place(&write).and_then(|(parent, before)| self.store.with(Some(write.id()), |s| Self::write_tree(s, &write, parent.as_deref(), before.as_deref())));
            let changes = journal.take();
            match (result, &write) {
                // Nothing recorded: the writer on its way had made it; what the rows recorded stands.
                (Ok(()), TreeWrite::Move(m)) => if !changes.is_empty() { self.tree_move_written(m, changes); },
                (Ok(()), TreeWrite::Rename(_)) => if !changes.is_empty() { self.confirm_tree_history(write.history(), changes); },
                (Err(e), TreeWrite::Rename(r)) => {
                    self.forget_file_change(write.history());
                    if let Some(g) = &r.gesture { self.rename_undone(r, g, cx); }
                    self.show_toast_kind(ToastKind::Error, &if r.gesture.is_some() { rename_failed(&e) } else { name_not_followed(&e) }, cx);
                }
                (Err(e), TreeWrite::Move(_)) => { self.forget_file_change(write.history()); self.show_toast_kind(ToastKind::Error, &move_failed(&e), cx); }
            }
        }
        self.entries.set(self.store.list());
        self.sort();
        self.sync_sub_pages(cx);
        self.run_after_tree_writes(cx);
        cx.notify();
    }

    /// The open document's autosave holds off while a write is on its way (it may move the file).
    pub(crate) async fn tree_writes_landed(this: &WeakEntity<Self>, cx: &mut AsyncWindowContext) {
        while this.update(cx, |w, _| w.store.file_task_busy() || w.tree_writes_pending()).unwrap_or(false) {
            cx.background_executor().timer(Duration::from_millis(50)).await;
        }
    }
}
