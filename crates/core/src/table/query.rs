//! A view's rows: 筛选, 排序, 分组, 统计, 填色. Conditions are read once
//! (`Sieve`), sort and group keys computed once per record, so a pass over
//! a large table allocates per row only what its text columns need.

use super::field;
use super::model::*;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

const DAY: i64 = 86_400_000;
/// Joins the labels of a group's path into its key; an empty label is `NONE` there.
const SEP: char = '\u{1f}';
const NONE: &str = "∅";
const CHECKED: &str = "已勾选";
const UNCHECKED: &str = "未勾选";

/// One line of a grid: a group header or a record.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Group(Group),
    /// Index into `Table::records`.
    Record(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    /// 0 for the outermost 分组 field.
    pub level: usize,
    pub field: FieldId,
    /// The group's value as shown (`display`), "" for 空.
    pub label: String,
    /// The value itself (a select's option for its colour).
    pub value: Value,
    /// Stable across edits: the path of labels from the outermost group, joined by `\u{1f}`.
    pub key: String,
    /// Records under it (through sub-groups).
    pub count: usize,
    pub folded: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewResult {
    /// What the grid lists, top to bottom; records under a folded group are left out.
    pub rows: Vec<Row>,
    /// The records that pass the filter, sorted (indexes into `Table::records`), folded or not.
    pub records: Vec<usize>,
}

pub fn run(base: &Base, t: &Table, view: &View) -> ViewResult {
    let (records, levels) = ordered(base, t, view, true);
    let mut rows = Vec::with_capacity(records.len());
    emit(&levels, 0, &records, None, &view.opts.folded, &mut rows);
    ViewResult { rows, records }
}

/// The records that pass the view's filter, in its order: by group (when
/// `grouped`), then by its sorts, then as they are in the table.
fn ordered<'a>(base: &'a Base, t: &'a Table, view: &View, grouped: bool) -> (Vec<usize>, Vec<Level<'a>>) {
    let (o, n) = (&view.opts, t.records.len());
    let sieve = Sieve::new(base, t, &o.filter, field::today());
    let mut recs: Vec<usize> = if sieve.checks.is_empty() { (0..n).collect() } else { (0..n).filter(|i| sieve.pass(&t.records[*i])).collect() };
    let levels: Vec<Level> = if grouped { o.groups.iter().take(3).filter_map(|g| Level::new(base, t, *g, &recs)).collect() } else { vec![] };
    let keys: Vec<(Vec<Key>, bool)> = o.sorts.iter().filter_map(|s| {
        let texts = Texts::new(base, t.field(s.field)?);
        let mut keys = vec![Key::Empty; n];
        for &i in &recs { keys[i] = sort_key(&texts, t.records[i].get(s.field)); }
        Some((keys, s.desc))
    }).collect();
    if !levels.is_empty() || !keys.is_empty() {
        // stable: records that tie stay in manual order
        recs.sort_by(|&a, &b| {
            for l in &levels { match l.of[a].cmp(&l.of[b]) { Ordering::Equal => {} o => return o } }
            for (k, desc) in &keys { match k[a].cmp(&k[b], *desc) { Ordering::Equal => {} o => return o } }
            Ordering::Equal
        });
    }
    (recs, levels)
}

/// The group headers and records of `recs` (sorted, so each group is a run) from `depth` down.
fn emit(levels: &[Level], depth: usize, recs: &[usize], path: Option<&str>, folded: &HashSet<String>, rows: &mut Vec<Row>) {
    let Some(l) = levels.get(depth) else { return rows.extend(recs.iter().map(|i| Row::Record(*i))) };
    let mut i = 0;
    while i < recs.len() {
        let g = l.of[recs[i]];
        let j = i + recs[i..].iter().position(|r| l.of[*r] != g).unwrap_or(recs.len() - i);
        let label = &l.labels[g as usize];
        let part = if label.is_empty() { NONE } else { label };
        let key = match path { Some(p) => format!("{p}{SEP}{part}"), None => part.to_string() };
        let shut = folded.contains(&key);
        rows.push(Row::Group(Group { level: depth, field: l.f.id, label: label.clone(), value: l.values[g as usize].clone(), key: key.clone(), count: j - i, folded: shut }));
        if !shut { emit(levels, depth + 1, &recs[i..j], Some(&key), folded, rows); }
        i = j;
    }
}

// ---- text ----

/// A field's cells as text, the way `field::display_in` shows them; a 关联
/// field's titles are looked up once, not per cell.
struct Texts<'a> {
    base: &'a Base,
    f: &'a Field,
    titles: Option<HashMap<RecId, String>>,
}

impl<'a> Texts<'a> {
    fn new(base: &'a Base, f: &'a Field) -> Self {
        let other = if f.ty == FieldType::Link { f.opts.link_table.and_then(|id| base.table(id)) } else { None };
        Texts { base, f, titles: other.map(|o| o.records.iter().map(|r| (r.id, o.title(r))).collect()) }
    }

    fn of<'v>(&self, v: &'v Value) -> Cow<'v, str> {
        match (v, &self.titles) {
            (Value::Empty, _) => Cow::Borrowed(""),
            (Value::Text(s), _) => Cow::Borrowed(s),
            (Value::Links(ids), Some(titles)) => Cow::Owned(ids.iter().map(|id| match titles.get(id) {
                Some(t) if !t.is_empty() => t.clone(),
                Some(_) => "未命名记录".into(),
                None => format!("#{id}"),
            }).collect::<Vec<_>>().join(", ")),
            _ => Cow::Owned(field::display_in(self.base, self.f, v)),
        }
    }
}

/// `s` lowercased, without a copy when it already is.
fn lower(s: &str) -> Cow<'_, str> {
    if s.bytes().any(|b| b.is_ascii_uppercase() || b >= 0x80) { Cow::Owned(s.to_lowercase()) } else { Cow::Borrowed(s) }
}

/// `s` as a key whose plain order is `field::text_cmp`'s: lowercased, each
/// digit run as a digit, its length, its digits (leading zeros dropped), and
/// the text itself after a NUL to break ties the way `text_cmp` does.
fn natural(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2 + 1);
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if !c.is_ascii_digit() { out.extend(c.to_lowercase()); continue; }
        let mut run = String::new();
        if c != '0' { run.push(c); }
        while let Some(d) = it.peek().copied().filter(char::is_ascii_digit) {
            if d != '0' || !run.is_empty() { run.push(d); }
            it.next();
        }
        // '0' so the run orders against a letter as a digit does
        out.push('0');
        out.push(char::from_u32(run.len().min(0xD000) as u32 + 1).unwrap_or('\u{1}'));
        out.push_str(&run);
    }
    out.push('\0');
    out.push_str(s);
    out
}

// ---- 筛选 ----

/// The 筛选 operators a field type offers, in 飞书's order.
pub fn ops_for(ty: FieldType) -> &'static [FilterOp] {
    use FilterOp::*;
    match ty {
        FieldType::Checkbox => &[Is],
        FieldType::Attachment => &[Empty, NotEmpty],
        // a formula's value may be text or a number
        FieldType::Formula | FieldType::Lookup => &[Is, IsNot, Contains, NotContains, Gt, Ge, Lt, Le, Empty, NotEmpty],
        t if t.is_date() => &[Is, Before, After, Empty, NotEmpty],
        t if t.numeric() => &[Is, IsNot, Gt, Ge, Lt, Le, Empty, NotEmpty],
        _ => &[Is, IsNot, Contains, NotContains, Empty, NotEmpty],
    }
}

/// A condition's value, read once.
enum Test {
    /// 为空 (true) / 不为空.
    Blank(bool),
    Check(bool),
    Num { op: FilterOp, n: f64 },
    /// Option ids; `known` is false when a name is no option (等于 can't hold then).
    Select { op: FilterOp, ids: Vec<OptId>, known: bool },
    /// `[start, end)`.
    Date { op: FilterOp, start: i64, end: i64 },
    /// By the cell's text, or by its number / day when the cell is one and the value reads as one.
    Text { op: FilterOp, needle: String, num: Option<f64>, day: Option<i64> },
}

struct Check<'a> {
    texts: Texts<'a>,
    test: Test,
}

/// A number typed into a condition; 进度 reads a bare number as a percentage, as its cells do.
fn number(f: &Field, s: &str) -> Option<f64> {
    let n = field::parse_number(s)?;
    Some(if f.ty == FieldType::Progress && !s.ends_with('%') { n / 100. } else { n })
}

/// The days a 日期 condition means, `[start, end)`. Weeks run Monday to Sunday.
fn date_range(d: DateRef, value: &str, today: i64) -> Option<(i64, i64)> {
    let week = today - (today.div_euclid(DAY) + 3).rem_euclid(7) * DAY;
    let month = |k: i64| {
        let (y, m, ..) = field::date_parts(today);
        let n = y * 12 + m as i64 - 1 + k;
        field::date_ms(n.div_euclid(12), n.rem_euclid(12) as u32 + 1, 1, 0, 0)
    };
    Some(match d {
        DateRef::Exact => { let s = field::parse_date(value)?.div_euclid(DAY) * DAY; (s, s + DAY) }
        DateRef::Today => (today, today + DAY),
        DateRef::Tomorrow => (today + DAY, today + 2 * DAY),
        DateRef::Yesterday => (today - DAY, today),
        DateRef::ThisWeek => (week, week + 7 * DAY),
        DateRef::LastWeek => (week - 7 * DAY, week),
        DateRef::NextWeek => (week + 7 * DAY, week + 14 * DAY),
        DateRef::ThisMonth => (month(0), month(1)),
        DateRef::LastMonth => (month(-1), month(0)),
        DateRef::NextMonth => (month(1), month(2)),
        DateRef::Past7 => (today - 7 * DAY, today + DAY),
        DateRef::Next7 => (today, today + 8 * DAY),
        DateRef::Past30 => (today - 30 * DAY, today + DAY),
        DateRef::Next30 => (today, today + 31 * DAY),
    })
}

