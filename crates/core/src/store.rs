//! On-disk library: one DocxXML file per document plus a small JSON index
//! (pinned flag, timestamps) under `~/Documents/Quiet.md`.

use crate::docxml;
use crate::model::Document;
use anyhow::{Context, Result};
use std::io::Write;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use crate::time::now;
use std::time::UNIX_EPOCH;
#[path = "file_history.rs"] mod file_history;
pub use file_history::{FileState, FileDelta, FileChanges};

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_order: Option<u64>,
    pub created: u64,
    pub modified: u64,
    #[serde(default)]
    pub pinned: bool,
    /// Last time the document was opened, for 最近访问.
    #[serde(default)]
    pub opened: u64,
    /// 页宽设置, remembered per document.
    #[serde(default, skip_serializing_if = "PageWidth::is_standard")]
    pub width: PageWidth,
    /// Catalogue visibility is remembered independently of document headings.
    #[serde(default = "catalogue_closed_default")]
    pub catalogue_closed: bool,
    /// 父文档: the page this one hangs under in the 我的文档库 tree.
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
    /// A Markdown file, edited as Markdown: `docs/<id>.md`, or `path`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub markdown: bool,
    /// 本机文件: a Markdown file opened where it is, outside the library.
    /// The library never moves, trashes or deletes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The title of a Markdown file in the library (the file has none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
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
            markdown: false, path: None, name: None,
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

/// Window-level UI state kept next to the library (`settings.json`).
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
}

/// A document in the 回收站, as kept in `trash/index.json`.
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
    index: HashMap<String, Meta>,
    trash: HashMap<String, Trashed>,
    /// Problems found while opening the library (a damaged index recovered
    /// from its backup …), for the window to tell the user once.
    pub warnings: Vec<String>,
}

/// The parts of an `Entry` that come from the document file.
#[derive(Clone)]
struct Parsed { title: String, icon: Option<String>, body: std::sync::Arc<str>, reminders: Vec<crate::model::InlineReminder>, links: Vec<crate::model::DocLink> }

/// One write to the indexes: several copies of the app may share a library
/// (builds tested side by side), so this is a short mutex, not a
/// single-instance guard. Released when the write is done.
struct WriteLock(std::fs::File);

impl Drop for WriteLock {
    fn drop(&mut self) { let _ = self.0.unlock(); }
}

fn lock_root(root: &std::path::Path) -> Result<WriteLock> {
    let file = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(root.join(".lock"))?;
    file.lock()?;
    Ok(WriteLock(file))
}

/// Version of the on-disk layout, kept in `format.json`. Bump it when a
/// change would make older builds misread the library.
pub const FORMAT: u32 = 3;
/// What a library is marked with until it holds a Markdown document or a
/// 本机文件 (FORMAT 3): until then 0.1.7 and older still open it.
const BASE_FORMAT: u32 = 2;

#[path = "folders.rs"]
mod folders;

#[derive(Serialize, Deserialize)]
struct FormatMarker {
    format: u32,
}

/// Write `bytes` to `path` through a temporary file, so a crash leaves
/// either the old or the new contents.
fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let tmp = PathBuf::from(tmp);
    let result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)?;
        #[cfg(unix)]
        if let Some(parent) = path.parent() { std::fs::File::open(parent)?.sync_all()?; }
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
            warnings.push(format!("{what}已损坏且没有可用备份，部分文档的置顶、层级等信息已重置（原文件保留为 {name}）"));
            T::default()
        }
    }
}

/// A JSON file as another copy of the app left it, if it can be read right now.
fn reread<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Option<T> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Atomically replace a JSON index, keeping the version it replaces as
/// `<name>.bak` for `load_json`.
fn save_json<T: Serialize>(path: &std::path::Path, v: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(v)?;
    if path.exists() {
        write_atomic(&with_suffix(path, ".bak"), &std::fs::read(path)?)?;
    }
    write_atomic(path, &bytes)
}

/// Library-owned assets have the same reference on every replica.
pub fn asset_source(path: &std::path::Path) -> String {
    #[cfg(not(feature = "sqlite-store"))]
    return path.to_string_lossy().into_owned();
    #[cfg(feature = "sqlite-store")]
    if !Store::sqlite_session() { return path.to_string_lossy().into_owned(); }
    #[cfg(feature = "sqlite-store")]
    let root = Store::root_dir();
    #[cfg(feature = "sqlite-store")]
    {
        path.strip_prefix(&root).ok().filter(|path| path.starts_with("assets"))
            .unwrap_or(path).to_string_lossy().into_owned()
    }
}

