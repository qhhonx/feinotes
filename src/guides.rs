//! Bundled first-launch library; existing libraries are never reseeded.
use std::path::Path;
use std::fs;

include!(concat!(env!("OUT_DIR"), "/guides.rs"));

/// Any entry but the lock file means an existing library, even if all
/// documents were deleted (`.quiet/` keeps its index); a pending directory is
/// a first launch to finish. Other hidden files (`.DS_Store`) are not a library.
pub(crate) fn seeded(root: &Path) -> std::io::Result<bool> {
    if root.join(".guides-pending").exists() { return Ok(false); }
    let quiet = root.join(crate::store::INTERNAL);
    let internal = quiet.is_dir() && fs::read_dir(&quiet)?.any(|e| e.map_or(true, |e| e.file_name() != ".lock"));
    Ok(internal || fs::read_dir(root)?.any(|e| e.map_or(true, |e| { let n = e.file_name().to_string_lossy().into_owned(); !n.starts_with('.') || n == crate::store::ASSETS })))
}

/// A guide document (not an asset or the index).
fn is_doc(name: &str) -> bool { !name.starts_with('.') && name.ends_with(".xml") }

/// Mac-only passages of the guides and what Windows reads instead; key
/// combinations go through `shortcuts::windows_keys` afterwards.
const WINDOWS_TEXT: &[(&str, &str)] = &[
    ("every document is a file on your Mac", "every document is a file on your computer"),
    ("Back up the whole folder with Time Machine or copy it to another Mac.", "Back up the whole folder with your backup tool or copy it to another computer."),
    ("On macOS, <code>⌘ ⌃ ⇧ 4</code> captures a selection to the clipboard.", "<code>Win+Shift+S</code> captures a selection to the clipboard."),
    ("Drag an image from Finder into the document.", "Drag an image from File Explorer into the document."),
    ("a project banner from your Mac.", "a project banner from your computer."),
    ("<h2>Move to another Mac</h2><ol><li>Quit Quiet.md on the old Mac with", "<h2>Move to another computer</h2><ol><li>Quit Quiet.md on the old computer with"),
    ("Copy the complete Quiet.md folder into Documents on the new Mac.", "Copy the complete Quiet.md folder into Documents on the new computer."),
    ("Press a new combination containing Command, Control or Option.", "Press a new combination containing Ctrl or Alt."),
    ("Hold <code>⌥</code> to do the same for every folder inside.", "Hold <code>Alt</code> to do the same for every folder inside."),
    // The system-wide quick-note shortcut exists on macOS only.
    ("<p>A global shortcut can open a small note window while you are using another app. Write the thought down, then close the window; it is saved with your documents.</p><ul><li>Enable the global quick-note shortcut in Settings › Shortcuts.</li><li>The default is <code>⌥ Space</code>. If another app already uses it, choose another combination, such as <code>⌥ ⇧ Space</code>. Registration failures are shown in settings.</li>",
     "<p>A note window is a small window of its own for writing a thought down. Close it when you are done; the note is saved with your documents.</p><ul>"),
];

/// A guide document as the Windows build seeds it: Ctrl / Alt keys, no Mac-only steps.
pub fn for_windows(xml: &str) -> String {
    let text = WINDOWS_TEXT.iter().fold(xml.to_string(), |s, (mac, win)| s.replace(mac, win));
    crate::shortcuts::windows_keys(&text)
}

