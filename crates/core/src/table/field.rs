//! What each field type does with a value: read it from text (typing, paste,
//! import), show it, order it, and carry it over to another type.

use super::model::*;
use crate::time::days_from_civil;
use std::cmp::Ordering;

const DAY: i64 = 86_400_000;

/// `(y, m, d, h, min)` of wall-clock milliseconds (no zone: `time::civil` would add the local offset).
pub fn date_parts(ms: i64) -> (i64, u32, u32, u32, u32) {
    let days = ms.div_euclid(DAY);
    let rest = ms.rem_euclid(DAY) / 60_000;
    // Howard Hinnant's civil-from-days
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + (m <= 2) as i64, m, d, (rest / 60) as u32, (rest % 60) as u32)
}

pub fn date_ms(y: i64, m: u32, d: u32, h: u32, min: u32) -> i64 {
    days_from_civil(y, m, d) * DAY + (h as i64 * 60 + min as i64) * 60_000
}

/// Today's wall-clock midnight.
pub fn today() -> i64 { now().div_euclid(DAY) * DAY }

/// Now, as wall-clock milliseconds.
pub fn now() -> i64 { crate::time::now_ms() as i64 + crate::time::local_offset() * 1000 }

/// `2026-11-17`, or `2026-11-17 09:30` when the value has a time of day (or `time` asks).
pub fn iso(ms: i64, time: bool) -> String {
    let (y, m, d, h, min) = date_parts(ms);
    if time || h != 0 || min != 0 { format!("{y:04}-{m:02}-{d:02} {h:02}:{min:02}") } else { format!("{y:04}-{m:02}-{d:02}") }
}

/// Unix milliseconds as UTC ISO 8601 to the millisecond, `2027-01-15T00:02:03.123Z`:
/// the file's `_created_at` / `_updated_at`. As strings they sort as the times do.
pub fn iso_utc(ms: i64) -> String {
    let (y, m, d, h, min) = date_parts(ms);
    let r = ms.rem_euclid(60_000);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{:02}.{:03}Z", r / 1000, r % 1000)
}

/// An ISO 8601 time as Unix milliseconds: `iso_utc`'s, and what other programs
/// write: a space for the `T`, no seconds or milliseconds, an offset
/// (`+08:00`, `+0800`, `-05`) for the `Z`. No zone at all is UTC; a date alone is its midnight.
pub fn parse_iso(s: &str) -> Option<i64> {
    let s = s.trim();
    let (day, time) = match s.find(['T', 't', ' ']) { Some(i) => (&s[..i], s[i + 1..].trim()), None => (s, "") };
    let p: Vec<&str> = day.split('-').collect();
    let [y, m, d] = p[..] else { return None };
    if y.len() != 4 || m.is_empty() || d.is_empty() || m.len() > 2 || d.len() > 2 { return None; }
    let (y, m, d): (i64, u32, u32) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    if !(1..=12).contains(&m) || d < 1 || d > crate::time::days_in_month(y, m) { return None; }
    // the zone: `Z`, or a sign and hours with optional minutes
    let (clock, off) = if let Some(c) = time.strip_suffix(['Z', 'z']) { (c, 0) } else if let Some(i) = time.rfind(['+', '-']) {
        let z = time[i + 1..].replace(':', "");
        if !z.bytes().all(|b| b.is_ascii_digit()) || !matches!(z.len(), 2 | 4) { return None; }
        let (h, mm): (i64, i64) = (z[..2].parse().ok()?, if z.len() == 4 { z[2..].parse().ok()? } else { 0 });
        if h > 23 || mm > 59 { return None; }
        (time[..i].trim_end(), if &time[i..i + 1] == "-" { -1 } else { 1 } * (h * 60 + mm) * 60_000)
    } else { (time, 0) };
    let mut ms = days_from_civil(y, m, d) * DAY;
    if !clock.is_empty() {
        let (hms, frac) = clock.split_once(['.', ',']).unwrap_or((clock, ""));
        let t: Vec<i64> = hms.split(':').map(|p| if p.len() == 2 { p.parse().ok() } else { None }).collect::<Option<_>>()?;
        let (h, mi, sec) = match t[..] { [h, mi] => (h, mi, 0), [h, mi, sec] => (h, mi, sec), _ => return None };
        if h > 23 || mi > 59 || sec > 60 || !frac.bytes().all(|b| b.is_ascii_digit()) { return None; }
        // milliseconds: the first three digits
        let frac: i64 = format!("{:0<3}", &frac[..frac.len().min(3)]).parse().ok()?;
        ms += ((h * 60 + mi) * 60 + sec) * 1000 + frac;
    }
    Some(ms - off)
}

