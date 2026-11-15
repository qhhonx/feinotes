//! FORMAT 3 → 4: the old layout (`docs/<id>.xml`, folders only in
//! `index.json`) becomes real folders and files named by title, with links
//! rewritten as relative paths and the library's own data in `.quiet/`.
//!
//! Nothing old is changed until the end: the new files are written beside the
//! old ones (each path listed in `.quiet/migration.json` first), then
//! `.quiet/format.json` commits the move and the old layout is set aside,
//! whole, in `.quiet/before-folders-4/`. A failure, or a crash found on the
//! next launch, removes what was written and leaves the old library as it was.

use super::*;

/// Where the old layout is kept once the move is done.
pub(super) const BACKUP: &str = "before-folders-4";
/// What a move in progress has written, to undo it.
const PLAN: &str = "migration.json";
/// What the old layout kept at the top of the library.
const OLD: &[&str] = &["docs", "trash", "index.json", "index.json.bak", "format.json", "settings.json", "preferences.json", "preferences.json.tmp", "shortcuts.json", "emoji.json", "themes", "index.before-folders.json"];
/// Old files the new layout keeps, in `.quiet/`.
const KEPT: &[&str] = &["settings.json", "preferences.json", "shortcuts.json", "emoji.json", "themes"];

/// Marks a library the user took back to the old layout: not moved again without asking.
const RESTORED: &str = "restored.json";
/// When the backup was made and how many launches have seen it.
const BACKUP_INFO: &str = "before-folders-4.json";
/// A backup is kept at least this long, and at least this many launches.
const BACKUP_DAYS: u64 = 30;
const BACKUP_LAUNCHES: u32 = 3;
/// Where `restore` sets aside what the new layout held.
const AFTER: &str = "after-folders-4";

/// Whether the old layout should move to the new one, and how.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MigrationState {
    /// Already in the new layout, or a new library.
    NotNeeded,
    /// Move it at launch without asking.
    Silent,
    /// Ask first; until the user agrees it opens in the old layout.
    NeedsConsent(ConsentReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConsentReason {
    /// The library is in a synced folder: another computer may still run an older build.
    Synced(SyncService),
    /// Signs of another computer using it (its 本机文件, conflict copies, iCloud placeholders).
    OtherDevice(DeviceHint),
    /// The user went back to the old layout (`restore_backup`).
    Restored,
}

/// What gave another computer away; the interface words it in its own language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceHint {
    /// A 本机文件 whose path doesn't exist here.
    MissingFile(String),
    /// A file sync left behind: a conflict copy, a placeholder, `index 2.json`.
    SyncCopy(String),
}

/// What a move did, for 更新日志.
#[derive(Clone, Debug)]
pub struct MigrationReport {
    pub docs: usize,
    pub folders: usize,
    /// The old layout, whole (for 在访达中显示).
    pub backup: PathBuf,
}

/// What `restore_backup` did.
#[derive(Clone, Debug)]
pub struct RestoreReport {
    /// What the new layout held, set aside (not deleted).
    pub kept: PathBuf,
    /// Documents changed or made after the move: their new versions are only in `kept`.
    pub changed: usize,
}

/// Who keeps a folder in sync with other computers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncService { ICloud, Provider(String), Local }

static UBIQUITY: std::sync::OnceLock<fn(&Path) -> bool> = std::sync::OnceLock::new();

/// How the app asks the system whether a folder is in iCloud (macOS:
/// `NSURLIsUbiquitousItemKey`); the core has no system bindings of its own.
pub fn set_ubiquity_probe(probe: fn(&Path) -> bool) { let _ = UBIQUITY.set(probe); }

