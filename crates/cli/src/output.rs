//! What commands print: compact text for people and agents alike (`--json`
//! has every field), and the commit report.

use serde_json::Value;

fn s(v: &Value, k: &str) -> String { match v.get(k) { Some(Value::String(x)) => x.clone(), Some(Value::Null) | None => String::new(), Some(x) => x.to_string() } }

fn list<'a>(v: &'a Value, k: &str) -> impl Iterator<Item = &'a Value> { v.get(k).and_then(Value::as_array).into_iter().flatten() }

fn refs(v: &Value) -> String { v.as_array().map(|l| l.iter().map(|r| r.as_str().map_or_else(|| s(r, "ref"), str::to_string)).collect::<Vec<_>>().join(" ")).unwrap_or_default() }

fn one_line(t: &str, n: usize) -> String {
    let t = t.replace('\n', " ⏎ ");
    if t.chars().count() > n { format!("{}…", t.chars().take(n).collect::<String>()) } else { t }
}

fn block_line(b: &Value) -> String {
    let kind = match (s(b, "kind").as_str(), b.get("done").and_then(Value::as_bool)) { ("todo", Some(true)) => "todo[x]".to_string(), ("todo", _) => "todo[ ]".to_string(), (k, _) => k.to_string() };
    let indent = "  ".repeat(b.get("depth").and_then(Value::as_u64).unwrap_or(0) as usize);
    format!("{:<12} {indent}{kind:<8} {}\n", s(b, "ref"), one_line(&s(b, "text"), 160))
}

fn size(n: u64) -> String { if n >= 1000 { format!("{:.1}k", n as f64 / 1000.) } else { n.to_string() } }

fn cell(v: &Value, k: &str) -> String { s(v, k).replace('|', "\\|").replace('\n', " ") }

