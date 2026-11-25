//! Markdown as 思源 exports it: kramdown attribute lists (`{: id="…"}`) on
//! their own lines or before an item's text, block references
//! `((id "text"))`, tags `#tag#` and embeds `{{SELECT …}}`. Each pattern is
//! narrow enough that Markdown from anywhere else passes through unchanged.

use regex::Regex;
use std::sync::LazyLock;

static IAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\{:\s*(?:[\w-]+="[^"]*"\s*)+\}"#).unwrap());
static IAL_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^\s*\{:\s*(?:[\w-]+="[^"]*"\s*)*\}\s*$"#).unwrap());
static REF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\(\((\d{14}-[0-9a-z]{7})\s+(?:"([^"\n]*)"|'([^'\n]*)')\)\)"#).unwrap());
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|[^A-Za-z0-9_#/&=?.\-])#([^\s#](?:[^#\n]*[^\s#])?)#").unwrap());
static ATTR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"([\w-]+)="([^"]*)""#).unwrap());

/// `text` with 思源's own syntax turned into Markdown: attribute lists
/// dropped, a reference as its text, `#tag#` as `#tag` (the way an Obsidian
/// tag stays), an embed's query as an `sql` code block. None when there is
/// none of it. Code is left alone.
pub fn markdown_text(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut fence: Option<String> = None;
    let mut changed = false;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let trimmed = body.trim_start();
        if let Some(f) = &fence {
            if trimmed.starts_with(f.as_str()) && trimmed.trim_start_matches(f.chars().next().unwrap_or('`')).trim().is_empty() { fence = None; }
            out.push_str(line);
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let c = trimmed.chars().next().unwrap_or('`');
            fence = Some(trimmed.chars().take_while(|x| *x == c).collect());
            out.push_str(line);
            continue;
        }
        if IAL_LINE.is_match(body) { changed = true; continue }
        if let Some(q) = trimmed.strip_prefix("{{").and_then(|r| r.strip_suffix("}}")) && q.trim_start().get(..6).is_some_and(|s| s.eq_ignore_ascii_case("select")) {
            out.push_str(&format!("```sql\n{}\n```\n", q.trim()));
            changed = true;
            continue;
        }
        let new = outside_code(body, |s| {
            let s = IAL.replace_all(s, "");
            let s = REF.replace_all(&s, |c: &regex::Captures| c.get(2).or(c.get(3)).map_or("", |m| m.as_str()).to_string());
            TAG.replace_all(&s, "$1#$2").into_owned()
        });
        if new != body { changed = true; }
        out.push_str(&new);
        out.push_str(&line[body.len()..]);
    }
    changed.then_some(out)
}

/// `f` applied to the parts of a line outside `code spans`.
fn outside_code(line: &str, f: impl Fn(&str) -> String) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(i) = rest.find('`') {
        out.push_str(&f(&rest[..i]));
        let ticks = rest[i..].chars().take_while(|c| *c == '`').count();
        let after = &rest[i + ticks..];
        match after.find(&"`".repeat(ticks)) {
            Some(j) => { out.push_str(&rest[i..i + ticks + j + ticks]); rest = &after[j + ticks..]; }
            None => { out.push_str(&rest[i..]); rest = ""; }
        }
    }
    out.push_str(&f(rest));
    out
}

/// When the page was made and last changed, from the attribute list 思源
/// writes for the document itself (`{: id="…" type="doc" updated="…"}`).
pub fn markdown_times(text: &str) -> (Option<u64>, Option<u64>) {
    for line in text.lines().rev() {
        if !IAL_LINE.is_match(line) { continue }
        let attrs: Vec<(&str, &str)> = ATTR.captures_iter(line).filter_map(|c| Some((c.get(1)?.as_str(), c.get(2)?.as_str()))).collect();
        let get = |k: &str| attrs.iter().find(|(a, _)| *a == k).map(|(_, v)| *v);
        if get("type") == Some("doc") { return (get("id").and_then(super::time), get("updated").and_then(super::time)) }
    }
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn siyuan_markdown_becomes_plain_markdown() {
        let md = "# 标题\n{: id=\"20240101120000-aaaaaaa\"}\n\n* {: id=\"20240101120000-bbbbbbb\"}一项\n\n见 ((20240101120000-ccccccc \"另一篇\")) 和 ((20240101120000-ddddddd '标题'))，#读书# #深度 阅读#。\n\n{{SELECT * FROM blocks WHERE content LIKE '%x%'}}\n\n`#不是标签#` ((20240101120000-eeeeeee))\n\n```\n{: id=\"20240101120000-fffffff\"}\n#代码#\n```\n{: id=\"20240101120000-ggggggg\" title=\"页\" type=\"doc\" updated=\"20240305081500\"}\n";
        let out = markdown_text(md).unwrap();
        assert_eq!(out, "# 标题\n\n* 一项\n\n见 另一篇 和 标题，#读书 #深度 阅读。\n\n```sql\nSELECT * FROM blocks WHERE content LIKE '%x%'\n```\n\n`#不是标签#` ((20240101120000-eeeeeee))\n\n```\n{: id=\"20240101120000-fffffff\"}\n#代码#\n```\n");
        let (made, changed) = markdown_times(md);
        assert_eq!(made, super::super::time("20240101120000"));
        assert_eq!(changed, super::super::time("20240305081500"));
        assert!(made.is_some() && changed > made);
    }

    #[test]
    fn other_markdown_is_left_alone() {
        for md in [
            "# Heading\n\n## Two #\n\nC# and F# are languages; see issue #12 and #34.\n",
            "A [link](http://x/#a#b) and {curly} and {: not an attribute list} and ((parens)) and ((20240101120000-aaaaaaa)).\n",
            "- item\n- [ ] task\n\n> quote #not a tag\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n{{template}}\n",
            "Text#with#hashes inside a word.\n",
        ] {
            assert_eq!(markdown_text(md), None, "{md}");
            assert_eq!(markdown_times(md), (None, None));
        }
    }
}
