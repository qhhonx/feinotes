//! Random edits through `Doc`'s actions: whatever the sequence, undoing
//! everything gives the starting base back, redoing gives the end again,
//! and the base stays well-formed (unique ids, no cell of a missing field,
//! views that only name fields that exist). `FDL_FUZZ_SEEDS` runs more seeds.

use super::*;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0 }
    fn below(&mut self, n: usize) -> usize { if n == 0 { 0 } else { (self.next() % n as u64) as usize } }
    fn pick<'a, T>(&mut self, v: &'a [T]) -> Option<&'a T> { if v.is_empty() { None } else { v.get(self.below(v.len())) } }
}

const TYPES: &[FieldType] = &[FieldType::Text, FieldType::Number, FieldType::SingleSelect, FieldType::MultiSelect, FieldType::Date, FieldType::Checkbox,
    FieldType::Rating, FieldType::Currency, FieldType::Progress, FieldType::Url, FieldType::Formula, FieldType::Link, FieldType::Lookup, FieldType::AutoNumber, FieldType::CreatedTime];
const TEXTS: &[&str] = &["", "苹果", "12", "3.5", "2026-01-30", "true", "红, 蓝", "https://a.b", "50%", "x"];

fn step(d: &mut Doc, r: &mut Rng) {
    let tids: Vec<TableId> = d.base.tables.iter().map(|t| t.id).collect();
    let tid = *r.pick(&tids).unwrap();
    let t = d.base.table(tid).unwrap();
    let recs: Vec<RecId> = t.records.iter().map(|x| x.id).collect();
    let fids: Vec<FieldId> = t.fields.iter().map(|f| f.id).collect();
    let vids: Vec<ViewId> = t.views.iter().map(|v| v.id).collect();
    let some_recs = |r: &mut Rng| -> Vec<RecId> { (0..r.below(3) + 1).filter_map(|_| r.pick(&recs).copied()).collect() };
    match r.below(22) {
        0..=2 => {
            let (Some(rid), Some(fid)) = (r.pick(&recs).copied(), r.pick(&fids).copied()) else { return };
            let f = t.field(fid).unwrap().clone();
            let v = match f.ty {
                FieldType::Link => Value::Links(f.opts.link_table.and_then(|o| d.base.table(o)).map(|o| o.records.iter().map(|x| x.id).filter(|_| r.below(3) == 0).collect()).unwrap_or_default()),
                _ => { let mut f = f; let mut n = d.base.next_id; let v = field::parse_new(&mut f, r.pick(TEXTS).unwrap(), &mut n); if f.ty.select() { Value::Empty } else { v } }
            };
            d.set_cells(tid, vec![(rid, fid, v)]);
        }
        3 => { let texts: Vec<Vec<String>> = (0..r.below(3) + 1).map(|_| (0..r.below(3) + 1).map(|_| r.pick(TEXTS).unwrap().to_string()).collect()).collect(); let at = r.below(recs.len() + 1); d.paste(tid, &recs[at.min(recs.len())..], &fids[r.below(fids.len())..], &texts); }
        4 => { d.add_records(tid, Some(r.below(recs.len() + 1)), r.below(3) + 1, &[]); }
        5 => { let ids = some_recs(r); d.remove_records(tid, &ids); }
        6 => { let ids = some_recs(r); d.duplicate_records(tid, &ids); }
        7 => { let ids = some_recs(r); d.move_records(tid, &ids, r.below(recs.len() + 1)); }
        8 | 9 => {
            let ty = *r.pick(TYPES).unwrap();
            let mut o = field::default_opts(ty);
            match ty {
                FieldType::Formula => o.formula = (*r.pick(&["1+1", "{文本} & \"x\"", "IF(1>2, 3, 4)", "LEN({文本})", "(", "{没有}"]).unwrap()).to_string(),
                FieldType::Link => { o.link_table = r.pick(&tids).copied(); o.back = (r.below(2) == 0).then_some(0); o.single = r.below(3) == 0; }
                FieldType::Lookup => { o.via = t.fields.iter().find(|f| f.ty == FieldType::Link).map(|f| f.id); o.target = o.via.and_then(|v| t.field(v)).and_then(|f| f.opts.link_table).and_then(|o| d.base.table(o)).and_then(|o| r.pick(&o.fields).map(|f| f.id)); o.rollup = *r.pick(Rollup::ALL).unwrap(); }
                _ => {}
            }
            d.add_field(tid, Some(r.below(fids.len() + 1)), r.pick(&["", "名称", "数量", "id", "_x", "文本"]).unwrap(), ty, o);
        }
        10 => if r.below(2) == 0 { if let Some(f) = r.pick(&fids) { d.remove_field(tid, *f); } } else {
            // several at once (表头多选后删除): one undo step
            let ids: Vec<FieldId> = (0..r.below(3) + 1).filter_map(|_| r.pick(&fids).copied()).collect();
            d.remove_fields(tid, &ids);
        },
        11 => { if let Some(f) = r.pick(&fids) { d.duplicate_field(tid, *f); } }
        12 | 13 => {
            let Some(mut f) = r.pick(&fids).and_then(|f| t.field(*f)).cloned() else { return };
            match r.below(4) {
                0 => f.name = (*r.pick(&["改名", "文本", "A", "数量 2"]).unwrap()).to_string(),
                1 => { f.ty = *r.pick(TYPES).unwrap(); f.opts = FieldOpts { options: f.opts.options.clone(), ..field::default_opts(f.ty) }; if f.ty == FieldType::Link { f.opts.link_table = r.pick(&tids).copied(); } }
                2 => { let id = d.base.new_id(); f.opts.options.push(SelectOption { id, name: format!("选项{id}"), color: 1 }); }
                _ => { f.opts.options.pop(); }
            }
            d.change_field(tid, f);
        }
        14 => { d.add_view(tid, *r.pick(ViewKind::ALL).unwrap(), ""); }
        15 => { if let Some(v) = r.pick(&vids) { d.duplicate_view(tid, *v); } }
        16 => { if let Some(v) = r.pick(&vids) { d.run_in(tid, vec![Op::RemoveView(*v)]); } }
        17 => {
            let (Some(v), Some(f)) = (r.pick(&vids).copied(), r.pick(&fids).copied()) else { return };
            let k = r.below(7);
            d.edit_view(tid, v, |v| match k {
                0 => { v.opts.hidden.insert(f); }
                1 => v.opts.sorts.push(SortBy { field: f, desc: true }),
                2 => v.opts.groups = vec![SortBy { field: f, desc: false }],
                3 => v.opts.filter.conds.push(Cond { field: f, op: FilterOp::NotEmpty, value: String::new(), date: DateRef::Exact }),
                4 => { v.opts.widths.insert(f, 222.); }
                5 => v.opts.order.insert(0, f),
                _ => { v.opts.stats.insert(f, Stat::Filled); v.opts.stack = Some(f); }
            });
        }
        18 => { d.add_table(r.pick(&["", "任务", "数据表"]).unwrap()); }
        19 => { d.remove_table(tid); }
        20 => { d.rename_table(tid, r.pick(&["项目", "A b", "数据表"]).unwrap()); }
        _ => { if r.below(2) == 0 { d.undo(); } else { d.redo(); } }
    }
}

