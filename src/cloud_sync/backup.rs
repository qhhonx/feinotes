// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Portable snapshots of an active library, including its durable sync queue.

use super::{assets, content, shared};
use crate::store::{SQLITE_FILE, Store};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{
    ffi::{CString, c_char, c_int, c_uint},
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Read, Write},
    os::unix::{ffi::OsStrExt, fs::{DirBuilderExt, OpenOptionsExt}},
    path::{Path, PathBuf},
    time::Duration,
};

const MAGIC: &[u8] = b"Fei Notes Backup\n\x01";
const MAX_PATH: usize = 4096;
const MAX_FILES: usize = 100_000;
const READY: &str = ".cloud-restore-ready";
const TICKET: &str = ".backup-restore-id";
const NAMESPACE_FILE: &str = ".backup-cloud-environment";
const CLOUD_NAMESPACE: &str = env!("FEI_CLOUD_NAMESPACE");
const LEGACY_NAMESPACE: &str = "Development:iCloud.app.quietmd.cloudlab";

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Summary {
    pub notes: i64,
    pub folders: i64,
    pub trash: i64,
    pub shared: i64,
    pub pending: i64,
}

#[derive(Debug)]
struct Stage(PathBuf);
impl Stage {
    fn new(path: PathBuf) -> Result<Self> {
        fs::DirBuilder::new().mode(0o700).create(&path)?;
        Ok(Self(path))
    }
    fn keep(self) { std::mem::forget(self); }
}
impl Drop for Stage {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

fn open_db(path: &Path, flags: OpenFlags) -> Result<Connection> {
    let db = Connection::open_with_flags(path, flags)?;
    db.busy_timeout(Duration::from_secs(5))?;
    db.pragma_update(None, "trusted_schema", false)?;
    Ok(db)
}

fn table(db: &Connection, name: &str) -> Result<bool> {
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)", [name], |row| row.get(0))?)
}

fn channel(db: &Connection) -> Result<Option<String>> {
    if !table(db, "state")? { return Ok(None); }
    Ok(db.query_row("SELECT value FROM state WHERE key='channel'", [], |row| row.get(0)).optional()?)
}

fn check_version(db: &Connection, supported: u32) -> Result<()> {
    let version = db.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?;
    ensure!((2..=supported).contains(&version), "This backup requires a different Fei Notes version.");
    Ok(())
}

