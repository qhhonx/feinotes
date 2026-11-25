//! File access for the engine and the app. Desktop builds use the real file
//! system (`std::fs` re-exported as is); the web build (wasm32, the site's
//! live demo) keeps a library in memory, seeded from a bundle at startup, and
//! forgets edits on reload.
//!
//! Path queries go through [`PathExt`] (`p.fs_exists()` for `p.exists()`, and
//! so on) and kinds through [`KindExt`], since `Path`'s own methods always ask
//! the real file system.

use std::io;
use std::path::{Path, PathBuf};

#[cfg(not(target_family = "wasm"))]
pub use std::fs::*;

#[cfg(target_family = "wasm")]
pub use mem::*;

/// Whether `path` is a placeholder whose contents are still only in the
/// cloud (iCloud's optimized storage, #406): reading it blocks until it is
/// downloaded, so nothing reads one on the main thread or in a background
/// scan; stat and listing a folder don't download. macOS: the stat flag
/// `SF_DATALESS`; false elsewhere. The one place that decides it (sync 1d
/// may ask the file provider instead, and Windows' RECALL_ON_DATA_ACCESS).
pub fn is_dataless(path: &Path) -> bool {
    #[cfg(test)]
    if DATALESS.with(|d| d.borrow().iter().any(|p| p == path)) { return true }
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::MetadataExt;
        const SF_DATALESS: u32 = 0x4000_0000;
        std::fs::symlink_metadata(path).is_ok_and(|m| m.st_flags() & SF_DATALESS != 0)
    }
    #[cfg(not(target_os = "macos"))]
    { let _ = path; false }
}

#[cfg(test)]
thread_local! {
    /// Files this test treats as cloud-only placeholders.
    static DATALESS: std::cell::RefCell<Vec<std::path::PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Treat `path` as a cloud-only placeholder in this test thread (or no longer).
#[cfg(test)]
pub fn set_dataless(path: &Path, on: bool) {
    DATALESS.with(|d| { let mut d = d.borrow_mut(); d.retain(|p| p != path); if on { d.push(path.to_path_buf()); } });
}

pub trait PathExt {
    fn fs_exists(&self) -> bool;
    fn fs_try_exists(&self) -> io::Result<bool>;
    fn fs_is_dir(&self) -> bool;
    fn fs_is_file(&self) -> bool;
    fn fs_metadata(&self) -> io::Result<Metadata>;
    fn fs_symlink_metadata(&self) -> io::Result<Metadata>;
    fn fs_canonicalize(&self) -> io::Result<PathBuf>;
    fn fs_read_dir(&self) -> io::Result<ReadDir>;
    /// `is_absolute()`, which is false for every path on wasm32-unknown-unknown.
    fn fs_is_absolute(&self) -> bool;
}

impl PathExt for Path {
    fn fs_exists(&self) -> bool { metadata(self).is_ok() }
    fn fs_try_exists(&self) -> io::Result<bool> {
        match metadata(self) { Ok(_) => Ok(true), Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false), Err(e) => Err(e) }
    }
    fn fs_is_dir(&self) -> bool { metadata(self).map(|m| m.is_dir()).unwrap_or(false) }
    fn fs_is_file(&self) -> bool { metadata(self).map(|m| m.is_file()).unwrap_or(false) }
    fn fs_metadata(&self) -> io::Result<Metadata> { metadata(self) }
    fn fs_symlink_metadata(&self) -> io::Result<Metadata> { symlink_metadata(self) }
    fn fs_canonicalize(&self) -> io::Result<PathBuf> { canonicalize(self) }
    fn fs_read_dir(&self) -> io::Result<ReadDir> { read_dir(self) }
    fn fs_is_absolute(&self) -> bool { if cfg!(target_family = "wasm") { self.has_root() } else { self.is_absolute() } }
}

