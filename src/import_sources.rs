//! 从其他应用导入: the apps notes come in from and the folders of theirs found on this
//! computer. The welcome pages (`onboarding`) and the 从其他应用导入… panel (`transfer`)
//! both list from here; each source says what to pick and which import brings it in.

use crate::settings::tr;
use crate::transfer::ImportKind;
use fdl_core::vfs::{KindExt as _, PathExt as _};
use std::path::{Path, PathBuf};

/// An app the panel offers, in its order. Notion is not one yet: its export's
/// names, nesting, links and databases need their own import (see the report of
/// feat/import-from-apps).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Source { Obsidian, SiYuan, Typora, Markdown }

impl Source {
    pub(crate) const ALL: [Source; 4] = [Source::Obsidian, Source::SiYuan, Source::Typora, Source::Markdown];
    pub(crate) fn name(self) -> &'static str {
        match self { Source::Obsidian => "Obsidian", Source::SiYuan => tr("思源笔记", "SiYuan"), Source::Typora => "Typora", Source::Markdown => tr("其他 Markdown 文件夹", "Other Markdown Folder") }
    }
    /// What to pick, in one line.
    pub(crate) fn hint(self) -> &'static str {
        match self {
            Source::Obsidian => tr("选择库文件夹", "Choose the vault folder"),
            Source::SiYuan => tr("选择工作空间、笔记本文件夹或 .sy.zip", "Choose a workspace, a notebook folder or a .sy.zip"),
            Source::Typora => tr("选择放 Markdown 的文件夹", "Choose the folder of your Markdown files"),
            Source::Markdown => tr("选择文件夹", "Choose a folder"),
        }
    }
    /// Every source is a folder picked on this computer: one folder glyph for all (the maintainer, 2026-11-25).
    pub(crate) fn icon(self) -> &'static str { "folder" }
    /// The import that brings it in: 思源笔记 its own, the rest a folder of Markdown.
    pub(crate) fn kind(self) -> ImportKind { if self == Source::SiYuan { ImportKind::SiYuan } else { ImportKind::Folder } }
    /// Short: a stable name for ids and scripts.
    pub(crate) fn key(self) -> &'static str { match self { Source::Obsidian => "obsidian", Source::SiYuan => "siyuan", Source::Typora => "typora", Source::Markdown => "markdown" } }
}

/// Notes another app keeps on this computer (`discover`).
#[derive(Clone)]
pub(crate) struct Found {
    /// The app, by its own name (`SIYUAN` for 思源笔记).
    pub(crate) app: &'static str,
    /// The name it is imported under.
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    /// Markdown files inside (思源: its documents).
    pub(crate) count: usize,
}

impl Found {
    /// 思源笔记 · 我的工作空间, Obsidian · Thesis; just the app when the folder goes by its name.
    pub(crate) fn title(&self) -> String {
        let app = if self.app == SIYUAN { tr("思源笔记", "SiYuan") } else { self.app };
        if self.app == "Markdown" || self.name == self.app { app.to_string() } else { format!("{} · {}", app, self.name) }
    }
    /// Where it is and how much is in it: iCloud 云盘 · 42 篇 Markdown.
    pub(crate) fn desc(&self) -> String {
        if self.app == SIYUAN { crate::localized_count!(self.count, "{} · {} 篇思源笔记文档", "{} · {} SiYuan document", "{} · {} SiYuan documents", where_short(&self.path), self.count) }
        else { crate::localized_count!(self.count, "{} · {} 篇 Markdown", "{} · {} Markdown file", "{} · {} Markdown files", where_short(&self.path), self.count) }
    }
    /// How it comes in: a 思源笔记 workspace by its own import, anything else as a folder.
    pub(crate) fn source(&self) -> Source { if self.app == SIYUAN { Source::SiYuan } else if self.app == "Obsidian" { Source::Obsidian } else if self.app == "Typora" { Source::Typora } else { Source::Markdown } }
}

/// Markdown files under `dir`, without hidden folders, up to a budget of entries.
fn count_md(dir: &Path, depth: usize, budget: &mut usize) -> usize {
    let Ok(rd) = fdl_core::vfs::read_dir(dir) else { return 0 };
    let mut n = 0;
    for e in rd.flatten() {
        if *budget == 0 { break }
        *budget -= 1;
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "node_modules" { continue }
        let Ok(ft) = e.file_type() else { continue };
        if ft.fs_is_dir() && depth > 0 { n += count_md(&e.path(), depth - 1, budget); }
        else if ft.fs_is_file() && crate::local_files::is_markdown_path(&e.path()) { n += 1; }
    }
    n
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    fdl_core::vfs::read_dir(dir).map(|rd| rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.fs_is_dir()) && !e.file_name().to_string_lossy().starts_with('.')).map(|e| e.path()).collect()).unwrap_or_default()
}

