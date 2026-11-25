//! 数据表 from the CLI: a table file, or the 数据表 block a document shows.
//! Rows have ids that last (`r12`); values are text, read as the app shows
//! them and written as if pasted (a number column parses numbers, a select
//! gains the options it lacks). Edits are held until commit and written in
//! one SQLite transaction; a file changed meanwhile stops the commit.

use std::path::PathBuf;

use fdl_core::model::BlockKind;
use fdl_core::table::{Base, FieldId, RecId, TableId, TableRef, field, ops, sqlite};
use serde_json::{Value, json};

use crate::blocks;
use crate::fail::{R, fail};
use crate::session::{Session, arg, num};

pub(crate) struct OpenTable {
    src: String,
    path: PathBuf,
    stamp: Option<u64>,
    doc: ops::Doc,
    log: Vec<String>,
}

impl OpenTable {
    pub(crate) fn dirty(&self) -> bool { !self.doc.dirty.is_clean() }
}

fn row_id(s: &str) -> R<RecId> {
    s.trim().trim_start_matches('r').parse::<RecId>().map_err(|_| fail("bad_args", format!("row {s:?}: rows are r1, r2… as `quiet table` lists them")))
}

/// A cell as the text a paste would hold: lists joined with commas.
fn cell_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Array(l) => l.iter().map(cell_text).collect::<Vec<_>>().join(", "),
        x => x.to_string(),
    }
}

impl Session {
    /// The 数据表 a document is or shows: `at` a datatable block, else its first one.
    fn table_ref(&mut self, a: &Value) -> R<TableRef> {
        let i = self.doc_arg(a)?;
        let d = &self.docs[i];
        Ok(match arg(a, "at") {
            Some(x) => { let id = blocks::resolve(&d.doc, x)?; match &blocks::find(&d.doc, id).unwrap().kind { BlockKind::DataTable(t) => t.clone(), k => return Err(fail("bad_kind", format!("{x} is a {} block, not a 数据表", blocks::kind_name(k)))) } }
            None => match d.doc.blocks.iter().find_map(|b| if let BlockKind::DataTable(t) = &b.kind { Some(t.clone()) } else { None }) {
                Some(t) => t,
                None => return Err(fail("bad_kind", format!("{} holds no 数据表", d.path)).hint("pass --at <ref> of a datatable block")),
            },
        })
    }

    /// The table file, read once per run; and which of its tables `a` names.
    fn open_table(&mut self, a: &Value) -> R<(usize, TableId)> {
        let tref = self.table_ref(a)?;
        // Relative to the top-level folder the document is in.
        let i = self.doc_arg(a)?;
        let path = self.top_dir(&self.docs[i].top).join(&tref.src);
        let k = match self.tables.iter().position(|t| t.path == path) {
            Some(k) => k,
            None => {
                let stamp = sqlite::stamp(&path);
                let base = sqlite::load(&path).map_err(|e| fail("io", format!("{}: {e:#}", tref.src)))?;
                self.tables.push(OpenTable { src: tref.src.clone(), path, stamp, doc: ops::Doc::new(base), log: vec![] });
                self.tables.len() - 1
            }
        };
        let base = &self.tables[k].doc.base;
        let name = arg(a, "table").map(str::to_string).or(tref.table.clone());
        let t = match &name {
            Some(n) => base.table_named(n).ok_or_else(|| fail("not_found", format!("no table {n:?}; tables: {}", names(base))))?,
            None => base.tables.first().ok_or_else(|| fail("not_found", "the file holds no tables"))?,
        };
        Ok((k, t.id))
    }

