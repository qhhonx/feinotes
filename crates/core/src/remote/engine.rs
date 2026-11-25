//! One sync of a folder with a remote branch (docs/design/sync.md §2.1, §2.3):
//! pull what the remote changed, merge where both sides changed a file, push
//! one commit whose parent is the remote's head. Nothing piles up here: an
//! offline device keeps its edits on disk and the next sync pushes them all.
//!
//! The base of a three-way merge is the file as it was at the last synced
//! commit, which the remote still has: its blob, by the id kept in [`State`]
//! (and cached next to it). So the base never depends on a snapshot this
//! device had to take, and a device that lost its cache just fetches it.
//!
//! Only the files Quiet.md manages are synced ([`synced`], D10); the rest of
//! the remote's tree is carried along untouched by each commit.

use super::{Error, Result, Sha, SyncRemote, blob_sha, safe_path};
use crate::merge_file::{FileMerge, Kind, merge_file};
use crate::vfs::{self, KindExt, PathExt};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// GitHub refuses files past this (2.5): not uploaded, listed as unsyncable.
pub const TOO_LARGE: u64 = 100 * 1024 * 1024;
/// GitHub warns past this: listed so the interface can say so.
pub const LARGE: u64 = 50 * 1024 * 1024;

// ------------------------------------------------------------ what is synced

/// Whether `path` (relative to the library, with `/`) is a file Quiet.md
/// manages and so goes to the remote (D10): documents, `.assets/` but its
/// thumbnails, and the shared parts of `.quiet/` (format, per-device metadata,
/// trash, themes). Never the history (D3), locks, temporary files, system
/// litter or anything in a folder of another app (`.git/`, `.obsidian/` …).
pub fn synced(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    let name = parts[parts.len() - 1];
    if matches!(name, ".DS_Store" | "Thumbs.db" | "desktop.ini") || [".lock", ".tmp", ".sqlite-journal"].iter().any(|s| name.ends_with(s)) { return false }
    match parts[..] {
        [".gitignore"] => true,
        [".quiet", "format.json"] => true,
        // FORMAT 5's whole file per device; FORMAT 6's shards in a folder per
        // device. Not a conversion's `<id>.tmp/` or the `<id>.json.single` it keeps.
        [".quiet", "meta", file] => file.ends_with(".json"),
        [".quiet", "meta", dev, file] => !dev.ends_with(".tmp") && file.ends_with(".json"),
        // FORMAT 5's shared trash index and the copy FORMAT 6 keeps of it are this computer's.
        [".quiet", "trash", "index.json" | "index.json.v5"] => false,
        [".quiet", "trash" | "themes", _, ..] => true,
        [".quiet", ..] => false,
        [".assets", "thumbs", ..] => false,
        [".assets", _, ..] => true,
        _ => !parts.iter().any(|p| p.starts_with('.')) && is_document(name),
    }
}

/// A folder nothing under which is synced, so a scan need not go in.
fn skipped_dir(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    match parts[..] {
        [".quiet", "meta", dev, ..] if dev.ends_with(".tmp") => true,
        [".quiet"] | [".assets"] | [".quiet", "meta" | "trash" | "themes", ..] => false,
        [".assets", "thumbs", ..] => true,
        [".assets", ..] => false,
        _ => parts.iter().any(|p| p.starts_with('.')),
    }
}

fn is_document(name: &str) -> bool {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "xml" | "md" | "excalidraw" | "sqlite")
}

/// The titles of the documents among `paths`, for a commit message
/// (`周报、提案 (MacBook)`): file names without extension, each once.
pub fn titles(paths: &[String]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for p in paths {
        let name = p.rsplit('/').next().unwrap_or(p);
        if p.split('/').any(|s| s.starts_with('.')) || !is_document(name) { continue }
        let title = name.rsplit_once('.').map_or(name, |(t, _)| t).to_string();
        if !out.contains(&title) { out.push(title) }
    }
    out
}

