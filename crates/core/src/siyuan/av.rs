//! 思源's 数据库 (attribute views, `storage/av/<id>.json`, kernel/av in
//! siyuan-note/siyuan): read into names and cell text. Today a database
//! becomes a static table on its page ([`StaticTables`]); an import into a
//! 数据表 later implements [`Databases`] and takes the same [`AttributeView`].

use crate::model::{Block, BlockKind, Marks, RichText, Table, TableCell};
use serde_json::Value;

/// One database as its active view shows it: the visible columns in their
/// order, and each row's cells as text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AttributeView {
    pub id: String,
    pub name: String,
    /// (name, 思源 field type: `block`, `text`, `number`, `date`, `select`, `mSelect`, …).
    pub columns: Vec<(String, String)>,
    pub rows: Vec<Vec<String>>,
}

/// Where a database on a page goes.
pub trait Databases {
    /// The blocks that take the database's place on its page.
    fn place(&mut self, av: &AttributeView) -> Vec<Block>;
}

/// The database's name in bold, then its rows as a table with a header row.
pub struct StaticTables;

impl Databases for StaticTables {
    fn place(&mut self, av: &AttributeView) -> Vec<Block> {
        let mut out = vec![];
        if !av.name.trim().is_empty() {
            let mut t = RichText::default();
            t.push(av.name.trim(), Marks { bold: true, ..Default::default() });
            out.push(Block::new(BlockKind::Text, t));
        }
        if av.columns.is_empty() { return out }
        let cell = |s: &str| TableCell { blocks: s.split('\n').map(Block::text).collect(), ..TableCell::empty() };
        let mut rows = vec![av.columns.iter().map(|(n, _)| cell(n)).collect::<Vec<_>>()];
        rows.extend(av.rows.iter().map(|r| (0..av.columns.len()).map(|i| cell(r.get(i).map_or("", String::as_str))).collect()));
        out.push(Block::new(BlockKind::Table(Table { col_widths: vec![Table::DEFAULT_WIDTH * 1.5; av.columns.len()], header_rows: 1, header_col: false, rows, axes: Default::default() }), RichText::default()));
        out
    }
}

