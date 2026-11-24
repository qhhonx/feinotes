//! FORMAT 5 (sync.md 3.3): what the files cannot say, split by device. Each
//! device writes only `.quiet/meta/<device id>.json`; every field there is
//! `[value, stamp]`, and reading takes, per (item, field), the value with the
//! latest stamp across all devices' files (ties: the larger device id). An
//! item is a document or folder by its id, with its place in `path` (folders
//! end in `/`, `null`: removed); `_root` holds the root's own settings.
//! A device keeps in its file only the fields it still wins.

use super::*;
use serde_json::Value;

/// The folder in `.quiet/` holding one file per device.
pub(super) const META: &str = "meta";
/// The item holding the root's own settings (history limit, look …).
pub(super) const ROOT_ITEM: &str = "_root";
/// The fields of an item this build reads and writes; others (a newer
/// build's) are kept as they are.
pub(super) const KNOWN: &[&str] = &["path", "created", "modified", "pinned", "starred", "order", "root_order", "width", "catalogue_closed", "reading", "info", "refs_hidden"];

type Fields = BTreeMap<String, (Value, u64)>;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub platform: String,
}

#[derive(Default, Serialize, Deserialize)]
struct DeviceFile {
    v: u32,
    device: Device,
    #[serde(default)]
    items: BTreeMap<String, Fields>,
}

/// One field's winning value.
#[derive(Clone, Debug)]
pub(super) struct Won { pub v: Value, pub ts: u64, pub dev: String }

/// Every device's file merged: per item and field, the latest value.
#[derive(Clone, Debug, Default)]
pub(super) struct Merged { pub items: BTreeMap<String, BTreeMap<String, Won>>, pub max: u64 }

impl Merged {
    fn put(&mut self, item: &str, field: &str, w: Won) {
        self.max = self.max.max(w.ts);
        let slot = self.items.entry(item.to_string()).or_default();
        match slot.get(field) {
            Some(o) if (o.ts, &o.dev) >= (w.ts, &w.dev) => {}
            _ => { slot.insert(field.to_string(), w); }
        }
    }

    pub fn get(&self, item: &str, field: &str) -> Option<&Value> { self.items.get(item)?.get(field).map(|w| &w.v).filter(|v| !v.is_null()) }

    /// The documents and folders by path as `.quiet/index.json` had them.
    /// Two ids claiming one path (renamed here while made there) become the
    /// older one, with the latest of each field from both.
    pub fn disk_index(&self) -> DiskIndex {
        let mut by_path: BTreeMap<String, Vec<&String>> = BTreeMap::new();
        for (id, fields) in &self.items {
            if id == ROOT_ITEM { continue; }
            if let Some(Value::String(p)) = fields.get("path").map(|w| &w.v) && !p.is_empty() { by_path.entry(p.clone()).or_default().push(id); }
        }
        let mut disk = DiskIndex::new();
        for (path, mut ids) in by_path {
            let first = |id: &String| self.items[id].values().map(|w| w.ts).min().unwrap_or(0);
            ids.sort_by(|a, b| (first(a), *a).cmp(&(first(b), *b)));
            let mut fields: BTreeMap<&str, &Won> = BTreeMap::new();
            for id in &ids {
                for (k, w) in &self.items[*id] {
                    if k == "path" { continue; }
                    match fields.get(k.as_str()) { Some(o) if (o.ts, &o.dev) >= (w.ts, &w.dev) => {} _ => { fields.insert(k, w); } }
                }
            }
            let mut obj = serde_json::Map::new();
            for (k, w) in fields { if KNOWN.contains(&k) && !w.v.is_null() { obj.insert(k.to_string(), w.v.clone()); } }
            obj.insert("id".into(), Value::String(ids[0].clone()));
            if let Ok(a) = serde_json::from_value::<Attached>(Value::Object(obj)) { disk.insert(path, a); }
        }
        disk
    }