/// `is_dir` / `is_file` on metadata and entry kinds, named like [`PathExt`]'s
/// so one rename covers both.
pub trait KindExt {
    fn fs_is_dir(&self) -> bool;
    fn fs_is_file(&self) -> bool;
}
impl KindExt for Metadata {
    fn fs_is_dir(&self) -> bool { self.is_dir() }
    fn fs_is_file(&self) -> bool { self.is_file() }
}
impl KindExt for FileType {
    fn fs_is_dir(&self) -> bool { self.is_dir() }
    fn fs_is_file(&self) -> bool { self.is_file() }
}

/// `std::env::temp_dir()`; the web build has `/tmp` in memory (std panics there).
pub fn temp_dir() -> PathBuf {
    #[cfg(not(target_family = "wasm"))] { std::env::temp_dir() }
    #[cfg(target_family = "wasm")] { PathBuf::from("/tmp") }
}

/// `std::process::id()`, which panics in the browser.
pub fn process_id() -> u32 {
    #[cfg(not(target_family = "wasm"))] { std::process::id() }
    #[cfg(target_family = "wasm")] { 1 }
}

/// When the file at `path` was made and last changed, in unix seconds, for
/// an import to keep. Made: its creation time where the system keeps one
/// (macOS, Windows; Linux on file systems with statx), else the change time,
/// and never later than that (a copied file is made after its last change).
/// None in the web build, or when neither can be read.
pub fn file_times(path: &Path) -> Option<(u64, u64)> {
    #[cfg(not(target_family = "wasm"))] {
        let m = std::fs::metadata(path).ok()?;
        let secs = |t: std::time::SystemTime| t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs());
        let modified = m.modified().ok().and_then(secs)?;
        let made = m.created().ok().and_then(secs).unwrap_or(modified).min(modified);
        Some((made, modified))
    }
    #[cfg(target_family = "wasm")] { let _ = path; None }
}

/// When the file or folder at `path` was made, in unix seconds, where the
/// system keeps that (macOS, Windows; Linux on file systems with statx).
pub fn made_time(path: &Path) -> Option<u64> {
    #[cfg(not(target_family = "wasm"))] { std::fs::metadata(path).ok()?.created().ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs()) }
    #[cfg(target_family = "wasm")] { let _ = path; None }
}

/// Set the change time of the file at `path` to unix second `secs`, as a
/// copy that keeps it (the web build keeps no times).
pub fn set_modified(path: &Path, secs: u64) -> io::Result<()> {
    #[cfg(not(target_family = "wasm"))] {
        std::fs::File::options().write(true).open(path)?.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
    }
    #[cfg(target_family = "wasm")] { let _ = (path, secs); Ok(()) }
}

/// Whether `path` is a lazy file still being fetched (web build): the UI shows
/// it as loading, not missing. Always false on the desktop.
pub fn pending(path: &Path) -> bool {
    #[cfg(target_family = "wasm")] { mem::pending(path) }
    #[cfg(not(target_family = "wasm"))] { let _ = path; false }
}

#[cfg(target_family = "wasm")]
pub use mem::{add_lazy, set_fetcher};

/// Puts a file into the in-memory library (web build), creating its folders.
#[cfg(target_family = "wasm")]
pub fn seed(path: &Path, bytes: Vec<u8>) {
    if let Some(dir) = path.parent() { let _ = create_dir_all(dir); }
    let _ = write(path, bytes);
}

#[cfg(target_family = "wasm")]
mod mem {
        use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::io::{self, Read, Seek, SeekFrom, Write};
    use std::path::{Component, Path, PathBuf};
    use std::sync::{Arc, LazyLock, Mutex};
    use web_time::SystemTime;

    enum Node { Dir(SystemTime), File(Arc<Mutex<Vec<u8>>>, SystemTime) }

    static NODES: LazyLock<Mutex<BTreeMap<PathBuf, Node>>> = LazyLock::new(|| Mutex::new(BTreeMap::from([(PathBuf::from("/"), Node::Dir(SystemTime::now()))])));

