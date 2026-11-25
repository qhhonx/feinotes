//! 思源笔记 import: a workspace (or its `data` folder), one notebook's folder
//! or a `.sy.zip` export, read straight from the `.sy` files — Lute's
//! syntax tree as JSON (github.com/88250/lute, ast/node.go) — into library
//! documents. A notebook becomes a folder and a document's child documents
//! (the folder named after its id) its 子文档. Two passes: every document
//! is made first, so that references between them can point at library
//! documents; then each is filled in. What has no counterpart here becomes
//! the nearest thing that keeps its content, and is counted in the [`Report`].

mod av;
mod markdown;
pub mod zip;
#[cfg(test)]
mod tests;

pub use av::{AttributeView, Databases, StaticTables};
pub use markdown::{markdown_text, markdown_times};

use crate::model::{Align, Block, BlockKind, ColorName, Cover, Document, FileBlock, FileView, InlineDocLink, InlineKind, Marks, RichText, Table, TableCell};
use crate::store::Store;
use crate::vfs::PathExt;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};

/// Text the import writes into pages, in the interface's language.
pub struct Labels {
    /// The folded heading over a page's 思源 attributes.
    pub attributes: String,
    /// That table's columns.
    pub block: String,
    pub name: String,
    pub value: String,
    /// The page itself, in the 块 column.
    pub document: String,
    /// Before an embed whose query could not be run.
    pub query: String,
}

impl Default for Labels {
    fn default() -> Self {
        Labels { attributes: "思源笔记属性".into(), block: "块".into(), name: "属性".into(), value: "值".into(), document: "文档".into(), query: "思源笔记嵌入块（查询结果没有导入）：".into() }
    }
}

/// A reference that did not become a link to what it pointed at.
#[derive(Clone, Debug, PartialEq)]
pub struct Ref {
    /// The page it is on (title, library id), its text, and the 思源 block id it pointed at.
    pub doc: String,
    pub page: String,
    pub text: String,
    pub target: String,
}

/// What an import did.
#[derive(Debug, Default)]
pub struct Report {
    pub notebooks: usize,
    /// The folders the notebooks became, in order.
    pub folders: Vec<String>,
    pub docs: usize,
    /// `.sy` files that could not be read, or documents not made.
    pub failed: Vec<String>,
    /// 思源 types and marks with no counterpart here, by kind (`kbd`,
    /// `NodeTabs`, …), and how many: each became the nearest form.
    pub degraded: BTreeMap<String, usize>,
    /// References to a block that is not a document or heading: a link to
    /// the document holding it (plain text within the same document).
    pub block_refs: Vec<Ref>,
    /// References whose target is not in what was imported: their text stays.
    pub broken_refs: Vec<Ref>,
    /// Embeds copied in, and those whose query is kept as code instead.
    pub embeds: usize,
    pub queries: usize,
    /// Databases put on their pages as static tables, by name.
    pub databases: Vec<String>,
    /// Images and files that were not found: kept as their 思源 paths.
    pub missing_assets: Vec<String>,
    /// Attributes (自定义属性, 命名, 别名, 备注, 书签, 标签) listed on their pages.
    pub attributes: usize,
}

/// A 思源 workspace, notebook or export, ready to read.
pub struct Source {
    pub notebooks: Vec<Notebook>,
    assets: Vec<PathBuf>,
    avs: Vec<PathBuf>,
    /// The folder a `.sy.zip` was unpacked into, removed when done.
    unpacked: Option<PathBuf>,
}

pub struct Notebook {
    pub name: String,
    pub dir: PathBuf,
    /// The file tree's order (`.siyuan/sort.json`): id → place.
    sort: HashMap<String, i64>,
}

impl Drop for Source {
    fn drop(&mut self) {
        if let Some(d) = &self.unpacked { let _ = crate::vfs::remove_dir_all(d); }
    }
}

const ZIP_LIMIT: u64 = 2 << 30;

fn is_notebook(dir: &Path) -> bool { dir.join(".siyuan").join("conf.json").fs_is_file() }

fn is_id(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 22 && b[14] == b'-' && b[..14].iter().all(u8::is_ascii_digit) && b[15..].iter().all(|c| c.is_ascii_digit() || c.is_ascii_lowercase())
}

fn sy_files(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = crate::vfs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "sy") && p.fs_is_file()).collect();
    out.sort();
    out
}

/// Whether `path` is something [`open`] reads: a `.sy.zip`, a notebook's
/// folder, a workspace or its `data` folder.
pub fn recognizes(path: &Path) -> bool {
    if path.fs_is_file() { return path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.to_lowercase().ends_with(".sy.zip")) }
    let data = path.join("data");
    let dir = if data.fs_is_dir() { data.as_path() } else { path };
    is_notebook(dir) || crate::vfs::read_dir(dir).into_iter().flatten().flatten().any(|e| is_notebook(&e.path()))
}

/// How many documents the notebooks in `path` hold (欢迎页's count); stops at `limit`.
pub fn count_docs(path: &Path, limit: usize) -> usize {
    fn walk(dir: &Path, n: &mut usize, limit: usize) {
        for e in crate::vfs::read_dir(dir).into_iter().flatten().flatten() {
            if *n >= limit { return }
            let p = e.path();
            if p.extension().is_some_and(|e| e == "sy") { *n += 1; } else if p.file_name().and_then(|n| n.to_str()).is_some_and(is_id) && p.fs_is_dir() { walk(&p, n, limit); }
        }
    }
    let Ok(src) = open(path) else { return 0 };
    let mut n = 0;
    for nb in &src.notebooks { walk(&nb.dir, &mut n, limit); }
    n
}

/// The notebooks in `path` (see [`recognizes`]); a `.sy.zip` is unpacked
/// into a temporary folder for as long as the `Source` lives.
pub fn open(path: &Path) -> anyhow::Result<Source> {
    let mut src = Source { notebooks: vec![], assets: vec![], avs: vec![], unpacked: None };
    if path.fs_is_file() {
        let bytes = crate::store::read_bounded(path, ZIP_LIMIT)?;
        let dir = crate::vfs::temp_dir().join(format!("quiet-md-siyuan-{}", uuid::Uuid::new_v4()));
        src.unpacked = Some(dir.clone());
        zip::unpack(&bytes, &dir)?;
        // The export's one folder (`<notebook or document>/`), else the zip's top.
        let dirs: Vec<PathBuf> = crate::vfs::read_dir(&dir)?.flatten().map(|e| e.path()).filter(|p| p.fs_is_dir()).collect();
        let base = if dirs.len() == 1 && sy_files(&dir).is_empty() { dirs[0].clone() } else { dir.clone() };
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let name = name.strip_suffix(".sy.zip").or_else(|| name.strip_suffix(".zip")).unwrap_or(name);
        src.add(&base, name);
        return Ok(src);
    }
    let data = path.join("data");
    let dir = if data.fs_is_dir() && !is_notebook(path) { data } else { path.to_path_buf() };
    if is_notebook(&dir) {
        src.add(&dir, "");
        if let Some(up) = dir.parent() { src.assets.push(up.join("assets")); src.avs.push(up.join("storage").join("av")); }
    } else {
        let mut nbs: Vec<PathBuf> = crate::vfs::read_dir(&dir)?.flatten().map(|e| e.path()).filter(|p| is_notebook(p)).collect();
        nbs.sort();
        for nb in &nbs { src.add(nb, ""); }
        if nbs.is_empty() && !sy_files(&dir).is_empty() { src.add(&dir, ""); }
        src.assets.push(dir.join("assets"));
        src.avs.push(dir.join("storage").join("av"));
        // Notebooks in their 文档树 order.
        let place = |nb: &Notebook| crate::vfs::read(nb.dir.join(".siyuan").join("conf.json")).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|v| v["sort"].as_i64()).unwrap_or(0);
        src.notebooks.sort_by_cached_key(|nb| (place(nb), nb.name.clone()));
    }
    anyhow::ensure!(!src.notebooks.is_empty(), "没有找到思源笔记的笔记本");
    Ok(src)
}