/// Whether `path` is synced, and by whom: iCloud Drive (or Documents with
/// 桌面与文稿 on), or a provider under `~/Library/CloudStorage`.
pub fn sync_of(path: &Path) -> SyncService {
    let home = dirs::home_dir().unwrap_or_default();
    if path.starts_with(home.join("Library/Mobile Documents")) { return SyncService::ICloud }
    if let Ok(rest) = path.strip_prefix(home.join("Library/CloudStorage"))
        && let Some(first) = rest.components().next() {
        // Folders are named like `Dropbox`, `OneDrive-Personal`, `GoogleDrive-me@…`.
        let name = first.as_os_str().to_string_lossy();
        return SyncService::Provider(name.split('-').next().unwrap_or(&name).to_string());
    }
    if UBIQUITY.get().is_some_and(|probe| probe(path)) { return SyncService::ICloud }
    SyncService::Local
}

/// Signs that another computer uses the old library at `root`.
fn other_device(root: &Path) -> Option<DeviceHint> {
    let index: HashMap<String, Meta> = reread(&root.join("index.json")).unwrap_or_default();
    if let Some(p) = index.values().filter_map(|m| m.path.as_deref()).find(|p| !Path::new(p).exists()) {
        return Some(DeviceHint::MissingFile(p.to_string()));
    }
    let conflict = |n: &str| { let l = n.to_lowercase(); l.contains("conflict") || n.contains("冲突") || l.ends_with(".icloud") };
    for dir in [root.to_path_buf(), root.join("docs")] {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            // iCloud names a clash `index 2.json`.
            let numbered = ["index", "format", "settings"].iter().any(|b| n.strip_prefix(b).and_then(|r| r.strip_suffix(".json")).is_some_and(|r| r.starts_with(' ')));
            if conflict(&n) || numbered { return Some(DeviceHint::SyncCopy(n)); }
        }
    }
    None
}

pub(super) fn state(root: &Path) -> MigrationState {
    if !Store::flat_at(root) { return MigrationState::NotNeeded; }
    if root.join(INTERNAL).join(RESTORED).exists() { return MigrationState::NeedsConsent(ConsentReason::Restored); }
    match sync_of(root) {
        SyncService::Local => {}
        s => return MigrationState::NeedsConsent(ConsentReason::Synced(s)),
    }
    match other_device(root) {
        Some(hint) => MigrationState::NeedsConsent(ConsentReason::OtherDevice(hint)),
        None => MigrationState::Silent,
    }
}

#[derive(Serialize, Deserialize)]
struct BackupInfo { created: u64, #[serde(default)] launches: u32, /// The move's end, in milliseconds: files changed after it are the user's.
    #[serde(default)] done_ms: u128 }

fn now_ms() -> u128 { std::time::SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis()) }