/// Image path for an image block source (only local files are displayed).
pub fn image_path(src: &str) -> Option<PathBuf> {
    let p = PathBuf::from(src.strip_prefix("file://").unwrap_or(src));
    if p.is_absolute() {
        return Some(p);
    }
    // Relative to the library (`assets/…`); not a URI or a cloud token.
    (!src.is_empty() && !src.contains(':') && src.contains('/')).then(|| Store::root_dir().join(p))
}

/// Export only files belonging to this library, including after symlink resolution.
pub fn export_path(src: &str) -> Option<PathBuf> {
    export_path_in(&Store::root_dir(), src)
}

pub fn export_path_in(root: &std::path::Path, src: &str) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;
    let raw = std::path::Path::new(src.strip_prefix("file://").unwrap_or(src));
    let path = if raw.is_absolute() { raw.to_path_buf() } else { root.join(raw) }.canonicalize().ok()?;
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

impl Store {
    /// Select a library before opening the first store in this process.
    pub fn set_session_root(root: PathBuf) -> Result<()> {
        SESSION_ROOT.set(root).map_err(|_| anyhow::anyhow!("Library already selected"))
    }

    /// Startup-selected library, `QUIET_MD_HOME`, else a folder of
    /// its own in the user's Documents, where they can see and back it up.
    pub fn root_dir() -> PathBuf {
        if let Some(root) = SESSION_ROOT.get() { return root.clone() }
        std::env::var_os("QUIET_MD_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs::document_dir().or_else(|| dirs::home_dir().map(|h| h.join("Documents"))).unwrap_or_else(|| PathBuf::from(".")).join("Quiet.md"))
    }

    /// Where pasted / inserted images are copied to.
    pub fn assets_dir() -> PathBuf {
        let d = Self::root_dir().join("assets");
        let _ = std::fs::create_dir_all(&d);
        d
    }

    pub fn open() -> Result<Self> {
        #[cfg(feature = "sqlite-store")]
        if Self::sqlite_session() { return Self::open_sqlite_at(Self::root_dir()); }
        Self::open_at(Self::root_dir())
    }

    pub fn open_at(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(root.join("docs"))?;
        let mut warnings = vec![];
        let marker = root.join("format.json");
        match std::fs::read(&marker).ok().and_then(|b| serde_json::from_slice::<FormatMarker>(&b).ok()) {
            Some(m) if m.format > FORMAT => anyhow::bail!("这个文档库由更新版本的 Quiet.md 创建，请升级后再打开"),
            Some(_) => {}
            None => write_atomic(&marker, &serde_json::to_vec_pretty(&FormatMarker { format: BASE_FORMAT })?)?,
        }
        let index = load_json(&root.join("index.json"), "文档索引", &mut warnings);
        let trash = load_json(&root.join("trash").join("index.json"), "回收站索引", &mut warnings);
        let mut store = Store {
            #[cfg(feature = "sqlite-store")]
            sqlite: None,
            file_changes: vec![], root, index, trash, warnings, loaded: Default::default(), copied: Default::default(), parsed: HashMap::new()
        };
        store.restore_recoveries();
        store.migrate_folders()?;
        Ok(store)
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
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.root.hash(&mut hash);
        dirs::data_local_dir().unwrap_or_else(std::env::temp_dir)
            .join("quiet-md/recovery").join(format!("{:016x}", hash.finish()))
    }

    pub fn recover_later(&self, doc: &Document) -> Result<()> {
        let dir = self.recovery_dir();
        std::fs::create_dir_all(&dir)?;
        write_atomic(&dir.join(format!("{}.xml", uuid::Uuid::new_v4())), docxml::serialize(doc).as_bytes())
    }

    fn path(&self, id: &str) -> PathBuf {
        match self.index.get(id) {
            Some(Meta { path: Some(p), .. }) => PathBuf::from(p),
            Some(m) if m.markdown => self.root.join("docs").join(format!("{id}.md")),
            _ => self.root.join("docs").join(format!("{id}.xml")),
        }
    }

    fn read_body(&self, id: &str) -> Result<String> {
        Ok(std::fs::read_to_string(self.path(id))?)
    }

    pub fn is_markdown(&self, id: &str) -> bool { self.index.get(id).is_some_and(|m| m.markdown) }

    /// The folder of a 本机文件, which its images are relative to.
    fn base(&self, id: &str) -> Option<PathBuf> {
        self.index.get(id)?.path.as_ref().and_then(|p| std::path::Path::new(p).parent()).map(PathBuf::from)
    }

    /// A Markdown document's title: its name in the library, its file name outside.
    fn markdown_title(&self, id: &str) -> String {
        let Some(m) = self.index.get(id) else { return String::new() };
        match &m.path {
            Some(p) => std::path::Path::new(p).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            None => m.name.clone().unwrap_or_default(),
        }
    }

    /// `text` (a file's contents) as a document.
    fn parse_doc(&self, id: &str, text: &str) -> Result<Document> {
        if !self.is_markdown(id) { return docxml::parse(text); }
        Ok(crate::markdown::parse_file(text, &self.markdown_title(id), self.base(id).as_deref()))
    }

    /// `doc` as `id`'s file contents; a Markdown file keeps the front matter it has on disk.
    fn serialize_doc(&self, id: &str, doc: &Document, disk: Option<&str>) -> String { self.serialize_with_copies(id, doc, disk).0 }

    /// `serialize_doc`, and the library files a 本机文件 needs copied beside
    /// it: (source, path relative to its folder).
    fn serialize_with_copies(&self, id: &str, doc: &Document, disk: Option<&str>) -> (String, Vec<(PathBuf, String)>) {
        if !self.is_markdown(id) { return (docxml::serialize(doc), vec![]); }
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

    /// Mark the library FORMAT 3 before its first Markdown document or
    /// 本机文件: older builds would drop their `markdown` / `path` / `name`.
    fn require_markdown_format(&self) -> Result<()> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() { return Ok(()); }
        let path = self.root.join("format.json");
        let marker: FormatMarker = serde_json::from_slice(&std::fs::read(&path)?)?;
        if marker.format < FORMAT { write_atomic(&path, &serde_json::to_vec_pretty(&FormatMarker { format: FORMAT })?)?; }
        Ok(())
    }

    /// Take the library's write lock and adopt what other copies wrote since
    /// this one last looked. Every change is written through, so memory holds
    /// nothing the disk does not. A lock failure must never allow an unguarded write.
    fn begin_write(&mut self) -> Result<WriteLock> {
        let lock = lock_root(&self.root)?;
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() {
            self.refresh_sqlite()?;
            return Ok(lock);
        }
        let marker: FormatMarker = serde_json::from_slice(&std::fs::read(self.root.join("format.json"))?)?;
        anyhow::ensure!(marker.format <= FORMAT, "这个文档库由更新版本的 Quiet.md 创建，请升级后再打开");
        // An unreadable index is not judged here (that is `open`'s job): the
        // version in memory is written back unchanged.
        if let Some(index) = reread(&self.root.join("index.json")) { self.index = index; }
        if let Some(trash) = reread(&self.root.join("trash").join("index.json")) { self.trash = trash; }
        Ok(lock)
    }

    fn save_index(&self) -> Result<()> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() { return self.save_sqlite_index(); }
        save_json(&self.root.join("index.json"), &self.index)
    }