pub fn format_date(ms: i64, fmt: DateFmt, time: bool) -> String {
    let (y, m, d, h, min) = date_parts(ms);
    let day = match fmt {
        DateFmt::Slash => format!("{y:04}/{m:02}/{d:02}"),
        DateFmt::Dash => format!("{y:04}-{m:02}-{d:02}"),
        DateFmt::MonthDay => format!("{m:02}-{d:02}"),
        DateFmt::Us => format!("{m:02}/{d:02}/{y:04}"),
        DateFmt::Eu => format!("{d:02}/{m:02}/{y:04}"),
        DateFmt::Cn => format!("{y}年{m}月{d}日"),
        DateFmt::CnMonthDay => format!("{m}月{d}日"),
    };
    if time { format!("{day} {h:02}:{min:02}") } else { day }
}

/// A date as people type it: `2026-1-30`, `2026/01/30`, `2026年1月30日`,
/// `1/30` and `1月30日` (this year), each with an optional `9:30` or
/// `T09:30:00` after it.
pub fn parse_date(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() { return None; }
    let (day, time) = match s.find(['T', ' ']) {
        Some(i) => (&s[..i], s[i + 1..].trim()),
        None => (s, ""),
    };
    let nums: Vec<i64> = day.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).map(|p| p.parse().ok()).collect::<Option<_>>()?;
    let this_year = date_parts(now()).0;
    let (y, m, d) = match nums[..] {
        [y, m, d] if y > 31 => (y, m, d),
        // 01/30/2026
        [m, d, y] if y > 31 => (y, m, d),
        [m, d] => (this_year, m, d),
        _ => return None,
    };
    if !(1..=12).contains(&m) || d < 1 || d > crate::time::days_in_month(y, m as u32) as i64 || !(1..=9999).contains(&y) { return None; }
    let (mut h, mut min) = (0, 0);
    if !time.is_empty() {
        let t: Vec<u32> = time.trim_end_matches('Z').split(':').map(|p| p.split('.').next().unwrap_or("").parse().ok()).collect::<Option<_>>()?;
        if t.len() < 2 || t[0] > 23 || t[1] > 59 { return None; }
        (h, min) = (t[0], t[1]);
    }
    Some(date_ms(y, m as u32, d as u32, h, min))
}

/// A number as typed: `1,234.5`, `¥12`, `50%` (a percentage is its fraction).
pub fn parse_number(s: &str) -> Option<f64> {
    let t: String = s.trim().chars().filter(|c| !matches!(c, ',' | '，' | ' ' | '¥' | '$' | '€' | '£' | '￥')).collect();
    let (t, pct) = match t.strip_suffix('%') { Some(t) => (t, true), None => (t.as_str(), false) };
    let n: f64 = t.parse().ok()?;
    n.is_finite().then_some(if pct { n / 100. } else { n })
}

fn group_thousands(int: &str) -> String {
    let (sign, digits) = match int.strip_prefix('-') { Some(d) => ("-", d), None => ("", int) };
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) { out.push(','); }
        out.push(c);
    }
    format!("{sign}{out}")
}

/// A number the shortest way that reads back the same (`3`, `0.1`, not `3.0`).
pub fn plain_number(n: f64) -> String {
    if n == n.trunc() && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n}") }
}

pub fn format_number(n: f64, f: &Field) -> String {
    let o = &f.opts;
    let fixed = |n: f64, p: Option<u8>| match p { Some(p) => format!("{n:.*}", p as usize), None => plain_number(n) };
    let split = |s: String| match s.split_once('.') { Some((i, d)) => format!("{}.{d}", group_thousands(i)), None => group_thousands(&s) };
    match f.ty {
        FieldType::Currency => format!("{}{}", if o.currency.is_empty() { "¥" } else { &o.currency }, split(fixed(n, Some(o.precision.unwrap_or(2))))),
        FieldType::Progress => format!("{}%", fixed(n * 100., Some(o.precision.unwrap_or(0)))),
        FieldType::AutoNumber => format!("{}{}", o.prefix, n as u64),
        _ => match o.style {
            NumberStyle::Percent => format!("{}%", fixed(n * 100., o.precision)),
            NumberStyle::Thousands => split(fixed(n, o.precision)),
            NumberStyle::Plain => fixed(n, o.precision),
        },
    }
}

