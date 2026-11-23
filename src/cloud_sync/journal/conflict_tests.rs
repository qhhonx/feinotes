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
    let (_, changed, _) = journal.merge(&Default::default())?;
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

    let (_, changes, _) = journal.merge(&Default::default())?;
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
    assert!(!journal.save(&mut store, &id, &core)?.1);
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
    let (_, changed, _) = journal.merge(&Default::default())?;
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

fn divergent_tables(overlap: bool) -> Result<(crate::model::Block, crate::model::Block)> {
    let mut left = crate::docxml::parse("<table><tbody><tr><td><p>Local cell</p></td></tr></tbody></table>")?.blocks.remove(0);
    let mut right = crate::docxml::parse("<table><tbody><tr><td><p>Cloud cell</p></td><td><p>Second cell</p></td></tr></tbody></table>")?.blocks.remove(0);
    right.id = left.id; // Reproduce the observed logical alias, independently of its origin.
    if overlap {
        table(&mut right).rows[0][0].blocks[0].id = table(&mut left).rows[0][0].blocks[0].id;
    }
    Ok((left, right))
}

fn table(block: &mut crate::model::Block) -> &mut crate::model::Table {
    let BlockKind::Table(table) = &mut block.kind else { panic!("Missing fixture table") };
    table
}

#[test]
fn independent_tables_keep_late_cell_changes_and_stable_ids_after_reorder() -> Result<()> {
    let base = crate::docxml::parse("<title>Table fixture</title><p>Common body</p>")?;
    let snapshot = content::seed(&base)?;
    let (mut a, b) = divergent_tables(false)?;
    let asset = crate::docxml::parse("<figure view-type=\"Card\"><source path=\"assets/files/fixture.txt\" name=\"fixture.txt\"/></figure>")?.blocks.remove(0);
    table(&mut a).rows[0][0].blocks.push(asset);
    let mut left_body = base.clone();
    let mut right_body = base;
    left_body.blocks.push(a);
    right_body.blocks.push(b);
    let left = content::open(&snapshot)?;
    let right = content::open(&snapshot)?;
    let local = content::edit(&left, 42, &left_body)?.unwrap();
    let received = content::edit(&right, 43, &right_body)?.unwrap();
    table(&mut right_body.blocks[1]).rows[0][0].blocks[0].text = RichText::plain("Cloud cell, late offline edit");
    let delayed = content::edit(&right, 43, &right_body)?.unwrap();
    left.import(&received)?;
    right.import(&local)?;
    let (mut merged, recovered) = content::project(&left)?;
    assert!(recovered);
    let ids: HashSet<_> = merged.blocks[1..].iter().map(|b| b.id).collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(content::local_assets(&merged), HashSet::from(["assets/files/fixture.txt".to_owned()]));
    assert!(content::edit(&left, 42, &merged)?.is_none());
    merged.blocks.swap(1, 2);
    let reordered = content::edit(&left, 42, &merged)?.unwrap();
    right.import(&reordered)?;
    left.import_batch(&[delayed.clone(), delayed])?;
    let merged = content::render(&left)?;
    assert_eq!(merged, content::render(&right)?);
    assert_eq!(merged.blocks[1..].iter().map(|b| b.id).collect::<HashSet<_>>(), ids);
    assert!(merged.plain_text().contains("Cloud cell, late offline edit"));
    assert_eq!(merged.plain_text().matches("Local cell").count(), 1);

    // Both upgraded clients continue writing the old cell containers.
    let mut on_left = content::render(&left)?;
    let mut on_right = content::render(&right)?;
    table(&mut on_left.blocks[1]).rows[0][0].blocks[0].text = RichText::plain("Cloud cell, late offline edit + A");
    table(&mut on_right.blocks[1]).rows[0][1].blocks[0].text = RichText::plain("Second cell + B");
    let a = content::edit(&left, 42, &on_left)?.unwrap();
    let b = content::edit(&right, 43, &on_right)?.unwrap();
    left.import(&b)?;
    right.import(&a)?;
    let merged = content::render(&left)?;
    assert_eq!(merged, content::render(&right)?);
    assert!(merged.plain_text().contains("Cloud cell, late offline edit + A"));
    assert!(merged.plain_text().contains("Second cell + B"));
    let mut one = merged.clone();
    one.blocks.pop();
    content::edit(&left, 42, &one)?;
    assert_eq!(content::render(&left)?, one);
    content::edit(&left, 42, &merged)?;
    assert_eq!(content::render(&left)?, merged);
    Ok(())
}

