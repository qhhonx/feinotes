//! The edits the UI asks for, each built from `Op`s and run as one undo
//! step: new records with their defaults, fields added / retyped / removed
//! (with a 双向关联's mirror field kept in step), paste, fill, duplicate.

use super::field;
use super::formula;
use super::model::*;
use super::ops::{BaseOp, Doc, Op};
use std::collections::{HashMap, HashSet};

impl Doc {
    pub fn table(&self, t: TableId) -> Option<&Table> { self.base.table(t) }

    /// A record for `t`, not yet in it: fresh id, the fields' default values.
    fn blank_record(&self, t: &Table, n: u64) -> Record {
        let now = field::now();
        let mut r = Record { id: t.next_rec + n, created: now, modified: now, auto: t.next_auto + n, ..Default::default() };
        for f in t.fields.iter().filter(|f| !f.ty.computed() && !f.opts.default.is_empty()) {
            let v = if f.ty.is_date() && f.opts.default == "now" { Value::Date(if f.opts.time { now - now.rem_euclid(60_000) } else { field::today() }) } else { field::parse(f, &f.opts.default) };
            r.set(f.id, v);
        }
        r
    }

    /// Insert `n` empty records at row `at` (None: the end), `with` cells set
    /// on each (a group's value, a filter's). Returns their ids.
    pub fn add_records(&mut self, tid: TableId, at: Option<usize>, n: usize, with: &[(FieldId, Value)]) -> Vec<RecId> {
        let Some(t) = self.base.table(tid) else { return vec![] };
        let at = at.unwrap_or(t.records.len()).min(t.records.len());
        let recs: Vec<(usize, Record)> = (0..n).map(|i| {
            let mut r = self.blank_record(t, i as u64);
            for (f, v) in with { if t.field(*f).is_some_and(|f| !f.ty.computed()) { r.set(*f, v.clone()); } }
            (at + i, r)
        }).collect();
        let ids = recs.iter().map(|r| r.1.id).collect();
        self.run_in(tid, vec![Op::InsertRecords(recs)]);
        ids
    }

    /// Copies of `ids`, each under its original.
    pub fn duplicate_records(&mut self, tid: TableId, ids: &[RecId]) -> Vec<RecId> {
        let Some(t) = self.base.table(tid) else { return vec![] };
        let mut recs = vec![];
        let mut n = 0;
        for (i, r) in t.records.iter().enumerate().filter(|(_, r)| ids.contains(&r.id)) {
            let mut c = self.blank_record(t, n);
            c.cells = r.cells.iter().filter(|(f, _)| t.field(**f).is_some_and(|f| !f.ty.computed())).map(|(f, v)| (*f, v.clone())).collect();
            n += 1;
            recs.push((i + n as usize, c));
        }
        let out = recs.iter().map(|r| r.1.id).collect();
        self.run_in(tid, vec![Op::InsertRecords(recs)]);
        out
    }

    /// Delete records; other tables' 关联 cells let go of them.
    pub fn remove_records(&mut self, tid: TableId, ids: &[RecId]) -> bool {
        let gone: HashSet<RecId> = ids.iter().copied().collect();
        let mut ops = vec![];
        for t in &self.base.tables {
            for f in t.fields.iter().filter(|f| f.ty == FieldType::Link && f.opts.link_table == Some(tid)) {
                let cells: Vec<(RecId, FieldId, Value)> = t.records.iter().filter(|r| t.id != tid || !gone.contains(&r.id)).filter_map(|r| match r.get(f.id) {
                    Value::Links(l) if l.iter().any(|id| gone.contains(id)) => Some((r.id, f.id, Value::Links(l.iter().copied().filter(|id| !gone.contains(id)).collect()))),
                    _ => None,
                }).collect();
                if !cells.is_empty() { ops.push(BaseOp::Table(t.id, Op::SetCells(cells))); }
            }
        }
        ops.push(BaseOp::Table(tid, Op::RemoveRecords(ids.to_vec())));
        self.run(ops)
    }

    /// Move `ids` (kept in their order) so they sit before the record now at row `to`.
    /// Nothing happens (false) when they are there already.
    pub fn move_records(&mut self, tid: TableId, ids: &[RecId], to: usize) -> bool { self.move_records_in(tid, ids, to, false) }

    /// `move_records`, joined to the previous undo step.
    pub fn move_records_joined(&mut self, tid: TableId, ids: &[RecId], to: usize) -> bool { self.move_records_in(tid, ids, to, true) }

