//! 数据表 (多维表格) model: a `Base` is one SQLite file holding tables; a
//! table has typed fields, records and views. Ids are small integers that
//! never leave the file's bookkeeping tables: what a reader of the file sees
//! is the table, column and option names (`sqlite.rs`).

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

pub type TableId = u32;
pub type FieldId = u32;
pub type ViewId = u32;
pub type OptId = u32;
/// A record's id: its SQLite rowid.
pub type RecId = u64;

/// The field types, in 飞书's picker order within their groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Text, SingleSelect, MultiSelect, Person, Date, Attachment, Number, Checkbox, Url, Formula, Lookup,
    AutoNumber, Phone, Email, Location, Barcode, Progress, Currency, Rating,
    Link, CreatedTime, ModifiedTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldGroup { General, Business, Advanced }

impl FieldGroup {
    pub fn label(self) -> &'static str {
        match self { FieldGroup::General => "常规", FieldGroup::Business => "业务", FieldGroup::Advanced => "高级" }
    }
}

impl FieldType {
    pub const ALL: &'static [FieldType] = {
        use FieldType::*;
        &[Text, SingleSelect, MultiSelect, Person, Date, Attachment, Number, Checkbox, Url, Formula, Lookup,
          AutoNumber, Phone, Email, Location, Barcode, Progress, Currency, Rating,
          Link, CreatedTime, ModifiedTime]
    };

    pub fn label(self) -> &'static str {
        use FieldType::*;
        match self {
            Text => "文本", SingleSelect => "单选", MultiSelect => "多选", Person => "人员", Date => "日期",
            Attachment => "附件", Number => "数字", Checkbox => "复选框", Url => "超链接", Formula => "公式",
            Lookup => "查找引用", AutoNumber => "自动编号", Phone => "电话号码", Email => "Email",
            Location => "地理位置", Barcode => "条码", Progress => "进度", Currency => "货币", Rating => "评分",
            Link => "关联", CreatedTime => "创建时间", ModifiedTime => "最后更新时间",
        }
    }

    /// The name in the file's `_fields.type` column and in `.sqlite` exports.
    pub fn key(self) -> &'static str {
        use FieldType::*;
        match self {
            Text => "text", SingleSelect => "single_select", MultiSelect => "multi_select", Person => "person", Date => "date",
            Attachment => "attachment", Number => "number", Checkbox => "checkbox", Url => "url", Formula => "formula",
            Lookup => "lookup", AutoNumber => "auto_number", Phone => "phone", Email => "email",
            Location => "location", Barcode => "barcode", Progress => "progress", Currency => "currency", Rating => "rating",
            Link => "link", CreatedTime => "created_time", ModifiedTime => "modified_time",
        }
    }

    pub fn from_key(k: &str) -> Option<FieldType> { Self::ALL.iter().copied().find(|t| t.key() == k) }

    pub fn group(self) -> FieldGroup {
        use FieldType::*;
        match self {
            Text | SingleSelect | MultiSelect | Person | Date | Attachment | Number | Checkbox | Url | Formula | Lookup => FieldGroup::General,
            AutoNumber | Phone | Email | Location | Barcode | Progress | Currency | Rating => FieldGroup::Business,
            Link | CreatedTime | ModifiedTime => FieldGroup::Advanced,
        }
    }

    /// Computed from other cells or the record itself: never edited.
    pub fn computed(self) -> bool {
        matches!(self, FieldType::Formula | FieldType::Lookup | FieldType::AutoNumber | FieldType::CreatedTime | FieldType::ModifiedTime)
    }

    /// Values are numbers (right-aligned, summed, formatted by `NumberFmt`).
    pub fn numeric(self) -> bool {
        matches!(self, FieldType::Number | FieldType::Currency | FieldType::Progress | FieldType::Rating | FieldType::AutoNumber)
    }

    /// Values are option ids of the field.
    pub fn select(self) -> bool { matches!(self, FieldType::SingleSelect | FieldType::MultiSelect | FieldType::Person) }

    /// Values are one line of text typed in place.
    pub fn textual(self) -> bool {
        matches!(self, FieldType::Text | FieldType::Url | FieldType::Phone | FieldType::Email | FieldType::Location | FieldType::Barcode)
    }

    pub fn is_date(self) -> bool { matches!(self, FieldType::Date | FieldType::CreatedTime | FieldType::ModifiedTime) }
}