impl Source {
    /// Notebook `dir`, named by its `conf.json`, else `fallback`, else the folder.
    fn add(&mut self, dir: &Path, fallback: &str) {
        let conf = |f: &str| crate::vfs::read(dir.join(".siyuan").join(f)).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok());
        let name = conf("conf.json").and_then(|v| v["name"].as_str().map(str::to_string)).filter(|n| !n.trim().is_empty())
            .or_else(|| (!fallback.is_empty()).then(|| fallback.to_string()))
            .unwrap_or_else(|| dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
        let sort = conf("sort.json").and_then(|v| v.as_object().map(|o| o.iter().filter_map(|(k, v)| Some((k.clone(), v.as_i64()?))).collect())).unwrap_or_default();
        self.notebooks.push(Notebook { name, dir: dir.to_path_buf(), sort });
        // 思源's own 用户指南 keeps its assets and databases inside the notebook.
        self.assets.push(dir.join("assets"));
        self.avs.push(dir.join("storage").join("av"));
    }

    /// Every document, a parent before its children, each level in 文档树 order.
    fn load(&self, report: &mut Report) -> Vec<Doc> {
        fn walk(dir: &Path, nb: usize, sort: &HashMap<String, i64>, parent: Option<usize>, out: &mut Vec<Doc>, report: &mut Report) {
            let mut files = sy_files(dir);
            let id = |p: &Path| p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            files.sort_by_cached_key(|p| (sort.get(&id(p)).copied().unwrap_or(i64::MAX), id(p)));
            for f in &files {
                let tree = crate::vfs::read(f).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).filter(|v| ty(v) == "NodeDocument");
                let at = match tree {
                    Some(tree) => {
                        let sy = Some(id(f)).filter(|i| is_id(i)).unwrap_or_else(|| st(&tree, "ID").to_string());
                        let title = Some(prop(&tree, "title").trim().to_string()).filter(|t| !t.is_empty()).unwrap_or_else(|| sy.clone());
                        out.push(Doc { sy, file: f.clone(), tree, title, parent, notebook: nb });
                        Some(out.len() - 1)
                    }
                    None => { report.failed.push(f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()); parent }
                };
                let kids = dir.join(id(f));
                if kids.fs_is_dir() { walk(&kids, nb, sort, at, out, report); }
            }
            // A folder of child documents whose parent's file is gone: they go where it would have been.
            let mut orphans: Vec<PathBuf> = crate::vfs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path())
                .filter(|p| p.fs_is_dir() && p.file_name().and_then(|n| n.to_str()).is_some_and(is_id) && !p.with_extension("sy").fs_is_file()).collect();
            orphans.sort();
            for o in orphans { walk(&o, nb, sort, parent, out, report); }
        }
        let mut out = vec![];
        for (n, nb) in self.notebooks.iter().enumerate() { walk(&nb.dir, n, &nb.sort, None, &mut out, report); }
        out
    }
}

struct Doc {
    sy: String,
    file: PathBuf,
    tree: Value,
    title: String,
    parent: Option<usize>,
    notebook: usize,
}

/// What a 思源 block id names, and on which page.
#[derive(Clone, Debug)]
enum Target { Doc, Heading(String), Block }

fn index(docs: &[Doc]) -> HashMap<String, (usize, Target)> {
    fn walk(n: &Value, d: usize, out: &mut HashMap<String, (usize, Target)>) {
        let id = st(n, "ID");
        if !id.is_empty() {
            let t = match ty(n) { "NodeDocument" => Target::Doc, "NodeHeading" => Target::Heading(plain(n).trim().to_string()), _ => Target::Block };
            out.entry(id.to_string()).or_insert((d, t));
        }
        for k in kids(n) { walk(k, d, out); }
    }
    let mut out = HashMap::new();
    for (d, doc) in docs.iter().enumerate() {
        out.insert(doc.sy.clone(), (d, Target::Doc));
        walk(&doc.tree, d, &mut out);
    }
    out
}

/// Imports `src` into `store`, each notebook a folder under `parent`;
/// databases go where `dbs` puts them. Call it inside a batch.
pub fn import(store: &mut Store, src: &Source, parent: Option<&str>, labels: &Labels, dbs: &mut dyn Databases) -> anyhow::Result<Report> {
    let mut report = Report::default();
    let docs = src.load(&mut report);
    let index = index(&docs);
    // First every page, empty, so references have somewhere to point.
    let mut lib = vec![String::new(); docs.len()];
    for (n, nb) in src.notebooks.iter().enumerate() {
        let folder = store.create_folder(&nb.name, parent)?;
        if parent.is_some() { let _ = store.set_parent(&folder, parent); }
        report.notebooks += 1;
        report.folders.push(folder.clone());
        for (i, d) in docs.iter().enumerate().filter(|(_, d)| d.notebook == n) {
            let stub = Document { title: RichText::plain(&d.title), ..Default::default() };
            let made = match d.parent.map(|p| lib[p].as_str()).filter(|p| !p.is_empty() && store.can_have_children(p)) {
                Some(p) => store.create_child(p, &stub),
                None => store.create_from_in(&stub, Some(&folder)),
            };
            match made { Ok(id) => lib[i] = id, Err(e) => report.failed.push(format!("{}：{e}", d.title)) }
        }
    }
    let files: Vec<Option<PathBuf>> = lib.iter().map(|id| store.file_path(id)).collect();
    let mut assets = Assets::new(Store::assets_in(store.top_dir()));
    for i in 0..docs.len() {
        if lib[i].is_empty() { continue }
        let doc = Conv { src, docs: &docs, index: &index, lib: &lib, files: &files, at: i, labels, dbs: &mut *dbs, report: &mut report, assets: &mut assets, attrs: vec![], depth: 0, in_column: false }.document();
        if let Err(e) = store.save(&lib[i], &doc) { report.failed.push(format!("{}：{e}", docs[i].title)); continue }
        let made = time(&docs[i].sy).unwrap_or_else(crate::time::now);
        let changed = time(prop(&docs[i].tree, "updated")).unwrap_or(made).max(made);
        if let Some(p) = store.file_path(&lib[i]) { let _ = crate::vfs::set_modified(&p, changed); }
        let _ = store.update_meta(&lib[i], |m| { m.created = made; m.modified = changed; });
        report.docs += 1;
    }
    Ok(report)
}

/// Unix seconds of a 思源 time (`20240102150405`, local), or of an id, which starts with one.
pub fn time(s: &str) -> Option<u64> {
    let d = s.get(..14).filter(|d| d.bytes().all(|b| b.is_ascii_digit()))?;
    let n = |r: std::ops::Range<usize>| d[r].parse::<u32>().ok();
    let (y, m, day, h, mi, sec) = (n(0..4)? as i64, n(4..6)?, n(6..8)?, n(8..10)?, n(10..12)?, n(12..14)?);
    if !(1..=12).contains(&m) || day == 0 || day > crate::time::days_in_month(y, m) || h > 23 || mi > 59 || sec > 60 { return None }
    u64::try_from(crate::time::local_to_unix(y, m, day, h, mi) + sec as i64).ok()
}

