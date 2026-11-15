//! Interface languages beyond Chinese and English. Each table maps the
//! Chinese string — the key `tr`, `text` and `localized_format!` already
//! carry — to its translation; anything missing shows in English.
use crate::settings::Lang;
use std::{collections::HashMap, sync::LazyLock};

mod fr;
mod ar;
mod es;
mod ja;
mod ko;

fn strings(lang: Lang) -> &'static [(&'static str, &'static str)] {
    match lang { Lang::Ja => ja::STRINGS, Lang::Ko => ko::STRINGS, Lang::Fr => fr::STRINGS, Lang::Es => es::STRINGS, Lang::Ar => ar::STRINGS, Lang::Zh | Lang::En => &[] }
}

pub fn get(lang: Lang, zh: &str) -> Option<&'static str> {
    type Table = LazyLock<HashMap<&'static str, &'static str>>;
    static JA: Table = LazyLock::new(|| ja::STRINGS.iter().copied().collect());
    static KO: Table = LazyLock::new(|| ko::STRINGS.iter().copied().collect());
    static ES: Table = LazyLock::new(|| es::STRINGS.iter().copied().collect());
    static AR: Table = LazyLock::new(|| ar::STRINGS.iter().copied().collect());
    static FR: Table = LazyLock::new(|| fr::STRINGS.iter().copied().collect());
    match lang { Lang::Ja => &JA, Lang::Ko => &KO, Lang::Fr => &FR, Lang::Es => &ES, Lang::Ar => &AR, Lang::Zh | Lang::En => return None }.get(zh).copied()
}

/// `text` was formatted from the Chinese `template`: read its values back
/// out and put them into `lang`'s version of the template. A translation
/// names a placeholder as the template does — `{n}`, or `{}` / `{0}` `{1}` …
/// for the unnamed ones in order; format specs only apply in the template.
pub fn refill(lang: Lang, template: &str, text: &str) -> Option<String> {
    let target = get(lang, template)?;
    fill(target, &read_back(template, text)?)
}

/// `refill` against every template of `lang` (core diagnostics arrive already formatted).
pub fn refill_any(lang: Lang, text: &str) -> Option<String> {
    strings(lang).iter().filter(|(zh, _)| zh.contains('{')).find_map(|(zh, _)| refill(lang, zh, text))
}

#[derive(Debug, PartialEq)]
enum Piece { Lit(String), Arg(String) }

/// A `format!` string as literal runs and placeholder keys: the name, or the
/// position of an unnamed one.
fn pieces(template: &str) -> Option<Vec<Piece>> {
    let (mut out, mut lit, mut next) = (Vec::new(), String::new(), 0);
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => { chars.next(); lit.push('{'); }
            '}' if chars.peek() == Some(&'}') => { chars.next(); lit.push('}'); }
            '{' => {
                let mut inner = String::new();
                loop { match chars.next()? { '}' => break, c => inner.push(c) } }
                let name = inner.split(':').next().unwrap_or("").trim();
                let key = if name.is_empty() { next += 1; (next - 1).to_string() } else { name.to_string() };
                if !lit.is_empty() { out.push(Piece::Lit(std::mem::take(&mut lit))); }
                out.push(Piece::Arg(key));
            }
            c => lit.push(c),
        }
    }
    if !lit.is_empty() { out.push(Piece::Lit(lit)); }
    Some(out)
}

