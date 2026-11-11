//! SQLite storage for the CloudKit test library. Bodies retain the DocxXML
//! codec, and the search fields are persisted alongside the body in each row.

use super::*;
use crate::model::{Block, BlockId};
use super::blocks::{Payload, assign_seed_ids};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::{cell::OnceCell, path::Path, time::Duration};

pub const SQLITE_FILE: &str = "library.sqlite3";
const SCHEMA_VERSION: u32 = 7;

pub(super) struct Library {
    path: PathBuf,
    reader: OnceCell<Connection>,
}

impl Clone for Library {
    fn clone(&self) -> Self { Self::new(self.path.clone()) }
}

impl Library {
    fn new(path: PathBuf) -> Self { Self { path, reader: OnceCell::new() } }

    pub(super) fn connect(&self) -> Result<Connection> {
        let db = Connection::open(&self.path)?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.pragma_update(None, "synchronous", "FULL")?;
        Ok(db)
    }

    pub(super) fn read(&self) -> Result<&Connection> {
        if let Some(db) = self.reader.get() { return Ok(db); }
        let db = self.connect()?;
        Ok(self.reader.get_or_init(|| db))
    }
}

fn schema(db: &mut Connection) -> Result<()> {
    let version: u32 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    anyhow::ensure!(matches!(version, 0 | 2..=SCHEMA_VERSION), "这个 SQLite 文档库由更新版本的 Quiet 创建");
    Store::ensure_sqlite_wal(db)?;
    if version == SCHEMA_VERSION { return Ok(()); }
    let tx = db.transaction()?;
    if version == 0 { tx.execute_batch("
        CREATE TABLE library_entries (id TEXT PRIMARY KEY, meta TEXT NOT NULL);
        CREATE TABLE library_trash (id TEXT PRIMARY KEY, meta TEXT NOT NULL);
        CREATE TABLE library_documents (
            id TEXT PRIMARY KEY, trashed INTEGER NOT NULL,
            header TEXT NOT NULL, front TEXT NOT NULL,
            title TEXT NOT NULL, icon TEXT, plain TEXT NOT NULL, reminders TEXT NOT NULL, links TEXT NOT NULL,
            revision INTEGER NOT NULL DEFAULT 1
        );
        CREATE TABLE library_blocks (
            document TEXT NOT NULL, block INTEGER NOT NULL, parent INTEGER, position INTEGER NOT NULL,
            payload TEXT NOT NULL, PRIMARY KEY (document, block)
        ) WITHOUT ROWID;
        CREATE TABLE library_values (key TEXT PRIMARY KEY, value BLOB NOT NULL);
    ")?; }
    tx.execute_batch("
        CREATE TABLE IF NOT EXISTS library_search_blocks (
            document TEXT NOT NULL, position INTEGER NOT NULL, fragment TEXT NOT NULL,
            PRIMARY KEY (document, position)
        ) WITHOUT ROWID;
    ")?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}

fn read_map<T: serde::de::DeserializeOwned>(db: &Connection, table: &str) -> Result<HashMap<String, T>> {
    let mut statement = db.prepare(&format!("SELECT id, meta FROM {table}"))?;
    let mut rows = statement.query([])?;
    let mut out = HashMap::new();
    while let Some(row) = rows.next()? {
        out.insert(row.get(0)?, serde_json::from_str::<T>(&row.get::<_, String>(1)?)?);
    }
    Ok(out)
}

fn write_map<T: Serialize>(db: &Connection, table: &str, map: &HashMap<String, T>) -> Result<()> {
    let mut existing = db.prepare(&format!("SELECT id FROM {table}"))?;
    let ids = existing.query_map([], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut delete = db.prepare(&format!("DELETE FROM {table} WHERE id = ?1"))?;
    for id in ids.into_iter().filter(|id| !map.contains_key(id)) { delete.execute([id])?; }
    let mut write = db.prepare(&format!("INSERT INTO {table} VALUES (?1, ?2)
        ON CONFLICT (id) DO UPDATE SET meta = excluded.meta WHERE meta <> excluded.meta"))?;
    for (id, meta) in map { write.execute(params![id, serde_json::to_string(meta)?])?; }
    Ok(())
}

fn document(db: &Connection, id: &str, trashed: bool) -> Result<(Document, i64)> {
    let (header, revision): (String, i64) = db.query_row("SELECT header, revision FROM library_documents WHERE id = ?1 AND trashed = ?2",
        params![id, trashed], |row| Ok((row.get(0)?, row.get(1)?)))?;
    let mut query = db.prepare("SELECT block, parent, payload FROM library_blocks WHERE document = ?1 ORDER BY parent, position")?;
    let mut rows = query.query([id])?;
    let mut children: HashMap<Option<i64>, Vec<Block>> = HashMap::new();
    while let Some(row) = rows.next()? {
        let payload: Payload = serde_json::from_str(&row.get::<_, String>(2)?)?;
        children.entry(row.get(1)?).or_default().push(payload.decode(BlockId(row.get::<_, i64>(0)? as u64))?);
    }
    fn attach(children: &mut HashMap<Option<i64>, Vec<Block>>, parent: Option<i64>) -> Vec<Block> {
        let mut blocks = children.remove(&parent).unwrap_or_default();
        for b in &mut blocks { b.children = attach(children, Some(b.id.0 as i64)); }
        blocks
    }
    let mut doc = docxml::parse(&header)?;
    doc.blocks = attach(&mut children, None);
    anyhow::ensure!(children.is_empty() && !doc.blocks.is_empty(), "Invalid stored block tree");
    Ok((doc, revision))
}

fn write_document(db: &Connection, id: &str, trashed: bool, doc: &Document, front: &str) -> Result<i64> {
    let header = docxml::serialize_header(doc);
    let previous: Option<(String, String, i64)> = db.query_row(
        "SELECT header, front, revision FROM library_documents WHERE id = ?1 AND trashed = ?2",
        params![id, trashed], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
    let existing = db.prepare("SELECT block FROM library_blocks WHERE document = ?1")?
        .query_map([id], |r| r.get::<_, i64>(0))?.collect::<rusqlite::Result<std::collections::HashSet<_>>>()?;
    let mut seen = std::collections::HashSet::new();
    let mut upsert = db.prepare("INSERT INTO library_blocks VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT (document, block) DO UPDATE SET parent = excluded.parent, position = excluded.position, payload = excluded.payload
        WHERE library_blocks.parent IS NOT excluded.parent OR library_blocks.position <> excluded.position
            OR library_blocks.payload <> excluded.payload")?;
    fn write_blocks(stmt: &mut rusqlite::Statement<'_>, id: &str, blocks: &[Block], parent: Option<i64>,
        seen: &mut std::collections::HashSet<i64>) -> Result<bool> {
        let mut changed = false;
        for (pos, block) in blocks.iter().enumerate() {
            anyhow::ensure!(block.id.0 != 0 && seen.insert(block.id.0 as i64), "Duplicate block identifier");
            changed |= stmt.execute(params![id, block.id.0 as i64, parent, pos as i64, serde_json::to_string(&Payload::encode(block))?])? != 0;
            changed |= write_blocks(stmt, id, &block.children, Some(block.id.0 as i64), seen)?;
        }
        Ok(changed)
    }
    let mut changed = write_blocks(&mut upsert, id, &doc.blocks, None, &mut seen)?;
    for block in existing.difference(&seen) {
        db.execute("DELETE FROM library_blocks WHERE document = ?1 AND block = ?2", params![id, block])?;
        changed = true;
    }
    if let Some((old_header, old_front, version)) = &previous
        && !changed && old_header == &header && old_front == front { return Ok(*version); }
    let revision = previous.map_or(1, |(_, _, version)| version + 1);
    write_search_blocks(db, id, doc)?;
    db.execute("INSERT INTO library_documents VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        ON CONFLICT (id) DO UPDATE SET header = excluded.header, front = excluded.front, title = excluded.title,
        icon = excluded.icon, plain = excluded.plain, reminders = excluded.reminders, links = excluded.links, revision = excluded.revision",
        params![id, trashed, header, front, doc.title.text, doc.icon, "",
            serde_json::to_string(&doc.reminders())?, serde_json::to_string(&doc.doc_links())?, revision])?;
    Ok(revision)
}

fn write_search_blocks(db: &Connection, id: &str, doc: &Document) -> Result<()> {
    let mut query = db.prepare("SELECT fragment FROM library_search_blocks WHERE document = ?1 ORDER BY position")?;
    let previous = query.query_map([id], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut fragments = Vec::new();
    doc.for_each_search_fragment(|part| fragments.push(part));
    let mut upsert = db.prepare("INSERT INTO library_search_blocks VALUES (?1, ?2, ?3)
        ON CONFLICT (document, position) DO UPDATE SET fragment = excluded.fragment")?;
    for (position, part) in fragments.iter().enumerate().filter(|(i, part)| previous.get(*i) != Some(*part)) {
        upsert.execute(params![id, position as i64, part])?;
    }
    if previous.len() > fragments.len() {
        db.execute("DELETE FROM library_search_blocks WHERE document = ?1 AND position >= ?2",
            params![id, fragments.len() as i64])?;
    }
    Ok(())
}

fn write_meta(db: &Connection, id: &str, meta: &Meta) -> Result<()> {
    db.execute("INSERT OR REPLACE INTO library_entries VALUES (?1, ?2)", params![id, serde_json::to_string(meta)?])?;
    Ok(())
}

fn move_document(db: &Connection, id: &str, to_trash: bool) -> Result<()> {
    anyhow::ensure!(db.execute("UPDATE library_documents SET trashed = ?2 WHERE id = ?1 AND trashed <> ?2",
        params![id, to_trash])? == 1, "文档不存在：{id}");
    Ok(())
}

fn set_value(db: &Connection, key: &str, value: &[u8]) -> Result<()> {
    db.execute("INSERT OR REPLACE INTO library_values VALUES (?1, ?2)", params![key, value])?;
    Ok(())
}

fn get_value(db: &Connection, key: &str) -> Result<Option<Vec<u8>>> {
    Ok(db.query_row("SELECT value FROM library_values WHERE key = ?1", [key], |row| row.get(0)).optional()?)
}

fn front_matter(db: &Connection, id: &str, trashed: bool) -> Result<String> {
    Ok(db.query_row("SELECT front FROM library_documents WHERE id = ?1 AND trashed = ?2",
        params![id, trashed], |row| row.get(0)).optional()?.unwrap_or_default())
}

// Older Quiet versions wrote absolute asset paths. A staged copy carries
// the bytes under assets/, so rewrite the reference before creating a seed.
fn portable_legacy_assets(doc: &mut Document, root: &Path, prefix: Option<&Path>) -> Result<()> {
    rewrite_import_references(doc, &mut |src| {
        if let Some(path) = portable_asset_path(root, src)? {
            *src = prefix.map_or(path.clone(), |prefix| prefix.join(path.strip_prefix("assets").unwrap()))
                .to_string_lossy().into_owned();
        }
        Ok(())
    }, &HashMap::new())
}

fn portable_asset_path(root: &Path, src: &str) -> Result<Option<PathBuf>> {
    let path = Path::new(src.strip_prefix("file://").unwrap_or(src));
    if !path.is_absolute() && !path.starts_with("assets") { return Ok(None); }
    let relative = if path.is_absolute() {
        path.ancestors().find(|candidate| candidate.file_name().is_some_and(|p| p == "assets"))
            .and_then(|base| path.strip_prefix(base).ok()).map(|suffix| Path::new("assets").join(suffix))
    } else { Some(path.to_path_buf()) };
    let path = relative
        .filter(|relative| relative.components().all(|part| matches!(part, std::path::Component::Normal(_))))
        .filter(|relative| export_path_in(root, &relative.to_string_lossy()).is_some())
        .with_context(|| format!("文档库缺少附件：{src}"))?;
    Ok(Some(path))
}

fn stage_copy_assets(doc: &mut Document, source: &Path, stage: &Path, prefix: &Path,
    ids: &HashMap<String, String>) -> Result<()> {
    rewrite_import_references(doc, &mut |src| {
        if let Some(path) = portable_asset_path(source, src)? {
            let suffix = path.strip_prefix("assets")?;
            let staged = stage.join(suffix);
            if !staged.is_file() {
                let input = export_path_in(source, &path.to_string_lossy()).context("无法读取本机附件")?;
                std::fs::create_dir_all(staged.parent().context("附件目录无效")?)?;
                copy_legacy_item(&input, &staged)?;
            }
            *src = prefix.join(suffix).to_string_lossy().into_owned();
        }
        Ok(())
    }, ids)
}

fn rewrite_import_references(doc: &mut Document, source: &mut impl FnMut(&mut String) -> Result<()>,
    ids: &HashMap<String, String>) -> Result<()> {
    use crate::model::{BlockKind, InlineKind, RichText};
    fn rich(text: &mut RichText, source: &mut impl FnMut(&mut String) -> Result<()>,
        ids: &HashMap<String, String>) -> Result<()> {
        for run in &mut text.runs {
            let Some(inline) = &mut run.marks.inline else { continue; };
            match &mut inline.kind {
                InlineKind::File(file) if file.src_kind == "path" => {
                    source(&mut file.src)?;
                    *inline = docxml::to_inline(InlineKind::File(file.clone()));
                }
                InlineKind::DocLink(link) if ids.contains_key(&link.id) => {
                    link.id = ids[&link.id].clone();
                    *inline = docxml::to_inline(InlineKind::DocLink(link.clone()));
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn visit(blocks: &mut [Block], source: &mut impl FnMut(&mut String) -> Result<()>,
        ids: &HashMap<String, String>) -> Result<()> {
        for block in blocks {
            match &mut block.kind {
                BlockKind::Image { src, .. } => source(src)?,
                BlockKind::File(file) if file.src_kind == "path" => source(&mut file.src)?,
                BlockKind::Table(table) => {
                    for cell in table.rows.iter_mut().flatten() { visit(&mut cell.blocks, source, ids)?; }
                }
                _ => {}
            }
            rich(&mut block.text, source, ids)?;
            visit(&mut block.children, source, ids)?;
        }
        Ok(())
    }
    if let Some(cover) = &mut doc.cover { source(&mut cover.src)?; }
    rich(&mut doc.title, source, ids)?;
    visit(&mut doc.blocks, source, ids)
}

fn legacy_trash_body(snapshot: &Store, id: &str, meta: &Meta) -> Result<(Document, String)> {
    let text = std::fs::read_to_string(snapshot.trash_path(id))?;
    Ok(if meta.markdown {
        let title = meta.name.as_deref().unwrap_or_default();
        (crate::markdown::parse_file(&text, title, None), crate::markdown::front_matter(&text).0.to_owned())
    } else { (docxml::parse(&text)?, String::new()) })
}

fn copy_legacy_item(source: &Path, destination: &Path) -> Result<()> {
    use std::fs;
    let metadata = fs::symlink_metadata(source)?;
    if metadata.is_dir() {
        fs::create_dir(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_legacy_item(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else {
        anyhow::ensure!(metadata.is_file(), "Quiet.md 文档库含有无法导入的文件：{}", source.display());
        let size = fs::copy(source, destination)?;
        let after = fs::metadata(source)?;
        anyhow::ensure!(size == metadata.len() && after.len() == metadata.len() && after.modified()? == metadata.modified()?,
            "Quiet.md 文档库在复制时发生变化，请退出 Quiet.md 后重试");
    }
    Ok(())
}

impl Store {
    pub fn initialize_local_sqlite(root: &Path) -> Result<()> {
        Self::initialize_empty_sqlite(root, b"local-only\n")
    }

    /// An isolated library starts empty and receives only its shared note.
    pub fn initialize_shared_sqlite(root: &Path, account: &str) -> Result<()> {
        Self::initialize_empty_sqlite(root, format!("{account}\nshared\n").as_bytes())
    }

    fn initialize_empty_sqlite(root: &Path, marker: &[u8]) -> Result<()> {
        let _lock = lock_root(root)?;
        let mut db = Library::new(root.join(SQLITE_FILE)).connect()?;
        schema(&mut db)?;
        if let Some(existing) = get_value(&db, "restored")? {
            anyhow::ensure!(existing == marker, "文档库与当前存储模式不匹配");
        } else {
            set_value(&db, "restored", marker)?;
        }
        std::fs::create_dir_all(root.join("assets"))?;
        write_atomic(&root.join(".cloud-restore-ready"), marker)
    }

    pub fn show_shared_note(db: &Connection, id: &str, front: &str) -> Result<bool> {
        if db.query_row("SELECT EXISTS(SELECT 1 FROM library_entries WHERE id = ?1)", [id], |row| row.get::<_, bool>(0))? {
            return Ok(false);
        }
        if !db.query_row("SELECT EXISTS(SELECT 1 FROM library_documents WHERE id = ?1 AND trashed = 0)", [id], |row| row.get::<_, bool>(0))? {
            return Ok(false);
        }
        let now = now();
        write_meta(db, id, &Meta { created: now, modified: now, markdown: !front.is_empty(), ..Default::default() })?;
        Ok(true)
    }

    /// Read a body for the sync journal, including bodies in the trash.
    pub fn read_sync_body(db: &Connection, id: &str, trashed: bool) -> Result<(Document, i64, String)> {
        let (doc, revision) = document(db, id, trashed)?;
        Ok((doc, revision, front_matter(db, id, trashed)?))
    }

    /// Called inside the journal transaction so CRDT state and the index can
    /// never get ahead of the body. A purge retains history in the journal.
    pub fn write_sync_body(db: &Connection, id: &str, trashed: bool, doc: &Document, front: &str) -> Result<i64> {
        db.execute("UPDATE library_documents SET trashed = ?2 WHERE id = ?1 AND trashed <> ?2", params![id, trashed])?;
        write_document(db, id, trashed, doc, front)
    }

    pub fn remove_sync_body(db: &Connection, id: &str) -> Result<()> {
        db.execute("DELETE FROM library_blocks WHERE document = ?1", [id])?;
        db.execute("DELETE FROM library_search_blocks WHERE document = ?1", [id])?;
        db.execute("DELETE FROM library_documents WHERE id = ?1", [id])?;
        Ok(())
    }
}

impl Store {
    pub fn set_sqlite_session_root(root: PathBuf) -> Result<()> {
        anyhow::ensure!(SESSION_ROOT.set(root).is_ok(), "Library root is already set for this process");
        SESSION_SQLITE.set(true).map_err(|_| anyhow::anyhow!("SQLite session is already set"))
    }

    pub fn open_sqlite_at(root: PathBuf) -> Result<Self> {
        let sqlite = Library::new(root.join(SQLITE_FILE));
        let mut db = sqlite.connect()?;
        let version: u32 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
        anyhow::ensure!(matches!(version, 2..=SCHEMA_VERSION), "SQLite 文档库版本不受支持");
        anyhow::ensure!(get_value(&db, "restored")?.is_some(), "SQLite 文档库尚未初始化");
        if version != SCHEMA_VERSION { schema(&mut db)?; }
        else { Self::ensure_sqlite_wal(&db)?; }
        let index = read_map(&db, "library_entries")?;
        let trash = read_map(&db, "library_trash")?;
        let mut store = Self { sqlite: Some(sqlite), file_changes: vec![], root, index, trash, warnings: vec![],
            loaded: Default::default(), copied: Default::default(), parsed: HashMap::new() };
        store.restore_recoveries();
        Ok(store)
    }

    /// VACUUM INTO produces a portable rollback-journal database. An active
    /// library must use WAL even when its schema is already current.
    pub fn ensure_sqlite_wal(db: &Connection) -> Result<()> {
        let mode: String = db.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
        if mode == "wal" { return Ok(()); }
        let mode: String = db.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))?;
        anyhow::ensure!(mode == "wal", "SQLite 文档库无法启用 WAL 日志");
        Ok(())
    }

    /// Read an old library via a staged copy and add it in one transaction.
    /// Imported attachment names are scoped to this import.
    pub fn import_legacy_copy(root: &Path, source: &Path) -> Result<usize> {
        let source = source.canonicalize()?;
        let root = root.canonicalize()?;
        anyhow::ensure!(!root.starts_with(&source) && !source.starts_with(&root), "请选择单独存放的 Quiet.md 文档库");
        anyhow::ensure!(source.join("format.json").is_file() && source.join("index.json").is_file(),
            "所选文件夹不是 Quiet.md 文档库");
        let batch = uuid::Uuid::new_v4().to_string();
        let stage = root.join(format!(".legacy-import-{batch}"));
        std::fs::create_dir(&stage)?;
        let prefix = Path::new("assets").join(format!("legacy-{batch}"));
        let assets = root.join(&prefix);
        let result = (|| {
            for name in ["format.json", "index.json", "index.json.bak", "docs", "trash", "assets"] {
                let path = source.join(name);
                if std::fs::symlink_metadata(&path).is_ok() { copy_legacy_item(&path, &stage.join(name))?; }
            }
            let _: FormatMarker = serde_json::from_slice(&std::fs::read(stage.join("format.json"))?)?;
            let mut snapshot = Self::open_at(stage.clone())?;
            snapshot.index.retain(|_, meta| !meta.local());
            snapshot.save_index()?;
            let entries = snapshot.list();
            anyhow::ensure!(snapshot.warnings.is_empty(), "无法完整读取 Quiet.md 文档库：{}", snapshot.warnings.join("；"));
            let entries: Vec<_> = entries.into_iter().filter(|entry| !entry.meta.local()).collect();
            anyhow::ensure!(snapshot.index.iter().filter(|(_, meta)| !meta.local()).count() == entries.len(),
                "Quiet.md 文档库索引中有找不到的文档");
            let _lock = lock_root(&root)?;
            let mut db = Library::new(root.join(SQLITE_FILE)).connect()?;
            anyhow::ensure!(get_value(&db, "restored")?.is_some(), "文档库尚未初始化");
            let tx = db.transaction()?;
            for id in entries.iter().map(|entry| &entry.id).chain(snapshot.trash.keys()) {
                anyhow::ensure!(uuid::Uuid::parse_str(id).is_ok(), "Quiet.md 文档库中的文档标识无效");
                let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM library_entries WHERE id = ?1
                    UNION ALL SELECT 1 FROM library_trash WHERE id = ?1
                    UNION ALL SELECT 1 FROM library_documents WHERE id = ?1)", [id], |r| r.get(0))?;
                anyhow::ensure!(!exists, "当前文档库已有同一份文档。请检查是否已导入此 Quiet.md 文档库。");
            }
            for entry in &entries {
                if entry.meta.folder.is_none() {
                    let mut doc = snapshot.load(&entry.id)?;
                    portable_legacy_assets(&mut doc, &stage, Some(&prefix))?;
                    write_document(&tx, &entry.id, false, &doc, &snapshot.front_matter_of(&entry.id))?;
                }
                write_meta(&tx, &entry.id, &entry.meta)?;
            }
            for (id, trashed) in &snapshot.trash {
                if trashed.meta.folder.is_none() {
                    let (mut doc, front) = legacy_trash_body(&snapshot, id, &trashed.meta)?;
                    portable_legacy_assets(&mut doc, &stage, Some(&prefix))?;
                    write_document(&tx, id, true, &doc, &front)?;
                }
                tx.execute("INSERT INTO library_trash VALUES (?1, ?2)", params![id, serde_json::to_string(trashed)?])?;
            }
            if stage.join("assets").is_dir() { std::fs::rename(stage.join("assets"), &assets)?; }
            if let Err(error) = tx.commit() {
                let _ = std::fs::remove_dir_all(&assets);
                return Err(error.into());
            }
            Ok(entries.len() + snapshot.trash.len())
        })();
        let _ = std::fs::remove_dir_all(&stage);
        result
    }

    /// Copy from the separate local library without rewriting existing cloud notes.
    /// The ID map is committed with the copies, so retries can safely skip them.
    pub fn copy_local_library(root: &Path, source: &Path) -> Result<usize> {
        use std::fs;
        let source = source.canonicalize()?;
        let root = root.canonicalize()?;
        anyhow::ensure!(!root.starts_with(&source) && !source.starts_with(&root), "请选择独立的本机文档库");
        let input = Library::new(source.join(SQLITE_FILE)).connect()?;
        let snapshot = input.unchecked_transaction()?;
        anyhow::ensure!(get_value(&snapshot, "restored")?.as_deref() == Some(b"local-only\n"), "来源不是本机文档库");
        let live: HashMap<String, Meta> = read_map(&snapshot, "library_entries")?;
        let trash: HashMap<String, Trashed> = read_map(&snapshot, "library_trash")?;

        let _lock = lock_root(&root)?;
        let mut output = Library::new(root.join(SQLITE_FILE)).connect()?;
        let tx = output.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        anyhow::ensure!(get_value(&tx, "restored")?.is_some_and(|marker| marker != b"local-only\n"),
            "目标 iCloud 文档库尚未准备好");
        let mut ids: HashMap<String, String> = get_value(&tx, "local-copy-ids")?
            .map(|value| serde_json::from_slice(&value)).transpose()?.unwrap_or_default();
        let fresh: Vec<_> = live.into_iter().map(|(id, meta)| (id, meta, None))
            .chain(trash.into_iter().map(|(id, Trashed { meta, deleted, root })| (id, meta, Some((deleted, root)))))
            .filter(|(id, meta, _)| !meta.local() && !ids.contains_key(id)).collect();
        if fresh.is_empty() { return Ok(0); }
        let count = fresh.len();
        for (id, _, _) in &fresh {
            anyhow::ensure!(ids.insert(id.clone(), uuid::Uuid::new_v4().to_string()).is_none(), "来源文档有重复标识");
        }

        let batch = format!("local-copy-{}", uuid::Uuid::new_v4());
        let prefix = Path::new("assets").join(&batch);
        let stage = root.join(format!(".{batch}"));
        let assets = root.join(&prefix);
        let result = (|| {
            for (source_id, mut meta, trash) in fresh {
                let id = &ids[&source_id];
                meta.parent = meta.parent.and_then(|parent| ids.get(&parent).cloned());
                if meta.folder.is_none() {
                    let (mut doc, _, front) = Self::read_sync_body(&snapshot, &source_id, trash.is_some())?;
                    stage_copy_assets(&mut doc, &source, &stage, &prefix, &ids)?;
                    write_document(&tx, id, trash.is_some(), &doc, &front)?;
                }
                if let Some((deleted, root)) = trash {
                    let value = Trashed { meta, deleted, root: root.and_then(|root| ids.get(&root).cloned()) };
                    tx.execute("INSERT INTO library_trash VALUES (?1, ?2)", params![id, serde_json::to_string(&value)?])?;
                } else {
                    tx.execute("INSERT INTO library_entries VALUES (?1, ?2)", params![id, serde_json::to_string(&meta)?])?;
                }
            }
            set_value(&tx, "local-copy-ids", &serde_json::to_vec(&ids)?)?;
            if stage.is_dir() { fs::rename(&stage, &assets)?; }
            if let Err(error) = tx.commit() {
                let _ = fs::remove_dir_all(&assets);
                return Err(error.into());
            }
            Ok(count)
        })();
        if stage.is_dir() { let _ = fs::remove_dir_all(&stage); }
        result
    }

    /// Copy a cached note and its local attachments into the separate offline library.
    pub fn copy_note_to_local(root: &Path, source: &Path, id: &str) -> Result<String> {
        use std::fs;
        let source = source.canonicalize()?;
        let root = root.canonicalize()?;
        anyhow::ensure!(!root.starts_with(&source) && !source.starts_with(&root), "请选择独立的本机文档库");
        let input = Library::new(source.join(SQLITE_FILE)).connect()?;
        let snapshot = input.unchecked_transaction()?;
        let meta: String = snapshot.query_row("SELECT meta FROM library_entries WHERE id = ?1", [id], |r| r.get(0))?;
        let mut meta: Meta = serde_json::from_str(&meta)?;
        anyhow::ensure!(meta.folder.is_none() && !meta.local(), "只能复制已保存的笔记");
        meta.parent = None;
        meta.root_order = None;
        meta.order = 0;
        meta.opened = 0;
        meta.pinned = false;
        meta.starred = 0;
        meta.reading = false;
        meta.created = now();
        meta.modified = meta.created;
        let (mut doc, _, front) = Self::read_sync_body(&snapshot, id, false)?;

        let _lock = lock_root(&root)?;
        let mut output = Library::new(root.join(SQLITE_FILE)).connect()?;
        let tx = output.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        anyhow::ensure!(get_value(&tx, "restored")?.as_deref() == Some(b"local-only\n"), "目标不是本机文档库");
        let new = uuid::Uuid::new_v4().to_string();
        let batch = format!("local-copy-{new}");
        let prefix = Path::new("assets").join(&batch);
        let stage = root.join(format!(".{batch}"));
        let assets = root.join(&prefix);
        let result = (|| {
            stage_copy_assets(&mut doc, &source, &stage, &prefix, &HashMap::from([(id.to_owned(), new.clone())]))?;
            write_document(&tx, &new, false, &doc, &front)?;
            write_meta(&tx, &new, &meta)?;
            if stage.is_dir() { fs::rename(&stage, &assets)?; }
            if let Err(error) = tx.commit() {
                let _ = fs::remove_dir_all(&assets);
                return Err(error.into());
            }
            Ok(new)
        })();
        if stage.is_dir() { let _ = fs::remove_dir_all(&stage); }
        result
    }

    /// Import the verified CloudKit snapshot once. Publishing is idempotent
    /// across a crash between moving assets, committing SQLite, and cleanup.
    pub fn initialize_sqlite(root: &Path) -> Result<()> {
        let _lock = lock_root(root)?;
        let mut db = Library::new(root.join(SQLITE_FILE)).connect()?;
        schema(&mut db)?;
        let source = root.join(".restore-source");
        if get_value(&db, "restored")?.is_none() {
            let marker = std::fs::read(source.join(".cloud-restore-ready"))?;
            let mut snapshot = Self::open_at(source.clone())?;
            let entries = snapshot.list();
            let assets = root.join("assets");
            if source.join("assets").exists() {
                anyhow::ensure!(!assets.exists(), "恢复目标已有附件目录");
                std::fs::rename(source.join("assets"), &assets)?;
            }
            std::fs::create_dir_all(assets)?;
            let tx = db.transaction()?;
            for entry in entries.into_iter().filter(|e| !e.meta.local()) {
                if entry.meta.folder.is_none() {
                    let mut doc = snapshot.load(&entry.id)?;
                    portable_legacy_assets(&mut doc, root, None)?;
                    assign_seed_ids(&mut doc);
                    write_document(&tx, &entry.id, false, &doc, &snapshot.front_matter_of(&entry.id))?;
                }
                write_meta(&tx, &entry.id, &entry.meta)?;
            }
            for (id, trashed) in &snapshot.trash {
                if trashed.meta.folder.is_none() {
                    let (mut doc, front) = legacy_trash_body(&snapshot, id, &trashed.meta)?;
                    portable_legacy_assets(&mut doc, root, None)?;
                    assign_seed_ids(&mut doc);
                    write_document(&tx, id, true, &doc, &front)?;
                }
            }
            write_map(&tx, "library_trash", &snapshot.trash)?;
            set_value(&tx, "restored", &marker)?;
            tx.commit()?;
        }
        let marker = get_value(&db, "restored")?.context("Missing restored account")?;
        if !root.join(".cloud-restore-ready").exists() {
            write_atomic(&root.join(".cloud-restore-ready"), &marker)?;
        }
        if source.exists() { std::fs::remove_dir_all(source)?; }
        Ok(())
    }

    pub fn sqlite_session() -> bool { SESSION_SQLITE.get() == Some(&true) }

    pub fn sqlite_setting(key: &str) -> Result<Option<Vec<u8>>> {
        get_value(&Library::new(Self::root_dir().join(SQLITE_FILE)).connect()?, key)
    }

    pub fn write_sqlite_setting(key: &str, value: &[u8]) -> Result<()> {
        set_value(&Library::new(Self::root_dir().join(SQLITE_FILE)).connect()?, key, value)
    }

    pub(super) fn refresh_sqlite(&mut self) -> Result<()> {
        let db = self.sqlite.as_ref().unwrap().read()?;
        self.index = read_map(db, "library_entries")?;
        self.trash = read_map(db, "library_trash")?;
        Ok(())
    }

    pub(super) fn save_sqlite_index(&self) -> Result<()> {
        let mut db = self.sqlite.as_ref().unwrap().connect()?;
        let tx = db.transaction()?;
        write_map(&tx, "library_entries", &self.index)?;
        tx.commit()?;
        Ok(())
    }

    pub(super) fn load_sqlite_doc(&self, id: &str, baseline: bool) -> Result<Document> {
        let db = self.sqlite.as_ref().unwrap().read()?;
        let tx = db.unchecked_transaction()?;
        let (doc, revision) = document(&tx, id, false)?;
        tx.commit()?;
        if baseline { self.loaded.borrow_mut().insert(id.to_string(), revision.to_string()); }
        Ok(doc)
    }

    pub(super) fn sqlite_size(&self, id: &str) -> Result<u64> {
        Ok(self.sqlite.as_ref().unwrap().read()?.query_row(
            "SELECT length(CAST(header AS BLOB)) + (SELECT coalesce(sum(length(CAST(payload AS BLOB))), 0)
                FROM library_blocks WHERE document = ?1) FROM library_documents WHERE id = ?1", [id], |r| r.get::<_, i64>(0))? as u64)
    }

    pub(super) fn accept_sqlite_title(&self, id: &str, title: &str) -> Result<()> {
        let Some(previous) = self.loaded.borrow().get(id).and_then(|r| r.parse::<i64>().ok()) else { return Ok(()) };
        let db = self.sqlite.as_ref().unwrap().read()?;
        let (actual, revision): (String, i64) = db.query_row("SELECT title, revision FROM library_documents WHERE id = ?1 AND trashed = 0",
            [id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        if actual == title && revision == previous + 1 {
            self.loaded.borrow_mut().insert(id.to_string(), revision.to_string());
        }
        Ok(())
    }

    pub(super) fn list_sqlite(&self) -> Result<Vec<Entry>> {
        let db = self.sqlite.as_ref().unwrap().read()?;
        let tx = db.unchecked_transaction()?;
        let mut entries = vec![];
        {
            let mut fragments = tx.prepare("SELECT fragment FROM library_search_blocks WHERE document = ?1 ORDER BY position")?;
            let mut stmt = tx.prepare("SELECT id, title, icon, plain, reminders, links FROM library_documents WHERE trashed = 0 ORDER BY id")?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let id: String = row.get(0)?;
                let Some(meta) = self.index.get(&id) else { continue };
                // Existing notes keep their old cache until their first write.
                let mut body: String = row.get(3)?;
                if body.is_empty() {
                    let mut parts = fragments.query([&id])?;
                    while let Some(part) = parts.next()? { body.push_str(part.get_ref(0)?.as_str()?); }
                }
                entries.push(Entry { id, meta: meta.clone(), title: row.get(1)?, icon: row.get(2)?, body: body.into(),
                    reminders: serde_json::from_str(&row.get::<_, String>(4)?)?, links: serde_json::from_str(&row.get::<_, String>(5)?)? });
            }
        }
        tx.commit()?;
        for (id, meta) in &self.index {
            if let Some(title) = &meta.folder {
                entries.push(Entry { id: id.clone(), meta: meta.clone(), title: title.clone(), icon: None, body: Default::default(), reminders: vec![], links: vec![] });
            } else if meta.local() && let Ok(doc) = self.load(id) {
                entries.push(Entry { id: id.clone(), meta: meta.clone(), title: doc.title.text.clone(), icon: doc.icon.clone(), body: doc.plain_text().into(),
                    reminders: doc.reminders(), links: doc.doc_links() });
            }
        }
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(entries)
    }

    pub(super) fn sqlite_manages(&self, id: &str) -> bool {
        self.sqlite.is_some() && !self.index.get(id).is_some_and(Meta::local)
    }

    pub(super) fn sqlite_front(&self, id: &str) -> Result<String> {
        front_matter(self.sqlite.as_ref().unwrap().read()?, id, false)
    }

    /// Body, index and the caller's sync changes either all commit or all roll back.
    pub fn save_sqlite_with<T>(&mut self, id: &str, doc: &Document, sync: impl FnOnce(&Transaction<'_>, i64) -> Result<T>) -> Result<(Meta, T)> {
        let _lock = self.begin_write()?;
        anyhow::ensure!(self.sqlite_manages(id), "This note is outside the SQLite library");
        self.save_sqlite_locked(id, doc, None, None, sync)
    }

    pub(super) fn save_sqlite_locked<T>(&mut self, id: &str, doc: &Document, meta: Option<Meta>, front: Option<&str>,
        sync: impl FnOnce(&Transaction<'_>, i64) -> Result<T>) -> Result<(Meta, T)> {
        let mut db = self.sqlite.as_ref().unwrap().connect()?;
        let tx = db.transaction()?;
        let revision = tx.query_row("SELECT revision FROM library_documents WHERE id = ?1 AND trashed = 0",
            [id], |row| row.get(0)).optional()?.unwrap_or(0);
        if let Some(previous) = self.loaded.borrow().get(id) {
            anyhow::ensure!(previous.parse::<i64>().ok() == Some(revision), "文档已被另一个实例修改，未覆盖内容");
        }
        let front = match front { Some(value) => value.to_owned(), None => front_matter(&tx, id, false)? };
        let now = now();
        let mut meta = meta.or_else(|| self.index.get(id).cloned()).unwrap_or(Meta { created: now, ..Default::default() });
        if meta.markdown { meta.name = Some(doc.title.text.trim().to_string()); }
        meta.modified = now;
        let written = write_document(&tx, id, false, doc, &front)?;
        write_meta(&tx, id, &meta)?;
        let result = sync(&tx, revision)?;
        tx.commit()?;
        self.index.insert(id.to_string(), meta.clone());
        self.loaded.borrow_mut().insert(id.to_string(), written.to_string());
        Ok((meta, result))
    }

    pub(super) fn delete_sqlite(&mut self, id: &str, root: Option<&str>) -> Result<()> {
        let before = self.file_state(id);
        let meta = self.index.get(id).cloned().context("文档不存在")?;
        let trashed = Trashed { meta, deleted: now(), root: root.map(str::to_string) };
        let mut db = self.sqlite.as_ref().unwrap().connect()?;
        let tx = db.transaction()?;
        if trashed.meta.folder.is_none() { move_document(&tx, id, true)?; }
        tx.execute("INSERT INTO library_trash VALUES (?1, ?2)", params![id, serde_json::to_string(&trashed)?])?;
        tx.execute("DELETE FROM library_entries WHERE id = ?1", [id])?;
        tx.commit()?;
        self.trash.insert(id.to_string(), trashed);
        self.index.remove(id);
        self.loaded.borrow_mut().remove(id);
        self.record_file_change(id, before);
        Ok(())
    }

    pub(super) fn restore_sqlite(&mut self, id: &str) -> Result<bool> {
        let parent = self.trash.get(id).and_then(|t| t.meta.parent.as_deref());
        let moved = parent.is_some_and(|p| !self.index_has(p));
        let mut db = self.sqlite.as_ref().unwrap().connect()?;
        let tx = db.transaction()?;
        let mut restored = vec![];
        for k in self.trash_group(id) {
            let Some(trash) = self.trash.get(&k) else { continue };
            anyhow::ensure!(!self.index.contains_key(&k), "文档库已存在同名文档，未覆盖：{k}");
            let mut meta = trash.meta.clone();
            if k == id && moved { meta.parent = None; }
            if meta.folder.is_none() { move_document(&tx, &k, false)?; }
            write_meta(&tx, &k, &meta)?;
            tx.execute("DELETE FROM library_trash WHERE id = ?1", [&k])?;
            restored.push((k, meta));
        }
        tx.commit()?;
        for (k, meta) in restored {
            self.trash.remove(&k);
            self.index.insert(k.clone(), meta);
            self.record_file_change(&k, None);
        }
        Ok(moved)
    }

    pub(super) fn purge_sqlite(&mut self, ids: Vec<String>) -> Result<()> {
        let mut db = self.sqlite.as_ref().unwrap().connect()?;
        let tx = db.transaction()?;
        for id in &ids {
            if tx.execute("DELETE FROM library_documents WHERE id = ?1 AND trashed = 1", [id])? != 0 {
                tx.execute("DELETE FROM library_blocks WHERE document = ?1", [id])?;
                tx.execute("DELETE FROM library_search_blocks WHERE document = ?1", [id])?;
            }
            tx.execute("DELETE FROM library_trash WHERE id = ?1", [id])?;
        }
        tx.commit()?;
        for id in ids { self.trash.remove(&id); }
        Ok(())
    }

    pub(super) fn list_sqlite_trash(&mut self, can_write: bool) -> Result<Vec<TrashEntry>> {
        let cutoff = now().saturating_sub(TRASH_DAYS * 86_400);
        if can_write {
            let expired: Vec<_> = self.trash.iter().filter(|(_, t)| t.deleted <= cutoff).map(|(id, _)| id.clone()).collect();
            if !expired.is_empty() { self.purge_sqlite(expired)?; }
        }
        let db = self.sqlite.as_ref().unwrap().read()?;
        let mut entries = vec![];
        for (id, trashed) in self.trash.iter().filter(|(_, t)| t.root.as_ref().is_none_or(|r| !self.trash.contains_key(r))) {
            let (title, icon) = if let Some(name) = &trashed.meta.folder { (name.clone(), None) }
            else { db.query_row("SELECT title, icon FROM library_documents WHERE id = ?1 AND trashed = 1", [id], |row| Ok((row.get(0)?, row.get(1)?)))? };
            entries.push(TrashEntry { id: id.clone(), title, icon, deleted: trashed.deleted, parent: trashed.meta.parent.clone() });
        }
        entries.sort_by(|a, b| b.deleted.cmp(&a.deleted).then_with(|| a.id.cmp(&b.id)));
        Ok(entries)
    }

    pub(super) fn load_sqlite_settings(&self) -> Result<Settings> {
        let data = get_value(self.sqlite.as_ref().unwrap().read()?, "settings")?;
        data.map(|bytes| serde_json::from_slice(&bytes).map_err(Into::into)).unwrap_or_else(|| Ok(Settings::default()))
    }

    pub(super) fn save_sqlite_settings(&self, settings: &Settings) -> Result<()> {
        set_value(self.sqlite.as_ref().unwrap().read()?, "settings", &serde_json::to_vec(settings)?)
    }

    pub(super) fn finish_sqlite_history(&self, index: &HashMap<String, Meta>, trash: &HashMap<String, Trashed>,
        moves: &[(String, bool)], titles: &[(String, String)]) -> Result<()> {
        let mut db = self.sqlite.as_ref().unwrap().connect()?;
        let tx = db.transaction()?;
        for (id, to_trash) in moves { move_document(&tx, id, *to_trash)?; }
        for (id, title) in titles {
            let (mut doc, _) = document(&tx, id, false)?;
            doc.title = crate::model::RichText::plain(title);
            write_document(&tx, id, false, &doc, &front_matter(&tx, id, false)?)?;
        }
        write_map(&tx, "library_entries", index)?;
        write_map(&tx, "library_trash", trash)?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Temp(PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
    }

    #[test]
    fn opening_existing_library_restores_wal_without_touching_notes() -> Result<()> {
        let temp = Temp(std::env::temp_dir().join(format!("quiet-wal-upgrade-{}", uuid::Uuid::new_v4())));
        std::fs::create_dir(&temp.0)?;
        Store::initialize_local_sqlite(&temp.0)?;
        let mut store = Store::open_sqlite_at(temp.0.clone())?;
        let id = store.create_from(&Document { title: crate::model::RichText::plain("本机笔记"), ..Default::default() })?;
        let expected = store.load(&id)?;
        drop(store);
        let path = temp.0.join(SQLITE_FILE);
        let mode = |db: &Connection| -> rusqlite::Result<String> {
            db.pragma_query_value(None, "journal_mode", |row| row.get(0))
        };
        for initialize in [false, true] {
            let db = Connection::open(&path)?;
            assert_eq!(db.query_row("PRAGMA journal_mode=DELETE", [], |row| row.get::<_, String>(0))?, "delete");
            drop(db);
            if initialize { Store::initialize_local_sqlite(&temp.0)?; }
            let store = Store::open_sqlite_at(temp.0.clone())?;
            assert_eq!(store.load(&id)?, expected);
            assert_eq!(mode(&Connection::open(&path)?)?, "wal");
        }
        Ok(())
    }

    #[test]
    fn copies_local_library_once_with_new_ids_and_intact_references() -> Result<()> {
        let temp = Temp(std::env::temp_dir().join(format!("quiet-local-copy-{}", uuid::Uuid::new_v4())));
        let source = temp.0.join("local");
        let target = temp.0.join("cloud");
        let stage = target.join(".restore-source");
        std::fs::create_dir_all(&source)?;
        std::fs::create_dir_all(&stage)?;
        std::fs::write(stage.join(".cloud-restore-ready"), "test account\n")?;
        Store::open_at(stage)?;
        Store::initialize_local_sqlite(&source)?;
        Store::initialize_sqlite(&target)?;
        let mut local = Store::open_sqlite_at(source.clone())?;
        let folder = local.create_folder("本机目录", None)?;
        let linked = local.create_from(&docxml::parse("<title>本机被引用</title><p>目标正文</p>")?)?;
        local.set_parent(&linked, Some(&folder))?;
        let note = local.create_markdown(&docxml::parse(&format!(
            "<title>本机笔记</title><p>正文<cite type=\"doc\" doc-id=\"{linked}\" file-type=\"docx\">本机被引用</cite></p><figure><source path=\"assets/files/a.txt\" name=\"a.txt\"/></figure>"))?,
            "---\nstatus: draft\n---\n")?;
        local.set_parent(&note, Some(&folder))?;
        let discarded = local.create_from(&docxml::parse("<title>本机回收站</title><p>删除前正文</p>")?)?;
        local.delete(&discarded, None)?;
        let db = Connection::open(target.join(SQLITE_FILE))?;
        let existing = docxml::parse("<title>云端已有笔记</title><p>不得覆盖</p>")?;
        write_document(&db, &linked, false, &existing, "")?;
        write_meta(&db, &linked, &Meta::default())?;

        assert!(Store::copy_local_library(&target, &source).is_err());
        assert_eq!(db.query_row("SELECT count(*) FROM library_entries", [], |row| row.get::<_, i64>(0))?, 1);
        std::fs::create_dir_all(source.join("assets/files"))?;
        std::fs::write(source.join("assets/files/a.txt"), b"local attachment bytes")?;
        assert_eq!(Store::copy_local_library(&target, &source)?, 4);
        assert_eq!(Store::copy_local_library(&target, &source)?, 0);

        let mut cloud = Store::open_sqlite_at(target.clone())?;
        assert_eq!(cloud.load(&linked)?.plain_text(), existing.plain_text());
        let entries = cloud.list();
        let folder_copy = entries.iter().find(|entry| entry.title == "本机目录").unwrap();
        let link_copy = entries.iter().find(|entry| entry.title == "本机被引用").unwrap();
        let note_copy = entries.iter().find(|entry| entry.title == "本机笔记").unwrap();
        assert_ne!(link_copy.id, linked);
        assert_eq!(note_copy.meta.parent.as_ref(), Some(&folder_copy.id));
        let copied = cloud.load(&note_copy.id)?;
        assert_eq!(copied.doc_links()[0].target, link_copy.id);
        assert_eq!(cloud.front_matter_of(&note_copy.id), "---\nstatus: draft\n---\n");
        assert_eq!(std::fs::read(target.join(&copied.asset_sources()[0]))?, b"local attachment bytes");
        assert_eq!(cloud.list_trash()[0].title, "本机回收站");
        assert_eq!(local.load(&note)?.doc_links()[0].target, linked);
        assert_eq!(local.list_trash()[0].id, discarded);

        let offline_id = Store::copy_note_to_local(&source, &target, &note_copy.id)?;
        assert_ne!(offline_id, note_copy.id);
        let mut offline = Store::open_sqlite_at(source.clone())?;
        let entry = offline.list().into_iter().find(|entry| entry.id == offline_id).unwrap();
        assert!(entry.meta.parent.is_none() && entry.meta.markdown && !entry.meta.reading);
        assert_eq!(offline.front_matter_of(&offline_id), "---\nstatus: draft\n---\n");
        let offline_note = offline.load(&offline_id)?;
        assert_eq!(offline_note.plain_text(), copied.plain_text());
        assert_eq!(std::fs::read(source.join(&offline_note.asset_sources()[0]))?, b"local attachment bytes");
        std::fs::remove_file(target.join(&copied.asset_sources()[0]))?;
        assert!(Store::copy_note_to_local(&source, &target, &note_copy.id).is_err());
        assert_eq!(Store::open_sqlite_at(source)?.list().len(), offline.list().len());
        Ok(())
    }

    #[test]
    fn imports_a_legacy_copy_without_overwriting_or_altering_it() -> Result<()> {
        let temp = Temp(std::env::temp_dir().join(format!("quiet-import-{}", uuid::Uuid::new_v4())));
        let root = temp.0.join("current");
        let stage = root.join(".restore-source");
        std::fs::create_dir_all(&stage)?;
        std::fs::write(stage.join(".cloud-restore-ready"), "test account\n")?;
        Store::initialize_sqlite(&root)?;

        let mut legacy = Store::open_at(temp.0.join("legacy"))?;
        let folder = legacy.create_folder("旧库目录", None)?;
        let id = legacy.create_from(&docxml::parse("<title>正文</title><p>原来的笔记</p><figure><source path=\"/old/Quiet.md/assets/files/test.txt\" name=\"test.txt\"/></figure>")?)?;
        legacy.set_parent(&id, Some(&folder))?;
        let archived = legacy.create_markdown(&docxml::parse("<title>已删除</title><p>旧稿</p>")?, "---\nstatus: draft\n---\n")?;
        legacy.delete(&archived, None)?;
        std::fs::create_dir_all(legacy.root.join("assets/files"))?;
        std::fs::write(legacy.root.join("assets/files/test.txt"), "附件原件")?;
        let original_index = std::fs::read(legacy.root.join("index.json"))?;

        assert_eq!(Store::import_legacy_copy(&root, &legacy.root)?, 3);
        assert!(Store::import_legacy_copy(&root, &legacy.root).is_err());
        assert_eq!(std::fs::read(legacy.root.join("index.json"))?, original_index);
        let mut store = Store::open_sqlite_at(root.clone())?;
        let entries = store.list();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries.iter().find(|entry| entry.id == id).unwrap().meta.parent.as_deref(), Some(folder.as_str()));
        let doc = store.load(&id)?;
        assert!(doc.plain_text().contains("原来的笔记"));
        let sources = doc.asset_sources();
        assert_eq!(std::fs::read_to_string(root.join(&sources[0]))?, "附件原件");
        let db = store.sqlite.as_ref().unwrap().connect()?;
        assert!(Store::read_sync_body(&db, &archived, true)?.2.contains("status: draft"));
        Ok(())
    }

    #[test]
    #[ignore = "requires an independently staged old library"]
    fn verifies_a_real_legacy_copy() -> Result<()> {
        let source = PathBuf::from(std::env::var_os("QUIET_LEGACY_QA_COPY").context("Set QUIET_LEGACY_QA_COPY to a temporary copy")?).canonicalize()?;
        anyhow::ensure!(source.parent() == Some(Path::new("/private/tmp")) &&
            source.file_name().is_some_and(|name| name.to_string_lossy().starts_with("quiet-legacy-copy-")),
            "Use a temporary legacy copy");
        let temp = Temp(std::env::temp_dir().join(format!("quiet-real-import-{}", uuid::Uuid::new_v4())));
        let stage = temp.0.join(".restore-source");
        std::fs::create_dir_all(&stage)?;
        std::fs::write(stage.join(".cloud-restore-ready"), "test account\n")?;
        Store::initialize_sqlite(&temp.0)?;
        let count = Store::import_legacy_copy(&temp.0, &source)?;
        assert!(Store::import_legacy_copy(&temp.0, &source).is_err());
        // The independently staged source can be opened by the old backend
        // after import and still contains the same document bodies.
        let mut legacy = Store::open_at(source.clone())?;
        legacy.index.retain(|_, meta| !meta.local());
        legacy.save_index()?;
        let entries = legacy.list();
        let mut imported = Store::open_sqlite_at(temp.0.clone())?;
        assert_eq!(count, entries.len() + legacy.trash.len());
        assert_eq!(entries.len(), imported.list().len());
        assert_eq!(serde_json::to_value(&legacy.index)?, serde_json::to_value(&imported.index)?);
        assert_eq!(serde_json::to_value(&legacy.trash)?, serde_json::to_value(&imported.trash)?);
        let db = imported.sqlite.as_ref().unwrap().connect()?;
        let mut files = 0;
        for (id, meta, trashed) in entries.iter().map(|entry| (&entry.id, &entry.meta, false))
            .chain(legacy.trash.iter().map(|(id, entry)| (id, &entry.meta, true))) {
            if meta.folder.is_some() { continue; }
            let (mut before, front) = if trashed { legacy_trash_body(&legacy, id, meta)? }
                else { (legacy.load(id)?, legacy.front_matter_of(id)) };
            let (after, _, imported_front) = Store::read_sync_body(&db, id, trashed)?;
            portable_legacy_assets(&mut before, &source, None)?;
            let original = before.asset_sources();
            let copied = after.asset_sources();
            assert_eq!(original.len(), copied.len());
            for (original, copied) in original.iter().zip(&copied) {
                if original.starts_with("assets/") {
                    assert_eq!(std::fs::read(source.join(original))?, std::fs::read(temp.0.join(copied))?);
                    files += 1;
                }
            }
            let mut imported_dirs = std::fs::read_dir(temp.0.join("assets"))?;
            if let Some(directory) = imported_dirs.next() {
                let prefix = Path::new("assets").join(directory?.file_name());
                portable_legacy_assets(&mut before, &source, Some(&prefix))?;
            }
            anyhow::ensure!(docxml::serialize(&before) == docxml::serialize(&after), "Changed document body for {id}");
            assert_eq!(front, imported_front, "Changed front matter for {id}");
        }
        eprintln!("LEGACY_COPY_VERIFIED entries={} trash={} attachment_references={files}", entries.len(), legacy.trash.len());
        Ok(())
    }

    #[test]
    fn imports_once_and_commits_body_search_and_sync_together() -> Result<()> {
        let root = Temp(std::env::temp_dir().join(format!("quiet-sqlite-{}", uuid::Uuid::new_v4())));
        let source = root.0.join(".restore-source");
        let mut legacy = Store::open_at(source.clone())?;
        let folder = legacy.create_folder("双机验收", None)?;
        let note = docxml::parse("<title>初始标题</title><p>海盐橙子</p><p>第二段</p>")?;
        let id = legacy.create_markdown(&note, "---\nstatus: draft\n---\n")?;
        legacy.set_parent(&id, Some(&folder))?;
        legacy.set_starred(&id, true)?;
        std::fs::write(source.join(".cloud-restore-ready"), "test account\n")?;
        std::fs::create_dir_all(source.join("assets/files"))?;
        std::fs::write(source.join("assets/files/test.txt"), "本地附件")?;
        drop(legacy);
        Store::initialize_sqlite(&root.0)?;
        Store::initialize_sqlite(&root.0)?;
        let mut store = Store::open_sqlite_at(root.0.clone())?;
        let initial = store.list().into_iter().find(|e| e.id == id).context("Initial index row")?;
        assert_eq!(initial.meta.parent.as_deref(), Some(folder.as_str()));
        assert!(initial.meta.markdown && initial.meta.starred > 0 && initial.body.contains("海盐橙子"));
        assert!(store.front_matter_of(&id).contains("status: draft"));
        assert_eq!(std::fs::read_to_string(root.0.join("assets/files/test.txt"))?, "本地附件");
        let db = store.sqlite.as_ref().unwrap().connect()?;
        db.execute("CREATE TABLE test_queue (edit TEXT)", [])?;
        db.execute_batch("CREATE TABLE test_block_writes (block INTEGER);
            CREATE TRIGGER test_block_updates AFTER UPDATE ON library_blocks BEGIN INSERT INTO test_block_writes VALUES (new.block); END;
            CREATE TABLE test_search_writes (position INTEGER);
            CREATE TRIGGER test_search_updates AFTER UPDATE ON library_search_blocks BEGIN INSERT INTO test_search_writes VALUES (new.position); END;")?;
        let mut edited = store.load_for_edit(&id, false)?;
        edited.title = crate::model::RichText::plain("新标题");
        edited.blocks[0].text = docxml::parse_inline("<strong>蓝鲸地图</strong>")?;
        let result: Result<(Meta, ())> = store.save_sqlite_with(&id, &edited, |tx, _| {
            tx.execute("INSERT INTO test_queue VALUES ('rolled back')", [])?;
            anyhow::bail!("simulated sync failure")
        });
        assert!(result.is_err());
        assert_eq!(store.load(&id)?.title.text, "初始标题");
        assert!(store.list().iter().any(|e| e.id == id && e.body.contains("海盐橙子")));
        assert_eq!(db.query_row("SELECT count(*) FROM test_queue", [], |r| r.get::<_, u32>(0))?, 0);
        store.save_sqlite_with(&id, &edited, |tx, _| {
            tx.execute("INSERT INTO test_queue VALUES ('committed')", [])?;
            Ok(())
        })?;
        drop(store);
        let mut store = Store::open_sqlite_at(root.0.clone())?;
        assert_eq!(store.load(&id)?, edited);
        assert_eq!(db.query_row("SELECT block FROM test_block_writes", [], |r| r.get::<_, i64>(0))?, edited.blocks[0].id.0 as i64);
        assert_eq!(db.query_row("SELECT count(*) FROM test_block_writes", [], |r| r.get::<_, u32>(0))?, 1);
        let search_positions: Vec<i64> = db.prepare("SELECT position FROM test_search_writes")?
            .query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        assert_eq!(search_positions, [0]);
        assert!(store.list().iter().any(|e| e.id == id && &*e.body == "蓝鲸地图\n第二段"));
        assert_eq!(db.query_row("SELECT count(*) FROM test_queue", [], |r| r.get::<_, u32>(0))?, 1);

        let changes = store.track_file_changes();
        store.delete(&id, None)?;
        assert_eq!(store.list_trash()[0].title, "新标题");
        let changes = changes.take();
        store.tree_writer().apply_file_history(&changes, false)?;
        assert!(store.list().iter().any(|e| e.id == id && e.body.contains("蓝鲸地图")));
        store.tree_writer().apply_file_history(&changes, true)?;
        assert_eq!(store.list_trash().len(), 1);
        store.restore(&id)?;
        assert!(store.list_trash().is_empty());
        assert!(store.front_matter_of(&id).contains("status: draft"));
        for name in [".restore-source", "Library", "docs", "trash", "index.json", "format.json", "settings.json"] {
            assert!(!root.0.join(name).exists(), "Found an unexpected file-based store entry: {name}");
        }
        Ok(())
    }

    #[test]
    fn old_search_cache_survives_upgrade_until_the_note_is_edited() -> Result<()> {
        let root = Temp(std::env::temp_dir().join(format!("quiet-search-upgrade-{}", uuid::Uuid::new_v4())));
        std::fs::create_dir(&root.0)?;
        Store::initialize_local_sqlite(&root.0)?;
        let mut store = Store::open_sqlite_at(root.0.clone())?;
        let note = docxml::parse("<title>索引验收</title><p>首段</p><blockquote><p>引用</p></blockquote>
            <table><tbody><tr><td><p>单元格 <latex>x</latex></p></td></tr></tbody></table><p>末段</p>")?;
        let id = store.create_from(&note)?;
        let expected = "首段\n引用\n单元格 x\n末段";
        assert_eq!(note.plain_text(), expected);
        let db = store.sqlite.as_ref().unwrap().connect()?;
        db.execute("UPDATE library_documents SET plain = ?1 WHERE id = ?2", params![expected, id])?;
        db.execute_batch("DROP TABLE library_search_blocks; PRAGMA user_version = 4;")?;
        drop((store, db));

        let mut store = Store::open_sqlite_at(root.0.clone())?;
        assert_eq!(&*store.list()[0].body, expected);
        let db = store.sqlite.as_ref().unwrap().connect()?;
        assert_eq!(db.query_row("SELECT count(*) FROM library_search_blocks", [], |r| r.get::<_, u32>(0))?, 0);
        let mut edited = store.load_for_edit(&id, false)?;
        edited.blocks.remove(0);
        store.save(&id, &edited)?;
        assert_eq!(&*store.list()[0].body, "引用\n单元格 x\n末段");
        assert_eq!(db.query_row("SELECT plain FROM library_documents WHERE id = ?1", [&id], |r| r.get::<_, String>(0))?, "");
        store.delete(&id, None)?;
        store.purge(&id)?;
        assert_eq!(db.query_row("SELECT count(*) FROM library_search_blocks", [], |r| r.get::<_, u32>(0))?, 0);
        Ok(())
    }
}