const GITIGNORE_START: &str = "# >>> Quiet.md（自动生成，请勿修改这一段）";
const GITIGNORE_END: &str = "# <<< Quiet.md";
/// What a clone of the repository should leave alone (2.4); mostly older
/// layouts' local files, since FORMAT 5 keeps them outside the library.
const GITIGNORE: &[&str] = &[
    ".quiet/.lock", ".quiet/index.json", ".quiet/index.json.bak", ".quiet/*.corrupt-*", ".quiet/settings.json",
    ".quiet/history/", ".quiet/history-trash/", ".quiet/before-folders-*/", ".quiet/trash/index.json", ".quiet/trash/index.json.v5", ".assets/thumbs/",
    ".quiet/meta/*.json.single", ".quiet/meta/*.tmp/",
    "*.sqlite-journal", "*.tmp", ".DS_Store", "Thumbs.db",
];

/// `.gitignore` with Quiet.md's marked part put in or brought up to date;
/// everything outside the markers stays as the person wrote it.
pub fn gitignore(existing: &str) -> String {
    let block = format!("{GITIGNORE_START}\n{}\n{GITIGNORE_END}\n", GITIGNORE.join("\n"));
    let lines: Vec<&str> = existing.split_inclusive('\n').collect();
    let start = lines.iter().position(|l| l.trim_end().starts_with("# >>> Quiet.md"));
    let Some(start) = start else {
        if existing.is_empty() { return block }
        let sep = if existing.ends_with("\n\n") { "" } else if existing.ends_with('\n') { "\n" } else { "\n\n" };
        return format!("{existing}{sep}{block}");
    };
    // No end marker: the part runs to the end.
    let end = lines[start..].iter().position(|l| l.trim_end().starts_with(GITIGNORE_END)).map_or(lines.len(), |i| start + i + 1);
    format!("{}{block}{}", lines[..start].concat(), lines[end..].concat())
}

// ------------------------------------------------------------ the local folder

/// A file in the folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Local {
    pub path: String,
    pub size: u64,
    /// Last modified, in nanoseconds since 1970; 0 when unknown.
    pub modified: u128,
}

/// The folder being synced, as sync sees it. Paths are relative, with `/`.
pub trait Folder {
    /// Every file that may be synced (it may list more; [`synced`] picks).
    fn list(&self) -> Result<Vec<Local>>;
    fn read(&self, path: &str) -> Result<Vec<u8>>;
    /// Whole or not at all, making the folders it needs.
    fn write(&self, path: &str, bytes: &[u8]) -> Result<()>;
    fn remove(&self, path: &str) -> Result<()>;
    fn exists(&self, path: &str) -> bool;
}

/// A folder on disk. (The library's own writes go through `Store`; joining
/// that up is the next step.)
pub struct Dir(pub PathBuf);

fn local(path: &Path, e: std::io::Error) -> Error { Error::Local(format!("{}: {e}", path.display())) }

impl Folder for Dir {
    fn list(&self) -> Result<Vec<Local>> {
        fn walk(root: &Path, rel: &str, out: &mut Vec<Local>) -> Result<()> {
            let dir = if rel.is_empty() { root.to_path_buf() } else { root.join(rel) };
            for e in vfs::read_dir(&dir).map_err(|e| local(&dir, e))? {
                let e = e.map_err(|e| local(&dir, e))?;
                let Some(name) = e.file_name().to_str().map(str::to_string) else { continue };
                let path = if rel.is_empty() { name } else { format!("{rel}/{name}") };
                let kind = e.file_type().map_err(|e| local(&dir, e))?;
                if kind.is_symlink() { continue }
                if kind.fs_is_dir() {
                    if !skipped_dir(&path) { walk(root, &path, out)? }
                } else if let Ok(m) = e.metadata() {
                    let modified = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
                    out.push(Local { path, size: m.len(), modified });
                }
            }
            Ok(())
        }
        let mut out = vec![];
        walk(&self.0, "", &mut out)?;
        Ok(out)
    }

    fn read(&self, path: &str) -> Result<Vec<u8>> {
        let p = self.0.join(path);
        vfs::read(&p).map_err(|e| local(&p, e))
    }

    fn write(&self, path: &str, bytes: &[u8]) -> Result<()> {
        let p = self.0.join(path);
        if let Some(dir) = p.parent() { vfs::create_dir_all(dir).map_err(|e| local(dir, e))? }
        crate::store::write_atomic(&p, bytes).map_err(|e| Error::Local(format!("{}: {e}", p.display())))
    }