// ---- reading nodes

fn ty(n: &Value) -> &str { n["Type"].as_str().unwrap_or_default() }
fn st<'v>(n: &'v Value, k: &str) -> &'v str { n[k].as_str().unwrap_or_default() }
fn prop<'v>(n: &'v Value, k: &str) -> &'v str { n["Properties"][k].as_str().unwrap_or_default() }
fn kids(n: &Value) -> &[Value] { n["Children"].as_array().map(Vec::as_slice).unwrap_or_default() }
fn child<'v>(n: &'v Value, t: &str) -> Option<&'v Value> { kids(n).iter().find(|k| ty(k) == t) }
fn folded(n: &Value) -> bool { prop(n, "fold") == "1" }

/// Syntax that carries nothing of its own: markers and attribute lists.
fn is_syntax(t: &str) -> bool {
    t.contains("Marker") || t.contains("IAL") || matches!(t, "NodeOpenBrace" | "NodeCloseBrace" | "NodeOpenBracket" | "NodeCloseBracket" | "NodeOpenParen" | "NodeCloseParen" | "NodeBang" | "NodeLess" | "NodeGreater")
}

/// The text a node shows, without styles.
fn plain(n: &Value) -> String {
    let mut s = String::new();
    fn walk(n: &Value, s: &mut String) {
        match ty(n) {
            "NodeTextMark" => {
                let types = st(n, "TextMarkType");
                if types.split(' ').any(|t| t == "inline-math") { s.push_str(st(n, "TextMarkInlineMathContent")) }
                else { if types.split(' ').any(|t| t == "tag") { s.push('#') } s.push_str(st(n, "TextMarkTextContent")) }
            }
            "NodeBr" => s.push('\n'),
            t if is_syntax(t) => {}
            _ => { if kids(n).is_empty() { s.push_str(st(n, "Data")) } else { for k in kids(n) { walk(k, s) } } }
        }
    }
    walk(n, &mut s);
    s.replace('\u{200b}', "")
}

fn base64(s: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s.trim()).ok().map(|b| String::from_utf8_lossy(&b).to_string()).unwrap_or_default()
}

/// A CSS declaration's value in a `style` attribute.
fn css<'s>(style: &'s str, key: &str) -> Option<&'s str> {
    style.split(';').find_map(|d| { let (k, v) = d.split_once(':')?; (k.trim() == key).then(|| v.trim()) }).filter(|v| !v.is_empty())
}

/// 思源's colours (the daylight theme's `--b3-font-color1…13`,
/// `--b3-font-background1…13`, `--b3-card-*`) and plain CSS ones as the
/// palette's names; a background becomes a light one.
fn color(v: &str, bg: bool) -> Option<ColorName> {
    const FONT: [Option<&str>; 13] = [Some("red"), Some("orange"), Some("blue"), Some("green"), Some("gray"), Some("blue"), Some("orange"), Some("red"), Some("purple"), Some("blue"), Some("green"), Some("yellow"), None];
    const BACK: [&str; 13] = ["light-red", "light-orange", "light-blue", "light-green", "light-gray", "light-blue", "light-orange", "light-red", "light-purple", "light-blue", "light-green", "light-yellow", "gray"];
    let v = v.trim();
    if let Some(var) = v.strip_prefix("var(--").and_then(|r| r.split(')').next()) {
        if let Some(n) = var.strip_prefix("b3-font-color").and_then(|n| n.parse::<usize>().ok()) { return FONT.get(n.wrapping_sub(1)).copied().flatten().map(|c| ColorName(c.into())) }
        if let Some(n) = var.strip_prefix("b3-font-background").and_then(|n| n.parse::<usize>().ok()) { return BACK.get(n.wrapping_sub(1)).map(|c| ColorName(c.to_string())) }
        let hue = ["error", "warning", "info", "success"].iter().position(|k| var.contains(k)).map(|i| ["red", "orange", "blue", "green"][i])?;
        return Some(ColorName(if var.ends_with("background") { format!("light-{hue}") } else { hue.to_string() }));
    }
    let (r, g, b) = if let Some(h) = v.strip_prefix('#') {
        let h: String = if h.len() == 3 { h.chars().flat_map(|c| [c, c]).collect() } else { h.chars().take(6).collect() };
        let x = u32::from_str_radix(&h, 16).ok()?;
        ((x >> 16) as f32, (x >> 8 & 0xff) as f32, (x & 0xff) as f32)
    } else {
        let inner = v.strip_prefix("rgba(").or_else(|| v.strip_prefix("rgb("))?.trim_end_matches(')');
        let c: Vec<f32> = inner.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        (*c.first()?, *c.get(1)?, *c.get(2)?)
    };
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let hue = if max - min < 24. { "gray" } else {
        let d = max - min;
        let h = if max == r { 60. * ((g - b) / d).rem_euclid(6.) } else if max == g { 60. * ((b - r) / d + 2.) } else { 60. * ((r - g) / d + 4.) };
        match h as u32 { 0..15 | 345.. => "red", 15..45 => "orange", 45..70 => "yellow", 70..170 => "green", 170..260 => "blue", _ => "purple" }
    };
    Some(ColorName(if bg { format!("light-{hue}") } else { hue.into() }))
}

/// A document icon: an emoji written as code points (`1f4d4`, `1f468-200d-1f4bb`)
/// or as itself; a custom icon file has no counterpart.
fn emoji(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() || s.contains(['/', '.']) { return None }
    if s.split('-').all(|p| !p.is_empty() && p.len() <= 6 && p.chars().all(|c| c.is_ascii_hexdigit())) {
        return s.split('-').map(|p| u32::from_str_radix(p, 16).ok().and_then(char::from_u32)).collect();
    }
    Some(s.to_string())
}

/// `from`'s folder to `to`, as a relative link.
fn relative(from: &Path, to: &Path) -> String {
    let a: Vec<Component> = from.parent().map(|p| p.components().collect()).unwrap_or_default();
    let b: Vec<Component> = to.components().collect();
    let same = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let up = std::iter::repeat_n("..".to_string(), a.len() - same);
    up.chain(b[same..].iter().map(|c| encode(&c.as_os_str().to_string_lossy()))).collect::<Vec<_>>().join("/")
}

/// Characters a link's path or fragment cannot hold as they are.
fn encode(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c { ' ' | '%' | '#' | '?' | '"' | '<' | '>' | '\\' => out.push_str(&format!("%{:02X}", c as u32)), c if c.is_control() => {}, c => out.push(c) }
    }
    out
}

// ---- assets

/// 思源's assets copied into the library's, once each.
struct Assets {
    root: PathBuf,
    done: HashMap<PathBuf, PathBuf>,
}

const IMAGES: [&str; 8] = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "avif"];

impl Assets {
    fn new(root: PathBuf) -> Self { Assets { root, done: HashMap::new() } }

    /// `from`'s copy: an image as `<8 hex>-<name>.<ext>` (as a picked image
    /// is kept), any other file as `files/<uuid>/<name>` (as an attached one).
    fn copy(&mut self, from: &Path) -> Option<PathBuf> {
        if let Some(p) = self.done.get(from) { return Some(p.clone()) }
        let name = asset_name(from);
        let ext = from.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let id = uuid::Uuid::new_v4().to_string();
        let to = if IMAGES.contains(&ext.as_str()) { self.root.join(format!("{}-{name}", &id[..8])) } else { self.root.join("files").join(&id).join(&name) };
        crate::vfs::create_dir_all(to.parent()?).ok()?;
        crate::vfs::copy(from, &to).ok()?;
        if let Some((_, changed)) = crate::vfs::file_times(from) { let _ = crate::vfs::set_modified(&to, changed); }
        self.done.insert(from.to_path_buf(), to.clone());
        Some(to)
    }
}