/// A 单选 / 多选 / 人员 option. `color` is a slot of the tag palette.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SelectOption {
    pub id: OptId,
    pub name: String,
    #[serde(default)]
    pub color: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumberStyle { #[default] Plain, Thousands, Percent }

/// 日期格式, 飞书's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateFmt {
    /// 2026/01/30
    #[default]
    Slash,
    /// 2026-01-30
    Dash,
    /// 01-30
    MonthDay,
    /// 01/30/2026
    Us,
    /// 30/01/2026
    Eu,
    /// 2026年1月30日
    Cn,
    /// 1月30日
    CnMonthDay,
}

impl DateFmt {
    pub const ALL: &'static [DateFmt] = &[DateFmt::Slash, DateFmt::Dash, DateFmt::MonthDay, DateFmt::Us, DateFmt::Eu, DateFmt::Cn, DateFmt::CnMonthDay];
    pub fn sample(self) -> &'static str {
        match self {
            DateFmt::Slash => "2026/01/30", DateFmt::Dash => "2026-01-30", DateFmt::MonthDay => "01-30", DateFmt::Us => "01/30/2026",
            DateFmt::Eu => "30/01/2026", DateFmt::Cn => "2026年1月30日", DateFmt::CnMonthDay => "1月30日",
        }
    }
}

/// How a 查找引用 folds the values it finds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rollup { #[default] Values, Unique, Sum, Average, Min, Max, Count, CountAll, Concat }

impl Rollup {
    pub const ALL: &'static [Rollup] = &[Rollup::Values, Rollup::Unique, Rollup::Sum, Rollup::Average, Rollup::Min, Rollup::Max, Rollup::Count, Rollup::CountAll, Rollup::Concat];
    pub fn label(self) -> &'static str {
        match self {
            Rollup::Values => "原样引用", Rollup::Unique => "去重", Rollup::Sum => "求和", Rollup::Average => "平均值", Rollup::Min => "最小值",
            Rollup::Max => "最大值", Rollup::Count => "计数（非空）", Rollup::CountAll => "计数", Rollup::Concat => "合并文本",
        }
    }
}

/// A field's settings. Flat, every member defaulted, so a new setting never
/// breaks an older file.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FieldOpts {
    /// 单选 / 多选 / 人员.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<SelectOption>,
    /// 数字 / 货币 / 进度 / 公式: digits after the point (None: as typed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precision: Option<u8>,
    #[serde(skip_serializing_if = "is_default")]
    pub style: NumberStyle,
    /// 货币 symbol (`¥`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub currency: String,
    #[serde(skip_serializing_if = "is_default")]
    pub date: DateFmt,
    /// 日期 shows the time of day too.
    #[serde(skip_serializing_if = "is_default")]
    pub time: bool,
    /// 评分's number of stars (0: five).
    #[serde(skip_serializing_if = "is_default")]
    pub max: u8,
    /// 公式.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub formula: String,
    /// 关联: the other table, and whether a cell may hold several records.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_table: Option<TableId>,
    #[serde(skip_serializing_if = "is_default")]
    pub single: bool,
    /// 双向关联: the mirror field in the other table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub back: Option<FieldId>,
    /// 查找引用: through which 关联 field, which field of the other table, folded how.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<FieldId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<FieldId>,
    #[serde(skip_serializing_if = "is_default")]
    pub rollup: Rollup,
    /// 自动编号 prefix.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub prefix: String,
    /// 新记录's value, as text the field parses (`field::parse`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub default: String,
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool { *v == T::default() }

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub id: FieldId,
    pub name: String,
    pub ty: FieldType,
    pub opts: FieldOpts,
    pub desc: String,
}

