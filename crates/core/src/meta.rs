//! FORMAT 5 (sync.md 3.3): what the files cannot say, split by device. Each
//! device writes only `.quiet/meta/<device id>.json`; every field there is
//! `[value, stamp]`, and reading takes, per (item, field), the value with the
//! latest stamp across all devices' files (ties: the larger device id). An
//! item is a document or folder by its id, with its place in `path` (folders
//! end in `/`, `null`: removed); `_root` holds the top-level folder's own settings.
//! A device keeps in its file only the fields it still wins.

use super::*;
use serde_json::Value;

/// How long a 彻底删除 tombstone (`purged`) stays in its device's file.
pub(super) const PURGED_KEPT_MS: u64 = 30 * 86_400_000;

/// The folder in `.quiet/` holding one file per device.
pub(super) const META: &str = "meta";
/// The item holding the top-level folder's own settings (history limit, look …).
pub(super) const FOLDER_ITEM: &str = "_root";
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
/// `files`: the folder as it was when read, so a later read can tell
/// nothing changed (`refresh`). `read`: which read this is, a number no
/// other read had, so the store can tell it has taken in what this one found.
#[derive(Clone, Debug, Default)]
pub(super) struct Merged { pub items: BTreeMap<String, BTreeMap<String, Won>>, pub max: u64, files: Option<Files>, pub read: u64 }

static READS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Each device file by name, as last seen.
pub(super) type Files = Vec<(String, Seen)>;

/// What a file's metadata tells of it: size, modification time, `written`,
/// the time it last changed (Unix ctime; none elsewhere) and whether it is
/// still only in iCloud (a placeholder keeps the size and time it will have).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Stat { pub len: u64, pub modified: Option<web_time::SystemTime>, pub written: u64, pub changed: Option<web_time::SystemTime>, pub dataless: bool }

/// A file's `Stat` when it was read, and a hash of its contents then if it
/// had changed within a `TICK` of that (`racy`).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Seen { pub stat: Stat, pub contents: Option<u64> }

/// The coarsest step a file system's change time takes: Linux stamps a write
/// from a clock that ticks every few milliseconds, HFS+ keeps whole seconds,
/// FAT two. Two writes within one step can leave the same time behind.
const TICK: std::time::Duration = std::time::Duration::from_secs(2);

pub(super) fn stat(p: &Path, m: Option<&crate::vfs::Metadata>) -> Stat {
    let dataless = crate::vfs::is_dataless(p);
    #[cfg(unix)]
    let changed = m.map(|m| { use std::os::unix::fs::MetadataExt; let (s, n) = (m.ctime(), m.ctime_nsec()); let d = std::time::Duration::new(s.unsigned_abs(), n as u32);
        if s < 0 { web_time::SystemTime::UNIX_EPOCH - d } else { web_time::SystemTime::UNIX_EPOCH + d } });
    #[cfg(not(unix))]
    let changed = None;
    Stat { len: m.map_or(0, |m| m.len()), modified: m.and_then(|m| m.modified().ok()), written: written(p, m, dataless), changed, dataless }
}

/// Whether a file seen at `at` may yet be written again in the same tick of
/// its change time (or that time is ahead of this computer's clock): then its
/// `Stat` can stay as it is through a write, and only its contents tell.
pub(super) fn racy(stat: &Stat, at: web_time::SystemTime) -> bool {
    !stat.dataless && stat.changed.is_some_and(|c| c + TICK > at)
}

/// `stat` seen at `at`, its contents hashed when `racy`.
pub(super) fn seen(p: &Path, stat: Stat, at: web_time::SystemTime) -> Seen {
    let contents = racy(&stat, at).then(|| contents(p));
    Seen { stat, contents }
}

fn contents(p: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = rustc_hash::FxHasher::default();
    crate::vfs::read(p).ok().hash(&mut h);
    h.finish()
}

/// Whether a file still holds what it held when `last` was seen, now that
/// its metadata says `now`: any change there says written; the same says so
/// too unless `last` was seen within a tick of the file's change, and then
/// the contents decide (`read` hashes them; called only then).
pub(super) fn unchanged(last: &Seen, now: &Stat, read: impl FnOnce() -> u64) -> bool {
    last.stat == *now && last.contents.is_none_or(|h| read() == h)
}