/// None: the condition says nothing yet (its field is gone, or its value is
/// still to be typed), so it is left out rather than emptying the table.
fn compile<'a>(base: &'a Base, t: &'a Table, c: &Cond, today: i64) -> Option<Check<'a>> {
    use FilterOp::*;
    let f = t.field(c.field)?;
    let value = c.value.trim();
    let date = f.ty.is_date();
    let op = match c.op { Before if !date => Lt, After if !date => Gt, op => op };
    let by_text = matches!(op, Contains | NotContains);
    let test = match op {
        Empty => Test::Blank(true),
        NotEmpty => Test::Blank(false),
        _ if date && !by_text => { let (start, end) = date_range(c.date, value, today)?; Test::Date { op, start, end } }
        _ if value.is_empty() => return None,
        _ if f.ty == FieldType::Checkbox => Test::Check(field::parse(f, value).is_empty() == (op == IsNot)),
        _ if f.ty.numeric() && !by_text => Test::Num { op, n: number(f, value)? },
        _ if f.ty.select() => {
            let names = field::split_list(value);
            let mut ids: Vec<OptId> = names.iter().filter_map(|n| f.option_named(n)).map(|o| o.id).collect();
            let known = ids.len() == names.len();
            ids.sort_unstable();
            ids.dedup();
            Test::Select { op, ids, known }
        }
        _ => {
            let typed = matches!(f.ty, FieldType::Formula | FieldType::Lookup);
            let num = if typed { field::parse_number(value) } else { None };
            let day = if typed { field::parse_date(value).map(|d| d.div_euclid(DAY) * DAY) } else { None };
            if !by_text && !matches!(op, Is | IsNot) && num.is_none() && day.is_none() { return None; }
            Test::Text { op, needle: value.to_lowercase(), num, day }
        }
    };
    Some(Check { texts: Texts::new(base, f), test })
}

fn num_test(op: FilterOp, x: Option<f64>, n: f64) -> bool {
    use FilterOp::*;
    // what was typed against what was computed: 0.1 + 0.2 is 0.3
    let eq = x.is_some_and(|x| (x - n).abs() <= 1e-9 * x.abs().max(n.abs()).max(1.));
    match (op, x) {
        (Is, _) => eq,
        (IsNot, _) => !eq,
        (Gt, Some(x)) => x > n && !eq,
        (Ge, Some(x)) => x >= n || eq,
        (Lt, Some(x)) => x < n && !eq,
        (Le, Some(x)) => x <= n || eq,
        _ => false,
    }
}

fn date_test(op: FilterOp, ms: i64, start: i64, end: i64) -> bool {
    use FilterOp::*;
    match op {
        Is => start <= ms && ms < end,
        IsNot => ms < start || ms >= end,
        Before | Lt => ms < start,
        After | Gt => ms >= end,
        Ge => ms >= start,
        Le => ms < end,
        _ => false,
    }
}

impl Check<'_> {
    fn pass(&self, r: &Record) -> bool {
        use FilterOp::*;
        let v = r.get(self.texts.f.id);
        match &self.test {
            Test::Blank(want) => v.is_empty() == *want,
            Test::Check(want) => matches!(v, Value::Bool(true)) == *want,
            Test::Num { op, n } => num_test(*op, match v { Value::Number(x) => Some(*x), _ => None }, *n),
            Test::Date { op, start, end } => match v { Value::Date(ms) => date_test(*op, *ms, *start, *end), _ => *op == IsNot },
            Test::Select { op, ids, known } => {
                let cell: &[OptId] = match v { Value::Select(c) => c, _ => &[] };
                let same = || *known && cell.len() == ids.len() && ids.iter().all(|id| cell.contains(id));
                let any = || cell.iter().any(|id| ids.contains(id));
                match op { Is => same(), IsNot => !same(), Contains => any(), NotContains => !any(), _ => false }
            }
            Test::Text { op, needle, num, day } => match (v, num, day) {
                (Value::Number(x), Some(n), _) if !matches!(op, Contains | NotContains) => num_test(*op, Some(*x), *n),
                (Value::Date(ms), _, Some(d)) if !matches!(op, Contains | NotContains) => date_test(*op, *ms, *d, *d + DAY),
                _ => {
                    let s = self.texts.of(v);
                    let s = lower(&s);
                    match op { Is => s == *needle, IsNot => s != *needle, Contains => s.contains(needle.as_str()), NotContains => !s.contains(needle.as_str()), _ => false }
                }
            },
        }
    }
}

/// A 筛选, read once for a pass over the records.
struct Sieve<'a> {
    all: bool,
    checks: Vec<Check<'a>>,
}

impl<'a> Sieve<'a> {
    fn new(base: &'a Base, t: &'a Table, f: &Filter, today: i64) -> Self {
        Sieve { all: f.all, checks: f.conds.iter().filter_map(|c| compile(base, t, c, today)).collect() }
    }

    fn pass(&self, r: &Record) -> bool {
        self.checks.is_empty() || if self.all { self.checks.iter().all(|c| c.pass(r)) } else { self.checks.iter().any(|c| c.pass(r)) }
    }
}

/// Whether a record meets one condition. A condition on a field that is
/// gone, or whose value is still to be typed, holds for every record.
pub fn matches(base: &Base, t: &Table, r: &Record, c: &Cond, today: i64) -> bool {
    compile(base, t, c, today).is_none_or(|c| c.pass(r))
}

/// Whether a record passes a 筛选: every condition (`all`) or any of them.
/// Conditions that say nothing yet (see `matches`) are left out; none left → true.
pub fn passes(base: &Base, t: &Table, r: &Record, f: &Filter, today: i64) -> bool {
    Sieve::new(base, t, f, today).pass(r)
}

/// 填色: the colour of the first rule the record meets. A rule with no
/// condition that says anything colours nothing.
pub fn row_color(base: &Base, t: &Table, r: &Record, view: &View, today: i64) -> Option<u8> {
    view.opts.colors.iter().find(|rule| { let s = Sieve::new(base, t, &rule.filter, today); !s.checks.is_empty() && s.pass(r) }).map(|rule| rule.color)
}

// ---- 排序 / 分组 ----

/// A cell as the sort sees it. `field::compare`'s order: selects by option,
/// numbers and dates by value, the rest by `text_cmp` (`natural`).
#[derive(Clone, Debug, PartialEq)]
enum Key {
    Num(f64),
    Str(String),
    Empty,
}

impl Key {
    /// Empties last, ascending or not.
    fn cmp(&self, other: &Key, desc: bool) -> Ordering {
        let o = match (self, other) {
            (Key::Empty, Key::Empty) => return Ordering::Equal,
            (Key::Empty, _) => return Ordering::Greater,
            (_, Key::Empty) => return Ordering::Less,
            (Key::Num(a), Key::Num(b)) => a.total_cmp(b),
            (Key::Str(a), Key::Str(b)) => a.cmp(b),
            (Key::Num(_), Key::Str(_)) => Ordering::Less,
            (Key::Str(_), Key::Num(_)) => Ordering::Greater,
        };
        if desc { o.reverse() } else { o }
    }
}

fn sort_key(texts: &Texts, v: &Value) -> Key {
    let f = texts.f;
    // unchecked is a value here, not a blank: it leads when ascending
    if f.ty == FieldType::Checkbox { return Key::Num(matches!(v, Value::Bool(true)) as u8 as f64); }
    if v.is_empty() { return Key::Empty; }
    match v {
        // + 0.: -0 and 0 are one number
        Value::Number(n) => Key::Num(*n + 0.),
        Value::Date(d) => Key::Num(*d as f64),
        Value::Bool(_) => Key::Num(1.),
        Value::Select(ids) => match f.opts.options.iter().position(|o| o.id == ids[0]) {
            Some(i) => Key::Num(i as f64),
            None => Key::Str(natural(&texts.of(v))),
        },
        _ => Key::Str(natural(&texts.of(v))),
    }
}

/// One 分组 field: its groups in order, and which each record is in.
struct Level<'a> {
    f: &'a Field,
    labels: Vec<String>,
    values: Vec<Value>,
    /// By record index: the record's group (meaningless for records the filter dropped).
    of: Vec<u32>,
}

impl<'a> Level<'a> {
    fn new(base: &'a Base, t: &'a Table, by: SortBy, recs: &[usize]) -> Option<Self> {
        let f = t.field(by.field)?;
        let texts = Texts::new(base, f);
        let mut index: HashMap<String, u32> = HashMap::new();
        let (mut labels, mut values, mut keys): (Vec<String>, Vec<Value>, Vec<Key>) = (vec![], vec![], vec![]);
        let mut of = vec![0u32; t.records.len()];
        for &i in recs {
            let v = t.records[i].get(f.id);
            // 复选框: two groups; 日期: one a day; the rest: one per text shown (a 多选's whole set)
            let label = match v {
                _ if f.ty == FieldType::Checkbox => if matches!(v, Value::Bool(true)) { CHECKED } else { UNCHECKED }.to_string(),
                _ if v.is_empty() => String::new(),
                Value::Date(ms) => field::format_date(ms.div_euclid(DAY) * DAY, f.opts.date, false),
                _ => texts.of(v).into_owned(),
            };
            of[i] = match index.get(&label) {
                Some(g) => *g,
                None => {
                    let value = match v {
                        _ if f.ty == FieldType::Checkbox => Value::Bool(matches!(v, Value::Bool(true))),
                        _ if label.is_empty() => Value::Empty,
                        Value::Date(ms) => Value::Date(ms.div_euclid(DAY) * DAY),
                        _ => v.clone(),
                    };
                    keys.push(sort_key(&texts, &value));
                    values.push(value);
                    labels.push(label.clone());
                    index.insert(label, labels.len() as u32 - 1);
                    labels.len() as u32 - 1
                }
            };
        }
        // groups whose values order the same (多选 sets with one first option) go by their text
        let mut order: Vec<usize> = (0..labels.len()).collect();
        order.sort_by(|&a, &b| keys[a].cmp(&keys[b], by.desc).then_with(|| { let o = field::text_cmp(&labels[a], &labels[b]); if by.desc { o.reverse() } else { o } }));
        let mut rank = vec![0u32; order.len()];
        for (r, g) in order.iter().enumerate() { rank[*g] = r as u32; }
        for &i in recs { of[i] = rank[of[i] as usize]; }
        let labels = order.iter().map(|g| std::mem::take(&mut labels[*g])).collect();
        let values = order.iter().map(|g| std::mem::take(&mut values[*g])).collect();
        Some(Level { f, labels, values, of })
    }
}

