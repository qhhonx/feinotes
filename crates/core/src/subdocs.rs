//! Child pages (#357) without a second kind of tree: a folder holding a
//! document of its own name (`方案/方案.xml`) is that document with the
//! rest of the folder as its child pages. Only documents take child pages: a
//! Markdown file is a file of its own, so `方案/方案.md` is a plain file in a
//! plain folder. One more file is its folder's own: a 数据表 with 笔记 columns
//! lives with its notes (`书单/书单.sqlite`, `Store::table_folder`), and shows
//! as the table with the notes under it; where a folder holds both, the
//! document is the one. The rule is read
//! from the disk only, nothing about it is kept in the index, so a library
//! moved there by v0.1.11 (`migrate::pages_to_folders`), copied in from
//! elsewhere or made by hand reads the same way. 新建子文档 under a document
//! that is not one yet puts it in a folder of its name first.

use super::*;

/// Whether `m` sits in a folder named like itself: a document (not a
/// Markdown file or a 画板) in the library, `X/X.xml`, or a 数据表, `X/X.sqlite`.
pub(super) fn note_shape(m: &Meta) -> bool {
    if m.folder.is_some() || m.local() || m.markdown || !(m.kind.is_doc() || m.kind == DocKind::Table) { return false; }
    let dir = parent_rel(&m.rel);
    !dir.is_empty() && stem_of(base_name(&m.rel)) == base_name(dir)
}

/// The listed folders get their 正文: its id, and its title, icon and last
/// change, which are the folder's on screen.
pub(super) fn mark_notes(notes: &HashMap<String, String>, out: &mut [Entry]) {
    if notes.is_empty() { return; }
    let by_id: HashMap<&str, usize> = out.iter().enumerate().map(|(i, e)| (e.id.as_str(), i)).collect();
    let picks: Vec<(usize, usize)> = notes.iter().filter_map(|(f, n)| Some((*by_id.get(f.as_str())?, *by_id.get(n.as_str())?))).collect();
    for (f, n) in picks {
        let (title, icon, modified, id, kind) = (out[n].title.clone(), out[n].icon.clone(), out[n].meta.modified, out[n].id.clone(), out[n].meta.kind);
        let e = &mut out[f];
        e.note = Some(id);
        // a table's folder is listed as the table (its glyph, named by its file)
        e.meta.kind = kind;
        e.title = title;
        e.icon = icon;
        e.meta.modified = e.meta.modified.max(modified);
    }
}

impl Store {
    /// Every folder's 正文 (folder id → document id).
    pub(super) fn folder_notes(&self) -> HashMap<String, String> {
        let mut out: HashMap<String, String> = HashMap::new();
        if self.flat { return HashMap::new(); }
        let dirs: HashMap<&str, &str> = self.index.iter().filter(|(_, m)| m.folder.is_some() && !m.local()).map(|(k, m)| (m.rel.as_str(), k.as_str())).collect();
        for (k, m) in &self.index {
            if !note_shape(m) { continue; }
            let Some(f) = dirs.get(parent_rel(&m.rel)) else { continue };
            if out.get(*f).is_none_or(|other| self.note_rank(k) < self.note_rank(other)) { out.insert(f.to_string(), k.clone()); }
        }
        out
    }

