//! 工作区: the root folders the sidebar lists, all alike. Each is a library
//! of its own (`Store`, its `.quiet/` holding its index, settings, 回收站 and
//! history); which folders are open is this computer's, in `roots.json`.
//! See docs/design/workspaces.md.

use crate::store::{Entry, Store};
use crate::vfs::PathExt as _;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// One line of `roots.json`. The key names what this computer keeps about
/// the root (its caches, its lock); it stays when the folder is renamed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RootEntry {
    pub key: String,
    pub path: PathBuf,
    /// A macOS / iOS bookmark that finds the folder after a move (not yet written).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bookmark: Option<String>,
    #[serde(default)]
    pub added: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct RootsFile {
    v: u32,
    #[serde(default)]
    roots: Vec<RootEntry>,
}

/// An open root: its library, or why it could not be opened (an unplugged
/// disk, a folder renamed in the Finder, a newer format).
pub struct Root {
    pub key: String,
    pub path: PathBuf,
    pub store: Option<Store>,
    pub error: Option<String>,
}

impl Root {
    /// What the sidebar calls it: the folder's own name.
    pub fn name(&self) -> String {
        self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| self.path.to_string_lossy().into_owned())
    }
}

/// Why a folder could not be added.
#[derive(Debug, PartialEq)]
pub enum AddError {
    /// Already open as this root.
    Open(String),
    /// Inside this open root: it is listed there already.
    Inside(String),
    /// Holds this open root: two indexes would keep the same files.
    Holds(String),
    Other(String),
}

impl std::fmt::Display for AddError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { AddError::Open(_) => write!(f, "这个文件夹已经打开"), AddError::Inside(_) => write!(f, "这个文件夹已经在打开的文件夹里"), AddError::Holds(_) => write!(f, "这个文件夹里有已经打开的文件夹"), AddError::Other(e) => write!(f, "{e}") }
    }
}

#[derive(Default)]
pub struct Workspace {
    pub roots: Vec<Root>,
    /// Which root each id is in, as of the last `list`.
    owner: HashMap<String, String>,
    /// `QUIET_MD_HOME`: one root, nothing remembered.
    fixed: bool,
}