/// 看板: the records (filtered and sorted) under each option of the view's
/// `stack` field, by a cell's first option; 未分类 (None) leads, then the
/// options in their order. No such field: everything is 未分类.
pub fn kanban(base: &Base, t: &Table, view: &View) -> Vec<(Option<OptId>, Vec<usize>)> {
    let (records, _) = ordered(base, t, view, false);
    let Some(f) = view.opts.stack.and_then(|id| t.field(id)) else { return vec![(None, records)] };
    let mut cols: Vec<(Option<OptId>, Vec<usize>)> = std::iter::once((None, vec![])).chain(f.opts.options.iter().map(|o| (Some(o.id), vec![]))).collect();
    let at: HashMap<OptId, usize> = f.opts.options.iter().enumerate().map(|(i, o)| (o.id, i + 1)).collect();
    for i in records {
        let col = match t.records[i].get(f.id) { Value::Select(ids) => ids.first().and_then(|id| at.get(id)).copied().unwrap_or(0), _ => 0 };
        cols[col].1.push(i);
    }
    cols
}

/// What a record made in this view starts with, so it shows where it was
/// made: the values the filter's 等于 conditions ask for (when all must
/// hold), and the values of the group it was added under (and that group's parents).
pub fn defaults_for(t: &Table, view: &View, group: Option<&Group>) -> Vec<(FieldId, Value)> {
    fn put(out: &mut Vec<(FieldId, Value)>, f: &Field, v: Value) {
        if f.ty.computed() || v.is_empty() { return; }
        out.retain(|(id, _)| *id != f.id);
        out.push((f.id, v));
    }
    let mut out = vec![];
    let filter = &view.opts.filter;
    if filter.all || filter.conds.len() == 1 {
        for c in &filter.conds {
            let Some(f) = t.field(c.field) else { continue };
            let v = match c.op {
                FilterOp::Is if f.ty.is_date() => match c.date {
                    DateRef::Exact => field::parse(f, &c.value),
                    DateRef::Today | DateRef::Tomorrow | DateRef::Yesterday => date_range(c.date, "", field::today()).map(|(s, _)| Value::Date(s)).unwrap_or_default(),
                    _ => continue,
                },
                FilterOp::Is => field::parse(f, &c.value),
                // 包含 any of these: the first will do for a 单选
                FilterOp::Contains if f.ty.select() => {
                    let names = field::split_list(c.value.trim());
                    let ids = names.iter().filter_map(|n| f.option_named(n)).map(|o| o.id);
                    Value::Select(if f.ty == FieldType::SingleSelect { ids.take(1).collect() } else { ids.collect() })
                }
                _ => continue,
            };
            put(&mut out, f, v);
        }
    }
    if let Some(g) = group {
        // the parents' values are only in the key, as their labels
        let levels = view.opts.groups.iter().take(3).filter_map(|s| t.field(s.field));
        for (f, label) in levels.zip(g.key.split(SEP)).take(g.level) {
            let v = if f.ty == FieldType::Checkbox { Value::Bool(label == CHECKED) } else if label == NONE { Value::Empty } else { field::parse(f, label) };
            put(&mut out, f, v);
        }
        if let Some(f) = t.field(g.field) { put(&mut out, f, g.value.clone()); }
    }
    out
}

/// 查找: the cells of `records` × `fields` whose text holds `needle`
/// (case-insensitive), row by row, each row in `fields` order.
pub fn find(base: &Base, t: &Table, records: &[usize], fields: &[FieldId], needle: &str) -> Vec<(usize, FieldId)> {
    let needle = needle.to_lowercase();
    if needle.is_empty() { return vec![]; }
    let cols: Vec<Texts> = fields.iter().filter_map(|id| t.field(*id)).map(|f| Texts::new(base, f)).collect();
    let mut out = vec![];
    for &i in records {
        let Some(r) = t.records.get(i) else { continue };
        for c in &cols {
            let v = r.get(c.f.id);
            if !v.is_empty() && lower(&c.of(v)).contains(&needle) { out.push((i, c.f.id)); }
        }
    }
    out
}

// ---- 统计 ----

/// The 统计 a field type offers, in 飞书's order.
pub fn stats_for(ty: FieldType) -> &'static [Stat] {
    use Stat::*;
    match ty {
        FieldType::Checkbox => &[Count, Checked, Unchecked],
        FieldType::Formula | FieldType::Lookup => &[Count, Filled, Empty, Unique, PercentFilled, PercentEmpty, Sum, Average, Min, Max],
        t if t.numeric() => &[Count, Filled, Empty, Unique, PercentFilled, PercentEmpty, Sum, Average, Min, Max],
        t if t.is_date() => &[Count, Filled, Empty, Unique, PercentFilled, PercentEmpty, Earliest, Latest],
        _ => &[Count, Filled, Empty, Unique, PercentFilled, PercentEmpty],
    }
}

/// `75%`, `33.33%`; nothing of nothing is "".
fn percent(n: usize, of: usize) -> String {
    if of == 0 { return String::new(); }
    let s = format!("{:.2}", n as f64 * 100. / of as f64);
    format!("{}%", s.trim_end_matches('0').trim_end_matches('.'))
}

