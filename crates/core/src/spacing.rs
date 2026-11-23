//! 中英文之间自动加空格 (#293): where a half-width space goes between a
//! Chinese character and a Latin letter or digit, as 盘古之白 spaces them
//! ("用 iPhone 拍了 3 张"). Only 汉字: kana and hangul are left as they are,
//! and so is everything beside punctuation, symbols and spaces. URLs and
//! e-mail addresses written out in the text are not split.

use crate::model::{Marks, RichText};
use std::ops::Range;

/// A Chinese character (CJK Unified Ideographs, their extensions and the
/// compatibility ideographs); not kana, hangul or CJK punctuation.
pub fn is_han(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2A6DF | 0x2A700..=0x2EBEF | 0x2F800..=0x2FA1F | 0x30000..=0x323AF)
}

/// Whether a space belongs between `a` and the `b` right after it.
pub fn wants_space(a: char, b: char) -> bool {
    is_han(a) && b.is_ascii_alphanumeric() || a.is_ascii_alphanumeric() && is_han(b)
}

/// URLs and e-mail addresses in `s`: what is inside one stays as it is.
fn addresses(s: &str) -> Vec<Range<usize>> {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"(?i)(?:(?-u:\b)[a-z][a-z0-9+.-]*://|(?-u:\b)www\.|(?-u:\b)mailto:)\S+|[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").unwrap());
    RE.find_iter(s).map(|m| m.range()).collect()
}

/// Byte offsets in `s` where a space goes (before the character there).
/// `joined(a, b)`: the characters at `a` and at `b` (right after it) belong
/// together and get no space between them (both in one piece of inline
/// code, one link …).
pub fn gaps(s: &str, joined: impl Fn(usize, usize) -> bool) -> Vec<usize> {
    let urls = addresses(s);
    let in_url = |a: usize, b: usize| urls.iter().any(|r| r.start <= a && b < r.end);
    let mut out = vec![];
    let mut prev: Option<(usize, char)> = None;
    for (i, c) in s.char_indices() {
        if let Some((j, p)) = prev && wants_space(p, c) && !in_url(j, i) && !joined(j, i) { out.push(i); }
        prev = Some((i, c));
    }
    out
}

/// `s` with its spaces added.
pub fn spaced(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    let mut at = 0;
    for g in gaps(s, |_, _| false) {
        out.push_str(&s[at..g]);
        out.push(' ');
        at = g;
    }
    out.push_str(&s[at..]);
    out
}

/// `gaps` in rich text: nothing inside inline code or a link, and nothing
/// beside an inline object (a formula, a mention …).
pub fn text_gaps(t: &RichText) -> Vec<usize> {
    let runs: Vec<(Range<usize>, &Marks)> = t.spans().collect();
    let at = |i: usize| runs.partition_point(|(r, _)| r.end <= i).min(runs.len().saturating_sub(1));
    gaps(&t.text, |a, b| {
        let (Some((_, x)), Some((_, y))) = (runs.get(at(a)), runs.get(at(b))) else { return false };
        x.inline.is_some() || y.inline.is_some() || x.code && y.code || x.link.is_some() && x.link == y.link
    })
}

/// Put a space at each of `gaps` (ascending) in `t`. A space takes the
/// styles around it, but never inline code, a link or an object.
pub fn insert_spaces(t: &mut RichText, gaps: &[usize]) {
    for &g in gaps.iter().rev() {
        let m = t.marks_at(g);
        t.insert_plain(g, " ", Marks { code: false, link: None, inline: None, ..m });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaces_between_han_and_latin() {
        for (a, b) in [
            ("用iPhone拍了3张照片", "用 iPhone 拍了 3 张照片"),
            ("Quiet.md是一个编辑器", "Quiet.md 是一个编辑器"),
            ("已经 有空格 a 中", "已经 有空格 a 中"),
            // Punctuation and symbols stay as they are.
            ("你好，world！", "你好，world！"),
            ("（A）中", "（A）中"),
            ("100%的人", "100%的人"),
            ("C++语言", "C++语言"),
            // Kana and hangul are not 汉字; a Han character among kana is.
            ("ひらがなabc", "ひらがなabc"),
            ("한국어abc", "한국어abc"),
            ("漢字abc", "漢字 abc"),
            // Extension B.
            ("𠀀a", "𠀀 a"),
            // URLs and e-mail addresses are not split; around them is spaced.
            ("见https://例子.com/路径abc", "见 https://例子.com/路径abc"),
            ("看www.a.com/文章", "看 www.a.com/文章"),
            ("写信给me@a.com", "写信给 me@a.com"),
            ("", ""),
        ] {
            assert_eq!(spaced(a), b, "{a}");
        }
        // Spacing twice changes nothing more.
        assert_eq!(spaced(&spaced("中a文b")), "中 a 文 b");
    }

    #[test]
    fn rich_text_keeps_code_links_and_objects() {
        let mut t = RichText::plain("在");
        t.push("code中文", Marks { code: true, ..Default::default() });
        t.push("里", Marks::default());
        t.push("链接a", Marks { link: Some("https://a.b".into()), ..Default::default() });
        t.push("b", Marks { bold: true, ..Default::default() });
        t.push("\u{fffc}", Marks { inline: Some(crate::docxml::to_inline(crate::model::InlineKind::Equation("x".into()))), ..Default::default() });
        t.push("中", Marks::default());
        // 在|code: boundary, spaced; code|中文 inside code: no; 文|里: no
        // (Han, Han); 里|链: no; 链接|a inside the link: no; a|b: no.
        let g = text_gaps(&t);
        assert_eq!(g, vec!["在".len()]);
        insert_spaces(&mut t, &g);
        assert_eq!(t.text, "在 code中文里链接ab\u{fffc}中");
        // The space is plain, not code.
        assert!(!t.spans().next().unwrap().1.code);
        assert_eq!(t.spans().next().unwrap().0, 0.."在 ".len());
    }
}
