// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Signed, isolated test commands use the same Store and CloudKit bridge as
//! the app. They leave the editor and the user's normal library untouched.

use anyhow::{Context, Result, ensure};
use std::{fs, path::Path, thread, time::Duration};
use crate::{editor::core::{Core, Pos, Selection}, model::{Block, BlockKind, Document, FileBlock, FileView, RichText, Table, TableCell}, shared_store::SharedStore, store::Store};
use super::{Changes, Journal, exchange};

const NOTE: &str = "d46bc580-ad2c-4bb9-8eeb-570caa057178";
const MD: &str = "13ff04aa-6bf5-4876-9034-7e7e13b9b788";
const ASSET: &str = "assets/files/31abff5b-de53-4884-8d05-777ae441b563/offline-attachment.bin";
const ATTACHMENT_BYTES: usize = 1024 * 1024 + 13;
const LARGE_ASSET: &str = "assets/files/daily-large.bin";
const DAILY_NOTES: usize = 150;
const SCALE_NOTES: usize = 1000;
const OFFLINE_EDITS: usize = 60;
const STRUCTURE_TITLE: &str = "协同结构验收";

pub fn run(command: &str) -> Result<()> {
    let channel = std::env::var("QUIET_SYNC_QA_CHANNEL").context("A QA channel is required")?;
    uuid::Uuid::parse_str(&channel)?;
    ensure!(super::test_root()?.is_some(), "An isolated QA root is required");
    if matches!(command, "trial-start" | "trial-status") { return crate::cloud_trial::qa(command == "trial-start"); }
    if matches!(command, "select-cloud" | "copy-to-cloud" | "select-local") {
        use crate::cloud_restore::LibrarySelection;
        ensure!(std::env::var_os("QUIET_SYNC_QA_LOCAL_FIRST").is_some()
            && std::env::var_os("QUIET_SYNC_QA_MIGRATION").is_some(), "Use an isolated library selection test");
        let selection = match command {
            "select-cloud" => LibrarySelection::Cloud,
            "copy-to-cloud" => LibrarySelection::CopyToCloud,
            _ => LibrarySelection::Local,
        };
        let count = crate::cloud_restore::select_library(selection)?;
        println!("LIBRARY_CHOICE_SAVED cloud={} copied={count}", selection != LibrarySelection::Local);
        return Ok(());
    }
    let root = Store::root_dir();
    let mut store = Store::open()?;
    let mut journal = Journal::open(&root, Some(channel))?;
    if matches!(command, "sync" | "replay" | "daily-sync") { replay_legacy(&mut journal)?; }
    match command {
        "edit-a" | "edit-b" => {
            let a = command == "edit-a";
            let mut doc = store.load(NOTE)?;
            let text = &mut doc.blocks[0].text;
            let position = if a { text.text.len() } else { 0 };
            text.replace(position..position, &RichText::plain(if a { "甲" } else { "乙" }));
            store.save(NOTE, &doc)?;
            if a {
                let folder = store.create_folder("离线目录 A", None)?;
                store.set_parent(NOTE, Some(&folder))?;
                store.set_pinned(NOTE, true)?;
                let mut md = store.load(MD)?;
                md.blocks[0].text = RichText::plain("回收站里的并发改动");
                store.save(MD, &md)?;
                let path = root.join(ASSET);
                fs::create_dir_all(path.parent().unwrap())?;
                fs::write(&path, attachment_bytes())?;
                let mut added = Document { title: RichText::plain("离线附件笔记"), ..Default::default() };
                added.blocks.push(Block::new(BlockKind::File(FileBlock {
                    src_kind: "path".into(), src: ASSET.into(), name: "offline-attachment.bin".into(), view: FileView::Card,
                }), RichText::default()));
                let added = store.create_from(&added)?;
                store.set_parent(&added, Some(&folder))?;
            } else {
                store.set_starred(NOTE, true)?;
                store.create_folder("离线目录 B", None)?;
                store.delete(MD, None)?;
            }
            journal.merge(&Default::default())?;
            ensure!(!journal.request()?.edits.is_empty(), "Offline edits were not queued");
            println!("OFFLINE_SAVED command={command}");
        }
        "cycle-a" | "cycle-b" => {
            let a = command == "cycle-a";
            let entries = store.list();
            let left = entries.iter().find(|entry| entry.title == "离线目录 A").context("Missing folder A")?;
            let right = entries.iter().find(|entry| entry.title == "离线目录 B").context("Missing folder B")?;
            let mut note = store.load(NOTE)?;
            if a {
                store.restore(MD)?;
                store.rename(&left.id, "离线目录 A（改名）")?;
                store.set_parent(&left.id, Some(&right.id))?;
                let added = entries.iter().find(|entry| entry.title == "离线附件笔记").context("Missing new note")?;
                store.move_in_tree(&added.id, Some(&left.id), Some(NOTE))?;
                let start = &mut note.blocks[0].text;
                let split = start.text.char_indices().nth(2).unwrap().0;
                let tail = start.split_off(split);
                note.blocks.insert(1, Block::new(BlockKind::Text, tail));
                let mut md = store.load(MD)?;
                md.blocks[0].text = RichText::plain("回收站里的并发改动／保留被删段的编辑");
                store.save(MD, &md)?;
            } else {
                store.set_parent(&right.id, Some(&left.id))?;
                let start = &mut note.blocks[0].text;
                let position = start.text.len();
                start.replace(position..position, &RichText::plain("尾端并发"));
                store.restore(MD)?;
                let mut md = store.load(MD)?;
                md.blocks.remove(0);
                if md.blocks.is_empty() { md.blocks.push(Block::text("")); }
                store.save(MD, &md)?;
            }
            store.save(NOTE, &note)?;
            journal.merge(&Default::default())?;
        }
        "sync" => {
            for _ in 0..18 {
                journal.merge(&Default::default())?;
                let changes = exchange(journal.request()?)?;
                let more = changes.more;
                journal.receive(changes)?;
                journal.merge(&Default::default())?;
                if !more && journal.request()?.edits.is_empty() { thread::sleep(Duration::from_secs(1)); }
            }
            println!("CLOUD_EXCHANGE_OK");
        }
        "sync-reordered" => {
            let mut request = journal.request()?;
            ensure!(request.token.is_none() && request.edits.is_empty(), "Use a fresh replica for the replay test");
            let mut edits = Vec::new();
            let token = loop {
                let page = exchange(request)?;
                edits.extend(page.edits);
                if !page.more { break page.token; }
                request = journal.request()?;
                request.token = page.token;
            };
            let count = edits.len();
            ensure!(count > 4, "Expected several CloudKit changes");
            // Deliver one record at a time so a note may arrive before its
            // dependencies, catalogue entry or attachment chunks.
            for edit in edits.into_iter().rev() {
                for _ in 0..2 {
                    journal.receive(Changes { uploaded: vec![], edits: vec![edit.clone()], token: token.clone(), more: false, legacy: false,
                        room_access: None, cursor_replayed: false, upload_quota_exceeded: false })?;
                    journal.merge(&Default::default())?;
                }
            }
            let db = rusqlite::Connection::open(root.join(crate::store::SQLITE_FILE))?;
            let pending: i64 = db.query_row("SELECT COUNT(*) FROM inbox", [], |row| row.get(0))?;
            ensure!(pending == 0, "Reversed replay left pending dependencies");
            println!("REPLAY_OK records={count} duplicate_delivery=2 order=reversed");
        }
        "verify-trash" | "verify-live" => verify(&root, &mut store, &journal, command == "verify-trash")?,
        "verify-migration" => verify_migration(&root, &mut store)?,
        "empty-baseline" => {
            ensure!(store.list().is_empty() && store.list_trash().is_empty(), "Expected a fresh empty library");
            println!("EMPTY_LIBRARY_OK");
        }
        "backup-seed" => {
            ensure!(store.list().is_empty() && store.list_trash().is_empty(), "Use an empty backup test library");
            let folder = store.create_folder("备份测试文件夹", None)?;
            let path = root.join(ASSET);
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(path, attachment_bytes())?;
            let note = store.create_from(&Document { title: RichText::plain("备份测试笔记"),
                blocks: vec![Block::text("离线正文，还没上传"), Block::new(BlockKind::File(FileBlock {
                    src_kind: "path".into(), src: ASSET.into(), name: "offline-attachment.bin".into(), view: FileView::Card,
                }), RichText::default())], ..Default::default() })?;
            store.set_parent(&note, Some(&folder))?;
            let archived = store.create_from(&Document { title: RichText::plain("备份回收站"),
                blocks: vec![Block::text("回收站正文")], ..Default::default() })?;
            store.set_parent(&archived, Some(&folder))?;
            store.delete(&archived, None)?;
            journal.merge(&Default::default())?;
            ensure!(journal.counts()?.0 > 0, "Expected pending edits in backup fixture");
            println!("BACKUP_FIXTURE_READY");
        }
        "backup-verify-offline" | "backup-verify-synced" => {
            let entries = store.list();
            let folder = entries.iter().find(|entry| entry.title == "备份测试文件夹").context("Missing backed-up folder")?;
            let note = entries.iter().find(|entry| entry.title == "备份测试笔记").context("Missing backed-up note")?;
            let trash = store.list_trash();
            ensure!(entries.len() == 2 && note.meta.parent.as_ref() == Some(&folder.id)
                && trash.len() == 1 && trash[0].title == "备份回收站"
                && Store::read_sync_body(&rusqlite::Connection::open(root.join(crate::store::SQLITE_FILE))?,
                    &trash[0].id, true)?.0.plain_text().contains("回收站正文"), "Backup hierarchy or Trash changed");
            ensure!(store.load(&note.id)?.plain_text().contains("离线正文，还没上传")
                && fs::read(root.join(ASSET))? == attachment_bytes(), "Backup note or attachment changed");
            let pending = journal.counts()?.0;
            ensure!((command == "backup-verify-offline") == (pending > 0), "Unexpected restored upload queue");
            println!("BACKUP_FIXTURE_VERIFIED pending={pending}");
        }
        "daily-seed" | "scale-seed" => {
            let total = if command == "scale-seed" { SCALE_NOTES } else { DAILY_NOTES };
            ensure!(store.list().is_empty(), "Use a fresh daily-use test library");
            let folder = store.create_folder("日常验收", None)?;
            let path = root.join(ASSET);
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(path, attachment_bytes())?;
            fs::File::create_new(root.join(LARGE_ASSET))?.set_len(64 * 1024 * 1024 + 1)?;
            for index in 0..total {
                let mut doc = Document { title: RichText::plain(format!("日常笔记 {index}")), ..Default::default() };
                doc.blocks = (0..10).map(|n| Block::text(&format!("文档 {index} 段落 {n} {}", "海盐橙子".repeat(40)))).collect();
                if index == 0 {
                    for (source, name) in [(ASSET, "small.bin"), (LARGE_ASSET, "large.bin")] {
                        doc.blocks.push(Block::new(BlockKind::File(FileBlock {
                            src_kind: "path".into(), src: source.into(), name: name.into(), view: FileView::Card,
                        }), RichText::default()));
                    }
                }
                let id = store.create_from(&doc)?;
                store.set_parent(&id, Some(&folder))?;
            }
            journal.merge(&Default::default())?;
            ensure!(journal.counts()?.3 == 1, "Expected one local-only attachment");
            println!("DAILY_SEEDED notes={total} attachment_local_only=1");
        }
        "daily-sync" => {
            let start = std::time::Instant::now();
            loop {
                let page = exchange(journal.request()?)?;
                let more = page.more;
                journal.receive(page)?;
                journal.merge(&Default::default())?;
                let counts = journal.counts()?;
                if !more && counts.0 == 0 && counts.1 == 0 { break; }
                ensure!(start.elapsed() < Duration::from_secs(300), "Timed out draining test changes");
            }
            println!("DAILY_SYNC_OK seconds={:.1}", start.elapsed().as_secs_f64());
        }
        "daily-verify" | "scale-verify" => {
            let total = if command == "scale-verify" { SCALE_NOTES } else { DAILY_NOTES };
            let entries = store.list();
            ensure!(entries.len() == total + 1, "Restored library is incomplete");
            let folder = entries.iter().find(|entry| entry.title == "日常验收").context("Missing folder")?;
            let note = entries.iter().find(|entry| entry.title == "日常笔记 0").context("Missing attachment note")?;
            ensure!(entries.iter().filter(|entry| entry.meta.parent.as_ref() == Some(&folder.id)).count() == total,
                "Missing folder membership");
            let document = store.load(&note.id)?;
            ensure!(document.asset_sources() == vec![ASSET, LARGE_ASSET] && document.plain_text().contains("海盐橙子"),
                "Missing attachment references or note text");
            ensure!(fs::read(root.join(ASSET))? == attachment_bytes() && !root.join(LARGE_ASSET).exists(),
                "Attachment was not restored or skipped correctly");
            ensure!(journal.counts()? == (0, 0, 0, 1), "Incorrect pending or warning count");
            println!("DAILY_RESTORED notes={total} supported_assets=1 local_only_assets=1");
        }
        "scale-offline-a" | "scale-offline-b" => {
            let a = command == "scale-offline-a";
            let entry = store.list().into_iter().find(|entry| entry.title == "日常笔记 1").context("Missing offline test note")?;
            let mut doc = store.load(&entry.id)?;
            for i in 0..OFFLINE_EDITS {
                let text = &mut doc.blocks[0].text;
                let at = if a { 0 } else { text.text.len() };
                text.replace(at..at, &RichText::plain(format!("{}{i:02} ", if a { "甲" } else { "乙" })));
                store.save(&entry.id, &doc)?;
                journal.merge(&Default::default())?;
            }
            ensure!(journal.counts()?.0 == OFFLINE_EDITS as u32, "Missing offline changes");
            println!("OFFLINE_QUEUED edits={OFFLINE_EDITS}");
        }
        "scale-offline-verify" => {
            let entry = store.list().into_iter().find(|entry| entry.title == "日常笔记 1").context("Missing offline test note")?;
            let doc = store.load(&entry.id)?;
            let text = &doc.blocks[0].text.text;
            for i in 0..OFFLINE_EDITS {
                ensure!(text.matches(&format!("甲{i:02} ")).count() == 1 && text.matches(&format!("乙{i:02} ")).count() == 1,
                    "Offline change missing or replayed twice");
            }
            ensure!(journal.counts()? == (0, 0, 0, 1), "Unexpected journal state");
            let digest = <sha2::Sha256 as sha2::Digest>::digest(crate::docxml::serialize(&doc).as_bytes());
            println!("OFFLINE_MERGED changes={} digest={digest:x}", 2 * OFFLINE_EDITS);
        }
        "damage-cursor" => {
            let db = rusqlite::Connection::open(root.join(crate::store::SQLITE_FILE))?;
            ensure!(db.execute("UPDATE state SET value = x'010203' WHERE key = 'token'", [])? == 1,
                "Fetch at least one page before testing cursor recovery");
            println!("CURSOR_DAMAGED");
        }
        "repair-attachments" => {
            ensure!(journal.request()?.token.is_some(), "Use a library that has already synced");
            ensure!(journal.retry_attachments()?, "Expected a missing previously synced attachment");
            ensure!(journal.request()?.token.is_none() && journal.issues()?.iter().any(|issue|
                issue.attachments.iter().any(|attachment| attachment.recovering)), "Missing durable repair request");
            println!("ATTACHMENT_REPAIR_STARTED");
        }
        "expire-cursor" => {
            let mut request = journal.request()?;
            ensure!(request.token.is_some() && request.edits.is_empty(), "Fetch existing changes before testing expiry");
            request.expire_cursor_for_qa = true;
            let page = exchange(request)?;
            ensure!(page.cursor_replayed && !page.edits.is_empty(), "Expected a CloudKit replay after simulated expiry");
            let count = page.edits.len();
            journal.receive(page)?;
            journal.merge(&Default::default())?;
            println!("CURSOR_EXPIRED_REPLAY records={count}");
        }
        "share-owner" => super::shared::live_owner_smoke(&root)?,
        "share-accept" | "share-accept-read-only" => super::shared::live_accept_smoke(&root, command == "share-accept-read-only")?,
        "share-members" | "share-invite-read" | "share-invite-write" | "share-permit-write" | "share-revoke" =>
            super::shared::live_member_change(&root, command)?,
        "structure-seed" => {
            ensure!(store.list().is_empty(), "Use an isolated empty structure test library");
            let table = Table { col_widths: vec![100., 100.], rows: [["零零", "零一"], ["一零", "一一"]].into_iter()
                .map(|row| row.into_iter().map(|text| TableCell { blocks: vec![Block::text(text)], ..TableCell::empty() }).collect()).collect(),
                ..Default::default() };
            let doc = Document { title: RichText::plain(STRUCTURE_TITLE),
                blocks: vec![Block::text("甲乙丙丁"), Block::text("戊己"), Block::text("庚辛"),
                    Block::new(BlockKind::Table(table), RichText::default())], ..Default::default() };
            store.create_from(&doc)?;
            journal.merge(&Default::default())?;
            println!("STRUCTURE_SEEDED");
        }
        "structure-a" | "structure-b" => {
            let id = store.list().into_iter().find(|e| e.title == STRUCTURE_TITLE).context("Missing structure test note")?.id;
            let mut editor = Core::new(store.load(&id)?);
            let (first, last, table) = (editor.doc.blocks[0].id, editor.doc.blocks[2].id, editor.doc.blocks[3].id);
            if command == "structure-a" {
                editor.sel = Selection::caret(Pos::new(first, "甲乙".len()));
                editor.enter();
                editor.sel = Selection::caret(Pos::new(last, 0));
                editor.backspace();
                editor.table_insert(table, true, 1);
                editor.select_cells(crate::editor::table_ops::CellSel {
                    table, anchor: (1, 0), head: (1, 0), kind: crate::editor::table_ops::CellSelKind::Rows,
                });
                ensure!(editor.table_move_selected(0), "Failed to move the test row");
            } else {
                editor.sel = Selection::caret(Pos::new(first, "甲乙丙".len()));
                editor.insert_text("远");
                editor.sel = Selection::caret(Pos::new(last, "庚".len()));
                editor.insert_text("端");
                editor.caret_in_cell(table, 1, 1);
                editor.insert_text("远");
            }
            let mut shared = SharedStore::open()?;
            journal.save(&mut shared, &id, &editor)?;
            ensure!(journal.counts()?.0 > 0, "Expected offline document changes");
            println!("STRUCTURE_QUEUED");
        }
        "structure-verify" => {
            let id = store.list().into_iter().find(|e| e.title == STRUCTURE_TITLE).context("Missing structure test note")?.id;
            let doc = store.load(&id)?;
            ensure!(doc.blocks.iter().take(3).map(|block| block.text.text.as_str()).eq(["甲乙", "丙远丁", "戊己庚端辛"]),
                "A concurrent character edit did not follow the split or joined paragraph");
            let Some(Block { kind: BlockKind::Table(table), .. }) = doc.blocks.get(3) else { anyhow::bail!("Missing table") };
            ensure!(table.ncols() == 3 && table.rows.len() == 2 && table.rows[0][2].blocks[0].text.text == "远一一"
                && table.rows[1][0].blocks[0].text.text == "零零", "A cell edit did not follow its row");
            ensure!(journal.counts()? == (0, 0, 0, 0), "Unexpected journal state");
            let digest = <sha2::Sha256 as sha2::Digest>::digest(crate::docxml::serialize(&doc).as_bytes());
            println!("STRUCTURE_MERGED digest={digest:x}");
        }
        "baseline" => {
            ensure!(store.list().len() == 3, "Expected the original test library");
            ensure!(journal.request()?.edits.is_empty(), "Unexpected pending test edits");
            println!("BASELINE_OK");
        }
        _ => anyhow::bail!("Unknown sync test command"),
    }
    Ok(())
}