impl Field {
    pub fn option(&self, id: OptId) -> Option<&SelectOption> { self.opts.options.iter().find(|o| o.id == id) }
    pub fn option_named(&self, name: &str) -> Option<&SelectOption> { self.opts.options.iter().find(|o| o.name == name) }
    pub fn stars(&self) -> u8 { if self.opts.max == 0 { 5 } else { self.opts.max.min(10) } }
}

/// A file in an 附件 cell: a path relative to the library (`.assets/…`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub path: String,
    pub name: String,
}

/// One cell.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Value {
    #[default]
    Empty,
    Text(String),
    Number(f64),
    Bool(bool),
    /// Wall-clock milliseconds since 1970 (no time zone: what was typed is what shows).
    Date(i64),
    /// Option ids, in the order picked.
    Select(Vec<OptId>),
    /// Records of the 关联 field's table.
    Links(Vec<RecId>),
    Files(Vec<Attachment>),
    /// 查找引用 (原样引用): the values found, one per linked record.
    List(Vec<Value>),
    /// A formula that could not be computed (`#ERROR` and the like).
    Error(String),
}

impl Value {
    pub fn is_empty(&self) -> bool {
        match self {
            Value::Empty => true,
            Value::Text(s) => s.is_empty(),
            Value::Bool(b) => !b,
            Value::Select(v) => v.is_empty(),
            Value::Links(v) => v.is_empty(),
            Value::Files(v) => v.is_empty(),
            Value::List(v) => v.iter().all(Value::is_empty),
            Value::Number(_) | Value::Date(_) | Value::Error(_) => false,
        }
    }
    /// The same cell with empties spelled one way.
    pub fn normal(self) -> Value { if self.is_empty() { Value::Empty } else { self } }
    pub fn as_number(&self) -> Option<f64> {
        match self { Value::Number(n) => Some(*n), Value::Bool(b) => Some(*b as u8 as f64), _ => None }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Record {
    pub id: RecId,
    pub cells: HashMap<FieldId, Value>,
    /// Wall-clock milliseconds.
    pub created: i64,
    pub modified: i64,
    /// 自动编号's number: handed out once, never reused.
    pub auto: u64,
}

pub static EMPTY: Value = Value::Empty;

impl Record {
    pub fn get(&self, f: FieldId) -> &Value { self.cells.get(&f).unwrap_or(&EMPTY) }
    pub fn set(&mut self, f: FieldId, v: Value) { if v.is_empty() { self.cells.remove(&f); } else { self.cells.insert(f, v); } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowHeight { #[default] Short, Medium, Tall, ExtraTall }

impl RowHeight {
    pub const ALL: &'static [RowHeight] = &[RowHeight::Short, RowHeight::Medium, RowHeight::Tall, RowHeight::ExtraTall];
    pub fn px(self) -> f32 { match self { RowHeight::Short => 36., RowHeight::Medium => 44., RowHeight::Tall => 88., RowHeight::ExtraTall => 128. } }
    pub fn label(self) -> &'static str { match self { RowHeight::Short => "低", RowHeight::Medium => "中等", RowHeight::Tall => "高", RowHeight::ExtraTall => "超高" } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// A file's old 画册 view reads back as 表格 (an unknown kind is the default).
pub enum ViewKind { #[default] Grid, Kanban, Calendar, Gantt, Form }

impl ViewKind {
    pub const ALL: &'static [ViewKind] = &[ViewKind::Grid, ViewKind::Kanban, ViewKind::Calendar, ViewKind::Gantt, ViewKind::Form];
    pub fn label(self) -> &'static str {
        match self { ViewKind::Grid => "表格视图", ViewKind::Kanban => "看板视图", ViewKind::Calendar => "日历视图", ViewKind::Gantt => "甘特视图", ViewKind::Form => "表单视图" }
    }
    pub fn short(self) -> &'static str {
        match self { ViewKind::Grid => "表格", ViewKind::Kanban => "看板", ViewKind::Calendar => "日历", ViewKind::Gantt => "甘特图", ViewKind::Form => "表单" }
    }
}

/// 筛选 operators; which apply to a field is `query::ops_for`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOp {
    Is, IsNot, Contains, NotContains, Empty, NotEmpty,
    Gt, Ge, Lt, Le,
    /// 日期: 早于 / 晚于 (等于 is `Is`).
    Before, After,
}

impl FilterOp {
    pub fn label(self) -> &'static str {
        match self {
            FilterOp::Is => "等于", FilterOp::IsNot => "不等于", FilterOp::Contains => "包含", FilterOp::NotContains => "不包含",
            FilterOp::Empty => "为空", FilterOp::NotEmpty => "不为空", FilterOp::Gt => ">", FilterOp::Ge => "≥", FilterOp::Lt => "<", FilterOp::Le => "≤",
            FilterOp::Before => "早于", FilterOp::After => "晚于",
        }
    }
    pub fn needs_value(self) -> bool { !matches!(self, FilterOp::Empty | FilterOp::NotEmpty) }
}

/// A 日期 condition's value: a day, or one relative to today.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateRef {
    #[default]
    Exact, Today, Tomorrow, Yesterday, ThisWeek, LastWeek, NextWeek, ThisMonth, LastMonth, NextMonth, Past7, Next7, Past30, Next30,
}

impl DateRef {
    pub const ALL: &'static [DateRef] = &[DateRef::Exact, DateRef::Today, DateRef::Tomorrow, DateRef::Yesterday, DateRef::ThisWeek, DateRef::LastWeek,
        DateRef::NextWeek, DateRef::ThisMonth, DateRef::LastMonth, DateRef::NextMonth, DateRef::Past7, DateRef::Next7, DateRef::Past30, DateRef::Next30];
    pub fn label(self) -> &'static str {
        match self {
            DateRef::Exact => "具体日期", DateRef::Today => "今天", DateRef::Tomorrow => "明天", DateRef::Yesterday => "昨天",
            DateRef::ThisWeek => "本周", DateRef::LastWeek => "上周", DateRef::NextWeek => "下周",
            DateRef::ThisMonth => "本月", DateRef::LastMonth => "上月", DateRef::NextMonth => "下月",
            DateRef::Past7 => "过去 7 天内", DateRef::Next7 => "未来 7 天内", DateRef::Past30 => "过去 30 天内", DateRef::Next30 => "未来 30 天内",
        }
    }
}

