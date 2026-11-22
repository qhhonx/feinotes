//! 本机文件夹: folders opened where they are (打开…, the Finder's Open With),
//! listed under 本机文件 with what is in them. Only the folder is remembered,
//! as this computer's state; what is inside is read from disk on every
//! `list`, each entry's id following from its path, so a note keeps its
//! last-opened time across launches. New notes and folders, renames and moves
//! happen on disk right away; deleting is the app's (the system's trash).

use super::*;

/// The id of what sits at `path` in an opened folder: its path, hashed, in
/// the shape of the library's ids.
fn path_id(path: &Path) -> String {
    let h = sha1_smol::Sha1::from(path.to_string_lossy().as_bytes()).digest().to_string();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// A canonical path in its ordinary form: Windows' are `\\?\C:\…`, which
/// the Explorer and the title don't show.
pub(super) fn plain(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|p| p.strip_prefix(r"\\?\")).filter(|p| !p.starts_with("UNC\\")) { Some(p) => PathBuf::from(p), None => path }
}

fn secs(t: std::io::Result<std::time::SystemTime>) -> Option<u64> { t.ok()?.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs()) }

impl Meta {
    /// In a folder opened from this computer, or such a folder itself.
    pub fn mounted(&self) -> bool { self.path.is_some() && (self.folder.is_some() || self.parent.is_some()) }
}

impl Store {
    /// The folders opened from this computer: (id, folder).
    pub(super) fn mount_roots(&self) -> Vec<(String, PathBuf)> {
        let mut v: Vec<(String, PathBuf)> = self.index.iter().filter(|(_, m)| m.mounted() && m.parent.is_none())
            .map(|(k, m)| (k.clone(), PathBuf::from(m.path.as_deref().unwrap()))).collect();
        v.sort();
        v
    }

    /// The folders opened from this computer, for watching.
    pub fn mount_dirs(&self) -> Vec<PathBuf> { self.mount_roots().into_iter().map(|(_, p)| p).collect() }

    /// The shape of folders `dirs`: what `list` would find in them, hashed.
    pub fn shape_of(dirs: &[PathBuf]) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for d in dirs { let f = layout::scan(d); (d, f.dirs, f.docs).hash(&mut h); }
        h.finish()
    }

    /// What is listed at `path` (an opened folder, or a file or folder in one).
    pub(super) fn mounted_at(&self, path: &Path) -> Option<String> {
        self.index.iter().find(|(_, m)| m.mounted() && m.path.as_deref().is_some_and(|p| Path::new(p) == path)).map(|(k, _)| k.clone())
    }

    /// Whether `id` is in an opened folder (or is one): moves, new notes and
    /// deletes there are the folder's, not the library's.
    pub fn is_mounted(&self, id: &str) -> bool { self.index.get(id).is_some_and(Meta::mounted) }

    /// The opened folder `id` is in (itself for the folder).
    pub fn mount_of(&self, id: &str) -> Option<String> {
        let mut cur = id;
        for _ in 0..256 {
            let m = self.index.get(cur).filter(|m| m.mounted())?;
            match &m.parent { Some(p) => cur = p, None => return Some(cur.to_string()) }
        }
        None
    }

    /// 打开… a folder: list it under 本机文件, edited where it is. A folder
    /// already open, or inside one that is, is that entry; opening a folder
    /// around ones open before takes them in.
    pub fn mount(&mut self, dir: &Path) -> Result<String> {
        #[cfg(feature = "sqlite-store")]
        anyhow::ensure!(self.sqlite.is_none(), "本机文件夹只支持文件文档库");
        let dir = plain(dir.fs_canonicalize()?);
        anyhow::ensure!(dir.fs_is_dir(), "文件夹不存在");
        let root = plain(self.root.fs_canonicalize().unwrap_or_else(|_| self.root.clone()));
        anyhow::ensure!(!dir.starts_with(&root), "这个文件夹在文档库里，它的内容已经在我的文档中");
        anyhow::ensure!(!root.starts_with(&dir), "这个文件夹里有文档库，不能作为本机文件夹打开");
        let _lock = self.begin_write()?;
        self.refresh_mounts();
        let t = now();
        if let Some(id) = self.mounted_at(&dir) {
            self.index.get_mut(&id).unwrap().opened = t;
            self.save_local(&self.local_state())?;
            return Ok(id);
        }
        // Single files and folders opened before from inside it now come with it.
        self.index.retain(|_, m| !(m.local() && m.parent.is_none() && m.path.as_deref().is_some_and(|p| Path::new(p).starts_with(&dir))));
        let id = path_id(&dir);
        let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| dir.to_string_lossy().into_owned());
        self.index.insert(id.clone(), Meta { folder: Some(name), path: Some(dir.to_string_lossy().into_owned()), created: t, modified: t, opened: t, ..Default::default() });
        self.refresh_mounts();
        if let Err(e) = self.save_local(&self.local_state()) { self.index.remove(&id); return Err(e); }
        Ok(id)
    }

    /// Re-read every opened folder: what was added in the Finder shows, what
    /// is gone goes. Entries keep their ids while their paths stay (and
    /// across renames made here); a folder that can't be read right now (an
    /// unplugged disk) lists nothing until it can.
    pub(super) fn refresh_mounts(&mut self) {
        let roots = self.mount_roots();
        let mut known: HashMap<PathBuf, String> = self.index.iter().filter(|(_, m)| m.mounted() && m.parent.is_some())
            .map(|(k, m)| (PathBuf::from(m.path.as_deref().unwrap()), k.clone())).collect();
        let mut keep: HashSet<String> = HashSet::new();
        let mut opened: Option<BTreeMap<String, u64>> = None;
        let lib = self.root.clone();
        for (rid, dir) in roots {
            if !dir.fs_is_dir() { continue; }
            let found = layout::scan(&dir);
            if found.partial { keep.extend(known.iter().filter(|(p, _)| p.starts_with(&dir)).map(|(_, k)| k.clone())); }
            let mut by_rel: HashMap<&str, String> = HashMap::new();
            // Folders come before what is in them (the scan walks top down).
            let items = found.dirs.iter().map(|r| (r, true)).chain(found.docs.iter().map(|r| (r, false)));
            for (rel, is_dir) in items {
                let p = rel.split('/').fold(dir.clone(), |p, part| p.join(part));
                let parent = match parent_rel(rel) { "" => rid.clone(), up => by_rel.get(up).cloned().unwrap_or_else(|| rid.clone()) };
                let id = known.remove(&p).unwrap_or_else(|| path_id(&p));
                let md = crate::vfs::metadata(&p).ok();
                let changed = md.as_ref().and_then(|m| secs(m.modified()));
                let name = base_name(rel);
                let m = self.index.entry(id.clone()).or_insert_with(|| {
                    let t = changed.unwrap_or_else(now);
                    let created = md.as_ref().and_then(|m| secs(m.created())).unwrap_or(t);
                    let opened = opened.get_or_insert_with(|| self_read_opened(&lib)).get(&id).copied().unwrap_or(0);
                    Meta { created, modified: t, opened, ..Default::default() }
                });
                m.path = Some(p.to_string_lossy().into_owned());
                m.parent = Some(parent);
                m.rel.clear();
                if is_dir { m.folder = Some(name.to_string()); }
                else {
                    m.folder = None;
                    (m.kind, m.markdown) = DocKind::of_name(name).unwrap_or_default();
                    if let Some(t) = changed { m.modified = m.modified.max(t); }
                }
                by_rel.insert(rel, id.clone());
                keep.insert(id);
            }
        }
        self.index.retain(|k, m| !(m.mounted() && m.parent.is_some()) || keep.contains(k));
    }

    /// The entries of opened folders `before` had, put back after the index
    /// was re-read from disk (which keeps only the folders themselves).
    pub(super) fn carry_mounts(&mut self, before: &HashMap<String, Meta>) {
        for (k, m) in before {
            if m.mounted() && m.parent.is_some() && !self.index.contains_key(k) { self.index.insert(k.clone(), m.clone()); }
        }
        // Children of a folder no longer open go with it.
        let roots: HashSet<String> = self.mount_roots().into_iter().map(|(k, _)| k).collect();
        let orphans: Vec<String> = self.index.keys().filter(|k| self.index[*k].mounted() && self.mount_of(k).is_none_or(|r| !roots.contains(&r))).cloned().collect();
        for k in orphans { self.index.remove(&k); }
    }

    /// A free name in folder `dir` on disk: `base`, then `base 2` …
    fn free_on_disk(dir: &Path, base: &str, ext: Option<&str>) -> PathBuf {
        for n in 1.. {
            let stem = if n == 1 { base.to_string() } else { format!("{base} {n}") };
            let p = dir.join(match ext { Some(e) => format!("{stem}.{e}"), None => stem });
            if p.fs_symlink_metadata().is_err() && layout::placeholder(&p).is_none_or(|ph| !ph.fs_exists()) { return p; }
        }
        unreachable!()
    }

    /// The folder on disk of opened folder (or folder in one) `parent`.
    fn mount_dir(&self, parent: &str) -> Result<PathBuf> {
        let m = self.index.get(parent).filter(|m| m.mounted() && m.folder.is_some()).ok_or_else(|| anyhow::anyhow!("目标文件夹不存在"))?;
        let dir = PathBuf::from(m.path.as_deref().unwrap());
        anyhow::ensure!(dir.fs_is_dir(), "目标文件夹不存在");
        Ok(dir)
    }

    /// The folder on disk of `parent`, where relative image paths start.
    pub(super) fn mount_dir_of(&self, parent: &str) -> Option<PathBuf> { self.mount_dir(parent).ok() }

    /// A new file in opened folder `parent`: `bytes`, named `title` with `ext`.
    pub(super) fn create_in_mount(&mut self, parent: &str, title: &str, ext: &str, bytes: &[u8]) -> Result<String> {
        let _lock = self.begin_write()?;
        let p = Self::free_on_disk(&self.mount_dir(parent)?, &file_name_for(title), Some(ext));
        self.write_file(&p, bytes)?;
        let t = now();
        let id = path_id(&p);
        let (kind, markdown) = DocKind::of_name(&p.file_name().unwrap().to_string_lossy()).unwrap_or_default();
        self.index.insert(id.clone(), Meta { path: Some(p.to_string_lossy().into_owned()), parent: Some(parent.to_string()), kind, markdown, created: t, modified: t, opened: t, ..Default::default() });
        self.save_local(&self.local_state())?;
        Ok(id)
    }

    /// 新建文件夹 in opened folder `parent`.
    pub(super) fn create_folder_in_mount(&mut self, parent: &str, title: &str) -> Result<String> {
        let _lock = self.begin_write()?;
        let p = Self::free_on_disk(&self.mount_dir(parent)?, &file_name_for(title), None);
        crate::vfs::create_dir(&p)?;
        let t = now();
        let id = path_id(&p);
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        self.index.insert(id.clone(), Meta { folder: Some(name), path: Some(p.to_string_lossy().into_owned()), parent: Some(parent.to_string()), created: t, modified: t, ..Default::default() });
        Ok(id)
    }

    /// Move `id` into `parent`, both in opened folders: the file or folder
    /// moves on disk, keeping its name when it is free there.
    pub(super) fn move_in_mount(&mut self, id: &str, parent: &str) -> Result<()> {
        let _lock = self.begin_write()?;
        let m = self.index.get(id).filter(|m| m.mounted() && m.parent.is_some()).ok_or_else(|| anyhow::anyhow!("文档或文件夹不存在"))?;
        if m.parent.as_deref() == Some(parent) { return Ok(()); }
        let from = PathBuf::from(m.path.as_deref().unwrap());
        let dir = self.mount_dir(parent)?;
        anyhow::ensure!(!dir.starts_with(&from), "不能移动到自身或后代文件夹中");
        let name = from.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let (stem, ext) = match name.rsplit_once('.') { Some((s, e)) if m.folder.is_none() && !s.is_empty() => (s.to_string(), Some(e.to_string())), _ => (name.clone(), None) };
        let to = if dir.join(&name).fs_symlink_metadata().is_err() { dir.join(&name) } else { Self::free_on_disk(&dir, layout::unnumbered(&stem), ext.as_deref()) };
        crate::vfs::rename(&from, &to)?;
        self.repath(&from, &to);
        self.index.get_mut(id).unwrap().parent = Some(parent.to_string());
        Ok(())
    }

    /// What was at `from` (and under it) is at `to` now.
    pub(super) fn repath(&mut self, from: &Path, to: &Path) {
        for m in self.index.values_mut() {
            let Some(p) = m.path.as_deref().map(PathBuf::from) else { continue };
            if let Ok(rest) = p.strip_prefix(from) {
                let new = if rest.as_os_str().is_empty() { to.to_path_buf() } else { to.join(rest) };
                m.path = Some(new.to_string_lossy().into_owned());
                if m.folder.is_some() && rest.as_os_str().is_empty() { m.folder = new.file_name().map(|n| n.to_string_lossy().into_owned()); }
            }
        }
        let moved: Vec<String> = self.loaded.borrow().keys().cloned().collect();
        for id in moved { if self.index.get(&id).and_then(|m| m.path.as_deref()).is_some_and(|p| Path::new(p).starts_with(to)) { self.loaded.borrow_mut().remove(&id); } }
    }

    /// After the system's trash took `id`'s file or folder: off the list.
    pub fn forget_mounted(&mut self, id: &str) {
        let Some(p) = self.index.get(id).and_then(|m| m.path.clone()).map(PathBuf::from) else { return };
        self.index.retain(|_, m| !(m.mounted() && m.parent.is_some() && m.path.as_deref().is_some_and(|q| Path::new(q).starts_with(&p))));
        self.loaded.borrow_mut().remove(id);
    }
}

/// This computer's last-opened times, for entries met for the first time.
fn self_read_opened(root: &Path) -> BTreeMap<String, u64> {
    reread::<Local>(&local_file_for(root)).map(|l| l.opened).unwrap_or_default()
}
