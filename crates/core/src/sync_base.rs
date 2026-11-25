//! Base snapshots for sync: the content of each file as this device last read
//! or wrote it, kept as the common ancestor of a later three-way merge.
//!
//! Layout under the directory the caller passes in (this module computes no
//! paths of its own): `bases/<hash>` holds contents, stored once per distinct
//! content; `state.json` maps each NFC relative path to its `{hash, size,
//! mtime}`. `mtime` is the wall-clock unix seconds at record time (not the
//! file's own mtime).
//!
//! The hash is the git blob SHA-1, `sha1("blob <len>\0" + bytes)` (sync.md
//! 3.1): a file then has the same hash as its blob sha in a GitHub tree, so
//! the GitHub backend can tell whether a file changed without downloading it.
//!
//! Paths are NFC, and compare case-insensitively (sync.md 1.7) while the entry
//! keeps the case it was last recorded with.

use crate::store::write_atomic;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub hash: String,
    pub size: u64,
    pub mtime: u64,
}

pub struct Bases {
    dir: PathBuf,
    /// Lowercase NFC path → (NFC path as recorded, entry).
    map: HashMap<String, (String, Entry)>,
    warnings: Vec<String>,
}

/// The git blob SHA-1 of `bytes`, lowercase hex (what `git hash-object` prints).
pub fn blob_sha(bytes: &[u8]) -> String {
    let mut h = sha1_smol::Sha1::new();
    h.update(format!("blob {}\0", bytes.len()).as_bytes());
    h.update(bytes);
    h.digest().to_string()
}

fn norm(rel: &str) -> (String, String) {
    use unicode_normalization::UnicodeNormalization;
    let nfc: String = rel.nfc().collect();
    (nfc.to_lowercase(), nfc)
}

impl Bases {
    /// Loads `dir/state.json`; a missing file is empty, an unreadable or corrupt
    /// one is empty plus a warning (see `take_warnings`).
    pub fn open(dir: &Path) -> Self {
        let mut warnings = Vec::new();
        let path = dir.join("state.json");
        let loaded = match crate::vfs::read(&path) {
            Ok(b) => serde_json::from_slice::<BTreeMap<String, Entry>>(&b).map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(e.to_string()),
        };
        let state = loaded.unwrap_or_else(|e| {
            warnings.push(format!("同步基线状态无法读取，已重置：{e}"));
            BTreeMap::new()
        });
        let map = state.into_iter().map(|(k, e)| { let (l, n) = norm(&k); (l, (n, e)) }).collect();
        Bases { dir: dir.to_path_buf(), map, warnings }
    }

    pub fn take_warnings(&mut self) -> Vec<String> { std::mem::take(&mut self.warnings) }

    fn blob_path(&self, hash: &str) -> PathBuf { self.dir.join("bases").join(hash) }

    fn save(&self) -> Result<()> {
        let state: BTreeMap<&str, &Entry> = self.map.values().map(|(k, e)| (k.as_str(), e)).collect();
        crate::vfs::create_dir_all(&self.dir)?;
        write_atomic(&self.dir.join("state.json"), &serde_json::to_vec_pretty(&state)?)
    }

    /// Stores `bytes` (unless that content is already there) as the base of
    /// `rel`, replacing any earlier base, and saves the state.
    pub fn record(&mut self, rel: &str, bytes: &[u8]) -> Result<()> {
        let hash = blob_sha(bytes);
        let blob = self.blob_path(&hash);
        if !crate::vfs::metadata(&blob).is_ok_and(|m| m.len() == bytes.len() as u64) {
            crate::vfs::create_dir_all(self.dir.join("bases"))?;
            write_atomic(&blob, bytes)?;
        }
        let (lower, nfc) = norm(rel);
        self.map.insert(lower, (nfc, Entry { hash, size: bytes.len() as u64, mtime: crate::time::now() }));
        self.save()
    }

    pub fn entry(&self, rel: &str) -> Option<&Entry> { self.map.get(&norm(rel).0).map(|(_, e)| e) }

    /// The recorded content; `None` without an entry or when its blob is gone.
    pub fn base(&self, rel: &str) -> Option<Vec<u8>> {
        crate::vfs::read(self.blob_path(&self.entry(rel)?.hash)).ok()
    }

    /// Drops the entry of `rel` (its blob stays until `gc`).
    pub fn forget(&mut self, rel: &str) -> Result<()> {
        if self.map.remove(&norm(rel).0).is_some() { self.save()?; }
        Ok(())
    }

