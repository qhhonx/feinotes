//! Bundled first-launch library; existing libraries are never reseeded.
use fdl_core::vfs::PathExt as _;
use std::path::Path;
use fdl_core::vfs as fs;

include!(concat!(env!("OUT_DIR"), "/guides.rs"));
/// The Simplified Chinese guides (`build.py --lang zh`): documents and index
/// only, the images are the English library's.
mod zh { include!(concat!(env!("OUT_DIR"), "/guides_zh.rs")); }

/// The guides for the interface language: Simplified Chinese gets the
/// Chinese ones, every other language the English ones.
fn files(chinese: bool) -> Vec<(&'static str, &'static [u8])> {
    if !chinese { return FILES.to_vec(); }
    zh::FILES.iter().chain(FILES.iter().filter(|(n, _)| n.starts_with(".assets/"))).copied().collect()
}

/// Any entry but the lock file means an existing library, even if all
/// documents were deleted (`.quiet/` keeps its index); a pending directory is
/// a first launch to finish. Other hidden files (`.DS_Store`) are not a library.
pub(crate) fn seeded(root: &Path) -> std::io::Result<bool> {
    if root.join(".guides-pending").fs_exists() { return Ok(false); }
    let quiet = root.join(crate::store::INTERNAL);
    let internal = quiet.fs_is_dir() && fs::read_dir(&quiet)?.any(|e| e.map_or(true, |e| e.file_name() != ".lock"));
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
    ("In the Finder, use Open With › Quiet.md.", "In File Explorer, use Open with › Quiet.md."),
    ("show the file in the Finder,", "show the file in File Explorer,"),
    ("what you see in the Finder is what the sidebar shows.", "what you see in File Explorer is what the sidebar shows."),
    ("you put into the folder in the Finder show up", "you put into the folder in File Explorer show up"),
    ("a project banner from your Mac.", "a project banner from your computer."),
    ("<h2>Move to another Mac</h2><ol><li>Quit Quiet.md on the old Mac with", "<h2>Move to another computer</h2><ol><li>Quit Quiet.md on the old computer with"),
    ("Copy the complete Quiet.md folder into Documents on the new Mac.", "Copy the complete Quiet.md folder into Documents on the new computer."),
    ("Press a new combination containing Command, Control or Option.", "Press a new combination containing Ctrl or Alt."),
    ("Hold <code>⌥</code> to do the same for every folder inside.", "Hold <code>Alt</code> to do the same for every folder inside."),
    // ⌃⌘D is Ctrl+Shift+D there (Ctrl+Alt reads as AltGr, #290); this entry serves the Chinese guides too
    ("<code>⌃ ⌘ D</code>", "<code>Ctrl+Shift+D</code>"),
    // The system-wide quick-note shortcut exists on macOS only.
    ("<p>A global shortcut can open a small note window while you are using another app. Write the thought down, then close the window; it is saved with your documents.</p><ul><li>Enable the global quick-note shortcut in Settings › Shortcuts.</li><li>The default is <code>⌥ Space</code>. If another app already uses it, choose another combination, such as <code>⌥ ⇧ Space</code>. Registration failures are shown in settings.</li>",
     "<p>A note window is a small window of its own for writing a thought down. Close it when you are done; the note is saved with your documents.</p><ul>"),
];