#[test]
fn normal_journals_project_the_same_tables_and_reuse_existing_outbox_after_restart() -> Result<()> {
    let a = HistoryFixture::new()?;
    let b = HistoryFixture::new()?;
    let mut left = Journal::open(&a.0, None)?;
    let mut right = Journal::open(&b.0, None)?;
    let mut left_store = Store::open_sqlite_at(a.0.clone())?;
    let mut right_store = Store::open_sqlite_at(b.0.clone())?;
    let id = left_store.create_from(&crate::docxml::parse("<title>Journal fixture</title><p>Common body</p>")?)?;
    left.merge(&Default::default())?;
    let base = left.request()?.edits;
    left.receive(page(vec![], base.iter().map(|edit| edit.id.clone()).collect(), 1))?;
    right.receive(page(base, vec![], 1))?;
    right.merge(&Default::default())?;
    let (a_table, b_table) = divergent_tables(false)?;
    for (journal, store, block) in [(&mut left, &mut left_store, a_table), (&mut right, &mut right_store, b_table)] {
        let mut doc = store.load_for_edit(&id, false)?;
        doc.blocks.push(block);
        store.save(&id, &doc)?;
        journal.merge(&Default::default())?;
    }
    let ours = left.request()?.edits;
    let theirs = right.request()?.edits;
    let left_queue = queued(&left.db, &id)?;
    let right_queue = queued(&right.db, &id)?;
    // One end is an older installation with a conflict parked before upgrade.
    block_document(&left.db, &id)?;
    left.receive(page(theirs.clone(), vec![], 2))?;
    block_document(&left.db, &id)?;
    left.db.execute("DELETE FROM state WHERE key = ?1", [DOCUMENT_REPLAY_UPGRADE])?;
    drop(left);
    let mut left = Journal::open(&a.0, None)?;
    right.receive(page(ours.clone(), vec![], 2))?;
    for journal in [&mut left, &mut right] {
        assert_eq!(journal.merge(&Default::default())?.1.len(), 1);
        assert!(!journal.document_conflict(&id));
    }
    assert_eq!(queued(&left.db, &id)?, left_queue);
    assert_eq!(queued(&right.db, &id)?, right_queue);
    let projected = left_store.load_for_edit(&id, false)?;
    assert_eq!(projected, right_store.load_for_edit(&id, false)?);
    assert_eq!(projected.blocks.len(), 3);
    assert!(projected.plain_text().contains("Local cell") && projected.plain_text().contains("Cloud cell"));
    drop(right);
    let mut right = Journal::open(&b.0, None)?;
    for (journal, duplicate, sent) in [(&mut left, theirs.clone(), ours.clone()), (&mut right, ours, theirs)] {
        journal.retry_document(&id)?;
        journal.receive(page(duplicate.clone(), sent.iter().map(|edit| edit.id.clone()).collect(), 3))?;
        assert!(journal.merge(&Default::default())?.1.is_empty());
        assert!(queued(&journal.db, &id)?.is_empty());
        assert_eq!(content::render(&journal.text_state(&id)?.unwrap())?, projected);
    }
    let branch = right.text_state(&id)?.unwrap();
    let mut latest = projected;
    latest.title = RichText::plain("Later, ordinary edit");
    let update = wire(&id, content::edit(&branch, right.peer, &latest)?.unwrap());
    left.receive(page(vec![update], vec![], 4))?;
    assert_eq!(left.merge(&Default::default())?.1[0].document, latest);
    assert!(!left.db.query_row("SELECT recovery FROM sync_history WHERE note = ?1 ORDER BY sequence DESC LIMIT 1",
        [&id], |row| row.get::<_, bool>(0))?);
    Ok(())
}