/// `path` seen now.
pub(super) fn seen_now(p: &Path) -> Option<Seen> {
    let at = web_time::SystemTime::now();
    let m = crate::vfs::metadata(p).ok()?;
    Some(seen(p, stat(p, Some(&m)), at))
}

/// Whether `p` is still as `last` saw it (`unchanged`); none: it wasn't there.
pub(super) fn still(p: &Path, last: &Option<Seen>) -> bool {
    match (last, crate::vfs::metadata(p).ok()) {
        (Some(last), Some(m)) => unchanged(last, &stat(p, Some(&m)), || contents(p)),
        (l, m) => l.is_none() && m.is_none(),
    }
}

/// The device files seen now.
pub(super) fn files(root: &Path) -> Files {
    let at = web_time::SystemTime::now();
    let Ok(rd) = crate::vfs::read_dir(dir(root)) else { return vec![] };
    let mut out: Files = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).map(|p| {
        let m = crate::vfs::metadata(&p).ok();
        (p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), seen(&p, stat(&p, m.as_ref()), at))
    }).collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Whether the device files seen as `now` still hold what `was` saw
/// (`unchanged`), leaving out `skip`.
fn same_files(root: &Path, was: &Files, now: &Files, skip: Option<&str>) -> bool {
    let keep = |f: &'_ Files| f.iter().filter(|(n, _)| Some(n.as_str()) != skip).cloned().collect::<Vec<_>>();
    let (was, now) = (keep(was), keep(now));
    was.len() == now.len() && was.iter().zip(&now).all(|((a, last), (b, seen))| a == b && unchanged(last, &seen.stat, || seen.contents.unwrap_or_else(|| contents(&dir(root).join(b)))))
}

/// The device files as `merged` saw them, seen again as at the moment each
/// changed: well within its tick.
#[cfg(test)]
pub(super) fn seen_as_changed(root: &Path, merged: &mut Merged) {
    for (n, last) in merged.files.iter_mut().flatten() { if let Some(c) = last.stat.changed { *last = seen(&dir(root).join(n), last.stat.clone(), c); } }
}

/// What a write within the same tick leaves on Unix: the `Stat` as `merged`
/// saw it (elsewhere `written` hashes the contents, so nothing is held).
#[cfg(test)]
pub(super) fn change_times_held(root: &Path, merged: &mut Merged) {
    if !cfg!(unix) { return }
    let now = files(root);
    for ((_, last), (_, seen)) in merged.files.iter_mut().flatten().zip(now) { last.stat = seen.stat; }
}

/// What changes whenever a file is written, though its size and time may not:
/// a sync (iCloud, a git checkout, rsync) can put back the time a file had on
/// the device that wrote it, and two writes can land on the same size. On
/// Unix the inode and change time (ctime), which a write or a replacing
/// rename moves (but within a tick of the clock it stamps from: `racy`) and
/// nothing can set back; elsewhere a hash of the
/// contents (a placeholder's are not read: they would download).
pub(super) fn written(p: &Path, m: Option<&crate::vfs::Metadata>, dataless: bool) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = rustc_hash::FxHasher::default();
    #[cfg(unix)]
    { let _ = (p, dataless); use std::os::unix::fs::MetadataExt; if let Some(m) = m { (m.ino(), m.ctime(), m.ctime_nsec()).hash(&mut h); } }
    #[cfg(all(not(unix), not(target_family = "wasm")))]
    { let _ = m; if !dataless && let Ok(b) = crate::vfs::read(p) { b.hash(&mut h); } }
    // The web build's files change only through this process.
    #[cfg(target_family = "wasm")]
    { let _ = (p, m, dataless); }
    h.finish()
}

/// `merged` read again unless no device file was added, removed or changed
/// (`unchanged`) since it was read (sync.md 3.3). Any change reads
/// everything again. Whether it was read again.
pub(super) fn refresh(root: &Path, me: &str, merged: &mut Merged, warnings: &mut Vec<String>) -> bool {
    let now = files(root);
    // Kept as seen now: one seen within a tick of its change but no longer is
    // trusted on its `Stat` from here on (a later write moves the change time past it).
    if merged.files.as_ref().is_some_and(|was| same_files(root, was, &now, None)) { merged.files = Some(now); return false }
    // Taken before reading: a file changing meanwhile makes the next look read again.
    *merged = load(root, me, warnings);
    merged.files = Some(now);
    merged.read = READS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    true
}