/// An asset's file name without the `-<id>` 思源 adds (`图-20240101120000-abcdefg.png` → `图.png`).
fn asset_name(p: &Path) -> String {
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let stem = stem.rsplit_once('-').and_then(|(a, b)| a.rsplit_once('-').filter(|(_, d)| is_id(&format!("{d}-{b}"))).map(|(s, _)| s.to_string())).filter(|s| !s.is_empty()).unwrap_or(stem);
    match p.extension() { Some(e) => format!("{stem}.{}", e.to_string_lossy()), None => stem }
}

// ---- converting

/// One piece of a line: text, or an image, which only stands as a block here.
enum Piece { Text(RichText), Image(Block) }

fn push(out: &mut Vec<Piece>, s: &str, m: &Marks) {
    if s.is_empty() { return }
    if !matches!(out.last(), Some(Piece::Text(_))) { out.push(Piece::Text(RichText::default())); }
    if let Some(Piece::Text(t)) = out.last_mut() { t.push(s, m.clone()); }
}

struct Conv<'a> {
    src: &'a Source,
    docs: &'a [Doc],
    index: &'a HashMap<String, (usize, Target)>,
    lib: &'a [String],
    files: &'a [Option<PathBuf>],
    /// The document being converted.
    at: usize,
    labels: &'a Labels,
    dbs: &'a mut dyn Databases,
    report: &'a mut Report,
    assets: &'a mut Assets,
    /// (block, attribute, value) for the page's attribute table.
    attrs: Vec<[String; 3]>,
    /// Embeds within embeds.
    depth: usize,
    /// Inside a column: columns within become blocks one after another.
    in_column: bool,
}

