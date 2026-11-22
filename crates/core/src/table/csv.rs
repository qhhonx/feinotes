//! CSV / TSV in and out, the clipboard's grid, a view as Markdown, and a
//! table's text for search.

use super::field;
use super::model::*;

/// RFC 4180: quoted cells may hold the separator, line breaks and `""` for a
/// quote. A leading BOM is dropped; `\r\n`, `\n` and `\r` all end a row. A
/// quote that doesn't open a well-formed quoted cell is just a character.
pub fn parse(text: &str, sep: char) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = vec![];
    let mut row = vec![];
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let (cell, len) = quoted(rest, sep).unwrap_or_else(|| {
            let len = rest.find([sep, '\n', '\r']).unwrap_or(rest.len());
            (rest[..len].to_string(), len)
        });
        row.push(cell);
        i += len;
        let rest = &text[i..];
        if rest.starts_with(sep) {
            i += sep.len_utf8();
            // a separator at the very end still has an (empty) cell after it
            if i == text.len() { row.push(String::new()); }
            continue;
        }
        i += if rest.starts_with("\r\n") { 2 } else if rest.is_empty() { 0 } else { 1 };
        rows.push(std::mem::take(&mut row));
    }
    if !row.is_empty() { rows.push(row); }
    rows
}

/// The quoted cell `s` starts with and its length in `s`; None when the
/// quotes don't close right before a separator, a line break or the end.
fn quoted(s: &str, sep: char) -> Option<(String, usize)> {
    let mut it = s.strip_prefix('"')?.char_indices().peekable();
    let mut cell = String::new();
    while let Some((i, c)) = it.next() {
        match c {
            '"' if it.peek().is_some_and(|(_, n)| *n == '"') => { cell.push('"'); it.next(); }
            '"' => return it.peek().is_none_or(|(_, n)| *n == sep || *n == '\n' || *n == '\r').then_some((cell, i + 2)),
            // line breaks inside a cell are `\n`, whatever the file used
            '\r' => { cell.push('\n'); it.next_if(|(_, n)| *n == '\n'); }
            c => cell.push(c),
        }
    }
    None
}

/// Rows as CSV / TSV, each ending in `\n`. Cells holding the separator, a
/// quote or a line break are quoted.
pub fn write(rows: &[Vec<String>], sep: char) -> String {
    let mut out = String::new();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i > 0 { out.push(sep); }
            if cell.contains([sep, '"', '\n', '\r']) {
                out.push('"');
                out.push_str(&cell.replace('"', "\"\""));
                out.push('"');
            } else { out.push_str(cell); }
        }
        out.push('\n');
    }
    out
}

/// Clipboard text as a grid: TSV when it has tabs (what spreadsheets and our
/// own copy put there), otherwise one cell per line. Never empty: a single
/// value is 1×1.
pub fn parse_clipboard(text: &str) -> Vec<Vec<String>> {
    let rows = if text.contains('\t') { parse(text, '\t') } else {
        let t = text.strip_suffix('\n').unwrap_or(text);
        let t = t.strip_suffix('\r').unwrap_or(t);
        t.replace("\r\n", "\n").split(['\n', '\r']).map(|l| vec![l.to_string()]).collect()
    };
    if rows.is_empty() { vec![vec![String::new()]] } else { rows }
}

/// A name the file can carry (`sqlite_name_ok`) that `taken` doesn't hold,
/// as close to `want` as that allows. `fallback` (a valid name) stands in
/// for an empty one.
pub(super) fn safe_name(want: &str, fallback: &str, taken: impl Fn(&str) -> bool) -> String {
    let w = want.replace('\0', "");
    let w = w.trim().trim_start_matches('_').trim_start();
    let w = if w.is_empty() { fallback.to_string() } else if w.to_ascii_lowercase().starts_with("sqlite_") { format!("{fallback} {w}") } else { w.to_string() };
    free_name(&w, |n| super::sqlite_name_ok(n) && !taken(n))
}

/// `,`, tab or `;`: whichever the first line has most of.
fn separator(text: &str) -> char {
    let line = text.lines().next().unwrap_or("");
    let count = |c: char| line.matches(c).count();
    if count('\t') > count(',').max(count(';')) { '\t' } else if count(';') > count(',') { ';' } else { ',' }
}