/// One 筛选 condition. `value` is text the field parses (option names for
/// selects, joined with `,`); `date` picks a relative day instead.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cond {
    pub field: FieldId,
    pub op: FilterOp,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub date: DateRef,
}

/// 筛选: conditions joined by 且 (`all`) or 或.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Filter {
    pub all: bool,
    pub conds: Vec<Cond>,
}

impl Default for Filter {
    fn default() -> Self { Filter { all: true, conds: vec![] } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortBy {
    pub field: FieldId,
    #[serde(default)]
    pub desc: bool,
}

/// 统计 at the foot of a column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stat { Count, Filled, Empty, Unique, PercentFilled, PercentEmpty, Sum, Average, Min, Max, Earliest, Latest, Checked, Unchecked }

impl Stat {
    pub fn label(self) -> &'static str {
        match self {
            Stat::Count => "记录总数", Stat::Filled => "已填写", Stat::Empty => "未填写", Stat::Unique => "唯一值",
            Stat::PercentFilled => "已填写占比", Stat::PercentEmpty => "未填写占比", Stat::Sum => "求和", Stat::Average => "平均值",
            Stat::Min => "最小值", Stat::Max => "最大值", Stat::Earliest => "最早", Stat::Latest => "最晚",
            Stat::Checked => "已勾选", Stat::Unchecked => "未勾选",
        }
    }
}

/// 填色: rows matching `filter` take the palette slot `color`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ColorRule {
    pub filter: Filter,
    pub color: u8,
}

