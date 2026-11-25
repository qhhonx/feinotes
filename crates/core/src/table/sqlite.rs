//! The 数据表 file: a plain SQLite database in which every table is a SQL
//! table under its own name, with the columns and the values the app shows,
//! so `sqlite3` or an agent reads and edits it without knowing Quiet.md.
//! What only the app needs (field types, options, views) sits in four `_`
//! tables beside them. Each call opens the file, works and closes it: no
//! connection is held, so the file can be renamed or moved in between.

use super::csv::safe_name;
use super::field;
use super::model::*;
use super::ops::Dirty;
use anyhow::{Context, Result, bail};
use rusqlite::types::{ToSqlOutput, Value as Sql, ValueRef};
use rusqlite::{Connection, OpenFlags, Statement, TransactionBehavior, params, params_from_iter};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// `_meta.format`. 2: ids are random (no `next_id`, `next_rec`), `_order`
/// is a text sort key, a row's times, writers and 自动编号 are columns of
/// their own (`SYSTEM`). A format 1 file reads as ever and is written in 2.
pub const FORMAT: u32 = 2;

const README: &str = "\
Quiet.md 数据表。每张数据表就是一张同名的 SQL 表，列名就是字段名，值就是界面上看到的内容。
- 每张表开头的七列：`id` 是记录的编号（主键），一个随机整数（小于 2^53）；`_order` 是手动排序的排序键，按字符串比较从小到大就是行的顺序（相同时按 id）；`_created_at`、`_updated_at` 是这条记录的创建、最后修改时间，UTC 的 ISO 8601，如 2027-01-15T00:02:03.123Z（也认 2027-01-15 08:02:03+08:00 这样带时区的写法，不带时区按 UTC）；`_created_by`、`_updated_by` 是谁建的、谁最后改的（`user` 是在 Quiet.md 里改的，`agent:名字` 是 Agent 写的），空表示不知道；`_auto` 是自动编号。
- 表、字段、视图、选项的 id 也是随机整数，整个文件里不重复；新加的行不填 id 和 `_order` 也行，Quiet.md 读到时补上，排在最后；时间不填就按现在算，`_auto` 不填就发新号。
- 单选存选项名；多选、人员存选项名的 JSON 数组；关联存对方表记录 id 的 JSON 数组；附件存 {path, name} 的 JSON 数组；笔记存那篇笔记相对这个文件所在文件夹的路径（如 `三体.xml`、`../别处/笔记.xml`）；日期是 2026-11-17 或 2026-11-17 09:30；复选框是 1 / 0；空格子是 NULL。
- `_fields` 里 type 为 formula、lookup、auto_number、created_time、modified_time 的列是算出来的，直接改会被覆盖。
- 别的列可以直接改，也可以加行、删行、加列、加表：Quiet.md 下次读取时照单全收，没见过的选项名会加进选项。
- 选项的颜色、数字和日期的显示格式、公式、是否唯一键（`unique`，只是标记，不阻止重复）在 `_fields.options`；视图（筛选、排序、分组、列宽等）在 `_views.config`；表的顺序在 `_tables`。

A Quiet.md data table file. Every data table is a SQL table of the same name; its columns are the fields and its values are what the app shows.
- Seven leading columns: `id` is the record's id (primary key), a random integer below 2^53; `_order` is its sort key in manual order: the rows sorted by it as strings are in order (equal keys by id); `_created_at` and `_updated_at` are when the record was made and last changed, UTC ISO 8601 such as 2027-01-15T00:02:03.123Z (2027-01-15 08:02:03+08:00, with an offset, reads too; no zone means UTC); `_created_by` and `_updated_by` say who made it and who changed it last (`user`: in Quiet.md; `agent:<name>`: an agent), empty when not known; `_auto` is its auto number.
- Table, field, view and option ids are random integers too, each used once in the file. A row added without an id or `_order` is fine: Quiet.md fills them in when it reads it, and puts the row last; with no times it is taken as made now, with no `_auto` it gets a new number.
- A single select holds the option's name; multi selects and people hold a JSON array of names; links hold a JSON array of the other table's record ids; attachments hold a JSON array of {path, name}; a note holds the path of its note relative to the folder this file is in (`三体.xml`, `../elsewhere/note.xml`); dates read 2026-11-17 or 2026-11-17 09:30; checkboxes are 1 / 0; an empty cell is NULL.
- Columns whose `_fields.type` is formula, lookup, auto_number, created_time or modified_time are computed: edits to them are overwritten.
- Every other column can be edited directly, and rows, columns and tables added or removed: Quiet.md takes it all in the next time it reads the file, adding option names it has not seen.
- Option colours, number and date formats, formulas and whether a field is a unique key (`unique`: a mark, duplicates are not refused) are in `_fields.options`; views (filter, sort, group, widths…) in `_views.config`; the order of tables in `_tables`.";

/// Make the file (an error if it is there already) and write `base` to it.
pub fn create(path: &Path, base: &Base) -> Result<()> {
    if path.exists() { bail!("{} already exists", path.display()); }
    if let Some(dir) = path.parent() { std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?; }
    let done = (|| {
        let mut conn = open(path, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE)?;
        let tx = conn.transaction()?;
        write(&tx, base, None)?;
        Ok(tx.commit()?)
    })();
    if done.is_err() { let _ = std::fs::remove_file(path); }
    done
}

/// Write what `dirty` says changed, in one transaction. Nothing changed:
/// the file is not touched. (A file that went missing is written whole.)
pub fn save(path: &Path, base: &Base, dirty: &Dirty) -> Result<()> {
    if dirty.is_clean() { return Ok(()); }
    let mut conn = open(path, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    write(&tx, base, Some(dirty))?;
    Ok(tx.commit()?)
}

/// Something that changes whenever the file does, to notice another
/// program's edits: its size, its time, and the header's change counter and
/// schema cookie (which move with every commit, where the time may not).
pub fn stamp(path: &Path) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    use std::io::Read;
    let meta = std::fs::metadata(path).ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    meta.len().hash(&mut h);
    meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).hash(&mut h);
    let mut head = [0u8; 100];
    if let Ok(mut f) = std::fs::File::open(path) { let n = f.read(&mut head).unwrap_or(0); head[..n].hash(&mut h); }
    Some(h.finish())
}

/// Whether `path` is a 数据表 file: SQLite with the four `_` tables beside the data
/// (`shown::ours`); another program's database is not one.
pub fn is_table_file(path: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 16];
    let sqlite = std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut head)).is_ok() && &head == b"SQLite format 3\0";
    if !sqlite { return false }
    let Ok(conn) = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX) else { return false };
    conn.query_row("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name IN ('_tables', '_fields', '_views', '_meta')", [], |r| r.get::<_, i64>(0)).is_ok_and(|n| n == 4)
}

fn open(path: &Path, flags: OpenFlags) -> Result<Connection> {
    let conn = Connection::open_with_flags(path, flags | OpenFlags::SQLITE_OPEN_NO_MUTEX).with_context(|| format!("opening {}", path.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(2))?;
    // DELETE, not WAL: one file on disk, nothing left beside it for a sync folder to trip over
    let _ = conn.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=OFF;");
    Ok(conn)
}

/// `name` as an SQL identifier.
fn q(name: &str) -> String { format!("\"{}\"", name.replace('"', "\"\"")) }

/// The file's tables, oldest first.
fn tables(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    conn.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY rowid")?.query_map([], |r| r.get(0))?.collect()
}

/// A data table, not one of ours or SQLite's.
fn user_table(n: &str) -> bool { !n.starts_with('_') && !n.to_ascii_lowercase().starts_with("sqlite_") }

/// A query's rows; none when it fails (a table that isn't there, a column someone dropped).
fn grid(conn: &Connection, sql: &str) -> Vec<Vec<Sql>> {
    let Ok(mut stmt) = conn.prepare(sql) else { return vec![] };
    let n = stmt.column_count();
    let Ok(rows) = stmt.query_map([], |r| (0..n).map(|i| r.get::<_, Sql>(i)).collect::<rusqlite::Result<Vec<_>>>()) else { return vec![] };
    rows.filter_map(|r| r.ok()).collect()
}

fn int(v: &Sql) -> Option<i64> {
    match v { Sql::Integer(i) => Some(*i), Sql::Real(x) => Some(*x as i64), Sql::Text(s) => s.trim().parse().ok(), _ => None }
}

fn text(v: &Sql) -> String {
    match v { Sql::Text(s) => s.clone(), Sql::Integer(i) => i.to_string(), Sql::Real(x) => field::plain_number(*x), Sql::Blob(b) => String::from_utf8_lossy(b).into_owned(), Sql::Null => String::new() }
}

// ---- writing ----

/// The SQL names of `names`, in step with them: their own, except where one
/// can't be an identifier of ours or two would collide.
fn sql_names<'a>(names: impl Iterator<Item = &'a str>, fallback: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for n in names { let n = safe_name(n, fallback, |x| out.iter().any(|o| o.eq_ignore_ascii_case(x))); out.push(n); }
    out
}

fn table_names(base: &Base) -> Vec<String> { sql_names(base.tables.iter().map(|t| t.name.as_str()), "数据表") }
fn column_names(t: &Table) -> Vec<String> { sql_names(t.fields.iter().map(|f| f.name.as_str()), "字段") }

/// A column's declared type. Computed results are stored as they come, so 公式 / 查找引用 declare none.
fn decl(ty: FieldType) -> &'static str {
    match ty {
        FieldType::Formula | FieldType::Lookup => "",
        FieldType::Rating | FieldType::AutoNumber | FieldType::Checkbox => " INTEGER",
        t if t.numeric() => " NUMERIC",
        _ => " TEXT",
    }
}

fn number(n: f64) -> Sql {
    if n.is_nan() { Sql::Null } else if n == n.trunc() && n.abs() < 9e15 { Sql::Integer(n as i64) } else { Sql::Real(n) }
}

/// The days that read back from text (`field::parse_date` wants a four-digit year): 1000 to 9999.
const DATES: std::ops::RangeInclusive<i64> = -30_610_224_000_000..=253_402_300_799_999;

/// `2026-11-17`, `2026-11-17 09:30`, and with seconds (`09:30:15.250`) only when the value has them.
fn date_text(ms: i64) -> String {
    let sub = ms.rem_euclid(60_000);
    if sub == 0 { return field::iso(ms, false); }
    let s = format!("{}:{:02}", field::iso(ms - sub, true), sub / 1000);
    if sub % 1000 == 0 { s } else { format!("{s}.{:03}", sub % 1000) }
}

/// `field::parse_date`, keeping the seconds it leaves out; a bare integer is milliseconds.
fn text_date(s: &str) -> Option<i64> {
    let Some(ms) = field::parse_date(s) else { return s.trim().parse().ok() };
    let secs = s.trim().split_once([' ', 'T']).and_then(|(_, t)| t.trim().trim_end_matches('Z').split(':').nth(2)).and_then(|x| x.parse::<f64>().ok());
    Some(ms + secs.filter(|x| (0.0..60.).contains(x)).map_or(0, |x| (x * 1000.).round() as i64))
}

fn date(ms: i64) -> Sql { if DATES.contains(&ms) { Sql::Text(date_text(ms)) } else { Sql::Integer(ms) } }

/// A value inside a list: numbers as numbers, the rest as text.
fn json(v: &Value) -> serde_json::Value {
    use serde_json::Value as J;
    match v {
        Value::Number(n) => match number(*n) { Sql::Integer(i) => i.into(), _ => serde_json::Number::from_f64(*n).map_or(J::Null, J::Number) },
        Value::Bool(b) => (*b as i64).into(),
        Value::Date(ms) => match date(*ms) { Sql::Text(s) => s.into(), _ => (*ms).into() },
        Value::Text(s) | Value::Error(s) => s.as_str().into(),
        Value::List(vs) => J::Array(vs.iter().map(json).collect()),
        // `calc` turns these into text before they get into a list
        Value::Empty | Value::Select(_) | Value::Links(_) | Value::Files(_) => J::Null,
    }
}

/// A cell as the file holds it: by what the value is, so one in a field of
/// another type is still written.
fn cell<'a>(f: &'a Field, v: &'a Value) -> ToSqlOutput<'a> {
    let text = |s: &'a str| ToSqlOutput::Borrowed(ValueRef::Text(s.as_bytes()));
    let owned = |s: String| ToSqlOutput::Owned(Sql::Text(s));
    match v {
        // 0 reads better than a blank in a column of 1s
        Value::Empty if f.ty == FieldType::Checkbox => ToSqlOutput::Owned(Sql::Integer(0)),
        Value::Bool(b) => ToSqlOutput::Owned(Sql::Integer(*b as i64)),
        v if v.is_empty() => ToSqlOutput::Owned(Sql::Null),
        Value::Empty => ToSqlOutput::Owned(Sql::Null),
        Value::Text(s) | Value::Error(s) => text(s),
        Value::Number(n) => ToSqlOutput::Owned(number(*n)),
        Value::Date(ms) => ToSqlOutput::Owned(date(*ms)),
        Value::Select(ids) => {
            let names: Vec<&'a str> = ids.iter().filter_map(|id| f.option(*id)).map(|o| o.name.as_str()).collect();
            match names[..] {
                [] => ToSqlOutput::Owned(Sql::Null),
                // a name that looks like an array goes in one, to read back as itself
                [n] if f.ty == FieldType::SingleSelect && !n.starts_with('[') => text(n),
                _ => owned(serde_json::to_string(&names).unwrap_or_default()),
            }
        }
        Value::Links(ids) => owned(serde_json::to_string(ids).unwrap_or_default()),
        Value::Files(fs) => owned(serde_json::to_string(fs).unwrap_or_default()),
        Value::List(_) => owned(json(v).to_string()),
    }
}

/// The columns every table starts with, and how they are declared: the record's id,
/// sort key, times, writers and 自动编号. Then come the fields.
const SYSTEM: [(&str, &str); 7] = [("id", "INTEGER PRIMARY KEY"), ("_order", "TEXT"), ("_created_at", "TEXT"), ("_updated_at", "TEXT"), ("_created_by", "TEXT"), ("_updated_by", "TEXT"), ("_auto", "INTEGER")];

fn insert_sql(name: &str, fields: usize) -> String { format!("INSERT OR REPLACE INTO {} VALUES (?{})", q(name), ", ?".repeat(SYSTEM.len() - 1 + fields)) }

/// The local offset our wall-clock milliseconds carry (`field::now`), in milliseconds:
/// the file's times are UTC.
fn zone() -> i64 { crate::time::local_offset() * 1000 }

/// Write one record (through `insert_sql`'s statement).
fn put(stmt: &mut Statement, t: &Table, r: &Record, zone: i64) -> rusqlite::Result<()> {
    let by = |b: &Option<String>| b.clone().map_or(Sql::Null, Sql::Text);
    let head = [Sql::Integer(r.id as i64), Sql::Text(r.ord.clone()), Sql::Text(field::iso_utc(r.created - zone)), Sql::Text(field::iso_utc(r.modified - zone)),
        by(&r.created_by), by(&r.updated_by), Sql::Integer(r.auto.min(i64::MAX as u64) as i64)].map(ToSqlOutput::Owned);
    stmt.execute(params_from_iter(head.into_iter().chain(t.fields.iter().map(|f| cell(f, r.get(f.id))))))?;
    Ok(())
}