/// `refresh` on a copy of `last`.
pub(super) fn cached(root: &Path, me: &str, last: &Merged, warnings: &mut Vec<String>) -> Merged {
    let mut merged = last.clone();
    refresh(root, me, &mut merged, warnings);
    merged
}

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
            if id == FOLDER_ITEM { continue; }
            if let Some(Value::String(p)) = fields.get("path").map(|w| &w.v) && !p.is_empty() { by_path.entry(p.clone()).or_default().push(id); }
        }
        let mut objs = vec![];
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
            objs.push((path, obj));
        }
        to_ranks(&mut objs);
        let mut disk = DiskIndex::new();
        for (path, obj) in objs {
            if let Ok(a) = serde_json::from_value::<Attached>(Value::Object(obj)) { disk.insert(path, a); }
        }
        disk
    }

    /// `keys_of` these fields as read, without copying them first.
    pub fn keys(&self) -> HashMap<String, String> {
        self.items.iter().filter_map(|(id, f)| {
            let path = f.get("path")?.v.as_str()?;
            Some((id.clone(), f.get(place_field(path).0)?.v.as_str()?.to_string()))
        }).collect()
    }

    /// FORMAT 6's 回收站: what is trashed (and not purged), by id, as written.
    pub fn trash(&self) -> BTreeMap<String, Value> {
        self.items.iter().filter(|(_, f)| !f.get("purged").is_some_and(|w| !w.v.is_null()))
            .filter_map(|(id, f)| Some((id.clone(), f.get("trashed").map(|w| &w.v).filter(|v| !v.is_null())?.clone()))).collect()
    }

    /// The top-level folder's settings under `prefix` (`look.` …) as one object.
    pub fn root_object(&self, prefix: &str) -> Value {
        let mut obj = serde_json::Map::new();
        if let Some(fields) = self.items.get(FOLDER_ITEM) {
            for (k, w) in fields { if let Some(k) = k.strip_prefix(prefix) && !w.v.is_null() { obj.insert(k.to_string(), w.v.clone()); } }
        }
        Value::Object(obj)
    }
}

/// Which field holds a page's place: `top_order` at the top of the top-level folder,
/// `order` inside a folder (a path ending in `/` is a folder).
fn place_field(path: &str) -> (&'static str, &'static str) {
    if path.trim_end_matches('/').contains('/') { ("order", "root_order") } else { ("root_order", "order") }
}

/// The folder `path` is in ("" for the top), for telling siblings apart.
pub(super) fn parent_of(path: &str) -> &str { path.trim_end_matches('/').rsplit_once('/').map_or("", |p| p.0) }

/// FORMAT 6 keeps a page's place among its siblings as a fractional index
/// ("a0V", sync.md 3.3), so a move writes the moved page only; the store
/// works with ranks (1, 2 …) as before. Keys become ranks per folder, equal
/// keys in id order so every device sees one order. Numbers (FORMAT 5 and
/// before) stay as they are.
fn to_ranks(objs: &mut [(String, serde_json::Map<String, Value>)]) {
    let mut groups: BTreeMap<String, Vec<(String, String, usize)>> = BTreeMap::new();
    for (n, (path, obj)) in objs.iter().enumerate() {
        let (field, _) = place_field(path);
        if let Some(Value::String(key)) = obj.get(field) {
            let id = obj.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
            groups.entry(parent_of(path).to_string()).or_default().push((key.clone(), id, n));
        }
    }
    for mut g in groups.into_values() {
        g.sort();
        for (rank, (_, _, n)) in g.into_iter().enumerate() {
            let (field, other) = place_field(&objs[n].0);
            let obj = &mut objs[n].1;
            obj.insert(field.into(), Value::from(rank as u64 + 1));
            // The other field is the store's as it left it (a number); never a key.
            if obj.get(other).is_some_and(Value::is_string) { obj.remove(other); }
        }
    }
}

/// What `to_ranks` read back: each page's key, by id (`fields` as `by_id` has them).
pub(super) fn keys_of(fields: &HashMap<String, BTreeMap<String, Value>>) -> HashMap<String, String> {
    fields.iter().filter_map(|(id, f)| {
        let path = f.get("path")?.as_str()?;
        Some((id.clone(), f.get(place_field(path).0)?.as_str()?.to_string()))
    }).collect()
}

