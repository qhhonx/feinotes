//! Undoing and bringing in: a document's 版本历史 (every save keeps the
//! text before it), the 回收站, and files from outside the library.

use std::path::{Path, PathBuf};

use fdl_core::store::Store;
use serde_json::{Value, json};

use crate::fail::{R, fail};
use crate::session::{Pending, Session, arg, date_ms, diff_lines, need};

/// Bigger files are not documents (as the app's 导入).
const IMPORT_LIMIT: u64 = 32 * 1024 * 1024;

impl Session {
    /// The version `spec` names among `id`'s: its file name, or `1` for the
    /// newest, `2` the one before…
    fn version(&mut self, id: &str, spec: &str) -> R<fdl_core::versions::Version> {
        let list = self.lib_of(id)?.versions(id);
        let s = spec.trim().trim_start_matches('@');
        let hit = match s.parse::<usize>() {
            Ok(n) if n >= 1 && n <= list.len() && s.len() < 6 => list.get(n - 1).cloned(),
            _ => list.iter().find(|v| v.name == s || v.label.as_deref() == Some(s)).cloned(),
        };
        hit.ok_or_else(|| fail("not_found", format!("no version {spec:?} ({} kept)", list.len())).hint("`quiet history DOC` lists them; 1 is the newest"))
    }

    /// Versions of a document, newest first; with `version`, what changed from it to now.
    pub(crate) fn history(&mut self, a: &Value) -> R<Value> {
        let i = self.doc_arg(a)?;
        let id = self.saved_id(i)?;
        let path = self.docs[i].path.clone();
        if let Some(spec) = arg(a, "version") {
            let v = self.version(&id, spec)?;
            let lines = self.lib_of(&id)?.version_diff(&id, &v.name, None).map_err(|e| fail("io", format!("{e:#}")))?;
            return Ok(json!({ "path": path, "version": v.name, "time": date_ms(v.ms), "label": v.label, "diff": diff_lines(&lines) }));
        }
        // What the save after each version changed: from it to the next newer one (or now).
        let store = self.lib_of(&id)?;
        let all = store.versions(&id);
        let list: Vec<Value> = all.iter().enumerate().map(|(n, v)| {
            let newer = n.checked_sub(1).map(|k| all[k].name.as_str());
            let lines = store.version_diff(&id, &v.name, newer).unwrap_or_default();
            let (add, del) = lines.iter().fold((0, 0), |(a, d), l| match l.kind { fdl_core::diff::Kind::Added => (a + 1, d), fdl_core::diff::Kind::Removed => (a, d + 1), _ => (a, d) });
            json!({ "n": n + 1, "version": v.name, "time": date_ms(v.ms), "size": v.size, "label": v.label, "then": format!("+{add} -{del}") })
        }).collect();
        Ok(json!({ "path": path, "versions": list }))
    }

    /// Put a document back as it was at a version; the text it has now is kept as a version first.
    pub(crate) fn restore(&mut self, a: &Value) -> R<Value> {
        let i = self.doc_arg(a)?;
        let id = self.saved_id(i)?;
        let v = self.version(&id, need(a, "version")?)?;
        if self.docs[i].kind == "table" {
            self.pending.push(Pending::RestoreTable { id, version: v.name.clone() });
            return Ok(json!({ "path": self.docs[i].path, "restore": v.name, "time": date_ms(v.ms) }));
        }
        let doc = self.lib_of(&id)?.version_document(&id, &v.name).map_err(|e| fail("io", format!("{e:#}")))?;
        let d = &mut self.docs[i];
        if d.doc != doc { d.doc = doc; d.dirty = true; d.keep_version = true; }
        Ok(json!({ "path": d.path, "restore": v.name, "time": date_ms(v.ms) }))
    }

    /// Every top-level folder's 回收站 (each top-level folder keeps its own), as (top-level folder key, entry).
    fn all_trash(&mut self) -> Vec<(String, fdl_core::store::TrashEntry)> {
        self.ws.folders.iter_mut().filter_map(|r| Some((r.key.clone(), r.store.as_mut()?)))
            .flat_map(|(k, s)| s.list_trash().into_iter().map(move |t| (k.clone(), t))).collect()
    }

    pub(crate) fn trash_list(&mut self) -> R<Value> {
        let list: Vec<Value> = self.all_trash().iter().map(|(k, t)| json!({ "id": t.id, "title": t.title, "folder": self.top_name(k), "deleted": date_ms(t.deleted * 1000) })).collect();
        Ok(json!({ "days": fdl_core::store::TRASH_DAYS, "items": list }))
    }

    /// Bring a document back from the 回收站 (by title or id), when the command or script ends.
    pub(crate) fn untrash(&mut self, a: &Value) -> R<Value> {
        let spec = need(a, "name")?.trim();
        let list = self.all_trash();
        let hits: Vec<_> = list.iter().filter(|(_, t)| t.id == spec || t.title == spec).collect();
        let hits = if hits.is_empty() { list.iter().filter(|(_, t)| t.title.to_lowercase().contains(&spec.to_lowercase())).collect() } else { hits };
        match hits.as_slice() {
            // Back into the top-level folder whose 回收站 it is in.
            [(k, t)] => { self.pending.push(Pending::Untrash { top: k.clone(), id: t.id.clone(), title: t.title.clone() }); Ok(json!({ "title": t.title, "folder": self.top_name(k), "untrash": true })) }
            [] => Err(fail("not_found", format!("nothing called {spec:?} in the trash")).hint("`quiet trash` lists it")),
            many => Err(fail("ambiguous", format!("{} items match {spec:?}: {}", many.len(), many.iter().map(|(k, t)| format!("{} ({}, in {})", t.title, t.id, self.top_name(k))).collect::<Vec<_>>().join(", "))).hint("pass the id")),
        }
    }

