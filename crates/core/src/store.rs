//! On-disk library (FORMAT 5): the folders on disk are the library's folders
//! and each document is a file named after its title (`工作/周报.xml`,
//! `随手记.md`). What the files cannot say (pinned, order …) is kept per
//! device in `.quiet/meta/` (`meta.rs`; `.quiet/index.json` in FORMAT 4), a
//! cache that can be lost; state that
//! belongs to this computer only (recently opened, window sizes, 本机文件)
//! is kept outside the library.

use crate::vfs::{PathExt as _, KindExt as _};
use crate::docxml;
use crate::model::Document;
use anyhow::{Context, Result};
use std::io::Write;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use crate::time::now;
use web_time::UNIX_EPOCH;
#[path = "file_history.rs"] mod file_history;
pub use file_history::{FileState, FileDelta, FileChanges};
#[path = "layout.rs"] mod layout;
pub use layout::{file_name_for, relative, resolve, file_link};
use layout::{parent_rel, base_name, stem_of, join_rel};
#[path = "migrate.rs"] mod migrate;
#[path = "meta.rs"] mod meta;
pub use meta::{Device, DeviceInfo, device, set_device_name};
pub use migrate::{MigrationState, ConsentReason, DeviceHint, MigrationReport, RestoreReport, SyncService, sync_of, set_ubiquity_probe};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    /// A folder, by its name on disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// Its place at the top level; `root_order` on disk (the name from before 主页's top-level folders).
    #[serde(default, rename = "root_order", skip_serializing_if = "Option::is_none")]
    pub top_order: Option<u64>,
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
    /// What the file holds; follows from its extension like `markdown`.
    #[serde(default, skip_serializing_if = "DocKind::is_doc")]
    pub kind: DocKind,
}

impl Meta {
    /// 本机文件: outside the library's tree.
    pub fn local(&self) -> bool { self.path.is_some() }
    /// A 本机文件 that is a page's attachment (`.assets/files/…` of a library), opened in a
    /// window of its own: kept out of 本机文件 and 最近浏览, and never renamed after its
    /// title, so the page's card keeps finding it.
    pub fn attachment(&self) -> bool { self.path.as_deref().is_some_and(|p| Path::new(p).components().any(|c| c.as_os_str() == ASSETS)) }
    /// Titled by its file name, not by a title inside it: Markdown and
    /// everything that is not a document.
    pub fn named_by_file(&self) -> bool { self.markdown || !self.kind.is_doc() }
}

/// What a library file holds. Whatever is not a document still lives in the
/// tree like one (index, folders, moves, 回收站); it has its own file format
/// and editor. The kind follows from the extension, the one place below.
#[derive(Default, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum DocKind {
    #[default]
    Doc,
    /// 画板: a `.excalidraw` file (Excalidraw's own JSON).
    Board,
    /// 数据表: a `.sqlite` file (`table::sqlite`). Read and written by the
    /// table itself; the store only knows it as a page holding one 数据表 block.
    Table,
}

impl DocKind {
    pub fn is_doc(&self) -> bool { *self == DocKind::Doc }

    /// The file extension (a document's is `md` when it is Markdown).
    pub fn ext(self, markdown: bool) -> &'static str {
        match self {
            DocKind::Doc => if markdown { "md" } else { "xml" },
            DocKind::Board => "excalidraw",
            DocKind::Table => "sqlite",
        }
    }

    /// A library file's kind and whether it is Markdown, by its name, in any
    /// case (`shown::LIBRARY_FILES`). `None`: not a library file (older builds
    /// skip `.excalidraw` the same way). By name only: `shown::ours` looks inside.
    pub fn of_name(name: &str) -> Option<(DocKind, bool)> { crate::shown::kind_of_name(name) }
}

/// A 数据表 file, by its name.
fn table_file(p: &Path) -> bool { p.file_name().and_then(|n| n.to_str()).and_then(DocKind::of_name).is_some_and(|k| k.0 == DocKind::Table) }

fn catalogue_closed_default() -> bool { true }

impl Default for Meta {
    fn default() -> Self {
        Self {
            folder: None, top_order: None, created: 0, modified: 0, pinned: false, opened: 0,
            width: PageWidth::default(), catalogue_closed: true,
            parent: None, order: 0, starred: 0, reading: false,
            info: InfoRow::default(), refs_hidden: false,
            markdown: false, path: None, name: None, rel: String::new(), kind: DocKind::Doc,
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
        let placed = if self.parent.is_none() { self.top_order.unwrap_or(0) } else { self.order };
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

/// A top-level folder's own look (W21): its document theme and font, its themes
/// folder beside them. A field left out falls back to the top-level folder's
/// preferences of builds before top-level folders, then to the built-in default. Kept
/// behind `Store::look_at` so FORMAT 5 can store it per device.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FolderLook {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feishu: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_family: Option<String>,
    /// 正文字号 ("" is the theme's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_size: Option<String>,
}

/// Window-level UI state: `.quiet/settings.json`, except the window sizes,
/// which belong to this computer.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    /// The top-level folder's look (`Store::look_at`), kept through other writes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<FolderLook>,
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
    /// 升序 / 降序 for `home_sort`; absent in settings from before it existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_sort_dir: Option<String>,
    #[serde(default)]
    pub home_folders_first: bool,
    #[serde(default)]
    pub home_manual_initialized: bool,
    /// Main window size, remembered like the quick note window's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_size: Option<[f32; 2]>,
    /// Main window position and the display it was on, so reopening returns
    /// to the same spot. The origin is in that display's top-left coordinates,
    /// and the display is named by its UUID — the one identifier that survives
    /// a restart; a display that is gone makes the window re-centre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_origin: Option<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_display: Option<String>,
    /// 阅读位置: the list offset each document was left at, so switching back
    /// returns to the same place. Kept out of `index.json`: it is window
    /// state, not document metadata.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub scroll_top: HashMap<String, [f32; 2]>,
    /// 版本历史: snapshots kept per document (1–30). None is the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_limit: Option<u32>,
}

impl Settings {
    pub fn history_limit(&self) -> u32 { crate::versions::clamp_limit(self.history_limit.unwrap_or(crate::versions::DEFAULT_LIMIT)) }
    pub fn set_history_limit(&mut self, n: u32) { self.history_limit = Some(crate::versions::clamp_limit(n)); }
}

/// A document in the 回收站, as kept in `.quiet/trash/index.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    /// A folder's 正文 (#357): the document of the folder's own name inside
    /// it (`方案/方案.xml`), shown as a document with child pages.
    pub note: Option<String>,
    /// The top-level folder it is in (`home::TopFolder::key`); empty from a lone `Store`.
    pub top: String,
}

pub struct Store {
    file_changes: Vec<std::sync::Weak<std::sync::Mutex<Vec<FileDelta>>>>,
    loaded: std::cell::RefCell<HashMap<String, String>>,
    /// Library images and attachments copied beside each 本机文件 this
    /// session, so autosaves reuse the copy instead of making another.
    copied: std::cell::RefCell<HashMap<String, HashMap<PathBuf, String>>>,
    /// What `list` parsed from each file, kept while its mtime and size stay
    /// the same: re-reading every document on each window activation stalled
    /// the main thread for seconds on big or iCloud-evicted libraries.
    parsed: HashMap<String, (web_time::SystemTime, u64, Parsed)>,
    /// `.xml` and `.sqlite` files looked inside (`shown::ours`) this session, by path:
    /// ours (`None`), or not as the file was then (its time and size).
    checked: HashMap<String, Option<(web_time::SystemTime, u64)>>,
    /// `writes`.
    writes: std::cell::Cell<u64>,
    top_dir: PathBuf,
    /// Every document and folder by its id. Ids are handed out per library
    /// and remembered in `.quiet/index.json`, but nothing in a file refers
    /// to them: links are relative paths.
    index: HashMap<String, Meta>,
    /// Index entries for files this build does not know (a newer build's
    /// document kinds), written back unchanged so their ids and settings
    /// survive this build's writes (an older build dropped 画板 this way).
    foreign: DiskIndex,
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
    /// 最近打开 and 阅读位置 not yet on disk, see [`Self::flush_view_state`].
    view: std::cell::RefCell<PendingView>,
    /// FORMAT 5: the index is every device's `meta/` file merged, not `index.json`.
    v5: bool,
    /// The device this store writes as.
    me: Device,
    /// The latest stamp given or seen (`meta::stamp`).
    clock: std::cell::Cell<u64>,
    /// What the index was when last read or written, by id: a write stamps
    /// only what changed since, so another device's newer value read in
    /// between is not overwritten with an old one.
    seen: std::cell::RefCell<HashMap<String, BTreeMap<String, serde_json::Value>>>,
    /// Every device's file as last read or written.
    merged: std::cell::RefCell<meta::Merged>,
    /// What this computer's data about the library is named by: its
    /// folder key, else `library_key` of its path.
    key: String,
    /// New pages in a folder get a hand-order number (手动排序). The window
    /// turns it off under 自动排序, so sorting never rewrites what the user
    /// arranged by hand; the core never reads the window's sort setting.
    place_by_hand: bool,
    /// FORMAT 6: the 回收站 and the hand order live in the device files too.
    v6: bool,
    /// FORMAT 6: the 回收站 records as last read or written, by id.
    seen_trash: std::cell::RefCell<HashMap<String, serde_json::Value>>,
    /// Pages leaving the 回收站 for good: written as `purged`, not restored.
    purging: std::cell::RefCell<HashSet<String>>,
    /// FORMAT 5: the library as `load_meta` last took it in or `save_meta`
    /// last wrote it (`None`: since then not known).
    synced: std::cell::RefCell<Option<Synced>>,
    /// `save_editor_naming_later` while it saves: whether the title asks for
    /// another file name, which it leaves to `follow_name` (`None`: every
    /// other save follows the title itself).
    name_later: std::cell::Cell<Option<bool>>,
    /// FORMAT 6: where this computer saw each page's file (`meta::Here`).
    here: meta::Here,
    /// FORMAT 6: the day's clean-up (`clean_meta`) is due: `Some(false)`
    /// until the first `list` is done, so it is not on the way to the first
    /// frame; `Some(true)` then, and the next `list` does it.
    clean_due: Option<bool>,
}

/// What `load_meta` would take in again is already in memory while no device
/// file has been read again since (`meta::Merged::read`), this computer's
/// file is as it was, and the index, the entries kept for others and the
/// 回收站 are as they were: every change is written through, so nothing was
/// written meanwhile. Reading 15,000 entries again costs about 40 ms.
struct Synced {
    read: u64,
    local: Option<meta::Seen>,
    index: HashMap<String, Meta>,
    foreign: DiskIndex,
    trash: HashMap<String, Trashed>,
    /// Read, not written, since: `seen` is what the files said, which the
    /// index need not say back field for field, so the next write compares all.
    whole: bool,
    /// Entries `list` left out because their files are not here (yet): the
    /// disk is memory plus these while none of their files has arrived.
    absent: HashSet<String>,
}

/// What switching documents remembers in memory only: each write of it is an
/// atomic write with two full fsyncs (about 8 ms on an idle SSD, past 100 ms
/// when the disk is busy), and it used to happen on every switch (#324).
#[derive(Default)]
struct PendingView {
    opened: HashMap<String, u64>,
    scroll: HashMap<String, Option<(usize, f32)>>,
}

impl Drop for Store {
    fn drop(&mut self) { let _ = self.flush_view_state(); }
}

/// How far a `list_with` on another thread has got: documents gone through
/// of all it found (0 until the folders are scanned). Several libraries
/// listed one after another add up.
#[derive(Default)]
pub struct ListProgress { pub done: std::sync::atomic::AtomicUsize, pub total: std::sync::atomic::AtomicUsize }

/// The parts of an `Entry` that come from the document file.
#[derive(Clone, Default)]
struct Parsed { title: String, icon: Option<String>, body: std::sync::Arc<str>, reminders: Vec<crate::model::InlineReminder>, links: Vec<crate::model::DocLink>,
    /// Plain links to files, as written (`layout::file_link`), and the pictures and attachments
    /// a move can break, as paths from the file (`Store::file_refs`): what a move may have to rewrite.
    files: Vec<String> }

/// `Store::parsed` kept between launches (#170): reading every document again
/// on launch took ~0.45 s a file on Windows, where each read is scanned. By
/// file path, so a cache never lends one file's text to another; the stamps
/// are this machine's mtimes, so it lives in the cache folder, not in the
/// (possibly synced) library. Bump `PARSED_CACHE` when what `list` takes from
/// a file changes; a release discards it anyway.
const PARSED_CACHE: u32 = 5;

#[derive(Serialize, Deserialize)]
struct ParsedCache { version: String, docs: HashMap<PathBuf, CachedDoc> }

#[derive(Serialize, Deserialize)]
struct CachedDoc {
    modified: web_time::SystemTime,
    len: u64,
    title: String,
    icon: Option<String>,
    body: String,
    reminders: Vec<(i64, i64, bool, bool)>,
    links: Vec<(String, usize, Vec<(String, Option<String>)>)>,
    #[serde(default)]
    files: Vec<String>,
}

/// A document's plain links to files, each once.
fn file_links(doc: &Document) -> Vec<String> {
    let mut out: Vec<String> = doc.links().into_iter().filter(|l| layout::file_link(l)).collect();
    out.sort();
    out.dedup();
    out
}

/// A path from a file's folder as a link is written (`layout::link_path` reads it back).
fn as_href(path: &str) -> String { path.replace('%', "%25").replace('#', "%23").replace('?', "%3F") }

fn parsed_cache_version() -> String { format!("{}-{PARSED_CACHE}", env!("CARGO_PKG_VERSION")) }

/// Beside the other things this computer keeps about `root` (named by `key`), in the cache folder.
fn parsed_cache_path(root: &Path, key: &str) -> PathBuf {
    let tmp = crate::vfs::temp_dir();
    let base = if cfg!(test) || root.starts_with(&tmp) { tmp.join("quiet-md-local") } else { dirs::cache_dir().unwrap_or(tmp).join("quiet-md") };
    base.join("list").join(format!("{key}.json"))
}

/// One write to the indexes: several copies of the app may share a library
/// (builds tested side by side), so this is a short mutex, not a
/// single-instance guard. Released when the write is done.
/// Empty: the lock a batch already holds.
struct WriteLock(Vec<crate::vfs::File>);

impl Drop for WriteLock {
    fn drop(&mut self) { for f in self.0.iter().rev() { let _ = f.unlock(); } }
}

/// `path` opened (made if missing) and locked, waiting for whoever holds it.
fn lock_file(path: &Path) -> Result<crate::vfs::File> {
    if let Some(dir) = path.parent() { crate::vfs::create_dir_all(dir)?; }
    let f = crate::vfs::OpenOptions::new().create(true).truncate(false).write(true).open(path)?;
    f.lock()?;
    Ok(f)
}

/// This computer's write lock on the library at `root` (sync.md 1.6): in
/// local data, as a lock in the top-level folder would only travel to other devices and
/// lock nothing there. Named by the library id in `format.json`, which goes
/// with the folder: the app (by folder key), the CLI and a lone
/// `open_at` (by `library_key`) all take the same one. Without an id yet
/// (before one is added, the old layout) it is `library_key`, and those
/// formats take `.quiet/.lock` as well.
fn lock_path_at(root: &Path) -> PathBuf {
    let id = reread::<FormatMarker>(&root.join(INTERNAL).join("format.json")).map(|m| m.id).unwrap_or_default();
    let name = if !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') { id } else { library_key(root) };
    local_base(root).join("locks").join(format!("{name}.lock"))
}

/// A batch of writes (`Store::batch`): the lock is held throughout and the
/// index is written once at the end, not once per document.
#[derive(Default)]
struct Batch { lock: Option<WriteLock>, depth: u32, index: std::cell::Cell<bool>, dirs: std::cell::RefCell<Vec<PathBuf>> }

/// Version of the on-disk layout, kept in `.quiet/format.json`. Bump it when
/// a change would make older builds misread the library. 4: real folders,
/// files named by title, internal data in `.quiet/`. 5: what the files
/// cannot say split by device, `.quiet/meta/<device id>.json` (`meta.rs`).
pub const FORMAT: u32 = 6;

/// `load` of a document still in iCloud.
pub const NOT_DOWNLOADED: &str = "这篇文档还在从 iCloud 下载，请稍后再打开";
/// The hidden folder holding the library's own data.
pub const INTERNAL: &str = ".quiet";
/// The hidden folder holding pasted and inserted images and attachments.
pub const ASSETS: &str = ".assets";

#[path = "folders.rs"]
mod folders;
#[path = "subdocs.rs"]
mod subdocs;

#[derive(Serialize, Deserialize)]
struct FormatMarker {
    format: u32,
    /// The library's own id, made with its `.quiet/` and synced with it: what
    /// tells two top-level folders apart on every device (sync.md 3.6). Older builds
    /// ignore it; one written without it gets one on opening.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    id: String,
}

/// An entry of `.quiet/index.json`, keyed by the path relative to the
/// library (folders end in `/`): what the files cannot say.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    #[serde(default, rename = "root_order", skip_serializing_if = "Option::is_none")]
    top_order: Option<u64>,
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
        Attached { id: id.to_string(), created: m.created, modified: m.modified, pinned: m.pinned, starred: m.starred, order: m.order, top_order: m.top_order,
            width: m.width, catalogue_closed: m.catalogue_closed, reading: m.reading, info: m.info, refs_hidden: m.refs_hidden }
    }
    fn meta(&self) -> Meta {
        Meta { created: self.created, modified: self.modified, pinned: self.pinned, starred: self.starred, order: self.order, top_order: self.top_order,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    window_origin: Option<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    window_display: Option<String>,
}

/// `library.json`: where the library is when it isn't the default.
#[derive(Serialize, Deserialize)]
struct Location { path: PathBuf }

/// The library this run uses once first launch has picked it.
static ROOT: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

/// Rename `tmp` over `path`. On Windows a file another program has just
/// opened (the virus scanner or the search indexer looking at what was
/// written a moment ago) refuses to be replaced for a few milliseconds:
/// `ERROR_ACCESS_DENIED` (5) or `ERROR_SHARING_VIOLATION` (32). Try again
/// for up to about a second before giving up.
fn replace(tmp: &Path, path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    for wait in [5, 10, 20, 40, 80, 160, 320, 400] {
        match crate::vfs::rename(tmp, path) {
            Err(e) if matches!(e.raw_os_error(), Some(5 | 32)) => std::thread::sleep(std::time::Duration::from_millis(wait)),
            r => return r,
        }
    }
    crate::vfs::rename(tmp, path)
}

/// Write `bytes` to `path` through a temporary file, so a crash leaves
/// either the old or the new contents.
pub(crate) fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> Result<()> { write_atomic_then(path, bytes, true) }

/// `write_atomic`; without `sync_dir` the folder's entry is left for the
/// caller to sync (a batch syncs each folder once, before its index).
fn write_atomic_then(path: &std::path::Path, bytes: &[u8], sync_dir: bool) -> Result<()> {
    if held(path) { return Err(not_written(path)) }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let tmp = PathBuf::from(tmp);
    #[cfg(test)]
    SYNCED_WRITES.with(|n| n.set(n.get() + 1));
    let result = (|| -> Result<()> {
        let mut file = crate::vfs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        replace(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
        #[cfg(unix)]
        if sync_dir && let Some(parent) = path.parent() { crate::vfs::File::open(parent)?.sync_all()?; }
        // Windows has no directory fsync; the rename is as far as it goes.
        #[cfg(not(unix))]
        let _ = sync_dir;
        Ok(())
    })();
    if result.is_err() { let _ = crate::vfs::remove_file(&tmp); }
    result
}

/// A page's text with the ways a file name is written undone: Markdown
/// percent-encodes (`%20` for a space) and may backslash-escape punctuation
/// (`a\_b.png`), DocxXML escapes (`&amp;` for `&`), and none of it is the name
/// on disk. Deliberately loose — a name that only appears in prose keeps its
/// file, which costs nothing and loses nothing. Lossy where
/// `markdown::percent_decode` gives up: one `%C4%E3` that is not UTF-8 (a
/// GBK link) must not leave the rest of the page undecoded.
fn decoded(text: &str) -> String {
    let unescaped = text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&");
    let b = unescaped.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |at: usize| b.get(at).and_then(|&c| (c as char).to_digit(16)).map(|d| d as u8);
        match (b[i], hex(i + 1), hex(i + 2), b.get(i + 1)) {
            (b'%', Some(h), Some(l), _) => { out.push(h << 4 | l); i += 3 }
            (b'\\', _, _, Some(&c)) if c.is_ascii_punctuation() => { out.push(c); i += 2 }
            (c, ..) => { out.push(c); i += 1 }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Replace a cache file through a temporary file, without syncing it to
/// the disk (a sync costs up to a good part of a second on a busy disk): a
/// crash can leave an old, empty or torn copy, which its reader discards.
fn not_written(path: &Path) -> anyhow::Error {
    anyhow::anyhow!("{}还在 iCloud 上没有下载，没有写入，以免覆盖其他设备的内容", path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default())
}

fn write_cache(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let tmp = PathBuf::from(tmp);
    let r = crate::vfs::write(&tmp, bytes).and_then(|_| replace(&tmp, path));
    if r.is_err() { let _ = crate::vfs::remove_file(&tmp); }
    Ok(r?)
}

fn with_suffix(path: &std::path::Path, suffix: &str) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(suffix);
    PathBuf::from(p)
}

/// The library's own files passed over this run because they were still only
/// in iCloud (#406): read as missing, and never written, so this device's
/// empty stand-in can't replace what another device wrote.
static HELD: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

/// Whether `path` is a cloud-only placeholder; if so it is held (see `HELD`).
pub(crate) fn hold_dataless(path: &Path) -> bool {
    if !crate::vfs::is_dataless(path) { return false }
    let mut h = HELD.lock().unwrap();
    if !h.iter().any(|p| p == path) { h.push(path.to_path_buf()); }
    true
}

fn held(path: &Path) -> bool { HELD.lock().unwrap().iter().any(|p| p == path) }

/// The warning for a file of the library's own passed over by `hold_dataless`.
pub(crate) fn not_downloaded(what: &str) -> String { format!("{what}还在 iCloud 上没有下载，这次没有读取") }

/// Read a JSON index written by `save_json`. A file that exists but does not
/// parse is set aside as `<name>.corrupt-<time>` (never overwritten) and the
/// previous version, `<name>.bak`, is used instead. One still only in iCloud
/// is neither: read as missing and held, with a warning.
fn load_json<T: serde::de::DeserializeOwned + Default>(path: &std::path::Path, what: &str, warnings: &mut Vec<String>) -> T {
    if hold_dataless(path) { warnings.push(not_downloaded(what)); return T::default() }
    let bytes = match crate::vfs::read(path) {
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
    let _ = crate::vfs::rename(path, &aside);
    let name = aside.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match crate::vfs::read(with_suffix(path, ".bak")).ok().and_then(|b| serde_json::from_slice(&b).ok()) {
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
/// The entries of `.quiet/index.json` for files of kinds this build does not know.
fn foreign_of(disk: &DiskIndex) -> DiskIndex {
    disk.iter().filter(|(k, _)| !k.ends_with('/') && !k.split('/').any(|p| p.is_empty() || p.starts_with('.')) && DocKind::of_name(k).is_none())
        .map(|(k, a)| (k.clone(), a.clone())).collect()
}

fn reread<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Option<T> {
    if hold_dataless(path) { return None }
    serde_json::from_slice(&crate::vfs::read(path).ok()?).ok()
}

/// Atomically replace a JSON index, keeping the version it replaces as
/// `<name>.bak` for `load_json`. Unchanged contents are not rewritten (a
/// synced library would upload every rewrite).
fn save_json<T: Serialize>(path: &std::path::Path, v: &T) -> Result<()> {
    if hold_dataless(path) { return Err(not_written(path)) }
    let bytes = serde_json::to_vec_pretty(v)?;
    match crate::vfs::read(path) {
        Ok(old) if old == bytes => return Ok(()),
        Ok(old) => write_atomic(&with_suffix(path, ".bak"), &old)?,
        Err(_) => {}
    }
    write_atomic(path, &bytes)
}

/// Image path for an image block source (only local files are displayed).
pub fn image_path(src: &str) -> Option<PathBuf> { image_path_in(&Store::active_dir(), src) }

pub fn image_path_in(root: &Path, src: &str) -> Option<PathBuf> {
    let p = PathBuf::from(src.strip_prefix("file://").unwrap_or(src));
    if p.fs_is_absolute() {
        return Some(moved_asset(root, &p).unwrap_or(p));
    }
    // Another system's absolute path: the same file in this library's images folder.
    if crate::paths::is_absolute_anywhere(src) { return in_assets(root, src) }
    // Relative to the library (`.assets/…`, either separator); not a URI or a cloud token.
    (!src.is_empty() && !src.contains(':') && src.contains(['/', '\\'])).then(|| crate::paths::resolve(src, root))
}

/// Pages from before docs/FORMAT.md's path rule keep images and attachments by
/// absolute path (pasted ones too), so one that is gone may just have moved with the library: `assets/` became
/// `.assets/` in the move to real folders (#213), or the library itself was
/// moved. The same file in this library's images folder, when it is there.
fn moved_asset(root: &Path, p: &Path) -> Option<PathBuf> {
    if p.fs_exists() { return None; }
    in_assets(root, &p.to_string_lossy())
}

/// The file an absolute path `written` names (this system's or another's: either
/// separator) as it is in the library at `root`'s images folder, when it is there:
/// the part after its last `assets` / `.assets` folder.
fn in_assets(root: &Path, written: &str) -> Option<PathBuf> {
    let parts: Vec<&str> = crate::paths::parts(written).collect();
    let dir = Store::assets_in(root);
    // The last folder so named that leads to a file; a file may be named `assets` itself.
    (0..parts.len()).rev().filter(|&i| matches!(parts[i], "assets" | ".assets")).find_map(|i| {
        let rest = &parts[i + 1..];
        if rest.is_empty() || rest.contains(&"..") { return None; }
        Some(rest.iter().fold(dir.clone(), |p, s| p.join(s))).filter(|q| q.fs_exists())
    })
}

/// Export only files belonging to this library, including after symlink resolution.
pub fn export_path(src: &str) -> Option<PathBuf> {
    export_path_in(&Store::active_dir(), src)
}

pub fn export_path_in(root: &std::path::Path, src: &str) -> Option<PathBuf> {
    let root = root.fs_canonicalize().ok()?;
    let raw = std::path::Path::new(src.strip_prefix("file://").unwrap_or(src));
    let path = if raw.fs_is_absolute() { moved_asset(&root, raw).unwrap_or_else(|| raw.to_path_buf()) } else { root.join(raw) }.fs_canonicalize().ok()?;
    (path.starts_with(root) && path.fs_is_file()).then_some(path)
}

pub const EXPORT_FILE_LIMIT: u64 = 256 * 1024 * 1024;

/// Bound the actual read as well as metadata: files can grow after inspection.
pub fn read_bounded(path: &std::path::Path, limit: u64) -> Result<Vec<u8>> {
    use std::io::Read;
    let file = crate::vfs::File::open(path)?;
    anyhow::ensure!(file.metadata()?.len() <= limit, "文件超过导出大小限制");
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() as u64 <= limit, "文件超过导出大小限制");
    Ok(bytes)
}

/// A short stable name for a library folder, for data kept about it elsewhere:
/// a lone `Store`'s (tests, `QUIET_MD_HOME`). A 主页 top-level folder's is its key in
/// `folders.json` instead, which a rename leaves alone (`Store::open_at_as`).
pub fn library_key(root: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut hash);
    format!("{:016x}", hash.finish())
}

/// Where this computer keeps what is not `root`'s own: the application data
/// folder, by library (`key`); a temporary one for tests and libraries in the
/// temporary folder (scripted runs), which never touch the real one.
fn local_file_for(root: &Path, key: &str) -> PathBuf { local_base(root).join("libraries").join(format!("{key}.json")) }

/// The application data folder `local_file_for` uses; `root` empty: this
/// computer's (a scripted run with `QUIET_MD_HOME` gets a temporary one).
fn local_base(root: &Path) -> PathBuf {
    let tmp = crate::vfs::temp_dir();
    let scripted = root.as_os_str().is_empty() && std::env::var_os("QUIET_MD_HOME").is_some();
    if cfg!(test) || scripted || (!root.as_os_str().is_empty() && (root.starts_with(&tmp) || tmp.fs_canonicalize().is_ok_and(|t| root.starts_with(t)))) { tmp.join("quiet-md-local") }
    else { dirs::data_local_dir().unwrap_or_else(crate::vfs::temp_dir).join("quiet-md") }
}

/// Whether an index key (a relative path, folders ending in `/`) is one
/// this build lists: not hidden, and of a kind it knows.
fn listed(key: &str) -> bool {
    let rel = key.trim_end_matches('/');
    !(rel.is_empty() || rel.split('/').any(|p| p.is_empty() || p.starts_with('.')) || (!key.ends_with('/') && DocKind::of_name(rel).is_none()))
}

/// The recovery copies (`Store::recover_later`) of library `key`.
fn recovery_dir_for(key: &str) -> PathBuf { dirs::data_local_dir().unwrap_or_else(crate::vfs::temp_dir).join("quiet-md/recovery").join(key) }

/// The id, name and parent of every library entry, from where it is.
fn derive_tree(index: &mut HashMap<String, Meta>) {
    let folders: HashMap<String, String> = index.iter().filter(|(_, m)| m.folder.is_some() && !m.rel.is_empty()).map(|(k, m)| (m.rel.clone(), k.clone())).collect();
    for m in index.values_mut() {
        if m.local() || m.rel.is_empty() { continue; }
        let name = base_name(&m.rel).to_string();
        if m.folder.is_some() { m.folder = Some(name); }
        else {
            (m.kind, m.markdown) = DocKind::of_name(&name).unwrap_or_default();
            m.name = m.named_by_file().then(|| stem_of(&name).to_string());
        }
        let up = parent_rel(&m.rel);
        m.parent = (!up.is_empty()).then(|| folders.get(up).cloned()).flatten();
    }
}

/// A doc file's extension in the library: the one it has (`.markdown`, `.MD` …), else its kind's.
fn ext_of(m: &Meta) -> &str {
    let name = base_name(&m.rel);
    match name.rsplit_once('.') { Some((_, e)) if DocKind::of_name(name) == Some((m.kind, m.markdown)) => e, _ => m.kind.ext(m.markdown) }
}

impl Store {
    /// The top-level folder being worked in (each top-level folder on 主页 is a library of its
    /// own): the one holding the document in front, which pasted images go to
    /// and its 数据表 and 画板 files resolve against. Before any is chosen,
    /// the library builds before top-level folders used (`legacy_dir`).
    pub fn active_dir() -> PathBuf {
        if let Some(p) = ROOT.read().unwrap().clone() { return p }
        Self::legacy_dir()
    }

    /// Make `root` the one `active_dir` names: the top-level folder of the document in front.
    pub fn set_active_dir(root: &Path) {
        let mut r = ROOT.write().unwrap();
        if r.as_deref() != Some(root) { *r = Some(root.to_path_buf()); }
    }

    /// The one library of builds before top-level folders: `QUIET_MD_HOME`, else the
    /// folder picked at first launch (`library.json`), else a folder of its
    /// own in the user's Documents, where they can see and back it up.
    pub fn legacy_dir() -> PathBuf {
        std::env::var_os("QUIET_MD_HOME").map(PathBuf::from)
            .or_else(|| crate::vfs::read(Self::location_file()).ok().and_then(|b| serde_json::from_slice::<Location>(&b).ok()).map(|l| l.path).filter(|p| p.fs_is_absolute()))
            .unwrap_or_else(Self::default_dir)
    }

    pub fn default_dir() -> PathBuf {
        // The web build's library lives in memory (vfs).
        if cfg!(target_family = "wasm") { return PathBuf::from("/Quiet.md") }
        dirs::document_dir().or_else(|| dirs::home_dir().map(|h| h.join("Documents"))).unwrap_or_else(|| PathBuf::from(".")).join("Quiet.md")
    }

    /// 应用级设置: preferences, shortcuts, themes, emoji and the window's
    /// state, this computer's and apart from every top-level folder (each top-level folder keeps its
    /// own in its `.quiet/`). Test runs (`QUIET_MD_HOME`) and the web build keep
    /// them beside their library, as before. The first run after top-level folders copies
    /// them from the library that held them; that copy stays for older builds.
    pub fn settings_dir() -> PathBuf {
        let legacy = || { let root = Self::legacy_dir(); if Self::flat_at(&root) { root } else { root.join(INTERNAL) } };
        if std::env::var_os("QUIET_MD_HOME").is_some() || cfg!(target_family = "wasm") {
            let d = legacy();
            let _ = crate::vfs::create_dir_all(&d);
            return d;
        }
        let d = dirs::data_local_dir().unwrap_or_else(crate::vfs::temp_dir).join("quiet-md").join("settings");
        if !d.fs_is_dir() {
            let from = legacy();
            let _ = crate::vfs::create_dir_all(&d);
            for name in ["preferences.json", "shortcuts.json", "emoji.json", "settings.json"] {
                if from.join(name).fs_is_file() { let _ = crate::vfs::copy(from.join(name), d.join(name)); }
            }
            if let Ok(rd) = crate::vfs::read_dir(from.join("themes")) {
                let _ = crate::vfs::create_dir_all(d.join("themes"));
                for e in rd.flatten() { if e.path().fs_is_file() { let _ = crate::vfs::copy(e.path(), d.join("themes").join(e.file_name())); } }
            }
            // The window's place was this computer's state about that library.
            let legacy = Self::legacy_dir();
            let local: Local = reread(&local_file_for(&legacy, &library_key(&legacy))).unwrap_or_default();
            let mut s: Settings = reread(&d.join("settings.json")).unwrap_or_default();
            (s.window_size, s.window_origin, s.window_display, s.quick_note_size) = (local.window_size, local.window_origin, local.window_display, local.quick_note_size.or(s.quick_note_size));
            if let Ok(b) = serde_json::to_vec_pretty(&s) { let _ = write_atomic(&d.join("settings.json"), &b); }
        }
        d
    }

    /// Top-level folder `root`'s look: its `look` in `.quiet/settings.json`, else what its
    /// `.quiet/preferences.json` (where builds before top-level folders kept it) says.
    /// FORMAT 5: the `look.` fields of `_root` in the devices' files.
    pub fn look_at(root: &Path) -> FolderLook {
        let internal = if Self::flat_at(root) { root.to_path_buf() } else { root.join(INTERNAL) };
        if Self::format_at(root) >= 5 {
            let o = meta::load(root, &device().id, &mut vec![]).root_object("look.");
            if o.as_object().is_some_and(|o| !o.is_empty()) { return serde_json::from_value(o).unwrap_or_default(); }
        }
        #[derive(Deserialize, Default)]
        struct Holder { #[serde(default)] look: Option<FolderLook> }
        if let Some(l) = reread::<Holder>(&internal.join("settings.json")).and_then(|h| h.look) { return l; }
        #[derive(Deserialize, Default)]
        struct Old { #[serde(default)] theme: Option<String>, #[serde(default)] feishu: Option<bool>, #[serde(default)] doc_family: Option<String> }
        let o: Old = reread(&internal.join("preferences.json")).unwrap_or_default();
        FolderLook { theme: o.theme, feishu: o.feishu, doc_family: o.doc_family, doc_size: None }
    }

    pub fn save_look_at(root: &Path, look: &FolderLook) -> Result<()> {
        let internal = if Self::flat_at(root) { root.to_path_buf() } else { root.join(INTERNAL) };
        if Self::format_at(root) >= 5 {
            // Each field on its own: a theme picked here and a font picked there both stay.
            // Compared with what the devices' files hold, not with `look_at`'s fallbacks: the
            // first save writes every field. (Against the old preferences.json, which under
            // QUIET_MD_HOME is the one just saved, a new font looked unchanged and was left
            // out; the look then had fields, so the next launch read only those.)
            let was = meta::load(root, &device().id, &mut vec![]).root_object("look.");
            let now = serde_json::to_value(look)?;
            let mut keys: BTreeMap<String, ()> = BTreeMap::new();
            for v in [&was, &now] { if let Some(o) = v.as_object() { keys.extend(o.keys().map(|k| (k.clone(), ()))); } }
            let changes: Vec<_> = keys.into_keys().filter(|k| was.get(k) != now.get(k))
                .map(|k| (meta::FOLDER_ITEM.to_string(), format!("look.{k}"), now.get(&k).cloned().unwrap_or(serde_json::Value::Null))).collect();
            if changes.is_empty() { return Ok(()) }
            return meta::commit(root, &device(), &std::cell::Cell::new(0), changes, &mut vec![]).map(|_| ());
        }
        let path = internal.join("settings.json");
        let mut v: serde_json::Value = reread(&path).unwrap_or_else(|| serde_json::json!({}));
        if !v.is_object() { v = serde_json::json!({}); }
        v["look"] = serde_json::to_value(look)?;
        crate::vfs::create_dir_all(&internal)?;
        save_json(&path, &v)
    }

    /// Top-level folder `root`'s themes folder.
    pub fn themes_dir_of(root: &Path) -> PathBuf { if Self::flat_at(root) { root.join("themes") } else { root.join(INTERNAL).join("themes") } }

    /// The 应用级 part of `Settings` (sidebar width, home view, windows),
    /// from `settings_dir`.
    pub fn app_settings() -> Settings { reread(&Self::settings_dir().join("settings.json")).unwrap_or_default() }

    pub fn save_app_settings(s: &Settings) -> Result<()> { save_json(&Self::settings_dir().join("settings.json"), s) }

    /// Whether `root` is (still) in the old layout.
    pub fn flat_at(root: &Path) -> bool { !root.join(INTERNAL).join("format.json").fs_is_file() && migrate::legacy(root) }

    /// Where images and attachments are in library `root`.
    pub fn assets_in(root: &Path) -> PathBuf { root.join(if Self::flat_at(root) { "assets" } else { ASSETS }) }

    /// What the library's files are, without reading them: changes when a
    /// document or folder is added, removed or renamed (by hand, or a model
    /// writing a note), not when one is saved. Cheap enough to poll.
    pub fn shape_at(root: &Path) -> u64 {
        use std::hash::{Hash, Hasher};
        // By name: a `.xml` that is not ours changes nothing shown, and `list` looks inside.
        let found = layout::scan(&if Self::flat_at(root) { root.join("docs") } else { root.to_path_buf() }, &mut |_, _, _, _| true);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (found.dirs, found.docs, found.pending).hash(&mut h);
        h.finish()
    }

    /// 设置 › 文档库 › 未使用资源 (#237): the files at the top of the asset
    /// folder that nothing mentions. Everything that can hold a reference is
    /// read: the documents (either layout, 画板 included) and their `.bak`
    /// copies; the 回收站 and the version history — restoring a page or an old
    /// version must not bring back a missing picture; every 数据表 — a library
    /// file, one embedded in a page (`.assets/<id>.sqlite`) or a version of
    /// one — through SQLite, since its text may sit anywhere in the file;
    /// `outside`, the 本机文件 and 本机文件夹, since a picture pasted into them
    /// is copied here too; and `live`, the references open editors hold that
    /// the disk does not: unsaved text, and the undo and redo snapshots.
    /// A name is matched as a name, with the ways it is written down undone
    /// first (Markdown percent-encodes a space, DocxXML escapes `&`). Whatever
    /// cannot be read — a file, a folder, an iCloud placeholder not downloaded
    /// yet, an outside folder on a disk not attached — makes the sweep fail:
    /// nothing is offered rather than something in use taken away.
    /// Standalone so it can run off the UI thread.
    pub fn unused_assets_at(root: PathBuf, live: Vec<String>, outside: Vec<PathBuf>) -> Result<Vec<(String, u64)>> {
        /// Every file under `dir`: a trashed folder keeps its pages inside it.
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
            let rd = match crate::vfs::read_dir(dir) {
                Ok(rd) => rd,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
            };
            for e in rd {
                let e = e?;
                let t = e.file_type()?;
                if t.fs_is_dir() { walk(&e.path(), out)? } else if t.fs_is_file() { out.push(e.path()) }
            }
            Ok(())
        }
        /// The pages under `dir`; a scan that could not see them all is refused.
        fn scan(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
            // Every file that may be a page is read, ours or not: nothing in use is offered.
            let found = layout::scan(dir, &mut |_, _, _, _| true);
            if found.partial { anyhow::bail!("a folder under {} could not be read", dir.display()) }
            if let Some(p) = found.pending.first() { anyhow::bail!("{} is not downloaded from iCloud", dir.join(p).display()) }
            out.extend(found.docs.iter().map(|rel| dir.join(rel)));
            Ok(())
        }
        /// A 数据表's values, a line each.
        fn table_text(path: &Path) -> Result<String> {
            #[cfg(not(target_family = "wasm"))]
            return crate::table::sqlite::all_text(path);
            #[cfg(target_family = "wasm")]
            anyhow::bail!("{} is a 数据表, not readable here", path.display())
        }
        let flat = Self::flat_at(&root);
        let internal = if flat { root.clone() } else { root.join(INTERNAL) };
        let docs = if flat { root.join("docs") } else { root.clone() };
        let mut pages: Vec<PathBuf> = vec![];
        scan(&docs, &mut pages)?;
        pages.extend(pages.clone().iter().map(|p| with_suffix(p, ".bak")));
        walk(&if flat { root.join("trash") } else { internal.join("trash") }, &mut pages)?;
        walk(&internal.join("history"), &mut pages)?;
        walk(&internal.join("history-trash"), &mut pages)?;
        for p in &outside {
            if p.fs_is_dir() { scan(p, &mut pages)? }
            else if p.fs_is_file() { pages.push(p.clone()) }
            // A 本机文件 deleted where it was is gone; with its folder gone too it may be on a disk not attached.
            else if !p.parent().is_some_and(|d| d.fs_is_dir()) { anyhow::bail!("{} cannot be reached", p.display()) }
        }

        let listed = match crate::vfs::read_dir(Self::assets_in(&root)) {
            Ok(rd) => rd,
            // A library that never took an image has no asset folder: nothing to list.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(e.into()),
        };
        // An entry that cannot be read is skipped: it is then never offered.
        let mut sizes: Vec<(String, u64)> = vec![];
        for e in listed.flatten() {
            let Ok(meta) = e.metadata() else { continue };
            if !meta.is_file() { continue }
            let Some(name) = e.file_name().to_str().map(str::to_string) else { continue };
            if name.starts_with('.') {
                if name.ends_with(".sqlite.icloud") { anyhow::bail!("{name} is not downloaded from iCloud") }
                continue
            }
            // An embedded 数据表 is read like a page (its attachments are here
            // too); SQLite's own files beside one are never offered.
            if name.ends_with(".sqlite") { pages.push(e.path()) }
            if ["-journal", "-wal", "-shm"].iter().any(|s| name.ends_with(s)) { continue }
            sizes.push((name, meta.len()));
        }
        // Read, one still only in iCloud would be downloaded (#406).
        if let Some(p) = pages.iter().find(|p| crate::vfs::is_dataless(p)) { anyhow::bail!("{} is not downloaded from iCloud", p.display()) }
        let mut used = std::collections::HashSet::new();
        // An open editor's sources are paths (`.assets/a b.png`), already in the
        // form the block holds them; a name is what the asset folder lists.
        for src in &live {
            for written in [src.clone(), decoded(src)] {
                if let Some(name) = Path::new(&written).file_name().and_then(|n| n.to_str()) { used.insert(name.to_string()); }
            }
        }
        // Read each page once, marking the names it mentions, so a big history
        // costs one pass and the text is not all held at once. The text as it
        // is on disk is tried first; only when a name is not there is the
        // decoded copy made, once per page. Bytes that are not UTF-8 (a stray
        // `.DS_Store`) are replaced, which cannot hide a name that is.
        pages.sort();
        pages.dedup();
        for page in &pages {
            if sizes.iter().all(|(name, _)| used.contains(name)) { break }
            if !page.fs_is_file() {
                if layout::placeholder(page).is_some_and(|p| p.fs_exists()) { anyhow::bail!("{} is not downloaded from iCloud", page.display()) }
                continue
            }
            if page.to_string_lossy().ends_with(".icloud") { anyhow::bail!("{} is not downloaded from iCloud", page.display()) }
            let bytes = crate::vfs::read(page).with_context(|| format!("reading {}", page.display()))?;
            let text = if bytes.starts_with(b"SQLite format 3\0") { table_text(page)? } else { String::from_utf8_lossy(&bytes).into_owned() };
            let mut loose: Option<String> = None;
            for (name, _) in &sizes {
                if used.contains(name) || text.contains(name.as_str()) { used.insert(name.clone()); continue }
                let loose = loose.get_or_insert_with(|| decoded(&text));
                if loose.contains(name.as_str()) { used.insert(name.clone()); }
            }
        }
        sizes.retain(|(name, _)| !used.contains(name));
        sizes.sort();
        Ok(sizes)
    }

    /// Take the sweep's files out of the asset folder into
    /// `.quiet/removed-assets/<date>/`, where the Finder can reach them:
    /// nothing is destroyed on the spot, so a picture the sweep got wrong can
    /// be dragged back by hand. A day's folder is cleared once it is older
    /// than the 回收站's own `TRASH_DAYS` — by its name, since a moved file
    /// keeps the time it was last written, which may be long before. A name
    /// with a path separator, a dot prefix or a vanished file is skipped, not
    /// a failure.
    pub fn remove_unused_assets(root: &Path, names: &[String]) -> usize {
        use crate::time::{civil, days_from_civil};
        let from = Self::assets_in(root);
        let internal = if Self::flat_at(root) { root.to_path_buf() } else { root.join(INTERNAL) };
        let gone = internal.join("removed-assets");
        let (y, m, d, ..) = civil(now());
        let today = days_from_civil(y, m, d);
        if let Ok(rd) = crate::vfs::read_dir(&gone) {
            for e in rd.flatten() {
                let Some(n) = e.file_name().to_str().map(str::to_string) else { continue };
                let p: Vec<u32> = n.split('-').map_while(|p| p.parse().ok()).collect();
                let &[y, m, d] = p.as_slice() else { continue };
                if e.file_type().is_ok_and(|t| t.fs_is_dir()) && days_from_civil(y as i64, m, d) + (TRASH_DAYS as i64) < today { let _ = crate::vfs::remove_dir_all(e.path()); }
            }
        }
        let gone = gone.join(format!("{y:04}-{m:02}-{d:02}"));
        let mut moved = 0;
        for name in names {
            if name.contains('/') || name.contains('\\') || name.starts_with('.') { continue }
            let src = from.join(name);
            if !src.fs_is_file() { continue }
            if crate::vfs::create_dir_all(&gone).is_err() { break }
            // A second sweep the same day may offer a name this folder already holds.
            let (stem, ext) = name.rsplit_once('.').map_or((name.as_str(), ""), |(s, e)| (s, e));
            let mut dst = gone.join(name);
            let mut n = 1;
            while crate::vfs::exists(&dst).unwrap_or(false) {
                n += 1;
                dst = gone.join(if ext.is_empty() { format!("{stem} {n}") } else { format!("{stem} {n}.{ext}") });
            }
            if crate::vfs::rename(&src, &dst).is_ok() { moved += 1 }
        }
        moved
    }

    /// Whether `dir` holds a library, of this layout or an older one.
    pub fn is_library(dir: &Path) -> bool {
        dir.join(INTERNAL).join("format.json").fs_is_file() || migrate::legacy(dir)
    }

    /// The pointer to a library outside Documents; per user, never synced.
    pub fn location_file() -> PathBuf {
        dirs::data_local_dir().unwrap_or_else(crate::vfs::temp_dir).join("quiet-md").join("library.json")
    }

    /// Use `root` for the rest of this run, and from now on unless
    /// `QUIET_MD_HOME` says otherwise. The default location needs no pointer.
    pub fn remember_dir(root: PathBuf) -> Result<()> {
        let file = Self::location_file();
        // Test runs (`QUIET_MD_HOME`) never move the real library.
        if std::env::var_os("QUIET_MD_HOME").is_some() {}
        else if root == Self::default_dir() { let _ = crate::vfs::remove_file(&file); }
        else {
            crate::vfs::create_dir_all(file.parent().unwrap())?;
            write_atomic(&file, &serde_json::to_vec_pretty(&Location { path: root.clone() })?)?;
        }
        *ROOT.write().unwrap() = Some(root);
        Ok(())
    }

    /// Where pasted / inserted images are copied to (`.assets/`).
    pub fn assets_dir() -> PathBuf {
        let d = Self::assets_in(&Self::active_dir());
        let _ = crate::vfs::create_dir_all(&d);
        d
    }

    /// The library builds before top-level folders used (`legacy_dir`), moved to the
    /// new layout first when that needs no question (`migration_state` is
    /// `Silent`); a library that needs the user's consent opens as it is, in
    /// the old layout. Other top-level folders never move without asking (`open_top_folder`).
    pub fn open() -> Result<Self> { Self::open_library(Self::legacy_dir()) }

    /// `open` for the library at `root`.
    pub fn open_library(root: PathBuf) -> Result<Self> { let key = library_key(&root); Self::open_library_as(root, &key) }

    /// `open_library` for top-level folder `key` (see `open_at_as`).
    pub fn open_library_as(root: PathBuf, key: &str) -> Result<Self> {
        Self::prune_backups(&root);
        let mut failed = None;
        let report = match Self::migration_state(&root) {
            MigrationState::Silent => Self::migrate(&root).map_err(|e| failed = Some(e.to_string())).ok(),
            _ => None,
        };
        let mut store = Self::open_at_as(root, key)?;
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
    pub fn backup_dir(root: &Path) -> Option<PathBuf> { Some(root.join(INTERNAL).join(migrate::BACKUP)).filter(|p| p.fs_is_dir()) }

    /// A top-level folder on 主页, as it is: one that is not there (an
    /// unplugged disk, renamed in the Finder) is an error, never made anew.
    /// A plain folder gets its `.quiet/`; an old layout is not moved.
    pub fn open_top_folder(root: &Path) -> Result<Self> { Self::open_top_folder_as(root, &library_key(root)) }

    /// `open_top_folder` for top-level folder `key` (see `open_at_as`).
    pub fn open_top_folder_as(root: &Path, key: &str) -> Result<Self> {
        anyhow::ensure!(root.fs_is_dir(), "找不到这个文件夹");
        Self::open_at_as(root.to_path_buf(), key)
    }

    /// The folders the library at `root` had opened in place (本机文件夹)
    /// before top-level folders: top-level folders of their own now.
    pub fn legacy_mounts(root: &Path) -> Vec<PathBuf> {
        let local: Local = reread(&local_file_for(root, &library_key(root))).unwrap_or_default();
        let mut v: Vec<PathBuf> = local.files.values().filter(|m| m.folder.is_some() && m.parent.is_none()).filter_map(|m| m.path.as_deref().map(PathBuf::from)).collect();
        v.sort();
        v
    }

    /// The library's own id (`format.json`); empty in the old flat layout.
    pub fn library_id(&self) -> String { reread::<FormatMarker>(&self.internal().join("format.json")).map(|m| m.id).unwrap_or_default() }

    /// A new library id: this folder was copied whole from another top-level folder.
    pub fn renew_library_id(&self) -> Result<()> {
        if self.flat { return Ok(()); }
        let path = self.internal().join("format.json");
        let m: FormatMarker = serde_json::from_slice(&crate::vfs::read(&path)?)?;
        write_atomic(&path, &serde_json::to_vec_pretty(&FormatMarker { format: m.format, id: uuid::Uuid::new_v4().to_string() })?)
    }

    /// The folder this library is.
    pub fn top_dir(&self) -> &Path { &self.top_dir }

    /// Whether `id` is in this library's 回收站, as the disk has it: a
    /// writer of its own (`tree_writer`) may have put it there meanwhile.
    pub fn trashed(&mut self, id: &str) -> bool {
        if !self.trash.contains_key(id) {
            if self.v6 {
                let m = meta::cached(&self.top_dir, &self.me.id, &self.merged.borrow(), &mut vec![]);
                self.trash = m.trash().iter().filter_map(|(k, v)| Some((k.clone(), meta::trashed_of(v)?))).collect();
            } else if let Some(t) = reread(&self.trash_index_file()) { self.trash = t; }
        }
        self.trash.contains_key(id)
    }

    /// Whether `id` is one of this library's documents or folders.
    pub fn has(&self, id: &str) -> bool { self.index.contains_key(id) }

    /// Every id this library hands out, for 主页 to tell its folders apart.
    pub fn ids(&self) -> impl Iterator<Item = &String> { self.index.keys() }

    /// New ids for entries whose ids another top-level folder already uses: a folder
    /// copied whole, `.quiet/` and all, would otherwise share them. Nothing in
    /// a file names an id, so only the index changes.
    pub fn rekey(&mut self, taken: &HashSet<String>) -> Result<()> {
        let clash: Vec<String> = self.index.keys().filter(|k| taken.contains(*k)).cloned().collect();
        if clash.is_empty() { return Ok(()); }
        let _lock = self.begin_write()?;
        for old in clash {
            let m = self.index.remove(&old).unwrap();
            self.index.insert(uuid::Uuid::new_v4().to_string(), m);
        }
        derive_tree(&mut self.index);
        self.save_index()
    }

    /// Open the library at `root` in the layout it has; never moves it.
    /// FORMAT 4 becomes 5 on opening unless it is in a synced folder, where
    /// another computer may still run a build that cannot read 5 (sync.md 3.3):
    /// it stays 4 until the user agrees.
    pub fn open_at(root: PathBuf) -> Result<Self> { let key = library_key(&root); Self::open_at_as(root, &key) }

    /// `open_at` for top-level folder `key`: what this computer keeps about it
    /// (最近打开, parse cache, recovery copies) is named by the key, which
    /// stays when the folder is renamed. What builds before it kept by path
    /// is taken over the first time (`adopt_path_data`).
    pub fn open_at_as(root: PathBuf, key: &str) -> Result<Self> { Self::open_as(root, key, device()) }

    /// `open_at` for tools beside the app (the `quiet` CLI): copies left by
    /// a crash stay for the app to restore and report.
    pub fn open_for_tools(root: PathBuf) -> Result<Self> { let key = library_key(&root); Self::open_for_tools_as(root, &key) }

    /// `open_for_tools` for top-level folder `key` (see `open_at_as`).
    pub fn open_for_tools_as(root: PathBuf, key: &str) -> Result<Self> { Self::open_with(root, key, device(), false) }

    /// `open_at_as`, writing as device `me` (tests play several devices).
    pub(crate) fn open_as(root: PathBuf, key: &str, me: Device) -> Result<Self> { Self::open_with(root, key, me, true) }

    fn open_with(root: PathBuf, key: &str, me: Device, recover: bool) -> Result<Self> {
        crate::vfs::create_dir_all(&root)?;
        Self::adopt_path_data(&root, key);
        let mut warnings = vec![];
        let internal = root.join(INTERNAL);
        let marker = internal.join("format.json");
        let flat = Self::flat_at(&root);
        if flat {
            let format = reread::<FormatMarker>(&root.join("format.json")).map_or(0, |m| m.format);
            anyhow::ensure!(format <= FORMAT, "这个文档库由更新版本的 Quiet.md 创建，请升级后再打开");
            if !root.join("format.json").fs_exists() { write_atomic(&root.join("format.json"), &serde_json::to_vec_pretty(&FormatMarker { format, id: String::new() })?)?; }
        } else if hold_dataless(&marker) {
            // Still only in iCloud: opened as the layout on disk says, and
            // nothing that would rewrite it (moving on, a new id) runs now.
            warnings.push(not_downloaded("format.json"));
        } else {
            match crate::vfs::read(&marker).ok().and_then(|b| serde_json::from_slice::<FormatMarker>(&b).ok()) {
                Some(m) if m.format > FORMAT => anyhow::bail!("这个文档库由更新版本的 Quiet.md 创建，请升级后再打开"),
                // Moved to the new layout; what was left of the old one is set aside.
                Some(m) => {
                    if migrate::legacy(&root) { migrate::finish(&root, &mut warnings); }
                    if m.id.is_empty() { let _ = write_atomic(&marker, &serde_json::to_vec_pretty(&FormatMarker { format: m.format, id: uuid::Uuid::new_v4().to_string() })?); }
                    if m.format < Self::newest() && sync_of(&root) == SyncService::Local && let Err(e) = Self::upgrade_meta(&root, &me, &mut warnings) { warnings.push(format!("无法升级文档索引：{e}")); }
                }
                None => {
                    crate::vfs::create_dir_all(&internal)?;
                    let format = Self::newest();
                    write_atomic(&marker, &serde_json::to_vec_pretty(&FormatMarker { format, id: uuid::Uuid::new_v4().to_string() })?)?;
                }
            }
        }
        if !flat && !crate::vfs::is_dataless(&marker) && Self::format_at(&root) >= 6 && let Err(e) = Self::tend_meta(&root, &me, key, &mut warnings) { warnings.push(format!("无法整理文档索引：{e}")); }
        let base = if flat { root.clone() } else { internal.clone() };
        let trash = load_json(&base.join("trash").join("index.json"), "回收站索引", &mut warnings);
        let v5 = !flat && Self::format_at(&root) >= 5;
        let v6 = v5 && Self::format_at(&root) >= 6;
        let mut store = Store { file_changes: vec![], top_dir: root, index: HashMap::new(), foreign: DiskIndex::new(), trash, flat, warnings, migrated: None, loaded: Default::default(), copied: Default::default(), parsed: HashMap::new(), checked: HashMap::new(), writes: Default::default(), batch: Default::default(), view: Default::default(),
            v5, me, clock: Default::default(), seen: Default::default(), merged: Default::default(), key: key.to_string(), place_by_hand: true, v6, seen_trash: Default::default(), purging: Default::default(), synced: Default::default(), name_later: Default::default() , here: Default::default() , clean_due: None };
        if flat {
            store.index = load_json(&base.join("index.json"), "文档索引", &mut store.warnings);
            store.flat_folders()?;
        } else if v5 {
            let mut warnings = vec![];
            if v6 {
                store.here = meta::Here::read(&store.top_dir, key, &store.me.id);
                let due = meta::now_ms().saturating_sub(store.here.cleaned) >= meta::DAY_MS;
                store.clean_due = (due && !crate::vfs::is_dataless(&marker)).then_some(false);
            }
            store.load_meta(&mut warnings);
            store.mark_synced(true, HashSet::new());
            store.warnings.extend(warnings);
            store.load_parsed();
        } else {
            let disk: DiskIndex = load_json(&internal.join("index.json"), "文档索引", &mut store.warnings);
            let local = store.read_local();
            store.index = store.from_disk(&disk, &local);
            store.foreign = foreign_of(&disk);
            store.load_parsed();
        }
        if recover && let Ok(entries) = crate::vfs::read_dir(store.recovery_dir()) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_none_or(|x| x != "xml") { continue; }
                let restored = (|| -> Result<()> {
                    let mut doc = docxml::parse(&crate::vfs::read_to_string(&path)?)?;
                    doc.title = crate::model::RichText::plain(format!("{} (recovered copy)", doc.title.text));
                    store.create_from(&doc)?;
                    crate::vfs::remove_file(&path)?;
                    Ok(())
                })();
                store.warnings.push(match restored {
                    Ok(()) => "已恢复上次退出时未保存的文档副本".into(),
                    Err(e) => format!("恢复副本仍保留在 {}：{e}", path.display()),
                });
            }
        }
        Ok(store)
    }

    /// FORMAT 4 → 5 under the locks a FORMAT 4 write takes: a copy opening
    /// the top-level folder at the same time waits, then finds it done.
    fn upgrade_meta(root: &Path, me: &Device, warnings: &mut Vec<String>) -> Result<()> {
        let _locks = WriteLock(vec![lock_file(&lock_path_at(root))?, lock_file(&root.join(INTERNAL).join(".lock"))?]);
        if Self::format_at(root) < 5 { meta::upgrade(root, me, warnings)?; }
        if Self::format_at(root) == 5 && Self::newest() >= 6 { meta::upgrade6(root, me, warnings)?; }
        Ok(())
    }

    /// FORMAT 6, under the write lock: this device's metadata from one whole
    /// file (as the development builds wrote it) to shards (sync.md 3.3).
    fn tend_meta(root: &Path, me: &Device, key: &str, warnings: &mut Vec<String>) -> Result<()> {
        if !meta::needs_shards(root, &me.id) { return Ok(()) }
        let _lock = WriteLock(vec![lock_file(&lock_path_at(root))?]);
        if meta::to_shards(root, me, warnings)? {
            let mut here = meta::Here::read(root, key, &me.id);
            here.converted = meta::now_ms();
            here.save(root, key, &me.id)?;
        }
        Ok(())
    }

    /// FORMAT 6's daily clean-up (sync.md 3.3), under the write lock: this
    /// device's shards cleaned, and its `.single` copy removed once 30 days
    /// are up. About 30 ms for 15,000 entries, so `list` does it, not opening.
    fn clean_meta(&mut self) -> Result<()> {
        let _lock = WriteLock(self.lock_files(self.v5)?);
        meta::clean_all(&self.top_dir, &self.me, &mut self.warnings)?;
        meta::tend_single(&self.top_dir, &self.me.id, &mut self.here)?;
        self.here.cleaned = meta::now_ms();
        self.here.save(&self.top_dir, &self.key, &self.me.id)
    }

    /// The format a library is moved to on opening: `FORMAT`, unless a test
    /// keeps it at an older one (as a synced folder would).
    fn newest() -> u32 { if tests::STAY_4.with(|k| k.get()) { 4 } else if tests::STAY_5.with(|k| k.get()) { 5 } else { FORMAT } }

    /// The format `root`'s `.quiet/format.json` names (0: none).
    /// One still only in iCloud: 5 when the per-device index folder is
    /// there, else 4 (the formats with a `.quiet/`).
    fn format_at(root: &Path) -> u32 {
        let marker = root.join(INTERNAL).join("format.json");
        // 5, not 6, when it cannot tell: a write checks the marker again and stops.
        if crate::vfs::is_dataless(&marker) { return if meta::dir(root).fs_is_dir() { 5 } else { 4 } }
        reread::<FormatMarker>(&marker).map_or(0, |m| m.format)
    }

    /// FORMAT 5: the index from every device's file (sync.md 3.3). Entries
    /// whose files are not here (yet) come along and `list` passes them over.
    fn load_meta(&mut self, warnings: &mut Vec<String>) {
        #[cfg(test)]
        META_LOADS.with(|n| n.set(n.get() + 1));
        meta::refresh(&self.top_dir, &self.me.id, self.merged.get_mut(), warnings);
        let merged = std::mem::take(self.merged.get_mut());
        self.clock.set(self.clock.get().max(merged.max));
        let mut disk = merged.disk_index();
        self.foreign = foreign_of(&disk);
        disk.retain(|k, _| listed(k));
        let local = self.read_local();
        self.index = self.from_disk(&disk, &local);
        disk.extend(self.foreign.clone());
        let mut seen = meta::by_id(&disk);
        if self.v6 {
            // As the keys were read: `to_ranks` kept their order, so none changes here.
            meta::to_keys(&mut seen, &merged.keys());
            let trash = merged.trash();
            self.trash = trash.iter().filter_map(|(id, v)| Some((id.clone(), meta::trashed_of(v)?))).collect();
            *self.seen_trash.get_mut() = trash.into_iter().collect();
        }
        *self.seen.get_mut() = seen;
        *self.merged.get_mut() = merged;
    }

    /// Whether `load_meta` has nothing to take in: no device file changed
    /// and memory is as the disk was last read or written (`Synced`).
    fn meta_held(&mut self) -> bool {
        let read = meta::refresh(&self.top_dir, &self.me.id, self.merged.get_mut(), &mut vec![]);
        let held = !read && self.synced.borrow().as_ref().is_some_and(|s| self.holds(s));
        #[cfg(test)]
        if held { self.check_held(); }
        held
    }

    /// Whether memory is still as `s` found it (`Synced`).
    /// An entry left out whose file has arrived is read in again, so the file
    /// keeps what its device set.
    fn holds(&self, s: &Synced) -> bool {
        s.read == self.merged.borrow().read && meta::still(&self.local_file(), &s.local) && s.index == self.index && s.foreign == self.foreign && s.trash == self.trash
            && !s.absent.iter().any(|k| self.abs(k.trim_end_matches('/')).fs_exists())
    }

    /// `holds` checked against reading everything again: the same entries by
    /// path, in the same order in each folder (reading numbers places 1, 2 …).
    #[cfg(test)]
    fn check_held(&self) {
        let merged = self.merged.borrow();
        let mut disk = merged.disk_index();
        let absent = self.synced.borrow().as_ref().map(|s| s.absent.clone()).unwrap_or_default();
        disk.retain(|k, _| listed(k) && !absent.contains(k));
        let mut fresh = self.from_disk(&disk, &self.read_local());
        // `begin_write` puts back the times not written yet either way.
        for (id, t) in &self.view.borrow().opened { if let Some(m) = fresh.get_mut(id) { m.opened = m.opened.max(*t); } }
        let shape = |index: &HashMap<String, Meta>| {
            let key = |m: &Meta| if m.folder.is_some() { format!("{}/", m.rel) } else { m.rel.clone() };
            let path_of = |id: &Option<String>| id.as_ref().and_then(|p| index.get(p)).map(key);
            let mut rows: Vec<(String, Option<String>, Meta)> = index.values().filter(|m| !m.local()).map(|m| (key(m), path_of(&m.parent), m.clone())).collect();
            rows.sort_by(|a, b| (&a.1, a.2.top_order.unwrap_or(0), a.2.order, &a.0).cmp(&(&b.1, b.2.top_order.unwrap_or(0), b.2.order, &b.0)));
            let mut place: HashMap<Option<String>, u64> = HashMap::new();
            rows.into_iter().map(|(k, parent, mut m)| {
                let n = place.entry(parent.clone()).or_default();
                *n += 1;
                let placed = m.top_order.is_some() || m.order > 0;
                (m.top_order, m.order, m.parent) = (None, if placed { *n } else { 0 }, None);
                (k, (parent, m))
            }).collect::<BTreeMap<_, _>>()
        };
        assert_eq!(shape(&self.index), shape(&fresh), "load_meta skipped, but reading again differs");
    }

    /// Memory and the disk agree now, but for `absent` (`Synced`): `whole`
    /// after reading, not after writing.
    fn mark_synced(&self, whole: bool, absent: HashSet<String>) {
        let read = self.merged.borrow().read;
        *self.synced.borrow_mut() = Some(Synced { read, local: self.local_stamp(), index: self.index.clone(), foreign: self.foreign.clone(), trash: self.trash.clone(), whole, absent });
    }

    /// This computer's file about the library as `meta::files` sees a device file.
    fn local_stamp(&self) -> Option<meta::Seen> { meta::seen_now(&self.local_file()) }

    /// FORMAT 5's `save_index`: what changed since the index was read,
    /// stamped as this device's.
    fn save_meta(&self) -> Result<()> {
        // Not known again until written.
        let synced = self.synced.borrow_mut().take();
        // Written or not, what `list` left out stays out (`seen` has none of it).
        let absent = synced.as_ref().map(|s| s.absent.clone()).unwrap_or_default();
        let (now, gone, changes) = match synced.as_ref().filter(|s| !s.whole && s.foreign == self.foreign) {
            Some(s) => self.meta_changes(s),
            None => {
                let now = self.meta_now();
                let changes = meta::diff(&self.seen.borrow(), &now);
                (now, None, changes)
            }
        };
        #[cfg(test)]
        if let Some(gone) = &gone {
            // The part has to come out as the whole would.
            let whole = self.meta_now();
            assert_eq!(changes, meta::diff(&self.seen.borrow(), &whole), "incremental save_meta");
            let mut seen = self.seen.borrow().clone();
            if !changes.is_empty() { for id in gone { seen.remove(id); } seen.extend(now.clone()); }
            assert!(changes.is_empty() || seen == whole, "incremental seen");
        }
        if !changes.is_empty() {
            let read = self.merged.borrow().read;
            meta::commit_over(&self.top_dir, &self.me, &self.clock, &mut self.merged.borrow_mut(), changes, &mut vec![])?;
            let mut seen = self.seen.borrow_mut();
            match gone { Some(gone) => { for id in gone { seen.remove(&id); } seen.extend(now); } None => *seen = now }
            drop(seen);
            // Read again on the way (another device wrote meanwhile): `load_meta` takes it in.
            if self.merged.borrow().read != read { return self.save_local(&self.local_state()) }
        }
        self.save_local(&self.local_state())?;
        self.mark_synced(false, absent);
        Ok(())
    }

    /// What the index says now, by id, as the device files keep it.
    fn meta_now(&self) -> HashMap<String, BTreeMap<String, serde_json::Value>> {
        let mut now = meta::by_id(&self.disk_index());
        // FORMAT 6: ranks back to keys, a new one only for what moved.
        if self.v6 { meta::to_keys(&mut now, &meta::keys_of(&self.seen.borrow())); }
        now
    }

    /// `meta_now` and its `diff` from `seen` for only what changed since `s`
    /// was written, and the folders it left or went to: every other entry
    /// is as `seen` has it. Returns the entries worked out, those of them to
    /// leave `seen`, and the changes.
    #[allow(clippy::type_complexity)]
    fn meta_changes(&self, s: &Synced) -> (HashMap<String, BTreeMap<String, serde_json::Value>>, Option<Vec<String>>, Vec<(String, String, serde_json::Value)>) {
        let key = |m: &Meta| if m.folder.is_some() { format!("{}/", m.rel) } else { m.rel.clone() };
        let changed: HashSet<&str> = self.index.iter().filter(|(id, m)| s.index.get(*id) != Some(*m)).map(|(id, _)| id.as_str())
            .chain(s.index.keys().filter(|id| !self.index.contains_key(*id)).map(String::as_str)).collect();
        if changed.is_empty() { return (HashMap::new(), Some(vec![]), vec![]) }
        let seen = self.seen.borrow();
        let path_seen = |id: &str| seen.get(id).and_then(|f| f.get("path")).and_then(|v| v.as_str()).map(str::to_string);
        // The folders whose pages are placed anew: where each changed one was and is.
        let mut folders: HashSet<String> = HashSet::new();
        for id in &changed {
            if let Some(m) = self.index.get(*id) { folders.insert(meta::parent_of(&key(m)).to_string()); }
            if let Some(p) = path_seen(id) { folders.insert(meta::parent_of(&p).to_string()); }
        }
        let inside = |k: &str| folders.contains(meta::parent_of(k));
        // `disk_index` for those entries, in the order it takes them (the same one wins a shared path).
        let mut disk: DiskIndex = self.index.iter().filter(|(_, m)| !m.local() && !m.rel.is_empty())
            .filter(|(id, m)| changed.contains(id.as_str()) || inside(&key(m)))
            .map(|(id, m)| (key(m), Attached::of(id, m))).collect();
        for (k, a) in self.foreign.iter().filter(|(k, _)| inside(k)) { disk.entry(k.clone()).or_insert_with(|| a.clone()); }
        let mut now = meta::by_id(&disk);
        if self.v6 {
            let keys: HashMap<String, BTreeMap<String, serde_json::Value>> = now.keys().filter_map(|id| Some((id.clone(), seen.get(id)?.clone()))).collect();
            meta::to_keys(&mut now, &meta::keys_of(&keys));
        }
        // What `seen` had there, and what changed wherever it is now.
        let before: HashMap<String, BTreeMap<String, serde_json::Value>> = seen.iter()
            .filter(|(id, f)| now.contains_key(*id) || changed.contains(id.as_str()) || f.get("path").and_then(|v| v.as_str()).is_some_and(inside))
            .map(|(id, f)| (id.clone(), f.clone())).collect();
        let gone = before.keys().filter(|id| !now.contains_key(*id)).cloned().collect();
        let changes = meta::diff(&before, &now);
        (now, Some(gone), changes)
    }

    /// Where the library keeps its own files: `.quiet/`, the top in the old layout.
    /// 版本历史 lives in `.quiet/` in either layout, so a library that has
    /// not moved to folders yet never grows a visible `history/` folder.
    fn history_base(&self) -> PathBuf { self.top_dir.join(INTERNAL) }
    fn internal(&self) -> PathBuf { if self.flat { self.top_dir.clone() } else { self.top_dir.join(INTERNAL) } }
    fn index_file(&self) -> PathBuf { self.internal().join("index.json") }
    fn trash_index_file(&self) -> PathBuf { self.internal().join("trash").join("index.json") }
    fn local_file(&self) -> PathBuf { local_file_for(&self.top_dir, &self.key) }
    fn lock_path(&self) -> PathBuf { lock_path_at(&self.top_dir) }

    /// The write locks, this computer's first. Before FORMAT 5 older builds,
    /// which may have the library open too, lock `.quiet/.lock` (the top's
    /// `.lock` in the old layout), so that one is held as well; FORMAT 5,
    /// which they cannot open, no longer makes it.
    fn lock_files(&self, v5: bool) -> Result<Vec<crate::vfs::File>> {
        let mut held = loop {
            let path = self.lock_path();
            let f = lock_file(&path)?;
            // A new library id meanwhile (`renew_library_id`): take the new one's.
            if self.lock_path() == path { break vec![f] }
            let _ = f.unlock();
        };
        if !v5 { held.push(lock_file(&self.internal().join(".lock"))?); }
        Ok(held)
    }

    /// The name this computer's data about the library goes by.
    pub fn key(&self) -> &str { &self.key }
    /// See `place_by_hand`: false while the window sorts automatically.
    pub fn set_place_by_hand(&mut self, on: bool) { self.place_by_hand = on; }

    /// The devices that write this top-level folder's metadata (FORMAT 5), the latest
    /// writer first; for 移除已停用的设备. Empty before FORMAT 5.
    pub fn devices(&self) -> Vec<DeviceInfo> { if self.v5 { meta::devices(&self.top_dir) } else { vec![] } }

    /// 移除已停用的设备: device `id`'s fields that still win become this
    /// device's, stamps unchanged, and its file goes; the merged view stays
    /// as it was. This device cannot remove itself.
    pub fn retire_device(&mut self, id: &str) -> Result<()> {
        anyhow::ensure!(self.v5, "这个文档库还没有按设备记录元数据");
        anyhow::ensure!(id != self.me.id, "不能移除当前设备");
        let _lock = self.begin_write()?;
        *self.merged.get_mut() = meta::retire(&self.top_dir, &self.me, id, &mut vec![])?;
        let before = std::mem::take(&mut self.index);
        self.load_meta(&mut vec![]);
        self.reclaim_open(before);
        self.mark_synced(true, HashSet::new());
        Ok(())
    }

    /// Copy the file at `src` into the library's attachments (sync.md 2.5),
    /// named by a new UUID with `src`'s extension, so two devices inserting a
    /// `截图.png` each never meet. Returns its path in the library
    /// (`.assets/<uuid>.png`); the original name is the caller's to keep in `name=`.
    pub fn import_attachment(&self, src: &Path) -> Result<String> {
        anyhow::ensure!(src.fs_is_file(), "找不到要导入的文件：{}", src.display());
        let dir = Self::assets_in(&self.top_dir);
        let ext = src.extension().and_then(|e| e.to_str()).filter(|e| !e.is_empty() && e.len() <= 16 && e.chars().all(|c| c.is_ascii_alphanumeric()));
        let name = match ext { Some(e) => format!("{}.{e}", uuid::Uuid::new_v4()), None => uuid::Uuid::new_v4().to_string() };
        crate::vfs::create_dir_all(&dir)?;
        crate::vfs::copy(src, dir.join(&name))?;
        Ok(format!("{}/{name}", dir.file_name().unwrap_or_default().to_string_lossy()))
    }

    /// This computer's data about `root` named by its path (builds before
    /// folder keys) goes by `key` from now on, unless `key` has some already.
    /// 最近打开 is copied, so an older build still finds it; recovery copies
    /// move (they would come back twice), and so does the parse cache.
    fn adopt_path_data(root: &Path, key: &str) {
        let old = library_key(root);
        if old == key { return }
        let (from, to) = (local_file_for(root, &old), local_file_for(root, key));
        if from.fs_is_file() && !to.fs_exists() { let _ = crate::vfs::copy(&from, &to); }
        for (from, to) in [(parsed_cache_path(root, &old), parsed_cache_path(root, key)), (recovery_dir_for(&old), recovery_dir_for(key))] {
            if from.fs_exists() && !to.fs_exists() { let _ = crate::vfs::rename(&from, &to); }
        }
    }

    /// The folder was renamed or moved (重命名): the same library at
    /// `root` now. Ids, the key and what is in memory stay; call between
    /// writes, after `flush_view_state`.
    pub fn moved_to(&mut self, root: PathBuf) { self.top_dir = root; }

    /// A relative path in the library as a path on disk.
    fn abs(&self, rel: &str) -> PathBuf {
        let mut p = self.top_dir.clone();
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
            if !listed(key) { continue; }
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
            // Folders opened in place before top-level folders are top-level folders of their own now.
            if index.contains_key(id) || m.path.is_none() || m.folder.is_some() || m.parent.is_some() { continue; }
            let mut m = m.clone();
            m.opened = local.opened.get(id).copied().unwrap_or(m.opened);
            index.insert(id.clone(), m);
        }
        index
    }

    /// What `.quiet/index.json` holds: this build's entries and the ones it keeps for others.
    fn disk_index(&self) -> DiskIndex {
        let mut disk = self.to_disk();
        for (k, a) in &self.foreign { disk.entry(k.clone()).or_insert_with(|| a.clone()); }
        disk
    }

    fn to_disk(&self) -> DiskIndex {
        self.index.iter().filter(|(_, m)| !m.local() && !m.rel.is_empty())
            .map(|(id, m)| (if m.folder.is_some() { format!("{}/", m.rel) } else { m.rel.clone() }, Attached::of(id, m))).collect()
    }

    /// This computer's state as the index has it; window sizes as on disk.
    fn local_state(&self) -> Local {
        let mut local = self.read_local();
        local.opened = self.index.iter().filter(|(_, m)| m.opened > 0).map(|(k, m)| (k.clone(), m.opened)).collect();
        // What is in an opened folder is read from disk, only the folder is kept.
        local.files = self.index.iter().filter(|(_, m)| m.local() && m.parent.is_none()).map(|(k, m)| (k.clone(), m.clone())).collect();
        local
    }

    fn save_local(&self, local: &Local) -> Result<()> {
        let path = self.local_file();
        let bytes = serde_json::to_vec_pretty(local)?;
        if crate::vfs::read(&path).is_ok_and(|b| b == bytes) { return Ok(()); }
        crate::vfs::create_dir_all(path.parent().unwrap())?;
        write_atomic(&path, &bytes)
    }

    fn recovery_dir(&self) -> PathBuf { recovery_dir_for(&self.key) }

    pub fn recover_later(&self, doc: &Document) -> Result<()> {
        let dir = self.recovery_dir();
        crate::vfs::create_dir_all(&dir)?;
        write_atomic(&dir.join(format!("{}.xml", uuid::Uuid::new_v4())), docxml::serialize(doc).as_bytes())
    }

    fn path(&self, id: &str) -> PathBuf {
        match self.index.get(id) {
            Some(Meta { path: Some(p), .. }) => PathBuf::from(p),
            m if self.flat => self.top_dir.join("docs").join(format!("{id}.{}", m.map_or("xml", ext_of))),
            Some(m) if !m.rel.is_empty() => self.abs(&m.rel),
            _ => self.internal().join("missing").join(id),
        }
    }

    /// The file a document is kept in.
    pub fn file_path(&self, id: &str) -> Option<PathBuf> {
        self.index.get(id).filter(|m| m.folder.is_none()).map(|_| self.path(id))
    }

    /// A folder on disk: `None` the library itself. The old flat layout keeps
    /// folders only in its index, so they open as the library.
    pub fn folder_dir(&self, id: Option<&str>) -> Option<PathBuf> {
        match id {
            None => Some(self.top_dir.clone()),
            Some(id) => self.index.get(id).filter(|m| m.folder.is_some()).map(|m| match &m.path { Some(p) => PathBuf::from(p), None if self.flat => self.top_dir.clone(), None => self.abs(&m.rel) }),
        }
    }

    pub fn is_markdown(&self, id: &str) -> bool { self.index.get(id).is_some_and(|m| m.markdown) }
    pub fn kind(&self, id: &str) -> DocKind { self.index.get(id).map_or(DocKind::Doc, |m| m.kind) }
    fn named_by_file(&self, id: &str) -> bool { self.index.get(id).is_some_and(Meta::named_by_file) }

    /// The folder of a 本机文件, which its images are relative to.
    fn base(&self, id: &str) -> Option<PathBuf> {
        self.index.get(id)?.path.as_ref().and_then(|p| std::path::Path::new(p).parent()).map(PathBuf::from)
    }

    /// A Markdown file's folder and the vault its `![[…]]` embeds are looked up in.
    fn wiki_dirs(&self, id: &str) -> Option<(PathBuf, PathBuf)> {
        let file = self.path(id);
        Some((file.parent()?.to_path_buf(), crate::markdown::wiki_vault(&file, Some(&self.top_dir), &[])))
    }

    /// The title of a Markdown file or a 画板: its file name.
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

    /// What `relocate` looks at to tell whether `id`'s file needs rewriting: its links to
    /// files, and its pictures and attachments in the library as links from its folder. A
    /// Markdown file names those from itself. A DocxXML page names them from the library's
    /// top, so its own move changes none and `.assets` (which never moves) is left out; the
    /// others are written here as from its folder up to the top and down again
    /// (`../P/图.png` in `P`), which no longer reads the same once a folder in that path moves.
    fn file_refs(&self, id: &str, doc: &Document) -> Vec<String> {
        let mut out = file_links(doc);
        let (dir, markdown) = (self.doc_dir(id), self.is_markdown(id));
        for src in doc.asset_sources() {
            let Some(rel) = crate::paths::portable_in(Path::new(&src), &self.top_dir) else { continue };
            if markdown { out.push(as_href(&layout::relative(&dir, &rel))); }
            else if !rel.starts_with(".assets/") { out.push(as_href(&format!("{}{rel}", "../".repeat(dir.split('/').filter(|s| !s.is_empty()).count())))); }
        }
        out.sort();
        out.dedup();
        out
    }

    /// `text` (a file's contents) as a document.
    fn parse_doc(&self, id: &str, text: &str) -> Result<Document> {
        if self.kind(id) == DocKind::Board { return crate::whiteboard::parse_file(text, &self.markdown_title(id)); }
        if self.kind(id) == DocKind::Table { return Ok(crate::table::file_doc(&self.markdown_title(id), &self.index[id].rel)); }
        if !self.is_markdown(id) {
            let mut doc = docxml::parse(text)?;
            if text.contains("href=") { self.links_to_ids(&self.doc_dir(id), &mut doc); }
            self.sources_here(&mut doc, None);
            return Ok(doc);
        }
        let mut doc = crate::markdown::parse_file(text, &self.markdown_title(id), self.base(id).as_deref());
        // A library file's `![](pics/a.png)` is beside it, as anywhere else.
        if self.base(id).is_none() && let Some(dir) = self.path(id).parent() { crate::markdown::resolve_present(&mut doc.blocks, dir); }
        // Obsidian's `![[picture.png]]` shows as the picture; the file keeps saying `![[…]]`.
        if text.contains("![[") && let Some((dir, vault)) = self.wiki_dirs(id) { crate::markdown::wiki_images(&mut doc.blocks, &dir, Some(&vault)); }
        // Images a Markdown file carries inline (#133) become assets, so the
        // page shows them and an export writes them like any other picture.
        crate::data_images::absorb(&mut doc.blocks, &self.top_dir);
        // What is not beside it is the library's (`.assets/…`, as older builds wrote
        // it), or another system's absolute path to one of its files.
        if self.base(id).is_none() { self.sources_here(&mut doc, Some(&Self::assets_in(&self.top_dir))); }
        Ok(doc)
    }

    /// A page's sources as this computer reads them (docs/FORMAT.md): a relative one
    /// from the library; an absolute one that is not here (another system's, or
    /// the library has moved) as the same file in this library's images folder,
    /// when it is there, so the next save writes it relative. Links, data and
    /// files outside the library stay as written. `within`: only those that
    /// are under it here.
    fn sources_here(&self, doc: &mut Document, within: Option<&Path>) {
        let top = &self.top_dir;
        let here = |src: &str| -> Option<String> {
            if src.is_empty() || src.contains("://") || src.starts_with("data:") { return None }
            if !crate::paths::is_absolute_anywhere(src) {
                // As `image_path_in` has read them: a folder in it, no scheme.
                return (src.contains(['/', '\\']) && !src.contains(':')).then(|| crate::paths::resolve(src, top).to_string_lossy().into_owned());
            }
            if Path::new(src).fs_is_absolute() && Path::new(src).fs_exists() { return None }
            in_assets(top, src).map(|p| p.to_string_lossy().into_owned())
        };
        doc.map_sources(|src| here(src).filter(|p| within.is_none_or(|w| Path::new(p).starts_with(w))));
    }

    /// `doc` with the library's own files written relative to it with `/`
    /// (`.assets/x.png`; docs/FORMAT.md), None when it has none to change.
    /// Files outside the library keep their absolute path. (A Markdown page
    /// writes its own relative to its file: `markdown::Options::dir`.)
    fn portable_sources(&self, doc: &Document) -> Option<Document> {
        let top = &self.top_dir;
        let portable = |src: &str| Path::new(src).fs_is_absolute().then(|| crate::paths::portable_in(Path::new(src), top)).flatten();
        let cover = doc.cover.as_ref().map(|c| c.src.as_str());
        if !doc.asset_sources().iter().map(String::as_str).chain(cover).any(|s| portable(s).is_some()) { return None }
        let mut out = doc.clone();
        out.map_sources(portable);
        Some(out)
    }

    /// `doc` as `id`'s file contents; a Markdown file keeps the front matter it has on disk.
    fn serialize_doc(&self, id: &str, doc: &Document, disk: Option<&str>) -> String { self.serialize_with_copies(id, doc, disk).0 }

    /// `serialize_doc`, and the library files a 本机文件 needs copied beside
    /// it: (source, path relative to its folder).
    fn serialize_with_copies(&self, id: &str, doc: &Document, disk: Option<&str>) -> (String, Vec<(PathBuf, String)>) {
        if self.kind(id) == DocKind::Board { return (crate::whiteboard::write_file(doc), vec![]); }
        if self.kind(id) == DocKind::Table { return (String::new(), vec![]); }
        if !self.is_markdown(id) {
            let portable = self.portable_sources(doc);
            let doc = portable.as_ref().unwrap_or(doc);
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
        let wiki = disk.filter(|d| d.contains("![[")).and_then(|_| self.wiki_dirs(id));
        // A library file's pictures are written relative to its folder too, never as absolute paths.
        let file = self.path(id);
        let dir = file.parent().filter(|_| base.is_none());
        let out = crate::markdown::write_file_with(doc, front, &crate::markdown::Options {
            library_dir: Some(&self.top_dir), base: base.as_deref(), name: name.as_deref(), copied: copied.get(id), original: disk,
            wiki: wiki.as_ref().map(|(d, v)| (d.as_path(), Some(v.as_path()))), dir, ..Default::default()
        });
        let bom = if disk.is_some_and(|d| d.starts_with('\u{feff}')) { "\u{feff}" } else { "" };
        (format!("{bom}{}", out.text), out.files)
    }

    /// Take the library's write lock and adopt what other copies wrote since
    /// this one last looked. Every change is written through, so memory holds
    /// nothing the disk does not. A lock failure must never allow an unguarded write.
    fn begin_write(&mut self) -> Result<WriteLock> {
        self.writes.set(self.writes.get() + 1);
        // Inside a batch the lock is held and memory is what the disk will be.
        if self.batch.lock.is_some() { return Ok(WriteLock(vec![])); }
        crate::vfs::create_dir_all(self.internal())?;
        let lock = WriteLock(self.lock_files(self.v5)?);
        // Another copy moved it to the new layout meanwhile: nothing more is written here.
        anyhow::ensure!(!self.flat || !self.top_dir.join(INTERNAL).join("format.json").fs_exists(), "文档库已升级到新格式，请重新打开 Quiet.md");
        let marker: FormatMarker = serde_json::from_slice(&crate::vfs::read(self.internal().join("format.json"))?)?;
        anyhow::ensure!(marker.format <= FORMAT, "这个文档库由更新版本的 Quiet.md 创建，请升级后再打开");
        anyhow::ensure!((self.v5 || marker.format < 5) && (self.v6 || marker.format < 6), "文档库已升级到新格式，请重新打开 Quiet.md");
        // An unreadable index is not judged here (that is `open`'s job): the
        // version in memory is written back unchanged.
        // Memory as the disk is already (`Synced`): nothing to take in.
        let read = self.v5 && !self.meta_held();
        if self.v5 {
            if read {
                let before = std::mem::take(&mut self.index);
                self.load_meta(&mut vec![]);
                self.reclaim_open(before);
            }
        } else if self.flat {
            if let Some(index) = reread(&self.index_file()) { self.index = index; }
        } else if let Some(disk) = reread::<DiskIndex>(&self.index_file()) {
            let before = std::mem::take(&mut self.index);
            let local = self.read_local();
            self.index = self.from_disk(&disk, &local);
            self.foreign = foreign_of(&disk);
            self.reclaim_open(before);
        }
        if !self.v6 && let Some(trash) = reread(&self.trash_index_file()) { self.trash = trash; }
        self.apply_pending_opened();
        if read { self.mark_synced(true, HashSet::new()); }
        Ok(lock)
    }

    /// An index just re-read from disk gets the 最近打开 times not written yet.
    fn apply_pending_opened(&mut self) {
        for (id, t) in &self.view.get_mut().opened {
            if let Some(m) = self.index.get_mut(id) { m.opened = m.opened.max(*t); }
        }
    }

    /// Write what [`Self::touch`] and [`Self::remember_scroll`] kept in memory.
    /// The window calls this once switching stops and on quit; dropping the store does too.
    pub fn flush_view_state(&mut self) -> Result<()> {
        let scroll = std::mem::take(&mut self.view.get_mut().scroll);
        // The times stay pending until written: `begin_write` puts them back on the re-read index.
        let opened = if self.view.get_mut().opened.is_empty() { Ok(()) } else {
            self.begin_write().and_then(|_lock| if self.flat { self.save_index() } else { self.save_local(&self.local_state()) })
                .map(|()| self.view.get_mut().opened.clear())
        };
        if scroll.is_empty() { return opened }
        let mut s = self.settings();
        let mut changed = false;
        for (id, top) in scroll {
            changed |= match top {
                Some((ix, y)) => s.scroll_top.insert(id, [ix as f32, y]) != Some([ix as f32, y]),
                None => s.scroll_top.remove(&id).is_some(),
            };
        }
        opened.and(if changed { self.save_settings(&s) } else { Ok(()) })
    }

    /// An editor still open on a document whose entry another copy of the app
    /// dropped (an older build that does not know its kind) keeps its id while
    /// its file is where it was: the index re-read gave the file a new id, or none.
    fn reclaim_open(&mut self, before: HashMap<String, Meta>) {
        let open: Vec<String> = self.loaded.borrow().keys().filter(|id| !self.index.contains_key(*id)).cloned().collect();
        let mut changed = false;
        for id in open {
            let Some(old) = before.get(&id).filter(|m| !m.local() && m.folder.is_none() && !m.rel.is_empty()) else { continue };
            let other = self.index.iter().find(|(_, m)| !m.local() && m.folder.is_none() && m.rel == old.rel).map(|(k, _)| k.clone());
            match other {
                Some(other) => { self.index.remove(&other); }
                None if self.abs(&old.rel).fs_exists() => {}
                None => continue,
            }
            self.index.insert(id, old.clone());
            changed = true;
        }
        if changed { derive_tree(&mut self.index); }
    }

    fn save_index(&self) -> Result<()> {
        if self.batch.lock.is_some() { self.batch.index.set(true); return Ok(()); }
        // The old layout keeps everything in its index, as older builds expect.
        if self.flat { return save_json(&self.index_file(), &self.index); }
        if self.v5 { return self.save_meta(); }
        save_json(&self.index_file(), &self.disk_index())?;
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
            if let Err(e) = crate::vfs::File::open(&dir).and_then(|f| f.sync_all()) { r = Err(e.into()); }
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
            let stem = if n == 1 { base.to_string() } else { layout::numbered(base, n) };
            let name = match ext { Some(e) => format!("{stem}.{e}"), None => stem };
            let rel = join_rel(dir, &name);
            let low = rel.to_lowercase();
            if own_rel.as_deref() == Some(low.as_str()) { return name; }
            let path = self.abs(&rel);
            let on_disk = path.fs_symlink_metadata().is_ok() || layout::placeholder(&path).is_some_and(|p| p.fs_exists());
            if !taken.contains(&low) && !on_disk { return name; }
        }
        unreachable!()
    }

    /// The old layout's marker raised to 3 before its first Markdown document
    /// or 本机文件: older builds would drop their `markdown` / `path` / `name`.
    fn require_markdown_format(&self) -> Result<()> {
        let path = self.internal().join("format.json");
        let format = reread::<FormatMarker>(&path).map_or(0, |m| m.format);
        if format < 3 { write_atomic(&path, &serde_json::to_vec_pretty(&FormatMarker { format: 3, id: String::new() })?)?; }
        Ok(())
    }

    /// The old layout as opened: pages that had pages under them (FORMAT 1)
    /// become folders, as older builds did.
    fn flat_folders(&mut self) -> Result<()> {
        let format = reread::<FormatMarker>(&self.internal().join("format.json")).map_or(0, |m| m.format);
        if format >= 2 { return Ok(()); }
        let _lock = self.begin_write()?;
        let backup = self.top_dir.join("index.before-folders.json");
        if !backup.fs_exists() { write_atomic(&backup, &serde_json::to_vec_pretty(&self.index)?)?; }
        let docs = self.top_dir.join("docs");
        let mut index = self.index.clone();
        migrate::pages_to_folders(&mut index, |id| docs.join(format!("{id}.xml")).fs_is_file(), |id| crate::vfs::read_to_string(docs.join(format!("{id}.xml"))).ok().and_then(|x| docxml::parse(&x).ok()).map(|d| d.title.text));
        self.index = index;
        self.save_index()?;
        write_atomic(&self.internal().join("format.json"), &serde_json::to_vec_pretty(&FormatMarker { format: 2, id: String::new() })?)
    }

    /// `list` in the old layout: the files in `docs/` (a `.md` dropped there by
    /// hand too) and the folders of the index.
    fn list_flat(&mut self, locked: bool) -> Vec<Entry> {
        let mut out = vec![];
        let mut dirty = false;
        let files: Vec<(String, PathBuf, bool)> = crate::vfs::read_dir(self.top_dir.join("docs")).map(|rd| rd.flatten().map(|e| e.path()).filter_map(|p| {
            let markdown = match p.extension().and_then(|s| s.to_str()) { Some("xml") => false, Some("md") => true, _ => return None };
            Some((p.file_stem()?.to_str()?.to_string(), p, markdown))
        }).collect()).unwrap_or_default();
        let local: Vec<(String, PathBuf, bool)> = self.index.iter().filter_map(|(id, m)| Some((id.clone(), PathBuf::from(m.path.as_ref()?), true))).collect();
        for (id, p, markdown) in files.into_iter().chain(local) {
            let md = crate::vfs::metadata(&p).ok();
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
                // Still only in iCloud (#406): listed by its name, read once someone opens it.
                None if crate::vfs::is_dataless(&p) => Parsed { title: self.index.get(&id).and_then(|m| m.name.clone()).unwrap_or_default(), ..Default::default() },
                None => {
                    #[cfg(test)]
                    DOC_READS.with(|n| n.set(n.get() + 1));
                    // a 数据表 is not text: its words come from the file's cells
                    let table = table_file(&p);
                    let Ok(text) = (if table { Ok(String::new()) } else { crate::vfs::read_to_string(&p) }) else { continue };
                    let doc = if table { Document::default() } else { self.parse_doc(&id, &text).unwrap_or_default() };
                    let body = if table { crate::table::block_text(&self.top_dir, &crate::table::TableRef::new(p.strip_prefix(&self.top_dir).map(|r| r.to_string_lossy().into_owned()).unwrap_or_default())) } else { doc.plain_text() };
                    let parsed = Parsed { title: doc.title.text.clone(), icon: doc.icon.clone(), body: body.into(), reminders: doc.reminders(), links: doc.doc_links(), files: self.file_refs(&id, &doc) };
                    if let Some((t, len)) = stamp { self.parsed.insert(id.clone(), (t, len, parsed.clone())); }
                    parsed
                }
            };
            if !self.index.contains_key(&id) {
                let t = md.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or_else(now);
                self.index.insert(id.clone(), Meta { created: t, modified: t, ..Default::default() });
                dirty = true;
            }
            let Parsed { mut title, icon, body, reminders, links, .. } = parsed;
            if self.index[&id].named_by_file() { title = self.markdown_title(&id); }
            out.push(Entry { id: id.clone(), meta: self.index[&id].clone(), title, icon, body, reminders, links, note: None, top: String::new() });
        }
        let mut folders: Vec<(&String, &Meta)> = self.index.iter().filter(|(_, m)| m.folder.is_some()).collect();
        folders.sort_by(|a, b| a.0.cmp(b.0));
        for (id, meta) in folders {
            out.push(Entry { id: id.clone(), meta: meta.clone(), title: meta.folder.clone().unwrap_or_default(), icon: None, body: Default::default(), reminders: vec![], links: vec![], note: None, top: String::new() });
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
        let mut dirs: Vec<(String, &str)> = vec![];
        for (id, to) in moves {
            let m = self.index.get(id).ok_or_else(|| anyhow::anyhow!("文档或文件夹不存在"))?;
            anyhow::ensure!(!m.local(), "本机文件不能移入文档库的文件夹");
            if m.folder.is_some() {
                anyhow::ensure!(*to != m.rel && !to.starts_with(&format!("{}/", m.rel)), "不能移动到自身或后代文件夹中");
                let prefix = format!("{}/", m.rel);
                for (k, c) in &self.index {
                    if let Some(rest) = c.rel.strip_prefix(&prefix) && !c.local() { new_rel.insert(k.clone(), format!("{to}/{rest}")); }
                }
                dirs.push((prefix, to));
            }
            new_rel.insert(id.clone(), to.clone());
        }
        // Documents whose links may change: the moved ones (links out of
        // them are relative to where they are) and those mentioning one.
        let affected: HashSet<&str> = new_rel.keys().map(String::as_str).collect();
        // Plain links to files (`[报告](附件/报告.pdf)`, in Markdown too) are
        // paths as well: where what moved went, a document or anything in a moved folder.
        let old_rel: HashMap<String, String> = new_rel.iter().map(|(k, to)| (self.index[k].rel.clone(), to.clone())).collect();
        let moved = |p: &str| old_rel.get(p).cloned().or_else(|| dirs.iter().find_map(|(d, to)| p.strip_prefix(d.as_str()).map(|r| format!("{to}/{r}"))));
        // Where what moves was, by id (the index says where it is, from the rename on).
        let was_at: HashMap<String, String> = new_rel.keys().map(|k| (k.clone(), self.index[k].rel.clone())).collect();
        // Only files with something to rewrite are read: a mention, link or picture is written
        // relative to its file (a DocxXML page's pictures to the library), so inside a folder
        // that moves whole, what points inside it reads the same afterwards.
        let mut docs = vec![];
        for (k, m) in &self.index {
            if m.folder.is_some() || m.local() || !m.kind.is_doc() { continue; }
            let (here, dir, parsed) = (affected.contains(k.as_str()), parent_rel(&m.rel), self.parsed.get(k).map(|p| &p.2));
            let to = new_rel.get(k).map_or(dir, |r| parent_rel(r));
            let after = |id: &str| new_rel.get(id).or_else(|| self.index.get(id).map(|m| &m.rel));
            let mentions = !m.markdown && parsed.is_none_or(|p| p.links.iter().any(|l| self.index.get(&l.target).zip(after(&l.target)).is_some_and(|(t, now)| layout::relative(dir, &t.rel) != layout::relative(to, now))));
            // Links to files and the pictures not in `.assets` (`file_refs`), as paths from the file.
            let files = parsed.is_none_or(|p| p.files.iter().any(|h| if here { layout::rebase_link(dir, to, h, &moved).is_some() } else { layout::link_path(dir, h).is_some_and(|t| moved(&t).is_some()) }));
            if !mentions && !files { continue; }
            let Ok(text) = crate::vfs::read_to_string(self.path(k)) else { continue };
            if !files && !text.contains("type=\"doc\"") { continue; }
            let Ok(doc) = self.parse_doc(k, &text) else { continue };
            docs.push((k.clone(), text, doc, dir.to_string()));
        }
        let mut done: Vec<(PathBuf, PathBuf)> = vec![];
        for (id, to) in moves {
            let from = self.abs(&self.index[id].rel);
            let dest = self.abs(to);
            let r = (|| -> Result<()> {
                if let Some(p) = dest.parent() { crate::vfs::create_dir_all(p)?; }
                // A change of case only is the same file on macOS and Windows.
                let same = from.to_string_lossy().to_lowercase() == dest.to_string_lossy().to_lowercase();
                anyhow::ensure!(same || dest.fs_symlink_metadata().is_err(), "已存在同名文件：{to}");
                crate::vfs::rename(&from, &dest)?;
                Ok(())
            })();
            if let Err(e) = r {
                for (a, b) in done.iter().rev() { let _ = crate::vfs::rename(b, a); }
                return Err(e);
            }
            done.push((from, dest));
        }
        for (k, rel) in &new_rel { if let Some(m) = self.index.get_mut(k) { m.rel = rel.clone(); } }
        // Files of kinds this build does not know went with their folders.
        self.foreign = std::mem::take(&mut self.foreign).into_iter()
            .map(|(k, a)| (dirs.iter().find_map(|(p, to)| k.strip_prefix(p.as_str()).map(|rest| format!("{to}/{rest}"))).unwrap_or(k), a)).collect();
        derive_tree(&mut self.index);
        // Each folder written into reaches the disk once, before the index.
        let mut synced: Vec<PathBuf> = vec![];
        for (k, text, mut doc, was) in docs {
            let dir = parent_rel(&self.index[&k].rel).to_string();
            let relinked = doc.map_links(&mut |h| layout::rebase_link(&was, &dir, h, &moved));
            let markdown = self.is_markdown(&k);
            // Pictures and attachments were read as paths on disk: what moved is elsewhere
            // now. A Markdown file writes them relative to itself, a DocxXML page to the library.
            let (from, to, mut resourced) = (self.abs(&was), self.abs(&dir), false);
            doc.map_sources(|src| {
                let at = Path::new(src);
                if !at.fs_is_absolute() { return None }
                let new = crate::paths::portable_in(at, &self.top_dir).and_then(|rel| moved(&rel)).map(|rel| self.abs(&rel));
                resourced |= if markdown { crate::paths::portable(at, &from) != crate::paths::portable(new.as_deref().unwrap_or(at), &to) } else { new.is_some() };
                new.map(|p| p.to_string_lossy().into_owned())
            });
            let mentioned = !markdown && doc.doc_links().iter().any(|l| self.index.get(&l.target).is_some_and(|t| layout::relative(&was, was_at.get(&l.target).unwrap_or(&t.rel)) != layout::relative(&dir, &t.rel)));
            if !relinked && !resourced && !mentioned { continue; }
            let out = self.serialize_doc(&k, &doc, markdown.then_some(text.as_str()));
            if out == text { continue; }
            let path = self.path(&k);
            match write_atomic_then(&path, out.as_bytes(), false) {
                Ok(()) => {
                    if let Some(dir) = path.parent() && !synced.iter().any(|d| d == dir) { synced.push(dir.to_path_buf()); }
                    // An open editor still holds the links as they were; its
                    // next save follows them (`follow_moved_links`).
                    let mut loaded = self.loaded.borrow_mut();
                    if !relinked && loaded.get(&k).is_some_and(|l| *l == text) { loaded.insert(k.clone(), out); }
                    // Its links as read are stale now; `list` reads it again.
                    self.parsed.remove(&k);
                }
                Err(e) => self.warnings.push(format!("未能更新文档中的链接（{}）：{e}", self.index[&k].rel)),
            }
        }
        // 数据表's 笔记 cells are paths too, relative to the table's file: they follow a note
        // that moved, and a table that did.
        #[cfg(not(target_family = "wasm"))]
        {
            let was: HashMap<&str, &str> = old_rel.iter().map(|(old, new)| (new.as_str(), old.as_str())).collect();
            let tables: Vec<String> = self.index.iter().filter(|(_, m)| m.folder.is_none() && !m.local() && m.kind == DocKind::Table).map(|(_, m)| m.rel.clone()).collect();
            for now in tables {
                let before = was.get(now.as_str()).copied().unwrap_or(now.as_str());
                let (from, to) = (parent_rel(before), parent_rel(&now));
                let r = crate::table::sqlite::rewrite_notes(&self.abs(&now), |href| {
                    let target = layout::resolve(from, href)?;
                    let new = layout::relative(to, &moved(&target).unwrap_or(target));
                    (new != href).then_some(new)
                });
                if let Err(e) = r { self.warnings.push(format!("未能更新数据表中的笔记路径（{now}）：{e}")); }
            }
        }
        #[cfg(unix)]
        for dir in &synced { crate::vfs::File::open(dir).and_then(|f| f.sync_all())?; }
        #[cfg(not(unix))]
        drop(synced);
        self.save_index()
    }

    /// A title change renames the file (and keeps the links to it); outside
    /// the library a Markdown file is renamed beside itself, kept as it is
    /// when the name is taken. `renamed`: the title was just changed (not a
    /// save of the same title), so a name numbered because it was taken
    /// drops its number once the name is free again (#358).
    fn follow_title(&mut self, id: &str, title: &str, renamed: bool) -> Result<()> {
        let Some(m) = self.index.get(id) else { return Ok(()) };
        // An attachment's name is what its page's card points at: a new title changes only its words.
        if m.attachment() { return Ok(()); }
        if let Some(old) = m.path.clone().map(PathBuf::from) {
            let name = title.replace(['/', '\\', ':'], "_");
            let name = name.trim().trim_start_matches('.');
            if name.is_empty() { return Ok(()); }
            let ext = old.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "md".into());
            let new = old.with_file_name(format!("{name}.{ext}"));
            if new == old || new.fs_try_exists()? { return Ok(()); }
            crate::vfs::rename(&old, &new)?;
            self.index.get_mut(id).unwrap().path = Some(new.to_string_lossy().into_owned());
            return Ok(());
        }
        if self.flat {
            // The old layout: names live in the index.
            let m = self.index.get_mut(id).unwrap();
            if m.folder.is_some() { m.folder = Some(title.to_string()); } else if m.named_by_file() { m.name = Some(title.to_string()); }
            return Ok(());
        }
        let base = file_name_for(title);
        if let Some(folder) = self.folder_of_note(id) { return self.follow_note_title(id, &folder, &base); }
        let rel = m.rel.clone();
        let dir = parent_rel(&rel).to_string();
        let name = base_name(&rel);
        let ext = m.folder.is_none().then(|| ext_of(m).to_string());
        let stem = if ext.is_some() { stem_of(name) } else { name };
        let free = |s: &Self| s.free_name(&dir, &base, ext.as_deref(), Some(id));
        let to = if layout::name_fits(stem, &base) {
            // Numbered because the name was taken: kept unless the plain name is free again.
            if stem == base || !renamed { return Ok(()); }
            let to = free(self);
            if ext.as_ref().map_or(to.as_str(), |_| stem_of(&to)) != base { return Ok(()); }
            to
        } else { free(self) };
        self.relocate(&[(id.to_string(), join_rel(&dir, &to))])
    }

    /// `list`'s write lock; without it, what it found is not written down.
    fn lock_for_list(&mut self) -> Option<WriteLock> {
        // The day's clean-up under its own lock, before `begin_write` reads what it wrote.
        if self.clean_due == Some(true) && self.batch.lock.is_none() {
            self.clean_due = None;
            if let Err(e) = self.clean_meta() { self.warnings.push(format!("无法整理文档索引：{e}")); }
        }
        match self.begin_write() {
            Ok(lock) => Some(lock),
            Err(e) => { self.warnings.push(format!("无法锁定文档库：{e}")); None }
        }
    }

    /// Goes up with every write (and `list`): a `tree_writer` listing off the main
    /// thread is taken over (`adopt`) only when this did not move meanwhile.
    pub fn writes(&self) -> u64 { self.writes.get() }

    /// All documents found on disk. Files and folders made in the Finder are
    /// picked up; entries whose files are gone are dropped.
    pub fn list(&mut self) -> Vec<Entry> { self.list_with(None, true) }

    /// The library's folders and files as `list` reads them.
    fn walk(&mut self) -> layout::Scan {
        // What `list` read before is ours (the cache keeps nothing else); the rest is looked at once.
        let (index, parsed, checked) = (&self.index, &self.parsed, &mut self.checked);
        let mut read: Option<HashSet<&str>> = None;
        layout::scan(&self.top_dir, &mut |rel, path, kind, md| {
            let stamp = || crate::vfs::metadata(path).ok().and_then(|m| Some((m.modified().ok()?, m.len())));
            match checked.get(rel) { Some(None) => return true, Some(Some(was)) if Some(*was) == stamp() => return false, _ => {} }
            let read = read.get_or_insert_with(|| parsed.keys().filter_map(|id| index.get(id)).map(|m| m.rel.as_str()).collect());
            let ok = read.contains(rel) || crate::shown::ours(path, kind, md);
            checked.insert(rel.to_string(), if ok { None } else { stamp() });
            ok
        })
    }

    /// Where the index has every page and folder, as one number: `list` sees from it whether
    /// a write moved anything while it walked the folders.
    fn paths_stamp(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut rels: Vec<(&str, &str)> = self.index.iter().map(|(k, m)| (k.as_str(), m.rel.as_str())).collect();
        rels.sort_unstable();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        rels.hash(&mut h);
        h.finish()
    }

    /// `list`, counting into `progress` as it goes. With `parse` off a
    /// document `list` has not read yet is listed by its file's name, the way
    /// one still in iCloud is, and read on a later `list` (本机文件 are read
    /// anyway: there are few, and one may be opening right now).
    pub fn list_with(&mut self, progress: Option<&ListProgress>, parse: bool) -> Vec<Entry> {
        if self.flat {
            let lock = self.lock_for_list();
            return self.list_flat(lock.is_some());
        }
        // Walked before the lock is taken: a big folder's walk holds up no one's save.
        let paths = self.paths_stamp();
        let mut found = self.walk();
        #[cfg(test)]
        if let Some(meanwhile) = AFTER_WALK.with(|h| h.borrow_mut().take()) { meanwhile(); }
        let _lock = self.lock_for_list();
        // A write made meanwhile (a `tree_writer` renaming or moving a page, another window,
        // another device's sync) can have moved files while the walk went on: the index it
        // saved, read under the lock, then names pages at paths the walk did not see, or the
        // walk saw a file where the index does not have it yet. Read as is, a moved page would
        // be dropped and listed again under the id its new path gives. Walked again under the
        // lock, the disk and the index agree.
        if self.paths_stamp() != paths { found = self.walk(); }
        let mut disk = self.to_disk();
        let mut keys: HashSet<String> = found.dirs.iter().map(|d| format!("{d}/")).collect();
        keys.extend(found.docs.iter().cloned());
        keys.extend(found.pending.iter().cloned());
        // A folder with nothing listed in it shows only when the user made it here (or
        // pinned, starred or renamed it): a found one has the id its path gives.
        let users = |k: &str, a: &Attached| !self.v5 || a.id != meta::path_id(k) || a.pinned || a.starred > 0;
        for d in &found.empty {
            let k = format!("{d}/");
            if !disk.get(&k).is_some_and(|a| users(&k, a)) { continue }
            let mut up = d.as_str();
            loop { keys.insert(format!("{up}/")); match up.rsplit_once('/') { Some((p, _)) => up = p, None => break } }
        }
        let hidden = found.hider();
        if self.v6 && _lock.is_some() && self.revive(&disk, &keys) { disk = self.to_disk(); }
        let mut left_out = HashSet::new();
        let mut gone = vec![];
        if !found.partial {
            // Not here (yet, or gone outside the app): left out, not removed, so
            // another device's entry for a file still on its way keeps its settings.
            // Here but not shown (`Scan::hider`): kept as it is, only not listed.
            // FORMAT 6: one whose file was here, where it is still expected, is
            // gone (deleted outside the app) and gets a tombstone.
            let (seen, here, v6) = (self.seen.get_mut(), &self.here.seen, self.v6);
            disk.retain(|k, a| keys.contains(k) || hidden(k) || {
                seen.remove(&a.id);
                if v6 && here.get(&a.id) == Some(k) { gone.push((a.id.clone(), k.clone())) } else { left_out.insert(k.clone()); }
                false
            });
        }
        let disk_left_out = !left_out.is_empty();
        let mut dirty = disk_left_out || !gone.is_empty();
        for key in &keys {
            if disk.contains_key(key) { continue; }
            let t = crate::vfs::metadata(self.abs(key.trim_end_matches('/'))).ok().and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or_else(now);
            // A file: made when the system says it was (`vfs::file_times`), else at its last change.
            let made = if key.ends_with('/') { t } else { crate::vfs::file_times(&self.abs(key)).map_or(t, |(made, _)| made) };
            // Found by every device alike: the same id on each (sync.md 3.3).
            let id = if self.v5 { meta::path_id(key) } else { String::new() };
            disk.insert(key.clone(), Attached { id: id.clone(), created: made, modified: t, ..Attached::of(&id, &Meta::default()) });
            dirty = true;
        }
        if dirty {
            let local = self.local_state();
            let fresh = self.from_disk(&disk, &local);
            self.index = fresh;
            // Paths changed: links parsed before may point elsewhere now.
            self.parsed.clear();
        }
        let pending: HashSet<&str> = found.pending.iter().map(String::as_str).collect();
        // A folder that could not be read keeps what was listed in it.
        let shown = |k: &str| keys.contains(k) || (found.partial && !hidden(k));
        let mut out = vec![];
        let mut reparsed = false;
        let mut ids: Vec<String> = self.index.iter().filter(|(_, m)| m.folder.is_none()).map(|(k, _)| k.clone()).collect();
        ids.sort();
        if let Some(p) = progress { p.total.fetch_add(ids.len(), std::sync::atomic::Ordering::Relaxed); }
        for id in ids {
            if let Some(p) = progress { p.done.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
            let m = &self.index[&id];
            if !m.local() && !shown(&m.rel) { continue; }
            let p = self.path(&id);
            if m.local() && !p.fs_exists() { continue; }
            if !m.local() && pending.contains(m.rel.as_str()) && !p.fs_exists() {
                // Not downloaded yet: listed by its name, read once it is here.
                if let Some(ph) = layout::placeholder(&p) { layout::download(&ph); }
                let title = stem_of(base_name(&m.rel)).to_string();
                out.push(Entry { id: id.clone(), meta: m.clone(), title, icon: None, body: Default::default(), reminders: vec![], links: vec![], note: None, top: String::new() });
                continue;
            }
            let md = crate::vfs::metadata(&p).ok();
            let stamp = md.as_ref().and_then(|m| Some((m.modified().ok()?, m.len())));
            let parsed = match self.parsed.get(&id).filter(|c| stamp == Some((c.0, c.1))) {
                Some(c) => c.2.clone(),
                // Still only in iCloud (#406): listed by its name, read once
                // someone opens it; reading it here would download the library.
                None if crate::vfs::is_dataless(&p) || (!parse && !m.local()) => Parsed { title: stem_of(base_name(&m.rel)).to_string(), ..Default::default() },
                None => {
                    #[cfg(test)]
                    DOC_READS.with(|n| n.set(n.get() + 1));
                    // a 数据表 is not text: its words come from the file's cells
                    let table = table_file(&p);
                    let Ok(text) = (if table { Ok(String::new()) } else { crate::vfs::read_to_string(&p) }) else { continue };
                    let doc = if table { Document::default() } else { self.parse_doc(&id, &text).unwrap_or_default() };
                    let body = if table { crate::table::block_text(&self.top_dir, &crate::table::TableRef::new(p.strip_prefix(&self.top_dir).map(|r| r.to_string_lossy().into_owned()).unwrap_or_default())) } else { doc.plain_text() };
                    let parsed = Parsed { title: doc.title.text.clone(), icon: doc.icon.clone(), body: body.into(), reminders: doc.reminders(), links: doc.doc_links(), files: self.file_refs(&id, &doc) };
                    if let Some((t, len)) = stamp { self.parsed.insert(id.clone(), (t, len, parsed.clone())); reparsed = true; }
                    parsed
                }
            };
            let Parsed { mut title, icon, body, reminders, links, .. } = parsed;
            if m.named_by_file() { title = self.markdown_title(&id); }
            out.push(Entry { id, meta: m.clone(), title, icon, body, reminders, links, note: None, top: String::new() });
        }
        // Folders in id order: the index is a HashMap, whose order changes every time it is re-read.
        let mut folders: Vec<(&String, &Meta)> = self.index.iter().filter(|(_, m)| m.folder.is_some() && shown(&format!("{}/", m.rel))).collect();
        folders.sort_by(|a, b| a.0.cmp(b.0));
        for (id, meta) in folders {
            let title = meta.folder.clone().unwrap_or_default();
            out.push(Entry { id: id.clone(), meta: meta.clone(), title, icon: None, body: Default::default(), reminders: vec![], links: vec![], note: None, top: String::new() });
        }
        subdocs::mark_notes(&self.folder_notes(), &mut out);
        let seen: HashSet<&str> = out.iter().map(|e| e.id.as_str()).collect();
        self.parsed.retain(|id, _| seen.contains(id.as_str()));
        // Files that went only leave stale paths behind, which the next launch skips.
        if reparsed { let _ = self.save_parsed(); }
        // Not written (no lock): still seen here, the next `list` writes it.
        let mut wrote_gone = false;
        if dirty && _lock.is_some() {
            let _ = self.save_index();
            wrote_gone = !gone.is_empty() && self.write_gone(&gone).is_ok();
        }
        if self.v6 { self.saw(&disk, &keys, &found.pending, found.partial, &gone, wrote_gone); }
        // Entries for files not here are left out of memory but not of the
        // disk: memory and the disk still agree but for them (`Synced::absent`),
        // so the next write reads nothing again until one of their files
        // arrives (and then keeps what its device set) or a device file changes.
        // Written just above (`save_index` marked memory as the disk); unwritten, read next time.
        if disk_left_out && _lock.is_some() && let Some(s) = self.synced.get_mut() { s.absent.extend(left_out); }
        if self.clean_due == Some(false) { self.clean_due = Some(true); }
        out
    }

    /// Takes over what `from` (a `tree_writer` of this library that listed it
    /// off the main thread) read: the index as merged, the 回收站 and what each
    /// document's file holds. Safe whatever was written meanwhile: whether
    /// the next write reads the device files again is decided from the disk.
    pub fn adopt(&mut self, mut from: Store) {
        use std::mem::take;
        (self.index, self.foreign, self.trash) = (take(&mut from.index), take(&mut from.foreign), take(&mut from.trash));
        for (id, c) in from.parsed.drain() { self.parsed.entry(id).or_insert(c); }
        self.checked.extend(from.checked.drain());
        (self.seen, self.merged, self.synced, self.seen_trash, self.here) = (take(&mut from.seen), take(&mut from.merged), take(&mut from.synced), take(&mut from.seen_trash), take(&mut from.here));
        self.clock.set(self.clock.get().max(from.clock.get()));
        // A writer that began before this one (a listing, another window's move) read an older
        // library: taking it over afterwards would put the old paths back.
        self.writes.set(self.writes.get() + 1);
    }

    /// FORMAT 6: the files `list` found here noted in `meta::Here` (a
    /// placeholder is not one: it never was here), the `gone` ones dropped
    /// once their tombstones are written (kept until then).
    /// Only items with a record: one without is found from its file on
    /// every device, and nothing is written when it goes. A whole scan
    /// also forgets what is no longer listed.
    fn saw(&mut self, disk: &DiskIndex, keys: &HashSet<String>, pending: &[String], partial: bool, gone: &[(String, String)], wrote: bool) {
        let mut changed = false;
        let seen = &mut self.here.seen;
        if wrote { for (id, _) in gone { changed |= seen.remove(id).is_some(); } }
        let pending: HashSet<&str> = pending.iter().map(String::as_str).collect();
        let merged = self.merged.borrow();
        let mut listed = 0;
        for (k, a) in disk {
            match seen.get(&a.id) {
                Some(p) if p == k => { listed += 1; continue }
                Some(_) => listed += 1,
                None => {}
            }
            if a.id.is_empty() || !keys.contains(k) || pending.contains(k.as_str()) || !merged.items.contains_key(&a.id) { continue }
            if seen.insert(a.id.clone(), k.clone()).is_none() { listed += 1; }
            changed = true;
        }
        drop(merged);
        if !partial && listed + if wrote { 0 } else { gone.len() } < seen.len() {
            let ids: HashSet<&str> = disk.values().map(|a| a.id.as_str()).chain(gone.iter().filter(|_| !wrote).map(|g| g.0.as_str())).collect();
            seen.retain(|id, _| ids.contains(id.as_str()));
            changed = true;
        }
        if changed { let _ = self.here.save(&self.top_dir, &self.key, &self.me.id); }
    }

    /// FORMAT 6: `gone` tombstones for pages deleted outside the app, later
    /// than everything about them, with the path each was at (`revive`).
    /// `path` goes too, at the same stamp, so a device's own older `path`
    /// does not outlive the tombstone in its files.
    fn write_gone(&self, gone: &[(String, String)]) -> Result<()> {
        let at = now();
        let changes = gone.iter().flat_map(|(id, from)| [(id.clone(), "gone".to_string(), serde_json::json!({ "at": at, "from": from })), (id.clone(), "path".to_string(), serde_json::Value::Null)]).collect();
        meta::commit_over(&self.top_dir, &self.me, &self.clock, &mut self.merged.borrow_mut(), changes, &mut vec![])
    }

    /// FORMAT 6: files found with no entry (not in `disk`) where a page
    /// deleted outside the app in the last 30 days was (its `gone`'s `from`):
    /// they are that page back (a git branch switched back, a folder put
    /// back), so it gets its `path` again, later than the tombstone, and with
    /// it its id and all it had (pins, order …). Whether any did; then memory
    /// is read again from the device files.
    fn revive(&mut self, disk: &DiskIndex, keys: &HashSet<String>) -> bool {
        let fresh: Vec<&String> = keys.iter().filter(|k| !disk.contains_key(*k)).collect();
        if fresh.is_empty() { return false }
        let from = self.merged.borrow().gone_from(meta::now_ms().saturating_sub(meta::PURGED_KEPT_MS));
        let changes: Vec<_> = fresh.into_iter().filter_map(|k| Some((from.get(k)?.clone(), "path".to_string(), serde_json::Value::String(k.clone())))).collect();
        if changes.is_empty() || meta::commit_over(&self.top_dir, &self.me, &self.clock, &mut self.merged.borrow_mut(), changes, &mut vec![]).is_err() { return false }
        let mut warnings = vec![];
        self.load_meta(&mut warnings);
        self.warnings.extend(warnings);
        true
    }

    /// What `list` parsed last launch, for the files still where they were.
    fn load_parsed(&mut self) {
        let Some(c) = crate::vfs::read(parsed_cache_path(&self.top_dir, &self.key)).ok().and_then(|b| serde_json::from_slice::<ParsedCache>(&b).ok()) else { return };
        if c.version != parsed_cache_version() { return }
        let mut docs = c.docs;
        let ids: Vec<String> = self.index.iter().filter(|(_, m)| m.folder.is_none()).map(|(k, _)| k.clone()).collect();
        for id in ids {
            let Some(d) = docs.remove(&self.path(&id)) else { continue };
            let reminders = d.reminders.into_iter().map(|(expire, notify, should_notify, whole_day)| crate::model::InlineReminder { expire, notify, should_notify, whole_day }).collect();
            let links = d.links.into_iter().map(|(target, block, line)| crate::model::DocLink { target, block, line }).collect();
            self.parsed.insert(id, (d.modified, d.len, Parsed { title: d.title, icon: d.icon, body: d.body.into(), reminders, links, files: d.files }));
        }
    }

    fn save_parsed(&self) -> Result<()> {
        let docs = self.parsed.iter().map(|(id, (modified, len, p))| (self.path(id), CachedDoc {
            modified: *modified, len: *len, title: p.title.clone(), icon: p.icon.clone(), body: p.body.to_string(),
            reminders: p.reminders.iter().map(|r| (r.expire, r.notify, r.should_notify, r.whole_day)).collect(),
            links: p.links.iter().map(|l| (l.target.clone(), l.block, l.line.clone())).collect(),
            files: p.files.clone(),
        })).collect();
        let path = parsed_cache_path(&self.top_dir, &self.key);
        if let Some(dir) = path.parent() { crate::vfs::create_dir_all(dir)?; }
        write_cache(&path, &serde_json::to_vec(&ParsedCache { version: parsed_cache_version(), docs })?)
    }

    fn read_doc_text(&self, id: &str) -> Result<String> {
        let p = self.path(id);
        // a 数据表 is not text: the table reads its own file
        if self.kind(id) == DocKind::Table { anyhow::ensure!(p.fs_exists(), "数据表文件已不在原处，可能已被移动或删除"); return Ok(String::new()); }
        anyhow::ensure!(!crate::vfs::is_dataless(&p), NOT_DOWNLOADED);
        match crate::vfs::read_to_string(&p) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && layout::placeholder(&p).is_some_and(|ph| ph.fs_exists()) => {
                if let Some(ph) = layout::placeholder(&p) { layout::download(&ph); }
                anyhow::bail!(NOT_DOWNLOADED)
            }
            r => Ok(r?),
        }
    }

    /// The file of `id` when it is still only in iCloud (#406): `load` fails
    /// on it at once, and reading it (off the main thread) downloads it.
    pub fn cloud_only(&self, id: &str) -> Option<PathBuf> {
        let p = self.path(id);
        (self.index.contains_key(id) && crate::vfs::is_dataless(&p)).then_some(p)
    }

    pub fn load(&self, id: &str) -> Result<Document> {
        self.parse_doc(id, &self.read_doc_text(id)?)
    }

    /// Only opening an editor establishes a new save baseline; previews must
    /// not hide external changes from an already open editor.
    pub fn load_for_edit(&self, id: &str, shared: bool) -> Result<Document> {
        let xml = self.read_doc_text(id)?;
        let doc = self.parse_doc(id, &xml)?;
        if !shared { self.loaded.borrow_mut().insert(id.to_string(), xml); }
        Ok(doc)
    }

    /// Accept an external revision (another program, the `quiet` CLI, an
    /// agent writing a 画板) only when the caller has a clean editor. Documents
    /// and 画板; a 数据表 follows its file its own way. An error (a file
    /// caught half written, a 画板 emptied before it is written again) keeps
    /// the baseline: the caller keeps what it shows and tries again later.
    pub fn refresh_external(&self, id: &str) -> Result<Option<Document>> {
        let kind = self.kind(id);
        if kind == DocKind::Table { return Ok(None) }
        let disk = self.read_doc_text(id)?;
        if self.loaded.borrow().get(id).is_none_or(|base| base == &disk) { return Ok(None) }
        // An empty `.excalidraw` reads as an empty board: here it is a writer between truncating and writing.
        anyhow::ensure!(kind != DocKind::Board || !disk.trim().is_empty(), "画板文件是空的，可能正在写入");
        let doc = self.parse_doc(id, &disk)?;
        self.loaded.borrow_mut().insert(id.into(), disk);
        Ok(Some(doc))
    }

    /// Release a closed editor's conflict-detection baseline, after its last window saved.
    pub fn forget_loaded(&self, id: &str) { self.loaded.borrow_mut().remove(id); }

    pub fn save(&mut self, id: &str, doc: &Document) -> Result<Meta> {
        let _lock = self.begin_write()?;
        self.save_now(id, doc)
    }

    /// A move rewrote the links to files in `id`'s file (`relocate`) while an
    /// editor held them as they were, and nothing else changed: take the file
    /// as the editor's baseline, and give each link as it was → as it is now,
    /// for the editor to follow before it saves. Mentions only compare by
    /// title: their paths were rewritten too, the editor holds ids.
    pub fn follow_moved_links(&self, id: &str) -> Option<HashMap<String, String>> {
        // A 画板 has no links, and its file's canvas settings aren't in what is compared below.
        if self.kind(id) != DocKind::Doc { return None }
        let before = self.loaded.borrow().get(id)?.clone();
        let disk = self.read_doc_text(id).ok().filter(|d| *d != before)?;
        let (mut was, mut now) = (self.parse_doc(id, &before).ok()?, self.parse_doc(id, &disk).ok()?);
        let (a, b) = (was.links(), now.links());
        if a.len() != b.len() { return None; }
        let mut map: HashMap<String, String> = HashMap::new();
        for (x, y) in a.into_iter().zip(b) {
            if map.entry(x).or_insert_with(|| y.clone()) != &y { return None; }
        }
        map.retain(|x, y| x != y);
        was.map_links(&mut |h| map.get(h).cloned());
        for d in [&mut was, &mut now] {
            layout::each_inline(d, &mut |i| if let Some(l) = i.doc_link().cloned() { *i = docxml::to_inline(crate::model::InlineKind::DocLink(crate::model::InlineDocLink { id: String::new(), title: l.title })); });
        }
        // Block ids differ between reads: compare what is written.
        if docxml::serialize(&was) != docxml::serialize(&now) { return None; }
        self.loaded.borrow_mut().insert(id.to_string(), disk);
        Some(map)
    }

    /// A live Markdown editor writes its current body even after an external save.
    pub fn save_editor(&mut self, id: &str, doc: &Document) -> Result<Meta> {
        let _lock = self.begin_write()?;
        if !self.is_markdown(id) { return self.save_now(id, doc) }
        let disk = self.read_doc_text(id)?;
        self.write_doc_against(id, doc, Some(disk), true)
    }

    /// `save_editor` for the window's thread: the text is written, but a file
    /// name the new title asks for is left to `follow_name` (true then).
    /// Renaming the file rewrites every link to it, and under a 正文 moves
    /// the whole folder: seconds for a big one. Until it has followed, the
    /// document is where it was, under its old name.
    pub fn save_editor_naming_later(&mut self, id: &str, doc: &Document) -> Result<(Meta, bool)> {
        self.name_later.set(Some(false));
        let r = self.save_editor(id, doc);
        let due = self.name_later.take().unwrap_or(false);
        r.map(|m| (m, due))
    }

    /// What `save_editor_naming_later` left: `id`'s file (under a 正文, its
    /// folder) takes the name of its title, links following. A document's
    /// title is read from its file, so a save since is never undone; `title`
    /// is for a Markdown file or 画板, whose title is only its name.
    pub fn follow_name(&mut self, id: &str, title: &str) -> Result<()> {
        let _lock = self.begin_write()?;
        anyhow::ensure!(self.index.contains_key(id), "文档不存在");
        let title = if self.named_by_file(id) { title.trim().to_string() } else { self.load(id)?.title.text.trim().to_string() };
        if title.is_empty() { return Ok(()); }
        self.follow_title(id, &title, true)
    }

    /// Whether `follow_title` would move something in the library for this
    /// title. A 本机文件 is renamed beside itself and the old layout only
    /// notes the name: neither is worth putting off.
    fn title_moves(&self, id: &str, title: &str, renamed: bool) -> bool {
        let Some(m) = self.index.get(id).filter(|m| m.path.is_none() && !self.flat) else { return false };
        let base = file_name_for(title);
        // As `follow_title`: a name numbered because it was taken is looked at again only on a rename.
        let moves = |stem: &str| if layout::name_fits(stem, &base) { renamed && stem != base } else { true };
        let stem = stem_of(base_name(&m.rel));
        match self.folder_of_note(id) {
            Some(f) => moves(base_name(&self.index[&f].rel)) || !layout::name_fits(stem, &base),
            None => moves(stem),
        }
    }

    /// `save` for callers that already hold the write lock. A new id becomes a
    /// DocxXML file at the top of the library, named after its title.
    fn save_now(&mut self, id: &str, doc: &Document) -> Result<Meta> { self.save_in(id, doc, None) }

    /// `save_now`, a new id going into folder `parent`.
    fn save_in(&mut self, id: &str, doc: &Document, parent: Option<&str>) -> Result<Meta> {
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
        // A 数据表 writes its own file as it is edited; saving its page only follows the title.
        if self.kind(id) == DocKind::Table {
            let title = doc.title.text.trim().to_string();
            if !title.is_empty() && title != self.markdown_title(id) && let Err(e) = self.follow_title(id, &title, true) { self.warnings.push(format!("未能按标题重命名文件：{e}")); }
            let t = now();
            let m = self.index.get_mut(id).ok_or_else(|| anyhow::anyhow!("文档不存在"))?;
            m.modified = t;
            let m = m.clone();
            self.save_index()?;
            return Ok(m);
        }
        let disk = if self.is_markdown(id) || self.loaded.borrow().contains_key(id) { crate::vfs::read_to_string(self.path(id)).ok() } else { None };
        self.write_doc_against(id, doc, disk, false)
    }

    fn write_doc_against(&mut self, id: &str, doc: &Document, disk: Option<String>, overwrite: bool) -> Result<Meta> {
        let (xml, copies) = self.serialize_with_copies(id, doc, disk.as_deref());
        if !overwrite && let Some(previous) = self.loaded.borrow().get(id) {
            let disk = disk.as_deref().ok_or_else(|| anyhow::anyhow!("文档文件已不在原处，可能已被移动或删除"))?;
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
                    if !to.fs_exists() {
                        if let Some(dir) = to.parent() { crate::vfs::create_dir_all(dir)?; }
                        crate::vfs::copy(src, &to)?;
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
        let title = doc.title.text.trim().to_string();
        let named = self.named_by_file(id);
        let changed = if named { title != self.markdown_title(id) } else { true };
        // A document's title is in its file: changed when it differs from the one `list` read.
        let renamed = named || self.parsed.get(id).is_some_and(|p| p.2.title != title);
        if changed && !(named && title.is_empty()) {
            // The window's save leaves the move to `follow_name`, off its thread.
            if self.name_later.get().is_some() && self.title_moves(id, &title, renamed) { self.name_later.set(Some(true)); }
            else if let Err(e) = self.follow_title(id, &title, renamed) { self.warnings.push(format!("未能按标题重命名文件：{e}")); }
        }
        let t = now();
        let m = self.index.get_mut(id).ok_or_else(|| anyhow::anyhow!("文档不存在"))?;
        m.modified = t;
        let m = m.clone();
        self.save_index()?;
        // What `list` read of it is stale (a link added, for `relocate`); it reads it again anyway.
        if !same { self.parsed.remove(id); }
        self.loaded.borrow_mut().insert(id.to_string(), if same { disk.unwrap_or(xml) } else { xml });
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
        // Its pictures relative to its folder, as a save writes them: the library can move.
        let file = self.path(&id);
        let text = format!("{front}{}", crate::markdown::write_file_with(doc, "", &crate::markdown::Options { library_dir: Some(&self.top_dir), dir: file.parent(), ..Default::default() }).text);
        if let Err(e) = self.write_file(&self.path(&id), text.as_bytes()).and_then(|_| self.save_index()) { self.index.remove(&id); return Err(e); }
        self.record_file_change(&id, None);
        drop(lock);
        self.place_new(&id, parent)?;
        Ok(id)
    }

    /// A new 画板 in folder `parent`: a `.excalidraw` file named `title`, holding
    /// `board`. Only in the folder layout (the old one is migrated on open).
    pub fn create_board_in(&mut self, title: &str, board: &crate::whiteboard::Board, parent: Option<&str>) -> Result<String> {
        anyhow::ensure!(!self.flat, "旧格式的文档库不能新建画板");
        let id = uuid::Uuid::new_v4().to_string();
        let lock = self.begin_write()?;
        self.validate_folder(None, parent)?;
        let t = now();
        let dir = self.folder_rel(parent);
        let rel = join_rel(&dir, &self.free_name(&dir, &file_name_for(title), Some(DocKind::Board.ext(false)), None));
        self.index.insert(id.clone(), Meta { created: t, modified: t, opened: t, rel, ..Default::default() });
        derive_tree(&mut self.index);
        let text = board.file_json();
        if let Err(e) = self.write_file(&self.path(&id), text.as_bytes()).and_then(|_| self.save_index()) { self.index.remove(&id); return Err(e); }
        self.record_file_change(&id, None);
        drop(lock);
        self.place_new(&id, parent)?;
        Ok(id)
    }

    /// A new 数据表 in folder `parent`: a `.sqlite` file named `title`, holding
    /// `base` (`Base::blank_named`, or an imported CSV). Only in the folder layout.
    #[cfg(not(target_family = "wasm"))]
    pub fn create_table_in(&mut self, title: &str, base: &crate::table::Base, parent: Option<&str>) -> Result<String> {
        anyhow::ensure!(!self.flat, "旧格式的文档库不能新建数据表");
        let id = uuid::Uuid::new_v4().to_string();
        let lock = self.begin_write()?;
        self.validate_folder(None, parent)?;
        let t = now();
        let dir = self.folder_rel(parent);
        let rel = join_rel(&dir, &self.free_name(&dir, &file_name_for(title), Some(DocKind::Table.ext(false)), None));
        self.index.insert(id.clone(), Meta { created: t, modified: t, opened: t, rel, ..Default::default() });
        derive_tree(&mut self.index);
        if let Err(e) = crate::table::sqlite::create(&self.path(&id), base).and_then(|_| self.save_index()) { self.index.remove(&id); return Err(e); }
        drop(lock);
        self.place_new(&id, parent)?;
        Ok(id)
    }

    /// 创建副本 of a 数据表: its file copied under `title`, beside nothing in particular (the caller places it).
    #[cfg(not(target_family = "wasm"))]
    pub fn copy_table(&mut self, src: &str, title: &str) -> Result<String> {
        anyhow::ensure!(!self.flat && self.kind(src) == DocKind::Table, "不是数据表");
        let id = uuid::Uuid::new_v4().to_string();
        let lock = self.begin_write()?;
        let t = now();
        let rel = self.free_name("", &file_name_for(title), Some(DocKind::Table.ext(false)), None);
        let from = self.path(src);
        self.index.insert(id.clone(), Meta { created: t, modified: t, opened: t, rel, ..Default::default() });
        derive_tree(&mut self.index);
        if let Err(e) = crate::vfs::copy(&from, self.path(&id)).map_err(anyhow::Error::from).and_then(|_| self.save_index()) { self.index.remove(&id); return Err(e); }
        drop(lock);
        Ok(id)
    }

    /// 另存副本 of a 数据表's version: the snapshot's file as a new 数据表 named `title`, beside the original.
    #[cfg(not(target_family = "wasm"))]
    pub fn copy_table_version(&mut self, src: &str, name: &str, title: &str) -> Result<String> {
        anyhow::ensure!(!self.flat && self.kind(src) == DocKind::Table, "不是数据表");
        let bytes = crate::versions::read_bytes(&self.history_base(), src, name, false)?;
        let parent = self.index.get(src).and_then(|m| m.parent.clone());
        let id = uuid::Uuid::new_v4().to_string();
        let lock = self.begin_write()?;
        let t = now();
        let dir = self.folder_rel(parent.as_deref());
        let rel = join_rel(&dir, &self.free_name(&dir, &file_name_for(title), Some(DocKind::Table.ext(false)), None));
        self.index.insert(id.clone(), Meta { created: t, modified: t, opened: t, rel, ..Default::default() });
        derive_tree(&mut self.index);
        if let Err(e) = write_atomic(&self.path(&id), &bytes).and_then(|_| self.save_index()) { self.index.remove(&id); return Err(e); }
        drop(lock);
        self.place_new(&id, parent.as_deref())?;
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
        // 自动排序 must not rewrite the hand order: the page stays unplaced,
        // and 手动排序 shows it at the top.
        if parent.is_some() { self.move_in_tree_with(id, parent, None, self.place_by_hand)?; }
        Ok(())
    }

    /// A Markdown document's front matter, as on disk.
    pub fn front_matter_of(&self, id: &str) -> String {
        if !self.is_markdown(id) { return String::new(); }
        let text = crate::vfs::read_to_string(self.path(id)).unwrap_or_default();
        crate::markdown::front_matter(text.trim_start_matches('\u{feff}')).0.to_string()
    }

    /// Where a 本机文件 is.
    pub fn local_path(&self, id: &str) -> Option<PathBuf> { self.index.get(id)?.path.as_ref().map(PathBuf::from) }

    /// 打开 a Markdown file where it is (本机文件): its document id, the one
    /// it already has when it was opened before. A file inside the library
    /// is the library's document.
    pub fn open_local(&mut self, path: &std::path::Path) -> Result<String> {
        let path = path.fs_canonicalize()?;
        // Windows canonical paths are `\\?\C:\…`: keep the ordinary form, which
        // the Explorer and the title show.
        let path = match path.to_str().and_then(|p| p.strip_prefix(r"\\?\")).filter(|p| !p.starts_with("UNC\\")) { Some(p) => PathBuf::from(p), None => path };
        anyhow::ensure!(path.fs_is_file(), "文件不存在");
        let _lock = self.begin_write()?;
        if let Some(rel) = self.top_dir.fs_canonicalize().ok().and_then(|r| path.strip_prefix(r).ok().map(|p| p.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")))
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
        // Made and changed as the file was, not when it was first opened here.
        let (made, changed) = crate::vfs::file_times(&path).unwrap_or((t, t));
        self.index.insert(id.clone(), Meta { created: made, modified: changed, opened: t, markdown: true, path: Some(key), ..Default::default() });
        if let Err(e) = self.save_index() { self.index.remove(&id); return Err(e); }
        Ok(id)
    }

    /// Size of the document on disk, plus the local images and files it references.
    pub fn size_of(&self, id: &str, doc: &Document) -> u64 {
        let mut n = crate::vfs::metadata(self.path(id)).map(|m| m.len()).unwrap_or(0);
        for src in doc.asset_sources() {
            if let Some(p) = image_path(&src) {
                n += crate::vfs::metadata(p).map(|m| m.len()).unwrap_or(0);
            }
        }
        n
    }

    /// Move a document or folder to the 回收站; `root` is the page whose
    /// deletion takes it along (`None`: deleted itself). A folder takes what
    /// is in it along (deleting those afterwards does nothing more).
    pub fn delete(&mut self, id: &str, root: Option<&str>) -> Result<()> {
        let before = self.warnings.len();
        let (_, r) = self.delete_many(&[(id.to_string(), root.map(str::to_string))]);
        // One page: what went wrong is the error, not a warning.
        if r.is_ok() && self.warnings.len() > before { return Err(anyhow::anyhow!(self.warnings.remove(before))); }
        r
    }

    /// `delete` for several pages, each with the page whose deletion takes
    /// it along, in order (one whose page is listed and did not go stays). One lock, the
    /// 回收站's index written once before anything moves and the library's
    /// once after, however many there are: each write is synced to the disk,
    /// which can take a good part of a second when the disk is busy.
    /// A page that cannot go is a warning and the others still go. Returns
    /// the pages now gone, and the error that stopped the moves midway.
    pub fn delete_many(&mut self, list: &[(String, Option<String>)]) -> (Vec<String>, Result<()>) {
        let _lock = match self.begin_write() { Ok(l) => l, Err(e) => return (vec![], Err(e)) };
        let mut items: Vec<(String, Option<String>)> = vec![];
        let mut gone: Vec<String> = vec![];
        let mut forgot = false;
        for (id, root) in list {
            if root.as_ref().is_some_and(|r| list.iter().any(|(k, _)| k == r) && !gone.contains(r)) { continue; }
            // Already taken along by a folder, or deleted before.
            if items.iter().any(|(k, _)| k == id) || (!self.index.contains_key(id) && self.trash.contains_key(id)) { gone.push(id.clone()); continue; }
            // 本机文件: only taken off the list; the file is the user's.
            if self.index.get(id).is_some_and(Meta::local) {
                self.index.remove(id);
                self.loaded.borrow_mut().remove(id);
                forgot = true;
                gone.push(id.clone());
                continue;
            }
            if !self.index.contains_key(id) { self.warnings.push("文档或文件夹不存在".into()); continue; }
            let ride = root.clone().unwrap_or_else(|| id.clone());
            let mut group = vec![];
            if self.index[id].folder.is_some() && !self.flat {
                let prefix = format!("{}/", self.index[id].rel);
                let mut inside: Vec<(String, usize, bool)> = self.index.iter().filter(|(_, m)| !m.local() && m.rel.starts_with(&prefix))
                    .map(|(k, m)| (k.clone(), m.rel.matches('/').count(), m.folder.is_some())).collect();
                // Deepest first, documents before the folder holding them.
                inside.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)).then(a.0.cmp(&b.0)));
                group.extend(inside.into_iter().map(|(k, _, _)| (k, Some(ride.clone()))).filter(|(k, _)| !items.iter().any(|(x, _)| x == k)));
            }
            group.push((id.clone(), root.clone()));
            // A name already taken in the 回收站 keeps this page (and only it) out.
            match group.iter().find(|(k, _)| self.trash_path(k).fs_try_exists().unwrap_or(true)) {
                Some((k, _)) => self.warnings.push(format!("回收站已存在同名文档，未覆盖：{k}")),
                None => { items.extend(group); gone.push(id.clone()); }
            }
        }
        let r = if items.is_empty() { Ok(()) } else { self.trash_many(&items, true) };
        // A failed move leaves the rest where they were (and still listed).
        if r.is_err() { gone.retain(|id| !self.index.contains_key(id)); }
        if forgot && (items.is_empty() || r.is_err()) && let Err(e) = self.save_index() { return (gone, Err(e)); }
        (gone, r)
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
            anyhow::ensure!(!target.fs_try_exists()? && !targets.contains(&target), "回收站已存在同名文档，未覆盖：{id}");
            let meta = self.index.get(id).cloned().unwrap_or_default();
            trash.insert(id.clone(), Trashed { meta, deleted: now(), root: root.clone() });
            targets.push(target);
        }
        crate::vfs::create_dir_all(self.trash_dir())?;
        // Persist destination metadata before moving content. An interrupted
        // move leaves extra index entries, never an unindexed trashed file.
        self.write_trash(&trash)?;
        self.trash = trash;
        let mut failed = None;
        for (n, ((id, _), target)) in items.iter().zip(&targets).enumerate() {
            let before = if record { self.file_state(id) } else { None };
            let from = self.path(id);
            let folder = self.index.get(id).is_some_and(|m| m.folder.is_some());
            if (!folder || from.fs_is_dir()) && let Err(e) = crate::vfs::rename(&from, target) { failed = Some((n, e)); break; }
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
        let _ = crate::vfs::create_dir_all(&d);
        d
    }

    /// A trashed document's file, `<id>.xml` / `<id>.md`; a folder's, `<id>/`.
    fn trash_path(&self, id: &str) -> PathBuf {
        let meta = self.trash.get(id).map(|t| &t.meta).or_else(|| self.index.get(id));
        let dir = self.internal().join("trash");
        match meta {
            Some(m) if m.folder.is_some() => dir.join(id),
            m => dir.join(format!("{id}.{}", m.map_or("xml", ext_of))),
        }
    }

    fn save_trash(&self) -> Result<()> { self.write_trash(&self.trash) }

    /// Keep `trash` as the 回收站: `trash/index.json`, or in FORMAT 6 this
    /// device's fields (sync.md 3.3) — `trashed` for what went in, `null`
    /// for what came back, and a `purged` tombstone for what is gone for good.
    fn write_trash(&self, trash: &HashMap<String, Trashed>) -> Result<()> {
        crate::vfs::create_dir_all(self.trash_dir())?;
        if !self.v6 { return save_json(&self.trash_index_file(), trash) }
        let purging = std::mem::take(&mut *self.purging.borrow_mut());
        let seen = self.seen_trash.borrow().clone();
        let mut changes = vec![];
        for (id, t) in trash {
            let v = meta::trashed_value(t);
            if seen.get(id) != Some(&v) { changes.push((id.clone(), "trashed".to_string(), v)); }
        }
        for id in seen.keys().filter(|id| !trash.contains_key(*id)) {
            changes.push(if purging.contains(id) { (id.clone(), "purged".to_string(), serde_json::Value::from(now())) } else { (id.clone(), "trashed".to_string(), serde_json::Value::Null) });
        }
        if changes.is_empty() { return Ok(()) }
        changes.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        meta::commit_over(&self.top_dir, &self.me, &self.clock, &mut self.merged.borrow_mut(), changes, &mut vec![])?;
        *self.seen_trash.borrow_mut() = trash.iter().map(|(id, t)| (id.clone(), meta::trashed_value(t))).collect();
        Ok(())
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
        let _lock = match self.begin_write() {
            Ok(lock) => Some(lock),
            Err(e) => { self.warnings.push(format!("无法锁定文档库：{e}")); None }
        };
        let dir = self.internal().join("trash");
        let _ = crate::vfs::create_dir_all(&dir);
        let mut dirty = false;
        let mut on_disk = HashSet::new();
        // Unreadable folder: list what the index knows, and leave it alone.
        let listed = crate::vfs::read_dir(&dir);
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
                if p.fs_is_dir() {
                    if let Some(id) = p.file_name().and_then(|s| s.to_str()) { on_disk.insert(id.to_string()); }
                    continue;
                }
                let Some((kind, markdown)) = p.file_name().and_then(|s| s.to_str()).and_then(DocKind::of_name) else { continue };
                let Some(id) = p.file_stem().and_then(|s| s.to_str()).map(str::to_string) else { continue };
                // FORMAT 6: a file whose record has not arrived yet waits for it.
                if !self.trash.contains_key(&id) && !self.v6 {
                    let t = e
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or_else(now);
                    // Modification time is not deletion time. Recovered orphans
                    // get a full retention period instead of immediate expiry.
                    let meta = Meta { created: t, modified: t, markdown, kind, name: (markdown || !kind.is_doc()).then(|| "未命名".into()), ..Default::default() };
                    self.trash.insert(id.clone(), Trashed { meta, deleted: now(), root: None });
                    dirty = true;
                }
                on_disk.insert(id);
            }
        }
        // FORMAT 6: a record whose file has not arrived yet is kept, only not shown.
        if readable && !self.v6 {
            let before = self.trash.len();
            self.trash.retain(|id, t| t.meta.folder.is_some() || on_disk.contains(id));
            dirty |= self.trash.len() != before;
        }
        let cutoff = now().saturating_sub(TRASH_DAYS * 86_400);
        let expired: Vec<String> = self.trash.iter().filter(|(_, t)| t.deleted <= cutoff).map(|(k, _)| k.clone()).collect();
        for id in expired.into_iter().filter(|_| _lock.is_some()) {
            let p = self.trash_path(&id);
            let r = if p.fs_is_dir() { crate::vfs::remove_dir_all(&p) } else { crate::vfs::remove_file(&p) };
            match r {
                Ok(()) => { self.trash.remove(&id); self.purging.borrow_mut().insert(id.clone()); dirty = true; crate::versions::purge(&self.history_base(), &id); }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => { self.trash.remove(&id); self.purging.borrow_mut().insert(id.clone()); dirty = true; crate::versions::purge(&self.history_base(), &id); }
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
            .filter(|(id, t)| !self.v6 || !readable || t.meta.folder.is_some() || on_disk.contains(*id))
            .map(|(id, t)| {
                let doc = if t.meta.named_by_file() || t.meta.folder.is_some() || crate::vfs::is_dataless(&self.trash_path(id)) { Document::default() } else { crate::vfs::read_to_string(self.trash_path(id)).ok().and_then(|x| docxml::parse(&x).ok()).unwrap_or_default() };
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
            if let Some((order, top_order)) = place { meta.order = order; meta.top_order = top_order; }
            let markdown = meta.markdown;
            let folder = meta.folder.is_some();
            self.index.insert(id.to_string(), meta);
            if !folder {
                let to = self.path(id);
                anyhow::ensure!(!to.fs_try_exists()? , "文档库已存在同名文档，未覆盖：{id}");
                if let Err(e) = crate::vfs::rename(&src, &to) { self.index.remove(id); return Err(e.into()); }
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
        let text = if meta.folder.is_none() && !meta.named_by_file() { crate::vfs::read_to_string(&src).ok() } else { None };
        // Read where it was, so relative links resolve as they did.
        let doc = text.as_deref().filter(|x| x.contains("type=\"doc\"")).and_then(|x| {
            let mut d = docxml::parse(x).ok()?;
            self.links_to_ids(parent_rel(&t.meta.rel), &mut d);
            Some(d)
        });
        let rel = if let Some(name) = &meta.folder {
            let rel = join_rel(&dir, &self.free_name(&dir, &file_name_for(name), None, None));
            if src.fs_is_dir() { crate::vfs::rename(&src, self.abs(&rel))?; } else { crate::vfs::create_dir_all(self.abs(&rel))?; }
            rel
        } else {
            let stem = stem_of(base_name(&t.meta.rel));
            let base = if !t.meta.rel.is_empty() { if meta.named_by_file() { stem } else { layout::unnumbered(stem) }.to_string() }
                else if meta.named_by_file() { file_name_for(meta.name.as_deref().unwrap_or("")) }
                else { file_name_for(&text.as_deref().and_then(|x| docxml::parse(x).ok()).map(|d| d.title.text).unwrap_or_default()) };
            let rel = join_rel(&dir, &self.free_name(&dir, &base, Some(ext_of(&meta)), None));
            crate::vfs::rename(&src, self.abs(&rel))?;
            rel
        };
        meta.rel = rel;
        meta.parent = parent;
        if let Some((order, top_order)) = place { meta.order = order; meta.top_order = top_order; }
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
        let Some(m) = self.index.get(id) else { return false };
        if m.folder.is_some() { return self.flat || self.abs(&m.rel).fs_is_dir() }
        #[cfg(test)]
        crate::store::DISK_LOOKUPS.with(|n| n.set(n.get() + 1));
        self.path(id).fs_exists()
    }

    /// 彻底删除 a page from the 回收站, with the pages that went with it.
    pub fn purge(&mut self, id: &str) -> Result<()> { self.purge_many(&[id.to_string()]) }

    /// `purge` several pages (清空回收站), in order, stopping at the first
    /// that fails. The 回收站's index is written once at the end: a crash
    /// midway leaves entries whose files are gone, which `list_trash` drops.
    pub fn purge_many(&mut self, ids: &[String]) -> Result<()> {
        let _lock = self.begin_write()?;
        let mut r = Ok(());
        'pages: for id in ids {
            for k in self.trash_group(id) {
                let p = self.trash_path(&k);
                let removed = if p.fs_is_dir() { crate::vfs::remove_dir_all(&p) } else { crate::vfs::remove_file(&p) };
                match removed {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => { r = Err(e.into()); break 'pages; }
                }
                self.trash.remove(&k);
                self.purging.borrow_mut().insert(k.clone());
                crate::versions::purge(&self.history_base(), &k);
            }
        }
        let saved = self.save_trash();
        r.and(saved)
    }

    /// Copy a DocxXML file into the library under a fresh id.
    pub fn import(&mut self, path: &std::path::Path) -> Result<String> { self.import_into(path, None) }

    /// `import` into folder `parent`.
    pub fn import_into(&mut self, path: &std::path::Path, parent: Option<&str>) -> Result<String> {
        let xml = crate::vfs::read_to_string(path)?;
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
            let (rel, folder, parent, markdown, name, path, kind) = (m.rel.clone(), m.folder.clone(), m.parent.clone(), m.markdown, m.name.clone(), m.path.clone(), m.kind);
            f(m);
            (m.rel, m.folder, m.parent, m.markdown, m.name, m.path, m.kind) = (rel, folder, parent, markdown, name, path, kind);
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

    /// Parse a snapshot into a document, for 另存副本. A 数据表's is its page
    /// showing the snapshot file itself (which opens read-only, being history).
    pub fn version_document(&self, id: &str, name: &str) -> Result<Document> {
        if self.kind(id) == DocKind::Table {
            let p = crate::versions::path(&self.history_base(), id, name, false)?;
            anyhow::ensure!(p.fs_exists(), "历史版本已不存在");
            let rel = crate::paths::portable_in(&p, &self.top_dir).unwrap_or_default();
            return Ok(crate::table::file_doc(&self.markdown_title(id), &rel));
        }
        let text = self.version_text(id, name)?;
        self.parse_doc(id, &text)
    }

    /// A 数据表's file kept as a version: `force` is 保存当前版本 (no interval;
    /// the same file as the newest version still adds nothing).
    #[cfg(not(target_family = "wasm"))]
    pub fn snapshot_table(&self, id: &str, force: bool) -> Result<bool> {
        anyhow::ensure!(self.kind(id) == DocKind::Table, "不是数据表");
        let bytes = crate::vfs::read(self.path(id))?;
        let before = self.versions(id).first().map(|v| v.name.clone());
        crate::versions::snapshot_bytes(&self.history_base(), id, "sqlite", &bytes, crate::time::now_ms(), self.settings().history_limit(), force)?;
        Ok(self.versions(id).first().map(|v| v.name.clone()) != before)
    }

    /// The text a version's changes are counted on: a 数据表's rows as lines.
    fn version_body(&self, id: &str, name: Option<&str>) -> Result<String> {
        if self.kind(id) == DocKind::Table {
            let p = match name { Some(n) => crate::versions::path(&self.history_base(), id, n, false)?, None => self.path(id) };
            return Ok(crate::table::diff_text(&p));
        }
        match name { Some(n) => self.version_text(id, n), None => Ok(crate::vfs::read_to_string(self.path(id))?) }
    }

    /// Diff a snapshot against a baseline. `None` is the document on disk
    /// now; `Some(other)` compares two snapshots. Lines are ordered by time,
    /// so `-` is always "only in the earlier version". The unchanged lines
    /// are kept, so the same list doubles as a read-only preview.
    pub fn version_diff(&self, id: &str, name: &str, baseline: Option<&str>) -> Result<Vec<crate::diff::Line>> {
        let a = self.version_body(id, Some(name))?;
        let a_ms = crate::versions::stamp(name).unwrap_or(0);
        let (b, b_ms) = match baseline {
            Some(other) => (self.version_body(id, Some(other))?, crate::versions::stamp(other).unwrap_or(0)),
            None => (self.version_body(id, None)?, u64::MAX),
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
        // a 数据表: its file now kept as a version, then the version's file put in its place
        if self.kind(id) == DocKind::Table {
            let bytes = crate::versions::read_bytes(&self.history_base(), id, name, false)?;
            let _ = self.snapshot_table(id, true);
            write_atomic(&self.path(id), &bytes)?;
            return Ok(crate::table::file_doc(&self.markdown_title(id), &self.index[id].rel));
        }
        let text = self.version_text(id, name)?;
        let current = crate::vfs::read_to_string(self.path(id)).unwrap_or_default();
        let ext = if self.is_markdown(id) { "md" } else { "xml" };
        let _ = crate::versions::snapshot(&self.history_base(), id, ext, &current, crate::time::now_ms(), self.settings().history_limit(), true);
        self.parse_doc(id, &text)
    }

    /// 手动保存版本: bypass the interval, still capped by the retention limit.
    /// `false` means the text matched the newest snapshot, so nothing was
    /// written — the caller can say so instead of silently doing nothing.
    pub fn save_version_now(&self, id: &str) -> Result<bool> {
        if self.kind(id) == DocKind::Table { return self.snapshot_table(id, true); }
        let text = crate::vfs::read_to_string(self.path(id))?;
        let ext = if self.is_markdown(id) { "md" } else { "xml" };
        let before = self.versions(id).len();
        crate::versions::snapshot(&self.history_base(), id, ext, &text, crate::time::now_ms(), self.settings().history_limit(), true)?;
        Ok(self.versions(id).len() > before)
    }

    pub fn settings(&self) -> Settings {
        if self.flat { return reread(&self.internal().join("settings.json")).unwrap_or_default(); }
        let mut s: Settings = reread(&self.internal().join("settings.json")).unwrap_or_default();
        let local = self.read_local();
        s.window_origin = local.window_origin;
        s.window_display = local.window_display;
        s.window_size = local.window_size;
        s.quick_note_size = local.quick_note_size;
        if self.v5 {
            // The top-level folder's own (D5): in the devices' files, the latest of each.
            let m = self.merged.borrow();
            s.history_limit = m.get(meta::FOLDER_ITEM, "history_limit").and_then(|v| v.as_u64()).map(|n| n as u32);
            s.scroll_top = m.items.keys().filter_map(|id| Some((id.clone(), serde_json::from_value(m.get(id, "scroll")?.clone()).ok()?))).collect();
        }
        s
    }

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        if self.flat { return write_atomic(&self.internal().join("settings.json"), &serde_json::to_vec_pretty(s)?); }
        let mut local = self.read_local();
        local.window_origin = s.window_origin;
        local.window_display.clone_from(&s.window_display);
        (local.window_size, local.quick_note_size) = (s.window_size, s.quick_note_size);
        self.save_local(&local)?;
        if self.v5 {
            let was = self.settings();
            let mut changes = vec![];
            if was.history_limit != s.history_limit { changes.push((meta::FOLDER_ITEM.to_string(), "history_limit".to_string(), serde_json::to_value(s.history_limit)?)); }
            for (id, v) in &s.scroll_top { if was.scroll_top.get(id) != Some(v) { changes.push((id.clone(), "scroll".into(), serde_json::to_value(v)?)); } }
            for id in was.scroll_top.keys() { if !s.scroll_top.contains_key(id) { changes.push((id.clone(), "scroll".into(), serde_json::Value::Null)); } }
            if changes.is_empty() { return Ok(()) }
            // Not inside `begin_write`: the lock is taken here, so two copies never write this device's file at once.
            let _lock = if self.batch.lock.is_some() { WriteLock(vec![]) } else { WriteLock(self.lock_files(true)?) };
            meta::commit_over(&self.top_dir, &self.me, &self.clock, &mut self.merged.borrow_mut(), changes, &mut vec![])?;
            return Ok(())
        }
        let shared = Settings { window_origin: None, window_display: None, window_size: None, quick_note_size: None, ..s.clone() };
        crate::vfs::create_dir_all(self.internal())?;
        write_atomic(&self.internal().join("settings.json"), &serde_json::to_vec_pretty(&shared)?)
    }

    /// 阅读位置: the list offset this document was left at.
    pub fn scroll_top(&self, id: &str) -> Option<(usize, f32)> {
        if let Some(top) = self.view.borrow().scroll.get(id) { return *top }
        let v: [f32; 2] = if self.v5 { serde_json::from_value(self.merged.borrow().get(id, "scroll")?.clone()).ok()? } else { self.settings().scroll_top.get(id).copied()? };
        Some((v[0] as usize, v[1]))
    }

    /// Remember — or, with `None`, forget — where a document was left; on
    /// disk at the next [`Self::flush_view_state`].
    pub fn remember_scroll(&self, id: &str, top: Option<(usize, f32)>) {
        self.view.borrow_mut().scroll.insert(id.to_string(), top);
    }

    /// 重命名 a document that isn't open: rewrite its title on disk and its
    /// file name; a folder's name is its folder's.
    pub fn rename(&mut self, id: &str, title: &str) -> Result<Meta> {
        // A folder with a 正文 is renamed through it, so the two keep one name.
        if let Some(note) = self.note_of(id) { return self.rename(&note, title); }
        let _lock = self.begin_write()?;
        let before = self.file_state(id);
        if self.index.get(id).is_some_and(|m| m.folder.is_some() || m.named_by_file()) {
            // A Markdown file or a 画板 is left as it is: only its name changes.
            self.follow_title(id, title, true)?;
            let m = self.index.get_mut(id).unwrap();
            m.modified = now();
            let meta = m.clone();
            self.save_index()?;
            if !meta.local() { self.record_file_change(id, before); }
            return Ok(meta);
        }
        let mut doc = self.load(id)?;
        let was = doc.title.text.trim().to_string();
        doc.title = crate::model::RichText::plain(title);
        let mut meta = self.save_now(id, &doc)?;
        // The save follows the title only as far as `list` last read it.
        let title = doc.title.text.trim();
        if was != title && !title.is_empty() { self.follow_title(id, title, true)?; meta = self.index[id].clone(); }
        self.record_file_change(id, before);
        Ok(meta)
    }

    /// Note that `id` was just opened (this computer only); on disk at the
    /// next [`Self::flush_view_state`] or with any other write of the index.
    pub fn touch(&mut self, id: &str) -> Option<Meta> {
        let m = self.index.get_mut(id)?;
        m.opened = now();
        let m = m.clone();
        self.view.get_mut().opened.insert(id.to_string(), m.opened);
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
thread_local! {
    /// Disk existence checks performed by `index_has`; compiled only in tests,
    /// to keep imports O(N) instead of O(N × library) (#216). Per thread:
    /// tests run in parallel and each counts only its own lookups.
    pub(crate) static DISK_LOOKUPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Atomic writes synced to the disk (each one or two full syncs).
    pub(crate) static SYNCED_WRITES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Documents `list` read and parsed (not taken from its cache).
    pub(crate) static DOC_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Run by the next `list` between walking the folders and taking the lock: a write made
    /// meanwhile, however fast or slow the machine is.
    pub(crate) static AFTER_WALK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
    /// Times `load_meta` took the device files in again.
    pub(crate) static META_LOADS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(not(test))]
mod tests { thread_local!(pub static STAY_4: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }); thread_local!(pub static STAY_5: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }); }

#[cfg(test)]
mod tests {
    use super::*;

    // Libraries opened while set stay FORMAT 4, as in a synced folder.
    thread_local!(pub static STAY_4: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });
    // … and FORMAT 5.
    thread_local!(pub static STAY_5: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });

    /// A library in a fresh temporary folder, removed on drop.
    struct Lib(PathBuf);

    impl Lib {
        fn new(name: &str) -> Self {
            let d = crate::vfs::temp_dir().join(format!("fdl-test-{name}-{}", uuid::Uuid::new_v4()));
            Lib(d)
        }
        fn open(&self) -> Store {
            Store::open_at(self.0.clone()).unwrap()
        }
        /// What every device's file says, by path.
        fn index(&self) -> DiskIndex { meta::load(&self.0, &device().id, &mut vec![]).disk_index() }
        /// Device `d`'s file holding `item`: its shard, or its whole file before shards.
        fn dev_file(&self, d: &str, item: &str) -> PathBuf {
            let own = meta::dir(&self.0).join(d);
            if own.fs_is_dir() { own.join(format!("{:02x}.json", meta::shard_of(item))) } else { meta::dir(&self.0).join(format!("{d}.json")) }
        }
        /// Device `d`'s files as one: `{"items": …}` from its shards, or its whole file.
        fn dev_items(&self, d: &str) -> serde_json::Value {
            let own = meta::dir(&self.0).join(d);
            if !own.fs_is_dir() { return reread(&meta::dir(&self.0).join(format!("{d}.json"))).unwrap_or(serde_json::json!({})) }
            let mut items = serde_json::Map::new();
            for e in crate::vfs::read_dir(&own).unwrap().flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name == "device.json" || !name.ends_with(".json") { continue }
                let v: serde_json::Value = reread(&e.path()).unwrap();
                items.extend(v["items"].as_object().cloned().unwrap_or_default());
            }
            serde_json::json!({ "items": items })
        }
        /// `dev_items` as the files print it.
        fn dev_text(&self, d: &str) -> String { serde_json::to_string_pretty(&self.dev_items(d)).unwrap() }
        /// This device's files as one (`dev_text`).
        fn meta_text(&self) -> String { self.dev_text(&device().id) }
        /// Moved to the new layout, then opened.
        fn migrated(&self) -> Store {
            Store::migrate(&self.0).unwrap();
            self.open()
        }
    }

    impl Drop for Lib {
        fn drop(&mut self) {
            let _ = crate::vfs::remove_dir_all(&self.0);
            let _ = crate::vfs::remove_file(super::parsed_cache_path(&self.0, &library_key(&self.0)));
        }
    }

    fn doc(title: &str) -> Document {
        Document { title: crate::model::RichText::plain(title), ..Default::default() }
    }

    /// #216: importing N files must stay O(N) disk checks — the per-file
    /// drop used to stat every entry of the library through `move_in_tree`.
    #[test]
    fn importing_files_does_not_walk_the_library() {
        let lib = Lib::new("import-walk");
        let mut store = lib.open();
        let folder = store.create_folder("imported", None).unwrap();
        for _ in 0..30 { store.create_from(&doc("background")).unwrap(); }
        DISK_LOOKUPS.with(|n| n.set(0));
        for _ in 0..10 {
            let id = store.create_from(&doc("page")).unwrap();
            store.set_parent(&id, Some(&folder)).unwrap();
        }
        let looked = DISK_LOOKUPS.with(|n| n.get());
        assert!(looked <= 20, "10 imports made {looked} disk checks, not O(N)");
    }

    fn doc_with(title: &str, body: &str) -> Document {
        let mut d = doc(title);
        d.blocks.push(crate::model::Block::text(body));
        d
    }

    /// 自动排序's new pages leave the hand order alone: switching back to
    /// 手动排序 shows what the user arranged, not what sorting touched.
    #[test]
    fn automatic_sort_new_page_keeps_the_hand_order() {
        let lib = Lib::new("sort-hand-order");
        let mut s = lib.open();
        let folder = s.create_folder("项目", None).unwrap();
        // 手动排序 is the default: pages made in a folder arrive numbered.
        s.create_from_in(&doc("a"), Some(&folder)).unwrap();
        s.create_from_in(&doc("b"), Some(&folder)).unwrap();
        let hand = |s: &mut Store| s.list().into_iter()
            .filter(|e| e.meta.parent.as_deref() == Some(folder.as_str()))
            .map(|e| (e.id, e.meta.order)).collect::<Vec<_>>();
        let before = hand(&mut s);
        assert_eq!(before.len(), 2);
        assert!(before.iter().all(|(_, order)| *order > 0));
        // 自动排序: the new page stays unplaced and its siblings keep their numbers.
        s.set_place_by_hand(false);
        let new = s.create_from_in(&doc("c"), Some(&folder)).unwrap();
        let after = hand(&mut s);
        let untouched = after.iter().filter(|(id, _)| *id != new).cloned().collect::<Vec<_>>();
        assert_eq!(untouched, before);
        assert_eq!(after.iter().find(|(id, _)| *id == new).unwrap().1, 0);
    }

    #[test]
    fn switching_documents_writes_view_state_later() {
        let lib = Lib::new("view-state");
        let mut s = lib.open();
        let id = s.create_from(&doc_with("one", "alpha")).unwrap();
        s.index.get_mut(&id).unwrap().opened = 1;
        s.save_index().unwrap();
        let opened_on_disk = || lib.open().list().into_iter().find(|e| e.id == id).unwrap().meta.opened;
        // #324: 最近打开 and 阅读位置 stay in memory while documents are switched …
        s.touch(&id);
        s.remember_scroll(&id, Some((3, 12.)));
        assert_eq!(opened_on_disk(), 1);
        assert_eq!(lib.open().scroll_top(&id), None);
        assert_eq!(s.scroll_top(&id), Some((3, 12.)));
        // … survive the index being re-read for another write …
        assert!(s.list().iter().find(|e| e.id == id).unwrap().meta.opened > 1);
        // … and reach the disk when flushed, or when the store goes away.
        s.flush_view_state().unwrap();
        assert!(opened_on_disk() > 1);
        assert_eq!(lib.open().scroll_top(&id), Some((3, 12.)));
        s.remember_scroll(&id, None);
        drop(s);
        assert_eq!(lib.open().scroll_top(&id), None);
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
        assert!(lib.0.join(".quiet").join("history").join(&id).fs_is_dir());
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
        assert!(lib.0.join(".quiet").join("history-trash").join(&id).fs_is_dir());
        s.restore(&id).unwrap();
        assert!(!s.versions(&id).is_empty());
        s.delete(&id, None).unwrap();
        s.purge(&id).unwrap();
    }

    /// #237: the older layout (everything at the root) is read the same way,
    /// and taking a file away leaves it in the library rather than destroying it.
    #[test]
    fn unused_assets_sweep_reads_the_old_layout() {
        let d = std::env::temp_dir().join(format!("fdl-test-unused-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join("docs")).unwrap();
        std::fs::create_dir_all(d.join("trash")).unwrap();
        std::fs::create_dir_all(d.join("assets")).unwrap();
        std::fs::write(d.join("format.json"), "{}").unwrap();
        std::fs::write(d.join("docs/a.xml"), "<title>A</title><img src=\"assets/used.png\"/>").unwrap();
        std::fs::write(d.join("docs/a.xml.bak"), "<title>A</title><img src=\"assets/bak-only.png\"/>").unwrap();
        std::fs::write(d.join("trash/b.xml"), "trash-only.png").unwrap();
        std::fs::write(d.join("assets/used.png"), b"12345").unwrap();
        std::fs::write(d.join("assets/bak-only.png"), b"12").unwrap();
        std::fs::write(d.join("assets/trash-only.png"), b"123").unwrap();
        std::fs::write(d.join("assets/lonely.png"), b"1234567").unwrap();
        std::fs::write(d.join("assets/.hidden.png"), b"1").unwrap();
        let unused = Store::unused_assets_at(d.clone(), vec![], vec![]).unwrap();
        assert_eq!(unused, vec![("lonely.png".to_string(), 7)]);
        assert_eq!(Store::remove_unused_assets(&d, &unused.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>()), 1);
        assert!(!d.join("assets/lonely.png").exists());
        assert!(removed(&d.join("removed-assets")).join("lonely.png").exists());
        assert!(d.join("assets/used.png").exists() && d.join("assets/trash-only.png").exists() && d.join("assets/bak-only.png").exists());
        // Path tricks and vanished files are skipped, not taken.
        assert_eq!(Store::remove_unused_assets(&d, &["../escape.png".into(), "no-such.png".into(), ".hidden.png".into()]), 0);
        let _ = std::fs::remove_dir_all(&d);
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
    fn a_board_written_outside_is_taken_in() {
        let lib = Lib::new("board-outside");
        let mut s = lib.open();
        let b = crate::whiteboard::Board::from_file(include_str!("../tests/fixtures/board/all-elements.excalidraw")).unwrap();
        let id = s.create_board_in("草图", &b, None).unwrap();
        let file = lib.0.join("草图.excalidraw");
        let d = s.load_for_edit(&id, false).unwrap();
        // Its own save is not an outside change.
        s.save(&id, &d).unwrap();
        assert!(s.refresh_external(&id).unwrap().is_none());
        // An agent adds a shape: the open editor takes the new board in.
        let mut more = crate::whiteboard::board_of(&d).unwrap().clone();
        let mut shape = more.elements[0].clone();
        shape.id = "from-agent".into();
        more.elements.push(shape);
        let written = more.file_json();
        // Caught half written, or emptied before being written again: an error, the baseline kept.
        crate::vfs::write(&file, &written[..written.len() / 2]).unwrap();
        assert!(s.refresh_external(&id).is_err());
        crate::vfs::write(&file, "").unwrap();
        assert!(s.refresh_external(&id).is_err());
        crate::vfs::write(&file, &written).unwrap();
        let got = s.refresh_external(&id).unwrap().expect("taken in");
        assert!(crate::whiteboard::board_of(&got).unwrap().elements.iter().any(|e| e.id == "from-agent"));
        assert!(s.refresh_external(&id).unwrap().is_none(), "taken in once");
        // Edited here meanwhile and changed outside again: the save refuses to write over it, as a document's does.
        crate::vfs::write(&file, b.file_json()).unwrap();
        let mut mine = got.clone();
        let crate::model::BlockKind::Board(bd) = &mut mine.blocks[0].kind else { panic!("not a board") };
        std::sync::Arc::make_mut(bd).elements.clear();
        assert!(s.save_editor(&id, &mine).is_err());
        assert_eq!(crate::vfs::read_to_string(&file).unwrap(), b.file_json(), "the outside version is left as it was");
    }

    #[test]
    fn a_board_is_a_file_of_its_own() {
        let lib = Lib::new("board");
        let mut s = lib.open();
        let folder = s.create_folder("草图", None).unwrap();
        let b = crate::whiteboard::Board::from_file(include_str!("../tests/fixtures/board/all-elements.excalidraw")).unwrap();
        let id = s.create_board_in("流程", &b, Some(&folder)).unwrap();
        let file = lib.0.join("草图/流程.excalidraw");
        let text = crate::vfs::read_to_string(&file).unwrap();
        assert!(text.starts_with("{\n  \"type\": \"excalidraw\""));
        let e = s.list().into_iter().find(|e| e.id == id).unwrap();
        assert_eq!((e.meta.kind, e.title.as_str(), e.meta.parent.as_deref()), (DocKind::Board, "流程", Some(folder.as_str())));
        // Editing saves the 画板 only; the title is the file's name.
        let mut d = s.load_for_edit(&id, false).unwrap();
        assert_eq!(d.title.text, "流程");
        let crate::model::BlockKind::Board(bd) = &mut d.blocks[0].kind else { panic!("not a board") };
        std::sync::Arc::make_mut(bd).elements[0].opacity = 50.;
        d.title = crate::model::RichText::plain("流程图");
        s.save(&id, &d).unwrap();
        let file = lib.0.join("草图/流程图.excalidraw");
        assert!(file.fs_is_file() && !lib.0.join("草图/流程.excalidraw").fs_exists());
        assert!(!crate::vfs::read_to_string(&file).unwrap().contains("流程图"));
        // Reopened from disk, it reads back the same.
        let mut s = lib.open();
        let e = s.list().into_iter().find(|e| e.id == id).unwrap();
        assert_eq!((e.meta.kind, e.title.as_str()), (DocKind::Board, "流程图"));
        let again = s.load(&id).unwrap();
        let kinds = |d: &Document| d.blocks.iter().map(|b| b.kind.clone()).collect::<Vec<_>>();
        assert_eq!(kinds(&again), kinds(&d));
        assert_eq!(crate::whiteboard::write_file(&again), crate::vfs::read_to_string(&file).unwrap());
        // Through the 回收站 and back as a 画板.
        s.delete(&id, None).unwrap();
        assert!(lib.0.join(".quiet/trash").join(format!("{id}.excalidraw")).fs_is_file());
        assert_eq!(s.list_trash()[0].title, "流程图");
        s.restore(&id).unwrap();
        assert!(file.fs_is_file());
        assert_eq!(kinds(&s.load(&id).unwrap()), kinds(&d));
    }

    #[test]
    fn index_keeps_entries_of_unknown_kinds() {
        // A newer build's file kind: this build neither lists it nor drops its entry.
        let lib = Lib::new("foreign");
        let mut s = lib.open();
        let (id, doc) = s.create().unwrap();
        let folder = s.create_folder("草图", None).unwrap();
        crate::vfs::write(lib.0.join("草图/新东西.future"), "{}").unwrap();
        // Written by another device running the newer build.
        let newer = Device { id: "newer".into(), ..Default::default() };
        meta::commit(&lib.0, &newer, &Default::default(), vec![("keep-me".into(), "path".into(), "草图/新东西.future".into()), ("keep-me".into(), "pinned".into(), true.into())], &mut vec![]).unwrap();
        let entry = |key: &str| lib.index().get(key).map(|a| (a.id.clone(), a.pinned));
        assert!(s.list().iter().all(|e| e.id != "keep-me"));
        s.save(&id, &doc).unwrap();
        assert_eq!(entry("草图/新东西.future"), Some(("keep-me".into(), true)));
        // Renaming its folder takes the entry along.
        s.rename(&folder, "图").unwrap();
        assert_eq!((entry("草图/新东西.future"), entry("图/新东西.future")), (None, Some(("keep-me".into(), true))));
    }

    #[test]
    fn an_open_board_survives_a_build_that_drops_it() {
        // An older build rewrites the index without the 画板; the open editor still saves.
        let lib = Lib::new("dropped");
        // Older builds cannot open FORMAT 5 at all.
        STAY_4.with(|k| k.set(true));
        let mut s = lib.open();
        let b = crate::whiteboard::Board::from_file(include_str!("../tests/fixtures/board/all-elements.excalidraw")).unwrap();
        let id = s.create_board_in("流程", &b, None).unwrap();
        let mut d = s.load_for_edit(&id, false).unwrap();
        let index = lib.0.join(".quiet/index.json");
        let rewrite = |f: &dyn Fn(&mut DiskIndex)| {
            let mut disk: DiskIndex = serde_json::from_slice(&crate::vfs::read(&index).unwrap()).unwrap();
            f(&mut disk);
            crate::vfs::write(&index, serde_json::to_vec(&disk).unwrap()).unwrap();
        };
        let edit = |d: &mut Document, o: f64| { let crate::model::BlockKind::Board(bd) = &mut d.blocks[0].kind else { panic!("not a board") }; std::sync::Arc::make_mut(bd).elements[0].opacity = o; };
        rewrite(&|disk| { disk.remove("流程.excalidraw"); });
        edit(&mut d, 50.);
        s.save(&id, &d).unwrap();
        // Or found again by a later `list`, under a new id.
        rewrite(&|disk| { disk.get_mut("流程.excalidraw").unwrap().id = "found-again".into(); });
        edit(&mut d, 40.);
        s.save(&id, &d).unwrap();
        let ids: Vec<String> = s.list().into_iter().filter(|e| e.meta.kind == DocKind::Board).map(|e| e.id).collect();
        assert_eq!(ids, vec![id.clone()]);
        assert_eq!(crate::whiteboard::write_file(&s.load(&id).unwrap()), crate::vfs::read_to_string(lib.0.join("流程.excalidraw")).unwrap());
    }

    #[test]
    fn markdown_documents_stay_markdown() {
        let lib = Lib::new("markdown");
        let mut s = lib.open();
        let mut d = crate::markdown::parse_file("Hello *there*\n", "Note", None);
        let id = s.create_markdown(&d, "---\na: 1\n---\n").unwrap();
        let file = lib.0.join("Note.md");
        assert_eq!(crate::vfs::read_to_string(&file).unwrap(), "---\na: 1\n---\nHello *there*\n");
        let e = s.list().into_iter().find(|e| e.id == id).unwrap();
        assert!(e.meta.markdown);
        assert_eq!(e.title, "Note");
        // Saving rewrites the body only; the front matter stays.
        let mut loaded = s.load_for_edit(&id, false).unwrap();
        loaded.blocks.push(crate::model::Block::text("more"));
        s.save(&id, &loaded).unwrap();
        assert_eq!(crate::vfs::read_to_string(&file).unwrap(), "---\na: 1\n---\nHello *there*\n\nmore\n");
        // A title change is the file's name, not its contents.
        d = s.load(&id).unwrap();
        d.title = crate::model::RichText::plain("Renamed");
        s.save(&id, &d).unwrap();
        assert_eq!(s.load(&id).unwrap().title.text, "Renamed");
        let file = lib.0.join("Renamed.md");
        assert!(!lib.0.join("Note.md").fs_exists());
        assert!(!crate::vfs::read_to_string(&file).unwrap().contains("Renamed"));
        // Through the 回收站 and back as Markdown.
        s.delete(&id, None).unwrap();
        assert!(lib.0.join(".quiet/trash").join(format!("{id}.md")).fs_is_file());
        assert_eq!(s.list_trash()[0].title, "Renamed");
        s.restore(&id).unwrap();
        assert!(file.fs_is_file());
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

    /// Obsidian's `![[picture.png]]` in a Markdown document — one in the
    /// library or a 本机文件 — shows as the picture, and saving writes it as it was.
    #[test]
    fn wiki_embeds_show_as_pictures_and_stay_written() {
        let lib = Lib::new("wiki-embeds");
        let mut s = lib.open();
        let outside = crate::vfs::temp_dir().join(format!("fdl-vault-{}", uuid::Uuid::new_v4()));
        let md = "# Embeds\n\n![[red.png]]\n\n![[red.png|300]]\n\n![[pics/green.png]]\n\n![[blue.png]]\n\n![[Other]]\n\nend\n";
        for v in [lib.0.join("vault"), outside.clone()] {
            for d in [".obsidian", "attachments", "pics", "notes/deep"] { crate::vfs::create_dir_all(v.join(d)).unwrap(); }
            crate::vfs::write(v.join(".obsidian/app.json"), r#"{"attachmentFolderPath":"attachments"}"#).unwrap();
            for f in ["attachments/red.png", "pics/green.png", "notes/deep/blue.png"] { crate::vfs::write(v.join(f), b"png").unwrap(); }
            crate::vfs::write(v.join("notes/Embeds.md"), md).unwrap();
        }
        let in_lib = s.list().into_iter().find(|e| e.meta.rel == "vault/notes/Embeds.md").unwrap().id;
        let local = s.open_local(&outside.join("notes/Embeds.md")).unwrap();
        let real = |p: &Path| p.fs_canonicalize().unwrap();
        for (id, v) in [(in_lib, lib.0.join("vault")), (local.clone(), outside.clone())] {
            let mut doc = s.load_for_edit(&id, false).unwrap();
            let pics: Vec<(PathBuf, Option<f32>)> = doc.blocks.iter().filter_map(|b| match &b.kind {
                crate::model::BlockKind::Image { src, width, .. } => Some((real(Path::new(src)), *width)),
                _ => None,
            }).collect();
            let (red, green, blue) = (real(&v.join("attachments/red.png")), real(&v.join("pics/green.png")), real(&v.join("notes/deep/blue.png")));
            assert_eq!(pics, vec![(red.clone(), None), (red, Some(300.)), (green, None), (blue, None)]);
            // `![[Other]]` embeds a note, not a picture: it stays text.
            assert!(doc.blocks.iter().any(|b| b.text.text == "![[Other]]"));
            let file = v.join("notes/Embeds.md");
            s.save(&id, &doc).unwrap();
            assert_eq!(crate::vfs::read_to_string(&file).unwrap(), md);
            *doc.blocks.last_mut().unwrap() = crate::model::Block::text("the end");
            s.save(&id, &doc).unwrap();
            assert_eq!(crate::vfs::read_to_string(&file).unwrap(), md.replace("\nend\n", "\nthe end\n"));
            // A changed picture is written relative to the file's folder, with `/`; a library
            // picture too (beside it for a file outside the library). Never an absolute path.
            if let crate::model::BlockKind::Image { width, .. } = &mut doc.blocks[2].kind { *width = Some(120.); }
            let asset = Store::assets_in(s.top_dir()).join("pasted.png");
            crate::vfs::create_dir_all(asset.parent().unwrap()).unwrap();
            crate::vfs::write(&asset, b"png").unwrap();
            doc.blocks.push(crate::model::Block::new(crate::model::BlockKind::Image { src_kind: "src".into(), src: asset.to_string_lossy().into_owned(), width: None, height: None, caption: None, crop: None }, Default::default()));
            s.save(&id, &doc).unwrap();
            let pasted = if id == local { "Embeds.assets/Embeds-1.png" } else { "../../.assets/pasted.png" };
            let want = md.replace("\nend\n", "\nthe end\n").replace("![[red.png|300]]", "<img src=\"../attachments/red.png\" width=\"120\">") + &format!("\n![]({pasted})\n");
            assert_eq!(crate::vfs::read_to_string(&file).unwrap(), want);
            // Read back, the same pictures.
            let back = s.load_for_edit(&id, false).unwrap();
            let srcs: Vec<PathBuf> = back.blocks.iter().filter_map(|b| match &b.kind { crate::model::BlockKind::Image { src, .. } => Some(real(Path::new(src))), _ => None }).collect();
            assert_eq!(srcs.len(), 5);
            assert_eq!(srcs[1], real(&v.join("attachments/red.png")));
            if id != local { assert_eq!(srcs[4], real(&asset)); }
        }
        let _ = crate::vfs::remove_dir_all(&outside);
    }

    #[test]
    fn a_hand_written_markdown_file_joins_the_library() {
        let lib = Lib::new("hand-written");
        let mut s = lib.open();
        let (id, doc) = s.create().unwrap();
        let before = Store::shape_at(&lib.0);
        s.save(&id, &doc).unwrap();
        assert_eq!(before, Store::shape_at(&lib.0), "saving is not a change of shape");
        crate::vfs::write(lib.0.join("model.md"), "# Written by a model\n").unwrap();
        assert_ne!(before, Store::shape_at(&lib.0), "a new file is");
        let e = s.list().into_iter().find(|e| e.meta.rel == "model.md").unwrap();
        assert!(e.meta.markdown);
        assert!(e.body.contains("Written by a model"), "{:?}", e.body);
    }

    #[test]
    fn markdown_editor_uses_the_last_saved_body() {
        let lib = Lib::new("markdown-external-save");
        let mut s = lib.open();
        let file = lib.0.join("note.md");
        crate::vfs::write(&file, "original\n").unwrap();
        let id = s.open_local(&file).unwrap();
        let mut editor = s.load_for_edit(&id, false).unwrap();
        editor.blocks = vec![crate::model::Block::text("from the editor")];
        crate::vfs::write(&file, "from another app\n").unwrap();
        s.save_editor(&id, &editor).unwrap();
        assert_eq!(crate::vfs::read_to_string(&file).unwrap(), "from the editor\n");
        assert!(s.refresh_external(&id).unwrap().is_none());
        crate::vfs::write(&file, "saved after the editor\n").unwrap();
        let refreshed = s.refresh_external(&id).unwrap().unwrap();
        assert_eq!(refreshed.blocks[0].text.text, "saved after the editor");
        assert!(s.refresh_external(&id).unwrap().is_none());
    }

    /// Blank lines and spaces typed in a page come back as typed after a
    /// save, the check made when the window comes forward (nothing to take in:
    /// the file is what this editor wrote) and reading the file again.
    #[test]
    fn blank_lines_and_spaces_survive_a_save_and_a_read() {
        let lib = Lib::new("blank-lines");
        let mut s = lib.open();
        let texts = ["first", "", "", "   ", "   leading", "middle   and end   ", "\u{a0}", "two<br>lines\n\nand a gap", "last"];
        let typed = |d: &Document| d.blocks.iter().map(|b| b.text.text.clone()).collect::<Vec<_>>();
        let md = s.create_markdown(&crate::markdown::parse_file("first\n", "Note", None), "").unwrap();
        let xml = s.create_from(&Document { title: crate::model::RichText::plain("Page"), blocks: vec![crate::model::Block::text("first")], ..Default::default() }).unwrap();
        for id in [md, xml] {
            let mut doc = s.load_for_edit(&id, false).unwrap();
            doc.blocks = texts.iter().map(|t| crate::model::Block::text(t)).collect();
            s.save_editor(&id, &doc).unwrap();
            assert!(s.refresh_external(&id).unwrap().is_none(), "its own save is not another program's");
            let back = s.load_for_edit(&id, false).unwrap();
            assert_eq!(typed(&back), texts, "{}", s.read_doc_text(&id).unwrap());
            // Saving what was read writes nothing new.
            let disk = s.read_doc_text(&id).unwrap();
            s.save_editor(&id, &back).unwrap();
            assert_eq!(s.read_doc_text(&id).unwrap(), disk);
            assert!(s.refresh_external(&id).unwrap().is_none());
        }
    }

    #[test]
    fn a_clean_editor_takes_in_another_programs_save() {
        let lib = Lib::new("xml-external-save");
        let mut s = lib.open();
        let id = s.create_from(&Document { title: crate::model::RichText::plain("Plan"), blocks: vec![crate::model::Block::text("first")], ..Default::default() }).unwrap();
        s.load_for_edit(&id, false).unwrap();
        assert!(s.refresh_external(&id).unwrap().is_none());
        // `quiet` (another Store on the same folder) saves the document.
        let mut cli = Store::open_for_tools(lib.0.clone()).unwrap();
        cli.list();
        let mut doc = cli.load(&id).unwrap();
        doc.blocks.push(crate::model::Block::text("from the CLI"));
        cli.save(&id, &doc).unwrap();
        let fresh = s.refresh_external(&id).unwrap().unwrap();
        assert_eq!(fresh.blocks.last().unwrap().text.text, "from the CLI");
        assert!(s.refresh_external(&id).unwrap().is_none());
        // Saving the taken-in version is not a conflict.
        s.save(&id, &fresh).unwrap();
    }

    /// The file's creation time where this system keeps one, in unix seconds.
    fn made_of(p: &Path) -> Option<u64> { std::fs::metadata(p).ok()?.created().ok()?.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs()) }

    #[test]
    fn files_found_in_place_keep_their_times() {
        let lib = Lib::new("found-times");
        let mut s = lib.open();
        // Put in the folder by hand (the Finder, Obsidian): last changed long ago, or later than made.
        let (then, later) = (1_600_000_000, now() + 86_400);
        crate::vfs::create_dir_all(lib.0.join("notes")).unwrap();
        for (rel, t) in [("old.md", then), ("notes/later.md", later)] {
            crate::vfs::write(lib.0.join(rel), "body\n").unwrap();
            crate::vfs::set_modified(&lib.0.join(rel), t).unwrap();
        }
        let list = s.list();
        let meta = |title: &str| list.iter().find(|e| e.title == title).unwrap().meta.clone();
        // Made after its last change (as a copy is): the change time is both.
        assert_eq!((meta("old").created, meta("old").modified), (then, then));
        // Made before: when it was made, where the system keeps that (Linux may not).
        let made = made_of(&lib.0.join("notes/later.md"));
        assert_eq!((meta("later").created, meta("later").modified), (made.unwrap_or(later), later));
        if cfg!(any(target_os = "macos", windows)) { assert!(meta("later").created < later); }
        // A folder keeps its change time, as before.
        let folder = list.iter().find(|e| e.meta.folder.is_some()).unwrap();
        assert_eq!(folder.meta.created, folder.meta.modified);
        // Kept: the next look finds the same.
        assert_eq!(lib.open().list().iter().find(|e| e.title == "old").unwrap().meta.created, then);
    }

    #[test]
    fn a_local_file_opened_in_place_has_its_own_times() {
        let lib = Lib::new("local-times");
        let away = crate::vfs::temp_dir().join(format!("fdl-local-times-{}", uuid::Uuid::new_v4()));
        crate::vfs::create_dir_all(&away).unwrap();
        let (file, later) = (away.join("later.md"), now() + 86_400);
        crate::vfs::write(&file, "body\n").unwrap();
        crate::vfs::set_modified(&file, later).unwrap();
        let old = away.join("old.md");
        crate::vfs::write(&old, "body\n").unwrap();
        crate::vfs::set_modified(&old, 1_600_000_000).unwrap();
        let mut s = lib.open();
        let before = now();
        let id = s.open_local(&file).unwrap();
        let m = s.list().into_iter().find(|e| e.id == id).unwrap().meta;
        assert_eq!((m.created, m.modified), (made_of(&file).unwrap_or(later), later));
        assert!(m.opened >= before, "opened is when it was opened");
        let id = s.open_local(&old).unwrap();
        let m = s.list().into_iter().find(|e| e.id == id).unwrap().meta;
        assert_eq!((m.created, m.modified), (1_600_000_000, 1_600_000_000));
        // Opened again: only `opened` moves.
        s.open_local(&old).unwrap();
        assert_eq!(s.list().into_iter().find(|e| e.id == id).unwrap().meta.created, 1_600_000_000);
        crate::vfs::remove_dir_all(away).unwrap();
    }

    /// A Markdown attachment (`.assets/files/<uuid>/<name>`) opened in its own
    /// window is a 本机文件, though it is inside the library: edited where it is,
    /// one id however often it is opened, and never a page of the tree.
    #[test]
    fn an_attachment_opens_in_place() {
        let lib = Lib::new("attachment-in-place");
        let mut s = lib.open();
        let page = s.create().unwrap().0;
        let dir = Store::assets_in(&lib.0).join("files").join("0f0e");
        crate::vfs::create_dir_all(&dir).unwrap();
        let file = dir.join("meeting-notes.md");
        crate::vfs::write(&file, "# Notes\n\nfirst\n").unwrap();
        let id = s.open_local(&file).unwrap();
        assert_eq!(s.open_local(&file).unwrap(), id);
        assert_eq!(s.file_path(&id).unwrap().fs_canonicalize().unwrap(), file.fs_canonicalize().unwrap());
        let e = s.list().into_iter().find(|e| e.id == id).unwrap();
        assert!(e.meta.local() && e.meta.markdown && e.meta.parent.is_none());
        assert_eq!(e.title, "meeting-notes");
        let mut d = s.load_for_edit(&id, false).unwrap();
        d.blocks.push(crate::model::Block::text("added"));
        s.save(&id, &d).unwrap();
        assert_eq!(crate::vfs::read_to_string(&file).unwrap(), "# Notes\n\nfirst\n\nadded\n");
        // An attachment, kept out of 本机文件; a title typed in its window changes no name: the
        // page's card still finds the file where it was.
        assert!(e.meta.attachment());
        s.follow_name(&id, "新的标题").unwrap();
        assert!(file.fs_is_file() && !dir.join("新的标题.md").exists());
        assert_eq!(s.file_path(&id).unwrap().fs_canonicalize().unwrap(), file.fs_canonicalize().unwrap());
        // A Markdown file opened from elsewhere is no attachment, and follows its title as before.
        let outside = lib.0.parent().unwrap().join(format!("{}-outside.md", base_name(&lib.0.to_string_lossy())));
        crate::vfs::write(&outside, "x\n").unwrap();
        let o = s.open_local(&outside).unwrap();
        assert!(!s.index[&o].attachment());
        crate::vfs::remove_file(&outside).ok();
        // Reopened: still that one file, beside the page it is attached to.
        drop(s);
        let mut s = lib.open();
        assert_eq!(s.open_local(&file).unwrap(), id);
        let ids: Vec<String> = s.list().into_iter().map(|e| e.id).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&page) && ids.contains(&id));
        crate::vfs::remove_dir_all(&lib.0).unwrap();
    }

    #[test]
    fn local_files_are_edited_in_place_and_never_trashed() {
        let lib = Lib::new("local");
        let dir = lib.0.join("outside");
        crate::vfs::create_dir_all(&dir).unwrap();
        let file = dir.join("todo.md");
        crate::vfs::write(&file, "* star bullet\n\n![x](pics/x.png)\n").unwrap();
        let mut s = lib.open();
        let id = s.open_local(&file).unwrap();
        assert_eq!(s.open_local(&file).unwrap(), id);
        let e = s.list().into_iter().find(|e| e.id == id).unwrap();
        assert!(e.meta.local());
        assert_eq!(e.title, "todo");
        let mut d = s.load_for_edit(&id, false).unwrap();
        // Saving untouched content leaves the user's formatting alone.
        s.save(&id, &d).unwrap();
        assert_eq!(crate::vfs::read_to_string(&file).unwrap(), "* star bullet\n\n![x](pics/x.png)\n");
        d.blocks.push(crate::model::Block::text("added"));
        s.save(&id, &d).unwrap();
        assert_eq!(crate::vfs::read_to_string(&file).unwrap(), "* star bullet\n\n![x](pics/x.png)\n\nadded\n");
        // Changed elsewhere: not overwritten.
        crate::vfs::write(&file, "edited elsewhere\n").unwrap();
        d.blocks.push(crate::model::Block::text("again"));
        assert!(s.save(&id, &d).is_err());
        assert_eq!(crate::vfs::read_to_string(&file).unwrap(), "edited elsewhere\n");
        // Renaming renames the file beside it.
        s.rename(&id, "done").unwrap();
        let renamed = dir.fs_canonicalize().unwrap().join("done.md");
        assert!(renamed.fs_is_file() && !file.fs_exists());
        // Removing it from the list keeps the file.
        s.delete(&id, None).unwrap();
        assert!(renamed.fs_is_file());
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
        crate::vfs::create_dir_all(&dir).unwrap();
        let file = dir.join("Plan.md");
        crate::vfs::write(&file, "text\n").unwrap();
        let mut s = lib.open();
        crate::vfs::create_dir_all(lib.0.join(ASSETS)).unwrap();
        let pasted = lib.0.join(".assets/3e5c9916-7c1f-4f7d-af25-74dd4a8a241e.png");
        let picked = lib.0.join(".assets/1a2b3c4d-截图说明.png");
        crate::vfs::write(&pasted, b"p").unwrap();
        crate::vfs::write(&picked, b"q").unwrap();
        // Something already there keeps its name.
        crate::vfs::create_dir_all(dir.join("Plan.assets")).unwrap();
        crate::vfs::write(dir.join("Plan.assets/Plan-1.png"), b"old").unwrap();
        let id = s.open_local(&file).unwrap();
        let mut d = s.load_for_edit(&id, false).unwrap();
        let image = |src: &std::path::Path| crate::model::Block::new(crate::model::BlockKind::Image { src_kind: "src".into(), src: src.to_string_lossy().into(), width: None, height: None, caption: None, crop: None }, Default::default());
        d.blocks.push(image(&pasted));
        d.blocks.push(image(&picked));
        s.save(&id, &d).unwrap();
        let text = crate::vfs::read_to_string(dir.fs_canonicalize().unwrap().join("Plan.md")).unwrap();
        assert_eq!(text, "text\n\n![](Plan.assets/Plan-2.png)\n\n![](Plan.assets/截图说明.png)\n");
        assert_eq!(crate::vfs::read(dir.join("Plan.assets/Plan-2.png")).unwrap(), b"p");
        assert_eq!(crate::vfs::read(dir.join("Plan.assets/Plan-1.png")).unwrap(), b"old");
        // The next autosave (the editor still points at the library copy) makes no new copy.
        d.blocks.push(crate::model::Block::text("more"));
        s.save(&id, &d).unwrap();
        assert_eq!(crate::vfs::read_dir(dir.join("Plan.assets")).unwrap().count(), 3);
        assert!(crate::vfs::read_to_string(dir.join("Plan.md")).unwrap().contains("Plan.assets/Plan-2.png"));
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
        crate::vfs::remove_file(s.lock_path()).unwrap();
        crate::vfs::create_dir(s.lock_path()).unwrap();
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
        crate::vfs::remove_dir_all(recovery).unwrap();
    }

    #[test]
    fn atomic_write_cleans_failed_temporary_files() {
        let lib = Lib::new("atomic-failure");
        crate::vfs::create_dir_all(lib.0.join("target")).unwrap();
        assert!(write_atomic(&lib.0.join("target"), b"new").is_err());
        assert_eq!(crate::vfs::read_dir(&lib.0).unwrap().count(), 1);
    }

    #[test]
    fn exports_stay_in_library_and_reads_are_bounded() {
        let lib = Lib::new("export");
        let outside = Lib::new("outside");
        crate::vfs::create_dir_all(lib.0.join("assets")).unwrap();
        crate::vfs::create_dir_all(&outside.0).unwrap();
        crate::vfs::write(lib.0.join("assets/good"), b"1234").unwrap();
        crate::vfs::write(outside.0.join("secret"), b"secret").unwrap();
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
        assert!(lib.0.join(".quiet/format.json").fs_is_file());
        // The library holds the note and its own hidden folder, nothing else.
        let names: Vec<String> = crate::vfs::read_dir(&lib.0).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
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
        let path = parsed_cache_path(&lib.0, &library_key(&lib.0));
        let mut c: ParsedCache = serde_json::from_slice(&crate::vfs::read(&path).unwrap()).unwrap();
        c.version = "0.0.0-0".into();
        crate::vfs::write(&path, serde_json::to_vec(&c).unwrap()).unwrap();
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
        crate::vfs::write(s.path(&b), docxml::serialize(&doc("二改"))).unwrap();
        let ap = s.path(&a);
        let stamp = crate::vfs::metadata(&ap).unwrap().modified().unwrap();
        s.parsed.get_mut(&a).unwrap().2.title = "缓存".into();
        assert_eq!(crate::vfs::metadata(&ap).unwrap().modified().unwrap(), stamp);
        let titles = |s: &mut Store| { let mut t: Vec<_> = s.list().into_iter().map(|e| e.title).collect(); t.sort(); t };
        assert_eq!(titles(&mut s), ["二改", "缓存"]);
        // Saving through the store, or deleting the file, is seen too.
        s.save(&a, &doc("一改")).unwrap();
        assert_eq!(titles(&mut s), ["一改", "二改"]);
        crate::vfs::remove_file(s.path(&b)).unwrap();
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
        crate::vfs::write(s.path(&id), docxml::serialize(&doc("external before reopen"))).unwrap();
        let reopened = s.load_for_edit(&id, false).unwrap();
        assert_eq!(reopened.title.text, "external before reopen");
        s.save(&id, &reopened).unwrap();
        let external = docxml::serialize(&doc("external after reopen"));
        crate::vfs::write(s.path(&id), &external).unwrap();
        assert!(s.save(&id, &doc("local edit")).is_err());
        assert_eq!(crate::vfs::read_to_string(s.path(&id)).unwrap(), external);
    }

    fn dev(id: &str) -> Device { Device { id: id.into(), ..Default::default() } }

    #[test]
    fn the_write_lock_is_local_and_shared_by_the_key() {
        let lib = Lib::new("local-lock");
        // The app opens a top-level folder by its key, the CLI and `open_at` by `library_key`: one lock still.
        let mut a = Store::open_as(lib.0.clone(), &uuid::Uuid::new_v4().to_string(), device()).unwrap();
        let key = library_key(&lib.0);
        a.create_from(&doc("甲")).unwrap();
        // FORMAT 5 makes no lock in the top-level folder: it would only travel to other devices.
        assert!(a.lock_path().fs_is_file() && !lib.0.join(".quiet/.lock").fs_exists());
        // A second copy on this computer (another window, the CLI) waits for the first.
        a.begin_batch().unwrap();
        let wrote = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let other = {
            let (root, key, wrote) = (lib.0.clone(), key.clone(), wrote.clone());
            std::thread::spawn(move || {
                let mut b = Store::open_as(root, &key, device()).unwrap();
                b.create_from(&doc("乙")).unwrap();
                wrote.store(true, std::sync::atomic::Ordering::SeqCst);
            })
        };
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(!wrote.load(std::sync::atomic::Ordering::SeqCst));
        a.create_from(&doc("丙")).unwrap();
        a.end_batch().unwrap();
        other.join().unwrap();
        assert!(wrote.load(std::sync::atomic::Ordering::SeqCst));
        let mut titles: Vec<String> = a.list().into_iter().map(|e| e.title).collect();
        titles.sort();
        assert_eq!(titles, ["丙", "乙", "甲"]);
        // Named by the library id, and a new id (a folder copied whole) moves it.
        let was = a.lock_path();
        assert!(was.ends_with(format!("{}.lock", a.library_id())));
        a.renew_library_id().unwrap();
        assert!(a.lock_path() != was && a.lock_path().ends_with(format!("{}.lock", a.library_id())));
        a.create_from(&doc("丁")).unwrap();
        assert!(a.lock_path().fs_is_file());
    }

    #[test]
    fn the_merged_view_is_reread_only_when_a_device_file_changed() {
        let lib = Lib::new("merged-cache");
        let mut a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        let x = a.create_from(&doc("共享")).unwrap();
        let first = meta::cached(&lib.0, "aaaa", &meta::Merged::default(), &mut vec![]);
        // Nothing changed: the same view without reading.
        assert!(meta::cached(&lib.0, "aaaa", &first, &mut vec![]).get(&x, "path").is_some());
        // Another device writes its file: the next write here sees it.
        meta::commit(&lib.0, &dev("bbbb"), &Default::default(), vec![(x.clone(), "pinned".into(), true.into())], &mut vec![]).unwrap();
        assert_eq!(meta::cached(&lib.0, "aaaa", &first, &mut vec![]).get(&x, "pinned"), Some(&serde_json::Value::Bool(true)));
        a.set_starred(&x, true).unwrap();
        let m = a.list().into_iter().find(|e| e.id == x).unwrap().meta;
        assert!(m.pinned && m.starred > 0);
        // Its own write keeps the view current, and a later one by another device still shows.
        meta::commit(&lib.0, &dev("bbbb"), &Default::default(), vec![(x.clone(), "pinned".into(), false.into())], &mut vec![]).unwrap();
        a.set_starred(&x, false).unwrap();
        assert!(!a.list().into_iter().find(|e| e.id == x).unwrap().meta.pinned);
    }

    fn loads() -> usize { META_LOADS.with(|n| n.get()) }
    fn pinned(s: &mut Store, id: &str) -> bool { s.list().into_iter().find(|e| e.id == id).unwrap().meta.pinned }
    fn other_pins(lib: &Lib, by: &str, id: &str, on: bool) {
        meta::commit(&lib.0, &dev(by), &Default::default(), vec![(id.into(), "pinned".into(), on.into())], &mut vec![]).unwrap();
    }

    #[test]
    fn nothing_is_read_again_until_a_device_file_changes() {
        let lib = Lib::new("meta-held");
        let mut a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        let x = a.create_from(&doc("共享")).unwrap();
        a.list();
        let n = loads();
        a.list();
        a.list();
        assert_eq!(loads(), n, "nothing changed: nothing read again");
        // This device's own writes keep memory and the disk together.
        a.set_starred(&x, true).unwrap();
        assert!(a.list().into_iter().find(|e| e.id == x).unwrap().meta.starred > 0);
        assert_eq!(loads(), n, "its own write: nothing read again");
        // Another device writes: the next look reads again, and only that once.
        other_pins(&lib, "bbbb", &x, true);
        assert!(pinned(&mut a, &x));
        assert_eq!(loads(), n + 1);
        a.list();
        assert_eq!(loads(), n + 1);
    }

    /// A code folder lists its notes only (docs/FORMAT.md, "What a folder shows"):
    /// not what is in dependency and build folders or ignored by git, not another
    /// program's `.xml` or `.sqlite`, not folders without notes; and none of that
    /// gets an entry written down.
    #[test]
    fn a_code_folder_lists_its_notes_only() {
        let lib = Lib::new("code-folder");
        let mut s = lib.open();
        let w = |rel: &str, text: &str| { let p = lib.0.join(rel); crate::vfs::create_dir_all(p.parent().unwrap()).unwrap(); crate::vfs::write(p, text).unwrap(); };
        w("app/package.json", "{}");
        w("app/README.md", "# App\n");
        w("app/node_modules/left-pad/README.md", "# left-pad\n");
        w("app/build/out.md", "generated\n");
        w("app/src/deep/main.rs", "fn main() {}\n");
        w("app/docs/guide.md", "# Guide\n");
        w("app/docs/draft.md", "# Draft\n");
        w("app/.gitignore", "docs/draft.md\nscratch/\n");
        w("app/scratch/todo.md", "x\n");
        w("app/pom.xml", "<?xml version=\"1.0\"?>\n<project><modelVersion>4.0.0</modelVersion></project>\n");
        w("app/res/layout/main.xml", "<LinearLayout/>\n");
        w("app/notes.xml", "<title>Notes</title>\n<p>ours</p>\n");
        w("app/dev.sqlite", "SQLite format 3\0 but not a table of ours");
        w("app/.git/info/exclude", "private.md\n");
        w("app/private.md", "x\n");
        // A folder called build with no project file beside it is a folder of notes.
        w("journal/build/day.md", "# Day\n");
        w("journal/target/goal.md", "# Goal\n");
        crate::vfs::create_dir_all(lib.0.join("empty/inner")).unwrap();
        let mine = s.create_folder("我的空文件夹", None).unwrap();
        let list = s.list();
        let mut rels: Vec<String> = list.iter().map(|e| if e.meta.folder.is_some() { format!("{}/", e.meta.rel) } else { e.meta.rel.clone() }).collect();
        rels.sort();
        assert_eq!(rels, ["app/", "app/README.md", "app/docs/", "app/docs/guide.md", "app/notes.xml", "journal/", "journal/build/", "journal/build/day.md", "journal/target/", "journal/target/goal.md", "我的空文件夹/"]);
        assert!(list.iter().any(|e| e.id == mine), "made here: shown though empty");
        let disk = lib.index();
        assert!(disk.keys().all(|k| rels.contains(k)), "nothing else written down: {:?}", disk.keys().filter(|k| !rels.contains(k)).collect::<Vec<_>>());
        // Looked inside once; a file rewritten is looked at again.
        w("app/res/layout/main.xml", "<title>Layout notes</title>\n");
        assert!(s.list().iter().any(|e| e.meta.rel == "app/res/layout/main.xml"));
    }

    /// An entry an older build wrote down that the rules now hide (a folder that
    /// became build output, a file git now ignores) stays on disk as it was and is
    /// only not listed: nothing is read again for it, and it comes back with its id.
    #[test]
    fn what_is_hidden_now_keeps_its_entry() {
        let lib = Lib::new("hidden-kept");
        let mut s = lib.open();
        let w = |rel: &str, text: &str| { let p = lib.0.join(rel); crate::vfs::create_dir_all(p.parent().unwrap()).unwrap(); crate::vfs::write(p, text).unwrap(); };
        w("proj/README.md", "# Proj\n");
        w("proj/build/notes.md", "# Notes\n");
        let id = |s: &mut Store| s.list().into_iter().find(|e| e.meta.rel == "proj/build/notes.md").map(|e| e.id);
        let was = id(&mut s).expect("a folder called build of notes");
        s.set_pinned(&was, true).unwrap();
        w("proj/package.json", "{}");
        assert_eq!(id(&mut s), None, "beside package.json, build is output");
        let n = loads();
        s.list();
        s.list();
        assert_eq!(loads(), n, "hidden, not gone: nothing read again");
        assert!(lib.index().contains_key("proj/build/notes.md"));
        crate::vfs::remove_file(lib.0.join("proj/package.json")).unwrap();
        assert_eq!(id(&mut s).as_deref(), Some(was.as_str()));
        assert!(s.list().into_iter().any(|e| e.id == was && e.meta.pinned), "its settings kept");
    }

    #[test]
    fn a_file_not_here_yet_keeps_nothing_read_again_until_it_arrives() {
        // Another device's entry for a file still on its way (or gone there).
        let lib = Lib::new("meta-absent");
        let mut a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        let x = a.create_from(&doc("甲")).unwrap();
        meta::commit(&lib.0, &dev("bbbb"), &Default::default(), vec![("far".into(), "path".into(), "远方.md".into()), ("far".into(), "pinned".into(), true.into())], &mut vec![]).unwrap();
        assert!(a.list().iter().all(|e| e.id != "far"), "not here: not listed");
        let n = loads();
        a.list();
        a.list();
        assert_eq!(loads(), n, "only its file is missing: nothing read again");
        // This device's writes keep it out of memory and leave its entry on disk.
        a.set_starred(&x, true).unwrap();
        a.list();
        assert_eq!(loads(), n);
        assert_eq!(meta::load(&lib.0, "aaaa", &mut vec![]).get("far", "pinned"), Some(&serde_json::Value::Bool(true)));
        // The file arrives: read again once, and it keeps what its device set.
        crate::vfs::write(lib.0.join("远方.md"), "# 远方\n").unwrap();
        let far = a.list().into_iter().find(|e| e.id == "far").expect("arrived: listed by its device's id");
        assert!(far.meta.pinned);
        assert_eq!(loads(), n + 1);
        a.list();
        assert_eq!(loads(), n + 1);
    }

    #[test]
    fn another_devices_write_reaches_list_and_saving() {
        let lib = Lib::new("meta-other");
        let mut a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        let x = a.create_from(&doc("甲")).unwrap();
        a.list();
        other_pins(&lib, "bbbb", &x, true);
        assert!(pinned(&mut a, &x), "list sees it");
        // Written there while this device saves without listing first: the save
        // takes it in and does not put the old value back.
        other_pins(&lib, "bbbb", &x, false);
        a.set_starred(&x, true).unwrap();
        let merged = meta::load(&lib.0, "aaaa", &mut vec![]);
        assert_eq!(merged.get(&x, "pinned"), Some(&serde_json::Value::Bool(false)));
        assert!(merged.get(&x, "starred").is_some());
        let m = a.list().into_iter().find(|e| e.id == x).unwrap().meta;
        assert!(!m.pinned && m.starred > 0);
        // And a change by this device afterwards still goes on top of it.
        a.set_pinned(&x, true).unwrap();
        assert_eq!(meta::load(&lib.0, "aaaa", &mut vec![]).get(&x, "pinned"), Some(&serde_json::Value::Bool(true)));
    }

    #[test]
    fn a_device_file_removed_or_added_is_noticed() {
        let lib = Lib::new("meta-files");
        let mut a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        let x = a.create_from(&doc("甲")).unwrap();
        a.list();
        other_pins(&lib, "bbbb", &x, true);
        assert!(pinned(&mut a, &x));
        crate::vfs::remove_dir_all(meta::dir(&lib.0).join("bbbb")).unwrap();
        assert!(!pinned(&mut a, &x), "its file gone, so is what it said");
        other_pins(&lib, "cccc", &x, true);
        assert!(pinned(&mut a, &x), "a new device's file");
    }

    #[test]
    fn a_rewrite_with_the_same_size_and_time_is_noticed() {
        // A sync (iCloud, a git checkout, rsync -t) may put a file back with the
        // time it had where it was written, and a change may keep its size; a
        // write in the same tick of a coarse clock (Linux's) keeps the change
        // time too: the file was seen within that tick, so its contents decide.
        let lib = Lib::new("meta-same-time");
        let mut a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        let x = a.create_from(&doc("甲")).unwrap();
        other_pins(&lib, "bbbb", &x, true);
        assert!(pinned(&mut a, &x));
        meta::seen_as_changed(&lib.0, a.merged.get_mut());
        let path = lib.dev_file("bbbb", &x);
        let was = crate::vfs::metadata(&path).unwrap();
        let text = crate::vfs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("true").count(), 1);
        crate::vfs::write(&path, text.replace("true", "null")).unwrap();
        crate::vfs::File::options().write(true).open(&path).unwrap().set_modified(was.modified().unwrap()).unwrap();
        let now = crate::vfs::metadata(&path).unwrap();
        assert_eq!((now.len(), now.modified().unwrap()), (was.len(), was.modified().unwrap()));
        meta::change_times_held(&lib.0, a.merged.get_mut());
        assert!(!pinned(&mut a, &x));
        // Read again, and the same holds for the next write.
        meta::seen_as_changed(&lib.0, a.merged.get_mut());
        crate::vfs::write(&path, crate::vfs::read_to_string(&path).unwrap().replace("null", "true")).unwrap();
        meta::change_times_held(&lib.0, a.merged.get_mut());
        assert!(pinned(&mut a, &x));
    }

    #[test]
    fn a_file_is_trusted_on_its_stat_only_a_tick_after_it_changed() {
        let t = web_time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_800_000_000);
        let s = |len, changed| meta::Stat { len, modified: Some(t), written: 7, changed: Some(changed), dataless: false };
        let sec = std::time::Duration::from_secs(1);
        // Changed within the tick before it was seen, or ahead of this clock.
        assert!(meta::racy(&s(1, t), t) && meta::racy(&s(1, t), t + sec) && meta::racy(&s(1, t + sec), t));
        assert!(!meta::racy(&s(1, t), t + 2 * sec) && !meta::racy(&meta::Stat { changed: None, ..s(1, t) }, t));
        assert!(!meta::racy(&meta::Stat { dataless: true, ..s(1, t) }, t));
        // Seen a tick after: the same stat is enough, the contents aren't read.
        let settled = meta::Seen { stat: s(1, t), contents: None };
        assert!(meta::unchanged(&settled, &s(1, t), || unreachable!()));
        assert!(!meta::unchanged(&settled, &s(2, t), || unreachable!()));
        // Seen within it: the same stat, and then the same contents.
        let racy = meta::Seen { stat: s(1, t), contents: Some(42) };
        assert!(meta::unchanged(&racy, &s(1, t), || 42));
        assert!(!meta::unchanged(&racy, &s(1, t), || 43));
        assert!(!meta::unchanged(&racy, &s(1, t + sec), || unreachable!()));
        // Seen for real: the contents are hashed only within the tick.
        let lib = Lib::new("meta-racy");
        crate::vfs::create_dir_all(&lib.0).unwrap();
        let p = lib.0.join("f.json");
        crate::vfs::write(&p, "{}").unwrap();
        let st = meta::stat(&p, Some(&crate::vfs::metadata(&p).unwrap()));
        let Some(changed) = st.changed else { return };
        assert!(meta::seen(&p, st.clone(), changed).contents.is_some());
        assert!(meta::seen(&p, st, changed + 2 * sec).contents.is_none());
    }

    #[test]
    fn devices_are_named_or_labelled() {
        let d = Device { id: "9b2f0c1e".into(), name: String::new(), platform: "macos".into() };
        assert_eq!(d.label(), "macOS 9b2f");
        assert_eq!(Device { name: " Sida 的 MacBook ".into(), ..d.clone() }.label(), "Sida 的 MacBook");
        assert_eq!(Device { platform: String::new(), ..d }.label(), "? 9b2f");
        set_device_name("书房的 Mac");
        assert_eq!(device().name, "书房的 Mac");
        set_device_name("");
    }

    #[test]
    fn retiring_a_device_keeps_the_merged_view() {
        let lib = Lib::new("retire");
        let mut a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        let x = a.create_from(&doc("甲")).unwrap();
        let y = a.create_from(&doc("乙")).unwrap();
        let file = |d: &str| meta::dir(&lib.0).join(format!("{d}.json"));
        // "eeee": wins x's pinned and y's width, loses x's starred to a later write here,
        // and ties "dddd" on y's width (it wins that tie by its id).
        let t = crate::time::now_ms();
        crate::vfs::write(file("eeee"), serde_json::to_vec(&serde_json::json!({ "v": 1, "device": { "id": "eeee", "name": "旧电脑", "platform": "windows" },
            "items": { (x.clone()): { "pinned": [true, t], "starred": [5, 1] }, (y.clone()): { "width": ["wide", t] } } })).unwrap()).unwrap();
        crate::vfs::write(file("dddd"), serde_json::to_vec(&serde_json::json!({ "v": 1, "device": { "id": "dddd" },
            "items": { (y.clone()): { "width": ["full", t] } } })).unwrap()).unwrap();
        a.set_starred(&x, true).unwrap();
        let view = || {
            let m = meta::load(&lib.0, "aaaa", &mut vec![]);
            m.items.into_iter().map(|(i, f)| (i, f.into_iter().map(|(k, w)| (k, w.v)).collect::<BTreeMap<_, _>>())).collect::<BTreeMap<_, _>>()
        };
        let before = view();
        assert_eq!(before[&y]["width"], "wide");
        let listed: Vec<String> = a.devices().into_iter().map(|d| d.device.id).collect();
        assert!(listed.contains(&"eeee".to_string()) && listed.contains(&"aaaa".to_string()));
        assert_eq!(a.devices().into_iter().find(|d| d.device.id == "eeee").unwrap().last, t);
        assert!(a.retire_device("aaaa").is_err() && a.retire_device("ffff").is_err());
        a.retire_device("eeee").unwrap();
        assert_eq!(view(), before);
        assert!(!file("eeee").fs_exists() && file("dddd").fs_exists());
        assert!(!a.devices().iter().any(|d| d.device.id == "eeee"));
        let m = a.list().into_iter().find(|e| e.id == x).unwrap().meta;
        assert!(m.pinned && m.starred > 5);
    }

    #[test]
    fn attachments_come_in_under_new_names() {
        let lib = Lib::new("attach");
        let s = lib.open();
        let src = lib.0.with_extension("src");
        crate::vfs::create_dir_all(&src).unwrap();
        crate::vfs::write(src.join("截图.PNG"), b"png").unwrap();
        let one = s.import_attachment(&src.join("截图.PNG")).unwrap();
        let two = s.import_attachment(&src.join("截图.PNG")).unwrap();
        assert!(one.starts_with(".assets/") && one.ends_with(".PNG") && one != two, "{one} {two}");
        assert_eq!(crate::vfs::read(lib.0.join(&one)).unwrap(), b"png");
        assert!(s.import_attachment(&src.join("没有.png")).is_err());
        let _ = crate::vfs::remove_dir_all(&src);
    }

    #[test]
    fn format_4_moves_to_device_files() {
        let lib = Lib::new("v4to5");
        STAY_4.with(|k| k.set(true));
        let mut s = lib.open();
        let a = s.create_from(&doc("甲")).unwrap();
        let f = s.create_folder("夹", None).unwrap();
        let b = s.create_from_in(&doc("乙"), Some(&f)).unwrap();
        s.set_pinned(&a, true).unwrap();
        s.set_starred(&b, true).unwrap();
        let mut st = s.settings();
        st.set_history_limit(7);
        st.scroll_top.insert(a.clone(), [3., 12.5]);
        s.save_settings(&st).unwrap();
        Store::save_look_at(&lib.0, &FolderLook { theme: Some("墨".into()), ..Default::default() }).unwrap();
        drop(s);
        assert_eq!(Store::format_at(&lib.0), 4);
        STAY_4.with(|k| k.set(false));
        let mut s = lib.open();
        // 4 → 5 → 6 in one go.
        assert!(s.v6 && Store::format_at(&lib.0) == 6, "{:?}", s.warnings);
        assert!(meta::dir(&lib.0).join(device().id).fs_is_dir() && lib.0.join(".quiet/index.json.v4").fs_is_file() && !lib.0.join(".quiet/index.json").fs_exists());
        let list = s.list();
        let by = |id: &str| list.iter().find(|e| e.id == id).unwrap().meta.clone();
        assert!(by(&a).pinned && by(&b).starred > 0 && by(&b).parent.as_deref() == Some(f.as_str()));
        assert_eq!(s.settings().history_limit(), 7);
        assert_eq!(s.scroll_top(&a), Some((3, 12.5)));
        assert_eq!(Store::look_at(&lib.0).theme.as_deref(), Some("墨"));
        // Opened by a build that only knows 4, it is refused; one still open stops writing.
        STAY_4.with(|k| k.set(true));
        let lib4 = Lib::new("v4open");
        let mut old = lib4.open();
        STAY_4.with(|k| k.set(false));
        let x = old.create_from(&doc("丙")).unwrap();
        let mut new = lib4.open();
        assert!(new.v6 && new.list().iter().any(|e| e.id == x));
        assert!(old.set_pinned(&x, true).unwrap_err().to_string().contains("新格式"));
    }

    /// Pages in folder `parent`, in the order the sidebar shows them.
    fn in_order(s: &mut Store, parent: Option<&str>) -> Vec<String> {
        let mut v: Vec<(bool, u64, std::cmp::Reverse<u64>, String)> = s.list().into_iter().filter(|e| e.meta.parent.as_deref() == parent && !e.meta.local())
            .map(|e| { let p = e.meta.place(); (!p.0, p.1, p.2, e.id) }).collect();
        v.sort();
        v.into_iter().map(|x| x.3).collect()
    }

    #[test]
    fn format_5_moves_to_6() {
        let lib = Lib::new("v5to6");
        STAY_5.with(|k| k.set(true));
        let mut s = lib.open();
        STAY_5.with(|k| k.set(false));
        assert!(s.v5 && !s.v6);
        let f = s.create_folder("夹", None).unwrap();
        let kids: Vec<String> = ["一", "二", "三", "四"].iter().map(|t| s.create_from_in(&doc(t), Some(&f)).unwrap()).collect();
        s.move_in_tree(&kids[3], Some(&f), Some(&kids[0])).unwrap();
        s.set_pinned(&kids[1], true).unwrap();
        let gone = kids[2].clone();
        s.delete(&gone, None).unwrap();
        let order = in_order(&mut s, Some(&f));
        drop(s);
        let state = |lib: &Lib| {
            let mut s = lib.open();
            assert!(s.v6 && Store::format_at(&lib.0) == 6, "{:?}", s.warnings);
            let trash: Vec<(String, Option<String>)> = s.list_trash().into_iter().map(|t| (t.id, t.parent)).collect();
            (in_order(&mut s, Some(&f)), trash, s.list().into_iter().find(|e| e.id == kids[1]).unwrap().meta.pinned)
        };
        let first = state(&lib);
        assert_eq!(first, (order.clone(), vec![(gone.clone(), Some(f.clone()))], true));
        let trash = lib.0.join(".quiet/trash");
        assert!(trash.join("index.json.v5").fs_is_file() && !trash.join("index.json").fs_exists());
        let file = lib.meta_text();
        assert!(file.contains("\"trashed\"") && file.contains("\"order\": [\n"), "{file}");
        // A crash after this device's file, before the marker: done again, the same.
        let marker = lib.0.join(".quiet/format.json");
        let m: FormatMarker = reread(&marker).unwrap();
        write_atomic(&marker, &serde_json::to_vec(&FormatMarker { format: 5, id: m.id }).unwrap()).unwrap();
        crate::vfs::rename(trash.join("index.json.v5"), trash.join("index.json")).unwrap();
        let mine = lib.meta_text();
        assert_eq!(state(&lib), first);
        let keys = |t: &str| -> Vec<String> { let v: serde_json::Value = serde_json::from_str(t).unwrap(); kids.iter().filter_map(|k| v["items"][k]["order"][0].as_str().map(str::to_string)).collect() };
        assert_eq!(keys(&lib.meta_text()), keys(&mine));
        // Restored, it goes back where it was.
        let mut s = lib.open();
        s.restore(&gone).unwrap();
        assert_eq!(s.list().into_iter().find(|e| e.id == gone).unwrap().meta.parent.as_deref(), Some(f.as_str()));
        assert!(s.list_trash().is_empty());
        // A FORMAT 5 build still open stops writing once it is 6.
        STAY_5.with(|k| k.set(true));
        let lib5 = Lib::new("v5open");
        let mut old = lib5.open();
        STAY_5.with(|k| k.set(false));
        let x = old.create_from(&doc("丙")).unwrap();
        let mut new = lib5.open();
        assert!(new.v6 && new.list().iter().any(|e| e.id == x));
        assert!(old.set_pinned(&x, true).unwrap_err().to_string().contains("新格式"));
        // … and one opening it refuses it (`format > FORMAT`, as 5 would see it).
        let marker = lib5.0.join(".quiet/format.json");
        let m: FormatMarker = reread(&marker).unwrap();
        write_atomic(&marker, &serde_json::to_vec(&FormatMarker { format: FORMAT + 1, id: m.id }).unwrap()).unwrap();
        assert!(Store::open_as(lib5.0.clone(), &library_key(&lib5.0), device()).is_err());
    }

    #[test]
    fn devices_share_the_trash() {
        let lib = Lib::new("trash-devices");
        let mut a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        let [x, y, z] = ["甲", "乙", "丙"].map(|t| a.create_from(&doc(t)).unwrap());
        let mut b = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("bbbb")).unwrap();
        b.list();
        // Each deletes one: both are in the 回收站 on both.
        a.delete(&x, None).unwrap();
        b.delete(&y, None).unwrap();
        let ids = |s: &mut Store| { let mut v: Vec<String> = s.list_trash().into_iter().map(|t| t.id).collect(); v.sort(); v };
        let both = { let mut v = vec![x.clone(), y.clone()]; v.sort(); v };
        assert_eq!(ids(&mut a), both);
        assert_eq!(ids(&mut b), both);
        // Deleted on one, restored on the other: the later write holds.
        a.delete(&z, None).unwrap();
        assert!(ids(&mut b).contains(&z));
        b.restore(&z).unwrap();
        assert!(!ids(&mut a).contains(&z) && a.list().iter().any(|e| e.id == z));
        // A record before its file: not shown until the file is here.
        let file = a.trash_path(&x);
        let away = lib.0.join("away.xml");
        crate::vfs::rename(&file, &away).unwrap();
        assert!(!ids(&mut b).contains(&x));
        crate::vfs::rename(&away, &file).unwrap();
        assert!(ids(&mut b).contains(&x));
        // 彻底删除: gone on both, a tombstone in the device's file.
        b.purge(&x).unwrap();
        assert!(!ids(&mut a).contains(&x));
        assert!(lib.dev_text("bbbb").contains("\"purged\""));
    }

    /// Device `d`'s shards made back into one whole file, as the
    /// development builds of FORMAT 6 wrote it.
    fn unshard(lib: &Lib, d: &str) {
        let own = meta::dir(&lib.0).join(d);
        let device: serde_json::Value = reread(&own.join("device.json")).unwrap();
        let v = serde_json::json!({ "v": 1, "device": device, "items": lib.dev_items(d)["items"] });
        crate::vfs::remove_dir_all(&own).unwrap();
        crate::vfs::write(meta::dir(&lib.0).join(format!("{d}.json")), serde_json::to_vec_pretty(&v).unwrap()).unwrap();
    }

    /// Every field of the merged view: value, stamp and device.
    fn view(lib: &Lib) -> BTreeMap<String, BTreeMap<String, (serde_json::Value, u64, String)>> {
        meta::load(&lib.0, "", &mut vec![]).items.into_iter().map(|(id, f)| (id, f.into_iter().map(|(k, w)| (k, (w.v, w.ts, w.dev))).collect())).collect()
    }

    /// A folder's files by name, with what they hold.
    fn contents(dir: &Path) -> BTreeMap<String, Vec<u8>> {
        crate::vfs::read_dir(dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".json")).map(|e| (e.file_name().to_string_lossy().into_owned(), crate::vfs::read(e.path()).unwrap())).collect()
    }

    /// A folder's files by name, with what changes whenever one is written.
    fn written_stamps(dir: &Path) -> BTreeMap<String, u64> {
        crate::vfs::read_dir(dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".json")).map(|e| {
            let m = crate::vfs::metadata(e.path()).unwrap();
            (e.file_name().to_string_lossy().into_owned(), meta::written(&e.path(), Some(&m), false))
        }).collect()
    }

    /// Two devices' whole files (development FORMAT 6): 回收站, 彻底删除,
    /// 置顶, 星标 and hand order. Returns the folder and its pages.
    fn whole_file_library(lib: &Lib) -> (String, Vec<String>) {
        let mut a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        let f = a.create_folder("夹", None).unwrap();
        let p: Vec<String> = (0..8).map(|n| a.create_from_in(&doc(&format!("页{n}")), Some(&f)).unwrap()).collect();
        a.move_in_tree(&p[0], Some(&f), None).unwrap();
        a.set_pinned(&p[1], true).unwrap();
        let mut b = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("bbbb")).unwrap();
        b.list();
        b.set_starred(&p[2], true).unwrap();
        b.move_in_tree(&p[5], Some(&f), Some(&p[1])).unwrap();
        b.set_pinned(&p[1], false).unwrap();
        a.delete(&p[3], None).unwrap();
        a.delete(&p[4], None).unwrap();
        a.purge(&p[4]).unwrap();
        drop((a, b));
        unshard(lib, "aaaa");
        unshard(lib, "bbbb");
        (f, p)
    }

    #[test]
    fn whole_device_files_become_shards() {
        let lib = Lib::new("to-shards");
        let (f, p) = whole_file_library(&lib);
        let before = view(&lib);
        assert!(before[&p[4]].contains_key("purged") && before[&p[3]].contains_key("trashed") && before[&p[2]].contains_key("starred"));
        // What a third device lists, before and after.
        let listed = |lib: &Lib| {
            let mut c = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("cccc")).unwrap();
            let mut l: Vec<(String, Meta)> = c.list().into_iter().map(|e| (e.id, e.meta)).collect();
            l.sort_by(|a, b| a.0.cmp(&b.0));
            let mut t: Vec<String> = c.list_trash().into_iter().map(|t| t.id).collect();
            t.sort();
            (l, t, in_order(&mut c, Some(&f)))
        };
        let list = listed(&lib);
        assert_eq!(list.1, vec![p[3].clone()]);
        let whole = crate::vfs::read(meta::dir(&lib.0).join("aaaa.json")).unwrap();
        crate::vfs::write(meta::dir(&lib.0).join("aaaa.json.bak"), &whole).unwrap();
        // This device converts its own file only, its backup with it (`.single` is one).
        let mut a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        assert!(a.warnings.is_empty(), "{:?}", a.warnings);
        let d = meta::dir(&lib.0);
        assert!(d.join("aaaa").fs_is_dir() && !d.join("aaaa.json").fs_exists() && !d.join("aaaa.tmp").fs_exists());
        assert!(!d.join("aaaa.json.bak").fs_exists());
        assert_eq!(crate::vfs::read(d.join("aaaa.json.single")).unwrap(), whole);
        assert!(d.join("bbbb.json").fs_is_file() && !d.join("bbbb").fs_exists(), "the other device's stays whole");
        assert_eq!(view(&lib), before);
        assert!(a.list().iter().any(|e| e.id == p[0]));
        assert_eq!(listed(&lib), list);
        // The other device, when it opens.
        drop(a);
        Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("bbbb")).unwrap();
        assert!(d.join("bbbb").fs_is_dir() && !d.join("bbbb.json").fs_exists() && d.join("bbbb.json.single").fs_is_file());
        assert_eq!(view(&lib), before);
        assert_eq!(listed(&lib), list);
        // Opened again: nothing to convert, nothing written.
        let stamps = written_stamps(&d.join("aaaa"));
        let a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        assert!(!meta::needs_shards(&lib.0, "aaaa") && a.warnings.is_empty());
        assert_eq!(written_stamps(&d.join("aaaa")), stamps);
        drop(a);
        // The `.single` copy stays 30 days, then the day's clean-up removes it.
        let t = meta::now_ms();
        let open_at = |days: u64| {
            meta::CLOCK.with(|c| c.set(Some(t + days * meta::DAY_MS)));
            let mut a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
            a.list();
            assert!(d.join("aaaa.json.single").fs_is_file(), "not on the way to the first list");
            a.list();
            meta::CLOCK.with(|c| c.set(None));
        };
        open_at(29);
        assert!(d.join("aaaa.json.single").fs_is_file());
        open_at(31);
        assert!(!d.join("aaaa.json.single").fs_exists() && d.join("bbbb.json.single").fs_is_file());
        assert_eq!(view(&lib), before);
    }

    /// A crash midway through `to_shards`: opened again, the same result.
    #[test]
    fn a_conversion_cut_short_ends_the_same() {
        let lib = Lib::new("to-shards-crash");
        whole_file_library(&lib);
        let d = meta::dir(&lib.0);
        let whole = crate::vfs::read(d.join("aaaa.json")).unwrap();
        meta::to_shards(&lib.0, &dev("aaaa"), &mut vec![]).unwrap();
        let (shards, before) = (contents(&d.join("aaaa")), view(&lib));
        // Cut short while writing `<id>.tmp/`.
        crate::vfs::remove_dir_all(d.join("aaaa")).unwrap();
        crate::vfs::write(d.join("aaaa.json"), &whole).unwrap();
        crate::vfs::create_dir_all(d.join("aaaa.tmp")).unwrap();
        crate::vfs::write(d.join("aaaa.tmp/07.json"), b"{ half").unwrap();
        assert!(!view(&lib).is_empty(), "the folder half made is not read");
        let a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        assert!(a.warnings.is_empty(), "{:?}", a.warnings);
        assert!(!d.join("aaaa.tmp").fs_exists() && !d.join("aaaa.json").fs_exists());
        assert_eq!((contents(&d.join("aaaa")), view(&lib)), (shards.clone(), before.clone()));
        drop(a);
        // Cut short before the whole file went: only that is left to do.
        crate::vfs::write(d.join("aaaa.json"), &whole).unwrap();
        assert_eq!(view(&lib), before, "the folder counts, not the file");
        let stamps = written_stamps(&d.join("aaaa"));
        let a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        assert!(a.warnings.is_empty(), "{:?}", a.warnings);
        assert!(!d.join("aaaa.json").fs_exists());
        assert_eq!(written_stamps(&d.join("aaaa")), stamps, "the shards are not written again");
        assert_eq!((contents(&d.join("aaaa")), view(&lib)), (shards, before));
    }

    #[test]
    fn a_change_rewrites_its_shard_only() {
        let lib = Lib::new("shard-write");
        let mut a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        let ids: Vec<String> = (0..40).map(|n| a.create_from(&doc(&format!("页{n}")))).collect::<Result<_>>().unwrap();
        a.list();
        let own = meta::dir(&lib.0).join("aaaa");
        let before = written_stamps(&own);
        assert!(before.len() > 20, "{before:?}");
        a.set_pinned(&ids[7], true).unwrap();
        let after = written_stamps(&own);
        let changed: Vec<&String> = before.keys().filter(|k| before.get(*k) != after.get(*k)).collect();
        assert_eq!(changed, vec![&format!("{:02x}.json", meta::shard_of(&ids[7]))]);
    }

    fn gone_of(lib: &Lib, d: &str, id: &str) -> bool { lib.dev_items(d)["items"][id]["gone"].is_array() }

    /// Deleted outside the app (sync.md 3.3): a tombstone, written only by a
    /// device that saw the file and finds it gone in a whole scan.
    #[test]
    fn a_file_deleted_outside_gets_a_tombstone() {
        let lib = Lib::new("gone");
        let mut a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        let x = a.create_from(&doc("甲")).unwrap();
        a.list();
        a.list();
        let mut b = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("bbbb")).unwrap();
        assert!(b.list().iter().any(|e| e.id == x));
        let file = a.path(&x);
        let text = crate::vfs::read(&file).unwrap();
        crate::vfs::remove_file(&file).unwrap();
        assert!(!a.list().iter().any(|e| e.id == x));
        assert!(gone_of(&lib, "aaaa", &x) && view(&lib)[&x]["path"].0.is_null());
        assert!(!b.list().iter().any(|e| e.id == x), "the other device leaves it out too");
        assert!(!lib.index().values().any(|e| e.id == x));
        // Written again later (its file back): here again.
        meta::commit(&lib.0, &dev("bbbb"), &Default::default(), vec![(x.clone(), "path".into(), "甲.xml".into())], &mut vec![]).unwrap();
        crate::vfs::write(&file, &text).unwrap();
        assert!(a.list().iter().any(|e| e.id == x));
        assert!(b.list().iter().any(|e| e.id == x));
    }

    /// A file put back where one deleted outside the app was (a git branch
    /// switched back) is that page again: its id, pin and place.
    #[test]
    fn a_file_put_back_where_it_was_takes_its_id_again() {
        let lib = Lib::new("gone-back");
        let mut a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        let x = a.create_from(&doc("甲")).unwrap();
        let y = a.create_from(&doc("乙")).unwrap();
        a.set_pinned(&x, true).unwrap();
        a.initialize_manual_order().unwrap();
        a.list();
        let (fx, fy) = (a.path(&x), a.path(&y));
        let (tx, ty) = (crate::vfs::read(&fx).unwrap(), crate::vfs::read(&fy).unwrap());
        let place = view(&lib)[&x]["root_order"].0.clone();
        assert!(place.is_string());
        let rel = a.index[&x].rel.clone();
        crate::vfs::remove_file(&fx).unwrap();
        crate::vfs::remove_file(&fy).unwrap();
        assert!(!a.list().iter().any(|e| e.id == x || e.id == y));
        assert_eq!(lib.dev_items("aaaa")["items"][&x]["gone"][0]["from"], serde_json::json!(rel));
        // The same path: the same page, pinned and in its place.
        crate::vfs::write(&fx, &tx).unwrap();
        assert!(pinned(&mut a, &x));
        assert_eq!(view(&lib)[&x]["root_order"].0, place);
        assert!(!lib.index().values().any(|e| e.id == meta::path_id(&rel)));
        // Elsewhere: a new page.
        let moved = fy.with_file_name("丙.xml");
        crate::vfs::write(&moved, &ty).unwrap();
        let list = a.list();
        assert!(!list.iter().any(|e| e.id == y) && list.iter().any(|e| e.id == meta::path_id("丙.xml")));
        crate::vfs::remove_file(&moved).unwrap();
        a.list();
        // Back where it was after 30 days: a new page too (its tombstone may be gone).
        meta::CLOCK.with(|c| c.set(Some(meta::now_ms() + meta::PURGED_KEPT_MS + 1)));
        crate::vfs::write(&fy, &ty).unwrap();
        let ry = a.index.get(&y).map(|m| m.rel.clone());
        let list = a.list();
        meta::CLOCK.with(|c| c.set(None));
        assert!(ry.is_none() && !list.iter().any(|e| e.id == y));
    }

    #[test]
    fn a_file_never_here_gets_no_tombstone() {
        let lib = Lib::new("gone-unseen");
        let mut a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        let x = a.create_from(&doc("甲")).unwrap();
        a.list();
        let file = a.path(&x);
        // This device never listed while the file was here: still on its way, for all it knows.
        crate::vfs::remove_file(&file).unwrap();
        let mut b = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("bbbb")).unwrap();
        assert!(!b.list().iter().any(|e| e.id == x));
        assert!(!gone_of(&lib, "bbbb", &x) && !view(&lib)[&x].contains_key("gone"));
        // Its file arrives: listed, nothing lost.
        crate::vfs::write(&file, "<title>甲</title>\n").unwrap();
        assert!(b.list().iter().any(|e| e.id == x));
    }

    #[test]
    fn a_placeholder_or_a_part_scan_is_not_a_deletion() {
        let lib = Lib::new("gone-not");
        let mut a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        let x = a.create_from(&doc("甲")).unwrap();
        let f = a.create_folder("夹", None).unwrap();
        let y = a.create_from_in(&doc("乙"), Some(&f)).unwrap();
        #[cfg(not(unix))]
        let _ = &y;
        a.list();
        a.list();
        // Only in iCloud now: a placeholder in its place.
        let file = a.path(&x);
        crate::vfs::remove_file(&file).unwrap();
        crate::vfs::write(layout::placeholder(&file).unwrap(), b"").unwrap();
        assert!(a.list().iter().any(|e| e.id == x));
        assert!(!gone_of(&lib, "aaaa", &x));
        // A folder that could not be read: the scan is not whole.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            crate::vfs::remove_file(a.path(&y)).unwrap();
            let dir = lib.0.join("夹");
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).unwrap();
            a.list();
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(!gone_of(&lib, "aaaa", &y));
            // Read whole again: gone.
            a.list();
            assert!(gone_of(&lib, "aaaa", &y));
        }
    }

    /// Clean-up (sync.md 3.3), on a clock the test sets.
    #[test]
    fn records_never_arrived_go_after_thirty_days() {
        let lib = Lib::new("clean-arrive");
        drop(Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap());
        let t = crate::time::now_ms();
        let at = |days: u64, item: &str, path: &str| {
            meta::CLOCK.with(|c| c.set(Some(t - days * meta::DAY_MS)));
            meta::commit(&lib.0, &dev("aaaa"), &Default::default(), vec![(item.into(), "path".into(), path.into()), (item.into(), "pinned".into(), true.into())], &mut vec![]).unwrap();
        };
        at(31, "old", "没来.xml");
        at(29, "young", "也没来.xml");
        at(31, "here", "在.xml");
        crate::vfs::write(lib.0.join("在.xml"), "<title>在</title>\n").unwrap();
        meta::CLOCK.with(|c| c.set(None));
        meta::clean_all(&lib.0, &dev("aaaa"), &mut vec![]).unwrap();
        let v = lib.dev_items("aaaa");
        assert!(v["items"].get("old").is_none(), "{v}");
        assert!(v["items"].get("young").is_some() && v["items"].get("here").is_some(), "{v}");
    }

    #[test]
    fn tombstones_wait_for_every_active_device() {
        let lib = Lib::new("clean-tomb");
        drop(Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap());
        let t = crate::time::now_ms();
        let write = |days: u64, by: &str, changes: Vec<(&str, &str, serde_json::Value)>| {
            meta::CLOCK.with(|c| c.set(Some(t - days * meta::DAY_MS)));
            let changes = changes.into_iter().map(|(a, b, c)| (a.to_string(), b.to_string(), c)).collect();
            meta::commit(&lib.0, &dev(by), &Default::default(), changes, &mut vec![]).unwrap();
        };
        // Long stopped: does not hold anything back.
        write(100, "cccc", vec![("c", "pinned", true.into())]);
        // Last wrote before the tombstone, but lately: has not seen it.
        write(40, "bbbb", vec![("b", "pinned", true.into())]);
        write(32, "aaaa", vec![("gone-1", "path", "甲.xml".into()), ("gone-2", "path", "乙.xml".into()), ("empty", "pinned", true.into())]);
        write(31, "aaaa", vec![("gone-1", "gone", serde_json::json!({ "at": 1 })), ("gone-1", "path", serde_json::Value::Null), ("empty", "pinned", serde_json::Value::Null)]);
        write(29, "aaaa", vec![("gone-2", "purged", 1.into())]);
        meta::CLOCK.with(|c| c.set(None));
        let clean = || { meta::clean_all(&lib.0, &dev("aaaa"), &mut vec![]).unwrap(); lib.dev_items("aaaa") };
        let v = clean();
        assert!(v["items"].get("gone-1").is_some() && v["items"].get("empty").is_some(), "bbbb has not seen them: {v}");
        // Once bbbb writes again, it has.
        write(0, "bbbb", vec![("b", "starred", 1.into())]);
        meta::CLOCK.with(|c| c.set(None));
        let v = clean();
        assert!(v["items"].get("gone-1").is_none() && v["items"].get("empty").is_none(), "{v}");
        assert!(v["items"].get("gone-2").is_some(), "29 days: kept");
        assert!(!lib.index().values().any(|e| e.id == "gone-2"));
    }

    #[test]
    fn a_null_stays_while_another_device_still_holds_the_old_value() {
        let lib = Lib::new("clean-held");
        drop(Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap());
        let t = crate::time::now_ms();
        let write = |days: u64, by: &str, item: &str, field: &str, v: serde_json::Value| {
            meta::CLOCK.with(|c| c.set(Some(t - days * meta::DAY_MS)));
            meta::commit(&lib.0, &dev(by), &Default::default(), vec![(item.to_string(), field.to_string(), v)], &mut vec![]).unwrap();
        };
        // bbbb pinned x; aaaa unpinned it later; bbbb has written since, but
        // to another shard, so its `pinned: true` for x is still in its files.
        write(40, "bbbb", "x", "pinned", true.into());
        write(35, "aaaa", "x", "pinned", serde_json::Value::Null);
        let other = (0..).map(|n| format!("别片-{n}")).find(|z| meta::shard_of(z) != meta::shard_of("x")).unwrap();
        write(1, "bbbb", &other, "starred", 1.into());
        meta::CLOCK.with(|c| c.set(None));
        meta::clean_all(&lib.0, &dev("aaaa"), &mut vec![]).unwrap();
        assert!(lib.dev_items("aaaa")["items"]["x"]["pinned"].is_array(), "the null keeps bbbb's true down");
        let mut warnings = vec![];
        assert!(meta::load(&lib.0, "aaaa", &mut warnings).get("x", "pinned").is_none(), "x stays unpinned");
        // Once bbbb writes x's shard again it drops the value it lost, and the null can go.
        meta::clean_all(&lib.0, &dev("bbbb"), &mut vec![]).unwrap();
        assert!(lib.dev_items("bbbb")["items"].get("x").is_none_or(|f| f.get("pinned").is_none()));
        meta::clean_all(&lib.0, &dev("aaaa"), &mut vec![]).unwrap();
        assert!(lib.dev_items("aaaa")["items"].get("x").is_none(), "{}", lib.dev_items("aaaa"));
        assert!(meta::load(&lib.0, "aaaa", &mut warnings).get("x", "pinned").is_none());
    }

    #[test]
    fn tombstones_go_after_thirty_days() {
        let lib = Lib::new("tombstones");
        let mut s = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        // Purged 31 days ago: the next write of its shard drops what this device said about it.
        let t = crate::time::now_ms();
        meta::CLOCK.with(|c| c.set(Some(t - 31 * meta::DAY_MS)));
        let x = s.create_from(&doc("甲")).unwrap();
        s.delete(&x, None).unwrap();
        s.purge(&x).unwrap();
        meta::CLOCK.with(|c| c.set(None));
        assert!(lib.dev_items("aaaa")["items"][&x]["purged"].is_array());
        let y = s.create_from(&doc("乙")).unwrap();
        s.set_pinned(&y, true).unwrap();
        // Another page of the same shard.
        let z = (0..).map(|n| format!("同片-{n}")).find(|z| meta::shard_of(z) == meta::shard_of(&x)).unwrap();
        meta::commit(&lib.0, &dev("aaaa"), &Default::default(), vec![(z.clone(), "pinned".into(), true.into())], &mut vec![]).unwrap();
        let v = lib.dev_items("aaaa");
        assert!(v["items"].get(&x).is_none() && v["items"].get(&y).is_some(), "{v}");
        assert!(s.list_trash().is_empty() && !s.list().iter().any(|e| e.id == x));
    }

    #[test]
    fn a_move_writes_the_moved_page_only() {
        let lib = Lib::new("fractional");
        let mut a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        let f = a.create_folder("夹", None).unwrap();
        let p: Vec<String> = (0..5).map(|n| a.create_from_in(&doc(&format!("页{n}")), Some(&f)).unwrap()).collect();
        a.move_in_tree(&p[0], Some(&f), Some(&p[1])).unwrap();
        let mut b = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("bbbb")).unwrap();
        let start = in_order(&mut b, Some(&f));
        let file = |d: &str| lib.dev_items(d);
        let orders = |v: &serde_json::Value| -> Vec<String> { p.iter().filter(|id| v["items"][*id].get("order").is_some()).cloned().collect() };
        // A moves the first to the end, B the last to the front: each device's file names only its page.
        let last = start[4].clone();
        let before = file("aaaa");
        a.move_in_tree(&start[0], Some(&f), None).unwrap();
        let after = file("aaaa");
        let changed: Vec<&String> = p.iter().filter(|id| before["items"][*id]["order"][0] != after["items"][*id]["order"][0]).collect();
        assert_eq!(changed, vec![&start[0]]);
        let front = in_order(&mut b, Some(&f))[0].clone();
        b.move_in_tree(&last, Some(&f), Some(&front)).unwrap();
        assert_eq!(orders(&file("bbbb")), vec![last.clone()]);
        let ia = in_order(&mut a, Some(&f));
        assert_eq!(ia, in_order(&mut b, Some(&f)));
        assert_eq!(ia, [&start[4..], &start[1..4], &start[..1]].concat());
        assert_eq!(orders(&file("aaaa")).len(), 5, "created here: every page has its key from A");
        // The same key on two devices: both order the pages by id.
        let key = file("aaaa")["items"][&start[0]]["order"][0].clone();
        let other = ia.iter().find(|id| **id != start[0] && **id != last).unwrap().clone();
        meta::commit(&lib.0, &dev("cccc"), &Default::default(), vec![(other.clone(), "order".into(), key)], &mut vec![]).unwrap();
        let (oa, ob) = (in_order(&mut a, Some(&f)), in_order(&mut b, Some(&f)));
        assert_eq!(oa, ob);
        let (i, j) = (oa.iter().position(|x| *x == start[0]).unwrap(), oa.iter().position(|x| *x == other).unwrap());
        assert_eq!(i < j, start[0] < other);
    }

    #[test]
    fn devices_merge_field_by_field() {
        let lib = Lib::new("devices");
        let mut a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        let x = a.create_from(&doc("共享")).unwrap();
        let mut b = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("bbbb")).unwrap();
        assert!(b.list().iter().any(|e| e.id == x));
        // Different fields of one document: both stay.
        a.set_pinned(&x, true).unwrap();
        b.set_starred(&x, true).unwrap();
        let mut c = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("cccc")).unwrap();
        let m = c.list().into_iter().find(|e| e.id == x).unwrap().meta;
        assert!(m.pinned && m.starred > 0);
        // One field: the later write wins, and the earlier device's file drops it on its next write.
        b.set_pinned(&x, false).unwrap();
        assert!(!c.list().into_iter().find(|e| e.id == x).unwrap().meta.pinned);
        a.set_starred(&x, false).unwrap();
        let file = |d: &str| lib.dev_text(d);
        assert!(!file("aaaa").contains("pinned") && file("bbbb").contains("pinned"));
        // A store that read before another device wrote does not undo it with what it read.
        let y = a.create_from(&doc("另一篇")).unwrap();
        b.list();
        a.set_pinned(&y, true).unwrap();
        b.set_starred(&y, true).unwrap();
        let m = c.list().into_iter().find(|e| e.id == y).unwrap().meta;
        assert!(m.pinned && m.starred > 0);
        // The top-level folder's own settings, each field on its own.
        let mut st = a.settings();
        st.set_history_limit(9);
        a.save_settings(&st).unwrap();
        b.list();
        assert_eq!(b.settings().history_limit(), 9);
        // Renamed on one device, the others follow.
        a.rename(&x, "改名").unwrap();
        assert_eq!(c.list().into_iter().find(|e| e.id == x).unwrap().meta.rel, "改名.xml");
    }

    #[test]
    fn devices_agree_on_files_found_and_wait_for_files_on_their_way() {
        let lib = Lib::new("found");
        let mut a = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("aaaa")).unwrap();
        a.list();
        crate::vfs::write(lib.0.join("外来.md"), "# 外来\n").unwrap();
        let mut b = Store::open_as(lib.0.clone(), &library_key(&lib.0), dev("bbbb")).unwrap();
        let id = |s: &mut Store| s.list().into_iter().find(|e| e.title == "外来").unwrap().id;
        assert_eq!(id(&mut a), id(&mut b));
        // Another device's entry for a file not synced here yet keeps its settings.
        meta::commit(&lib.0, &dev("cccc"), &Default::default(), vec![("late".into(), "path".into(), "晚到.xml".into()), ("late".into(), "pinned".into(), true.into())], &mut vec![]).unwrap();
        a.list();
        let x = a.create_from(&doc("触发写入")).unwrap();
        a.set_pinned(&x, true).unwrap();
        assert!(!a.list().iter().any(|e| e.id == "late"));
        // Listed again with nothing changed: still read back in, not forgotten.
        a.list();
        assert!(!a.list().iter().any(|e| e.id == "late"));
        crate::vfs::write(lib.0.join("晚到.xml"), "<title>晚到</title>\n").unwrap();
        let late = a.list().into_iter().find(|e| e.id == "late").unwrap();
        assert!(late.meta.pinned);
        // Deleted in the app: gone for every device, even before the file is.
        a.delete(&x, None).unwrap();
        assert_eq!(meta::load(&lib.0, "", &mut vec![]).get(&x, "path"), None);
    }

    #[test]
    fn look_keeps_each_devices_field() {
        let lib = Lib::new("look");
        lib.open();
        meta::commit(&lib.0, &dev("other"), &Default::default(), vec![(meta::FOLDER_ITEM.into(), "look.doc_family".into(), "宋体".into())], &mut vec![]).unwrap();
        let mut look = Store::look_at(&lib.0);
        assert_eq!(look.doc_family.as_deref(), Some("宋体"));
        look.theme = Some("纸".into());
        Store::save_look_at(&lib.0, &look).unwrap();
        // The other device's later font and this one's theme both stay.
        meta::commit(&lib.0, &dev("other"), &Default::default(), vec![(meta::FOLDER_ITEM.into(), "look.doc_family".into(), "黑体".into())], &mut vec![]).unwrap();
        let look = Store::look_at(&lib.0);
        assert_eq!((look.theme.as_deref(), look.doc_family.as_deref()), (Some("纸"), Some("黑体")));
    }

    /// The first look saved where the old preferences.json already says the same
    /// (it is the settings file the app just wrote, under QUIET_MD_HOME) keeps
    /// every field: the next launch reads only the saved look.
    #[test]
    fn first_look_saved_keeps_fields_the_old_preferences_share() {
        let lib = Lib::new("look-first");
        lib.open();
        let look = FolderLook { theme: Some("notion".into()), feishu: Some(false), doc_family: Some("LXGW WenKai".into()), doc_size: Some(String::new()) };
        crate::vfs::write(lib.0.join(INTERNAL).join("preferences.json"), r#"{"theme":"notion","feishu":false,"doc_family":"LXGW WenKai"}"#).unwrap();
        Store::save_look_at(&lib.0, &look).unwrap();
        crate::vfs::write(lib.0.join(INTERNAL).join("preferences.json"), "{}").unwrap();
        assert_eq!(Store::look_at(&lib.0), look);
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
        let shard = lib.dev_file(&device().id, &id);
        crate::vfs::write(&shard, b"{ not json").unwrap();
        let mut s = lib.open();
        assert_eq!(s.warnings.len(), 1, "{:?}", s.warnings);
        assert!(s.warnings[0].contains("已从备份恢复"));
        assert!(s.list()[0].meta.pinned);
        // The damaged file is kept for inspection, not overwritten.
        let name = shard.file_name().unwrap().to_string_lossy().into_owned();
        let kept = crate::vfs::read_dir(shard.parent().unwrap()).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with(&format!("{name}.corrupt-"))).count();
        assert_eq!(kept, 1);
        // Put back, for the next write to start from.
        assert!(lib.index().values().any(|a| a.pinned));
    }

    #[test]
    fn corrupt_index_without_backup_still_lists_documents() {
        let lib = Lib::new("nobak");
        let mut s = lib.open();
        let id = s.create_from(&doc("一")).unwrap();
        let shard = lib.dev_file(&device().id, &id);
        crate::vfs::write(&shard, b"garbage").unwrap();
        let _ = crate::vfs::remove_file(with_suffix(&shard, ".bak"));
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
        assert!(lib.0.join("父/父.xml").fs_is_file() && lib.0.join("父/子一.xml").fs_is_file());
    }

    #[test]
    fn newer_format_is_refused() {
        // A newer build may have changed what the index means (folders, FORMAT 2):
        // opening it here could misread and then rewrite it, so it is refused.
        let lib = Lib::new("format");
        drop(lib.open());
        crate::vfs::write(lib.0.join(".quiet/format.json"), format!("{{\"format\": {}}}", FORMAT + 1)).unwrap();
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

    /// A page a tree writer deleted is in this copy's 回收站 too, and comes back.
    #[test]
    fn trashed_by_a_tree_writer() {
        let lib = Lib::new("writertrash");
        let mut s = lib.open();
        let id = s.create_from(&doc("页")).unwrap();
        let _ = (s.list(), s.list_trash());
        let mut w = s.tree_writer();
        w.delete(&id, None).unwrap();
        assert_eq!(w.list_trash().len(), 1);
        assert!(s.trashed(&id));
        assert!(!s.restore(&id).unwrap());
        assert_eq!(s.list().len(), 1);
        assert!(!s.trashed(&id));
    }

    /// Deleting many pages (a folder and a selection) writes each index
    /// once, not once a page: every synced write waits for the disk.
    #[test]
    fn delete_many_writes_each_index_once() {
        let lib = Lib::new("delmany");
        let mut s = lib.open();
        let f = s.create_folder("夹", None).unwrap();
        let inside: Vec<String> = (0..10).map(|i| { let d = s.create_from(&doc(&format!("里 {i}"))).unwrap(); s.set_parent(&d, Some(&f)).unwrap(); d }).collect();
        let loose: Vec<String> = (0..10).map(|i| s.create_from(&doc(&format!("散 {i}"))).unwrap()).collect();
        let _ = s.list();
        let mut items = vec![(f.clone(), None)];
        items.extend(inside.iter().map(|d| (d.clone(), Some(f.clone()))));
        items.extend(loose.iter().map(|d| (d.clone(), None)));
        let journal = s.track_file_changes();
        SYNCED_WRITES.with(|n| n.set(0));
        let (gone, r) = s.delete_many(&items);
        r.unwrap();
        let writes = SYNCED_WRITES.with(|n| n.get());
        // Each shard written once for the index and once for the 回收站, and the local state.
        let shards = items.iter().map(|(id, _)| meta::shard_of(id)).collect::<HashSet<_>>().len();
        assert!(writes <= 2 * shards + 1, "{writes} synced writes for {shards} shards");
        assert_eq!(gone.len(), 21);
        assert!(s.list().is_empty());
        assert_eq!(s.list_trash().len(), 11, "the folder's pages ride along with it");
        assert_eq!(journal.take().len(), 21, "撤销 sees every page");
        // Opened again: nothing half deleted, and it all comes back.
        let mut s = lib.open();
        assert!(s.list().is_empty());
        assert_eq!(s.list_trash().len(), 11);
        assert!(!s.restore(&f).unwrap());
        assert_eq!(s.list().len(), 11);
    }

    /// One page that cannot go (its name is taken in the 回收站) stays, alone.
    #[test]
    fn delete_many_keeps_only_the_page_that_cannot_go() {
        let lib = Lib::new("delconflict");
        let mut s = lib.open();
        let a = s.create_from(&doc("一")).unwrap();
        let b = s.create_from(&doc("二")).unwrap();
        let c = s.create_from(&doc("三")).unwrap();
        crate::vfs::write(s.trash_dir().join(format!("{b}.xml")), b"x").unwrap();
        let (gone, r) = s.delete_many(&[(a.clone(), None), (b.clone(), None), (c.clone(), None)]);
        r.unwrap();
        assert_eq!(gone, vec![a.clone(), c.clone()]);
        assert_eq!(s.take_warnings().len(), 1);
        assert_eq!(s.list().into_iter().map(|e| e.id).collect::<Vec<_>>(), vec![b.clone()]);
        assert_eq!(crate::vfs::read(s.trash_dir().join(format!("{b}.xml"))).unwrap(), b"x", "never overwritten");
        // Alone, it is an error.
        assert!(s.delete(&b, None).is_err());
        assert!(s.take_warnings().is_empty());
    }

    /// 清空回收站 writes the 回收站's index once.
    #[test]
    fn purge_many_writes_the_trash_index_once() {
        let lib = Lib::new("purgemany");
        let mut s = lib.open();
        let ids: Vec<String> = (0..10).map(|i| s.create_from(&doc(&format!("页 {i}"))).unwrap()).collect();
        s.delete_many(&ids.iter().map(|d| (d.clone(), None)).collect::<Vec<_>>()).1.unwrap();
        assert_eq!(s.list_trash().len(), 10);
        SYNCED_WRITES.with(|n| n.set(0));
        s.purge_many(&ids).unwrap();
        let shards = ids.iter().map(|id| meta::shard_of(id)).collect::<HashSet<_>>().len();
        assert!(SYNCED_WRITES.with(|n| n.get()) <= shards, "each shard once");
        assert!(s.list_trash().is_empty());
        let left = crate::vfs::read_dir(s.trash_dir()).unwrap().filter_map(|e| e.ok()).filter(|e| !e.file_name().to_string_lossy().starts_with("index.json")).count();
        assert_eq!(left, 0);
        assert!(lib.open().list_trash().is_empty());
    }

    /// The background writer reads back only files that changed.
    #[test]
    fn tree_writer_lists_from_the_parsed_documents() {
        let lib = Lib::new("writerlist");
        let mut s = lib.open();
        let ids: Vec<String> = (0..5).map(|i| s.create_from(&doc(&format!("页 {i}"))).unwrap()).collect();
        let _ = s.list();
        let mut w = s.tree_writer();
        DOC_READS.with(|n| n.set(0));
        w.delete(&ids[0], None).unwrap();
        let listed = w.list();
        assert_eq!(listed.len(), 4);
        assert!(listed.iter().all(|e| e.title.starts_with("页")));
        assert_eq!(DOC_READS.with(|n| n.get()), 0);
    }

    /// A first listing off the main thread: counted as it goes, and what it
    /// read is taken over, so the main store reads no document again.
    #[test]
    fn a_listing_elsewhere_is_counted_and_taken_over() {
        let lib = Lib::new("adopt");
        let mut s = lib.open();
        for i in 0..6 { s.create_from(&doc(&format!("页 {i}"))).unwrap(); }
        let mut s = lib.open();
        // Not parsing: listed by their names, nothing read.
        DOC_READS.with(|n| n.set(0));
        assert_eq!(s.list_with(None, false).len(), 6);
        assert_eq!(DOC_READS.with(|n| n.get()), 0);
        let mut w = s.tree_writer();
        let progress = ListProgress::default();
        let listed = w.list_with(Some(&progress), true);
        assert_eq!((progress.done.load(std::sync::atomic::Ordering::Relaxed), progress.total.load(std::sync::atomic::Ordering::Relaxed)), (6, 6));
        assert!(listed.iter().all(|e| e.title.starts_with("页")));
        // Written meanwhile on the main store: still there after taking over.
        let late = s.create_from(&doc("晚到")).unwrap();
        s.adopt(w);
        DOC_READS.with(|n| n.set(0));
        let now = s.list();
        assert_eq!(DOC_READS.with(|n| n.get()), 1, "only the page written meanwhile");
        assert_eq!(now.len(), 7);
        assert!(now.iter().any(|e| e.id == late && e.title == "晚到"));
        // What the writer read and wrote stands: numbering pages there, the main store reads nothing again.
        let mut w = s.tree_writer();
        let _ = w.list();
        assert!(w.initialize_manual_order().unwrap());
        let _ = w.list();
        s.adopt(w);
        let before = loads();
        assert!(!s.initialize_manual_order().unwrap());
        let _ = s.list();
        assert_eq!(loads(), before, "nothing read again");
    }

    #[test]
    fn unreadable_trash_folder_keeps_its_index() {
        let lib = Lib::new("trashdir");
        // FORMAT 5 keeps the 回收站 in `trash/index.json`; 6 keeps its records in the device files.
        STAY_5.with(|k| k.set(true));
        let mut s = lib.open();
        STAY_5.with(|k| k.set(false));
        let a = s.create_from(&doc("一")).unwrap();
        s.delete(&a, None).unwrap();
        // Swap the folder for a file: read_dir fails.
        let trash = lib.0.join(".quiet/trash");
        let index = crate::vfs::read(trash.join("index.json")).unwrap();
        crate::vfs::rename(&trash, lib.0.join(".quiet/trash-away")).unwrap();
        crate::vfs::write(&trash, b"").unwrap();
        let mut s = Store { file_changes: vec![], top_dir: lib.0.clone(), index: HashMap::new(), foreign: DiskIndex::new(), flat: false, migrated: None, trash: serde_json::from_slice(&index).unwrap(), warnings: vec![], loaded: Default::default(), copied: Default::default(), parsed: HashMap::new(), checked: HashMap::new(), writes: Default::default(), batch: Default::default(), view: Default::default(), v5: false, me: device(), clock: Default::default(), seen: Default::default(), merged: Default::default(), key: library_key(&lib.0), place_by_hand: true, v6: false, seen_trash: Default::default(), purging: Default::default(), synced: Default::default(), name_later: Default::default() , here: Default::default() , clean_due: None };
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
        crate::vfs::create_dir(with_suffix(&s.path(&id), ".tmp")).unwrap();
        s.save(&id, &doc("新")).unwrap();
        assert_eq!(s.load(&id).unwrap().title.text, "新");
    }

    /// A library in the old layout (FORMAT 3 and older), written by hand.
    struct Old<'a> { lib: &'a Lib, format: u32, index: HashMap<String, Meta>, trash: HashMap<String, Trashed> }

    impl<'a> Old<'a> {
        fn new(lib: &'a Lib, format: u32) -> Self {
            crate::vfs::create_dir_all(lib.0.join("docs")).unwrap();
            Old { lib, format, index: HashMap::new(), trash: HashMap::new() }
        }
        fn put(&mut self, d: &Document, meta: Meta) -> String {
            let id = uuid::Uuid::new_v4().to_string();
            crate::vfs::write(self.lib.0.join("docs").join(format!("{id}.xml")), docxml::serialize(d)).unwrap();
            self.index.insert(id.clone(), Meta { created: meta.created.max(1), modified: 1, ..meta });
            id
        }
        fn doc(&mut self, title: &str, meta: Meta) -> String { self.put(&doc(title), meta) }
        fn raw(&mut self, xml: &str, meta: Meta) -> String {
            let id = uuid::Uuid::new_v4().to_string();
            crate::vfs::write(self.lib.0.join("docs").join(format!("{id}.xml")), xml).unwrap();
            self.index.insert(id.clone(), Meta { created: 1, modified: 1, ..meta });
            id
        }
        fn md(&mut self, name: &str, body: &str, meta: Meta) -> String {
            let id = uuid::Uuid::new_v4().to_string();
            crate::vfs::write(self.lib.0.join("docs").join(format!("{id}.md")), body).unwrap();
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
            crate::vfs::create_dir_all(self.lib.0.join("trash")).unwrap();
            crate::vfs::write(self.lib.0.join("trash").join(format!("{id}.xml")), docxml::serialize(&doc(title))).unwrap();
            self.trash.insert(id.clone(), Trashed { meta: Meta { created: 1, modified: 1, ..Default::default() }, deleted: now(), root: None });
            id
        }
        fn write(&self) {
            crate::vfs::write(self.lib.0.join("index.json"), serde_json::to_vec(&self.index).unwrap()).unwrap();
            crate::vfs::create_dir_all(self.lib.0.join("trash")).unwrap();
            crate::vfs::write(self.lib.0.join("trash/index.json"), serde_json::to_vec(&self.trash).unwrap()).unwrap();
            crate::vfs::write(self.lib.0.join("format.json"), format!("{{\"format\": {}}}", self.format)).unwrap();
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
        crate::vfs::create_dir_all(&outside.0).unwrap();
        crate::vfs::write(outside.0.join("外面.md"), "外面的笔记\n").unwrap();
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
        let local_file = outside.0.fs_canonicalize().unwrap().join("外面.md");
        old.index.insert(local.clone(), Meta { created: 1, modified: 1, markdown: true, path: Some(local_file.to_string_lossy().into()), opened: 5, ..Default::default() });
        // An entry whose file is gone is dropped.
        old.index.insert(uuid::Uuid::new_v4().to_string(), Meta { created: 1, modified: 1, ..Default::default() });
        let trashed = old.trashed("删掉的");
        // Another computer's 本机文件: asked before moving.
        let away = uuid::Uuid::new_v4().to_string();
        old.index.insert(away.clone(), Meta { created: 1, modified: 1, markdown: true, path: Some("/no/such/notes.md".into()), ..Default::default() });
        old.write();
        crate::vfs::create_dir_all(lib.0.join("assets")).unwrap();
        crate::vfs::write(lib.0.join("assets/p.png"), b"png").unwrap();
        crate::vfs::create_dir_all(lib.0.join("themes")).unwrap();
        crate::vfs::write(lib.0.join("themes/mine.json"), b"{}").unwrap();
        crate::vfs::write(lib.0.join("preferences.json"), b"{\"a\":1}").unwrap();
        crate::vfs::write(lib.0.join("settings.json"), b"{\"window_size\":[800.0,600.0],\"sidebar_width\":250.0}").unwrap();
        // What the old layout says, to compare with.
        let before: HashMap<String, (String, String)> = crate::vfs::read_dir(lib.0.join("docs")).unwrap().flatten().map(|e| {
            let p = e.path();
            let id = p.file_stem().unwrap().to_string_lossy().into_owned();
            let text = crate::vfs::read_to_string(&p).unwrap();
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
        assert_eq!(folders_of(&list, &pic), ["同名 (2)"]);
        assert!(folders_of(&list, &top).is_empty() && folders_of(&list, &con).is_empty());
        assert_eq!(by[same1.as_str()].title, "同名");
        // Files named by title.
        for (id, rel) in [(&w1, "工作/周报.xml"), (&w2, "工作/周报 (2).xml"), (&top, "周报.xml"), (&odd, "工作/子/x：y？＜z＞.xml"), (&empty, "a／b/未命名文档.xml"),
            (&con, "CON_.xml"), (&dot, "孤儿/hidden.xml"), (&md, "工作/子/深/随手记.md"), (&index_doc, "索引.xml"), (&back, "工作/子/深/回链.xml"), (&pic, "同名 (2)/图.xml")] {
            assert_eq!(s.file_path(id).unwrap(), s.abs(rel), "{rel}");
            assert!(s.abs(rel).fs_is_file(), "{rel}");
        }
        // Links: the same targets, written as relative paths.
        assert_eq!(targets(by[index_doc.as_str()]), { let mut t = vec![w1.clone(), md.clone(), "gone-id".into()]; t.sort(); t });
        assert_eq!(targets(by[back.as_str()]), { let mut t = vec![index_doc.clone(), w2.clone()]; t.sort(); t });
        let text = crate::vfs::read_to_string(s.abs("索引.xml")).unwrap();
        assert!(text.contains("href=\"工作/周报.xml\"") && text.contains("href=\"工作/子/深/随手记.md\"") && text.contains("doc-id=\"gone-id\""), "{text}");
        let text = crate::vfs::read_to_string(s.abs("工作/子/深/回链.xml")).unwrap();
        assert!(text.contains("href=\"../../../索引.xml\"") && text.contains("href=\"../../周报 (2).xml\""), "{text}");
        // No id is written in any document.
        for id in before.keys() { for e in &list { if let Some(p) = s.file_path(&e.id) && !e.meta.local() { assert!(!crate::vfs::read_to_string(p).unwrap().contains(id.as_str())); } } }
        // Images: `.assets/`, the sources with them.
        assert!(lib.0.join(".assets/p.png").fs_is_file() && !lib.0.join("assets").fs_exists());
        assert_eq!(crate::vfs::read_to_string(s.abs("同名 (2)/图.xml")).unwrap(), "<title>图</title>\n<img src=\".assets/p.png\"/>\n<p>=\"assets/x\"</p>\n");
        assert_eq!(crate::vfs::read_to_string(s.abs("工作/子/深/随手记.md")).unwrap(), "看图 ![](.assets/p.png)\n");
        // Attached and local information.
        assert!(by[w1.as_str()].meta.pinned);
        assert_eq!(by[w1.as_str()].meta.opened, 77);
        let index = lib.meta_text();
        assert!(!index.contains("opened") && !index.contains("外面") && index.contains("\"工作/子/\""));
        assert!(by[local.as_str()].meta.local());
        assert_eq!(s.load(&local).unwrap().plain_text(), "外面的笔记");
        assert!(local_file.fs_is_file());
        assert_eq!(s.settings().window_size, Some([800., 600.]));
        assert_eq!(s.settings().sidebar_width, Some(250.));
        assert!(lib.0.join(".quiet/preferences.json").fs_is_file() && lib.0.join(".quiet/themes/mine.json").fs_is_file());
        // What the user sees: their folders and notes only.
        let mut top_names: Vec<String> = crate::vfs::read_dir(&lib.0).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        top_names.sort();
        assert_eq!(top_names, [".assets", ".quiet", "CON_.xml", "a／b", "同名", "同名 (2)", "周报.xml", "孤儿", "工作", "索引.xml"]);
        // The old layout, whole, as a backup.
        let backup = lib.0.join(".quiet").join(migrate::BACKUP);
        for id in before.keys() { assert!(backup.join("docs").join(format!("{id}.xml")).fs_is_file() || backup.join("docs").join(format!("{id}.md")).fs_is_file()); }
        assert!(backup.join("index.json").fs_is_file() && backup.join("format.json").fs_is_file() && backup.join("trash/index.json").fs_is_file());
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
            let bytes = crate::vfs::read(&p).unwrap();
            let d = s.load_for_edit(&e.id, false).unwrap();
            s.save(&e.id, &d).unwrap();
            assert_eq!(s.file_path(&e.id).unwrap(), p, "{}", e.title);
            // (图 was written by hand, not in the canonical form: stable from its first save.)
            if e.id == pic { let d = s.load_for_edit(&e.id, false).unwrap(); let once = crate::vfs::read(&p).unwrap(); s.save(&e.id, &d).unwrap(); assert_eq!(crate::vfs::read(&p).unwrap(), once); continue; }
            assert_eq!(crate::vfs::read(&p).unwrap(), bytes, "{}", e.title);
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
        crate::vfs::create_dir_all(lib.0.join("assets")).unwrap();
        let snapshot = |root: &Path| { let mut v = vec![]; fn walk(d: &Path, v: &mut Vec<(PathBuf, Vec<u8>)>) { for e in crate::vfs::read_dir(d).unwrap().flatten() { let p = e.path(); if p.fs_is_dir() { v.push((p.clone(), vec![])); walk(&p, v); } else { v.push((p.clone(), crate::vfs::read(&p).unwrap())); } } } walk(root, &mut v); v.sort(); v };
        let before: Vec<_> = snapshot(&lib.0).into_iter().filter(|(p, _)| !p.starts_with(lib.0.join(".quiet")) && !p.ends_with(".lock")).collect();
        let err = Store::migrate(&lib.0).expect_err("the injected failure");
        assert!(err.to_string().contains("已保持原样"), "{err}");
        let after: Vec<_> = snapshot(&lib.0).into_iter().filter(|(p, _)| !p.starts_with(lib.0.join(".quiet")) && !p.ends_with(".lock")).collect();
        assert_eq!(after, before);
        assert!(!lib.0.join(".quiet/format.json").fs_exists() && !lib.0.join(".quiet/index.json").fs_exists());
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
        crate::vfs::create_dir_all(crashed.0.join(".quiet")).unwrap();
        crate::vfs::create_dir_all(crashed.0.join("半路")).unwrap();
        crate::vfs::write(crashed.0.join("甲.xml"), b"half").unwrap();
        crate::vfs::write(crashed.0.join(".quiet/migration.json"), b"{\"files\":[\"\xe7\x94\xb2.xml\"],\"dirs\":[\"\xe5\x8d\x8a\xe8\xb7\xaf\"],\"internal\":[],\"assets\":false}").unwrap();
        let mut s = crashed.migrated();
        assert!(!crashed.0.join("半路").fs_exists());
        assert_eq!(s.load(&a).unwrap().title.text, "甲");
        assert_eq!(s.file_path(&a).unwrap(), s.abs("甲.xml"));
        assert_eq!(s.list().len(), 1);
        // Committed but not cleaned up: the old layout is set aside on open.
        crate::vfs::write(crashed.0.join("format.json"), b"{\"format\": 3}").unwrap();
        crate::vfs::create_dir_all(crashed.0.join("docs")).unwrap();
        drop(s);
        let mut s = crashed.open();
        assert!(!crashed.0.join("format.json").fs_exists() && !crashed.0.join("docs").fs_exists());
        assert!(crashed.0.join(".quiet").join(migrate::BACKUP).join("format.json.2").fs_is_file());
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
        assert!(crate::vfs::read_to_string(s.abs("甲.xml")).unwrap().contains("href=\"乙.xml\""));
        s.load_for_edit(&a, false).unwrap();
        let f = s.create_folder("夹", None).unwrap();
        assert!(lib.0.join("夹").fs_is_dir());
        s.set_parent(&b, Some(&f)).unwrap();
        assert!(s.abs("夹/乙.xml").fs_is_file() && !s.abs("乙.xml").fs_exists());
        assert!(crate::vfs::read_to_string(s.abs("甲.xml")).unwrap().contains("href=\"夹/乙.xml\""));
        assert!(crate::vfs::read_to_string(s.abs("夹/乙.xml")).unwrap().contains("href=\"../甲.xml\""));
        // The open editor of 甲 saves without a conflict.
        s.save(&a, &a_doc).unwrap();
        s.rename(&f, "新夹").unwrap();
        assert!(crate::vfs::read_to_string(s.abs("甲.xml")).unwrap().contains("href=\"新夹/乙.xml\""));
        // A title typed in the editor renames the file; a taken name is numbered.
        s.save(&a, &linking("丙", &[(&b, "乙")])).unwrap();
        assert!(s.abs("丙.xml").fs_is_file() && !s.abs("甲.xml").fs_exists());
        assert!(crate::vfs::read_to_string(s.abs("新夹/乙.xml")).unwrap().contains("href=\"../丙.xml\""));
        let c = s.create_from(&doc("丙")).unwrap();
        assert_eq!(s.file_path(&c).unwrap(), s.abs("丙 (2).xml"));
        s.save(&c, &doc("丙")).unwrap();
        assert_eq!(s.file_path(&c).unwrap(), s.abs("丙 (2).xml"), "a numbered name fits its title");
        let list = s.list();
        assert_eq!(targets(list.iter().find(|e| e.id == b).unwrap()), std::slice::from_ref(&a));
        // Undo of the move puts the file back, links too.
        let journal = s.track_file_changes();
        s.set_parent(&b, None).unwrap();
        let changes = journal.take();
        assert!(s.abs("乙.xml").fs_is_file());
        s.apply_file_history(&changes, false).unwrap();
        assert!(s.abs("新夹/乙.xml").fs_is_file() && !s.abs("乙.xml").fs_exists());
        assert!(crate::vfs::read_to_string(s.abs("丙.xml")).unwrap().contains("href=\"新夹/乙.xml\""));
    }

    /// 重命名 made off the window's thread (`src/tree_writes.rs`): a writer renames the folder
    /// by id while the window's library still holds the old paths. Taken over, the library
    /// opens and saves what is inside where it now is; not taken over, a save still lands
    /// there, as every write reads the index again under the lock.
    #[test]
    fn a_writers_rename_is_followed_by_id() {
        let lib = Lib::new("rename-writer");
        let mut s = lib.open();
        let f = s.create_folder("项目", None).unwrap();
        let a = s.create_from_in(&doc_with("甲", "一"), Some(&f)).unwrap();
        let b = s.create_from_in(&doc_with("乙", "二"), Some(&f)).unwrap();
        s.list();
        let mut open = s.load_for_edit(&a, false).unwrap();
        // Renamed twice before the window hears of either: each writer finds the folder by id.
        let (n, mut w) = (s.writes(), s.tree_writer());
        let journal = w.track_file_changes();
        w.rename(&f, "改过").unwrap();
        assert_eq!(journal.take().len(), 1);
        let mut w2 = s.tree_writer();
        w2.rename(&f, "再改").unwrap();
        let listed = w2.list();
        assert!(lib.0.join("再改/甲.xml").is_file() && !lib.0.join("项目").exists() && !lib.0.join("改过").exists());
        assert_eq!(listed.iter().find(|e| e.id == f).unwrap().title, "再改");
        // The window's library still has the old path …
        assert_eq!(s.index[&a].rel, "项目/甲.xml");
        assert!(s.load(&b).is_err());
        // … yet a save from it lands in the renamed folder (the index is read again under the lock).
        open.blocks.push(crate::model::Block::text("改名途中写的"));
        s.save_editor(&a, &open).unwrap();
        assert!(crate::vfs::read_to_string(lib.0.join("再改/甲.xml")).unwrap().contains("改名途中写的"));
        assert!(!lib.0.join("项目").exists());
        // That save counted as a write: the writer is not taken over (`SharedStore::adopt_written` lists instead).
        assert_ne!(s.writes(), n);
        let n = s.writes();
        // Nothing written here since: the next writer is taken over, and a listing begun before it is not.
        let (mut early, mut w3) = (s.tree_writer(), s.tree_writer());
        early.list();
        w3.rename(&f, "定稿").unwrap();
        w3.list();
        assert_eq!(s.writes(), n);
        s.adopt(w3);
        assert_ne!(s.writes(), n, "a writer from before the rename could still be taken over");
        assert_eq!(s.index[&b].rel, "定稿/乙.xml");
        assert_eq!(s.load(&b).unwrap().title.text, "乙");
        drop(early);
    }

    /// The background queue's moves and renames (`tree_writer`, taken over
    /// with `adopt`) land in this device's shards: read afresh from the disk,
    /// the library has them, and no whole device file comes back.
    #[test]
    fn a_writers_moves_and_renames_land_in_the_shards() {
        let lib = Lib::new("writer-shards");
        let mut s = lib.open();
        let f = s.create_folder("项目", None).unwrap();
        let g = s.create_folder("归档", None).unwrap();
        let a = s.create_from_in(&doc_with("甲", "一"), Some(&f)).unwrap();
        s.list();
        let mut w = s.tree_writer();
        w.rename(&f, "改过").unwrap();
        w.move_in_tree(&a, Some(&g), None).unwrap();
        w.list();
        s.adopt(w);
        assert_eq!(s.index[&a].rel, "归档/甲.xml");
        let d = lib.0.join(".quiet/meta");
        let me = s.me.id.clone();
        assert!(d.join(&me).fs_is_dir() && !d.join(format!("{me}.json")).fs_exists());
        let mut warnings = vec![];
        let m = meta::load(&lib.0, &me, &mut warnings);
        assert_eq!(m.get(&a, "path").and_then(|v| v.as_str()), Some("归档/甲.xml"));
        assert_eq!(m.get(&f, "path").and_then(|v| v.as_str()), Some("改过/"));
        let mut fresh = lib.open();
        let listed = fresh.list();
        assert_eq!(listed.iter().find(|e| e.id == f).unwrap().title, "改过");
        assert_eq!(listed.iter().find(|e| e.id == a).unwrap().meta.parent.as_deref(), Some(g.as_str()));
    }

    /// A rename that cannot be made (the folder above is read-only) leaves the folder, its
    /// pages and the index as they were: the window puts the old name back from this.
    #[cfg(unix)]
    #[test]
    fn a_failed_rename_changes_nothing() {
        use std::os::unix::fs::PermissionsExt;
        let lib = Lib::new("rename-fails");
        let mut s = lib.open();
        let outer = s.create_folder("外", None).unwrap();
        let inner = s.create_folder("内", Some(&outer)).unwrap();
        let a = s.create_from_in(&doc("甲"), Some(&inner)).unwrap();
        s.list();
        let dir = lib.0.join("外");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let mut w = s.tree_writer();
        let journal = w.track_file_changes();
        let r = w.rename(&inner, "新名");
        let listed = w.list();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(r.is_err());
        assert!(journal.take().is_empty());
        assert!(lib.0.join("外/内/甲.xml").is_file() && !lib.0.join("外/新名").exists());
        assert_eq!(listed.iter().find(|e| e.id == inner).unwrap().title, "内");
        assert_eq!(w.index[&a].rel, "外/内/甲.xml");
    }

    /// On Windows a folder holding a file another program has open (no share
    /// for delete) cannot be renamed: the rename fails and leaves everything as
    /// it was, as when its folder is read-only.
    #[cfg(windows)]
    #[test]
    fn a_rename_held_up_by_an_open_file_changes_nothing() {
        use std::os::windows::fs::OpenOptionsExt;
        let lib = Lib::new("rename-held");
        let mut s = lib.open();
        let outer = s.create_folder("外", None).unwrap();
        let inner = s.create_folder("内", Some(&outer)).unwrap();
        let a = s.create_from_in(&doc("甲"), Some(&inner)).unwrap();
        s.list();
        let held = std::fs::OpenOptions::new().read(true).share_mode(0).open(lib.0.join("外/内/甲.xml")).unwrap();
        let mut w = s.tree_writer();
        let journal = w.track_file_changes();
        let r = w.rename(&inner, "新名");
        let listed = w.list();
        drop(held);
        assert!(r.is_err(), "a folder with a file held open was renamed");
        assert!(journal.take().is_empty());
        assert!(lib.0.join("外/内/甲.xml").is_file() && !lib.0.join("外/新名").exists());
        assert_eq!(listed.iter().find(|e| e.id == inner).unwrap().title, "内");
        assert_eq!(w.index[&a].rel, "外/内/甲.xml");
        // Let go, the same rename goes through.
        let mut w = s.tree_writer();
        w.rename(&inner, "新名").unwrap();
        assert!(lib.0.join("外/新名/甲.xml").is_file());
    }

    /// The window's save writes a new title but leaves the file's name (and the links to it)
    /// A page renamed by a `tree_writer` while the window's library walks its folders for
    /// `list` (as on a slow machine: the walk sees the old file, the index under the lock the
    /// new one) keeps its id: `list` walks again rather than drop the page and list its new
    /// file as a page it has never seen.
    #[test]
    fn a_rename_made_while_listing_keeps_the_page() {
        let lib = Lib::new("rename-while-listing");
        let mut s = lib.open();
        let a = s.create_from(&doc("未命名")).unwrap();
        let b = s.create_from(&doc("别的")).unwrap();
        s.list();
        let mut d = s.load_for_edit(&a, false).unwrap();
        d.title = crate::model::RichText::plain("自检");
        assert!(s.save_editor_naming_later(&a, &d).unwrap().1);
        let mut w = s.tree_writer();
        let renamed = a.clone();
        AFTER_WALK.with(|h| *h.borrow_mut() = Some(Box::new(move || w.follow_name(&renamed, "自检").unwrap())));
        let listed = s.list();
        assert!(AFTER_WALK.with(|h| h.borrow().is_none()), "the write did not run inside list");
        assert!(lib.0.join("自检.xml").is_file() && !lib.0.join("未命名.xml").exists());
        let ids: Vec<&str> = listed.iter().filter(|e| e.meta.folder.is_none()).map(|e| e.id.as_str()).collect();
        assert_eq!(ids.len(), 2, "a page was dropped or listed twice: {ids:?}");
        assert!(ids.contains(&a.as_str()) && ids.contains(&b.as_str()), "the renamed page lost its id: {ids:?}");
        assert_eq!(s.index[&a].rel, "自检.xml");
        assert!(s.file_path(&a).is_some_and(|p| p.is_file()));
    }

    /// to `follow_name`, which reads the title from the file: a later save is never undone.
    #[test]
    fn the_editors_save_names_the_file_later() {
        let lib = Lib::new("name-later");
        let mut s = lib.open();
        let a = s.create_from(&doc("甲")).unwrap();
        let mut other = doc("乙");
        other.blocks.push(crate::model::Block::text(""));
        other.blocks[0].text.push("甲", crate::model::Marks { link: Some("甲.xml".into()), ..Default::default() });
        let b = s.create_from(&other).unwrap();
        s.list();
        let mut d = s.load_for_edit(&a, false).unwrap();
        // The same title: nothing to follow.
        d.blocks.push(crate::model::Block::text("正文"));
        assert!(!s.save_editor_naming_later(&a, &d).unwrap().1);
        s.list();
        // A new title: written, the file stays where it is.
        d.title = crate::model::RichText::plain("新题");
        assert!(s.save_editor_naming_later(&a, &d).unwrap().1);
        assert!(lib.0.join("甲.xml").is_file() && !lib.0.join("新题.xml").exists());
        assert!(crate::vfs::read_to_string(lib.0.join("甲.xml")).unwrap().contains("新题"));
        // Typed on and saved again before the name followed: the name is the title on disk, not the one queued.
        d.title = crate::model::RichText::plain("新题二");
        assert!(s.save_editor_naming_later(&a, &d).unwrap().1);
        let mut w = s.tree_writer();
        w.follow_name(&a, "新题").unwrap();
        assert!(lib.0.join("新题二.xml").is_file() && !lib.0.join("甲.xml").exists() && !lib.0.join("新题.xml").exists());
        assert_eq!(w.load(&b).unwrap().links(), ["新题二.xml"]);
        // Followed again (the window made it itself meanwhile): nothing changes.
        w.follow_name(&a, "新题二").unwrap();
        assert!(lib.0.join("新题二.xml").is_file());
        // The window's library saves on at the new name without being told.
        d.blocks.push(crate::model::Block::text("改名后"));
        assert!(!s.save_editor_naming_later(&a, &d).unwrap().1);
        assert!(crate::vfs::read_to_string(lib.0.join("新题二.xml")).unwrap().contains("改名后"));
        // A Markdown file's title is only its name: the save leaves it, `follow_name` is given it.
        crate::vfs::write(lib.0.join("笔记.md"), "内容\n").unwrap();
        s.list();
        let m = s.index.iter().find(|(_, m)| m.rel == "笔记.md").map(|(k, _)| k.clone()).unwrap();
        let mut md = s.load_for_edit(&m, false).unwrap();
        md.title = crate::model::RichText::plain("改名笔记");
        assert!(s.save_editor_naming_later(&m, &md).unwrap().1);
        assert!(lib.0.join("笔记.md").is_file());
        s.follow_name(&m, "改名笔记").unwrap();
        assert!(lib.0.join("改名笔记.md").is_file() && !lib.0.join("笔记.md").exists());
        // Any other save follows the title itself, as before.
        d.title = crate::model::RichText::plain("末题");
        s.save_editor(&a, &d).unwrap();
        assert!(lib.0.join("末题.xml").is_file());
    }

    /// What every page of the library points at, by the name of the page: the documents it
    /// mentions and links to, and its pictures. Every one has to be there: a mention names a
    /// page of the library, a link's and a picture's file is on disk.
    fn points_at(s: &Store) -> BTreeMap<String, Vec<String>> {
        let name = |rel: &str| base_name(rel).to_string();
        let mut out = BTreeMap::new();
        for (id, m) in s.index.iter().filter(|(_, m)| m.folder.is_none()) {
            let doc = s.load(id).unwrap();
            let dir = parent_rel(&m.rel);
            let mut at: Vec<String> = vec![];
            for l in doc.doc_links() { at.push(format!("@{}", name(&s.index.get(&l.target).unwrap_or_else(|| panic!("{}: a mention of nothing", m.rel)).rel))); }
            for l in doc.links().iter().filter(|l| layout::file_link(l)) {
                let to = layout::link_path(dir, l).unwrap();
                assert!(s.abs(&to).fs_is_file(), "{}: link {l} finds no file at {to}", m.rel);
                at.push(format!(">{}", name(&to)));
            }
            for src in doc.asset_sources() {
                assert!(Path::new(&src).fs_is_file(), "{}: picture {src} is not there", m.rel);
                at.push(format!("!{}", name(&src.replace('\\', "/"))));
            }
            at.sort();
            out.insert(name(&m.rel), at);
        }
        out
    }

    /// Renaming or moving a folder, or a page out of one, keeps every mention, link and
    /// picture pointing at what it pointed at (docs/FORMAT.md: a mention and a link are
    /// relative to the file, a Markdown file's pictures too, a DocxXML page's to the library),
    /// for files the app has never saved as well. What is inside a folder that moves whole
    /// keeps its depth: only files with a link across its edge are written.
    #[test]
    fn renames_and_moves_keep_every_link_and_picture() {
        let lib = Lib::new("links-round-trip");
        let w = |rel: &str, text: &str| { let p = lib.0.join(rel); crate::vfs::create_dir_all(p.parent().unwrap()).unwrap(); crate::vfs::write(p, text).unwrap(); };
        let cite = |href: &str| format!("<cite type=\"doc\" href=\"{href}\" file-type=\"docx\">x</cite>");
        w(".assets/a.png", "png");
        w("P/img/p.png", "png");
        w("P/A.xml", &format!("<title>A</title>\n<p>{} {} <a href=\"B.xml\">b</a> <a href=\"sub/C.md\">c</a> <a href=\"../Out.md\">o</a></p>\n<img src=\".assets/a.png\"/>\n<img src=\"P/img/p.png\"/>\n", cite("B.xml"), cite("../Out.xml")));
        w("P/B.xml", "<title>B</title>\n<p>b</p>\n");
        w("P/N.md", "![p](img/p.png)\n\n![a](../.assets/a.png)\n\n[B](B.xml) [C](sub/C.md) [Out](../Out.xml)\n");
        w("P/sub/C.md", "![p](../img/p.png)\n\n![a](../../.assets/a.png)\n\n[A](../A.xml) [Out](../../Out.md)\n");
        w("Out.xml", &format!("<title>Out</title>\n<p>{} <a href=\"P/N.md\">n</a></p>\n<img src=\".assets/a.png\"/>\n", cite("P/A.xml")));
        w("Out.md", "![p](P/img/p.png)\n\n[N](P/N.md) [C](P/sub/C.md)\n");
        w("Q/Keep.md", "keep\n");
        let mut s = lib.open();
        s.list();
        let id = |rel: &str, s: &Store| s.index.iter().find(|(_, m)| m.rel == rel).map(|(k, _)| k.clone()).unwrap_or_else(|| panic!("no {rel}"));
        let before = points_at(&s);
        assert_eq!(before["A.xml"], ["!a.png", "!p.png", ">B.xml", ">C.md", ">Out.md", "@B.xml", "@Out.xml"]);
        assert_eq!(before["N.md"], ["!a.png", "!p.png", ">B.xml", ">C.md", ">Out.xml"]);
        assert_eq!(before["C.md"], ["!a.png", "!p.png", ">A.xml", ">Out.md"]);
        assert_eq!(before["Out.xml"], ["!a.png", ">N.md", "@A.xml"]);
        assert_eq!(before["Out.md"], ["!p.png", ">C.md", ">N.md"]);
        let text = |rel: &str| crate::vfs::read_to_string(lib.0.join(rel)).unwrap();
        let inside: Vec<(&str, String)> = ["B.xml", "N.md", "sub/C.md"].into_iter().map(|f| (f, text(&format!("P/{f}")))).collect();
        // 重命名: the folder keeps its place, so nothing in it is written but the page whose picture is named from the library's top;
        // outside it, the pages pointing into it.
        s.rename(&id("P", &s), "R").unwrap();
        for (f, was) in &inside { assert_eq!(&text(&format!("R/{f}")), was, "{f} was rewritten by a rename of its folder"); }
        assert!(text("R/A.xml").contains("src=\"R/img/p.png\"") && text("Out.md").contains("(R/img/p.png)") && text("Out.xml").contains("href=\"R/A.xml\""));
        assert_eq!(points_at(&s), before);
        s.list();
        assert_eq!(points_at(&s), before);
        // 移动到 another folder, one level down: links across its edge get one `../` more, those inside stay.
        s.set_parent(&id("R", &s), Some(&id("Q", &s))).unwrap();
        assert_eq!(points_at(&s), before);
        assert_eq!(text("Q/R/B.xml"), inside[0].1);
        assert!(text("Q/R/N.md").contains("(img/p.png)") && text("Q/R/N.md").contains("(../../.assets/a.png)") && text("Q/R/N.md").contains("(../../Out.xml)"));
        assert!(text("Q/R/sub/C.md").contains("(../img/p.png)") && text("Q/R/sub/C.md").contains("(../A.xml)"));
        // A page moved out of its folder alone: its pictures and the pages beside it stay behind.
        s.list();
        s.set_parent(&id("Q/R/N.md", &s), None).unwrap();
        assert_eq!(points_at(&s), before);
        assert!(text("N.md").contains("(Q/R/img/p.png)") && text("N.md").contains("(.assets/a.png)") && text("N.md").contains("(Q/R/B.xml)"));
        s.set_parent(&id("Q/R/A.xml", &s), None).unwrap();
        assert_eq!(points_at(&s), before);
        // … and back in, then the folder back to the top under its first name: as it began.
        let r = id("Q/R", &s);
        s.set_parent(&id("N.md", &s), Some(&r)).unwrap();
        s.set_parent(&id("A.xml", &s), Some(&r)).unwrap();
        s.set_parent(&r, None).unwrap();
        s.rename(&r, "P").unwrap();
        assert_eq!(points_at(&s), before);
        for (f, was) in &inside { assert_eq!(&text(&format!("P/{f}")), was, "{f} did not come back as it was"); }
        assert_eq!(text("Out.md"), "![p](P/img/p.png)\n\n[N](P/N.md) [C](P/sub/C.md)\n");
    }

    /// Plain links to files (#67), in DocxXML and in Markdown, follow a move
    /// of the file, of what they point at or of a folder holding either; an
    /// open editor takes the rewritten links before it saves.
    #[test]
    fn moves_keep_file_links() {
        let lib = Lib::new("file-links");
        let mut s = lib.open();
        let linked = |text: &str, url: &str| { let mut b = crate::model::Block::text(""); b.text.push(text, crate::model::Marks { link: Some(url.into()), ..Default::default() }); b };
        let mut x = doc("甲");
        x.blocks = vec![linked("报告", "附件/报告%201.pdf"), linked("乙", "./乙.md#小节"), linked("网页", "https://example.com/a.pdf"), linked("本页", "#标题")];
        let a = s.create_from(&x).unwrap();
        s.load_for_edit(&a, false).unwrap();
        crate::vfs::create_dir_all(lib.0.join("附件")).unwrap();
        crate::vfs::write(lib.0.join("附件/报告 1.pdf"), b"%PDF").unwrap();
        // A folder shows once it holds a page (`Scan::empty`).
        crate::vfs::write(lib.0.join("附件/说明.md"), "附件\n").unwrap();
        let md = "---\ntags: [x]\n---\n见 [甲](%E7%94%B2.xml)  和 [报告](附件/报告%201.pdf)\n\n* 原样   保留\n";
        crate::vfs::write(lib.0.join("乙.md"), md).unwrap();
        let entries = s.list();
        let id = |rel: &str, s: &Store| s.index.iter().find(|(_, m)| m.rel == rel).map(|(k, _)| k.clone()).unwrap();
        let (b, files) = (id("乙.md", &s), id("附件", &s));
        assert!(entries.iter().any(|e| e.id == b));
        let f = s.create_folder("夹", None).unwrap();
        s.set_parent(&a, Some(&f)).unwrap();
        let links = |s: &Store, id: &str| s.load(id).unwrap().links();
        assert_eq!(links(&s, &a), ["../附件/报告%201.pdf", "../乙.md#小节", "https://example.com/a.pdf", "#标题"]);
        // Only the link changed in the Markdown file: front matter and the other lines as they were.
        assert_eq!(crate::vfs::read_to_string(s.abs("乙.md")).unwrap(), md.replace("%E7%94%B2.xml", "夹/甲.xml"));
        // The open editor of 甲 still has the old links: saving them as they are is refused …
        assert!(s.save(&a, &x).is_err());
        // … it follows them first, and then saves.
        let map = s.follow_moved_links(&a).unwrap();
        assert_eq!(map.len(), 2);
        x.map_links(&mut |l| map.get(l).cloned());
        s.save(&a, &x).unwrap();
        assert_eq!(links(&s, &a), ["../附件/报告%201.pdf", "../乙.md#小节", "https://example.com/a.pdf", "#标题"]);
        assert!(s.follow_moved_links(&a).is_none());
        // A folder holding the target moves: the links into it follow.
        s.set_parent(&files, Some(&f)).unwrap();
        assert!(s.abs("夹/附件/报告 1.pdf").fs_is_file());
        assert_eq!(links(&s, &a)[0], "附件/报告%201.pdf");
        assert_eq!(links(&s, &b), ["夹/甲.xml", "夹/附件/报告%201.pdf"]);
        // Renaming the folder holding both: links between them stay, the one from outside follows.
        s.rename(&f, "新夹").unwrap();
        assert_eq!(links(&s, &a)[..2], ["附件/报告%201.pdf", "../乙.md#小节"]);
        assert_eq!(links(&s, &b), ["新夹/甲.xml", "新夹/附件/报告%201.pdf"]);
    }

    #[test]
    fn file_link_paths() {
        assert_eq!(layout::link_path("工作", "../附件/周%20报.pdf#p2").as_deref(), Some("附件/周 报.pdf"));
        assert_eq!(layout::link_path("", "../外面/a.pdf").as_deref(), Some("../外面/a.pdf"));
        assert_eq!(layout::link_path("a", "https://x.y/z.pdf"), None);
        assert_eq!(layout::link_path("a", "#小节"), None);
        let none = |_: &str| None;
        // Out of the library: still the same file from the new place.
        assert_eq!(layout::rebase_link("甲", "乙/丙", "../../外面/a.pdf", &none).as_deref(), Some("../../../外面/a.pdf"));
        assert_eq!(layout::rebase_link("甲", "甲", "./a.pdf", &none), None);
        assert_eq!(layout::rebase_link("", "甲", "./a b.pdf?x#y", &none).as_deref(), Some("../a b.pdf?x#y"));
        let moved = |p: &str| (p == "a b.pdf").then(|| "夹/a b.pdf".to_string());
        assert_eq!(layout::rebase_link("", "", "./a%20b.pdf", &moved).as_deref(), Some("./夹/a%20b.pdf"));
    }

    /// #358: renaming 「AAA (2)」 back to 「AAA」 once 「AAA」 is gone takes the
    /// name; while it is taken the number stays as it is. 「AAA 2」 from older
    /// versions counts as numbered too (D8).
    #[test]
    fn renaming_back_to_a_freed_name_takes_it() {
        let lib = Lib::new("rename-freed");
        let mut s = lib.open();
        // The exact names in the folder, whatever the disk does with case.
        let names = |s: &Store| -> Vec<String> {
            let mut v: Vec<String> = crate::vfs::read_dir(&s.top_dir).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| !n.starts_with('.')).collect();
            v.sort();
            v
        };
        let file = |s: &Store, id: &str| s.index[id].rel.clone();
        // Folders.
        let a = s.create_folder("AAA", None).unwrap();
        let b = s.create_folder("AAA", None).unwrap();
        assert_eq!(file(&s, &b), "AAA (2)");
        s.rename(&b, "AAA").unwrap();
        assert_eq!(file(&s, &b), "AAA (2)", "taken: the number stays, not 「AAA (3)」");
        s.rename(&b, "AAA (2)").unwrap();
        assert_eq!(file(&s, &b), "AAA (2)", "unchanged");
        s.delete(&a, None).unwrap();
        s.rename(&b, "AAA").unwrap();
        assert_eq!(file(&s, &b), "AAA");
        assert_eq!(names(&s), ["AAA"]);
        s.rename(&b, "aaa").unwrap();
        assert_eq!(file(&s, &b), "aaa");
        assert_eq!(names(&s), ["aaa"], "a change of case renames the folder");
        // 「AAA 3」 next to a taken 「AAA」 and a free 「AAA (2)」 is not renumbered.
        let c = s.create_folder("AAA", None).unwrap();
        let d = s.create_folder("AAA 3", None).unwrap();
        assert_eq!(file(&s, &c), "AAA (2)");
        s.delete(&c, None).unwrap();
        s.rename(&d, "AAA").unwrap();
        assert_eq!(file(&s, &d), "AAA 3");
        s.delete(&b, None).unwrap();
        s.delete(&d, None).unwrap();
        // Documents, renamed from the list; 「BBB 2」 is numbered the old way.
        let e = s.create_from(&doc("BBB")).unwrap();
        let f = s.create_from(&doc("BBB 2")).unwrap();
        s.rename(&f, "BBB").unwrap();
        assert_eq!(file(&s, &f), "BBB 2.xml", "taken: the number stays");
        s.rename(&f, "BBB 2").unwrap();
        assert_eq!(file(&s, &f), "BBB 2.xml");
        s.delete(&e, None).unwrap();
        s.rename(&f, "BBB").unwrap();
        assert_eq!(file(&s, &f), "BBB.xml");
        s.rename(&f, "bbb").unwrap();
        assert_eq!(names(&s), ["bbb.xml"], "a change of case renames the file");
        // …and typed in the editor; a save of the same title leaves the number alone.
        let g = s.create_from(&doc("CCC")).unwrap();
        let h = s.create_from(&doc("CCC")).unwrap();
        assert_eq!(file(&s, &h), "CCC (2).xml");
        s.list();
        s.delete(&g, None).unwrap();
        s.save(&h, &doc("CCC")).unwrap();
        assert_eq!(file(&s, &h), "CCC (2).xml", "the title did not change");
        s.save(&h, &doc("CCC 2")).unwrap();
        s.list();
        s.save(&h, &doc("CCC")).unwrap();
        assert_eq!(file(&s, &h), "CCC.xml");
        assert!(s.abs("CCC.xml").fs_is_file() && !s.abs("CCC 2.xml").fs_exists() && !s.abs("CCC (2).xml").fs_exists());
    }

    /// A name that is its title (Markdown, folders) keeps its number when moved
    /// or restored: 「报告 (2).md」 is often just what the file is called.
    #[test]
    fn titles_that_look_numbered_survive_moves() {
        let lib = Lib::new("move-numbered");
        let mut s = lib.open();
        crate::vfs::write(lib.0.join("报告 (2).md"), "x\n").unwrap();
        crate::vfs::write(lib.0.join("第 2.md"), "x\n").unwrap();
        crate::vfs::create_dir_all(lib.0.join("照片 (2)")).unwrap();
        crate::vfs::write(lib.0.join("照片 (2)/说明.xml"), "<title>说明</title>\n").unwrap();
        let f = s.create_folder("夹", None).unwrap();
        let list = s.list();
        let ids: Vec<String> = list.iter().filter(|e| e.meta.markdown || e.title == "照片 (2)").map(|e| e.id.clone()).collect();
        for id in &ids { s.move_in_tree(id, Some(&f), None).unwrap(); }
        let rels = |s: &mut Store| { let mut r: Vec<String> = s.list().into_iter().filter(|e| ids.contains(&e.id)).map(|e| e.meta.rel).collect(); r.sort(); r };
        assert_eq!(rels(&mut s), ["夹/报告 (2).md", "夹/照片 (2)", "夹/第 2.md"]);
        let md = ids.iter().find(|id| s.index[*id].rel == "夹/报告 (2).md").unwrap().clone();
        s.delete(&md, None).unwrap();
        s.restore(&md).unwrap();
        assert_eq!(s.index[&md].rel, "夹/报告 (2).md");
    }

    /// D8: a 「 2」 an older version gave a clash keeps its name; new clashes get 「 (2)」.
    #[test]
    fn old_numbered_names_stay() {
        let lib = Lib::new("old-numbered");
        let mut s = lib.open();
        crate::vfs::write(lib.0.join("周报.xml"), "<title>周报</title>\n").unwrap();
        crate::vfs::write(lib.0.join("周报 2.xml"), "<title>周报</title>\n").unwrap();
        let old = s.list().into_iter().find(|e| e.meta.rel == "周报 2.xml").unwrap().id;
        s.save(&old, &doc("周报")).unwrap();
        s.list();
        assert_eq!(s.index[&old].rel, "周报 2.xml", "fits its title, not renamed");
        let new = s.create_from(&doc("周报")).unwrap();
        assert_eq!(s.index[&new].rel, "周报 (2).xml");
        let third = s.create_from(&doc("周报")).unwrap();
        assert_eq!(s.index[&third].rel, "周报 (3).xml");
    }

    /// 新建子文档 (#357): the page goes into a folder of its name, as its 正文,
    /// in its place; links to it and out of it follow; undo and redo replay it.
    #[test]
    fn child_pages_live_in_a_folder_of_the_pages_name() {
        let lib = Lib::new("subdoc");
        let mut s = lib.open();
        let other = s.create_from(&doc("其他")).unwrap();
        let plan = s.create_from(&linking("方案", &[(&other, "其他")])).unwrap();
        s.save(&other, &linking("其他", &[(&plan, "方案")])).unwrap();
        let journal = s.track_file_changes();
        let child = s.create_child(&plan, &doc("子页")).unwrap();
        assert_eq!(journal.take().len(), 3, "the folder, the page's move and the new page");
        assert!(s.abs("方案/方案.xml").fs_is_file() && s.abs("方案/子页.xml").fs_is_file() && !s.abs("方案.xml").fs_exists());
        assert!(crate::vfs::read_to_string(s.abs("其他.xml")).unwrap().contains("href=\"方案/方案.xml\""));
        assert!(crate::vfs::read_to_string(s.abs("方案/方案.xml")).unwrap().contains("href=\"../其他.xml\""));
        let folder = s.folder_of_note(&plan).expect("the page is its folder's 正文");
        assert_eq!(s.note_of(&folder).as_deref(), Some(plan.as_str()));
        assert_eq!(s.index[&child].parent.as_deref(), Some(folder.as_str()));
        let list = s.list();
        let f = list.iter().find(|e| e.id == folder).unwrap();
        assert_eq!((f.note.as_deref(), f.title.as_str()), (Some(plan.as_str()), "方案"));
        assert!(list.iter().filter(|e| e.id != folder).all(|e| e.note.is_none()));
        // The page is first inside, the new one last.
        let mut inside: Vec<&Entry> = list.iter().filter(|e| e.meta.parent.as_deref() == Some(folder.as_str())).collect();
        inside.sort_by_key(|e| (e.meta.place(), e.id.clone()));
        assert_eq!(inside.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), [plan.as_str(), child.as_str()]);
        // A second one goes straight into the folder, under the page or the folder alike.
        let second = s.create_child(&plan, &doc("子页二")).unwrap();
        let third = s.create_child(&folder, &doc("子页三")).unwrap();
        assert!(s.abs("方案/子页二.xml").fs_is_file() && s.abs("方案/子页三.xml").fs_is_file());
        // Deeper: a child page gets children of its own.
        let grandchild = s.create_child(&child, &doc("孙页")).unwrap();
        assert!(s.abs("方案/子页/子页.xml").fs_is_file() && s.abs("方案/子页/孙页.xml").fs_is_file());
        assert!(s.folder_of_note(&child).is_some() && s.index[&grandchild].parent == s.folder_of_note(&child));
        assert_eq!(s.index[&second].parent, s.index[&third].parent);
        // Undo and redo.
        let lone = s.create_from(&doc("单页")).unwrap();
        let journal = s.track_file_changes();
        let kid = s.create_child(&lone, &doc("新页")).unwrap();
        let changes = journal.take();
        assert!(s.abs("单页/单页.xml").fs_is_file());
        s.apply_file_history(&changes, false).unwrap();
        assert!(s.abs("单页.xml").fs_is_file() && !s.abs("单页").fs_exists() && !s.index.contains_key(&kid));
        assert!(s.index[&lone].parent.is_none() && s.folder_of_note(&lone).is_none());
        s.apply_file_history(&changes, true).unwrap();
        assert!(s.abs("单页/单页.xml").fs_is_file() && s.abs("单页/新页.xml").fs_is_file() && !s.abs("单页.xml").fs_exists());
        assert!(s.folder_of_note(&lone).is_some());
    }

    /// A page that gets its first child stays where it was among its
    /// siblings: by its last change, or by the place it was given by hand.
    #[test]
    fn a_page_keeps_its_place_when_it_gets_children() {
        let lib = Lib::new("subdoc-place");
        let mut s = lib.open();
        let old = s.create_from(&doc("旧")).unwrap();
        let mid = s.create_from(&doc("中")).unwrap();
        let new = s.create_from(&doc("新")).unwrap();
        for (id, t) in [(&old, 100), (&mid, 200), (&new, 300)] { s.index.get_mut(id).unwrap().modified = t; }
        s.save_index().unwrap();
        let order = |s: &mut Store| {
            let list = s.list();
            let mut top: Vec<&Entry> = list.iter().filter(|e| e.meta.parent.is_none()).collect();
            top.sort_by(|a, b| (a.meta.place(), &a.id).cmp(&(b.meta.place(), &b.id)));
            top.iter().map(|e| e.title.clone()).collect::<Vec<_>>()
        };
        assert_eq!(order(&mut s), ["新", "中", "旧"]);
        s.create_child(&mid, &doc("子")).unwrap();
        assert_eq!(order(&mut s), ["新", "中", "旧"], "by last change, the folder takes the page's");
        // Placed by hand: the folder takes the page's position.
        s.move_in_tree(&old, None, Some(&new)).unwrap();
        assert_eq!(order(&mut s), ["旧", "新", "中"]);
        s.create_child(&old, &doc("子")).unwrap();
        assert_eq!(order(&mut s), ["旧", "新", "中"], "a hand-placed page keeps its position");
    }

    /// Undoing 新建子文档 restores the page's place and its links.
    #[test]
    fn undoing_a_child_page_puts_links_back() {
        let lib = Lib::new("subdoc-undo");
        let mut s = lib.open();
        let other = s.create_from(&doc("其他")).unwrap();
        let plan = s.create_from(&doc("方案")).unwrap();
        s.save(&other, &linking("其他", &[(&plan, "方案")])).unwrap();
        let journal = s.track_file_changes();
        s.create_child(&plan, &doc("子页")).unwrap();
        let changes = journal.take();
        s.apply_file_history(&changes, false).unwrap();
        assert!(s.abs("方案.xml").fs_is_file() && !s.abs("方案").fs_exists());
        assert!(crate::vfs::read_to_string(s.abs("其他.xml")).unwrap().contains("href=\"方案.xml\""));
        // Only documents get children: not 画板, not folders without a 正文.
        let f = s.create_folder("夹", None).unwrap();
        assert!(!s.can_have_children(&f) && s.create_child(&f, &doc("x")).is_err());
        let board = s.create_board_in("板", &crate::whiteboard::Board::default(), None).unwrap();
        assert!(!s.can_have_children(&board));
        // Nor a Markdown file: it stays where it is, no folder is made.
        let md = s.create_markdown(&doc("笔记"), "").unwrap();
        assert!(!s.can_have_children(&md) && s.create_child(&md, &doc("x")).is_err());
        assert!(s.abs("笔记.md").fs_is_file() && !s.abs("笔记").fs_exists() && s.folder_of_note(&md).is_none());
    }

    /// Renaming a page with children renames its folder and its file
    /// together, either way round, and a numbered name goes back (#358 aside).
    #[test]
    fn a_page_with_children_keeps_one_name() {
        let lib = Lib::new("subdoc-rename");
        let mut s = lib.open();
        let plan = s.create_from(&doc("方案")).unwrap();
        let child = s.create_child(&plan, &doc("子页")).unwrap();
        let folder = s.folder_of_note(&plan).unwrap();
        let ref_ = s.create_from(&linking("引用", &[(&child, "子页"), (&plan, "方案")])).unwrap();
        // From the editor: the title is saved.
        s.save(&plan, &doc("计划")).unwrap();
        assert!(s.abs("计划/计划.xml").fs_is_file() && s.abs("计划/子页.xml").fs_is_file() && !s.abs("方案").fs_exists());
        assert_eq!(s.folder_of_note(&plan).as_deref(), Some(folder.as_str()));
        let text = crate::vfs::read_to_string(s.path(&ref_)).unwrap();
        assert!(text.contains("href=\"计划/子页.xml\"") && text.contains("href=\"计划/计划.xml\""), "{text}");
        // From the sidebar, on the folder or on the page.
        s.rename(&folder, "大计").unwrap();
        assert!(s.abs("大计/大计.xml").fs_is_file());
        assert_eq!(s.load(&plan).unwrap().title.text, "大计");
        let journal = s.track_file_changes();
        s.rename(&plan, "方略").unwrap();
        let changes = journal.take();
        assert!(s.abs("方略/方略.xml").fs_is_file());
        s.apply_file_history(&changes, false).unwrap();
        assert!(s.abs("大计/大计.xml").fs_is_file() && !s.abs("方略").fs_exists());
        assert_eq!(s.note_of(&folder).as_deref(), Some(plan.as_str()));
        // A taken name is numbered, and comes back when it is free again.
        let clash = s.create_folder("甲", None).unwrap();
        s.rename(&plan, "甲").unwrap();
        assert!(s.abs("甲 (2)/甲 (2).xml").fs_is_file());
        s.delete(&clash, None).unwrap();
        s.rename(&plan, "乙").unwrap();
        s.rename(&plan, "甲").unwrap();
        assert!(s.abs("甲/甲.xml").fs_is_file(), "back to the plain name");
        // A child page holding the new name inside makes way.
        let _kid = s.create_child(&plan, &doc("丙")).unwrap();
        s.rename(&plan, "丙").unwrap();
        assert!(s.abs("丙/丙.xml").fs_is_file() && s.abs("丙/丙 (2).xml").fs_is_file());
        assert_eq!(s.note_of(&folder).as_deref(), Some(plan.as_str()));
    }

    /// Moving or deleting a page with children takes its folder along; its
    /// last child going leaves the folder as it is (it still reads as the page).
    #[test]
    fn a_page_with_children_moves_and_goes_whole() {
        let lib = Lib::new("subdoc-move");
        let mut s = lib.open();
        let plan = s.create_from(&doc("方案")).unwrap();
        let child = s.create_child(&plan, &doc("子页")).unwrap();
        let folder = s.folder_of_note(&plan).unwrap();
        let dest = s.create_folder("归档", None).unwrap();
        s.set_parent(&folder, Some(&dest)).unwrap();
        assert!(s.abs("归档/方案/方案.xml").fs_is_file() && s.abs("归档/方案/子页.xml").fs_is_file());
        assert_eq!(s.note_of(&folder).as_deref(), Some(plan.as_str()));
        // The last child moved out: the folder stays, still the page's.
        s.set_parent(&child, Some(&dest)).unwrap();
        assert!(s.abs("归档/方案/方案.xml").fs_is_file() && s.abs("归档/子页.xml").fs_is_file());
        assert_eq!(s.note_of(&folder).as_deref(), Some(plan.as_str()));
        s.delete(&folder, None).unwrap();
        assert!(!s.abs("归档/方案").fs_exists());
        s.list();
        assert!(!s.index.contains_key(&plan));
    }

    /// A library v0.1.11 moved to real folders reads its old child pages as
    /// child pages again. A hand-made `X/X.md` does not make `X` a page: it
    /// is a plain Markdown file in a plain folder.
    #[test]
    fn migrated_pages_have_children_markdown_ones_do_not() {
        let lib = Lib::new("subdoc-migrated");
        let mut old = Old::new(&lib, 1);
        let p = old.doc("父", Meta::default());
        let a = old.doc("子一", Meta { parent: Some(p.clone()), created: 100, ..Default::default() });
        old.write();
        let mut s = lib.migrated();
        let list = s.list();
        let folder = list.iter().find(|e| e.note.as_deref() == Some(p.as_str())).expect("父 is its folder's 正文");
        assert_eq!(folder.title, "父");
        assert_eq!(s.index[&a].parent.as_deref(), Some(folder.id.as_str()));
        crate::vfs::create_dir_all(lib.0.join("手写")).unwrap();
        crate::vfs::write(lib.0.join("手写/手写.md"), "# 手写\n").unwrap();
        crate::vfs::write(lib.0.join("手写/别的.md"), "x\n").unwrap();
        // Beside an XML 正文 of the same name, the Markdown one is a child page.
        crate::vfs::create_dir_all(lib.0.join("双")).unwrap();
        crate::vfs::write(lib.0.join("双/双.md"), "x\n").unwrap();
        crate::vfs::write(lib.0.join("双/双.xml"), docxml::serialize(&doc("双"))).unwrap();
        let list = s.list();
        let by_rel = |rel: &str| list.iter().find(|e| e.meta.rel == rel).unwrap();
        let (hand, md) = (by_rel("手写"), by_rel("手写/手写.md"));
        assert!(hand.note.is_none() && hand.meta.folder.is_some() && hand.title == "手写");
        assert_eq!(md.meta.parent.as_deref(), Some(hand.id.as_str()));
        assert!(s.folder_of_note(&md.id).is_none() && s.note_of(&hand.id).is_none() && !s.can_have_children(&hand.id));
        assert_eq!(by_rel("手写/别的.md").meta.parent.as_deref(), Some(hand.id.as_str()));
        assert_eq!(by_rel("双").note.as_deref(), Some(by_rel("双/双.xml").id.as_str()));
        assert_eq!(by_rel("双/双.md").meta.parent.as_deref(), Some(by_rel("双").id.as_str()));
        assert!(s.folder_of_note(&by_rel("双/双.md").id).is_none());
        // A 画板 is never a 正文.
        crate::vfs::create_dir_all(lib.0.join("板")).unwrap();
        crate::vfs::write(lib.0.join("板/板.excalidraw"), crate::whiteboard::Board::default().file_json()).unwrap();
        assert!(s.list().iter().find(|e| e.meta.rel == "板").unwrap().note.is_none());
    }

    /// A 数据表 with 笔记 columns lives in a folder of its name with its notes
    /// (`Store::table_folder`): the folder shows as the table, renames with it,
    /// goes to the 回收站 and back with it, and the paths in its 笔记 cells
    /// follow whatever moves.
    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn a_table_lives_with_its_notes() {
        use crate::table::{sqlite, Doc, FieldOpts, FieldType, Value};
        let lib = Lib::new("table-folder");
        let mut s = lib.open();
        let outside = s.create_from(&doc("别处的笔记")).unwrap();
        let table = s.create_table_in("书单", &crate::table::Base::blank(0), None).unwrap();
        assert!(s.abs("书单.sqlite").fs_is_file() && s.folder_of_note(&table).is_none() && !s.can_have_children(&table));
        // the table's cells: a note beside it, one elsewhere
        let cells = |s: &Store, rel: &str| -> Vec<String> {
            let b = sqlite::load(&s.abs(rel)).unwrap();
            let t = &b.tables[0];
            let f = t.fields.iter().find(|f| f.ty == FieldType::Note).unwrap().id;
            t.records.iter().filter_map(|r| match r.get(f) { Value::Text(h) => Some(h.clone()), _ => None }).collect()
        };
        let point = |s: &Store, rel: &str, hrefs: &[&str]| {
            let mut d = Doc::new(sqlite::load(&s.abs(rel)).unwrap());
            let t = d.base.tables[0].id;
            let f = d.add_field(t, None, "读书笔记", FieldType::Note, FieldOpts::default()).unwrap();
            let rows: Vec<_> = d.base.tables[0].records.iter().map(|r| r.id).collect();
            d.set_cells(t, hrefs.iter().enumerate().map(|(i, h)| (rows[i], f, Value::Text(h.to_string()))).collect());
            sqlite::save(&s.abs(rel), &d.base, &d.dirty).unwrap();
        };
        point(&s, "书单.sqlite", &["别处的笔记.xml"]);
        // its first 笔记 column: into a folder of its name, which takes its place; the path out of it follows
        let journal = s.track_file_changes();
        let folder = s.table_folder(&table).unwrap();
        assert!(s.abs("书单/书单.sqlite").fs_is_file() && !s.abs("书单.sqlite").fs_exists());
        assert_eq!(cells(&s, "书单/书单.sqlite"), ["../别处的笔记.xml"]);
        assert_eq!((s.folder_of_note(&table).as_deref(), s.note_of(&folder).as_deref()), (Some(folder.as_str()), Some(table.as_str())));
        assert_eq!(s.table_folder(&table).unwrap(), folder, "already there");
        // listed as one row: the table (its name, its kind), the notes under it
        let note = s.create_from_in(&doc("三体"), Some(&folder)).unwrap();
        assert!(s.abs("书单/三体.xml").fs_is_file());
        let list = s.list();
        let row = list.iter().find(|e| e.id == folder).unwrap();
        assert_eq!((row.note.as_deref(), row.title.as_str(), row.meta.kind, row.meta.folder.is_some()), (Some(table.as_str()), "书单", DocKind::Table, true));
        assert_eq!(list.iter().find(|e| e.id == note).unwrap().meta.parent.as_deref(), Some(folder.as_str()));
        // a note renamed, a note moved out: the cells follow
        let mut d = Doc::new(sqlite::load(&s.abs("书单/书单.sqlite")).unwrap());
        let (t, f) = (d.base.tables[0].id, d.base.tables[0].fields.iter().find(|f| f.ty == FieldType::Note).unwrap().id);
        let second = d.base.tables[0].records[1].id;
        d.base.writer = crate::table::Writer("agent:整理".into());
        d.set_cells(t, vec![(second, f, Value::Text("三体.xml".into()))]);
        sqlite::save(&s.abs("书单/书单.sqlite"), &d.base, &d.dirty).unwrap();
        s.rename(&note, "三体 II").unwrap();
        assert_eq!(cells(&s, "书单/书单.sqlite"), ["../别处的笔记.xml", "三体 II.xml"]);
        // the row whose cell followed was changed by the app, and says so beside when
        let by = |s: &Store| sqlite::load(&s.abs("书单/书单.sqlite")).unwrap().tables[0].records.iter().map(|r| r.updated_by.clone().unwrap_or_default()).collect::<Vec<_>>();
        assert_eq!(by(&s)[..2], ["user", "user"]);
        s.rename(&outside, "搬走的").unwrap();
        s.set_parent(&note, None).unwrap();
        assert_eq!(cells(&s, "书单/书单.sqlite"), ["../搬走的.xml", "../三体 II.xml"]);
        s.set_parent(&note, Some(&folder)).unwrap();
        // the table renamed (through its row or itself): folder and file keep one name, the paths still right
        s.rename(&folder, "读过的书").unwrap();
        assert!(s.abs("读过的书/读过的书.sqlite").fs_is_file() && s.abs("读过的书/三体 II.xml").fs_is_file() && !s.abs("书单").fs_exists());
        assert_eq!(cells(&s, "读过的书/读过的书.sqlite"), ["../搬走的.xml", "三体 II.xml"]);
        assert_eq!(s.note_of(&folder).as_deref(), Some(table.as_str()));
        // moved into another folder: the notes go with it
        let shelf = s.create_folder("架", None).unwrap();
        s.set_parent(&folder, Some(&shelf)).unwrap();
        assert!(s.abs("架/读过的书/读过的书.sqlite").fs_is_file() && s.abs("架/读过的书/三体 II.xml").fs_is_file());
        assert_eq!(cells(&s, "架/读过的书/读过的书.sqlite"), ["../../搬走的.xml", "三体 II.xml"]);
        // deleted: the notes go with the table, and come back with it
        s.delete(&folder, None).unwrap();
        assert!(!s.abs("架/读过的书").fs_exists() && s.abs("搬走的.xml").fs_is_file());
        assert!(s.list().iter().all(|e| e.id != note && e.id != table));
        s.restore(&folder).unwrap();
        assert!(s.abs("架/读过的书/读过的书.sqlite").fs_is_file() && s.abs("架/读过的书/三体 II.xml").fs_is_file());
        assert_eq!(s.list().iter().find(|e| e.id == folder).unwrap().note.as_deref(), Some(table.as_str()));
        drop(journal);
        // a folder that holds a document of its name too: the document is its 正文, the table a child
        crate::vfs::write(s.abs("架/读过的书/读过的书.xml"), docxml::serialize(&doc("读过的书"))).unwrap();
        let list = s.list();
        let page = list.iter().find(|e| e.meta.rel == "架/读过的书/读过的书.xml").unwrap();
        assert_eq!(list.iter().find(|e| e.id == folder).unwrap().note.as_deref(), Some(page.id.as_str()));
        // a 画板 or a Markdown file is never moved into a folder this way
        let md = s.create_markdown(&doc("纯文本"), "").unwrap();
        assert!(s.table_folder(&md).is_err() && s.table_folder(&outside).is_err());
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
        crate::vfs::create_dir_all(lib.0.join("外部/更深")).unwrap();
        crate::vfs::write(lib.0.join("外部/更深/新.md"), "从访达放进来\n").unwrap();
        crate::vfs::write(lib.0.join("外部/.隐藏.md"), "x").unwrap();
        crate::vfs::write(lib.0.join("外部/.云端.xml.icloud"), "").unwrap();
        crate::vfs::write(lib.0.join("外部/图.png"), "").unwrap();
        let list = s.list();
        let new = list.iter().find(|e| e.title == "新").expect("a file from the Finder");
        assert!(new.meta.markdown);
        assert_eq!(folders_of(&list, &new.id), ["外部", "更深"]);
        let cloud = list.iter().find(|e| e.title == "云端").expect("an iCloud placeholder is listed");
        assert!(s.load(&cloud.id).unwrap_err().to_string().contains("iCloud"));
        assert_eq!(list.len(), 7, "{:?}", list.iter().map(|e| &e.title).collect::<Vec<_>>());
        let new_id = new.id.clone();
        assert_eq!(s.list().iter().find(|e| e.title == "新").unwrap().id, new_id, "ids stay");
        crate::vfs::remove_dir_all(lib.0.join("外部")).unwrap();
        assert_eq!(s.list().len(), 3);
        drop(s);
        crate::vfs::remove_dir_all(lib.0.join(".quiet")).unwrap();
        let mut s = lib.open();
        let list = s.list();
        assert_eq!(list.len(), 3);
        let a2 = list.iter().find(|e| e.title == "甲").unwrap();
        let b2 = list.iter().find(|e| e.title == "乙").unwrap();
        assert!(!a2.meta.pinned, "attached information is lost, nothing else");
        assert_eq!(folders_of(&list, &b2.id), ["夹"]);
        assert_eq!(targets(b2), std::slice::from_ref(&a2.id));
    }

    /// Markdown by any name 打开文件… takes (`.markdown`, `.mdown`, `.MD`)
    /// shows too, and keeps its extension when the title renames it.
    #[test]
    fn markdown_by_any_extension() {
        let lib = Lib::new("exts");
        let mut s = lib.open();
        for name in ["夹/长.markdown", "短.mdown", "大写.MD", "文.XML.txt", "无名/.markdown"] {
            std::fs::create_dir_all(lib.0.join(name).parent().unwrap()).unwrap();
            std::fs::write(lib.0.join(name), "# x\n").unwrap();
        }
        std::fs::write(lib.0.join(".云端.markdown.icloud"), "").unwrap();
        let list = s.list();
        let mut titles: Vec<_> = list.iter().filter(|e| e.meta.folder.is_none()).map(|e| (e.title.as_str(), e.meta.markdown)).collect();
        titles.sort();
        assert_eq!(titles, [("云端", true), ("大写", true), ("短", true), ("长", true)]);
        let long = list.iter().find(|e| e.title == "长").unwrap().id.clone();
        assert_eq!(folders_of(&list, &long), ["夹"]);
        s.rename(&long, "更长").unwrap();
        assert!(lib.0.join("夹/更长.markdown").is_file() && !lib.0.join("夹/长.markdown").exists());
        let upper = list.iter().find(|e| e.title == "大写").unwrap().id.clone();
        s.rename(&upper, "小写").unwrap();
        let names: Vec<String> = std::fs::read_dir(&lib.0).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(names.iter().any(|n| n == "小写.MD"), "{names:?}");
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
        crate::vfs::write(lib.0.join("夹/里/附件.pdf"), b"pdf").unwrap();
        let journal = s.track_file_changes();
        s.delete(&f, None).unwrap();
        s.delete(&g, Some(&f)).unwrap();
        s.delete(&a, Some(&f)).unwrap();
        let changes = journal.take();
        assert_eq!(changes.len(), 3);
        assert!(!lib.0.join("夹").fs_exists());
        assert!(s.list().is_empty());
        assert_eq!(s.list_trash().len(), 1);
        s.restore(&f).unwrap();
        assert!(s.abs("夹/里/甲.xml").fs_is_file() && s.abs("夹/里/附件.pdf").fs_is_file());
        let list = s.list();
        assert_eq!(folders_of(&list, &a), ["夹", "里"]);
        s.delete(&f, None).unwrap();
        s.apply_file_history(&changes, false).unwrap();
        assert!(s.abs("夹/里/甲.xml").fs_is_file() && s.abs("夹/里/附件.pdf").fs_is_file());
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
        assert!(lib.0.join("docs").join(format!("{b}.xml")).fs_is_file());
        assert!(crate::vfs::read_to_string(lib.0.join("docs").join(format!("{b}.xml"))).unwrap().contains(&format!("doc-id=\"{a}\"")));
        let g = s.create_folder("新夹", None).unwrap();
        s.set_parent(&b, Some(&g)).unwrap();
        s.rename(&g, "改名夹").unwrap();
        let md = s.create_markdown(&doc("记"), "").unwrap();
        assert!(lib.0.join("docs").join(format!("{md}.md")).fs_is_file());
        s.delete(&a, None).unwrap();
        assert!(lib.0.join("trash").join(format!("{a}.xml")).fs_is_file());
        s.restore(&a).unwrap();
        s.set_pinned(&a, true).unwrap();
        s.touch(&a);
        s.flush_view_state().unwrap();
        let mut st = s.settings();
        st.window_size = Some([1., 2.]);
        s.save_settings(&st).unwrap();
        let list = s.list();
        assert_eq!(list.len(), 5);
        assert_eq!(folders_of(&list, &b), ["改名夹"]);
        assert!(!lib.0.join(".quiet").fs_exists() && !lib.0.join("改名夹").fs_exists(), "nothing of the new layout yet");
        let index: HashMap<String, Meta> = serde_json::from_slice(&crate::vfs::read(lib.0.join("index.json")).unwrap()).unwrap();
        assert!(index[&a].pinned && index[&a].opened > 0 && index[&md].markdown);
        assert_eq!(serde_json::from_slice::<FormatMarker>(&crate::vfs::read(lib.0.join("format.json")).unwrap()).unwrap().format, 3);
        drop(s);
        let report = Store::migrate(&lib.0).unwrap();
        assert_eq!((report.docs, report.folders), (3, 2));
        let mut s = lib.open();
        assert!(!s.flat);
        let list = s.list();
        assert_eq!(folders_of(&list, &b), ["改名夹"]);
        assert!(s.abs("改名夹/乙.xml").fs_is_file() && s.abs("夹/甲.xml").fs_is_file() && s.abs("记.md").fs_is_file());
        assert!(crate::vfs::read_to_string(s.abs("改名夹/乙.xml")).unwrap().contains("href=\"../夹/甲.xml\""));
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
        let moved = crate::vfs::temp_dir().join("elsewhere/Quiet.md/assets/p.png").to_string_lossy().into_owned();
        let d = Document { title: RichText::plain("图"), blocks: vec![
            image(abs("assets/p.png")),
            Block::new(BlockKind::File(FileBlock { src_kind: "path".into(), src: abs("assets/files/1234/报告.pdf"), name: "报告.pdf".into(), view: FileView::Card }), RichText::default()),
            image(moved.clone()),
            image(abs("assets/gone.png")),
            // Only through `.assets/x/` once moved: the fallback must not follow it.
            image(crate::vfs::temp_dir().join("elsewhere/Quiet.md/assets/x/../../secret.png").to_string_lossy().into_owned()),
        ], ..Default::default() };
        let id = old.put(&d, Meta::default());
        old.write();
        crate::vfs::create_dir_all(lib.0.join("assets/files/1234")).unwrap();
        crate::vfs::write(lib.0.join("assets/p.png"), b"png").unwrap();
        crate::vfs::write(lib.0.join("assets/files/1234/报告.pdf"), b"pdf").unwrap();
        crate::vfs::write(lib.0.join("secret.png"), b"s").unwrap();
        let s = lib.migrated();
        assert!(!lib.0.join("assets").fs_exists());
        crate::vfs::create_dir_all(lib.0.join(".assets/x")).unwrap();
        let srcs = s.load(&id).unwrap().asset_sources();
        let found: Vec<Option<PathBuf>> = srcs.iter().map(|src| image_path_in(&lib.0, src).filter(|p| p.fs_is_file())).collect();
        assert_eq!(found, [Some(lib.0.join(".assets/p.png")), Some(lib.0.join(".assets/files/1234/报告.pdf")), Some(lib.0.join(".assets/p.png")), None, None]);
        assert!(export_path_in(&lib.0, &srcs[0]).is_some() && export_path_in(&lib.0, &srcs[1]).is_some());
        assert!(export_path_in(&lib.0, &srcs[3]).is_none() && export_path_in(&lib.0, &srcs[4]).is_none());
        // A path that still exists is used as it is.
        crate::vfs::write(lib.0.join("here.png"), b"h").unwrap();
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
        crate::vfs::create_dir_all(lib.0.join("assets")).unwrap();
        crate::vfs::write(lib.0.join("assets/p.png"), b"p").unwrap();
        crate::vfs::write(lib.0.join("preferences.json"), b"{\"old\":1}").unwrap();
        let original = crate::vfs::read(lib.0.join("docs").join(format!("{a}.xml"))).unwrap();
        let mut s = lib.migrated();
        // Edits are told from the move by modification time; some file
        // systems stamp a little behind the clock, so leave a gap.
        std::thread::sleep(std::time::Duration::from_millis(50));
        s.save(&a, &doc("甲改")).unwrap();
        s.create_from(&doc("新的")).unwrap();
        crate::vfs::write(lib.0.join(".quiet/preferences.json"), b"{\"new\":1}").unwrap();
        drop(s);
        // Pruning: not before both 30 days and three launches.
        let info = lib.0.join(".quiet").join(migrate::BACKUP).with_extension("json");
        for _ in 0..5 { Store::prune_backups(&lib.0); }
        assert!(Store::backup_dir(&lib.0).is_some());
        let report = Store::restore_backup(&lib.0).unwrap();
        assert_eq!(report.changed, 2);
        assert!(report.kept.join("夹/甲改.xml").fs_is_file() && report.kept.join("新的.xml").fs_is_file());
        assert_eq!(crate::vfs::read(lib.0.join("docs").join(format!("{a}.xml"))).unwrap(), original);
        assert!(lib.0.join("assets/p.png").fs_is_file() && !lib.0.join(".assets").fs_exists());
        assert_eq!(crate::vfs::read(lib.0.join("preferences.json")).unwrap(), b"{\"new\":1}");
        assert!(!lib.0.join("夹").fs_exists() && !lib.0.join(".quiet/format.json").fs_exists());
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
        crate::vfs::write(&info, b"{\"created\": 1, \"launches\": 0}").unwrap();
        for _ in 0..3 { Store::prune_backups(&lib.0); }
        assert!(Store::backup_dir(&lib.0).is_some());
        Store::prune_backups(&lib.0);
        assert!(Store::backup_dir(&lib.0).is_none() && !info.fs_exists());
    }

    #[test]
    fn synced_folders_are_recognised() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(sync_of(&home.join("Library/Mobile Documents/com~apple~CloudDocs/Quiet.md")), SyncService::ICloud);
        assert_eq!(sync_of(&home.join("Library/CloudStorage/OneDrive-Personal/Quiet.md")), SyncService::Provider("OneDrive".into()));
        assert_eq!(sync_of(&crate::vfs::temp_dir().join("x")), SyncService::Local);
        // An old library with a conflict copy left by syncing asks first.
        let lib = Lib::new("conflict");
        let mut old = Old::new(&lib, 3);
        old.doc("甲", Meta::default());
        old.write();
        crate::vfs::write(lib.0.join("index 2.json"), b"{}").unwrap();
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
            let index = || if flat { crate::vfs::read_to_string(lib.0.join("index.json")).unwrap_or_default() } else { lib.meta_text() };
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
                assert!(!index().contains(&ids[0]));
                assert_eq!(s.batch(|s| s.create_markdown_in(&doc("嵌套"), "", Some(&f)).unwrap()).map(|_| ()).ok(), Some(()));
                (f, ids)
            }).unwrap();
            assert!(index().contains(&ids[0]));
            if !flat {
                assert!(lib.0.join("导入/笔记 4.md").fs_is_file() && lib.0.join("导入/子/深处.xml").fs_is_file());
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
                if e.meta.folder.is_none() { assert!(again.path(&e.id).fs_is_file(), "{} has no file", e.title); }
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
            let t = web_time::Instant::now();
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
            let t = web_time::Instant::now();
            s.batch(|s| s.delete(&b, None)).unwrap().unwrap();
            eprintln!("flat {flat}: one by one {slow:?}, batch {fast:?}, delete {:?}", t.elapsed());
        }
        // What 导入文件夹 does, from Markdown files on disk to the list shown.
        let src = Lib::new("import-src");
        crate::vfs::create_dir_all(src.0.join("子")).unwrap();
        for i in 0..120 {
            let body = format!("---\ntags: [a]\n---\n# 笔记 {i}\n\n{}\n\n- 一\n- 二\n\n```rust\nfn main() {{}}\n```\n", "一段正文，带 **加粗** 和 [链接](https://example.com)。".repeat(20));
            crate::vfs::write(src.0.join(if i % 3 == 0 { format!("子/笔记 {i}.md") } else { format!("笔记 {i}.md") }), body).unwrap();
        }
        let lib = Lib::new("import-real");
        let mut s = lib.open();
        let t = web_time::Instant::now();
        s.batch(|s| {
            let top = s.create_folder("导入", None).unwrap();
            let sub = s.create_folder("子", Some(&top)).unwrap();
            for (dir, parent) in [(src.0.clone(), &top), (src.0.join("子"), &sub)] {
                for e in crate::vfs::read_dir(dir).unwrap().flatten().filter(|e| e.path().fs_is_file()) {
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

    /// 设置 › 文档库 › 未使用资源 (#237): an asset file named `name`.
    fn asset(lib: &Lib, name: &str) -> PathBuf {
        let dir = Store::assets_in(&lib.0);
        crate::vfs::create_dir_all(&dir).unwrap();
        crate::vfs::write(dir.join(name), b"png").unwrap();
        dir.join(name)
    }

    /// Today's folder of what a sweep set aside.
    fn removed(gone: &Path) -> PathBuf {
        let (y, m, d, ..) = crate::time::civil(now());
        gone.join(format!("{y:04}-{m:02}-{d:02}"))
    }

    /// A page's file, body verbatim; folders made as needed.
    fn page_at(lib: &Lib, rel: &str, body: &str) {
        let p = lib.0.join(rel);
        crate::vfs::create_dir_all(p.parent().unwrap()).unwrap();
        crate::vfs::write(&p, body).unwrap();
    }

    fn spare_assets(lib: &Lib, live: Vec<String>) -> Vec<String> {
        Store::unused_assets_at(lib.0.clone(), live, vec![]).unwrap().into_iter().map(|(name, _)| name).collect()
    }

    #[test]
    fn unused_assets_match_names_the_way_they_are_written() {
        let lib = Lib::new("sweep-names");
        asset(&lib, "1a2b-Screen Shot 2026-11-17 at 10.00.png");
        asset(&lib, "a&b.png");
        asset(&lib, "spare.png");
        // Markdown percent-encodes a space, DocxXML escapes `&`, and a page in
        // a subfolder writes its source relative to itself.
        page_at(&lib, "note.md", "![](/Users/x/Quiet.md/.assets/1a2b-Screen%20Shot%202026-11-17%20at%2010.00.png)");
        page_at(&lib, "sub/deep.md", "![](../.assets/a&amp;b.png)");
        // One link that does not decode to UTF-8 leaves the rest decodable,
        // and Markdown may escape punctuation with a backslash.
        asset(&lib, "c d.png");
        asset(&lib, "e_f.png");
        page_at(&lib, "gbk.md", "[百度](https://www.baidu.com/s?wd=%C4%E3)\n\n![](.assets/c%20d.png) ![](.assets/e\\_f.png)");
        assert_eq!(Store::unused_assets_at(lib.0.clone(), vec![], vec![]).unwrap(), vec![("spare.png".to_string(), 3)]);
    }

    #[test]
    fn unused_assets_read_the_trash_the_history_and_the_backups() {
        let lib = Lib::new("sweep-keep");
        for name in ["trashed.png", "old.png", "backup.png"] { asset(&lib, name); }
        // Restoring a page, or a version of one, must not bring back a missing picture.
        page_at(&lib, ".quiet/trash/9f2.xml", "<image src=\"/x/.assets/trashed.png\"/>");
        page_at(&lib, ".quiet/history/9f2/1.xml", "<image src=\"/x/.assets/old.png\"/>");
        // The `.bak` of a page that is still there, with the picture its
        // current text no longer has.
        page_at(&lib, "note.md", "no pictures now");
        page_at(&lib, "note.md.bak", "![](./.assets/backup.png)");
        assert!(spare_assets(&lib, vec![]).is_empty());
    }

    #[test]
    fn unused_assets_keep_what_an_open_editor_holds() {
        let lib = Lib::new("sweep-live");
        asset(&lib, "unsaved.png");
        asset(&lib, "undone.png");
        // Text not saved yet, and a snapshot Undo can still bring back.
        let live = vec![".assets/unsaved.png".to_string(), "/Users/x/Quiet.md/.assets/undone.png".to_string()];
        assert!(spare_assets(&lib, live).is_empty());
    }

    #[test]
    fn removing_unused_assets_sets_them_aside_in_the_library() {
        let lib = Lib::new("sweep-move");
        asset(&lib, "spare.png");
        // A name that leaves the folder is not followed.
        let names = vec!["../outside.png".to_string(), "spare.png".to_string()];
        assert_eq!(Store::remove_unused_assets(&lib.0, &names), 1);
        let from = Store::assets_in(&lib.0);
        assert!(!from.join("spare.png").fs_is_file());
        let day = removed(&lib.0.join(".quiet").join("removed-assets"));
        assert!(day.join("spare.png").fs_is_file());
        // A second sweep of the same name adds a number rather than overwriting.
        asset(&lib, "spare.png");
        assert_eq!(Store::remove_unused_assets(&lib.0, &["spare.png".to_string()]), 1);
        assert!(day.join("spare 2.png").fs_is_file());
    }

    /// A 数据表 keeps its attachments inside SQLite, as a library file, one
    /// embedded in a page, or a version of either; and SQLite's own files
    /// beside an embedded one are never offered.
    #[test]
    fn unused_assets_read_every_table() {
        let lib = Lib::new("sweep-tables");
        for name in ["in-file.png", "in-embedded.png", "in-version.png", "spare.png", "t.sqlite-journal"] { asset(&lib, name); }
        let base = |name: &str| {
            use crate::table::*;
            let mut b = Base::blank_named(0, "T", "文本");
            let id = b.new_id();
            let t = &mut b.tables[0];
            t.fields.push(Field { id, name: "附件".into(), ty: FieldType::Attachment, opts: FieldOpts::default(), desc: String::new() });
            t.records[0].cells.insert(id, Value::Files(vec![Attachment { path: format!(".assets/{name}"), name: name.into() }]));
            b
        };
        crate::table::sqlite::create(&lib.0.join("表.sqlite"), &base("in-file.png")).unwrap();
        crate::table::sqlite::create(&Store::assets_in(&lib.0).join("t.sqlite"), &base("in-embedded.png")).unwrap();
        page_at(&lib, "page.xml", "<title>P</title>\n<table src=\".assets/t.sqlite\"/>");
        crate::vfs::create_dir_all(lib.0.join(".quiet/history/x")).unwrap();
        crate::table::sqlite::create(&lib.0.join(".quiet/history/x/1700000000000-0000abcd.sqlite"), &base("in-version.png")).unwrap();
        assert_eq!(spare_assets(&lib, vec![]), ["spare.png"]);
    }

    /// Bytes that are not text (a Finder `.DS_Store` in the 回收站) are no
    /// reason to give up; a page iCloud has not downloaded is.
    #[test]
    fn unused_assets_refuse_what_they_cannot_see() {
        let lib = Lib::new("sweep-blind");
        asset(&lib, "a.png");
        page_at(&lib, "note.md", "nothing");
        crate::vfs::create_dir_all(lib.0.join(".quiet/trash")).unwrap();
        crate::vfs::write(lib.0.join(".quiet/trash/.DS_Store"), [0u8, 159, 146, 150, 0xff]).unwrap();
        assert_eq!(spare_assets(&lib, vec![]), ["a.png"]);
        page_at(&lib, "sub/.cloud.md.icloud", "bplist00");
        assert!(Store::unused_assets_at(lib.0.clone(), vec![], vec![]).is_err());
    }

    /// A picture pasted into a 本机文件 or a note in a 本机文件夹 is copied into
    /// this library too; an outside folder that cannot be reached stops the sweep.
    #[test]
    fn unused_assets_read_the_files_opened_outside() {
        let lib = Lib::new("sweep-outside");
        for name in ["file.png", "folder.png", "spare.png"] { asset(&lib, name); }
        let out = Lib::new("sweep-outside-files");
        page_at(&out, "single.md", "![](/x/.assets/file.png)");
        page_at(&out, "dir/deep/n.md", "![](/x/.assets/folder.png)");
        let outside = vec![out.0.join("single.md"), out.0.join("dir"), out.0.join("deleted.md")];
        assert_eq!(Store::unused_assets_at(lib.0.clone(), vec![], outside).unwrap(), vec![("spare.png".to_string(), 3)]);
        assert!(Store::unused_assets_at(lib.0.clone(), vec![], vec![out.0.join("unplugged/n.md")]).is_err());
    }

    /// What a sweep set aside waits in a folder named by its day, cleared by
    /// that name once older than the 回收站's days: a moved file keeps an old
    /// write time, which must not count.
    #[test]
    fn removed_assets_are_cleared_by_the_day_they_were_removed() {
        let lib = Lib::new("sweep-days");
        let gone = lib.0.join(".quiet/removed-assets");
        page_at(&lib, ".quiet/removed-assets/2001-02-03/old.png", "png");
        page_at(&lib, ".quiet/removed-assets/notes.txt", "kept");
        asset(&lib, "spare.png");
        assert_eq!(Store::remove_unused_assets(&lib.0, &["spare.png".to_string()]), 1);
        assert!(!gone.join("2001-02-03").fs_exists() && gone.join("notes.txt").fs_is_file());
        assert!(removed(&gone).join("spare.png").fs_is_file());
    }

    #[test]
    fn a_library_without_an_asset_folder_has_nothing_to_sweep() {
        let lib = Lib::new("sweep-none");
        page_at(&lib, "note.md", "no pictures here");
        assert!(spare_assets(&lib, vec![]).is_empty());
    }

    /// 正文字号 goes with the top-level folder like its theme and font; a look saved
    /// before it existed reads as the theme's size.
    #[test]
    fn look_keeps_doc_size() {
        let lib = Lib::new("look");
        lib.migrated();
        let old = FolderLook { theme: Some("craft".into()), feishu: Some(true), doc_family: Some("Songti SC".into()), doc_size: None };
        Store::save_look_at(&lib.0, &old).unwrap();
        assert_eq!(Store::look_at(&lib.0).doc_size, None);
        let look = FolderLook { doc_size: Some("large".into()), ..old };
        Store::save_look_at(&lib.0, &look).unwrap();
        assert_eq!(Store::look_at(&lib.0), look);
    }

    /// #406: a document still only in iCloud is listed by its file name and
    /// not read (reading downloads it), and loading it fails at once; once
    /// it is here it is read as usual.
    #[test]
    fn cloud_only_documents_are_listed_unread() {
        let lib = Lib::new("dataless");
        let mut s = lib.open();
        s.create_from(&doc("本地")).unwrap();
        let _ = s.list();
        let file = lib.0.join("云端笔记.xml");
        crate::vfs::write(&file, docxml::serialize(&doc("标题在文件里"))).unwrap();
        crate::vfs::set_dataless(&file, true);
        let mut s = lib.open();
        // The first list finds the new file, which re-reads 本地 (paths changed).
        let _ = s.list();
        DOC_READS.with(|n| n.set(0));
        let list = s.list();
        assert_eq!(DOC_READS.with(|n| n.get()), 0, "云端笔记 must not be read");
        let cloud = list.iter().find(|e| e.title == "云端笔记").expect("listed by its file name");
        assert!(cloud.body.is_empty());
        assert_eq!(s.cloud_only(&cloud.id), Some(file.clone()));
        assert_eq!(s.load(&cloud.id).unwrap_err().to_string(), NOT_DOWNLOADED);
        // Not cached as read: once downloaded, the next list reads it.
        crate::vfs::set_dataless(&file, false);
        assert_eq!(s.cloud_only(&cloud.id), None);
        let list = s.list();
        assert!(list.iter().any(|e| e.title == "标题在文件里"));
        assert_eq!(s.load(&cloud.id).unwrap().title.text, "标题在文件里");
    }

    /// #406: the library's own files still only in iCloud are passed over
    /// with a warning: not restored from `.bak`, not set aside as
    /// `.corrupt-`, and never written over (another device's data).
    #[test]
    fn cloud_only_library_files_are_held_not_replaced() {
        let lib = Lib::new("dataless-meta");
        let mut s = lib.open();
        let id = s.create_from(&doc("一")).unwrap();
        s.set_pinned(&id, true).unwrap();
        let other = meta::dir(&lib.0).join("other-device.json");
        crate::vfs::write(&other, b"{}").unwrap();
        let trash = lib.0.join(INTERNAL).join("trash").join("index.json");
        crate::vfs::create_dir_all(trash.parent().unwrap()).unwrap();
        crate::vfs::write(&trash, b"{}").unwrap();
        // Every one of this device's files.
        let own = meta::dir(&lib.0).join(device().id);
        let mine: Vec<PathBuf> = crate::vfs::read_dir(&own).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
        let (before, marker) = (mine.iter().map(|p| crate::vfs::read(p).unwrap()).collect::<Vec<_>>(), lib.0.join(INTERNAL).join("format.json"));
        let format = crate::vfs::read(&marker).unwrap();
        for f in mine.iter().chain([&other, &trash, &marker]) { crate::vfs::set_dataless(f, true); }
        let mut s = lib.open();
        for what in ["文档索引", "other-device.json", "回收站索引", "format.json"] {
            assert!(s.warnings.iter().any(|w| w == &not_downloaded(what)), "{what}: {:?}", s.warnings);
        }
        assert!(s.warnings.iter().all(|w| !w.contains("备份") && !w.contains("损坏")), "{:?}", s.warnings);
        // The marker cannot be read: 5, the oldest with device files, until a write reads it.
        assert_eq!(Store::format_at(&lib.0), 5);
        // This device's changes can't go to its file now: refused, not written over.
        let _ = s.list();
        assert!(s.create_from(&doc("二")).is_err());
        assert_eq!(mine.iter().map(|p| crate::vfs::read(p).unwrap()).collect::<Vec<_>>(), before);
        assert_eq!(crate::vfs::read(&marker).unwrap(), format);
        let names: Vec<String> = crate::vfs::read_dir(meta::dir(&lib.0)).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(names.iter().all(|n| !n.contains(".corrupt-")), "{names:?}");
    }

    /// #406: 回收站 lists a page still only in iCloud without reading it.
    #[test]
    fn cloud_only_trash_is_listed_unread() {
        let lib = Lib::new("dataless-trash");
        let mut s = lib.open();
        let id = s.create_from(&doc("要删")).unwrap();
        s.delete(&id, None).unwrap();
        crate::vfs::set_dataless(&s.trash_path(&id), true);
        let t = s.list_trash();
        assert_eq!(t.len(), 1);
        crate::vfs::write(s.trash_path(&id), b"not even xml").unwrap();
        // Read, it would not parse either: what shows comes from the index.
        assert_eq!(s.list_trash()[0].id, id);
    }

    /// v0.1.14's `image_path_in` (before docs/FORMAT.md's rule, without its
    /// moved-library fallback): what an older build makes of a source.
    fn image_path_v0_1_14(root: &Path, src: &str) -> Option<PathBuf> {
        let p = PathBuf::from(src.strip_prefix("file://").unwrap_or(src));
        if p.fs_is_absolute() { return Some(p) }
        (!src.is_empty() && !src.contains(':') && src.contains('/')).then(|| root.join(p))
    }

    /// The library's own files are written relative to it with `/`, whatever
    /// the system, and read back as paths here (docs/FORMAT.md); an older build
    /// finds them too, so FORMAT stays. Files outside the library and links stay
    /// as written.
    #[test]
    fn sources_are_written_relative_to_the_library() {
        let lib = Lib::new("portable");
        let mut s = lib.open();
        let assets = Store::assets_in(&lib.0);
        crate::vfs::create_dir_all(crate::paths::resolve("files/u", &assets)).unwrap();
        for f in ["p.png", "c.png", "files/u/说明.pdf"] { crate::vfs::write(crate::paths::resolve(f, &assets), b"x").unwrap(); }
        let outside = crate::vfs::temp_dir().join(format!("qmd-outside-{}.png", uuid::Uuid::new_v4()));
        crate::vfs::write(&outside, b"x").unwrap();
        let abs = |p: &Path| crate::docxml::esc_attr(&p.to_string_lossy());
        let page = |c: &str, p: &str, o: &str, f: &str| format!("<title cover=\"{c}\">页</title>\n<img src=\"{p}\"/>\n<img src=\"{o}\"/>\n<img src=\"https://example.com/b.png\"/>\n<p>附件：<source path=\"{f}\" name=\"说明.pdf\"/></p>\n<figure view-type=\"Preview\"><source path=\"{f}\" name=\"说明.pdf\"/></figure>\n");
        let doc = docxml::parse(&page(&abs(&assets.join("c.png")), &abs(&assets.join("p.png")), &abs(&outside), &abs(&crate::paths::resolve("files/u/说明.pdf", &assets)))).unwrap();
        let id = s.create_from(&doc).unwrap();
        let written = crate::vfs::read_to_string(s.path(&id)).unwrap();
        let canonical = |x: String| docxml::serialize(&docxml::parse(&x).unwrap());
        let want = canonical(page(".assets/c.png", ".assets/p.png", &abs(&outside), ".assets/files/u/说明.pdf"));
        assert_eq!(written, want);
        // Read back: the same paths as before it was written.
        let back = s.load_for_edit(&id, false).unwrap();
        assert_eq!(docxml::serialize(&back), docxml::serialize(&doc));
        // Saved unchanged, the same bytes.
        s.save(&id, &back).unwrap();
        assert_eq!(crate::vfs::read_to_string(s.path(&id)).unwrap(), want);
        // An older build reads each of the library's files where it is.
        for src in [".assets/c.png", ".assets/p.png", ".assets/files/u/说明.pdf"] {
            assert!(image_path_v0_1_14(&lib.0, src).is_some_and(|p| p.fs_is_file()), "{src}");
        }
        let _ = crate::vfs::remove_file(&outside);
    }

    /// A page from before the rule keeps absolute paths: this computer's from
    /// where the library was, another system's (`C:\…` on a Mac, `/Users/…` on
    /// Windows). Each finds the file in this library's images folder and is
    /// written relative at the next save; one found nowhere stays as written.
    #[test]
    fn absolute_sources_from_elsewhere_become_relative() {
        let lib = Lib::new("portable-old");
        let mut s = lib.open();
        let assets = Store::assets_in(&lib.0);
        crate::vfs::create_dir_all(crate::paths::resolve("files/u", &assets)).unwrap();
        for f in ["p.png", "c.png", "files/u/说明.pdf"] { crate::vfs::write(crate::paths::resolve(f, &assets), b"x").unwrap(); }
        let id = s.create_from(&doc("页")).unwrap();
        let moved = crate::vfs::temp_dir().join("qmd-library-was-here").join(".assets").join("files").join("u").join("说明.pdf");
        let page = |c: &str, p: &str, f: &str, gone: &str| format!("<title cover=\"{c}\">页</title>\n<img src=\"{p}\"/>\n<img src=\"{gone}\"/>\n<p>附件：<source path=\"{f}\" name=\"说明.pdf\"/></p>\n");
        let gone = "C:\\Users\\me\\Pictures\\别处.png";
        let old = page("C:\\Users\\me\\Documents\\Quiet.md\\.assets\\c.png", "/Users/me/Documents/Quiet.md/.assets/p.png", &crate::docxml::esc_attr(&moved.to_string_lossy()), gone);
        crate::vfs::write(s.path(&id), &old).unwrap();
        let here = s.load_for_edit(&id, false).unwrap();
        let shown: Vec<String> = here.cover.iter().map(|c| c.src.clone()).chain(here.asset_sources()).collect();
        let found = |f: &str| crate::paths::resolve(f, &assets).to_string_lossy().into_owned();
        assert_eq!(shown, [found("c.png"), found("p.png"), gone.to_string(), found("files/u/说明.pdf")]);
        // A board keeps its images' paths as written and reads them here too.
        for w in ["C:\\Users\\me\\Documents\\Quiet.md\\.assets\\p.png", "/Users/me/Documents/Quiet.md/.assets/p.png", ".assets\\p.png", ".assets/p.png"] {
            assert_eq!(image_path_in(&lib.0, w), Some(assets.join("p.png")), "{w}");
        }
        // Found nowhere: on Windows it is this system's path, kept for the caller to find missing.
        assert!(image_path_in(&lib.0, gone).is_none_or(|p| !p.fs_is_file()));
        s.save(&id, &here).unwrap();
        let want = docxml::serialize(&docxml::parse(&page(".assets/c.png", ".assets/p.png", ".assets/files/u/说明.pdf", &crate::docxml::esc_attr(gone))).unwrap());
        assert_eq!(crate::vfs::read_to_string(s.path(&id)).unwrap(), want);
    }

    /// docs/FORMAT.md's rule, for every writer: what a library holds names its
    /// own files relative, with `/`. Every file each one writes (a page, a
    /// Markdown page in a folder, one outside the library with its copies,
    /// 思源导入, a board, the library's own records) is scanned for a `\`, a
    /// drive letter or the library's absolute path. (A file outside the
    /// library keeps its absolute path: the test uses none.)
    #[test]
    fn no_writer_keeps_a_platform_path() {
        let lib = Lib::new("portable-guard");
        let mut s = lib.open();
        let assets = Store::assets_in(&lib.0);
        crate::vfs::create_dir_all(crate::paths::resolve("files/u", &assets)).unwrap();
        for f in ["p.png", "c.png", "files/u/说明.pdf"] { crate::vfs::write(crate::paths::resolve(f, &assets), b"x").unwrap(); }
        let abs = |f: &str| crate::docxml::esc_attr(&crate::paths::resolve(f, &assets).to_string_lossy());
        // As pasting, inserting and importing leave them in memory: absolute.
        let page = docxml::parse(&format!("<title cover=\"{}\">页</title>\n<img src=\"{}\"/>\n<p>附件：<source path=\"{f}\" name=\"说明.pdf\"/></p>\n<figure view-type=\"Preview\"><source path=\"{f}\" name=\"说明.pdf\"/></figure>\n", abs("c.png"), abs("p.png"), f = abs("files/u/说明.pdf"))).unwrap();
        let folder = s.create_folder("工作", None).unwrap();
        s.create_from(&page).unwrap();
        s.create_from_in(&page, Some(&folder)).unwrap();
        // Markdown in the library: 导入 Markdown (its pictures copied into the library first), at
        // the top and in a folder, and one moved into a folder afterwards (the move rewrites it).
        s.create_markdown(&page, "").unwrap();
        s.create_markdown_in(&page, "", Some(&folder)).unwrap();
        let md = s.create_markdown(&page, "").unwrap();
        s.set_parent(&md, Some(&folder)).unwrap();
        // A Markdown file outside the library: the library's files copied beside it.
        let outside = crate::vfs::temp_dir().join(format!("qmd-outside-{}", uuid::Uuid::new_v4()));
        crate::vfs::create_dir_all(&outside).unwrap();
        let out = crate::markdown::write_file_with(&page, "", &crate::markdown::Options { library_dir: Some(&lib.0), base: Some(&outside), name: Some("外面"), ..Default::default() });
        crate::vfs::write(outside.join("外面.md"), &out.text).unwrap();
        assert!(!out.files.is_empty());
        for (from, rel) in &out.files { let to = crate::paths::resolve(rel, &outside); crate::vfs::create_dir_all(to.parent().unwrap()).unwrap(); crate::vfs::copy(from, to).unwrap(); }
        // 思源导入.
        let siyuan = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/siyuan/data");
        s.begin_batch().unwrap();
        crate::siyuan::import(&mut s, &crate::siyuan::open(&siyuan).unwrap(), None, &Default::default(), &mut crate::siyuan::StaticTables).unwrap();
        s.end_batch().unwrap();
        // A board's pictures, as inserting one keeps it.
        let mut board = crate::whiteboard::Board::default();
        let path = crate::paths::portable_in(&assets.join("p.png"), &lib.0);
        board.files.insert("f".into(), crate::board::types::FileData { id: "f".into(), mime_type: "image/png".into(), data_url: None, path, created: 0, last_retrieved: None, version: None, extra: Default::default() });
        s.create_board_in("画板", &board, None).unwrap();
        fn files(dir: &Path, out: &mut Vec<PathBuf>) {
            for e in crate::vfs::read_dir(dir).unwrap().flatten() { let p = e.path(); if p.fs_is_dir() { files(&p, out) } else { out.push(p) } }
        }
        let mut written = vec![];
        files(&lib.0, &mut written);
        files(&outside, &mut written);
        let drive = regex::Regex::new(r#"(^|[^A-Za-z0-9])[A-Za-z]:[\\/]"#).unwrap();
        let texts = ["xml", "md", "excalidraw", "json"];
        let source = regex::Regex::new(r#"\]\(([^)\s]+)\)|src="([^"]+)""#).unwrap();
        let (mut scanned, mut mds) = (0, 0);
        for p in written.iter().filter(|p| p.extension().is_some_and(|e| texts.contains(&&*e.to_string_lossy()))) {
            let text = crate::vfs::read_to_string(p).unwrap();
            // JSON escapes its own (`\"`, `\n`, `\u…`); a path's `\` is `\\` there.
            let slash = if p.extension().is_some_and(|e| e != "md" && e != "xml") { text.contains("\\\\") } else { text.contains('\\') };
            assert!(!slash && !drive.is_match(&text) && !text.contains(&*lib.0.to_string_lossy()), "{}:\n{text}", p.display());
            // A Markdown file's pictures and files are relative to it, as other editors read them.
            if p.extension().is_some_and(|e| e == "md") {
                for src in source.captures_iter(&text).map(|c| c.get(1).or(c.get(2)).unwrap().as_str()).filter(|s| !s.contains("://")) {
                    assert!(crate::paths::resolve(&crate::markdown::percent_decode(src), p.parent().unwrap()).fs_is_file(), "{src} in {}:\n{text}", p.display());
                    mds += 1;
                }
            }
            scanned += 1;
        }
        assert!(scanned >= 10 && mds >= 12, "{scanned} {mds}");
        let _ = crate::vfs::remove_dir_all(&outside);
    }
}