/// The store's ranks as FORMAT 6 keys, against `keys`, what was read: the
/// longest run of siblings still in their read order keeps its keys, and
/// only the others get new ones between their neighbours. A move thus
/// changes the moved page alone; when nothing fits between two equal keys
/// (or a key cannot be read), the folder gets fresh keys throughout.
pub(super) fn to_keys(fields: &mut HashMap<String, BTreeMap<String, Value>>, keys: &HashMap<String, String>) {
    use crate::board::fractional_index::{generate_n_keys_between, validate_order_key};
    let mut groups: BTreeMap<String, Vec<(u64, String)>> = BTreeMap::new();
    for (id, f) in fields.iter_mut() {
        let Some(path) = f.get("path").and_then(Value::as_str).map(str::to_string) else { continue };
        let field = place_field(&path).0;
        let rank = f.get(field).and_then(Value::as_u64).unwrap_or(0);
        f.remove(field);
        if rank > 0 { groups.entry(parent_of(&path).to_string()).or_default().push((rank, id.clone())); }
    }
    for (_, mut g) in groups {
        g.sort();
        let ids: Vec<&String> = g.iter().map(|(_, id)| id).collect();
        let old: Vec<Option<&str>> = ids.iter().map(|id| keys.get(*id).map(String::as_str).filter(|k| validate_order_key(k).is_ok())).collect();
        let keep = longest_run(&ids, &old);
        let mut new: Vec<Option<String>> = old.iter().zip(&keep).map(|(k, keep)| if *keep { k.map(str::to_string) } else { None }).collect();
        let filled = (|| -> Option<()> {
            let mut i = 0;
            while i < new.len() {
                if new[i].is_some() { i += 1; continue; }
                let end = (i..new.len()).find(|&j| new[j].is_some()).unwrap_or(new.len());
                let (a, b) = (i.checked_sub(1).and_then(|p| new[p].clone()), new.get(end).cloned().flatten());
                if a.is_some() && a == b { return None; }
                let made = generate_n_keys_between(a.as_deref(), b.as_deref(), end - i).ok()?;
                for (j, k) in made.into_iter().enumerate() { new[i + j] = Some(k); }
                i = end;
            }
            Some(())
        })();
        if filled.is_none() { new = generate_n_keys_between(None, None, ids.len()).unwrap_or_default().into_iter().map(Some).collect(); }
        for (id, key) in ids.iter().zip(new) {
            let f = fields.get_mut(*id).unwrap();
            let path = f.get("path").and_then(Value::as_str).unwrap_or_default().to_string();
            if let Some(key) = key { f.insert(place_field(&path).0.into(), Value::String(key)); }
        }
    }
}

/// Which of `old` (keys, `None`: none) to keep: a longest run increasing by
/// (key, id), the order `to_ranks` reads them in.
fn longest_run(ids: &[&String], old: &[Option<&str>]) -> Vec<bool> {
    let at: Vec<usize> = (0..old.len()).filter(|&i| old[i].is_some()).collect();
    let less = |a: usize, b: usize| (old[a], ids[a]) < (old[b], ids[b]);
    // Patience sorting: `tails[l]` ends the best run of length l + 1.
    let (mut tails, mut prev): (Vec<usize>, Vec<Option<usize>>) = (vec![], vec![None; old.len()]);
    for &i in &at {
        let l = tails.partition_point(|&t| less(t, i));
        prev[i] = l.checked_sub(1).map(|p| tails[p]);
        if l == tails.len() { tails.push(i) } else { tails[l] = i }
    }
    let mut keep = vec![false; old.len()];
    let mut cur = tails.last().copied();
    while let Some(i) = cur { keep[i] = true; cur = prev[i]; }
    keep
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
        } else if hold_dataless(&p) {
            // Another device's, still only in iCloud: its changes count once it is here.
            warnings.push(not_downloaded(&format!("{stem}.json")));
            continue;
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
    let mut merged = Merged::default();
    commit_over(root, me, clock, &mut merged, changes, warnings)?;
    Ok(merged)
}