fn check(b: &Base) {
    let mut ids = std::collections::HashSet::new();
    for t in &b.tables {
        assert!(ids.insert(t.id), "table id twice");
        assert!(!t.fields.is_empty() && !t.views.is_empty(), "a table keeps its primary field and a view");
        assert!(sqlite_name_ok(&t.name), "table name {:?}", t.name);
        let mut names = std::collections::HashSet::new();
        for f in &t.fields {
            assert!(ids.insert(f.id), "field id twice");
            assert!(sqlite_name_ok(&f.name) && names.insert(f.name.to_lowercase()), "field name {:?}", f.name);
        }
        let mut recs = std::collections::HashSet::new();
        for r in &t.records {
            assert!(recs.insert(r.id) && r.id < t.next_rec, "record id");
            for (f, v) in &r.cells { assert!(t.field(*f).is_some(), "a cell of a field that is gone"); assert!(!v.is_empty(), "an empty cell is stored"); }
        }
        for v in &t.views {
            assert!(ids.insert(v.id), "view id twice");
            let o = &v.opts;
            for f in o.hidden.iter().chain(o.widths.keys()).chain(o.sorts.iter().map(|s| &s.field)).chain(o.groups.iter().map(|s| &s.field)).chain(o.filter.conds.iter().map(|c| &c.field)).chain(o.stats.keys()).chain(o.stack.iter()) {
                assert!(t.field(*f).is_some(), "a view names a field that is gone");
            }
        }
        // every view computes
        for v in &t.views { let res = query::run(b, t, v); assert!(res.records.len() <= t.records.len()); }
    }
}

#[test]
fn random_table_edits() {
    let seeds: u64 = std::env::var("FDL_FUZZ_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(60);
    for seed in 1..=seeds {
        let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut d = Doc::new(Base::blank(0));
        let start = d.base.clone();
        for i in 0..120 {
            step(&mut d, &mut r);
            let b = d.base.clone();
            std::panic::catch_unwind(|| check(&b)).unwrap_or_else(|_| panic!("seed {seed} step {i}"));
        }
        // everything undone is the start again (ids handed out aside); redone, the end
        // steps undone by the run itself are redone first: the end is the top of the history
        while d.redo() {}
        let end = d.base.clone();
        let mut n = 0;
        while d.undo() { n += 1; assert!(n < 10_000); }
        let mut back = d.base.clone();
        back.next_id = start.next_id;
        for (t, s) in back.tables.iter_mut().zip(&start.tables) { t.next_rec = s.next_rec; t.next_auto = s.next_auto; for (r, sr) in t.records.iter_mut().zip(&s.records) { r.modified = sr.modified; } }
        assert_eq!(back, start, "seed {seed}: undo does not return to the start");
        while d.redo() {}
        let mut again = d.base.clone();
        for (t, e) in again.tables.iter_mut().zip(&end.tables) { for (r, er) in t.records.iter_mut().zip(&e.records) { r.modified = er.modified; r.created = er.created; } }
        let mut end = end;
        end.next_id = again.next_id;
        assert_eq!(again, end, "seed {seed}: redo does not return to the end");
    }
}
