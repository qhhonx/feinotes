//! Every change to a base is an `Op`. Applying one returns the ops that undo
//! it, so 撤销 / 重做 are two stacks of op groups (`Doc`), and what was
//! touched is noted in `Dirty` for the file writer (`sqlite::save`).

use super::field;
use super::model::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// Set cells of one table. Computed fields are not set this way.
    SetCells(Vec<(RecId, FieldId, Value)>),
    /// Records with the row each lands on, rows ascending.
    InsertRecords(Vec<(usize, Record)>),
    RemoveRecords(Vec<RecId>),
    /// The manual order: every record id, once.
    SetOrder(Vec<RecId>),
    InsertField { at: usize, field: Field, cells: Vec<(RecId, Value)> },
    RemoveField(FieldId),
    /// Replace a field's definition; `cells` (when given) replaces the whole column.
    SetField { field: Field, cells: Option<Vec<(RecId, Value)>> },
    MoveField { id: FieldId, to: usize },
    InsertView { at: usize, view: View },
    RemoveView(ViewId),
    SetView(View),
    MoveView { id: ViewId, to: usize },
    Rename(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum BaseOp {
    Table(TableId, Op),
    InsertTable { at: usize, table: Box<Table> },
    RemoveTable(TableId),
    MoveTable { id: TableId, to: usize },
}

/// What changed since the file was last written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dirty {
    /// Tables whose columns, order or name changed (or that are new): rewritten whole.
    pub schema: HashSet<TableId>,
    /// Rows to write again, and rows to delete.
    pub rows: HashMap<TableId, HashSet<RecId>>,
    pub removed: HashMap<TableId, HashSet<RecId>>,
    /// Row order changed.
    pub order: HashSet<TableId>,
    /// Views or the table list changed (`_views`, `_tables`).
    pub meta: bool,
    /// Tables dropped from the file, by the name they were saved under.
    pub dropped: Vec<String>,
}

impl Dirty {
    pub fn is_clean(&self) -> bool { *self == Dirty::default() }
    pub fn row(&mut self, t: TableId, r: RecId) { self.rows.entry(t).or_default().insert(r); }
}

impl Base {
    /// Apply `op`; returns the ops that undo it, in the order to apply them
    /// (empty: the op did not apply).
    pub fn apply(&mut self, op: BaseOp, dirty: &mut Dirty) -> Vec<BaseOp> {
        match op {
            BaseOp::Table(tid, op) => {
                let Some(t) = self.table_mut(tid) else { return vec![] };
                t.apply(op, dirty).into_iter().map(|o| BaseOp::Table(tid, o)).collect()
            }
            BaseOp::InsertTable { at, table } => {
                let id = table.id;
                if self.table(id).is_some() { return vec![]; }
                self.next_id = self.next_id.max(table.id).max(table.fields.iter().map(|f| f.id.max(f.opts.options.iter().map(|o| o.id).max().unwrap_or(0))).max().unwrap_or(0)).max(table.views.iter().map(|v| v.id).max().unwrap_or(0));
                self.tables.insert(at.min(self.tables.len()), *table);
                dirty.schema.insert(id);
                dirty.meta = true;
                vec![BaseOp::RemoveTable(id)]
            }
            BaseOp::RemoveTable(id) => {
                let Some(at) = self.tables.iter().position(|t| t.id == id) else { return vec![] };
                let table = self.tables.remove(at);
                dirty.dropped.push(table.name.clone());
                dirty.schema.remove(&id);
                dirty.rows.remove(&id);
                dirty.removed.remove(&id);
                dirty.meta = true;
                vec![BaseOp::InsertTable { at, table: Box::new(table) }]
            }
            BaseOp::MoveTable { id, to } => {
                let Some(from) = self.tables.iter().position(|t| t.id == id) else { return vec![] };
                let t = self.tables.remove(from);
                self.tables.insert(to.min(self.tables.len()), t);
                dirty.meta = true;
                vec![BaseOp::MoveTable { id, to: from }]
            }
        }
    }
}