/// The cell as the grid shows it and as copy / search / export read it.
/// 关联 cells need the other table: `display_in`.
pub fn display(_t: &Table, f: &Field, v: &Value) -> String { show(None, f, v) }

pub fn display_in(base: &Base, f: &Field, v: &Value) -> String { show(Some(base), f, v) }

fn show(base: Option<&Base>, f: &Field, v: &Value) -> String {
    match v {
        Value::Empty => String::new(),
        Value::Text(s) if f.ty == FieldType::Note => note_title(s).to_string(),
        Value::Text(s) => s.clone(),
        Value::Number(n) => format_number(*n, f),
        Value::Bool(b) => if *b { "✓".into() } else { String::new() },
        Value::Date(ms) => format_date(*ms, f.opts.date, f.opts.time),
        Value::Select(ids) => ids.iter().filter_map(|id| f.option(*id)).map(|o| o.name.as_str()).collect::<Vec<_>>().join(", "),
        Value::Links(ids) => {
            let other = base.and_then(|b| f.opts.link_table.and_then(|t| b.table(t)));
            ids.iter().map(|id| match other.and_then(|t| t.record(*id).map(|r| t.title(r))) {
                Some(t) if !t.is_empty() => t,
                Some(_) => "未命名记录".into(),
                None => format!("#{id}"),
            }).collect::<Vec<_>>().join(", ")
        }
        Value::Files(fs) => fs.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "),
        Value::List(vs) => vs.iter().map(|v| show(base, f, v)).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", "),
        Value::Error(e) => e.clone(),
    }
}

/// What typing `text` into a cell of `f` means. Unknown option names are
/// dropped (`parse_new` adds them).
pub fn parse(f: &Field, text: &str) -> Value {
    let t = text.trim();
    if t.is_empty() { return Value::Empty; }
    match f.ty {
        // a percentage column takes a bare number as a percentage, like 进度
        FieldType::Number if f.opts.style == NumberStyle::Percent && !t.ends_with('%') => parse_number(t).map(|n| Value::Number(n / 100.)).unwrap_or(Value::Empty),
        FieldType::Number | FieldType::Currency | FieldType::AutoNumber => parse_number(t).map(Value::Number).unwrap_or(Value::Empty),
        FieldType::Rating => parse_number(t).map(|n| Value::Number(n.round().clamp(0., f.stars() as f64))).unwrap_or(Value::Empty).normal_rating(),
        // a bare number is a percentage: `50` and `50%` are both half
        FieldType::Progress => parse_number(t).map(|n| Value::Number(if t.ends_with('%') { n } else { n / 100. })).unwrap_or(Value::Empty),
        FieldType::Checkbox => Value::Bool(matches!(t.to_lowercase().as_str(), "1" | "true" | "yes" | "y" | "✓" | "是" | "√" | "x" | "checked")).normal(),
        FieldType::Date | FieldType::CreatedTime | FieldType::ModifiedTime => parse_date(t).map(Value::Date).unwrap_or(Value::Empty),
        FieldType::SingleSelect => f.option_named(t).map(|o| Value::Select(vec![o.id])).unwrap_or(Value::Empty),
        FieldType::MultiSelect | FieldType::Person => {
            let mut ids = vec![];
            for n in split_list(t) { if let Some(o) = f.option_named(&n) && !ids.contains(&o.id) { ids.push(o.id); } }
            Value::Select(ids).normal()
        }
        FieldType::Link => Value::Links(split_list(t).iter().filter_map(|s| s.trim_start_matches('#').parse().ok()).collect()).normal(),
        FieldType::Attachment => Value::Files(split_list(t).into_iter().map(|p| Attachment { name: p.rsplit('/').next().unwrap_or(&p).to_string(), path: p }).collect()).normal(),
        // a note isn't typed into its cell: the cell is given the note's path (`note_href`)
        FieldType::Formula | FieldType::Lookup | FieldType::Note => Value::Empty,
        FieldType::Text | FieldType::Url | FieldType::Phone | FieldType::Email | FieldType::Location | FieldType::Barcode => Value::Text(text.to_string()),
    }
}

