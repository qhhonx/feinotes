// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
use super::*;
use crate::{editor::core::Selection, model::{BlockKind, RichText}};
use crate::cloud_sync::Bytes;

fn page(edits: Vec<Edit>, uploaded: Vec<String>, token: u8) -> Changes {
    Changes { retention: Some(retention::Receipt { ready: true, ..Default::default() }), edits, uploaded,
        token: Some(Bytes(vec![token])), more: false, legacy: false, room_access: None, upload_quota_exceeded: false,
        #[cfg(feature = "devtools")] cursor_replayed: false }
}

fn wire(note: &str, bytes: Vec<u8>) -> Edit {
    Edit { id: uuid::Uuid::new_v4().to_string(), note: note.into(), data: Bytes(bytes), collaborator: false }
}

fn queued(db: &Connection, note: &str) -> Result<Vec<(String, Vec<u8>)>> {
    Ok(db.prepare("SELECT id, data FROM outbox WHERE note = ?1 ORDER BY rowid")?
        .query_map([note], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<rusqlite::Result<_>>()?)
}

#[test]
fn conflicting_document_preserves_queues_and_does_not_stop_other_documents() -> Result<()> {
    let fixture = HistoryFixture::new()?;
    let mut journal = Journal::open(&fixture.0, None)?;
    let mut store = Store::open_sqlite_at(fixture.0.clone())?;
    let seed = crate::docxml::parse("<title>Two replicas</title><p>Shared base</p>")?;
    let id = store.create_from(&seed)?;
    let healthy = store.create_from(&seed)?;
    journal.merge(&Default::default())?;
    let uploaded = journal.request()?.edits.into_iter().map(|edit| edit.id).collect();
    journal.receive(page(vec![], uploaded, 1))?;

    let mut local = store.load_for_edit(&id, false)?;
    let file = crate::docxml::parse("<p/><source path=\"https://example.invalid/local.zip\" name=\"local.zip\"/>")?.blocks.remove(1);
    let mut conflicting = crate::docxml::parse("<p/><img src=\"https://example.invalid/remote.png\"/>")?.blocks.remove(1);
    conflicting.id = file.id;
    // Two independent tree nodes use one logical block identifier, with
    // different attachment shapes and different parents.
    let remote = journal.text_state(&id)?.context("Missing shared base")?;
    let remote_peer = journal.peer + 1;
    let mut remote_body = content::render(&remote)?;
    remote_body.blocks[0].children.push(conflicting);
    let remote_insert = wire(&id, content::edit(&remote, remote_peer, &remote_body)?.context("Missing remote insert")?);
    local.blocks.push(file);
    store.save(&id, &local)?;
    journal.merge(&Default::default())?;
    let local_queue = queued(&journal.db, &id)?;
    assert!(!local_queue.is_empty());
    let local_version = journal.text_state(&id)?.unwrap().oplog_vv();
    let mut local_uncaptured = local.clone();
    local_uncaptured.title = RichText::plain("Uncaptured edit");
    store.save(&id, &local_uncaptured)?;

    let other_remote = journal.text_state(&healthy)?.unwrap();
    let mut other_body = content::render(&other_remote)?;
    other_body.blocks[0].text = RichText::plain("Healthy download");
    let healthy_update = wire(&healthy, content::edit(&other_remote, remote_peer, &other_body)?.unwrap());
    let new_local = store.create_from(&crate::docxml::parse("<title>Healthy upload</title><p>New note</p>")?)?;
    journal.receive(page(vec![remote_insert.clone(), healthy_update], vec![], 2))?;
    let (_, changed) = journal.merge(&Default::default())?;
    assert_eq!(changed.iter().map(|change| change.id.as_str()).collect::<Vec<_>>(), [healthy.as_str()]);
    assert_eq!(store.load_for_edit(&healthy, false)?, other_body);
    assert_eq!(store.load_for_edit(&id, false)?, local_uncaptured);
    assert_eq!(journal.text_state(&id)?.unwrap().oplog_vv(), local_version);
    assert_eq!(queued(&journal.db, &id)?, local_queue);
    assert_eq!(journal.db.query_row("SELECT data FROM inbox WHERE id = ?1", [&remote_insert.id],
        |row| row.get::<_, Vec<u8>>(0))?, remote_insert.data.0);
    let issues = journal.issues()?;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].id, id);
    assert!(issues[0].conflict);

    // A repeated page may carry valid receipts while the same malformed
    // projection stays in the inbox. The retry bit is only set by new data.
    let outgoing = journal.request()?.edits;
    assert!(outgoing.iter().any(|edit| edit.note == new_local));
    assert!(outgoing.iter().all(|edit| edit.note != id));
    let uploaded = outgoing.into_iter().map(|edit| edit.id).collect();
    assert!(journal.receive_page(page(vec![remote_insert.clone()], uploaded, 3))?.1);
    assert_eq!(journal.pending_work()?, (0, 0, 1));
    drop(journal);

    let mut journal = Journal::open(&fixture.0, None)?;
    assert_eq!(journal.request()?.token.unwrap().0, [3]);
    assert!(journal.merge(&Default::default())?.1.is_empty());
    assert!(journal.request()?.maintenance.is_none());
    journal.retry_conflicts()?;
    assert!(journal.merge(&Default::default())?.1.is_empty());
    assert_eq!(journal.pending_work()?, (0, 0, 1));
    assert_eq!(queued(&journal.db, &id)?, local_queue);

    // The sender independently changes its block's identity. Its first
    // version is still recoverable because the two branches are renderable
    // separately. This is a fixture repair, never a local rewrite on receipt.
    remote_body.blocks[0].children[0] = remote_body.blocks[0].children[0].fresh_ids();
    let resolved = wire(&id, content::edit(&remote, remote_peer, &remote_body)?.unwrap());
    journal.receive(page(vec![resolved], vec![], 4))?;
    journal.db.execute_batch("CREATE TRIGGER reject_test_projection BEFORE INSERT ON notes
        BEGIN SELECT RAISE(ABORT, 'injected transient projection failure'); END;")?;
    assert!(journal.merge(&Default::default()).is_err());
    assert_eq!(journal.counts()?.1, 2);
    assert_eq!(journal.request_with_uploads(false)?.token.unwrap().0, [4]);
    assert_eq!(store.load_for_edit(&id, false)?, local_uncaptured);
    assert_eq!(queued(&journal.db, &id)?, local_queue);
    journal.db.execute_batch("DROP TRIGGER reject_test_projection;")?;

    let (_, changes) = journal.merge(&Default::default())?;
    assert_eq!(changes.len(), 1);
    assert_eq!(journal.pending_work()?.2, 0);
    assert_eq!(journal.counts()?.1, 0);
    let merged = store.load_for_edit(&id, false)?;
    assert_eq!(merged.title, local_uncaptured.title);
    assert!(matches!(merged.blocks[0].children[0].kind, BlockKind::Image { .. }));
    assert!(matches!(merged.blocks[1].kind, BlockKind::File(_)));
    assert!(journal.request()?.edits.iter().any(|edit| edit.note == id));
    Ok(())
}

fn add_shape_conflict(local: &LoroDoc, remote: &LoroDoc, original: Document) -> Result<crate::model::BlockId> {
    let mut a = original.clone();
    let mut b = original;
    let mut alias = crate::docxml::parse("<h2>Heading branch</h2>")?.blocks.remove(0);
    a.blocks.push(alias.clone());
    alias.kind = BlockKind::Bullet { dash: false };
    b.blocks.push(alias);
    content::edit(local, 42, &a)?;
    local.import(&content::edit(remote, 43, &b)?.unwrap())?;
    Ok(a.blocks[1].id)
}

#[test]
fn unrenderable_queued_branch_does_not_stop_upload_request_or_local_save() -> Result<()> {
    let fixture = HistoryFixture::new()?;
    let mut journal = Journal::open(&fixture.0, None)?;
    let mut store = Store::open_sqlite_at(fixture.0.clone())?;
    let original = crate::docxml::parse("<title>Old local branch</title><p>Initial</p>")?;
    let id = store.create_from(&original)?;
    journal.merge(&Default::default())?;
    let sent = journal.request()?.edits.into_iter().map(|edit| edit.id).collect();
    journal.receive(page(vec![], sent, 1))?;

    let local_body = store.load_for_edit(&id, false)?;
    let old = journal.text_state(&id)?.unwrap();
    let before = old.oplog_vv();
    let branch = content::open(&old.export(ExportMode::Snapshot)?)?;
    add_shape_conflict(&old, &branch, local_body.clone())?;
    let mut previous = note(&journal.db, &id)?.unwrap();
    previous.snapshot = old.export(ExportMode::Snapshot)?;
    checkpoint_note(&journal.db, &id, &previous)?;
    enqueue(&journal.db, &id, old.export(ExportMode::updates(&before))?)?;
    let originally_queued = queued(&journal.db, &id)?;

    let healthy = store.create_from(&crate::docxml::parse("<title>Healthy</title><p>Another edit</p>")?)?;
    journal.merge(&Default::default())?;
    let request = journal.request()?;
    assert!(request.edits.iter().any(|edit| edit.note == healthy));
    assert!(request.edits.iter().all(|edit| edit.note != id));
    assert!(has_conflict(&journal.db, &id)?);
    assert_eq!(queued(&journal.db, &id)?, originally_queued);

    let mut store = SharedStore::for_test(store);
    let mut core = Core::new(local_body);
    core.sel = Selection::caret(Pos::new(core.doc.blocks[0].id, "Initial".len()));
    core.insert_text(" plus local edits");
    assert!(journal.save(&mut store, &id, &core)?.1);
    assert_eq!(store.load_for_edit(&id, false)?, core.doc);
    assert_eq!(note(&journal.db, &id)?.unwrap().snapshot, previous.snapshot);
    assert_eq!(queued(&journal.db, &id)?, originally_queued);
    Ok(())
}

#[test]
fn checkpoint_with_a_conflicting_protected_version_stays_byte_exact() -> Result<()> {
    let fixture = HistoryFixture::new()?;
    let mut receiver = Journal::open(&fixture.0, None)?;
    let unknown = uuid::Uuid::new_v4().to_string();
    let original = crate::docxml::parse("<title>Protected history</title><p>Initial</p>")?;
    let base = content::seed(&original)?;
    let old = content::open(&base)?;
    let valid_version = old.oplog_vv();
    let branch = content::open(&base)?;
    let alias_id = add_shape_conflict(&old, &branch, original)?;
    let invalid_version = old.oplog_vv();
    assert!(content::render(&old).unwrap_err().is::<content::ConflictingBlockShapes>());
    let tree = old.get_tree("blocks");
    let id = alias_id.0.to_string();
    let other = tree.roots().into_iter().find(|node|
        node.peer == 43 && content::value(&tree.get_meta(*node).unwrap(), "block").as_deref() == Some(id.as_str())).unwrap();
    tree.delete(other)?;
    assert_eq!(content::render(&old)?.blocks.len(), 2);
    let payload = retention::pack(&retention::Index { versions: vec![
        retention::Anchor { version: valid_version, recovery: true },
        retention::Anchor { version: invalid_version, recovery: true },
    ], records: vec![] }, &old.export(ExportMode::Snapshot)?)?;
    let checkpoint = wire(&unknown, payload.clone());
    receiver.receive(page(vec![checkpoint.clone()], vec![], 1))?;
    receiver.merge(&Default::default())?;
    assert_eq!(receiver.pending_work()?, (0, 0, 1));
    assert_eq!(receiver.issues()?[0].title, None);
    assert_eq!(receiver.db.query_row("SELECT data FROM inbox WHERE id = ?1", [&checkpoint.id],
        |row| row.get::<_, Vec<u8>>(0))?, payload);
    assert_eq!(receiver.db.query_row("SELECT count(*) FROM sync_history WHERE note = ?1", [&unknown],
        |row| row.get::<_, u32>(0))?, 0);
    assert!(receiver.request()?.maintenance.is_none());
    Ok(())
}

#[test]
fn feishu_style_html_paste_round_trips_and_concurrent_pastes_use_distinct_ids() -> Result<()> {
    let html = r#"<div data-block-id="copied"><h2>Project plan</h2>
        <p><b>Owner</b> and <a href="https://example.invalid/">reference</a></p>
        <ul class="list-done1"><li data-list="done"><div>First task</div></li></ul>
        <ul><li>Section<ul><li>Nested list</li></ul></li></ul>
        <table><tr><th>Item</th><th>Due</th></tr><tr><td>Design</td><td>Nov 18</td></tr></table>
        <blockquote><p>Multiple blocks</p></blockquote></div>"#;
    let seed = crate::docxml::parse("<title>Paste target</title><p>Before</p><p>After</p>")?;
    let snapshot = content::seed(&seed)?;
    let mut edits = Vec::new();
    for (peer, target) in [(52, 0), (53, 1)] {
        let mut core = Core::new(seed.clone());
        core.sel = Selection::caret(Pos::new(seed.blocks[target].id, 0));
        core.paste_blocks(crate::paste::html_to_blocks(html));
        assert!(core.doc.blocks.iter().any(|block| matches!(block.kind, BlockKind::Table(_))));
        assert!(core.doc.blocks.iter().any(|block| matches!(block.kind, BlockKind::Todo { done: true })));
        let loro = content::open(&snapshot)?;
        edits.push(content::edit(&loro, peer, &core.doc)?.unwrap());
        assert_eq!(crate::docxml::serialize(&content::render(&loro)?), crate::docxml::serialize(&core.doc));
    }
    let left = content::open(&snapshot)?;
    let right = content::open(&snapshot)?;
    assert!(left.import_batch(&edits)?.pending.is_none());
    edits.reverse();
    assert!(right.import_batch(&edits)?.pending.is_none());
    let merged = content::render(&left)?;
    assert_eq!(merged, content::render(&right)?);
    let mut seen = HashSet::new();
    merged.for_each_block(|block| assert!(seen.insert(block.id)));
    assert_eq!(merged.blocks.iter().filter(|block| matches!(block.kind, BlockKind::Table(_))).count(), 2);
    Ok(())
}

#[test]
fn discarded_replay_candidate_does_not_stop_a_later_import() -> Result<()> {
    let result = with_replay_candidate(LoroDoc::new(), |doc| -> Result<(), loro::LoroError> {
        doc.get_map("test").insert("unpublished", true)?;
        panic!("simulated replay failure");
    });
    assert!(matches!(result, Err(ReplayFailure::Panicked)));
    let sender = LoroDoc::new();
    sender.get_map("test").insert("published", true)?;
    let snapshot = sender.export(ExportMode::Snapshot)?;
    let (next, _) = with_replay_candidate(LoroDoc::new(), |doc| doc.import(&snapshot))?;
    assert!(next.get_map("test").get("published").is_some());
    assert!(next.get_map("test").get("unpublished").is_none());
    Ok(())
}

#[test]
fn catalogue_replay_pause_survives_restart_and_existing_documents_sync() -> Result<()> {
    let fixture = HistoryFixture::new()?;
    let mut journal = Journal::open(&fixture.0, None)?;
    let mut store = Store::open_sqlite_at(fixture.0.clone())?;
    let seed = crate::docxml::parse("<title>Fixture</title><p>Shared base</p>")?;
    let outgoing = store.create_from(&seed)?;
    let incoming_body = store.create_from(&seed)?;
    journal.merge(&Default::default())?;
    let initial = journal.request()?.edits.into_iter().map(|edit| edit.id).collect();
    journal.receive(page(vec![], initial, 1))?;

    // The catalogue already has an outbound metadata change that must stay durable.
    store.set_pinned(&outgoing, true)?;
    journal.merge(&Default::default())?;
    let original_records = queued(&journal.db, catalogue::SCOPE)?;
    assert!(!original_records.is_empty());
    let original_catalogue = get_state(&journal.db, "catalogue")?;

    let rejected = wire(catalogue::SCOPE, b"not a Loro record".to_vec());
    let remote = journal.text_state(&incoming_body)?.context("Missing fixture state")?;
    let mut downloaded = seed.clone();
    downloaded.blocks[0].text = RichText::plain("Remote body");
    let remote_update = wire(&incoming_body, content::edit(&remote, journal.peer + 1, &downloaded)?
        .context("Missing fixture update")?);
    let mut edited = seed.clone();
    edited.blocks[0].text = RichText::plain("Local body");
    store.save(&outgoing, &edited)?;
    // Exercise replay from the durable inbox without inventory validation.
    let mut staged = page(vec![rejected.clone(), remote_update], vec![], 2);
    staged.retention = None;
    journal.receive(staged)?;
    let (_, changed) = journal.merge(&Default::default())?;
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].id, incoming_body);
    assert_eq!(store.load_for_edit(&incoming_body, false)?, downloaded);
    assert_eq!(store.load_for_edit(&outgoing, false)?, edited);
    assert!(journal.catalogue_paused()?);
    assert_eq!(get_state(&journal.db, "catalogue")?, original_catalogue);
    assert_eq!(queued(&journal.db, catalogue::SCOPE)?, original_records);
    assert_eq!(journal.db.query_row("SELECT data FROM inbox WHERE id = ?1", [&rejected.id],
        |row| row.get::<_, Vec<u8>>(0))?, rejected.data.0);
    let request = journal.request()?;
    assert!(request.maintenance.is_none());
    assert!(request.edits.iter().any(|edit| edit.note == outgoing));
    assert!(request.edits.iter().all(|edit| edit.note != catalogue::SCOPE));
    journal.receive(page(vec![], request.edits.into_iter().map(|edit| edit.id).collect(), 3))?;
    drop(journal);

    let mut journal = Journal::open(&fixture.0, None)?;
    assert_eq!(journal.request()?.token.unwrap().0, [3]);
    assert!(journal.merge(&Default::default())?.1.is_empty());
    assert!(journal.catalogue_paused()?);
    assert_eq!(journal.pending_work()?, (0, 0, 0));
    assert_eq!(journal.counts()?.1, 1);
    assert_eq!(get_state(&journal.db, "catalogue")?, original_catalogue);
    assert_eq!(queued(&journal.db, catalogue::SCOPE)?, original_records);
    journal.retry_conflicts()?;
    assert!(journal.merge(&Default::default())?.1.is_empty());
    assert!(journal.catalogue_paused()?);
    assert_eq!(journal.counts()?.1, 1);
    Ok(())
}

