//! FORMAT 4 layout: the folders on disk are the library's folders, a
//! document's file name is its title, and links are paths relative to the
//! file. Names, relative paths and the directory scan live here.

use super::*;

/// Characters no file name may hold on macOS or Windows, and what stands in for them.
const ILLEGAL: [(char, char); 9] = [('/', '／'), ('\\', '＼'), (':', '：'), ('*', '＊'), ('?', '？'), ('"', '＂'), ('<', '＜'), ('>', '＞'), ('|', '｜')];
/// Longest name kept, in bytes, before the ` 2` suffix and the extension.
const NAME_BYTES: usize = 120;

/// A title as a file or folder name (no extension): illegal characters become
/// their full-width forms, control characters spaces; no leading `.`, no
/// trailing dot or space (Windows drops them), Windows device names get a
/// `_`, and at most 120 bytes. Empty: 未命名文档.
pub fn file_name_for(title: &str) -> String {
    let mut s: String = title.chars().map(|c| match ILLEGAL.iter().find(|(a, _)| *a == c) {
        Some((_, b)) => *b,
        None if c.is_control() => ' ',
        None => c,
    }).collect();
    s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let s = s.trim_start_matches(['.', ' ']).trim_end_matches(['.', ' ']);
    let mut s = if s.is_empty() { crate::model::UNTITLED.to_string() } else { s.to_string() };
    if s.len() > NAME_BYTES {
        let mut end = NAME_BYTES;
        while !s.is_char_boundary(end) { end -= 1; }
        s.truncate(end);
        s = s.trim_end_matches(['.', ' ']).to_string();
    }
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && stem.as_bytes()[3].is_ascii_digit() && stem.as_bytes()[3] != b'0');
    if reserved { s.insert(stem.len(), '_'); }
    s
}

/// Whether `stem` is `base`, or `base` numbered (`base 2`, `base 3` …) because the name was taken.
pub(super) fn name_fits(stem: &str, base: &str) -> bool {
    stem == base || stem.strip_prefix(base).and_then(|r| r.strip_prefix(' ')).is_some_and(|n| !n.starts_with('0') && n.parse::<u32>().is_ok_and(|n| n >= 2))
}

/// `stem` without the ` 2` a clash gave it.
pub(super) fn unnumbered(stem: &str) -> &str {
    match stem.rsplit_once(' ') {
        Some((head, n)) if !head.is_empty() && name_fits(stem, head) && !n.is_empty() => head,
        _ => stem,
    }
}

/// The folder part of a relative path ("" at the top).
pub(super) fn parent_rel(rel: &str) -> &str { rel.rsplit_once('/').map_or("", |(d, _)| d) }
/// The last part of a relative path.
pub(super) fn base_name(rel: &str) -> &str { rel.rsplit_once('/').map_or(rel, |(_, n)| n) }
/// A file name without its extension.
pub(super) fn stem_of(name: &str) -> &str { name.rsplit_once('.').filter(|(s, _)| !s.is_empty()).map_or(name, |(s, _)| s) }
pub(super) fn join_rel(dir: &str, name: &str) -> String { if dir.is_empty() { name.to_string() } else { format!("{dir}/{name}") } }

/// `to` (relative to the library) as written in a file in folder `from`.
pub fn relative(from: &str, to: &str) -> String {
    let a: Vec<&str> = from.split('/').filter(|s| !s.is_empty()).collect();
    let b: Vec<&str> = to.split('/').filter(|s| !s.is_empty()).collect();
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let mut parts: Vec<&str> = std::iter::repeat_n("..", a.len() - common).collect();
    parts.extend(&b[common..]);
    parts.join("/")
}