impl Value {
    fn normal_rating(self) -> Value { if matches!(self, Value::Number(n) if n == 0.) { Value::Empty } else { self } }
}

/// `a, b，c` → its parts (a JSON array of strings reads too).
pub fn split_list(t: &str) -> Vec<String> {
    if t.starts_with('[') && let Ok(v) = serde_json::from_str::<Vec<serde_json::Value>>(t) {
        return v.into_iter().map(|x| match x { serde_json::Value::String(s) => s, o => o.to_string() }).filter(|s| !s.is_empty()).collect();
    }
    t.split([',', '，', '、', ';', '；', '\n']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// `parse`, adding the options a select doesn't have yet (paste, import, a type change).
pub fn parse_new(f: &mut Field, text: &str, ids: &mut IdGen) -> Value {
    if f.ty.select() {
        let names = if f.ty == FieldType::SingleSelect { vec![text.trim().to_string()] } else { split_list(text) };
        for n in names.iter().filter(|n| !n.is_empty()) {
            if f.option_named(n).is_none() {
                let color = (f.opts.options.len() % 10) as u8;
                f.opts.options.push(SelectOption { id: ids.fresh(), name: n.clone(), color });
            }
        }
    }
    parse(f, text)
}

/// A value of `old` as a value of `new` (改字段类型): by its text, except
/// where the text would lose it.
pub fn convert(old: &Field, new: &mut Field, v: &Value, ids: &mut IdGen) -> Value {
    if v.is_empty() || new.ty.computed() { return Value::Empty; }
    match (v, new.ty) {
        (Value::Number(n), t) if t.numeric() && old.ty.numeric() => match t {
            FieldType::Rating => Value::Number(n.round().clamp(0., new.stars() as f64)).normal_rating(),
            _ => Value::Number(*n),
        },
        (Value::Number(n), FieldType::Checkbox) => Value::Bool(*n != 0.).normal(),
        (Value::Bool(b), t) if t.numeric() => Value::Number(*b as u8 as f64),
        (Value::Date(_), t) if t.is_date() => v.clone(),
        (Value::Select(ids), t) if t.select() && old.ty.select() => {
            // the options were carried over with their ids (`carry_options`)
            let kept: Vec<OptId> = ids.iter().copied().filter(|id| new.option(*id).is_some()).collect();
            Value::Select(if t == FieldType::SingleSelect { kept.into_iter().take(1).collect() } else { kept }).normal()
        }
        (Value::Links(_), FieldType::Link) if old.opts.link_table == new.opts.link_table => v.clone(),
        (Value::Files(_), FieldType::Attachment) => v.clone(),
        (Value::Text(_), FieldType::Note) if old.ty == FieldType::Note => v.clone(),
        // numbers go over unformatted, so `¥1,234.00` → 文本 → 数字 survives
        (Value::Number(n), _) if new.ty.textual() => Value::Text(format_number(*n, old)),
        (Value::Date(ms), _) if new.ty.textual() => Value::Text(format_date(*ms, old.opts.date, old.opts.time)),
        _ => {
            let text = match v { Value::Number(n) => plain_number(*n), Value::Date(ms) => iso(*ms, false), _ => show(None, old, v) };
            parse_new(new, &text, ids)
        }
    }
}

/// A 笔记 cell holds its note's path as the file has it: relative to the
/// folder the `.sqlite` is in, like a document's `<cite type="doc" href>`
/// (`layout::relative`), so the table and its notes can be moved together.
/// `table` and `note` are paths from the library's top.
pub fn note_href(table: &str, note: &str) -> String { crate::store::relative(folder_of(table), note) }

/// The note a 笔记 cell of the table at `table` points at, from the library's
/// top. None: not a path, or one that leaves the library.
pub fn note_rel(table: &str, href: &str) -> Option<String> { crate::store::resolve(folder_of(table), href) }

fn folder_of(rel: &str) -> &str { rel.rsplit_once('/').map_or("", |(d, _)| d) }

/// What a 笔记 cell shows where the note itself isn't at hand (formulas,
/// filters, export): its file's name. The app shows the note's own title.
pub fn note_title(href: &str) -> &str {
    let name = href.rsplit(['/', '\\']).next().unwrap_or(href);
    name.rsplit_once('.').filter(|(s, _)| !s.is_empty()).map_or(name, |(s, _)| s)
}

/// How two cells of one field order (升序). Empties are the caller's to place.
pub fn compare(f: &Field, a: &Value, b: &Value) -> Ordering {
    let rank = |v: &Value| match v {
        Value::Select(ids) => ids.first().and_then(|id| f.opts.options.iter().position(|o| o.id == *id)).map(|i| i as f64),
        Value::Number(n) => Some(*n),
        Value::Bool(b) => Some(*b as u8 as f64),
        Value::Date(d) => Some(*d as f64),
        _ => None,
    };
    match (rank(a), rank(b)) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        _ => text_cmp(&show(None, f, a), &show(None, f, b)),
    }
}

/// Text order that reads naturally: case-insensitive, digit runs by value (`a2` < `a10`).
pub fn text_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| { let mut n = 0u128; while let Some(c) = it.peek().and_then(|c| c.to_digit(10)) { n = n.saturating_mul(10).saturating_add(c as u128); it.next(); } n };
                match take(&mut x).cmp(&take(&mut y)) { Ordering::Equal => {} o => return o }
            }
            (Some(c), Some(d)) => {
                match c.to_lowercase().cmp(d.to_lowercase()) { Ordering::Equal => {} o => return o }
                x.next();
                y.next();
            }
        }
    }
}