/// A canonical path in its ordinary form: Windows' are `\\?\C:\…`.
pub fn plain(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|p| p.strip_prefix(r"\\?\")).filter(|p| !p.starts_with("UNC\\")) { Some(p) => PathBuf::from(p), None => path }
}

fn canonical(p: &Path) -> PathBuf { plain(p.fs_canonicalize().unwrap_or_else(|_| p.to_path_buf())) }

use crate::time::now;

impl Workspace {
    /// `roots.json`: beside the other things this computer keeps, or where
    /// `QUIET_MD_ROOTS` says (test runs with several roots, which must not
    /// touch this computer's list).
    pub fn roots_file() -> PathBuf {
        if let Some(f) = std::env::var_os("QUIET_MD_ROOTS") { return PathBuf::from(f); }
        dirs::data_local_dir().unwrap_or_else(crate::vfs::temp_dir).join("quiet-md").join("roots.json")
    }

    /// The roots as last left; the first run after 工作区 takes the library
    /// builds before it used, and the folders it had opened in place. With
    /// `QUIET_MD_HOME`, that folder alone (test runs touch nothing else).
    pub fn entries() -> (Vec<RootEntry>, bool) {
        // Several roots in a test run: the list in its own file, `QUIET_MD_HOME` first when it is new.
        if std::env::var_os("QUIET_MD_ROOTS").is_some() {
            if let Ok(b) = crate::vfs::read(Self::roots_file()) && let Ok(f) = serde_json::from_slice::<RootsFile>(&b) { return (f.roots, false); }
            let home = std::env::var_os("QUIET_MD_HOME").map(|h| RootEntry { key: new_key(), path: PathBuf::from(h), bookmark: None, added: now() });
            return (home.into_iter().collect(), false);
        }
        if let Some(home) = std::env::var_os("QUIET_MD_HOME") {
            return (vec![RootEntry { key: "home".into(), path: PathBuf::from(home), bookmark: None, added: 0 }], true);
        }
        if cfg!(target_family = "wasm") {
            return (vec![RootEntry { key: "web".into(), path: Store::default_root(), bookmark: None, added: 0 }], true);
        }
        if let Ok(b) = crate::vfs::read(Self::roots_file()) && let Ok(f) = serde_json::from_slice::<RootsFile>(&b) { return (f.roots, false); }
        (Self::from_legacy(), false)
    }

    /// The library and opened folders builds before 工作区 kept.
    fn from_legacy() -> Vec<RootEntry> {
        let lib = Store::legacy_root();
        let mut out = vec![];
        if Store::is_library(&lib) {
            out.push(RootEntry { key: new_key(), path: lib.clone(), bookmark: None, added: now() });
            for dir in Store::legacy_mounts(&lib) {
                if dir.fs_is_dir() && !out.iter().any(|e: &RootEntry| e.path == dir) { out.push(RootEntry { key: new_key(), path: dir, bookmark: None, added: now() }); }
            }
        }
        out
    }

    /// Open every root in `entries`. The library builds before 工作区 used
    /// still moves to the new layout without asking when it may (`Store::open`).
    pub fn open(entries: Vec<RootEntry>, fixed: bool) -> Self {
        let legacy = Store::legacy_root();
        let mut ws = Workspace { fixed, ..Default::default() };
        for e in entries {
            let store = if e.path == legacy && e.path.fs_is_dir() { Store::check_file_root(&e.path).and_then(|()| Store::open_library(e.path.clone())) } else { Store::open_root(&e.path) };
            ws.push(e.key, e.path, store);
        }
        ws
    }

    /// A caller-owned library, already opened with its own account and permissions.
    pub fn single(store: Store) -> Self {
        let path = store.root().to_path_buf();
        Self { roots: vec![Root { key: "session".into(), path, store: Some(store), error: None }], fixed: true, ..Default::default() }
    }

    /// The roots as last left, opened.
    pub fn load() -> Self {
        let (entries, fixed) = Self::entries();
        let first = !fixed && !Self::roots_file().fs_exists();
        let ws = Self::open(entries, fixed);
        if first && !ws.roots.is_empty() { let _ = ws.save(); }
        ws
    }

    fn push(&mut self, key: String, path: PathBuf, store: Result<Store>) {
        let (store, error) = match store {
            Ok(mut s) => {
                let open: Vec<&Store> = self.roots.iter().filter_map(|r| r.store.as_ref()).collect();
                // A folder copied whole, `.quiet/` and all: a library of its own from now on.
                let lib = s.library_id();
                if !lib.is_empty() && open.iter().any(|o| o.library_id() == lib) && let Err(e) = s.renew_library_id() { s.warnings.push(format!("{e}")); }
                let taken: HashSet<String> = open.iter().flat_map(|s| s.ids().cloned().collect::<Vec<_>>()).collect();
                if let Err(e) = s.rekey(&taken) { s.warnings.push(format!("{e}")); }
                (Some(s), None)
            }
            Err(e) => (None, Some(e.to_string())),
        };
        self.roots.push(Root { key, path, store, error });
    }

    pub fn save(&self) -> Result<()> {
        if self.fixed { return Ok(()); }
        let f = RootsFile { v: 1, roots: self.roots.iter().map(|r| RootEntry { key: r.key.clone(), path: r.path.clone(), bookmark: None, added: 0 }).collect() };
        let path = Self::roots_file();
        crate::vfs::create_dir_all(path.parent().unwrap())?;
        crate::store::write_atomic(&path, &serde_json::to_vec_pretty(&f)?)?;
        // Older builds look for one library: the first that opens.
        if let Some(r) = self.roots.iter().find(|r| r.store.is_some()) { let _ = Store::set_root_dir(r.path.clone()); }
        Ok(())
    }

    /// Where `dir` stands among the open roots.
    pub fn check(&self, dir: &Path) -> std::result::Result<PathBuf, AddError> {
        let dir = canonical(dir);
        for r in &self.roots {
            let p = canonical(&r.path);
            if p == dir { return Err(AddError::Open(r.key.clone())); }
            if dir.starts_with(&p) { return Err(AddError::Inside(r.key.clone())); }
            if p.starts_with(&dir) { return Err(AddError::Holds(r.key.clone())); }
        }
        Ok(dir)
    }

    /// Open folder `dir` as a new root at the end; its key.
    pub fn add(&mut self, dir: &Path) -> std::result::Result<String, AddError> {
        let dir = self.check(dir)?;
        let store = Store::open_root(&dir).map_err(|e| AddError::Other(e.to_string()))?;
        let key = new_key();
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
        self.roots.retain(|r| r.key != key);
        self.owner.retain(|_, k| k != key);
        self.save()
    }

    /// Try a root that could not be opened again (the disk is back).
    pub fn retry(&mut self, key: &str) {
        let Some(r) = self.roots.iter_mut().find(|r| r.key == key && r.store.is_none()) else { return };
        match Store::open_root(&r.path) { Ok(s) => { r.store = Some(s); r.error = None; } Err(e) => r.error = Some(e.to_string()) }
    }

    pub fn root(&self, key: &str) -> Option<&Root> { self.roots.iter().find(|r| r.key == key) }

    /// The first root that opened: where things go when nothing says otherwise.
    pub fn first(&self) -> Option<&Root> { self.roots.iter().find(|r| r.store.is_some()) }

    /// The key of the root `id` is in.
    pub fn owner(&mut self, id: &str) -> Option<String> {
        if let Some(k) = self.owner.get(id) && self.root(k).and_then(|r| r.store.as_ref()).is_some_and(|s| s.has(id)) { return Some(k.clone()); }
        let k = self.roots.iter().find(|r| r.store.as_ref().is_some_and(|s| s.has(id)))?.key.clone();
        self.owner.insert(id.to_string(), k.clone());
        Some(k)
    }

    /// The library `id` is in.
    pub fn store_of(&mut self, id: &str) -> Option<&mut Store> {
        let key = self.owner(id)?;
        self.roots.iter_mut().find(|r| r.key == key)?.store.as_mut()
    }

    pub fn store(&mut self, key: &str) -> Option<&mut Store> { self.roots.iter_mut().find(|r| r.key == key)?.store.as_mut() }

    /// The root folder holding `path`, if one does.
    pub fn root_of_path(&self, path: &Path) -> Option<&Root> {
        let path = canonical(path);
        self.roots.iter().find(|r| path.starts_with(canonical(&r.path)))
    }

    /// Every root's entries, each marked with its root.
    pub fn list(&mut self) -> Vec<Entry> {
        let mut out = vec![];
        self.owner.clear();
        for r in &mut self.roots {
            let Some(s) = r.store.as_mut() else { continue };
            for mut e in s.list() {
                self.owner.insert(e.id.clone(), r.key.clone());
                e.root = r.key.clone();
                out.push(e);
            }
        }
        out
    }

    /// Problems the roots met while opening or listing, each once.
    pub fn take_warnings(&mut self) -> Vec<String> {
        self.roots.iter_mut().filter_map(|r| r.store.as_mut()).flat_map(|s| std::mem::take(&mut s.warnings)).collect()
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

    #[cfg(feature = "sqlite-store")]
    #[test]
    fn sqlite_library_cannot_be_registered_as_a_file_root() {
        let root = dir("sqlite-boundary");
        Store::initialize_local_sqlite(&root).unwrap();
        let mut store = Store::open_sqlite_at(root.clone()).unwrap();
        let id = store.create_from(&doc("SQLite document")).unwrap();

        let mut files = Workspace { fixed: true, ..Default::default() };
        assert!(matches!(files.add(&root), Err(AddError::Other(_))));
        assert!(files.roots.is_empty());
        assert!(!root.join(crate::store::INTERNAL).join("index.json").fs_exists());

        let mut session = Workspace::single(store);
        assert_eq!(session.store("session").unwrap().load(&id).unwrap().title.text, "SQLite document");
    }

    #[test]
    fn roots_are_libraries_of_their_own() {
        let (a, b) = (dir("a"), dir("b"));
        crate::vfs::write(b.join("note.md"), "# note\n").unwrap();
        let mut ws = Workspace { fixed: true, ..Default::default() };
        let ka = ws.add(&a).unwrap();
        let kb = ws.add(&b).unwrap();
        assert!(a.join(".quiet/format.json").fs_is_file() && b.join(".quiet/format.json").fs_is_file(), "every root has its .quiet/");
        let made = ws.store(&ka).unwrap().create_from(&doc("in a")).unwrap();
        let list = ws.list();
        assert_eq!(list.iter().find(|e| e.id == made).unwrap().root, ka);
        assert_eq!(list.iter().find(|e| e.title == "note").unwrap().root, kb);
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
        assert!(ws.list().iter().any(|e| e.id == made && e.root == back), "same id after reopening");
    }

    #[test]
    fn a_copied_folder_gets_ids_of_its_own() {
        let a = dir("orig");
        let mut s = Store::open_root(&a).unwrap();
        s.create_from(&doc("one")).unwrap();
        s.list();
        drop(s);
        let b = dir("copy");
        crate::vfs::copy(a.join("one.xml"), b.join("one.xml")).unwrap();
        crate::vfs::create_dir_all(b.join(".quiet")).unwrap();
        for name in ["format.json", "index.json"] { crate::vfs::copy(a.join(".quiet").join(name), b.join(".quiet").join(name)).unwrap(); }
        let mut ws = Workspace { fixed: true, ..Default::default() };
        ws.add(&a).unwrap();
        ws.add(&b).unwrap();
        let list = ws.list();
        let ids: HashSet<&String> = list.iter().map(|e| &e.id).collect();
        assert_eq!(ids.len(), list.len(), "no id in two roots");
        let (la, lb) = (ws.roots[0].store.as_ref().unwrap().library_id(), ws.roots[1].store.as_ref().unwrap().library_id());
        assert!(!la.is_empty() && la != lb, "the copy is a library of its own");
    }

    #[test]
    fn a_missing_root_is_listed_not_fatal() {
        let a = dir("gone");
        let entries = vec![RootEntry { key: "k".into(), path: a.join("not-here"), bookmark: None, added: 0 }];
        let mut ws = Workspace::open(entries, true);
        assert!(ws.roots[0].store.is_none() && ws.roots[0].error.is_some());
        assert!(!a.join("not-here").fs_exists(), "never made anew");
        assert!(ws.list().is_empty());
    }
}