fn validate_library(database: &Path, marker: &[u8], files: &Path) -> Result<Summary> {
    let db = open_db(database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    ensure!(db.pragma_query_value(None, "integrity_check", |row| row.get::<_, String>(0))? == "ok",
        "The backup's SQLite database failed its integrity check.");
    ensure!(db.query_row("SELECT count(*) FROM sqlite_schema WHERE type NOT IN ('table','index')
        OR sql LIKE 'CREATE VIRTUAL TABLE%'", [], |row| row.get::<_, i64>(0))? == 0,
        "The backup contains an unsupported database schema.");
    ensure!(db.query_row("SELECT value FROM library_values WHERE key='restored'", [], |row| row.get::<_, Vec<u8>>(0))? == marker,
        "The backup's library identity does not match its database.");
    let mut docs = db.prepare("SELECT id, trashed FROM library_documents")?;
    let mut rows = docs.query([])?;
    let mut summary = Summary::default();
    while let Some(row) = rows.next()? {
        let (id, trashed): (String, bool) = (row.get(0)?, row.get(1)?);
        let (doc, _, _) = Store::read_sync_body(&db, &id, trashed)?;
        for source in content::local_assets(&doc).into_iter().filter(|src| src.starts_with("assets/")) {
            ensure!(assets::present(files, &source), "Missing attachment: {source}");
        }
    }
    summary.notes = db.query_row("SELECT count(*) FROM library_entries WHERE id IN
        (SELECT id FROM library_documents WHERE trashed=0)", [], |row| row.get(0))?;
    summary.folders = db.query_row("SELECT count(*) FROM library_entries WHERE json_type(meta, '$.folder')='text'",
        [], |row| row.get(0))?;
    summary.trash = db.query_row("SELECT count(*) FROM library_trash", [], |row| row.get(0))?;
    if table(&db, "outbox")? {
        summary.pending = db.query_row("SELECT count(*) FROM outbox", [], |row| row.get(0))?;
    }
    Ok(summary)
}

// Every path is relative to the selected library. Lock files, WAL sidecars,
// temporary restore folders and external Markdown files are never archived.
fn valid_path(name: &str) -> bool {
    let parts: Vec<_> = name.split('/').collect();
    if name.len() > MAX_PATH || parts.iter().any(|part| part.is_empty() || *part == "." || *part == ".."
        || part.contains([':', '\\', '\0'])) { return false; }
    let valid_room = |id: &str| uuid::Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == id);
    match parts.as_slice() {
        [SQLITE_FILE | READY | NAMESPACE_FILE] => true,
        ["assets" | "themes", _, ..] => true,
        ["shared-notes", id, SQLITE_FILE | READY | "shared-note.json" | "seed-published"] => valid_room(id),
        ["shared-notes", id, "assets" | "themes", _, ..] => valid_room(id),
        _ => false,
    }
}

fn copy_hashed(reader: &mut impl Read, writer: &mut impl Write, size: u64) -> Result<[u8; 32]> {
    let mut data = [0u8; 64 * 1024];
    let mut digest = Sha256::new();
    let mut left = size;
    while left > 0 {
        let len = left.min(data.len() as u64) as usize;
        reader.read_exact(&mut data[..len])?;
        writer.write_all(&data[..len])?;
        digest.update(&data[..len]);
        left -= len as u64;
    }
    Ok(digest.finalize().into())
}

struct Archive<W> {
    file: W,
    digest: Sha256,
    count: usize,
}

impl<W: Write> Archive<W> {
    fn add(&mut self, source: &Path, name: &str) -> Result<()> {
        ensure!(valid_path(name) && self.count < MAX_FILES, "Unsupported backup file: {name}");
        ensure!(source.symlink_metadata()?.is_file(), "Backup requires a regular file: {name}");
        let mut input = File::open(source)?;
        let meta = input.metadata()?;
        let size = meta.len();
        let header = [(name.len() as u32).to_le_bytes().as_slice(), name.as_bytes(), &size.to_le_bytes()].concat();
        self.file.write_all(&header)?;
        let digest = copy_hashed(&mut input, &mut self.file, size)?;
        ensure!(input.read(&mut [0])? == 0 && input.metadata()?.modified()? == meta.modified()?,
            "File changed during backup: {name}");
        self.file.write_all(&digest)?;
        self.digest.update(&header);
        self.digest.update(digest);
        self.count += 1;
        Ok(())
    }

    fn folder(&mut self, root: &Path, prefix: &str) -> Result<()> {
        let path = root.join(prefix);
        let meta = match path.symlink_metadata() {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        ensure!(meta.is_dir(), "Invalid library folder: {prefix}");
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let name = format!("{prefix}/{}", entry.file_name().to_str().context("Unsupported file name in library")?);
            ensure!(valid_path(&name), "Unsupported backup path: {name}");
            if entry.file_type()?.is_dir() { self.folder(root, &name)?; }
            else { self.add(&entry.path(), &name)?; }
        }
        Ok(())
    }
}

fn capture(archive: &mut Archive<impl Write>, root: &Path, prefix: &str, snapshot: &Path) -> Result<Summary> {
    let marker = fs::read(root.join(READY))?;
    let database = root.join(SQLITE_FILE);
    ensure!(database.symlink_metadata()?.is_file(), "Invalid library database.");
    open_db(&database, OpenFlags::SQLITE_OPEN_READ_WRITE)?
        .execute("VACUUM INTO ?1", [snapshot.to_str().context("Invalid backup staging path")?])?;
    let summary = validate_library(snapshot, &marker, root)?;
    archive.add(snapshot, &format!("{prefix}{SQLITE_FILE}"))?;
    archive.add(&root.join(READY), &format!("{prefix}{READY}"))?;
    if !prefix.is_empty() {
        archive.add(&root.join(shared::MARKER), &format!("{prefix}{}", shared::MARKER))?;
        let published = root.join("seed-published");
        if published.exists() { archive.add(&published, &format!("{prefix}seed-published"))?; }
    }
    fs::remove_file(snapshot)?;
    Ok(summary)
}

pub(crate) fn create(root: &Path, folder: &Path) -> Result<(PathBuf, Summary)> {
    let folder = folder.canonicalize()?;
    ensure!(!folder.starts_with(root.canonicalize()?), "Choose a backup folder outside the document library.");
    let stage = Stage::new(folder.join(format!(".fei-backup-{}", uuid::Uuid::new_v4())))?;
    let scratch = Stage::new(root.parent().context("Missing library parent")?.join(format!(".backup-{}", uuid::Uuid::new_v4())))?;
    let snapshot = scratch.0.join(SQLITE_FILE);
    let temporary = stage.0.join("archive");
    let mut file = BufWriter::new(OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary)?);
    file.write_all(MAGIC)?;
    let mut archive = Archive { file, digest: Sha256::new(), count: 0 };
    let mut summary = capture(&mut archive, root, "", &snapshot)?;
    if owner(&fs::read(root.join(READY))?)? != "local-only" {
        let marker = scratch.0.join(NAMESPACE_FILE);
        fs::write(&marker, CLOUD_NAMESPACE)?;
        archive.add(&marker, NAMESPACE_FILE)?;
    }
    archive.folder(root, "assets")?;
    archive.folder(root, "themes")?;
    for room in shared::rooms(root)? {
        let path = shared::room_root(root, &room.scope.note)?;
        let prefix = format!("shared-notes/{}", room.scope.note);
        let info = capture(&mut archive, &path, &format!("{prefix}/"), &snapshot)?;
        archive.folder(root, &format!("{prefix}/assets"))?;
        archive.folder(root, &format!("{prefix}/themes"))?;
        summary.shared += 1;
        summary.pending += info.pending;
    }
    archive.file.write_all(&0u32.to_le_bytes())?;
    archive.file.write_all(&archive.digest.finalize())?;
    archive.file.flush()?;
    archive.file.get_ref().sync_all()?;
    let (year, month, day, hour, minute) = crate::time::civil(crate::time::now());
    let id = uuid::Uuid::new_v4().simple().to_string();
    let output = folder.join(format!("Fei Notes {year:04}-{month:02}-{day:02} {hour:02}{minute:02} {}.feinotesbackup", &id[..8]));
    rename(&temporary, &output, 0x00000004)?; // RENAME_EXCL
    Ok((output, summary))
}

fn read_archive(backup: &Path, stage: &Path) -> Result<()> {
    let file = File::open(backup)?;
    let length = file.metadata()?.len();
    let mut input = BufReader::new(file);
    let mut magic = vec![0; MAGIC.len()];
    input.read_exact(&mut magic)?;
    ensure!(magic == MAGIC, "This file is not a supported Fei Notes backup.");
    let mut manifest = Sha256::new();
    let mut count = 0;
    loop {
        let mut name_size = [0u8; 4];
        input.read_exact(&mut name_size)?;
        let len = u32::from_le_bytes(name_size) as usize;
        if len == 0 { break; }
        ensure!(len <= MAX_PATH && count < MAX_FILES, "Invalid backup file list.");
        let mut name = vec![0u8; len];
        input.read_exact(&mut name)?;
        let name = std::str::from_utf8(&name)?;
        ensure!(valid_path(name), "Invalid path in backup.");
        let mut size = [0u8; 8];
        input.read_exact(&mut size)?;
        let bytes = u64::from_le_bytes(size);
        ensure!(bytes <= length, "Invalid file size in backup.");
        let path = stage.join(name);
        fs::create_dir_all(path.parent().context("Missing restore file directory")?)?;
        let mut output = BufWriter::new(OpenOptions::new().write(true).create_new(true).open(&path)?);
        let actual = copy_hashed(&mut input, &mut output, bytes)?;
        let mut expected = [0; 32];
        input.read_exact(&mut expected)?;
        ensure!(expected == actual, "Backup checksum failed: {name}");
        output.flush()?;
        output.get_ref().sync_all()?;
        manifest.update(name_size);
        manifest.update(name.as_bytes());
        manifest.update(size);
        manifest.update(expected);
        count += 1;
    }
    let mut expected = [0; 32];
    input.read_exact(&mut expected)?;
    ensure!(<[u8; 32]>::from(manifest.finalize()) == expected && input.read(&mut [0])? == 0,
        "The backup file list is damaged.");
    Ok(())
}

fn owner(marker: &[u8]) -> Result<&str> {
    let value = std::str::from_utf8(marker)?.lines().next().context("Missing backup library identity")?;
    ensure!(value == "local-only" || (value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())),
        "Invalid backup library identity.");
    Ok(value)
}

