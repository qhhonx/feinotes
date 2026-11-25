//! 主页: the top-level folders the sidebar lists, all alike. Each is a library
//! of its own (`Store`, its `.quiet/` holding its index, settings, 回收站 and
//! history); which folders are open is this computer's, in `folders.json`.
//! See docs/design/workspaces.md.

use crate::store::{Entry, Store};
use crate::vfs::PathExt as _;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// One line of `folders.json`. The key names what this computer keeps about
/// the top-level folder (its caches, its lock); it stays when the folder is renamed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TopFolderEntry {
    pub key: String,
    pub path: PathBuf,
    /// A macOS / iOS bookmark that finds the folder after a move (not yet written).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bookmark: Option<String>,
    #[serde(default)]
    pub added: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct FolderList {
    v: u32,
    /// `roots` in builds before the rename (`roots.json`).
    #[serde(default, alias = "roots")]
    folders: Vec<TopFolderEntry>,
}

/// An open top-level folder: its library, or why it could not be opened (an unplugged
/// disk, a folder renamed in the Finder, a newer format).
pub struct TopFolder {
    pub key: String,
    pub path: PathBuf,
    pub store: Option<Store>,
    pub error: Option<String>,
}

impl TopFolder {
    /// What the sidebar calls it: the folder's own name.
    pub fn name(&self) -> String {
        self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| self.path.to_string_lossy().into_owned())
    }
}

/// Why a folder could not be added.
#[derive(Debug, PartialEq)]
pub enum AddError {
    /// Already open as this top-level folder.
    Open(String),
    /// Inside this open top-level folder: it is listed there already.
    Inside(String),
    /// Holds this open top-level folder: two indexes would keep the same files.
    Holds(String),
    Other(String),
}

impl std::fmt::Display for AddError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { AddError::Open(_) => write!(f, "这个文件夹已经打开"), AddError::Inside(_) => write!(f, "这个文件夹已经在打开的文件夹里"), AddError::Holds(_) => write!(f, "这个文件夹里有已经打开的文件夹"), AddError::Other(e) => write!(f, "{e}") }
    }
}

/// Why a top-level folder could not be renamed; the folder keeps its name.
#[derive(Debug, PartialEq)]
pub enum RenameError {
    /// Something else in the same folder has this name.
    Taken(String),
    Empty,
    /// No permission, or a read-only disk.
    Denied,
    /// Not open (an unplugged disk, a folder gone missing).
    Unavailable,
    /// A folder the system names, or a test run's fixed top-level folder.
    Fixed,
    Other(String),
}

#[derive(Default)]
pub struct Home {
    pub folders: Vec<TopFolder>,
    /// Which top-level folder each id is in, as of the last `list`.
    owner: HashMap<String, String>,
    /// `QUIET_MD_HOME`: one top-level folder, nothing remembered.
    fixed: bool,
}