/// `commit` over `merged`, read again first unless the device files are
/// still as it read them. Not written: `merged` holds changes the disk does
/// not, so the next look reads everything again.
pub(super) fn commit_over(root: &Path, me: &Device, clock: &std::cell::Cell<u64>, merged: &mut Merged, changes: Vec<(String, String, Value)>, warnings: &mut Vec<String>) -> Result<()> {
    refresh(root, &me.id, merged, warnings);
    if !changes.is_empty() {
        let ts = stamp(clock, merged.max);
        for (item, field, v) in changes { merged.put(&item, &field, Won { v, ts, dev: me.id.clone() }); }
    }
    write_own(root, me, merged).inspect_err(|_| merged.files = None)
}

/// Write device `me`'s file from `merged`: the fields `me` wins there. Only
/// this device writes its file, so when the others' files are as `merged`
/// read them, the result stays good for `cached`.
fn write_own(root: &Path, me: &Device, merged: &mut Merged) -> Result<()> {
    let mut items: BTreeMap<String, Fields> = BTreeMap::new();
    // A page purged here 30 days ago: every device has had time to see it go,
    // so this device's fields about it are dropped (others drop their own).
    let expired = crate::time::now_ms().saturating_sub(PURGED_KEPT_MS);
    for fields in merged.items.values_mut() {
        if fields.get("purged").is_some_and(|w| w.dev == me.id && !w.v.is_null() && w.ts < expired) { fields.retain(|_, w| w.dev != me.id); }
    }
    merged.items.retain(|_, f| !f.is_empty());
    for (item, fields) in &merged.items {
        for (k, w) in fields { if w.dev == me.id { items.entry(item.clone()).or_default().insert(k.clone(), (w.v.clone(), w.ts)); } }
    }
    crate::vfs::create_dir_all(dir(root))?;
    let own = format!("{}.json", me.id);
    save_json(&dir(root).join(&own), &DeviceFile { v: 1, device: me.clone(), items })?;
    let now = files(root);
    merged.files = merged.files.as_ref().filter(|was| same_files(root, was, &now, Some(&own))).map(|_| now);
    Ok(())
}

/// This computer's id, made once and kept with its other local data; the
/// same for every top-level folder (sync.md 3.6).
pub fn device() -> Device {
    static DEVICE: std::sync::OnceLock<Device> = std::sync::OnceLock::new();
    let mut d = DEVICE.get_or_init(|| {
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
    }).clone();
    let named = NAME.read().unwrap_or_else(|e| e.into_inner());
    if !named.is_empty() { d.name = named.clone(); }
    d
}

/// The computer's name as the system shows it (macOS 电脑名称), handed in by
/// the app at launch: the core does not ask the system. Set it before the
/// first top-level folder opens; an empty name keeps the environment's.
pub fn set_device_name(name: &str) { *NAME.write().unwrap_or_else(|e| e.into_inner()) = name.trim().to_string(); }
static NAME: std::sync::RwLock<String> = std::sync::RwLock::new(String::new());

/// A name for this computer, for 移除已停用的设备 later.
fn whoami_host() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_default()
}

impl Device {
    /// What to show for this device: its name, else its platform and the
    /// start of its id ("macOS 9b2f"), so two unnamed ones stay apart.
    pub fn label(&self) -> String {
        if !self.name.trim().is_empty() { return self.name.trim().to_string() }
        let platform = match self.platform.as_str() {
            "macos" => "macOS", "ios" => "iOS", "windows" => "Windows", "linux" => "Linux", "android" => "Android",
            "" => "?", p => p,
        };
        format!("{platform} {}", self.id.chars().take(4).collect::<String>())
    }
}

/// A device with a file in `.quiet/meta/`, for 移除已停用的设备.
#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub device: Device,
    /// The latest stamp in its file (milliseconds): when it last wrote here.
    pub last: u64,
}

/// Every device with a file under `root`, the latest writer first. A file
/// that does not parse, or is still only in iCloud, is left out.
pub(super) fn devices(root: &Path) -> Vec<DeviceInfo> {
    let mut by_id: BTreeMap<String, DeviceInfo> = BTreeMap::new();
    for (_, f) in device_files(root) {
        let last = f.items.values().flat_map(|fields| fields.values().map(|(_, ts)| *ts)).max().unwrap_or(0);
        let e = by_id.entry(f.device.id.clone()).or_insert(DeviceInfo { device: f.device.clone(), last: 0 });
        if last >= e.last { *e = DeviceInfo { device: f.device, last }; }
    }
    let mut out: Vec<DeviceInfo> = by_id.into_values().collect();
    out.sort_by(|a, b| (b.last, &a.device.id).cmp(&(a.last, &b.device.id)));
    out
}

