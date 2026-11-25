//! The paths a document keeps (a picture's or attachment's source, a cover,
//! a board's images): written relative to a folder (the library's, or the
//! document's own) and with `/` on every system, so the same file reads the
//! same wherever the folder is and whichever system opens it; read back with
//! either separator, and absolute ones as they were written. docs/FORMAT.md.

use crate::vfs::PathExt;
use std::path::{Component, Path, PathBuf};

/// `path` (absolute) as a document writes it from the folder `from`: `/` between
/// parts and `../` up to where the two part. None when no relative path reaches
/// it: another drive, or either not absolute.
pub fn portable(path: &Path, from: &Path) -> Option<String> {
    if !path.fs_is_absolute() || !from.fs_is_absolute() { return None }
    let (a, b) = (lexical(path), lexical(from));
    let (a, b): (Vec<Component>, Vec<Component>) = (a.components().collect(), b.components().collect());
    let same = a.iter().zip(&b).take_while(|(x, y)| same_part(x, y)).count();
    // Different drives (`C:` and `D:`), or one on a share: nothing relative reaches.
    if same == 0 { return None }
    let up = b.len() - same;
    let down = a[same..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned());
    let parts: Vec<String> = std::iter::repeat_n("..".to_string(), up).chain(down).collect();
    Some(if parts.is_empty() { ".".into() } else { parts.join("/") })
}

/// `portable` for a path inside the folder `dir` only (never `../`): what a
/// library writes for its own files. None for anything outside it.
pub fn portable_in(path: &Path, dir: &Path) -> Option<String> {
    portable(path, dir).filter(|p| p != "." && !p.starts_with("../") && p != "..")
}

/// What a document wrote, as a path on this system: a relative one from the
/// folder `from`, either separator taken; an absolute one (this system's or
/// another's) as it was written.
pub fn resolve(written: &str, from: &Path) -> PathBuf {
    if is_absolute_anywhere(written) { return PathBuf::from(written) }
    lexical(&parts(written).fold(from.to_path_buf(), |p, s| p.join(s)))
}

/// Whether `written` is an absolute path on some system: `/…`, `\\server…`,
/// `C:\…` or `C:/…`.
pub fn is_absolute_anywhere(written: &str) -> bool {
    let b = written.as_bytes();
    written.starts_with('/') || written.starts_with('\\') || (b.len() > 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'/' | b'\\'))
}

/// `written`'s parts, split at either separator, empty ones and `.` left out.
pub fn parts(written: &str) -> impl Iterator<Item = &str> {
    written.split(['/', '\\']).filter(|s| !s.is_empty() && *s != ".")
}

/// `path` with its `.` and `..` folded away, without asking the disk.
pub fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => { if !matches!(out.components().next_back(), Some(Component::Normal(_))) || !out.pop() { out.push(".."); } }
            c => out.push(c),
        }
    }
    out
}

/// A browser-selected relative path, in the `webkitRelativePath` spelling.
/// Keep the original parts so callers can preserve the folder hierarchy.
pub fn selection_parts(path: &str) -> Option<Vec<&str>> {
    if path.len() > 4096 { return None; }
    let parts: Vec<_> = path.split('/').collect();
    (parts.len() <= 34 && parts.iter().all(|part| selection_component(part))).then_some(parts)
}

/// One name in a browser file selection.
pub fn selection_component(name: &str) -> bool {
    !name.is_empty() && !matches!(name, "." | "..") && !name.contains([':', '\\', '/'])
        && !name.chars().any(char::is_control)
}

/// Paths a relative attachment reference can name inside a browser-selected
/// folder. DocxXML can also name a file from the original library's root.
pub fn selection_attachment_paths(dir: &[&str], src: &str, from_library: bool) -> Vec<String> {
    fn in_selection(dir: &[&str], src: &str) -> Option<String> {
        let mut path = dir.to_vec();
        for part in src.split(['/', '\\']) {
            match part {
                "." => {}
                ".." if path.len() > 1 => { path.pop(); }
                _ if selection_component(part) => path.push(part),
                _ => return None,
            }
        }
        let parts = path.get(1..)?;
        if parts.is_empty() || parts.iter().enumerate().any(|(i, p)| {
            matches!(*p, "node_modules" | "target") || (p.starts_with('.') && !(i == 0 && *p == ".assets"))
        }) { return None; }
        Some(path.join("/"))
    }

    if dir.is_empty() { return vec![]; }
    let decoded = crate::markdown::percent_decode(src);
    if is_absolute_anywhere(&decoded) { return vec![]; }
    let mut paths: Vec<_> = in_selection(dir, &decoded).into_iter().collect();
    if from_library && dir.len() > 1 && let Some(rooted) = in_selection(&dir[..1], &decoded)
        && paths.first() != Some(&rooted) { paths.push(rooted); }
    paths
}