fn read_back(template: &str, text: &str) -> Option<HashMap<String, String>> {
    let pieces = pieces(template)?;
    let (mut values, mut rest, mut i) = (HashMap::new(), text, 0);
    while i < pieces.len() {
        match &pieces[i] {
            Piece::Lit(s) => rest = rest.strip_prefix(s.as_str())?,
            Piece::Arg(key) => {
                let value = match pieces.get(i + 1) {
                    None => std::mem::take(&mut rest),
                    // The last literal is matched from the end, so a value may contain it.
                    Some(Piece::Lit(s)) if i + 2 == pieces.len() => { let v = rest.strip_suffix(s.as_str())?; rest = ""; v }
                    Some(Piece::Lit(s)) => { let (v, next) = rest.split_once(s.as_str())?; rest = next; v }
                    Some(Piece::Arg(_)) => return None,
                };
                if values.insert(key.clone(), value.to_string()).is_some_and(|old| old != value) { return None; }
                i += 1;
            }
        }
        i += 1;
    }
    rest.is_empty().then_some(values)
}

fn fill(target: &str, values: &HashMap<String, String>) -> Option<String> {
    pieces(target)?.into_iter().map(|p| match p { Piece::Lit(s) => Some(s), Piece::Arg(key) => values.get(&key).cloned() }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_read_back_into_the_translation() {
        let values = read_back("{vy}年 {vm}月", "2026年 11月").unwrap();
        assert_eq!(fill("{vy}년 {vm}월", &values).as_deref(), Some("2026년 11월"));
        let values = read_back("无法清理过期回收站文档 {}：{}", "无法清理过期回收站文档 笔记：甲.xml：Permission denied").unwrap();
        assert_eq!(fill("{1} — {0}", &values).as_deref(), Some("甲.xml：Permission denied — 笔记"), "middle literals split at their first match");
        let values = read_back("已删除「{}」", "已删除「a」b」").unwrap();
        assert_eq!(values["0"], "a」b", "the last literal is matched from the end");
        assert_eq!(read_back("{h:02}:{mi:02}", "09:05").unwrap()["mi"], "05");
        assert!(read_back("{a}{b}", "12").is_none(), "adjacent placeholders are ambiguous");
        assert!(read_back("已选 {n} 项", "Selected: 3").is_none());
        assert!(fill("{missing}", &values).is_none());
        assert_eq!(pieces("{{x}} {}").unwrap(), vec![Piece::Lit("{x} ".into()), Piece::Arg("0".into())]);
    }

    /// A placeholder the Chinese template doesn't have would make the
    /// translation fall back to English every time.
    #[test]
    fn translations_only_use_their_templates_placeholders() {
        for lang in Lang::ALL {
            for (zh, tr) in strings(lang) {
                let keys = |s: &str| -> Vec<String> { pieces(s).unwrap_or_else(|| panic!("bad template {s:?}")).into_iter().filter_map(|p| match p { Piece::Arg(k) => Some(k), Piece::Lit(_) => None }).collect() };
                let have = keys(zh);
                for key in keys(tr) { assert!(have.contains(&key), "{lang:?} {tr:?} uses {{{key}}}, which {zh:?} lacks"); }
            }
        }
    }

    /// Every key must still be a Chinese string somewhere in the source;
    /// a typo or an upstream rewording would otherwise go unnoticed.
    #[test]
    fn every_key_is_in_the_source() {
        fn walk(dir: &std::path::Path, out: &mut String) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() { if path.file_name().is_some_and(|n| n != "l10n") { walk(&path, out); } }
                else if path.extension().is_some_and(|e| e == "rs") { out.push_str(&std::fs::read_to_string(path).unwrap()); }
            }
        }
        let mut source = String::new();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        walk(&root.join("src"), &mut source);
        walk(&root.join("crates/core/src"), &mut source);
        for lang in Lang::ALL {
            let stale: Vec<_> = strings(lang).iter().map(|(zh, _)| *zh).filter(|zh| !source.contains(&format!("\"{}\"", zh.escape_debug()))).collect();
            assert!(stale.is_empty(), "{lang:?} keys not found in the source: {stale:#?}");
        }
        for lang in Lang::ALL {
            let mut seen = std::collections::HashSet::new();
            for (zh, _) in strings(lang) { assert!(seen.insert(zh), "{lang:?} has {zh:?} twice"); }
        }
    }
}
