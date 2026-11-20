// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Read shared archives from the personal Trash without reopening a share.

use super::*;
use crate::store::DocKind;
use rusqlite::OpenFlags;
use sha2::{Digest, Sha256};
use std::fs;

const PREFIX: &str = "shared-recovery:";
const PEER_PREFIX: &str = "offline-recovery-peer:";

pub(in crate::cloud_sync) fn is_shared(id: &str) -> bool { id.starts_with(PREFIX) }

pub(in crate::cloud_sync) fn preserve_peers(db: &Connection) -> Result<()> {
    db.execute("INSERT OR IGNORE INTO library_values(key, value)
        SELECT ?1 || r.note, CAST(s.value AS BLOB) FROM offline_recoveries r, state s
        WHERE s.key = 'peer' AND EXISTS(SELECT 1 FROM state WHERE key = 'shared_scope')", [PEER_PREFIX])?;
    Ok(())
}

struct Candidate {
    key: String,
    note: String,
    sequence: i64,
    title: String,
    markdown: bool,
    kind: DocKind,
}

fn caches(root: &Path) -> impl Iterator<Item = PathBuf> {
    ["shared-notes", "retired-shared-notes"].into_iter().flat_map(|name| {
        let dir = root.join(name);
        if !dir.symlink_metadata().is_ok_and(|meta| meta.is_dir()) { return None; }
        fs::read_dir(dir).ok()
    }).flatten().filter_map(|entry| {
        let entry = entry.ok()?;
        entry.file_type().ok()?.is_dir().then(|| entry.path())
    })
}

pub(super) fn access(db: &Connection) -> Result<(Option<Access>, bool)> {
    let access = get_state(db, "shared_access")?.map(|raw| serde_json::from_slice::<Access>(&raw)).transpose()?;
    let ended = db.query_row("SELECT EXISTS(SELECT 1 FROM library_values WHERE key = 'shared-activity-revoked')", [], |r| r.get(0))?;
    Ok((access, ended))
}

fn can_recover(scope: &shared::Scope, access: Option<Access>, ended: bool) -> bool {
    if scope.folder {
        scope.owner.is_none() || (!ended && matches!(access, Some(Access::ReadOnly | Access::ReadWrite)))
    } else { scope.owner.is_none() && (ended || access == Some(Access::Revoked)) }
}

fn candidates(db: &Connection, source: &Path, account: &str, scope: &shared::Scope) -> Result<Vec<Candidate>> {
    ensure!(shared::account(source)? == account, "Shared recovery belongs to another iCloud account");
    let saved = get_state(db, "shared_scope")?.context("Missing shared scope")?;
    ensure!(&serde_json::from_slice::<shared::Scope>(&saved)? == scope, "Shared recovery scope changed");
    let (access, ended) = access(db)?;
    if !can_recover(scope, access, ended || source.join("shared-note-ended").exists()) { return Ok(vec![]); }
    let rows = db.prepare("SELECT r.note, r.sequence, r.title, r.markdown, coalesce(CAST(origin.value AS INTEGER), s.value)
        FROM offline_recoveries r JOIN sync_history h ON h.note = r.note AND h.sequence = r.sequence
        JOIN state s ON s.key = 'peer' LEFT JOIN library_values origin ON origin.key = ?1 || r.note ORDER BY r.sequence DESC")?
        .query_map([PEER_PREFIX], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?,
            r.get::<_, bool>(3)?, r.get::<_, i64>(4)? as u64)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.is_empty() { return Ok(vec![]); }
    let catalogue = scope.folder.then(|| stored_catalogue(db)).transpose()?;
    let mut result = vec![];
    for (note, sequence, title, markdown, peer) in rows {
        if !uuid::Uuid::parse_str(&note).is_ok_and(|id| id.to_string() == note) || !scope.permits(&note) { continue; }
        let kind = if let Some(doc) = &catalogue {
            if !catalogue::permanently_deleted(doc, &note) { continue; }
            // Older builds also created recovery rows for clocks that only
            // differed on another peer. Prove a local edit before exposing one.
            if let Some(deleted) = catalogue::deleted_version(doc, &note)? {
                let held = super::note(db, &note)?.is_some_and(|state| state.held);
                let local_edit = note_version(db, &note)?.is_some_and(|local| {
                    local.get(&peer).copied().unwrap_or(0) > deleted.get(&peer).copied().unwrap_or(0)
                });
                if !held && !local_edit { continue; }
            }
            content::value(&doc.get_map(format!("entry-{note}")), "kind")
                .map(|kind| serde_json::from_str(&kind)).transpose()?.unwrap_or_default()
        } else {
            if note != scope.note { continue; }
            DocKind::Doc
        };
        let key = format!("{PREFIX}{}:{peer}:{note}:{sequence}", scope.note);
        result.push(Candidate { key, note, sequence, title, markdown, kind });
    }
    Ok(result)
}

fn reader(source: &Path, account: &str) -> Result<Option<(Connection, shared::Scope)>> {
    let Some(room) = shared::descriptor(source)? else { return Ok(None); };
    let name = source.file_name().and_then(|name| name.to_str()).unwrap_or_default();
    let room_id = room.scope.note.as_str();
    if name != room_id && !name.strip_prefix(room_id).and_then(|suffix| suffix.strip_prefix('-'))
        .is_some_and(|suffix| uuid::Uuid::parse_str(suffix).is_ok()) { return Ok(None); }
    if shared::account(source)? != account { return Ok(None); }
    let database = source.join(SQLITE_FILE);
    if !database.symlink_metadata().is_ok_and(|meta| meta.is_file()) { return Ok(None); }
    // A closed WAL library (including a just-restored backup) may need SQLite
    // to recreate its sidecars. Keep the session query-only and never create a database.
    let db = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    db.pragma_update(None, "query_only", true)?;
    let initialized: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name = 'offline_recoveries')", [], |r| r.get(0))?;
    Ok(initialized.then_some((db, room.scope)))
}

fn resolution(db: &Connection, id: &str) -> Result<Option<Option<String>>> {
    Ok(db.query_row("SELECT copy FROM shared_recovery_resolutions WHERE id = ?1", [id], |r| r.get(0)).optional()?)
}

impl Journal {
    pub(in crate::cloud_sync) fn shared_recoveries(&self) -> Result<Vec<(String, String)>> {
        ensure!(self.room.is_none(), "Recovery copies belong to the personal library.");
        let mut result = Vec::new();
        let mut seen = HashSet::new();
        for source in caches(&self.root) {
            let rows = (|| -> Result<_> {
                let Some((db, scope)) = reader(&source, &self.account)? else { return Ok(vec![]); };
                let tx = db.unchecked_transaction()?;
                candidates(&tx, &source, &self.account, &scope)
            })();
            // A room with an unreadable cache must not suppress other rooms.
            for row in rows.unwrap_or_default() {
                if seen.insert(row.key.clone()) && resolution(&self.db, &row.key)?.is_none() {
                    result.push((row.key, row.title));
                }
            }
        }
        Ok(result)
    }

    pub(in crate::cloud_sync) fn resolve_shared_recovery(&mut self, id: &str, keep: bool, verify: impl Fn() -> Result<()>) -> Result<Option<String>> {
        ensure!(self.room.is_none() && is_shared(id), "Recovery copies belong to the personal library.");
        verify()?;
        if let Some(copy) = resolution(&self.db, id)? { return Ok(if keep { copy } else { None }); }
        let scope_id = id.strip_prefix(PREFIX).and_then(|id| id.split(':').next());
        for source in caches(&self.root) {
            if !source.file_name().and_then(|name| name.to_str()).is_some_and(|name|
                scope_id.is_some_and(|id| name == id || name.strip_prefix(id).is_some_and(|rest| rest.starts_with('-')))) { continue; }
            let Some((db, scope)) = reader(&source, &self.account)? else { continue; };
            let archive = db.unchecked_transaction()?;
            let Some(row) = candidates(&archive, &source, &self.account, &scope)?.into_iter().find(|row| row.key == id) else { continue; };
            let verify_source = || -> Result<()> {
                verify()?;
                let (current, verified_scope) = reader(&source, &self.account)?.context("This shared cache has moved.")?;
                let (access, ended) = access(&current)?;
                ensure!(verified_scope == scope && can_recover(&scope, access, ended || source.join("shared-note-ended").exists()),
                    "Shared access has ended.");
                Ok(())
            };
            let tx = self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if let Some(copy) = resolution(&tx, id)? { return Ok(if keep { copy } else { None }); }
            if !keep {
                verify_source()?;
                tx.execute("INSERT INTO shared_recovery_resolutions VALUES (?1, NULL)", [id])?;
                tx.commit()?;
                return Ok(None);
            }
            let (mut doc, front) = archived_body(&archive, &row.note, row.sequence)?;
            for asset in content::local_assets(&doc) {
                let path = assets::path(&source, &asset)?;
                ensure!(path.is_file(), "{}", crate::settings::tr(
                    "这台设备缺少恢复副本所需的附件。离线改动仍保留在本机。",
                    "An attachment needed for this recovery is missing. The offline changes are still on this device."));
                let digest: Option<String> = archive.query_row("SELECT digest FROM sync_assets WHERE path = ?1", [&asset], |r| r.get(0)).optional()?;
                if let Some(digest) = digest {
                    let bytes = crate::store::read_bounded(&path, assets::LIMIT as u64)?;
                    ensure!(format!("{:x}", Sha256::digest(&bytes)) == digest, "{}", crate::settings::tr(
                        "本机的恢复附件已损坏。离线改动仍保留在本机。",
                        "A local recovery attachment is damaged. The offline changes are still on this device."));
                }
            }
            let copy = uuid::Uuid::new_v4().to_string();
            let batch = format!("shared-recovery-{copy}");
            let prefix = format!("assets/{batch}");
            let stage = self.root.join(format!(".{batch}"));
            let published = assets::path(&self.root, &prefix)?;
            let result = (|| {
                Store::stage_recovery_assets(&mut doc, &source, &stage, Path::new(&prefix), &row.note, &copy)?;
                write_recovery_copy(&tx, &copy, &mut doc, &front, Meta { markdown: row.markdown, kind: row.kind, ..Default::default() })?;
                tx.execute("INSERT INTO shared_recovery_resolutions VALUES (?1, ?2)", params![id, copy])?;
                verify_source()?;
                if stage.is_dir() {
                    fs::create_dir_all(self.root.join("assets"))?;
                    fs::rename(&stage, &published).context("Could not publish local recovery attachments")?;
                }
                if let Err(error) = tx.commit() { let _ = fs::remove_dir_all(&published); return Err(error.into()); }
                Ok(Some(copy))
            })();
            if stage.is_dir() { let _ = fs::remove_dir_all(&stage); }
            return result;
        }
        anyhow::bail!("{}", crate::settings::tr("此离线恢复项已不可用。", "This offline recovery is no longer available."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud_sync::{Bytes, shared::Room};
    use crate::model::RichText;

    fn personal(path: PathBuf, account: char) -> Result<HistoryFixture> {
        let fixture = HistoryFixture(path);
        let source = fixture.0.join(".restore-source");
        Store::open_at(source.clone())?;
        fs::write(source.join(".cloud-restore-ready"), format!("{}\n", account.to_string().repeat(64)))?;
        Store::initialize_sqlite(&fixture.0)?;
        Ok(fixture)
    }

    fn cache(personal: &Path, room: &Room) -> Result<PathBuf> {
        let root = shared::room_root(personal, &room.scope.note)?;
        Store::initialize_shared_sqlite(&root, &shared::account(personal)?)?;
        fs::write(root.join(shared::MARKER), serde_json::to_vec(room)?)?;
        Ok(root)
    }

    fn page(access: Access, edits: Vec<Edit>, uploaded: Vec<String>) -> Changes {
        Changes { retention: None, uploaded, edits, token: (access != Access::Revoked).then(|| Bytes(vec![1])),
            more: false, legacy: false, room_access: Some(access), upload_quota_exceeded: false,
            #[cfg(feature = "devtools")] cursor_replayed: false }
    }

    #[test]
    fn shared_folder_offline_edit_recovers_privately_after_permanent_delete() -> Result<()> {
        let owner = HistoryFixture::new()?;
        let guest = personal(owner.0.join("guest"), 'b')?;
        let read_only = personal(owner.0.join("reader"), 'c')?;
        let mut owner_private = Store::open_sqlite_at(owner.0.clone())?;
        let folder = owner_private.create_folder("Shared Team", None)?;
        let nested = owner_private.create_folder("Nested", Some(&folder))?;
        let asset = "assets/files/offline-recovery.txt";
        let doc = crate::docxml::parse(&format!("<title>Team agenda</title><p>Team text</p><figure view-type=\"Card\"><source path=\"{asset}\" name=\"offline-recovery.txt\"/></figure>"))?;
        fs::create_dir_all(owner.0.join("assets/files"))?;
        fs::write(owner.0.join(asset), b"verified fixture asset")?;
        let front = "---\nstatus: draft\n---\n";
        let note = owner_private.create_markdown_in(&doc, front, Some(&nested))?;
        let room = shared::original::prepare_local(&owner.0, &folder, "Shared Team".into(), false)?;
        let owner_root = shared::room_root(&owner.0, &folder)?;
        let mut remote = Journal::open(&owner_root, None)?;
        let mut remote_store = Store::open_sqlite_at(owner_root.clone())?;
        let seed = remote.request()?.edits;
        remote.receive(page(Access::Owner, seed.clone(), seed.iter().map(|e| e.id.clone()).collect()))?;
        remote.merge(&Default::default())?;

        let mut invite = room.clone();
        invite.scope.owner = Some("synthetic-folder-owner".into());
        let root = cache(&guest.0, &invite)?;
        let reader_root = cache(&read_only.0, &invite)?;
        let mut local = Journal::open(&root, None)?;
        let mut reader = Journal::open(&reader_root, None)?;
        for (journal, access) in [(&mut local, Access::ReadWrite), (&mut reader, Access::ReadOnly)] {
            journal.receive(page(access, seed.clone(), vec![]))?;
            journal.merge(&Default::default())?;
        }
        let mut shared_store = SharedStore::from_shared_store(Store::open_sqlite_at(root.clone())?);
        let mut edited = shared_store.load_for_edit(&note, true)?;
        edited.blocks[0].text = RichText::plain("A's change written offline");
        let core = Core::new(edited.clone());
        let mut reader_store = SharedStore::from_shared_store(Store::open_sqlite_at(reader_root.clone())?);
        assert!(reader.save(&mut reader_store, &note, &core).is_err());
        local.save(&mut shared_store, &note, &core)?;
        let materialized = Store::read_sync_body(&local.db, &note, false)?.0;
        let delayed = local.request()?.edits;
        assert!(delayed.iter().any(|edit| edit.note == note));

        remote_store.delete(&note, None)?;
        remote.merge(&Default::default())?;
        remote_store.purge(&note)?;
        remote.merge(&Default::default())?;
        let deletion = remote.request()?.edits;
        assert!(deletion.iter().any(|edit| edit.note == catalogue::SCOPE));
        shared::original::refresh_owner(&owner_root, &room.scope, None)?;
        assert!(owner_private.load(&note).is_err());
        assert!(owner_private.list_trash().is_empty());
        let mut target = Journal::open(&guest.0, None)?;
        assert!(target.shared_recoveries()?.is_empty());
        for _ in 0..2 {
            local.receive(page(Access::ReadWrite, deletion.iter().chain(&delayed).cloned().collect(),
                delayed.iter().map(|edit| edit.id.clone()).collect()))?;
            local.merge(&Default::default())?;
            assert!(catalogue::permanently_deleted(&stored_catalogue(&local.db)?, &note));
            assert!(shared_store.load(&note).is_err());
            assert!(local.request()?.edits.iter().all(|edit| edit.note != note));
        }
        reader.receive(page(Access::ReadOnly, deletion, vec![]))?;
        reader.merge(&Default::default())?;
        assert!(Journal::open(&read_only.0, None)?.shared_recoveries()?.is_empty());
        let versions = local.versions(&note)?;
        assert!(versions.iter().any(|(sequence, _, _)| local.version(&note, *sequence)
            .is_ok_and(|(doc, _)| crate::docxml::serialize(&doc) == crate::docxml::serialize(&materialized))));
        let items = target.shared_recoveries()?;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].1, "Team agenda");
        let key = &items[0].0;

        // Background activity can learn about revocation before the editor's journal.
        activity::remember_access(&local.db, Access::Revoked)?;
        assert!(target.shared_recoveries()?.is_empty());
        assert!(target.resolve_shared_recovery(key, true, || Ok(())).is_err());
        local.receive(page(Access::ReadOnly, vec![], vec![]))?;
        assert_eq!(target.shared_recoveries()?, items);
        drop((local, shared_store, target));
        let mut target = Journal::open(&guest.0, None)?;
        assert_eq!(target.shared_recoveries()?, items);
        fs::write(root.join(".cloud-restore-ready"), format!("{}\nshared\n", "c".repeat(64)))?;
        assert!(target.shared_recoveries()?.is_empty());
        assert!(target.resolve_shared_recovery(key, true, || Ok(())).is_err());
        fs::write(root.join(".cloud-restore-ready"), format!("{}\nshared\n", "b".repeat(64)))?;
        fs::remove_file(root.join(asset))?;
        assert!(target.resolve_shared_recovery(key, true, || Ok(())).is_err());
        fs::write(root.join(asset), b"corrupt asset")?;
        assert!(target.resolve_shared_recovery(key, true, || Ok(())).is_err());
        fs::write(root.join(asset), b"verified fixture asset")?;
        let calls = std::cell::Cell::new(0);
        assert!(target.resolve_shared_recovery(key, true, || {
            calls.set(calls.get() + 1);
            ensure!(calls.get() == 1, "Synthetic account switch before commit");
            Ok(())
        }).is_err());
        assert_eq!(calls.get(), 2);
        assert_eq!(target.shared_recoveries()?, items);
        assert!(fs::read_dir(guest.0.join("assets"))?.next().is_none());

        let backup_root = owner.0.join("backups");
        fs::create_dir(&backup_root)?;
        let (backup, _) = crate::cloud_sync::backup::create(&guest.0, &backup_root)?;
        let restored = personal(owner.0.join("restored"), 'b')?;
        drop(Journal::open(&restored.0, None)?);
        crate::cloud_sync::backup::prepare(&restored.0, &backup)?.schedule(&restored.0)?;
        let old_lock = crate::cloud_restore::lock_library(&restored.0)?;
        let new_lock = crate::cloud_sync::backup::install(&restored.0)?.context("The backup did not install")?;
        drop((old_lock, new_lock));
        let mut backup_target = Journal::open(&restored.0, None)?;
        assert_eq!(backup_target.shared_recoveries()?, items);
        let backup_copy = backup_target.resolve_shared_recovery(key, true, || Ok(()))?.context("The archive did not survive backup")?;
        assert!(Store::open_sqlite_at(restored.0.clone())?.load(&backup_copy)?.plain_text().contains("A's change written offline"));

        let copy = target.resolve_shared_recovery(key, true, || Ok(()))?.context("Missing personal copy")?;
        assert_ne!(copy, note);
        assert!(target.shared_recoveries()?.is_empty());
        let mut private = Store::open_sqlite_at(guest.0.clone())?;
        let saved = private.load(&copy)?;
        assert_eq!(saved.blocks[0].text, edited.blocks[0].text);
        assert_eq!(private.front_matter_of(&copy), front);
        let meta = &private.list()[0].meta;
        assert!(meta.parent.is_none() && meta.share_root.is_none() && meta.markdown);
        let saved_asset = content::local_assets(&saved).into_iter().next().context("Missing rebased asset")?;
        assert_ne!(saved_asset, asset);
        assert_eq!(fs::read(guest.0.join(saved_asset))?, b"verified fixture asset");
        assert!(private.load(&note).is_err());
        target.merge(&Default::default())?;
        assert!(target.request()?.edits.iter().any(|edit| edit.note == copy));
        assert!(target.request()?.edits.iter().all(|edit| edit.note != note));
        drop(target);
        let mut restarted = Journal::open(&guest.0, None)?;
        assert_eq!(restarted.resolve_shared_recovery(key, true, || Ok(()))?, Some(copy));
        assert_eq!(private.list().len(), 1);
        let local = Journal::open(&root, None)?;
        assert_eq!(local.versions(&note)?, versions);
        assert!(local.request()?.edits.is_empty());
        Ok(())
    }

    #[test]
    fn ended_single_document_only_recovers_the_owners_pending_body() -> Result<()> {
        let fixture = HistoryFixture::new()?;
        let guest = personal(fixture.0.join("guest"), 'b')?;
        let room = Room { scope: shared::Scope { note: uuid::Uuid::new_v4().to_string(), owner: None, folder: false },
            title: "Standalone".into() };
        let root = cache(&fixture.0, &room)?;
        let mut owner = Journal::open(&root, None)?;
        let doc = crate::docxml::parse("<title>Standalone</title><p>Seed</p>")?;
        owner.seed_shared(&doc, "")?;
        let seed = owner.request()?.edits;
        owner.receive(page(Access::Owner, seed.clone(), seed.iter().map(|e| e.id.clone()).collect()))?;
        let mut invite = room.clone();
        invite.scope.owner = Some("synthetic-owner".into());
        let guest_root = cache(&guest.0, &invite)?;
        let mut writer = Journal::open(&guest_root, None)?;
        writer.receive(page(Access::ReadWrite, seed, vec![]))?;
        writer.merge(&Default::default())?;
        for (journal, path) in [(&mut owner, &root), (&mut writer, &guest_root)] {
            let mut store = SharedStore::from_shared_store(Store::open_sqlite_at(path.clone())?);
            let mut doc = store.load_for_edit(&room.scope.note, true)?;
            doc.blocks[0].text = RichText::plain("Pending standalone edit");
            journal.save(&mut store, &room.scope.note, &Core::new(doc))?;
            assert!(!journal.request()?.edits.is_empty());
            journal.receive(page(Access::Revoked, vec![], vec![]))?;
            assert!(journal.request()?.edits.is_empty());
            assert!(journal.counts()?.0 > 0);
        }
        assert!(Journal::open(&guest.0, None)?.shared_recoveries()?.is_empty());
        fs::write(root.join("shared-note-ended"), b"ended")?;
        assert!(shared::rooms(&fixture.0)?.is_empty());
        drop(owner);
        let retired = fixture.0.join("retired-shared-notes");
        fs::create_dir(&retired)?;
        fs::rename(&root, retired.join(format!("{}-{}", room.scope.note, uuid::Uuid::new_v4())))?;
        let mut target = Journal::open(&fixture.0, None)?;
        let rows = target.shared_recoveries()?;
        assert_eq!(rows.len(), 1);
        assert_eq!(target.resolve_shared_recovery(&rows[0].0, false, || Ok(()))?, None);
        drop(target);
        let mut target = Journal::open(&fixture.0, None)?;
        assert!(target.shared_recoveries()?.is_empty());
        assert_eq!(target.resolve_shared_recovery(&rows[0].0, true, || Ok(()))?, None);
        Ok(())
    }
}