/// Compact text for people and agents alike; `--json` has every field.
/// Paths start at 主页: the top-level folder's name first.
pub fn text(op: &str, v: &Value) -> String {
    let mut o = String::new();
    match op {
        "ls" => for e in v.as_array().into_iter().flatten() {
            let mark = format!("{}{}", if e.get("pinned").is_some() { " pinned" } else { "" }, if e.get("starred").is_some() { " starred" } else { "" });
            if e.get("top").is_some() {
                // A top-level folder on 主页: where it is on disk, and whether it opens.
                let state = match e.get("documents") { Some(n) => format!("{n} documents{}", if e.get("default").is_some() { ", new documents go here" } else { "" }), None => format!("unavailable: {}", s(e, "error")) };
                o += &format!("{}/  ({state})  {}\n", s(e, "path"), s(e, "dir"));
            } else if s(e, "kind") == "folder" { o += &format!("{}/\n", s(e, "path")); } else { o += &format!("{}  [{}] {}{mark}\n", s(e, "path"), s(e, "kind"), s(e, "modified")); }
        },
        "find" => {
            let res: Vec<&Value> = list(v, "results").collect();
            let total = v["total"].as_u64().unwrap_or(0) as usize;
            o += &format!("{total} documents match{}\n", if res.len() < total { format!(" (first {} shown; --limit for more)", res.len()) } else { String::new() });
            for r in res {
                o += &format!("\n{}  rev {}\n", s(r, "path"), s(r, "rev"));
                for h in list(r, "hits") { o += &format!("  {:<12} {:<8} {}\n", s(h, "ref"), s(h, "kind"), s(h, "text")); }
                if let Some(n) = r.get("more_hits") { o += &format!("  … {n} more blocks match (quiet blocks DOC --grep WORD)\n"); }
            }
        }
        "outline" | "open" => {
            o += &format!("{}  [{}] rev {}  {} blocks, {} chars\n", s(v, "path"), s(v, "kind"), s(v, "rev"), s(v, "block_count"), size(v["chars"].as_u64().unwrap_or(0)));
            for h in list(v, "headings") {
                let l = h["level"].as_u64().unwrap_or(1) as usize;
                o += &format!("{:<12} {}{} {}  ({})\n", s(h, "ref"), "  ".repeat(l.saturating_sub(1)), "#".repeat(l), s(h, "text"), size(h["chars"].as_u64().unwrap_or(0)));
            }
            for b in list(v, "top") { o += &format!("{:<12} {:<8} {}\n", s(b, "ref"), s(b, "kind"), s(b, "text")); }
        }
        "read" | "blocks" if v.get("blocks").is_some() => { o += &format!("{}  rev {}\n", s(v, "path"), s(v, "rev")); for b in list(v, "blocks") { o += &block_line(b); } }
        "read" => {
            o += &s(v, "text");
            if !o.ends_with('\n') { o.push('\n'); }
            if v.get("truncated").is_some() { o += "…[truncated: read a section with DOC#REF --section (refs from `quiet outline`), or pass --max-chars N (0: all)]\n"; }
        }
        "links" => {
            o += &format!("{}\nout:\n", s(v, "path"));
            for l in list(v, "out") { o += &format!("  → {}  {}\n", if l["broken"].as_bool() == Some(true) { "(broken)".to_string() } else { s(l, "to") }, s(l, "text")); }
            o += "back:\n";
            for l in list(v, "back") { o += &format!("  ← {}  {}\n", s(l, "from"), l["text"].as_array().map(|t| t.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" / ")).unwrap_or_default()); }
        }
        "table" => {
            let fields: Vec<String> = list(v, "fields").filter_map(Value::as_str).map(str::to_string).collect();
            let types: Vec<String> = list(v, "schema").map(|f| format!("{}: {}{}", s(f, "name"), s(f, "type"), if f.get("computed").is_some() { " (computed)" } else { "" })).collect();
            o += &format!("{} › {}  ({} rows)\nfields: {}\n| _id | {} |\n| --- |{}\n", s(v, "src"), s(v, "table"), s(v, "total"), types.join(", "), fields.join(" | "), " --- |".repeat(fields.len()));
            for r in list(v, "rows") { o += &format!("| {} | {} |\n", s(r, "_id"), fields.iter().map(|f| cell(r, f)).collect::<Vec<_>>().join(" | ")); }
            if let Some(n) = v.get("more") { o += &format!("… {n} more rows (--offset / --limit)\n"); }
        }
        "board" => {
            let labels = list(v, "elements").filter(|e| e.get("in").is_some()).count();
            o += &format!("{}  rev {}  {} elements{}", s(v, "path"), s(v, "rev"), list(v, "elements").count() - labels, if labels > 0 { format!(" (+{labels} labels, shown on their shapes)") } else { String::new() });
            if let Some(b) = v.get("bounds") { o += &format!(", within x {} y {} w {} h {}", s(b, "x"), s(b, "y"), s(b, "w"), s(b, "h")); }
            o += "\n";
            for e in list(v, "elements") {
                if e.get("in").is_some() { continue; } // labels show on their container
                let what = [e.get("label"), e.get("text"), e.get("name")].into_iter().flatten().filter_map(Value::as_str).map(|t| format!("{:?}", one_line(t, 80))).collect::<Vec<_>>().join(" ");
                let ends = match (e.get("from"), e.get("to")) { (None, None) => String::new(), (f, t) => format!(" {} → {}", f.and_then(Value::as_str).unwrap_or("·"), t.and_then(Value::as_str).unwrap_or("·")) };
                o += &format!("{:<22} {:<10} {},{} {}×{} {what}{ends}\n", s(e, "id"), s(e, "type"), s(e, "x"), s(e, "y"), s(e, "w"), s(e, "h"));
            }
        }
        "history" if v.get("diff").is_some() => {
            o += &format!("{}: from version {} ({}) to now\n", s(v, "path"), s(v, "version"), s(v, "time"));
            for l in list(v, "diff") { o += &format!("  {}\n", l.as_str().unwrap_or("")); }
        }
        "history" => {
            let vs: Vec<&Value> = list(v, "versions").collect();
            o += &format!("{}: {} versions, newest first. Each is the text before a save; \"then\" is what that save changed (lines added/removed).\n", s(v, "path"), vs.len());
            for x in vs { o += &format!("{:>3}  {}  then {:<10} {:<28} {}{}\n", s(x, "n"), s(x, "time"), s(x, "then"), s(x, "version"), size(x["size"].as_u64().unwrap_or(0)), x.get("label").and_then(Value::as_str).map(|l| format!("  “{l}”")).unwrap_or_default()); }
        }
        "trash_list" => {
            let items: Vec<&Value> = list(v, "items").collect();
            o += &format!("{} in the trash (kept {} days)\n", items.len(), s(v, "days"));
            for t in items { o += &format!("{}  {}/{}  ({})\n", s(t, "deleted"), s(t, "folder"), s(t, "title"), s(t, "id")); }
        }
        "check" => {
            let issues: Vec<&Value> = list(v, "issues").collect();
            o += &format!("{} documents, {} issues\n", s(v, "documents"), issues.len());
            for i in issues { o += &format!("{:<14} {}  {}\n", s(i, "code"), s(i, "path"), s(i, "message")); }
        }
        "insert" => o += &format!("inserted {}\n", refs(&v["inserted"])),
        "replace" => o += &format!("replaced {} block(s) with {}\n", s(v, "replaced"), refs(&v["inserted"])),
        "remove" => o += &format!("removed {} block(s)\n", s(v, "removed")),
        "move" => o += &format!("moved to {}\n", refs(&v["moved"])),
        "set" => o += &format!("set {}\n", refs(&v["set"])),
        "table_add" => o += &format!("added {}\n", refs(&v["added"])),
        "table_update" => o += &format!("updated {}\n", refs(&v["updated"])),
        "table_delete" => o += &format!("deleted {} row(s)\n", s(v, "deleted")),
        "board_add" => o += &format!("added {}\n", refs(&v["added"])),
        "board_remove" => o += &format!("removed {} element(s)\n", s(v, "removed")),
        "create" | "mkdir" | "rename" | "mv" | "trash" | "restore" | "untrash" | "import" => {}
        _ => o += &format!("{}\n", serde_json::to_string_pretty(v).unwrap_or_default()),
    }
    o
}

/// What a commit wrote (or, `dry`, would write), by paths from 主页.
pub fn report(r: &Value, dry: bool) -> String {
    let mut o = String::new();
    for c in r.as_array().into_iter().flatten() {
        let rev = match (c.get("rev_before"), c.get("rev")) { (Some(a), Some(b)) => format!("  rev {} → {}", a.as_str().unwrap_or(""), b.as_str().unwrap_or("")), (None, Some(b)) => format!("  rev {}", b.as_str().unwrap_or("")), _ => String::new() };
        let extra = c.get("from").map(|f| format!("  (was {})", f.as_str().unwrap_or(""))).or_else(|| c.get("source").map(|f| format!("  (from {})", f.as_str().unwrap_or("")))).or_else(|| c.get("to").map(|t| format!(" → {}", t.as_str().unwrap_or("")))).unwrap_or_default();
        o += &format!("{} {}{extra}{rev}\n", s(c, "action"), s(c, "path"));
        for l in list(c, "diff").chain(list(c, "changes")) { o += &format!("    {}\n", l.as_str().unwrap_or("")); }
    }
    if dry { o += if r.as_array().is_some_and(|l| !l.is_empty()) { "(dry run: nothing written)\n" } else { "(dry run: nothing would change)\n" }; }
    o
}
