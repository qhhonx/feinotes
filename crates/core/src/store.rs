//! On-disk library (FORMAT 4): the folders on disk are the library's folders
//! and each document is a file named after its title (`工作/周报.xml`,
//! `随手记.md`). What the files cannot say (pinned, order …) is kept per
//! relative path in `.quiet/index.json`, a cache that can be lost; state that
//! belongs to this computer only (recently opened, window sizes, 本机文件)
//! is kept outside the library.

use crate::docxml;
use crate::model::Document;
use anyhow::{Context, Result};
use std::io::Write;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;
use std::path::{Path, PathBuf};
use crate::time::now;
use std::time::UNIX_EPOCH;
#[path = "file_history.rs"] mod file_history;
pub use file_history::{FileState, FileDelta, FileChanges};
#[path = "layout.rs"] mod layout;
pub use layout::{file_name_for, relative, resolve};
use layout::{parent_rel, base_name, stem_of, join_rel};
#[path = "migrate.rs"] mod migrate;
pub use migrate::{MigrationState, ConsentReason, DeviceHint, MigrationReport, RestoreReport, SyncService, sync_of, set_ubiquity_probe};

static SESSION_ROOT: OnceLock<PathBuf> = OnceLock::new();
#[cfg(feature = "sqlite-store")]
static SESSION_SQLITE: OnceLock<bool> = OnceLock::new();
#[cfg(feature = "sqlite-store")]
mod sqlite;
#[cfg(feature = "sqlite-store")]
pub mod blocks;
#[cfg(feature = "sqlite-store")]
pub use sqlite::SQLITE_FILE;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Meta {
    /// A folder, by its name on disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_order: Option<u64>,
    pub created: u64,
    pub modified: u64,
    #[serde(default)]
    pub pinned: bool,
    /// Last time the document was opened, for 最近访问 (this computer only).
    #[serde(default)]
    pub opened: u64,
    /// 页宽设置, remembered per document.
    #[serde(default, skip_serializing_if = "PageWidth::is_standard")]
    pub width: PageWidth,
    /// Catalogue visibility is remembered independently of document headings.
    #[serde(default = "catalogue_closed_default")]
    pub catalogue_closed: bool,
    /// The folder it is in (its id); follows from where the file is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Position among its siblings (milliseconds of the last move); 0 falls
    /// back to `created`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub order: u64,
    /// 收藏时间 (0: not starred).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub starred: u64,
    /// 阅读模式, remembered per document like the page width.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reading: bool,
    /// The 文档信息 row under the title.
    #[serde(default, skip_serializing_if = "InfoRow::is_default")]
    pub info: InfoRow,
    /// 隐藏引用列表 (the "…" menu's 显示引用列表 switch off).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub refs_hidden: bool,
    /// A Markdown file, edited as Markdown.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub markdown: bool,
    /// 本机文件: a Markdown file opened where it is, outside the library.
    /// The library never moves, trashes or deletes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The title of a Markdown file in the library: its file name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Where it is in the library, `/`-separated (empty for a 本机文件). In
    /// the 回收站: where it was.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rel: String,
}

impl Meta {
    /// 本机文件: outside the library's tree.
    pub fn local(&self) -> bool { self.path.is_some() }
}

fn catalogue_closed_default() -> bool { true }

impl Default for Meta {
    fn default() -> Self {
        Self {
            folder: None, root_order: None, created: 0, modified: 0, pinned: false, opened: 0,
            width: PageWidth::default(), catalogue_closed: true,
            parent: None, order: 0, starred: 0, reading: false,
            info: InfoRow::default(), refs_hidden: false,
            markdown: false, path: None, name: None, rel: String::new(),
        }
    }
}

/// The 文档信息 row under the title: which items it shows (选择要展示的文档信息)
/// and whether it is hidden altogether (隐藏文档信息).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct InfoRow {
    pub hidden: bool,
    pub created: bool,
    pub modified: bool,
}

impl Default for InfoRow {
    fn default() -> Self {
        InfoRow { hidden: false, created: false, modified: true }
    }
}

impl InfoRow {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

impl Meta {
    /// Where a page or folder sits among its siblings: the sidebar's order, the
    /// numbers a drop writes and Home's manual order all use it. Those never
    /// placed by hand come first, most recently changed first; then the placed
    /// ones in their order. Callers break remaining ties by id.
    pub fn place(&self) -> (bool, u64, std::cmp::Reverse<u64>) {
        let placed = if self.parent.is_none() { self.root_order.unwrap_or(0) } else { self.order };
        (placed > 0, placed, std::cmp::Reverse(self.modified))
    }
}

/// Three page widths: 默认 (820px column), 较宽 (1020px), 全宽.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageWidth {
    #[default]
    Standard,
    Wide,
    Full,
}

impl PageWidth {
    fn is_standard(&self) -> bool {
        *self == PageWidth::Standard
    }

    /// Max width of the content column; `None` fills the page.
    pub fn max(self) -> Option<f32> {
        match self {
            PageWidth::Standard => Some(820.),
            PageWidth::Wide => Some(1020.),
            PageWidth::Full => None,
        }
    }
}

/// A new, private editor session. Existing documents never enter this lifecycle.
pub struct NewNoteDraft {
    pub seed: Document,
    pub parent: Option<String>,
}

impl NewNoteDraft {
    pub fn new(parent: Option<String>) -> Self { Self { seed: Document::default(), parent } }
    pub fn untouched(&self, core: &crate::editor::core::Core) -> bool {
        core.generation == 0 && core.doc == self.seed
    }
    /// Seed the normal store with the same block IDs so its first autosave retains undo history.
    pub fn materialize(&self, store: &mut Store) -> Result<String> {
        store.create_from_in(&self.seed, self.parent.as_deref())
    }
}

/// Window-level UI state: `.quiet/settings.json`, except the window sizes,
/// which belong to this computer.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quick_note_size: Option<[f32; 2]>,
    /// Dragged sidebar width; `None` is the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_width: Option<f32>,
    /// 隐藏引用列表's 不再提示.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub refs_hide_no_ask: bool,
    /// 主页: list instead of grid, and the sort order ("visited" …).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub home_list: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_sort: Option<String>,
    #[serde(default)]
    pub home_folders_first: bool,
    #[serde(default)]
    pub home_manual_initialized: bool,
    /// Main window size, remembered like the quick note window's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_size: Option<[f32; 2]>,
    /// 阅读位置: the list offset each document was left at, so switching back
    /// returns to the same place. Kept out of `index.json`: it is window
    /// state, not document metadata.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub scroll_top: HashMap<String, [f32; 2]>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub selections: HashMap<String, crate::editor::core::ReadingSelection>,
    /// 版本历史: snapshots kept per document (1–30). None is the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_limit: Option<u32>,
}

impl Settings {
    pub fn history_limit(&self) -> u32 { crate::versions::clamp_limit(self.history_limit.unwrap_or(crate::versions::DEFAULT_LIMIT)) }
    pub fn set_history_limit(&mut self, n: u32) { self.history_limit = Some(crate::versions::clamp_limit(n)); }
}

/// A document in the 回收站, as kept in `.quiet/trash/index.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Trashed {
    pub meta: Meta,
    pub deleted: u64,
    /// The page whose deletion took this one along (`None`: deleted itself).
    /// Such pages are listed, restored and purged with that page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
}

/// A row of the 回收站 page: a deleted page and how many went with it.
#[derive(Clone, Debug)]
pub struct TrashEntry {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub deleted: u64,
    /// Where it was: its parent page, `None` for the top of 我的文档库.
    pub parent: Option<String>,
}

/// The 回收站 is emptied after 30 days.
pub const TRASH_DAYS: u64 = 30;

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: String,
    pub meta: Meta,
    pub title: String,
    /// 文档图标 emoji.
    pub icon: Option<String>,
    /// Plain text body, used for search and the list snippet.
    pub body: std::sync::Arc<str>,
    /// Its 日期提醒, for the system notifications.
    pub reminders: Vec<crate::model::InlineReminder>,
    /// Its @文档 mentions, for the 反向引用 of the documents they point at.
    pub links: Vec<crate::model::DocLink>,
}

pub struct Store {
    #[cfg(feature = "sqlite-store")]
    sqlite: Option<sqlite::Library>,
    file_changes: Vec<std::sync::Weak<std::sync::Mutex<Vec<FileDelta>>>>,
    loaded: std::cell::RefCell<HashMap<String, String>>,
    /// Library images and attachments copied beside each 本机文件 this
    /// session, so autosaves reuse the copy instead of making another.
    copied: std::cell::RefCell<HashMap<String, HashMap<PathBuf, String>>>,
    /// What `list` parsed from each file, kept while its mtime and size stay
    /// the same: re-reading every document on each window activation stalled
    /// the main thread for seconds on big or iCloud-evicted libraries.
    parsed: HashMap<String, (std::time::SystemTime, u64, Parsed)>,
    root: PathBuf,
    /// Every document and folder by its id. Ids are handed out per library
    /// and remembered in `.quiet/index.json`, but nothing in a file refers
    /// to them: links are relative paths.
    index: HashMap<String, Meta>,
    trash: HashMap<String, Trashed>,
    /// The old layout (FORMAT 3: `docs/<id>.xml`, folders only in the index,
    /// the app's files at the top), kept until the user agrees to the move.
    flat: bool,
    /// Problems found while opening the library (a damaged index recovered
    /// from its backup …), for the window to tell the user once.
    pub warnings: Vec<String>,
    /// What `open` moved to the new layout this launch, for 更新日志.
    pub migrated: Option<MigrationReport>,
    batch: Batch,
}

/// The parts of an `Entry` that come from the document file.
#[derive(Clone)]
struct Parsed { title: String, icon: Option<String>, body: std::sync::Arc<str>, reminders: Vec<crate::model::InlineReminder>, links: Vec<crate::model::DocLink> }

/// `Store::parsed` kept between launches (#170): reading every document again
/// on launch took ~0.45 s a file on Windows, where each read is scanned. By
/// file path, so a cache never lends one file's text to another; the stamps
/// are this machine's mtimes, so it lives in the cache folder, not in the
/// (possibly synced) library. Bump `PARSED_CACHE` when what `list` takes from
/// a file changes; a release discards it anyway.
const PARSED_CACHE: u32 = 1;

#[derive(Serialize, Deserialize)]
struct ParsedCache { version: String, docs: HashMap<PathBuf, CachedDoc> }

#[derive(Serialize, Deserialize)]
struct CachedDoc {
    modified: std::time::SystemTime,
    len: u64,
    title: String,
    icon: Option<String>,
    body: String,
    reminders: Vec<(i64, i64, bool, bool)>,
    links: Vec<(String, usize, Vec<(String, Option<String>)>)>,
}

fn parsed_cache_version() -> String { format!("{}-{PARSED_CACHE}", env!("CARGO_PKG_VERSION")) }

/// Beside the other things this computer keeps about `root`, in the cache folder.
fn parsed_cache_path(root: &Path) -> PathBuf {
    let tmp = std::env::temp_dir();
    let base = if cfg!(test) || root.starts_with(&tmp) { tmp.join("quiet-md-local") } else { dirs::cache_dir().unwrap_or(tmp).join("quiet-md") };
    base.join("list").join(format!("{}.json", library_key(root)))
}

/// One write to the indexes: several copies of the app may share a library
/// (builds tested side by side), so this is a short mutex, not a
/// single-instance guard. Released when the write is done.
/// `None`: the lock a batch already holds.
struct WriteLock(Option<std::fs::File>);

impl Drop for WriteLock {
    fn drop(&mut self) { if let Some(f) = &self.0 { let _ = f.unlock(); } }
}

#[cfg(feature = "sqlite-store")]
fn lock_root(root: &Path) -> Result<WriteLock> {
    std::fs::create_dir_all(root)?;
    let file = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(root.join(".lock"))?;
    file.lock()?;
    Ok(WriteLock(Some(file)))
}

/// A batch of writes (`Store::batch`): the lock is held throughout and the
/// index is written once at the end, not once per document.
#[derive(Default)]
struct Batch { lock: Option<WriteLock>, depth: u32, index: std::cell::Cell<bool>, dirs: std::cell::RefCell<Vec<PathBuf>> }

/// Version of the on-disk layout, kept in `.quiet/format.json`. Bump it when
/// a change would make older builds misread the library. 4: real folders,
/// files named by title, internal data in `.quiet/`.
pub const FORMAT: u32 = 4;
/// The hidden folder holding the library's own data.
pub const INTERNAL: &str = ".quiet";
/// The hidden folder holding pasted and inserted images and attachments.
pub const ASSETS: &str = ".assets";

#[path = "folders.rs"]
mod folders;

#[derive(Serialize, Deserialize)]
struct FormatMarker {
    format: u32,
}

/// An entry of `.quiet/index.json`, keyed by the path relative to the
/// library (folders end in `/`): what the files cannot say.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Attached {
    #[serde(default)]
    id: String,
    #[serde(default)]
    created: u64,
    #[serde(default)]
    modified: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pinned: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    starred: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    order: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    root_order: Option<u64>,
    #[serde(default, skip_serializing_if = "PageWidth::is_standard")]
    width: PageWidth,
    #[serde(default = "catalogue_closed_default", skip_serializing_if = "Clone::clone")]
    catalogue_closed: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    reading: bool,
    #[serde(default, skip_serializing_if = "InfoRow::is_default")]
    info: InfoRow,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    refs_hidden: bool,
}

type DiskIndex = BTreeMap<String, Attached>;

impl Attached {
    fn of(id: &str, m: &Meta) -> Self {
        Attached { id: id.to_string(), created: m.created, modified: m.modified, pinned: m.pinned, starred: m.starred, order: m.order, root_order: m.root_order,
            width: m.width, catalogue_closed: m.catalogue_closed, reading: m.reading, info: m.info, refs_hidden: m.refs_hidden }
    }
    fn meta(&self) -> Meta {
        Meta { created: self.created, modified: self.modified, pinned: self.pinned, starred: self.starred, order: self.order, root_order: self.root_order,
            width: self.width, catalogue_closed: self.catalogue_closed, reading: self.reading, info: self.info, refs_hidden: self.refs_hidden, ..Default::default() }
    }
}

/// What belongs to this computer, not to the library (which may be synced):
/// kept per library in the local application data folder.
#[derive(Clone, Default, Serialize, Deserialize)]
struct Local {
    /// Document id → last opened.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    opened: BTreeMap<String, u64>,
    /// 本机文件: their paths are this computer's.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    files: BTreeMap<String, Meta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    quick_note_size: Option<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    window_size: Option<[f32; 2]>,
}

/// `library.json`: where the library is when it isn't the default.
#[derive(Serialize, Deserialize)]
struct Location { path: PathBuf }

/// The library this run uses once first launch has picked it.
static ROOT: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

/// Write `bytes` to `path` through a temporary file, so a crash leaves
/// either the old or the new contents.
pub(crate) fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> Result<()> { write_atomic_then(path, bytes, true) }

/// `write_atomic`; without `sync_dir` the folder's entry is left for the
/// caller to sync (a batch syncs each folder once, before its index).
fn write_atomic_then(path: &std::path::Path, bytes: &[u8], sync_dir: bool) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let tmp = PathBuf::from(tmp);
    let result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)?;
        #[cfg(unix)]
        if sync_dir && let Some(parent) = path.parent() { std::fs::File::open(parent)?.sync_all()?; }
        // Windows has no directory fsync; the rename is as far as it goes.
        #[cfg(not(unix))]
        let _ = sync_dir;
        Ok(())
    })();
    if result.is_err() { let _ = std::fs::remove_file(&tmp); }
    result
}

fn with_suffix(path: &std::path::Path, suffix: &str) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(suffix);
    PathBuf::from(p)
}

/// Read a JSON index written by `save_json`. A file that exists but does not
/// parse is set aside as `<name>.corrupt-<time>` (never overwritten) and the
/// previous version, `<name>.bak`, is used instead.
fn load_json<T: serde::de::DeserializeOwned + Default>(path: &std::path::Path, what: &str, warnings: &mut Vec<String>) -> T {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return T::default(),
        Err(e) => {
            warnings.push(format!("无法读取{what}：{e}"));
            return T::default();
        }
    };
    if let Ok(v) = serde_json::from_slice(&bytes) {
        return v;
    }
    let aside = with_suffix(path, &format!(".corrupt-{}", now()));
    let _ = std::fs::rename(path, &aside);
    let name = aside.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match std::fs::read(with_suffix(path, ".bak")).ok().and_then(|b| serde_json::from_slice(&b).ok()) {
        Some(v) => {
            warnings.push(format!("{what}已损坏，已从备份恢复（原文件保留为 {name}）"));
            v
        }
        None => {
            warnings.push(format!("{what}已损坏且没有可用备份，部分文档的置顶、排序等信息已重置（原文件保留为 {name}）"));
            T::default()
        }
    }
}

/// A JSON file as another copy of the app left it, if it can be read right now.
fn reread<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Option<T> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Atomically replace a JSON index, keeping the version it replaces as
/// `<name>.bak` for `load_json`. Unchanged contents are not rewritten (a
/// synced library would upload every rewrite).
fn save_json<T: Serialize>(path: &std::path::Path, v: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(v)?;
    match std::fs::read(path) {
        Ok(old) if old == bytes => return Ok(()),
        Ok(old) => write_atomic(&with_suffix(path, ".bak"), &old)?,
        Err(_) => {}
    }
    write_atomic(path, &bytes)
}

/// Library-owned assets have the same reference on every replica.
pub fn asset_source(path: &std::path::Path) -> String { asset_source_in(&Store::root_dir(), path) }

pub fn asset_source_in(root: &std::path::Path, path: &std::path::Path) -> String {
    #[cfg(not(feature = "sqlite-store"))]
    { let _ = root; path.to_string_lossy().into_owned() }
    #[cfg(feature = "sqlite-store")]
    {
        if !root.join(SQLITE_FILE).is_file() { return path.to_string_lossy().into_owned(); }
        path.strip_prefix(root).ok().filter(|path| path.starts_with("assets"))
            .unwrap_or(path).to_string_lossy().into_owned()
    }
}

/// Image path for an image block source (only local files are displayed).
pub fn image_path(src: &str) -> Option<PathBuf> { image_path_in(&Store::root_dir(), src) }

pub fn image_path_in(root: &Path, src: &str) -> Option<PathBuf> {
    let p = PathBuf::from(src.strip_prefix("file://").unwrap_or(src));
    if p.is_absolute() {
        return Some(moved_asset(root, &p).unwrap_or(p));
    }
    // Relative to the library (`.assets/…`); not a URI or a cloud token.
    (!src.is_empty() && !src.contains(':') && src.contains('/')).then(|| root.join(p))
}

/// Images and attachments are kept by absolute path (pasted ones too), so one
/// that is gone may just have moved with the library: `assets/` became
/// `.assets/` in the move to real folders (#213), or the library itself was
/// moved. The same file in this library's images folder, when it is there.
fn moved_asset(root: &Path, p: &Path) -> Option<PathBuf> {
    use std::path::Component;
    if p.exists() { return None; }
    let parts: Vec<Component> = p.components().collect();
    let dir = Store::assets_in(root);
    // The last folder so named that leads to a file; a file may be named `assets` itself.
    (0..parts.len()).rev().filter(|&i| matches!(parts[i].as_os_str().to_str(), Some("assets" | ".assets"))).find_map(|i| {
        let rest = &parts[i + 1..];
        if rest.is_empty() || !rest.iter().all(|c| matches!(c, Component::Normal(_))) { return None; }
        Some(dir.join(rest.iter().collect::<PathBuf>())).filter(|q| q.exists())
    })
}

/// Export only files belonging to this library, including after symlink resolution.
pub fn export_path(src: &str) -> Option<PathBuf> {
    export_path_in(&Store::root_dir(), src)
}

pub fn export_path_in(root: &std::path::Path, src: &str) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;
    let raw = std::path::Path::new(src.strip_prefix("file://").unwrap_or(src));
    let path = if raw.is_absolute() { moved_asset(&root, raw).unwrap_or_else(|| raw.to_path_buf()) } else { root.join(raw) }.canonicalize().ok()?;
    (path.starts_with(root) && path.is_file()).then_some(path)
}

pub const EXPORT_FILE_LIMIT: u64 = 256 * 1024 * 1024;

/// Bound the actual read as well as metadata: files can grow after inspection.
pub fn read_bounded(path: &std::path::Path, limit: u64) -> Result<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    anyhow::ensure!(file.metadata()?.len() <= limit, "文件超过导出大小限制");
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() as u64 <= limit, "文件超过导出大小限制");
    Ok(bytes)
}

/// A short stable name for a library folder, for data kept about it elsewhere.
fn library_key(root: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut hash);
    format!("{:016x}", hash.finish())
}

/// Where this computer keeps what is not `root`'s own: the application data
/// folder, by library; a temporary one for tests and libraries in the
/// temporary folder (scripted runs), which never touch the real one.
fn local_file_for(root: &Path) -> PathBuf {
    let tmp = std::env::temp_dir();
    let base = if cfg!(test) || root.starts_with(&tmp) || tmp.canonicalize().is_ok_and(|t| root.starts_with(t)) { tmp.join("quiet-md-local") }
        else { dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("quiet-md") };
    base.join("libraries").join(format!("{}.json", library_key(root)))
}

/// The id, name and parent of every library entry, from where it is.
fn derive_tree(index: &mut HashMap<String, Meta>) {
    let folders: HashMap<String, String> = index.iter().filter(|(_, m)| m.folder.is_some() && !m.rel.is_empty()).map(|(k, m)| (m.rel.clone(), k.clone())).collect();
    for m in index.values_mut() {
        if m.local() || m.rel.is_empty() { continue; }
        let name = base_name(&m.rel).to_string();
        if m.folder.is_some() { m.folder = Some(name); }
        else {
            m.markdown = name.ends_with(".md");
            m.name = m.markdown.then(|| stem_of(&name).to_string());
        }
        let up = parent_rel(&m.rel);
        m.parent = (!up.is_empty()).then(|| folders.get(up).cloned()).flatten();
    }
}