    /// Rows as text by field name, with their ids; `limit` / `offset` page through.
    pub(crate) fn table(&mut self, a: &Value) -> R<Value> {
        let (k, tid) = self.open_table(a)?;
        let base = &self.tables[k].doc.base;
        let t = base.table(tid).unwrap();
        let (offset, limit) = (num(a, "offset").unwrap_or(0), num(a, "limit").unwrap_or(usize::MAX));
        let rows: Vec<Value> = t.records.iter().skip(offset).take(limit).map(|r| {
            let mut o = serde_json::Map::new();
            o.insert("_id".into(), json!(format!("r{}", r.id)));
            for f in &t.fields { o.insert(f.name.clone(), json!(field::display_in(base, f, r.get(f.id)))); }
            Value::Object(o)
        }).collect();
        let schema: Vec<Value> = t.fields.iter().map(|f| {
            let mut v = json!({ "name": f.name, "type": f.ty.key() });
            if f.ty.computed() { v["computed"] = json!(true); }
            if f.ty.select() && !f.opts.options.is_empty() { v["options"] = json!(f.opts.options.iter().map(|o| o.name.as_str()).collect::<Vec<_>>()); }
            v
        }).collect();
        let mut v = json!({ "src": self.tables[k].src, "table": t.name, "tables": base.tables.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), "fields": t.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), "schema": schema, "total": t.records.len(), "rows": rows });
        if offset + limit < t.records.len() { v["more"] = json!(t.records.len() - offset - limit); }
        Ok(v)
    }

    /// The fields an object of cells names, checked: there, and not computed.
    fn fields_of(&self, k: usize, tid: TableId, cells: &serde_json::Map<String, Value>) -> R<Vec<(FieldId, String)>> {
        let t = self.tables[k].doc.base.table(tid).unwrap();
        cells.iter().filter(|(name, _)| name.as_str() != "_id").map(|(name, v)| {
            let f = t.field_named(name).ok_or_else(|| fail("not_found", format!("no field {name:?} in {}", t.name)).hint(format!("fields: {}", t.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", "))))?;
            if f.ty.computed() { return Err(fail("read_only", format!("{name:?} is a {} field, computed by the table", f.ty.key()))); }
            Ok((f.id, cell_text(v)))
        }).collect()
    }

    fn row_objects(a: &Value) -> R<Vec<serde_json::Map<String, Value>>> {
        let rows = match a.get("rows") { Some(Value::Array(l)) => l.clone(), Some(o @ Value::Object(_)) => vec![o.clone()], _ => return Err(fail("bad_args", "rows: a list of {field: value} objects")) };
        rows.into_iter().map(|r| match r { Value::Object(o) => Ok(o), x => Err(fail("bad_args", format!("a row is an object of field: value, not {x}"))) }).collect()
    }

    pub(crate) fn table_add(&mut self, a: &Value) -> R<Value> {
        let (k, tid) = self.open_table(a)?;
        let rows = Self::row_objects(a)?;
        let mut made = vec![];
        for r in &rows {
            let cells = self.fields_of(k, tid, r)?;
            let t = &mut self.tables[k];
            let before: Vec<RecId> = t.doc.base.table(tid).unwrap().records.iter().map(|r| r.id).collect();
            let fids: Vec<FieldId> = cells.iter().map(|c| c.0).collect();
            let line: Vec<String> = cells.into_iter().map(|c| c.1).collect();
            if fids.is_empty() { t.doc.add_records(tid, None, 1, &[]); } else { t.doc.paste(tid, &[], &fids, &[line]); }
            made.extend(t.doc.base.table(tid).unwrap().records.iter().map(|r| r.id).filter(|id| !before.contains(id)));
        }
        let t = &mut self.tables[k];
        t.log.push(format!("+ {} row(s)", made.len()));
        Ok(json!({ "src": t.src, "added": made.iter().map(|id| format!("r{id}")).collect::<Vec<_>>() }))
    }

    pub(crate) fn table_update(&mut self, a: &Value) -> R<Value> {
        let (k, tid) = self.open_table(a)?;
        // `{row, set}` for one row.
        let rows = match (arg(a, "row"), a.get("set")) {
            (Some(r), Some(Value::Object(set))) => { let mut o = set.clone(); o.insert("_id".into(), json!(r)); vec![o] }
            _ => Self::row_objects(a)?,
        };
        let mut done = vec![];
        for r in &rows {
            let id = row_id(r.get("_id").and_then(Value::as_str).ok_or_else(|| fail("bad_args", "each row to update needs its _id (r12)"))?)?;
            if !self.tables[k].doc.base.table(tid).unwrap().records.iter().any(|x| x.id == id) { return Err(fail("not_found", format!("no row r{id}")).hint("`quiet table DOC` lists the rows")); }
            let cells = self.fields_of(k, tid, r)?;
            let fids: Vec<FieldId> = cells.iter().map(|c| c.0).collect();
            let line: Vec<String> = cells.into_iter().map(|c| c.1).collect();
            if !fids.is_empty() { self.tables[k].doc.paste(tid, &[id], &fids, &[line]); }
            done.push(format!("r{id}"));
        }
        let t = &mut self.tables[k];
        t.log.push(format!("~ {}", done.join(" ")));
        Ok(json!({ "src": t.src, "updated": done }))
    }

    pub(crate) fn table_delete(&mut self, a: &Value) -> R<Value> {
        let (k, tid) = self.open_table(a)?;
        let ids: Vec<RecId> = match a.get("rows") {
            Some(Value::Array(l)) => l.iter().map(|v| row_id(&cell_text(v))).collect::<R<_>>()?,
            Some(v) => vec![row_id(&cell_text(v))?],
            None => return Err(fail("bad_args", "rows: the ids to delete")),
        };
        let t = &mut self.tables[k];
        let have: Vec<RecId> = t.doc.base.table(tid).unwrap().records.iter().map(|r| r.id).collect();
        if let Some(x) = ids.iter().find(|id| !have.contains(id)) { return Err(fail("not_found", format!("no row r{x}"))); }
        t.doc.remove_records(tid, &ids);
        t.log.push(format!("- {}", ids.iter().map(|id| format!("r{id}")).collect::<Vec<_>>().join(" ")));
        Ok(json!({ "src": t.src, "deleted": ids.len() }))
    }

    pub(crate) fn check_tables(&self) -> R<()> {
        for t in self.tables.iter().filter(|t| t.dirty()) {
            if sqlite::stamp(&t.path) != t.stamp {
                return Err(fail("conflict", format!("{} changed on disk while this ran; nothing was written", t.src)).hint("read it again and redo the edit"));
            }
        }
        Ok(())
    }

    pub(crate) fn describe_tables(&self) -> Vec<Value> {
        self.tables.iter().filter(|t| t.dirty()).map(|t| json!({ "path": t.src, "action": "would save", "diff": t.log })).collect()
    }

    pub(crate) fn save_tables(&mut self) -> R<Vec<Value>> {
        let mut out = vec![];
        for t in self.tables.iter_mut().filter(|t| t.dirty()) {
            sqlite::save(&t.path, &t.doc.base, &t.doc.dirty).map_err(|e| fail("io", format!("{}: {e:#}", t.src)))?;
            t.doc.dirty = Default::default();
            t.stamp = sqlite::stamp(&t.path);
            out.push(json!({ "path": t.src, "action": "saved", "changes": std::mem::take(&mut t.log) }));
        }
        Ok(out)
    }
}

fn names(base: &Base) -> String { base.tables.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ") }