impl Table {
    pub fn apply(&mut self, op: Op, dirty: &mut Dirty) -> Vec<Op> {
        let tid = self.id;
        match op {
            Op::SetCells(cells) => {
                let now = field::now();
                let index: HashMap<RecId, usize> = self.records.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
                let mut old = Vec::with_capacity(cells.len());
                for (rid, fid, v) in cells {
                    let Some(&i) = index.get(&rid) else { continue };
                    let r = &mut self.records[i];
                    let v = v.normal();
                    if *r.get(fid) == v { continue; }
                    old.push((rid, fid, r.get(fid).clone()));
                    r.set(fid, v);
                    r.modified = now;
                    dirty.row(tid, rid);
                }
                if old.is_empty() { return vec![]; }
                old.reverse();
                vec![Op::SetCells(old)]
            }
            Op::InsertRecords(recs) => {
                let mut ids = vec![];
                for (at, r) in recs {
                    if self.record(r.id).is_some() { continue; }
                    self.next_rec = self.next_rec.max(r.id + 1);
                    self.next_auto = self.next_auto.max(r.auto + 1);
                    ids.push(r.id);
                    dirty.row(tid, r.id);
                    if let Some(gone) = dirty.removed.get_mut(&tid) { gone.remove(&r.id); }
                    self.records.insert(at.min(self.records.len()), r);
                }
                if ids.is_empty() { return vec![]; }
                dirty.order.insert(tid);
                vec![Op::RemoveRecords(ids)]
            }
            Op::RemoveRecords(ids) => {
                let ids: HashSet<RecId> = ids.into_iter().collect();
                let mut gone = vec![];
                let mut i = 0;
                let mut row = 0;
                while i < self.records.len() {
                    if ids.contains(&self.records[i].id) {
                        let r = self.records.remove(i);
                        if let Some(rows) = dirty.rows.get_mut(&tid) { rows.remove(&r.id); }
                        dirty.removed.entry(tid).or_default().insert(r.id);
                        gone.push((row, r));
                    } else { i += 1; }
                    row += 1;
                }
                if gone.is_empty() { return vec![]; }
                dirty.order.insert(tid);
                vec![Op::InsertRecords(gone)]
            }
            Op::SetOrder(order) => {
                let old: Vec<RecId> = self.records.iter().map(|r| r.id).collect();
                if old == order || old.len() != order.len() { return vec![]; }
                let mut by_id: HashMap<RecId, Record> = self.records.drain(..).map(|r| (r.id, r)).collect();
                for id in &order { if let Some(r) = by_id.remove(id) { self.records.push(r); } }
                // an order that named a record twice or missed one: the rest keep their places at the end
                let mut rest: Vec<Record> = by_id.into_values().collect();
                rest.sort_by_key(|r| old.iter().position(|id| *id == r.id));
                self.records.extend(rest);
                dirty.order.insert(tid);
                vec![Op::SetOrder(old)]
            }
            Op::InsertField { at, field, cells } => {
                if self.field(field.id).is_some() { return vec![]; }
                let id = field.id;
                self.fields.insert(at.clamp(if self.fields.is_empty() { 0 } else { 1 }, self.fields.len()), field);
                let index: HashMap<RecId, usize> = self.records.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
                for (rid, v) in cells { if let Some(&i) = index.get(&rid) { self.records[i].set(id, v); } }
                dirty.schema.insert(tid);
                vec![Op::RemoveField(id)]
            }
            Op::RemoveField(id) => {
                // the primary field stays
                let Some(at) = self.fields.iter().position(|f| f.id == id).filter(|at| *at > 0) else { return vec![] };
                let field = self.fields.remove(at);
                let cells: Vec<(RecId, Value)> = self.records.iter_mut().filter_map(|r| r.cells.remove(&id).map(|v| (r.id, v))).collect();
                // views forget the field; undo puts each view back as it was
                let mut undo = vec![Op::InsertField { at, field, cells }];
                for v in &mut self.views {
                    let before = v.clone();
                    forget_field(&mut v.opts, id);
                    if *v != before { undo.push(Op::SetView(before)); dirty.meta = true; }
                }
                dirty.schema.insert(tid);
                undo
            }
            Op::SetField { field, cells } => {
                let Some(at) = self.fields.iter().position(|f| f.id == field.id) else { return vec![] };
                let id = field.id;
                let old = std::mem::replace(&mut self.fields[at], field);
                let old_cells = cells.map(|cells| {
                    let before: Vec<(RecId, Value)> = self.records.iter_mut().filter_map(|r| r.cells.remove(&id).map(|v| (r.id, v))).collect();
                    let index: HashMap<RecId, usize> = self.records.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
                    for (rid, v) in cells { if let Some(&i) = index.get(&rid) { self.records[i].set(id, v); } }
                    before
                });
                dirty.schema.insert(tid);
                vec![Op::SetField { field: old, cells: old_cells }]
            }
            Op::MoveField { id, to } => {
                let Some(from) = self.fields.iter().position(|f| f.id == id).filter(|at| *at > 0) else { return vec![] };
                let to = to.clamp(1, self.fields.len() - 1);
                if from == to { return vec![]; }
                let f = self.fields.remove(from);
                self.fields.insert(to, f);
                dirty.schema.insert(tid);
                vec![Op::MoveField { id, to: from }]
            }
            Op::InsertView { at, view } => {
                if self.view(view.id).is_some() { return vec![]; }
                let id = view.id;
                self.views.insert(at.min(self.views.len()), view);
                dirty.meta = true;
                vec![Op::RemoveView(id)]
            }
            Op::RemoveView(id) => {
                // a table keeps at least one view
                if self.views.len() <= 1 { return vec![]; }
                let Some(at) = self.views.iter().position(|v| v.id == id) else { return vec![] };
                let view = self.views.remove(at);
                dirty.meta = true;
                vec![Op::InsertView { at, view }]
            }
            Op::SetView(view) => {
                let Some(v) = self.view_mut(view.id) else { return vec![] };
                if *v == view { return vec![]; }
                let old = std::mem::replace(v, view);
                dirty.meta = true;
                vec![Op::SetView(old)]
            }
            Op::MoveView { id, to } => {
                let Some(from) = self.views.iter().position(|v| v.id == id) else { return vec![] };
                let to = to.min(self.views.len() - 1);
                if from == to { return vec![]; }
                let v = self.views.remove(from);
                self.views.insert(to, v);
                dirty.meta = true;
                vec![Op::MoveView { id, to: from }]
            }
            Op::Rename(name) => {
                if name == self.name { return vec![]; }
                let old = std::mem::replace(&mut self.name, name);
                dirty.dropped.push(old.clone());
                dirty.schema.insert(tid);
                dirty.meta = true;
                vec![Op::Rename(old)]
            }
        }
    }
}

