// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Attachments are immutable, verified byte streams. Only their references
//! participate in the note CRDT. CloudKit transports bounded chunks.

use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::{self, File, OpenOptions}, io::{Read, Write}, path::{Component, Path, PathBuf}};

pub const SCOPE: &str = "00000000-0000-0000-0000-000000000002";
const CHUNK: usize = 1024 * 1024;
pub(super) const LIMIT: usize = 64 * CHUNK;

#[derive(Serialize, Deserialize)]
struct Chunk {
    path: String,
    digest: String,
    size: usize,
    part: usize,
}

impl Chunk {
    fn count(&self) -> usize { self.size.max(1).div_ceil(CHUNK) }

    fn validate(&self, data: &[u8]) -> Result<()> {
        ensure!(self.size <= LIMIT && self.part < self.count() &&
            data.len() == self.size.saturating_sub(self.part * CHUNK).min(CHUNK), "Invalid CloudKit attachment chunk");
        ensure!(self.digest.len() == 64 && self.digest.bytes().all(|b| b.is_ascii_hexdigit()), "Invalid attachment checksum");
        Ok(())
    }
}

pub fn path(root: &Path, source: &str) -> Result<PathBuf> {
    let mut parts = Path::new(source).components();
    ensure!(parts.next() == Some(Component::Normal("assets".as_ref())) &&
        parts.clone().next().is_some() && source.len() <= 2048 && !source.contains(':'),
        "Attachment must be copied into the document library");
    let mut path = root.join("assets");
    for part in parts {
        ensure!(matches!(part, Component::Normal(_)), "Invalid attachment path");
        ensure!(!fs::symlink_metadata(&path).is_ok_and(|m| m.is_symlink()), "Invalid attachment symlink");
        path.push(part);
    }
    ensure!(!fs::symlink_metadata(&path).is_ok_and(|m| m.is_symlink()), "Invalid attachment symlink");
    Ok(path)
}

pub fn present(root: &Path, source: &str) -> bool {
    path(root, source).is_ok_and(|path| path.is_file())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AssetProblem { External, Missing, Oversized, Unreadable }

impl AssetProblem {
    pub(super) fn code(self) -> &'static str {
        match self { Self::External => "external", Self::Missing => "missing", Self::Oversized => "oversized", Self::Unreadable => "unreadable" }
    }

    pub(super) fn from_code(code: &str) -> Option<Self> {
        match code {
            "external" => Some(Self::External), "missing" => Some(Self::Missing), "oversized" => Some(Self::Oversized),
            "unreadable" => Some(Self::Unreadable), _ => None,
        }
    }
}

pub(super) fn source_file(root: &Path, source: &str) -> std::result::Result<PathBuf, AssetProblem> {
    let path = path(root, source).map_err(|_| AssetProblem::External)?;
    let metadata = fs::metadata(&path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => AssetProblem::Missing,
        _ => AssetProblem::Unreadable,
    })?;
    if !metadata.is_file() { return Err(AssetProblem::Unreadable); }
    if metadata.len() > LIMIT as u64 { return Err(AssetProblem::Oversized); }
    Ok(path)
}

/// A missing, unreadable or oversized source stays local. Database errors
/// still abort the transaction so a partially enqueued asset is retried.
pub fn queue(db: &Connection, root: &Path, source: &str) -> Result<Option<AssetProblem>> {
    let sent: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sync_assets WHERE path = ?1)", [source], |row| row.get(0))?;
    if sent { return Ok(None); }
    let path = match source_file(root, source) { Ok(path) => path, Err(problem) => return Ok(Some(problem)) };
    let Ok(bytes) = crate::store::read_bounded(&path, LIMIT as u64) else { return Ok(Some(AssetProblem::Unreadable)); };
    let mut header = Chunk { path: source.into(), digest: format!("{:x}", Sha256::digest(&bytes)), size: bytes.len(), part: 0 };
    for part in 0..header.count() {
        header.part = part;
        let mut record = serde_json::to_vec(&header)?;
        record.push(b'\n');
        let offset = part * CHUNK;
        record.extend_from_slice(&bytes[offset..(offset + CHUNK).min(bytes.len())]);
        super::journal::enqueue(db, SCOPE, record)?;
    }
    db.execute("INSERT INTO sync_assets VALUES (?1, ?2)", params![source, header.digest])?;
    Ok(None)
}

pub fn ingest(db: &Connection, root: &Path) -> Result<()> {
    let mut stmt = db.prepare("SELECT id, data FROM inbox WHERE note = ?1 ORDER BY rowid")?;
    let mut rows = stmt.query([SCOPE])?;
    let mut ids = Vec::new();
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let bytes: Vec<u8> = row.get(1)?;
        let split = bytes.iter().take(4096).position(|b| *b == b'\n').context("Invalid attachment record")?;
        let header: Chunk = serde_json::from_slice(&bytes[..split])?;
        let data = &bytes[split + 1..];
        header.validate(data)?;
        let target = path(root, &header.path)?;
        let cached: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sync_assets WHERE path = ?1 AND digest = ?2)",
            params![header.path, header.digest], |row| row.get(0))?;
        if !cached || !target.is_file() {
            db.execute("INSERT OR IGNORE INTO sync_asset_chunks VALUES (?1, ?2, ?3, ?4, ?5)",
                params![header.path, header.digest, header.part as i64, header.size as i64, data])?;
        }
        ids.push(id);
    }
    drop(rows);
    drop(stmt);
    for id in ids { db.execute("DELETE FROM inbox WHERE id = ?1", [&id])?; }
    let completed = db.prepare("SELECT path, digest FROM sync_asset_chunks GROUP BY path, digest
        HAVING COUNT(*) = MAX((MAX(size, 1) + ?1 - 1) / ?1)")?
        .query_map([CHUNK as i64], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (source, digest) in completed {
        let target = path(root, &source)?;
        let parent = target.parent().context("Missing attachment parent")?;
        fs::create_dir_all(parent)?;
        let staged = parent.join(format!(".quiet-{}.part", uuid::Uuid::new_v4()));
        let result = materialize(db, &source, &digest, &staged, &target);
        let _ = fs::remove_file(staged);
        result?;
        db.execute("DELETE FROM sync_asset_chunks WHERE path = ?1 AND digest = ?2", params![source, digest])?;
        db.execute("INSERT OR REPLACE INTO sync_assets VALUES (?1, ?2)", params![source, digest])?;
        db.execute("DELETE FROM sync_asset_repairs WHERE path = ?1", [&source])?;
    }
    Ok(())
}

fn materialize(db: &Connection, source: &str, digest: &str, staged: &Path, target: &Path) -> Result<()> {
    let mut stmt = db.prepare("SELECT data FROM sync_asset_chunks WHERE path = ?1 AND digest = ?2 ORDER BY part")?;
    let mut rows = stmt.query(params![source, digest])?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(staged)?;
    let mut hash = Sha256::new();
    while let Some(row) = rows.next()? {
        let data = row.get_ref(0)?.as_blob()?;
        file.write_all(data)?;
        hash.update(data);
    }
    ensure!(format!("{:x}", hash.finalize()) == digest, "CloudKit attachment checksum mismatch");
    file.sync_all()?;
    if let Err(error) = fs::hard_link(staged, target) {
        if error.kind() != std::io::ErrorKind::AlreadyExists { return Err(error.into()); }
        let mut hash = Sha256::new();
        let mut existing = File::open(target)?.take(LIMIT as u64 + 1);
        std::io::copy(&mut existing, &mut hash)?;
        ensure!(format!("{:x}", hash.finalize()) == digest, "A local attachment already uses this path");
    }
    Ok(())
}