impl Conv<'_> {
    fn degrade(&mut self, what: &str) { *self.report.degraded.entry(what.to_string()).or_default() += 1; }

    fn document(mut self) -> Document {
        let docs = self.docs;
        let tree = &docs[self.at].tree;
        let mut blocks = vec![];
        self.blocks(kids(tree), &mut blocks);
        let doc_label = self.labels.document.clone();
        self.attributes(tree, &doc_label);
        let cover = Some(prop(tree, "title-img")).filter(|s| s.contains("url(")).and_then(|s| {
            let url = s.split("url(").nth(1)?.split(')').next()?.trim().trim_matches(['"', '\'']);
            let from = self.asset(url)?;
            Some(Cover { src: self.assets.copy(&from)?.to_string_lossy().to_string(), x: 0., y: 0. })
        });
        if !self.attrs.is_empty() {
            let mut h = Block::new(BlockKind::Heading { level: 2, seq_auto: false }, RichText::plain(&self.labels.attributes));
            h.folded = true;
            blocks.push(h);
            let cell = |s: &str| TableCell { blocks: s.split('\n').map(Block::text).collect(), ..TableCell::empty() };
            let mut rows = vec![[&self.labels.block, &self.labels.name, &self.labels.value].map(|s| cell(s)).to_vec()];
            rows.extend(self.attrs.iter().map(|r| r.iter().map(|s| cell(s)).collect()));
            blocks.push(Block::new(BlockKind::Table(Table { col_widths: vec![180., 140., 260.], header_rows: 1, header_col: false, rows, axes: Default::default() }), RichText::default()));
        }
        if blocks.is_empty() { blocks.push(Block::text("")); }
        Document { title: RichText::plain(&self.docs[self.at].title), icon: emoji(prop(tree, "icon")), cover, blocks }
    }

    /// `n`'s own attributes (命名, 别名, 备注, 书签, 标签, 自定义属性), for the
    /// page's table, which a page without any does not get; `label` says which block.
    fn attributes(&mut self, n: &Value, label: &str) {
        // A copy an embed made: its attributes are listed on its own page.
        if self.depth > 0 { return }
        let Some(props) = n["Properties"].as_object() else { return };
        // 思源's own `custom-` keys (a 日记's date, a block's databases, its internals) are not the writer's.
        let own = |k: &str| ["custom-sy-", "custom-riff-", "custom-dailynote-"].iter().any(|p| k.starts_with(p)) || k == "custom-avs";
        let mut keys: Vec<&String> = props.keys().filter(|k| matches!(k.as_str(), "name" | "alias" | "memo" | "bookmark" | "tags") || (k.starts_with("custom-") && !own(k))).collect();
        keys.sort_by_key(|k| (k.starts_with("custom-"), k.as_str()));
        for k in keys {
            let Some(v) = props[k].as_str().filter(|v| !v.is_empty()) else { continue };
            self.attrs.push([label.to_string(), k.strip_prefix("custom-").unwrap_or(k).to_string(), v.to_string()]);
            self.report.attributes += 1;
        }
    }

    fn blocks(&mut self, nodes: &[Value], out: &mut Vec<Block>) {
        for n in nodes { self.block(n, out); }
    }

    fn block(&mut self, n: &Value, out: &mut Vec<Block>) {
        let start = out.len();
        match ty(n) {
            "NodeParagraph" => self.paragraph(n, out),
            "NodeHeading" => {
                let (text, pics) = self.line(kids(n));
                let mut b = Block::new(BlockKind::Heading { level: n["HeadingLevel"].as_u64().unwrap_or(1).clamp(1, 6) as u8, seq_auto: false }, text);
                b.folded = folded(n);
                out.push(b);
                out.extend(pics);
            }
            t @ ("NodeList" | "NodeMindmap") => {
                if t == "NodeMindmap" { self.degrade("NodeMindmap") }
                let mut first = true;
                for item in kids(n).iter().filter(|k| matches!(ty(k), "NodeListItem" | "NodeMindmapItem")) {
                    self.item(item, n, first, out);
                    first = false;
                }
            }
            "NodeBlockquote" => self.container(BlockKind::Quote, kids(n), out),
            "NodeCallout" => self.callout(n, out),
            "NodeSuperBlock" => self.superblock(n, out),
            "NodeCodeBlock" => self.code(n, out),
            "NodeMathBlock" => {
                let src = child(n, "NodeMathBlockContent").map(|c| st(c, "Data")).unwrap_or_default().trim().to_string();
                out.push(equation(&src));
            }
            "NodeTable" => self.table(n, out),
            "NodeThematicBreak" => out.push(Block::new(BlockKind::Divider, RichText::default())),
            "NodeHTMLBlock" => { self.degrade("NodeHTMLBlock"); out.push(code_block("html", st(n, "Data"))) }
            t @ ("NodeIFrame" | "NodeWidget") => {
                self.degrade(t);
                let src = html_src(st(n, "Data")).unwrap_or_default();
                let src = if src.starts_with("//") { format!("https:{src}") } else { src };
                let mut text = RichText::default();
                if !src.is_empty() { text.push(&src, Marks { link: Some(src.clone()), ..Default::default() }); }
                out.push(Block::new(BlockKind::Text, text));
            }
            "NodeVideo" | "NodeAudio" => self.media(n, out),
            "NodeBlockQueryEmbed" => self.embed(n, out),
            "NodeAttributeView" => self.database(n, out),
            "NodeTabs" => {
                self.degrade("NodeTabs");
                for tab in kids(n).iter().filter(|k| ty(k) == "NodeTabItem") {
                    let mut title = RichText::default();
                    title.push(st(tab, "TabItemTitle"), Marks { bold: true, ..Default::default() });
                    let mut body = vec![];
                    self.blocks(kids(tab), &mut body);
                    out.push(Block::new(BlockKind::Text, title).with_children(body));
                }
            }
            "NodeYamlFrontMatter" => {
                let yaml = child(n, "NodeYamlFrontMatterContent").map(|c| st(c, "Data")).unwrap_or_default();
                out.push(code_block("yaml", yaml));
            }
            "NodeFootnotesDefBlock" => {
                self.degrade("NodeFootnotesDef");
                for def in kids(n).iter().filter(|k| ty(k) == "NodeFootnotesDef") {
                    let mut body = vec![];
                    self.blocks(kids(def), &mut body);
                    out.push(Block::text(&format!("[^{}]", st(def, "Data").trim_start_matches('^'))).with_children(body));
                }
            }
            t if is_syntax(t) => {}
            t => {
                // Anything newer than this import: its blocks, else its text.
                self.degrade(t);
                if kids(n).iter().any(|k| !st(k, "ID").is_empty()) { self.blocks(kids(n), out) }
                else { let s = plain(n); if !s.trim().is_empty() { out.push(Block::text(s.trim_end())) } }
            }
        }
        if let Some(b) = out.get(start) {
            let label = excerpt(b);
            self.attributes(n, &label);
        }
        let style = prop(n, "style");
        if !style.is_empty() && !matches!(ty(n), "NodeSuperBlock" | "NodeList" | "NodeMindmap") {
            for b in &mut out[start..] { styled(b, style); }
        }
    }

    fn paragraph(&mut self, n: &Value, out: &mut Vec<Block>) {
        let mut pieces = vec![];
        self.inline(kids(n), &Marks::default(), &mut pieces);
        let only_images = pieces.iter().all(|p| match p { Piece::Text(t) => t.text.trim().is_empty(), _ => true }) && pieces.iter().any(|p| matches!(p, Piece::Image(_)));
        let before = out.len();
        for p in pieces {
            match p {
                Piece::Image(b) => out.push(b),
                Piece::Text(_) if only_images => {}
                Piece::Text(t) => out.push(Block::new(BlockKind::Text, trim_breaks(t))),
            }
        }
        if out.len() == before { out.push(Block::text("")); }
    }

    /// A line of text (a heading's, a list item's, a cell's), and the
    /// images it held, which follow it.
    fn line(&mut self, nodes: &[Value]) -> (RichText, Vec<Block>) {
        let mut pieces = vec![];
        self.inline(nodes, &Marks::default(), &mut pieces);
        let (mut text, mut pics) = (RichText::default(), vec![]);
        for p in pieces { match p { Piece::Text(t) => text.append(&t), Piece::Image(b) => pics.push(b) } }
        (trim_breaks(text), pics)
    }

    fn item(&mut self, item: &Value, list: &Value, first: bool, out: &mut Vec<Block>) {
        let typ = item["ListData"]["Typ"].as_i64().or(list["ListData"]["Typ"].as_i64()).unwrap_or(0);
        let done = kids(item).iter().any(|k| ty(k) == "NodeTaskListItemMarker" && k["TaskListItemChecked"].as_bool() == Some(true));
        let start = list["ListData"]["Start"].as_u64().unwrap_or(1) as u32;
        let kind = match typ { 1 => BlockKind::Ordered { seq: (first && start != 1).then_some(start) }, 3 => BlockKind::Todo { done }, _ => BlockKind::Bullet { dash: false } };
        let body: Vec<&Value> = kids(item).iter().filter(|k| !is_syntax(ty(k))).collect();
        let (text, mut children) = match body.first() {
            Some(p) if ty(p) == "NodeParagraph" => { let (t, pics) = self.line(kids(p)); let label = t.text.clone(); self.attributes(p, &label); (t, pics) }
            _ => (RichText::default(), vec![]),
        };
        let skip = usize::from(body.first().is_some_and(|p| ty(p) == "NodeParagraph"));
        for k in &body[skip..] { self.block(k, &mut children); }
        let mut b = Block::new(kind, text).with_children(children);
        b.folded = folded(item);
        let style = prop(item, "style");
        if !style.is_empty() { styled(&mut b, style); }
        let label = excerpt(&b);
        self.attributes(item, &label);
        out.push(b);
    }

    fn container(&mut self, kind: BlockKind, nodes: &[Value], out: &mut Vec<Block>) {
        let mut body = vec![];
        self.blocks(nodes, &mut body);
        if body.is_empty() { body.push(Block::text("")); }
        out.push(Block::new(kind, RichText::default()).with_children(body));
    }

    fn callout(&mut self, n: &Value, out: &mut Vec<Block>) {
        let (emoji_for, hue) = match st(n, "CalloutType").to_ascii_lowercase().as_str() {
            "note" => ("ℹ️", "blue"),
            "tip" => ("💡", "green"),
            "important" => ("❗", "purple"),
            "warning" => ("⚠️", "yellow"),
            "caution" => ("🚫", "red"),
            _ => ("💡", "orange"),
        };
        let icon = emoji(st(n, "CalloutIcon")).unwrap_or_else(|| emoji_for.to_string());
        let mut body = vec![];
        self.blocks(kids(n), &mut body);
        let title = st(n, "CalloutTitle").trim();
        if !title.is_empty() {
            let mut t = RichText::default();
            t.push(title, Marks { bold: true, ..Default::default() });
            body.insert(0, Block::new(BlockKind::Text, t));
        }
        if body.is_empty() { body.push(Block::text("")); }
        let kind = BlockKind::Callout { emoji: icon, bg: Some(ColorName(format!("light-{hue}"))), border: Some(ColorName(hue.into())), text_color: None };
        out.push(Block::new(kind, RichText::default()).with_children(body));
    }

    /// 超级块: side by side (`col`) is a column group; stacked (`row`) is
    /// its blocks one after another, as is `col` within a column.
    fn superblock(&mut self, n: &Value, out: &mut Vec<Block>) {
        let col = child(n, "NodeSuperBlockLayoutMarker").is_some_and(|m| st(m, "Data") == "col");
        let inner: Vec<&Value> = kids(n).iter().filter(|k| !is_syntax(ty(k))).collect();
        if !col || inner.len() < 2 || self.in_column {
            if col && inner.len() > 1 { self.degrade("NodeSuperBlock col") }
            for k in inner { self.block(k, out); }
            return;
        }
        let ratio = 1. / inner.len() as f32;
        let mut columns = vec![];
        for k in inner {
            self.in_column = true;
            let mut body = vec![];
            self.block(k, &mut body);
            self.in_column = false;
            if body.is_empty() { body.push(Block::text("")); }
            columns.push(Block::new(BlockKind::Column { ratio }, RichText::default()).with_children(body));
        }
        out.push(Block::new(BlockKind::Grid, RichText::default()).with_children(columns));
    }

    fn code(&mut self, n: &Value, out: &mut Vec<Block>) {
        let info = Some(base64(st(n, "CodeBlockInfo"))).filter(|s| !s.trim().is_empty())
            .or_else(|| child(n, "NodeCodeBlockFenceInfoMarker").map(|m| base64(st(m, "CodeBlockInfo")))).unwrap_or_default();
        let lang = info.split_whitespace().next().unwrap_or_default().to_ascii_lowercase();
        // Charts 思源 draws from code; here they stay code.
        if matches!(lang.as_str(), "echarts" | "abc" | "graphviz" | "flowchart" | "plantuml" | "mindmap" | "smiles") { self.degrade(&format!("NodeCodeBlock {lang}")) }
        let code = child(n, "NodeCodeBlockCode").map(|c| st(c, "Data")).unwrap_or_default();
        out.push(code_block(&lang, code));
    }

    fn table(&mut self, n: &Value, out: &mut Vec<Block>) {
        let aligns: Vec<i64> = n["TableAligns"].as_array().map(|a| a.iter().map(|x| x.as_i64().unwrap_or(0)).collect()).unwrap_or_default();
        let mut rows_in: Vec<&Value> = vec![];
        let mut header_rows = 0;
        for k in kids(n) {
            match ty(k) {
                "NodeTableHead" => for r in kids(k).iter().filter(|r| ty(r) == "NodeTableRow") { rows_in.push(r); header_rows += 1; },
                "NodeTableRow" => rows_in.push(k),
                _ => {}
            }
        }
        // Cells as a grid, a merged cell covering the slots it spans.
        let mut grid: Vec<Vec<Option<TableCell>>> = vec![];
        for (r, row) in rows_in.iter().enumerate() {
            if grid.len() <= r { grid.resize_with(r + 1, Vec::new); }
            let mut c = 0;
            for cell in kids(row).iter().filter(|k| ty(k) == "NodeTableCell") {
                while grid[r].get(c).is_some_and(Option::is_some) { c += 1; }
                let span = |k: &str| prop(cell, k).parse::<u32>().unwrap_or(1).max(1);
                let (cs, rs) = (span("colspan"), span("rowspan"));
                let mut blocks = self.cell(cell);
                let align = match aligns.get(c) { Some(2) => Align::Center, Some(3) => Align::Right, _ => Align::Left };
                for b in &mut blocks { if matches!(b.kind, BlockKind::Text) && b.align == Align::Left { b.align = align; } }
                let style = prop(cell, "style");
                let bg = css(style, "background-color").and_then(|v| color(v, true));
                if let Some(c) = css(style, "color").and_then(|v| color(v, false)) { for b in &mut blocks { b.text.update_marks(0..b.text.len(), |m| if m.text_color.is_none() { m.text_color = Some(c.clone()) }); } }
                for dr in 0..rs as usize {
                    if grid.len() <= r + dr { grid.resize_with(r + dr + 1, Vec::new); }
                    for dc in 0..cs as usize {
                        let slot = &mut grid[r + dr];
                        if slot.len() <= c + dc { slot.resize_with(c + dc + 1, || None); }
                        slot[c + dc] = Some(TableCell::covered());
                    }
                }
                grid[r][c] = Some(TableCell { blocks, bg, valign: None, colspan: cs, rowspan: rs });
                c += cs as usize;
            }
        }
        let ncols = grid.iter().map(Vec::len).max().unwrap_or(0).max(1);
        let rows: Vec<Vec<TableCell>> = grid.into_iter().map(|mut r| { r.resize_with(ncols, || None); r.into_iter().map(|c| c.unwrap_or_else(TableCell::empty)).collect() }).collect();
        if rows.is_empty() { return }
        let widths: Vec<f32> = prop(n, "colgroup").split('|').map(|c| css(c, "width").and_then(|w| w.trim_end_matches("px").trim().parse().ok()).unwrap_or(0.)).collect();
        let auto = crate::paste::pipe_table_widths(&rows, ncols);
        let col_widths = (0..ncols).map(|i| widths.get(i).copied().filter(|w| *w > 0.).unwrap_or(auto[i]).max(Table::MIN_WIDTH)).collect();
        let mut t = Table { col_widths, header_rows: header_rows.min(rows.len()), header_col: false, rows, axes: Default::default() };
        t.repair();
        out.push(Block::new(BlockKind::Table(t), RichText::default()));
    }

    /// A table cell's blocks: its rich source (思源's 富文本单元格, kramdown)
    /// when it has one, else its line.
    fn cell(&mut self, cell: &Value) -> Vec<Block> {
        let rich = &cell["TableCellRich"];
        if rich["format"].as_str() == Some("kramdown") && let Some(src) = rich["content"].as_str().filter(|s| !s.trim().is_empty()) {
            let text = markdown_text(src).unwrap_or_else(|| src.to_string());
            let mut blocks = crate::paste::markdown_to_blocks(&text, crate::paste::Off { file: true, ..Default::default() });
            self.local_images(&mut blocks);
            if !blocks.is_empty() { return blocks }
        }
        let (text, pics) = self.line(kids(cell));
        let mut blocks = if text.is_empty() && !pics.is_empty() { vec![] } else { vec![Block::new(BlockKind::Text, text)] };
        blocks.extend(pics);
        blocks
    }

    /// Images a Markdown source names by their 思源 path, copied.
    fn local_images(&mut self, blocks: &mut [Block]) {
        for b in blocks {
            self.local_images(&mut b.children);
            if let BlockKind::Image { src, .. } = &mut b.kind && !src.contains("://") && !Path::new(src.as_str()).fs_is_absolute() {
                match self.asset(src).and_then(|p| self.assets.copy(&p)) {
                    Some(p) => *src = p.to_string_lossy().to_string(),
                    None => self.report.missing_assets.push(src.clone()),
                }
            }
        }
    }

    fn media(&mut self, n: &Value, out: &mut Vec<Block>) {
        let src = html_src(st(n, "Data")).unwrap_or_default();
        if let Some(from) = self.asset(&src) && let Some(to) = self.assets.copy(&from) {
            let f = FileBlock { src_kind: "path".into(), src: to.to_string_lossy().to_string(), name: asset_name(&from), view: FileView::Preview };
            out.push(Block::new(BlockKind::File(f), RichText::default()));
            return;
        }
        if !src.contains("://") && !src.is_empty() { self.report.missing_assets.push(src.clone()); }
        self.degrade(ty(n));
        let mut text = RichText::default();
        text.push(&src, Marks { link: src.contains("://").then(|| src.clone()), ..Default::default() });
        out.push(Block::new(BlockKind::Text, text));
    }

    /// 嵌入块: a query by block ids becomes a copy of those blocks (a
    /// heading's with its section, a document's with its content); any
    /// other query is kept, as code, under a note.
    fn embed(&mut self, n: &Value, out: &mut Vec<Block>) {
        let script = child(n, "NodeBlockQueryEmbedScript").map(|s| st(s, "Data").to_string())
            .unwrap_or_else(|| st(n, "Data").trim().trim_start_matches("{{").trim_end_matches("}}").to_string());
        let ids: Vec<&str> = script.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).filter(|w| is_id(w)).collect();
        let by_id = { let s = script.to_lowercase(); s.contains("id") && (s.contains(" in ") || s.contains('=')) && !s.contains(" like ") };
        if by_id && !ids.is_empty() && self.depth < 3 && ids.iter().all(|i| self.index.contains_key(*i)) {
            self.report.embeds += 1;
            self.depth += 1;
            for id in ids {
                let (d, target) = self.index[id].clone();
                let docs = self.docs;
                let tree = &docs[d].tree;
                match target {
                    Target::Doc => self.blocks(kids(tree), out),
                    _ => if let Some((siblings, i)) = find(kids(tree), id) {
                        let level = |n: &Value| (ty(n) == "NodeHeading").then(|| n["HeadingLevel"].as_u64().unwrap_or(1));
                        let end = match level(&siblings[i]) {
                            Some(l) => siblings[i + 1..].iter().position(|s| level(s).is_some_and(|m| m <= l)).map_or(siblings.len(), |p| i + 1 + p),
                            None => i + 1,
                        };
                        self.blocks(&siblings[i..end], out);
                    },
                }
            }
            self.depth -= 1;
            return;
        }
        self.report.queries += 1;
        out.push(Block::text(&self.labels.query));
        out.push(code_block("sql", script.trim()));
    }

    fn database(&mut self, n: &Value, out: &mut Vec<Block>) {
        let id = st(n, "AttributeViewID");
        let json = self.src.avs.iter().map(|d| d.join(format!("{id}.json"))).find(|p| p.fs_is_file()).and_then(|p| crate::vfs::read_to_string(p).ok());
        let (docs, index) = (self.docs, self.index);
        let title = |b: &str| index.get(b).map(|(d, _)| docs[*d].title.clone());
        match json.and_then(|j| av::read(&j, prop(n, "custom-sy-av-view"), &title)) {
            Some(av) => {
                self.report.databases.push(if av.name.trim().is_empty() { id.to_string() } else { av.name.clone() });
                out.extend(self.dbs.place(&av));
            }
            None => {
                self.degrade("NodeAttributeView");
                let mut t = RichText::default();
                t.push(id, Marks { code: true, ..Default::default() });
                out.push(Block::new(BlockKind::Text, t));
            }
        }
    }

    /// Where a 思源 asset path (`assets/…`, beside the document or in the
    /// workspace) is on disk.
    fn asset(&self, href: &str) -> Option<PathBuf> {
        let href = crate::markdown::percent_decode(href.trim());
        let href = href.trim_start_matches("./");
        if let Some(p) = href.strip_prefix("file://").map(PathBuf::from) { return p.fs_is_file().then_some(p) }
        let rest = href.strip_prefix("assets/")?;
        let rel = PathBuf::from(rest);
        if !rel.components().all(|c| matches!(c, Component::Normal(_))) { return None }
        let beside = self.docs.get(self.at).and_then(|d| d.file.parent()).map(|d| d.join("assets"));
        beside.into_iter().chain(self.src.assets.iter().cloned()).map(|d| d.join(&rel)).find(|p| p.fs_is_file())
    }

    fn image(&mut self, n: &Value) -> Block {
        let part = |t: &str| child(n, t).map(|c| st(c, "Data").to_string()).unwrap_or_default();
        let (dest, title) = (part("NodeLinkDest"), part("NodeLinkTitle"));
        let src = if dest.contains("://") { dest.clone() } else {
            match self.asset(&dest).and_then(|p| self.assets.copy(&p)) {
                Some(p) => p.to_string_lossy().to_string(),
                None => { self.report.missing_assets.push(dest.clone()); dest.clone() }
            }
        };
        let width = css(prop(n, "style"), "width").and_then(|w| w.strip_suffix("px")).and_then(|w| w.trim().parse::<f32>().ok());
        let mut b = Block::new(BlockKind::Image { src_kind: "src".into(), src, width, height: None, caption: Some(title).filter(|t| !t.trim().is_empty()), crop: None }, RichText::default());
        b.align = Align::Center;
        b
    }

    fn inline(&mut self, nodes: &[Value], m: &Marks, out: &mut Vec<Piece>) {
        for n in nodes {
            let with = |f: &dyn Fn(&mut Marks)| { let mut m = m.clone(); f(&mut m); m };
            match ty(n) {
                "NodeText" => push(out, &st(n, "Data").replace('\u{200b}', ""), m),
                "NodeTextMark" => self.text_mark(n, m, out),
                "NodeBr" => push(out, "\n", m),
                "NodeImage" => { let b = self.image(n); out.push(Piece::Image(b)) }
                "NodeInlineHTML" | "NodeHTMLInline" => {
                    let s = st(n, "Data");
                    if s.trim_start().to_ascii_lowercase().starts_with("<br") { push(out, "\n", m) } else { self.degrade("NodeInlineHTML"); push(out, s, m) }
                }
                "NodeEmojiImg" | "NodeEmojiAlias" => { self.degrade("NodeEmojiImg"); push(out, &plain(n), m) }
                "NodeFootnotesRef" => { self.degrade("NodeFootnotesRef"); push(out, &format!("[^{}]", st(n, "Data").trim_start_matches('^')), m) }
                // Before TextMark: a node per style.
                "NodeStrong" => self.inline(kids(n), &with(&|m| m.bold = true), out),
                "NodeEmphasis" => self.inline(kids(n), &with(&|m| m.italic = true), out),
                "NodeStrikethrough" => self.inline(kids(n), &with(&|m| m.strike = true), out),
                "NodeUnderline" => self.inline(kids(n), &with(&|m| m.underline = true), out),
                "NodeMark" => self.inline(kids(n), &with(&|m| { m.bg_color.get_or_insert(ColorName("light-yellow".into())); }), out),
                "NodeKbd" => { self.degrade("kbd"); self.inline(kids(n), &with(&|m| m.code = true), out) }
                "NodeSup" | "NodeSub" => { self.degrade("sup / sub"); self.inline(kids(n), m, out) }
                "NodeCodeSpan" => push(out, child(n, "NodeCodeSpanContent").map(|c| st(c, "Data")).unwrap_or_default(), &with(&|m| m.code = true)),
                "NodeInlineMath" => {
                    let src = child(n, "NodeInlineMathContent").map(|c| st(c, "Data")).unwrap_or_default();
                    push(out, crate::editor::core::EQUATION, &with(&|m| m.inline = Some(crate::docxml::to_inline(InlineKind::Equation(src.to_string())))));
                }
                "NodeTag" => push(out, &format!("#{}", plain(n)), m),
                "NodeLink" => {
                    let href = child(n, "NodeLinkDest").map(|c| st(c, "Data")).unwrap_or_default().to_string();
                    let text = child(n, "NodeLinkText").map(|c| st(c, "Data")).unwrap_or_default().to_string();
                    self.link(&href, &text, m, out);
                }
                "NodeBlockRef" => {
                    let id = child(n, "NodeBlockRefID").map(|c| st(c, "Data")).unwrap_or_default().to_string();
                    let (text, sub) = match (child(n, "NodeBlockRefText"), child(n, "NodeBlockRefDynText")) { (Some(t), _) => (plain(t), "s"), (_, Some(t)) => (plain(t), "d"), _ => (String::new(), "d") };
                    self.reference(&id, &text, sub, m, out);
                }
                "NodeFileAnnotationRef" => { self.degrade("file-annotation-ref"); push(out, &plain(n), m) }
                t if is_syntax(t) => {}
                _ => if kids(n).is_empty() { push(out, &st(n, "Data").replace('\u{200b}', ""), m) } else { self.inline(kids(n), m, out) },
            }
        }
    }

    fn text_mark(&mut self, n: &Value, base: &Marks, out: &mut Vec<Piece>) {
        let types: Vec<&str> = st(n, "TextMarkType").split_whitespace().collect();
        let has = |t: &str| types.contains(&t);
        let mut m = base.clone();
        let style = prop(n, "style");
        if let Some(c) = css(style, "color").and_then(|v| color(v, false)) { m.text_color = Some(c); }
        if let Some(c) = css(style, "background-color").and_then(|v| color(v, true)) { m.bg_color = Some(c); }
        for t in &types {
            match *t {
                "strong" => m.bold = true,
                "em" => m.italic = true,
                "u" => m.underline = true,
                "s" => m.strike = true,
                "code" => m.code = true,
                "kbd" => { m.code = true; self.degrade("kbd") }
                "mark" => { m.bg_color.get_or_insert(ColorName("light-yellow".into())); }
                "sup" | "sub" => self.degrade("sup / sub"),
                "file-annotation-ref" => self.degrade("file-annotation-ref"),
                "inline-memo" => self.degrade("inline-memo"),
                "text" | "tag" | "a" | "block-ref" | "inline-math" => {}
                other => self.degrade(other),
            }
        }
        let text = st(n, "TextMarkTextContent").replace('\u{200b}', "");
        if has("inline-math") {
            m.inline = Some(crate::docxml::to_inline(InlineKind::Equation(st(n, "TextMarkInlineMathContent").trim().to_string())));
            return push(out, crate::editor::core::EQUATION, &m);
        }
        if has("block-ref") { return self.reference(st(n, "TextMarkBlockRefID"), &text, st(n, "TextMarkBlockRefSubtype"), &m, out) }
        if has("a") { return self.link(st(n, "TextMarkAHref"), &text, &m, out) }
        // A tag, `#标签#` in 思源, is text as Obsidian's `#标签` is.
        push(out, &if has("tag") { format!("#{text}") } else { text }, &m);
        if has("inline-memo") {
            let memo = st(n, "TextMarkInlineMemoContent").trim();
            if !memo.is_empty() { push(out, &format!("（{memo}）"), &Marks { text_color: Some(ColorName("gray".into())), ..base.clone() }); }
        }
    }

    /// A link: to a 思源 asset, the file itself (copied); else as it is.
    fn link(&mut self, href: &str, text: &str, m: &Marks, out: &mut Vec<Piece>) {
        if !href.contains("://") && let Some(from) = self.asset(href) && let Some(to) = self.assets.copy(&from) {
            let name = if text.trim().is_empty() { asset_name(&from) } else { text.trim().to_string() };
            let f = FileBlock { src_kind: "path".into(), src: to.to_string_lossy().to_string(), name, view: FileView::Card };
            return push(out, crate::editor::core::EQUATION, &Marks { inline: Some(crate::docxml::to_inline(InlineKind::File(f))), ..m.clone() });
        }
        if href.starts_with("assets/") { self.report.missing_assets.push(href.to_string()); }
        let text = if text.is_empty() { href } else { text };
        push(out, text, &Marks { link: (!href.is_empty()).then(|| href.to_string()), ..m.clone() });
    }

    /// 块引用 `((id "text"))`: to a document, an @文档 link (or a link
    /// with the reference's own text, when it has one); to a heading, a
    /// link to it; to any other block, a link to its document.
    fn reference(&mut self, target: &str, text: &str, subtype: &str, m: &Marks, out: &mut Vec<Piece>) {
        let (here, page) = (self.docs[self.at].title.clone(), self.lib[self.at].clone());
        let note = |text: &str| Ref { doc: here.clone(), page: page.clone(), text: text.to_string(), target: target.to_string() };
        let Some((d, t)) = self.index.get(target).cloned().filter(|(d, _)| !self.lib[*d].is_empty()) else {
            let shown = if text.is_empty() { format!("(({target}))") } else { text.to_string() };
            self.report.broken_refs.push(note(&shown));
            return push(out, &shown, m);
        };
        let title = self.docs[d].title.clone();
        let text = if text.trim().is_empty() { match &t { Target::Heading(h) => h.clone(), _ => title.clone() } } else { text.to_string() };
        let path = match (&self.files[self.at], &self.files[d]) { _ if d == self.at => String::new(), (Some(a), Some(b)) => relative(a, b), _ => String::new() };
        let href = |anchor: Option<&str>| match anchor { Some(a) => format!("{path}#{}", encode(a)), None => path.clone() };
        match t {
            Target::Doc if subtype != "s" || text == title => {
                let link = InlineDocLink { id: self.lib[d].clone(), title };
                push(out, crate::editor::core::EQUATION, &Marks { inline: Some(crate::docxml::to_inline(InlineKind::DocLink(link))), ..m.clone() })
            }
            Target::Doc => { let h = href(None); push(out, &text, &Marks { link: Some(h).filter(|h| !h.is_empty()), ..m.clone() }) }
            Target::Heading(h) => push(out, &text, &Marks { link: Some(href(Some(&h))), ..m.clone() }),
            Target::Block => {
                self.report.block_refs.push(note(&text));
                let h = href(None);
                push(out, &text, &Marks { link: Some(h).filter(|h| !h.is_empty()), ..m.clone() })
            }
        }
    }
}