/// A canonical path in its ordinary form: Windows' are `\\?\C:\…`.
pub fn plain(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|p| p.strip_prefix(r"\\?\")).filter(|p| !p.starts_with("UNC\\")) { Some(p) => PathBuf::from(p), None => path }
}

fn canonical(p: &Path) -> PathBuf { plain(p.fs_canonicalize().unwrap_or_else(|_| p.to_path_buf())) }

use crate::time::now;

impl Home {
    /// `folders.json`: beside the other things this computer keeps, or where
    /// `QUIET_MD_FOLDERS` says (test runs with several top-level folders, which must not
    /// touch this computer's list).
    pub fn list_file() -> PathBuf {
        if let Some(f) = std::env::var_os("QUIET_MD_FOLDERS") { return PathBuf::from(f); }
        dirs::data_local_dir().unwrap_or_else(crate::vfs::temp_dir).join("quiet-md").join("folders.json")
    }

    /// Where builds before the rename kept the list (`roots.json`): read once
    /// when `folders.json` is not there yet, and left in place (older builds
    /// read it; it is also the way back). Drop the reading after 0.1.16.
    fn old_list_file() -> Option<PathBuf> {
        if std::env::var_os("QUIET_MD_FOLDERS").is_some() { return None; }
        Some(dirs::data_local_dir().unwrap_or_else(crate::vfs::temp_dir).join("quiet-md").join("roots.json"))
    }

    /// The list in `path`, if it is there and reads.
    fn read_list(path: &Path) -> Option<Vec<TopFolderEntry>> { serde_json::from_slice::<FolderList>(&crate::vfs::read(path).ok()?).ok().map(|f| f.folders) }

    /// The top-level folders as last left; the first run with top-level folders takes the library
    /// builds before it used, and the folders it had opened in place. With
    /// `QUIET_MD_HOME`, that folder alone (test runs touch nothing else).
    pub fn entries() -> (Vec<TopFolderEntry>, bool) {
        // Several top-level folders in a test run: the list in its own file, `QUIET_MD_HOME` first when it is new.
        if std::env::var_os("QUIET_MD_FOLDERS").is_some() {
            if let Some(v) = Self::read_list(&Self::list_file()) { return (v, false); }
            let home = std::env::var_os("QUIET_MD_HOME").map(|h| TopFolderEntry { key: new_key(), path: PathBuf::from(h), bookmark: None, added: now() });
            return (home.into_iter().collect(), false);
        }
        if let Some(home) = std::env::var_os("QUIET_MD_HOME") {
            // Named by its path, so two test runs never share what this computer keeps.
            let path = PathBuf::from(home);
            return (vec![TopFolderEntry { key: crate::store::library_key(&path), path, bookmark: None, added: 0 }], true);
        }
        if cfg!(target_family = "wasm") {
            let path = Store::default_dir();
            return (vec![TopFolderEntry { key: crate::store::library_key(&path), path, bookmark: None, added: 0 }], true);
        }
        if let Some(v) = Self::read_list(&Self::list_file()) { return (v, false); }
        if let Some(v) = Self::old_list_file().and_then(|p| Self::read_list(&p)) { return (v, false); }
        (Self::from_legacy(), false)
    }

    /// The library and opened folders builds before top-level folders kept.
    fn from_legacy() -> Vec<TopFolderEntry> {
        let lib = Store::legacy_dir();
        let mut out = vec![];
        if Store::is_library(&lib) {
            out.push(TopFolderEntry { key: new_key(), path: lib.clone(), bookmark: None, added: now() });
            for dir in Store::legacy_mounts(&lib) {
                if dir.fs_is_dir() && !out.iter().any(|e: &TopFolderEntry| e.path == dir) { out.push(TopFolderEntry { key: new_key(), path: dir, bookmark: None, added: now() }); }
            }
        }
        out
    }

    /// Open every top-level folder in `entries`. The library builds before top-level folders used
    /// still moves to the new layout without asking when it may (`Store::open`).
    pub fn open(entries: Vec<TopFolderEntry>, fixed: bool) -> Self {
        let legacy = Store::legacy_dir();
        Self::open_with(entries, fixed, |e| if e.path == legacy && e.path.fs_is_dir() { Store::open_library_as(e.path.clone(), &e.key) } else { Store::open_top_folder_as(&e.path, &e.key) })
    }

    /// `open`, each top-level folder's library opened by `open`: tools beside the app
    /// (the `quiet` CLI) open them as they are and leave crash copies alone.
    pub fn open_with(entries: Vec<TopFolderEntry>, fixed: bool, mut open: impl FnMut(&TopFolderEntry) -> Result<Store>) -> Self {
        let mut ws = Home { fixed, ..Default::default() };
        for e in entries {
            let store = open(&e);
            ws.push(e.key, e.path, store);
        }
        ws
    }

    /// The top-level folders as last left, opened.
    pub fn load() -> Self {
        let (entries, fixed) = Self::entries();
        let first = !fixed && !Self::list_file().fs_exists();
        let ws = Self::open(entries, fixed);
        // The first run here, or the first after the rename: the list is written under its name now.
        // The old file stays as it was: a way back, and what older builds read.
        if first && (!ws.folders.is_empty() || Self::old_list_file().is_some_and(|p| p.fs_exists())) { let _ = ws.save(); }
        ws
    }

    fn push(&mut self, key: String, path: PathBuf, store: Result<Store>) {
        let (store, error) = match store {
            Ok(mut s) => {
                let open: Vec<&Store> = self.folders.iter().filter_map(|r| r.store.as_ref()).collect();
                // A folder copied whole, `.quiet/` and all: a library of its own from now on.
                let lib = s.library_id();
                if !lib.is_empty() && open.iter().any(|o| o.library_id() == lib) && let Err(e) = s.renew_library_id() { s.warnings.push(format!("{e}")); }
                let taken: HashSet<String> = open.iter().flat_map(|s| s.ids().cloned().collect::<Vec<_>>()).collect();
                if let Err(e) = s.rekey(&taken) { s.warnings.push(format!("{e}")); }
                (Some(s), None)
            }
            Err(e) => (None, Some(e.to_string())),
        };
        self.folders.push(TopFolder { key, path, store, error });
    }

    pub fn save(&self) -> Result<()> {
        if self.fixed { return Ok(()); }
        let f = FolderList { v: 1, folders: self.folders.iter().map(|r| TopFolderEntry { key: r.key.clone(), path: r.path.clone(), bookmark: None, added: 0 }).collect() };
        let path = Self::list_file();
        crate::vfs::create_dir_all(path.parent().unwrap())?;
        crate::store::write_atomic(&path, &serde_json::to_vec_pretty(&f)?)?;
        // Older builds look for one library: the first that opens.
        if let Some(r) = self.folders.iter().find(|r| r.store.is_some()) { let _ = Store::remember_dir(r.path.clone()); }
        Ok(())
    }

    /// Where `dir` stands among the open top-level folders.
    pub fn check(&self, dir: &Path) -> std::result::Result<PathBuf, AddError> {
        let dir = canonical(dir);
        for r in &self.folders {
            let p = canonical(&r.path);
            if p == dir { return Err(AddError::Open(r.key.clone())); }
            if dir.starts_with(&p) { return Err(AddError::Inside(r.key.clone())); }
            if p.starts_with(&dir) { return Err(AddError::Holds(r.key.clone())); }
        }
        Ok(dir)
    }

    /// Open folder `dir` as a new top-level folder at the end; its key.
    pub fn add(&mut self, dir: &Path) -> std::result::Result<String, AddError> {
        let dir = self.check(dir)?;
        let key = new_key();
        let store = Store::open_top_folder_as(&dir, &key).map_err(|e| AddError::Other(e.to_string()))?;
        self.push(key.clone(), dir, Ok(store));
        self.save().map_err(|e| AddError::Other(e.to_string()))?;
        Ok(key)
    }

    /// Make folder `dir` (it must not exist yet, or be empty) and open it.
    pub fn create(&mut self, dir: &Path) -> std::result::Result<String, AddError> {
        crate::vfs::create_dir_all(dir).map_err(|e| AddError::Other(e.to_string()))?;
        self.add(dir)
    }

    /// Off the sidebar; the folder and its `.quiet/` stay as they are.
    pub fn remove(&mut self, key: &str) -> Result<()> {
        self.folders.retain(|r| r.key != key);
        self.owner.retain(|_, k| k != key);
        self.save()
    }

    /// Top-level folder `key`'s folder is gone from the disk: its library closes, so nothing is
    /// written there again (writing would make the folder anew, empty); it stays on the list,
    /// unopened, until it is back (`retry`) or removed.
    pub fn lose(&mut self, key: &str, why: String) {
        if let Some(r) = self.folders.iter_mut().find(|r| r.key == key) { r.store = None; r.error = Some(why); }
    }

    /// Try a top-level folder that could not be opened again (the disk is back).
    pub fn retry(&mut self, key: &str) {
        let Some(r) = self.folders.iter_mut().find(|r| r.key == key && r.store.is_none()) else { return };
        match Store::open_top_folder_as(&r.path, &r.key) { Ok(s) => { r.store = Some(s); r.error = None; } Err(e) => r.error = Some(e.to_string()) }
    }

    /// 重命名 (2.11): top-level folder `key`'s folder takes `name` on disk, in the same
    /// folder as before. The key stays, so the ids the windows hold and what
    /// this computer keeps about the top-level folder stay valid. The new path.
    pub fn rename(&mut self, key: &str, name: &str) -> std::result::Result<PathBuf, RenameError> {
        if self.folder(key).is_none_or(|r| r.store.is_none()) { return Err(RenameError::Unavailable); }
        if !self.can_rename(key) { return Err(RenameError::Fixed); }
        let r = self.folders.iter_mut().find(|r| r.key == key).ok_or(RenameError::Unavailable)?;
        let store = r.store.as_mut().ok_or(RenameError::Unavailable)?;
        if name.trim().is_empty() { return Err(RenameError::Empty); }
        let name = crate::store::file_name_for(name.trim());
        let to = r.path.with_file_name(&name);
        if to == r.path { return Ok(to); }
        // Taken: anything there but the folder itself (a change of case on a disk that ignores case).
        let same = |a: &Path, b: &Path| a.file_name().map(|n| n.to_string_lossy().to_lowercase()) == b.file_name().map(|n| n.to_string_lossy().to_lowercase()) && canonical(a) == canonical(b);
        if crate::vfs::symlink_metadata(&to).is_ok() && !same(&r.path, &to) { return Err(RenameError::Taken(name)); }
        store.flush_view_state().map_err(|e| RenameError::Other(e.to_string()))?;
        crate::vfs::rename(&r.path, &to).map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem => RenameError::Denied,
            std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::DirectoryNotEmpty => RenameError::Taken(name.clone()),
            _ => RenameError::Other(e.to_string()),
        })?;
        store.moved_to(to.clone());
        r.path = to.clone();
        let _ = self.save();
        Ok(to)
    }

    /// Whether top-level folder `key` may be renamed from the sidebar: open, not a test
    /// run's one top-level folder (`QUIET_MD_HOME`), and not a folder the system names
    /// (home, Desktop, Documents, Downloads …).
    pub fn can_rename(&self, key: &str) -> bool {
        // A test run's one top-level folder stays where the run put it (a scripted
        // launch's environment: unit tests are not that run, whatever their shell sets).
        let pinned = cfg!(target_family = "wasm") || (!cfg!(test) && std::env::var_os("QUIET_MD_HOME").is_some() && std::env::var_os("QUIET_MD_FOLDERS").is_none());
        let Some(r) = self.folder(key).filter(|r| r.store.is_some() && !pinned) else { return false };
        let system = [dirs::home_dir(), dirs::desktop_dir(), dirs::document_dir(), dirs::download_dir(), dirs::picture_dir(), dirs::audio_dir(), dirs::video_dir(), dirs::public_dir()];
        r.path.parent().is_some() && !system.into_iter().flatten().any(|d| canonical(&d) == canonical(&r.path))
    }

    pub fn folder(&self, key: &str) -> Option<&TopFolder> { self.folders.iter().find(|r| r.key == key) }

    /// The first top-level folder that opened: where things go when nothing says otherwise.
    pub fn first(&self) -> Option<&TopFolder> { self.folders.iter().find(|r| r.store.is_some()) }

    /// Where a 快速笔记 goes (2.8): top-level folder `chosen` (Settings › 文档库) while it
    /// is open, else the first that is; `None` with no top-level folder open.
    pub fn quick_note_folder(&self, chosen: &str) -> Option<&TopFolder> {
        self.folders.iter().find(|r| r.key == chosen && r.store.is_some()).or_else(|| self.first())
    }

    /// The key of the top-level folder `id` is in.
    pub fn owner(&mut self, id: &str) -> Option<String> {
        if let Some(k) = self.owner.get(id) { return Some(k.clone()); }
        let k = self.folders.iter().find(|r| r.store.as_ref().is_some_and(|s| s.has(id)))?.key.clone();
        self.owner.insert(id.to_string(), k.clone());
        Some(k)
    }

    /// The library `id` is in.
    pub fn store_of(&mut self, id: &str) -> Option<&mut Store> {
        let key = self.owner(id)?;
        self.folders.iter_mut().find(|r| r.key == key)?.store.as_mut()
    }

    pub fn store(&mut self, key: &str) -> Option<&mut Store> { self.folders.iter_mut().find(|r| r.key == key)?.store.as_mut() }

    /// The top-level folder holding `path`, if one does.
    pub fn folder_of_path(&self, path: &Path) -> Option<&TopFolder> {
        let path = canonical(path);
        self.folders.iter().find(|r| path.starts_with(canonical(&r.path)))
    }

    /// Every top-level folder's entries, each marked with its top-level folder.
    pub fn list(&mut self) -> Vec<Entry> { self.list_with(true) }

    /// `list`; with `parse` off, documents not read yet go by their file's name (`Store::list_with`).
    pub fn list_with(&mut self, parse: bool) -> Vec<Entry> {
        let mut out = vec![];
        self.owner.clear();
        for r in &mut self.folders {
            let Some(s) = r.store.as_mut() else { continue };
            for mut e in s.list_with(None, parse) {
                self.owner.insert(e.id.clone(), r.key.clone());
                e.top = r.key.clone();
                out.push(e);
            }
        }
        out
    }

    /// Problems the top-level folders met while opening or listing, each once.
    pub fn take_warnings(&mut self) -> Vec<String> {
        self.folders.iter_mut().filter_map(|r| r.store.as_mut()).flat_map(|s| std::mem::take(&mut s.warnings)).collect()
    }
}

