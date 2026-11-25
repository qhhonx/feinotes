// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! The catalogue is a CRDT of entries. Life cycle, position and individual
//! metadata fields merge separately from the character operations in each note.

use anyhow::{Result, ensure};
use loro::{LoroDoc, LoroMap, VersionVector};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use crate::store::{Meta, Store, Trashed};
use super::{content, journal};

pub const SCOPE: &str = "00000000-0000-0000-0000-000000000001";
// Internal projection key. The wire format uses a separate Loro map, also read by Web.
const LIST_TITLE: &str = "list-title";
type Fields = BTreeMap<String, Value>;
pub type Catalogue = BTreeMap<String, Fields>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Life {
    Live,
    Trash { deleted: u64, root: Option<String> },
    Purged,
}

fn purge_marker(doc: &LoroDoc, id: &str) -> bool {
    doc.get_map("purged-entries").get(id).is_some()
}

fn mark_purged(doc: &LoroDoc, id: &str, version: Option<VersionVector>) -> Result<()> {
    // A restore or metadata edit can never erase this separate marker.
    if !purge_marker(doc, id) {
        let markers = doc.get_map("purged-entries");
        if let Some(version) = version { markers.insert(id, serde_json::to_string(&version)?)?; }
        else { markers.insert(id, true)?; }
    }
    Ok(())
}

pub(super) fn permanently_deleted(doc: &LoroDoc, id: &str) -> bool {
    purge_marker(doc, id) || content::value(&doc.get_map(format!("entry-{id}")), "life").as_deref() == Some("\"Purged\"")
}

pub(super) fn deleted_version(doc: &LoroDoc, id: &str) -> Result<Option<VersionVector>> {
    Ok(content::value(&doc.get_map("purged-entries"), id)
        .map(|clock| serde_json::from_str(&clock)).transpose()?)
}

fn fields(meta: &Meta, life: Life) -> Result<Fields> {
    let mut fields = serde_json::from_value::<Fields>(serde_json::to_value(meta)?)?;
    fields.remove("opened");
    fields.remove("modified");
    fields.remove("path");
    fields.remove("name"); // Markdown title lives with its body.
    fields.remove("parent");
    fields.remove("order");
    fields.remove("root_order");
    fields.insert("location".into(), json!([meta.parent, meta.order, meta.top_order]));
    fields.insert("life".into(), json!(life));
    Ok(fields)
}

pub fn local(db: &Connection) -> Result<Catalogue> {
    let mut result = Catalogue::new();
    for (table, trashed) in [("library_entries", false), ("library_trash", true)] {
        let mut stmt = db.prepare(&format!("SELECT e.id, e.meta, d.title FROM {table} e LEFT JOIN library_documents d ON e.id = d.id ORDER BY e.id"))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let raw: String = row.get(1)?;
            let (meta, life) = if trashed {
                let Trashed { meta, deleted, root } = serde_json::from_str(&raw)?;
                (meta, Life::Trash { deleted, root })
            } else {
                (serde_json::from_str(&raw)?, Life::Live)
            };
            if !meta.local() {
                let mut entry = fields(&meta, life)?;
                if let Some(title) = row.get::<_, Option<String>>(2)? {
                    entry.insert(LIST_TITLE.to_string(), json!(title));
                }
                result.insert(id, entry);
            }
        }
    }
    Ok(result)
}

pub fn capture(db: &Connection, doc: &LoroDoc, old: &Catalogue, new: &Catalogue) -> Result<()> {
    let index = doc.get_map("entries");
    let titles = doc.get_map("list-titles");
    for (id, current) in new {
        let title = current.get(LIST_TITLE).and_then(Value::as_str);
        let title_missing = title.is_some() && titles.get(id).is_none();
        if old.get(id) == Some(current) && !title_missing { continue; }
        ensure!(uuid::Uuid::parse_str(id).is_ok(), "Invalid library entry ID");
        let entry = doc.get_map(format!("entry-{id}"));
        if permanently_deleted(doc, id) {
            mark_purged(doc, id, None)?;
            continue;
        }
        if index.get(id).is_none() { index.insert(id, true)?; }
        if let Some(title) = title
            && (title_missing || old.get(id).and_then(|entry| entry.get(LIST_TITLE)).and_then(Value::as_str) != Some(title)) {
            titles.insert(id, title)?;
        }
        for (key, value) in current {
            if key != LIST_TITLE && old.get(id).and_then(|entry| entry.get(key)) != Some(value) {
                entry.insert(key, serde_json::to_string(value)?)?;
            }
        }
        if let Some(previous) = old.get(id) {
            for key in previous.keys().filter(|key| key.as_str() != LIST_TITLE && !current.contains_key(*key)) {
                entry.delete(key)?;
            }
        }
    }
    for id in old.keys().filter(|id| !new.contains_key(*id)) {
        if titles.get(id).is_some() { titles.delete(id)?; }
        capture_purge(db, doc, id)?;
    }
    Ok(())
}

pub(super) fn capture_purge(db: &Connection, doc: &LoroDoc, id: &str) -> Result<()> {
    mark_purged(doc, id, journal::note_version(db, id)?)?;
    let entry = doc.get_map(format!("entry-{id}"));
    if content::value(&entry, "life").as_deref() != Some("\"Purged\"") {
        entry.insert("life", serde_json::to_string(&Life::Purged)?)?;
    }
    Ok(())
}