    /// All documents found on disk. A `.xml` file dropped into `docs/` by hand
    /// is picked up too.
    pub fn list(&mut self) -> Vec<Entry> {
        let _lock = match self.begin_write() {
            Ok(lock) => Some(lock),
            Err(e) => { self.warnings.push(format!("无法锁定文档库：{e}")); None }
        };
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() {
            return self.list_sqlite().unwrap_or_else(|err| {
                self.warnings.push(format!("无法读取文档索引：{err}"));
                vec![]
            });
        }
        let mut out = vec![];
        let Ok(rd) = std::fs::read_dir(self.root.join("docs")) else { return out };
        let mut dirty = false;
        let local: Vec<(String, PathBuf)> = self.index.iter().filter_map(|(id, m)| Some((id.clone(), PathBuf::from(m.path.as_ref()?)))).collect();
        let files = rd.flatten().map(|e| e.path()).filter_map(|p| {
            let markdown = match p.extension().and_then(|s| s.to_str()) { Some("xml") => false, Some("md") => true, _ => return None };
            Some((p.file_stem()?.to_str()?.to_string(), p, markdown))
        }).collect::<Vec<_>>();
        for (id, p, markdown) in files.into_iter().chain(local.into_iter().map(|(id, p)| (id, p, true))) {
            let md = std::fs::metadata(&p).ok();
            if md.is_none() && self.index.get(&id).is_some_and(Meta::local) { continue; }
            let stamp = md.as_ref().and_then(|m| Some((m.modified().ok()?, m.len())));
            let parsed = match self.parsed.get(&id).filter(|c| stamp == Some((c.0, c.1))) {
                Some(c) => c.2.clone(),
                None => {
                    let Ok(xml) = std::fs::read_to_string(&p) else { continue };
                    if markdown && !self.index.get(&id).is_some_and(|m| m.markdown) && _lock.is_some() && self.require_markdown_format().is_ok() {
                        // A .md dropped into docs/ by hand.
                        let t = now();
                        let name = Some("未命名".to_string());
                        self.index.entry(id.clone()).or_insert(Meta { created: t, modified: t, ..Default::default() }).markdown = true;
                        if let Some(m) = self.index.get_mut(&id) { m.name = m.name.take().or(name); }
                        dirty = true;
                    }
                    let doc = self.parse_doc(&id, &xml).unwrap_or_default();
                    let parsed = Parsed { title: doc.title.text.clone(), icon: doc.icon.clone(), body: doc.plain_text().into(), reminders: doc.reminders(), links: doc.doc_links() };
                    if let Some((t, len)) = stamp { self.parsed.insert(id.clone(), (t, len, parsed.clone())); }
                    parsed
                }
            };
            let meta = self.index.entry(id.clone()).or_insert_with(|| {
                dirty = true;
                let m = md
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or_else(now);
                Meta { created: m, modified: m, ..Default::default() }
            });
            let Parsed { mut title, icon, body, reminders, links } = parsed;
            if meta.markdown { title = self.markdown_title(&id); }
            let meta = self.index[&id].clone();
            out.push(Entry { id, meta, title, icon, body, reminders, links });
        }
        // Folders in id order: the index is a HashMap, whose order changes every time it is re-read.
        let mut folders: Vec<(&String, &Meta)> = self.index.iter().filter(|(_, m)| m.folder.is_some()).collect();
        folders.sort_by(|a, b| a.0.cmp(b.0));
        for (id, meta) in folders {
            let title = meta.folder.clone().unwrap_or_default();
            out.push(Entry { id: id.clone(), meta: meta.clone(), title, icon: None, body: Default::default(), reminders: vec![], links: vec![] });
        }
        let seen: std::collections::HashSet<&str> = out.iter().map(|e| e.id.as_str()).collect();
        self.parsed.retain(|id, _| seen.contains(id.as_str()));
        if dirty && _lock.is_some() {
            let _ = self.save_index();
        }
        out
    }