pub(super) fn prune(root: &Path) {
    let internal = root.join(INTERNAL);
    let info_path = internal.join(BACKUP_INFO);
    let Some(mut info) = reread::<BackupInfo>(&info_path) else { return };
    let backup = internal.join(BACKUP);
    if !backup.exists() { let _ = std::fs::remove_file(&info_path); return; }
    info.launches += 1;
    if info.launches > BACKUP_LAUNCHES && now().saturating_sub(info.created) >= BACKUP_DAYS * 86_400 {
        if std::fs::remove_dir_all(&backup).is_ok() { let _ = std::fs::remove_file(&info_path); }
        return;
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(&info) { let _ = write_atomic(&info_path, &bytes); }
}

pub(super) fn restore(root: &Path) -> Result<RestoreReport> {
    let internal = root.join(INTERNAL);
    let backup = internal.join(BACKUP);
    anyhow::ensure!(internal.join("format.json").is_file() && (backup.join("format.json").is_file() || backup.join("index.json").is_file()), "没有可恢复的迁移前备份");
    let new = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(internal.join(".lock"))?;
    new.lock()?;
    let _new = WriteLock(Some(new));
    let since = reread::<BackupInfo>(&internal.join(BACKUP_INFO)).map_or(0, |i| if i.done_ms > 0 { i.done_ms } else { i.created as u128 * 1000 });
    let kept = internal.join(format!("{AFTER}-{}", now()));
    std::fs::create_dir_all(&kept)?;
    // What the new layout holds now: changed since the move?
    let mut changed = 0;
    fn count(dir: &Path, since: u128, n: &mut usize) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if e.file_name().to_string_lossy().starts_with('.') { continue; }
            if p.is_dir() { count(&p, since, n); continue; }
            let ext = p.extension().and_then(|x| x.to_str());
            let t = e.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis());
            if matches!(ext, Some("xml" | "md")) && t > since { *n += 1; }
        }
    }
    count(root, since, &mut changed);
    // Set aside: the notes and folders, then the new layout's own files.
    for e in std::fs::read_dir(root)?.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if n.starts_with('.') { continue; }
        std::fs::rename(e.path(), kept.join(&n))?;
    }
    let inner = kept.join(INTERNAL);
    std::fs::create_dir_all(&inner)?;
    for name in ["format.json", "index.json", "index.json.bak", "trash", "settings.json"] {
        let p = internal.join(name);
        if p.symlink_metadata().is_ok() { std::fs::rename(&p, inner.join(name))?; }
    }
    // The backup comes back, with the preferences, shortcuts and themes as they are now.
    for e in std::fs::read_dir(&backup)?.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        let to = root.join(&n);
        // (Copies numbered when set aside twice stay in the backup.)
        if !(OLD.contains(&n.as_str()) || n.starts_with("index.json.corrupt-")) || to.symlink_metadata().is_ok() { continue; }
        std::fs::rename(e.path(), to)?;
    }
    for name in KEPT {
        let (from, to) = (internal.join(name), root.join(name));
        if !from.exists() { continue; }
        if to.is_dir() { let _ = std::fs::remove_dir_all(&to); } else if to.exists() { let _ = std::fs::remove_file(&to); }
        std::fs::rename(&from, &to)?;
    }
    if root.join(ASSETS).is_dir() && !root.join("assets").exists() { std::fs::rename(root.join(ASSETS), root.join("assets"))?; }
    let _ = std::fs::remove_dir(&backup);
    let _ = std::fs::remove_file(internal.join(BACKUP_INFO));
    write_atomic(&internal.join(RESTORED), &serde_json::to_vec_pretty(&BackupInfo { created: now(), launches: 0, done_ms: now_ms() })?)?;
    Ok(RestoreReport { kept, changed })
}

/// Whether `root` holds a library of the old layout (FORMAT 3 and older).
pub(super) fn legacy(root: &Path) -> bool {
    root.join("format.json").is_file() || (root.join("index.json").is_file() && root.join("docs").is_dir())
}

#[derive(Default, Serialize, Deserialize)]
struct Plan {
    /// Documents written (relative to the library).
    files: Vec<String>,
    /// Folders made, outermost first.
    dirs: Vec<String>,
    /// Entries made in `.quiet/`.
    internal: Vec<String>,
    /// `assets/` renamed `.assets/`.
    assets: bool,
}