#[test]
fn rejected_document_replay_keeps_inbox_and_other_documents_moving() -> Result<()> {
    let fixture = HistoryFixture::new()?;
    let mut journal = Journal::open(&fixture.0, None)?;
    let mut store = Store::open_sqlite_at(fixture.0.clone())?;
    let seed = crate::docxml::parse("<title>Fixture</title><p>Initial body</p>")?;
    let bad = store.create_from(&seed)?;
    let healthy = store.create_from(&seed)?;
    journal.merge(&Default::default())?;
    let initial = journal.request()?.edits.into_iter().map(|edit| edit.id).collect();
    journal.receive(page(vec![], initial, 1))?;
    let original = journal.text_state(&bad)?.context("Missing fixture state")?.oplog_vv();
    let rejected = wire(&bad, b"not a Loro record".to_vec());
    let mut edited = seed.clone();
    edited.blocks[0].text = RichText::plain("Healthy new body");
    store.save(&healthy, &edited)?;
    let mut staged = page(vec![rejected.clone()], vec![], 2);
    staged.retention = None;
    journal.receive(staged)?;
    journal.merge(&Default::default())?;
    assert!(has_conflict(&journal.db, &bad)?);
    assert_eq!(store.load_for_edit(&bad, false)?, seed);
    assert_eq!(journal.text_state(&bad)?.unwrap().oplog_vv(), original);
    assert_eq!(journal.db.query_row("SELECT data FROM inbox WHERE id = ?1", [&rejected.id],
        |row| row.get::<_, Vec<u8>>(0))?, rejected.data.0);
    assert!(journal.request()?.edits.iter().any(|edit| edit.note == healthy));
    assert!(!journal.catalogue_paused()?);
    Ok(())
}