    fn move_records_in(&mut self, tid: TableId, ids: &[RecId], to: usize, joined: bool) -> bool {
        let Some(t) = self.base.table(tid) else { return false };
        let before: Option<RecId> = t.records.iter().skip(to).map(|r| r.id).find(|id| !ids.contains(id));
        let moved: Vec<RecId> = t.records.iter().map(|r| r.id).filter(|id| ids.contains(id)).collect();
        let mut order: Vec<RecId> = t.records.iter().map(|r| r.id).filter(|id| !ids.contains(id)).collect();
        let at = before.and_then(|b| order.iter().position(|id| *id == b)).unwrap_or(order.len());
        order.splice(at..at, moved);
        if order.iter().copied().eq(t.records.iter().map(|r| r.id)) { return false }
        let ops = vec![BaseOp::Table(tid, Op::SetOrder(order))];
        if joined { self.run_joined(ops) } else { self.run(ops) }
    }

    /// Set cells, as typed. A 双向关联's other side follows; computed fields are skipped.
    pub fn set_cells(&mut self, tid: TableId, cells: Vec<(RecId, FieldId, Value)>) -> bool { let ops = self.cell_ops(tid, cells); self.run(ops) }

    /// `set_cells`, joined to the previous undo step.
    pub fn set_cells_joined(&mut self, tid: TableId, cells: Vec<(RecId, FieldId, Value)>) -> bool { let ops = self.cell_ops(tid, cells); self.run_joined(ops) }

    fn cell_ops(&self, tid: TableId, cells: Vec<(RecId, FieldId, Value)>) -> Vec<BaseOp> {
        let Some(t) = self.base.table(tid) else { return vec![] };
        let cells: Vec<(RecId, FieldId, Value)> = cells.into_iter().filter(|(_, f, _)| t.field(*f).is_some_and(|f| !f.ty.computed())).map(|(r, f, v)| {
            // a 单向 link to one record keeps one
            let v = match (&v, t.field(f)) { (Value::Links(l), Some(fl)) if fl.opts.single && l.len() > 1 => Value::Links(vec![l[l.len() - 1]]), _ => v };
            (r, f, v)
        }).collect();
        let mut ops = vec![];
        // mirrors: for each changed link cell, the records gained and lost on the other side
        let mut mirror: HashMap<(TableId, FieldId), HashMap<RecId, (Vec<RecId>, Vec<RecId>)>> = HashMap::new();
        for (rid, fid, v) in &cells {
            let Some(f) = t.field(*fid).filter(|f| f.ty == FieldType::Link) else { continue };
            let (Some(other), Some(back)) = (f.opts.link_table, f.opts.back) else { continue };
            let Some(r) = t.record(*rid) else { continue };
            let old: Vec<RecId> = match r.get(*fid) { Value::Links(l) => l.clone(), _ => vec![] };
            let new: Vec<RecId> = match v { Value::Links(l) => l.clone(), _ => vec![] };
            let m = mirror.entry((other, back)).or_default();
            for id in new.iter().filter(|id| !old.contains(id)) { m.entry(*id).or_default().0.push(*rid); }
            for id in old.iter().filter(|id| !new.contains(id)) { m.entry(*id).or_default().1.push(*rid); }
        }
        ops.push(BaseOp::Table(tid, Op::SetCells(cells)));
        for ((other, back), changes) in mirror {
            let Some(ot) = self.base.table(other) else { continue };
            let set: Vec<(RecId, FieldId, Value)> = changes.into_iter().filter_map(|(rid, (add, del))| {
                let r = ot.record(rid)?;
                let mut l: Vec<RecId> = match r.get(back) { Value::Links(l) => l.clone(), _ => vec![] };
                l.retain(|id| !del.contains(id));
                for id in add { if !l.contains(&id) { l.push(id); } }
                Some((rid, back, Value::Links(l)))
            }).collect();
            if !set.is_empty() { ops.push(BaseOp::Table(other, Op::SetCells(set))); }
        }
        ops
    }