/// `json` (an attribute view file) as its view `view` (the block's
/// `custom-sy-av-view`; else the file's current view, else the first) shows it;
/// `title` names a bound block (`block` field) that has no text of its own.
pub fn read(json: &str, view: &str, title: &dyn Fn(&str) -> Option<String>) -> Option<AttributeView> {
    let v: Value = serde_json::from_str(json).ok()?;
    let keys = v["keyValues"].as_array()?;
    let views = v["views"].as_array().map(Vec::as_slice).unwrap_or_default();
    let pick = |id: &str| views.iter().find(|x| !id.is_empty() && x["id"].as_str() == Some(id));
    let shown = pick(view).or_else(|| pick(v["viewID"].as_str().unwrap_or_default())).or(views.first());
    // Every layout keeps its fields under its own name (`table`, `gallery`, `kanban`, …).
    let layout = shown.and_then(|s| s.as_object()).and_then(|o| o.values().find(|x| x.get("columns").or(x.get("fields")).is_some()));
    let key_of = |id: &str| keys.iter().find(|k| k["key"]["id"].as_str() == Some(id));
    let cols: Vec<&Value> = match layout.and_then(|l| l.get("columns").or(l.get("fields"))).and_then(Value::as_array) {
        Some(c) => c.iter().filter(|c| !c["hidden"].as_bool().unwrap_or(false)).filter_map(|c| key_of(c["id"].as_str()?)).collect(),
        None => keys.iter().collect(),
    };
    // Rows in the view's order, then any it does not list, as the primary key's values have them.
    let primary = keys.iter().find(|k| k["key"]["type"].as_str() == Some("block")).or(keys.first());
    let mut ids: Vec<String> = layout.and_then(|l| l["rowIds"].as_array().or(l["itemIds"].as_array())).map(|r| r.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
    for x in primary.and_then(|k| k["values"].as_array()).map(Vec::as_slice).unwrap_or_default() {
        if let Some(b) = x["blockID"].as_str() && !ids.iter().any(|i| i == b) { ids.push(b.to_string()); }
    }
    let rows = ids.iter().enumerate().map(|(n, row)| cols.iter().map(|k| {
        let ty = k["key"]["type"].as_str().unwrap_or_default();
        if ty == "lineNumber" { return (n + 1).to_string() }
        let value = k["values"].as_array().and_then(|vs| vs.iter().find(|x| x["blockID"].as_str() == Some(row.as_str())));
        match value { Some(x) => text(x, ty, title), None if ty == "checkbox" => "☐".into(), None => String::new() }
    }).collect()).collect();
    Some(AttributeView {
        id: v["id"].as_str().unwrap_or_default().to_string(),
        name: v["name"].as_str().unwrap_or_default().to_string(),
        columns: cols.iter().map(|k| (k["key"]["name"].as_str().unwrap_or_default().to_string(), k["key"]["type"].as_str().unwrap_or_default().to_string())).collect(),
        rows,
    })
}

/// A local date (and time, unless `time` is false) of unix milliseconds.
fn date(ms: i64, time: bool) -> String {
    let (y, m, d, h, mi) = crate::time::civil((ms / 1000).max(0) as u64);
    if time { format!("{y}-{m:02}-{d:02} {h:02}:{mi:02}") } else { format!("{y}-{m:02}-{d:02}") }
}

/// One cell's value as text.
fn text(x: &Value, ty: &str, title: &dyn Fn(&str) -> Option<String>) -> String {
    let content = |o: &Value| match &o["content"] { Value::String(s) => s.clone(), Value::Number(n) => n.to_string(), _ => String::new() };
    let o = &x[ty];
    match ty {
        "block" => { let c = content(o); if c.is_empty() { o["id"].as_str().and_then(title).unwrap_or_default() } else { c } }
        "number" => if o["isNotEmpty"].as_bool() == Some(false) { String::new() } else { o["formattedContent"].as_str().filter(|s| !s.is_empty()).map_or_else(|| content(o), str::to_string) }
        "date" | "created" | "updated" => {
            if o["isNotEmpty"].as_bool() == Some(false) && ty == "date" { return String::new() }
            let time = !o["isNotTime"].as_bool().unwrap_or(false);
            let mut s = o["content"].as_i64().map(|ms| date(ms, time)).unwrap_or_default();
            if o["hasEndDate"].as_bool() == Some(true) && let Some(end) = o["content2"].as_i64() { s = format!("{s} → {}", date(end, time)); }
            s
        }
        "checkbox" => if o["checked"].as_bool() == Some(true) { "☑".into() } else { "☐".into() },
        "relation" => {
            let shown: Vec<String> = o["contents"].as_array().map(|c| c.iter().map(|v| content(&v["block"])).filter(|s| !s.is_empty()).collect()).unwrap_or_default();
            if !shown.is_empty() { return shown.join(", ") }
            o["blockIDs"].as_array().map(|ids| ids.iter().filter_map(|i| i.as_str()).map(|i| title(i).unwrap_or_else(|| i.to_string())).collect::<Vec<_>>().join(", ")).unwrap_or_default()
        }
        "rollup" => o["contents"].as_array().map(|c| c.iter().map(|v| v["type"].as_str().map(|t| text(v, t, title)).unwrap_or_default()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ")).unwrap_or_default(),
        "mSelect" | "select" | "mAsset" => x[if ty == "mAsset" { "mAsset" } else { "mSelect" }].as_array().map(|a| a.iter().map(|v| {
            let c = content(v);
            match v["name"].as_str() { Some(n) if !n.is_empty() && ty == "mAsset" => n.to_string(), _ => c }
        }).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ")).unwrap_or_default(),
        _ => content(o),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AV: &str = r#"{"spec":1,"id":"20240101000000-avavavv","name":"书单","viewID":"v1",
      "keyValues":[
        {"key":{"id":"k1","name":"书名","type":"block"},"values":[
          {"blockID":"r1","block":{"id":"20240101000000-doc0001","content":""}},
          {"blockID":"r2","block":{"content":"三体"}}]},
        {"key":{"id":"k2","name":"评分","type":"number"},"values":[{"blockID":"r2","number":{"content":9.5,"isNotEmpty":true}}]},
        {"key":{"id":"k3","name":"标签","type":"mSelect"},"values":[{"blockID":"r2","mSelect":[{"content":"科幻"},{"content":"长篇"}]}]},
        {"key":{"id":"k4","name":"读完","type":"checkbox"},"values":[{"blockID":"r1","checkbox":{"checked":true}}]},
        {"key":{"id":"k5","name":"藏起来","type":"text"},"values":[]}],
      "views":[{"id":"v1","name":"表格","type":"table","table":{"columns":[{"id":"k1"},{"id":"k3"},{"id":"k2"},{"id":"k4"},{"id":"k5","hidden":true}],"rowIds":["r2"]}}]}"#;

    #[test]
    fn a_database_reads_as_its_view_shows_it() {
        let av = read(AV, "", &|id| (id == "20240101000000-doc0001").then(|| "活着".to_string())).unwrap();
        assert_eq!(av.name, "书单");
        assert_eq!(av.columns.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), ["书名", "标签", "评分", "读完"]);
        assert_eq!(av.rows, [vec!["三体", "科幻, 长篇", "9.5", "☐"], vec!["活着", "", "", "☑"]]);
        let blocks = StaticTables.place(&av);
        let BlockKind::Table(t) = &blocks[1].kind else { panic!() };
        assert_eq!((t.rows.len(), t.header_rows, blocks[0].text.text.as_str()), (3, 1, "书单"));
    }
}