fn read_fields(map: &LoroMap) -> Result<Fields> {
    let mut result = Fields::new();
    for key in map.keys() {
        let value = map.get(&key).and_then(|value| value.into_value().ok());
        if let Some(text) = value.as_ref().and_then(|value| value.as_string()) {
            result.insert(key.to_string(), serde_json::from_str(text.as_str())?);
        }
    }
    Ok(result)
}

pub fn project(db: &Connection, doc: &LoroDoc, shared_folder: bool) -> Result<bool> {
    let mut entries = BTreeMap::new();
    for id in doc.get_map("entries").keys() {
        let id = id.to_string();
        ensure!(uuid::Uuid::parse_str(&id).is_ok(), "Invalid synchronized entry ID");
        let mut fields = read_fields(&doc.get_map(format!("entry-{id}")))?;
        let life = fields.remove("life").unwrap_or(Value::Null);
        let life: Life = if purge_marker(doc, &id) { Life::Purged } else { serde_json::from_value(life)? };
        if life == Life::Purged { mark_purged(doc, &id, None)?; }
        let (parent, order, root_order) = serde_json::from_value(fields.remove("location").unwrap_or(json!([null, 0, null])))?;
        fields.insert("modified".into(), fields.get("created").cloned().unwrap_or(json!(0)));
        let mut meta: Meta = serde_json::from_value(serde_json::to_value(fields)?)?;
        if shared_folder {
            ensure!(meta.path.is_none() && meta.rel.is_empty() && meta.share_root.is_none(), "Invalid private metadata in shared folder");
        }
        meta.parent = parent;
        meta.order = order;
        meta.top_order = root_order;
        if meta.folder.is_none() && life != Life::Purged {
            let body: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM library_documents WHERE id = ?1)", [&id], |row| row.get(0))?;
            let held: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM notes WHERE id = ?1 AND held <> 0)", [&id], |row| row.get(0))?;
            if held { continue; }
            if !body && (journal::has_conflict(db, &id)? || !journal::rebuild_body(db, &id, life != Life::Live)?) { continue; }
        }
        entries.insert(id, (meta, life));
    }
    // Keep raw CRDT positions intact. A missing parent can arrive later.
    // Cut each concurrent move cycle at the lexicographically smallest ID.
    let mut roots = BTreeSet::new();
    for id in entries.keys() {
        let mut path: Vec<&String> = Vec::new();
        let mut next = id;
        while let Some((meta, Life::Live)) = entries.get(next) {
            if let Some(start) = path.iter().position(|item| *item == next) {
                roots.insert(path[start..].iter().min().unwrap().to_string());
                break;
            }
            path.push(next);
            let Some(parent) = &meta.parent else { break; };
            if !matches!(entries.get(parent), Some((_, Life::Live))) {
                roots.insert(next.to_string());
                break;
            }
            next = parent;
        }
    }
    let mut changed = false;
    for (id, (mut meta, life)) in entries {
        if life == Life::Purged && journal::has_conflict(db, &id)? { continue; }
        let (table, other) = if life == Life::Live { ("library_entries", "library_trash") }
            else { ("library_trash", "library_entries") };
        changed |= db.execute(&format!("DELETE FROM {other} WHERE id = ?1"), [&id])? != 0;
        if life == Life::Purged {
            changed |= db.execute("DELETE FROM library_trash WHERE id = ?1", [&id])? != 0;
            let version = deleted_version(doc, &id)?;
            journal::archive_body(db, &id, version.as_ref(), meta.markdown, shared_folder.then(|| doc.peer_id()))?;
            db.execute("DELETE FROM outbox WHERE note = ?1", [&id])?;
            Store::remove_sync_body(db, &id)?;
            continue;
        }
        if roots.contains(&id) { meta.parent = None; }
        let stored: Option<String> = db.query_row(&format!("SELECT meta FROM {table} WHERE id = ?1"), [&id], |row| row.get(0))
            .optional()?;
        let old = stored.as_ref().map(|raw| -> Result<Meta> {
            Ok(if life == Life::Live { serde_json::from_str(raw)? }
                else { serde_json::from_str::<Trashed>(raw)?.meta })
        }).transpose()?;
        if let Some(old) = &old {
            meta.opened = old.opened;
            meta.modified = old.modified;
        } else { meta.modified = meta.created; }
        if meta.markdown {
            meta.name = db.query_row("SELECT title FROM library_documents WHERE id = ?1", [&id], |row| row.get(0)).optional()?;
        }
        let trashed = life != Life::Live;
        let raw = match life {
            Life::Live => serde_json::to_string(&meta)?,
            Life::Trash { deleted, root } => serde_json::to_string(&Trashed { meta, deleted, root })?,
            Life::Purged => unreachable!(),
        };
        if stored.as_deref() != Some(&raw) {
            db.execute(&format!("INSERT OR REPLACE INTO {table} VALUES (?1, ?2)"), params![id, raw])?;
            changed = true;
        }
        changed |= db.execute("UPDATE library_documents SET trashed = ?2 WHERE id = ?1 AND trashed <> ?2",
            params![id, trashed])? != 0;
    }
    Ok(changed)
}
