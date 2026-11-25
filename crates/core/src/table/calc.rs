//! Computed columns: 公式, 查找引用, 自动编号, 创建时间, 最后更新时间. After
//! every change the lot is computed again, in dependency order across
//! tables; a column that feeds itself shows `#循环引用`.

use super::field;
use super::formula;
use super::model::*;
use super::ops::Dirty;
use std::collections::HashMap;

pub const CYCLE: &str = "#循环引用";
pub const BAD_FORMULA: &str = "#公式错误";
pub const BAD_REF: &str = "#引用失效";

type Node = (TableId, FieldId);

pub fn recalc(base: &mut Base, dirty: &mut Dirty) {
    // what the record itself knows
    for t in &mut base.tables {
        let stamps: Vec<(FieldId, FieldType)> = t.fields.iter().filter(|f| matches!(f.ty, FieldType::AutoNumber | FieldType::CreatedTime | FieldType::ModifiedTime)).map(|f| (f.id, f.ty)).collect();
        if stamps.is_empty() { continue; }
        let tid = t.id;
        for r in &mut t.records {
            for (fid, ty) in &stamps {
                let v = match ty { FieldType::AutoNumber => Value::Number(r.auto as f64), FieldType::CreatedTime => Value::Date(r.created), _ => Value::Date(r.modified) };
                if *r.get(*fid) != v { r.set(*fid, v); dirty.row(tid, r.id); }
            }
        }
    }
    // formulas and lookups, dependencies first
    let mut exprs: HashMap<Node, Result<formula::Expr, String>> = HashMap::new();
    let mut edges: HashMap<Node, Vec<Node>> = HashMap::new();
    for t in &base.tables {
        for f in &t.fields {
            let node = (t.id, f.id);
            match f.ty {
                FieldType::Formula => {
                    let e = formula::compile(&f.opts.formula, t);
                    edges.insert(node, e.as_ref().map(|e| formula::deps(e).into_iter().map(|d| (t.id, d)).collect()).unwrap_or_default());
                    exprs.insert(node, e);
                }
                FieldType::Lookup => {
                    let other = f.opts.via.and_then(|v| t.field(v)).and_then(|v| v.opts.link_table);
                    edges.insert(node, other.zip(f.opts.target).into_iter().collect());
                }
                _ => {}
            }
        }
    }
    let mut order: Vec<Node> = vec![];
    let mut state: HashMap<Node, u8> = HashMap::new();
    let mut cyclic: Vec<Node> = vec![];
    let mut nodes: Vec<Node> = edges.keys().copied().collect();
    nodes.sort();
    for n in nodes { visit(n, &edges, &mut state, &mut order, &mut cyclic); }
    let now = field::now();
    for (tid, fid) in order {
        let Some(t) = base.table(tid) else { continue };
        let Some(f) = t.field(fid) else { continue };
        let values: Vec<Value> = if cyclic.contains(&(tid, fid)) {
            vec![Value::Error(CYCLE.into()); t.records.len()]
        } else if f.ty == FieldType::Formula {
            match &exprs[&(tid, fid)] {
                Ok(e) => t.records.iter().map(|r| formula::eval(e, &formula::Ctx { base, table: t, record: r, now }).normal()).collect(),
                Err(_) if f.opts.formula.trim().is_empty() => vec![Value::Empty; t.records.len()],
                Err(_) => vec![Value::Error(BAD_FORMULA.into()); t.records.len()],
            }
        } else {
            lookup(base, t, f)
        };
        let t = base.table_mut(tid).unwrap();
        for (r, v) in t.records.iter_mut().zip(values) {
            if *r.get(fid) != v { r.set(fid, v); dirty.row(tid, r.id); }
        }
    }
}