/// Drop every mention of a removed field from a view.
fn forget_field(o: &mut ViewOpts, id: FieldId) {
    o.order.retain(|f| *f != id);
    o.hidden.remove(&id);
    o.widths.remove(&id);
    o.filter.conds.retain(|c| c.field != id);
    o.sorts.retain(|s| s.field != id);
    o.groups.retain(|s| s.field != id);
    o.stats.remove(&id);
    for rule in &mut o.colors { rule.filter.conds.retain(|c| c.field != id); }
    o.colors.retain(|r| !r.filter.conds.is_empty());
    for slot in [&mut o.stack, &mut o.cover, &mut o.date, &mut o.end] { if *slot == Some(id) { *slot = None; } }
}

/// A base being edited: the data, what the file hasn't got yet, and the
/// undo stacks. Every edit goes through `run`.
#[derive(Clone, Debug, Default)]
pub struct Doc {
    pub base: Base,
    pub dirty: Dirty,
    undo: Vec<Vec<BaseOp>>,
    redo: Vec<Vec<BaseOp>>,
    /// Bumped by every change (views cache query results against it).
    pub rev: u64,
}

const UNDO_DEPTH: usize = 200;

impl Doc {
    pub fn new(base: Base) -> Doc {
        let mut d = Doc { base, ..Default::default() };
        let mut dirty = Dirty::default();
        super::calc::recalc(&mut d.base, &mut dirty);
        d
    }

    /// Apply `ops` as one undo step. False: nothing changed.
    pub fn run(&mut self, ops: Vec<BaseOp>) -> bool {
        let undo = self.apply_all(ops);
        if undo.is_empty() { return false; }
        self.undo.push(undo);
        if self.undo.len() > UNDO_DEPTH { self.undo.remove(0); }
        self.redo.clear();
        true
    }

    /// Like `run`, for a table's ops.
    pub fn run_in(&mut self, table: TableId, ops: Vec<Op>) -> bool { self.run(ops.into_iter().map(|o| BaseOp::Table(table, o)).collect()) }

    /// `run`, joined to the last undo step (a drag that sets a value on every move).
    pub fn run_joined(&mut self, ops: Vec<BaseOp>) -> bool {
        let undo = self.apply_all(ops);
        if undo.is_empty() { return false; }
        match self.undo.last_mut() {
            Some(last) => { let mut u = undo; u.append(last); *last = u; }
            None => self.undo.push(undo),
        }
        self.redo.clear();
        true
    }

    fn apply_all(&mut self, ops: Vec<BaseOp>) -> Vec<BaseOp> {
        let mut undo = vec![];
        for op in ops {
            let mut inv = self.base.apply(op, &mut self.dirty);
            inv.append(&mut undo);
            undo = inv;
        }
        if !undo.is_empty() {
            super::calc::recalc(&mut self.base, &mut self.dirty);
            self.rev += 1;
        }
        undo
    }

    pub fn can_undo(&self) -> bool { !self.undo.is_empty() }
    pub fn can_redo(&self) -> bool { !self.redo.is_empty() }

    pub fn undo(&mut self) -> bool {
        let Some(ops) = self.undo.pop() else { return false };
        let inv = self.apply_all(ops);
        if !inv.is_empty() { self.redo.push(inv); }
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(ops) = self.redo.pop() else { return false };
        let inv = self.apply_all(ops);
        if !inv.is_empty() { self.undo.push(inv); }
        true
    }

    /// The file was read again from disk: nothing to undo into.
    pub fn replace(&mut self, base: Base) {
        *self = Doc { rev: self.rev + 1, ..Doc::new(base) };
    }
}