#[cfg(test)]
thread_local! { static WHOLE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

/// Drop and write a table whole.
fn write_table(conn: &Connection, t: &Table, name: &str) -> Result<()> {
    #[cfg(test)]
    WHOLE.with(|w| w.set(w.get() + 1));
    let n = q(name);
    let head: Vec<String> = SYSTEM.iter().map(|(c, decl)| format!("{} {decl}", q(c))).collect();
    let mut sql = format!("DROP TABLE IF EXISTS {n}; CREATE TABLE {n} ({}", head.join(", "));
    for (f, c) in t.fields.iter().zip(column_names(t)) { sql.push_str(&format!(", {}{}", q(&c), decl(f.ty))); }
    sql.push_str(");");
    conn.execute_batch(&sql)?;
    let mut stmt = conn.prepare(&insert_sql(name, t.fields.len()))?;
    let zone = zone();
    for r in &t.records { put(&mut stmt, t, r, zone)?; }
    Ok(())
}

/// Bring the file's rows of `t` up to `dirty`. False: the file's table isn't
/// what we left there (rows came or went), and it is to be written whole.
fn patch(conn: &Connection, t: &Table, name: &str, dirty: &Dirty) -> Result<bool> {
    let n = q(name);
    if let Some(gone) = dirty.removed.get(&t.id).filter(|g| !g.is_empty()) {
        let mut stmt = conn.prepare(&format!("DELETE FROM {n} WHERE \"id\" = ?"))?;
        for id in gone { stmt.execute([*id as i64])?; }
    }
    if let Some(rows) = dirty.rows.get(&t.id).filter(|r| !r.is_empty()) {
        let index: HashMap<RecId, usize> = t.records.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
        let mut stmt = conn.prepare(&insert_sql(name, t.fields.len()))?;
        let zone = zone();
        for id in rows { if let Some(&i) = index.get(id) { put(&mut stmt, t, &t.records[i], zone)?; } }
    }
    let count: i64 = conn.query_row(&format!("SELECT count(*) FROM {n}"), [], |r| r.get(0))?;
    Ok(count as usize == t.records.len())
}

/// A view's settings as JSON that is the same every time: the sets (`hidden`, `folded`) sorted.
fn view_config(o: &ViewOpts) -> String {
    use serde_json::Value as J;
    // through text, so widths read `180.5` and not the f32's digits as an f64
    let mut v: J = serde_json::to_string(o).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(J::Null);
    if let Some(J::Array(a)) = v.get_mut("hidden") { a.sort_by_key(|x| x.as_u64()); }
    if let Some(J::Array(a)) = v.get_mut("folded") { a.sort_by(|x, y| x.as_str().cmp(&y.as_str())); }
    v.to_string()
}

/// `_tables`, `_fields`, `_views`, `_meta`: small, so written afresh every time.
fn write_meta(conn: &Connection, base: &Base, names: &[String]) -> Result<()> {
    conn.execute_batch("
        DROP TABLE IF EXISTS _tables; DROP TABLE IF EXISTS _fields; DROP TABLE IF EXISTS _views; DROP TABLE IF EXISTS _meta;
        CREATE TABLE _tables (id INTEGER PRIMARY KEY, name TEXT, position INTEGER, next_auto INTEGER);
        CREATE TABLE _fields (id INTEGER PRIMARY KEY, table_id INTEGER, table_name TEXT, name TEXT, position INTEGER, type TEXT, options TEXT, description TEXT);
        CREATE TABLE _views (id INTEGER PRIMARY KEY, table_id INTEGER, table_name TEXT, name TEXT, position INTEGER, type TEXT, config TEXT);
        CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT);")?;
    // OR REPLACE: ids are unique, but a base where two aren't must still save (the loser is read back as a column nobody described)
    let mut table = conn.prepare("INSERT OR REPLACE INTO _tables VALUES (?, ?, ?, ?)")?;
    let mut field = conn.prepare("INSERT OR REPLACE INTO _fields VALUES (?, ?, ?, ?, ?, ?, ?, ?)")?;
    let mut view = conn.prepare("INSERT OR REPLACE INTO _views VALUES (?, ?, ?, ?, ?, ?, ?)")?;
    for (pos, (t, name)) in base.tables.iter().zip(names).enumerate() {
        table.execute(params![t.id as i64, name, pos as i64, t.next_auto as i64])?;
        for (i, (f, col)) in t.fields.iter().zip(column_names(t)).enumerate() {
            field.execute(params![f.id as i64, t.id as i64, name, col, i as i64, f.ty.key(), serde_json::to_string(&f.opts)?, f.desc])?;
        }
        for (i, v) in t.views.iter().enumerate() {
            let kind = serde_json::to_value(v.kind)?;
            view.execute(params![v.id as i64, t.id as i64, name, v.name, i as i64, kind.as_str().unwrap_or("grid"), view_config(&v.opts)])?;
        }
    }
    let mut meta = conn.prepare("INSERT INTO _meta VALUES (?, ?)")?;
    for (k, v) in [("format", FORMAT.to_string()), ("app", "Quiet.md".to_string()), ("readme", README.to_string())] { meta.execute(params![k, v])?; }
    Ok(())
}

/// `dirty` None: everything.
fn write(conn: &Connection, base: &Base, dirty: Option<&Dirty>) -> Result<()> {
    let names = table_names(base);
    let file = tables(conn)?;
    if let Some(d) = dirty {
        // a renamed table is here under its old name and in `schema` under the new one
        for n in d.dropped.iter().filter(|n| user_table(n)) {
            let kept = base.tables.iter().zip(&names).any(|(t, name)| name.eq_ignore_ascii_case(n) && !d.schema.contains(&t.id));
            if !kept { conn.execute_batch(&format!("DROP TABLE IF EXISTS {}", q(n)))?; }
        }
    }
    // tables `load` had to give another name are in `base` under that one
    for n in file.iter().filter(|n| user_table(n) && !super::sqlite_name_ok(n)) { conn.execute_batch(&format!("DROP TABLE IF EXISTS {}", q(n)))?; }
    for (t, name) in base.tables.iter().zip(&names) {
        let patched = match dirty {
            Some(d) if !d.schema.contains(&t.id) && file.iter().any(|n| n.eq_ignore_ascii_case(name)) => {
                // a table that isn't in our shape (another program's, one it altered, a format 1 table's
                // numbered `_order`, an older one's `_meta`) is rewritten in it
                let info = grid(conn, &format!("PRAGMA table_info({})", q(name)));
                let cols: Vec<String> = info.iter().map(|r| text(&r[1])).collect();
                cols.iter().map(String::as_str).eq(SYSTEM.iter().map(|c| c.0).chain(column_names(t).iter().map(String::as_str)))
                    && SYSTEM.iter().zip(&info).skip(1).all(|(c, r)| text(&r[2]).eq_ignore_ascii_case(c.1)) && patch(conn, t, name, d)?
            }
            _ => false,
        };
        if !patched { write_table(conn, t, name)?; }
    }
    write_meta(conn, base, &names)
}

// ---- reading ----

/// What the file says about a row besides its cells: its system columns, or
/// before them the `_meta` JSON (`c`, `m`: wall-clock milliseconds; `created`,
/// `modified`: the same to the minute, as text). The writers only have columns.
#[derive(Default, Deserialize)]
#[serde(default)]
struct RowMeta {
    c: Option<i64>, m: Option<i64>, auto: Option<u64>, created: Option<String>, modified: Option<String>,
    #[serde(skip)] by: (Option<String>, Option<String>),
    /// Its rowid when that was too big to keep (`ID_LIMIT`), else 0; and where it came in the read.
    #[serde(skip)] was: u64,
    #[serde(skip)] at: u64,
}

/// Every value of every table in the file, ours and SQLite's too, as text, a
/// line each: what the unused-asset sweep (#237) looks for a file's name in.
/// Read-only, and strict where `load` is lenient: a table that cannot be read
/// fails the whole read rather than hide the names it holds.
pub fn all_text(path: &Path) -> Result<String> {
    let conn = open(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let _ = conn.execute_batch("BEGIN");
    let mut out = String::new();
    for t in tables(&conn).with_context(|| format!("{} is not a SQLite database", path.display()))? {
        let mut stmt = conn.prepare(&format!("SELECT * FROM {}", q(&t)))?;
        let n = stmt.column_count();
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            for i in 0..n { out.push_str(&text(&r.get::<_, Sql>(i)?)); out.push('\n'); }
        }
    }
    Ok(out)
}

/// Give every 笔记 cell of the file the path `f` returns for it (None: as it
/// is): a note or the table itself was renamed or moved (`Store::relocate`).
/// Only the cells that change are written, the rows stamped as changed now, by
/// the app (`user`); a file with no 笔记 column is not written at all. Returns the cells changed.
pub fn rewrite_notes(path: &Path, f: impl Fn(&str) -> Option<String>) -> Result<usize> {
    let mut conn = open(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    let file = tables(&conn)?;
    if !file.iter().any(|t| t == "_fields") { return Ok(0); }
    let columns: Vec<(String, String)> = grid(&conn, "SELECT table_name, name FROM _fields WHERE type = 'note'").iter().map(|r| (text(&r[0]), text(&r[1]))).filter(|(t, _)| file.contains(t)).collect();
    let mut changes = vec![];
    for (t, c) in &columns {
        let Ok(mut stmt) = conn.prepare(&format!("SELECT rowid, {c} FROM {t} WHERE {c} IS NOT NULL", t = q(t), c = q(c))) else { continue };
        let cells: Vec<(i64, String)> = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Sql>(1)?)))?.filter_map(|r| r.ok()).map(|(id, v)| (id, text(&v))).collect();
        for (id, href) in cells { if let Some(new) = f(&href) { changes.push((t.clone(), c.clone(), id, new)); } }
    }
    if changes.is_empty() { return Ok(0); }
    let (now, by) = (field::iso_utc(crate::time::now_ms() as i64), Writer::default().0);
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for (t, c, id, new) in &changes {
        // when and by whom, where the table has the columns (ours); a table another program made has only the cell
        let stamped = tx.execute(&format!("UPDATE {} SET {} = ?, \"_updated_at\" = ?, \"_updated_by\" = ? WHERE rowid = ?", q(t), q(c)), params![new, now, by, id]);
        if stamped.is_err() { tx.execute(&format!("UPDATE {} SET {} = ? WHERE rowid = ?", q(t), q(c)), params![new, id])?; }
    }
    tx.commit()?;
    Ok(changes.len())
}

/// Read the whole file. Lenient: the file may have been edited by another
/// program, or never been ours — then every table that isn't SQLite's or
/// `_`-prefixed is a data table, with field types guessed from its columns.
/// The SQL tables and their columns are what counts; `_tables` / `_fields`
/// only describe them. Computed cells (公式, 查找引用…) come back empty:
/// `Doc::new` computes them.
pub fn load(path: &Path) -> Result<Base> { load_marked(path).map(|(base, _)| base) }