    /// Moves the entry of `from` to `to`, replacing one already there.
    pub fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        let Some((_, e)) = self.map.remove(&norm(from).0) else { return Ok(()) };
        let (lower, nfc) = norm(to);
        self.map.insert(lower, (nfc, e));
        self.save()
    }

    /// Deletes blobs no entry refers to; returns how many.
    pub fn gc(&self) -> Result<usize> {
        let live: HashSet<&str> = self.map.values().map(|(_, e)| e.hash.as_str()).collect();
        let Ok(dir) = crate::vfs::read_dir(self.dir.join("bases")) else { return Ok(0) };
        let mut n = 0;
        for f in dir.flatten() {
            if !live.contains(f.file_name().to_string_lossy().as_ref()) && crate::vfs::remove_file(f.path()).is_ok() { n += 1; }
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = crate::vfs::temp_dir().join(format!("quiet-md-base-{}", uuid::Uuid::new_v4()));
        crate::vfs::create_dir_all(&d).unwrap();
        d
    }
    fn done(d: PathBuf) { let _ = crate::vfs::remove_dir_all(d); }
    fn nblobs(d: &Path) -> usize { crate::vfs::read_dir(d.join("bases")).map_or(0, |r| r.count()) }

    #[test]
    fn round_trip_and_reopen() {
        let d = tmp();
        let mut b = Bases::open(&d);
        b.record("a/x.md", b"hello\n").unwrap();
        assert_eq!(b.base("a/x.md").unwrap(), b"hello\n");
        assert_eq!(b.entry("a/x.md").unwrap().size, 6);
        assert!(b.base("nope.md").is_none());
        let mut b2 = Bases::open(&d);
        assert!(b2.take_warnings().is_empty());
        assert_eq!(b2.entry("a/x.md"), b.entry("a/x.md"));
        assert_eq!(b2.base("a/x.md").unwrap(), b"hello\n");
        done(d);
    }

    #[test]
    fn same_content_one_blob() {
        let d = tmp();
        let mut b = Bases::open(&d);
        b.record("a.md", b"same").unwrap();
        b.record("b.md", b"same").unwrap();
        assert_eq!(nblobs(&d), 1);
        done(d);
    }

    #[test]
    fn rename_forget_gc() {
        let d = tmp();
        let mut b = Bases::open(&d);
        b.record("a.md", b"one").unwrap();
        b.record("b.md", b"two").unwrap();
        b.rename("a.md", "c.md").unwrap();
        assert!(b.entry("a.md").is_none());
        assert_eq!(b.base("c.md").unwrap(), b"one");
        assert_eq!(Bases::open(&d).base("c.md").unwrap(), b"one");
        assert_eq!(b.gc().unwrap(), 0);
        b.forget("b.md").unwrap();
        assert!(b.base("b.md").is_none());
        assert_eq!(b.gc().unwrap(), 1);
        assert_eq!(nblobs(&d), 1);
        assert_eq!(b.base("c.md").unwrap(), b"one");
        done(d);
    }

    #[test]
    fn corrupt_state_and_missing_blob() {
        let d = tmp();
        crate::vfs::write(d.join("state.json"), b"{ nope").unwrap();
        let mut b = Bases::open(&d);
        assert_eq!(b.take_warnings().len(), 1);
        assert!(b.entry("a.md").is_none());
        b.record("a.md", b"x").unwrap();
        let h = b.entry("a.md").unwrap().hash.clone();
        crate::vfs::remove_file(d.join("bases").join(h)).unwrap();
        assert!(b.entry("a.md").is_some());
        assert!(b.base("a.md").is_none());
        done(d);
    }

    #[test]
    fn nfc_and_case_insensitive_keys() {
        let d = tmp();
        let mut b = Bases::open(&d);
        b.record("caf\u{e9}.md", b"1").unwrap();
        assert_eq!(b.base("cafe\u{301}.md").unwrap(), b"1");
        b.record("a/Notes.md", b"2").unwrap();
        assert!(b.entry("A/notes.MD").is_some());
        b.record("A/NOTES.md", b"3").unwrap();
        assert_eq!(b.map.len(), 2);
        assert_eq!(b.base("a/notes.md").unwrap(), b"3");
        let s = crate::vfs::read_to_string(d.join("state.json")).unwrap();
        assert!(s.contains("A/NOTES.md") && !s.contains("a/Notes.md"));
        done(d);
    }

    #[test]
    fn blob_sha_matches_git() {
        assert_eq!(blob_sha(b""), "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391");
        assert_eq!(blob_sha(b"a"), "2e65efe2a145dda7ee51d1741299f848e5bf752e");
        assert_eq!(blob_sha(b"hello\n"), "ce013625030ba8dba906f756967f9e9ca394464a");
        assert_eq!(blob_sha("中文\n".as_bytes()), "0c3dd90b19be56e9cd94f052f74526aac2458521");
    }
}