    /// Folders left empty go too, as git has no empty folders.
    fn remove(&self, path: &str) -> Result<()> {
        let p = self.0.join(path);
        match vfs::remove_file(&p) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(local(&p, e)),
            _ => {}
        }
        let mut dir = p.parent();
        while let Some(d) = dir && d != self.0 && vfs::remove_dir(d).is_ok() { dir = d.parent() }
        Ok(())
    }

    fn exists(&self, path: &str) -> bool { self.0.join(path).fs_exists() }
}

// ------------------------------------------------------------ what this device remembers

/// What this device knows of the remote (`github.json` in the folder the
/// caller gives, later `quiet-md/sync/<root key>/`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    /// `owner/name`, for the caller; sync doesn't read it.
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub branch: String,
    /// The last commit this folder was synced with; None before the first sync.
    #[serde(default)]
    pub commit: Option<Sha>,
    /// The synced files at `commit` (path → blob): the base of the next merge.
    #[serde(default)]
    pub files: BTreeMap<String, Sha>,
    /// Conflict copies (theirs), kept off the remote until the person deals with them.
    #[serde(default)]
    pub held: BTreeSet<String>,
    /// File ids by size and time, so unchanged files aren't read again.
    #[serde(default)]
    pub seen: BTreeMap<String, Seen>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seen { pub size: u64, pub modified: u128, pub sha: Sha }

const STATE: &str = "github.json";
/// Bases of mergeable files, by blob id.
const BASES: &str = "base";

impl State {
    pub fn new(repo: &str, branch: &str) -> Self { State { repo: repo.into(), branch: branch.into(), ..Default::default() } }

    /// The state in `dir`; a fresh one for `branch` when there is none.
    pub fn load(dir: &Path) -> Result<State> {
        let p = dir.join(STATE);
        match vfs::read(&p) {
            Ok(b) => serde_json::from_slice(&b).map_err(|e| Error::Local(format!("{}: {e}", p.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
            Err(e) => Err(local(&p, e)),
        }
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        vfs::create_dir_all(dir).map_err(|e| local(dir, e))?;
        let p = dir.join(STATE);
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| Error::Local(e.to_string()))?;
        crate::store::write_atomic(&p, &bytes).map_err(|e| Error::Local(format!("{}: {e}", p.display())))
    }
}

/// The person dealt with a conflict copy by keeping it as a document of its
/// own: from the next sync on it is uploaded like any other.
pub fn release(dir: &Path, path: &str) -> Result<()> {
    let mut s = State::load(dir)?;
    if s.held.remove(path) { s.save(dir)? }
    Ok(())
}

// ------------------------------------------------------------ merging

pub enum Merged {
    Clean(Vec<u8>),
    /// `merged` (ours plus whatever merged on its own) goes in place, `theirs`
    /// becomes a conflict copy; `regions` is how many places kept ours over
    /// theirs (1 when the whole file did), for the interface to say.
    Conflict { merged: Vec<u8>, theirs: Vec<u8>, regions: usize },
}

/// How a file both sides changed comes together: [`DocMerger`] merges
/// documents block by block; [`KeepBoth`] merges nothing.
pub trait Merger {
    /// Whether `path` is merged by its contents (then `merge` gets the base).
    fn mergeable(&self, path: &str) -> bool;
    fn merge(&self, path: &str, base: Option<&[u8]>, ours: &[u8], theirs: &[u8]) -> Merged;
}

/// No merging: two different versions are a conflict.
pub struct KeepBoth;

impl Merger for KeepBoth {
    fn mergeable(&self, _: &str) -> bool { false }
    fn merge(&self, _: &str, _: Option<&[u8]>, ours: &[u8], theirs: &[u8]) -> Merged {
        if ours == theirs { Merged::Clean(ours.to_vec()) } else { Merged::Conflict { merged: ours.to_vec(), theirs: theirs.to_vec(), regions: 1 } }
    }
}

/// Documents (`.xml`, `.md`) merged by block (`merge_file`, sync.md 3.2);
/// anything else as [`KeepBoth`].
pub struct DocMerger;

impl Merger for DocMerger {
    fn mergeable(&self, path: &str) -> bool { Kind::of_name(path) != Kind::Other }
    fn merge(&self, path: &str, base: Option<&[u8]>, ours: &[u8], theirs: &[u8]) -> Merged {
        let kind = Kind::of_name(path);
        if kind == Kind::Other { return KeepBoth.merge(path, base, ours, theirs) }
        match merge_file(base, ours, theirs, kind) {
            FileMerge::Clean(b) => Merged::Clean(b),
            FileMerge::Conflicts { merged, regions, theirs } => Merged::Conflict { merged, theirs, regions: regions.len().max(1) },
        }
    }
}

// ------------------------------------------------------------ one sync

/// The first sync of a folder that has files, with a remote that has files too (2.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum First {
    /// Stop and say so ([`Outcome::needs_choice`]): the person picks between
    /// downloading into a new folder (sync that empty folder) and [`First::Merge`].
    Ask,
    /// Put the two together: a file both have, differently, keeps ours and gets theirs as a conflict copy.
    Merge,
}

pub struct Options<'a> {
    /// [`DocMerger`] unless set.
    pub merger: &'a dyn Merger,
    /// Where theirs goes when a file conflicts, e.g. `周报（冲突 · iPad · 11-22 14:30）.xml`
    /// for `周报.xml`; taken names get ` (2)`, ` (3)` …
    pub conflict_copy: &'a dyn Fn(&str) -> String,
    /// The commit message for the files a commit changes (see [`titles`]).
    pub message: &'a dyn Fn(&[String]) -> String,
    pub first: First,
    /// How many times to pull and merge again when another device pushed first.
    pub retries: usize,
    /// Files past this are not uploaded ([`TOO_LARGE`]).
    pub too_large: u64,
    /// Files past this are uploaded with a word ([`LARGE`]).
    pub large: u64,
}