    /// The root's settings under `prefix` (`look.` …) as one object.
    pub fn root_object(&self, prefix: &str) -> Value {
        let mut obj = serde_json::Map::new();
        if let Some(fields) = self.items.get(ROOT_ITEM) {
            for (k, w) in fields { if let Some(k) = k.strip_prefix(prefix) && !w.v.is_null() { obj.insert(k.to_string(), w.v.clone()); } }
        }
        Value::Object(obj)
    }
}

/// An index entry as fields; defaults are left out, like on disk.
pub(super) fn fields_of(key: &str, a: &Attached) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    if let Ok(Value::Object(obj)) = serde_json::to_value(a) {
        for (k, v) in obj { if k != "id" { out.insert(k, v); } }
    }
    out.insert("path".into(), Value::String(key.to_string()));
    out
}

/// What an index says, by id.
pub(super) fn by_id(disk: &DiskIndex) -> HashMap<String, BTreeMap<String, Value>> {
    disk.iter().map(|(k, a)| (a.id.clone(), fields_of(k, a))).collect()
}

/// The changes from `before` to `after`: (item, field, value), `null` for
/// a field back to its default and for an item's `path` once it is gone.
pub(super) fn diff(before: &HashMap<String, BTreeMap<String, Value>>, after: &HashMap<String, BTreeMap<String, Value>>) -> Vec<(String, String, Value)> {
    let mut out = vec![];
    for (id, want) in after {
        let was = before.get(id);
        for k in KNOWN {
            let old = was.and_then(|w| w.get(*k)).filter(|v| !v.is_null());
            let new = want.get(*k).filter(|v| !v.is_null());
            if old != new { out.push((id.clone(), k.to_string(), new.cloned().unwrap_or(Value::Null))); }
        }
    }
    for id in before.keys() { if !after.contains_key(id) { out.push((id.clone(), "path".into(), Value::Null)); } }
    out.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    out
}

pub(super) fn dir(root: &Path) -> PathBuf { root.join(INTERNAL).join(META) }

/// Every device's file under `root` merged. A file that does not parse is
/// passed over (a sync's torn copy); this device's own falls back to its
/// `.bak`, as the index did.
pub(super) fn load(root: &Path, me: &str, warnings: &mut Vec<String>) -> Merged {
    let mut merged = Merged::default();
    let Ok(rd) = crate::vfs::read_dir(dir(root)) else { return merged };
    let mut paths: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    paths.sort();
    for p in paths {
        let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let f: DeviceFile = if stem == me {
            // Recovered from its backup: put back, or the next read finds nothing.
            let n = warnings.len();
            let f = load_json(&p, "文档索引", warnings);
            if warnings.len() > n && !p.fs_exists() { let _ = save_json(&p, &f); }
            f
        } else {
            match crate::vfs::read(&p).ok().and_then(|b| serde_json::from_slice(&b).ok()) { Some(f) => f, None => continue }
        };
        // A sync's copy (`<id> 2.json`) is still that device's.
        let dev = if f.device.id.is_empty() { stem } else { f.device.id.clone() };
        for (item, fields) in f.items {
            for (k, (v, ts)) in fields { merged.put(&item, &k, Won { v, ts, dev: dev.clone() }); }
        }
    }
    merged
}

/// A stamp later than every one seen (a hybrid logical clock): this
/// computer's time in milliseconds, or one past the latest seen when the
/// clock is behind.
pub(super) fn stamp(clock: &std::cell::Cell<u64>, seen: u64) -> u64 {
    let t = crate::time::now_ms().max(clock.get().max(seen) + 1);
    clock.set(t);
    t
}

/// Write `changes` as device `me`'s, over what every device's file says now;
/// returns the merged result. Only the fields `me` still wins are kept.
pub(super) fn commit(root: &Path, me: &Device, clock: &std::cell::Cell<u64>, changes: Vec<(String, String, Value)>, warnings: &mut Vec<String>) -> Result<Merged> {
    let mut merged = load(root, &me.id, warnings);
    if !changes.is_empty() {
        let ts = stamp(clock, merged.max);
        for (item, field, v) in changes { merged.put(&item, &field, Won { v, ts, dev: me.id.clone() }); }
    }
    let mut items: BTreeMap<String, Fields> = BTreeMap::new();
    for (item, fields) in &merged.items {
        for (k, w) in fields { if w.dev == me.id { items.entry(item.clone()).or_default().insert(k.clone(), (w.v.clone(), w.ts)); } }
    }
    crate::vfs::create_dir_all(dir(root))?;
    save_json(&dir(root).join(format!("{}.json", me.id)), &DeviceFile { v: 1, device: me.clone(), items })?;
    Ok(merged)
}