fn attachment_bytes() -> Vec<u8> { (0..ATTACHMENT_BYTES).map(|i| (i % 251) as u8).collect() }

fn replay_legacy(journal: &mut Journal) -> Result<()> {
    loop {
        let page = exchange(journal.legacy_request()?)?;
        let more = page.more;
        journal.receive(page)?;
        journal.merge(&Default::default())?;
        if !more { return Ok(()); }
    }
}

fn verify(root: &Path, store: &mut Store, journal: &Journal, trashed: bool) -> Result<()> {
    let entries = store.list();
    let note = entries.iter().find(|entry| entry.id == NOTE).context("Missing shared note")?;
    let folder = entries.iter().find(|entry| entry.title == if trashed { "离线目录 A" } else { "离线目录 A（改名）" })
        .context("Missing folder A")?;
    let other = entries.iter().find(|entry| entry.title == "离线目录 B").context("Missing folder B")?;
    ensure!(note.meta.pinned && note.meta.starred > 0 && note.meta.parent.as_deref() == Some(&folder.id),
        "Concurrent catalogue changes did not merge");
    let doc = store.load(NOTE)?;
    ensure!(doc.blocks[0].text.text.starts_with("乙") && doc.plain_text().contains("甲"),
        "Concurrent character changes did not merge");
    ensure!(!journal.versions(NOTE)?.is_empty(), "Missing merge history");
    let added = entries.iter().find(|entry| entry.title == "离线附件笔记").context("Missing new note")?;
    ensure!(added.meta.parent.as_deref() == Some(&folder.id), "Missing new note's parent");
    ensure!(store.load(&added.id)?.asset_sources() == vec![ASSET], "Missing attachment link");
    ensure!(fs::read(root.join(ASSET))? == attachment_bytes(), "Restored attachment bytes differ");
    if trashed {
        ensure!(store.list_trash().iter().any(|entry| entry.id == MD), "Missing trashed Markdown note");
        let db = rusqlite::Connection::open(root.join(crate::store::SQLITE_FILE))?;
        let (md, _, _) = Store::read_sync_body(&db, MD, true)?;
        ensure!(md.plain_text().contains("回收站里的并发改动"), "Concurrent edit lost in trash");
    } else {
        ensure!(store.is_markdown(MD) && !store.list_trash().iter().any(|entry| entry.id == MD),
            "Restored Markdown did not return to the library");
        ensure!(added.meta.place() < note.meta.place(), "Sibling order did not sync");
        let (root, child) = if folder.id < other.id { (folder, other) } else { (other, folder) };
        ensure!(root.meta.parent.is_none() && child.meta.parent.as_ref() == Some(&root.id),
            "The folder move cycle was not resolved deterministically");
        ensure!(doc.blocks.len() > 1 && doc.plain_text().contains("尾端并发"), "The split hid the concurrent insertion");
        ensure!(journal.versions(MD)?.iter().any(|(sequence, _, _)| journal.version(MD, *sequence)
            .is_ok_and(|(version, _)| version.plain_text().contains("保留被删段的编辑"))),
            "The deleted block's concurrent edit is missing from history");
    }
    ensure!(journal.request()?.edits.is_empty(), "Outgoing edits still pending");
    println!("LIBRARY_OK concurrent_chars=2 concurrent_metadata=3 new_folders=2 new_notes=1 attachment_bytes={ATTACHMENT_BYTES} markdown_trashed={trashed}");
    Ok(())
}