    /// Which of a folder's candidates is its 正文: the document before the 数据表, then by id.
    fn note_rank<'a>(&self, id: &'a str) -> (bool, &'a str) { (self.index.get(id).is_some_and(|m| !m.kind.is_doc()), id) }

    /// Folder `id`'s 正文, if it has one.
    pub fn note_of(&self, id: &str) -> Option<String> {
        let f = self.index.get(id).filter(|m| m.folder.is_some() && !m.local() && !self.flat)?;
        self.index.iter().filter(|(_, m)| m.parent.as_deref() == Some(id) && note_shape(m) && parent_rel(&m.rel) == f.rel).map(|(k, _)| k).min_by_key(|k| self.note_rank(k)).cloned()
    }

    /// The folder 数据表 `id` keeps its notes in, the table itself moved into
    /// one of its name first (`书单.sqlite` → `书单/书单.sqlite`) when it has
    /// none yet: its first 笔记 column asks for this. The folder takes the
    /// table's place and shows as the table; the move is recorded for 撤销.
    pub fn table_folder(&mut self, id: &str) -> Result<String> {
        anyhow::ensure!(!self.flat && self.index.get(id).is_some_and(|m| m.folder.is_none() && !m.local() && m.kind == DocKind::Table), "不是文档库里的数据表");
        match self.folder_of_note(id) { Some(f) => Ok(f), None => self.wrap_in_folder(id) }
    }

    /// The folder document `id` is the 正文 of.
    pub fn folder_of_note(&self, id: &str) -> Option<String> {
        let parent = self.index.get(id).filter(|m| note_shape(m))?.parent.clone()?;
        (self.note_of(&parent).as_deref() == Some(id)).then_some(parent)
    }

    /// Whether 新建子文档 can be offered under `id`: a document in the library
    /// (not a Markdown file, a 画板, a 数据表 or a 本机文件), or a folder with a 正文.
    pub fn can_have_children(&self, id: &str) -> bool {
        if self.flat { return false; }
        match self.index.get(id) {
            Some(m) if m.folder.is_some() => self.note_of(id).is_some(),
            Some(m) => !m.local() && m.kind.is_doc() && !m.markdown && !m.rel.is_empty(),
            None => false,
        }
    }

    /// 新建子文档: `doc` saved as a new child page of document `id` (or of
    /// the folder whose 正文 it is), the last one. A document without children
    /// is first moved into a folder of its own name, which takes its place.
    /// Returns the new document's id; the steps are recorded for 撤销.
    pub fn create_child(&mut self, id: &str, doc: &Document) -> Result<String> {
        anyhow::ensure!(self.can_have_children(id), "不能在这里新建子文档");
        let folder = match self.index.get(id) {
            Some(m) if m.folder.is_some() => id.to_string(),
            _ => match self.folder_of_note(id) { Some(f) => f, None => self.wrap_in_folder(id)? },
        };
        self.create_from_in(doc, Some(&folder))
    }

    /// Put document `id` in a new folder named like its file, as its 正文
    /// (first inside); the folder takes the document's place.
    fn wrap_in_folder(&mut self, id: &str) -> Result<String> {
        let _lock = self.begin_write()?;
        let m = self.index.get(id).filter(|m| m.folder.is_none() && !m.local() && !m.markdown && (m.kind.is_doc() || m.kind == DocKind::Table)).cloned().ok_or_else(|| anyhow::anyhow!("文档不存在"))?;
        let previous = self.index.clone();
        let dir = parent_rel(&m.rel).to_string();
        let name = self.free_name(&dir, stem_of(base_name(&m.rel)), None, None);
        let rel = join_rel(&dir, &name);
        crate::vfs::create_dir_all(self.abs(&rel))?;
        let folder = uuid::Uuid::new_v4().to_string();
        // The folder sorts where the document did: its place by hand, else its last change (`Meta::place`).
        self.index.insert(folder.clone(), Meta { folder: Some(String::new()), rel: rel.clone(), created: m.created, modified: m.modified, order: m.order, top_order: m.top_order, ..Default::default() });
        derive_tree(&mut self.index);
        let to = join_rel(&rel, &format!("{name}.{}", ext_of(&m)));
        if let Err(e) = self.relocate(&[(id.to_string(), to)]) {
            self.index.remove(&folder);
            derive_tree(&mut self.index);
            let _ = crate::vfs::remove_dir(self.abs(&rel));
            return Err(e);
        }
        if let Some(d) = self.index.get_mut(id) { d.order = 1; d.top_order = None; }
        self.save_index()?;
        self.record_tree_changes(&previous);
        Ok(folder)
    }

    /// A 正文's title changed (`base`: the title as a name): the folder is
    /// renamed after it and the file after the folder, so the two keep the
    /// same name. A child page already holding that name inside makes way.
    pub(super) fn follow_note_title(&mut self, id: &str, folder: &str, base: &str) -> Result<()> {
        let frel = self.index[folder].rel.clone();
        let dir = parent_rel(&frel).to_string();
        // Its own name counts as free (`free_name`'s `own`), so 「方案 (2)」 renamed back to 「方案」 goes back when that is free.
        let name = self.free_name(&dir, base, None, Some(folder));
        let ext = ext_of(&self.index[id]).to_string();
        let want = format!("{name}.{ext}");
        if name == base_name(&frel) && base_name(&self.index[id].rel) == want { return Ok(()); }
        let inside = join_rel(&frel, &want).to_lowercase();
        let clash = self.index.iter().find(|(k, m)| k.as_str() != id && m.rel.to_lowercase() == inside).map(|(k, m)| (k.clone(), m.folder.is_none().then(|| ext_of(m).to_string())));
        if let Some((other, other_ext)) = clash {
            let to = join_rel(&frel, &self.free_name(&frel, &name, other_ext.as_deref(), None));
            self.relocate(&[(other, to)])?;
        }
        if name != base_name(&frel) { self.relocate(&[(folder.to_string(), join_rel(&dir, &name))])?; }
        let to = join_rel(&self.index[folder].rel, &want);
        if self.index[id].rel != to { self.relocate(&[(id.to_string(), to)])?; }
        Ok(())
    }
}