/// `load`, and the rows of our own tables that the file has to be told about
/// to be in step: a sort key made here (a format 1 file's row numbers, a row
/// another program added, two copies' same key), a record that needed an id.
pub fn load_marked(path: &Path) -> Result<(Base, Dirty)> {
    let conn = open(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    // one snapshot, whatever another program writes meanwhile
    let _ = conn.execute_batch("BEGIN");
    let file = tables(&conn).with_context(|| format!("{} is not a SQLite database", path.display()))?;
    let has = |n: &str| file.iter().any(|x| x == n);
    let described = has("_tables") && has("_fields");
    // the ids as the file has them (a negative or missing one: 0); `stable_ids` replaces those that can't stay
    let id = |v: &Sql| int(v).and_then(|i| u64::try_from(i).ok()).unwrap_or(0);
    // (the id in the file, which the fields and views point at; the table so far)
    let mut known: Vec<(Option<i64>, Table)> = if !described { vec![] } else {
        grid(&conn, "SELECT id, name, next_auto FROM _tables ORDER BY position, id").iter().map(|r| {
            (int(&r[0]), Table { id: id(&r[0]), name: text(&r[1]), next_auto: int(&r[2]).unwrap_or(0).max(0) as u64, ..Default::default() })
        }).collect()
    };
    for r in if described { grid(&conn, "SELECT table_id, id, name, type, options, description FROM _fields ORDER BY position, id") } else { vec![] } {
        let Some((_, t)) = known.iter_mut().find(|(tid, _)| tid.is_some() && *tid == int(&r[0])) else { continue };
        let opts: FieldOpts = serde_json::from_str(&text(&r[4])).unwrap_or_default();
        t.fields.push(Field { id: id(&r[1]), name: text(&r[2]), ty: FieldType::from_key(&text(&r[3])).unwrap_or(FieldType::Text), opts, desc: text(&r[5]) });
    }
    for r in if described { grid(&conn, "SELECT table_id, id, name, type, config FROM _views ORDER BY position, id") } else { vec![] } {
        let Some((_, t)) = known.iter_mut().find(|(tid, _)| tid.is_some() && *tid == int(&r[0])) else { continue };
        let kind = serde_json::from_value(serde_json::Value::String(text(&r[3]))).unwrap_or_default();
        let opts = serde_json::from_str(&text(&r[4])).unwrap_or_default();
        t.views.push(View { id: id(&r[1]), name: text(&r[2]), kind, opts });
    }
    let (mut kept, moved, replaced) = stable_ids(&mut known);
    if !moved.is_empty() {
        let to = |x: &mut Option<u64>| if let Some(n) = x.and_then(|x| moved.get(&x)) { *x = Some(*n); };
        for (_, t) in &mut known {
            for f in &mut t.fields { to(&mut f.opts.link_table); to(&mut f.opts.back); to(&mut f.opts.via); to(&mut f.opts.target); }
            for v in &mut t.views { super::ops::rename_fields(&mut v.opts, &moved); }
        }
    }
    // the described tables that are in the file, in their order, then the rest of the file's
    let mut rest: Vec<&String> = file.iter().filter(|n| user_table(n)).collect();
    let mut found: Vec<(&String, Table, bool)> = vec![];
    for (_, t) in known {
        let Some(k) = rest.iter().position(|n| **n == t.name).or_else(|| rest.iter().position(|n| n.eq_ignore_ascii_case(&t.name))) else { continue };
        found.push((rest.remove(k), t, true));
    }
    // a table nobody described gets a `stable_id` too (its records' are made from it)
    for (k, n) in rest.into_iter().enumerate() {
        let id = stable_id(IdKind::Table, 0, 0, (found.len() + k) as u64, |x| kept.contains(&x));
        kept.insert(id);
        found.push((n, Table { id, next_auto: 1, ..Default::default() }, false));
    }
    let mut ids = IdGen::default();
    ids.take(kept);
    let mut base = Base::default();
    // `_tables`, `_fields`, `_views` to write with the new ids
    let mut dirty = Dirty { meta: replaced, ..Default::default() };
    // per table, the records whose id was too big: the old id, the new
    let mut renamed: HashMap<TableId, HashMap<RecId, RecId>> = HashMap::new();
    let mut ours = HashSet::new();
    for i in 0..found.len() {
        // a name the app can carry, that no other table of the file has
        let name = safe_name(found[i].0, "数据表", |x| base.tables.iter().any(|t| t.name.eq_ignore_ascii_case(x)) || found[i + 1..].iter().any(|f| f.0.eq_ignore_ascii_case(x)));
        let (sql, t, described) = &mut found[i];
        t.name = name;
        if *described { ours.insert(t.id); }
        if let Some(t) = read_table(&conn, sql, std::mem::take(t), *described, &mut ids, &mut dirty, &mut renamed) { base.tables.push(t); }
    }
    // 关联 cells follow records that got a new id
    for t in &mut base.tables {
        let links: Vec<(FieldId, &HashMap<RecId, RecId>)> = t.fields.iter().filter(|f| f.ty == FieldType::Link).filter_map(|f| Some((f.id, renamed.get(&f.opts.link_table?)?))).collect();
        for r in &mut t.records {
            for (f, to) in &links {
                let Some(Value::Links(l)) = r.cells.get_mut(f) else { continue };
                if !l.iter().any(|id| to.contains_key(id)) { continue; }
                for id in l.iter_mut() { if let Some(n) = to.get(id) { *id = *n; } }
                if ours.contains(&t.id) { dirty.row(t.id, r.id); }
            }
        }
    }
    base.ids = ids;
    Ok((base, dirty))
}

/// Keep the ids of `known` (as read, tables first, then each table's fields with
/// their options, then its views) that are in range and not had before; give the
/// rest a `stable_id`. Returns the ids in use, the ids 2^53 or more with their
/// new ones (what pointed at them is to follow; a 0 or a duplicate means
/// nothing to point at), and whether any was replaced.
fn stable_ids(known: &mut [(Option<i64>, Table)]) -> (HashSet<u64>, HashMap<u64, u64>, bool) {
    let mut good: HashSet<u64> = known.iter().flat_map(|(_, t)| t.all_ids()).filter(|i| (1..ID_LIMIT).contains(i)).collect();
    let mut kept = HashSet::new();
    let mut moved = HashMap::new();
    let mut replaced = false;
    let mut fix = |slot: &mut u64, kind: IdKind, table: u64, seq: u64| {
        if (1..ID_LIMIT).contains(slot) && kept.insert(*slot) { return; }
        let new = stable_id(kind, *slot, table, seq, |x| good.contains(&x) || kept.contains(&x));
        kept.insert(new);
        good.insert(new);
        if *slot >= ID_LIMIT { moved.entry(*slot).or_insert(new); }
        *slot = new;
        replaced = true;
    };
    for (i, (_, t)) in known.iter_mut().enumerate() { fix(&mut t.id, IdKind::Table, 0, i as u64); }
    for (_, t) in known.iter_mut() {
        for (j, f) in t.fields.iter_mut().enumerate() {
            fix(&mut f.id, IdKind::Field, t.id, j as u64);
            for (k, o) in f.opts.options.iter_mut().enumerate() { fix(&mut o.id, IdKind::Option, t.id, (j as u64) << 20 | k as u64); }
        }
    }
    for (_, t) in known.iter_mut() { for (j, v) in t.views.iter_mut().enumerate() { fix(&mut v.id, IdKind::View, t.id, j as u64); } }
    (kept, moved, replaced)
}

/// A row's `_order`: a sort key (format 2), or a row number (format 1, another program's).
enum Place { Key(String), Row(f64) }

/// Guess the type of a column `_fields` doesn't describe: from its declared
/// type where the cells agree with it, otherwise from the cells.
fn guess(conn: &Connection, table: &str, col: &str, decl: &str) -> FieldType {
    let cells: Vec<String> = grid(conn, &format!("SELECT {c} FROM {table} WHERE {c} IS NOT NULL", c = q(col))).iter().map(|r| text(&r[0])).collect();
    if decl.contains("BOOL") && cells.iter().all(|s| s == "0" || s == "1") { return FieldType::Checkbox; }
    if ["INT", "REAL", "FLOA", "DOUB", "NUM", "DEC"].iter().any(|k| decl.contains(k)) && cells.iter().all(|s| field::parse_number(s).is_some()) { return FieldType::Number; }
    field::infer_type(cells.iter().map(String::as_str))
}

/// Fill `t` (its id, name, and the fields and views `_fields` / `_views`
/// describe) from the SQL table `sql`. None: the table can't be read at all.
/// Rows that got a sort key or an id here are marked in `dirty` (only in a
/// described table: another program's table is rewritten whole when written).
fn read_table(conn: &Connection, sql: &str, mut t: Table, described: bool, ids: &mut IdGen, dirty: &mut Dirty, renamed: &mut HashMap<TableId, HashMap<RecId, RecId>>) -> Option<Table> {
    let n = q(sql);
    // WITHOUT ROWID tables have no rowid: their records are numbered as they come
    let mut stmt = conn.prepare(&format!("SELECT rowid, * FROM {n}")).or_else(|_| conn.prepare(&format!("SELECT NULL, * FROM {n}"))).ok()?;
    let cols: Vec<String> = stmt.column_names().skip(1).map(str::to_string).collect();
    let info = grid(conn, &format!("PRAGMA table_xinfo({n})"));
    let keys = info.iter().filter(|r| int(&r[5]).is_some_and(|k| k > 0)).count();
    // `id INTEGER PRIMARY KEY` is the rowid; an `id` that is anything else is a column like any other
    let is_id = |c: &str| c.eq_ignore_ascii_case("id") && keys == 1 && info.iter().any(|r| text(&r[1]) == c && int(&r[5]) == Some(1) && text(&r[2]).eq_ignore_ascii_case("INTEGER"));
    let col = |name: &str| cols.iter().position(|c| c == name);
    let (order_at, meta_at) = (col("_order"), col("_meta"));
    let [made_at, changed_at, made_by, changed_by, auto_at] = ["_created_at", "_updated_at", "_created_by", "_updated_by", "_auto"].map(col);
    let system = [order_at, meta_at, made_at, changed_at, made_by, changed_by, auto_at];
    // the columns that are fields; `at` is each field's column
    let mut free: Vec<usize> = (0..cols.len()).filter(|i| !is_id(&cols[*i]) && !system.contains(&Some(*i))).collect();
    let mut at: Vec<Option<usize>> = vec![];
    for mut f in std::mem::take(&mut t.fields) {
        // described but not in the table: gone
        let Some(k) = free.iter().position(|i| cols[*i] == f.name).or_else(|| free.iter().position(|i| cols[*i].eq_ignore_ascii_case(&f.name))) else { continue };
        f.name = cols[free[k]].clone();
        at.push(Some(free.remove(k)));
        t.fields.push(f);
    }
    // in the table but not described: new fields, after the others
    let guessed = t.fields.is_empty();
    for (k, &i) in free.iter().enumerate() {
        let name = safe_name(&cols[i], "字段", |x| t.fields.iter().any(|f| f.name.eq_ignore_ascii_case(x)) || free[k + 1..].iter().any(|j| cols[*j].eq_ignore_ascii_case(x)));
        let decl = info.iter().find(|r| text(&r[1]) == cols[i]).map(|r| text(&r[2]).to_ascii_uppercase()).unwrap_or_default();
        let ty = guess(conn, &n, &cols[i], &decl);
        // an id every reader of the file gives it (`stable_id`), like the rest of what is made here
        t.fields.push(Field { id: ids.stable(IdKind::Field, 0, t.id, i as u64), name, ty, opts: field::default_opts(ty), desc: String::new() });
        at.push(Some(i));
    }
    // the first field is the title
    if guessed && let Some(f) = t.fields.first_mut().filter(|f| matches!(f.ty, FieldType::SingleSelect | FieldType::Checkbox)) { f.ty = FieldType::Text; }
    if t.fields.is_empty() {
        t.fields.push(Field { id: ids.stable(IdKind::Field, 0, t.id, cols.len() as u64), name: "文本".into(), ty: FieldType::Text, opts: FieldOpts::default(), desc: String::new() });
        at.push(None);
    }

    // (its `_order`, whether it came with an id, the record, what its `_meta` said)
    let mut recs: Vec<(Option<Place>, bool, Record, RowMeta)> = vec![];
    let zone = zone();
    let mut rows = stmt.query([]).ok()?;
    while let Ok(Some(row)) = rows.next() {
        // a rowid of 2^53 or more is replaced like a missing one (and remembered, for the links to it)
        let (id, was) = match row.get_ref(0) { Ok(ValueRef::Integer(i)) if i > 0 => if (i as u64) < ID_LIMIT { (Some(i as u64), 0) } else { (None, i as u64) }, _ => (None, 0) };
        let order = order_at.and_then(|c| match row.get_ref(c + 1) {
            Ok(ValueRef::Text(b)) => Some(Place::Key(String::from_utf8_lossy(b).into_owned())),
            Ok(ValueRef::Integer(i)) => Some(Place::Row(i as f64)),
            Ok(ValueRef::Real(x)) => Some(Place::Row(x)),
            _ => None,
        });
        let mut meta: RowMeta = meta_at.and_then(|c| match row.get_ref(c + 1) { Ok(ValueRef::Text(b)) => serde_json::from_slice(b).ok(), _ => None }).unwrap_or_default();
        // the columns: what isn't there or doesn't read stays unknown
        let get = |at: Option<usize>| at.and_then(|c| row.get_ref(c + 1).ok());
        let words = |at| match get(at) { Some(ValueRef::Text(b)) => Some(String::from_utf8_lossy(b).trim().to_string()).filter(|s| !s.is_empty()), _ => None };
        let time = |at| words(at).and_then(|s| field::parse_iso(&s)).map(|ms| ms + zone);
        if let Some(c) = time(made_at) { meta.c = Some(c); }
        if let Some(m) = time(changed_at) { meta.m = Some(m); }
        meta.by = (words(made_by), words(changed_by));
        match get(auto_at) {
            Some(ValueRef::Integer(i)) if i >= 0 => meta.auto = Some(i as u64),
            Some(ValueRef::Real(x)) if x >= 0. && x.fract() == 0. => meta.auto = Some(x as u64),
            Some(ValueRef::Text(b)) => if let Ok(i) = std::str::from_utf8(b).unwrap_or("").trim().parse::<u64>() { meta.auto = Some(i) },
            _ => {}
        }
        let mut r = Record { id: id.unwrap_or(0), ..Default::default() };
        for (f, c) in t.fields.iter_mut().zip(&at) {
            let Some(Ok(v)) = c.map(|c| row.get_ref(c + 1)) else { continue };
            match (f.ty, read(f, v, ids)) {
                (_, Value::Empty) => {}
                // what the record itself knows is in `_meta`; a row someone else wrote has only the cell
                (FieldType::AutoNumber, Value::Number(n)) => { meta.auto.get_or_insert(n as u64); }
                (FieldType::CreatedTime, Value::Date(d)) => { meta.c.get_or_insert(d); }
                (FieldType::ModifiedTime, Value::Date(d)) => { meta.m.get_or_insert(d); }
                (FieldType::AutoNumber | FieldType::CreatedTime | FieldType::ModifiedTime, _) => {}
                (_, v) => { r.cells.insert(f.id, v); }
            }
        }
        (meta.was, meta.at) = (was, recs.len() as u64);
        recs.push((order, id.is_some(), r, meta));
    }
    drop(rows);
    // rows another program added have no `_order`: they go last (those without a rowid as they
    // came). Row numbers (format 1) are that order and nothing more: every row gets a key made afresh
    let id = |r: &(Option<Place>, bool, Record, RowMeta)| if r.1 { r.2.id } else { RecId::MAX };
    if recs.iter().any(|r| matches!(r.0, Some(Place::Row(_)))) {
        let row = |p: &Option<Place>| match p { Some(Place::Row(x)) => *x, _ => f64::INFINITY };
        recs.sort_by(|a, b| row(&a.0).total_cmp(&row(&b.0)).then(id(a).cmp(&id(b))));
    } else {
        for r in &mut recs { if let Some(Place::Key(k)) = r.0.take() { r.2.ord = k; } }
        recs.sort_by(|a, b| (a.2.ord.is_empty(), &a.2.ord, id(a)).cmp(&(b.2.ord.is_empty(), &b.2.ord, id(b))));
    }
    // a row without a rowid we can keep (a WITHOUT ROWID table, a 0, 2^53 or more) gets an id no
    // other row of the table has: a `stable_id`, the same wherever the file is read
    let mut have: HashSet<RecId> = recs.iter().filter(|r| r.1).map(|r| r.2.id).collect();
    let mut marked = vec![];
    for r in recs.iter_mut().filter(|r| !r.1) {
        r.2.id = stable_id(IdKind::Record, r.3.was, t.id, r.3.at, |id| have.contains(&id));
        have.insert(r.2.id);
        marked.push(r.2.id);
        if r.3.was != 0 { renamed.entry(t.id).or_default().insert(r.3.was, r.2.id); }
    }
    t.next_auto = t.next_auto.max(recs.iter().filter_map(|r| r.3.auto).max().map_or(0, |a| a + 1)).max(if described { 0 } else { 1 });
    let now = field::now();
    // `_meta` (format 1, and 2 before the system columns): the table is written again in today's shape
    if described && meta_at.is_some() { dirty.schema.insert(t.id); }
    for (_, _, mut r, meta) in recs {
        let c = meta.c.or_else(|| meta.created.as_deref().and_then(field::parse_date));
        let m = meta.m.or_else(|| meta.modified.as_deref().and_then(field::parse_date));
        r.created = c.or(m).unwrap_or(now);
        r.modified = m.unwrap_or(r.created);
        r.auto = meta.auto.unwrap_or_else(|| { t.next_auto += 1; t.next_auto - 1 });
        (r.created_by, r.updated_by) = meta.by;
        t.records.push(r);
    }
    marked.extend(super::order::repair(&mut t.records));
    if described { for id in marked { dirty.row(t.id, id); } }
    if t.views.is_empty() {
        t.views.push(View { id: ids.stable(IdKind::View, 0, t.id, 0), name: "表格视图".into(), kind: ViewKind::Grid, opts: ViewOpts::default() });
    }
    Some(t)
}

/// A cell of the file as a value of `f`, converting what can be. An option
/// name the field doesn't have is added to it. What a text field can't make
/// sense of stays as text; other fields let it go.
fn read(f: &mut Field, v: ValueRef, ids: &mut IdGen) -> Value {
    use FieldType::*;
    let string = |v: ValueRef| match v {
        ValueRef::Text(b) | ValueRef::Blob(b) => String::from_utf8_lossy(b).into_owned(),
        ValueRef::Integer(i) => i.to_string(),
        ValueRef::Real(x) => field::plain_number(x),
        ValueRef::Null => String::new(),
    };
    match (f.ty, v) {
        (_, ValueRef::Null) | (Formula | Lookup, _) => Value::Empty,
        (t, v) if t.textual() => Value::Text(string(v)).normal(),
        (_, ValueRef::Blob(_)) => Value::Empty,
        (Note, v) => Value::Text(string(v).trim().to_string()).normal(),
        (Checkbox, ValueRef::Integer(i)) => Value::Bool(i != 0).normal(),
        (Checkbox, ValueRef::Real(x)) => Value::Bool(x != 0.).normal(),
        (Checkbox, v) => field::parse(f, &string(v)),
        (t, ValueRef::Integer(i)) if t.numeric() => Value::Number(i as f64),
        (t, ValueRef::Real(x)) if t.numeric() => Value::Number(x),
        (t, v) if t.numeric() => field::parse_number(&string(v)).map(Value::Number).unwrap_or(Value::Empty),
        (t, ValueRef::Integer(i)) if t.is_date() => Value::Date(i),
        (t, ValueRef::Real(x)) if t.is_date() => Value::Date(x as i64),
        (t, v) if t.is_date() => text_date(&string(v)).map(Value::Date).unwrap_or(Value::Empty),
        (SingleSelect | MultiSelect | Person, v) => {
            let s = string(v);
            // a single select's cell is the name itself, unless it was written as an array (see `cell`)
            let names = if f.ty == SingleSelect && !(s.starts_with('[') && serde_json::from_str::<Vec<serde_json::Value>>(&s).is_ok()) { vec![s] } else { field::split_list(&s) };
            let ids = names.iter().filter(|n| !n.is_empty()).map(|n| match f.option_named(n) {
                Some(o) => o.id,
                None => {
                    // the field's id where the others have their table's: an option is its field's
                    let (id, color) = (ids.stable(IdKind::Option, 0, f.id, f.opts.options.len() as u64), (f.opts.options.len() % 10) as u8);
                    f.opts.options.push(SelectOption { id, name: n.clone(), color });
                    id
                }
            }).collect();
            Value::Select(ids).normal()
        }
        (Link, ValueRef::Integer(i)) => u64::try_from(i).map(|i| Value::Links(vec![i])).unwrap_or(Value::Empty),
        (Link, v) => { let s = string(v); serde_json::from_str::<Vec<RecId>>(&s).map(|ids| Value::Links(ids).normal()).unwrap_or_else(|_| field::parse(f, &s)) }
        (Attachment, v) => { let s = string(v); serde_json::from_str::<Vec<super::model::Attachment>>(&s).map(|fs| Value::Files(fs).normal()).unwrap_or_else(|_| field::parse(f, &s)) }
        _ => Value::Empty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::ops::{BaseOp, Doc, Op};
    use crate::table::order;
    use std::path::PathBuf;

    /// A directory of its own under the system's, gone when the test is.
    struct Tmp(PathBuf);
    impl Tmp {
        fn new(tag: &str) -> Tmp {
            static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!("quiet-md-table-{}-{}-{tag}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Tmp(dir)
        }
        fn file(&self, name: &str) -> PathBuf { self.0.join(name) }
    }
    impl Drop for Tmp { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

    /// The file as the app has it after opening.
    fn reload(path: &Path) -> Base { Doc::new(load(path).unwrap()).base }

    /// Every table of a file as text, tables by name and rows by id.
    fn dump(path: &Path) -> String {
        let conn = Connection::open(path).unwrap();
        let mut out = String::new();
        for t in grid(&conn, "SELECT name, sql FROM sqlite_master WHERE type='table' ORDER BY name") {
            out.push_str(&format!("== {}\n{}\n", text(&t[0]), text(&t[1])));
            for row in grid(&conn, &format!("SELECT * FROM {} ORDER BY rowid", q(&text(&t[0])))) { out.push_str(&format!("{row:?}\n")); }
        }
        out
    }

    fn fld(b: &mut Base, name: &str, ty: FieldType) -> Field { Field { id: b.new_id(), name: name.into(), ty, opts: field::default_opts(ty), desc: String::new() } }

    fn choices(b: &mut Base, mut f: Field, names: &[&str]) -> Field {
        for (i, n) in names.iter().enumerate() { f.opts.options.push(SelectOption { id: b.new_id(), name: n.to_string(), color: (i * 3 % 10) as u8 }); }
        f
    }

    const T0: i64 = 1_800_000_123_456;

    /// Two linked tables with every field type, every kind of value and views with every setting.
    fn library() -> Base {
        use FieldType::*;
        let mut b = Base::default();
        let (books, authors) = (b.new_id(), b.new_id());
        let name = fld(&mut b, "姓名", Text);
        let mut works = fld(&mut b, "作品", Link);
        let mut title = fld(&mut b, "书名", Text);
        title.opts.unique = true;
        let state = { let f = fld(&mut b, "状态", SingleSelect); choices(&mut b, f, &["想读", "在读", "读完", "[暂存]"]) };
        let tags = { let f = fld(&mut b, "标签", MultiSelect); choices(&mut b, f, &["小说", "科幻", "历史, 传记"]) };
        let reader = { let f = fld(&mut b, "读者", Person); choices(&mut b, f, &["小明", "小红"]) };
        let mut start = fld(&mut b, "开始", Date);
        start.opts.time = true;
        start.opts.date = DateFmt::Cn;
        let cover = fld(&mut b, "封面", Attachment);
        let mut pages = fld(&mut b, "页数", Number);
        pages.opts.precision = Some(0);
        pages.opts.style = NumberStyle::Thousands;
        pages.desc = "不含附录".into();
        let done = fld(&mut b, "读完", Checkbox);
        let url = fld(&mut b, "链接", Url);
        let mut twice = fld(&mut b, "两倍页数", Formula);
        twice.opts.formula = "{页数}*2".into();
        let mut who = fld(&mut b, "作者信息", Lookup);
        let mut no = fld(&mut b, "编号", AutoNumber);
        no.opts.prefix = "B-".into();
        let phone = fld(&mut b, "电话", Phone);
        let email = fld(&mut b, "邮箱", Email);
        let place = fld(&mut b, "地点", Location);
        let barcode = fld(&mut b, "条码", Barcode);
        let progress = fld(&mut b, "进度", Progress);
        let price = fld(&mut b, "价格", Currency);
        let rating = fld(&mut b, "评分", Rating);
        let mut author = fld(&mut b, "作者", Link);
        let created = fld(&mut b, "创建", CreatedTime);
        let modified = fld(&mut b, "更新", ModifiedTime);
        let broken = fld(&mut b, "失效引用", Lookup);
        author.opts.link_table = Some(authors);
        author.opts.back = Some(works.id);
        works.opts.link_table = Some(books);
        works.opts.back = Some(author.id);
        who.opts.via = Some(author.id);
        who.opts.target = Some(name.id);
        let lookup = |b: &mut Base, n: &str, target: FieldId, rollup: Rollup| { let mut f = fld(b, n, Lookup); f.opts.via = Some(works.id); f.opts.target = Some(target); f.opts.rollup = rollup; f };
        let count = lookup(&mut b, "作品数", title.id, Rollup::Count);
        let titles = lookup(&mut b, "作品名", title.id, Rollup::Values);
        let total = lookup(&mut b, "页数合计", pages.id, Rollup::Sum);
        let first = lookup(&mut b, "最早开始", start.id, Rollup::Min);

        let rec = |id: RecId, cells: Vec<(FieldId, Value)>| Record { id, cells: cells.into_iter().collect(), created: T0 + id as i64 * 1000, modified: T0 + id as i64 * 77_777, auto: id + 10, ord: String::new(),
            // made in the app, changed by an agent; not known
            created_by: (!id.is_multiple_of(3)).then(|| "user".into()), updated_by: (!id.is_multiple_of(3)).then(|| format!("agent:读书 {id}")) };
        let pick = |f: &Field, names: &[&str]| Value::Select(names.iter().map(|n| f.option_named(n).unwrap().id).collect());
        let file = |n: &str| super::super::model::Attachment { path: format!(".assets/{n}"), name: n.to_string() };
        let text = |s: &str| Value::Text(s.to_string());
        let book_rows = vec![
            rec(1, vec![(title.id, text("三体")), (state.id, pick(&state, &["读完"])), (tags.id, pick(&tags, &["小说", "科幻"])), (reader.id, pick(&reader, &["小红", "小明"])),
                (start.id, Value::Date(field::date_ms(2026, 11, 17, 9, 30))), (cover.id, Value::Files(vec![file("a.png"), file("带 空格.jpg")])), (pages.id, Value::Number(302.)),
                (done.id, Value::Bool(true)), (url.id, text("https://example.com/三体")), (phone.id, text("010-1234")), (email.id, text("a@b.c")), (place.id, text("北京")),
                (barcode.id, text("9787536692930")), (progress.id, Value::Number(0.5)), (price.id, Value::Number(23.5)), (rating.id, Value::Number(4.)), (author.id, Value::Links(vec![1]))]),
            rec(2, vec![(title.id, text("球状闪电")), (state.id, pick(&state, &["[暂存]"])), (tags.id, pick(&tags, &["历史, 传记"])), (start.id, Value::Date(field::date_ms(1969, 12, 31, 0, 0))),
                (pages.id, Value::Number(-1.5e300)), (price.id, Value::Number(1e20)), (progress.id, Value::Number(1. / 3.)), (author.id, Value::Links(vec![1, 2]))]),
            rec(3, vec![(title.id, text("带\"引号\"\n和换行 'x' [1]")), (start.id, Value::Date(field::date_ms(2026, 1, 2, 3, 4) + 5_678)), (pages.id, Value::Number(0.)), (barcode.id, text("0012")),
                (state.id, pick(&state, &["想读"])), (reader.id, pick(&reader, &["小明"]))]),
            rec(7, vec![]),
        ];
        let author_rows = vec![
            rec(1, vec![(name.id, text("刘慈欣")), (works.id, Value::Links(vec![1, 2]))]),
            rec(2, vec![(name.id, text("佚名")), (works.id, Value::Links(vec![2]))]),
            rec(3, vec![]),
        ];
        let grid = |b: &mut Base, name: &str, kind: ViewKind, opts: ViewOpts| View { id: b.new_id(), name: name.into(), kind, opts };
        let cond = |f: &Field, op: FilterOp, value: &str, date: DateRef| Cond { field: f.id, op, value: value.into(), date };
        let busy = ViewOpts {
            order: vec![pages.id, state.id, title.id],
            hidden: [cover.id, url.id, phone.id, email.id, place.id].into_iter().collect(),
            widths: [(title.id, 300.5), (pages.id, 88.1), (state.id, 120.)].into_iter().collect(),
            frozen: 2,
            row_height: RowHeight::Tall,
            filter: Filter { all: false, conds: vec![cond(&state, FilterOp::Is, "在读", DateRef::Exact), cond(&start, FilterOp::After, "", DateRef::ThisMonth), cond(&pages, FilterOp::Gt, "100", DateRef::Exact)] },
            sorts: vec![SortBy { field: pages.id, desc: true }, SortBy { field: title.id, desc: false }],
            groups: vec![SortBy { field: state.id, desc: false }],
            stats: [(pages.id, Stat::Sum), (title.id, Stat::Count)].into_iter().collect(),
            colors: vec![ColorRule { filter: Filter { all: true, conds: vec![cond(&done, FilterOp::Is, "true", DateRef::Exact)] }, color: 3 }],
            folded: ["读完".to_string(), "想读\u{1f}x".to_string(), String::new()].into_iter().collect(),
            ..Default::default()
        };
        let views = vec![
            grid(&mut b, "全部", ViewKind::Grid, ViewOpts { frozen: 1, ..Default::default() }),
            grid(&mut b, "在读", ViewKind::Grid, busy),
            grid(&mut b, "看板", ViewKind::Kanban, ViewOpts { stack: Some(state.id), cover: Some(cover.id), ..Default::default() }),
            grid(&mut b, "日历", ViewKind::Calendar, ViewOpts { date: Some(start.id), ..Default::default() }),
            grid(&mut b, "甘特", ViewKind::Gantt, ViewOpts { date: Some(start.id), end: Some(start.id), ..Default::default() }),
            grid(&mut b, "表单", ViewKind::Form, ViewOpts::default()),
        ];
        let author_views = vec![grid(&mut b, "表格", ViewKind::Grid, ViewOpts { frozen: 1, ..Default::default() })];
        b.tables.push(Table {
            id: books, name: "读书清单".into(),
            fields: vec![title, state, tags, reader, start, cover, pages, done, url, twice, who, no, phone, email, place, barcode, progress, price, rating, author, created, modified, broken],
            records: book_rows, views, next_auto: 30,
        });
        b.tables.push(Table { id: authors, name: "作者".into(), fields: vec![name, works, count, titles, total, first], records: author_rows, views: author_views, next_auto: 14 });
        for t in &mut b.tables { crate::table::order::repair(&mut t.records); }
        Doc::new(b).base
    }

    #[test]
    fn every_type_round_trips() {
        let dir = Tmp::new("types");
        let path = dir.file("deep/er/读书清单.sqlite");
        let base = library();
        // the computed columns did compute: a list, a number, a date, an error
        let a = &base.tables[1];
        assert_eq!(a.records[0].get(a.fields[3].id), &Value::List(vec![Value::Text("三体".into()), Value::Text("球状闪电".into())]));
        assert_eq!(a.records[0].get(a.fields[2].id), &Value::Number(2.));
        assert_eq!(a.records[0].get(a.fields[5].id), &Value::Date(field::date_ms(1969, 12, 31, 0, 0)));
        assert_eq!(base.tables[0].records[0].get(base.tables[0].fields[22].id), &Value::Error(crate::table::calc::BAD_REF.into()));
        create(&path, &base).unwrap();
        assert_eq!(reload(&path), base);
        // what `load` itself returns differs only in the computed cells
        let raw = load(&path).unwrap();
        assert!(raw.all_ids().eq(base.all_ids()));
        assert!(raw.tables[0].records[0].get(base.tables[0].fields[11].id).is_empty());
        assert!(create(&path, &base).is_err());
        assert!(load(&dir.file("missing.sqlite")).is_err());
        std::fs::write(dir.file("not.sqlite"), "this is not a database, just some text that is long enough to have a header").unwrap();
        assert!(load(&dir.file("not.sqlite")).is_err());
        // an empty base is a file too
        let empty = dir.file("empty.sqlite");
        create(&empty, &Base::default()).unwrap();
        assert_eq!(load(&empty).unwrap(), Base::default());
        // no journal is left beside the file
        assert_eq!(std::fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn the_file_reads_as_the_app_shows() {
        let dir = Tmp::new("readable");
        let path = dir.file("a.sqlite");
        let base = library();
        let base_ids: Vec<i64> = base.tables.iter().map(|t| t.id as i64).collect();
        create(&path, &base).unwrap();
        let conn = Connection::open(&path).unwrap();
        assert_eq!(tables(&conn).unwrap(), ["读书清单", "作者", "_tables", "_fields", "_views", "_meta"]);
        let rows = grid(&conn, r#"select "书名", "状态", "标签", "读者", "开始", "读完", "页数", "进度", "评分", "作者", "封面", "编号", "创建", id, _order from "读书清单" order by _order"#);
        let t = |s: &str| Sql::Text(s.into());
        assert_eq!(rows[0], [t("三体"), t("读完"), t(r#"["小说","科幻"]"#), t(r#"["小红","小明"]"#), t("2026-11-17 09:30"), Sql::Integer(1), Sql::Integer(302), Sql::Real(0.5), Sql::Integer(4), t("[1]"),
            t(r#"[{"path":".assets/a.png","name":"a.png"},{"path":".assets/带 空格.jpg","name":"带 空格.jpg"}]"#), Sql::Integer(11), t("2027-01-15 08:02:04.456"), Sql::Integer(1), t("a0")]);
        assert_eq!(rows[1][..5], [t("球状闪电"), t(r#"["[暂存]"]"#), t(r#"["历史, 传记"]"#), Sql::Null, t("1969-12-31")]);
        assert_eq!((&rows[2][4], &rows[2][5], &rows[2][6]), (&t("2026-01-02 03:04:05.678"), &Sql::Integer(0), &Sql::Integer(0)));
        // an empty record: nothing but the unchecked box and what the record knows
        assert_eq!(rows[3][..11], [Sql::Null, Sql::Null, Sql::Null, Sql::Null, Sql::Null, Sql::Integer(0), Sql::Null, Sql::Null, Sql::Null, Sql::Null, Sql::Null]);
        assert_eq!(rows[3][13..], [Sql::Integer(7), t("a3")]);
        // computed results are there to read: a count, a list, a date, an error
        let rows = grid(&conn, r#"select "姓名", "作品", "作品数", "作品名", "页数合计", "最早开始" from "作者" order by _order"#);
        assert_eq!(rows[0], [t("刘慈欣"), t("[1,2]"), Sql::Integer(2), t(r#"["三体","球状闪电"]"#), Sql::Real(-1.5e300), t("1969-12-31")]);
        assert_eq!(grid(&conn, r#"select "失效引用" from "读书清单" where id = 1"#)[0], [t(crate::table::calc::BAD_REF)]);
        // the record's own data, readable and exact: UTC to the millisecond, the writers (or nothing), the number
        let utc = |ms: i64| t(&field::iso_utc(ms - zone()));
        assert_eq!(grid(&conn, r#"select _created_at, _updated_at, _created_by, _updated_by, _auto from "读书清单" where id in (2, 3) order by id"#),
            [[utc(T0 + 2000), utc(T0 + 155_554), t("user"), t("agent:读书 2"), Sql::Integer(12)], [utc(T0 + 3000), utc(T0 + 233_331), Sql::Null, Sql::Null, Sql::Integer(13)]]);
        assert_eq!(grid(&conn, r#"select name, type from pragma_table_info('读书清单') where name like '\_%' escape '\' or name = 'id'"#).iter().map(|r| format!("{} {}", text(&r[0]), text(&r[1]))).collect::<Vec<_>>(),
            SYSTEM.map(|(c, d)| format!("{c} {}", d.replace(" PRIMARY KEY", ""))));
        assert_eq!(grid(&conn, "select options from _fields where name = '书名'")[0], [t(r#"{"unique":true}"#)]);
        // the descriptions
        assert_eq!(grid(&conn, "select table_name, type, description from _fields where name = '页数'")[0], [t("读书清单"), t("number"), t("不含附录")]);
        assert_eq!(grid(&conn, "select type, options from _fields where name = '开始'")[0], [t("date"), t(r#"{"date":"cn","time":true}"#)]);
        assert_eq!(grid(&conn, "select table_name, type from _views where name = '看板'")[0], [t("读书清单"), t("kanban")]);
        assert_eq!(grid(&conn, "select * from _tables order by position"), [[Sql::Integer(base_ids[0]), t("读书清单"), Sql::Integer(0), Sql::Integer(30)], [Sql::Integer(base_ids[1]), t("作者"), Sql::Integer(1), Sql::Integer(14)]]);
        assert_eq!(grid(&conn, "select key, value from _meta where key != 'readme' order by key"), [[t("app"), t("Quiet.md")], [t("format"), t("2")]]);
        assert!(text(&grid(&conn, "select value from _meta where key = 'readme'")[0][0]).contains("_order"));
        let config: serde_json::Value = serde_json::from_str(&text(&grid(&conn, "select config from _views where name = '在读'")[0][0])).unwrap();
        assert_eq!(config["folded"], serde_json::json!(["", "想读\u{1f}x", "读完"]));
        assert_eq!(config["widths"].as_object().unwrap().values().map(|v| v.to_string()).collect::<HashSet<_>>(), ["300.5", "88.1", "120.0"].map(String::from).into_iter().collect());
    }

    fn whole() -> usize { WHOLE.with(|w| w.replace(0)) }

    /// Save what changed, then check the file reads back as the document.
    #[track_caller]
    fn sync(doc: &mut Doc, path: &Path) {
        save(path, &doc.base, &doc.dirty).unwrap();
        doc.dirty = Dirty::default();
        let got = reload(path);
        if got != doc.base {
            for (a, b) in got.tables.iter().zip(&doc.base.tables) {
                assert_eq!((&a.id, &a.name, &a.fields, &a.views, a.next_auto), (&b.id, &b.name, &b.fields, &b.views, b.next_auto));
                for (x, y) in a.records.iter().zip(&b.records) { assert_eq!(x, y, "in {}", a.name); }
            }
            assert_eq!(got, doc.base);
        }
    }

    #[test]
    fn edits_are_saved_step_by_step() {
        let dir = Tmp::new("steps");
        let path = dir.file("a.sqlite");
        let mut doc = Doc::new(library());
        create(&path, &doc.base).unwrap();
        let (books, authors) = (doc.base.tables[0].id, doc.base.tables[1].id);
        let fid = |doc: &Doc, n: &str| doc.base.tables[0].field_named(n).unwrap().id;
        let (title, pages, state, author) = (fid(&doc, "书名"), fid(&doc, "页数"), fid(&doc, "状态"), fid(&doc, "作者"));
        whole();

        // cells, the other side of a link included
        assert!(doc.set_cells(books, vec![(3, title, Value::Text("新名字".into())), (7, pages, Value::Number(12.5)), (3, author, Value::Links(vec![3, 2]))]));
        sync(&mut doc, &path);
        assert!(doc.set_cells(books, vec![(1, title, Value::Empty), (1, pages, Value::Empty)]));
        sync(&mut doc, &path);
        // rows added (in the middle and at the end), removed, moved
        let new = doc.add_records(books, Some(1), 2, &[(title, Value::Text("插进来的".into()))]);
        doc.add_records(books, None, 1, &[]);
        sync(&mut doc, &path);
        assert!(doc.remove_records(books, &[2, new[0]]));
        sync(&mut doc, &path);
        assert!(doc.move_records(books, &[7], 0));
        sync(&mut doc, &path);
        assert!(doc.remove_records(authors, &[1]));
        assert!(doc.undo());
        sync(&mut doc, &path);
        // none of that rewrote a table
        assert_eq!(whole(), 0);
        let conn = Connection::open(&path).unwrap();
        assert_eq!(grid(&conn, r#"select id from "读书清单" order by _order"#).iter().map(|r| int(&r[0]).unwrap()).collect::<Vec<_>>(), doc.base.tables[0].records.iter().map(|r| r.id as i64).collect::<Vec<_>>());
        drop(conn);

        // fields: added, retyped, renamed, removed
        let note = doc.add_field(books, Some(1), "备注", FieldType::Text, FieldOpts::default()).unwrap();
        assert!(doc.set_cells(books, vec![(1, note, Value::Text("好书, 推荐".into())), (3, note, Value::Text("一般".into()))]));
        sync(&mut doc, &path);
        assert_eq!(whole(), 1);
        let mut f = doc.base.tables[0].field(note).unwrap().clone();
        f.ty = FieldType::MultiSelect;
        assert!(doc.change_field(books, f));
        sync(&mut doc, &path);
        let mut f = doc.base.tables[0].field(state).unwrap().clone();
        f.ty = FieldType::Text;
        assert!(doc.change_field(books, f));
        sync(&mut doc, &path);
        let mut f = doc.base.tables[0].field(pages).unwrap().clone();
        f.name = "select".into();
        assert!(doc.change_field(books, f));
        sync(&mut doc, &path);
        assert!(doc.remove_field(books, note));
        // a link goes with its mirror
        assert!(doc.remove_field(books, author));
        sync(&mut doc, &path);
        assert!(doc.undo());
        sync(&mut doc, &path);

        // views
        let v = doc.add_view(books, ViewKind::Kanban, "").unwrap();
        assert!(doc.edit_view(books, v, |v| { v.opts.hidden.extend([title, pages]); v.opts.widths.insert(title, 123.25); v.opts.folded.insert("x".into()); }));
        whole();
        sync(&mut doc, &path);
        assert_eq!(whole(), 0);

        // tables: renamed (and back, and onto a name just freed), added, removed
        assert!(doc.rename_table(books, "order"));
        sync(&mut doc, &path);
        assert!(doc.rename_table(books, "书"));
        assert!(doc.rename_table(authors, "order"));
        sync(&mut doc, &path);
        let t = doc.add_table("新表");
        doc.add_records(t, None, 2, &[]);
        sync(&mut doc, &path);
        assert!(doc.remove_table(authors));
        sync(&mut doc, &path);
        let again = doc.add_table("order");
        assert!(doc.remove_table(t));
        assert!(doc.rename_table(again, "新表"));
        sync(&mut doc, &path);
        while doc.undo() {}
        sync(&mut doc, &path);
        let conn = Connection::open(&path).unwrap();
        assert_eq!(tables(&conn).unwrap().into_iter().filter(|n| user_table(n)).collect::<HashSet<_>>(), ["读书清单", "作者"].map(String::from).into_iter().collect());
        drop(conn);
        // what was patched all along is the file a fresh write gives
        let fresh = dir.file("b.sqlite");
        create(&fresh, &doc.base).unwrap();
        assert_eq!(dump(&path), dump(&fresh));
        // a file that went missing comes back whole
        std::fs::remove_file(&path).unwrap();
        assert!(doc.set_cells(doc.base.tables[0].id, vec![(1, title, Value::Text("又一个名字".into()))]));
        sync(&mut doc, &path);
    }

    #[test]
    fn writing_is_stable() {
        let dir = Tmp::new("stable");
        let (a, b) = (dir.file("a.sqlite"), dir.file("b.sqlite"));
        let base = library();
        create(&a, &base).unwrap();
        create(&b, &base.clone()).unwrap();
        assert_eq!(dump(&a), dump(&b));
        assert!(dump(&a).contains("三体"));
        // every set in a different order is the same text
        let mut other = base.clone();
        for v in &mut other.tables[0].views {
            let (mut hidden, mut folded): (Vec<_>, Vec<_>) = (v.opts.hidden.drain().collect(), v.opts.folded.drain().collect());
            hidden.reverse();
            folded.reverse();
            v.opts.hidden = hidden.into_iter().collect();
            v.opts.folded = folded.into_iter().collect();
        }
        std::fs::remove_file(&b).unwrap();
        create(&b, &other).unwrap();
        assert_eq!(dump(&a), dump(&b));
        // nothing to save: the file is not touched
        let before = stamp(&a).unwrap();
        save(&a, &base, &Dirty::default()).unwrap();
        assert_eq!(stamp(&a), Some(before));
        assert_eq!(stamp(&a), Some(before));
        // and a save moves the stamp
        let mut doc = Doc::new(base);
        let (t, f) = (doc.base.tables[0].id, doc.base.tables[0].fields[0].id);
        assert!(doc.set_cells(t, vec![(1, f, Value::Text("三体 II".into()))]));
        save(&a, &doc.base, &doc.dirty).unwrap();
        assert_ne!(stamp(&a), Some(before));
        assert_eq!(stamp(&dir.file("missing.sqlite")), None);
    }

    #[test]
    fn edits_by_other_programs_are_read() {
        let dir = Tmp::new("external");
        let path = dir.file("a.sqlite");
        let base = library();
        create(&path, &base).unwrap();
        let before = stamp(&path);
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(r#"
            INSERT INTO "读书清单" ("书名", "状态", "标签", "页数", "读完", "开始", "作者", "封面", "评分") VALUES ('外来的书', '弃读', '科幻, 新标签', '1,024', 'yes', '2026/3/4 5:06', '2, 3', 'x/y.png', 'five');
            UPDATE "读书清单" SET "书名" = 12, "页数" = 'many', "读者" = '["小红","小刚"]', "开始" = 1800000000000 WHERE id = 1;
            INSERT INTO "读书清单" (id, "书名", _created_at, _updated_by, _auto) VALUES (9, '带时间的书', '2027-01-15 08:02:03+08:00', 'agent:脚本', 99);
            UPDATE "读书清单" SET _created_at = NULL, _updated_at = 'not a time', _created_by = '', _auto = NULL, "状态" = 3 WHERE id = 3;
            DELETE FROM "读书清单" WHERE id = 2;
            ALTER TABLE "读书清单" ADD COLUMN "备注" TEXT;
            ALTER TABLE "读书清单" ADD COLUMN "_score" INTEGER;
            UPDATE "读书清单" SET "备注" = 'note ' || id, "_score" = id * 2;
            ALTER TABLE "读书清单" DROP COLUMN "条码";
            CREATE TABLE "购物" ("名称" TEXT, "价格" REAL, "买了" BOOLEAN, "日期" TEXT);
            INSERT INTO "购物" VALUES ('牛奶', 12.5, 1, '2026-11-17'), ('面包', 8, 0, '2026-11-18');
            DELETE FROM _views WHERE table_name = '作者';
        "#).unwrap();
        drop(conn);
        assert_ne!(stamp(&path), before);
        let got = load(&path).unwrap();
        assert_eq!(got.tables.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["读书清单", "作者", "购物"]);
        let t = &got.tables[0];
        let f = |n: &str| t.field_named(n).unwrap();
        // the dropped column's field is gone, the added ones follow the rest
        assert!(t.field_named("条码").is_none());
        assert_eq!(t.fields[t.fields.len() - 2..].iter().map(|f| (f.name.as_str(), f.ty)).collect::<Vec<_>>(), [("备注", FieldType::Text), ("score", FieldType::Number)]);
        assert_eq!(t.fields.len(), base.tables[0].fields.len() + 1);
        assert_eq!(t.records.iter().map(|r| r.id).collect::<Vec<_>>(), [1, 3, 7, 8, 9]);
        // the new row: its values as each field reads them, new option names added
        let r = t.record(8).unwrap();
        assert_eq!(r.get(f("书名").id), &Value::Text("外来的书".into()));
        assert_eq!(f("状态").opts.options.iter().map(|o| (o.name.as_str(), o.color)).collect::<Vec<_>>(), [("想读", 0), ("在读", 3), ("读完", 6), ("[暂存]", 9), ("3", 4), ("弃读", 5)]);
        assert_eq!(r.get(f("状态").id), &Value::Select(vec![f("状态").option_named("弃读").unwrap().id]));
        assert_eq!(r.get(f("标签").id), &Value::Select(vec![f("标签").option_named("科幻").unwrap().id, f("标签").option_named("新标签").unwrap().id]));
        assert_eq!(r.get(f("页数").id), &Value::Number(1024.));
        assert_eq!(r.get(f("读完").id), &Value::Bool(true));
        assert_eq!(r.get(f("开始").id), &Value::Date(field::date_ms(2026, 3, 4, 5, 6)));
        assert_eq!(r.get(f("作者").id), &Value::Links(vec![2, 3]));
        assert_eq!(r.get(f("封面").id), &Value::Files(vec![super::super::model::Attachment { path: "x/y.png".into(), name: "y.png".into() }]));
        // what a number field can't read is let go, the rest of the row stays
        assert_eq!(r.get(f("评分").id), &Value::Empty);
        assert_eq!(r.get(f("备注").id), &Value::Text("note 8".into()));
        assert_eq!(r.get(f("score").id), &Value::Number(16.));
        // a row nobody stamped is new as of now
        let fresh = |r: &Record| (field::now() - r.created).abs() < 60_000 && r.modified == r.created;
        assert!(r.auto == 100 && fresh(r) && t.next_auto == r.auto + 1, "after the 99 another program gave");
        // and nobody is guessed to have written it
        assert_eq!((&r.created_by, &r.updated_by), (&None, &None));
        // one stamped as other programs write times: that instant, in our wall clock; the writer it gave
        let r = t.record(9).unwrap();
        assert_eq!((r.created, r.modified, r.auto, &r.created_by, r.updated_by.as_deref()), (field::date_ms(2027, 1, 15, 0, 2) + 3000 + zone(), r.created, 99, &None, Some("agent:脚本")));
        // the edited rows
        let r = t.record(1).unwrap();
        assert_eq!((r.get(f("书名").id), r.get(f("页数").id), r.get(f("开始").id)), (&Value::Text("12".into()), &Value::Empty, &Value::Date(1_800_000_000_000)));
        assert_eq!(r.get(f("读者").id), &Value::Select(vec![f("读者").option_named("小红").unwrap().id, f("读者").option_named("小刚").unwrap().id]));
        assert_eq!((r.created, r.auto), (T0 + 1000, 11));
        let r = t.record(3).unwrap();
        assert_eq!(r.get(f("状态").id), &Value::Select(vec![f("状态").option_named("3").unwrap().id]));
        // its system columns are emptied or garbled, but the 创建 / 更新 / 编号 cells still say what they held
        assert_eq!((r.created, r.modified, r.auto, &r.created_by), (T0 + 3000, T0 + 233_331, 13, &None));
        // a table we were never told about, and one that lost its views
        let t = &got.tables[2];
        assert_eq!(t.fields.iter().map(|f| (f.name.as_str(), f.ty)).collect::<Vec<_>>(), [("名称", FieldType::Text), ("价格", FieldType::Number), ("买了", FieldType::Checkbox), ("日期", FieldType::Date)]);
        assert_eq!(t.records.iter().map(|r| (t.title(r), r.get(t.fields[2].id).clone(), r.id)).collect::<Vec<_>>(), [("牛奶".to_string(), Value::Bool(true), 1), ("面包".to_string(), Value::Empty, 2)]);
        assert_eq!((t.views.len(), t.views[0].kind, t.views[0].opts.frozen, got.tables[1].views.len()), (1, ViewKind::Grid, 0, 1));
        // every id is its own
        let mut ids: Vec<u64> = got.all_ids().collect();
        assert!(ids.iter().all(|i| (1..ID_LIMIT).contains(i)));
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), got.tables.iter().map(|t| 1 + t.fields.len() + t.views.len() + t.fields.iter().map(|f| f.opts.options.len()).sum::<usize>()).sum::<usize>());
        // and the app goes on from there
        let mut doc = Doc::new(got);
        let (t, f) = (doc.base.tables[2].id, doc.base.tables[2].fields[0].id);
        assert!(doc.set_cells(t, vec![(2, f, Value::Text("全麦面包".into()))]));
        sync(&mut doc, &path);
        doc.add_records(t, None, 1, &[]);
        whole();
        sync(&mut doc, &path);
        assert_eq!(whole(), 0);
    }

    #[test]
    fn a_plain_sqlite_file_is_a_base() {
        let dir = Tmp::new("plain");
        let path = dir.file("plain.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(r#"
            CREATE TABLE people (name TEXT, age INTEGER, joined DATE, email, vip BOOLEAN, score NUMERIC, team TEXT, blob BLOB);
            INSERT INTO people VALUES ('Ann', 31, '2024-01-05', 'ann@example.com', 1, 9.5, 'red', x'00ff'), ('Bob', 'unknown', '2024-02-06 10:20:30', 'bob@example.com', 0, 7, 'blue', NULL), (NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL);
            DELETE FROM people WHERE name = 'nobody';
            CREATE TABLE "id" ("id" TEXT PRIMARY KEY, "ID2" INT, "_x" TEXT, "x" TEXT, " padded " TEXT);
            INSERT INTO "id" VALUES ('k1', 5, 'under', 'plain', 'p');
            CREATE TABLE codes (code TEXT PRIMARY KEY, label TEXT) WITHOUT ROWID;
            INSERT INTO codes VALUES ('b', 'second'), ('a', 'first');
            CREATE TABLE bare (id INTEGER PRIMARY KEY);
            INSERT INTO bare VALUES (5), (9);
            CREATE TABLE _private (x);
            CREATE VIEW seen AS SELECT name FROM people;
        "#).unwrap();
        drop(conn);
        let got = load(&path).unwrap();
        assert_eq!(got.tables.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["people", "id 2", "codes", "bare"]);
        let t = &got.tables[0];
        use FieldType::*;
        assert_eq!(t.fields.iter().map(|f| (f.name.as_str(), f.ty)).collect::<Vec<_>>(), [("name", Text), ("age", Text), ("joined", Date), ("email", Email), ("vip", Checkbox), ("score", Number), ("team", Text), ("blob", Text)]);
        let cells = |r: &Record| t.fields.iter().map(|f| r.get(f.id).clone()).collect::<Vec<_>>();
        let s = |s: &str| Value::Text(s.into());
        assert_eq!(cells(&t.records[0]), [s("Ann"), s("31"), Value::Date(field::date_ms(2024, 1, 5, 0, 0)), s("ann@example.com"), Value::Bool(true), Value::Number(9.5), s("red"), s("\0\u{fffd}")]);
        assert_eq!(cells(&t.records[1]), [s("Bob"), s("unknown"), Value::Date(field::date_ms(2024, 2, 6, 10, 20) + 30_000), s("bob@example.com"), Value::Empty, Value::Number(7.), s("blue"), Value::Empty]);
        assert!(t.records[2].cells.is_empty());
        assert_eq!((t.records.iter().map(|r| (r.id, r.auto)).collect::<Vec<_>>(), t.next_auto, t.views.len()), (vec![(1, 1), (2, 2), (3, 3)], 4, 1));
        // names the app can't carry are mended, without taking another column's
        let t = &got.tables[1];
        assert_eq!(t.fields.iter().map(|f| (f.name.as_str(), f.ty)).collect::<Vec<_>>(), [("id 2", Text), ("ID2", Number), ("x 2", Text), ("x", Text), ("padded", Text)]);
        assert_eq!(t.title(&t.records[0]), "k1");
        let t = &got.tables[2];
        // no rowids: ids of their own, the rows in the order they came
        assert_eq!(t.records.iter().map(|r| t.title(r)).collect::<Vec<_>>(), ["a", "b"]);
        assert!(t.records.iter().all(|r| (1..ID_LIMIT).contains(&r.id)) && t.records[0].id != t.records[1].id);
        // and the same ones each time the file is read (`stable_id`)
        assert_eq!(load(&path).unwrap().tables[2].records.iter().map(|r| r.id).collect::<Vec<_>>(), t.records.iter().map(|r| r.id).collect::<Vec<_>>());
        // so is everything else made for a file nobody described: tables, the fields guessed, the view each is given
        let again = load(&path).unwrap();
        assert!(again.all_ids().eq(got.all_ids()));
        assert!(got.all_ids().all(|i| (1..ID_LIMIT).contains(&i)) && got.all_ids().collect::<HashSet<_>>().len() == got.all_ids().count());
        // a table with no columns of its own still has a title field
        let t = &got.tables[3];
        assert_eq!((t.fields.len(), t.fields[0].ty, t.records.iter().map(|r| r.id).collect::<Vec<_>>()), (1, Text, vec![5, 9]));

        // saving makes it ours, and nothing is lost on the way
        let mut doc = Doc::new(got);
        let (t, f) = (doc.base.tables[0].id, doc.base.tables[0].fields[0].id);
        assert!(doc.set_cells(t, vec![(3, f, Value::Text("Cy".into()))]));
        sync(&mut doc, &path);
        let conn = Connection::open(&path).unwrap();
        assert_eq!(tables(&conn).unwrap().into_iter().filter(|n| user_table(n)).collect::<HashSet<_>>(), ["people", "id 2", "codes", "bare"].map(String::from).into_iter().collect());
        assert_eq!(grid(&conn, "select name, _order, age from people order by id").iter().map(|r| (text(&r[0]), text(&r[1]), text(&r[2]))).collect::<Vec<_>>(), [("Ann".into(), "a0".into(), "31".into()), ("Bob".into(), "a1".into(), "unknown".into()), ("Cy".into(), "a2".into(), String::new())]);
        // `_private` isn't ours to touch
        assert!(tables(&conn).unwrap().contains(&"_private".to_string()));
    }

    #[test]
    fn awkward_names_round_trip() {
        let dir = Tmp::new("names");
        let path = dir.file("names.sqlite");
        let mut doc = Doc::new(Base::blank(T0));
        let first = doc.base.tables[0].id;
        let names = ["order", "select", "a\"b", "it's", "有 空格", "中文，标点；", "Group By", "x.y", "[brackets]", "`tick`", "from", "100%", "tab\there", "semi;colon --", "INDEX"];
        for n in names {
            let t = doc.add_table(n);
            assert_eq!(doc.base.table(t).unwrap().name, n);
            for f in names { doc.add_field(t, None, f, FieldType::Text, FieldOpts::default()).unwrap(); }
            let fields: Vec<FieldId> = doc.base.table(t).unwrap().fields.iter().filter(|f| f.ty == FieldType::Text).map(|f| f.id).collect();
            let r = doc.base.table(t).unwrap().records[0].id;
            assert!(doc.set_cells(t, fields.iter().map(|f| (r, *f, Value::Text(format!("{n} / {f}")))).collect()));
        }
        assert!(doc.remove_table(first));
        create(&path, &doc.base).unwrap();
        assert_eq!(reload(&path), doc.base);
        assert_eq!(grid(&Connection::open(&path).unwrap(), r#"select "a""b", "select" from "order" where "a""b" like 'order / %'"#).len(), 1);
        // renamed among themselves, with a case-only change
        doc.dirty = Dirty::default();
        let ids: Vec<TableId> = doc.base.tables.iter().map(|t| t.id).collect();
        assert!(doc.rename_table(ids[0], "tmp"));
        assert!(doc.rename_table(ids[1], "order"));
        assert!(doc.rename_table(ids[0], "SELECT"));
        assert!(doc.rename_table(ids[2], "A\"B"));
        let f = doc.base.tables[3].fields[5].clone();
        assert!(doc.change_field(ids[3], Field { name: "ORDER by".into(), ..f }));
        sync(&mut doc, &path);
        // names only a hand-built base could have are mended on the way out
        let mut odd = Base::blank(T0);
        odd.tables[0].name = "数据表".into();
        let again = odd.tables[0].clone();
        odd.tables.push(Table { id: 90, name: "数据表".into(), fields: again.fields.iter().map(|f| Field { id: 300 + f.id, ..f.clone() }).collect(), views: vec![], ..again.clone() });
        odd.tables.push(Table { id: 91, name: "_tables".into(), fields: again.fields.iter().take(4).enumerate().map(|(i, f)| Field { id: 100 + i as u64, name: ["id", "ID", "_meta", ""][i].into(), ..f.clone() }).collect(), views: vec![], ..again });
        let path = dir.file("odd.sqlite");
        create(&path, &odd).unwrap();
        let got = load(&path).unwrap();
        assert_eq!(got.tables.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["数据表", "数据表 2", "tables"]);
        assert_eq!(got.tables[2].fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["id 2", "ID 3", "meta", "字段"]);
        assert_eq!((got.tables[2].records.len(), got.tables[2].views.len()), (5, 1));
    }

    /// A file as format 1 wrote it: row numbers in `_order`, counters in `_meta` and `_tables`.
    const FORMAT_1: &str = r#"
        CREATE TABLE "书" ("id" INTEGER PRIMARY KEY, "_order" INTEGER, "_meta" TEXT, "书名" TEXT, "作者" TEXT);
        INSERT INTO "书" VALUES (1, 2, '{"c":1000,"m":2000,"auto":1}', '甲', '[1]'), (2, 0, '{"c":1000,"m":2000,"auto":2}', '乙', '[1,2]'), (3, 1, '{"c":1000,"m":2000,"auto":3}', '丙', NULL);
        CREATE TABLE "人" ("id" INTEGER PRIMARY KEY, "_order" INTEGER, "_meta" TEXT, "姓名" TEXT, "书" TEXT);
        INSERT INTO "人" VALUES (1, 0, '{"c":1000,"m":2000,"auto":1}', '张', '[1,2]'), (2, 1, '{"c":1000,"m":2000,"auto":2}', '李', '[2]');
        CREATE TABLE _tables (id INTEGER PRIMARY KEY, name TEXT, position INTEGER, next_rec INTEGER, next_auto INTEGER);
        INSERT INTO _tables VALUES (1, '书', 0, 4, 4), (5, '人', 1, 3, 3);
        CREATE TABLE _fields (id INTEGER PRIMARY KEY, table_id INTEGER, table_name TEXT, name TEXT, position INTEGER, type TEXT, options TEXT, description TEXT);
        INSERT INTO _fields VALUES (2, 1, '书', '书名', 0, 'text', '{}', ''), (3, 1, '书', '作者', 1, 'link', '{"link_table":5,"back":7}', ''),
            (6, 5, '人', '姓名', 0, 'text', '{}', ''), (7, 5, '人', '书', 1, 'link', '{"link_table":1,"back":3}', '');
        CREATE TABLE _views (id INTEGER PRIMARY KEY, table_id INTEGER, table_name TEXT, name TEXT, position INTEGER, type TEXT, config TEXT);
        INSERT INTO _views VALUES (4, 1, '书', '表格', 0, 'grid', '{"frozen":1}'), (8, 5, '人', '表格', 0, 'grid', '{"frozen":1}');
        CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT);
        INSERT INTO _meta VALUES ('format', '1'), ('app', 'Quiet.md'), ('next_id', '8');
    "#;

    #[test]
    fn a_format_1_file_is_upgraded_in_place() {
        let dir = Tmp::new("format1");
        let path = dir.file("old.sqlite");
        Connection::open(&path).unwrap().execute_batch(FORMAT_1).unwrap();
        let (base, dirty) = load_marked(&path).unwrap();
        let check = |base: &Base| {
            let (books, people) = (&base.tables[0], &base.tables[1]);
            // the ids it had, the order the row numbers gave, links as they were
            assert_eq!((books.id, people.id), (1, 5));
            assert!(base.all_ids().eq([1, 2, 3, 4, 5, 6, 7, 8]));
            assert_eq!(books.records.iter().map(|r| (r.id, books.title(r))).collect::<Vec<_>>(), [(2, "乙".to_string()), (3, "丙".into()), (1, "甲".into())]);
            assert_eq!(books.records[0].get(3), &Value::Links(vec![1, 2]));
            assert_eq!(people.records.iter().map(|r| (r.id, r.get(7).clone())).collect::<Vec<_>>(), [(1, Value::Links(vec![1, 2])), (2, Value::Links(vec![2]))]);
            assert!(base.tables.iter().all(|t| t.records.windows(2).all(|w| w[0].ord < w[1].ord) && t.records.iter().all(|r| order::valid(&r.ord))));
            assert_eq!((books.next_auto, books.records[2].auto), (4, 1));
            // `_meta`'s times and numbers; nobody known to have written them
            assert!(base.tables.iter().flat_map(|t| &t.records).all(|r| (r.created, r.modified, &r.created_by, &r.updated_by) == (1000, 2000, &None, &None)));
        };
        check(&base);
        // every row has a key made here, to be written
        assert_eq!(dirty.rows.values().map(|r| r.len()).collect::<Vec<_>>().iter().sum::<usize>(), 5);
        let mut doc = Doc::new(base);
        doc.dirty = dirty;
        sync(&mut doc, &path);
        check(&reload(&path));
        let conn = Connection::open(&path).unwrap();
        assert_eq!(grid(&conn, "select value from _meta where key = 'format'"), [[Sql::Text("2".into())]]);
        assert!(grid(&conn, "select value from _meta where key = 'next_id'").is_empty());
        assert_eq!(grid(&conn, "select name from pragma_table_info('_tables')").iter().map(|r| text(&r[0])).collect::<Vec<_>>(), ["id", "name", "position", "next_auto"]);
        assert_eq!(grid(&conn, r#"select type from pragma_table_info('书') where name = '_order'"#), [[Sql::Text("TEXT".into())]]);
        assert_eq!(grid(&conn, r#"select id from "书" order by _order"#), [[Sql::Integer(2)], [Sql::Integer(3)], [Sql::Integer(1)]]);
        assert_eq!(columns(&conn, "人"), ["id", "_order", "_created_at", "_updated_at", "_created_by", "_updated_by", "_auto", "姓名", "书"]);
        drop(conn);
        // read again, it is what it was, with nothing left to write
        let (again, dirty) = load_marked(&path).unwrap();
        assert!(dirty.is_clean());
        assert_eq!(Doc::new(again).base, doc.base);
    }

    fn columns(conn: &Connection, table: &str) -> Vec<String> { grid(conn, &format!("select name from pragma_table_info('{table}')")).iter().map(|r| text(&r[0])).collect() }

    /// A file as format 2 was first written (#424): sort keys, but the row's data still in `_meta`.
    const WITH_META: &str = r#"
        CREATE TABLE "书" ("id" INTEGER PRIMARY KEY, "_order" TEXT, "_meta" TEXT, "书名" TEXT, "编号" INTEGER);
        INSERT INTO "书" VALUES (11, 'a0', '{"created":"2027-01-15 08:02","modified":"2027-01-15 08:05","auto":1,"c":1800000123456,"m":1800000300000}', '甲', 1),
            (22, 'a1', '{"created":"2027-01-15 08:02","modified":"2027-01-16 09:00","auto":2}', '乙', 2), (33, 'a2', NULL, '丙', 7);
        CREATE TABLE _tables (id INTEGER PRIMARY KEY, name TEXT, position INTEGER, next_auto INTEGER);
        INSERT INTO _tables VALUES (101, '书', 0, 8);
        CREATE TABLE _fields (id INTEGER PRIMARY KEY, table_id INTEGER, table_name TEXT, name TEXT, position INTEGER, type TEXT, options TEXT, description TEXT);
        INSERT INTO _fields VALUES (102, 101, '书', '书名', 0, 'text', '{}', ''), (103, 101, '书', '编号', 1, 'auto_number', '{}', '');
        CREATE TABLE _views (id INTEGER PRIMARY KEY, table_id INTEGER, table_name TEXT, name TEXT, position INTEGER, type TEXT, config TEXT);
        INSERT INTO _views VALUES (104, 101, '书', '表格', 0, 'grid', '{"frozen":1}');
        CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT);
        INSERT INTO _meta VALUES ('format', '2'), ('app', 'Quiet.md');
    "#;

    #[test]
    fn row_data_in_meta_moves_to_its_columns() {
        let dir = Tmp::new("with-meta");
        let path = dir.file("a.sqlite");
        Connection::open(&path).unwrap().execute_batch(WITH_META).unwrap();
        let (base, dirty) = load_marked(&path).unwrap();
        let check = |base: &Base| {
            let t = &base.tables[0];
            let got: Vec<_> = t.records.iter().map(|r| (r.id, r.created, r.modified, r.auto, r.created_by.clone(), r.updated_by.clone())).collect();
            // the exact milliseconds; to the minute where only the text was; the 编号 cell where `_meta` is missing
            assert_eq!(got[..2], [(11, 1_800_000_123_456, 1_800_000_300_000, 1, None, None), (22, field::date_ms(2027, 1, 15, 8, 2), field::date_ms(2027, 1, 16, 9, 0), 2, None, None)]);
            assert_eq!((got[2].0, got[2].3, got[2].2 == got[2].1, t.next_auto), (33, 7, true, 8));
        };
        check(&base);
        // the whole table is written again, in today's columns
        assert_eq!(dirty.schema, [101].into_iter().collect());
        let mut doc = Doc::new(base);
        let made = doc.base.tables[0].records[2].created;
        doc.dirty = dirty;
        whole();
        sync(&mut doc, &path);
        assert_eq!(whole(), 1);
        check(&reload(&path));
        assert_eq!(reload(&path).tables[0].records[2].created, made);
        let conn = Connection::open(&path).unwrap();
        assert_eq!(columns(&conn, "书"), ["id", "_order", "_created_at", "_updated_at", "_created_by", "_updated_by", "_auto", "书名", "编号"]);
        assert_eq!(grid(&conn, r#"select _created_at, _created_by, _auto from "书" where id = 11"#), [[Sql::Text(field::iso_utc(1_800_000_123_456 - zone())), Sql::Null, Sql::Integer(1)]]);
        drop(conn);
        let (again, dirty) = load_marked(&path).unwrap();
        assert!(dirty.is_clean());
        assert_eq!(Doc::new(again).base, doc.base);
    }

    /// An edit stamps the record with the time and who made it; undo puts both back, in the file too.
    #[test]
    fn an_edit_is_stamped_and_undone() {
        let dir = Tmp::new("stamp");
        let path = dir.file("a.sqlite");
        let mut doc = Doc::new(library());
        create(&path, &doc.base).unwrap();
        let (t, f) = (doc.base.tables[0].id, doc.base.tables[0].fields[0].id);
        let row = |path: &Path| grid(&Connection::open(path).unwrap(), r#"select _created_at, _updated_at, _created_by, _updated_by from "读书清单" where id = 3"#).remove(0);
        let before = (doc.base.tables[0].record(3).cloned().unwrap(), row(&path));
        doc.base.writer = Writer("agent:整理".into());
        assert!(doc.set_cells(t, vec![(3, f, Value::Text("改过".into()))]));
        sync(&mut doc, &path);
        let r = doc.base.tables[0].record(3).unwrap();
        assert!((field::now() - r.modified).abs() < 60_000);
        assert_eq!((r.created, &r.created_by, r.updated_by.as_deref()), (before.0.created, &None, Some("agent:整理")));
        let after = row(&path);
        assert_eq!((&after[0], &after[2], &after[3]), (&before.1[0], &Sql::Null, &Sql::Text("agent:整理".into())));
        assert_ne!(after[1], before.1[1]);
        // the writer of the moment is whoever edits: undo isn't an edit of its own
        doc.base.writer = Writer::default();
        doc.undo();
        sync(&mut doc, &path);
        assert_eq!((doc.base.tables[0].record(3).unwrap(), row(&path)), (&before.0, before.1.clone()));
        doc.redo();
        sync(&mut doc, &path);
        assert_eq!(row(&path), after);
        // a new record is the writer's from the start
        let id = doc.add_records(t, None, 1, &[])[0];
        let r = doc.base.tables[0].record(id).unwrap();
        assert_eq!((r.created_by.as_deref(), r.updated_by.as_deref(), r.created), (Some("user"), Some("user"), r.modified));
    }

    #[test]
    fn a_moved_row_is_the_only_one_written() {
        let dir = Tmp::new("move");
        let path = dir.file("a.sqlite");
        let mut doc = Doc::new(library());
        create(&path, &doc.base).unwrap();
        let orders = || grid(&Connection::open(&path).unwrap(), r#"select id, _order from "读书清单" order by id"#).into_iter().map(|r| (int(&r[0]).unwrap(), text(&r[1]))).collect::<HashMap<_, _>>();
        let before = orders();
        let books = doc.base.tables[0].id;
        whole();
        assert!(doc.move_records(books, &[7], 1));
        assert_eq!(doc.dirty.rows[&books], [7].into_iter().collect());
        sync(&mut doc, &path);
        assert_eq!(whole(), 0);
        let after = orders();
        assert_eq!(after.iter().filter(|(id, k)| before[id] != **k).map(|(id, _)| *id).collect::<Vec<_>>(), [7]);
        assert_eq!(doc.base.tables[0].records.iter().map(|r| r.id).collect::<Vec<_>>(), [1, 7, 2, 3]);
        // undone, the row has its old key back
        assert!(doc.undo());
        sync(&mut doc, &path);
        assert_eq!(orders(), before);
    }

    /// A 笔记 cell is its note's path, as text; option names another program wrote get the same ids on every read.
    #[test]
    fn note_cells_hold_paths() {
        let dir = Tmp::new("notes");
        let path = dir.file("读书.sqlite");
        let mut doc = Doc::new(Base::blank_seeded(T0, 9));
        let t = doc.base.tables[0].id;
        let note = doc.add_field(t, None, "读书笔记", FieldType::Note, FieldOpts::default()).unwrap();
        let pick = doc.add_field(t, None, "状态", FieldType::SingleSelect, FieldOpts::default()).unwrap();
        let rows: Vec<RecId> = doc.base.tables[0].records.iter().map(|r| r.id).collect();
        assert!(doc.set_cells(t, vec![(rows[0], note, Value::Text("三体.xml".into())), (rows[1], note, Value::Text("../别处/球状 闪电.xml".into()))]));
        create(&path, &doc.base).unwrap();
        assert_eq!(reload(&path), doc.base);
        let conn = Connection::open(&path).unwrap();
        assert_eq!(grid(&conn, r#"select "读书笔记" from "未命名数据表" order by _order limit 3"#), [[Sql::Text("三体.xml".into())], [Sql::Text("../别处/球状 闪电.xml".into())], [Sql::Null]]);
        assert_eq!(grid(&conn, "select type from _fields where name = '读书笔记'"), [[Sql::Text("note".into())]]);
        // another program points a row at a note, and writes option names the field hasn't got
        conn.execute_batch(&format!(r#"UPDATE "未命名数据表" SET "读书笔记" = ' 新的.xml ', "状态" = '在读' WHERE id = {}; UPDATE "未命名数据表" SET "状态" = '读完' WHERE id = {};"#, rows[2], rows[3])).unwrap();
        drop(conn);
        let got = load(&path).unwrap();
        assert_eq!(got.tables[0].record(rows[2]).unwrap().get(note), &Value::Text("新的.xml".into()));
        let options = |b: &Base| b.tables[0].field(pick).unwrap().opts.options.clone();
        assert_eq!(options(&got).iter().map(|o| o.name.as_str()).collect::<Vec<_>>(), ["在读", "读完"]);
        assert_eq!(options(&load(&path).unwrap()), options(&got));
        // what the engine does with them: shown, searched and exported by title; never copied to another cell
        let mut doc = Doc::new(got);
        assert_eq!(crate::table::csv::plain_text(&doc.base, &doc.base.tables[0]).lines().next(), Some("三体"));
        let copy = doc.duplicate_records(t, &[rows[0]])[0];
        assert!(doc.base.tables[0].record(copy).unwrap().get(note).is_empty());
        assert!(!doc.fill(t, &[rows[0]], &[rows[4]], &[note]));
        assert!(!doc.paste(t, &rows[4..], &[note], &[vec!["x.xml".into()]]));
        let twin = doc.duplicate_field(t, note).unwrap();
        assert!(doc.base.tables[0].records.iter().all(|r| r.get(twin).is_empty()));
    }

    /// Ids that can't stay (2^53 or more, 0, a duplicate) are replaced as they are read,
    /// what points at them following, and the same way on every read: two devices
    /// opening one damaged file make the same file of it.
    #[test]
    fn bad_ids_are_replaced_the_same_everywhere() {
        let dir = Tmp::new("bad-ids");
        let path = dir.file("a.sqlite");
        create(&path, &library()).unwrap();
        const BIG: u64 = ID_LIMIT + 5;
        Connection::open(&path).unwrap().execute_batch(&format!(r#"
            INSERT INTO "作者" (id, _order, _created_at, _updated_at, "姓名") VALUES ({BIG}, 'a5', '2027-01-15T00:02:03Z', '2027-01-15T00:02:03Z', '大号');
            UPDATE "读书清单" SET "作者" = '[1,{BIG}]' WHERE id = 3;
            UPDATE _tables SET id = {t} WHERE name = '作者';
            UPDATE _fields SET table_id = {t} WHERE table_name = '作者';
            UPDATE _views SET table_id = {t} WHERE table_name = '作者';
            UPDATE _fields SET options = json_set(options, '$.link_table', {t}) WHERE table_name = '读书清单' AND name = '作者';
            UPDATE _fields SET id = {f} WHERE name = '状态';
            UPDATE _views SET config = json_set(config, '$.stack', {f}) WHERE name = '看板';
            UPDATE _fields SET id = 0 WHERE name = '页数';
            UPDATE _views SET id = (SELECT id FROM _fields WHERE name = '书名') WHERE name = '看板';
        "#, t = ID_LIMIT + 7, f = ID_LIMIT + 8)).unwrap();
        let (base, dirty) = load_marked(&path).unwrap();
        // another read, its own random ids for anything new: the same base
        assert_eq!(load_marked(&path).unwrap(), (base.clone(), dirty.clone()));
        assert!(base.all_ids().all(|i| (1..ID_LIMIT).contains(&i)) && base.tables.iter().flat_map(|t| &t.records).all(|r| (1..ID_LIMIT).contains(&r.id)));
        let ids: Vec<u64> = base.all_ids().collect();
        assert_eq!(ids.len(), ids.iter().collect::<HashSet<_>>().len());
        let (books, people) = (&base.tables[0], &base.tables[1]);
        let f = |n: &str| books.field_named(n).unwrap();
        // the table and the field that moved are pointed at where they went
        assert!(people.id < ID_LIMIT && f("作者").opts.link_table == Some(people.id));
        let kanban = books.views.iter().find(|v| v.name == "看板").unwrap();
        assert_eq!(kanban.opts.stack, Some(f("状态").id));
        assert_ne!(kanban.id, f("书名").id);
        // the record too: the link to it holds its new id
        let big = people.records.iter().find(|r| people.title(r) == "大号").unwrap().id;
        assert!(big < ID_LIMIT);
        assert_eq!(books.record(3).unwrap().get(f("作者").id), &Value::Links(vec![1, big]));
        // written back at once: `_tables` / `_fields` / `_views`, the new row, the row whose link changed
        assert!(dirty.meta && dirty.rows[&people.id].contains(&big) && dirty.rows[&books.id].contains(&3));
        let mut doc = Doc::new(base);
        doc.dirty = dirty;
        sync(&mut doc, &path);
        let conn = Connection::open(&path).unwrap();
        for sql in [r#"select max(id) from "作者""#, "select max(id) from _tables", "select max(id) from _fields", "select max(id) from _views"] { assert!(int(&grid(&conn, sql)[0][0]).unwrap() < ID_LIMIT as i64, "{sql}"); }
        assert_eq!(grid(&conn, r#"select "作者" from "读书清单" where id = 3"#), [[Sql::Text(format!("[1,{}]", big))]]);
        drop(conn);
        assert!(load_marked(&path).unwrap().1.is_clean());
    }

    #[test]
    fn keys_that_clash_are_mended() {
        let dir = Tmp::new("clash");
        let path = dir.file("a.sqlite");
        create(&path, &library()).unwrap();
        // two copies that inserted at one place, a row added without a key or an id
        Connection::open(&path).unwrap().execute_batch(r#"
            UPDATE "读书清单" SET _order = (SELECT _order FROM "读书清单" WHERE id = 2) WHERE id = 7;
            INSERT INTO "读书清单" ("书名") VALUES ('外来的');
        "#).unwrap();
        let (base, dirty) = load_marked(&path).unwrap();
        let t = &base.tables[0];
        assert_eq!(t.records.iter().map(|r| r.id).collect::<Vec<_>>(), [1, 2, 7, 3, 8]);
        assert!(t.records.windows(2).all(|w| w[0].ord < w[1].ord));
        // the second of the two equal keys and the new row get keys of their own
        assert_eq!(dirty.rows[&t.id], [7, 8].into_iter().collect());
        let mut doc = Doc::new(base);
        doc.dirty = dirty;
        sync(&mut doc, &path);
        assert!(load_marked(&path).unwrap().1.is_clean());
    }

    #[test]
    fn dates_keep_their_seconds() {
        for ms in [0, 59_999, 60_000, -1, -86_400_000, T0, field::date_ms(2026, 11, 17, 0, 0), field::date_ms(2026, 11, 17, 23, 59) + 59_000, *DATES.start(), *DATES.end()] {
            assert_eq!(text_date(&date_text(ms)), Some(ms), "{}", date_text(ms));
            assert!(matches!(date(ms), Sql::Text(_)));
        }
        assert_eq!((*DATES.start(), *DATES.end()), (field::date_ms(1000, 1, 1, 0, 0), field::date_ms(9999, 12, 31, 23, 59) + 59_999));
        assert_eq!(date_text(field::date_ms(2026, 11, 17, 0, 0)), "2026-11-17");
        assert_eq!(date_text(field::date_ms(2026, 11, 17, 9, 5) + 7_000), "2026-11-17 09:05:07");
        assert_eq!(date_text(field::date_ms(2026, 11, 17, 0, 0) + 7_089), "2026-11-17 00:00:07.089");
        assert_eq!(text_date("2026-11-17T09:05:07.5Z"), Some(field::date_ms(2026, 11, 17, 9, 5) + 7_500));
        assert_eq!(text_date("nonsense"), None);
        // out of the calendar's range: the milliseconds themselves
        assert_eq!(date(DATES.end() + 1), Sql::Integer(DATES.end() + 1));
        assert_eq!(text_date(&(DATES.start() - 1).to_string()), Some(DATES.start() - 1));
    }

    /// xorshift: the same edits for the same seed.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0 }
        fn below(&mut self, n: usize) -> usize { if n == 0 { 0 } else { (self.next() % n as u64) as usize } }
        fn chance(&mut self, of: usize) -> bool { self.below(of) == 0 }
        fn pick<T: Clone>(&mut self, xs: &[T]) -> Option<T> { if xs.is_empty() { None } else { Some(xs[self.below(xs.len())].clone()) } }
    }

    const NAMES: &[&str] = &["名称", "select", "order", "has space", "引\"号", "a'b", "where", "列", "Group", "x.y", "百分比%", "ID", "任务", "表 一"];
    const TEXTS: &[&str] = &["甲", "hello world", "a, b", "[1]", "[\"x\"]", "line\nbreak", "'q' \"dq\"", " lead", "123", "2026-01-02", "true", "￥1,200.5", "🙂", "红", "0"];

    fn random_value(rng: &mut Rng, base: &Base, f: &Field) -> Value {
        use FieldType::*;
        if rng.chance(6) { return Value::Empty; }
        match f.ty {
            t if t.textual() => Value::Text(rng.pick(TEXTS).unwrap().to_string()),
            Number | Currency => Value::Number(rng.pick(&[0., 1., -2., 3.25, 1e20, -0.001, 1234567., 0.1 + 0.2]).unwrap()),
            Progress => Value::Number(rng.below(101) as f64 / 100.),
            Rating => Value::Number(1. + rng.below(5) as f64),
            Checkbox => Value::Bool(true),
            Date => Value::Date(field::date_ms(1960 + rng.below(80) as i64, 1 + rng.below(12) as u32, 1 + rng.below(28) as u32, rng.below(24) as u32, rng.below(60) as u32) + if rng.chance(4) { rng.below(60_000) as i64 } else { 0 }),
            SingleSelect => rng.pick(&f.opts.options).map(|o| Value::Select(vec![o.id])).unwrap_or_default(),
            MultiSelect | Person => { let mut ids: Vec<OptId> = f.opts.options.iter().map(|o| o.id).filter(|_| rng.chance(2)).collect(); if rng.chance(2) { ids.reverse(); } Value::Select(ids) }
            Link => {
                let ids: Vec<RecId> = f.opts.link_table.and_then(|t| base.table(t)).map(|t| t.records.iter().map(|r| r.id).collect()).unwrap_or_default();
                Value::Links((0..rng.below(3)).filter_map(|_| rng.pick(&ids)).collect::<HashSet<_>>().into_iter().collect())
            }
            Attachment => Value::Files((0..1 + rng.below(2)).map(|i| super::super::model::Attachment { path: format!(".assets/{i}.png"), name: format!("图 \"{i}\".png") }).collect()),
            _ => Value::Empty,
        }
    }

    fn random_opts(rng: &mut Rng, doc: &mut Doc, tid: TableId, ty: FieldType) -> FieldOpts {
        use FieldType::*;
        let mut o = field::default_opts(ty);
        let tables: Vec<TableId> = doc.base.tables.iter().map(|t| t.id).collect();
        match ty {
            SingleSelect | MultiSelect | Person => for n in ["红", "绿", "蓝, 靛", "[紫]"].iter().take(1 + rng.below(4)) { o.options.push(SelectOption { id: doc.base.new_id(), name: n.to_string(), color: rng.below(10) as u8 }); },
            Link => { o.link_table = rng.pick(&tables); o.single = rng.chance(3); if rng.chance(2) { o.back = Some(0); } }
            Lookup => {
                let t = doc.base.table(tid).unwrap();
                let links: Vec<&Field> = t.fields.iter().filter(|f| f.ty == Link).collect();
                if let Some(via) = rng.pick(&links) {
                    o.via = Some(via.id);
                    o.target = via.opts.link_table.and_then(|t| doc.base.table(t)).and_then(|t| rng.pick(&t.fields)).map(|f| f.id);
                }
                o.rollup = rng.pick(Rollup::ALL).unwrap();
            }
            Formula => o.formula = rng.pick(&["1+2", "{名称}", "", "((", "\"a\" & \"b\""]).unwrap().to_string(),
            AutoNumber => o.prefix = rng.pick(&["", "N-"]).unwrap().to_string(),
            Date => { o.time = rng.chance(2); o.date = rng.pick(DateFmt::ALL).unwrap(); }
            Number => { o.precision = rng.pick(&[None, Some(0), Some(2)]).unwrap(); }
            _ => {}
        }
        o
    }

    /// One random edit, through the actions the UI uses.
    fn step(doc: &mut Doc, rng: &mut Rng) {
        let tables: Vec<TableId> = doc.base.tables.iter().map(|t| t.id).collect();
        let tid = rng.pick(&tables).unwrap();
        let t = doc.base.table(tid).unwrap();
        let recs: Vec<RecId> = t.records.iter().map(|r| r.id).collect();
        let fields: Vec<FieldId> = t.fields.iter().map(|f| f.id).collect();
        let views: Vec<ViewId> = t.views.iter().map(|v| v.id).collect();
        let some = |rng: &mut Rng, n: usize| -> Vec<RecId> { (0..1 + rng.below(n)).filter_map(|_| rng.pick(&recs)).collect() };
        match rng.below(22) {
            0..=5 => {
                let cells: Vec<(RecId, FieldId, Value)> = (0..1 + rng.below(6)).filter_map(|_| {
                    let (r, f) = (rng.pick(&recs)?, t.field(rng.pick(&fields)?)?);
                    Some((r, f.id, random_value(rng, &doc.base, f)))
                }).collect();
                doc.set_cells(tid, cells);
            }
            6 | 7 => {
                let at = if rng.chance(2) { None } else { Some(rng.below(recs.len() + 1)) };
                let with: Vec<(FieldId, Value)> = rng.pick(&fields).and_then(|f| t.field(f)).map(|f| (f.id, random_value(rng, &doc.base, f))).into_iter().collect();
                if recs.len() < 40 { doc.add_records(tid, at, 1 + rng.below(3), &with); }
            }
            8 => { let ids = some(rng, 3); doc.remove_records(tid, &ids); }
            9 => { let ids = some(rng, 3); doc.move_records(tid, &ids, rng.below(recs.len() + 1)); }
            10 => { let ids = some(rng, 2); doc.duplicate_records(tid, &ids); }
            11 | 12 => {
                let ty = rng.pick(FieldType::ALL).unwrap();
                let opts = random_opts(rng, doc, tid, ty);
                let at = if rng.chance(2) { None } else { Some(rng.below(fields.len() + 1)) };
                if fields.len() < 16 { doc.add_field(tid, at, rng.pick(NAMES).unwrap(), ty, opts); }
            }
            13 | 14 => {
                let Some(mut f) = rng.pick(&fields).and_then(|f| t.field(f)).cloned() else { return };
                match rng.below(5) {
                    0 => f.name = rng.pick(NAMES).unwrap().to_string(),
                    1 => {
                        let ty = rng.pick(FieldType::ALL).unwrap();
                        let options = std::mem::take(&mut f.opts.options);
                        f.opts = random_opts(rng, doc, tid, ty);
                        if ty.select() && f.ty.select() { f.opts.options = options; }
                        // `back: Some(0)` asks `add_field` for a mirror; here it would be taken for a field
                        if f.opts.back == Some(0) { f.opts.back = None; }
                        f.ty = ty;
                    }
                    2 if f.ty.select() => { let id = doc.base.new_id(); f.opts.options.push(SelectOption { id, name: format!("选项 {id}"), color: rng.below(10) as u8 }); }
                    3 if f.ty.select() && !f.opts.options.is_empty() => { f.opts.options.remove(rng.below(f.opts.options.len())); }
                    _ => { f.desc = rng.pick(TEXTS).unwrap().to_string(); f.opts.default = rng.pick(&["", "1", "now"]).unwrap().to_string(); }
                }
                doc.change_field(tid, f);
            }
            15 => { if let Some(f) = rng.pick(&fields) { doc.remove_field(tid, f); } }
            16 => { if views.len() < 5 { doc.add_view(tid, rng.pick(ViewKind::ALL).unwrap(), rng.pick(&["", "视图", "it's \"mine\""]).unwrap()); } }
            17 => {
                let (vid, f, g) = (rng.pick(&views).unwrap(), rng.pick(&fields).unwrap(), rng.pick(&fields).unwrap());
                let (what, n) = (rng.below(12), rng.below(400));
                doc.edit_view(tid, vid, |v| {
                    let o = &mut v.opts;
                    match what {
                        0 => { if !o.hidden.remove(&f) { o.hidden.insert(f); o.hidden.insert(g); } }
                        1 => { o.widths.insert(f, 60. + n as f32 / 3.); }
                        2 => { o.frozen = n % 4; o.row_height = RowHeight::ALL[n % 4]; }
                        3 => o.sorts.push(SortBy { field: f, desc: n % 2 == 0 }),
                        4 => o.groups = vec![SortBy { field: f, desc: n % 2 == 0 }],
                        5 => { o.filter.all = n % 2 == 0; o.filter.conds.push(Cond { field: f, op: FilterOp::Contains, value: TEXTS[n % TEXTS.len()].into(), date: DateRef::ALL[n % DateRef::ALL.len()] }); }
                        6 => { o.stats.insert(f, if n % 2 == 0 { Stat::Count } else { Stat::Sum }); }
                        7 => o.colors.push(ColorRule { filter: Filter { all: true, conds: vec![Cond { field: f, op: FilterOp::NotEmpty, value: String::new(), date: DateRef::Exact }] }, color: (n % 10) as u8 }),
                        8 => { o.stack = Some(f); o.cover = Some(g); o.date = Some(f); o.end = Some(g); }
                        9 => { o.folded.insert(TEXTS[n % TEXTS.len()].into()); o.folded.insert(format!("{n}\u{1f}x")); }
                        10 => o.order = vec![g, f],
                        _ => v.name = format!("视图 {n}"),
                    }
                });
            }
            18 => { doc.rename_table(tid, rng.pick(NAMES).unwrap()); }
            19 => { if tables.len() < 4 { doc.add_table(rng.pick(NAMES).unwrap()); } else { doc.remove_table(tid); } }
            20 => { doc.undo(); }
            _ => { if rng.chance(2) { doc.redo(); } else { doc.undo(); doc.undo(); } }
        }
    }

    #[test]
    fn random_edits_save_and_load() {
        let seeds: u64 = std::env::var("FDL_FUZZ_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
        let dir = Tmp::new("random");
        for seed in std::env::var("FDL_FUZZ_FROM").ok().and_then(|s| s.parse().ok()).unwrap_or(0)..seeds {
            let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
            let path = dir.file(&format!("{seed}.sqlite"));
            let mut doc = Doc::new(if seed % 2 == 0 { library() } else { Base::blank_seeded(T0, seed) });
            doc.base.ids = IdGen::seeded(seed);
            create(&path, &doc.base).unwrap();
            for i in 0..60 {
                step(&mut doc, &mut rng);
                if rng.chance(3) || i == 59 {
                    let what = format!("seed {seed}, step {i}");
                    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sync(&mut doc, &path)));
                    if let Err(e) = caught { eprintln!("{what}"); std::panic::resume_unwind(e); }
                }
            }
            // the patched file holds what a fresh one would
            let fresh = dir.file("fresh.sqlite");
            let _ = std::fs::remove_file(&fresh);
            create(&fresh, &doc.base).unwrap();
            save(&path, &doc.base, &Dirty { meta: true, ..Default::default() }).unwrap();
            let (patched, whole) = (dump(&path), dump(&fresh));
            if let Some((a, b)) = patched.lines().zip(whole.lines()).find(|(a, b)| a != b) { panic!("seed {seed}: the patched file has\n{a}\nwhere a fresh one has\n{b}"); }
            assert_eq!(patched.len(), whole.len(), "seed {seed}");
            let _ = std::fs::remove_file(&path);
        }
    }

    /// `cargo test --release -p fdl_core table::sqlite::tests::large -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn large_tables_are_quick() {
        let dir = Tmp::new("large");
        let path = dir.file("large.sqlite");
        let base = crate::table::sample::large(100_000);
        let cols = base.tables[0].fields.len();
        let t = std::time::Instant::now();
        create(&path, &base).unwrap();
        println!("create 100000 × {cols}: {:?} ({:.1} MB)", t.elapsed(), std::fs::metadata(&path).unwrap().len() as f64 / 1e6);
        let t = std::time::Instant::now();
        let got = load(&path).unwrap();
        println!("load: {:?}", t.elapsed());
        let t = std::time::Instant::now();
        let mut doc = Doc::new(got);
        println!("Doc::new: {:?}", t.elapsed());
        assert_eq!(doc.base, Doc::new(base).base);
        let (tid, f) = (doc.base.tables[0].id, doc.base.tables[0].fields[0].id);
        assert!(doc.set_cells(tid, (0..100).map(|i| (1 + i * 997, f, Value::Text(format!("改过 {i}")))).collect()));
        let t = std::time::Instant::now();
        save(&path, &doc.base, &doc.dirty).unwrap();
        println!("save 100 changed rows: {:?}", t.elapsed());
        doc.dirty = Dirty::default();
        doc.add_records(tid, Some(50_000), 1, &[]);
        let t = std::time::Instant::now();
        save(&path, &doc.base, &doc.dirty).unwrap();
        println!("save a row inserted in the middle: {:?}", t.elapsed());
        doc.dirty = Dirty::default();
        assert!(doc.move_records(tid, &[doc.base.tables[0].records[10].id], 70_000));
        let t = std::time::Instant::now();
        save(&path, &doc.base, &doc.dirty).unwrap();
        println!("save a row moved: {:?}", t.elapsed());
        doc.dirty = Dirty::default();
        assert!(doc.run(vec![BaseOp::Table(tid, Op::MoveField { id: doc.base.tables[0].fields[3].id, to: 1 })]));
        let t = std::time::Instant::now();
        save(&path, &doc.base, &doc.dirty).unwrap();
        println!("save after a column moved (the table rewritten): {:?}", t.elapsed());
        let t = std::time::Instant::now();
        assert!(stamp(&path).is_some());
        println!("stamp: {:?}", t.elapsed());
        assert_eq!(reload(&path), doc.base);
    }
}
