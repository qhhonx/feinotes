// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Personal folder placements share the private catalogue's CRDT and transaction.
use anyhow::Result;
use loro::LoroDoc;
use rusqlite::{Connection, OptionalExtension, params};
use crate::sharing::organizer::{KEY, Organizer, Placement};

fn read(db: &Connection, key: &str) -> Result<Organizer> {
    let bytes: Option<Vec<u8>> = db.query_row("SELECT value FROM library_values WHERE key=?1", [key], |row| row.get(0)).optional()?;
    Ok(bytes.map(|bytes| serde_json::from_slice(&bytes)).transpose()?.unwrap_or_default())
}

pub(super) fn capture(db: &Connection, doc: &LoroDoc) -> Result<()> {
    let current = read(db, KEY)?;
    let seen = read(db, "shared-personal-folders-seen")?;
    let map = doc.get_map(KEY);
    for (key, value) in &current.0 {
        if Some(value) != seen.0.get(key) { map.insert(key, serde_json::to_string(value)?)?; }
    }
    for key in seen.0.keys().filter(|key| !current.0.contains_key(*key)) { map.delete(key)?; }
    Ok(())
}

pub(super) fn project(db: &Connection, doc: &LoroDoc) -> Result<()> {
    let map = doc.get_map(KEY);
    let mut projected = Organizer::default();
    for key in map.keys() {
        if let Some(raw) = super::content::value(&map, &key) {
            projected.0.insert(key.to_string(), serde_json::from_str::<Placement>(&raw)?);
        }
    }
    if projected == read(db, KEY)? && projected == read(db, "shared-personal-folders-seen")? { return Ok(()); }
    let bytes = serde_json::to_vec(&projected)?;
    for key in [KEY, "shared-personal-folders-seen"] {
        db.execute("INSERT OR REPLACE INTO library_values(key,value) VALUES (?1,?2)", params![key, bytes])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use loro::ExportMode;

    #[test]
    fn personal_folder_placements_merge_by_item_across_devices() -> Result<()> {
        let device = || -> Result<Connection> {
            let db = Connection::open_in_memory()?;
            db.execute_batch("CREATE TABLE library_values(key TEXT PRIMARY KEY, value BLOB NOT NULL)")?;
            Ok(db)
        };
        let a = device()?; let b = device()?;
        let left = LoroDoc::new(); left.set_peer_id(1)?;
        let right = LoroDoc::new(); right.set_peer_id(2)?;
        let mut tree = Organizer::default();
        tree.0.insert("collection-team".into(), Placement { title: Some("My folder".into()), parent: None });
        tree.place("first", Some("collection-team"))?;
        assert!(tree.place("collection-team", Some("collection-team")).is_err());
        let write = |db: &Connection, tree: &Organizer| -> Result<()> {
            db.execute("INSERT OR REPLACE INTO library_values VALUES(?1,?2)", params![KEY, serde_json::to_vec(tree)?])?; Ok(())
        };
        write(&a, &tree)?; capture(&a, &left)?; project(&a, &left)?;
        right.import(&left.export(ExportMode::Snapshot)?)?; project(&b, &right)?;
        let mut other = read(&b, KEY)?;
        assert_eq!(other.0["first"].parent.as_deref(), Some("collection-team"));
        tree.place("second", Some("collection-team"))?; write(&a,&tree)?; capture(&a,&left)?;
        other.place("third", Some("collection-team"))?; write(&b,&other)?; capture(&b,&right)?;
        left.import(&right.export(ExportMode::Snapshot)?)?; project(&a,&left)?;
        let merged = read(&a, KEY)?;
        for id in ["first","second","third"] { assert_eq!(merged.0[id].parent.as_deref(), Some("collection-team")); }
        Ok(())
    }
}