/// The device files there are, each with the device it is (`<id> 2.json`, a
/// sync's copy, is still `<id>`'s).
fn device_files(root: &Path) -> Vec<(PathBuf, DeviceFile)> {
    let Ok(rd) = crate::vfs::read_dir(dir(root)) else { return vec![] };
    let mut paths: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    paths.sort();
    paths.into_iter().filter(|p| !crate::vfs::is_dataless(p)).filter_map(|p| {
        let mut f: DeviceFile = serde_json::from_slice(&crate::vfs::read(&p).ok()?).ok()?;
        if f.device.id.is_empty() { f.device.id = p.file_stem()?.to_string_lossy().into_owned(); }
        Some((p, f))
    }).collect()
}

/// 移除已停用的设备: what device `gone` still wins becomes `me`'s, with the
/// same stamp, and its files go. The merged view stays as it was; only when
/// a third device holds a field at the very same stamp, and would now win
/// the tie, is the stamp moved on by one to keep the old winner.
pub(super) fn retire(root: &Path, me: &Device, gone: &str, warnings: &mut Vec<String>) -> Result<Merged> {
    anyhow::ensure!(gone != me.id, "不能移除当前设备");
    let all = load(root, &me.id, warnings);
    let files = device_files(root);
    anyhow::ensure!(files.iter().any(|(_, f)| f.device.id == gone), "没有这台设备");
    // Every file but `gone`'s, as `load` reads them (this device's own may come from its `.bak`).
    let own = dir(root).join(format!("{}.json", me.id));
    let mut rest = Merged::default();
    let mine: DeviceFile = load_json(&own, "文档索引", warnings);
    for (dev, f) in files.iter().filter(|(p, f)| f.device.id != gone && *p != own).map(|(_, f)| (&f.device.id, f)).chain([(&me.id, &mine)]) {
        for (item, fields) in &f.items {
            for (k, (v, ts)) in fields { rest.put(item, k, Won { v: v.clone(), ts: *ts, dev: dev.clone() }); }
        }
    }
    for (item, fields) in &all.items {
        for (k, w) in fields.iter().filter(|(_, w)| w.dev == gone) {
            let slot = rest.items.entry(item.clone()).or_default();
            // `gone` beat everyone, so only a tie at its stamp can come out differently.
            let ts = match slot.get(k) { Some(o) if o.dev != me.id && (o.ts, &o.dev) >= (w.ts, &me.id) => o.ts + 1, _ => w.ts };
            rest.max = rest.max.max(ts);
            slot.insert(k.clone(), Won { v: w.v.clone(), ts, dev: me.id.clone() });
        }
    }
    // This device's file first: a crash before the others go loses nothing.
    write_own(root, me, &mut rest)?;
    for (p, f) in files { if f.device.id == gone { crate::vfs::remove_file(&p)?; } }
    Ok(rest)
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

/// FORMAT 4 → 5: `index.json` and the top-level folder's settings become this device's
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
    if let Some(n) = settings.get("history_limit").filter(|v| !v.is_null()) { changes.push((FOLDER_ITEM.into(), "history_limit".into(), n.clone())); }
    if let Some(Value::Object(look)) = settings.get("look") {
        for (k, v) in look { changes.push((FOLDER_ITEM.into(), format!("look.{k}"), v.clone())); }
    }
    if let Some(Value::Object(tops)) = settings.get("scroll_top") {
        for (id, v) in tops { if ids.contains(id) { changes.push((id.clone(), "scroll".into(), v.clone())); } }
    }
    // Over a file a crash left behind: the index is still the truth.
    let _ = crate::vfs::remove_file(dir(root).join(format!("{}.json", me.id)));
    commit(root, me, &std::cell::Cell::new(0), changes, warnings)?;
    let marker = internal.join("format.json");
    let id = reread::<FormatMarker>(&marker).map(|m| m.id).unwrap_or_default();
    write_atomic(&marker, &serde_json::to_vec_pretty(&FormatMarker { format: 5, id: if id.is_empty() { uuid::Uuid::new_v4().to_string() } else { id } })?)?;
    if internal.join("index.json").fs_exists() { let _ = crate::vfs::rename(internal.join("index.json"), internal.join("index.json.v4")); }
    Ok(())
}