pub fn prepare_legacy() -> Result<()> {
    let root = super::test_root()?.context("An isolated QA root is required")?.join("legacy-copy");
    ensure!(!root.exists(), "Use a fresh legacy copy");
    let mut old = Store::open_at(root.clone())?;
    let folder = old.create_folder("旧库副本", None)?;
    let mut doc = Document { title: RichText::plain("Legacy rich text"), ..Default::default() };
    let path = root.join(ASSET);
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, attachment_bytes())?;
    doc.blocks.push(Block::new(BlockKind::File(FileBlock {
        src_kind: "path".into(), src: format!("/old-machine/Quiet.md/{ASSET}"),
        name: "offline-attachment.bin".into(), view: FileView::Card,
    }), RichText::default()));
    let id = old.create_from(&doc)?;
    old.set_parent(&id, Some(&folder))?;
    doc.title = RichText::plain("Legacy Markdown");
    doc.blocks = vec![Block::text("archived Markdown body")];
    let id = old.create_markdown(&doc, "---\nkey: legacy\n---\n")?;
    old.set_parent(&id, Some(&folder))?;
    old.delete(&id, None)?;
    println!("LEGACY_COPY_READY");
    Ok(())
}

fn verify_migration(root: &Path, store: &mut Store) -> Result<()> {
    let entries = store.list();
    let folder = entries.iter().find(|entry| entry.title == "旧库副本").context("Missing migrated folder")?;
    let note = entries.iter().find(|entry| entry.title == "Legacy rich text").context("Missing migrated note")?;
    ensure!(note.meta.parent.as_ref() == Some(&folder.id), "Legacy folder relationship did not migrate");
    ensure!(store.load(&note.id)?.asset_sources() == vec![ASSET] && fs::read(root.join(ASSET))? == attachment_bytes(),
        "Legacy attachment reference or bytes did not migrate");
    let trash = store.list_trash();
    let md = trash.iter().find(|entry| entry.title == "Legacy Markdown").context("Missing migrated trash")?;
    let db = rusqlite::Connection::open(root.join(crate::store::SQLITE_FILE))?;
    let (body, _, front) = Store::read_sync_body(&db, &md.id, true)?;
    ensure!(body.plain_text().contains("archived Markdown body") && front.contains("key: legacy"),
        "Markdown front matter or trashed body did not migrate");
    for old in ["docs", "trash", "index.json", "Library", ".restore-source"] {
        ensure!(!root.join(old).exists(), "A legacy store was left in the active library");
    }
    println!("MIGRATION_OK folder=1 rich_note=1 trashed_markdown=1 attachment_bytes={ATTACHMENT_BYTES}");
    Ok(())
}
