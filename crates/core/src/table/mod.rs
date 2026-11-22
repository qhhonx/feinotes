//! 数据表 (多维表格): the engine. `model` is the data, `field` what each
//! field type does with a value, `ops` every change (with undo), `calc` the
//! computed columns, `formula` the 公式 language, `query` a view's rows
//! (筛选 / 排序 / 分组 / 统计), `actions` the edits the UI asks for, `sqlite`
//! the file, `csv` import / export and the clipboard. No GPUI.

pub mod actions;
pub mod calc;
pub mod csv;
pub mod field;
pub mod formula;
#[cfg(test)]
mod fuzz_tests;
pub mod model;
pub mod ops;
pub mod query;
pub mod sample;
#[cfg(not(target_family = "wasm"))]
pub mod sqlite;

pub use model::*;
pub use ops::{BaseOp, Dirty, Doc, Op};

/// The block's default height in a page.
pub const DEFAULT_HEIGHT: f32 = 420.;

/// A table or column name the file can carry as it is: not empty, not one of
/// our bookkeeping names (`_fields`, `id`), nothing SQLite reserves.
pub fn sqlite_name_ok(n: &str) -> bool {
    let l = n.to_ascii_lowercase();
    !n.trim().is_empty() && n.trim() == n && !n.starts_with('_') && !l.starts_with("sqlite_") && l != "id" && !n.contains('\0')
}

/// A 数据表 block: which file, and which of its tables and views shows.
/// The data itself lives in the file (`.sqlite`), not in the page.
#[derive(Clone, Debug, PartialEq)]
pub struct TableRef {
    /// Relative to the library (`.assets/<uuid>.sqlite`, or a table file's own path).
    pub src: String,
    /// The table and view shown, by name (None: the first).
    pub table: Option<String>,
    pub view: Option<String>,
    pub height: f32,
    /// Dragged wider or narrower than the text column, centred on it (None: the column's width).
    pub width: Option<f32>,
}

impl TableRef {
    pub fn new(src: impl Into<String>) -> Self { TableRef { src: src.into(), table: None, view: None, height: DEFAULT_HEIGHT, width: None } }
}

/// A table file as the page the store hands out: its title and one 数据表 block showing it.
pub fn file_doc(title: &str, rel: &str) -> crate::model::Document {
    let mut doc = crate::model::Document::default();
    doc.title = crate::model::RichText::plain(title);
    doc.blocks = vec![crate::model::Block::new(crate::model::BlockKind::DataTable(TableRef::new(rel)), crate::model::RichText::default())];
    doc
}

/// The table and view a block shows: by name, else the first.
pub fn shown<'a>(base: &'a Base, r: &TableRef) -> Option<(&'a Table, &'a View)> {
    let t = r.table.as_deref().and_then(|n| base.table_named(n)).or(base.tables.first())?;
    let v = r.view.as_deref().and_then(|n| t.views.iter().find(|v| v.name == n)).or(t.views.first())?;
    Some((t, v))
}

/// Resolve a file-library table without following a path out of that library.
#[cfg(not(target_family = "wasm"))]
pub fn resolve_file(root: &std::path::Path, src: &str) -> Option<std::path::PathBuf> {
    use std::path::{Component, Path};
    #[cfg(feature = "sqlite-store")]
    if root.join(crate::store::SQLITE_FILE).is_file() { return None; }
    let relative = Path::new(src);
    if !relative.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("sqlite"))
        || !relative.components().all(|c| matches!(c, Component::Normal(_))) { return None; }
    let root = root.canonicalize().ok()?;
    let file = root.join(relative).canonicalize().ok()?;
    (file.starts_with(root) && file.is_file()).then_some(file)
}

/// A 数据表 block as a Markdown table (导出): the view it shows, as it is filtered and sorted.
#[cfg(not(target_family = "wasm"))]
pub fn block_markdown(root: &std::path::Path, r: &TableRef) -> String {
    let Some(base) = resolve_file(root, &r.src).and_then(|path| sqlite::load(&path).ok()) else { return String::new() };
    let base = Doc::new(base).base;
    let Some((t, v)) = shown(&base, r) else { return String::new() };
    csv::to_markdown(&base, t, v, &query::run(&base, t, v).records)
}
#[cfg(target_family = "wasm")]
pub fn block_markdown(_root: &std::path::Path, _r: &TableRef) -> String { String::new() }

/// A 数据表 block's words (搜索): every cell of the file.
#[cfg(not(target_family = "wasm"))]
pub fn block_text(root: &std::path::Path, r: &TableRef) -> String {
    let Some(base) = resolve_file(root, &r.src).and_then(|path| sqlite::load(&path).ok()) else { return String::new() };
    let base = Doc::new(base).base;
    base.tables.iter().map(|t| csv::plain_text(&base, t)).collect::<Vec<_>>().join("\n")
}
#[cfg(target_family = "wasm")]
pub fn block_text(_root: &std::path::Path, _r: &TableRef) -> String { String::new() }

/// A 数据表 file as lines of text, a line per row (版本历史 counts changes on these).
#[cfg(not(target_family = "wasm"))]
pub fn diff_text(path: &std::path::Path) -> String {
    let Ok(base) = sqlite::load(path) else { return String::new() };
    let base = Doc::new(base).base;
    let Some((t, v)) = base.tables.first().and_then(|t| Some((t, t.views.first()?))) else { return String::new() };
    csv::export(&base, t, v, &query::run(&base, t, v).records).iter().map(|r| r.join("\t")).collect::<Vec<_>>().join("\n")
}
#[cfg(target_family = "wasm")]
pub fn diff_text(_path: &std::path::Path) -> String { String::new() }

/// A 数据表 block as a grid of text, header first (HTML 导出, printing).
#[cfg(not(target_family = "wasm"))]
pub fn block_grid(root: &std::path::Path, r: &TableRef) -> Vec<Vec<String>> {
    let Some(base) = resolve_file(root, &r.src).and_then(|path| sqlite::load(&path).ok()) else { return vec![] };
    let base = Doc::new(base).base;
    let Some((t, v)) = shown(&base, r) else { return vec![] };
    csv::export(&base, t, v, &query::run(&base, t, v).records)
}
#[cfg(target_family = "wasm")]
pub fn block_grid(_root: &std::path::Path, _r: &TableRef) -> Vec<Vec<String>> { vec![] }
