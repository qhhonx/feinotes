use super::*;

impl Store {
    /// Independent handle for background tree writes; begin_write reloads under the library lock.
    /// It takes the parsed documents along, so its `list` reads only files that changed.
    pub fn tree_writer(&self) -> Self {
        Self { file_changes: vec![], top_dir: self.top_dir.clone(), index: self.index.clone(), foreign: self.foreign.clone(), trash: self.trash.clone(), flat: self.flat,
            loaded: Default::default(), copied: Default::default(), parsed: self.parsed.clone(), checked: self.checked.clone(), writes: Default::default(), warnings: vec![], migrated: None, batch: Default::default(), view: Default::default(),
            v5: self.v5, me: self.me.clone(), clock: self.clock.clone(), seen: self.seen.clone(), merged: self.merged.clone(), key: self.key.clone(), place_by_hand: self.place_by_hand, v6: self.v6, seen_trash: self.seen_trash.clone(), purging: Default::default(), synced: Default::default(), name_later: Default::default() , here: self.here.clone() , clean_due: None }
    }

    pub fn take_warnings(&mut self) -> Vec<String> { std::mem::take(&mut self.warnings) }

    /// Where `id` would go in folder `parent`: its name there, numbered when taken.
    pub(super) fn rel_in(&self, id: &str, parent: Option<&str>) -> String {
        let m = &self.index[id];
        let dir = parent.and_then(|p| self.index.get(p)).map(|p| p.rel.clone()).unwrap_or_default();
        let name = base_name(&m.rel);
        let ext = m.folder.is_none().then(|| ext_of(m));
        let stem = if ext.is_some() { stem_of(name) } else { name };
        // A folder's or Markdown file's name is its title: 「报告 (2).md」 stays that.
        let base = if m.folder.is_some() || m.named_by_file() { stem } else { layout::unnumbered(stem) };
        join_rel(&dir, &self.free_name(&dir, base, ext, None))
    }