/// A column's 统计 over `records` (indexes into `Table::records`), as the
/// footer shows it. Nothing to work on gives "" (求和 gives the field's zero).
pub fn stat(base: &Base, t: &Table, f: &Field, records: &[usize], s: Stat) -> String {
    let cells = || records.iter().filter_map(|i| t.records.get(*i)).map(|r| r.get(f.id));
    let total = cells().count();
    let filled = || cells().filter(|v| !v.is_empty()).count();
    // a 查找引用's list counts each number in it
    fn numbers(v: &Value, out: &mut Vec<f64>) {
        match v { Value::Number(n) => out.push(*n), Value::List(vs) => vs.iter().for_each(|v| numbers(v, out)), _ => {} }
    }
    let nums = || { let mut out = vec![]; cells().for_each(|v| numbers(v, &mut out)); out };
    let show = |n: f64| if f.ty == FieldType::AutoNumber { field::plain_number(n) } else { field::format_number(n, f) };
    let date = |d: Option<i64>| d.map(|ms| field::format_date(ms, f.opts.date, f.opts.time)).unwrap_or_default();
    let dates = || cells().filter_map(|v| match v { Value::Date(d) => Some(*d), _ => None });
    match s {
        Stat::Count => total.to_string(),
        Stat::Filled => filled().to_string(),
        Stat::Empty => (total - filled()).to_string(),
        Stat::Unique => {
            let texts = Texts::new(base, f);
            cells().filter(|v| !v.is_empty()).map(|v| texts.of(v)).collect::<HashSet<_>>().len().to_string()
        }
        Stat::PercentFilled => percent(filled(), total),
        Stat::PercentEmpty => percent(total - filled(), total),
        // + 0.: an empty sum is -0
        Stat::Sum => show(nums().iter().sum::<f64>() + 0.),
        Stat::Average => {
            let ns = nums();
            if ns.is_empty() { return String::new(); }
            let avg = ns.iter().sum::<f64>() / ns.len() as f64;
            // no precision set: two digits at most, of what shows (a percentage's are two further down)
            let percent = f.opts.style == NumberStyle::Percent && !matches!(f.ty, FieldType::Currency | FieldType::Progress | FieldType::AutoNumber);
            match f.opts.precision {
                None if percent => format!("{}%", field::plain_number((avg * 1e4).round() / 1e2)),
                None => show((avg * 1e2).round() / 1e2),
                Some(_) => show(avg),
            }
        }
        Stat::Min => nums().into_iter().reduce(f64::min).map(show).unwrap_or_default(),
        Stat::Max => nums().into_iter().reduce(f64::max).map(show).unwrap_or_default(),
        Stat::Earliest => date(dates().min()),
        Stat::Latest => date(dates().max()),
        Stat::Checked => cells().filter(|v| matches!(v, Value::Bool(true))).count().to_string(),
        Stat::Unchecked => cells().filter(|v| !matches!(v, Value::Bool(true))).count().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use FilterOp::*;

    const TEXT: FieldId = 1;
    const STATUS: FieldId = 2;
    const TAGS: FieldId = 3;
    const OWNER: FieldId = 4;
    const DUE: FieldId = 5;
    const QTY: FieldId = 6;
    const DONE: FieldId = 7;
    const PRICE: FieldId = 8;
    const PROGRESS: FieldId = 9;
    const STARS: FieldId = 10;
    const CALC: FieldId = 11;
    const FILES: FieldId = 12;
    const LINK: FieldId = 13;
    const URL: FieldId = 14;
    const AUTO: FieldId = 15;
    const CREATED: FieldId = 16;
    const LOOKUP: FieldId = 17;
    const PHONE: FieldId = 18;
    const EMAIL: FieldId = 19;
    const PLACE: FieldId = 20;
    const CODE: FieldId = 21;
    const EDITED: FieldId = 22;
    // options
    const TODO: OptId = 31;
    const DOING: OptId = 32;
    const FINISHED: OptId = 33;
    const RED: OptId = 41;
    const BLUE: OptId = 42;
    const GREEN: OptId = 43;
    const ZHANG: OptId = 51;
    const LI: OptId = 52;

    fn fld(id: FieldId, name: &str, ty: FieldType, options: &[(OptId, &str)]) -> Field {
        let mut opts = field::default_opts(ty);
        opts.options = options.iter().map(|(id, n)| SelectOption { id: *id, name: n.to_string(), color: 0 }).collect();
        Field { id, name: name.into(), ty, opts, desc: String::new() }
    }

    /// A table with a field of every type and these records (ids 1…), plus the table its 关联 points at.
    fn base(rows: Vec<Vec<(FieldId, Value)>>) -> Base {
        use FieldType::*;
        let mut t = Table { id: 1, name: "任务".into(), ..Default::default() };
        t.fields = vec![
            fld(TEXT, "名称", Text, &[]), fld(STATUS, "状态", SingleSelect, &[(TODO, "待办"), (DOING, "进行中"), (FINISHED, "完成")]),
            fld(TAGS, "标签", MultiSelect, &[(RED, "红"), (BLUE, "蓝"), (GREEN, "绿")]), fld(OWNER, "负责人", Person, &[(ZHANG, "张三"), (LI, "李四")]),
            fld(DUE, "截止", Date, &[]), fld(QTY, "数量", Number, &[]), fld(DONE, "完成", Checkbox, &[]), fld(PRICE, "金额", Currency, &[]),
            fld(PROGRESS, "进度", Progress, &[]), fld(STARS, "评分", Rating, &[]), fld(CALC, "公式", Formula, &[]), fld(FILES, "附件", Attachment, &[]),
            fld(LINK, "关联", Link, &[]), fld(URL, "链接", Url, &[]), fld(AUTO, "编号", AutoNumber, &[]), fld(CREATED, "创建时间", CreatedTime, &[]),
            fld(LOOKUP, "引用", Lookup, &[]), fld(PHONE, "电话", Phone, &[]), fld(EMAIL, "邮箱", Email, &[]), fld(PLACE, "地点", Location, &[]),
            fld(CODE, "条码", Barcode, &[]), fld(EDITED, "更新时间", ModifiedTime, &[]),
        ];
        t.field_mut(LINK).unwrap().opts.link_table = Some(2);
        for (i, cells) in rows.into_iter().enumerate() {
            let mut r = Record { id: i as u64 + 1, ..Default::default() };
            for (f, v) in cells { r.set(f, v); }
            t.records.push(r);
        }
        let mut other = Table { id: 2, name: "客户".into(), fields: vec![fld(1, "名称", Text, &[])], ..Default::default() };
        for (id, name) in [(1, "甲公司"), (2, "乙公司"), (3, "")] {
            let mut r = Record { id, ..Default::default() };
            r.set(1, s(name));
            other.records.push(r);
        }
        Base { tables: vec![t, other], next_id: 100 }
    }

    fn s(t: &str) -> Value { Value::Text(t.into()) }
    fn n(x: f64) -> Value { Value::Number(x) }
    fn sel(ids: &[OptId]) -> Value { Value::Select(ids.to_vec()) }
    fn ms(y: i64, m: u32, d: u32) -> i64 { field::date_ms(y, m, d, 0, 0) }
    fn day(y: i64, m: u32, d: u32) -> Value { Value::Date(ms(y, m, d)) }
    fn file(name: &str) -> Value { Value::Files(vec![Attachment { path: format!(".assets/{name}"), name: name.into() }]) }
    /// A Tuesday.
    fn today() -> i64 { ms(2026, 11, 17) }
    fn cond(field: FieldId, op: FilterOp, value: &str) -> Cond { Cond { field, op, value: value.into(), date: DateRef::Exact } }
    fn view(set: impl FnOnce(&mut ViewOpts)) -> View {
        let mut v = View { id: 9, name: "表格".into(), ..Default::default() };
        set(&mut v.opts);
        v
    }
    fn asc(field: FieldId) -> SortBy { SortBy { field, desc: false } }
    fn desc(field: FieldId) -> SortBy { SortBy { field, desc: true } }

    /// Whether a record whose `f` cell is `v` meets `f op value`.
    fn hit(f: FieldId, v: Value, op: FilterOp, value: &str) -> bool {
        let b = base(vec![vec![(f, v)]]);
        matches(&b, &b.tables[0], &b.tables[0].records[0], &cond(f, op, value), today())
    }

    /// The rows in short: `[label:count]` (`-` when folded) for a header, the index for a record.
    fn brief(r: &ViewResult) -> String {
        r.rows.iter().map(|row| match row {
            Row::Group(g) => format!("{}[{}:{}]{}", "·".repeat(g.level), g.label, g.count, if g.folded { "-" } else { "" }),
            Row::Record(i) => i.to_string(),
        }).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn operators_by_type() {
        use FieldType::*;
        for ty in [Text, Url, Phone, Email, Location, Barcode, Link, SingleSelect, MultiSelect, Person] {
            assert_eq!(ops_for(ty), &[Is, IsNot, Contains, NotContains, Empty, NotEmpty], "{ty:?}");
        }
        for ty in [Number, Currency, Progress, Rating, AutoNumber] { assert_eq!(ops_for(ty), &[Is, IsNot, Gt, Ge, Lt, Le, Empty, NotEmpty], "{ty:?}"); }
        for ty in [Date, CreatedTime, ModifiedTime] { assert_eq!(ops_for(ty), &[Is, Before, After, Empty, NotEmpty], "{ty:?}"); }
        for ty in [Formula, Lookup] { assert_eq!(ops_for(ty), &[Is, IsNot, Contains, NotContains, Gt, Ge, Lt, Le, Empty, NotEmpty], "{ty:?}"); }
        assert_eq!(ops_for(Checkbox), &[Is]);
        assert_eq!(ops_for(Attachment), &[Empty, NotEmpty]);
        // every type answers, and every operator it offers is one `matches` knows
        for ty in FieldType::ALL { assert!(!ops_for(*ty).is_empty()); }
    }

    #[test]
    fn text_conditions() {
        for f in [TEXT, URL, PHONE, EMAIL, PLACE, CODE] {
            assert!(hit(f, s("Hello World"), Is, "hello world"), "{f}");
            assert!(!hit(f, s("Hello World"), Is, "hello"));
            assert!(hit(f, s("Hello World"), IsNot, "hello"));
            assert!(!hit(f, s("Hello World"), IsNot, "HELLO WORLD"));
            assert!(hit(f, s("Hello World"), Contains, "LO WO"));
            assert!(!hit(f, s("Hello World"), Contains, "xyz"));
            assert!(hit(f, s("Hello World"), NotContains, "xyz"));
            assert!(!hit(f, s("Hello World"), NotContains, "world"));
            assert!(hit(f, Value::Empty, Empty, ""));
            assert!(!hit(f, s("x"), Empty, ""));
            assert!(hit(f, s("x"), NotEmpty, ""));
            assert!(!hit(f, Value::Empty, NotEmpty, ""));
            // a blank cell is not what was asked for, and does not contain it
            assert!(!hit(f, Value::Empty, Is, "x"));
            assert!(hit(f, Value::Empty, IsNot, "x"));
            assert!(!hit(f, Value::Empty, Contains, "x"));
            assert!(hit(f, Value::Empty, NotContains, "x"));
        }
        assert!(hit(TEXT, s("中文 Ünï"), Contains, "ÜNÏ"));
        // text stays text: no numeric reading outside 公式 / 查找引用
        assert!(!hit(TEXT, s("1,000"), Is, "1000"));
    }

    #[test]
    fn link_and_attachment_conditions() {
        let links = Value::Links(vec![1, 2]);
        assert!(hit(LINK, links.clone(), Is, "甲公司, 乙公司"));
        assert!(hit(LINK, links.clone(), IsNot, "甲公司"));
        assert!(hit(LINK, links.clone(), Contains, "乙"));
        assert!(hit(LINK, links.clone(), NotContains, "丙"));
        assert!(hit(LINK, Value::Links(vec![3]), Is, "未命名记录"));
        assert!(hit(LINK, Value::Links(vec![9]), Contains, "#9"));
        assert!(hit(LINK, Value::Empty, Empty, "") && !hit(LINK, links.clone(), Empty, ""));
        assert!(hit(LINK, links, NotEmpty, "") && !hit(LINK, Value::Empty, NotEmpty, ""));
        assert!(hit(FILES, Value::Empty, Empty, "") && !hit(FILES, file("a.png"), Empty, ""));
        assert!(hit(FILES, file("a.png"), NotEmpty, "") && !hit(FILES, Value::Empty, NotEmpty, ""));
        // by file name
        assert!(hit(FILES, file("Report.PDF"), Contains, "report"));
        assert!(hit(FILES, file("a.png"), Is, "A.PNG") && hit(FILES, file("a.png"), IsNot, "b.png") && hit(FILES, file("a.png"), NotContains, "pdf"));
    }

    #[test]
    fn number_conditions() {
        for f in [QTY, PRICE, STARS, AUTO] {
            assert!(hit(f, n(3.), Is, "3") && !hit(f, n(3.), Is, "4"), "{f}");
            assert!(hit(f, n(3.), IsNot, "4") && !hit(f, n(3.), IsNot, "3"));
            assert!(hit(f, n(3.), Gt, "2") && !hit(f, n(3.), Gt, "3"));
            assert!(hit(f, n(3.), Ge, "3") && !hit(f, n(3.), Ge, "4"));
            assert!(hit(f, n(3.), Lt, "4") && !hit(f, n(3.), Lt, "3"));
            assert!(hit(f, n(3.), Le, "3") && !hit(f, n(3.), Le, "2"));
            assert!(hit(f, Value::Empty, Empty, "") && !hit(f, n(0.), Empty, ""));
            assert!(hit(f, n(0.), NotEmpty, "") && !hit(f, Value::Empty, NotEmpty, ""));
            // a blank is no number: only 不等于 holds
            assert!(hit(f, Value::Empty, IsNot, "3"));
            for op in [Is, Gt, Ge, Lt, Le] { assert!(!hit(f, Value::Empty, op, "3"), "{f} {op:?}"); }
        }
        assert!(hit(PRICE, n(1234.5), Is, "¥1,234.50"));
        assert!(hit(QTY, n(0.1 + 0.2), Is, "0.3") && !hit(QTY, n(0.1 + 0.2), Gt, "0.3") && hit(QTY, n(0.1 + 0.2), Le, "0.3"));
        assert!(hit(QTY, n(-2.), Lt, "-1"));
        // 进度: a bare number is a percentage, as in its cells
        assert!(hit(PROGRESS, n(0.5), Is, "50") && hit(PROGRESS, n(0.5), Is, "50%") && !hit(PROGRESS, n(0.5), Is, "0.5"));
        assert!(hit(PROGRESS, n(0.5), IsNot, "40") && hit(PROGRESS, n(0.5), Gt, "40") && hit(PROGRESS, n(0.5), Ge, "50"));
        assert!(hit(PROGRESS, n(0.5), Lt, "60%") && hit(PROGRESS, n(0.5), Le, "50") && !hit(PROGRESS, n(0.5), Lt, "50"));
        assert!(hit(PROGRESS, Value::Empty, Empty, "") && hit(PROGRESS, n(0.), NotEmpty, ""));
    }

    #[test]
    fn select_conditions() {
        assert!(hit(STATUS, sel(&[TODO]), Is, "待办") && !hit(STATUS, sel(&[TODO]), Is, "完成"));
        assert!(hit(STATUS, sel(&[TODO]), IsNot, "完成") && !hit(STATUS, sel(&[TODO]), IsNot, "待办"));
        assert!(hit(STATUS, sel(&[TODO]), Contains, "完成, 待办") && !hit(STATUS, sel(&[TODO]), Contains, "完成,进行中"));
        assert!(hit(STATUS, sel(&[TODO]), NotContains, "完成,进行中") && !hit(STATUS, sel(&[TODO]), NotContains, "待办"));
        assert!(hit(STATUS, Value::Empty, Empty, "") && !hit(STATUS, sel(&[TODO]), Empty, ""));
        assert!(hit(STATUS, sel(&[TODO]), NotEmpty, "") && !hit(STATUS, Value::Empty, NotEmpty, ""));
        assert!(!hit(STATUS, Value::Empty, Is, "待办") && hit(STATUS, Value::Empty, IsNot, "待办"));
        assert!(!hit(STATUS, Value::Empty, Contains, "待办") && hit(STATUS, Value::Empty, NotContains, "待办"));
        // a name that is no option matches nothing
        assert!(!hit(STATUS, sel(&[TODO]), Is, "没有") && !hit(STATUS, Value::Empty, Is, "没有") && hit(STATUS, sel(&[TODO]), IsNot, "没有"));
        assert!(!hit(STATUS, sel(&[TODO]), Contains, "没有") && hit(STATUS, sel(&[TODO]), Contains, "没有, 待办"));
        for (f, a, b, c, an, bn, cn) in [(TAGS, RED, BLUE, GREEN, "红", "蓝", "绿"), (OWNER, ZHANG, LI, 99, "张三", "李四", "王五")] {
            let both = format!("{bn}，{an}");
            // 等于: the same set, in any order
            assert!(hit(f, sel(&[a, b]), Is, &both) && !hit(f, sel(&[a, b]), Is, an) && !hit(f, sel(&[a]), Is, &both), "{f}");
            assert!(hit(f, sel(&[a, b]), IsNot, an) && !hit(f, sel(&[a, b]), IsNot, &both));
            assert!(hit(f, sel(&[a]), Contains, &both) && !hit(f, sel(&[c]), Contains, &both));
            assert!(hit(f, sel(&[c]), NotContains, &both) && !hit(f, sel(&[b, c]), NotContains, &both));
            assert!(hit(f, Value::Empty, Empty, "") && !hit(f, sel(&[a]), Empty, ""));
            assert!(hit(f, sel(&[a]), NotEmpty, "") && !hit(f, Value::Empty, NotEmpty, ""));
            assert!(!hit(f, sel(&[a]), Is, &format!("{an},{cn},没有")));
        }
    }

    #[test]
    fn date_conditions() {
        for f in [DUE, CREATED, EDITED] {
            // 等于: any time that day
            assert!(hit(f, Value::Date(field::date_ms(2026, 11, 17, 9, 30)), Is, "2026-11-17"), "{f}");
            assert!(hit(f, day(2026, 11, 17), Is, "2026/11/17 08:00") && !hit(f, day(2026, 11, 18), Is, "2026-11-17"));
            assert!(hit(f, day(2026, 11, 16), Before, "2026-11-17") && !hit(f, day(2026, 11, 17), Before, "2026-11-17"));
            assert!(hit(f, day(2026, 11, 18), After, "2026-11-17") && !hit(f, Value::Date(field::date_ms(2026, 11, 17, 23, 59)), After, "2026-11-17"));
            assert!(hit(f, Value::Empty, Empty, "") && !hit(f, day(2026, 1, 1), Empty, ""));
            assert!(hit(f, day(2026, 1, 1), NotEmpty, "") && !hit(f, Value::Empty, NotEmpty, ""));
            for op in [Is, Before, After] { assert!(!hit(f, Value::Empty, op, "2026-11-17"), "{f} {op:?}"); }
        }
    }

    #[test]
    fn relative_dates() {
        use DateRef::*;
        let b = base(vec![vec![]]);
        let t = &b.tables[0];
        let on = |date: DateRef, op: FilterOp, d: i64| {
            let mut r = Record::default();
            r.set(DUE, Value::Date(d));
            matches(&b, t, &r, &Cond { field: DUE, op, value: String::new(), date }, today())
        };
        // [first day, last day] of each, around Tuesday 2026-11-17
        let spans = [
            (Today, ms(2026, 11, 17), ms(2026, 11, 17)), (Tomorrow, ms(2026, 11, 18), ms(2026, 11, 18)), (Yesterday, ms(2026, 11, 16), ms(2026, 11, 16)),
            (ThisWeek, ms(2026, 11, 16), ms(2026, 11, 22)), (LastWeek, ms(2026, 11, 9), ms(2026, 11, 15)), (NextWeek, ms(2026, 11, 23), ms(2026, 11, 29)),
            (ThisMonth, ms(2026, 11, 1), ms(2026, 11, 30)), (LastMonth, ms(2026, 10, 1), ms(2026, 10, 31)), (NextMonth, ms(2026, 12, 1), ms(2026, 12, 31)),
            (Past7, ms(2026, 11, 10), ms(2026, 11, 17)), (Next7, ms(2026, 11, 17), ms(2026, 11, 24)),
            (Past30, ms(2026, 10, 18), ms(2026, 11, 17)), (Next30, ms(2026, 11, 17), ms(2026, 12, 17)),
        ];
        assert_eq!(spans.len() + 1, DateRef::ALL.len());
        for (date, first, last) in spans {
            let late = last + DAY - 60_000;
            assert!(on(date, Is, first) && on(date, Is, late), "{date:?}");
            assert!(!on(date, Is, first - 60_000) && !on(date, Is, last + DAY), "{date:?}");
            assert!(on(date, Before, first - 60_000) && !on(date, Before, first) && !on(date, Before, late), "{date:?}");
            assert!(on(date, After, last + DAY) && !on(date, After, late) && !on(date, After, first), "{date:?}");
        }
        // months wrap the year
        let jan = ms(2027, 1, 15);
        let at = |date: DateRef, d: i64, today: i64| { let mut r = Record::default(); r.set(DUE, Value::Date(d)); matches(&b, t, &r, &Cond { field: DUE, op: Is, value: String::new(), date }, today) };
        assert!(at(LastMonth, ms(2026, 12, 31), jan) && at(NextMonth, ms(2027, 2, 28), jan) && !at(NextMonth, ms(2027, 3, 1), jan));
        assert!(at(NextMonth, ms(2027, 1, 1), ms(2026, 12, 5)));
        // a Monday and a Sunday are in their own week
        assert!(at(ThisWeek, ms(2026, 11, 22), ms(2026, 11, 16)) && at(ThisWeek, ms(2026, 11, 16), ms(2026, 11, 22)) && !at(ThisWeek, ms(2026, 11, 23), ms(2026, 11, 22)));
    }

    #[test]
    fn checkbox_conditions() {
        assert!(hit(DONE, Value::Bool(true), Is, "true") && !hit(DONE, Value::Empty, Is, "true"));
        assert!(hit(DONE, Value::Empty, Is, "false") && hit(DONE, Value::Bool(false), Is, "false") && !hit(DONE, Value::Bool(true), Is, "false"));
        assert!(hit(DONE, Value::Empty, IsNot, "true") && !hit(DONE, Value::Bool(true), IsNot, "true"));
    }

    #[test]
    fn formula_conditions_follow_the_value() {
        for f in [CALC, LOOKUP] {
            // a number
            assert!(hit(f, n(1200.), Is, "1,200") && hit(f, n(1200.), IsNot, "1") && !hit(f, n(1200.), IsNot, "1200"), "{f}");
            assert!(hit(f, n(10.), Gt, "9") && !hit(f, n(10.), Gt, "10") && hit(f, n(10.), Ge, "10"));
            assert!(hit(f, n(10.), Lt, "11") && !hit(f, n(10.), Lt, "10") && hit(f, n(10.), Le, "10"));
            assert!(hit(f, n(10.), Contains, "1") && hit(f, n(10.), NotContains, "2"));
            // text
            assert!(hit(f, s("Abc"), Is, "abc") && hit(f, s("Abc"), IsNot, "ab") && hit(f, s("Abc"), Contains, "BC") && hit(f, s("Abc"), NotContains, "x"));
            assert!(!hit(f, s("Abc"), Gt, "1") && !hit(f, s("N/A"), Lt, "5") && !hit(f, Value::Empty, Gt, "1"));
            // a date, by the day
            assert!(hit(f, Value::Date(field::date_ms(2026, 11, 17, 9, 0)), Is, "2026-11-17") && hit(f, day(2026, 11, 17), Gt, "2026-11-16"));
            assert!(hit(f, day(2026, 11, 17), Le, "2026-11-17") && !hit(f, day(2026, 11, 17), Lt, "2026-11-17"));
            assert!(hit(f, Value::Empty, Empty, "") && hit(f, n(0.), NotEmpty, "") && hit(f, Value::Error("#ERROR".into()), NotEmpty, ""));
            assert!(hit(f, Value::List(vec![s("a"), s("b")]), Is, "a, b") && hit(f, Value::List(vec![n(1.), n(2.)]), Contains, "2"));
        }
    }

    #[test]
    fn unfinished_conditions_hold() {
        // a value still to be typed, one that does not read, a field that is gone
        for (f, op, value) in [(TEXT, Is, ""), (TEXT, Contains, "  "), (QTY, Gt, ""), (QTY, Gt, "abc"), (STATUS, Is, ""), (TAGS, Contains, ""), (DUE, Is, ""),
            (DUE, Before, "soon"), (DONE, Is, ""), (CALC, Gt, "abc"), (999, Is, "x"), (999, Empty, "")] {
            assert!(hit(f, Value::Empty, op, value), "{f} {op:?} {value:?}");
        }
        // a relative day needs no value
        let b = base(vec![vec![(DUE, day(2026, 11, 17))], vec![(DUE, day(2026, 11, 20))]]);
        let t = &b.tables[0];
        let c = Cond { field: DUE, op: Is, value: String::new(), date: DateRef::Today };
        assert!(matches(&b, t, &t.records[0], &c, today()) && !matches(&b, t, &t.records[1], &c, today()));
    }

    #[test]
    fn all_or_any() {
        let b = base(vec![vec![(TEXT, s("apple")), (QTY, n(5.))], vec![(TEXT, s("banana")), (QTY, n(1.))], vec![(TEXT, s("cherry")), (QTY, n(9.))], vec![]]);
        let t = &b.tables[0];
        let pass = |all: bool, conds: Vec<Cond>| -> Vec<usize> {
            let f = Filter { all, conds };
            (0..t.records.len()).filter(|i| passes(&b, t, &t.records[*i], &f, today())).collect()
        };
        assert_eq!(pass(true, vec![]), [0, 1, 2, 3]);
        assert_eq!(pass(false, vec![]), [0, 1, 2, 3]);
        assert_eq!(pass(true, vec![cond(TEXT, Contains, "a"), cond(QTY, Gt, "2")]), [0]);
        assert_eq!(pass(false, vec![cond(TEXT, Contains, "a"), cond(QTY, Gt, "2")]), [0, 1, 2]);
        // an unfinished condition is left out, 且 or 或
        assert_eq!(pass(true, vec![cond(TEXT, Contains, ""), cond(QTY, Gt, "2")]), [0, 2]);
        assert_eq!(pass(false, vec![cond(TEXT, Contains, ""), cond(QTY, Gt, "2"), cond(999, Is, "x")]), [0, 2]);
        assert_eq!(pass(false, vec![cond(TEXT, Contains, "")]), [0, 1, 2, 3]);
        // and through a view
        let v = view(|o| o.filter = Filter { all: true, conds: vec![cond(QTY, Ge, "5")] });
        let r = run(&b, t, &v);
        assert_eq!(r.records, [0, 2]);
        assert_eq!(r.rows, [Row::Record(0), Row::Record(2)]);
    }

    #[test]
    fn natural_keys_order_as_text_cmp() {
        let words = ["", "a", "A", "a2", "a10", "a02", "a2b", "a 2", "B", "b", "ab", "10", "9", "09", "0", "00", "张三", "李四", "x1y2", "x1y10", "x01y2", "é", "E", "a-1", "a.5", "2a", "10a",
            "123456789012345678901234567890", "123456789012345678901234567891", "z"];
        for a in words {
            for b in words {
                assert_eq!(natural(a).cmp(&natural(b)), field::text_cmp(a, b), "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn sorting() {
        let b = base(vec![
            vec![(TEXT, s("b")), (QTY, n(3.)), (STATUS, sel(&[FINISHED]))],
            vec![(TEXT, s("a")), (STATUS, sel(&[TODO]))],
            vec![(TEXT, s("c")), (QTY, n(1.)), (DONE, Value::Bool(true))],
            vec![(TEXT, s("a")), (QTY, n(3.)), (STATUS, sel(&[DOING]))],
            vec![(QTY, n(2.)), (STATUS, sel(&[TODO])), (DONE, Value::Bool(true))],
        ]);
        let t = &b.tables[0];
        let order = |sorts: Vec<SortBy>| run(&b, t, &view(|o| o.sorts = sorts)).records;
        // none: manual order
        assert_eq!(order(vec![]), [0, 1, 2, 3, 4]);
        // blanks last either way; ties keep manual order
        assert_eq!(order(vec![asc(QTY)]), [2, 4, 0, 3, 1]);
        assert_eq!(order(vec![desc(QTY)]), [0, 3, 4, 2, 1]);
        assert_eq!(order(vec![asc(TEXT)]), [1, 3, 0, 2, 4]);
        assert_eq!(order(vec![desc(TEXT)]), [2, 0, 1, 3, 4]);
        // several: the next breaks the first's ties
        assert_eq!(order(vec![asc(QTY), asc(TEXT)]), [2, 4, 3, 0, 1]);
        assert_eq!(order(vec![desc(QTY), desc(TEXT)]), [0, 3, 4, 2, 1]);
        assert_eq!(order(vec![asc(TEXT), desc(QTY)]), [3, 1, 0, 2, 4]);
        // 单选 by option order, not by name
        assert_eq!(order(vec![asc(STATUS)]), [1, 4, 3, 0, 2]);
        assert_eq!(order(vec![desc(STATUS), asc(TEXT)]), [0, 3, 1, 4, 2]);
        // 复选框: unchecked is a value, first when ascending
        assert_eq!(order(vec![asc(DONE)]), [0, 1, 3, 2, 4]);
        assert_eq!(order(vec![desc(DONE)]), [2, 4, 0, 1, 3]);
        // a sort on a field that is gone is skipped
        assert_eq!(order(vec![asc(999), desc(QTY)]), [0, 3, 4, 2, 1]);
        // filtered first
        let v = view(|o| { o.filter.conds = vec![cond(QTY, NotEmpty, "")]; o.sorts = vec![asc(QTY)]; });
        assert_eq!(run(&b, t, &v).records, [2, 4, 0, 3]);
        // natural text order, dates, 关联 by title
        let b = base(vec![
            vec![(TEXT, s("a10")), (DUE, day(2026, 3, 1)), (LINK, Value::Links(vec![2]))],
            vec![(TEXT, s("A2")), (DUE, day(2025, 12, 31)), (LINK, Value::Links(vec![1]))],
            vec![(TEXT, s("a1")), (LINK, Value::Links(vec![3]))],
        ]);
        let t = &b.tables[0];
        let order = |sorts: Vec<SortBy>| run(&b, t, &view(|o| o.sorts = sorts)).records;
        assert_eq!(order(vec![asc(TEXT)]), [2, 1, 0]);
        assert_eq!(order(vec![asc(DUE)]), [1, 0, 2]);
        assert_eq!(order(vec![desc(DUE)]), [0, 1, 2]);
        assert_eq!(order(vec![asc(LINK)]), [0, 2, 1]);
        // the same order `field::compare` gives
        let f = t.field(TEXT).unwrap();
        let mut by_compare = vec![0, 1, 2];
        by_compare.sort_by(|a, b| field::compare(f, t.records[*a].get(TEXT), t.records[*b].get(TEXT)));
        assert_eq!(order(vec![asc(TEXT)]), by_compare);
    }

    fn grouped() -> Base {
        base(vec![
            vec![(STATUS, sel(&[DOING])), (QTY, n(1.)), (DONE, Value::Bool(true)), (DUE, Value::Date(field::date_ms(2026, 11, 17, 9, 0)))],
            vec![(STATUS, sel(&[TODO])), (QTY, n(2.)), (DUE, day(2026, 11, 17))],
            vec![(QTY, n(3.))],
            vec![(STATUS, sel(&[DOING])), (QTY, n(4.)), (DONE, Value::Bool(true)), (DUE, day(2026, 11, 18))],
            vec![(STATUS, sel(&[FINISHED])), (QTY, n(5.))],
            vec![(STATUS, sel(&[TODO])), (QTY, n(6.)), (TAGS, sel(&[RED, BLUE]))],
        ])
    }

    #[test]
    fn one_level_of_groups() {
        let b = grouped();
        let t = &b.tables[0];
        let r = run(&b, t, &view(|o| o.groups = vec![asc(STATUS)]));
        assert_eq!(brief(&r), "[待办:2] 1 5 [进行中:2] 0 3 [完成:1] 4 [:1] 2");
        assert_eq!(r.records, [1, 5, 0, 3, 4, 2]);
        let Row::Group(g) = &r.rows[0] else { panic!() };
        assert_eq!(*g, Group { level: 0, field: STATUS, label: "待办".into(), value: sel(&[TODO]), key: "待办".into(), count: 2, folded: false });
        let Row::Group(g) = &r.rows[8] else { panic!() };
        assert_eq!((g.label.as_str(), &g.value, g.key.as_str()), ("", &Value::Empty, "∅"));
        // 降序: the blank group still last
        let r = run(&b, t, &view(|o| o.groups = vec![desc(STATUS)]));
        assert_eq!(brief(&r), "[完成:1] 4 [进行中:2] 0 3 [待办:2] 1 5 [:1] 2");
        // sorted inside the groups
        let r = run(&b, t, &view(|o| { o.groups = vec![asc(STATUS)]; o.sorts = vec![desc(QTY)]; }));
        assert_eq!(brief(&r), "[待办:2] 5 1 [进行中:2] 3 0 [完成:1] 4 [:1] 2");
        // folded: counted, listed in `records`, not in `rows`
        let r = run(&b, t, &view(|o| { o.groups = vec![asc(STATUS)]; o.folded = ["进行中".to_string(), "∅".to_string()].into(); }));
        assert_eq!(brief(&r), "[待办:2] 1 5 [进行中:2]- [完成:1] 4 [:1]-");
        assert_eq!(r.records, [1, 5, 0, 3, 4, 2]);
        // filtered: groups with nothing left are not shown
        let r = run(&b, t, &view(|o| { o.groups = vec![asc(STATUS)]; o.filter.conds = vec![cond(QTY, Gt, "3")]; }));
        assert_eq!(brief(&r), "[待办:1] 5 [进行中:1] 3 [完成:1] 4");
        // a group field that is gone is skipped
        let r = run(&b, t, &view(|o| o.groups = vec![asc(999)]));
        assert_eq!(brief(&r), "0 1 2 3 4 5");
    }

    #[test]
    fn groups_by_type() {
        let b = grouped();
        let t = &b.tables[0];
        let by = |g: SortBy| run(&b, t, &view(|o| o.groups = vec![g]));
        // 复选框: two groups, unchecked first
        let r = by(asc(DONE));
        assert_eq!(brief(&r), "[未勾选:4] 1 2 4 5 [已勾选:2] 0 3");
        let Row::Group(g) = &r.rows[0] else { panic!() };
        assert_eq!(g.value, Value::Bool(false));
        assert_eq!(brief(&by(desc(DONE))), "[已勾选:2] 0 3 [未勾选:4] 1 2 4 5");
        // 日期: by the day, in the field's format
        let r = by(asc(DUE));
        assert_eq!(brief(&r), "[2026/11/17:2] 0 1 [2026/11/18:1] 3 [:3] 2 4 5");
        let Row::Group(g) = &r.rows[0] else { panic!() };
        assert_eq!(g.value, day(2026, 11, 17));
        assert_eq!(brief(&by(desc(DUE))), "[2026/11/18:1] 3 [2026/11/17:2] 0 1 [:3] 2 4 5");
        // 多选: by the whole set
        assert_eq!(brief(&by(asc(TAGS))), "[红, 蓝:1] 5 [:5] 0 1 2 3 4");
        // 数字: by what shows, ordered as numbers
        let b = base(vec![vec![(QTY, n(10.))], vec![(QTY, n(2.))], vec![(QTY, n(10.))], vec![], vec![(TEXT, s("b"))], vec![(TEXT, s("B"))], vec![(TEXT, s("a"))]]);
        let t = &b.tables[0];
        assert_eq!(brief(&run(&b, t, &view(|o| o.groups = vec![asc(QTY)]))), "[2:1] 1 [10:2] 0 2 [:4] 3 4 5 6");
        assert_eq!(brief(&run(&b, t, &view(|o| o.groups = vec![desc(QTY)]))), "[10:2] 0 2 [2:1] 1 [:4] 3 4 5 6");
        // 文本: by the text itself
        assert_eq!(brief(&run(&b, t, &view(|o| o.groups = vec![asc(TEXT)]))), "[a:1] 6 [B:1] 5 [b:1] 4 [:4] 0 1 2 3");
        // 人员 and 多选 sets with the same first option
        let b = base(vec![vec![(OWNER, sel(&[LI]))], vec![(OWNER, sel(&[ZHANG, LI]))], vec![(OWNER, sel(&[ZHANG]))], vec![(OWNER, sel(&[LI]))]]);
        let t = &b.tables[0];
        assert_eq!(brief(&run(&b, t, &view(|o| o.groups = vec![asc(OWNER)]))), "[张三:1] 2 [张三, 李四:1] 1 [李四:2] 0 3");
    }

    #[test]
    fn nested_groups() {
        let b = grouped();
        let t = &b.tables[0];
        let r = run(&b, t, &view(|o| o.groups = vec![asc(STATUS), desc(DONE)]));
        assert_eq!(brief(&r), "[待办:2] ·[未勾选:2] 1 5 [进行中:2] ·[已勾选:2] 0 3 [完成:1] ·[未勾选:1] 4 [:1] ·[未勾选:1] 2");
        let keys: Vec<&str> = r.rows.iter().filter_map(|row| match row { Row::Group(g) => Some(g.key.as_str()), _ => None }).collect();
        assert_eq!(keys, ["待办", "待办\u{1f}未勾选", "进行中", "进行中\u{1f}已勾选", "完成", "完成\u{1f}未勾选", "∅", "∅\u{1f}未勾选"]);
        let Row::Group(g) = &r.rows[1] else { panic!() };
        assert_eq!((g.level, g.field, g.count), (1, DONE, 2));
        // folding an outer group hides its sub-groups; an inner one only its records
        let r = run(&b, t, &view(|o| { o.groups = vec![asc(STATUS), desc(DONE)]; o.folded = ["待办".to_string(), "进行中\u{1f}已勾选".to_string()].into(); }));
        assert_eq!(brief(&r), "[待办:2]- [进行中:2] ·[已勾选:2]- [完成:1] ·[未勾选:1] 4 [:1] ·[未勾选:1] 2");
        assert_eq!(r.records, [1, 5, 0, 3, 4, 2]);
        // three levels, the fourth ignored; sorts order the innermost
        let r = run(&b, t, &view(|o| { o.groups = vec![asc(DONE), asc(STATUS), desc(DUE), asc(QTY)]; o.sorts = vec![desc(QTY)]; }));
        assert_eq!(brief(&r), "[未勾选:4] ·[待办:2] ··[2026/11/17:1] 1 ··[:1] 5 ·[完成:1] ··[:1] 4 ·[:1] ··[:1] 2 [已勾选:2] ·[进行中:2] ··[2026/11/18:1] 3 ··[2026/11/17:1] 0");
        let deepest = r.rows.iter().filter_map(|row| match row { Row::Group(g) => Some(g.level), _ => None }).max();
        assert_eq!(deepest, Some(2));
        let Row::Group(g) = &r.rows[2] else { panic!() };
        assert_eq!(g.key, "未勾选\u{1f}待办\u{1f}2026/11/17");
    }

    #[test]
    fn stats_by_type() {
        use FieldType::*;
        let common = [Stat::Count, Stat::Filled, Stat::Empty, Stat::Unique, Stat::PercentFilled, Stat::PercentEmpty];
        let numeric: Vec<Stat> = common.iter().copied().chain([Stat::Sum, Stat::Average, Stat::Min, Stat::Max]).collect();
        let dated: Vec<Stat> = common.iter().copied().chain([Stat::Earliest, Stat::Latest]).collect();
        for ty in [Text, SingleSelect, MultiSelect, Person, Attachment, Url, Phone, Email, Location, Barcode, Link] { assert_eq!(stats_for(ty), common, "{ty:?}"); }
        for ty in [Number, Currency, Progress, Rating, AutoNumber, Formula, Lookup] { assert_eq!(stats_for(ty), numeric, "{ty:?}"); }
        for ty in [Date, CreatedTime, ModifiedTime] { assert_eq!(stats_for(ty), dated, "{ty:?}"); }
        assert_eq!(stats_for(Checkbox), [Stat::Count, Stat::Checked, Stat::Unchecked]);
    }

    #[test]
    fn stats() {
        let mut b = grouped();
        for (i, p) in [(0, 1000.), (1, 234.5)] { b.tables[0].records[i].set(PRICE, n(p)); }
        for (i, p) in [(0, 0.5), (1, 0.25), (2, 0.255)] { b.tables[0].records[i].set(PROGRESS, n(p)); }
        b.tables[0].records[2].set(STATUS, sel(&[TODO]));
        b.tables[0].records[2].set(STATUS, Value::Empty);
        let t = &b.tables[0];
        let all: Vec<usize> = (0..t.records.len()).collect();
        let of = |f: FieldId, records: &[usize], s: Stat| stat(&b, t, t.field(f).unwrap(), records, s);
        assert_eq!(of(QTY, &all, Stat::Count), "6");
        assert_eq!(of(STATUS, &all, Stat::Count), "6");
        assert_eq!(of(STATUS, &all, Stat::Filled), "5");
        assert_eq!(of(STATUS, &all, Stat::Empty), "1");
        assert_eq!(of(STATUS, &all, Stat::Unique), "3");
        assert_eq!(of(STATUS, &all, Stat::PercentFilled), "83.33%");
        assert_eq!(of(STATUS, &all, Stat::PercentEmpty), "16.67%");
        assert_eq!(of(STATUS, &[0, 1, 2, 3], Stat::PercentFilled), "75%");
        assert_eq!(of(STATUS, &[0, 1, 2, 3], Stat::PercentEmpty), "25%");
        assert_eq!(of(TEXT, &all, Stat::PercentFilled), "0%");
        assert_eq!(of(TEXT, &all, Stat::PercentEmpty), "100%");
        assert_eq!(of(TEXT, &all, Stat::Unique), "0");
        assert_eq!(of(QTY, &all, Stat::Sum), "21");
        assert_eq!(of(QTY, &all, Stat::Average), "3.5");
        assert_eq!(of(QTY, &[0, 1, 1], Stat::Average), "1.67");
        assert_eq!(of(QTY, &all, Stat::Min), "1");
        assert_eq!(of(QTY, &all, Stat::Max), "6");
        assert_eq!(of(QTY, &[1, 3], Stat::Sum), "6");
        // in the field's format
        assert_eq!(of(PRICE, &all, Stat::Sum), "¥1,234.50");
        assert_eq!(of(PRICE, &all, Stat::Average), "¥617.25");
        assert_eq!(of(PRICE, &all, Stat::Max), "¥1,000.00");
        assert_eq!(of(PRICE, &all, Stat::Filled), "2");
        assert_eq!(of(PROGRESS, &all, Stat::Min), "25%");
        assert_eq!(of(PROGRESS, &all, Stat::Average), "34%");
        // nothing to work on
        assert_eq!(of(PRICE, &[2, 3], Stat::Sum), "¥0.00");
        assert_eq!(of(QTY, &[], Stat::Sum), "0");
        for s in [Stat::Average, Stat::Min, Stat::Max, Stat::PercentFilled, Stat::PercentEmpty, Stat::Earliest, Stat::Latest] { assert_eq!(of(PRICE, &[], s), "", "{s:?}"); }
        assert_eq!(of(PRICE, &[2, 3], Stat::Average), "");
        assert_eq!(of(QTY, &[], Stat::Count), "0");
        // 日期
        assert_eq!(of(DUE, &all, Stat::Earliest), "2026/11/17");
        assert_eq!(of(DUE, &all, Stat::Latest), "2026/11/18");
        assert_eq!(of(DUE, &all, Stat::Unique), "2");
        assert_eq!(of(DUE, &[2, 4], Stat::Earliest), "");
        // 复选框
        assert_eq!(of(DONE, &all, Stat::Checked), "2");
        assert_eq!(of(DONE, &all, Stat::Unchecked), "4");
        assert_eq!(of(DONE, &all, Stat::Count), "6");
        // a percentage's average keeps two digits of the percentage; a lookup's list counts each number
        let mut b = base(vec![vec![(QTY, n(0.1)), (LOOKUP, Value::List(vec![n(1.), n(2.)]))], vec![(QTY, n(0.2)), (LOOKUP, n(6.))], vec![(QTY, n(0.2))]]);
        b.tables[0].field_mut(QTY).unwrap().opts.style = NumberStyle::Percent;
        let t = &b.tables[0];
        assert_eq!(stat(&b, t, t.field(QTY).unwrap(), &[0, 1, 2], Stat::Average), "16.67%");
        assert_eq!(stat(&b, t, t.field(QTY).unwrap(), &[0, 1, 2], Stat::Unique), "2");
        assert_eq!(stat(&b, t, t.field(LOOKUP).unwrap(), &[0, 1, 2], Stat::Sum), "9");
        assert_eq!(stat(&b, t, t.field(LOOKUP).unwrap(), &[0, 1, 2], Stat::Average), "3");
    }

    #[test]
    fn row_colors() {
        let b = grouped();
        let t = &b.tables[0];
        let rule = |conds: Vec<Cond>, color: u8| ColorRule { filter: Filter { all: true, conds }, color };
        let v = view(|o| o.colors = vec![rule(vec![cond(QTY, Gt, "4")], 3), rule(vec![cond(STATUS, Is, "待办")], 5), rule(vec![cond(DUE, Is, "")], 7), rule(vec![], 8)]);
        let colors: Vec<Option<u8>> = t.records.iter().map(|r| row_color(&b, t, r, &v, today())).collect();
        // the first rule met wins; a rule that says nothing yet colours nothing
        assert_eq!(colors, [None, Some(5), None, None, Some(3), Some(3)]);
        let v = view(|o| o.colors = vec![ColorRule { filter: Filter { all: false, conds: vec![cond(QTY, Lt, "2"), Cond { field: DUE, op: Is, value: String::new(), date: DateRef::Tomorrow }] }, color: 1 }]);
        let colors: Vec<Option<u8>> = t.records.iter().map(|r| row_color(&b, t, r, &v, today())).collect();
        assert_eq!(colors, [Some(1), None, None, Some(1), None, None]);
        assert_eq!(row_color(&b, t, &t.records[0], &view(|_| {}), today()), None);
    }

    #[test]
    fn new_record_defaults() {
        let b = grouped();
        let t = &b.tables[0];
        let v = view(|o| o.filter.conds = vec![
            cond(STATUS, Is, "进行中"), cond(TEXT, Is, "标题"), cond(QTY, Is, "7"), cond(DONE, Is, "true"), cond(DUE, Is, "2026-12-01"), cond(TAGS, Is, "红,绿"),
            // not ones a value follows from
            cond(PRICE, Gt, "5"), cond(URL, Contains, "x"), cond(PHONE, IsNot, "1"), cond(EMAIL, Is, ""), cond(CALC, Is, "1"), cond(AUTO, Is, "3"), cond(999, Is, "x"),
            // 包含 on a select is
            cond(OWNER, Contains, "李四, 张三"),
        ]);
        assert_eq!(defaults_for(t, &v, None), [(STATUS, sel(&[DOING])), (TEXT, s("标题")), (QTY, n(7.)), (DONE, Value::Bool(true)), (DUE, day(2026, 12, 1)),
            (TAGS, sel(&[RED, GREEN])), (OWNER, sel(&[LI, ZHANG]))]);
        // 单选 包含 several: the first; a relative day; unchecked needs nothing
        let v = view(|o| o.filter.conds = vec![cond(STATUS, Contains, "完成,待办"), Cond { field: DUE, op: Is, value: String::new(), date: DateRef::Tomorrow },
            Cond { field: CREATED, op: Is, value: String::new(), date: DateRef::Today }, cond(DONE, Is, "false")]);
        assert_eq!(defaults_for(t, &v, None), [(STATUS, sel(&[FINISHED])), (DUE, Value::Date(field::today() + DAY))]);
        let v = view(|o| o.filter.conds = vec![Cond { field: DUE, op: Is, value: String::new(), date: DateRef::ThisWeek }]);
        assert_eq!(defaults_for(t, &v, None), []);
        // 或: nothing follows, unless there is one condition
        let v = view(|o| o.filter = Filter { all: false, conds: vec![cond(STATUS, Is, "进行中"), cond(QTY, Is, "7")] });
        assert_eq!(defaults_for(t, &v, None), []);
        let v = view(|o| o.filter = Filter { all: false, conds: vec![cond(QTY, Is, "7")] });
        assert_eq!(defaults_for(t, &v, None), [(QTY, n(7.))]);
        // under a group: its value, over the filter's
        let v = view(|o| { o.groups = vec![asc(STATUS), asc(DONE), asc(DUE)]; o.filter.conds = vec![cond(QTY, Is, "7"), cond(STATUS, Contains, "待办,进行中")]; });
        // (the headers of the view unfiltered: nothing here passes its filter)
        let r = run(&b, t, &view(|o| o.groups = v.opts.groups.clone()));
        let group = |key: &str| r.rows.iter().find_map(|row| match row { Row::Group(g) if g.key == key => Some(g), _ => None }).unwrap();
        assert_eq!(defaults_for(t, &v, Some(group("进行中"))), [(QTY, n(7.)), (STATUS, sel(&[DOING]))]);
        // and its parents'
        assert_eq!(defaults_for(t, &v, Some(group("进行中\u{1f}已勾选\u{1f}2026/11/18"))), [(QTY, n(7.)), (STATUS, sel(&[DOING])), (DONE, Value::Bool(true)), (DUE, day(2026, 11, 18))]);
        assert_eq!(defaults_for(t, &v, Some(group("待办\u{1f}未勾选\u{1f}∅"))), [(QTY, n(7.)), (STATUS, sel(&[TODO]))]);
        assert_eq!(defaults_for(t, &view(|_| {}), None), []);
    }

    #[test]
    fn finding() {
        let b = base(vec![
            vec![(TEXT, s("Alpha")), (STATUS, sel(&[TODO])), (QTY, n(12.)), (LINK, Value::Links(vec![1]))],
            vec![(TEXT, s("beta")), (URL, s("https://ALPHA.example")), (PRICE, n(1200.))],
            vec![(TEXT, s("待办事项")), (DUE, day(2026, 11, 12))],
        ]);
        let t = &b.tables[0];
        let fields = [TEXT, STATUS, QTY, PRICE, URL, DUE, LINK, 999];
        assert_eq!(find(&b, t, &[0, 1, 2], &fields, "alpha"), [(0, TEXT), (1, URL)]);
        // by what shows: option names, formatted numbers and dates, linked titles
        assert_eq!(find(&b, t, &[0, 1, 2], &fields, "待办"), [(0, STATUS), (2, TEXT)]);
        assert_eq!(find(&b, t, &[0, 1, 2], &fields, "12"), [(0, QTY), (2, DUE)]);
        assert_eq!(find(&b, t, &[0, 1, 2], &fields, "1,200"), [(1, PRICE)]);
        assert_eq!(find(&b, t, &[0, 1, 2], &fields, "甲公"), [(0, LINK)]);
        // in the order given, only the records and fields given
        assert_eq!(find(&b, t, &[2, 0], &[QTY, DUE, TEXT], "1"), [(2, DUE), (0, QTY)]);
        assert_eq!(find(&b, t, &[1], &[TEXT], "alpha"), []);
        assert_eq!(find(&b, t, &[0, 1, 2], &fields, ""), []);
        assert_eq!(find(&b, t, &[0, 7], &fields, "ALPHA"), [(0, TEXT)]);
    }

    #[test]
    fn kanban_columns() {
        let b = grouped();
        let t = &b.tables[0];
        assert_eq!(kanban(&b, t, &view(|o| o.stack = Some(STATUS))), [(None, vec![2]), (Some(TODO), vec![1, 5]), (Some(DOING), vec![0, 3]), (Some(FINISHED), vec![4])]);
        // filtered and sorted; the view's groups don't matter; empty columns stay
        let v = view(|o| { o.stack = Some(STATUS); o.sorts = vec![desc(QTY)]; o.groups = vec![asc(DONE)]; o.filter.conds = vec![cond(QTY, Lt, "6")]; });
        assert_eq!(kanban(&b, t, &v), [(None, vec![2]), (Some(TODO), vec![1]), (Some(DOING), vec![3, 0]), (Some(FINISHED), vec![4])]);
        // 多选 / 人员: by the first option
        assert_eq!(kanban(&b, t, &view(|o| o.stack = Some(TAGS))), [(None, vec![0, 1, 2, 3, 4]), (Some(RED), vec![5]), (Some(BLUE), vec![]), (Some(GREEN), vec![])]);
        let b = base(vec![vec![(OWNER, sel(&[LI, ZHANG]))], vec![(OWNER, sel(&[ZHANG]))], vec![(OWNER, sel(&[99]))]]);
        let t = &b.tables[0];
        assert_eq!(kanban(&b, t, &view(|o| o.stack = Some(OWNER))), [(None, vec![2]), (Some(ZHANG), vec![1]), (Some(LI), vec![0])]);
        // no stack field
        assert_eq!(kanban(&b, t, &view(|_| {})), [(None, vec![0, 1, 2])]);
        assert_eq!(kanban(&b, t, &view(|o| o.stack = Some(999))), [(None, vec![0, 1, 2])]);
    }

    #[test]
    fn blank_table_runs() {
        let b = Base::blank(0);
        let t = &b.tables[0];
        // a new table is five plain 文本 columns
        assert_eq!(t.fields.iter().map(|f| (f.name.as_str(), f.ty)).collect::<Vec<_>>(), (1..=5).map(|i| (["文本 1", "文本 2", "文本 3", "文本 4", "文本 5"][i - 1], FieldType::Text)).collect::<Vec<_>>());
        let r = run(&b, t, &t.views[0]);
        assert_eq!(r.records, [0, 1, 2, 3, 4]);
        assert_eq!(r.rows.len(), 5);
        let r = run(&b, t, &view(|o| { o.groups = vec![asc(t.fields[1].id)]; o.sorts = vec![asc(t.fields[0].id)]; }));
        assert_eq!(brief(&r), "[:5] 0 1 2 3 4");
        assert_eq!(run(&b, &Table::default(), &view(|o| o.groups = vec![asc(1)])), ViewResult::default());
    }

    /// `cargo test --release -p fdl_core table::query -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn hundred_thousand_rows() {
        use std::time::Instant;
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut rnd = move |n: u64| { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed % n };
        let rows: Vec<Vec<(FieldId, Value)>> = (0..100_000).map(|_| vec![
            (TEXT, s(&format!("Item {} of batch {}", rnd(50_000), rnd(100)))),
            (STATUS, match rnd(4) { 0 => Value::Empty, k => sel(&[TODO + k as u32 - 1]) }),
            (QTY, if rnd(20) == 0 { Value::Empty } else { n(rnd(1000) as f64) }),
            (DUE, Value::Date(ms(2026, 1, 1) + rnd(365) as i64 * DAY)),
        ]).collect();
        let b = base(rows);
        let t = &b.tables[0];
        let time = |name: &str, v: &View| {
            let mut best = f64::MAX;
            let mut r = ViewResult::default();
            for _ in 0..5 { let at = Instant::now(); r = run(&b, t, v); best = best.min(at.elapsed().as_secs_f64() * 1000.); }
            println!("{name}: {best:.1} ms ({} of {} records, {} rows)", r.records.len(), t.records.len(), r.rows.len());
            best
        };
        let filter = Filter { all: true, conds: vec![cond(QTY, Gt, "100"), cond(TEXT, Contains, "item 1")] };
        time("no filter, no sort", &view(|_| {}));
        time("filter (number + text)", &view(|o| o.filter = filter.clone()));
        time("2 sorts (select, text), no filter", &view(|o| o.sorts = vec![asc(STATUS), desc(TEXT)]));
        time("2 sorts (number, date), no filter", &view(|o| o.sorts = vec![desc(QTY), asc(DUE)]));
        time("2 groups + sort, no filter", &view(|o| { o.groups = vec![asc(STATUS), desc(DUE)]; o.sorts = vec![asc(TEXT)]; }));
        let ms = time("filter + 2 sorts (select, text)", &view(|o| { o.filter = filter.clone(); o.sorts = vec![asc(STATUS), desc(TEXT)]; }));
        assert!(cfg!(debug_assertions) || ms < 100., "{ms:.1} ms");
    }
}