impl<'a> Options<'a> {
    /// Documents merged by block, asking on a first sync with files on both
    /// sides, three tries, GitHub's limits.
    pub fn new(conflict_copy: &'a dyn Fn(&str) -> String, message: &'a dyn Fn(&[String]) -> String) -> Self {
        Options { merger: &DocMerger, conflict_copy, message, first: First::Ask, retries: 3, too_large: TOO_LARGE, large: LARGE }
    }
}

/// What a sync did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Put on the remote (changed or removed there).
    pub uploaded: Vec<String>,
    /// Taken from the remote (written or removed here).
    pub downloaded: Vec<String>,
    /// Changed on both sides and merged.
    pub merged: Vec<String>,
    /// Changed on both sides and not wholly merged: (the file, with ours where
    /// they differ; the conflict copy, theirs; how many places differ).
    pub conflicts: Vec<(String, String, usize)>,
    /// Past [`Options::too_large`]: not uploaded (path, size).
    pub too_large: Vec<(String, u64)>,
    /// Past [`Options::large`]: uploaded, but worth a word (path, size).
    pub large: Vec<(String, u64)>,
    /// On the remote under a name this device can't write (`a:b.xml` on Windows …).
    pub skipped: Vec<String>,
    /// The commit pushed; None when there was nothing to push.
    pub commit: Option<Sha>,
    /// First sync, both sides have files: nothing was done, see [`First::Ask`].
    pub needs_choice: bool,
}

/// Pull, merge, push: one sync of `folder` with `remote`, remembering in `dir`.
pub fn sync(folder: &dyn Folder, remote: &dyn SyncRemote, dir: &Path, o: &Options) -> Result<Outcome> {
    let mut state = State::load(dir)?;
    let mut out = Outcome::default();
    for _ in 0..=o.retries {
        match round(folder, remote, dir, &mut state, o, &mut out) {
            Err(Error::Rejected) => continue,
            r => {
                r?;
                for list in [&mut out.uploaded, &mut out.downloaded, &mut out.merged] { list.sort(); list.dedup() }
                return Ok(out);
            }
        }
    }
    Err(Error::Rejected)
}

/// What a round does with one file.
enum Act { Take(Option<Sha>), Push, Merge(Sha) }