/// Two parts of a path name the same thing: a drive letter in either case.
fn same_part(a: &Component, b: &Component) -> bool {
    match (a, b) {
        (Component::Prefix(x), Component::Prefix(y)) => x.as_os_str().eq_ignore_ascii_case(y.as_os_str()),
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder on this system, absolute here.
    fn here(rest: &str) -> PathBuf {
        let base = if cfg!(windows) { PathBuf::from("C:\\lib") } else { PathBuf::from("/lib") };
        parts(rest).fold(base, |p, s| p.join(s))
    }


    #[test]
    fn browser_attachment_stays_in_its_selected_tree() {
        let dir = ["Vault", "Nested"];
        assert_eq!(selection_attachment_paths(&dir, "../Images/cover%20page.png", false), ["Vault/Images/cover page.png"]);
        assert_eq!(selection_attachment_paths(&dir, ".assets\\a.png", true), ["Vault/.assets/a.png"]);
        assert_eq!(selection_attachment_paths(&dir, "dir/a.png", true), ["Vault/Nested/dir/a.png", "Vault/dir/a.png"]);
        for src in ["../../private.txt", "%2Fprivate.txt", "C:\\secret.png", "https://site/path", ".quiet/index.json", "../.env", "target/chunk.png"] {
            assert!(selection_attachment_paths(&dir, src, true).is_empty(), "{src}");
        }
    }

    #[test]
    fn browser_selection_keeps_its_root_and_relative_names() {
        assert_eq!(selection_parts("资料/nested/notes.md").as_deref(), Some(&["资料", "nested", "notes.md"][..]));
        for path in ["", "/notes.md", "资料/../notes.md", "资料//notes.md", "资料/x\\y.md", "C:/notes.md"] {
            assert_eq!(selection_parts(path), None, "{path}");
        }
    }

    #[test]
    fn written_paths_are_relative_with_slashes() {
        let lib = here("");
        assert_eq!(portable(&here(".assets/a b.png"), &lib).as_deref(), Some(".assets/a b.png"));
        assert_eq!(portable(&here(".assets/files/u/说明.pdf"), &lib).as_deref(), Some(".assets/files/u/说明.pdf"));
        // From a document's own folder: up to where they part.
        assert_eq!(portable(&here(".assets/x.png"), &here("工作/周报")).as_deref(), Some("../../.assets/x.png"));
        assert_eq!(portable(&here("工作/./pics/../图.png"), &here("工作")).as_deref(), Some("图.png"));
        assert_eq!(portable(&lib, &lib).as_deref(), Some("."));
        // Only inside the library for its own files.
        assert_eq!(portable_in(&here(".assets/x.png"), &lib).as_deref(), Some(".assets/x.png"));
        assert_eq!(portable_in(&here("../elsewhere/x.png"), &lib), None);
        assert_eq!(portable_in(&lib, &lib), None);
        assert_eq!(portable(Path::new("relative/x.png"), &lib), None);
        if cfg!(windows) {
            assert_eq!(portable(Path::new("D:\\other\\x.png"), &lib), None);
            assert_eq!(portable(Path::new("c:\\lib\\.assets\\x.png"), &lib).as_deref(), Some(".assets/x.png"));
        }
    }

    #[test]
    fn written_paths_read_back_with_either_separator() {
        let lib = here("");
        for w in [".assets/a b.png", ".assets\\a b.png", "./.assets/a b.png"] { assert_eq!(resolve(w, &lib), here(".assets/a b.png"), "{w}"); }
        assert_eq!(resolve("../.assets/x.png", &here("工作")), here(".assets/x.png"));
        // Absolute ones, this system's or another's, stay as written.
        for w in ["/Users/me/Quiet.md/.assets/x.png", "C:\\Users\\me\\Quiet.md\\.assets\\x.png", "C:/x.png", "\\\\server\\share\\x.png"] {
            assert!(is_absolute_anywhere(w), "{w}");
            assert_eq!(resolve(w, &lib), PathBuf::from(w));
        }
        for w in [".assets/x.png", "x.png", "C:x.png", "http://a/b.png"] { assert!(!is_absolute_anywhere(w) || w.starts_with("http"), "{w}"); }
        assert_eq!(parts("C:\\Users\\me/.assets//x.png").collect::<Vec<_>>(), ["C:", "Users", "me", ".assets", "x.png"]);
        // Written and read back, the same file.
        for p in [here(".assets/x.png"), here("a/b/c.pdf")] { assert_eq!(resolve(&portable(&p, &here("a")).unwrap(), &here("a")), p); }
    }
}
