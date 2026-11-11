use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileState {
    pub title: String,
    pub folder: bool,
    pub parent: Option<String>,
    pub order: u64,
    pub root_order: Option<u64>,
}
impl FileState {
    pub fn of(e: &Entry) -> Self { Self { title: e.title.clone(), folder: e.meta.folder.is_some(), parent: e.meta.parent.clone(), order: e.meta.order, root_order: e.meta.root_order } }
    fn place(&self) -> (&Option<String>, u64, Option<u64>) { (&self.parent, self.order, self.root_order) }
}

#[derive(Clone)]
pub struct FileDelta { pub id: String, pub before: Option<FileState>, pub after: Option<FileState> }

/// A scoped journal of successful writes, independent of any window's cached list.
#[derive(Default)]
pub struct FileChanges(std::sync::Arc<std::sync::Mutex<Vec<FileDelta>>>, bool);
impl FileChanges {
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

    pub(super) fn file_state(&self, id: &str) -> Option<FileState> {
        let m = self.index.get(id)?;
        let title = m.folder.clone().or_else(|| self.load(id).ok().map(|d| d.title.text))?;
        Some(FileState { title, folder: m.folder.is_some(), parent: m.parent.clone(), order: m.order, root_order: m.root_order })
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
            if old.is_some_and(|o| (&o.parent, o.order, o.root_order) == (&m.parent, m.order, m.root_order)) { continue; }
            let before = old.and_then(|o| self.file_state(id).map(|mut state| {
                state.parent = o.parent.clone(); state.order = o.order; state.root_order = o.root_order; state
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
        let disk = self.read_body(id)?;
        // Accept only our title change, never unrelated edits from another instance.
        if docxml::serialize(&expected) == docxml::serialize(&docxml::parse(&disk)?) { loaded.insert(id.to_string(), disk); }
        Ok(())
    }

    /// Apply one management gesture under the library lock. Content is read fresh;
    /// only titles and tree fields are changed, never restored from old snapshots.
    pub fn apply_file_history(&mut self, changes: &[FileDelta], redo: bool) -> Result<()> {
        let _lock = self.begin_write()?;
        let previous_index = self.index.clone();
        let previous_trash = self.trash.clone();
        let mut files: Vec<(PathBuf, Option<Vec<u8>>, Option<Vec<u8>>)> = vec![];
        #[cfg(feature = "sqlite-store")]
        let sql = self.sqlite.is_some();
        #[cfg(not(feature = "sqlite-store"))]
        let sql = false;
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
                    anyhow::ensure!((&m.parent, m.order, m.root_order) == expected.place(), "文件位置已被其他操作改变，请先刷新");
                }
                if let Some(target) = to && target.title != expected.title {
                    let title = if let Some(name) = &m.folder { name.clone() } else { self.load(&c.id)?.title.text };
                    anyhow::ensure!(title == expected.title, "标题已被其他操作改变，未覆盖新标题");
                }
            }
            match (from, to) {
                (Some(_), None) => {
                    anyhow::ensure!(!self.index.iter().any(|(id, m)| m.parent.as_deref() == Some(&c.id) && !deleted.contains(id.as_str())), "文件夹里有后来新增的内容，不能撤销创建");
                    anyhow::ensure!(!trash.contains_key(&c.id) && (sql || !self.trash_path(&c.id).try_exists()?), "回收站存在冲突，未覆盖");
                    let meta = index.remove(&c.id).unwrap();
                    let mut ancestor = meta.parent.as_deref();
                    let mut root = None;
                    while let Some(p) = ancestor.filter(|p| deleted.contains(*p)) { root = Some(p.to_string()); ancestor = self.index.get(p).and_then(|m| m.parent.as_deref()); }
                    if meta.folder.is_none() {
                        if sql { moves.push((c.id.clone(), true)); }
                        else {
                            let bytes = std::fs::read(self.path(&c.id))?;
                            files.push((self.trash_path(&c.id), None, Some(bytes.clone())));
                            files.push((self.path(&c.id), Some(bytes), None));
                        }
                    }
                    trash.insert(c.id.clone(), Trashed { meta, deleted: now(), root });
                }
                (None, Some(target)) => {
                    let saved = trash.remove(&c.id).ok_or_else(|| anyhow::anyhow!("恢复内容已不在回收站，无法重做或撤销"))?;
                    if !target.folder {
                        if sql { moves.push((c.id.clone(), false)); }
                        else {
                            anyhow::ensure!(!self.path(&c.id).try_exists()?, "目标文档已存在，未覆盖");
                            let bytes = std::fs::read(self.trash_path(&c.id))?;
                            files.push((self.path(&c.id), None, Some(bytes.clone())));
                            files.push((self.trash_path(&c.id), Some(bytes), None));
                        }
                    }
                    let mut meta = saved.meta;
                    meta.parent = target.parent.clone(); meta.order = target.order; meta.root_order = target.root_order;
                    index.insert(c.id.clone(), meta);
                }
                (Some(from), Some(to)) => {
                    let m = index.get_mut(&c.id).unwrap();
                    if from.place() != to.place() { m.parent = to.parent.clone(); m.order = to.order; m.root_order = to.root_order; }
                    if from.title != to.title {
                        if to.folder { m.folder = Some(to.title.clone()); }
                        else if sql {
                            if m.markdown { m.name = Some(to.title.clone()); }
                            titles.push((c.id.clone(), to.title.clone()));
                        }
                        else if m.markdown { m.name = Some(to.title.clone()); }
                        else {
                            let path = self.path(&c.id); let bytes = std::fs::read(&path)?;
                            let mut doc = docxml::parse(std::str::from_utf8(&bytes)?)?;
                            doc.title = crate::model::RichText::plain(&to.title);
                            files.push((path, Some(bytes), Some(docxml::serialize(&doc).into_bytes())));
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
        #[cfg(feature = "sqlite-store")]
        if sql {
            self.finish_sqlite_history(&index, &trash, &moves, &titles)?;
            self.index = index;
            self.trash = trash;
            for c in changes { self.loaded.borrow_mut().remove(&c.id); }
            return Ok(());
        }
        std::fs::create_dir_all(self.trash_dir())?;
        let result = (|| -> Result<()> {
            // Copy destinations first. A crash may leave a duplicate, never lose both copies.
            for (path, _, bytes) in &files { if let Some(bytes) = bytes { write_atomic(path, bytes)?; } }
            save_json(&self.root.join("trash/index.json"), &trash)?;
            save_json(&self.root.join("index.json"), &index)?;
            for (path, _, bytes) in &files { if bytes.is_none() { std::fs::remove_file(path)?; } }
            Ok(())
        })();
        if let Err(error) = result {
            let mut rollback = vec![];
            for (path, bytes, _) in &files {
                let r = if let Some(bytes) = bytes { write_atomic(path, bytes) }
                    else { std::fs::remove_file(path).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) }).map_err(Into::into) };
                if let Err(e) = r { rollback.push(e.to_string()); }
            }
            if let Err(e) = save_json(&self.root.join("index.json"), &previous_index) { rollback.push(e.to_string()); }
            if let Err(e) = save_json(&self.root.join("trash/index.json"), &previous_trash) { rollback.push(e.to_string()); }
            anyhow::bail!("{error}{}", if rollback.is_empty() { String::new() } else { format!("；恢复未完成：{}", rollback.join("；")) });
        }
        self.index = index; self.trash = trash;
        for c in changes { self.parsed.remove(&c.id); self.loaded.borrow_mut().remove(&c.id); }
        Ok(())
    }
}