/// Depth-first, dependencies before dependants; a node met again while open is in a cycle.
fn visit(n: Node, edges: &HashMap<Node, Vec<Node>>, state: &mut HashMap<Node, u8>, order: &mut Vec<Node>, cyclic: &mut Vec<Node>) -> bool {
    match state.get(&n) {
        Some(2) => return cyclic.contains(&n),
        Some(1) => { cyclic.push(n); return true; }
        _ => {}
    }
    let Some(deps) = edges.get(&n) else { return false };
    state.insert(n, 1);
    let mut bad = false;
    for d in deps { bad |= visit(*d, edges, state, order, cyclic); }
    if bad && !cyclic.contains(&n) { cyclic.push(n); }
    state.insert(n, 2);
    order.push(n);
    bad
}

/// A 查找引用 column: for each record, the target field's values in the linked records, folded.
fn lookup(base: &Base, t: &Table, f: &Field) -> Vec<Value> {
    let broken = || vec![Value::Error(BAD_REF.into()); t.records.len()];
    let Some(via) = f.opts.via.and_then(|v| t.field(v)).filter(|v| v.ty == FieldType::Link) else { return broken() };
    let Some(other) = via.opts.link_table.and_then(|id| base.table(id)) else { return broken() };
    let Some(target) = f.opts.target.and_then(|id| other.field(id)) else { return broken() };
    let index: HashMap<RecId, usize> = other.records.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
    t.records.iter().map(|r| {
        let Value::Links(ids) = r.get(via.id) else { return Value::Empty };
        let found: Vec<&Value> = ids.iter().filter_map(|id| index.get(id)).map(|i| other.records[*i].get(target.id)).collect();
        fold(base, target, &found, f.opts.rollup)
    }).collect()
}

/// A looked-up value in a form the lookup column can show by itself:
/// options and links become their names.
fn portable(base: &Base, target: &Field, v: &Value) -> Value {
    match v {
        Value::Select(_) | Value::Links(_) | Value::Files(_) | Value::List(_) => Value::Text(field::display_in(base, target, v)),
        Value::Text(_) if target.ty == FieldType::Note => Value::Text(field::display_in(base, target, v)),
        v => v.clone(),
    }
}

fn fold(base: &Base, target: &Field, found: &[&Value], how: Rollup) -> Value {
    let nums = || found.iter().filter_map(|v| match v { Value::Number(n) => Some(*n), Value::Date(d) => Some(*d as f64), Value::Bool(b) => Some(*b as u8 as f64), _ => None });
    let dated = found.iter().any(|v| matches!(v, Value::Date(_)));
    let num = |n: f64| if dated { Value::Date(n as i64) } else { Value::Number(n) };
    match how {
        Rollup::Values | Rollup::Unique => {
            let mut vs: Vec<Value> = found.iter().filter(|v| !v.is_empty()).map(|v| portable(base, target, v)).collect();
            if how == Rollup::Unique { let mut seen: Vec<Value> = vec![]; vs.retain(|v| if seen.contains(v) { false } else { seen.push(v.clone()); true }); }
            match vs.len() { 0 => Value::Empty, 1 => vs.remove(0), _ => Value::List(vs) }
        }
        Rollup::Sum => if found.is_empty() { Value::Empty } else { Value::Number(nums().sum()) },
        Rollup::Average => { let n = nums().count(); if n == 0 { Value::Empty } else { Value::Number(nums().sum::<f64>() / n as f64) } }
        Rollup::Min => nums().fold(None, |a: Option<f64>, b| Some(a.map_or(b, |a| a.min(b)))).map(num).unwrap_or(Value::Empty),
        Rollup::Max => nums().fold(None, |a: Option<f64>, b| Some(a.map_or(b, |a| a.max(b)))).map(num).unwrap_or(Value::Empty),
        Rollup::Count => Value::Number(found.iter().filter(|v| !v.is_empty()).count() as f64),
        Rollup::CountAll => Value::Number(found.len() as f64),
        Rollup::Concat => Value::Text(found.iter().map(|v| field::display_in(base, target, v)).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ")).normal(),
    }
}