    fn norm(p: &Path) -> PathBuf {
        let mut out = PathBuf::from("/");
        for c in p.components() {
            match c {
                Component::Normal(s) => out.push(s),
                Component::ParentDir => { out.pop(); }
                _ => {}
            }
        }
        out
    }
    fn not_found(p: &Path) -> io::Error { io::Error::new(io::ErrorKind::NotFound, format!("{}: not found", p.display())) }
    fn with<R>(f: impl FnOnce(&mut BTreeMap<PathBuf, Node>) -> R) -> R { f(&mut NODES.lock().unwrap()) }
    fn parent_is_dir(m: &BTreeMap<PathBuf, Node>, p: &Path) -> bool {
        p.parent().is_none_or(|d| matches!(m.get(d), Some(Node::Dir(_))))
    }
    fn children<'a>(m: &'a BTreeMap<PathBuf, Node>, dir: &'a Path) -> impl Iterator<Item = &'a PathBuf> + 'a {
        m.range(dir.to_path_buf()..).skip(1).take_while(move |(k, _)| k.starts_with(dir)).map(|(k, _)| k)
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct FileType { dir: bool }
    impl FileType {
        pub fn is_dir(&self) -> bool { self.dir }
        pub fn is_file(&self) -> bool { !self.dir }
        pub fn is_symlink(&self) -> bool { false }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Permissions { readonly: bool }
    impl Permissions {
        pub fn readonly(&self) -> bool { self.readonly }
        pub fn set_readonly(&mut self, readonly: bool) { self.readonly = readonly }
    }

    #[derive(Clone, Debug)]
    pub struct Metadata { kind: FileType, len: u64, modified: SystemTime }
    impl Metadata {
        pub fn is_dir(&self) -> bool { self.kind.dir }
        pub fn is_file(&self) -> bool { !self.kind.dir }
        pub fn is_symlink(&self) -> bool { false }
        pub fn file_type(&self) -> FileType { self.kind }
        pub fn len(&self) -> u64 { self.len }
        pub fn modified(&self) -> io::Result<SystemTime> { Ok(self.modified) }
        pub fn created(&self) -> io::Result<SystemTime> { Ok(self.modified) }
        pub fn accessed(&self) -> io::Result<SystemTime> { Ok(self.modified) }
        pub fn permissions(&self) -> Permissions { Permissions { readonly: false } }
    }

    // Lazy files (pictures): known by path, fetched the first time something
    // asks about them. Until they arrive they don't exist (`pending` is true).
    enum Lazy { Remote(String), Loading }
    static LAZY: LazyLock<Mutex<BTreeMap<PathBuf, Lazy>>> = LazyLock::new(Default::default);
    static FETCHER: Mutex<Option<fn(PathBuf, String)>> = Mutex::new(None);

    /// `path` will be fetched from `url` on first use, by the fetcher.
    pub fn add_lazy(path: &Path, url: String) { LAZY.lock().unwrap().insert(norm(path), Lazy::Remote(url)); }
    /// Fetches a lazy file; it calls `super::seed` with the bytes when they arrive.
    pub fn set_fetcher(f: fn(PathBuf, String)) { *FETCHER.lock().unwrap() = Some(f); }
    pub(super) fn pending(p: &Path) -> bool {
        let p = norm(p);
        let mut lazy = LAZY.lock().unwrap();
        let start = match lazy.get_mut(&p) {
            Some(l @ Lazy::Remote(_)) => match std::mem::replace(l, Lazy::Loading) { Lazy::Remote(url) => Some(url), Lazy::Loading => None },
            Some(Lazy::Loading) => None,
            None => return false,
        };
        drop(lazy);
        if let (Some(url), Some(fetch)) = (start, *FETCHER.lock().unwrap()) { fetch(p, url) }
        true
    }
    fn arrived(p: &Path) { LAZY.lock().unwrap().remove(p); }