/// Notes other apps keep on this computer, read only and off the main
/// thread. Only an app's own places are looked at (never the user's other
/// folders), and an app shows only when its place really holds notes.
/// Only folders that already hold Markdown: apps that keep notes in their
/// own format (Bear, Ulysses…) are not offered.
/// `skip`: folders it must not offer, nor anything inside or around them (the
/// library being imported into, top-level folders already open).
pub(crate) fn discover(skip: &[PathBuf]) -> Vec<Found> {
    let home = dirs::home_dir().unwrap_or_default();
    let mobile = home.join("Library/Mobile Documents");
    let mut candidates: Vec<(&'static str, PathBuf)> = vec![];
    // Obsidian lists its vaults in obsidian.json (macOS, Windows and Linux alike), and keeps iCloud vaults in its container.
    if let Some(cfg) = dirs::config_dir().and_then(|d| fdl_core::vfs::read(d.join("obsidian/obsidian.json")).ok())
        && let Ok(v) = serde_json::from_slice::<serde_json::Value>(&cfg)
        && let Some(vaults) = v.get("vaults").and_then(|v| v.as_object()) {
        for vault in vaults.values() {
            if let Some(p) = vault.get("path").and_then(|p| p.as_str()) { candidates.push(("Obsidian", PathBuf::from(p))); }
        }
    }
    for d in subdirs(&mobile.join("iCloud~md~obsidian/Documents")) { candidates.push(("Obsidian", d)); }
    // Logseq names each graph's folder in ~/.logseq/graphs: `logseq_local_++Users++me++notes.transit`.
    for g in fdl_core::vfs::read_dir(home.join(".logseq/graphs")).into_iter().flatten().flatten() {
        let name = g.file_name().to_string_lossy().into_owned();
        if let Some(p) = name.strip_prefix("logseq_local_").and_then(|n| n.strip_suffix(".transit")) { candidates.push(("Logseq", PathBuf::from(p.replace("++", "/")))); }
    }
    for d in subdirs(&mobile.join("iCloud~com~logseq~logseq/Documents")) { candidates.push(("Logseq", d)); }
    candidates.push(("iA Writer", mobile.join("27N4MQEA55~pro~writer/Documents")));
    candidates.push(("Typora", mobile.join("iCloud~abnerworks~Typora/Documents")));
    // 思源笔记's workspaces: the default one, and those its workspace.json lists. Not Markdown: always imported as copies.
    candidates.push((SIYUAN, home.join("SiYuan")));
    if let Ok(list) = fdl_core::vfs::read(home.join(".config/siyuan/workspace.json")) && let Ok(v) = serde_json::from_slice::<Vec<String>>(&list) {
        for p in v { candidates.push((SIYUAN, PathBuf::from(p))); }
    }
    let mut out: Vec<Found> = vec![];
    for (app, path) in candidates {
        if !path.fs_is_dir() || skip.iter().any(|s| path.starts_with(s) || s.starts_with(&path)) { continue }
        let key = fdl_core::vfs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if out.iter().any(|f| fdl_core::vfs::canonicalize(&f.path).unwrap_or_else(|_| f.path.clone()) == key) { continue }
        let mut budget = 20_000;
        let count = if app == SIYUAN { if fdl_core::siyuan::recognizes(&path) { fdl_core::siyuan::count_docs(&path, budget) } else { 0 } } else { count_md(&path, 6, &mut budget) };
        if count == 0 { continue }
        let name = found_name(app, &path, &mobile);
        out.push(Found { app, name, path, count });
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.count));
    out
}

/// `Found::app` for a 思源笔记 workspace, shown as 思源笔记.
pub(crate) const SIYUAN: &str = "SiYuan";

/// The name a found folder shows and is imported under: its own, except an
/// app's iCloud folder, which is just "Documents" and goes by the app's name.
fn found_name(app: &str, path: &Path, mobile: &Path) -> String {
    if path.starts_with(mobile) && path.file_name().is_some_and(|n| n == "Documents") { return app.to_string() }
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Where a found folder lives, in a few words: its app's iCloud container
/// reads as iCloud 云盘, anything else as its short path.
fn where_short(p: &Path) -> String {
    let mobile = dirs::home_dir().unwrap_or_default().join("Library/Mobile Documents");
    if p.starts_with(&mobile) { tr("iCloud 云盘", "iCloud Drive").to_string() } else { crate::onboarding::short_path(p) }
}

#[cfg(test)]
mod tests {
    use super::{found_name, Found, Source, SIYUAN};
    use std::path::{Path, PathBuf};

    #[test]
    fn icloud_app_folders_go_by_the_app() {
        let m = Path::new("/Users/me/Library/Mobile Documents");
        assert_eq!(found_name("iA Writer", &m.join("27N4MQEA55~pro~writer/Documents"), m), "iA Writer");
        assert_eq!(found_name("Typora", &m.join("iCloud~abnerworks~Typora/Documents"), m), "Typora");
        assert_eq!(found_name("Obsidian", &m.join("iCloud~md~obsidian/Documents/Thesis"), m), "Thesis");
        assert_eq!(found_name("Obsidian", Path::new("/Users/me/Documents"), m), "Documents");
    }

    /// 思源笔记 comes in by its own import, everything else as a folder of Markdown;
    /// a found folder by the import of its app.
    #[test]
    fn each_source_names_its_import() {
        use crate::transfer::ImportKind;
        assert_eq!(Source::ALL.iter().filter(|s| matches!(s.kind(), ImportKind::SiYuan)).collect::<Vec<_>>(), [&Source::SiYuan]);
        let found = |app| Found { app, name: "x".into(), path: PathBuf::from("/x"), count: 1 };
        assert_eq!(found(SIYUAN).source(), Source::SiYuan);
        assert_eq!(found("Obsidian").source(), Source::Obsidian);
        assert_eq!(found("Logseq").source(), Source::Markdown);
        assert_eq!(found("Obsidian").title(), "Obsidian · x");
    }
}