/// `nodes` or their descendants holding block `id`: the list it is in, and where.
fn find<'v>(nodes: &'v [Value], id: &str) -> Option<(&'v [Value], usize)> {
    if let Some(i) = nodes.iter().position(|n| st(n, "ID") == id) { return Some((nodes, i)) }
    nodes.iter().find_map(|n| find(kids(n), id))
}

/// A `src="…"` in an HTML tag.
fn html_src(html: &str) -> Option<String> {
    let at = html.find(" src=")? + 5;
    let rest = &html[at..];
    let q = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    Some(rest[1..].split(q).next()?.replace("&amp;", "&"))
}

fn equation(src: &str) -> Block {
    let mut text = RichText::default();
    if !src.is_empty() { text.push(crate::editor::core::EQUATION, Marks { inline: Some(crate::docxml::to_inline(InlineKind::Equation(src.to_string()))), ..Default::default() }); }
    let mut b = Block::new(BlockKind::Text, text);
    b.align = Align::Center;
    b
}

fn code_block(lang: &str, code: &str) -> Block {
    let code = code.strip_suffix('\n').unwrap_or(code);
    Block::new(BlockKind::Code { lang: crate::lang::from_token(lang), caption: None, wrap: false, show_code: false }, RichText::plain(code))
}

/// Line breaks 思源 keeps at a line's ends, which show nothing.
fn trim_breaks(t: RichText) -> RichText {
    let (start, end) = (t.text.len() - t.text.trim_start_matches('\n').len(), t.text.trim_end_matches('\n').len());
    if start >= end { return if start == 0 { t } else { RichText::default() } }
    t.slice(start..end)
}

/// A block's text, shortened, to name it in the attribute table.
fn excerpt(b: &Block) -> String {
    let s: String = b.text.text.replace(crate::editor::core::EQUATION, "").split_whitespace().collect::<Vec<_>>().join(" ");
    let mut short: String = s.chars().take(24).collect();
    if short.len() < s.len() { short.push('…'); }
    short
}

/// A block's `style` attribute: its background, text colour and alignment.
fn styled(b: &mut Block, style: &str) {
    if let Some(c) = css(style, "background-color").and_then(|v| color(v, true)) {
        match &mut b.kind {
            BlockKind::Callout { bg, .. } => *bg = Some(c),
            k if k.takes_bg() => b.bg = Some(c),
            _ => {}
        }
    }
    if let Some(c) = css(style, "color").and_then(|v| color(v, false)) && b.kind.has_text() && !matches!(b.kind, BlockKind::Code { .. }) {
        b.text.update_marks(0..b.text.len(), |m| if m.text_color.is_none() { m.text_color = Some(c.clone()) });
    }
    if let Some(a) = css(style, "text-align") && matches!(b.kind, BlockKind::Text | BlockKind::Heading { .. }) {
        b.align = match a { "center" => Align::Center, "right" => Align::Right, _ => b.align };
    }
}
