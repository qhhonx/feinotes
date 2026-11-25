use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileState {
    pub title: String,
    pub folder: bool,
    pub parent: Option<String>,
    pub order: u64,
    pub top_order: Option<u64>,
}
impl FileState {
    pub fn of(e: &Entry) -> Self { Self { title: e.title.clone(), folder: e.meta.folder.is_some(), parent: e.meta.parent.clone(), order: e.meta.order, top_order: e.meta.top_order } }
    fn place(&self) -> (&Option<String>, u64, Option<u64>) { (&self.parent, self.order, self.top_order) }
}

#[derive(Clone)]
pub struct FileDelta { pub id: String, pub before: Option<FileState>, pub after: Option<FileState> }

/// A scoped journal of successful writes, independent of any window's cached list.
#[derive(Default)]
pub struct FileChanges(std::sync::Arc<std::sync::Mutex<Vec<FileDelta>>>, bool);
impl FileChanges {
    /// A journal nothing writes to (no library to watch).
    pub fn none() -> Self { FileChanges(Default::default(), true) }
    pub fn take(self) -> Vec<FileDelta> {
        if self.1 { return vec![]; }
        let mut merged: HashMap<String, FileDelta> = HashMap::new();
        for delta in std::mem::take(&mut *self.0.lock().unwrap()) {
            if let Some(previous) = merged.get_mut(&delta.id) { previous.after = delta.after; }
            else { merged.insert(delta.id.clone(), delta); }
        }
        let mut changes: Vec<_> = merged.into_values().filter(|c| c.before != c.after).collect();
        changes.sort_by(|a, b| a.id.cmp(&b.id));
        changes
    }
}

impl Store {
    pub fn track_file_changes(&mut self) -> FileChanges {
        self.file_changes.retain(|w| w.strong_count() > 0);
        let changes = FileChanges(Default::default(), !self.file_changes.is_empty());
        self.file_changes.push(std::sync::Arc::downgrade(&changes.0));
        changes
    }

    /// Record into `changes` too: one journal across every top-level folder.
    pub fn share_file_changes(&mut self, changes: &FileChanges) {
        self.file_changes.retain(|w| w.strong_count() > 0);
        self.file_changes.push(std::sync::Arc::downgrade(&changes.0));
    }

    pub(super) fn file_state(&self, id: &str) -> Option<FileState> {
        let m = self.index.get(id)?;
        let title = m.folder.clone().or_else(|| self.load(id).ok().map(|d| d.title.text))?;
        Some(FileState { title, folder: m.folder.is_some(), parent: m.parent.clone(), order: m.order, top_order: m.top_order })
    }

    pub(super) fn record_file_change(&self, id: &str, before: Option<FileState>) {
        if !self.file_changes.iter().any(|w| w.strong_count() > 0) { return; }
        let after = self.file_state(id);
        if before == after { return; }
        let delta = FileDelta { id: id.to_string(), before, after };
        for journal in self.file_changes.iter().filter_map(|w| w.upgrade()) { journal.lock().unwrap().push(delta.clone()); }
    }

    pub(super) fn record_tree_changes(&self, previous: &HashMap<String, Meta>) {
        if !self.file_changes.iter().any(|w| w.strong_count() > 0) { return; }
        for (id, m) in &self.index {
            let old = previous.get(id);
            if old.is_some_and(|o| (&o.parent, o.order, o.top_order) == (&m.parent, m.order, m.top_order)) { continue; }
            let before = old.and_then(|o| self.file_state(id).map(|mut state| {
                state.parent = o.parent.clone(); state.order = o.order; state.top_order = o.top_order; state
            }));
            self.record_file_change(id, before);
        }
    }
}