/// Move the library at `root` to FORMAT 4, or leave it as it was.
pub(super) fn run(root: &Path, warnings: &mut Vec<String>) -> Result<MigrationReport> {
    let internal = root.join(INTERNAL);
    std::fs::create_dir_all(&internal)?;
    // Old builds take the lock at the top, this one in `.quiet/`: hold both.
    let old = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(root.join(".lock"))?;
    old.lock()?;
    let _old = WriteLock(Some(old));
    let new = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(internal.join(".lock"))?;
    new.lock()?;
    let _new = WriteLock(Some(new));
    // Another copy may have finished while this one waited.
    let report = |docs, folders| MigrationReport { docs, folders, backup: internal.join(BACKUP) };
    if internal.join("format.json").is_file() { finish(root, warnings); return Ok(report(0, 0)); }
    let format = reread::<FormatMarker>(&root.join("format.json")).map_or(0, |m| m.format);
    anyhow::ensure!(format <= 3, "这个文档库由更新版本的 Quiet.md 创建，请升级后再打开");
    if let Some(plan) = reread::<Plan>(&internal.join(PLAN)) {
        undo(root, &plan);
        let _ = std::fs::remove_file(internal.join(PLAN));
    }
    // A backup of an earlier move (the user went back since) is kept apart.
    if internal.join(BACKUP).exists() { std::fs::rename(internal.join(BACKUP), internal.join(format!("{BACKUP}-{}", now())))?; }
    let mut plan = Plan::default();
    match migrate(root, format, &mut plan, warnings) {
        Ok((docs, folders)) => {
            let _ = std::fs::remove_file(internal.join(PLAN));
            let _ = std::fs::remove_file(internal.join(RESTORED));
            let _ = write_atomic(&internal.join(BACKUP_INFO), &serde_json::to_vec_pretty(&BackupInfo { created: now(), launches: 0, done_ms: now_ms() })?);
            drop(_old);
            finish(root, warnings);
            Ok(report(docs, folders))
        }
        Err(e) => {
            undo(root, &plan);
            let _ = std::fs::remove_file(internal.join(PLAN));
            Err(anyhow::anyhow!("文档库升级到新格式失败，已保持原样：{e}"))
        }
    }
}

/// Remove what an unfinished move wrote; the old layout was never touched.
fn undo(root: &Path, plan: &Plan) {
    let abs = |rel: &str| rel.split('/').fold(root.to_path_buf(), |p, s| p.join(s));
    for f in &plan.files { let _ = std::fs::remove_file(abs(f)); }
    for d in plan.dirs.iter().rev() { let _ = std::fs::remove_dir(abs(d)); }
    let internal = root.join(INTERNAL);
    for name in &plan.internal {
        let p = internal.join(name);
        let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
    }
    if plan.assets && !root.join("assets").exists() && root.join(ASSETS).is_dir() { let _ = std::fs::rename(root.join(ASSETS), root.join("assets")); }
}

/// Set the old layout aside, whole, in `.quiet/before-folders-4/`.
pub(super) fn finish(root: &Path, warnings: &mut Vec<String>) {
    let backup = root.join(INTERNAL).join(BACKUP);
    let mut names: Vec<String> = OLD.iter().map(|s| s.to_string()).collect();
    if let Ok(rd) = std::fs::read_dir(root) {
        names.extend(rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("index.json.corrupt-")));
    }
    for name in names {
        let from = root.join(&name);
        if from.symlink_metadata().is_err() { continue; }
        let r = std::fs::create_dir_all(&backup).and_then(|_| {
            let mut to = backup.join(&name);
            let mut n = 2;
            while to.symlink_metadata().is_ok() { to = backup.join(format!("{name}.{n}")); n += 1; }
            std::fs::rename(&from, to)
        });
        if let Err(e) = r { warnings.push(format!("旧格式的 {name} 未能移入 .quiet/{BACKUP}：{e}")); }
    }
    let _ = std::fs::remove_file(root.join(".lock"));
}