/// A 回收站 record as FORMAT 6's `trashed` field: where it was (`from`,
/// a folder's ending in `/`), when (`at`, seconds), the folder it was in
/// (`parent`) and the page it went with (`root`), and what the index knew
/// about it (`meta`; nothing of this computer's own, like 最近打开).
pub(super) fn trashed_value(t: &Trashed) -> Value {
    let folder = t.meta.folder.is_some();
    let mut fields = fields_of("", &Attached::of("", &t.meta));
    fields.remove("path");
    let mut v = serde_json::json!({ "from": if folder { format!("{}/", t.meta.rel) } else { t.meta.rel.clone() }, "at": t.deleted, "meta": fields });
    if let Some(p) = &t.meta.parent { v["parent"] = Value::String(p.clone()); }
    if let Some(r) = &t.root { v["root"] = Value::String(r.clone()); }
    v
}

/// `trashed_value` read back; `None` for what this build cannot read.
pub(super) fn trashed_of(v: &Value) -> Option<Trashed> {
    let from = v.get("from")?.as_str()?;
    let mut obj = v.get("meta").and_then(Value::as_object).cloned().unwrap_or_default();
    obj.insert("id".into(), Value::String(String::new()));
    let mut meta = serde_json::from_value::<Attached>(Value::Object(obj)).ok()?.meta();
    meta.rel = from.trim_end_matches('/').to_string();
    if from.ends_with('/') { meta.folder = Some(String::new()); }
    // Its name and kind follow from where it was, as in the index.
    let mut one = HashMap::from([(String::new(), meta)]);
    derive_tree(&mut one);
    let mut meta = one.remove("").unwrap_or_default();
    meta.parent = v.get("parent").and_then(Value::as_str).map(str::to_string);
    Some(Trashed { meta, deleted: v.get("at")?.as_u64()?, root: v.get("root").and_then(Value::as_str).map(str::to_string) })
}

/// FORMAT 5 → 6 (sync.md 3.3): the 回收站's `trash/index.json` becomes this
/// device's `trashed` fields, and the hand order (`order`, `top_order`)
/// fractional indices in the order it had. This device's file is written
/// first, the marker next and `trash/index.json` is kept as
/// `index.json.v5` last, so a crash midway redoes it to the same result.
pub(super) fn upgrade6(root: &Path, me: &Device, warnings: &mut Vec<String>) -> Result<()> {
    let internal = root.join(INTERNAL);
    let merged = load(root, &me.id, warnings);
    let read = by_id(&merged.disk_index());
    // Keys a crash left behind are kept: the same order comes out again.
    let raw: HashMap<String, BTreeMap<String, Value>> = merged.items.iter().map(|(id, f)| (id.clone(), f.iter().filter(|(_, w)| !w.v.is_null()).map(|(k, w)| (k.clone(), w.v.clone())).collect())).collect();
    let mut keyed = read.clone();
    to_keys(&mut keyed, &keys_of(&raw));
    let mut changes = vec![];
    for (id, f) in &keyed {
        for k in ["order", "root_order"] {
            let (was, now) = (raw.get(id).and_then(|r| r.get(k)), f.get(k));
            if was != now { changes.push((id.clone(), k.to_string(), now.cloned().unwrap_or(Value::Null))); }
        }
    }
    let index = internal.join("trash").join("index.json");
    let trash: HashMap<String, Trashed> = load_json(&index, "回收站索引", warnings);
    for (id, t) in &trash {
        let v = trashed_value(t);
        if raw.get(id).and_then(|r| r.get("trashed")) != Some(&v) { changes.push((id.clone(), "trashed".into(), v)); }
    }
    changes.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    commit(root, me, &std::cell::Cell::new(0), changes, warnings)?;
    let marker = internal.join("format.json");
    let id = reread::<FormatMarker>(&marker).map(|m| m.id).unwrap_or_default();
    write_atomic(&marker, &serde_json::to_vec_pretty(&FormatMarker { format: 6, id: if id.is_empty() { uuid::Uuid::new_v4().to_string() } else { id } })?)?;
    if index.fs_exists() { crate::vfs::rename(&index, internal.join("trash").join("index.json.v5"))?; }
    Ok(())
}