/// A view's settings, all of it saved (as one JSON object in `_views`).
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewOpts {
    /// Field order in this view; fields not listed follow in table order.
    pub order: Vec<FieldId>,
    pub hidden: HashSet<FieldId>,
    pub widths: BTreeMap<FieldId, f32>,
    /// Leading columns that stay put when scrolling sideways (at least the first).
    pub frozen: usize,
    pub row_height: RowHeight,
    pub filter: Filter,
    pub sorts: Vec<SortBy>,
    pub groups: Vec<SortBy>,
    pub stats: BTreeMap<FieldId, Stat>,
    pub colors: Vec<ColorRule>,
    /// 看板: the 单选 / 人员 field its columns come from.
    pub stack: Option<FieldId>,
    /// 看板: the 附件 field whose first image is the card's cover.
    pub cover: Option<FieldId>,
    /// 日历 / 甘特: start and end 日期 fields.
    pub date: Option<FieldId>,
    pub end: Option<FieldId>,
    /// Group headers folded shut, by their key (`query::Group::key`).
    pub folded: HashSet<String>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct View {
    pub id: ViewId,
    pub name: String,
    pub kind: ViewKind,
    pub opts: ViewOpts,
}

pub const DEFAULT_WIDTH: f32 = 180.;
pub const PRIMARY_WIDTH: f32 = 240.;
pub const MIN_WIDTH: f32 = 60.;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Table {
    pub id: TableId,
    pub name: String,
    /// The first field is the 标题 (primary) column: always there, always text-like.
    pub fields: Vec<Field>,
    /// In manual (drag) order.
    pub records: Vec<Record>,
    pub views: Vec<View>,
    pub next_rec: RecId,
    pub next_auto: u64,
}

impl Table {
    pub fn field(&self, id: FieldId) -> Option<&Field> { self.fields.iter().find(|f| f.id == id) }
    pub fn field_mut(&mut self, id: FieldId) -> Option<&mut Field> { self.fields.iter_mut().find(|f| f.id == id) }
    pub fn field_named(&self, name: &str) -> Option<&Field> { self.fields.iter().find(|f| f.name == name) }
    pub fn primary(&self) -> Option<&Field> { self.fields.first() }
    pub fn view(&self, id: ViewId) -> Option<&View> { self.views.iter().find(|v| v.id == id) }
    pub fn view_mut(&mut self, id: ViewId) -> Option<&mut View> { self.views.iter_mut().find(|v| v.id == id) }
    pub fn record(&self, id: RecId) -> Option<&Record> { self.records.iter().find(|r| r.id == id) }
    pub fn record_mut(&mut self, id: RecId) -> Option<&mut Record> { self.records.iter_mut().find(|r| r.id == id) }
    pub fn index_of(&self, id: RecId) -> Option<usize> { self.records.iter().position(|r| r.id == id) }

    /// The fields a view shows, in its order.
    pub fn visible_fields(&self, view: &View) -> Vec<&Field> {
        self.ordered_fields(view).into_iter().filter(|f| !view.opts.hidden.contains(&f.id)).collect()
    }

    /// Every field in the view's order (hidden ones too); the primary field leads.
    pub fn ordered_fields(&self, view: &View) -> Vec<&Field> {
        let mut out: Vec<&Field> = Vec::with_capacity(self.fields.len());
        let primary = self.fields.first().map(|f| f.id);
        out.extend(self.fields.first());
        for id in &view.opts.order {
            if Some(*id) != primary && !out.iter().any(|f| f.id == *id) && let Some(f) = self.field(*id) { out.push(f); }
        }
        for f in &self.fields { if !out.iter().any(|o| o.id == f.id) { out.push(f); } }
        out
    }

    pub fn width(&self, view: &View, f: FieldId) -> f32 {
        view.opts.widths.get(&f).copied().unwrap_or(if self.fields.first().is_some_and(|p| p.id == f) { PRIMARY_WIDTH } else { DEFAULT_WIDTH })
    }

    /// A record's title: its primary cell as text.
    pub fn title(&self, r: &Record) -> String {
        self.primary().map(|f| super::field::display(self, f, r.get(f.id))).unwrap_or_default()
    }

    /// A name no other field has (SQLite column names compare without case).
    pub fn free_field_name(&self, want: &str, except: Option<FieldId>) -> String {
        free_name(want, |n| super::sqlite_name_ok(n) && !self.fields.iter().any(|f| Some(f.id) != except && f.name.eq_ignore_ascii_case(n)))
    }

    pub fn free_view_name(&self, want: &str) -> String { free_name(want, |n| !self.views.iter().any(|v| v.name == n)) }
}

/// `want`, or `want 2`, `want 3`… until `ok`.
pub fn free_name(want: &str, ok: impl Fn(&str) -> bool) -> String {
    // what no file can carry as a name is dropped first (`sqlite_name_ok`), so a number always helps
    let want = want.trim().trim_start_matches('_').replace('\0', "");
    let want = match want.to_ascii_lowercase().as_str() { "" => "未命名".to_string(), l if l.starts_with("sqlite_") => format!("表 {want}"), _ => want };
    if ok(&want) { return want; }
    (2..100_000).map(|i| format!("{want} {i}")).find(|n| ok(n)).unwrap_or_else(|| format!("{want} {}", uuid::Uuid::new_v4()))
}

/// One file: its tables, in tab order.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Base {
    pub tables: Vec<Table>,
    pub next_id: u32,
}