impl Store {
    pub fn accept_file_title(&self, id: &str, title: &str) -> Result<()> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite_manages(id) { return self.accept_sqlite_title(id, title); }
        let mut loaded = self.loaded.borrow_mut();
        let Some(previous) = loaded.get(id) else { return Ok(()) };
        // A Markdown file's title is not in the file.
        if self.is_markdown(id) { return Ok(()); }
        let mut expected = docxml::parse(previous)?;
        expected.title = crate::model::RichText::plain(title);
        let disk = crate::vfs::read_to_string(self.path(id))?;
        // Accept only our title change, never unrelated edits from another instance.
        if docxml::serialize(&expected) == docxml::serialize(&docxml::parse(&disk)?) { loaded.insert(id.to_string(), disk); }
        Ok(())
    }

    /// Apply one management gesture under the library lock. Content is read fresh;
    /// only titles and tree fields are changed, never restored from old snapshots.
    /// Everything is checked before the first file moves.
    pub fn apply_file_history(&mut self, changes: &[FileDelta], redo: bool) -> Result<()> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() { return self.apply_sqlite_file_history(changes, redo); }
        let _lock = self.begin_write()?;
        let mut projected = self.index.clone();
        let pairs: Vec<_> = changes.iter().map(|c| (c, if redo { (&c.before, &c.after) } else { (&c.after, &c.before) })).collect();
        let deleted: HashSet<_> = pairs.iter().filter(|(_, (_, to))| to.is_none()).map(|(c, _)| c.id.as_str()).collect();
        for (c, (from, to)) in &pairs {
            let exists = self.index_has(&c.id);
            anyhow::ensure!(exists == from.is_some(), "文件状态已改变，未执行撤销或重做：{}", c.id);
            if let Some(expected) = from {
                let m = &self.index[&c.id];
                if to.as_ref().is_none_or(|t| t.place() != expected.place()) {
                    anyhow::ensure!((&m.parent, m.order, m.top_order) == expected.place(), "文件位置已被其他操作改变，请先刷新");
                }
                if let Some(target) = to && target.title != expected.title {
                    let title = if let Some(name) = &m.folder { name.clone() } else { self.load(&c.id)?.title.text };
                    anyhow::ensure!(title == expected.title, "标题已被其他操作改变，未覆盖新标题");
                }
            }
            match (from, to) {
                (Some(_), None) => {
                    anyhow::ensure!(!self.trash.contains_key(&c.id) && !self.trash_path(&c.id).fs_try_exists()?, "回收站存在冲突，未覆盖");
                    projected.remove(&c.id);
                }
                (None, Some(target)) => {
                    let saved = self.trash.get(&c.id).ok_or_else(|| anyhow::anyhow!("恢复内容已不在回收站，无法重做或撤销"))?;
                    anyhow::ensure!(target.folder || self.trash_path(&c.id).fs_is_file(), "恢复内容已不在回收站，无法重做或撤销");
                    let mut meta = saved.meta.clone();
                    meta.parent = target.parent.clone(); meta.order = target.order; meta.top_order = target.top_order;
                    projected.insert(c.id.clone(), meta);
                }
                (Some(from), Some(to)) if from.place() != to.place() => {
                    let m = projected.get_mut(&c.id).unwrap();
                    m.parent = to.parent.clone(); m.order = to.order; m.top_order = to.top_order;
                }
                _ => {}
            }
        }
        // A folder going away must be empty once this gesture's own moves are
        // done (新建子文档 undone moves the page back out of its new folder).
        for (c, _) in pairs.iter().filter(|(_, (f, t))| f.is_some() && t.is_none()) {
            anyhow::ensure!(!projected.iter().any(|(id, m)| m.parent.as_deref() == Some(&c.id) && !deleted.contains(id.as_str())), "文件夹里有后来新增的内容，不能撤销创建");
        }
        // Validate what this gesture moves before touching files (including
        // external changes). Other entries are not its business: an old page
        // whose parent is gone already shows at the top level, and must not
        // block every undo in the library.
        for (id, meta) in changes.iter().filter_map(|c| projected.get(&c.id).map(|m| (&c.id, m))) {
            let mut seen = HashSet::from([id.as_str()]);
            let mut parent = meta.parent.as_deref();
            while let Some(p) = parent {
                anyhow::ensure!(seen.insert(p), "目标层级包含循环");
                let m = projected.get(p).filter(|m| m.folder.is_some()).ok_or_else(|| anyhow::anyhow!("原文件夹已不存在，未改变层级"))?;
                parent = m.parent.as_deref();
            }
        }
        let depth = |index: &HashMap<String, Meta>, id: &str| { let mut n = 0; let mut p = index.get(id).and_then(|m| m.parent.clone()); while let Some(q) = p.filter(|_| n < 64) { n += 1; p = index.get(&q).and_then(|m| m.parent.clone()); } n };
        let result = (|| -> Result<()> {
            // What comes back, folders first; then the moves, so a page can go
            // into a folder that just came back or out of one about to go.
            let mut back: Vec<(&str, &FileState)> = pairs.iter().filter_map(|(c, (f, t))| if f.is_none() { t.as_ref().map(|t| (c.id.as_str(), t)) } else { None }).collect();
            back.sort_by_key(|(id, _)| depth(&projected, id));
            for (id, target) in back {
                self.untrash(id, target.parent.clone(), Some((target.order, target.top_order)))?;
            }
            // Moves and renames.
            for (c, (from, to)) in &pairs {
                let (Some(from), Some(to)) = (from, to) else { continue };
                if from.parent != to.parent && !self.index[&c.id].local() && !self.flat {
                    let rel = self.rel_in(&c.id, to.parent.as_deref());
                    self.relocate(&[(c.id.clone(), rel)])?;
                }
                if from.title != to.title {
                    if !to.folder && !self.index[&c.id].markdown && !self.index[&c.id].local() {
                        let path = self.path(&c.id);
                        let text = crate::vfs::read_to_string(&path)?;
                        let mut doc = docxml::parse(&text)?;
                        doc.title = crate::model::RichText::plain(&to.title);
                        // Only the title changes: links stay as written.
                        let out = docxml::serialize(&doc);
                        if out != text { write_atomic(&path, out.as_bytes())?; }
                    }
                    self.follow_title(&c.id, &to.title, true)?;
                    if let Some(m) = self.index.get_mut(&c.id) { m.modified = now(); }
                }
                if let Some(m) = self.index.get_mut(&c.id) && from.place() != to.place() {
                    m.parent = to.parent.clone(); m.order = to.order; m.top_order = to.top_order;
                }
            }
            // What goes to the 回收站, deepest first; the root is the outermost
            // folder going with it.
            let mut gone: Vec<&str> = pairs.iter().filter(|(_, (f, t))| f.is_some() && t.is_none()).map(|(c, _)| c.id.as_str()).collect();
            gone.sort_by_key(|id| std::cmp::Reverse(depth(&self.index, id)));
            let roots: HashMap<&str, Option<String>> = gone.iter().map(|id| {
                let mut root = None;
                let mut ancestor = self.index.get(*id).and_then(|m| m.parent.clone());
                while let Some(p) = ancestor.filter(|p| deleted.contains(p.as_str())) { ancestor = self.index.get(&p).and_then(|m| m.parent.clone()); root = Some(p); }
                (*id, root)
            }).collect();
            for id in gone { self.trash_one(id, roots[id].clone())?; }
            self.save_index()?;
            self.save_trash()
        })();
        for c in changes { self.parsed.remove(&c.id); self.loaded.borrow_mut().remove(&c.id); }
        result.map_err(|e| anyhow::anyhow!("{e}；已完成的部分保留在文档库"))
    }

    #[cfg(feature = "sqlite-store")]
    fn apply_sqlite_file_history(&mut self, changes: &[FileDelta], redo: bool) -> Result<()> {
        let _lock = self.begin_write()?;
        let mut moves = vec![];
        let mut titles = vec![];
        let mut index = self.index.clone();
        let mut trash = self.trash.clone();
        let pairs: Vec<_> = changes.iter().map(|c| (c, if redo { (&c.before, &c.after) } else { (&c.after, &c.before) })).collect();
        let deleted: std::collections::HashSet<_> = pairs.iter().filter(|(_, (_, to))| to.is_none()).map(|(c, _)| c.id.as_str()).collect();
        for (c, (from, to)) in &pairs {
            let exists = self.index_has(&c.id);
            anyhow::ensure!(exists == from.is_some(), "文件状态已改变，未执行撤销或重做：{}", c.id);
            if let Some(expected) = from {
                let m = &self.index[&c.id];
                if to.as_ref().is_none_or(|t| t.place() != expected.place()) {
                    anyhow::ensure!((&m.parent, m.order, m.top_order) == expected.place(), "文件位置已被其他操作改变，请先刷新");
                }
                if let Some(target) = to && target.title != expected.title {
                    let title = if let Some(name) = &m.folder { name.clone() } else { self.load(&c.id)?.title.text };
                    anyhow::ensure!(title == expected.title, "标题已被其他操作改变，未覆盖新标题");
                }
            }
            match (from, to) {
                (Some(_), None) => {
                    anyhow::ensure!(!self.index.iter().any(|(id, m)| m.parent.as_deref() == Some(&c.id) && !deleted.contains(id.as_str())), "文件夹里有后来新增的内容，不能撤销创建");
                    anyhow::ensure!(!trash.contains_key(&c.id), "回收站存在冲突，未覆盖");
                    let meta = index.remove(&c.id).unwrap();
                    let mut ancestor = meta.parent.as_deref();
                    let mut root = None;
                    while let Some(p) = ancestor.filter(|p| deleted.contains(*p)) { root = Some(p.to_string()); ancestor = self.index.get(p).and_then(|m| m.parent.as_deref()); }
                    if meta.folder.is_none() {
                        moves.push((c.id.clone(), true));
                    }
                    trash.insert(c.id.clone(), Trashed { meta, deleted: now(), root });
                }
                (None, Some(target)) => {
                    let saved = trash.remove(&c.id).ok_or_else(|| anyhow::anyhow!("恢复内容已不在回收站，无法重做或撤销"))?;
                    if !target.folder {
                        moves.push((c.id.clone(), false));
                    }
                    let mut meta = saved.meta;
                    meta.parent = target.parent.clone(); meta.order = target.order; meta.top_order = target.top_order;
                    index.insert(c.id.clone(), meta);
                }
                (Some(from), Some(to)) => {
                    let m = index.get_mut(&c.id).unwrap();
                    if from.place() != to.place() { m.parent = to.parent.clone(); m.order = to.order; m.top_order = to.top_order; }
                    if from.title != to.title {
                        if to.folder { m.folder = Some(to.title.clone()); }
                        else {
                            if m.markdown { m.name = Some(to.title.clone()); }
                            titles.push((c.id.clone(), to.title.clone()));
                        }
                        m.modified = now();
                    }
                }
                _ => {},
            }
        }
        // Validate what this gesture moves before touching files (including
        // external changes). Other entries are not its business: an old page
        // whose parent is gone already shows at the top level, and must not
        // block every undo in the library.
        for (id, meta) in changes.iter().filter_map(|c| index.get(&c.id).map(|m| (&c.id, m))) {
            let mut seen = std::collections::HashSet::from([id.as_str()]);
            let mut parent = meta.parent.as_deref();
            while let Some(p) = parent {
                anyhow::ensure!(seen.insert(p), "目标层级包含循环");
                let m = index.get(p).filter(|m| m.folder.is_some()).ok_or_else(|| anyhow::anyhow!("原文件夹已不存在，未改变层级"))?;
                parent = m.parent.as_deref();
            }
        }
        self.finish_sqlite_history(&index, &trash, &moves, &titles)?;
        self.index = index;
        self.trash = trash;
        for (id, to_trash) in moves {
            if to_trash { crate::versions::move_to_trash(&self.history_base(), &id); }
            else { crate::versions::restore_from_trash(&self.history_base(), &id); }
        }
        for c in changes { self.loaded.borrow_mut().remove(&c.id); }
        Ok(())
    }
}
