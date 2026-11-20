// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Review parked versions without changing the original graph or queues.

use super::*;
use sha2::{Digest, Sha256};

pub(crate) struct ConflictVersion {
    pub document: Document,
    pub stamp: [u8; 32],
    pub copyable: bool,
    front: String,
}

pub(crate) struct ConflictVersions {
    pub local: ConflictVersion,
    pub received: Option<ConflictVersion>,
}

impl Journal {
    pub(in crate::cloud_sync) fn document_conflict(&self, id: &str) -> bool {
        self.access != Some(Access::Revoked) && has_conflict(&self.db, id).unwrap_or(false)
    }

    pub(in crate::cloud_sync) fn can_review_conflict(&self) -> bool {
        self.room.is_none() || self.access.is_some_and(|access| access != Access::Revoked)
            && !self.root.join("shared-note-ended").exists()
            && super::recovery::access(&self.db).is_ok_and(|(_, revoked)| !revoked)
    }

    pub(in crate::cloud_sync) fn conflict_versions(&self, id: &str) -> Result<ConflictVersions> {
        let tx = self.db.unchecked_transaction()?;
        read_versions(&tx, &self.root, id, self.room.as_ref())
    }

    pub(in crate::cloud_sync) fn copy_conflict_versions(&mut self, id: &str, selected: &[(bool, [u8; 32])],
        verify_account: impl Fn() -> Result<()>) -> Result<Vec<String>> {
        ensure!(self.room.is_none(), "Recovery copies belong to the personal library");
        let tx = self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        verify_account()?;
        let versions = read_versions(&tx, &self.root, id, None)?;
        let mut copies = Vec::new();
        for &(received, stamp) in selected {
            let version = if received { versions.received.as_ref() } else { Some(&versions.local) };
            let version = version.filter(|v| v.stamp == stamp && v.copyable).context("Review the latest versions before saving a copy")?;
            let key = format!("conflict-copy:{id}:{received}:{:x}", Sha256::digest(stamp));
            if let Some(copy) = get_state(&tx, &key)? {
                let copy = String::from_utf8(copy)?;
                if tx.query_row("SELECT EXISTS(SELECT 1 FROM library_documents WHERE id = ?1 AND trashed = 0)",
                    [&copy], |row| row.get::<_, bool>(0))? {
                    copies.push(copy);
                    continue;
                }
            }
            let copy = uuid::Uuid::new_v4().to_string();
            let mut doc = version.document.clone();
            let title = crate::settings::display_title(&doc.title.text);
            let label = if received { crate::localized_format!("{} · iCloud 版本", "{} · iCloud version", title) }
                else { crate::localized_format!("{} · 本机版本", "{} · This device's version", title) };
            doc.title = crate::model::RichText::plain(label);
            verify_assets(&tx, &self.root, &doc)?;
            let meta: String = tx.query_row("SELECT meta FROM library_entries WHERE id = ?1", [id], |row| row.get(0))?;
            let meta: Meta = serde_json::from_str(&meta)?;
            write_recovery_copy(&tx, &copy, &mut doc, &version.front, Meta { markdown: meta.markdown, kind: meta.kind, ..Default::default() })?;
            put_state(&tx, &key, copy.as_bytes())?;
            copies.push(copy);
        }
        verify_account()?;
        tx.commit()?;
        Ok(copies)
    }

    pub(in crate::cloud_sync) fn retry_document(&mut self, id: &str) -> Result<()> {
        if self.document_conflict(id) {
            self.db.execute("UPDATE sync_document_conflicts SET retry = 1 WHERE note = ?1", [id])?;
        }
        Ok(())
    }
}

fn read_versions(db: &Connection, root: &Path, id: &str, room: Option<&shared::Scope>) -> Result<ConflictVersions> {
    ensure!(has_conflict(db, id)?, "The document no longer needs recovery");
    if let Some(room) = room {
        let (access, revoked) = super::recovery::access(db)?;
        ensure!(room.permits(id) && access.is_some_and(|access| access != Access::Revoked)
            && !revoked && !root.join("shared-note-ended").exists(), "Shared access has ended");
    }
    let (local, _, front) = Store::read_sync_body(db, id, false)?;
    let version = |document: Document, front: String| -> ConflictVersion {
        let serialized = crate::docxml::serialize(&document);
        let mut hash = Sha256::new();
        hash.update((serialized.len() as u64).to_be_bytes());
        hash.update(serialized.as_bytes());
        hash.update(front.as_bytes());
        let copyable = room.is_none() && content::local_assets(&document).iter().all(|src| assets::source_file(root, src).is_ok());
        ConflictVersion { document, front, stamp: hash.finalize().into(), copyable }
    };
    let local = version(local, front);
    let received = received_branch(db, id).ok().flatten()
        .filter(|(doc, _)| content::local_assets(doc).iter().all(|src| assets::path(root, src).is_ok()))
        .map(|(doc, front)| version(doc, front));
    Ok(ConflictVersions { local, received })
}

fn received_branch(db: &Connection, id: &str) -> Result<Option<(Document, String)>> {
    let records: Vec<Vec<u8>> = db.prepare("SELECT data FROM inbox WHERE note = ?1 ORDER BY rowid")?
        .query_map([id], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
    if records.is_empty() { return Ok(None); }
    let mut received = VersionVector::default();
    let mut updates = Vec::with_capacity(records.len());
    for mut data in records {
        let (_, payload) = retention::unpack(&data)?;
        received.merge(&LoroDoc::decode_import_blob_meta(payload, false)?.partial_end_vv);
        let prefix = data.len() - payload.len();
        data.drain(..prefix);
        updates.push(data);
    }
    let old = note(db, id)?;
    let loro = open_note(db, id, old.as_ref().map_or(&[], |note| note.snapshot.as_slice()))?;
    let (joined, status) = with_replay_candidate(loro, |doc| doc.import_batch(&updates))?;
    if status.pending.is_some() { return Ok(None); }
    // The inbox alone may be a delta. Its exact causal dependencies include
    // the common history, while unrelated local edits are excluded.
    let tips = joined.minimize_frontiers(&joined.vv_to_frontiers(&received))
        .map_err(|_| anyhow::anyhow!("Missing iCloud history"))?;
    let cloud = joined.fork_at(&tips)?;
    Ok(Some((content::render(&cloud)?, front(&cloud))))
}

fn verify_assets(db: &Connection, root: &Path, doc: &Document) -> Result<()> {
    for source in content::local_assets(doc) {
        let path = assets::path(root, &source)?;
        ensure!(path.is_file(), "An attachment for the selected version is missing");
        let expected: Option<String> = db.query_row("SELECT digest FROM sync_assets WHERE path = ?1", [&source], |r| r.get(0)).optional()?;
        if let Some(expected) = expected {
            let bytes = crate::store::read_bounded(&path, assets::LIMIT as u64)?;
            ensure!(format!("{:x}", Sha256::digest(bytes)) == expected, "An attachment for the selected version is damaged");
        }
    }
    Ok(())
}