/// A doc file's extension in the library.
fn ext_of(m: &Meta) -> &'static str { if m.markdown { "md" } else { "xml" } }

impl Store {
    pub fn library_root(&self) -> &std::path::Path { &self.root }

    /// Select the account-owned library before opening any Store.
    pub fn set_session_root(root: PathBuf) -> Result<()> {
        SESSION_ROOT.set(root).map_err(|_| anyhow::anyhow!("Library already selected"))
    }

    /// `QUIET_MD_HOME`, else the folder picked at first launch (`library.json`
    /// outside the library, since everything else lives inside it), else a
    /// folder of its own in the user's Documents, where they can see and back it up.
    pub fn root_dir() -> PathBuf {
        if let Some(root) = SESSION_ROOT.get() { return root.clone(); }

        if let Some(p) = ROOT.read().unwrap().clone() { return p }
        std::env::var_os("QUIET_MD_HOME").map(PathBuf::from)
            .or_else(|| std::fs::read(Self::location_file()).ok().and_then(|b| serde_json::from_slice::<Location>(&b).ok()).map(|l| l.path).filter(|p| p.is_absolute()))
            .unwrap_or_else(Self::default_root)
    }

    pub fn default_root() -> PathBuf {
        dirs::document_dir().or_else(|| dirs::home_dir().map(|h| h.join("Documents"))).unwrap_or_else(|| PathBuf::from(".")).join("Quiet.md")
    }

    /// The library's own data (`.quiet/`): preferences, shortcuts, themes,
    /// the index and the 回收站. Created on demand.
    pub fn internal_dir() -> PathBuf {
        #[cfg(feature = "sqlite-store")]
        if Self::sqlite_session() { return Self::root_dir(); }

        let root = Self::root_dir();
        // The old layout keeps them at the top, where older builds look.
        if Self::flat_at(&root) { return root; }
        let d = root.join(INTERNAL);
        let _ = std::fs::create_dir_all(&d);
        d
    }

    /// Whether `root` is (still) in the old layout.
    pub fn flat_at(root: &Path) -> bool { !root.join(INTERNAL).join("format.json").is_file() && migrate::legacy(root) }

    /// Where images and attachments are in library `root`.
    pub fn assets_in(root: &Path) -> PathBuf {
        #[cfg(feature = "sqlite-store")]
        if root.join(SQLITE_FILE).is_file() { return root.join("assets"); }
 root.join(if Self::flat_at(root) { "assets" } else { ASSETS }) }

    /// What the library's files are, without reading them: changes when a
    /// document or folder is added, removed or renamed (by hand, or a model
    /// writing a note), not when one is saved. Cheap enough to poll.
    pub fn shape_at(root: &Path) -> u64 {
        use std::hash::{Hash, Hasher};
        let found = layout::scan(&if Self::flat_at(root) { root.join("docs") } else { root.to_path_buf() });
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (found.dirs, found.docs, found.pending).hash(&mut h);
        h.finish()
    }

    /// Whether `dir` holds a library, of this layout or an older one.
    pub fn is_library(dir: &Path) -> bool {
        dir.join(INTERNAL).join("format.json").is_file() || migrate::legacy(dir)
    }

    /// The pointer to a library outside Documents; per user, never synced.
    pub fn location_file() -> PathBuf {
        dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("quiet-md").join("library.json")
    }

    /// Use `root` for the rest of this run, and from now on unless
    /// `QUIET_MD_HOME` says otherwise. The default location needs no pointer.
    pub fn set_root_dir(root: PathBuf) -> Result<()> {
        anyhow::ensure!(SESSION_ROOT.get().is_none(), "The account-selected library cannot be changed as a file folder");

        let file = Self::location_file();
        // Test runs (`QUIET_MD_HOME`) never move the real library.
        if std::env::var_os("QUIET_MD_HOME").is_some() {}
        else if root == Self::default_root() { let _ = std::fs::remove_file(&file); }
        else {
            std::fs::create_dir_all(file.parent().unwrap())?;
            write_atomic(&file, &serde_json::to_vec_pretty(&Location { path: root.clone() })?)?;
        }
        *ROOT.write().unwrap() = Some(root);
        Ok(())
    }

    /// Where pasted / inserted images are copied to (`.assets/`).
    pub fn assets_dir() -> PathBuf {
        let d = Self::assets_in(&Self::root_dir());
        let _ = std::fs::create_dir_all(&d);
        d
    }

    /// The library at `root_dir`, moved to the new layout first when that
    /// needs no question (`migration_state` is `Silent`); a library that
    /// needs the user's consent opens as it is, in the old layout.
    pub fn open() -> Result<Self> {
        #[cfg(feature = "sqlite-store")]
        if Self::sqlite_session() { return Self::open_sqlite_at(Self::root_dir()); }

        let root = Self::root_dir();
        Self::prune_backups(&root);
        let mut failed = None;
        let report = match Self::migration_state(&root) {
            MigrationState::Silent => Self::migrate(&root).map_err(|e| failed = Some(e.to_string())).ok(),
            _ => None,
        };
        let mut store = Self::open_at(root)?;
        store.migrated = report;
        store.warnings.extend(failed);
        Ok(store)
    }

    /// Whether the library at `root` needs moving to the new layout, and
    /// whether the user must agree first (it is synced, or used from another
    /// computer, or they went back to the old layout before).
    pub fn migration_state(root: &Path) -> MigrationState { migrate::state(root) }

    /// Move the library at `root` to the new layout (FORMAT 4), or leave it
    /// as it was on failure. The old layout is kept in `.quiet/` as a backup.
    pub fn migrate(root: &Path) -> Result<MigrationReport> {
        let mut warnings = vec![];
        migrate::run(root, &mut warnings)
    }

    /// Go back to the layout from before the move: the backup returns, and what
    /// the new layout holds now is set aside in `.quiet/` (not deleted).
    pub fn restore_backup(root: &Path) -> Result<RestoreReport> { migrate::restore(root) }

    /// Once per launch: count it, and remove a move's backup once it is 30
    /// days old and three launches have passed.
    pub fn prune_backups(root: &Path) { migrate::prune(root) }

    /// The backup of the old layout, while there is one.
    pub fn backup_dir(root: &Path) -> Option<PathBuf> { Some(root.join(INTERNAL).join(migrate::BACKUP)).filter(|p| p.is_dir()) }

    /// Open the library at `root` in the layout it has; never moves it.
    pub fn open_at(root: PathBuf) -> Result<Self> {
        #[cfg(feature = "sqlite-store")]
        if root.join(SQLITE_FILE).is_file() { return Self::open_sqlite_at(root); }

        std::fs::create_dir_all(&root)?;
        let mut warnings = vec![];
        let internal = root.join(INTERNAL);
        let marker = internal.join("format.json");
        let flat = Self::flat_at(&root);
        if flat {
            let format = reread::<FormatMarker>(&root.join("format.json")).map_or(0, |m| m.format);
            anyhow::ensure!(format <= FORMAT, "这个文档库由更新版本的 Quiet.md 创建，请升级后再打开");
            if !root.join("format.json").exists() { write_atomic(&root.join("format.json"), &serde_json::to_vec_pretty(&FormatMarker { format })?)?; }
        } else {
            match std::fs::read(&marker).ok().and_then(|b| serde_json::from_slice::<FormatMarker>(&b).ok()) {
                Some(m) if m.format > FORMAT => anyhow::bail!("这个文档库由更新版本的 Quiet.md 创建，请升级后再打开"),
                // Moved to the new layout; what was left of the old one is set aside.
                Some(_) => if migrate::legacy(&root) { migrate::finish(&root, &mut warnings); },
                None => {
                    std::fs::create_dir_all(&internal)?;
                    write_atomic(&marker, &serde_json::to_vec_pretty(&FormatMarker { format: FORMAT })?)?;
                }
            }
        }
        let base = if flat { root.clone() } else { internal.clone() };
        let trash = load_json(&base.join("trash").join("index.json"), "回收站索引", &mut warnings);
        let mut store = Store {
            #[cfg(feature = "sqlite-store")]
            sqlite: None,
            file_changes: vec![], root, index: HashMap::new(), trash, flat, warnings, migrated: None, loaded: Default::default(), copied: Default::default(), parsed: HashMap::new(), batch: Default::default() };
        if flat {
            store.index = load_json(&base.join("index.json"), "文档索引", &mut store.warnings);
            store.flat_folders()?;
        } else {
            let disk: DiskIndex = load_json(&internal.join("index.json"), "文档索引", &mut store.warnings);
            let local = store.read_local();
            store.index = store.from_disk(&disk, &local);
            store.load_parsed();
        }
        store.restore_recoveries();
        Ok(store)
    }

    /// Where the library keeps its own files: `.quiet/`, the top in the old layout.
    /// 版本历史 lives in `.quiet/` in either layout, so a library that has
    /// not moved to folders yet never grows a visible `history/` folder.
    fn history_base(&self) -> PathBuf { self.root.join(INTERNAL) }
    fn internal(&self) -> PathBuf { if self.flat { self.root.clone() } else { self.root.join(INTERNAL) } }
    fn index_file(&self) -> PathBuf { self.internal().join("index.json") }
    fn trash_index_file(&self) -> PathBuf { self.internal().join("trash").join("index.json") }
    fn local_file(&self) -> PathBuf { local_file_for(&self.root) }

    /// A relative path in the library as a path on disk.
    fn abs(&self, rel: &str) -> PathBuf {
        let mut p = self.root.clone();
        for part in rel.split('/').filter(|s| !s.is_empty()) { p.push(part); }
        p
    }

    fn read_local(&self) -> Local { reread(&self.local_file()).unwrap_or_default() }

    /// The in-memory index from `.quiet/index.json` and this computer's state.
    /// Folders without an entry of their own (made in the Finder) get one.
    fn from_disk(&self, disk: &DiskIndex, local: &Local) -> HashMap<String, Meta> {
        let mut index: HashMap<String, Meta> = HashMap::new();
        let mut folders: HashSet<String> = HashSet::new();
        for (key, a) in disk {
            let folder = key.ends_with('/');
            let rel = key.trim_end_matches('/');
            if rel.is_empty() || rel.split('/').any(|p| p.is_empty() || p.starts_with('.')) || (!folder && !(rel.ends_with(".md") || rel.ends_with(".xml"))) { continue; }
            if folder && !folders.insert(rel.to_string()) { continue; }
            let id = if a.id.is_empty() || index.contains_key(&a.id) || local.files.contains_key(&a.id) { uuid::Uuid::new_v4().to_string() } else { a.id.clone() };
            let mut m = a.meta();
            m.rel = rel.to_string();
            if folder { m.folder = Some(String::new()); }
            m.opened = local.opened.get(&id).copied().unwrap_or(0);
            index.insert(id, m);
        }
        let rels: Vec<String> = index.values().map(|m| m.rel.clone()).collect();
        for rel in rels {
            let mut up = parent_rel(&rel);
            while !up.is_empty() {
                if folders.insert(up.to_string()) {
                    let t = now();
                    index.insert(uuid::Uuid::new_v4().to_string(), Meta { folder: Some(String::new()), rel: up.to_string(), created: t, modified: t, ..Default::default() });
                }
                up = parent_rel(up);
            }
        }
        derive_tree(&mut index);
        for (id, m) in &local.files {
            if index.contains_key(id) || m.path.is_none() { continue; }
            let mut m = m.clone();
            m.opened = local.opened.get(id).copied().unwrap_or(m.opened);
            index.insert(id.clone(), m);
        }
        index
    }

    fn to_disk(&self) -> DiskIndex {
        self.index.iter().filter(|(_, m)| !m.local() && !m.rel.is_empty())
            .map(|(id, m)| (if m.folder.is_some() { format!("{}/", m.rel) } else { m.rel.clone() }, Attached::of(id, m))).collect()
    }

    /// This computer's state as the index has it; window sizes as on disk.
    fn local_state(&self) -> Local {
        let mut local = self.read_local();
        local.opened = self.index.iter().filter(|(_, m)| m.opened > 0).map(|(k, m)| (k.clone(), m.opened)).collect();
        local.files = self.index.iter().filter(|(_, m)| m.local()).map(|(k, m)| (k.clone(), m.clone())).collect();
        local
    }

    fn save_local(&self, local: &Local) -> Result<()> {
        let path = self.local_file();
        let bytes = serde_json::to_vec_pretty(local)?;
        if std::fs::read(&path).is_ok_and(|b| b == bytes) { return Ok(()); }
        std::fs::create_dir_all(path.parent().unwrap())?;
        write_atomic(&path, &bytes)
    }