    pub fn load(&self, id: &str) -> Result<Document> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite_manages(id) { return self.load_sqlite_doc(id, false); }
        self.parse_doc(id, &self.read_body(id)?)
    }

    /// Only opening an editor establishes a new save baseline; previews must
    /// not hide external changes from an already open editor.
    pub fn load_for_edit(&self, id: &str, shared: bool) -> Result<Document> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite_manages(id) { return self.load_sqlite_doc(id, !shared); }
        let xml = self.read_body(id)?;
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

    /// `save` for callers that already hold the write lock.
    fn save_now(&mut self, id: &str, doc: &Document) -> Result<Meta> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite_manages(id) { return self.save_sqlite_locked(id, doc, None, None, |_, _| Ok(())).map(|(meta, _)| meta); }
        let disk = if self.is_markdown(id) || self.loaded.borrow().contains_key(id) { std::fs::read_to_string(self.path(id)).ok() } else { None };
        let (xml, copies) = self.serialize_with_copies(id, doc, disk.as_deref());
        if let Some(previous) = self.loaded.borrow().get(id) {
            let disk = disk.as_deref().ok_or_else(|| anyhow::anyhow!("无法读取文档"))?;
            let message = if self.index.get(id).is_some_and(Meta::local) { "文件已在别处被修改，未覆盖磁盘内容。请另存副本或重新打开文件" } else { "文档已被另一个实例修改，未覆盖磁盘内容。请另存副本或重新载入磁盘版本" };
            anyhow::ensure!(disk == previous || disk == xml, "{message}");
        }
        let title = self.is_markdown(id).then(|| doc.title.text.trim().to_string());
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
            write_atomic(&self.path(id), xml.as_bytes())?;
        }
        self.loaded.borrow_mut().insert(id.to_string(), if same { disk.unwrap_or(xml) } else { xml });
        if let Some(title) = title.filter(|t| *t != self.markdown_title(id)) { self.retitle_markdown(id, &title)?; }
        let t = now();
        let m = self.index.entry(id.to_string()).or_insert(Meta { created: t, modified: t, ..Default::default() });
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
    pub fn create_from(&mut self, doc: &Document) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let _lock = self.begin_write()?;
        self.save_now(&id, doc)?;
        self.record_file_change(&id, None);
        Ok(id)
    }

    /// A Markdown file's new title: its name in the library; outside, the
    /// file is renamed in its folder (kept as it is when the name is taken).
    fn retitle_markdown(&mut self, id: &str, title: &str) -> Result<()> {
        let Some(m) = self.index.get(id) else { return Ok(()) };
        let Some(old) = m.path.clone().map(PathBuf::from) else {
            self.index.get_mut(id).unwrap().name = Some(title.to_string());
            return Ok(());
        };
        let name = title.replace(['/', '\\', ':'], "_");
        let name = name.trim().trim_start_matches('.');
        if name.is_empty() { return Ok(()); }
        let ext = old.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "md".into());
        let new = old.with_file_name(format!("{name}.{ext}"));
        if new == old || new.try_exists()? { return Ok(()); }
        std::fs::rename(&old, &new)?;
        self.index.get_mut(id).unwrap().path = Some(new.to_string_lossy().into_owned());
        Ok(())
    }

    /// A new Markdown document in the library (导入, 创建副本): `front`
    /// matter, then `doc`; titled `doc`'s title.
    pub fn create_markdown(&mut self, doc: &Document, front: &str) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let _lock = self.begin_write()?;
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() {
            let meta = Meta { created: now(), markdown: true, ..Default::default() };
            self.save_sqlite_locked(&id, doc, Some(meta), Some(front), |_, _| Ok(()))?;
            self.record_file_change(&id, None);
            return Ok(id);
        }
        self.require_markdown_format()?;
        let t = now();
        self.index.insert(id.clone(), Meta { created: t, modified: t, markdown: true, name: Some(doc.title.text.trim().to_string()), ..Default::default() });
        let text = format!("{front}{}", crate::markdown::write_file(doc, "", None));
        if let Err(e) = write_atomic(&self.path(&id), text.as_bytes()).and_then(|_| self.save_index()) { self.index.remove(&id); return Err(e); }
        self.record_file_change(&id, None);
        Ok(id)
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
    /// it already has when it was opened before.
    pub fn open_local(&mut self, path: &std::path::Path) -> Result<String> {
        let path = path.canonicalize()?;
        // Windows canonical paths are `\\?\C:\…`: keep the ordinary form, which
        // the Explorer and the title show.
        let path = match path.to_str().and_then(|p| p.strip_prefix(r"\\?\")).filter(|p| !p.starts_with("UNC\\")) { Some(p) => PathBuf::from(p), None => path };
        anyhow::ensure!(path.is_file(), "文件不存在");
        let _lock = self.begin_write()?;
        let key = path.to_string_lossy().into_owned();
        let t = now();
        if let Some((id, m)) = self.index.iter_mut().find(|(_, m)| m.path.as_deref() == Some(key.as_str())) {
            m.opened = t;
            let id = id.clone();
            self.save_index()?;
            return Ok(id);
        }
        self.require_markdown_format()?;
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

    /// Move a document to the 回收站; `root` is the page whose deletion
    /// takes it along (`None`: deleted itself).
    pub fn delete(&mut self, id: &str, root: Option<&str>) -> Result<()> {
        let _lock = self.begin_write()?;
        // 本机文件: only taken off the list; the file is the user's.
        if self.index.get(id).is_some_and(Meta::local) {
            self.index.remove(id);
            self.loaded.borrow_mut().remove(id);
            return self.save_index();
        }
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() { return self.delete_sqlite(id, root); }
        let before = self.file_state(id);
        let target = self.trash_path(id);
        anyhow::ensure!(!target.try_exists()?, "回收站已存在同名文档，未覆盖：{id}");
        std::fs::create_dir_all(self.root.join("trash"))?;
        // Persist destination metadata before moving content. An interrupted
        // move leaves an extra index entry, never an unindexed trashed file.
        let mut trash = self.trash.clone();
        let meta = self.index.get(id).cloned().unwrap_or_default();
        trash.insert(id.to_string(), Trashed { meta, deleted: now(), root: root.map(str::to_string) });
        save_json(&self.root.join("trash/index.json"), &trash)?;
        self.trash = trash;
        if self.index.get(id).is_none_or(|m| m.folder.is_none()) { std::fs::rename(self.path(id), target)?; }
        self.index.remove(id);
        // Content has moved: cleanup failure must not report deletion as failed.
        if let Err(e) = self.save_index() {
            self.warnings.push(format!("文档已移入回收站，文档索引清理失败：{e}"));
        }
        self.record_file_change(id, before);
        Ok(())
    }

    pub fn trash_dir(&self) -> PathBuf {
        let d = self.root.join("trash");
        let _ = std::fs::create_dir_all(&d);
        d
    }

    fn trash_path(&self, id: &str) -> PathBuf {
        let markdown = self.trash.get(id).map(|t| &t.meta).or_else(|| self.index.get(id)).is_some_and(|m| m.markdown);
        self.root.join("trash").join(format!("{id}.{}", if markdown { "md" } else { "xml" }))
    }

    fn save_trash(&self) -> Result<()> {
        save_json(&self.trash_dir().join("index.json"), &self.trash)
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
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() {
            return self.list_sqlite_trash(_lock.is_some()).unwrap_or_else(|err| {
                self.warnings.push(format!("无法读取回收站：{err}"));
                vec![]
            });
        }
        let dir = self.trash_dir();
        let mut dirty = false;
        let mut on_disk = std::collections::HashSet::new();
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
                    let meta = self.index.get(&id).cloned().unwrap_or(Meta { created: t, modified: t, markdown, name: markdown.then(|| "未命名".into()), ..Default::default() });
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
            match std::fs::remove_file(self.trash_path(&id)) {
                Ok(()) => { self.trash.remove(&id); dirty = true; }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => { self.trash.remove(&id); dirty = true; }
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
                let doc = if t.meta.markdown { Document::default() } else { std::fs::read_to_string(self.trash_path(id)).ok().and_then(|x| docxml::parse(&x).ok()).unwrap_or_default() };
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
        let group = self.trash_group(id);
        let mut index = self.index.clone();
        let parent = self.trash.get(id).and_then(|t| t.meta.parent.clone());
        let moved = parent.is_some_and(|p| !self.index_has(&p));
        for k in &group {
            let Some(t) = self.trash.get(k) else { continue };
            anyhow::ensure!(!self.path(k).try_exists()?, "文档库已存在同名文档，未覆盖：{k}");
            let mut meta = t.meta.clone();
            if k == id && moved { meta.parent = None; }
            index.insert(k.clone(), meta);
        }
        // Stage the destination metadata before the first move. On partial
        // failure, both sides retain enough metadata to retry after reopening.
        save_json(&self.root.join("index.json"), &index)?;
        self.index = index;
        for k in &group {
            if !self.trash.contains_key(k) { continue; }
            if self.trash.get(k).is_none_or(|t| t.meta.folder.is_none()) { std::fs::rename(self.trash_path(k), self.path(k)).with_context(|| format!("恢复文档 {k} 失败，已恢复的文档保留在文档库"))?; }
            self.trash.remove(k);
            self.record_file_change(k, None);
        }
        if let Err(e) = self.save_trash() {
            self.warnings.push(format!("文档已恢复，回收站索引清理失败：{e}"));
        }
        drop(_lock);
        self.migrate_folders()?;
        Ok(moved)
    }

    fn index_has(&self, id: &str) -> bool {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite_manages(id) { return self.index.contains_key(id); }
        self.index.get(id).is_some_and(|m| m.folder.is_some() || self.path(id).exists())
    }

    /// 彻底删除 a page from the 回收站, with the pages that went with it.
    pub fn purge(&mut self, id: &str) -> Result<()> {
        let _lock = self.begin_write()?;
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() { return self.purge_sqlite(self.trash_group(id)); }
        for k in self.trash_group(id) {
            match std::fs::remove_file(self.trash_path(&k)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            self.trash.remove(&k);
        }
        self.save_trash()
    }

    /// Copy a DocxXML file into the library under a fresh id.
    pub fn import(&mut self, path: &std::path::Path) -> Result<String> {
        let xml = std::fs::read_to_string(path)?;
        self.create_from(&docxml::parse(&xml)?)
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
            f(m);
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

    pub fn settings(&self) -> Settings {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() { return self.load_sqlite_settings().unwrap_or_default(); }
        std::fs::read(self.root.join("settings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        #[cfg(feature = "sqlite-store")]
        if self.sqlite.is_some() { return self.save_sqlite_settings(s); }
        write_atomic(&self.root.join("settings.json"), &serde_json::to_vec_pretty(s)?)
    }

    /// 重命名 a document that isn't open: rewrite its title on disk.
    pub fn rename(&mut self, id: &str, title: &str) -> Result<Meta> {
        let _lock = self.begin_write()?;
        let before = self.file_state(id);
        if self.index.get(id).is_some_and(|m| m.folder.is_some()) {
            let previous = self.index.clone();
            let m = self.index.get_mut(id).unwrap();
            m.folder = Some(title.to_string());
            m.modified = now();
            let meta = m.clone();
            if let Err(e) = self.save_index() { self.index = previous; return Err(e); }
            self.record_file_change(id, before);
            return Ok(meta);
        }
        #[cfg(feature = "sqlite-store")]
        if self.sqlite_manages(id) {
            let mut doc = self.load(id)?;
            doc.title = crate::model::RichText::plain(title);
            let meta = self.save_now(id, &doc)?;
            self.record_file_change(id, before);
            return Ok(meta);
        }
        if self.is_markdown(id) {
            // The file is left as it is: only its name changes.
            self.retitle_markdown(id, title)?;
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

    /// Note that `id` was just opened.
    pub fn touch(&mut self, id: &str) -> Option<Meta> {
        let _lock = self.begin_write().ok()?;
        let m = self.index.get_mut(id)?;
        m.opened = now();
        let m = m.clone();
        let _ = self.save_index();
        Some(m)
    }

    /// 移动到: hang `id` under `parent` (`None` = top level of 我的文档库), as its last child.
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
    }

    impl Drop for Lib {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn doc(title: &str) -> Document {
        Document { title: crate::model::RichText::plain(title), ..Default::default() }
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
        let file = lib.0.join("docs").join(format!("{id}.md"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "---\na: 1\n---\nHello *there*\n");
        let e = s.list().into_iter().find(|e| e.id == id).unwrap();
        assert!(e.meta.markdown);
        assert_eq!(e.title, "Note");
        // Saving rewrites the body only; the front matter stays.
        let mut loaded = s.load_for_edit(&id, false).unwrap();
        loaded.blocks.push(crate::model::Block::text("more"));
        s.save(&id, &loaded).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "---\na: 1\n---\nHello *there*\n\nmore\n");
        // A title change is the name, not the file.
        d = s.load(&id).unwrap();
        d.title = crate::model::RichText::plain("Renamed");
        s.save(&id, &d).unwrap();
        assert_eq!(s.load(&id).unwrap().title.text, "Renamed");
        assert!(!std::fs::read_to_string(&file).unwrap().contains("Renamed"));
        // Through the 回收站 and back as Markdown.
        s.delete(&id, None).unwrap();
        assert!(lib.0.join("trash").join(format!("{id}.md")).is_file());
        assert_eq!(s.list_trash()[0].title, "Renamed");
        s.restore(&id).unwrap();
        assert!(file.is_file());
        assert_eq!(s.load(&id).unwrap().title.text, "Renamed");
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
        std::fs::create_dir_all(lib.0.join("assets")).unwrap();
        let pasted = lib.0.join("assets/3e5c9916-7c1f-4f7d-af25-74dd4a8a241e.png");
        let picked = lib.0.join("assets/1a2b3c4d-截图说明.png");
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
        // A page left by another build whose parent page is gone (no file, no folder).
        let stale = s.create_from(&doc("old")).unwrap();
        let ghost = uuid::Uuid::new_v4().to_string();
        s.index.insert(ghost.clone(), Meta::default());
        s.index.get_mut(&stale).unwrap().parent = Some(ghost);
        s.save_index().unwrap();
        s.list();
        let journal = s.track_file_changes();
        s.move_in_tree(&id, Some(&folder), None).unwrap();
        let changes = journal.take();
        s.apply_file_history(&changes, false).unwrap();
        assert!(s.index[&id].parent.is_none());
        s.apply_file_history(&changes, true).unwrap();
        assert_eq!(s.index[&id].parent.as_deref(), Some(folder.as_str()));
    }

    #[test]
    fn failed_lock_never_writes() {
        let lib = Lib::new("lock-failure");
        let mut s = lib.open();
        let id = s.create_from(&doc("original")).unwrap();
        std::fs::remove_file(lib.0.join(".lock")).unwrap();
        std::fs::create_dir(lib.0.join(".lock")).unwrap();
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
        assert!(lib.0.join("format.json").is_file());
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
        std::fs::write(lib.0.join("index.json"), b"{ not json").unwrap();
        let mut s = lib.open();
        assert_eq!(s.warnings.len(), 1, "{:?}", s.warnings);
        assert!(s.warnings[0].contains("已从备份恢复"));
        assert!(s.list()[0].meta.pinned);
        // The damaged file is kept for inspection, not overwritten.
        let kept = std::fs::read_dir(&lib.0).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("index.json.corrupt-")).count();
        assert_eq!(kept, 1);
    }

    #[test]
    fn corrupt_index_without_backup_still_lists_documents() {
        let lib = Lib::new("nobak");
        let mut s = lib.open();
        s.create_from(&doc("一")).unwrap();
        std::fs::write(lib.0.join("index.json"), b"garbage").unwrap();
        let _ = std::fs::remove_file(lib.0.join("index.json.bak"));
        let mut s = lib.open();
        assert!(s.warnings[0].contains("没有可用备份"));
        assert_eq!(s.list().len(), 1);
    }

    #[test]
    fn pages_with_children_become_folders_in_their_old_order() {
        // A FORMAT 1 library: 父 holds 子一 (older) and 子二 (newer).
        let lib = Lib::new("migrate");
        let mut s = lib.open();
        let p = s.create_from(&doc("父")).unwrap();
        let a = s.create_from(&doc("子一")).unwrap();
        let b = s.create_from(&doc("子二")).unwrap();
        drop(s);
        let path = lib.0.join("index.json");
        let mut index: HashMap<String, Meta> = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        for (k, created) in [(&a, 100), (&b, 200)] {
            let m = index.get_mut(k).unwrap();
            m.parent = Some(p.clone());
            m.created = created;
            m.order = 0;
        }
        std::fs::write(&path, serde_json::to_vec(&index).unwrap()).unwrap();
        std::fs::write(lib.0.join("format.json"), b"{\"format\": 1}").unwrap();

        let mut s = lib.open();
        let list = s.list();
        let folder = list.iter().find(|e| e.meta.folder.as_deref() == Some("父")).expect("父 became a folder");
        assert!(folder.meta.parent.is_none());
        let mut inside: Vec<&Entry> = list.iter().filter(|e| e.meta.parent.as_deref() == Some(folder.id.as_str())).collect();
        inside.sort_by_key(|e| (e.meta.place(), e.id.clone()));
        let titles: Vec<&str> = inside.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, ["父", "子一", "子二"], "the page first, then its children oldest first");
        assert!(lib.0.join("index.before-folders.json").exists());
        assert!(String::from_utf8(std::fs::read(lib.0.join("format.json")).unwrap()).unwrap().contains(&BASE_FORMAT.to_string()));
    }

    /// A library is marked FORMAT 3 only once it holds Markdown, so older
    /// builds keep opening libraries that never used it.
    #[test]
    fn markdown_raises_the_format_only_when_used() {
        let format = |lib: &Lib| serde_json::from_slice::<FormatMarker>(&std::fs::read(lib.0.join("format.json")).unwrap()).unwrap().format;
        let lib = Lib::new("lazy-format");
        let mut s = lib.open();
        s.create_from(&doc("plain")).unwrap();
        s.list();
        assert_eq!(format(&lib), BASE_FORMAT);
        s.create_markdown(&doc("md"), "").unwrap();
        assert_eq!(format(&lib), FORMAT);
        drop(s);
        // Reopening keeps it.
        lib.open().list();
        assert_eq!(format(&lib), FORMAT);
        let other = Lib::new("lazy-format-local");
        std::fs::create_dir_all(&other.0).unwrap();
        let file = other.0.join("x.md");
        std::fs::write(&file, "x\n").unwrap();
        let mut s = other.open();
        assert_eq!(format(&other), BASE_FORMAT);
        s.open_local(&file).unwrap();
        assert_eq!(format(&other), FORMAT);
    }

    #[test]
    fn newer_format_is_refused() {
        // A newer build may have changed what the index means (folders, FORMAT 2):
        // opening it here could misread and then rewrite it, so it is refused.
        let lib = Lib::new("format");
        drop(lib.open());
        std::fs::write(lib.0.join("format.json"), format!("{{\"format\": {}}}", FORMAT + 1)).unwrap();
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
        let trash = lib.0.join("trash");
        let index = std::fs::read(trash.join("index.json")).unwrap();
        std::fs::rename(&trash, lib.0.join("trash-away")).unwrap();
        std::fs::write(&trash, b"").unwrap();
        let mut s = Store {
            #[cfg(feature = "sqlite-store")]
            sqlite: None,
            file_changes: vec![], root: lib.0.clone(), index: HashMap::new(), trash: serde_json::from_slice(&index).unwrap(),
            warnings: vec![], loaded: Default::default(), copied: Default::default(), parsed: HashMap::new()
        };
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
        std::fs::create_dir(lib.0.join("docs").join(format!("{id}.xml.tmp"))).unwrap();
        s.save(&id, &doc("新")).unwrap();
        assert_eq!(s.load(&id).unwrap().title.text, "新");
    }
}