/// Pages that had pages under them (FORMAT 1) become folders of the same
/// name, holding the page first and then its children in their old order.
pub(super) fn pages_to_folders(index: &mut HashMap<String, Meta>, has_file: impl Fn(&str) -> bool, title: impl Fn(&str) -> Option<String>) {
    let legacy: Vec<String> = index.values().filter_map(|m| m.parent.clone())
        .filter(|id| index.get(id).is_some_and(|m| m.folder.is_none()) && has_file(id))
        .collect::<HashSet<_>>().into_iter().collect();
    if legacy.is_empty() { return; }
    let mapping: HashMap<String, String> = legacy.iter().map(|id| (id.clone(), uuid::Uuid::new_v4().to_string())).collect();
    for id in &legacy {
        let mut meta = index[id].clone();
        meta.folder = Some(title(id).unwrap_or_default());
        meta.pinned = false;
        meta.starred = 0;
        meta.parent = meta.parent.map(|p| mapping.get(&p).cloned().unwrap_or(p));
        index.insert(mapping[id].clone(), meta);
    }
    for (id, meta) in index.iter_mut() {
        if meta.folder.is_some() { continue; }
        if let Some(folder) = mapping.get(id) { meta.parent = Some(folder.clone()); meta.order = 1; }
        else if let Some(folder) = meta.parent.as_ref().and_then(|p| mapping.get(p)) { meta.parent = Some(folder.clone()); }
    }
    for (doc, folder) in &mapping {
        let mut kids: Vec<String> = index.iter().filter(|(_, m)| m.parent.as_ref() == Some(folder)).map(|(k, _)| k.clone()).collect();
        let old = |k: &String| { let m = &index[k]; (k != doc, if m.order > 0 { m.order } else { m.created * 1000 }, k.clone()) };
        kids.sort_by_key(old);
        for (i, k) in kids.iter().enumerate() { if let Some(m) = index.get_mut(k) { m.order = i as u64 + 1; m.root_order = None; } }
    }
}

/// Names already used in folder `dir` (lowercase: macOS and Windows ignore case).
struct Names(HashMap<String, HashSet<String>>);

impl Names {
    fn take(&mut self, root: &Path, dir: &str, base: &str, ext: Option<&str>) -> String {
        let used = self.0.entry(dir.to_string()).or_insert_with(|| {
            let path = dir.split('/').filter(|s| !s.is_empty()).fold(root.to_path_buf(), |p, s| p.join(s));
            std::fs::read_dir(path).map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_lowercase()).collect()).unwrap_or_default()
        });
        for n in 1.. {
            let stem = if n == 1 { base.to_string() } else { format!("{base} {n}") };
            let name = match ext { Some(e) => format!("{stem}.{e}"), None => stem };
            if used.insert(name.to_lowercase()) { return name; }
        }
        unreachable!()
    }
}

