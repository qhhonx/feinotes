use super::*;

impl Store {
    /// Independent handle for background tree writes; begin_write reloads under the library lock.
    pub fn tree_writer(&self) -> Self {
        Self {
            #[cfg(feature = "sqlite-store")]
            sqlite: self.sqlite.clone(),
            file_changes: vec![], root: self.root.clone(), index: self.index.clone(), trash: HashMap::new(),
            loaded: Default::default(), copied: Default::default(), parsed: HashMap::new(), warnings: vec![] }
    }

    pub(super) fn migrate_folders(&mut self) -> Result<()> {
        let _lock = self.begin_write()?;
        let marker: FormatMarker = serde_json::from_slice(&std::fs::read(self.root.join("format.json"))?)?;
        if marker.format < BASE_FORMAT {
            let backup = self.root.join("index.before-folders.json");
            if !backup.exists() { write_atomic(&backup, &serde_json::to_vec_pretty(&self.index)?)?; }
        }
        let legacy: Vec<String> = self.index.values().filter_map(|m| m.parent.clone())
            .filter(|id| self.index.get(id).is_some_and(|m| m.folder.is_none()) && self.path(id).is_file())
            .collect::<std::collections::HashSet<_>>().into_iter().collect();
        if !legacy.is_empty() {
            let backup = self.root.join("index.before-folders.json");
            if !backup.exists() { write_atomic(&backup, &serde_json::to_vec_pretty(&self.index)?)?; }
            let previous = self.index.clone();
            let mapping: HashMap<String, String> = legacy.iter().map(|id| (id.clone(), uuid::Uuid::new_v4().to_string())).collect();
            for id in &legacy {
                let doc = self.load(id)?;
                let mut meta = self.index[id].clone();
                meta.folder = Some(doc.title.text.clone());
                meta.pinned = false;
                meta.starred = 0;
                meta.parent = meta.parent.map(|p| mapping.get(&p).cloned().unwrap_or(p));
                self.index.insert(mapping[id].clone(), meta);
            }
            for (id, meta) in &mut self.index {
                if meta.folder.is_some() { continue; }
                if let Some(folder) = mapping.get(id) { meta.parent = Some(folder.clone()); meta.order = 1; }
                else if let Some(folder) = meta.parent.as_ref().and_then(|p| mapping.get(p)) { meta.parent = Some(folder.clone()); }
            }
            // Number each new folder's contents in their old order (the page that
            // became the folder first, then its children oldest first), so the
            // tree reads as it did before.
            for (doc, folder) in &mapping {
                let mut kids: Vec<String> = self.index.iter().filter(|(_, m)| m.parent.as_ref() == Some(folder)).map(|(k, _)| k.clone()).collect();
                let old = |k: &String| { let m = &self.index[k]; (k != doc, if m.order > 0 { m.order } else { m.created * 1000 }, k.clone()) };
                kids.sort_by_key(old);
                for (i, k) in kids.iter().enumerate() { if let Some(m) = self.index.get_mut(k) { m.order = i as u64 + 1; m.root_order = None; } }
            }
            if let Err(e) = self.save_index() { self.index = previous; return Err(e); }
            self.record_tree_changes(&previous);
        }
        if marker.format < BASE_FORMAT { write_atomic(&self.root.join("format.json"), &serde_json::to_vec_pretty(&FormatMarker { format: BASE_FORMAT })?)?; }
        Ok(())
    }

    fn validate_folder(&self, id: Option<&str>, parent: Option<&str>) -> Result<()> {
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
        self.index.insert(id.clone(), Meta { folder: Some(title.to_string()), parent: parent.map(str::to_string), created: t, modified: t, ..Default::default() });
        if let Err(e) = self.save_index() { self.index.remove(&id); return Err(e); }
        self.record_file_change(&id, None);
        Ok(id)
    }

    pub fn initialize_manual_order(&mut self) -> Result<()> {
        let _lock = self.begin_write()?;
        let mut settings = self.settings();
        let initialized = settings.home_manual_initialized;
        let previous = self.index.clone();
        let mut groups: HashMap<Option<String>, Vec<String>> = HashMap::new();
        for (id, m) in &self.index { groups.entry(m.parent.clone()).or_default().push(id.clone()); }
        let mut changed = false;
        // Freeze the order the sidebar shows (`Meta::place`): pages not yet placed
        // keep their spot at the top instead of dropping to the end.
        for (parent, mut ids) in groups {
            ids.sort_by(|a, b| (self.index[a].place(), a).cmp(&(self.index[b].place(), b)));
            if initialized && ids.iter().all(|id| self.index[id].place().0) { continue; }
            changed = true;
            for (i, id) in ids.iter().enumerate() {
                let m = self.index.get_mut(id).unwrap();
                if parent.is_none() { m.root_order = Some(i as u64 + 1); }
                else { m.order = i as u64 + 1; m.root_order = None; }
            }
        }
        if !changed && initialized { return Ok(()); }
        if let Err(e) = self.save_index() { self.index = previous; return Err(e); }
        self.record_tree_changes(&previous);
        settings.home_manual_initialized = true;
        self.save_settings(&settings)
    }

    /// Persist one drop atomically; neither document content nor timestamps change.
    pub fn move_in_tree(&mut self, id: &str, parent: Option<&str>, before: Option<&str>) -> Result<()> {
        let _lock = self.begin_write()?;
        anyhow::ensure!(self.index_has(id), "文档或文件夹不存在");
        self.validate_folder(Some(id), parent)?;
        if let Some(anchor) = before {
            anyhow::ensure!(self.index_has(anchor) && self.index[anchor].parent.as_deref() == parent, "目标位置已改变，请重试");
            if anchor == id { return Ok(()); }
        }
        let mut siblings: Vec<String> = self.index.iter().filter(|(k, m)| self.index_has(k) && m.parent.as_deref() == parent).map(|(k, _)| k.clone()).collect();
        // The sidebar's order (`Meta::place`, ties by id), so the numbers written
        // here match the order the drop was made in.
        siblings.sort_by(|ka, kb| (self.index[ka].place(), ka).cmp(&(self.index[kb].place(), kb)));
        siblings.retain(|key| key != id);
        let pos = before.and_then(|key| siblings.iter().position(|x| x == key)).unwrap_or(siblings.len());
        siblings.insert(pos, id.to_string());
        let previous = self.index.clone();
        self.index.get_mut(id).unwrap().parent = parent.map(str::to_string);
        for (i, key) in siblings.iter().enumerate() {
            let m = self.index.get_mut(key).unwrap();
            if parent.is_none() { m.root_order = Some(i as u64 + 1); }
            else { m.order = i as u64 + 1; m.root_order = None; }
        }
        if let Err(e) = self.save_index() { self.index = previous; return Err(e); }
        self.record_tree_changes(&previous);
        Ok(())
    }
}