#[test]
fn overlapping_table_cell_ids_require_review_and_copy_selected_versions_atomically() -> Result<()> {
    let fixture = HistoryFixture::new()?;
    let mut journal = Journal::open(&fixture.0, None)?;
    let mut store = Store::open_sqlite_at(fixture.0.clone())?;
    let id = store.create_from(&crate::docxml::parse("<title>Compare tables</title><p>Common</p>")?)?;
    journal.merge(&Default::default())?;
    let sent = journal.request()?.edits.into_iter().map(|edit| edit.id).collect();
    journal.receive(page(vec![], sent, 1))?;
    let branch = journal.text_state(&id)?.unwrap();
    let (a, b) = divergent_tables(true)?;
    let mut local = store.load_for_edit(&id, false)?;
    let mut received = local.clone();
    local.blocks.push(a);
    received.blocks.push(b);
    let asset = "assets/files/table-review.txt";
    let file = crate::docxml::parse(&format!("<figure view-type=\"Card\"><source path=\"{asset}\" name=\"table-review.txt\"/></figure>"))?.blocks.remove(0);
    table(&mut received.blocks[1]).rows[0][1].blocks.push(file);
    let remote = wire(&id, content::edit(&branch, journal.peer + 1, &received)?.unwrap());
    store.save(&id, &local)?;
    journal.merge(&Default::default())?;
    let pending = queued(&journal.db, &id)?;
    journal.receive(page(vec![remote.clone()], vec![], 2))?;
    journal.merge(&Default::default())?;
    assert!(journal.document_conflict(&id));
    assert_eq!(journal.pending_work()?.2, 1);
    let versions = journal.conflict_versions(&id)?;
    assert_eq!(versions.local.document, local);
    assert_eq!(versions.received.as_ref().unwrap().document, received);
    assert!(!versions.received.unwrap().copyable);
    assert!(journal.request()?.edits.iter().all(|e| e.note != id));

    std::fs::create_dir_all(fixture.0.join("assets/files"))?;
    std::fs::write(fixture.0.join(asset), b"synthetic attachment")?;
    journal.retry_document(&id)?;
    assert!(journal.merge(&Default::default())?.1.is_empty());
    drop(journal);
    let mut journal = Journal::open(&fixture.0, None)?;
    let versions = journal.conflict_versions(&id)?;
    let selected = [(false, versions.local.stamp), (true, versions.received.unwrap().stamp)];
    let calls = std::cell::Cell::new(0);
    assert!(journal.copy_conflict_versions(&id, &selected, || {
        calls.set(calls.get() + 1);
        ensure!(calls.get() == 1, "Synthetic account switch");
        Ok(())
    }).is_err());
    assert_eq!(store.list().len(), 1);

    local.title = RichText::plain("Local change after opening review");
    store.save(&id, &local)?;
    assert!(journal.copy_conflict_versions(&id, &selected, || Ok(())).is_err());
    assert_eq!(store.list().len(), 1);
    let versions = journal.conflict_versions(&id)?;
    let selected = [(false, versions.local.stamp), (true, versions.received.unwrap().stamp)];
    let copies = journal.copy_conflict_versions(&id, &selected, || Ok(()))?;
    assert_eq!(copies.len(), 2);
    assert_eq!(journal.copy_conflict_versions(&id, &selected, || Ok(()))?, copies);
    assert_eq!(store.list().len(), 3);
    assert_eq!(store.load(&copies[0])?.blocks, local.blocks);
    assert_eq!(store.load(&copies[1])?.blocks, received.blocks);
    assert_eq!(std::fs::read(fixture.0.join(asset))?, b"synthetic attachment");
    assert_eq!(queued(&journal.db, &id)?, pending);
    assert_eq!(journal.db.query_row("SELECT data FROM inbox WHERE id = ?1", [&remote.id], |row| row.get::<_, Vec<u8>>(0))?, remote.data.0);
    assert!(journal.document_conflict(&id));
    Ok(())
}

#[test]
fn write_rejects_reused_cell_ids_before_creating_loro_operations() -> Result<()> {
    let base = crate::docxml::parse("<title>Unique IDs</title><p>Common</p>")?;
    let loro = content::open(&content::seed(&base)?)?;
    let before = loro.oplog_vv();
    let mut doc = base;
    let (a, mut b) = divergent_tables(true)?;
    b.id = crate::model::BlockId::new();
    doc.blocks.extend([a, b]);
    assert!(content::edit(&loro, 42, &doc).unwrap_err().is::<content::ConflictingBlockShapes>());
    assert_eq!(loro.oplog_vv(), before);
    Ok(())
}