/// CSV (or TSV) text as a new table, not yet in `base` (which hands out its
/// ids). The first row names the columns; each column's type is guessed
/// from its cells, and 单选 columns get their options.
pub fn import(base: &mut Base, name: &str, text: &str) -> Table {
    let mut rows = parse(text, separator(text));
    rows.retain(|r| r.iter().any(|c| !c.trim().is_empty()));
    let header = if rows.is_empty() { vec![] } else { rows.remove(0) };
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0).max(header.len()).max(1);
    let name = safe_name(name, "数据表", |n| base.tables.iter().any(|t| t.name.eq_ignore_ascii_case(n)));
    let mut t = Table { id: base.new_id(), name, next_rec: 1, next_auto: 1, ..Default::default() };
    for c in 0..cols {
        let cells = || rows.iter().filter_map(move |r| r.get(c)).map(String::as_str);
        let mut ty = field::infer_type(cells());
        // the first column is the title: nothing that can't be one
        if c == 0 && matches!(ty, FieldType::SingleSelect | FieldType::Checkbox) { ty = FieldType::Text; }
        let mut opts = field::default_opts(ty);
        match ty {
            FieldType::Date => opts.time = cells().filter_map(field::parse_date).any(|ms| ms.rem_euclid(86_400_000) != 0),
            FieldType::Number if cells().any(|s| !s.trim().is_empty()) && cells().all(|s| s.trim().is_empty() || s.trim().ends_with('%')) => opts.style = NumberStyle::Percent,
            _ => {}
        }
        let want = header.get(c).map(String::as_str).unwrap_or("");
        let name = safe_name(want, &format!("字段{}", c + 1), |n| t.fields.iter().any(|f| f.name.eq_ignore_ascii_case(n)));
        t.fields.push(Field { id: base.new_id(), name, ty, opts, desc: String::new() });
    }
    let now = field::now();
    for row in &rows {
        let mut r = Record { id: t.next_rec, created: now, modified: now, auto: t.next_auto, ..Default::default() };
        for (f, cell) in t.fields.iter_mut().zip(row) { r.set(f.id, field::parse_new(f, cell, &mut base.next_id)); }
        t.records.push(r);
        t.next_rec += 1;
        t.next_auto += 1;
    }
    t.views.push(View { id: base.new_id(), name: "表格视图".into(), kind: ViewKind::Grid, opts: ViewOpts { frozen: 1, ..Default::default() } });
    t
}

/// A view as a grid of text: the visible fields' names, then each of
/// `records` (indexes into `t.records`, a query's result) as the cells show.
pub fn export(base: &Base, t: &Table, view: &View, records: &[usize]) -> Vec<Vec<String>> {
    let fields = t.visible_fields(view);
    let mut out = vec![fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>()];
    out.extend(records.iter().filter_map(|i| t.records.get(*i)).map(|r| fields.iter().map(|f| field::display_in(base, f, r.get(f.id))).collect()));
    out
}

/// A view as a GFM table. An empty one still has its header and rule.
pub fn to_markdown(base: &Base, t: &Table, view: &View, records: &[usize]) -> String {
    let cell = |s: &String| { let s = s.replace('|', "\\|").replace("\r\n", "<br>").replace(['\n', '\r'], "<br>"); if s.is_empty() { " ".to_string() } else { format!(" {s} ") } };
    let mut out = String::new();
    for (i, row) in export(base, t, view, records).iter().enumerate() {
        if row.is_empty() { return String::new(); }
        out.push('|');
        for c in row { out.push_str(&cell(c)); out.push('|'); }
        out.push('\n');
        if i == 0 { out.push_str(&format!("|{}\n", " --- |".repeat(row.len()))); }
    }
    out
}

