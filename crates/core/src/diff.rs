//! Line-level diff between two document versions. DocxXML is turned into
//! plain text first, so the view compares what the reader sees rather than
//! markup; Markdown files are compared as written. No diff crate: the LCS
//! table is bounded, which keeps a million-character note from allocating
//! hundreds of megabytes during a preview.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind { Same, Added, Removed }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub kind: Kind,
    pub text: String,
}

/// Beyond this many lines the preview is truncated; the table is O(n·m).
const MAX_LINES: usize = 1200;

/// What a reader sees: block text in document order, one line per paragraph.
pub fn readable(text: &str, markdown: bool) -> String {
    if markdown { return text.to_string() }
    crate::docxml::parse(text).map_or_else(|_| text.to_string(), |d| d.plain_text())
}

fn split(text: &str, markdown: bool) -> Vec<String> {
    let body = readable(text, markdown);
    let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
    if lines.len() > MAX_LINES {
        lines.truncate(MAX_LINES);
        lines.push(format!("… 仅显示前 {MAX_LINES} 行 / only the first {MAX_LINES} lines"));
    }
    lines
}

pub fn diff(old: &str, new: &str, markdown: bool) -> Vec<Line> {
    let a = split(old, markdown);
    let b = split(new, markdown);
    let (n, m) = (a.len(), b.len());
    let w = m + 1;
    let mut dp = vec![0u32; (n + 1) * w];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i * w + j] = if a[i] == b[j] {
                dp[(i + 1) * w + j + 1] + 1
            } else {
                dp[(i + 1) * w + j].max(dp[i * w + j + 1])
            };
        }
    }
    let mut out = Vec::with_capacity(n + m);
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            out.push(Line { kind: Kind::Same, text: a[i].clone() });
            i += 1; j += 1;
        } else if dp[(i + 1) * w + j] >= dp[i * w + j + 1] {
            out.push(Line { kind: Kind::Removed, text: a[i].clone() });
            i += 1;
        } else {
            out.push(Line { kind: Kind::Added, text: b[j].clone() });
            j += 1;
        }
    }
    while i < n { out.push(Line { kind: Kind::Removed, text: a[i].clone() }); i += 1 }
    while j < m { out.push(Line { kind: Kind::Added, text: b[j].clone() }); j += 1 }
    out
}

pub fn counts(lines: &[Line]) -> (usize, usize) {
    (
        lines.iter().filter(|l| l.kind == Kind::Added).count(),
        lines.iter().filter(|l| l.kind == Kind::Removed).count(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn added_removed_and_unchanged_lines() {
        let d = diff("one\ntwo\nthree", "one\nTWO\nthree\nfour", true);
        assert_eq!(counts(&d), (2, 1));
        assert_eq!(d[0], Line { kind: Kind::Same, text: "one".into() });
        assert_eq!(d[1], Line { kind: Kind::Removed, text: "two".into() });
        assert_eq!(d[2], Line { kind: Kind::Added, text: "TWO".into() });
        assert_eq!(d[3], Line { kind: Kind::Same, text: "three".into() });
        assert_eq!(d[4], Line { kind: Kind::Added, text: "four".into() });
    }

    #[test]
    fn identical_documents_have_no_changes() {
        let d = diff("<p>a</p>", "<p>a</p>", false);
        assert_eq!(counts(&d), (0, 0));
        assert!(d.iter().all(|l| l.kind == Kind::Same));
    }

    #[test]
    fn xml_markup_is_not_compared() {
        let a = "<title>x</title><p>first</p>";
        let b = "<title>x</title><p>second</p>";
        let d = diff(a, b, false);
        assert_eq!(counts(&d), (1, 1));
        assert!(d.iter().any(|l| l.text == "first" && l.kind == Kind::Removed));
        assert!(d.iter().any(|l| l.text == "second" && l.kind == Kind::Added));
    }

    #[test]
    fn long_documents_are_truncated() {
        let many = (0..MAX_LINES + 100).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let d = diff(&many, "", true);
        assert!(d.len() <= MAX_LINES + 1);
    }
}