pub fn initialize(root: &Path) -> anyhow::Result<()> {
    if FILES.is_empty() { return Ok(()); }
    fs::create_dir_all(root)?;
    if seeded(root)? { return Ok(()); }
    // Copies of the app may share a library: two started together on a new
    // one seed it in turn. Released when `lock` drops.
    let quiet = root.join(crate::store::INTERNAL);
    fs::create_dir_all(&quiet)?;
    let lock = fs::OpenOptions::new().create(true).truncate(false).write(true).open(quiet.join(".lock"))?;
    lock.lock()?;
    if seeded(root)? { return Ok(()); }
    let pending = root.join(".guides-pending");
    fs::create_dir_all(&pending)?;
    // Stage complete files before publishing. The pending directory lets a
    // failed first launch resume without mistaking it for an existing library.
    for (name, bytes) in FILES {
        let path = pending.join(name);
        fs::create_dir_all(path.parent().unwrap())?;
        let windows = cfg!(target_os = "windows") && is_doc(name);
        match std::str::from_utf8(bytes) {
            Ok(xml) if windows => fs::write(path, for_windows(xml))?,
            _ => fs::write(path, bytes)?,
        }
    }
    // Published top level first, `.quiet/` (the index and format) last: an
    // interrupted launch is still pending, never a library missing its notes.
    let mut names: Vec<_> = fs::read_dir(&pending)?.collect::<Result<Vec<_>, _>>()?.into_iter().map(|e| e.file_name()).collect();
    names.sort_by_key(|n| *n == *crate::store::INTERNAL);
    for name in names {
        if name == *crate::store::INTERNAL {
            for e in fs::read_dir(pending.join(&name))? {
                let e = e?;
                let dest = quiet.join(e.file_name());
                if !dest.exists() { fs::rename(e.path(), dest)?; }
            }
            continue;
        }
        let dest = root.join(&name);
        if !dest.exists() { fs::rename(pending.join(&name), dest)?; }
    }
    fs::remove_dir_all(pending)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    /// Guides go only into an empty folder: an existing library, even one whose
    /// documents were all deleted, is never refilled or overwritten.
    /// The guide documents under `root`, folders included.
    fn docs(root: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = vec![];
        for e in std::fs::read_dir(root).unwrap() {
            let p = e.unwrap().path();
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            if name.starts_with('.') { continue; }
            if p.is_dir() { out.extend(docs(&p)); } else if super::is_doc(&name) { out.push(p); }
        }
        out
    }

    #[test]
    fn existing_library_is_never_reseeded() {
        let root = std::env::temp_dir().join(format!("qmd-guides-{}", uuid::Uuid::new_v4()));
        super::initialize(&root).unwrap();
        let all = docs(&root);
        let first = all[0].clone();
        std::fs::write(&first, "<title>mine</title>").unwrap();
        super::initialize(&root).unwrap();
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "<title>mine</title>");
        // Every note and folder deleted: `.quiet/` still says it is a library.
        for e in std::fs::read_dir(&root).unwrap() {
            let p = e.unwrap().path();
            if p.file_name().unwrap().to_string_lossy().starts_with('.') { continue; }
            if p.is_dir() { std::fs::remove_dir_all(p).unwrap(); } else { std::fs::remove_file(p).unwrap(); }
        }
        super::initialize(&root).unwrap();
        assert_eq!(docs(&root).len(), 0);
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// The seeded library opens as it is (FORMAT 4, no migration): every
    /// guide listed in its folder, links resolved to the documents, names
    /// following the store's rules.
    #[test]
    fn seeded_library_opens_with_its_folders_and_links() {
        let root = std::env::temp_dir().join(format!("qmd-guides-open-{}", uuid::Uuid::new_v4()));
        super::initialize(&root).unwrap();
        let mut store = crate::store::Store::open_at(root.clone()).unwrap();
        let list = store.list();
        assert!(store.warnings.is_empty(), "{:?}", store.warnings);
        let docs: Vec<_> = list.iter().filter(|e| e.meta.folder.is_none()).collect();
        assert_eq!(docs.len(), super::FILES.iter().filter(|(n, _)| super::is_doc(n)).count());
        assert!(list.iter().filter(|e| e.meta.folder.is_some()).count() >= 2);
        assert!(docs.iter().any(|e| e.meta.parent.is_some()));
        let ids: std::collections::HashSet<&str> = list.iter().map(|e| e.id.as_str()).collect();
        let links: Vec<_> = docs.iter().flat_map(|e| e.links.iter()).collect();
        assert!(!links.is_empty());
        for l in links { assert!(ids.contains(l.target.as_str()), "unresolved link {}", l.target); }
        for e in &docs {
            let name = store.file_path(&e.id).unwrap().file_stem().unwrap().to_string_lossy().into_owned();
            assert_eq!(name, crate::store::file_name_for(&e.title), "build.py names {} differently", e.title);
        }
        assert!(!root.join("docs").exists() && !root.join("index.json").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Every Mac-only passage still exists, and the Windows guides keep no Mac key.
    #[test]
    fn windows_guides_use_windows_keys() {
        let docs: Vec<&str> = super::FILES.iter().filter(|(n, _)| super::is_doc(n)).map(|(_, b)| std::str::from_utf8(b).unwrap()).collect();
        for (mac, _) in super::WINDOWS_TEXT { assert!(docs.iter().any(|d| d.contains(mac)), "stale WINDOWS_TEXT entry: {mac}"); }
        let all: String = docs.iter().map(|d| super::for_windows(d)).collect();
        assert!(all.contains("<code>Ctrl+B</code>") && all.contains("<code>Ctrl+Alt+1</code>") && all.contains("<code>Ctrl+Shift+C</code>"));
        let mac = |l: &&str| l.contains(['⌘', '⌥', '⇧', '⌃']);
        assert!(!all.lines().any(|l| mac(&l)), "{}", all.lines().find(mac).unwrap_or_default());
    }

    /// Copies launched together on a new library: one seeds it, none fails.
    #[test]
    fn simultaneous_first_launches() {
        let root = std::env::temp_dir().join(format!("fdl-guides-{}", uuid::Uuid::new_v4()));
        let runs: Vec<_> = (0..4).map(|_| { let root = root.clone(); std::thread::spawn(move || super::initialize(&root)) }).collect();
        for r in runs { r.join().unwrap().unwrap(); }
        let count = super::FILES.iter().filter(|(n, _)| super::is_doc(n)).count();
        assert!(count > 0);
        assert_eq!(docs(&root).len(), count);
        assert!(!root.join(".guides-pending").exists());
        // A library with content is never reseeded.
        for p in docs(&root) { std::fs::remove_file(p).unwrap(); }
        super::initialize(&root).unwrap();
        assert_eq!(docs(&root).len(), 0);
        let _ = std::fs::remove_dir_all(&root);
    }
}