    /// Add a field after position `at` (None: last). A 双向关联 gets its mirror in the other table.
    pub fn add_field(&mut self, tid: TableId, at: Option<usize>, name: &str, ty: FieldType, opts: FieldOpts) -> Option<FieldId> {
        let t = self.base.table(tid)?;
        let name = t.free_field_name(if name.trim().is_empty() { ty.label() } else { name }, None);
        let at = at.unwrap_or(t.fields.len());
        let id = self.base.new_id();
        let mut f = Field { id, name, ty, opts, desc: String::new() };
        let mut ops = vec![];
        if ty == FieldType::Link && f.opts.link_table.is_none() { f.opts.link_table = Some(tid); }
        if ty == FieldType::Link && let Some(other) = f.opts.link_table.filter(|o| f.opts.back == Some(0) && *o != tid) {
            // `back: Some(0)` asks for a mirror
            let ot = self.base.table(other)?;
            let (oname, olen) = (ot.free_field_name(&self.base.table(tid)?.name, None), ot.fields.len());
            let bid = self.base.new_id();
            f.opts.back = Some(bid);
            let back = Field { id: bid, name: oname, ty: FieldType::Link, opts: FieldOpts { link_table: Some(tid), back: Some(id), ..Default::default() }, desc: String::new() };
            ops.push(BaseOp::Table(other, Op::InsertField { at: olen, field: back, cells: vec![] }));
        } else if f.opts.back == Some(0) { f.opts.back = None; }
        ops.insert(0, BaseOp::Table(tid, Op::InsertField { at, field: f, cells: vec![] }));
        self.run(ops).then_some(id)
    }

    /// A copy of a field with its cells, right after it.
    pub fn duplicate_field(&mut self, tid: TableId, fid: FieldId) -> Option<FieldId> {
        let t = self.base.table(tid)?;
        let at = t.fields.iter().position(|f| f.id == fid)?;
        let mut f = t.fields[at].clone();
        f.name = t.free_field_name(&format!("{} 副本", f.name), None);
        f.opts.back = None;
        let cells: Vec<(RecId, Value)> = if f.ty.computed() { vec![] } else { t.records.iter().filter_map(|r| r.cells.get(&fid).map(|v| (r.id, v.clone()))).collect() };
        let view_ops: Vec<Op> = t.views.iter().filter(|v| v.opts.order.contains(&fid)).cloned().collect::<Vec<_>>().into_iter().map(Op::SetView).collect();
        f.id = self.base.new_id();
        let id = f.id;
        let mut ops = vec![Op::InsertField { at: at + 1, field: f, cells }];
        // in views with their own order the copy sits after the original too
        for op in view_ops {
            let Op::SetView(mut v) = op else { continue };
            let i = v.opts.order.iter().position(|x| *x == fid).unwrap();
            v.opts.order.insert(i + 1, id);
            ops.push(Op::SetView(v));
        }
        self.run_in(tid, ops).then_some(id)
    }

    /// Change a field's name, type or settings. Cells are carried over to a new
    /// type (`field::convert`), selects drop options that are gone, formulas
    /// that name the field are rewritten for a new name.
    pub fn change_field(&mut self, tid: TableId, mut new: Field) -> bool {
        let Some(t) = self.base.table(tid) else { return false };
        let Some(at) = t.fields.iter().position(|f| f.id == new.id) else { return false };
        let old = t.fields[at].clone();
        new.name = t.free_field_name(&new.name, Some(new.id));
        // the primary field stays something a title can be
        if at == 0 && !(new.ty.textual() || matches!(new.ty, FieldType::Number | FieldType::Date | FieldType::Formula | FieldType::AutoNumber | FieldType::Currency | FieldType::Phone | FieldType::Email)) { new.ty = old.ty; }
        if new == old { return false; }
        let mut ops: Vec<BaseOp> = vec![];
        let mut next_id = self.base.next_id;
        let gone_opts = old.opts.options.iter().any(|o| new.option(o.id).is_none());
        let cells = if new.ty != old.ty || (new.ty == FieldType::Link && new.opts.link_table != old.opts.link_table) {
            if new.ty != old.ty && !new.ty.select() { new.opts.options.clear(); }
            Some(t.records.iter().filter_map(|r| { let v = field::convert(&old, &mut new, r.get(old.id), &mut next_id); (!v.is_empty()).then_some((r.id, v)) }).collect::<Vec<_>>())
        } else if gone_opts && new.ty.select() {
            Some(t.records.iter().filter_map(|r| match r.get(old.id) {
                Value::Select(ids) => { let v = Value::Select(ids.iter().copied().filter(|id| new.option(*id).is_some()).collect()).normal(); (!v.is_empty()).then_some((r.id, v)) }
                _ => None,
            }).collect())
        } else { None };
        // a link that changed type or table leaves its mirror behind
        if old.ty == FieldType::Link && let (Some(other), Some(back)) = (old.opts.link_table, old.opts.back) && (new.ty != FieldType::Link || new.opts.link_table != old.opts.link_table) {
            ops.push(BaseOp::Table(other, Op::RemoveField(back)));
            new.opts.back = None;
        }
        if new.ty != FieldType::Link { new.opts.link_table = None; new.opts.back = None; }
        // a mirror is only made with a new field (`add_field`): here it would point at nothing
        if new.opts.back == Some(0) || (new.opts.back.is_some() && new.opts.back != old.opts.back) { new.opts.back = old.opts.back.filter(|_| new.ty == FieldType::Link && new.opts.link_table == old.opts.link_table); }
        if new.name != old.name {
            for f in t.fields.iter().filter(|f| f.ty == FieldType::Formula && f.id != new.id) {
                let src = formula::rename_field(&f.opts.formula, &old.name, &new.name);
                if src != f.opts.formula { let mut f = f.clone(); f.opts.formula = src; ops.push(BaseOp::Table(tid, Op::SetField { field: f, cells: None })); }
            }
        }
        ops.insert(0, BaseOp::Table(tid, Op::SetField { field: new, cells }));
        self.base.next_id = next_id;
        self.run(ops)
    }