fn migrate(root: &Path, format: u32, plan: &mut Plan, warnings: &mut Vec<String>) -> Result<(usize, usize)> {
    let internal = root.join(INTERNAL);
    let docs_dir = root.join("docs");
    let mut index: HashMap<String, Meta> = load_json(&root.join("index.json"), "文档索引", warnings);
    let trash: HashMap<String, Trashed> = load_json(&root.join("trash").join("index.json"), "回收站索引", warnings);
    let file_of = |id: &str| -> Option<PathBuf> { ["xml", "md"].iter().map(|x| docs_dir.join(format!("{id}.{x}"))).find(|p| p.is_file()) };
    if format < 2 {
        pages_to_folders(&mut index, |id| file_of(id).is_some(), |id| file_of(id).and_then(|p| std::fs::read_to_string(p).ok()).and_then(|x| docxml::parse(&x).ok()).map(|d| d.title.text));
    }
    // The documents: every file in docs/, indexed or dropped there by hand.
    let mut docs: Vec<(String, PathBuf, bool)> = vec![];
    if let Ok(rd) = std::fs::read_dir(&docs_dir) {
        for e in rd {
            let p = e?.path();
            let markdown = match p.extension().and_then(|s| s.to_str()) { Some("xml") => false, Some("md") => true, _ => continue };
            let Some(id) = p.file_stem().and_then(|s| s.to_str()).filter(|s| !s.contains('.')).map(str::to_string) else { continue };
            if docs.iter().any(|(d, _, _)| *d == id) { continue; }
            docs.push((id, p, markdown));
        }
    }
    docs.sort_by_key(|(id, p, _)| {
        let created = index.get(id).map(|m| m.created).or_else(|| std::fs::metadata(p).ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()));
        (created.unwrap_or(0), id.clone())
    });
    let assets = root.join("assets");
    let hide = !(assets.exists() && root.join(ASSETS).exists());
    if !hide { warnings.push(format!("文档库里已有 {ASSETS} 文件夹，图片仍保留在 assets 中")); }
    // Folders, outermost first, each named after its title in its parent.
    let mut names = Names(HashMap::new());
    let mut rels: HashMap<String, String> = HashMap::new();
    let is_folder = |index: &HashMap<String, Meta>, id: &str| index.get(id).is_some_and(|m| m.folder.is_some() && !m.local());
    let mut folders: Vec<String> = index.iter().filter(|(_, m)| m.folder.is_some() && !m.local()).map(|(k, _)| k.clone()).collect();
    folders.sort_by_key(|k| (index[k].created, k.clone()));
    fn folder_rel(id: &str, index: &HashMap<String, Meta>, rels: &mut HashMap<String, String>, names: &mut Names, root: &Path, seen: &mut Vec<String>, is_folder: &dyn Fn(&HashMap<String, Meta>, &str) -> bool) -> String {
        if let Some(r) = rels.get(id) { return r.clone(); }
        seen.push(id.to_string());
        let m = &index[id];
        // A missing parent, or a loop, puts it at the top.
        let dir = match m.parent.as_deref().filter(|p| is_folder(index, p) && !seen.iter().any(|s| s == p)) {
            Some(p) => folder_rel(p, index, rels, names, root, seen, is_folder),
            None => String::new(),
        };
        let name = names.take(root, &dir, &file_name_for(m.folder.as_deref().unwrap_or("")), None);
        let rel = join_rel(&dir, &name);
        rels.insert(id.to_string(), rel.clone());
        rel
    }
    for id in &folders { folder_rel(id, &index, &mut rels, &mut names, root, &mut vec![], &is_folder); }
    // Documents, in the folder they were in.
    let mut written: Vec<(String, String, Meta, String)> = vec![]; // id, rel, meta, old text
    for (id, path, markdown) in &docs {
        let text = std::fs::read_to_string(path).with_context(|| format!("无法读取 {}", path.display()))?;
        let t = std::fs::metadata(path).ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or_else(now);
        let mut meta = index.get(id).cloned().filter(|m| m.folder.is_none() && !m.local()).unwrap_or(Meta { created: t, modified: t, ..Default::default() });
        meta.markdown = *markdown;
        let title = if *markdown { meta.name.clone().unwrap_or_else(|| "未命名".into()) } else { docxml::parse(&text).map(|d| d.title.text).unwrap_or_default() };
        let dir = meta.parent.as_deref().and_then(|p| rels.get(p)).cloned().unwrap_or_default();
        let name = names.take(root, &dir, &file_name_for(&title), Some(if *markdown { "md" } else { "xml" }));
        written.push((id.clone(), join_rel(&dir, &name), meta, text));
    }
    let target: HashMap<&str, &str> = written.iter().map(|(id, rel, _, _)| (id.as_str(), rel.as_str())).collect();
    let mut contents = vec![];
    for (id, rel, meta, text) in &written {
        let out = if meta.markdown { if hide { layout::hide_assets_md(text) } else { text.clone() } } else {
            let mut out = text.clone();
            if text.contains("doc-id=") {
                let before = docxml::parse(text).with_context(|| format!("无法读取文档 {id}"))?;
                let mut doc = before.clone();
                let dir = parent_rel(rel);
                layout::each_inline(&mut doc, &mut |i| {
                    let Some(l) = i.doc_link().cloned() else { return };
                    if let Some(to) = target.get(l.id.as_str()) { i.xml = docxml::doc_link_href_xml(&layout::relative(dir, to), &l.title); }
                });
                out = docxml::serialize(&doc);
                let again = docxml::parse(&out)?;
                anyhow::ensure!(again.plain_text() == before.plain_text() && again.title.text == before.title.text, "文档 {id} 改写链接后内容不一致");
            }
            if hide { layout::hide_assets_xml(&out) } else { out }
        };
        contents.push((rel.clone(), out));
    }
    // Everything that will be written, recorded before the first write.
    let mut dirs: Vec<String> = rels.values().cloned().collect();
    dirs.sort_by_key(|d| (d.matches('/').count(), d.clone()));
    plan.dirs = dirs.clone();
    plan.files = contents.iter().map(|(r, _)| r.clone()).collect();
    plan.internal = ["index.json", "trash", "settings.json", "preferences.json", "shortcuts.json", "emoji.json", "themes"].iter()
        .filter(|n| !internal.join(n).exists()).map(|n| n.to_string()).collect();
    plan.assets = hide && assets.is_dir();
    write_atomic(&internal.join(PLAN), &serde_json::to_vec_pretty(plan)?)?;
    let abs = |rel: &str| rel.split('/').fold(root.to_path_buf(), |p, s| p.join(s));
    for d in &dirs { std::fs::create_dir(abs(d)).with_context(|| format!("无法创建文件夹 {d}"))?; }
    for (n, (rel, text)) in contents.iter().enumerate() {
        // Tests break a move half way through in a library named so.
        #[cfg(test)]
        anyhow::ensure!(n < 2 || !root.to_string_lossy().contains("fail-migration"), "测试注入的失败");
        let _ = n;
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(abs(rel)).with_context(|| format!("无法写入 {rel}"))?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
    }
    // The 回收站: same files under `.quiet/trash/`, same index.
    let trash_dir = internal.join("trash");
    std::fs::create_dir_all(&trash_dir)?;
    if let Ok(rd) = std::fs::read_dir(root.join("trash")) {
        for e in rd {
            let p = e?.path();
            let md = match p.extension().and_then(|s| s.to_str()) { Some("xml") => false, Some("md") => true, _ => continue };
            let text = std::fs::read_to_string(&p)?;
            let text = if !hide { text } else if md { layout::hide_assets_md(&text) } else { layout::hide_assets_xml(&text) };
            write_atomic(&trash_dir.join(p.file_name().unwrap()), text.as_bytes())?;
        }
    }
    write_atomic(&trash_dir.join("index.json"), &serde_json::to_vec_pretty(&trash)?)?;
    for name in KEPT {
        let (from, to) = (root.join(name), internal.join(name));
        if to.exists() || !from.exists() { continue; }
        if from.is_dir() { copy_dir(&from, &to)?; } else { std::fs::copy(&from, &to)?; }
    }
    // The new index, by path; ids stay, so open windows and links agree.
    let mut disk = DiskIndex::new();
    for id in &folders { disk.insert(format!("{}/", rels[id]), Attached::of(id, &index[id])); }
    for (id, rel, meta, _) in &written { disk.insert(rel.clone(), Attached::of(id, meta)); }
    write_atomic(&internal.join("index.json"), &serde_json::to_vec_pretty(&disk)?)?;
    // This computer's: recently opened, 本机文件, window sizes.
    let local_path = local_file_for(root);
    let mut local: Local = reread(&local_path).unwrap_or_default();
    local.opened = index.iter().filter(|(_, m)| m.opened > 0).map(|(k, m)| (k.clone(), m.opened)).collect();
    local.files = index.iter().filter(|(_, m)| m.local()).map(|(k, m)| (k.clone(), m.clone())).collect();
    if let Some(s) = reread::<Settings>(&root.join("settings.json")) { local.window_size = s.window_size; local.quick_note_size = s.quick_note_size; }
    std::fs::create_dir_all(local_path.parent().unwrap())?;
    write_atomic(&local_path, &serde_json::to_vec_pretty(&local)?)?;
    if plan.assets { std::fs::rename(&assets, root.join(ASSETS))?; }
    write_atomic(&internal.join("format.json"), &serde_json::to_vec_pretty(&FormatMarker { format: FORMAT })?)?;
    Ok((written.len(), folders.len()))
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let dest = to.join(e.file_name());
        if e.file_type()?.is_dir() { copy_dir(&e.path(), &dest)?; } else { std::fs::copy(e.path(), dest)?; }
    }
    Ok(())
}
