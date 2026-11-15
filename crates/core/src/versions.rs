//! Per-document version history. A snapshot is the previous on-disk text of
//! a document, kept verbatim under `.quiet/history/<id>/`. The directory is its own
//! index: no global manifest to merge, back up or corrupt. Older builds do
//! not read this directory, so the layout needs no FORMAT bump.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const MIN_LIMIT: u32 = 1;
pub const MAX_LIMIT: u32 = 30;
pub const DEFAULT_LIMIT: u32 = 20;
/// Two automatic snapshots of one document are at least this far apart (s).
pub const MIN_INTERVAL: u64 = 300;
/// Ceiling for every file under `history/` in one library.
pub const LIBRARY_BUDGET: u64 = 200 << 20;

pub fn default_history_limit() -> u32 { DEFAULT_LIMIT }
pub fn clamp_limit(n: u32) -> u32 { n.clamp(MIN_LIMIT, MAX_LIMIT) }

/// Keep the 20 snapshots by default; a library restored from an old build has
/// no `history_limit` and must not silently keep zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub name: String,
    pub ms: u64,
    pub size: u64,
    /// 命名版本: a name the user gave this snapshot (A3). Stored beside the
    /// snapshot in `<name>.name` so the JSON-free directory stays the index.
    pub label: Option<String>,
}

/// Longest label kept; longer input is cut rather than refused.
pub const MAX_LABEL: usize = 40;

// `root` is the library's `.quiet/`; the 回收站's snapshots sit beside, not
// inside, `trash/`, whose entries are trashed documents.
fn history_root(root: &Path) -> PathBuf { root.join("history") }
fn trash_root(root: &Path) -> PathBuf { root.join("history-trash") }
fn base(root: &Path, trashed: bool) -> PathBuf { if trashed { trash_root(root) } else { history_root(root) } }

/// A snapshot's label file: `<snapshot>.name`, next to the snapshot.
fn label_path(dir: &Path, name: &str) -> PathBuf { dir.join(format!("{name}.name")) }

fn label_of(dir: &Path, name: &str) -> Option<String> {
    let text = std::fs::read_to_string(label_path(dir, name)).ok()?;
    let label = text.lines().next().unwrap_or("").trim().to_string();
    (!label.is_empty()).then_some(label)
}

/// Set or clear a snapshot's name. An empty name removes the file rather than
/// leaving a blank one behind.
pub fn set_label(root: &Path, id: &str, name: &str, label: Option<&str>, trashed: bool) -> Result<()> {
    anyhow::ensure!(safe_name(name), "无效的历史版本文件名");
    let dir = base(root, trashed).join(id);
    let path = label_path(&dir, name);
    let text: String = label.unwrap_or("").lines().next().unwrap_or("").trim().chars().take(MAX_LABEL).collect();
    if text.is_empty() {
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    } else {
        std::fs::create_dir_all(&dir)?;
        crate::store::write_atomic(&path, text.as_bytes())
    }
}

/// `<13-digit ms>-<8 hex>.xml|md`; the fixed-width stamp sorts by time.
pub fn stamp(name: &str) -> Option<u64> { name.split_once('-')?.0.parse().ok() }

fn safe_name(name: &str) -> bool {
    !name.contains('/') && !name.contains('\\') && !name.contains("..")
        && stamp(name).is_some()
        && (name.ends_with(".xml") || name.ends_with(".md"))
}

/// A label file moves and dies with its snapshot.
fn is_label(name: &str) -> bool { name.ends_with(".name") && safe_name(name.trim_end_matches(".name")) }

/// Newest first. A missing directory is simply an empty history.
pub fn list(root: &Path, id: &str, trashed: bool) -> Vec<Version> {
    let mut out = vec![];
    let dir = base(root, trashed).join(id);
    let Ok(entries) = std::fs::read_dir(&dir) else { return out };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(ms) = stamp(&name) else { continue };
        if !safe_name(&name) { continue }
        let size = e.metadata().map_or(0, |m| m.len());
        let label = label_of(&dir, &name);
        out.push(Version { name, ms, size, label });
    }
    out.sort_by(|a, b| b.name.cmp(&a.name));
    out
}

pub fn read(root: &Path, id: &str, name: &str, trashed: bool) -> Result<String> {
    anyhow::ensure!(safe_name(name), "无效的历史版本文件名");
    std::fs::read_to_string(base(root, trashed).join(id).join(name))
        .with_context(|| format!("读取历史版本 {name} 失败"))
}