fn new_key() -> String { uuid::Uuid::new_v4().to_string() }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Document, RichText};

    fn dir(name: &str) -> PathBuf {
        let d = crate::vfs::temp_dir().join(format!("quiet-ws-{name}-{}", uuid::Uuid::new_v4()));
        crate::vfs::create_dir_all(&d).unwrap();
        d
    }

    fn doc(t: &str) -> Document { Document { title: RichText::plain(t), ..Default::default() } }

    #[test]
    fn folders_are_libraries_of_their_own() {
        let (a, b) = (dir("a"), dir("b"));
        crate::vfs::write(b.join("note.md"), "# note\n").unwrap();
        let mut ws = Home { fixed: true, ..Default::default() };
        let ka = ws.add(&a).unwrap();
        let kb = ws.add(&b).unwrap();
        assert!(a.join(".quiet/format.json").fs_is_file() && b.join(".quiet/format.json").fs_is_file(), "every top-level folder has its .quiet/");
        let made = ws.store(&ka).unwrap().create_from(&doc("in a")).unwrap();
        let list = ws.list();
        assert_eq!(list.iter().find(|e| e.id == made).unwrap().top, ka);
        assert_eq!(list.iter().find(|e| e.title == "note").unwrap().top, kb);
        assert_eq!(ws.owner(&made).as_deref(), Some(ka.as_str()));
        // Nesting: the same folder, one inside, one around.
        assert_eq!(ws.add(&a), Err(AddError::Open(ka.clone())));
        crate::vfs::create_dir_all(a.join("sub")).unwrap();
        assert_eq!(ws.add(&a.join("sub")), Err(AddError::Inside(ka.clone())));
        assert!(matches!(ws.add(a.parent().unwrap()), Err(AddError::Holds(_))));
        // Removed: the folder stays, and comes back as it was.
        ws.remove(&ka).unwrap();
        assert!(a.join("in a.xml").fs_is_file() || a.join("in a.md").fs_is_file());
        let back = ws.add(&a).unwrap();
        assert!(ws.list().iter().any(|e| e.id == made && e.top == back), "same id after reopening");
    }

    #[test]
    fn a_folder_gone_from_the_disk_is_never_written_again() {
        let (a, b) = (dir("gone"), dir("stays"));
        let mut ws = Home { fixed: true, ..Default::default() };
        let ka = ws.add(&a).unwrap();
        let kb = ws.add(&b).unwrap();
        let made = ws.store(&ka).unwrap().create_from(&doc("in a")).unwrap();
        // Moved away while open (in the Finder): the window finds it gone and closes its library.
        let moved = a.with_file_name(format!("{}-moved", a.file_name().unwrap().to_string_lossy()));
        crate::vfs::rename(&a, &moved).unwrap();
        ws.lose(&ka, "gone".into());
        assert!(ws.store(&ka).is_none(), "no library left to write with");
        // Whatever the app does meanwhile, nothing appears at the old path, not even an empty .quiet/.
        let _ = ws.list();
        ws.save().unwrap();
        ws.store(&kb).unwrap().create_from(&doc("in b")).unwrap();
        ws.retry(&ka);
        assert!(ws.store(&ka).is_none() && !a.fs_exists(), "the folder is not made anew at its old path");
        assert!(!ws.list().iter().any(|e| e.top == ka), "its documents are not listed");
        // Back where it was: it opens again, as it was.
        crate::vfs::rename(&moved, &a).unwrap();
        ws.retry(&ka);
        assert!(ws.store(&ka).is_some());
        assert!(ws.list().iter().any(|e| e.id == made && e.top == ka));
    }

    #[test]
    fn a_copied_folder_gets_ids_of_its_own() {
        let a = dir("orig");
        let mut s = Store::open_top_folder(&a).unwrap();
        s.create_from(&doc("one")).unwrap();
        s.list();
        drop(s);
        let b = dir("copy");
        crate::vfs::copy(a.join("one.xml"), b.join("one.xml")).unwrap();
        let own = format!("meta/{}", crate::store::device().id);
        crate::vfs::create_dir_all(b.join(".quiet").join(&own)).unwrap();
        crate::vfs::copy(a.join(".quiet/format.json"), b.join(".quiet/format.json")).unwrap();
        for e in crate::vfs::read_dir(a.join(".quiet").join(&own)).unwrap().flatten() { crate::vfs::copy(e.path(), b.join(".quiet").join(&own).join(e.file_name())).unwrap(); }
        let mut ws = Home { fixed: true, ..Default::default() };
        ws.add(&a).unwrap();
        ws.add(&b).unwrap();
        let list = ws.list();
        let ids: HashSet<&String> = list.iter().map(|e| &e.id).collect();
        assert_eq!(ids.len(), list.len(), "no id in two top-level folders");
        let (la, lb) = (ws.folders[0].store.as_ref().unwrap().library_id(), ws.folders[1].store.as_ref().unwrap().library_id());
        assert!(!la.is_empty() && la != lb, "the copy is a library of its own");
    }

    #[test]
    fn quick_notes_go_to_the_chosen_folder_while_it_is_open() {
        let (a, b) = (dir("qa"), dir("qb"));
        let mut ws = Home { fixed: true, ..Default::default() };
        let ka = ws.add(&a).unwrap();
        let kb = ws.add(&b).unwrap();
        let at = |ws: &Home, chosen: &str| ws.quick_note_folder(chosen).map(|r| r.key.clone());
        assert_eq!(at(&ws, ""), Some(ka.clone()), "nothing chosen: the first top-level folder");
        assert_eq!(at(&ws, &kb), Some(kb.clone()));
        assert_eq!(at(&ws, "gone"), Some(ka.clone()), "a top-level folder no longer listed: the first");
        // The chosen top-level folder can't be opened (an unplugged disk): the first that can.
        ws.folders[1].store = None;
        assert_eq!(at(&ws, &kb), Some(ka.clone()));
        // The first can't either: the next one that can.
        ws.folders[1].store = Some(Store::open_top_folder(&b).unwrap());
        ws.folders[0].store = None;
        assert_eq!(at(&ws, &ka), Some(kb.clone()));
        ws.folders[1].store = None;
        assert_eq!(at(&ws, &kb), None, "none open: the empty state");
        assert!(Home::default().quick_note_folder("").is_none());
    }

    #[test]
    fn a_renamed_folder_keeps_its_key_ids_and_data() {
        // In a folder of their own: the new names are looked for beside them.
        let (a, b) = (dir("ra").join("a"), dir("rb"));
        crate::vfs::create_dir_all(&a).unwrap();
        let mut ws = Home { fixed: true, ..Default::default() };
        let ka = ws.add(&a).unwrap();
        ws.add(&b).unwrap();
        let id = ws.store(&ka).unwrap().create_from(&doc("kept")).unwrap();
        ws.store(&ka).unwrap().touch(&id);
        ws.list();
        let to = ws.rename(&ka, " renamed/root ").unwrap();
        assert_eq!(to.file_name().unwrap().to_str(), Some(crate::store::file_name_for("renamed/root").as_str()), "the same name rules as a document's file");
        assert_ne!(to.file_name().unwrap(), "renamed/root");
        assert!(!a.fs_exists() && to.join(".quiet/format.json").fs_is_file(), "the folder itself moved");
        assert_eq!(ws.folder(&ka).unwrap().path, to);
        assert_eq!(ws.store(&ka).unwrap().top_dir(), to.as_path());
        let list = ws.list();
        let e = list.iter().find(|e| e.id == id).expect("same id after the rename");
        assert_eq!((e.top.as_str(), e.title.as_str()), (ka.as_str(), "kept"));
        assert!(e.meta.opened > 0, "this computer's 最近打开 still found by the key");
        assert!(ws.store_of(&id).unwrap().load(&id).is_ok());
        // Taken by a sibling, empty, or not open: the name stays.
        crate::vfs::create_dir_all(to.with_file_name("other")).unwrap();
        assert_eq!(ws.rename(&ka, "other"), Err(RenameError::Taken("other".into())));
        assert_eq!(ws.rename(&ka, "  "), Err(RenameError::Empty));
        assert_eq!(ws.rename("nope", "x"), Err(RenameError::Unavailable));
        assert_eq!(ws.folder(&ka).unwrap().path, to);
        // A change of case only is a rename of the same folder.
        let upper = ws.rename(&ka, &to.file_name().unwrap().to_string_lossy().to_uppercase()).unwrap();
        assert!(upper != to && upper.fs_is_dir() && ws.folder(&ka).unwrap().path == upper);
    }

    #[test]
    fn data_kept_by_path_moves_to_the_key_once() {
        let a = dir("adopt");
        let mut s = Store::open_top_folder(&a).unwrap();
        let id = s.create_from(&doc("one")).unwrap();
        s.touch(&id);
        s.list();
        s.recover_later(&doc("unsaved")).unwrap();
        drop(s);
        let key = new_key();
        let mut ws = Home::open(vec![TopFolderEntry { key: key.clone(), path: a.clone(), bookmark: None, added: 0 }], true);
        let s = ws.store(&key).unwrap();
        assert_eq!(s.key(), key);
        let list = s.list();
        assert!(list.iter().any(|e| e.title == "unsaved (recovered copy)"), "the recovery copy kept by path comes back");
        assert!(list.iter().find(|e| e.id == id).unwrap().meta.opened > 0, "最近打开 kept by path is found");
        drop(ws);
        // Taken over once: the next launch neither restores nor loses anything.
        let mut ws = Home::open(vec![TopFolderEntry { key: key.clone(), path: a.clone(), bookmark: None, added: 0 }], true);
        assert_eq!(ws.list().len(), 2);
        if let Some(d) = dirs::data_local_dir() { let _ = crate::vfs::remove_dir_all(d.join("quiet-md/recovery").join(&key)); }
    }

    /// A list written before the rename (`roots.json`, its `roots`) still reads.
    #[test]
    fn the_list_from_before_the_rename_reads() {
        let d = dir("old-list");
        let old = d.join("roots.json");
        crate::vfs::write(&old, r#"{"v":1,"roots":[{"key":"k1","path":"/tmp/a"}]}"#).unwrap();
        let v = Home::read_list(&old).unwrap();
        assert_eq!((v.len(), v[0].key.as_str()), (1, "k1"));
    }

    #[test]
    fn a_missing_folder_is_listed_not_fatal() {
        let a = dir("gone");
        let entries = vec![TopFolderEntry { key: "k".into(), path: a.join("not-here"), bookmark: None, added: 0 }];
        let mut ws = Home::open(entries, true);
        assert!(ws.folders[0].store.is_none() && ws.folders[0].error.is_some());
        assert!(!a.join("not-here").fs_exists(), "never made anew");
        assert!(ws.list().is_empty());
    }
}