/// Every cell's text, a record per line, for full-text search.
pub fn plain_text(base: &Base, t: &Table) -> String {
    let mut out = String::new();
    for r in &t.records {
        let line = t.fields.iter().map(|f| field::display_in(base, f, r.get(f.id))).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
        if line.is_empty() { continue; }
        out.push_str(&line.replace(['\n', '\r'], " "));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: &[&[&str]]) -> Vec<Vec<String>> { rows.iter().map(|r| r.iter().map(|c| c.to_string()).collect()).collect() }

    #[test]
    fn parses_quotes_and_line_endings() {
        assert_eq!(parse("a,b\n1,2\n", ','), grid(&[&["a", "b"], &["1", "2"]]));
        assert_eq!(parse("\u{feff}a,b\r\n1,2", ','), grid(&[&["a", "b"], &["1", "2"]]));
        assert_eq!(parse("a\rb\r", ','), grid(&[&["a"], &["b"]]));
        assert_eq!(parse("\"x,y\",\"say \"\"hi\"\"\",\"two\r\nlines\"\n", ','), grid(&[&["x,y", "say \"hi\"", "two\nlines"]]));
        // empty cells, an empty line, a trailing separator
        assert_eq!(parse("a,,c\n\n,\n", ','), grid(&[&["a", "", "c"], &[""], &["", ""]]));
        assert_eq!(parse("a,b,", ','), grid(&[&["a", "b", ""]]));
        assert_eq!(parse("\"\",x", ','), grid(&[&["", "x"]]));
        // quotes that aren't a quoted cell stay as typed
        assert_eq!(parse("\"hi\" there,5\" disk,\"open", ','), grid(&[&["\"hi\" there", "5\" disk", "\"open"]]));
        assert_eq!(parse("a\tb,c\n", '\t'), grid(&[&["a", "b,c"]]));
        assert_eq!(parse("", ','), Vec::<Vec<String>>::new());
    }

    #[test]
    fn writes_and_reads_back() {
        let rows = grid(&[&["名称", "备注", ""], &["a,b", "say \"hi\"", "two\nlines"], &["", "\t", " padded "], &["\"", "\r", "，"]]);
        for sep in [',', '\t'] {
            let text = write(&rows, sep);
            let mut want = rows.clone();
            want[3][1] = "\n".into();
            assert_eq!(parse(&text, sep), want, "{text:?}");
        }
        assert_eq!(write(&grid(&[&["a", "b c", "d,e"], &["1", "\"", ""]]), ','), "a,b c,\"d,e\"\n1,\"\"\"\",\n");
    }

    #[test]
    fn clipboard_grids() {
        assert_eq!(parse_clipboard("hello"), grid(&[&["hello"]]));
        assert_eq!(parse_clipboard(""), grid(&[&[""]]));
        assert_eq!(parse_clipboard("a, \"b\"\nc\r\n"), grid(&[&["a, \"b\""], &["c"]]));
        assert_eq!(parse_clipboard("a\tb\n1\t2\n"), grid(&[&["a", "b"], &["1", "2"]]));
        assert_eq!(parse_clipboard("x\t\"two\nlines\"\ny\t"), grid(&[&["x", "two\nlines"], &["y", ""]]));
    }

    #[test]
    fn import_guesses_types_and_mends_names() {
        let mut base = Base::blank(0);
        let before = base.next_id;
        let mut text = String::from("\u{feff}书名,状态,页数,开始,读完,链接,状态,,_内部,id,占比\n");
        for i in 0..9 {
            text.push_str(&format!("书 {i},{},{},2026-01-{:02}{},{},https://example.com/{i},x{i},备注,q,{i},{}%\n",
                ["想读", "在读", "读完"][i % 3], 100 + i * 10, i + 1, if i == 2 { " 09:30" } else { "" }, i % 2 == 0, i * 10));
        }
        text.push_str(",,,,,,,,,,\n");
        let t = import(&mut base, "未命名数据表", &text);
        // the name is free in the base, ids are fresh
        assert_eq!(t.name, "未命名数据表 2");
        assert!(t.id > before && t.fields.iter().all(|f| f.id > before) && base.next_id >= t.views[0].id);
        let names: Vec<&str> = t.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["书名", "状态", "页数", "开始", "读完", "链接", "状态 2", "字段8", "内部", "id 2", "占比"]);
        assert!(names.iter().all(|n| crate::table::sqlite_name_ok(n)));
        let types: Vec<FieldType> = t.fields.iter().map(|f| f.ty).collect();
        use FieldType::*;
        assert_eq!(types, [Text, SingleSelect, Number, Date, Checkbox, Url, Text, SingleSelect, SingleSelect, Number, Number]);
        assert_eq!(t.records.len(), 9);
        assert_eq!((t.next_rec, t.next_auto), (10, 10));
        let (state, pages, start, done, share) = (&t.fields[1], &t.fields[2], &t.fields[3], &t.fields[4], &t.fields[10]);
        assert_eq!(state.opts.options.iter().map(|o| o.name.as_str()).collect::<Vec<_>>(), ["想读", "在读", "读完"]);
        assert_eq!(state.opts.options.iter().map(|o| o.color).collect::<Vec<_>>(), [0, 1, 2]);
        let r = &t.records[2];
        assert_eq!(r.get(t.fields[0].id), &Value::Text("书 2".into()));
        assert_eq!(r.get(state.id), &Value::Select(vec![state.opts.options[2].id]));
        assert_eq!(r.get(pages.id), &Value::Number(120.));
        assert_eq!(r.get(start.id), &Value::Date(field::date_ms(2026, 1, 3, 9, 30)));
        assert!(start.opts.time);
        assert_eq!(r.get(done.id), &Value::Bool(true));
        assert_eq!(t.records[1].get(done.id), &Value::Empty);
        assert_eq!((share.opts.style, r.get(share.id)), (NumberStyle::Percent, &Value::Number(0.2)));
        assert_eq!((t.views.len(), t.views[0].kind, t.views[0].opts.frozen), (1, ViewKind::Grid, 1));

        // TSV and `;` are told apart by the first line; nothing at all is still a table
        let t = import(&mut base, "_x", "a\tb\nx,y\t2\n");
        assert_eq!((t.name.as_str(), t.fields.len(), t.records[0].get(t.fields[0].id)), ("x", 2, &Value::Text("x,y".into())));
        let t = import(&mut base, "", "a;b\n1;2\n");
        assert_eq!((t.name.as_str(), t.fields.len()), ("数据表", 2));
        let t = import(&mut base, "空", "");
        assert_eq!((t.fields.len(), t.fields[0].name.as_str(), t.fields[0].ty, t.records.len(), t.views.len()), (1, "字段1", Text, 0, 1));
        // a first column of few distinct values is still text
        let t = import(&mut base, "t", "kind\na\nb\na\nb\na\nb\na\nb\n");
        assert_eq!(t.fields[0].ty, Text);
    }

    fn shelf() -> Base {
        let mut b = Base::default();
        let mut t = import(&mut b, "书", "书名,价格,备注,读完\nA|B,1234.5,\"第一行\n第二行\",true\n,,,false\nC,8,plain,true\n");
        t.fields[1].ty = FieldType::Currency;
        t.fields[1].opts = field::default_opts(FieldType::Currency);
        b.tables.push(t);
        b
    }

    #[test]
    fn export_shows_what_the_view_shows() {
        let mut b = shelf();
        let hide = b.tables[0].fields[3].id;
        let (note, price) = (b.tables[0].fields[2].id, b.tables[0].fields[1].id);
        b.tables[0].views[0].opts.hidden.insert(hide);
        b.tables[0].views[0].opts.order = vec![note, price];
        let t = &b.tables[0];
        let rows = export(&b, t, &t.views[0], &[2, 0, 9]);
        assert_eq!(rows, grid(&[&["书名", "备注", "价格"], &["C", "plain", "¥8.00"], &["A|B", "第一行\n第二行", "¥1,234.50"]]));
        assert_eq!(parse(&write(&rows, ','), ','), rows);
    }

    #[test]
    fn markdown_escapes_and_keeps_its_header() {
        let b = shelf();
        let t = &b.tables[0];
        assert_eq!(to_markdown(&b, t, &t.views[0], &[0, 1]),
            "| 书名 | 价格 | 备注 | 读完 |\n| --- | --- | --- | --- |\n| A\\|B | ¥1,234.50 | 第一行<br>第二行 | ✓ |\n| | | | |\n");
        assert_eq!(to_markdown(&b, t, &t.views[0], &[]), "| 书名 | 价格 | 备注 | 读完 |\n| --- | --- | --- | --- |\n");
    }

    #[test]
    fn plain_text_is_a_line_per_record() {
        let b = shelf();
        assert_eq!(plain_text(&b, &b.tables[0]), "A|B ¥1,234.50 第一行 第二行 ✓\nC ¥8.00 plain ✓\n");
    }
}