/// A field of `ty` for a new column, with 飞书's defaults.
pub fn default_opts(ty: FieldType) -> FieldOpts {
    let mut o = FieldOpts::default();
    match ty {
        FieldType::Currency => { o.currency = "¥".into(); o.precision = Some(2); }
        FieldType::Progress => o.precision = Some(0),
        FieldType::Rating => o.max = 5,
        FieldType::CreatedTime | FieldType::ModifiedTime => o.time = true,
        _ => {}
    }
    o
}

/// Guess a column's type from its text cells (CSV import, a `.sqlite` without `_fields`).
pub fn infer_type<'a>(cells: impl Iterator<Item = &'a str>) -> FieldType {
    let (mut n, mut num, mut date, mut boolean, mut url, mut email) = (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    let mut distinct = std::collections::HashSet::new();
    let mut long = false;
    for c in cells.map(str::trim).filter(|c| !c.is_empty()) {
        n += 1;
        if parse_number(c).is_some() { num += 1; }
        if c.len() >= 6 && parse_date(c).is_some() && c.chars().any(|ch| !ch.is_ascii_digit() && ch != '.') { date += 1; }
        if matches!(c.to_lowercase().as_str(), "true" | "false" | "0" | "1") { boolean += 1; }
        if c.starts_with("http://") || c.starts_with("https://") { url += 1; }
        if c.contains('@') && c.contains('.') && !c.contains(' ') { email += 1; }
        long |= c.chars().count() > 30;
        if distinct.len() <= 24 { distinct.insert(c.to_string()); }
    }
    if n == 0 { return FieldType::Text; }
    if date == n { return FieldType::Date; }
    if boolean == n && distinct.iter().any(|d| d.eq_ignore_ascii_case("true") || d.eq_ignore_ascii_case("false")) { return FieldType::Checkbox; }
    if num == n { return FieldType::Number; }
    if url == n { return FieldType::Url; }
    if email == n { return FieldType::Email; }
    if !long && n >= 6 && distinct.len() <= 12 && distinct.len() * 3 <= n { return FieldType::SingleSelect; }
    FieldType::Text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(ty: FieldType) -> Field { Field { id: 1, name: "f".into(), ty, opts: default_opts(ty), desc: String::new() } }

    #[test]
    fn note_paths_go_both_ways() {
        for (table, note, href) in [("读书.sqlite", "三体.xml", "三体.xml"), ("读书/读书.sqlite", "读书/三体 2.xml", "三体 2.xml"), ("a/读书/读书.sqlite", "a/别处/笔记.xml", "../别处/笔记.xml"), ("a/b.sqlite", "c.xml", "../c.xml")] {
            assert_eq!(note_href(table, note), href);
            assert_eq!(note_rel(table, href).as_deref(), Some(note));
        }
        assert_eq!((note_rel("b.sqlite", "../c.xml"), note_rel("b.sqlite", "https://x/y"), note_rel("b.sqlite", "")), (None, None, None));
        assert_eq!((note_title("../别处/读书 笔记.xml"), note_title("a.b.xml"), note_title("无扩展名")), ("读书 笔记", "a.b", "无扩展名"));
        // a cell shows, sorts and converts by the title; nothing typed makes a note
        let note = f(FieldType::Note);
        let v = Value::Text("子/三体 2.xml".into());
        assert_eq!(show(None, &note, &v), "三体 2");
        assert_eq!((parse(&note, "三体"), compare(&note, &v, &Value::Text("z/三体 10.xml".into()))), (Value::Empty, Ordering::Less));
        let mut ids = IdGen::seeded(1);
        assert_eq!(convert(&note, &mut f(FieldType::Text), &v, &mut ids), Value::Text("三体 2".into()));
        assert_eq!(convert(&f(FieldType::Text), &mut f(FieldType::Note), &Value::Text("三体".into()), &mut ids), Value::Empty);
        assert_eq!(convert(&note, &mut f(FieldType::Note), &v, &mut ids), v);
        assert!(!FieldType::Note.keyable() && !FieldType::Note.computed() && !FieldType::Note.copies() && FieldType::from_key("note") == Some(FieldType::Note));
    }

    #[test]
    fn utc_times_go_both_ways() {
        let t = date_ms(2027, 1, 15, 0, 2) + 3123;
        assert_eq!(iso_utc(t), "2027-01-15T00:02:03.123Z");
        assert_eq!(parse_iso(&iso_utc(t)), Some(t));
        for ms in [0, -1, 1_800_000_000_999, date_ms(1969, 12, 31, 23, 59) + 59_999] { assert_eq!(parse_iso(&iso_utc(ms)), Some(ms), "{ms}"); }
        // as other programs write them
        assert_eq!(parse_iso("2027-01-15 08:02:03+08:00"), Some(t - 123));
        assert_eq!(parse_iso("2027-01-15T08:02:03.123+0800"), Some(t));
        assert_eq!(parse_iso("2027-01-14T19:02:03.123-05"), Some(t));
        assert_eq!(parse_iso("2027-01-15T00:02:03"), Some(t - 123));
        assert_eq!(parse_iso("2027-01-15 00:02"), Some(t - 3123));
        assert_eq!(parse_iso("2027-01-15T00:02:03.1234567Z"), Some(t));
        assert_eq!(parse_iso("2027-01-15T00:02:03.1Z"), Some(t - 23));
        assert_eq!(parse_iso("2027-01-15"), Some(date_ms(2027, 1, 15, 0, 0)));
        for bad in ["", "x", "2027-13-01", "2027-02-30", "27-01-15", "2027-01-15T25:00", "2027-01-15T00:02:03+8", "2027-01-15T0:2"] { assert_eq!(parse_iso(bad), None, "{bad}"); }
        // as strings, in time order
        assert!(iso_utc(t) < iso_utc(t + 1) && iso_utc(t + 1) < iso_utc(t + 60_000));
    }

    #[test]
    fn dates_read_and_show() {
        let ms = parse_date("2026-11-17").unwrap();
        assert_eq!(date_parts(ms), (2026, 11, 17, 0, 0));
        assert_eq!(parse_date("2026/1/5 9:30"), Some(date_ms(2026, 1, 5, 9, 30)));
        assert_eq!(parse_date("2026年1月30日"), Some(date_ms(2026, 1, 30, 0, 0)));
        assert_eq!(parse_date("2026-11-17T08:05:59.123Z"), Some(date_ms(2026, 11, 17, 8, 5)));
        assert_eq!(parse_date("01/30/2026"), Some(date_ms(2026, 1, 30, 0, 0)));
        assert_eq!(parse_date("2026-02-30"), None);
        assert_eq!(parse_date("hello"), None);
        assert_eq!(iso(ms, false), "2026-11-17");
        assert_eq!(iso(date_ms(1969, 12, 31, 23, 59), false), "1969-12-31 23:59");
        assert_eq!(format_date(ms, DateFmt::Cn, false), "2026年11月17日");
        assert_eq!(format_date(date_ms(2026, 1, 30, 7, 5), DateFmt::Slash, true), "2026/01/30 07:05");
        for fmt in DateFmt::ALL { assert_eq!(format_date(date_ms(2026, 1, 30, 0, 0), *fmt, false), fmt.sample()); }
    }

    #[test]
    fn numbers_read_and_show() {
        assert_eq!(parse_number("1,234.5"), Some(1234.5));
        assert_eq!(parse_number("¥12"), Some(12.));
        assert_eq!(parse_number("50%"), Some(0.5));
        assert_eq!(parse_number("abc"), None);
        assert_eq!(format_number(1234.5, &f(FieldType::Currency)), "¥1,234.50");
        assert_eq!(format_number(-1234567., &{ let mut x = f(FieldType::Number); x.opts.style = NumberStyle::Thousands; x }), "-1,234,567");
        assert_eq!(format_number(0.256, &{ let mut x = f(FieldType::Number); x.opts.style = NumberStyle::Percent; x.opts.precision = Some(1); x }), "25.6%");
        assert_eq!(format_number(0.5, &f(FieldType::Progress)), "50%");
        assert_eq!(format_number(3., &f(FieldType::Number)), "3");
        assert_eq!(format_number(0.1, &f(FieldType::Number)), "0.1");
        assert_eq!(parse(&f(FieldType::Progress), "50"), Value::Number(0.5));
        assert_eq!(parse(&f(FieldType::Rating), "9"), Value::Number(5.));
    }

    #[test]
    fn selects_parse_and_convert() {
        let mut s = f(FieldType::MultiSelect);
        let mut next = IdGen::seeded(1);
        let v = parse_new(&mut s, "红, 蓝，红", &mut next);
        let (r, b) = (s.option_named("红").unwrap().id, s.option_named("蓝").unwrap().id);
        assert!(r != b && (1..ID_LIMIT).contains(&r) && (1..ID_LIMIT).contains(&b));
        assert_eq!(v, Value::Select(vec![r, b]));
        assert_eq!(display(&Table::default(), &s, &Value::Select(vec![b, r])), "蓝, 红");
        assert_eq!(parse(&s, "绿"), Value::Empty);
        // 多选 → 单选 keeps the first; → 文本 its names; 文本 → 单选 makes options
        let mut single = Field { ty: FieldType::SingleSelect, ..s.clone() };
        assert_eq!(convert(&s, &mut single, &Value::Select(vec![b, r]), &mut next), Value::Select(vec![b]));
        let mut text = f(FieldType::Text);
        assert_eq!(convert(&s, &mut text, &Value::Select(vec![r, b]), &mut next), Value::Text("红, 蓝".into()));
        let mut sel = f(FieldType::SingleSelect);
        let v = convert(&text, &mut sel, &Value::Text("新".into()), &mut next);
        assert_eq!(sel.opts.options[0].name, "新");
        assert!(v == Value::Select(vec![sel.opts.options[0].id]) && ![r, b].contains(&sel.opts.options[0].id));
        let mut num = f(FieldType::Number);
        assert_eq!(convert(&text, &mut num, &Value::Text("1,200".into()), &mut next), Value::Number(1200.));
        let mut date = f(FieldType::Date);
        assert_eq!(convert(&text, &mut date, &Value::Text("2026-01-02".into()), &mut next), Value::Date(date_ms(2026, 1, 2, 0, 0)));
        assert_eq!(convert(&date, &mut text, &Value::Date(date_ms(2026, 1, 2, 0, 0)), &mut next), Value::Text("2026/01/02".into()));
    }

    #[test]
    fn natural_text_order() {
        assert_eq!(text_cmp("a2", "a10"), Ordering::Less);
        assert_eq!(text_cmp("B", "a"), Ordering::Greater);
        assert_eq!(text_cmp("abc", "abc"), Ordering::Equal);
    }

    #[test]
    fn column_types_are_guessed() {
        assert_eq!(infer_type(["1", "2.5", "-3"].into_iter()), FieldType::Number);
        assert_eq!(infer_type(["2026-01-02", "2026/3/4"].into_iter()), FieldType::Date);
        assert_eq!(infer_type(["true", "false", ""].into_iter()), FieldType::Checkbox);
        assert_eq!(infer_type(["https://a.b", "http://c.d"].into_iter()), FieldType::Url);
        assert_eq!(infer_type(["待办", "完成", "待办", "完成", "进行中", "待办", "完成", "待办", "完成"].into_iter()), FieldType::SingleSelect);
        assert_eq!(infer_type(["some text", "other"].into_iter()), FieldType::Text);
    }
}