    /// Delete a field (not the primary one); a 双向关联's mirror goes with it.
    pub fn remove_field(&mut self, tid: TableId, fid: FieldId) -> bool { self.remove_fields(tid, &[fid]) }

    /// Remove several fields as one undo step; a 双向关联's other side goes with it.
    pub fn remove_fields(&mut self, tid: TableId, ids: &[FieldId]) -> bool {
        let Some(t) = self.base.table(tid) else { return false };
        let mut ops = vec![];
        for f in ids.iter().filter_map(|id| t.field(*id)) {
            ops.push(BaseOp::Table(tid, Op::RemoveField(f.id)));
            if f.ty == FieldType::Link && let (Some(other), Some(back)) = (f.opts.link_table, f.opts.back) && !(other == tid && ids.contains(&back)) { ops.push(BaseOp::Table(other, Op::RemoveField(back))); }
        }
        !ops.is_empty() && self.run(ops)
    }

    /// Replace a view's settings (筛选, 排序, 列宽…).
    pub fn set_view(&mut self, tid: TableId, view: View) -> bool { self.run_in(tid, vec![Op::SetView(view)]) }

    /// Change one view in place.
    pub fn edit_view(&mut self, tid: TableId, vid: ViewId, f: impl FnOnce(&mut View)) -> bool {
        let Some(mut v) = self.base.table(tid).and_then(|t| t.view(vid)).cloned() else { return false };
        f(&mut v);
        self.set_view(tid, v)
    }

    pub fn add_view(&mut self, tid: TableId, kind: ViewKind, name: &str) -> Option<ViewId> {
        let t = self.base.table(tid)?;
        let name = t.free_view_name(if name.is_empty() { kind.label() } else { name });
        let at = t.views.len();
        let mut opts = ViewOpts { frozen: 1, ..Default::default() };
        match kind {
            ViewKind::Kanban => opts.stack = t.fields.iter().find(|f| f.ty == FieldType::SingleSelect).map(|f| f.id),
            ViewKind::Calendar | ViewKind::Gantt => {
                let mut dates = t.fields.iter().filter(|f| f.ty == FieldType::Date).map(|f| f.id);
                opts.date = dates.next();
                opts.end = if kind == ViewKind::Gantt { dates.next() } else { None };
            }
            _ => {}
        }
        let id = self.base.new_id();
        self.run_in(tid, vec![Op::InsertView { at, view: View { id, name, kind, opts } }]).then_some(id)
    }

    pub fn duplicate_view(&mut self, tid: TableId, vid: ViewId) -> Option<ViewId> {
        let t = self.base.table(tid)?;
        let at = t.views.iter().position(|v| v.id == vid)?;
        let mut v = t.views[at].clone();
        v.name = t.free_view_name(&format!("{} 副本", v.name));
        v.id = self.base.new_id();
        let id = v.id;
        self.run_in(tid, vec![Op::InsertView { at: at + 1, view: v }]).then_some(id)
    }