fn round(folder: &dyn Folder, remote: &dyn SyncRemote, dir: &Path, state: &mut State, o: &Options, out: &mut Outcome) -> Result<()> {
    let ours = scan(folder, state)?;
    let head = remote.head(&state.branch)?;
    // The branch is gone (or the repository emptied) after syncing with it:
    // start over as on a first sync with an empty remote, uploading all.
    if head.is_none() { state.commit = None; state.files.clear() }
    let base = state.files.clone();
    // Contents already in hand (a snapshot), by blob id.
    let mut known: HashMap<Sha, Vec<u8>> = HashMap::new();
    let mut theirs: BTreeMap<String, Sha> = BTreeMap::new();
    match (&state.commit, &head) {
        (_, None) => {}
        (None, Some(h)) => {
            if !ours.is_empty() && o.first == First::Ask { out.needs_choice = true; return Ok(()) }
            for (path, bytes) in remote.snapshot(h)? {
                if !synced(&path) { continue }
                let sha = blob_sha(&bytes);
                theirs.insert(path, sha.clone());
                known.insert(sha, bytes);
            }
        }
        (Some(c), Some(h)) if c == h => theirs = base.clone(),
        (Some(c), Some(h)) => {
            theirs = base.clone();
            match remote.changed(c, h) {
                Ok(changes) => for ch in changes {
                    if let Some(p) = &ch.previous { theirs.remove(p); }
                    match ch.sha { Some(s) => { theirs.insert(ch.path, s); } None => { theirs.remove(&ch.path); } }
                },
                // The last synced commit is gone (history rewritten elsewhere): the head's tree.
                Err(Error::NotFound(_)) => theirs = remote.tree(h)?.into_iter().map(|e| (e.path, e.sha)).collect(),
                Err(e) => return Err(e),
            }
        }
    }
    theirs.retain(|p, _| synced(p));
    out.skipped = theirs.keys().filter(|p| !safe_path(p)).cloned().collect();
    theirs.retain(|p, _| safe_path(p));

    let paths: BTreeSet<&String> = base.keys().chain(ours.keys()).chain(theirs.keys()).collect();
    let mut acts = vec![];
    for p in paths {
        let (b, l, r) = (base.get(p), ours.get(p), theirs.get(p));
        let act = if l == r { continue }
            else if l == b { Act::Take(r.cloned()) }
            else if r == b { Act::Push }
            else {
                match (l, r) {
                    // Removed on one side, changed on the other: the change stays.
                    (None, Some(_)) => Act::Take(r.cloned()),
                    (Some(_), None) => Act::Push,
                    (Some(_), Some(r)) => Act::Merge(r.clone()),
                    (None, None) => continue,
                }
            };
        acts.push((p.clone(), act));
    }

    let fetch = |sha: &str, known: &HashMap<Sha, Vec<u8>>| -> Result<Vec<u8>> {
        match known.get(sha) { Some(b) => Ok(b.clone()), None => remote.blob(sha) }
    };
    // What goes up: path → new contents (None: removed).
    let mut push: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
    let mut taken_copies = HashSet::new();
    for (p, act) in acts {
        match act {
            Act::Take(Some(sha)) => {
                let bytes = fetch(&sha, &known)?;
                folder.write(&p, &bytes)?;
                keep_base(dir, o, &p, &sha, &bytes);
                out.downloaded.push(p);
            }
            Act::Take(None) => { folder.remove(&p)?; out.downloaded.push(p) }
            Act::Push => { let bytes = if ours.contains_key(&p) { Some(folder.read(&p)?) } else { None }; push.insert(p, bytes); }
            Act::Merge(sha) => {
                let mine = folder.read(&p)?;
                let other = fetch(&sha, &known)?;
                keep_base(dir, o, &p, &sha, &other);
                let b = match base.get(&p) { Some(s) if o.merger.mergeable(&p) => Some(base_blob(dir, remote, s)?), _ => None };
                match o.merger.merge(&p, b.as_deref(), &mine, &other) {
                    Merged::Clean(m) => {
                        if m != mine { folder.write(&p, &m)? }
                        if m != other { push.insert(p.clone(), Some(m)); }
                        out.merged.push(p);
                    }
                    Merged::Conflict { merged: m, theirs: t, regions } => {
                        if m != mine { folder.write(&p, &m)? }
                        let copy = free_name(folder, &(o.conflict_copy)(&p), &taken_copies);
                        folder.write(&copy, &t)?;
                        taken_copies.insert(copy.clone());
                        state.held.insert(copy.clone());
                        if m != other { push.insert(p.clone(), Some(m)); }
                        out.conflicts.push((p, copy, regions));
                    }
                }
            }
        }
    }

    // Pulled and merged: the remote's head is the base from here on, also if
    // the push below is refused (then the next round merges from it).
    state.commit = head.clone();
    state.files = theirs;
    let mut changes = vec![];
    for (p, bytes) in push {
        let size = bytes.as_ref().map_or(0, |b| b.len() as u64);
        if size > o.too_large { out.too_large.push((p, size)); continue }
        if size > o.large { out.large.push((p.clone(), size)) }
        changes.push((p, bytes));
    }
    out.too_large.dedup();
    out.large.dedup();
    if changes.is_empty() { return finish(dir, state) }
    state.save(dir)?;
    let paths: Vec<String> = changes.iter().map(|(p, _)| p.clone()).collect();
    let commit = remote.commit(&state.branch, head.as_deref(), &changes, &(o.message)(&paths))?;
    for (p, bytes) in changes {
        match bytes {
            Some(b) => { let sha = blob_sha(&b); keep_base(dir, o, &p, &sha, &b); state.files.insert(p, sha); }
            None => { state.files.remove(&p); }
        }
    }
    state.commit = Some(commit.clone());
    out.commit = Some(commit);
    out.uploaded.extend(paths);
    finish(dir, state)
}