/// `WINDOWS_TEXT` for the Chinese guides.
const WINDOWS_TEXT_ZH: &[(&str, &str)] = &[
    ("每篇文档都是 Mac 上的一个文件", "每篇文档都是电脑上的一个文件"),
    ("用时间机器备份整个文件夹，或者把它拷到另一台 Mac 上即可。", "用你的备份工具备份整个文件夹，或者把它拷到另一台电脑上即可。"),
    ("在 macOS 上，按 <code>⌘ ⌃ ⇧ 4</code> 可以把框选的区域截图到剪贴板。", "按 <code>Win+Shift+S</code> 可以把框选的区域截图到剪贴板。"),
    ("从访达把图片拖进文档。", "从文件资源管理器把图片拖进文档。"),
    ("在访达中，也可以用“打开方式 › Quiet.md”打开。", "在文件资源管理器中，也可以用“打开方式 › Quiet.md”打开。"),
    ("在访达中显示该文件，", "在文件资源管理器中显示该文件，"),
    ("你在访达里看到的，就是侧栏里显示的。", "你在文件资源管理器里看到的，就是侧栏里显示的。"),
    ("在访达中放进这个文件夹", "在文件资源管理器中放进这个文件夹"),
    ("可以使用 Mac 里的照片、插画或项目横幅。", "可以使用电脑里的照片、插画或项目横幅。"),
    ("<h2>迁移到另一台 Mac</h2><ol><li>在旧 Mac 上按", "<h2>迁移到另一台电脑</h2><ol><li>在旧电脑上按"),
    ("把完整的 Quiet.md 文件夹拷贝到新 Mac 的“文稿”文件夹中。", "把完整的 Quiet.md 文件夹复制到新电脑的“文档”文件夹中。"),
    ("只要“文稿”文件夹在时间机器的备份范围内，它也会一并受到保护。", "只要“文档”文件夹在你的备份范围内，它也会一并受到保护。"),
    ("按下新的组合键，需包含 Command、Control 或 Option。", "按下新的组合键，需包含 Ctrl 或 Alt。"),
    ("按住 <code>⌥</code> 点击，可以一并展开或收起里面的所有文件夹。", "按住 <code>Alt</code> 点击，可以一并展开或收起里面的所有文件夹。"),
    // The system-wide quick-note shortcut exists on macOS only.
    ("<p>使用全局快捷键，即使正在用别的应用，也能随时打开一个小笔记窗口。记下想法后关闭窗口即可，笔记会和你的文档保存在一起。</p><ul><li>在 设置 › 快捷键 中开启“全局快速新建笔记窗口”。</li><li>默认快捷键是 <code>⌥ Space</code>。如果已被其他应用占用，可以换一个组合，比如 <code>⌥ ⇧ Space</code>。注册失败时，设置中会显示提示。</li>", "<p>笔记窗口是一个独立的小窗口，用来随手记下想法。写完关闭即可，笔记会和你的文档保存在一起。</p><ul>"),
];

/// A guide document as the Windows build seeds it: Ctrl / Alt keys, no Mac-only steps.
pub fn for_windows(xml: &str) -> String {
    let text = WINDOWS_TEXT.iter().chain(WINDOWS_TEXT_ZH).fold(xml.to_string(), |s, (mac, win)| s.replace(mac, win));
    crate::shortcuts::windows_keys(&text)
}

pub fn initialize(root: &Path) -> anyhow::Result<()> {
    seed(root, crate::settings::prefs().lang() == crate::settings::Lang::Zh)
}