/// A link written in a file in folder `from`, relative to the library;
/// `None` when it is not a relative path or leaves the library.
pub fn resolve(from: &str, href: &str) -> Option<String> {
    if href.is_empty() || href.starts_with('/') || href.starts_with('\\') || href.contains("://") || href.starts_with('#') { return None; }
    let href = href.split(['#', '?']).next().unwrap_or("");
    let mut parts: Vec<&str> = from.split('/').filter(|s| !s.is_empty()).collect();
    for p in href.split(['/', '\\']) {
        match p {
            "" | "." => {}
            ".." => { parts.pop()?; }
            p => parts.push(p),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// What a scan found: the folders and documents, relative to the library.
#[derive(Default)]
pub(super) struct Scan {
    pub dirs: Vec<String>,
    pub docs: Vec<String>,
    /// iCloud placeholders (`.<name>.icloud`), by the path the file will have.
    pub pending: Vec<String>,
    /// A folder could not be read: what is missing may just be unreadable.
    pub partial: bool,
}

/// Walk the library: `.md` and `.xml` files and the folders holding them;
/// names starting with `.` (`.quiet`, `.assets`, `.git` …) are not the user's notes.
pub(super) fn scan(root: &Path) -> Scan {
    fn walk(root: &Path, rel: &str, out: &mut Scan) {
        let Ok(rd) = std::fs::read_dir(if rel.is_empty() { root.to_path_buf() } else { root.join(rel) }) else { out.partial = true; return };
        for e in rd {
            let Ok(e) = e else { out.partial = true; continue };
            let Some(name) = e.file_name().to_str().map(str::to_string) else { continue };
            if let Some(real) = name.strip_prefix('.').and_then(|n| n.strip_suffix(".icloud")) {
                if real.ends_with(".md") || real.ends_with(".xml") { out.pending.push(join_rel(rel, real)); }
                continue;
            }
            if name.starts_with('.') { continue; }
            let Ok(kind) = e.file_type() else { continue };
            // Symbolic links are not followed: a link to a parent would never end.
            let dir = kind.is_dir();
            let child = join_rel(rel, &name);
            if dir { out.dirs.push(child.clone()); walk(root, &child, out); }
            else if kind.is_file() && (name.ends_with(".md") || name.ends_with(".xml")) && !stem_of(&name).is_empty() { out.docs.push(child); }
        }
    }
    let mut out = Scan::default();
    walk(root, "", &mut out);
    out
}

/// Ask iCloud to download a placeholder, once per run.
pub(super) fn download(path: &Path) {
    static ASKED: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());
    let mut asked = ASKED.lock().unwrap();
    if asked.iter().any(|p| p == path) { return; }
    asked.push(path.to_path_buf());
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("/usr/bin/brctl").arg("download").arg(path).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
}

/// The iCloud placeholder that stands for `path` while it is not downloaded.
pub(super) fn placeholder(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    Some(path.with_file_name(format!(".{name}.icloud")))
}

/// Visit every inline object of a document, titles of tables' cells included.
pub(super) fn each_inline(doc: &mut Document, f: &mut dyn FnMut(&mut crate::model::Inline)) {
    fn walk(blocks: &mut [crate::model::Block], f: &mut dyn FnMut(&mut crate::model::Inline)) {
        for b in blocks {
            for r in &mut b.text.runs { if let Some(i) = r.marks.inline.as_mut() { f(i); } }
            walk(&mut b.children, f);
            if let crate::model::BlockKind::Table(t) = &mut b.kind {
                for row in &mut t.rows { for cell in row { walk(&mut cell.blocks, f); } }
            }
        }
    }
    walk(&mut doc.blocks, f);
}

/// The library's images and attachments moved from `assets/` to `.assets/`:
/// `="assets/…"` inside a tag of a DocxXML file.
pub(super) fn hide_assets_xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if in_tag && rest.starts_with("=\"assets/") {
            out.push_str("=\".assets/");
            rest = &rest["=\"assets/".len()..];
            continue;
        }
        match c { '<' => in_tag = true, '>' => in_tag = false, _ => {} }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// The same for a Markdown file: `](assets/…` and `src="assets/…"`.
pub(super) fn hide_assets_md(text: &str) -> String {
    text.replace("](assets/", "](.assets/").replace("src=\"assets/", "src=\".assets/").replace("](<assets/", "](<.assets/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_become_file_names() {
        assert_eq!(file_name_for("周报"), "周报");
        assert_eq!(file_name_for("a/b\\c:d*e?f\"g<h>i|j"), "a／b＼c：d＊e？f＂g＜h＞i｜j");
        assert_eq!(file_name_for("  ..隐藏. "), "隐藏");
        assert_eq!(file_name_for(""), crate::model::UNTITLED);
        assert_eq!(file_name_for("..."), crate::model::UNTITLED);
        assert_eq!(file_name_for("一\n二\t三"), "一 二 三");
        assert_eq!(file_name_for("con"), "con_");
        assert_eq!(file_name_for("COM3.txt"), "COM3_.txt");
        assert_eq!(file_name_for("COM0"), "COM0");
        assert_eq!(file_name_for("console"), "console");
        let long = file_name_for(&"字".repeat(100));
        assert!(long.len() <= 120 && long.chars().all(|c| c == '字'));
    }

    #[test]
    fn numbered_names() {
        assert!(name_fits("周报", "周报") && name_fits("周报 2", "周报") && name_fits("周报 12", "周报"));
        assert!(!name_fits("周报 1", "周报") && !name_fits("周报 02", "周报") && !name_fits("周报2", "周报") && !name_fits("周", "周报"));
        assert_eq!(unnumbered("周报 3"), "周报");
        assert_eq!(unnumbered("周报"), "周报");
        assert_eq!(unnumbered("Q 1"), "Q 1");
    }

    #[test]
    fn relative_paths() {
        assert_eq!(relative("", "a.xml"), "a.xml");
        assert_eq!(relative("工作", "工作/周报.xml"), "周报.xml");
        assert_eq!(relative("工作/子", "随手记.md"), "../../随手记.md");
        assert_eq!(relative("工作/子", "工作/其他/b.xml"), "../其他/b.xml");
        for (from, to) in [("", "a/b.xml"), ("a/b", "a/c/d.xml"), ("x", "y.md")] {
            assert_eq!(resolve(from, &relative(from, to)).as_deref(), Some(to));
        }
        assert_eq!(resolve("", "../a.xml"), None);
        assert_eq!(resolve("a", "https://x/y"), None);
        assert_eq!(resolve("a", "./b/../c.xml#x"), Some("a/c.xml".into()));
    }

    #[test]
    fn assets_hidden_only_in_tags() {
        let x = "<title cover=\"assets/c.png\">T</title>\n<p>=\"assets/x\"</p>\n<img src=\"assets/i.png\"/>";
        assert_eq!(hide_assets_xml(x), "<title cover=\".assets/c.png\">T</title>\n<p>=\"assets/x\"</p>\n<img src=\".assets/i.png\"/>");
    }
}