#[test]
fn shared_editors_and_readers_project_identical_tables_without_permission_changes() -> Result<()> {
    for folder in [false, true] {
        let origin = HistoryFixture::new()?;
        let writer = HistoryFixture::new()?;
        let reader = HistoryFixture::new()?;
        let mut private = Store::open_sqlite_at(origin.0.clone())?;
        let parent = folder.then(|| private.create_folder("Table folder", None)).transpose()?;
        let seed = crate::docxml::parse("<title>Shared tables</title><p>Common body</p>")?;
        let id = private.create_from_in(&seed, parent.as_deref())?;
        let room = shared::original::prepare_local(&origin.0, parent.as_deref().unwrap_or(&id), "Shared tables".into(), false)?;
        let owner_root = shared::room_root(&origin.0, &room.scope.note)?;
        let mut owner = Journal::open(&owner_root, None)?;
        let mut owner_store = SharedStore::from_shared_store(Store::open_sqlite_at(owner_root)?);
        let mut invite = room.clone();
        invite.scope.owner = Some("synthetic-share-owner".into());
        let guest_root = |personal: &Path| -> Result<PathBuf> {
            let root = shared::room_root(personal, &room.scope.note)?;
            Store::initialize_shared_sqlite(&root, &shared::account(personal)?)?;
            std::fs::write(root.join(shared::MARKER), serde_json::to_vec(&invite)?)?;
            Ok(root)
        };
        let writer_root = guest_root(&writer.0)?;
        let reader_root = guest_root(&reader.0)?;
        let mut guest = Journal::open(&writer_root, None)?;
        let mut observer = Journal::open(&reader_root, None)?;
        let mut guest_store = SharedStore::from_shared_store(Store::open_sqlite_at(writer_root)?);
        let observer_store = Store::open_sqlite_at(reader_root)?;
        let page = |access, edits, uploaded| Changes { room_access: Some(access),
            retention: None, token: (access != Access::Revoked).then(|| Bytes(vec![1])),
            ..page(edits, uploaded, 1)
        };
        let seed = owner.request()?.edits;
        owner.receive(page(Access::Owner, vec![], seed.iter().map(|e| e.id.clone()).collect()))?;
        for (journal, access) in [(&mut guest, Access::ReadWrite), (&mut observer, Access::ReadOnly)] {
            journal.receive(page(access, seed.clone(), vec![]))?;
            journal.merge(&Default::default())?;
        }
        let (left, right) = divergent_tables(false)?;
        for (journal, store, block) in [(&mut owner, &mut owner_store, left), (&mut guest, &mut guest_store, right)] {
            let mut body = store.load_for_edit(&id, true)?;
            body.blocks.push(block);
            journal.save(store, &id, &Core::new(body))?;
        }
        let local = owner.request()?.edits;
        let collaborator = guest.request()?.edits;
        let guest_queue = queued(&guest.db, &id)?;
        let both: Vec<_> = local.iter().chain(&collaborator).cloned().collect();
        owner.receive(page(Access::Owner, collaborator, vec![]))?;
        guest.receive(page(Access::ReadWrite, local, vec![]))?;
        observer.receive(page(Access::ReadOnly, both.clone(), vec![]))?;
        for journal in [&mut owner, &mut guest, &mut observer] {
            assert_eq!(journal.merge(&Default::default())?.1.len(), 1);
            assert!(!journal.document_conflict(&id));
        }
        let merged = owner_store.load_for_edit(&id, true)?;
        assert_eq!(merged, guest_store.load_for_edit(&id, true)?);
        assert_eq!(merged, observer_store.load_for_edit(&id, true)?);
        assert_eq!(merged.blocks.len(), 3);
        assert!(observer.request()?.edits.is_empty());
        guest.receive(page(Access::ReadOnly, both, vec![]))?;
        assert!(guest.merge(&Default::default())?.1.is_empty());
        assert!(guest.save(&mut guest_store, &id, &Core::new(merged)).is_err());
        assert!(guest.request()?.edits.is_empty());
        assert_eq!(queued(&guest.db, &id)?, guest_queue);
        guest.receive(page(Access::Revoked, vec![], vec![]))?;
        assert!(guest.merge(&Default::default())?.1.is_empty());
        assert!(guest.conflict_versions(&id).is_err());
        assert!(guest.request()?.edits.is_empty());
        assert_eq!(queued(&guest.db, &id)?, guest_queue);
    }
    Ok(())
}