fn seed(root: &Path, chinese: bool) -> anyhow::Result<()> {
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
    for (name, bytes) in files(chinese) {
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
                if !dest.fs_exists() { fs::rename(e.path(), dest)?; }
            }
            continue;
        }
        let dest = root.join(&name);
        if !dest.fs_exists() { fs::rename(pending.join(&name), dest)?; }
    }
    fs::remove_dir_all(pending)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use fdl_core::vfs::PathExt as _;
    /// Guides go only into an empty folder: an existing library, even one whose
    /// documents were all deleted, is never refilled or overwritten.
    /// The guide documents under `root`, folders included.
    fn docs(root: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = vec![];
        for e in fdl_core::vfs::read_dir(root).unwrap() {
            let p = e.unwrap().path();
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            if name.starts_with('.') { continue; }
            if p.fs_is_dir() { out.extend(docs(&p)); } else if super::is_doc(&name) { out.push(p); }
        }
        out
    }

    #[test]
    fn existing_library_is_never_reseeded() {
        let root = fdl_core::vfs::temp_dir().join(format!("qmd-guides-{}", uuid::Uuid::new_v4()));
        super::initialize(&root).unwrap();
        let all = docs(&root);
        let first = all[0].clone();
        fdl_core::vfs::write(&first, "<title>mine</title>").unwrap();
        super::initialize(&root).unwrap();
        assert_eq!(fdl_core::vfs::read_to_string(&first).unwrap(), "<title>mine</title>");
        // Every note and folder deleted: `.quiet/` still says it is a library.
        for e in fdl_core::vfs::read_dir(&root).unwrap() {
            let p = e.unwrap().path();
            if p.file_name().unwrap().to_string_lossy().starts_with('.') { continue; }
            if p.fs_is_dir() { fdl_core::vfs::remove_dir_all(p).unwrap(); } else { fdl_core::vfs::remove_file(p).unwrap(); }
        }
        super::initialize(&root).unwrap();
        assert_eq!(docs(&root).len(), 0);
        fdl_core::vfs::remove_dir_all(&root).unwrap();
    }

    /// The seeded library opens as it is (FORMAT 4, no migration): every
    /// guide listed in its folder, links resolved to the documents, names
    /// following the store's rules.
    #[test]
    fn seeded_library_opens_with_its_folders_and_links() {
        for chinese in [false, true] { seeded_library_opens(chinese); }
    }

    fn seeded_library_opens(chinese: bool) {
        let root = fdl_core::vfs::temp_dir().join(format!("qmd-guides-open-{}", uuid::Uuid::new_v4()));
        super::seed(&root, chinese).unwrap();
        let mut store = crate::store::Store::open_at(root.clone()).unwrap();
        let list = store.list();
        assert!(store.warnings.is_empty(), "{:?}", store.warnings);
        let docs: Vec<_> = list.iter().filter(|e| e.meta.folder.is_none()).collect();
        assert_eq!(docs.len(), super::files(chinese).iter().filter(|(n, _)| super::is_doc(n)).count());
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
        assert!(!root.join("docs").fs_exists() && !root.join("index.json").fs_exists());
        fdl_core::vfs::remove_dir_all(&root).unwrap();
    }

    /// Every Mac-only passage still exists, and the Windows guides keep no Mac
    /// key, in English and in Chinese.
    #[test]
    fn windows_guides_use_windows_keys() {
        for (chinese, table) in [(false, super::WINDOWS_TEXT), (true, super::WINDOWS_TEXT_ZH)] {
            let files = super::files(chinese);
            let docs: Vec<&str> = files.iter().filter(|(n, _)| super::is_doc(n)).map(|(_, b)| std::str::from_utf8(b).unwrap()).collect();
            for (mac, _) in table { assert!(docs.iter().any(|d| d.contains(mac)), "stale WINDOWS_TEXT entry: {mac}"); }
            let all: String = docs.iter().map(|d| super::for_windows(d)).collect();
            assert!(all.contains("<code>Ctrl+B</code>") && all.contains("<code>Ctrl+Alt+1</code>") && all.contains("<code>Ctrl+Shift+C</code>"));
            let mac = |l: &&str| l.contains(['⌘', '⌥', '⇧', '⌃']);
            assert!(!all.lines().any(|l| mac(&l)), "{}", all.lines().find(mac).unwrap_or_default());
            for finder in ["Finder", "访达"] { assert!(!all.contains(finder), "{}", all.lines().find(|l| l.contains(finder)).unwrap_or_default()); }
        }
    }

    /// The Chinese guides are the English ones in other words: the same
    /// documents and folders (ids, places, order), and only images the
    /// English library brings along.
    #[test]
    fn chinese_guides_match_the_english_ones() {
        let index = |files: &[(&str, &[u8])]| -> Vec<serde_json::Value> {
            let (_, json) = files.iter().find(|(n, _)| *n == ".quiet/index.json").unwrap();
            let map: serde_json::Map<String, serde_json::Value> = serde_json::from_slice(json).unwrap();
            let mut v: Vec<_> = map.into_values().map(|mut m| { m.as_object_mut().unwrap().retain(|k, _| !matches!(k.as_str(), "created" | "modified")); m }).collect();
            v.sort_by_key(|m| m["id"].as_str().unwrap().to_string());
            v
        };
        assert_eq!(index(super::zh::FILES), index(super::FILES));
        let names = |files: &[(&str, &[u8])]| files.iter().filter(|(n, _)| super::is_doc(n)).count();
        assert_eq!(names(super::zh::FILES), names(super::FILES));
        assert!(super::zh::FILES.iter().all(|(n, _)| !n.starts_with(".assets/")));
        let assets: std::collections::HashSet<&str> = super::FILES.iter().map(|(n, _)| *n).filter(|n| n.starts_with(".assets/")).collect();
        for (n, b) in super::zh::FILES.iter().filter(|(n, _)| super::is_doc(n)) {
            let xml = std::str::from_utf8(b).unwrap();
            for part in xml.split("=\".assets/").skip(1) {
                let path = format!(".assets/{}", &part[..part.find('"').unwrap()]);
                assert!(assets.contains(path.as_str()), "{n}: {path} is not in the English library");
            }
        }
    }

    /// Copies launched together on a new library: one seeds it, none fails.
    #[test]
    fn simultaneous_first_launches() {
        let root = fdl_core::vfs::temp_dir().join(format!("fdl-guides-{}", uuid::Uuid::new_v4()));
        let runs: Vec<_> = (0..4).map(|_| { let root = root.clone(); std::thread::spawn(move || super::initialize(&root)) }).collect();
        for r in runs { r.join().unwrap().unwrap(); }
        let count = super::FILES.iter().filter(|(n, _)| super::is_doc(n)).count();
        assert!(count > 0);
        assert_eq!(docs(&root).len(), count);
        assert!(!root.join(".guides-pending").fs_exists());
        // A library with content is never reseeded.
        for p in docs(&root) { fdl_core::vfs::remove_file(p).unwrap(); }
        super::initialize(&root).unwrap();
        assert_eq!(docs(&root).len(), 0);
        let _ = fdl_core::vfs::remove_dir_all(&root);
    }
}