    fn restore_recoveries(&mut self) {
        if let Ok(entries) = std::fs::read_dir(self.recovery_dir()) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_none_or(|x| x != "xml") { continue; }
                let restored = (|| -> Result<()> {
                    let mut doc = docxml::parse(&std::fs::read_to_string(&path)?)?;
                    doc.title = crate::model::RichText::plain(format!("{} (recovered copy)", doc.title.text));
                    self.create_from(&doc)?;
                    std::fs::remove_file(&path)?;
                    Ok(())
                })();
                self.warnings.push(match restored {
                    Ok(()) => "已恢复上次退出时未保存的文档副本".into(),
                    Err(e) => format!("恢复副本仍保留在 {}：{e}", path.display()),
                });
            }
        }
    }

    fn recovery_dir(&self) -> PathBuf {
        dirs::data_local_dir().unwrap_or_else(std::env::temp_dir)
            .join("quiet-md/recovery").join(library_key(&self.root))
    }

    pub fn recover_later(&self, doc: &Document) -> Result<()> {
        let dir = self.recovery_dir();
        std::fs::create_dir_all(&dir)?;
        write_atomic(&dir.join(format!("{}.xml", uuid::Uuid::new_v4())), docxml::serialize(doc).as_bytes())
    }

    fn path(&self, id: &str) -> PathBuf {
        match self.index.get(id) {
            Some(Meta { path: Some(p), .. }) => PathBuf::from(p),
            m if self.flat => self.root.join("docs").join(format!("{id}.{}", if m.is_some_and(|m| m.markdown) { "md" } else { "xml" })),
            Some(m) if !m.rel.is_empty() => self.abs(&m.rel),
            _ => self.internal().join("missing").join(id),
        }
    }

    /// The file a document is kept in.
    pub fn file_path(&self, id: &str) -> Option<PathBuf> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite_manages(id) { return None; }

        self.index.get(id).filter(|m| m.folder.is_none()).map(|_| self.path(id))
    }

    /// A folder on disk: `None` the library itself. The old flat layout keeps
    /// folders only in its index, so they open as the library.
    pub fn folder_dir(&self, id: Option<&str>) -> Option<PathBuf> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() { return None; }

        match id {
            None => Some(self.root.clone()),
            Some(id) => self.index.get(id).filter(|m| m.folder.is_some()).map(|m| if self.flat { self.root.clone() } else { self.abs(&m.rel) }),
        }
    }

    fn read_body(&self, id: &str) -> Result<String> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite_manages(id) { let d = self.load_sqlite_doc(id, false)?; return Ok(if self.is_markdown(id) { crate::markdown::write_file(&d, &self.sqlite_front(id)?, None) } else { docxml::serialize(&d) }); }
        Ok(std::fs::read_to_string(self.path(id))?)
    }

    pub fn is_markdown(&self, id: &str) -> bool { self.index.get(id).is_some_and(|m| m.markdown) }

    /// The folder of a 本机文件, which its images are relative to.
    fn base(&self, id: &str) -> Option<PathBuf> {
        self.index.get(id)?.path.as_ref().and_then(|p| std::path::Path::new(p).parent()).map(PathBuf::from)
    }

    /// A Markdown document's title: its file name.
    fn markdown_title(&self, id: &str) -> String {
        let Some(m) = self.index.get(id) else { return String::new() };
        match &m.path {
            Some(p) => std::path::Path::new(p).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            None => m.name.clone().unwrap_or_default(),
        }
    }

    /// The folder a document is (or was, in the 回收站) in, which its links are relative to.
    fn doc_dir(&self, id: &str) -> String {
        let rel = self.index.get(id).map(|m| m.rel.as_str()).or_else(|| self.trash.get(id).map(|t| t.meta.rel.as_str())).unwrap_or("");
        parent_rel(rel).to_string()
    }

    /// Mentions as written in a file in `dir` (relative paths) as the ids of
    /// the documents they point at; mentions of nothing in the library stay
    /// as they are, and so do old `doc-id` ones.
    fn links_to_ids(&self, dir: &str, doc: &mut Document) {
        let mut by_rel: Option<HashMap<&str, &str>> = None;
        layout::each_inline(doc, &mut |i| {
            let Some(l) = i.doc_link().cloned() else { return };
            let Some(href) = docxml::doc_link_href(&i.xml) else { return };
            let by_rel = by_rel.get_or_insert_with(|| self.index.iter().filter(|(_, m)| !m.local() && m.folder.is_none()).map(|(k, m)| (m.rel.as_str(), k.as_str())).collect());
            if let Some(id) = layout::resolve(dir, &href).and_then(|rel| by_rel.get(rel.as_str()).copied()) {
                *i = docxml::to_inline(crate::model::InlineKind::DocLink(crate::model::InlineDocLink { id: id.to_string(), title: l.title }));
            }
        });
    }

    /// The other way: mentions of documents in the library (or in the 回收站,
    /// by where they were) as paths relative to `dir`.
    fn links_to_paths(&self, dir: &str, doc: &mut Document) {
        layout::each_inline(doc, &mut |i| {
            let Some(l) = i.doc_link().cloned() else { return };
            if docxml::doc_link_href(&i.xml).is_some() { return; }
            let target = self.index.get(&l.id).or_else(|| self.trash.get(&l.id).map(|t| &t.meta)).filter(|m| !m.local() && m.folder.is_none() && !m.rel.is_empty());
            if let Some(m) = target { i.xml = docxml::doc_link_href_xml(&layout::relative(dir, &m.rel), &l.title); }
        });
    }

    /// `text` (a file's contents) as a document.
    fn parse_doc(&self, id: &str, text: &str) -> Result<Document> {
        if !self.is_markdown(id) {
            let mut doc = docxml::parse(text)?;
            if text.contains("href=") { self.links_to_ids(&self.doc_dir(id), &mut doc); }
            return Ok(doc);
        }
        let mut doc = crate::markdown::parse_file(text, &self.markdown_title(id), self.base(id).as_deref());
        // Images a Markdown file carries inline (#133) become assets, so the
        // page shows them and an export writes them like any other picture.
        crate::data_images::absorb(&mut doc.blocks, &self.root);
        Ok(doc)
    }

    /// `doc` as `id`'s file contents; a Markdown file keeps the front matter it has on disk.
    fn serialize_doc(&self, id: &str, doc: &Document, disk: Option<&str>) -> String { self.serialize_with_copies(id, doc, disk).0 }

    /// `serialize_doc`, and the library files a 本机文件 needs copied beside
    /// it: (source, path relative to its folder).
    fn serialize_with_copies(&self, id: &str, doc: &Document, disk: Option<&str>) -> (String, Vec<(PathBuf, String)>) {
        if !self.is_markdown(id) {
            if !self.flat && !doc.doc_links().is_empty() {
                let mut doc = doc.clone();
                self.links_to_paths(&self.doc_dir(id), &mut doc);
                return (docxml::serialize(&doc), vec![]);
            }
            return (docxml::serialize(doc), vec![]);
        }
        let front = disk.map(|d| crate::markdown::front_matter(d.trim_start_matches('\u{feff}')).0).unwrap_or_default();
        let base = self.base(id);
        let name = base.is_some().then(|| self.markdown_title(id));
        let copied = self.copied.borrow();
        let out = crate::markdown::write_file_with(doc, front, &crate::markdown::Options {
            library_root: Some(&self.root), base: base.as_deref(), name: name.as_deref(), copied: copied.get(id), original: disk, ..Default::default()
        });
        let bom = if disk.is_some_and(|d| d.starts_with('\u{feff}')) { "\u{feff}" } else { "" };
        (format!("{bom}{}", out.text), out.files)
    }

    /// Take the library's write lock and adopt what other copies wrote since
    /// this one last looked. Every change is written through, so memory holds
    /// nothing the disk does not. A lock failure must never allow an unguarded write.
    fn begin_write(&mut self) -> Result<WriteLock> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() { if self.batch.lock.is_some() { return Ok(WriteLock(None)); } let lock = lock_root(&self.root)?; self.refresh_sqlite()?; return Ok(lock); }

        // Inside a batch the lock is held and memory is what the disk will be.
        if self.batch.lock.is_some() { return Ok(WriteLock(None)); }
        std::fs::create_dir_all(self.internal())?;
        let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(self.internal().join(".lock"))?;
        f.lock()?;
        let lock = WriteLock(Some(f));
        // Another copy moved it to the new layout meanwhile: nothing more is written here.
        anyhow::ensure!(!self.flat || !self.root.join(INTERNAL).join("format.json").exists(), "文档库已升级到新格式，请重新打开 Quiet.md");
        let marker: FormatMarker = serde_json::from_slice(&std::fs::read(self.internal().join("format.json"))?)?;
        anyhow::ensure!(marker.format <= FORMAT, "这个文档库由更新版本的 Quiet.md 创建，请升级后再打开");
        // An unreadable index is not judged here (that is `open`'s job): the
        // version in memory is written back unchanged.
        if self.flat {
            if let Some(index) = reread(&self.index_file()) { self.index = index; }
        } else if let Some(disk) = reread::<DiskIndex>(&self.index_file()) {
            let local = self.read_local();
            self.index = self.from_disk(&disk, &local);
        }
        if let Some(trash) = reread(&self.trash_index_file()) { self.trash = trash; }
        Ok(lock)
    }

    fn save_index(&self) -> Result<()> {

        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() { return self.save_sqlite_index(); }

        if self.batch.lock.is_some() { self.batch.index.set(true); return Ok(()); }
        // The old layout keeps everything in its index, as older builds expect.
        if self.flat { return save_json(&self.index_file(), &self.index); }
        save_json(&self.index_file(), &self.to_disk())?;
        self.save_local(&self.local_state())
    }

    /// Run `f` as one write: the library is locked once and the index written
    /// once at the end, so importing or deleting a hundred documents costs
    /// about what one does. Files are written as `f` goes and the index
    /// last, so a failure or crash midway never leaves an entry without its
    /// file: the files already written are picked up by the next `list`
    /// (the old layout's `docs/` too), as files added in the Finder are.
    /// Batches nest; the outermost one writes.
    pub fn batch<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> Result<T> {
        self.begin_batch()?;
        let out = f(self);
        self.end_batch()?;
        Ok(out)
    }

    /// `batch` for callers that cannot hand over a closure (the window's
    /// store is shared): every `begin_batch` that succeeded needs its `end_batch`.
    pub fn begin_batch(&mut self) -> Result<()> {
        if self.batch.depth == 0 {
            let lock = self.begin_write()?;
            self.batch.lock = Some(lock);
        }
        self.batch.depth += 1;
        Ok(())
    }

    pub fn end_batch(&mut self) -> Result<()> {
        anyhow::ensure!(self.batch.depth > 0, "没有进行中的批量写入");
        self.batch.depth -= 1;
        if self.batch.depth > 0 { return Ok(()); }
        let lock = self.batch.lock.take();
        // The documents' folders reach the disk before the index naming them.
        let mut r: Result<()> = Ok(());
        #[cfg(unix)]
        for dir in self.batch.dirs.take() {
            if let Err(e) = std::fs::File::open(&dir).and_then(|f| f.sync_all()) { r = Err(e.into()); }
        }
        #[cfg(not(unix))]
        self.batch.dirs.take();
        if self.batch.index.take() && r.is_ok() { r = self.save_index(); }
        drop(lock);
        r
    }

    /// Write a document's file; in a batch its folder is synced at the end.
    fn write_file(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        if self.batch.lock.is_none() { return write_atomic(path, bytes); }
        write_atomic_then(path, bytes, false)?;
        if let Some(dir) = path.parent() {
            let mut dirs = self.batch.dirs.borrow_mut();
            if !dirs.iter().any(|d| d == dir) { dirs.push(dir.to_path_buf()); }
        }
        Ok(())
    }

    /// A name for a new entry in folder `dir`: `base` (plus `.ext` for a
    /// document), numbered when taken, by a file or by another entry. `own`
    /// is the entry being renamed, whose current name does not count.
    fn free_name(&self, dir: &str, base: &str, ext: Option<&str>, own: Option<&str>) -> String {
        let own_rel = own.and_then(|id| self.index.get(id)).map(|m| m.rel.to_lowercase());
        let taken: HashSet<String> = self.index.values().filter(|m| !m.local() && parent_rel(&m.rel) == dir).map(|m| m.rel.to_lowercase()).collect();
        for n in 1.. {
            let stem = if n == 1 { base.to_string() } else { format!("{base} {n}") };
            let name = match ext { Some(e) => format!("{stem}.{e}"), None => stem };
            let rel = join_rel(dir, &name);
            let low = rel.to_lowercase();
            if own_rel.as_deref() == Some(low.as_str()) { return name; }
            let path = self.abs(&rel);
            let on_disk = path.symlink_metadata().is_ok() || layout::placeholder(&path).is_some_and(|p| p.exists());
            if !taken.contains(&low) && !on_disk { return name; }
        }
        unreachable!()
    }

    /// The old layout's marker raised to 3 before its first Markdown document
    /// or 本机文件: older builds would drop their `markdown` / `path` / `name`.
    fn require_markdown_format(&self) -> Result<()> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() { return Ok(()); }
        let path = self.internal().join("format.json");
        let format = reread::<FormatMarker>(&path).map_or(0, |m| m.format);
        if format < 3 { write_atomic(&path, &serde_json::to_vec_pretty(&FormatMarker { format: 3 })?)?; }
        Ok(())
    }

    /// The old layout as opened: pages that had pages under them (FORMAT 1)
    /// become folders, as older builds did.
    fn flat_folders(&mut self) -> Result<()> {
        let format = reread::<FormatMarker>(&self.internal().join("format.json")).map_or(0, |m| m.format);
        if format >= 2 { return Ok(()); }
        let _lock = self.begin_write()?;
        let backup = self.root.join("index.before-folders.json");
        if !backup.exists() { write_atomic(&backup, &serde_json::to_vec_pretty(&self.index)?)?; }
        let docs = self.root.join("docs");
        let mut index = self.index.clone();
        migrate::pages_to_folders(&mut index, |id| docs.join(format!("{id}.xml")).is_file(), |id| std::fs::read_to_string(docs.join(format!("{id}.xml"))).ok().and_then(|x| docxml::parse(&x).ok()).map(|d| d.title.text));
        self.index = index;
        self.save_index()?;
        write_atomic(&self.internal().join("format.json"), &serde_json::to_vec_pretty(&FormatMarker { format: 2 })?)
    }

    /// `list` in the old layout: the files in `docs/` (a `.md` dropped there by
    /// hand too) and the folders of the index.
    fn list_flat(&mut self, locked: bool) -> Vec<Entry> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() { return self.list_sqlite().unwrap_or_else(|err| { self.warnings.push(format!("无法读取文档索引：{err}")); vec![] }); }

        let mut out = vec![];
        let mut dirty = false;
        let files: Vec<(String, PathBuf, bool)> = std::fs::read_dir(self.root.join("docs")).map(|rd| rd.flatten().map(|e| e.path()).filter_map(|p| {
            let markdown = match p.extension().and_then(|s| s.to_str()) { Some("xml") => false, Some("md") => true, _ => return None };
            Some((p.file_stem()?.to_str()?.to_string(), p, markdown))
        }).collect()).unwrap_or_default();
        let local: Vec<(String, PathBuf, bool)> = self.index.iter().filter_map(|(id, m)| Some((id.clone(), PathBuf::from(m.path.as_ref()?), true))).collect();
        for (id, p, markdown) in files.into_iter().chain(local) {
            let md = std::fs::metadata(&p).ok();
            if md.is_none() && self.index.get(&id).is_some_and(Meta::local) { continue; }
            if markdown && !self.index.get(&id).is_some_and(|m| m.markdown) && locked && self.require_markdown_format().is_ok() {
                let t = now();
                let m = self.index.entry(id.clone()).or_insert(Meta { created: t, modified: t, ..Default::default() });
                m.markdown = true;
                m.name = m.name.take().or(Some("未命名".into()));
                dirty = true;
            }
            let stamp = md.as_ref().and_then(|m| Some((m.modified().ok()?, m.len())));
            let parsed = match self.parsed.get(&id).filter(|c| stamp == Some((c.0, c.1))) {
                Some(c) => c.2.clone(),
                None => {
                    let Ok(text) = std::fs::read_to_string(&p) else { continue };
                    let doc = self.parse_doc(&id, &text).unwrap_or_default();
                    let parsed = Parsed { title: doc.title.text.clone(), icon: doc.icon.clone(), body: doc.plain_text().into(), reminders: doc.reminders(), links: doc.doc_links() };
                    if let Some((t, len)) = stamp { self.parsed.insert(id.clone(), (t, len, parsed.clone())); }
                    parsed
                }
            };
            if !self.index.contains_key(&id) {
                let t = md.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or_else(now);
                self.index.insert(id.clone(), Meta { created: t, modified: t, ..Default::default() });
                dirty = true;
            }
            let Parsed { mut title, icon, body, reminders, links } = parsed;
            if self.index[&id].markdown { title = self.markdown_title(&id); }
            out.push(Entry { id: id.clone(), meta: self.index[&id].clone(), title, icon, body, reminders, links });
        }
        let mut folders: Vec<(&String, &Meta)> = self.index.iter().filter(|(_, m)| m.folder.is_some()).collect();
        folders.sort_by(|a, b| a.0.cmp(b.0));
        for (id, meta) in folders {
            out.push(Entry { id: id.clone(), meta: meta.clone(), title: meta.folder.clone().unwrap_or_default(), icon: None, body: Default::default(), reminders: vec![], links: vec![] });
        }
        let seen: HashSet<&str> = out.iter().map(|e| e.id.as_str()).collect();
        self.parsed.retain(|id, _| seen.contains(id.as_str()));
        if dirty && locked { let _ = self.save_index(); }
        out
    }

    /// Rename or move entries (`id`, new relative path): the files on disk
    /// first, then the paths of what is inside moved folders, then the links
    /// that pointed at them or out of them, so no mention breaks.
    fn relocate(&mut self, moves: &[(String, String)]) -> Result<()> {
        anyhow::ensure!(!self.flat, "旧格式的文档库没有真实文件夹");
        let mut new_rel: HashMap<String, String> = HashMap::new();
        for (id, to) in moves {
            let m = self.index.get(id).ok_or_else(|| anyhow::anyhow!("文档或文件夹不存在"))?;
            anyhow::ensure!(!m.local(), "本机文件不能移入文档库的文件夹");
            if m.folder.is_some() {
                anyhow::ensure!(*to != m.rel && !to.starts_with(&format!("{}/", m.rel)), "不能移动到自身或后代文件夹中");
                let prefix = format!("{}/", m.rel);
                for (k, c) in &self.index {
                    if let Some(rest) = c.rel.strip_prefix(&prefix) && !c.local() { new_rel.insert(k.clone(), format!("{to}/{rest}")); }
                }
            }
            new_rel.insert(id.clone(), to.clone());
        }
        // Documents whose links may change: the moved ones (links out of
        // them are relative to where they are) and those mentioning one.
        let affected: HashSet<&str> = new_rel.keys().map(String::as_str).collect();
        let mut docs = vec![];
        for (k, m) in &self.index {
            if m.folder.is_some() || m.local() || m.markdown { continue; }
            let mentions = self.parsed.get(k).is_none_or(|p| p.2.links.iter().any(|l| affected.contains(l.target.as_str())));
            if !affected.contains(k.as_str()) && !mentions { continue; }
            let Ok(text) = std::fs::read_to_string(self.path(k)) else { continue };
            if !text.contains("type=\"doc\"") { continue; }
            let Ok(doc) = self.parse_doc(k, &text) else { continue };
            docs.push((k.clone(), text, doc));
        }
        let mut done: Vec<(PathBuf, PathBuf)> = vec![];
        for (id, to) in moves {
            let from = self.abs(&self.index[id].rel);
            let dest = self.abs(to);
            let r = (|| -> Result<()> {
                if let Some(p) = dest.parent() { std::fs::create_dir_all(p)?; }
                // A change of case only is the same file on macOS and Windows.
                let same = from.to_string_lossy().to_lowercase() == dest.to_string_lossy().to_lowercase();
                anyhow::ensure!(same || dest.symlink_metadata().is_err(), "已存在同名文件：{to}");
                std::fs::rename(&from, &dest)?;
                Ok(())
            })();
            if let Err(e) = r {
                for (a, b) in done.iter().rev() { let _ = std::fs::rename(b, a); }
                return Err(e);
            }
            done.push((from, dest));
        }
        for (k, rel) in &new_rel { if let Some(m) = self.index.get_mut(k) { m.rel = rel.clone(); } }
        derive_tree(&mut self.index);
        for (k, text, doc) in docs {
            let out = self.serialize_doc(&k, &doc, None);
            if out == text { continue; }
            match write_atomic(&self.path(&k), out.as_bytes()) {
                Ok(()) => {
                    let mut loaded = self.loaded.borrow_mut();
                    if loaded.get(&k).is_some_and(|l| *l == text) { loaded.insert(k.clone(), out); }
                }
                Err(e) => self.warnings.push(format!("未能更新文档中的链接（{}）：{e}", self.index[&k].rel)),
            }
        }
        self.save_index()
    }

    /// A title change renames the file (and keeps the links to it); outside
    /// the library a Markdown file is renamed beside itself, kept as it is
    /// when the name is taken.
    fn follow_title(&mut self, id: &str, title: &str) -> Result<()> {
        let Some(m) = self.index.get(id) else { return Ok(()) };
        if let Some(old) = m.path.clone().map(PathBuf::from) {
            let name = title.replace(['/', '\\', ':'], "_");
            let name = name.trim().trim_start_matches('.');
            if name.is_empty() { return Ok(()); }
            let ext = old.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "md".into());
            let new = old.with_file_name(format!("{name}.{ext}"));
            if new == old || new.try_exists()? { return Ok(()); }
            std::fs::rename(&old, &new)?;
            self.index.get_mut(id).unwrap().path = Some(new.to_string_lossy().into_owned());
            return Ok(());
        }
        if self.flat {
            // The old layout: names live in the index.
            let m = self.index.get_mut(id).unwrap();
            if m.folder.is_some() { m.folder = Some(title.to_string()); } else if m.markdown { m.name = Some(title.to_string()); }
            return Ok(());
        }
        let base = file_name_for(title);
        let rel = m.rel.clone();
        let dir = parent_rel(&rel).to_string();
        let name = base_name(&rel);
        if m.folder.is_some() {
            if layout::name_fits(name, &base) { return Ok(()); }
            let to = join_rel(&dir, &self.free_name(&dir, &base, None, Some(id)));
            return self.relocate(&[(id.to_string(), to)]);
        }
        if layout::name_fits(stem_of(name), &base) { return Ok(()); }
        let to = join_rel(&dir, &self.free_name(&dir, &base, Some(ext_of(m)), Some(id)));
        self.relocate(&[(id.to_string(), to)])
    }

    /// Adopt metadata from a completed background writer without reparsing bodies or releasing editor baselines.
    pub fn adopt_listing(&mut self, entries: &[Entry]) {
        self.index = entries.iter().map(|e| (e.id.clone(), e.meta.clone())).collect();
    }

    /// All documents found on disk. Files and folders made in the Finder are
    /// picked up; entries whose files are gone are dropped.
    pub fn list(&mut self) -> Vec<Entry> {
        let _lock = match self.begin_write() {
            Ok(lock) => Some(lock),
            Err(e) => { self.warnings.push(format!("无法锁定文档库：{e}")); None }
        };
        if self.flat { return self.list_flat(_lock.is_some()); }
        let found = layout::scan(&self.root);
        let mut disk = self.to_disk();
        let mut keys: HashSet<String> = found.dirs.iter().map(|d| format!("{d}/")).collect();
        keys.extend(found.docs.iter().cloned());
        keys.extend(found.pending.iter().cloned());
        let before = disk.len();
        if !found.partial { disk.retain(|k, _| keys.contains(k)); }
        let mut dirty = disk.len() != before;
        for key in &keys {
            if disk.contains_key(key) { continue; }
            let t = std::fs::metadata(self.abs(key.trim_end_matches('/'))).ok().and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or_else(now);
            disk.insert(key.clone(), Attached { id: String::new(), created: t, modified: t, ..Attached::of("", &Meta::default()) });
            dirty = true;
        }
        if dirty {
            let local = self.local_state();
            self.index = self.from_disk(&disk, &local);
            // Paths changed: links parsed before may point elsewhere now.
            self.parsed.clear();
        }
        let pending: HashSet<&str> = found.pending.iter().map(String::as_str).collect();
        let mut out = vec![];
        let mut reparsed = false;
        let mut ids: Vec<String> = self.index.iter().filter(|(_, m)| m.folder.is_none()).map(|(k, _)| k.clone()).collect();
        ids.sort();
        for id in ids {
            let m = &self.index[&id];
            let p = self.path(&id);
            if m.local() && !p.exists() { continue; }
            if !m.local() && pending.contains(m.rel.as_str()) && !p.exists() {
                // Not downloaded yet: listed by its name, read once it is here.
                if let Some(ph) = layout::placeholder(&p) { layout::download(&ph); }
                let title = stem_of(base_name(&m.rel)).to_string();
                out.push(Entry { id: id.clone(), meta: m.clone(), title, icon: None, body: Default::default(), reminders: vec![], links: vec![] });
                continue;
            }
            let md = std::fs::metadata(&p).ok();
            let stamp = md.as_ref().and_then(|m| Some((m.modified().ok()?, m.len())));
            let parsed = match self.parsed.get(&id).filter(|c| stamp == Some((c.0, c.1))) {
                Some(c) => c.2.clone(),
                None => {
                    let Ok(text) = std::fs::read_to_string(&p) else { continue };
                    let doc = self.parse_doc(&id, &text).unwrap_or_default();
                    let parsed = Parsed { title: doc.title.text.clone(), icon: doc.icon.clone(), body: doc.plain_text().into(), reminders: doc.reminders(), links: doc.doc_links() };
                    if let Some((t, len)) = stamp { self.parsed.insert(id.clone(), (t, len, parsed.clone())); reparsed = true; }
                    parsed
                }
            };
            let Parsed { mut title, icon, body, reminders, links } = parsed;
            if m.markdown { title = self.markdown_title(&id); }
            out.push(Entry { id, meta: m.clone(), title, icon, body, reminders, links });
        }
        // Folders in id order: the index is a HashMap, whose order changes every time it is re-read.
        let mut folders: Vec<(&String, &Meta)> = self.index.iter().filter(|(_, m)| m.folder.is_some()).collect();
        folders.sort_by(|a, b| a.0.cmp(b.0));
        for (id, meta) in folders {
            let title = meta.folder.clone().unwrap_or_default();
            out.push(Entry { id: id.clone(), meta: meta.clone(), title, icon: None, body: Default::default(), reminders: vec![], links: vec![] });
        }
        let seen: HashSet<&str> = out.iter().map(|e| e.id.as_str()).collect();
        let cached = self.parsed.len();
        self.parsed.retain(|id, _| seen.contains(id.as_str()));
        if reparsed || self.parsed.len() != cached { let _ = self.save_parsed(); }
        if dirty && _lock.is_some() {
            let _ = self.save_index();
        }
        out
    }

    /// What `list` parsed last launch, for the files still where they were.
    fn load_parsed(&mut self) {

        let Some(c) = std::fs::read(parsed_cache_path(&self.root)).ok().and_then(|b| serde_json::from_slice::<ParsedCache>(&b).ok()) else { return };
        if c.version != parsed_cache_version() { return }
        let mut docs = c.docs;
        let ids: Vec<String> = self.index.iter().filter(|(_, m)| m.folder.is_none()).map(|(k, _)| k.clone()).collect();
        for id in ids {
            let Some(d) = docs.remove(&self.path(&id)) else { continue };
            let reminders = d.reminders.into_iter().map(|(expire, notify, should_notify, whole_day)| crate::model::InlineReminder { expire, notify, should_notify, whole_day }).collect();
            let links = d.links.into_iter().map(|(target, block, line)| crate::model::DocLink { target, block, line }).collect();
            self.parsed.insert(id, (d.modified, d.len, Parsed { title: d.title, icon: d.icon, body: d.body.into(), reminders, links }));
        }
    }

    fn save_parsed(&self) -> Result<()> {
        let docs = self.parsed.iter().map(|(id, (modified, len, p))| (self.path(id), CachedDoc {
            modified: *modified, len: *len, title: p.title.clone(), icon: p.icon.clone(), body: p.body.to_string(),
            reminders: p.reminders.iter().map(|r| (r.expire, r.notify, r.should_notify, r.whole_day)).collect(),
            links: p.links.iter().map(|l| (l.target.clone(), l.block, l.line.clone())).collect(),
        })).collect();
        let path = parsed_cache_path(&self.root);
        if let Some(dir) = path.parent() { std::fs::create_dir_all(dir)?; }
        write_atomic(&path, &serde_json::to_vec(&ParsedCache { version: parsed_cache_version(), docs })?)
    }

    fn read_doc_text(&self, id: &str) -> Result<String> {
        let p = self.path(id);
        match std::fs::read_to_string(&p) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && layout::placeholder(&p).is_some_and(|ph| ph.exists()) => {
                if let Some(ph) = layout::placeholder(&p) { layout::download(&ph); }
                anyhow::bail!("这篇文档还在从 iCloud 下载，请稍后再打开")
            }
            r => Ok(r?),
        }
    }

    pub fn load(&self, id: &str) -> Result<Document> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite_manages(id) { return self.load_sqlite_doc(id, false); }

        self.parse_doc(id, &self.read_doc_text(id)?)
    }

    /// Only opening an editor establishes a new save baseline; previews must
    /// not hide external changes from an already open editor.
    pub fn load_for_edit(&self, id: &str, shared: bool) -> Result<Document> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite_manages(id) { return self.load_sqlite_doc(id, !shared); }

        let xml = self.read_doc_text(id)?;
        let doc = self.parse_doc(id, &xml)?;
        if !shared { self.loaded.borrow_mut().insert(id.to_string(), xml); }
        Ok(doc)
    }

    /// Release a closed editor's conflict-detection baseline, after its last window saved.
    pub fn forget_loaded(&self, id: &str) { self.loaded.borrow_mut().remove(id); }

    pub fn save(&mut self, id: &str, doc: &Document) -> Result<Meta> {
        let _lock = self.begin_write()?;
        self.save_now(id, doc)
    }

    /// `save` for callers that already hold the write lock. A new id becomes a
    /// DocxXML file at the top of the library, named after its title.
    fn save_now(&mut self, id: &str, doc: &Document) -> Result<Meta> { self.save_in(id, doc, None) }

    /// `save_now`, a new id going into folder `parent`.
    fn save_in(&mut self, id: &str, doc: &Document, parent: Option<&str>) -> Result<Meta> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite_manages(id) { return self.save_sqlite_locked(id, doc, None, None, |_, _| Ok(())).map(|(meta, _)| meta); }

        let new = !self.index.contains_key(id);
        if new {
            let t = now();
            let dir = self.folder_rel(parent);
            let rel = if self.flat { String::new() } else { join_rel(&dir, &self.free_name(&dir, &file_name_for(&doc.title.text), Some("xml"), None)) };
            self.index.insert(id.to_string(), Meta { created: t, modified: t, rel, ..Default::default() });
            if !dir.is_empty() { derive_tree(&mut self.index); }
        }
        let r = self.write_doc(id, doc);
        if r.is_err() && new { self.index.remove(id); }
        r
    }

    fn write_doc(&mut self, id: &str, doc: &Document) -> Result<Meta> {
        let disk = if self.is_markdown(id) || self.loaded.borrow().contains_key(id) { std::fs::read_to_string(self.path(id)).ok() } else { None };
        let (xml, copies) = self.serialize_with_copies(id, doc, disk.as_deref());
        if let Some(previous) = self.loaded.borrow().get(id) {
            let disk = disk.as_deref().ok_or_else(|| anyhow::anyhow!("无法读取文档"))?;
            let message = if self.index.get(id).is_some_and(Meta::local) { "文件已在别处被修改，未覆盖磁盘内容。请另存副本或重新打开文件" } else { "文档已被另一个实例修改，未覆盖磁盘内容。请另存副本或重新载入磁盘版本" };
            anyhow::ensure!(disk == previous || disk == xml, "{message}");
        }
        // A Markdown file whose content did not change is left alone: its
        // formatting is the user's.
        let same = self.is_markdown(id) && disk.as_deref().is_some_and(|d| d == xml || self.parse_doc(id, d).is_ok_and(|was| self.serialize_doc(id, &was, Some(d)) == xml));
        if !same {
            // Images and attachments first, so the file never links to a missing one.
            if let Some(base) = self.base(id) {
                for (src, rel) in &copies {
                    let to = base.join(rel);
                    if !to.exists() {
                        if let Some(dir) = to.parent() { std::fs::create_dir_all(dir)?; }
                        std::fs::copy(src, &to)?;
                    }
                    self.copied.borrow_mut().entry(id.to_string()).or_default().insert(src.clone(), rel.clone());
                }
            }
            self.write_file(&self.path(id), xml.as_bytes())?;
            // The previous text is already in `disk`: a snapshot costs no
            // extra read. History is best-effort — it never fails the save.
            if let Some(previous) = disk.as_deref()
                && previous != xml.as_str()
                && !self.index.get(id).is_some_and(Meta::local)
            {
                let ext = if self.is_markdown(id) { "md" } else { "xml" };
                let limit = self.settings().history_limit();
                if let Err(e) = crate::versions::snapshot(&self.history_base(), id, ext, previous, crate::time::now_ms(), limit, false) {
                    self.warnings.push(format!("保存版本历史失败：{e}"));
                }
            }
        }
        self.loaded.borrow_mut().insert(id.to_string(), if same { disk.unwrap_or(xml) } else { xml });
        let title = doc.title.text.trim().to_string();
        let changed = if self.is_markdown(id) { title != self.markdown_title(id) } else { true };
        if changed && !(self.is_markdown(id) && title.is_empty()) && let Err(e) = self.follow_title(id, &title) {
            self.warnings.push(format!("未能按标题重命名文件：{e}"));
        }
        let t = now();
        let m = self.index.get_mut(id).ok_or_else(|| anyhow::anyhow!("文档不存在"))?;
        m.modified = t;
        let m = m.clone();
        self.save_index()?;
        Ok(m)
    }

    pub fn create(&mut self) -> Result<(String, Document)> {
        let doc = Document::default();
        let id = self.create_from(&doc)?;
        Ok((id, doc))
    }

    /// Save `doc` as a new document (创建副本, import).
    pub fn create_from(&mut self, doc: &Document) -> Result<String> { self.create_from_in(doc, None) }

    /// `create_from` into folder `parent` (`None`: the top), as its last
    /// child; the file is written there, never moved there afterwards.
    pub fn create_from_in(&mut self, doc: &Document, parent: Option<&str>) -> Result<String> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() {
            let _lock = self.begin_write()?;
            self.validate_folder(None, parent)?;
            let id = uuid::Uuid::new_v4().to_string();
            let meta = Meta { created: now(), parent: parent.map(str::to_string), markdown: false, ..Default::default() };
            self.save_sqlite_locked(&id, doc, Some(meta), None, |_, _| Ok(()))?;
            self.record_file_change(&id, None);
            return Ok(id);
        }

        let id = uuid::Uuid::new_v4().to_string();
        let lock = self.begin_write()?;
        self.validate_folder(None, parent)?;
        self.save_in(&id, doc, parent)?;
        self.record_file_change(&id, None);
        // `move_in_tree` takes the lock itself: outside a batch, holding it here would wait on ourselves.
        drop(lock);
        self.place_new(&id, parent)?;
        Ok(id)
    }

    /// A new Markdown document in the library (导入, 创建副本): `front`
    /// matter, then `doc`; named after `doc`'s title.
    pub fn create_markdown(&mut self, doc: &Document, front: &str) -> Result<String> { self.create_markdown_in(doc, front, None) }

    /// `create_markdown` into folder `parent`, like `create_from_in`.
    pub fn create_markdown_in(&mut self, doc: &Document, front: &str, parent: Option<&str>) -> Result<String> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() {
            let _lock = self.begin_write()?;
            self.validate_folder(None, parent)?;
            let id = uuid::Uuid::new_v4().to_string();
            let meta = Meta { created: now(), parent: parent.map(str::to_string), markdown: true, ..Default::default() };
            self.save_sqlite_locked(&id, doc, Some(meta), Some(front), |_, _| Ok(()))?;
            self.record_file_change(&id, None);
            return Ok(id);
        }

        let id = uuid::Uuid::new_v4().to_string();
        let lock = self.begin_write()?;
        self.validate_folder(None, parent)?;
        let t = now();
        if self.flat {
            self.require_markdown_format()?;
            self.index.insert(id.clone(), Meta { created: t, modified: t, markdown: true, name: Some(doc.title.text.trim().to_string()), ..Default::default() });
        } else {
            let dir = self.folder_rel(parent);
            let rel = join_rel(&dir, &self.free_name(&dir, &file_name_for(&doc.title.text), Some("md"), None));
            self.index.insert(id.clone(), Meta { created: t, modified: t, rel, ..Default::default() });
            derive_tree(&mut self.index);
        }
        let text = format!("{front}{}", crate::markdown::write_file(doc, "", None));
        if let Err(e) = self.write_file(&self.path(&id), text.as_bytes()).and_then(|_| self.save_index()) { self.index.remove(&id); return Err(e); }
        self.record_file_change(&id, None);
        drop(lock);
        self.place_new(&id, parent)?;
        Ok(id)
    }

    /// The relative path of folder `parent` ("" for the top or the old layout).
    fn folder_rel(&self, parent: Option<&str>) -> String {
        if self.flat { return String::new(); }
        parent.and_then(|p| self.index.get(p)).map(|m| m.rel.clone()).unwrap_or_default()
    }

    /// A document just made in folder `parent` becomes its last child, as
    /// 移动到 would put it (the old layout records the folder only here).
    fn place_new(&mut self, id: &str, parent: Option<&str>) -> Result<()> {
        if parent.is_some() { self.move_in_tree(id, parent, None)?; }
        Ok(())
    }

    /// A Markdown document's front matter, as on disk.
    pub fn front_matter_of(&self, id: &str) -> String {
        if !self.is_markdown(id) { return String::new(); }
        #[cfg(feature = "sqlite-store")]
        if self.sqlite_manages(id) { return self.sqlite_front(id).unwrap_or_default(); }
        let text = std::fs::read_to_string(self.path(id)).unwrap_or_default();
        crate::markdown::front_matter(text.trim_start_matches('\u{feff}')).0.to_string()
    }

    /// Where a 本机文件 is.
    pub fn local_path(&self, id: &str) -> Option<PathBuf> { self.index.get(id)?.path.as_ref().map(PathBuf::from) }

    /// 打开 a Markdown file where it is (本机文件): its document id, the one
    /// it already has when it was opened before. A file inside the library
    /// is the library's document.
    pub fn open_local(&mut self, path: &std::path::Path) -> Result<String> {
        let path = path.canonicalize()?;
        // Windows canonical paths are `\\?\C:\…`: keep the ordinary form, which
        // the Explorer and the title show.
        let path = match path.to_str().and_then(|p| p.strip_prefix(r"\\?\")).filter(|p| !p.starts_with("UNC\\")) { Some(p) => PathBuf::from(p), None => path };
        anyhow::ensure!(path.is_file(), "文件不存在");
        let _lock = self.begin_write()?;
        if let Some(rel) = self.root.canonicalize().ok().and_then(|r| path.strip_prefix(r).ok().map(|p| p.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")))
            && let Some((id, _)) = self.index.iter().find(|(_, m)| !m.local() && m.rel == rel) {
            let id = id.clone();
            self.index.get_mut(&id).unwrap().opened = now();
            self.save_local(&self.local_state())?;
            return Ok(id);
        }
        let key = path.to_string_lossy().into_owned();
        let t = now();
        if let Some((id, m)) = self.index.iter_mut().find(|(_, m)| m.path.as_deref() == Some(key.as_str())) {
            m.opened = t;
            let id = id.clone();
            self.save_index()?;
            return Ok(id);
        }
        if self.flat { self.require_markdown_format()?; }
        let id = uuid::Uuid::new_v4().to_string();
        self.index.insert(id.clone(), Meta { created: t, modified: t, opened: t, markdown: true, path: Some(key), ..Default::default() });
        if let Err(e) = self.save_index() { self.index.remove(&id); return Err(e); }
        Ok(id)
    }

    /// Size of the document on disk, plus the local images and files it references.
    pub fn size_of(&self, id: &str, doc: &Document) -> u64 {
        #[cfg(feature = "sqlite-store")]
        let mut n = if self.sqlite_manages(id) { self.sqlite_size(id).unwrap_or(0) }
            else { std::fs::metadata(self.path(id)).map_or(0, |m| m.len()) };
        #[cfg(not(feature = "sqlite-store"))]
        let mut n = std::fs::metadata(self.path(id)).map_or(0, |m| m.len());
        for src in doc.asset_sources() {
            if let Some(p) = image_path(&src) {
                n += std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
            }
        }
        n
    }

    /// Move a document or folder to the 回收站; `root` is the page whose
    /// deletion takes it along (`None`: deleted itself). A folder takes what
    /// is in it along (deleting those afterwards does nothing more).
    pub fn delete(&mut self, id: &str, root: Option<&str>) -> Result<()> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() && !self.index.get(id).is_some_and(Meta::local) { let _lock = self.begin_write()?; return self.delete_sqlite(id, root); }

        let _lock = self.begin_write()?;
        // 本机文件: only taken off the list; the file is the user's.
        if self.index.get(id).is_some_and(Meta::local) {
            self.index.remove(id);
            self.loaded.borrow_mut().remove(id);
            return self.save_index();
        }
        if !self.index.contains_key(id) && self.trash.contains_key(id) { return Ok(()); }
        anyhow::ensure!(self.index.contains_key(id), "文档或文件夹不存在");
        let ride = root.unwrap_or(id).to_string();
        let mut items = vec![];
        if self.index[id].folder.is_some() && !self.flat {
            let prefix = format!("{}/", self.index[id].rel);
            let mut inside: Vec<(String, usize, bool)> = self.index.iter().filter(|(_, m)| !m.local() && m.rel.starts_with(&prefix))
                .map(|(k, m)| (k.clone(), m.rel.matches('/').count(), m.folder.is_some())).collect();
            // Deepest first, documents before the folder holding them.
            inside.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)).then(a.0.cmp(&b.0)));
            items.extend(inside.into_iter().map(|(k, _, _)| (k, Some(ride.clone()))));
        }
        items.push((id.to_string(), root.map(str::to_string)));
        self.trash_many(&items, true)
    }

    /// Put one entry in the 回收站: a document's file, or a folder with what is left in it.
    fn trash_one(&mut self, id: &str, root: Option<String>) -> Result<()> { self.trash_many(&[(id.to_string(), root)], false) }

    /// Put entries in the 回收站 (each with the page whose deletion takes it
    /// along), in order; `record`: for 撤销. The 回收站's index is written
    /// once before anything moves and the library's once after, so a big
    /// folder costs two index writes, not two per document.
    fn trash_many(&mut self, items: &[(String, Option<String>)], record: bool) -> Result<()> {
        let mut trash = self.trash.clone();
        let mut targets = vec![];
        for (id, root) in items {
            let target = self.trash_path(id);
            anyhow::ensure!(!target.try_exists()? && !targets.contains(&target), "回收站已存在同名文档，未覆盖：{id}");
            let meta = self.index.get(id).cloned().unwrap_or_default();
            trash.insert(id.clone(), Trashed { meta, deleted: now(), root: root.clone() });
            targets.push(target);
        }
        std::fs::create_dir_all(self.trash_dir())?;
        // Persist destination metadata before moving content. An interrupted
        // move leaves extra index entries, never an unindexed trashed file.
        save_json(&self.trash_index_file(), &trash)?;
        self.trash = trash;
        let mut failed = None;
        for (n, ((id, _), target)) in items.iter().zip(&targets).enumerate() {
            let before = if record { self.file_state(id) } else { None };
            let from = self.path(id);
            let folder = self.index.get(id).is_some_and(|m| m.folder.is_some());
            if (!folder || from.is_dir()) && let Err(e) = std::fs::rename(&from, target) { failed = Some((n, e)); break; }
            if !folder { crate::versions::move_to_trash(&self.history_base(), id); }
            self.index.remove(id);
            self.loaded.borrow_mut().remove(id);
            if record { self.record_file_change(id, before); }
        }
        if let Some((n, _)) = &failed {
            // What did not move stays out of the 回收站.
            for (id, _) in &items[*n..] { self.trash.remove(id); }
            let _ = self.save_trash();
        }
        // Content has moved: cleanup failure must not report deletion as failed.
        if (failed.is_none() || failed.as_ref().is_some_and(|(n, _)| *n > 0)) && let Err(e) = self.save_index() {
            self.warnings.push(format!("文档已移入回收站，文档索引清理失败：{e}"));
        }
        match failed { Some((_, e)) => Err(e.into()), None => Ok(()) }
    }

    pub fn trash_dir(&self) -> PathBuf {
        let d = self.internal().join("trash");
        let _ = std::fs::create_dir_all(&d);
        d
    }

    /// A trashed document's file, `<id>.xml` / `<id>.md`; a folder's, `<id>/`.
    fn trash_path(&self, id: &str) -> PathBuf {
        let meta = self.trash.get(id).map(|t| &t.meta).or_else(|| self.index.get(id));
        let dir = self.internal().join("trash");
        match meta {
            Some(m) if m.folder.is_some() => dir.join(id),
            m => dir.join(format!("{id}.{}", if m.is_some_and(|m| m.markdown) { "md" } else { "xml" })),
        }
    }

    fn save_trash(&self) -> Result<()> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() { return self.save_sqlite_index(); }

        std::fs::create_dir_all(self.trash_dir())?;
        save_json(&self.trash_index_file(), &self.trash)
    }

    /// `id` and the pages deleted along with it.
    fn trash_group(&self, id: &str) -> Vec<String> {
        let mut out = vec![id.to_string()];
        out.extend(self.trash.iter().filter(|(k, t)| t.root.as_deref() == Some(id) && k.as_str() != id).map(|(k, _)| k.clone()));
        out
    }

    /// The 回收站, newest first: one row per deleted page (its child pages
    /// ride along). Files trashed by older versions are adopted, and pages
    /// past 30 days are removed for good.
    pub fn list_trash(&mut self) -> Vec<TrashEntry> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() { let lock = self.begin_write().ok(); return self.list_sqlite_trash(lock.is_some()).unwrap_or_else(|err| { self.warnings.push(format!("无法读取回收站：{err}")); vec![] }); }

        let _lock = match self.begin_write() {
            Ok(lock) => Some(lock),
            Err(e) => { self.warnings.push(format!("无法锁定文档库：{e}")); None }
        };
        let dir = self.internal().join("trash");
        let _ = std::fs::create_dir_all(&dir);
        let mut dirty = false;
        let mut on_disk = HashSet::new();
        // Unreadable folder: list what the index knows, and leave it alone.
        let listed = std::fs::read_dir(&dir);
        let mut readable = listed.is_ok();
        if let Ok(rd) = listed {
            for entry in rd {
                let e = match entry {
                    Ok(e) => e,
                    Err(e) => {
                        readable = false;
                        self.warnings.push(format!("回收站目录读取不完整：{e}"));
                        continue;
                    }
                };
                let p = e.path();
                if p.is_dir() {
                    if let Some(id) = p.file_name().and_then(|s| s.to_str()) { on_disk.insert(id.to_string()); }
                    continue;
                }
                let markdown = match p.extension().and_then(|s| s.to_str()) { Some("xml") => false, Some("md") => true, _ => continue };
                let Some(id) = p.file_stem().and_then(|s| s.to_str()).map(str::to_string) else { continue };
                if !self.trash.contains_key(&id) {
                    let t = e
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or_else(now);
                    // Modification time is not deletion time. Recovered orphans
                    // get a full retention period instead of immediate expiry.
                    let meta = Meta { created: t, modified: t, markdown, name: markdown.then(|| "未命名".into()), ..Default::default() };
                    self.trash.insert(id.clone(), Trashed { meta, deleted: now(), root: None });
                    dirty = true;
                }
                on_disk.insert(id);
            }
        }
        if readable {
            let before = self.trash.len();
            self.trash.retain(|id, t| t.meta.folder.is_some() || on_disk.contains(id));
            dirty |= self.trash.len() != before;
        }
        let cutoff = now().saturating_sub(TRASH_DAYS * 86_400);
        let expired: Vec<String> = self.trash.iter().filter(|(_, t)| t.deleted <= cutoff).map(|(k, _)| k.clone()).collect();
        for id in expired.into_iter().filter(|_| _lock.is_some()) {
            let p = self.trash_path(&id);
            let r = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
            match r {
                Ok(()) => { self.trash.remove(&id); dirty = true; crate::versions::purge(&self.history_base(), &id); }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => { self.trash.remove(&id); dirty = true; crate::versions::purge(&self.history_base(), &id); }
                Err(e) => self.warnings.push(format!("无法清理过期回收站文档 {id}：{e}")),
            }
        }
        if dirty && _lock.is_some() && let Err(e) = self.save_trash() {
            self.warnings.push(format!("回收站索引保存失败：{e}"));
        }
        let mut out: Vec<TrashEntry> = self
            .trash
            .iter()
            .filter(|(_, t)| t.root.as_ref().is_none_or(|r| !self.trash.contains_key(r)))
            .map(|(id, t)| {
                let doc = if t.meta.markdown || t.meta.folder.is_some() { Document::default() } else { std::fs::read_to_string(self.trash_path(id)).ok().and_then(|x| docxml::parse(&x).ok()).unwrap_or_default() };
                TrashEntry {
                    id: id.clone(),
                    title: t.meta.folder.clone().or_else(|| t.meta.name.clone()).unwrap_or_else(|| doc.title.text.clone()),
                    icon: doc.icon.clone(),
                    deleted: t.deleted,
                    parent: t.meta.parent.clone(),
                }
            })
            .collect();
        out.sort_by(|a, b| b.deleted.cmp(&a.deleted).then_with(|| a.id.cmp(&b.id)));
        out
    }

    /// 恢复: put a deleted page (and the pages that went with it) back. True
    /// when its parent page is gone, so it landed at the top of 我的文档库.
    pub fn restore(&mut self, id: &str) -> Result<bool> {
        let _lock = self.begin_write()?;
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() { return self.restore_sqlite(id); }

        anyhow::ensure!(self.trash.contains_key(id), "回收站中没有这篇文档");
        let mut group = self.trash_group(id);
        // Folders before what is in them.
        let depth = |k: &String| { let mut n = 0; let mut p = self.trash.get(k).and_then(|t| t.meta.parent.clone()); while let Some(q) = p.filter(|_| n < 64) { n += 1; p = self.trash.get(&q).and_then(|t| t.meta.parent.clone()); } n };
        group.sort_by_key(|k| (depth(k), k.clone()));
        let parent = self.trash.get(id).and_then(|t| t.meta.parent.clone());
        let moved = parent.as_ref().is_some_and(|p| !self.index_has(p));
        for k in &group {
            let Some(t) = self.trash.get(k) else { continue };
            let parent = if k == id && moved { None } else { t.meta.parent.clone().filter(|p| self.index.get(p).is_some_and(|m| m.folder.is_some())) };
            self.untrash(k, parent, None).with_context(|| format!("恢复文档 {k} 失败，已恢复的文档保留在文档库"))?;
        }
        if let Err(e) = self.save_trash() {
            self.warnings.push(format!("文档已恢复，回收站索引清理失败：{e}"));
        }
        Ok(moved)
    }

    /// Bring one entry back from the 回收站 into folder `parent`, under its
    /// old name (numbered when taken); `place` overrides its position.
    /// Its links are rewritten for where it lands.
    fn untrash(&mut self, id: &str, parent: Option<String>, place: Option<(u64, Option<u64>)>) -> Result<()> {
        let t = self.trash.get(id).cloned().ok_or_else(|| anyhow::anyhow!("恢复内容已不在回收站"))?;
        let src = self.trash_path(id);
        if self.flat {
            let mut meta = t.meta.clone();
            meta.parent = parent;
            if let Some((order, root_order)) = place { meta.order = order; meta.root_order = root_order; }
            let markdown = meta.markdown;
            let folder = meta.folder.is_some();
            self.index.insert(id.to_string(), meta);
            if !folder {
                let to = self.path(id);
                anyhow::ensure!(!to.try_exists()? , "文档库已存在同名文档，未覆盖：{id}");
                if let Err(e) = std::fs::rename(&src, &to) { self.index.remove(id); return Err(e.into()); }
            }
            let _ = markdown;
            self.trash.remove(id);
            if !folder { crate::versions::restore_from_trash(&self.history_base(), id); }
            self.save_index()?;
            self.record_file_change(id, None);
            return Ok(());
        }
        let dir = parent.as_ref().and_then(|p| self.index.get(p)).map(|m| m.rel.clone()).unwrap_or_default();
        let mut meta = t.meta.clone();
        let text = if meta.folder.is_none() && !meta.markdown { std::fs::read_to_string(&src).ok() } else { None };
        // Read where it was, so relative links resolve as they did.
        let doc = text.as_deref().filter(|x| x.contains("type=\"doc\"")).and_then(|x| {
            let mut d = docxml::parse(x).ok()?;
            self.links_to_ids(parent_rel(&t.meta.rel), &mut d);
            Some(d)
        });
        let rel = if let Some(name) = &meta.folder {
            let rel = join_rel(&dir, &self.free_name(&dir, &file_name_for(name), None, None));
            if src.is_dir() { std::fs::rename(&src, self.abs(&rel))?; } else { std::fs::create_dir_all(self.abs(&rel))?; }
            rel
        } else {
            let base = if !t.meta.rel.is_empty() { layout::unnumbered(stem_of(base_name(&t.meta.rel))).to_string() }
                else if meta.markdown { file_name_for(meta.name.as_deref().unwrap_or("")) }
                else { file_name_for(&text.as_deref().and_then(|x| docxml::parse(x).ok()).map(|d| d.title.text).unwrap_or_default()) };
            let rel = join_rel(&dir, &self.free_name(&dir, &base, Some(ext_of(&meta)), None));
            std::fs::rename(&src, self.abs(&rel))?;
            rel
        };
        meta.rel = rel;
        meta.parent = parent;
        if let Some((order, root_order)) = place { meta.order = order; meta.root_order = root_order; }
        self.index.insert(id.to_string(), meta);
        derive_tree(&mut self.index);
        self.trash.remove(id);
        crate::versions::restore_from_trash(&self.history_base(), id);
        if let Some(doc) = doc {
            let out = self.serialize_doc(id, &doc, None);
            if text.as_deref() != Some(out.as_str()) && let Err(e) = write_atomic(&self.path(id), out.as_bytes()) {
                self.warnings.push(format!("未能更新恢复文档中的链接：{e}"));
            }
        }
        self.save_index()?;
        self.record_file_change(id, None);
        Ok(())
    }

    fn index_has(&self, id: &str) -> bool {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite_manages(id) { return self.index.contains_key(id); }

        self.index.get(id).is_some_and(|m| if m.folder.is_some() { self.flat || self.abs(&m.rel).is_dir() } else { self.path(id).exists() })
    }

    /// 彻底删除 a page from the 回收站, with the pages that went with it.
    pub fn purge(&mut self, id: &str) -> Result<()> {
        let _lock = self.begin_write()?;
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() { return self.purge_sqlite(self.trash_group(id)); }

        for k in self.trash_group(id) {
            let p = self.trash_path(&k);
            let r = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
            match r {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            self.trash.remove(&k);
            crate::versions::purge(&self.history_base(), &k);
        }
        self.save_trash()
    }

    /// Copy a DocxXML file into the library under a fresh id.
    pub fn import(&mut self, path: &std::path::Path) -> Result<String> { self.import_into(path, None) }

    /// `import` into folder `parent`.
    pub fn import_into(&mut self, path: &std::path::Path, parent: Option<&str>) -> Result<String> {
        let xml = std::fs::read_to_string(path)?;
        self.create_from_in(&docxml::parse(&xml)?, parent)
    }

    pub fn set_width(&mut self, id: &str, width: PageWidth) -> Result<()> {
        let _lock = self.begin_write()?;
        if let Some(m) = self.index.get_mut(id) {
            m.width = width;
        }
        self.save_index()
    }

    pub fn update_meta(&mut self, id: &str, f: impl FnOnce(&mut Meta)) -> Result<()> {
        let _lock = self.begin_write()?;
        if let Some(m) = self.index.get_mut(id) {
            // Where it is follows from the file, not from here.
            let (rel, folder, parent, markdown, name, path) = (m.rel.clone(), m.folder.clone(), m.parent.clone(), m.markdown, m.name.clone(), m.path.clone());
            f(m);
            (m.rel, m.folder, m.parent, m.markdown, m.name, m.path) = (rel, folder, parent, markdown, name, path);
        }
        self.save_index()
    }

    pub fn set_catalogue(&mut self, id: &str, pinned: bool) -> Result<()> {
        let _lock = self.begin_write()?;
        if let Some(m) = self.index.get_mut(id) {
            m.catalogue_closed = !pinned;
        }
        self.save_index()
    }

    /// 版本历史: snapshots of one document, newest first.
    pub fn versions(&self, id: &str) -> Vec<crate::versions::Version> { crate::versions::list(&self.history_base(), id, false) }

    pub fn version_text(&self, id: &str, name: &str) -> Result<String> { crate::versions::read(&self.history_base(), id, name, false) }

    pub fn delete_version(&self, id: &str, name: &str) -> Result<()> { crate::versions::remove(&self.history_base(), id, name, false) }

    /// Parse a snapshot into a document, for 另存副本.
    pub fn version_document(&self, id: &str, name: &str) -> Result<Document> {
        let text = self.version_text(id, name)?;
        self.parse_doc(id, &text)
    }

    /// Diff a snapshot against a baseline. `None` is the document on disk
    /// now; `Some(other)` compares two snapshots. Lines are ordered by time,
    /// so `-` is always "only in the earlier version". The unchanged lines
    /// are kept, so the same list doubles as a read-only preview.
    pub fn version_diff(&self, id: &str, name: &str, baseline: Option<&str>) -> Result<Vec<crate::diff::Line>> {
        let a = self.version_text(id, name)?;
        let a_ms = crate::versions::stamp(name).unwrap_or(0);
        let (b, b_ms) = match baseline {
            Some(other) => (self.version_text(id, other)?, crate::versions::stamp(other).unwrap_or(0)),
            None => (self.read_body(id)?, u64::MAX),
        };
        let (old, new) = if a_ms <= b_ms { (a.as_str(), b.as_str()) } else { (b.as_str(), a.as_str()) };
        Ok(crate::diff::diff(old, new, self.is_markdown(id)))
    }

    /// 命名版本: set or clear a snapshot's name.
    pub fn set_version_label(&self, id: &str, name: &str, label: Option<&str>) -> Result<()> {
        crate::versions::set_label(&self.history_base(), id, name, label, false)
    }

    /// Restore never loses the present: the current on-disk text becomes a
    /// snapshot first, then the chosen version is parsed for `save` to write.
    pub fn restore_version(&self, id: &str, name: &str) -> Result<Document> {
        let text = self.version_text(id, name)?;
        let current = self.read_body(id)?;
        let ext = if self.is_markdown(id) { "md" } else { "xml" };
        let _ = crate::versions::snapshot(&self.history_base(), id, ext, &current, crate::time::now_ms(), self.settings().history_limit(), true);
        self.parse_doc(id, &text)
    }

    /// 手动保存版本: bypass the interval, still capped by the retention limit.
    /// `false` means the text matched the newest snapshot, so nothing was
    /// written — the caller can say so instead of silently doing nothing.
    pub fn save_version_now(&self, id: &str) -> Result<bool> {
        let text = self.read_body(id)?;
        let ext = if self.is_markdown(id) { "md" } else { "xml" };
        let before = self.versions(id).len();
        crate::versions::snapshot(&self.history_base(), id, ext, &text, crate::time::now_ms(), self.settings().history_limit(), true)?;
        Ok(self.versions(id).len() > before)
    }

    pub fn settings(&self) -> Settings {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() { return self.load_sqlite_settings().unwrap_or_default(); }

        if self.flat { return reread(&self.internal().join("settings.json")).unwrap_or_default(); }
        let mut s: Settings = reread(&self.internal().join("settings.json")).unwrap_or_default();
        let local = self.read_local();
        s.window_size = local.window_size;
        s.quick_note_size = local.quick_note_size;
        s
    }

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() { return self.save_sqlite_settings(s); }

        if self.flat { return write_atomic(&self.internal().join("settings.json"), &serde_json::to_vec_pretty(s)?); }
        let mut local = self.read_local();
        (local.window_size, local.quick_note_size) = (s.window_size, s.quick_note_size);
        self.save_local(&local)?;
        let shared = Settings { window_size: None, quick_note_size: None, ..s.clone() };
        std::fs::create_dir_all(self.internal())?;
        write_atomic(&self.internal().join("settings.json"), &serde_json::to_vec_pretty(&shared)?)
    }

    /// 阅读位置: the list offset this document was left at.
    pub fn scroll_top(&self, id: &str) -> Option<(usize, f32)> {
        let v = self.settings().scroll_top.get(id).copied()?;
        Some((v[0] as usize, v[1]))
    }

    /// Remember — or, with `None`, forget — where a document was left.
    pub fn remember_scroll(&self, id: &str, top: Option<(usize, f32)>) -> Result<()> {
        self.update_position(id, None, top)
    }

    pub fn remember_position(&self, id: &str, selection: crate::editor::core::ReadingSelection, top: Option<(usize, f32)>) -> Result<()> {
        self.update_position(id, Some(selection), top)
    }

    fn update_position(&self, id: &str, selection: Option<crate::editor::core::ReadingSelection>, top: Option<(usize, f32)>) -> Result<()> {
        let mut s = self.settings();
        let same = selection.is_none_or(|sel| s.selections.insert(id.to_owned(), sel) == Some(sel));
        match top {
            Some((ix, y)) => {
                let v = [ix as f32, y];
                if same && s.scroll_top.get(id) == Some(&v) { return Ok(()) }
                s.scroll_top.insert(id.to_string(), v);
            }
            None => {
                if s.scroll_top.remove(id).is_none() && same { return Ok(()) }
            }
        }
        self.save_settings(&s)
    }

    /// 重命名 a document that isn't open: rewrite its title on disk and its
    /// file name; a folder's name is its folder's.
    pub fn rename(&mut self, id: &str, title: &str) -> Result<Meta> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite_manages(id) { let _lock = self.begin_write()?; let before = self.file_state(id); if self.index.get(id).is_some_and(|m| m.folder.is_some()) { self.index.get_mut(id).unwrap().folder = Some(title.to_string()); self.save_index()?; self.record_file_change(id, before); return Ok(self.index[id].clone()); } let mut doc = self.load(id)?; doc.title = crate::model::RichText::plain(title); let meta = self.save_now(id, &doc)?; self.record_file_change(id, before); return Ok(meta); }

        let _lock = self.begin_write()?;
        let before = self.file_state(id);
        if self.index.get(id).is_some_and(|m| m.folder.is_some() || m.markdown) {
            // A Markdown file is left as it is: only its name changes.
            self.follow_title(id, title)?;
            let m = self.index.get_mut(id).unwrap();
            m.modified = now();
            let meta = m.clone();
            self.save_index()?;
            if !meta.local() { self.record_file_change(id, before); }
            return Ok(meta);
        }
        let mut doc = self.load(id)?;
        doc.title = crate::model::RichText::plain(title);
        let meta = self.save_now(id, &doc)?;
        self.record_file_change(id, before);
        Ok(meta)
    }

    /// Note that `id` was just opened (this computer only).
    pub fn touch(&mut self, id: &str) -> Option<Meta> {
        let _lock = self.begin_write().ok()?;
        let m = self.index.get_mut(id)?;
        m.opened = now();
        let m = m.clone();
        let _ = if self.flat { self.save_index() } else { self.save_local(&self.local_state()) };
        Some(m)
    }

    /// 移动到: put `id` in folder `parent` (`None` = top level of 我的文档库), as its last child.
    pub fn set_parent(&mut self, id: &str, parent: Option<&str>) -> Result<Meta> {
        self.move_in_tree(id, parent, None)?;
        Ok(self.index[id].clone())
    }

    /// 收藏 / 取消收藏; returns the new 收藏时间 (0: not starred).
    pub fn set_starred(&mut self, id: &str, on: bool) -> Result<u64> {
        let _lock = self.begin_write()?;
        let t = if on { now() } else { 0 };
        if let Some(m) = self.index.get_mut(id) {
            m.starred = t;
        }
        self.save_index()?;
        Ok(t)
    }

    pub fn set_pinned(&mut self, id: &str, pinned: bool) -> Result<()> {
        let _lock = self.begin_write()?;
        if let Some(m) = self.index.get_mut(id) {
            m.pinned = pinned;
        }
        self.save_index()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A library in a fresh temporary folder, removed on drop.
    struct Lib(PathBuf);

    impl Lib {
        fn new(name: &str) -> Self {
            let d = std::env::temp_dir().join(format!("fdl-test-{name}-{}", uuid::Uuid::new_v4()));
            Lib(d)
        }
        fn open(&self) -> Store {
            Store::open_at(self.0.clone()).unwrap()
        }
        /// Moved to the new layout, then opened.
        fn migrated(&self) -> Store {
            Store::migrate(&self.0).unwrap();
            self.open()
        }
    }

    impl Drop for Lib {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
            let _ = std::fs::remove_file(super::parsed_cache_path(&self.0));
        }
    }

    fn doc(title: &str) -> Document {
        Document { title: crate::model::RichText::plain(title), ..Default::default() }
    }

    fn doc_with(title: &str, body: &str) -> Document {
        let mut d = doc(title);
        d.blocks.push(crate::model::Block::text(body));
        d
    }

    #[test]
    fn abandoning_a_new_session_never_changes_an_existing_empty_document() {
        let lib = Lib::new("private-new-note");
        let mut store = lib.open();
        let existing = store.create().unwrap().0;
        let draft = NewNoteDraft::new(None);
        let mut core = crate::editor::core::Core::new(draft.seed.clone());
        assert!(draft.untouched(&core));
        assert_eq!(store.list().len(), 1);
        core.insert_text("字");
        assert!(core.undo());
        assert!(core.doc.is_blank() && !draft.untouched(&core));
        let id = draft.materialize(&mut store).unwrap();
        assert_ne!(id, existing);
        assert_eq!(store.list().len(), 2);
        assert!(store.list_trash().is_empty());
    }

    fn check_reading_position_state(mut store: Store) {
        let id = store.create_from(&doc("reading state")).unwrap();
        let before = docxml::serialize(&store.load(&id).unwrap());
        let journal = store.track_file_changes();
        let settings = Settings { window_size: Some([900., 700.]), ..Default::default() };
        store.save_settings(&settings).unwrap();
        assert_eq!(store.scroll_top(&id), None);
        let mut core = crate::editor::core::Core::new(store.load(&id).unwrap());
        core.sel = crate::editor::core::Selection { anchor: crate::editor::core::Pos::new(crate::editor::core::TITLE, 7),
            head: crate::editor::core::Pos::new(crate::editor::core::TITLE, 2) };
        let saved = crate::editor::core::ReadingSelection::capture(&core, false);
        store.remember_position(&id, saved, Some((12, 18.5))).unwrap();
        assert_eq!(store.settings().selections.get(&id), Some(&saved));
        assert_eq!(store.scroll_top(&id), Some((12, 18.5)));
        assert_eq!(store.settings().window_size, settings.window_size);
        assert_eq!(docxml::serialize(&store.load(&id).unwrap()), before);
        assert!(journal.take().is_empty());
        store.remember_scroll(&id, None).unwrap();
        assert_eq!(store.settings().selections.get(&id), Some(&saved));
        assert_eq!(store.scroll_top(&id), None);
        assert_eq!(store.settings().window_size, settings.window_size);
    }

    #[test]
    fn reading_position_preserves_file_document_and_window_settings() {
        let lib = Lib::new("reading-state");
        check_reading_position_state(lib.open());
    }

    #[cfg(feature = "sqlite-store")]
    #[test]
    fn reading_position_preserves_sqlite_document_and_window_settings() {
        let lib = Lib::new("sqlite-reading-state");
        std::fs::create_dir_all(&lib.0).unwrap();
        Store::initialize_local_sqlite(&lib.0).unwrap();
        check_reading_position_state(Store::open_sqlite_at(lib.0.clone()).unwrap());
    }

    #[test]
    fn background_listing_updates_paths_without_accepting_external_edits() {
        let lib = Lib::new("background-listing");
        let mut store = Store::open_at(lib.0.clone()).unwrap();
        let (id, original) = store.create().unwrap();
        store.load_for_edit(&id, false).unwrap();
        let mut writer = store.tree_writer();
        let folder = writer.create_folder("Background", None).unwrap();
        writer.set_parent(&id, Some(&folder)).unwrap();
        let added = writer.create_markdown(&docxml::parse("<title>Imported</title><p>new</p>").unwrap(), "").unwrap();
        let mut external = writer.load(&id).unwrap(); external.title = crate::model::RichText::plain("External");
        writer.save(&id, &external).unwrap();
        store.adopt_listing(&writer.list());
        assert_eq!(store.load(&added).unwrap().title.text, "Imported");
        assert_eq!(store.load(&id).unwrap().title.text, "External");
        assert!(store.save(&id, &original).is_err());
    }

    #[test]
    fn saves_keep_previous_versions_and_follow_the_trash() {
        let lib = Lib::new("versions");
        let mut s = lib.open();
        let id = s.create_from(&doc_with("first", "alpha")).unwrap();
        s.load_for_edit(&id, false).unwrap();
        s.save(&id, &doc_with("second", "beta")).unwrap();
        assert_eq!(s.versions(&id).len(), 1);
        // Snapshots stay in `.quiet/`: never a folder in the sidebar or the Finder,
        // and still found after the retitle renamed the file.
        assert!(lib.0.join(".quiet").join("history").join(&id).is_dir());
        assert_eq!(s.list().iter().map(|e| e.id.clone()).collect::<Vec<_>>(), std::slice::from_ref(&id));
        let name = s.versions(&id)[0].name.clone();
        assert_eq!(s.version_text(&id, &name).unwrap(), docxml::serialize(&doc_with("first", "alpha")));
        // Saving identical content again must not create another file.
        s.save(&id, &doc_with("second", "beta")).unwrap();
        assert_eq!(s.versions(&id).len(), 1);
        // The diff compares the reader's text, not the XML tags.
        let d = s.version_diff(&id, &name, None).unwrap();
        assert!(d.iter().any(|l| l.text == "alpha" && l.kind == crate::diff::Kind::Removed));
        assert!(d.iter().any(|l| l.text == "beta" && l.kind == crate::diff::Kind::Added));
        // Two snapshots compare directly, always in time order: the older
        // text is removed, the newer one added, whichever was picked first.
        s.save_version_now(&id).unwrap();
        let v = s.versions(&id);
        assert_eq!(v.len(), 2);
        // The same text again reports "nothing written" instead of piling up.
        assert!(!s.save_version_now(&id).unwrap());
        assert_eq!(s.versions(&id).len(), 2);
        let d = s.version_diff(&id, &v[0].name, Some(&v[1].name)).unwrap();
        assert!(d.iter().any(|l| l.text == "alpha" && l.kind == crate::diff::Kind::Removed));
        assert!(d.iter().any(|l| l.text == "beta" && l.kind == crate::diff::Kind::Added));
        let d = s.version_diff(&id, &v[1].name, Some(&v[0].name)).unwrap();
        assert!(d.iter().any(|l| l.text == "alpha" && l.kind == crate::diff::Kind::Removed));
        assert!(d.iter().any(|l| l.text == "beta" && l.kind == crate::diff::Kind::Added));
        // Restoring first leaves a snapshot of the present.
        let restored = s.restore_version(&id, &name).unwrap();
        assert_eq!(restored.title.text, "first");
        s.save(&id, &restored).unwrap();
        assert!(s.versions(&id).len() >= 2);
        s.delete(&id, None).unwrap();
        assert!(s.versions(&id).is_empty());
        assert!(lib.0.join(".quiet").join("history-trash").join(&id).is_dir());
        s.restore(&id).unwrap();
        assert!(!s.versions(&id).is_empty());
        s.delete(&id, None).unwrap();
        s.purge(&id).unwrap();
    }

    #[test]
    fn file_history_ignores_external_creations() {
        let lib = Lib::new("file-journal");
        let mut local = lib.open();
        let mut other = lib.open();
        let journal = local.track_file_changes();
        let external = other.create_from(&doc("external")).unwrap();
        let own = local.create_folder("own", None).unwrap();
        local.list();
        let changes = journal.take();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].id, own);
        local.apply_file_history(&changes, false).unwrap();
        assert!(local.list().iter().any(|e| e.id == external));
        assert_eq!(local.load(&external).unwrap().title.text, "external");
        local.apply_file_history(&changes, true).unwrap();
        assert!(local.list().iter().any(|e| e.id == own));
    }

    #[test]
    fn tree_history_uses_fresh_disk_positions() {
        let lib = Lib::new("tree-journal");
        let mut local = lib.open();
        let a = local.create_folder("a", None).unwrap();
        let b = local.create_folder("b", None).unwrap();
        let page = local.create_from(&doc("page")).unwrap();
        let mut other = lib.open();
        other.set_parent(&page, Some(&a)).unwrap();
        let journal = local.track_file_changes();
        local.set_parent(&page, Some(&b)).unwrap();
        let changes = journal.take();
        local.apply_file_history(&changes, false).unwrap();
        assert_eq!(local.index[&page].parent.as_ref(), Some(&a));
    }

    #[test]
    fn markdown_documents_stay_markdown() {
        let lib = Lib::new("markdown");
        let mut s = lib.open();
        let mut d = crate::markdown::parse_file("Hello *there*\n", "Note", None);
        let id = s.create_markdown(&d, "---\na: 1\n---\n").unwrap();
        let file = lib.0.join("Note.md");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "---\na: 1\n---\nHello *there*\n");
        let e = s.list().into_iter().find(|e| e.id == id).unwrap();
        assert!(e.meta.markdown);
        assert_eq!(e.title, "Note");
        // Saving rewrites the body only; the front matter stays.
        let mut loaded = s.load_for_edit(&id, false).unwrap();
        loaded.blocks.push(crate::model::Block::text("more"));
        s.save(&id, &loaded).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "---\na: 1\n---\nHello *there*\n\nmore\n");
        // A title change is the file's name, not its contents.
        d = s.load(&id).unwrap();
        d.title = crate::model::RichText::plain("Renamed");
        s.save(&id, &d).unwrap();
        assert_eq!(s.load(&id).unwrap().title.text, "Renamed");
        let file = lib.0.join("Renamed.md");
        assert!(!lib.0.join("Note.md").exists());
        assert!(!std::fs::read_to_string(&file).unwrap().contains("Renamed"));
        // Through the 回收站 and back as Markdown.
        s.delete(&id, None).unwrap();
        assert!(lib.0.join(".quiet/trash").join(format!("{id}.md")).is_file());
        assert_eq!(s.list_trash()[0].title, "Renamed");
        s.restore(&id).unwrap();
        assert!(file.is_file());
        assert_eq!(s.load(&id).unwrap().title.text, "Renamed");
    }

    /// Into a folder outside a batch (a 本机文件 dragged onto a sidebar folder):
    /// placing it takes the write lock again, which must not wait on itself.
    #[test]
    fn creating_into_a_folder_outside_a_batch() {
        let lib = Lib::new("create-into");
        let mut s = lib.open();
        let folder = s.create_folder("Notes", None).unwrap();
        let (_, doc) = s.create().unwrap();
        let md = s.create_markdown_in(&doc, "", Some(&folder)).unwrap();
        let xml = s.create_from_in(&doc, Some(&folder)).unwrap();
        for id in [md, xml] { assert_eq!(s.list().into_iter().find(|e| e.id == id).unwrap().meta.parent.as_deref(), Some(folder.as_str())); }
    }

    #[test]
    fn a_hand_written_markdown_file_joins_the_library() {
        let lib = Lib::new("hand-written");
        let mut s = lib.open();
        let (id, doc) = s.create().unwrap();
        let before = Store::shape_at(&lib.0);
        s.save(&id, &doc).unwrap();
        assert_eq!(before, Store::shape_at(&lib.0), "saving is not a change of shape");
        std::fs::write(lib.0.join("model.md"), "# Written by a model\n").unwrap();
        assert_ne!(before, Store::shape_at(&lib.0), "a new file is");
        let e = s.list().into_iter().find(|e| e.meta.rel == "model.md").unwrap();
        assert!(e.meta.markdown);
        assert!(e.body.contains("Written by a model"), "{:?}", e.body);
    }

    #[test]
    fn local_files_are_edited_in_place_and_never_trashed() {
        let lib = Lib::new("local");
        let dir = lib.0.join("outside");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("todo.md");
        std::fs::write(&file, "* star bullet\n\n![x](pics/x.png)\n").unwrap();
        let mut s = lib.open();
        let id = s.open_local(&file).unwrap();
        assert_eq!(s.open_local(&file).unwrap(), id);
        let e = s.list().into_iter().find(|e| e.id == id).unwrap();
        assert!(e.meta.local());
        assert_eq!(e.title, "todo");
        let mut d = s.load_for_edit(&id, false).unwrap();
        // Saving untouched content leaves the user's formatting alone.
        s.save(&id, &d).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "* star bullet\n\n![x](pics/x.png)\n");
        d.blocks.push(crate::model::Block::text("added"));
        s.save(&id, &d).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "* star bullet\n\n![x](pics/x.png)\n\nadded\n");
        // Changed elsewhere: not overwritten.
        std::fs::write(&file, "edited elsewhere\n").unwrap();
        d.blocks.push(crate::model::Block::text("again"));
        assert!(s.save(&id, &d).is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "edited elsewhere\n");
        // Renaming renames the file beside it.
        s.rename(&id, "done").unwrap();
        let renamed = dir.canonicalize().unwrap().join("done.md");
        assert!(renamed.is_file() && !file.exists());
        // Removing it from the list keeps the file.
        s.delete(&id, None).unwrap();
        assert!(renamed.is_file());
        assert!(s.list().iter().all(|e| e.id != id));
        assert!(s.list_trash().is_empty());
    }

    /// Images from the library used in a 本机文件 are copied beside it:
    /// pasted ones named after the file, picked ones keeping their name;
    /// autosaves reuse the copies.
    #[test]
    fn local_files_get_their_images_beside_them() {
        let lib = Lib::new("local-images");
        let dir = lib.0.join("outside");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Plan.md");
        std::fs::write(&file, "text\n").unwrap();
        let mut s = lib.open();
        std::fs::create_dir_all(lib.0.join(ASSETS)).unwrap();
        let pasted = lib.0.join(".assets/3e5c9916-7c1f-4f7d-af25-74dd4a8a241e.png");
        let picked = lib.0.join(".assets/1a2b3c4d-截图说明.png");
        std::fs::write(&pasted, b"p").unwrap();
        std::fs::write(&picked, b"q").unwrap();
        // Something already there keeps its name.
        std::fs::create_dir_all(dir.join("Plan.assets")).unwrap();
        std::fs::write(dir.join("Plan.assets/Plan-1.png"), b"old").unwrap();
        let id = s.open_local(&file).unwrap();
        let mut d = s.load_for_edit(&id, false).unwrap();
        let image = |src: &std::path::Path| crate::model::Block::new(crate::model::BlockKind::Image { src_kind: "src".into(), src: src.to_string_lossy().into(), width: None, height: None, caption: None, crop: None }, Default::default());
        d.blocks.push(image(&pasted));
        d.blocks.push(image(&picked));
        s.save(&id, &d).unwrap();
        let text = std::fs::read_to_string(dir.canonicalize().unwrap().join("Plan.md")).unwrap();
        assert_eq!(text, "text\n\n![](Plan.assets/Plan-2.png)\n\n![](Plan.assets/截图说明.png)\n");
        assert_eq!(std::fs::read(dir.join("Plan.assets/Plan-2.png")).unwrap(), b"p");
        assert_eq!(std::fs::read(dir.join("Plan.assets/Plan-1.png")).unwrap(), b"old");
        // The next autosave (the editor still points at the library copy) makes no new copy.
        d.blocks.push(crate::model::Block::text("more"));
        s.save(&id, &d).unwrap();
        assert_eq!(std::fs::read_dir(dir.join("Plan.assets")).unwrap().count(), 3);
        assert!(std::fs::read_to_string(dir.join("Plan.md")).unwrap().contains("Plan.assets/Plan-2.png"));
        // Reopened, the links point at the copies.
        let again = s.load(&id).unwrap();
        assert!(again.blocks.iter().any(|b| matches!(&b.kind, crate::model::BlockKind::Image { src, .. } if src.ends_with("Plan.assets/截图说明.png"))));
    }

    #[test]
    /// Undoing a move checks what it moves, not unrelated stale entries.
    fn markdown_moves_undo() {
        let lib = Lib::new("md-undo");
        let mut s = lib.open();
        let folder = s.create_folder("f", None).unwrap();
        let id = s.create_markdown(&crate::markdown::parse_file("x\n", "Note", None), "").unwrap();
        // A page whose parent is gone (in memory only: on disk, where a file
        // is decides its folder).
        let stale = s.create_from(&doc("old")).unwrap();
        s.list();
        s.index.get_mut(&stale).unwrap().parent = Some(uuid::Uuid::new_v4().to_string());
        let journal = s.track_file_changes();
        s.move_in_tree(&id, Some(&folder), None).unwrap();
        let changes = journal.take();
        s.apply_file_history(&changes, false).unwrap();
        assert!(s.index[&id].parent.is_none());
        s.apply_file_history(&changes, true).unwrap();
        assert_eq!(s.index[&id].parent.as_deref(), Some(folder.as_str()));
    }

    #[test]
    fn missing_move_anchor_is_rejected_without_panicking() {
        let lib = Lib::new("missing-move-anchor");
        let mut s = lib.open();
        let id = s.create_from(&doc("moved")).unwrap();
        let anchor = uuid::Uuid::new_v4().to_string();
        let err = s.move_in_tree(&id, None, Some(&anchor)).unwrap_err();
        assert!(err.to_string().contains("目标位置已改变，请重试"));
    }

    #[test]
    fn failed_lock_never_writes() {
        let lib = Lib::new("lock-failure");
        let mut s = lib.open();
        let id = s.create_from(&doc("original")).unwrap();
        std::fs::remove_file(lib.0.join(".quiet/.lock")).unwrap();
        std::fs::create_dir(lib.0.join(".quiet/.lock")).unwrap();
        assert!(s.save(&id, &doc("lost")).is_err());
        assert_eq!(s.load(&id).unwrap().title.text, "original");
        assert!(s.create_from(&doc("new")).is_err());
        assert!(s.delete(&id, None).is_err());
        assert_eq!(s.list().len(), 1);
    }

    #[test]
    fn recovery_preserves_both_versions_and_is_consumed_once() {
        let lib = Lib::new("recovery");
        let mut s = lib.open();
        let id = s.create_from(&doc("disk")).unwrap();
        s.recover_later(&doc("unsaved")).unwrap();
        let recovery = s.recovery_dir();
        drop(s);
        let mut s = lib.open();
        assert_eq!(s.load(&id).unwrap().title.text, "disk");
        assert!(s.list().iter().any(|e| e.title == "unsaved (recovered copy)"));
        assert_eq!(s.list().len(), 2);
        drop(s);
        assert_eq!(lib.open().list().len(), 2);
        std::fs::remove_dir_all(recovery).unwrap();
    }

    #[test]
    fn atomic_write_cleans_failed_temporary_files() {
        let lib = Lib::new("atomic-failure");
        std::fs::create_dir_all(lib.0.join("target")).unwrap();
        assert!(write_atomic(&lib.0.join("target"), b"new").is_err());
        assert_eq!(std::fs::read_dir(&lib.0).unwrap().count(), 1);
    }

    #[test]
    fn exports_stay_in_library_and_reads_are_bounded() {
        let lib = Lib::new("export");
        let outside = Lib::new("outside");
        std::fs::create_dir_all(lib.0.join("assets")).unwrap();
        std::fs::create_dir_all(&outside.0).unwrap();
        std::fs::write(lib.0.join("assets/good"), b"1234").unwrap();
        std::fs::write(outside.0.join("secret"), b"secret").unwrap();
        assert!(export_path_in(&lib.0, "assets/good").is_some());
        assert!(export_path_in(&lib.0, &outside.0.join("secret").to_string_lossy()).is_none());
        let traversal = format!("../{}/secret", outside.0.file_name().unwrap().to_string_lossy());
        assert!(export_path_in(&lib.0, &traversal).is_none());
        #[cfg(unix)] {
            std::os::unix::fs::symlink(&outside.0, lib.0.join("assets/link")).unwrap();
            assert!(export_path_in(&lib.0, "assets/link/secret").is_none());
        }
        assert!(read_bounded(&lib.0.join("assets/good"), 3).is_err());
        assert_eq!(read_bounded(&lib.0.join("assets/good"), 4).unwrap(), b"1234");
    }

    #[test]
    fn save_list_and_reopen() {
        let lib = Lib::new("basic");
        let mut s = lib.open();
        assert!(s.warnings.is_empty());
        let id = s.create_from(&doc("一")).unwrap();
        s.set_pinned(&id, true).unwrap();
        let mut s = lib.open();
        let list = s.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "一");
        assert!(list[0].meta.pinned);
        assert!(lib.0.join(".quiet/format.json").is_file());
        // The library holds the note and its own hidden folder, nothing else.
        let names: Vec<String> = std::fs::read_dir(&lib.0).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!({ let mut n = names; n.sort(); n }, [".quiet", "一.xml"]);
    }

    #[test]
    fn list_cache_survives_a_relaunch() {
        let lib = Lib::new("listcache-disk");
        let mut s = lib.open();
        let a = s.create_from(&doc("一")).unwrap();
        s.create_from(&doc("二")).unwrap();
        assert_eq!(s.list().len(), 2);
        drop(s);
        // A new launch starts with what the last one parsed (#170) …
        let mut s = lib.open();
        assert_eq!(s.parsed.len(), 2);
        s.parsed.get_mut(&a).unwrap().2.title = "缓存".into();
        assert!(s.list().iter().any(|e| e.title == "缓存"));
        drop(s);
        // … and drops a cache written by another version.
        let path = parsed_cache_path(&lib.0);
        let mut c: ParsedCache = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        c.version = "0.0.0-0".into();
        std::fs::write(&path, serde_json::to_vec(&c).unwrap()).unwrap();
        assert!(lib.open().parsed.is_empty());
    }

    #[test]
    fn list_rereads_only_changed_files() {
        let lib = Lib::new("listcache");
        let mut s = lib.open();
        let a = s.create_from(&doc("一")).unwrap();
        let b = s.create_from(&doc("二")).unwrap();
        assert_eq!(s.list().len(), 2);
        // Another app edits b. a is unchanged on disk, so list keeps what it
        // parsed before (marked here to tell it from a re-read).
        std::fs::write(s.path(&b), docxml::serialize(&doc("二改"))).unwrap();
        let ap = s.path(&a);
        let stamp = std::fs::metadata(&ap).unwrap().modified().unwrap();
        s.parsed.get_mut(&a).unwrap().2.title = "缓存".into();
        assert_eq!(std::fs::metadata(&ap).unwrap().modified().unwrap(), stamp);
        let titles = |s: &mut Store| { let mut t: Vec<_> = s.list().into_iter().map(|e| e.title).collect(); t.sort(); t };
        assert_eq!(titles(&mut s), ["二改", "缓存"]);
        // Saving through the store, or deleting the file, is seen too.
        s.save(&a, &doc("一改")).unwrap();
        assert_eq!(titles(&mut s), ["一改", "二改"]);
        std::fs::remove_file(s.path(&b)).unwrap();
        assert_eq!(titles(&mut s), ["一改"]);
        assert_eq!(s.parsed.len(), 1);
    }

    #[test]
    fn reopening_after_releasing_baseline_still_detects_conflicts() {
        let lib = Lib::new("released-baseline");
        let mut s = lib.open();
        let id = s.create_from(&doc("initial")).unwrap();
        s.load_for_edit(&id, false).unwrap();
        s.forget_loaded(&id);
        std::fs::write(s.path(&id), docxml::serialize(&doc("external before reopen"))).unwrap();
        let reopened = s.load_for_edit(&id, false).unwrap();
        assert_eq!(reopened.title.text, "external before reopen");
        s.save(&id, &reopened).unwrap();
        let external = docxml::serialize(&doc("external after reopen"));
        std::fs::write(s.path(&id), &external).unwrap();
        assert!(s.save(&id, &doc("local edit")).is_err());
        assert_eq!(std::fs::read_to_string(s.path(&id)).unwrap(), external);
    }

    #[test]
    fn corrupt_index_falls_back_to_backup() {
        let lib = Lib::new("corrupt");
        let mut s = lib.open();
        let id = s.create_from(&doc("一")).unwrap();
        s.set_pinned(&id, true).unwrap();
        // The second write left the first (pinned: false) as the backup;
        // one more makes the backup pinned too.
        s.set_starred(&id, true).unwrap();
        std::fs::write(lib.0.join(".quiet/index.json"), b"{ not json").unwrap();
        let mut s = lib.open();
        assert_eq!(s.warnings.len(), 1, "{:?}", s.warnings);
        assert!(s.warnings[0].contains("已从备份恢复"));
        assert!(s.list()[0].meta.pinned);
        // The damaged file is kept for inspection, not overwritten.
        let kept = std::fs::read_dir(lib.0.join(".quiet")).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("index.json.corrupt-")).count();
        assert_eq!(kept, 1);
    }

    #[test]
    fn corrupt_index_without_backup_still_lists_documents() {
        let lib = Lib::new("nobak");
        let mut s = lib.open();
        s.create_from(&doc("一")).unwrap();
        std::fs::write(lib.0.join(".quiet/index.json"), b"garbage").unwrap();
        let _ = std::fs::remove_file(lib.0.join(".quiet/index.json.bak"));
        let mut s = lib.open();
        assert!(s.warnings[0].contains("没有可用备份"));
        assert_eq!(s.list().len(), 1);
    }

    #[test]
    fn pages_with_children_become_folders_in_their_old_order() {
        // A FORMAT 1 library: 父 holds 子一 (older) and 子二 (newer).
        let lib = Lib::new("migrate");
        let mut old = Old::new(&lib, 1);
        let p = old.doc("父", Meta::default());
        let a = old.doc("子一", Meta { parent: Some(p.clone()), created: 100, ..Default::default() });
        let b = old.doc("子二", Meta { parent: Some(p.clone()), created: 200, ..Default::default() });
        old.write();
        let mut s = lib.migrated();
        let list = s.list();
        let folder = list.iter().find(|e| e.meta.folder.as_deref() == Some("父")).expect("父 became a folder");
        assert!(folder.meta.parent.is_none());
        let mut inside: Vec<&Entry> = list.iter().filter(|e| e.meta.parent.as_deref() == Some(folder.id.as_str())).collect();
        inside.sort_by_key(|e| (e.meta.place(), e.id.clone()));
        let titles: Vec<&str> = inside.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, ["父", "子一", "子二"], "the page first, then its children oldest first");
        assert_eq!(inside.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), [p.as_str(), a.as_str(), b.as_str()]);
        assert!(lib.0.join("父/父.xml").is_file() && lib.0.join("父/子一.xml").is_file());
    }

    #[test]
    fn newer_format_is_refused() {
        // A newer build may have changed what the index means (folders, FORMAT 2):
        // opening it here could misread and then rewrite it, so it is refused.
        let lib = Lib::new("format");
        drop(lib.open());
        std::fs::write(lib.0.join(".quiet/format.json"), format!("{{\"format\": {}}}", FORMAT + 1)).unwrap();
        let err = Store::open_at(lib.0.clone()).err().expect("a newer library must not open");
        assert!(err.to_string().contains("更新版本"));
    }

    #[test]
    fn trash_round_trip() {
        let lib = Lib::new("trash");
        let mut s = lib.open();
        // Only folders hold pages: a folder goes to the trash with what is in it.
        let a = s.create_folder("父", None).unwrap();
        let b = s.create_from(&doc("子")).unwrap();
        s.set_parent(&b, Some(&a)).unwrap();
        s.delete(&b, Some(&a)).unwrap();
        s.delete(&a, None).unwrap();
        assert!(s.list().is_empty());
        let t = s.list_trash();
        assert_eq!(t.len(), 1, "the page rides along with its folder");
        assert_eq!(t[0].title, "父");
        let mut s = lib.open();
        assert!(!s.restore(&a).unwrap());
        let list = s.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list.iter().find(|e| e.id == b).unwrap().meta.parent.as_deref(), Some(a.as_str()));
        assert!(s.list_trash().is_empty());
    }

    #[test]
    fn unreadable_trash_folder_keeps_its_index() {
        let lib = Lib::new("trashdir");
        let mut s = lib.open();
        let a = s.create_from(&doc("一")).unwrap();
        s.delete(&a, None).unwrap();
        // Swap the folder for a file: read_dir fails.
        let trash = lib.0.join(".quiet/trash");
        let index = std::fs::read(trash.join("index.json")).unwrap();
        std::fs::rename(&trash, lib.0.join(".quiet/trash-away")).unwrap();
        std::fs::write(&trash, b"").unwrap();
        let mut s = Store {
            #[cfg(feature = "sqlite-store")]
            sqlite: None,
            file_changes: vec![], root: lib.0.clone(), index: HashMap::new(), flat: false, migrated: None, trash: serde_json::from_slice(&index).unwrap(), warnings: vec![], loaded: Default::default(), copied: Default::default(), parsed: HashMap::new(), batch: Default::default() };
        assert_eq!(s.list_trash().len(), 1);
        assert_eq!(s.trash.len(), 1);
    }

    /// Two copies of the app on one library: neither writes back a stale
    /// index over what the other just changed.
    #[test]
    fn two_instances_keep_each_others_entries() {
        let lib = Lib::new("two");
        let mut a = lib.open();
        let mut b = lib.open();
        let one = a.create_from(&doc("一")).unwrap();
        let two = b.create_from(&doc("二")).unwrap();
        a.set_pinned(&one, true).unwrap();
        b.set_starred(&two, true).unwrap();
        let mut c = lib.open();
        let list = c.list();
        assert_eq!(list.len(), 2);
        assert!(list.iter().find(|e| e.id == one).unwrap().meta.pinned, "b kept a's entry");
        assert!(list.iter().find(|e| e.id == two).unwrap().meta.starred > 0, "a kept b's entry");
    }

    #[test]
    fn an_instance_does_not_undo_another_ones_delete() {
        let lib = Lib::new("two-del");
        let mut a = lib.open();
        let mut b = lib.open();
        let id = a.create_from(&doc("一")).unwrap();
        let two = a.create_from(&doc("二")).unwrap();
        a.delete(&id, None).unwrap();
        // `b` still has `id` in memory and changes it: it must not come back.
        b.set_pinned(&id, true).unwrap();
        let mut c = lib.open();
        assert_eq!(c.list().iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec![two.as_str()]);
        assert_eq!(c.list_trash().len(), 1);
    }

    #[test]
    fn stale_temporary_path_does_not_block_save() {
        let lib = Lib::new("savefail");
        let mut s = lib.open();
        let id = s.create_from(&doc("旧")).unwrap();
        std::fs::create_dir(with_suffix(&s.path(&id), ".tmp")).unwrap();
        s.save(&id, &doc("新")).unwrap();
        assert_eq!(s.load(&id).unwrap().title.text, "新");
    }

    /// A library in the old layout (FORMAT 3 and older), written by hand.
    struct Old<'a> { lib: &'a Lib, format: u32, index: HashMap<String, Meta>, trash: HashMap<String, Trashed> }

    impl<'a> Old<'a> {
        fn new(lib: &'a Lib, format: u32) -> Self {
            std::fs::create_dir_all(lib.0.join("docs")).unwrap();
            Old { lib, format, index: HashMap::new(), trash: HashMap::new() }
        }
        fn put(&mut self, d: &Document, meta: Meta) -> String {
            let id = uuid::Uuid::new_v4().to_string();
            std::fs::write(self.lib.0.join("docs").join(format!("{id}.xml")), docxml::serialize(d)).unwrap();
            self.index.insert(id.clone(), Meta { created: meta.created.max(1), modified: 1, ..meta });
            id
        }
        fn doc(&mut self, title: &str, meta: Meta) -> String { self.put(&doc(title), meta) }
        fn raw(&mut self, xml: &str, meta: Meta) -> String {
            let id = uuid::Uuid::new_v4().to_string();
            std::fs::write(self.lib.0.join("docs").join(format!("{id}.xml")), xml).unwrap();
            self.index.insert(id.clone(), Meta { created: 1, modified: 1, ..meta });
            id
        }
        fn md(&mut self, name: &str, body: &str, meta: Meta) -> String {
            let id = uuid::Uuid::new_v4().to_string();
            std::fs::write(self.lib.0.join("docs").join(format!("{id}.md")), body).unwrap();
            self.index.insert(id.clone(), Meta { created: 1, modified: 1, markdown: true, name: Some(name.into()), ..meta });
            id
        }
        fn folder(&mut self, name: &str, parent: Option<&str>) -> String {
            let id = uuid::Uuid::new_v4().to_string();
            self.index.insert(id.clone(), Meta { folder: Some(name.into()), parent: parent.map(str::to_string), created: 1, modified: 1, ..Default::default() });
            id
        }
        fn trashed(&mut self, title: &str) -> String {
            let id = uuid::Uuid::new_v4().to_string();
            std::fs::create_dir_all(self.lib.0.join("trash")).unwrap();
            std::fs::write(self.lib.0.join("trash").join(format!("{id}.xml")), docxml::serialize(&doc(title))).unwrap();
            self.trash.insert(id.clone(), Trashed { meta: Meta { created: 1, modified: 1, ..Default::default() }, deleted: now(), root: None });
            id
        }
        fn write(&self) {
            std::fs::write(self.lib.0.join("index.json"), serde_json::to_vec(&self.index).unwrap()).unwrap();
            std::fs::create_dir_all(self.lib.0.join("trash")).unwrap();
            std::fs::write(self.lib.0.join("trash/index.json"), serde_json::to_vec(&self.trash).unwrap()).unwrap();
            std::fs::write(self.lib.0.join("format.json"), format!("{{\"format\": {}}}", self.format)).unwrap();
        }
    }

    /// `doc` with a paragraph mentioning each of `targets`.
    fn linking(title: &str, targets: &[(&str, &str)]) -> Document {
        let mut d = doc(title);
        for (id, t) in targets {
            let mut text = crate::model::RichText::plain("见 ");
            text.push("\u{fffc}", crate::model::Marks { inline: Some(docxml::to_inline(crate::model::InlineKind::DocLink(crate::model::InlineDocLink { id: id.to_string(), title: t.to_string() }))), ..Default::default() });
            let mut b = crate::model::Block::text("");
            b.text = text;
            d.blocks.push(b);
        }
        d
    }

    /// The names of the folders `id` is in, outermost first.
    fn folders_of(list: &[Entry], id: &str) -> Vec<String> {
        let by: HashMap<&str, &Entry> = list.iter().map(|e| (e.id.as_str(), e)).collect();
        let mut out = vec![];
        let mut p = by[id].meta.parent.as_deref();
        while let Some(f) = p { out.insert(0, by[f].title.clone()); p = by[f].meta.parent.as_deref(); }
        out
    }

    fn targets(e: &Entry) -> Vec<String> { let mut t: Vec<String> = e.links.iter().map(|l| l.target.clone()).collect(); t.sort(); t }

    /// FORMAT 3 → 4: every document keeps its id, text, folder and links;
    /// files are named by title (clashes numbered, illegal characters
    /// replaced), links become relative paths, Markdown stays Markdown,
    /// 本机文件 stay where they are, the 回收站 and the app's own files move
    /// into `.quiet/`, and the old layout is kept whole as a backup.
    #[test]
    fn format_3_moves_to_real_folders() {
        let lib = Lib::new("to-folders");
        let outside = Lib::new("to-folders-outside");
        std::fs::create_dir_all(&outside.0).unwrap();
        std::fs::write(outside.0.join("外面.md"), "外面的笔记\n").unwrap();
        let mut old = Old::new(&lib, 3);
        let work = old.folder("工作", None);
        let sub = old.folder("子", Some(&work));
        let deep = old.folder("深", Some(&sub));
        let same1 = old.folder("同名", None);
        let same2 = old.folder("同名", None);
        old.index.get_mut(&same2).unwrap().created = 2;
        let slash = old.folder("a/b", None);
        let lost = old.folder("孤儿", Some("no-such-folder"));
        let w1 = old.doc("周报", Meta { parent: Some(work.clone()), created: 10, pinned: true, opened: 77, ..Default::default() });
        let w2 = old.doc("周报", Meta { parent: Some(work.clone()), created: 20, ..Default::default() });
        let top = old.doc("周报", Meta { created: 30, ..Default::default() });
        let odd = old.doc("x:y?<z>", Meta { parent: Some(sub.clone()), ..Default::default() });
        let empty = old.doc("", Meta { parent: Some(slash.clone()), ..Default::default() });
        let con = old.doc("CON", Meta::default());
        let dot = old.doc(".hidden", Meta { parent: Some(lost.clone()), ..Default::default() });
        let md = old.md("随手记", "看图 ![](assets/p.png)\n", Meta { parent: Some(deep.clone()), ..Default::default() });
        let index_doc = old.put(&linking("索引", &[(&w1, "周报"), (&md, "随手记"), ("gone-id", "已删除")]), Meta::default());
        let back = old.put(&linking("回链", &[(&index_doc, "索引"), (&w2, "周报")]), Meta { parent: Some(deep.clone()), ..Default::default() });
        let pic = old.raw("<title>图</title>\n<img src=\"assets/p.png\"/>\n<p>=\"assets/x\"</p>\n", Meta { parent: Some(same2.clone()), ..Default::default() });
        let local = uuid::Uuid::new_v4().to_string();
        let local_file = outside.0.canonicalize().unwrap().join("外面.md");
        old.index.insert(local.clone(), Meta { created: 1, modified: 1, markdown: true, path: Some(local_file.to_string_lossy().into()), opened: 5, ..Default::default() });
        // An entry whose file is gone is dropped.
        old.index.insert(uuid::Uuid::new_v4().to_string(), Meta { created: 1, modified: 1, ..Default::default() });
        let trashed = old.trashed("删掉的");
        // Another computer's 本机文件: asked before moving.
        let away = uuid::Uuid::new_v4().to_string();
        old.index.insert(away.clone(), Meta { created: 1, modified: 1, markdown: true, path: Some("/no/such/notes.md".into()), ..Default::default() });
        old.write();
        std::fs::create_dir_all(lib.0.join("assets")).unwrap();
        std::fs::write(lib.0.join("assets/p.png"), b"png").unwrap();
        std::fs::create_dir_all(lib.0.join("themes")).unwrap();
        std::fs::write(lib.0.join("themes/mine.json"), b"{}").unwrap();
        std::fs::write(lib.0.join("preferences.json"), b"{\"a\":1}").unwrap();
        std::fs::write(lib.0.join("settings.json"), b"{\"window_size\":[800.0,600.0],\"sidebar_width\":250.0}").unwrap();
        // What the old layout says, to compare with.
        let before: HashMap<String, (String, String)> = std::fs::read_dir(lib.0.join("docs")).unwrap().flatten().map(|e| {
            let p = e.path();
            let id = p.file_stem().unwrap().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(&p).unwrap();
            let d = if p.extension().unwrap() == "md" { crate::markdown::parse_file(&text, "", None) } else { docxml::parse(&text).unwrap() };
            (id, (d.title.text.clone(), d.plain_text()))
        }).collect();

        assert_eq!(Store::migration_state(&lib.0), MigrationState::NeedsConsent(ConsentReason::OtherDevice(DeviceHint::MissingFile("/no/such/notes.md".into()))));
        old.index.retain(|k, _| k != &away);
        old.write();
        assert_eq!(Store::migration_state(&lib.0), MigrationState::Silent);
        let report = Store::migrate(&lib.0).unwrap();
        assert_eq!((report.docs, report.folders), (before.len(), 7));
        assert_eq!(report.backup, lib.0.join(".quiet").join(migrate::BACKUP));
        assert_eq!(Store::migration_state(&lib.0), MigrationState::NotNeeded);
        let mut s = lib.open();
        let list = s.list();
        assert!(s.warnings.is_empty(), "{:?}", s.warnings);
        let by: HashMap<&str, &Entry> = list.iter().map(|e| (e.id.as_str(), e)).collect();
        // Text and titles.
        for (id, (title, body)) in &before {
            let d = s.load(id).unwrap();
            assert_eq!(d.plain_text(), body.replace("](assets/", "](.assets/"), "{title}");
            if id != &md { assert_eq!(d.title.text, *title); }
        }
        assert_eq!(by[md.as_str()].title, "随手记");
        // Folders.
        assert_eq!(folders_of(&list, &w1), ["工作"]);
        assert_eq!(folders_of(&list, &odd), ["工作", "子"]);
        assert_eq!(folders_of(&list, &md), ["工作", "子", "深"]);
        assert_eq!(folders_of(&list, &back), ["工作", "子", "深"]);
        assert_eq!(folders_of(&list, &empty), ["a／b"]);
        assert_eq!(folders_of(&list, &dot), ["孤儿"]);
        assert_eq!(folders_of(&list, &pic), ["同名 2"]);
        assert!(folders_of(&list, &top).is_empty() && folders_of(&list, &con).is_empty());
        assert_eq!(by[same1.as_str()].title, "同名");
        // Files named by title.
        for (id, rel) in [(&w1, "工作/周报.xml"), (&w2, "工作/周报 2.xml"), (&top, "周报.xml"), (&odd, "工作/子/x：y？＜z＞.xml"), (&empty, "a／b/未命名文档.xml"),
            (&con, "CON_.xml"), (&dot, "孤儿/hidden.xml"), (&md, "工作/子/深/随手记.md"), (&index_doc, "索引.xml"), (&back, "工作/子/深/回链.xml"), (&pic, "同名 2/图.xml")] {
            assert_eq!(s.file_path(id).unwrap(), s.abs(rel), "{rel}");
            assert!(s.abs(rel).is_file(), "{rel}");
        }
        // Links: the same targets, written as relative paths.
        assert_eq!(targets(by[index_doc.as_str()]), { let mut t = vec![w1.clone(), md.clone(), "gone-id".into()]; t.sort(); t });
        assert_eq!(targets(by[back.as_str()]), { let mut t = vec![index_doc.clone(), w2.clone()]; t.sort(); t });
        let text = std::fs::read_to_string(s.abs("索引.xml")).unwrap();
        assert!(text.contains("href=\"工作/周报.xml\"") && text.contains("href=\"工作/子/深/随手记.md\"") && text.contains("doc-id=\"gone-id\""), "{text}");
        let text = std::fs::read_to_string(s.abs("工作/子/深/回链.xml")).unwrap();
        assert!(text.contains("href=\"../../../索引.xml\"") && text.contains("href=\"../../周报 2.xml\""), "{text}");
        // No id is written in any document.
        for id in before.keys() { for e in &list { if let Some(p) = s.file_path(&e.id) && !e.meta.local() { assert!(!std::fs::read_to_string(p).unwrap().contains(id.as_str())); } } }
        // Images: `.assets/`, the sources with them.
        assert!(lib.0.join(".assets/p.png").is_file() && !lib.0.join("assets").exists());
        assert_eq!(std::fs::read_to_string(s.abs("同名 2/图.xml")).unwrap(), "<title>图</title>\n<img src=\".assets/p.png\"/>\n<p>=\"assets/x\"</p>\n");
        assert_eq!(std::fs::read_to_string(s.abs("工作/子/深/随手记.md")).unwrap(), "看图 ![](.assets/p.png)\n");
        // Attached and local information.
        assert!(by[w1.as_str()].meta.pinned);
        assert_eq!(by[w1.as_str()].meta.opened, 77);
        let index = std::fs::read_to_string(lib.0.join(".quiet/index.json")).unwrap();
        assert!(!index.contains("opened") && !index.contains("外面") && index.contains("\"工作/子/\""));
        assert!(by[local.as_str()].meta.local());
        assert_eq!(s.load(&local).unwrap().plain_text(), "外面的笔记");
        assert!(local_file.is_file());
        assert_eq!(s.settings().window_size, Some([800., 600.]));
        assert_eq!(s.settings().sidebar_width, Some(250.));
        assert!(lib.0.join(".quiet/preferences.json").is_file() && lib.0.join(".quiet/themes/mine.json").is_file());
        // What the user sees: their folders and notes only.
        let mut top_names: Vec<String> = std::fs::read_dir(&lib.0).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        top_names.sort();
        assert_eq!(top_names, [".assets", ".quiet", "CON_.xml", "a／b", "同名", "同名 2", "周报.xml", "孤儿", "工作", "索引.xml"]);
        // The old layout, whole, as a backup.
        let backup = lib.0.join(".quiet").join(migrate::BACKUP);
        for id in before.keys() { assert!(backup.join("docs").join(format!("{id}.xml")).is_file() || backup.join("docs").join(format!("{id}.md")).is_file()); }
        assert!(backup.join("index.json").is_file() && backup.join("format.json").is_file() && backup.join("trash/index.json").is_file());
        // The 回收站 came along.
        let t = s.list_trash();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].title, "删掉的");
        drop(s);
        // Reopening changes nothing.
        let mut s = lib.open();
        let again = s.list();
        assert!(s.warnings.is_empty(), "{:?}", s.warnings);
        assert_eq!({ let mut a: Vec<_> = again.iter().map(|e| (e.id.clone(), e.title.clone(), e.meta.parent.clone())).collect(); a.sort(); a },
            { let mut a: Vec<_> = list.iter().map(|e| (e.id.clone(), e.title.clone(), e.meta.parent.clone())).collect(); a.sort(); a });
        // Saving what was loaded writes the same bytes: the new layout round-trips.
        for e in again.iter().filter(|e| e.meta.folder.is_none()) {
            let p = s.file_path(&e.id).unwrap();
            let bytes = std::fs::read(&p).unwrap();
            let d = s.load_for_edit(&e.id, false).unwrap();
            s.save(&e.id, &d).unwrap();
            assert_eq!(s.file_path(&e.id).unwrap(), p, "{}", e.title);
            // (图 was written by hand, not in the canonical form: stable from its first save.)
            if e.id == pic { let d = s.load_for_edit(&e.id, false).unwrap(); let once = std::fs::read(&p).unwrap(); s.save(&e.id, &d).unwrap(); assert_eq!(std::fs::read(&p).unwrap(), once); continue; }
            assert_eq!(std::fs::read(&p).unwrap(), bytes, "{}", e.title);
        }
        s.restore(&trashed).unwrap();
        assert_eq!(s.file_path(&trashed).unwrap(), s.abs("删掉的.xml"));
    }

    /// A move that fails half way leaves the old library exactly as it was;
    /// one interrupted by a crash is undone on the next launch, then done.
    #[test]
    fn failed_migration_leaves_the_old_library() {
        let lib = Lib::new("fail-migration");
        let mut old = Old::new(&lib, 3);
        let f = old.folder("夹", None);
        for i in 0..4 { old.doc(&format!("文{i}"), Meta { parent: Some(f.clone()), ..Default::default() }); }
        old.write();
        std::fs::create_dir_all(lib.0.join("assets")).unwrap();
        let snapshot = |root: &Path| { let mut v = vec![]; fn walk(d: &Path, v: &mut Vec<(PathBuf, Vec<u8>)>) { for e in std::fs::read_dir(d).unwrap().flatten() { let p = e.path(); if p.is_dir() { v.push((p.clone(), vec![])); walk(&p, v); } else { v.push((p.clone(), std::fs::read(&p).unwrap())); } } } walk(root, &mut v); v.sort(); v };
        let before: Vec<_> = snapshot(&lib.0).into_iter().filter(|(p, _)| !p.starts_with(lib.0.join(".quiet")) && !p.ends_with(".lock")).collect();
        let err = Store::migrate(&lib.0).expect_err("the injected failure");
        assert!(err.to_string().contains("已保持原样"), "{err}");
        let after: Vec<_> = snapshot(&lib.0).into_iter().filter(|(p, _)| !p.starts_with(lib.0.join(".quiet")) && !p.ends_with(".lock")).collect();
        assert_eq!(after, before);
        assert!(!lib.0.join(".quiet/format.json").exists() && !lib.0.join(".quiet/index.json").exists());
        // It still opens and works, in the old layout.
        let mut s = lib.open();
        assert_eq!(s.list().len(), 5);
        assert!(!s.create_from(&doc("还能写")).unwrap().is_empty());
        drop(s);

        // A crash: the plan and some of what it wrote are there.
        let crashed = Lib::new("crashed");
        let mut old = Old::new(&crashed, 3);
        let a = old.doc("甲", Meta::default());
        old.write();
        std::fs::create_dir_all(crashed.0.join(".quiet")).unwrap();
        std::fs::create_dir_all(crashed.0.join("半路")).unwrap();
        std::fs::write(crashed.0.join("甲.xml"), b"half").unwrap();
        std::fs::write(crashed.0.join(".quiet/migration.json"), b"{\"files\":[\"\xe7\x94\xb2.xml\"],\"dirs\":[\"\xe5\x8d\x8a\xe8\xb7\xaf\"],\"internal\":[],\"assets\":false}").unwrap();
        let mut s = crashed.migrated();
        assert!(!crashed.0.join("半路").exists());
        assert_eq!(s.load(&a).unwrap().title.text, "甲");
        assert_eq!(s.file_path(&a).unwrap(), s.abs("甲.xml"));
        assert_eq!(s.list().len(), 1);
        // Committed but not cleaned up: the old layout is set aside on open.
        std::fs::write(crashed.0.join("format.json"), b"{\"format\": 3}").unwrap();
        std::fs::create_dir_all(crashed.0.join("docs")).unwrap();
        drop(s);
        let mut s = crashed.open();
        assert!(!crashed.0.join("format.json").exists() && !crashed.0.join("docs").exists());
        assert!(crashed.0.join(".quiet").join(migrate::BACKUP).join("format.json.2").is_file());
        assert_eq!(s.list().len(), 1);
    }

    /// Moving and renaming change the files, and the links pointing at them
    /// (and out of them) follow; an open editor's baseline follows too.
    #[test]
    fn moves_and_renames_keep_links() {
        let lib = Lib::new("relink");
        let mut s = lib.open();
        let a = s.create_from(&doc("甲")).unwrap();
        let b = s.create_from(&linking("乙", &[(&a, "甲")])).unwrap();
        let a_doc = linking("甲", &[(&b, "乙")]);
        s.save(&a, &a_doc).unwrap();
        assert!(std::fs::read_to_string(s.abs("甲.xml")).unwrap().contains("href=\"乙.xml\""));
        s.load_for_edit(&a, false).unwrap();
        let f = s.create_folder("夹", None).unwrap();
        assert!(lib.0.join("夹").is_dir());
        s.set_parent(&b, Some(&f)).unwrap();
        assert!(s.abs("夹/乙.xml").is_file() && !s.abs("乙.xml").exists());
        assert!(std::fs::read_to_string(s.abs("甲.xml")).unwrap().contains("href=\"夹/乙.xml\""));
        assert!(std::fs::read_to_string(s.abs("夹/乙.xml")).unwrap().contains("href=\"../甲.xml\""));
        // The open editor of 甲 saves without a conflict.
        s.save(&a, &a_doc).unwrap();
        s.rename(&f, "新夹").unwrap();
        assert!(std::fs::read_to_string(s.abs("甲.xml")).unwrap().contains("href=\"新夹/乙.xml\""));
        // A title typed in the editor renames the file; a taken name is numbered.
        s.save(&a, &linking("丙", &[(&b, "乙")])).unwrap();
        assert!(s.abs("丙.xml").is_file() && !s.abs("甲.xml").exists());
        assert!(std::fs::read_to_string(s.abs("新夹/乙.xml")).unwrap().contains("href=\"../丙.xml\""));
        let c = s.create_from(&doc("丙")).unwrap();
        assert_eq!(s.file_path(&c).unwrap(), s.abs("丙 2.xml"));
        s.save(&c, &doc("丙")).unwrap();
        assert_eq!(s.file_path(&c).unwrap(), s.abs("丙 2.xml"), "a numbered name fits its title");
        let list = s.list();
        assert_eq!(targets(list.iter().find(|e| e.id == b).unwrap()), std::slice::from_ref(&a));
        // Undo of the move puts the file back, links too.
        let journal = s.track_file_changes();
        s.set_parent(&b, None).unwrap();
        let changes = journal.take();
        assert!(s.abs("乙.xml").is_file());
        s.apply_file_history(&changes, false).unwrap();
        assert!(s.abs("新夹/乙.xml").is_file() && !s.abs("乙.xml").exists());
        assert!(std::fs::read_to_string(s.abs("丙.xml")).unwrap().contains("href=\"新夹/乙.xml\""));
    }

    /// What is done in the Finder shows: new folders and files, removed ones,
    /// iCloud files not downloaded yet. Deleting `.quiet/` loses only the
    /// attached information: notes, folders and links are all in the files.
    #[test]
    fn the_disk_is_the_library() {
        let lib = Lib::new("disk");
        let mut s = lib.open();
        let a = s.create_from(&doc("甲")).unwrap();
        let f = s.create_folder("夹", None).unwrap();
        let b = s.create_from(&linking("乙", &[(&a, "甲")])).unwrap();
        s.set_parent(&b, Some(&f)).unwrap();
        s.set_pinned(&a, true).unwrap();
        std::fs::create_dir_all(lib.0.join("外部/更深")).unwrap();
        std::fs::write(lib.0.join("外部/更深/新.md"), "从访达放进来\n").unwrap();
        std::fs::write(lib.0.join("外部/.隐藏.md"), "x").unwrap();
        std::fs::write(lib.0.join("外部/.云端.xml.icloud"), "").unwrap();
        std::fs::write(lib.0.join("外部/图.png"), "").unwrap();
        let list = s.list();
        let new = list.iter().find(|e| e.title == "新").expect("a file from the Finder");
        assert!(new.meta.markdown);
        assert_eq!(folders_of(&list, &new.id), ["外部", "更深"]);
        let cloud = list.iter().find(|e| e.title == "云端").expect("an iCloud placeholder is listed");
        assert!(s.load(&cloud.id).unwrap_err().to_string().contains("iCloud"));
        assert_eq!(list.len(), 7, "{:?}", list.iter().map(|e| &e.title).collect::<Vec<_>>());
        let new_id = new.id.clone();
        assert_eq!(s.list().iter().find(|e| e.title == "新").unwrap().id, new_id, "ids stay");
        std::fs::remove_dir_all(lib.0.join("外部")).unwrap();
        assert_eq!(s.list().len(), 3);
        drop(s);
        std::fs::remove_dir_all(lib.0.join(".quiet")).unwrap();
        let mut s = lib.open();
        let list = s.list();
        assert_eq!(list.len(), 3);
        let a2 = list.iter().find(|e| e.title == "甲").unwrap();
        let b2 = list.iter().find(|e| e.title == "乙").unwrap();
        assert!(!a2.meta.pinned, "attached information is lost, nothing else");
        assert_eq!(folders_of(&list, &b2.id), ["夹"]);
        assert_eq!(targets(b2), std::slice::from_ref(&a2.id));
    }

    /// A folder deleted first takes what is in it along; deleting those
    /// afterwards (as the window does) changes nothing; restoring brings the
    /// folder back with everything, and undo puts it all back too.
    #[test]
    fn folders_go_to_the_trash_whole() {
        let lib = Lib::new("trash-folder");
        let mut s = lib.open();
        let f = s.create_folder("夹", None).unwrap();
        let g = s.create_folder("里", Some(&f)).unwrap();
        let a = s.create_from(&doc("甲")).unwrap();
        s.set_parent(&a, Some(&g)).unwrap();
        std::fs::write(lib.0.join("夹/里/附件.pdf"), b"pdf").unwrap();
        let journal = s.track_file_changes();
        s.delete(&f, None).unwrap();
        s.delete(&g, Some(&f)).unwrap();
        s.delete(&a, Some(&f)).unwrap();
        let changes = journal.take();
        assert_eq!(changes.len(), 3);
        assert!(!lib.0.join("夹").exists());
        assert!(s.list().is_empty());
        assert_eq!(s.list_trash().len(), 1);
        s.restore(&f).unwrap();
        assert!(s.abs("夹/里/甲.xml").is_file() && s.abs("夹/里/附件.pdf").is_file());
        let list = s.list();
        assert_eq!(folders_of(&list, &a), ["夹", "里"]);
        s.delete(&f, None).unwrap();
        s.apply_file_history(&changes, false).unwrap();
        assert!(s.abs("夹/里/甲.xml").is_file() && s.abs("夹/里/附件.pdf").is_file());
        assert_eq!(s.list().len(), 3);
    }

    /// Until the user agrees, the old layout opens as it is and is written as
    /// older builds expect; the move afterwards takes along what was done.
    #[test]
    fn the_old_layout_keeps_working_until_the_move() {
        let lib = Lib::new("flat");
        let mut old = Old::new(&lib, 2);
        let f = old.folder("夹", None);
        let a = old.doc("甲", Meta { parent: Some(f.clone()), ..Default::default() });
        old.write();
        let mut s = lib.open();
        assert!(s.flat);
        let b = s.create_from(&linking("乙", &[(&a, "甲")])).unwrap();
        assert!(lib.0.join("docs").join(format!("{b}.xml")).is_file());
        assert!(std::fs::read_to_string(lib.0.join("docs").join(format!("{b}.xml"))).unwrap().contains(&format!("doc-id=\"{a}\"")));
        let g = s.create_folder("新夹", None).unwrap();
        s.set_parent(&b, Some(&g)).unwrap();
        s.rename(&g, "改名夹").unwrap();
        let md = s.create_markdown(&doc("记"), "").unwrap();
        assert!(lib.0.join("docs").join(format!("{md}.md")).is_file());
        s.delete(&a, None).unwrap();
        assert!(lib.0.join("trash").join(format!("{a}.xml")).is_file());
        s.restore(&a).unwrap();
        s.set_pinned(&a, true).unwrap();
        s.touch(&a);
        let mut st = s.settings();
        st.window_size = Some([1., 2.]);
        s.save_settings(&st).unwrap();
        let list = s.list();
        assert_eq!(list.len(), 5);
        assert_eq!(folders_of(&list, &b), ["改名夹"]);
        assert!(!lib.0.join(".quiet").exists() && !lib.0.join("改名夹").exists(), "nothing of the new layout yet");
        let index: HashMap<String, Meta> = serde_json::from_slice(&std::fs::read(lib.0.join("index.json")).unwrap()).unwrap();
        assert!(index[&a].pinned && index[&a].opened > 0 && index[&md].markdown);
        assert_eq!(serde_json::from_slice::<FormatMarker>(&std::fs::read(lib.0.join("format.json")).unwrap()).unwrap().format, 3);
        drop(s);
        let report = Store::migrate(&lib.0).unwrap();
        assert_eq!((report.docs, report.folders), (3, 2));
        let mut s = lib.open();
        assert!(!s.flat);
        let list = s.list();
        assert_eq!(folders_of(&list, &b), ["改名夹"]);
        assert!(s.abs("改名夹/乙.xml").is_file() && s.abs("夹/甲.xml").is_file() && s.abs("记.md").is_file());
        assert!(std::fs::read_to_string(s.abs("改名夹/乙.xml")).unwrap().contains("href=\"../夹/甲.xml\""));
        assert!(list.iter().find(|e| e.id == a).unwrap().meta.pinned);
        assert_eq!(s.settings().window_size, Some([1., 2.]));
    }

    /// Pasted images and attachments kept their absolute path in `assets/`;
    /// after the move to real folders (`.assets/`), or after the library
    /// itself moved, they are found where they are now (#213). Nothing
    /// outside the library's images folder is reached that way.
    #[test]
    fn absolute_asset_paths_survive_the_move() {
        use crate::model::{Block, BlockKind, FileBlock, FileView, RichText};
        let lib = Lib::new("absolute-assets");
        let mut old = Old::new(&lib, 3);
        let abs = |rel: &str| lib.0.join(rel).to_string_lossy().into_owned();
        let image = |src: String| Block::new(BlockKind::Image { src_kind: "src".into(), src, width: None, height: None, caption: None, crop: None }, RichText::default());
        let moved = std::env::temp_dir().join("elsewhere/Quiet.md/assets/p.png").to_string_lossy().into_owned();
        let d = Document { title: RichText::plain("图"), blocks: vec![
            image(abs("assets/p.png")),
            Block::new(BlockKind::File(FileBlock { src_kind: "path".into(), src: abs("assets/files/1234/报告.pdf"), name: "报告.pdf".into(), view: FileView::Card }), RichText::default()),
            image(moved.clone()),
            image(abs("assets/gone.png")),
            // Only through `.assets/x/` once moved: the fallback must not follow it.
            image(std::env::temp_dir().join("elsewhere/Quiet.md/assets/x/../../secret.png").to_string_lossy().into_owned()),
        ], ..Default::default() };
        let id = old.put(&d, Meta::default());
        old.write();
        std::fs::create_dir_all(lib.0.join("assets/files/1234")).unwrap();
        std::fs::write(lib.0.join("assets/p.png"), b"png").unwrap();
        std::fs::write(lib.0.join("assets/files/1234/报告.pdf"), b"pdf").unwrap();
        std::fs::write(lib.0.join("secret.png"), b"s").unwrap();
        let s = lib.migrated();
        assert!(!lib.0.join("assets").exists());
        std::fs::create_dir_all(lib.0.join(".assets/x")).unwrap();
        let srcs = s.load(&id).unwrap().asset_sources();
        let found: Vec<Option<PathBuf>> = srcs.iter().map(|src| image_path_in(&lib.0, src).filter(|p| p.is_file())).collect();
        assert_eq!(found, [Some(lib.0.join(".assets/p.png")), Some(lib.0.join(".assets/files/1234/报告.pdf")), Some(lib.0.join(".assets/p.png")), None, None]);
        assert!(export_path_in(&lib.0, &srcs[0]).is_some() && export_path_in(&lib.0, &srcs[1]).is_some());
        assert!(export_path_in(&lib.0, &srcs[3]).is_none() && export_path_in(&lib.0, &srcs[4]).is_none());
        // A path that still exists is used as it is.
        std::fs::write(lib.0.join("here.png"), b"h").unwrap();
        assert_eq!(image_path_in(&lib.0, &abs("here.png")), Some(lib.0.join("here.png")));
    }

    /// 恢复到迁移前: the backup comes back as it was, what the new layout
    /// held is set aside, and the library is not moved again without asking.
    /// Pruning keeps a backup 30 days and three launches.
    #[test]
    fn going_back_to_the_old_layout() {
        let lib = Lib::new("restore-backup");
        let mut old = Old::new(&lib, 3);
        let f = old.folder("夹", None);
        let a = old.doc("甲", Meta { parent: Some(f.clone()), ..Default::default() });
        old.write();
        std::fs::create_dir_all(lib.0.join("assets")).unwrap();
        std::fs::write(lib.0.join("assets/p.png"), b"p").unwrap();
        std::fs::write(lib.0.join("preferences.json"), b"{\"old\":1}").unwrap();
        let original = std::fs::read(lib.0.join("docs").join(format!("{a}.xml"))).unwrap();
        let mut s = lib.migrated();
        // Edits are told from the move by modification time; some file
        // systems stamp a little behind the clock, so leave a gap.
        std::thread::sleep(std::time::Duration::from_millis(50));
        s.save(&a, &doc("甲改")).unwrap();
        s.create_from(&doc("新的")).unwrap();
        std::fs::write(lib.0.join(".quiet/preferences.json"), b"{\"new\":1}").unwrap();
        drop(s);
        // Pruning: not before both 30 days and three launches.
        let info = lib.0.join(".quiet").join(migrate::BACKUP).with_extension("json");
        for _ in 0..5 { Store::prune_backups(&lib.0); }
        assert!(Store::backup_dir(&lib.0).is_some());
        let report = Store::restore_backup(&lib.0).unwrap();
        assert_eq!(report.changed, 2);
        assert!(report.kept.join("夹/甲改.xml").is_file() && report.kept.join("新的.xml").is_file());
        assert_eq!(std::fs::read(lib.0.join("docs").join(format!("{a}.xml"))).unwrap(), original);
        assert!(lib.0.join("assets/p.png").is_file() && !lib.0.join(".assets").exists());
        assert_eq!(std::fs::read(lib.0.join("preferences.json")).unwrap(), b"{\"new\":1}");
        assert!(!lib.0.join("夹").exists() && !lib.0.join(".quiet/format.json").exists());
        assert_eq!(Store::migration_state(&lib.0), MigrationState::NeedsConsent(ConsentReason::Restored));
        let mut s = lib.open();
        assert!(s.flat);
        assert_eq!(s.load(&a).unwrap().title.text, "甲");
        assert_eq!(s.list().len(), 2);
        drop(s);
        // Moved again on consent, with a fresh backup.
        let report = Store::migrate(&lib.0).unwrap();
        assert_eq!(report.docs, 1);
        assert_eq!(Store::migration_state(&lib.0), MigrationState::NotNeeded);
        // An old backup is pruned after 30 days and three launches.
        std::fs::write(&info, b"{\"created\": 1, \"launches\": 0}").unwrap();
        for _ in 0..3 { Store::prune_backups(&lib.0); }
        assert!(Store::backup_dir(&lib.0).is_some());
        Store::prune_backups(&lib.0);
        assert!(Store::backup_dir(&lib.0).is_none() && !info.exists());
    }

    #[test]
    fn synced_folders_are_recognised() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(sync_of(&home.join("Library/Mobile Documents/com~apple~CloudDocs/Quiet.md")), SyncService::ICloud);
        assert_eq!(sync_of(&home.join("Library/CloudStorage/OneDrive-Personal/Quiet.md")), SyncService::Provider("OneDrive".into()));
        assert_eq!(sync_of(&std::env::temp_dir().join("x")), SyncService::Local);
        // An old library with a conflict copy left by syncing asks first.
        let lib = Lib::new("conflict");
        let mut old = Old::new(&lib, 3);
        old.doc("甲", Meta::default());
        old.write();
        std::fs::write(lib.0.join("index 2.json"), b"{}").unwrap();
        assert!(matches!(Store::migration_state(&lib.0), MigrationState::NeedsConsent(ConsentReason::OtherDevice(_))));
    }

    /// A batch writes the documents where they go and the index once; in
    /// both layouts the result is what one write at a time gave.
    #[test]
    fn batches_write_the_index_once() {
        for flat in [false, true] {
            let lib = Lib::new("batch");
            if flat { Old::new(&lib, 3).write(); }
            let mut s = lib.open();
            assert_eq!(s.flat, flat);
            let index = lib.0.join(if flat { "index.json" } else { ".quiet/index.json" });
            let journal = s.track_file_changes();
            let (f, ids) = s.batch(|s| {
                let f = s.create_folder("导入", None).unwrap();
                let g = s.create_folder("子", Some(&f)).unwrap();
                let mut ids = vec![];
                for i in 0..5 { ids.push(s.create_markdown_in(&doc(&format!("笔记 {i}")), "", Some(&f)).unwrap()); }
                ids.push(s.create_from_in(&doc("深处"), Some(&g)).unwrap());
                // A failure midway leaves the rest of the batch alone.
                assert!(s.create_from_in(&doc("无处"), Some("不存在")).is_err());
                // Nothing reached the index yet.
                assert!(!std::fs::read_to_string(&index).unwrap_or_default().contains(&ids[0]));
                assert_eq!(s.batch(|s| s.create_markdown_in(&doc("嵌套"), "", Some(&f)).unwrap()).map(|_| ()).ok(), Some(()));
                (f, ids)
            }).unwrap();
            assert!(std::fs::read_to_string(&index).unwrap().contains(&ids[0]));
            if !flat {
                assert!(lib.0.join("导入/笔记 4.md").is_file() && lib.0.join("导入/子/深处.xml").is_file());
            }
            assert_eq!(journal.take().iter().filter(|d| d.before.is_none()).count(), 9);
            let mut again = lib.open();
            let list = again.list();
            assert_eq!(list.len(), 9);
            let kids: Vec<&Entry> = { let mut k: Vec<&Entry> = list.iter().filter(|e| e.meta.parent.as_deref() == Some(f.as_str()) && e.meta.folder.is_none()).collect(); k.sort_by_key(|e| e.meta.order); k };
            let titles: Vec<&str> = kids.iter().map(|e| e.title.as_str()).collect();
            assert_eq!(titles, ["笔记 0", "笔记 1", "笔记 2", "笔记 3", "笔记 4", "嵌套"]);
            assert_eq!(folders_of(&list, &ids[5]), ["导入", "子"]);
        }
    }

    /// A batch cut short (the app quit or crashed) never leaves an index
    /// entry without its file: what was written is found on the next look.
    #[test]
    fn an_unfinished_batch_leaves_a_consistent_library() {
        for flat in [false, true] {
            let lib = Lib::new("batch-cut");
            if flat { Old::new(&lib, 3).write(); }
            let mut s = lib.open();
            let kept = s.create_from(&doc("原有")).unwrap();
            let f = s.create_folder("夹", None).unwrap();
            s.begin_batch().unwrap();
            for i in 0..3 { s.create_from_in(&doc(&format!("新 {i}")), Some(&f)).unwrap(); }
            s.delete(&kept, None).unwrap();
            drop(s);
            let mut again = lib.open();
            let list = again.list();
            for e in &list {
                if e.meta.folder.is_none() { assert!(again.path(&e.id).is_file(), "{} has no file", e.title); }
            }
            let mut titles: Vec<&str> = list.iter().filter(|e| e.meta.folder.is_none()).map(|e| e.title.as_str()).collect();
            titles.sort();
            assert_eq!(titles, ["新 0", "新 1", "新 2"]);
            assert_eq!(again.list_trash().len(), 1);
        }
    }

    /// Deleting a big folder in a batch: two index writes, one 回收站 row,
    /// and 恢复 brings it all back.
    #[test]
    fn a_folder_deleted_in_a_batch_comes_back_whole() {
        for flat in [false, true] {
            let lib = Lib::new("batch-trash");
            if flat { Old::new(&lib, 3).write(); }
            let mut s = lib.open();
            let f = s.create_folder("夹", None).unwrap();
            let ids = s.batch(|s| (0..20).map(|i| s.create_from_in(&doc(&format!("文 {i}")), Some(&f)).unwrap()).collect::<Vec<_>>()).unwrap();
            let journal = s.track_file_changes();
            // As the window does it: the folder, then each page under it.
            s.batch(|s| {
                s.delete(&f, None).unwrap();
                for id in &ids { s.delete(id, Some(&f)).unwrap(); }
            }).unwrap();
            assert_eq!(journal.take().len(), 21);
            let mut again = lib.open();
            assert!(again.list().is_empty());
            assert_eq!(again.list_trash().len(), 1);
            again.restore(&f).unwrap();
            let list = again.list();
            assert_eq!(list.len(), 21);
            assert!(ids.iter().all(|id| folders_of(&list, id) == ["夹"]));
        }
    }

    /// 导入文件夹和删除大文件夹的耗时：
    /// `cargo test --release -p fdl_core import_speed -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn import_speed() {
        let notes = |s: &mut Store, f: &str, batch: bool| {
            let t = std::time::Instant::now();
            let run = |s: &mut Store| for i in 0..120 {
                let mut d = doc(&format!("笔记 {i}"));
                d.blocks.push(crate::model::Block::text("正文"));
                if batch { s.create_markdown_in(&d, "", Some(f)).unwrap(); }
                else { let id = s.create_markdown(&d, "").unwrap(); s.set_parent(&id, Some(f)).unwrap(); }
            };
            if batch { s.batch(run).unwrap(); } else { run(s); }
            t.elapsed()
        };
        for flat in [false, true] {
            let lib = Lib::new("import-speed");
            if flat { Old::new(&lib, 3).write(); }
            let mut s = lib.open();
            assert_eq!(s.flat, flat);
            let a = s.create_folder("逐篇", None).unwrap();
            let b = s.create_folder("批量", None).unwrap();
            let slow = notes(&mut s, &a, false);
            let fast = notes(&mut s, &b, true);
            let t = std::time::Instant::now();
            s.batch(|s| s.delete(&b, None)).unwrap().unwrap();
            eprintln!("flat {flat}: one by one {slow:?}, batch {fast:?}, delete {:?}", t.elapsed());
        }
        // What 导入文件夹 does, from Markdown files on disk to the list shown.
        let src = Lib::new("import-src");
        std::fs::create_dir_all(src.0.join("子")).unwrap();
        for i in 0..120 {
            let body = format!("---\ntags: [a]\n---\n# 笔记 {i}\n\n{}\n\n- 一\n- 二\n\n```rust\nfn main() {{}}\n```\n", "一段正文，带 **加粗** 和 [链接](https://example.com)。".repeat(20));
            std::fs::write(src.0.join(if i % 3 == 0 { format!("子/笔记 {i}.md") } else { format!("笔记 {i}.md") }), body).unwrap();
        }
        let lib = Lib::new("import-real");
        let mut s = lib.open();
        let t = std::time::Instant::now();
        s.batch(|s| {
            let top = s.create_folder("导入", None).unwrap();
            let sub = s.create_folder("子", Some(&top)).unwrap();
            for (dir, parent) in [(src.0.clone(), &top), (src.0.join("子"), &sub)] {
                for e in std::fs::read_dir(dir).unwrap().flatten().filter(|e| e.path().is_file()) {
                    let p = e.path();
                    let text = String::from_utf8_lossy(&read_bounded(&p, 1 << 24).unwrap()).into_owned();
                    let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
                    let d = crate::markdown::parse_file(&text, &stem, p.parent());
                    s.create_markdown_in(&d, crate::markdown::front_matter(&text).0, Some(parent)).unwrap();
                }
            }
        }).unwrap();
        let n = s.list().len();
        eprintln!("导入文件夹 ({n} entries, list included): {:?}", t.elapsed());
    }
}