fn rotate_peer(database: &Path) -> Result<()> {
    let db = Connection::open(database)?;
    if table(&db, "state")? {
        let peer = (uuid::Uuid::new_v4().as_u128() as i64 & i64::MAX).max(2);
        db.execute("UPDATE state SET value=?1 WHERE key='peer'", [peer])?;
    }
    Ok(())
}

fn ticket(root: &Path) -> Result<PathBuf> {
    let name = root.file_name().and_then(|name| name.to_str()).context("Missing library name")?;
    Ok(root.with_file_name(format!(".restore-{name}")))
}

pub(crate) fn scheduled(root: &Path) -> bool {
    ticket(root).is_ok_and(|request| request.is_file())
}

#[derive(Debug)]
pub(crate) struct Prepared {
    stage: Stage,
    pub summary: Summary,
}

pub(crate) fn prepare(root: &Path, source: &Path) -> Result<Prepared> {
    let id = uuid::Uuid::new_v4().to_string();
    let stage = Stage::new(root.with_file_name(format!("Library Before Restore {id}")))?;
    read_archive(source, &stage.0)?;
    let marker = fs::read(stage.0.join(READY))?;
    let current = fs::read(root.join(READY))?;
    let namespace = stage.0.join(NAMESPACE_FILE);
    let saved_namespace = match fs::read_to_string(&namespace) {
        Ok(value) => { fs::remove_file(namespace)?; value }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => LEGACY_NAMESPACE.into(),
        Err(error) => return Err(error.into()),
    };
    ensure!(owner(&marker)? == "local-only" || saved_namespace == CLOUD_NAMESPACE,
        "The backup was made in a different CloudKit environment.");
    ensure!(owner(&marker)? == owner(&current)? && std::str::from_utf8(&marker)?.lines().nth(1) != Some("shared"),
        "Open the same library type and Apple Account used to make this backup, then try again.");
    let database = stage.0.join(SQLITE_FILE);
    let mut summary = validate_library(&database, &marker, &stage.0)?;
    let active = open_db(&root.join(SQLITE_FILE), OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    let supported = active.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?;
    let snapshot = open_db(&database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    check_version(&snapshot, supported)?;
    ensure!(channel(&snapshot)? == channel(&active)?,
        "The backup belongs to a different sync environment.");
    drop(snapshot);
    rotate_peer(&database)?;
    for room in shared::rooms(&stage.0)? {
        let root = shared::room_root(&stage.0, &room.scope.note)?;
        let marker = fs::read(root.join(READY))?;
        ensure!(owner(&marker)? == owner(&current)? && std::str::from_utf8(&marker)?.lines().nth(1) == Some("shared"),
            "The backup includes a shared note from a different account.");
        let database = root.join(SQLITE_FILE);
        check_version(&open_db(&database, OpenFlags::SQLITE_OPEN_READ_ONLY)?, supported)?;
        let info = validate_library(&database, &marker, &root)?;
        rotate_peer(&database)?;
        summary.shared += 1;
        summary.pending += info.pending;
    }
    let mut ready = File::create(stage.0.join(TICKET))?;
    ready.write_all(id.as_bytes())?;
    ready.sync_all()?;
    Ok(Prepared { stage, summary })
}

impl Prepared {
    pub(crate) fn schedule(self, root: &Path) -> Result<()> {
        // Publishing the request is atomic. A later restore cannot overwrite it.
        fs::hard_link(self.stage.0.join(TICKET), ticket(root)?).context("A restore may already be scheduled.")?;
        self.stage.keep();
        Ok(())
    }
}

unsafe extern "C" {
    fn renamex_np(from: *const c_char, to: *const c_char, flags: c_uint) -> c_int;
}

fn rename(from: &Path, to: &Path, flags: c_uint) -> Result<()> {
    let source = CString::new(from.as_os_str().as_bytes())?;
    let target = CString::new(to.as_os_str().as_bytes())?;
    if unsafe { renamex_np(source.as_ptr(), target.as_ptr(), flags) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

fn discard_unusable_request(request: &Path) -> Result<Option<File>> {
    fs::remove_file(request).context("Could not clear the failed backup restore request.")?;
    anyhow::bail!("上一次恢复请求已失效。请重新选择备份。");
}

// Called during startup with the current library locked. The staged library
// gets its own lock before the atomic exchange. If startup stops immediately
// after the exchange, the marker in the active library prevents a second swap.
pub(crate) fn install(root: &Path) -> Result<Option<File>> {
    let request = ticket(root)?;
    let id = match fs::read_to_string(&request) {
        Ok(id) => id,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => return discard_unusable_request(&request),
        Err(error) => return Err(error.into()),
    };
    if !uuid::Uuid::parse_str(&id).is_ok_and(|uuid| uuid.to_string() == id) {
        return discard_unusable_request(&request);
    }
    if fs::read_to_string(root.join(TICKET)).is_ok_and(|value| value == id) {
        fs::remove_file(request)?;
        return Ok(None);
    }
    let stage = root.with_file_name(format!("Library Before Restore {id}"));
    let ready = || -> std::io::Result<bool> {
        Ok(stage.symlink_metadata()?.is_dir() && fs::read_to_string(stage.join(TICKET))? == id)
    };
    match ready() {
        Ok(true) => {}
        Ok(false) => return discard_unusable_request(&request),
        Err(error) if matches!(error.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::InvalidData) =>
            return discard_unusable_request(&request),
        Err(error) => return Err(error.into()),
    }
    let lock = crate::cloud_restore::lock_library(&stage)?;
    let mut room_locks = vec![];
    let open = |library: &Path| open_db(&library.join(SQLITE_FILE), OpenFlags::SQLITE_OPEN_READ_WRITE);
    Store::ensure_sqlite_wal(&open(&stage)?)?;
    for library in [root, &stage] {
        for room in shared::rooms(library)? {
            let path = shared::room_root(library, &room.scope.note)?;
            room_locks.push(crate::cloud_restore::lock_library(&path)
                .context("请关闭打开的共享笔记窗口，再重试恢复。")?);
            if library == stage { Store::ensure_sqlite_wal(&open(&path)?)?; }
        }
    }
    const RENAME_SWAP: c_uint = 0x00000002;
    rename(&stage, root, RENAME_SWAP)?;
    if let Err(error) = fs::remove_file(request) {
        crate::logging::write(&format!("Backup restore request cleanup: {error}"));
    }
    Ok(Some(lock))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud_sync::journal::Journal;
    use crate::model::{Block, BlockKind, Document, FileBlock, FileView, RichText};

    fn library(root: &Path, account: char) -> Result<()> {
        let staged = root.join(".restore-source");
        Store::open_at(staged.clone())?;
        fs::write(staged.join(READY), format!("{}\n", account.to_string().repeat(64)))?;
        Store::initialize_sqlite(root)
    }

    #[test]
    fn complete_backup_restores_pending_edits_trash_and_attachment_without_reusing_peer() -> Result<()> {
        let fixture = Stage::new(std::env::temp_dir().join(format!("fei-backup-test-{}", uuid::Uuid::new_v4())))?;
        let source = fixture.0.join("source");
        let target = fixture.0.join("target");
        library(&source, 'a')?;
        library(&target, 'a')?;
        fs::write(target.join("untouched"), b"previous library")?;
        let mut store = Store::open_sqlite_at(source.clone())?;
        let mut journal = Journal::open(&source, None)?;
        let target_peer = Journal::open(&target, None)?.identity().2;
        let folder = store.create_folder("工作", None)?;
        let asset = "assets/files/sample.bin";
        fs::create_dir_all(source.join("assets/files"))?;
        let bytes = vec![0x93; 1024 * 1024 + 13];
        fs::write(source.join(asset), &bytes)?;
        let body = Document { title: RichText::plain("离线编辑"),
            blocks: vec![Block::text("还没上传"), Block::new(BlockKind::File(FileBlock {
                src_kind: "path".into(), src: asset.into(), name: "sample.bin".into(), view: FileView::Card,
            }), RichText::default())], ..Default::default() };
        let note = store.create_from(&body)?;
        store.set_parent(&note, Some(&folder))?;
        let trashed = store.create_from(&Document { title: RichText::plain("已删除"), ..Default::default() })?;
        store.set_parent(&trashed, Some(&folder))?;
        store.delete(&trashed, None)?;
        journal.merge(&Default::default())?;
        let mut edited = store.load_for_edit(&note, false)?;
        edited.blocks[0].text = RichText::plain("快照之后还没上传的改动");
        store.save(&note, &edited)?;
        journal.merge(&Default::default())?;
        let source_db = open_db(&source.join(SQLITE_FILE), OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        assert_eq!(source_db.query_row("SELECT count(*) FROM note_local_updates WHERE note = ?1", [&note], |r| r.get::<_, i64>(0))?, 1);
        let previous_peer = journal.identity().2;
        let outgoing = journal.counts()?.0;
        assert!(outgoing > 0);
        let shared_id = uuid::Uuid::new_v4().to_string();
        let shared_path = source.join("shared-notes").join(&shared_id);
        fs::create_dir_all(&shared_path)?;
        Store::initialize_shared_sqlite(&shared_path, &"a".repeat(64))?;
        let shared_doc = Document { title: RichText::plain("共享的离线笔记"), ..Default::default() };
        let descriptor = shared::Room { scope: shared::Scope { note: shared_id.clone(), owner: None },
            title: shared_doc.title.text.clone() };
        fs::write(shared_path.join(shared::MARKER), serde_json::to_vec(&descriptor)?)?;
        let mut shared = Journal::open(&shared_path, None)?;
        shared.seed_shared(&shared_doc, "")?;
        let shared_pending = shared.counts()?.0;
        let shared_peer = shared.identity().2;
        let records = |path: &Path| -> Result<Vec<(String, Vec<u8>)>> {
            let db = open_db(&path.join(SQLITE_FILE), OpenFlags::SQLITE_OPEN_READ_WRITE)?;
            Ok(db.prepare("SELECT id, data FROM outbox ORDER BY id")?
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<rusqlite::Result<_>>()?)
        };
        let expected = records(&source)?;
        let (file, summary) = create(&source, &fixture.0)?;
        assert_eq!(summary, Summary { notes: 1, folders: 1, trash: 1, shared: 1, pending: (outgoing + shared_pending).into() });

        let broken = fixture.0.join("damaged.feinotesbackup");
        let mut content = fs::read(&file)?;
        let position = content.len() / 2;
        content[position] ^= 1;
        fs::write(&broken, content)?;
        assert!(prepare(&target, &broken).is_err());
        let other = fixture.0.join("other-account");
        library(&other, 'b')?;
        assert!(prepare(&other, &file).is_err());
        assert!(!ticket(&target)?.exists());

        let ready = prepare(&target, &file)?;
        assert_eq!(ready.summary, summary);
        let previous = ready.stage.0.clone();
        ready.schedule(&target)?;
        let lock = crate::cloud_restore::lock_library(&target)?;
        let shared_lock = crate::cloud_restore::lock_library(&previous.join("shared-notes").join(&shared_id))?;
        assert!(install(&target).is_err());
        assert!(scheduled(&target));
        assert!(target.join("untouched").is_file());
        drop(shared_lock);
        let new_lock = install(&target)?.context("Restore did not run")?;
        assert!(!scheduled(&target));
        for library in [target.clone(), target.join("shared-notes").join(&shared_id)] {
            assert_eq!(open_db(&library.join(SQLITE_FILE), OpenFlags::SQLITE_OPEN_READ_WRITE)?
                .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))?, "wal");
        }
        // Restarting after the exchange but before removing the request must
        // not switch back to the old library.
        fs::hard_link(target.join(TICKET), ticket(&target)?)?;
        assert!(install(&target)?.is_none());
        drop((lock, new_lock));
        assert_eq!(fs::read(previous.join("untouched"))?, b"previous library");
        let mut restored = Store::open_sqlite_at(target.clone())?;
        assert_eq!(restored.load(&note)?, store.load(&note)?);
        assert_eq!(restored.list_trash()[0].id, trashed);
        assert_eq!(restored.list().iter().find(|entry| entry.id == note).unwrap().meta.parent.as_deref(), Some(folder.as_str()));
        assert_eq!(fs::read(target.join(asset))?, bytes);
        assert_eq!(records(&target)?, expected);
        let restored = Journal::open(&target, None)?;
        assert_ne!(restored.identity().2, previous_peer);
        assert_ne!(restored.identity().2, target_peer);
        assert_eq!(restored.counts()?.0, outgoing);
        assert_eq!(content::render(&restored.text_state(&note)?.unwrap())?, edited);
        let restored_shared = target.join("shared-notes").join(&shared_id);
        assert_eq!(records(&restored_shared)?, records(&shared_path)?);
        assert_eq!(Store::open_sqlite_at(restored_shared.clone())?.load(&shared_id)?, shared_doc);
        assert_ne!(Journal::open(&restored_shared, None)?.identity().2, shared_peer);
        Ok(())
    }

    #[test]
    fn local_library_backup_can_restore_without_an_icloud_account() -> Result<()> {
        let fixture = Stage::new(std::env::temp_dir().join(format!("fei-local-backup-test-{}", uuid::Uuid::new_v4())))?;
        let source = fixture.0.join("source");
        let target = fixture.0.join("target");
        for path in [&source, &target] {
            fs::create_dir(path)?;
            Store::initialize_local_sqlite(path)?;
        }
        let mut store = Store::open_sqlite_at(source.clone())?;
        let note = Document { title: RichText::plain("本机备份"), blocks: vec![Block::text("待恢复的正文")], ..Default::default() };
        let id = store.create_from(&note)?;
        let old = open_db(&source.join(SQLITE_FILE), OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        old.execute("UPDATE library_documents SET plain = ?1 WHERE id = ?2", rusqlite::params![note.plain_text(), id])?;
        old.execute_batch("DROP TABLE library_search_blocks; PRAGMA user_version = 4;")?;
        drop(old);
        let original = Document { title: RichText::plain("当前本机笔记"), ..Default::default() };
        let original_id = Store::open_sqlite_at(target.clone())?.create_from(&original)?;
        let (file, summary) = create(&source, &fixture.0)?;
        assert_eq!(summary, Summary { notes: 1, ..Default::default() });
        let interrupted = prepare(&target, &file)?;
        let stage = interrupted.stage.0.clone();
        interrupted.schedule(&target)?;
        fs::rename(stage, fixture.0.join("missing-stage"))?;
        let _old_lock = crate::cloud_restore::lock_library(&target)?;
        assert!(install(&target).is_err());
        assert!(!ticket(&target)?.exists());
        assert_eq!(Store::open_sqlite_at(target.clone())?.load(&original_id)?, original);
        fs::write(ticket(&target)?, [0xff])?;
        assert!(install(&target).is_err());
        assert!(!ticket(&target)?.exists());
        prepare(&target, &file)?.schedule(&target)?;
        let _new_lock = install(&target)?.context("Local restore did not run")?;
        assert_eq!(open_db(&target.join(SQLITE_FILE), OpenFlags::SQLITE_OPEN_READ_WRITE)?
            .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))?, "wal");
        assert_eq!(fs::read(target.join(READY))?, b"local-only\n");
        let mut restored = Store::open_sqlite_at(target)?;
        assert_eq!(restored.load(&id)?, store.load(&id)?);
        assert_eq!(&*restored.list()[0].body, "待恢复的正文");
        Ok(())
    }
}
