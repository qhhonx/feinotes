//! Sample bases: a small project tracker with every kind of field and view
//! (devtools, screenshots, tests), and one big flat table for timing.

use super::field;
use super::model::*;
use super::ops::Doc;

const DAY: i64 = 86_400_000;

fn fld(b: &mut Base, name: &str, ty: FieldType) -> Field { Field { id: b.new_id(), name: name.into(), ty, opts: field::default_opts(ty), desc: String::new() } }

/// `f` with options of the given names and palette slots.
fn choices(b: &mut Base, mut f: Field, names: &[(&str, u8)]) -> Field {
    for (n, color) in names { f.opts.options.push(SelectOption { id: b.new_id(), name: n.to_string(), color: *color }); }
    f
}

fn pick(f: &Field, names: &[&str]) -> Value { Value::Select(names.iter().filter_map(|n| f.option_named(n)).map(|o| o.id).collect()) }

fn text(s: &str) -> Value { Value::Text(s.to_string()) }

const PEOPLE: &[(&str, u8)] = &[("王磊", 0), ("李娜", 2), ("张伟", 4), ("刘洋", 6), ("陈静", 8)];
const TAGS: &[(&str, u8)] = &[("设计", 1), ("开发", 3), ("测试", 5), ("文档", 7), ("运营", 9), ("紧急", 0)];

/// (项目名称, 阶段, 项目负责人, 启动日期 in days from today)
const PROJECTS: &[(&str, &str, &str, i64)] = &[
    ("官网改版", "进行中", "李娜", -60), ("移动端 2.0", "进行中", "王磊", -45), ("数据平台迁移", "规划中", "张伟", 10),
    ("客户成功体系", "已上线", "陈静", -120), ("年度品牌活动", "进行中", "刘洋", -20), ("内部工具整合", "已暂停", "张伟", -90),
];

/// (任务名称, its project's row)
const TASKS: &[(&str, usize)] = &[
    ("梳理现有页面结构", 0), ("首页视觉方案", 0), ("产品页文案重写", 0), ("响应式布局开发", 0), ("SEO 检查与修复", 0), ("上线前走查", 0), ("旧链接重定向", 0),
    ("需求评审", 1), ("登录流程重做", 1), ("消息推送接入", 1), ("离线缓存方案", 1), ("深色模式适配", 1), ("应用商店素材", 1), ("灰度发布计划", 1), ("崩溃率监控", 1),
    ("现状盘点与容量评估", 2), ("迁移方案设计", 2), ("历史数据清洗脚本", 2), ("双写验证", 2), ("切换演练", 2), ("回滚预案", 2),
    ("客户分层模型", 3), ("入门培训课程", 3), ("健康度看板", 3), ("续约提醒流程", 3), ("满意度回访", 3), ("帮助中心改版", 3),
    ("活动主题与预算", 4), ("场地与供应商确认", 4), ("嘉宾邀请", 4), ("宣传物料设计", 4), ("社交媒体排期", 4), ("现场流程彩排", 4), ("活动复盘报告", 4),
    ("工具使用情况调研", 5), ("账号体系打通", 5), ("审批流程合并", 5), ("知识库迁移", 5), ("旧系统下线通知", 5), ("权限梳理", 5),
];