impl Base {
    /// A fresh id for a table, field, view or option.
    pub fn new_id(&mut self) -> u32 { self.next_id += 1; self.next_id }
    pub fn table(&self, id: TableId) -> Option<&Table> { self.tables.iter().find(|t| t.id == id) }
    pub fn table_mut(&mut self, id: TableId) -> Option<&mut Table> { self.tables.iter_mut().find(|t| t.id == id) }
    pub fn table_named(&self, name: &str) -> Option<&Table> { self.tables.iter().find(|t| t.name == name) }
    pub fn free_table_name(&self, want: &str, except: Option<TableId>) -> String {
        free_name(want, |n| super::sqlite_name_ok(n) && !self.tables.iter().any(|t| Some(t.id) != except && t.name.eq_ignore_ascii_case(n)))
    }

    /// A blank table: five 文本 columns (文本 1 … 文本 5, the first is the title), five empty rows, one 表格 view.
    pub fn blank_table(&mut self, name: &str, now: i64) -> Table { self.blank_table_in(name, "文本", now) }

    /// `blank_table` with its columns called `column` 1 … 5 (the interface language's word for 文本).
    pub fn blank_table_in(&mut self, name: &str, column: &str, now: i64) -> Table {
        let name = self.free_table_name(name, None);
        let mut t = Table { id: self.new_id(), name, next_rec: 1, next_auto: 1, ..Default::default() };
        for i in 1..=5 {
            t.fields.push(Field { id: self.new_id(), name: format!("{column} {i}"), ty: FieldType::Text, opts: FieldOpts::default(), desc: String::new() });
        }
        for _ in 0..5 {
            t.records.push(Record { id: t.next_rec, created: now, modified: now, auto: t.next_auto, ..Default::default() });
            t.next_rec += 1;
            t.next_auto += 1;
        }
        t.views.push(View { id: self.new_id(), name: "表格视图".into(), kind: ViewKind::Grid, opts: ViewOpts { frozen: 1, ..Default::default() } });
        t
    }

    /// A new file's content: one blank table.
    pub fn blank(now: i64) -> Base { Base::blank_named(now, "未命名数据表", "文本") }

    /// `blank` with the table and its columns named in the interface language.
    pub fn blank_named(now: i64, name: &str, column: &str) -> Base {
        let mut b = Base::default();
        let t = b.blank_table_in(name, column, now);
        b.tables.push(t);
        b
    }
}