/// This computer's id, made once and kept with its other local data; the
/// same for every root (sync.md 3.6).
pub fn device() -> Device {
    static DEVICE: std::sync::OnceLock<Device> = std::sync::OnceLock::new();
    DEVICE.get_or_init(|| {
        let path = local_base(Path::new("")).join("device.json");
        let mut d: Device = reread(&path).unwrap_or_default();
        if d.id.is_empty() {
            d.id = uuid::Uuid::new_v4().to_string();
            if let Some(p) = path.parent() { let _ = crate::vfs::create_dir_all(p); }
            let _ = write_atomic(&path, &serde_json::to_vec_pretty(&d).unwrap_or_default());
        }
        d.platform = std::env::consts::OS.to_string();
        if d.name.is_empty() { d.name = whoami_host(); }
        d
    }).clone()
}

/// A name for this computer, for 移除已停用的设备 later.
fn whoami_host() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_default()
}

/// The id of a file found on disk with no entry yet: the same on every
/// device that finds it (the hash of its NFC path), so two devices finding
/// one file agree on it.
pub(super) fn path_id(key: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let nfc: String = key.nfc().collect();
    let digest = sha1_smol::Sha1::from(nfc.as_bytes()).digest().bytes();
    let mut b = [0u8; 16];
    b.copy_from_slice(&digest[..16]);
    uuid::Builder::from_sha1_bytes(b).into_uuid().to_string()
}

/// FORMAT 4 → 5: `index.json` and the root's settings become this device's
/// file, every field stamped now; `index.json` is kept as `index.json.v4`.
/// The marker is written last, so a crash midway redoes it.
pub(super) fn upgrade(root: &Path, me: &Device, warnings: &mut Vec<String>) -> Result<()> {
    let internal = root.join(INTERNAL);
    let disk: DiskIndex = load_json(&internal.join("index.json"), "文档索引", warnings);
    let mut changes = vec![];
    let mut ids = HashSet::new();
    for (key, a) in &disk {
        let id = if a.id.is_empty() || ids.contains(&a.id) { path_id(key) } else { a.id.clone() };
        ids.insert(id.clone());
        for (k, v) in fields_of(key, a) { changes.push((id.clone(), k, v)); }
    }
    let settings: Value = reread(&internal.join("settings.json")).unwrap_or(Value::Null);
    if let Some(n) = settings.get("history_limit").filter(|v| !v.is_null()) { changes.push((ROOT_ITEM.into(), "history_limit".into(), n.clone())); }
    if let Some(Value::Object(look)) = settings.get("look") {
        for (k, v) in look { changes.push((ROOT_ITEM.into(), format!("look.{k}"), v.clone())); }
    }
    if let Some(Value::Object(tops)) = settings.get("scroll_top") {
        for (id, v) in tops { if ids.contains(id) { changes.push((id.clone(), "scroll".into(), v.clone())); } }
    }
    // Over a file a crash left behind: the index is still the truth.
    let _ = crate::vfs::remove_file(dir(root).join(format!("{}.json", me.id)));
    commit(root, me, &std::cell::Cell::new(0), changes, warnings)?;
    let marker = internal.join("format.json");
    let id = reread::<FormatMarker>(&marker).map(|m| m.id).unwrap_or_default();
    write_atomic(&marker, &serde_json::to_vec_pretty(&FormatMarker { format: FORMAT, id: if id.is_empty() { uuid::Uuid::new_v4().to_string() } else { id } })?)?;
    if internal.join("index.json").fs_exists() { let _ = crate::vfs::rename(internal.join("index.json"), internal.join("index.json.v4")); }
    Ok(())
}