/// A project tracker around `now` (wall-clock milliseconds): 「项目任务」,
/// 40 records over every field type but 附件, in four views (表格, grouped
/// by 状态 with a filter and a sort, 看板), linked both ways to
/// 「项目」, which counts and sums its tasks. The same `now` gives the same base.
pub fn demo(now: i64) -> Base {
    use FieldType::*;
    let today = now.div_euclid(DAY) * DAY;
    let mut b = Base::default();
    let (tasks, projects) = (b.new_id(), b.new_id());

    let name = fld(&mut b, "任务名称", Text);
    let status = { let f = fld(&mut b, "状态", SingleSelect); choices(&mut b, f, &[("未开始", 9), ("进行中", 4), ("已完成", 2), ("已搁置", 0)]) };
    let tags = { let f = fld(&mut b, "标签", MultiSelect); choices(&mut b, f, TAGS) };
    let owner = { let f = fld(&mut b, "负责人", Person); choices(&mut b, f, PEOPLE) };
    let start = fld(&mut b, "开始日期", Date);
    let due = fld(&mut b, "截止日期", Date);
    let mut hours = fld(&mut b, "工时", Number);
    hours.opts.precision = Some(1);
    let budget = fld(&mut b, "预算", Currency);
    let progress = fld(&mut b, "进度", Progress);
    let priority = fld(&mut b, "优先级", Rating);
    let late = fld(&mut b, "是否延期", Checkbox);
    let url = fld(&mut b, "参考链接", Url);
    let mut project = fld(&mut b, "所属项目", Link);
    let mut lead = fld(&mut b, "项目负责人", Lookup);
    let mut left = fld(&mut b, "剩余天数", Formula);
    left.opts.formula = "DATEDIF(TODAY(), {截止日期}, \"D\")".into();
    let mut spent = fld(&mut b, "已用预算", Formula);
    spent.opts.formula = "{预算} * {进度}".into();
    spent.opts.precision = Some(2);
    let mut no = fld(&mut b, "编号", AutoNumber);
    no.opts.prefix = "T-".into();
    let created = fld(&mut b, "创建时间", CreatedTime);

    let pname = fld(&mut b, "项目名称", Text);
    let stage = { let f = fld(&mut b, "阶段", SingleSelect); choices(&mut b, f, &[("规划中", 9), ("进行中", 4), ("已上线", 2), ("已暂停", 0)]) };
    let plead = { let f = fld(&mut b, "项目负责人", Person); choices(&mut b, f, PEOPLE) };
    let kickoff = fld(&mut b, "启动日期", Date);
    let mut ptasks = fld(&mut b, "任务", Link);
    let mut count = fld(&mut b, "任务数", Lookup);
    let mut total = fld(&mut b, "预算合计", Lookup);
    project.opts = FieldOpts { link_table: Some(projects), single: true, back: Some(ptasks.id), ..Default::default() };
    ptasks.opts = FieldOpts { link_table: Some(tasks), back: Some(project.id), ..Default::default() };
    lead.opts = FieldOpts { via: Some(project.id), target: Some(plead.id), ..Default::default() };
    count.opts = FieldOpts { via: Some(ptasks.id), target: Some(name.id), rollup: Rollup::Count, ..Default::default() };
    total.opts = FieldOpts { via: Some(ptasks.id), target: Some(budget.id), rollup: Rollup::Sum, precision: Some(2), style: NumberStyle::Thousands, ..Default::default() };

    let mut task_rows = vec![];
    for (i, (title, p)) in TASKS.iter().enumerate() {
        let n = i as i64;
        let state = match i % 10 { 0 | 4 | 8 => "已完成", 1 | 2 | 5 | 9 => "进行中", 3 | 6 => "未开始", _ => "已搁置" };
        let begins = today + (n * 2 - 50) * DAY;
        let ends = begins + (4 + n * 7 % 11) * DAY;
        let done = match state { "已完成" => 1., "未开始" => 0., "已搁置" => 0.3, _ => (10 + n * 13 % 80) as f64 / 100. };
        let made = now - (45 - n) * DAY - n * 37 * 60_000;
        let mut r = Record { id: i as u64 + 1, created: made, modified: made + (n % 9 + 1) * 3_600_000, auto: i as u64 + 1, ..Default::default() };
        r.set(name.id, text(title));
        r.set(status.id, pick(&status, &[state]));
        r.set(tags.id, pick(&tags, &if i % 4 == 0 { vec![TAGS[i % 6].0, TAGS[(i + 2) % 6].0] } else { vec![TAGS[i % 6].0] }));
        r.set(owner.id, pick(&owner, &if i % 7 == 0 { vec![PEOPLE[i % 5].0, PEOPLE[(i + 3) % 5].0] } else { vec![PEOPLE[i % 5].0] }));
        r.set(start.id, Value::Date(begins));
        r.set(due.id, Value::Date(ends));
        r.set(hours.id, Value::Number(4. + (n * 5 % 36) as f64 + if i % 3 == 0 { 0.5 } else { 0. }));
        r.set(budget.id, Value::Number((2000 + n * 1700 % 18000) as f64));
        r.set(progress.id, Value::Number(done));
        r.set(priority.id, Value::Number((1 + n * 3 % 5) as f64));
        r.set(late.id, Value::Bool(ends < today && state != "已完成"));
        if i % 3 == 1 { r.set(url.id, text(&format!("https://example.com/docs/task-{}", i + 1))); }
        r.set(project.id, Value::Links(vec![*p as u64 + 1]));
        task_rows.push(r);
    }
    let mut project_rows = vec![];
    for (i, (title, phase, who, offset)) in PROJECTS.iter().enumerate() {
        let made = now - (130 - i as i64 * 3) * DAY;
        let mut r = Record { id: i as u64 + 1, created: made, modified: made + DAY, auto: i as u64 + 1, ..Default::default() };
        r.set(pname.id, text(title));
        r.set(stage.id, pick(&stage, &[phase]));
        r.set(plead.id, pick(&plead, &[who]));
        r.set(kickoff.id, Value::Date(today + offset * DAY));
        r.set(ptasks.id, Value::Links(TASKS.iter().enumerate().filter(|(_, t)| t.1 == i).map(|(k, _)| k as u64 + 1).collect()));
        project_rows.push(r);
    }

    let shelved = Filter { all: true, conds: vec![Cond { field: status.id, op: FilterOp::IsNot, value: "已搁置".into(), date: DateRef::Exact }] };
    let overdue = Filter { all: true, conds: vec![Cond { field: late.id, op: FilterOp::Is, value: "true".into(), date: DateRef::Exact }] };
    let view = |b: &mut Base, name: &str, kind: ViewKind, opts: ViewOpts| View { id: b.new_id(), name: name.into(), kind, opts };
    let views = vec![
        view(&mut b, "全部任务", ViewKind::Grid, ViewOpts { frozen: 1, widths: [(name.id, 260.), (left.id, 110.), (priority.id, 130.)].into_iter().collect(), ..Default::default() }),
        view(&mut b, "按状态分组", ViewKind::Grid, ViewOpts {
            frozen: 1,
            hidden: [url.id, created.id, no.id].into_iter().collect(),
            filter: shelved,
            sorts: vec![SortBy { field: due.id, desc: false }],
            groups: vec![SortBy { field: status.id, desc: false }],
            stats: [(name.id, Stat::Count), (hours.id, Stat::Sum), (budget.id, Stat::Sum), (progress.id, Stat::Average)].into_iter().collect(),
            colors: vec![ColorRule { filter: overdue, color: 0 }],
            ..Default::default()
        }),
        view(&mut b, "看板", ViewKind::Kanban, ViewOpts { frozen: 1, stack: Some(status.id), hidden: [url.id, created.id, no.id, spent.id, lead.id].into_iter().collect(), ..Default::default() }),
    ];
    let project_views = vec![view(&mut b, "表格", ViewKind::Grid, ViewOpts { frozen: 1, ..Default::default() })];
    b.tables.push(Table {
        id: tasks, name: "项目任务".into(),
        fields: vec![name, status, tags, owner, start, due, hours, budget, progress, priority, late, url, project, lead, left, spent, no, created],
        next_rec: task_rows.len() as u64 + 1, next_auto: task_rows.len() as u64 + 1, records: task_rows, views,
    });
    b.tables.push(Table {
        id: projects, name: "项目".into(), fields: vec![pname, stage, plead, kickoff, ptasks, count, total],
        next_rec: project_rows.len() as u64 + 1, next_auto: project_rows.len() as u64 + 1, records: project_rows, views: project_views,
    });
    Doc::new(b).base
}