    pub fn metadata<P: AsRef<Path>>(p: P) -> io::Result<Metadata> {
        let p = norm(p.as_ref());
        with(|m| match m.get(&p) {
            Some(Node::Dir(t)) => Ok(Metadata { kind: FileType { dir: true }, len: 0, modified: *t }),
            Some(Node::File(d, t)) => Ok(Metadata { kind: FileType { dir: false }, len: d.lock().unwrap().len() as u64, modified: *t }),
            None => Err(not_found(&p)),
        })
    }
    pub fn symlink_metadata<P: AsRef<Path>>(p: P) -> io::Result<Metadata> { metadata(p) }
    pub fn exists<P: AsRef<Path>>(p: P) -> io::Result<bool> { Ok(metadata(p).is_ok()) }
    pub fn canonicalize<P: AsRef<Path>>(p: P) -> io::Result<PathBuf> {
        let p = norm(p.as_ref());
        metadata(&p).map(|_| p)
    }

    pub fn read<P: AsRef<Path>>(p: P) -> io::Result<Vec<u8>> {
        let p = norm(p.as_ref());
        with(|m| match m.get(&p) {
            Some(Node::File(d, _)) => Ok(d.lock().unwrap().clone()),
            Some(Node::Dir(_)) => Err(io::Error::new(io::ErrorKind::IsADirectory, "is a directory")),
            None => Err(not_found(&p)),
        })
    }
    pub fn read_to_string<P: AsRef<Path>>(p: P) -> io::Result<String> {
        String::from_utf8(read(p)?).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
    pub fn write<P: AsRef<Path>, C: AsRef<[u8]>>(p: P, contents: C) -> io::Result<()> {
        let p = norm(p.as_ref());
        with(|m| {
            if !parent_is_dir(m, &p) { return Err(not_found(p.parent().unwrap_or(&p))) }
            if matches!(m.get(&p), Some(Node::Dir(_))) { return Err(io::Error::new(io::ErrorKind::IsADirectory, "is a directory")) }
            m.insert(p.clone(), Node::File(Arc::new(Mutex::new(contents.as_ref().to_vec())), SystemTime::now()));
            arrived(&p);
            Ok(())
        })
    }
    pub fn create_dir<P: AsRef<Path>>(p: P) -> io::Result<()> {
        let p = norm(p.as_ref());
        with(|m| {
            if m.contains_key(&p) { return Err(io::Error::new(io::ErrorKind::AlreadyExists, "exists")) }
            if !parent_is_dir(m, &p) { return Err(not_found(&p)) }
            m.insert(p, Node::Dir(SystemTime::now()));
            Ok(())
        })
    }
    pub fn create_dir_all<P: AsRef<Path>>(p: P) -> io::Result<()> {
        let p = norm(p.as_ref());
        with(|m| {
            for a in p.ancestors().collect::<Vec<_>>().into_iter().rev() {
                match m.get(a) {
                    Some(Node::Dir(_)) => {}
                    Some(Node::File(..)) => return Err(io::Error::new(io::ErrorKind::NotADirectory, "not a directory")),
                    None => { m.insert(a.to_path_buf(), Node::Dir(SystemTime::now())); }
                }
            }
            Ok(())
        })
    }
    pub fn remove_file<P: AsRef<Path>>(p: P) -> io::Result<()> {
        let p = norm(p.as_ref());
        with(|m| match m.get(&p) {
            Some(Node::File(..)) => { m.remove(&p); Ok(()) }
            Some(Node::Dir(_)) => Err(io::Error::new(io::ErrorKind::IsADirectory, "is a directory")),
            None => Err(not_found(&p)),
        })
    }
    pub fn remove_dir<P: AsRef<Path>>(p: P) -> io::Result<()> {
        let p = norm(p.as_ref());
        with(|m| {
            if !matches!(m.get(&p), Some(Node::Dir(_))) { return Err(not_found(&p)) }
            if children(m, &p).next().is_some() { return Err(io::Error::new(io::ErrorKind::DirectoryNotEmpty, "not empty")) }
            m.remove(&p);
            Ok(())
        })
    }
    pub fn remove_dir_all<P: AsRef<Path>>(p: P) -> io::Result<()> {
        let p = norm(p.as_ref());
        with(|m| {
            if !m.contains_key(&p) { return Err(not_found(&p)) }
            let gone: Vec<PathBuf> = children(m, &p).cloned().collect();
            for k in gone { m.remove(&k); }
            m.remove(&p);
            Ok(())
        })
    }
    pub fn rename<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<()> {
        let (from, to) = (norm(from.as_ref()), norm(to.as_ref()));
        if from == to { return metadata(&from).map(|_| ()) }
        with(|m| {
            if !m.contains_key(&from) { return Err(not_found(&from)) }
            if !parent_is_dir(m, &to) { return Err(not_found(&to)) }
            let mut moved: Vec<(PathBuf, Node)> = Vec::new();
            let keys: Vec<PathBuf> = std::iter::once(from.clone()).chain(children(m, &from).cloned()).collect();
            for k in keys {
                let node = m.remove(&k).unwrap();
                let rest = k.strip_prefix(&from).unwrap();
                moved.push((if rest.as_os_str().is_empty() { to.clone() } else { to.join(rest) }, node));
            }
            if let Some(Node::Dir(_)) = m.get(&to) {
                let gone: Vec<PathBuf> = children(m, &to).cloned().collect();
                for k in gone { m.remove(&k); }
            }
            for (k, n) in moved { m.insert(k, n); }
            Ok(())
        })
    }
    pub fn copy<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<u64> {
        let data = read(from)?;
        let n = data.len() as u64;
        write(to, data)?;
        Ok(n)
    }
    pub fn hard_link<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<()> { copy(from, to).map(|_| ()) }
    pub fn set_permissions<P: AsRef<Path>>(p: P, _: Permissions) -> io::Result<()> { metadata(p).map(|_| ()) }

    pub struct DirEntry { path: PathBuf, kind: FileType }
    impl DirEntry {
        pub fn path(&self) -> PathBuf { self.path.clone() }
        pub fn file_name(&self) -> OsString { self.path.file_name().unwrap_or_default().to_owned() }
        pub fn file_type(&self) -> io::Result<FileType> { Ok(self.kind) }
        pub fn metadata(&self) -> io::Result<Metadata> { metadata(&self.path) }
    }
    pub struct ReadDir(std::vec::IntoIter<DirEntry>);
    impl Iterator for ReadDir {
        type Item = io::Result<DirEntry>;
        fn next(&mut self) -> Option<Self::Item> { self.0.next().map(Ok) }
    }
    pub fn read_dir<P: AsRef<Path>>(p: P) -> io::Result<ReadDir> {
        let p = norm(p.as_ref());
        with(|m| {
            if !matches!(m.get(&p), Some(Node::Dir(_))) { return Err(not_found(&p)) }
            let depth = p.components().count() + 1;
            let list = children(m, &p).filter(|k| k.components().count() == depth)
                .map(|k| DirEntry { path: k.clone(), kind: FileType { dir: matches!(m.get(k), Some(Node::Dir(_))) } })
                .collect::<Vec<_>>();
            Ok(ReadDir(list.into_iter()))
        })
    }

    #[derive(Clone, Debug, Default)]
    pub struct OpenOptions { read: bool, write: bool, append: bool, truncate: bool, create: bool, create_new: bool }
    impl OpenOptions {
        pub fn new() -> Self { Self::default() }
        pub fn read(&mut self, v: bool) -> &mut Self { self.read = v; self }
        pub fn write(&mut self, v: bool) -> &mut Self { self.write = v; self }
        pub fn append(&mut self, v: bool) -> &mut Self { self.append = v; self }
        pub fn truncate(&mut self, v: bool) -> &mut Self { self.truncate = v; self }
        pub fn create(&mut self, v: bool) -> &mut Self { self.create = v; self }
        pub fn create_new(&mut self, v: bool) -> &mut Self { self.create_new = v; self }
        pub fn open<P: AsRef<Path>>(&self, p: P) -> io::Result<File> {
            let p = norm(p.as_ref());
            let data = with(|m| -> io::Result<Arc<Mutex<Vec<u8>>>> {
                match m.get(&p) {
                    Some(Node::File(d, _)) => {
                        if self.create_new { return Err(io::Error::new(io::ErrorKind::AlreadyExists, "exists")) }
                        if self.truncate { d.lock().unwrap().clear() }
                        Ok(d.clone())
                    }
                    Some(Node::Dir(_)) => Err(io::Error::new(io::ErrorKind::IsADirectory, "is a directory")),
                    None if (self.create || self.create_new) && parent_is_dir(m, &p) => {
                        let d = Arc::new(Mutex::new(Vec::new()));
                        m.insert(p.clone(), Node::File(d.clone(), SystemTime::now()));
                        Ok(d)
                    }
                    None => Err(not_found(&p)),
                }
            })?;
            let pos = if self.append { data.lock().unwrap().len() as u64 } else { 0 };
            Ok(File { data, pos, append: self.append })
        }
    }

    pub struct File { data: Arc<Mutex<Vec<u8>>>, pos: u64, append: bool }
    impl File {
        pub fn open<P: AsRef<Path>>(p: P) -> io::Result<File> { OpenOptions::new().read(true).open(p) }
        pub fn create<P: AsRef<Path>>(p: P) -> io::Result<File> { OpenOptions::new().write(true).create(true).truncate(true).open(p) }
        pub fn options() -> OpenOptions { OpenOptions::new() }
        pub fn lock(&self) -> io::Result<()> { Ok(()) }
        pub fn lock_shared(&self) -> io::Result<()> { Ok(()) }
        pub fn try_lock(&self) -> Result<(), std::fs::TryLockError> { Ok(()) }
        pub fn unlock(&self) -> io::Result<()> { Ok(()) }
        pub fn sync_all(&self) -> io::Result<()> { Ok(()) }
        pub fn sync_data(&self) -> io::Result<()> { Ok(()) }
        pub fn set_len(&self, n: u64) -> io::Result<()> { self.data.lock().unwrap().resize(n as usize, 0); Ok(()) }
        pub fn metadata(&self) -> io::Result<Metadata> {
            Ok(Metadata { kind: FileType { dir: false }, len: self.data.lock().unwrap().len() as u64, modified: SystemTime::now() })
        }
    }
    impl Read for File {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let d = self.data.lock().unwrap();
            let start = (self.pos as usize).min(d.len());
            let n = buf.len().min(d.len() - start);
            buf[..n].copy_from_slice(&d[start..start + n]);
            self.pos += n as u64;
            Ok(n)
        }
    }
    impl Write for File {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let mut d = self.data.lock().unwrap();
            if self.append { self.pos = d.len() as u64 }
            let start = self.pos as usize;
            if d.len() < start + buf.len() { d.resize(start + buf.len(), 0) }
            d[start..start + buf.len()].copy_from_slice(buf);
            self.pos += buf.len() as u64;
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> { Ok(()) }
    }
    impl Seek for File {
        fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
            let len = self.data.lock().unwrap().len() as i64;
            let at = match to { SeekFrom::Start(n) => n as i64, SeekFrom::End(n) => len + n, SeekFrom::Current(n) => self.pos as i64 + n };
            if at < 0 { return Err(io::Error::new(io::ErrorKind::InvalidInput, "seek before start")) }
            self.pos = at as u64;
            Ok(self.pos)
        }
    }
}