pub fn remove(root: &Path, id: &str, name: &str, trashed: bool) -> Result<()> {
    anyhow::ensure!(safe_name(name), "无效的历史版本文件名");
    let dir = base(root, trashed).join(id);
    let p = dir.join(name);
    let _ = std::fs::remove_file(label_path(&dir, name));
    match std::fs::remove_file(&p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Save `previous` as one version. The caller treats every error as a
/// warning: a history failure must never fail the document's own save.
pub fn snapshot(root: &Path, id: &str, ext: &str, previous: &str, ms: u64, limit: u32, force: bool) -> Result<()> {
    if previous.is_empty() || (ext != "xml" && ext != "md") { return Ok(()) }
    let mut versions = list(root, id, false);
    if let Some(last) = versions.first() {
        // The same text never adds a snapshot, forced or not: 保存当前版本
        // twice in a row leaves one row.
        if read(root, id, &last.name, false).is_ok_and(|s| s == previous) { return Ok(()) }
        // Autosave fires on a 500ms debounce; the interval keeps a long
        // editing session from filling the list.
        if !force && ms.saturating_sub(last.ms) < MIN_INTERVAL * 1000 { return Ok(()) }
    }
    let dir = history_root(root).join(id);
    std::fs::create_dir_all(&dir)?;
    let name = format!("{ms:013}-{}.{ext}", &uuid::Uuid::new_v4().to_string()[..8]);
    crate::store::write_atomic(&dir.join(&name), previous.as_bytes())?;
    versions.insert(0, Version { name, ms, size: previous.len() as u64, label: None });
    trim(root, id, &mut versions, limit);
    trim_library(root);
    Ok(())
}

fn trim(root: &Path, id: &str, versions: &mut Vec<Version>, limit: u32) {
    let keep = clamp_limit(limit) as usize;
    if versions.len() <= keep { return }
    let dir = history_root(root).join(id);
    for v in versions.drain(keep..) {
        let _ = std::fs::remove_file(label_path(&dir, &v.name));
        let _ = std::fs::remove_file(dir.join(&v.name));
    }
    if versions.is_empty() { let _ = std::fs::remove_dir(&dir); }
}

/// Global ceiling: drop the oldest snapshots across all documents until the
/// library fits again. Per-document limits are applied first, so a single
/// long note cannot starve the rest.
fn trim_library(root: &Path) {
    let mut all: Vec<(u64, u64, PathBuf)> = vec![];
    let Ok(dirs) = std::fs::read_dir(history_root(root)) else { return };
    for d in dirs.flatten() {
        let Ok(files) = std::fs::read_dir(d.path()) else { continue };
        for f in files.flatten() {
            let name = f.file_name().to_string_lossy().into_owned();
            let Some(ms) = stamp(&name) else { continue };
            if !safe_name(&name) { continue }
            let size = f.metadata().map_or(0, |m| m.len());
            all.push((ms, size, f.path()));
        }
    }
    let total: u64 = all.iter().map(|(_, n, _)| *n).sum();
    let mut over = total.saturating_sub(LIBRARY_BUDGET);
    if over == 0 { return }
    all.sort_by_key(|(ms, _, _)| *ms);
    for (_, size, path) in all {
        if over == 0 { break }
        let label = PathBuf::from(format!("{}.name", path.display()));
        let _ = std::fs::remove_file(label);
        if std::fs::remove_file(&path).is_ok() { over = over.saturating_sub(size) }
    }
}

/// Follow the document into the 回收站, then back, then away for good.
/// Files are moved one by one so a pre-existing directory is never replaced.
fn move_dir(src: &Path, dst: &Path) {
    if !src.is_dir() { return }
    if std::fs::create_dir_all(dst).is_err() { return }
    let Ok(entries) = std::fs::read_dir(src) else { return };
    for e in entries.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if safe_name(&name) || is_label(&name) { let _ = std::fs::rename(e.path(), dst.join(name.as_ref())); }
    }
    let _ = std::fs::remove_dir(src);
}

pub fn move_to_trash(root: &Path, id: &str) {
    move_dir(&history_root(root).join(id), &trash_root(root).join(id));
}

pub fn restore_from_trash(root: &Path, id: &str) {
    move_dir(&trash_root(root).join(id), &history_root(root).join(id));
}

pub fn purge(root: &Path, id: &str) {
    let dir = trash_root(root).join(id);
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    for e in entries.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if safe_name(&name) || is_label(&name) { let _ = std::fs::remove_file(e.path()); }
    }
    let _ = std::fs::remove_dir(&dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> PathBuf {
        let d = std::env::temp_dir().join(format!("quiet-versions-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn snapshots_are_ordered_deduplicated_and_capped() {
        let root = lib();
        let base = 1_700_000_000_000;
        for i in 0..25 {
            let text = format!("body {i}");
            snapshot(&root, "a", "xml", &text, base + i * MIN_INTERVAL * 1000, 20, true).unwrap();
        }
        let v = list(&root, "a", false);
        assert_eq!(v.len(), 20);
        assert_eq!(read(&root, "a", &v[0].name, false).unwrap(), "body 24");
        assert_eq!(read(&root, "a", &v[19].name, false).unwrap(), "body 5");
        // Same text again: no new file, forced or not.
        snapshot(&root, "a", "xml", "body 24", base + 25 * MIN_INTERVAL * 1000, 20, false).unwrap();
        snapshot(&root, "a", "xml", "body 24", base + 26 * MIN_INTERVAL * 1000, 20, true).unwrap();
        assert_eq!(list(&root, "a", false).len(), 20);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn autosave_throttles_within_the_interval() {
        let root = lib();
        let ms = 1_700_000_000_000;
        snapshot(&root, "a", "xml", "one", ms, 20, false).unwrap();
        snapshot(&root, "a", "xml", "two", ms + 1_000, 20, false).unwrap();
        assert_eq!(list(&root, "a", false).len(), 1);
        snapshot(&root, "a", "xml", "three", ms + MIN_INTERVAL * 1000, 20, false).unwrap();
        assert_eq!(list(&root, "a", false).len(), 2);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn history_follows_the_document_through_the_trash() {
        let root = lib();
        snapshot(&root, "a", "xml", "one", 1_700_000_000_000, 20, true).unwrap();
        move_to_trash(&root, "a");
        assert!(list(&root, "a", false).is_empty());
        assert_eq!(list(&root, "a", true).len(), 1);
        restore_from_trash(&root, "a");
        assert_eq!(list(&root, "a", false).len(), 1);
        move_to_trash(&root, "a");
        purge(&root, "a");
        assert!(list(&root, "a", true).is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn names_are_validated_and_limits_clamped() {
        assert!(safe_name("1700000000000-abcd1234.xml"));
        assert!(!safe_name("../index.json"));
        assert!(!safe_name("1700000000000-abcd1234.txt"));
        assert_eq!(clamp_limit(0), 1);
        assert_eq!(clamp_limit(99), 30);
    }

    #[test]
    fn labels_stick_to_their_snapshot_and_leave_with_it() {
        let root = lib();
        let base = 1_700_000_000_000;
        snapshot(&root, "a", "xml", "one", base, 2, true).unwrap();
        let v = list(&root, "a", false);
        assert_eq!(v[0].label, None);
        set_label(&root, "a", &v[0].name, Some("投稿前"), false).unwrap();
        assert_eq!(list(&root, "a", false)[0].label.as_deref(), Some("投稿前"));
        // The label is not a snapshot of its own.
        assert_eq!(list(&root, "a", false).len(), 1);
        // An empty name clears it.
        set_label(&root, "a", &v[0].name, Some("  "), false).unwrap();
        assert_eq!(list(&root, "a", false)[0].label, None);
        // Trimmed away: the label file goes with the snapshot.
        set_label(&root, "a", &v[0].name, Some("旧稿"), false).unwrap();
        snapshot(&root, "a", "xml", "two", base + 1_000_000, 2, true).unwrap();
        snapshot(&root, "a", "xml", "three", base + 2_000_000, 2, true).unwrap();
        let v = list(&root, "a", false);
        assert_eq!(v.len(), 2);
        assert!(v.iter().all(|v| v.label.is_none()), "the oldest snapshot took its label along");
        // Trash: both the snapshot and its label follow the document.
        set_label(&root, "a", &v[0].name, Some("最新"), false).unwrap();
        move_to_trash(&root, "a");
        assert_eq!(list(&root, "a", true)[0].label.as_deref(), Some("最新"));
        restore_from_trash(&root, "a");
        assert_eq!(list(&root, "a", false)[0].label.as_deref(), Some("最新"));
        move_to_trash(&root, "a");
        purge(&root, "a");
        assert!(list(&root, "a", true).is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