const ADJECTIVES: &[&str] = &["年度", "季度", "紧急", "常规", "试点", "海外", "华东", "线上", "内部", "重点"];
const NOUNS: &[&str] = &["采购单", "回访记录", "发货批次", "合同", "工单", "报销", "盘点", "排期", "询价", "验收"];
const NOTES: &[&str] = &["已和对方确认，等回复", "需要补充材料后重新提交", "按原计划推进", "金额有出入，待财务核对", "客户要求提前，已协调", "无"];

/// One table of `rows` records and 12 fields of the common types, filled
/// from a fixed seed: the same base every time (timing, stress tests).
pub fn large(rows: usize) -> Base {
    use FieldType::*;
    let mut b = Base::default();
    let id = b.new_id();
    let name = fld(&mut b, "名称", Text);
    let status = { let f = fld(&mut b, "状态", SingleSelect); choices(&mut b, f, &[("待处理", 9), ("处理中", 4), ("已完成", 2), ("已取消", 0), ("已归档", 7)]) };
    let tags = { let f = fld(&mut b, "标签", MultiSelect); choices(&mut b, f, &[("A", 0), ("B", 1), ("C", 2), ("D", 3), ("E", 4), ("F", 5), ("G", 6), ("H", 7)]) };
    let owner = { let f = fld(&mut b, "负责人", Person); choices(&mut b, f, PEOPLE) };
    let day = fld(&mut b, "日期", Date);
    let amount = fld(&mut b, "数量", Number);
    let price = fld(&mut b, "金额", Currency);
    let progress = fld(&mut b, "进度", Progress);
    let rating = fld(&mut b, "评分", Rating);
    let done = fld(&mut b, "完成", Checkbox);
    let url = fld(&mut b, "链接", Url);
    let note = fld(&mut b, "备注", Text);
    let view = View { id: b.new_id(), name: "表格".into(), kind: ViewKind::Grid, opts: ViewOpts { frozen: 1, ..Default::default() } };

    // xorshift
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut rnd = |n: usize| { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; (seed % n as u64) as usize };
    let epoch = field::date_ms(2026, 1, 1, 8, 0);
    let opt = |f: &Field, i: usize| f.opts.options[i % f.opts.options.len()].id;
    let mut records = Vec::with_capacity(rows);
    for i in 0..rows {
        let made = epoch + i as i64 * 60_000;
        let mut r = Record { id: i as u64 + 1, created: made, modified: made + rnd(86_400) as i64 * 1000, auto: i as u64 + 1, ..Default::default() };
        r.cells.reserve(12);
        r.set(name.id, Value::Text(format!("{}{} {}", ADJECTIVES[rnd(10)], NOUNS[rnd(10)], i + 1)));
        r.set(status.id, Value::Select(vec![opt(&status, rnd(5))]));
        let (a, c) = (rnd(8), rnd(8));
        r.set(tags.id, Value::Select(if a == c { vec![opt(&tags, a)] } else { vec![opt(&tags, a), opt(&tags, c)] }));
        r.set(owner.id, Value::Select(vec![opt(&owner, rnd(5))]));
        r.set(day.id, Value::Date(epoch - 8 * 3_600_000 + rnd(730) as i64 * DAY));
        r.set(amount.id, Value::Number(rnd(10_000) as f64));
        r.set(price.id, Value::Number(rnd(10_000_000) as f64 / 100.));
        r.set(progress.id, Value::Number(rnd(101) as f64 / 100.));
        r.set(rating.id, Value::Number(1. + rnd(5) as f64));
        r.set(done.id, Value::Bool(rnd(2) == 0));
        if rnd(4) == 0 { r.set(url.id, Value::Text(format!("https://example.com/item/{}", i + 1))); }
        if rnd(3) != 0 { r.set(note.id, text(NOTES[rnd(NOTES.len())])); }
        records.push(r);
    }
    b.tables.push(Table { id, name: "大表".into(), fields: vec![name, status, tags, owner, day, amount, price, progress, rating, done, url, note], next_rec: rows as u64 + 1, next_auto: rows as u64 + 1, records, views: vec![view] });
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::sqlite;

    const NOW: i64 = 1_795_000_000_000;

    #[test]
    fn demo_is_whole_and_the_same_every_time() {
        let b = demo(NOW);
        assert_eq!(b, demo(NOW));
        let (t, p) = (&b.tables[0], &b.tables[1]);
        assert_eq!((t.name.as_str(), t.records.len(), t.fields.len(), p.name.as_str(), p.records.len()), ("项目任务", 40, 18, "项目", 6));
        assert_eq!(t.views.iter().map(|v| v.kind).collect::<Vec<_>>(), [ViewKind::Grid, ViewKind::Grid, ViewKind::Kanban]);
        // every type the picker has but 附件
        for ty in FieldType::ALL.iter().filter(|ty| !matches!(ty, FieldType::Attachment | FieldType::Phone | FieldType::Email | FieldType::Location | FieldType::Barcode | FieldType::ModifiedTime)) {
            assert!(t.fields.iter().chain(&p.fields).any(|f| f.ty == *ty), "{ty:?}");
        }
        // names the file can carry, ids all different
        assert!(b.tables.iter().all(|t| crate::table::sqlite_name_ok(&t.name) && t.fields.iter().all(|f| crate::table::sqlite_name_ok(&f.name))));
        let mut ids: Vec<u32> = b.tables.iter().flat_map(|t| t.fields.iter().flat_map(|f| f.opts.options.iter().map(|o| o.id).chain([f.id])).chain(t.views.iter().map(|v| v.id)).chain([t.id])).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert!(ids.len() == n && ids.iter().all(|i| *i > 0 && *i <= b.next_id));
        // the two sides of the link agree, and the lookups computed
        let (link, back) = (t.field_named("所属项目").unwrap(), p.field_named("任务").unwrap());
        for r in &t.records {
            let Value::Links(l) = r.get(link.id) else { panic!("task {} has no project", r.id) };
            assert!(matches!(p.record(l[0]).unwrap().get(back.id), Value::Links(ids) if ids.contains(&r.id)));
            assert!(!r.get(t.field_named("项目负责人").unwrap().id).is_empty());
            assert_eq!(r.get(t.field_named("编号").unwrap().id), &Value::Number(r.auto as f64));
        }
        let counted: f64 = p.records.iter().filter_map(|r| r.get(p.field_named("任务数").unwrap().id).as_number()).sum();
        assert_eq!(counted, 40.);
        let summed: f64 = p.records.iter().filter_map(|r| r.get(p.field_named("预算合计").unwrap().id).as_number()).sum();
        assert_eq!(summed, t.records.iter().filter_map(|r| r.get(t.field_named("预算").unwrap().id).as_number()).sum::<f64>());
        // every status has tasks, some are late
        let status = t.field_named("状态").unwrap();
        for o in &status.opts.options { assert!(t.records.iter().any(|r| r.get(status.id) == &Value::Select(vec![o.id])), "{}", o.name); }
        assert!(t.records.iter().any(|r| r.get(t.field_named("是否延期").unwrap().id) == &Value::Bool(true)));
        // dates sit around `now`
        let due = t.field_named("截止日期").unwrap().id;
        assert!(t.records.iter().any(|r| matches!(r.get(due), Value::Date(d) if *d < NOW)) && t.records.iter().any(|r| matches!(r.get(due), Value::Date(d) if *d > NOW)));
    }

    #[test]
    fn large_is_the_size_asked() {
        let b = large(1000);
        assert_eq!(b, large(1000));
        assert_eq!((b.tables.len(), b.tables[0].records.len(), b.tables[0].fields.len(), b.tables[0].next_rec), (1, 1000, 12, 1001));
        assert_eq!(large(0).tables[0].records.len(), 0);
        assert_eq!(large(10).tables[0].records, b.tables[0].records[..10]);
    }

    #[test]
    fn samples_round_trip_through_the_file() {
        let dir = std::env::temp_dir().join(format!("quiet-md-table-sample-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (name, base) in [("demo.sqlite", demo(NOW)), ("large.sqlite", large(1000))] {
            let path = dir.join(name);
            sqlite::create(&path, &base).unwrap();
            assert_eq!(Doc::new(sqlite::load(&path).unwrap()).base, Doc::new(base).base);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