/// The state saved, and bases no longer needed dropped.
fn finish(dir: &Path, state: &State) -> Result<()> {
    state.save(dir)?;
    let wanted: HashSet<&str> = state.files.values().map(String::as_str).collect();
    if let Ok(rd) = vfs::read_dir(dir.join(BASES)) {
        for e in rd.flatten() {
            if !e.file_name().to_str().is_some_and(|n| wanted.contains(n)) { let _ = vfs::remove_file(e.path()); }
        }
    }
    Ok(())
}

/// The folder's synced files by id, reading only those whose size or time changed.
fn scan(folder: &dyn Folder, state: &mut State) -> Result<BTreeMap<String, Sha>> {
    let files = folder.list()?;
    let present: HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    // A conflict copy the person removed is dealt with.
    state.held.retain(|p| present.contains(p.as_str()));
    // A file changed within the last two seconds may change again without its
    // time moving (coarse clocks): read it next time too.
    let now = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let mut out = BTreeMap::new();
    let mut seen = BTreeMap::new();
    for f in files {
        if !synced(&f.path) || state.held.contains(&f.path) { continue }
        let sha = match state.seen.get(&f.path) {
            Some(s) if f.modified != 0 && s.size == f.size && s.modified == f.modified => s.sha.clone(),
            _ => blob_sha(&folder.read(&f.path)?),
        };
        if f.modified != 0 && now.saturating_sub(f.modified) > 2_000_000_000 {
            seen.insert(f.path.clone(), Seen { size: f.size, modified: f.modified, sha: sha.clone() });
        }
        out.insert(f.path, sha);
    }
    state.seen = seen;
    Ok(out)
}

/// The base of a merge: cached, or fetched by its id.
fn base_blob(dir: &Path, remote: &dyn SyncRemote, sha: &str) -> Result<Vec<u8>> {
    if let Ok(b) = vfs::read(dir.join(BASES).join(sha)) && blob_sha(&b) == sha { return Ok(b) }
    remote.blob(sha)
}

/// A mergeable file's contents kept as a future base.
fn keep_base(dir: &Path, o: &Options, path: &str, sha: &str, bytes: &[u8]) {
    if !o.merger.mergeable(path) { return }
    let d = dir.join(BASES);
    if vfs::create_dir_all(&d).is_ok() && !d.join(sha).fs_is_file() { let _ = crate::store::write_atomic(&d.join(sha), bytes); }
}

/// `name`, or `name (2)`, `name (3)` … before the extension, whichever is free.
fn free_name(folder: &dyn Folder, name: &str, taken: &HashSet<String>) -> String {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && !s.ends_with('/') && !e.contains('/') => (s, format!(".{e}")),
        _ => (name, String::new()),
    };
    let free = |n: &str| !folder.exists(n) && !taken.contains(n);
    if free(name) { return name.to_string() }
    (2..).map(|i| format!("{stem} ({i}){ext}")).find(|n| free(n)).expect("a free name")
}

#[cfg(test)]
mod tests;