    pub fn rename_view(&mut self, tid: TableId, vid: ViewId, name: &str) -> bool {
        let Some(t) = self.base.table(tid) else { return false };
        let name = name.trim();
        if name.is_empty() || t.views.iter().any(|v| v.id != vid && v.name == name) { return false; }
        let name = name.to_string();
        self.edit_view(tid, vid, |v| v.name = name)
    }

    pub fn add_table(&mut self, name: &str) -> TableId {
        let t = self.base.blank_table(if name.is_empty() { "未命名数据表" } else { name }, field::now());
        let (id, at) = (t.id, self.base.tables.len());
        self.run(vec![BaseOp::InsertTable { at, table: Box::new(t) }]);
        id
    }

    /// Delete a table (not the last); links into it turn to text of what they showed.
    pub fn remove_table(&mut self, tid: TableId) -> bool {
        if self.base.tables.len() <= 1 { return false; }
        let mut ops = vec![];
        for t in self.base.tables.iter().filter(|t| t.id != tid) {
            for f in t.fields.iter().filter(|f| f.ty == FieldType::Link && f.opts.link_table == Some(tid)) {
                let cells: Vec<(RecId, Value)> = t.records.iter().filter_map(|r| { let s = field::display_in(&self.base, f, r.get(f.id)); (!s.is_empty()).then_some((r.id, Value::Text(s))) }).collect();
                let field = Field { ty: FieldType::Text, opts: FieldOpts::default(), ..f.clone() };
                ops.push(BaseOp::Table(t.id, Op::SetField { field, cells: Some(cells) }));
            }
        }
        ops.push(BaseOp::RemoveTable(tid));
        self.run(ops)
    }

    pub fn rename_table(&mut self, tid: TableId, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || !super::sqlite_name_ok(name) || self.base.tables.iter().any(|t| t.id != tid && t.name.eq_ignore_ascii_case(name)) { return false; }
        self.run_in(tid, vec![Op::Rename(name.to_string())])
    }

    /// Paste a grid of text at (`rows`, `fields`): `rows` are the records from
    /// the anchor down in view order, `fields` the columns from the anchor
    /// right. Rows past the end are appended; options a select lacks are added.
    pub fn paste(&mut self, tid: TableId, rows: &[RecId], fields: &[FieldId], text: &[Vec<String>]) -> bool {
        let Some(t) = self.base.table(tid) else { return false };
        let mut next_id = self.base.next_id;
        let mut ops: Vec<Op> = vec![];
        let extra = text.len().saturating_sub(rows.len());
        let mut new_recs: Vec<(usize, Record)> = (0..extra).map(|i| (t.records.len() + i, self.blank_record(t, i as u64))).collect();
        let mut fields_now: HashMap<FieldId, Field> = HashMap::new();
        let mut cells = vec![];
        for (ri, line) in text.iter().enumerate() {
            for (ci, s) in line.iter().enumerate() {
                let Some(fid) = fields.get(ci) else { break };
                let Some(f) = t.field(*fid).filter(|f| !f.ty.computed()) else { continue };
                let f = fields_now.entry(*fid).or_insert_with(|| f.clone());
                let v = field::parse_new(f, s, &mut next_id);
                match rows.get(ri) {
                    Some(rid) => cells.push((*rid, *fid, v)),
                    None => new_recs[ri - rows.len()].1.set(*fid, v),
                }
            }
        }
        for (fid, f) in fields_now { if t.field(fid) != Some(&f) { ops.push(Op::SetField { field: f, cells: None }); } }
        if !new_recs.is_empty() { ops.push(Op::InsertRecords(new_recs)); }
        let mut ops: Vec<BaseOp> = ops.into_iter().map(|o| BaseOp::Table(tid, o)).collect();
        ops.extend(self.cell_ops(tid, cells));
        self.base.next_id = next_id;
        self.run(ops)
    }

    /// 填充柄: repeat the `src` rows' cells of `fields` down (or up) over `dst`.
    pub fn fill(&mut self, tid: TableId, src: &[RecId], dst: &[RecId], fields: &[FieldId]) -> bool {
        let Some(t) = self.base.table(tid) else { return false };
        if src.is_empty() { return false; }
        let mut cells = vec![];
        for (i, rid) in dst.iter().enumerate() {
            let Some(from) = t.record(src[i % src.len()]) else { continue };
            for f in fields { cells.push((*rid, *f, from.get(*f).clone())); }
        }
        self.set_cells(tid, cells)
    }
}