    /// A file from outside the library, brought in when the command ends.
    pub(crate) fn import(&mut self, a: &Value) -> R<Value> {
        if self.sandbox { return Err(fail("sandbox", "a script cannot read files outside the library").hint("run `quiet import FILE` from the shell")); }
        let path = PathBuf::from(need(a, "path")?);
        if !path.is_file() { return Err(fail("not_found", format!("no file {}", path.display()))); }
        kind_of_file(&path)?;
        let (top, folder) = self.landing(arg(a, "folder"))?;
        let to = self.folder_rel(folder.as_deref());
        let shown = self.home_path(&top, &to);
        self.pending.push(Pending::Import { top, path: path.clone(), folder: Some(to).filter(|t| !t.is_empty()) });
        Ok(json!({ "path": path.display().to_string(), "import": true, "to": shown }))
    }
}

enum FileKind { Markdown, DocxXml, Csv, Board }

fn kind_of_file(p: &Path) -> R<FileKind> {
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    Ok(match ext.as_str() {
        "md" | "markdown" | "mark" | "txt" => FileKind::Markdown,
        "xml" => FileKind::DocxXml,
        "csv" | "tsv" => FileKind::Csv,
        "excalidraw" => FileKind::Board,
        _ => return Err(fail("unsupported", format!("cannot import .{ext} files")).hint("Markdown (.md, .txt), DocxXML (.xml), CSV/TSV and .excalidraw are imported")),
    })
}

/// `path` as a new document in folder `parent` (as the app's 导入): Markdown
/// stays Markdown with its pictures copied in, CSV becomes a 数据表.
pub(crate) fn import_file(store: &mut Store, top: &Path, path: &Path, parent: Option<&str>) -> R<String> {
    let bytes = fdl_core::store::read_bounded(path, IMPORT_LIMIT)?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{feff}');
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().trim().to_string();
    let made = match kind_of_file(path)? {
        FileKind::Markdown => {
            let mut doc = fdl_core::markdown::parse_file(text, &stem, path.parent());
            if let Some(dir) = path.parent() { fdl_core::markdown::wiki_images(&mut doc.blocks, dir, None); }
            // Its pictures copied in, as the app's 导入 does.
            let assets = Store::assets_in(top);
            let mut n = 0;
            let mut failed = None;
            crate::blocks::each_block(&mut doc.blocks, &mut |b| {
                let fdl_core::model::BlockKind::Image { src, .. } = &mut b.kind else { return };
                let Some((from, to)) = picture_copy(top, &assets, src, n) else { return };
                n += 1;
                match to.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::copy(&from, &to)) {
                    Ok(_) => *src = to.to_string_lossy().into_owned(),
                    Err(e) => { failed.get_or_insert(format!("{}: {e}", from.display())); }
                }
            });
            if let Some(e) = failed { return Err(fail("io", e)); }
            store.create_markdown_in(&doc, fdl_core::markdown::front_matter(text).0, parent)
        }
        FileKind::DocxXml => store.import_into(path, parent),
        FileKind::Csv => {
            let mut base = fdl_core::table::Base::default();
            let t = fdl_core::table::csv::import(&mut base, &stem, text);
            base.tables.push(t);
            store.create_table_in(&stem, &base, parent)
        }
        FileKind::Board => {
            let board = fdl_core::whiteboard::Board::from_file(text).map_err(|e| fail("parse", format!("{}: {e:#}", path.display())))?;
            store.create_board_in(&stem, &board, parent)
        }
    };
    made.map_err(|e| fail("io", format!("{}: {e:#}", path.display())))
}

/// A picture a block shows from outside the library (`src` an absolute path
/// or `file://` URL to an image that exists), and where its copy goes in
/// `assets`: `<8 hex>-<name>.<ext>`, as the app names them. `n` tells copies apart.
pub(crate) fn picture_copy(top: &Path, assets: &Path, src: &str, n: usize) -> Option<(PathBuf, PathBuf)> {
    let from = PathBuf::from(fdl_core::markdown::percent_decode(src.strip_prefix("file://").unwrap_or(src)));
    if !from.is_absolute() || from.starts_with(top) { return None; }
    let ext = from.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if !["jpeg", "jpg", "png", "gif", "webp", "bmp"].contains(&ext.as_str()) || !from.is_file() { return None; }
    if from.metadata().map(|m| m.len()).unwrap_or(0) > IMPORT_LIMIT { return None; }
    let stem: String = from.file_stem().and_then(|s| s.to_str()).unwrap_or("image").chars().map(|c| if matches!(c, '/' | '\\' | ':') || c.is_control() { '_' } else { c }).take(80).collect();
    let stem = stem.trim().trim_start_matches('.');
    let tag = sha1_smol::Sha1::from(format!("{}\0{n}\0{}", from.display(), fdl_core::time::now_ms())).digest().to_string();
    Some((from.clone(), assets.join(format!("{}-{}.{ext}", &tag[..8], if stem.is_empty() { "image" } else { stem }))))
}