    pub(super) fn validate_folder(&self, id: Option<&str>, parent: Option<&str>) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        let mut p = parent;
        while let Some(key) = p {
            anyhow::ensure!(Some(key) != id && seen.insert(key), "不能移动到自身或后代文件夹中");
            let m = self.index.get(key).filter(|m| m.folder.is_some()).ok_or_else(|| anyhow::anyhow!("目标文件夹不存在"))?;
            p = m.parent.as_deref();
        }
        Ok(())
    }

    pub fn create_folder(&mut self, title: &str, parent: Option<&str>) -> Result<String> {
        let _lock = self.begin_write()?;
        self.validate_folder(None, parent)?;
        let id = uuid::Uuid::new_v4().to_string();
        let t = now();
        if self.flat {
            self.index.insert(id.clone(), Meta { folder: Some(title.to_string()), parent: parent.map(str::to_string), created: t, modified: t, ..Default::default() });
            if let Err(e) = self.save_index() { self.index.remove(&id); return Err(e); }
            self.record_file_change(&id, None);
            return Ok(id);
        }
        // A folder is a folder on disk, named after its title.
        let dir = parent.and_then(|p| self.index.get(p)).map(|m| m.rel.clone()).unwrap_or_default();
        let rel = join_rel(&dir, &self.free_name(&dir, &file_name_for(title), None, None));
        crate::vfs::create_dir_all(self.abs(&rel))?;
        self.index.insert(id.clone(), Meta { folder: Some(String::new()), rel: rel.clone(), created: t, modified: t, ..Default::default() });
        derive_tree(&mut self.index);
        if let Err(e) = self.save_index() { self.index.remove(&id); let _ = crate::vfs::remove_dir(self.abs(&rel)); return Err(e); }
        self.record_file_change(&id, None);
        Ok(id)
    }

    /// True when it numbered something (else nothing was written).
    pub fn initialize_manual_order(&mut self) -> Result<bool> {
        let _lock = self.begin_write()?;
        let mut settings = self.settings();
        let initialized = settings.home_manual_initialized;
        let mut groups: HashMap<Option<&str>, Vec<&str>> = HashMap::new();
        for (id, m) in &self.index { groups.entry(m.parent.as_deref()).or_default().push(id); }
        // Freeze the order the sidebar shows (`Meta::place`): pages not yet placed
        // keep their spot at the top instead of dropping to the end.
        let mut numbers = vec![];
        for (parent, mut ids) in groups {
            ids.sort_by(|a, b| (self.index[*a].place(), a).cmp(&(self.index[*b].place(), b)));
            if initialized && ids.iter().all(|id| self.index[*id].place().0) { continue; }
            for (i, id) in ids.into_iter().enumerate() {
                let m = &self.index[id];
                let n = if parent.is_none() { (m.order, Some(i as u64 + 1)) } else { (i as u64 + 1, None) };
                // Numbered so already: nothing to write. A FORMAT 5 library keeps no
                // `home_manual_initialized` (its settings live in the device files),
                // so every folder opened came here and rewrote the whole index.
                if (m.order, m.top_order) != n { numbers.push((id.to_string(), n)); }
            }
        }
        if numbers.is_empty() { return Ok(false); }
        let previous = self.index.clone();
        for (id, (order, top_order)) in numbers { if let Some(m) = self.index.get_mut(&id) { (m.order, m.top_order) = (order, top_order); } }
        if let Err(e) = self.save_index() { self.index = previous; return Err(e); }
        self.record_tree_changes(&previous);
        settings.home_manual_initialized = true;
        self.save_settings(&settings).map(|()| true)
    }

    /// Persist one drop atomically; neither document content nor timestamps change.
    pub fn move_in_tree(&mut self, id: &str, parent: Option<&str>, before: Option<&str>) -> Result<()> {
        self.move_in_tree_with(id, parent, before, true)
    }

    /// `hand_order`: also rewrite the siblings' order numbers, which a drop or
    /// a new page under 手动排序 needs. 自动排序's new pages pass false, so the
    /// order 手动排序 left behind stays exactly as it was.
    pub(super) fn move_in_tree_with(&mut self, id: &str, parent: Option<&str>, before: Option<&str>, hand_order: bool) -> Result<()> {
        let _lock = self.begin_write()?;
        anyhow::ensure!(self.index_has(id), "文档或文件夹不存在");
        self.validate_folder(Some(id), parent)?;
        if let Some(anchor) = before {
            anyhow::ensure!(self.index.get(anchor).is_some_and(|m| m.parent.as_deref() == parent) && self.index_has(anchor), "目标位置已改变，请重试");
            if anchor == id { return Ok(()); }
        }
        // Siblings straight from the index: a disk check per entry made an
        // import of N files walk the whole library N times (#216); entries
        // whose file vanished stay unlisted in `list` either way.
        let mut siblings: Vec<String> = self.index.iter().filter(|(_, m)| m.parent.as_deref() == parent).map(|(k, _)| k.clone()).collect();
        // The sidebar's order (`Meta::place`, ties by id), so the numbers written
        // here match the order the drop was made in.
        siblings.sort_by(|ka, kb| (self.index[ka].place(), ka).cmp(&(self.index[kb].place(), kb)));
        siblings.retain(|key| key != id);
        let pos = before.and_then(|key| siblings.iter().position(|x| x == key)).unwrap_or(siblings.len());
        siblings.insert(pos, id.to_string());
        let mut previous = self.index.clone();
        if self.index[id].parent.as_deref() != parent && !self.index[id].local() && !self.flat {
            // Into another folder: the file moves there, keeping its name when it is free.
            self.relocate(&[(id.to_string(), self.rel_in(id, parent))])?;
            for (k, m) in &self.index { if let Some(p) = previous.get_mut(k) { p.rel = m.rel.clone(); p.folder = m.folder.clone(); p.name = m.name.clone(); } }
        }
        self.index.get_mut(id).unwrap().parent = parent.map(str::to_string);
        if hand_order {
            for (i, key) in siblings.iter().enumerate() {
                let m = self.index.get_mut(key).unwrap();
                if parent.is_none() { m.top_order = Some(i as u64 + 1); }
                else { m.order = i as u64 + 1; m.top_order = None; }
            }
        }
        if let Err(e) = self.save_index() { self.index = previous; return Err(e); }
        self.record_tree_changes(&previous);
        Ok(())
    }
}
