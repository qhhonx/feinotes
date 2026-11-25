//! The browser supplies copies of files and the paths below a selected folder.
//! Stage them in a temporary VFS tree for the regular importers.

use std::{collections::{BTreeMap, BTreeSet}, path::{Path, PathBuf}, sync::Arc};
use fdl_core::{model::Document, store::Store, vfs::PathExt as _};
use wasm_bindgen::{JsCast, JsValue, prelude::wasm_bindgen};
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen(inline_js = r#"
export function pickQuietImport(accept, directory) {
  return new Promise((resolve, reject) => {
    const input = document.createElement("input");
    input.type = "file";
    if (directory) {
      if (!("webkitdirectory" in input)) { reject(new Error("Folder selection is unavailable")); return; }
      input.webkitdirectory = true;
    } else {
      input.accept = accept;
      input.multiple = true;
    }
    input.hidden = true;
    const settle = files => { input.remove(); resolve(files); };
    input.onchange = () => settle(Array.from(input.files));
    input.oncancel = () => settle([]);
    document.body.append(input);
    try { if (input.showPicker) input.showPicker(); else input.click(); }
    catch (error) { input.remove(); reject(error); }
  });
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = pickQuietImport)]
    fn pick_import(accept: &str, directory: bool) -> js_sys::Promise;
}

/// Open the picker while the click still has browser user activation.
pub(crate) fn select(accept: Option<&str>) -> JsFuture {
    JsFuture::from(pick_import(accept.unwrap_or_default(), accept.is_none()))
}

pub(crate) struct Staging(PathBuf);

impl Staging {
    fn new() -> Arc<Self> { Arc::new(Self(PathBuf::from("/.quiet-web-import").join(uuid::Uuid::new_v4().to_string()))) }

    async fn write(&self, path: &Path, file: web_sys::File) -> Result<(), JsValue> {
        let data = JsFuture::from(file.array_buffer()).await?;
        let parent = path.parent().ok_or_else(invalid_path)?;
        fdl_core::vfs::create_dir_all(parent).and_then(|()| fdl_core::vfs::write(path, js_sys::Uint8Array::new(&data).to_vec()))
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }
}

impl Drop for Staging {
    fn drop(&mut self) { let _ = fdl_core::vfs::remove_dir_all(&self.0); }
}

fn invalid_path() -> JsValue { JsValue::from_str("Invalid selected path") }

/// Stage individual documents as given in the browser's file list.
pub(crate) async fn stage_documents(files: JsFuture) -> Result<Option<(Arc<Staging>, Vec<PathBuf>)>, JsValue> {
    use crate::transfer::{IMPORT_FILE_LIMIT, IMPORT_FOLDER_LIMIT};
    let files = files.await?.dyn_into::<js_sys::Array>()?;
    if files.length() == 0 { return Ok(None); }
    if files.length() as usize > IMPORT_FOLDER_LIMIT { return Err(JsValue::from_str("Too many selected documents")); }
    let staging = Staging::new();
    let mut paths = Vec::new();
    let mut bytes = 0.;
    for (index, item) in files.iter().enumerate() {
        let file = item.dyn_into::<web_sys::File>()?;
        let name = file.name();
        if !fdl_core::paths::selection_component(&name) { return Err(invalid_path()); }
        bytes += file.size();
        if file.size() > IMPORT_FILE_LIMIT as f64 || bytes > (2 * IMPORT_FILE_LIMIT) as f64 {
            return Err(JsValue::from_str("Selected files exceed the browser import limit"));
        }
        let path = staging.0.join(index.to_string()).join(name);
        staging.write(&path, file).await?;
        paths.push(path);
    }
    Ok(Some((staging, paths)))
}

/// Use one directory for all selected files, so relative links work.
pub(crate) async fn stage_folder(files: JsFuture) -> Result<Option<(Arc<Staging>, PathBuf)>, JsValue> {
    use crate::transfer::{IMPORT_FILE_LIMIT, IMPORT_FOLDER_LIMIT, importable};
    let selected = files.await?.dyn_into::<js_sys::Array>()?;
    if selected.length() == 0 { return Ok(None); }
    let mut root = String::new();
    let mut documents = 0;
    let mut files = BTreeMap::new();
    let mut initial = BTreeSet::new();
    let mut xml = Vec::new();
    for item in selected.iter() {
        let file = item.dyn_into::<web_sys::File>()?;
        let rel = js_sys::Reflect::get(&file, &"webkitRelativePath".into())?.as_string().ok_or_else(invalid_path)?;
        let parts = fdl_core::paths::selection_parts(&rel).filter(|parts| parts.len() >= 2).ok_or_else(invalid_path)?;
        if root.is_empty() { root = parts[0].to_owned(); }
        if root != parts[0] { return Err(invalid_path()); }
        if parts[1..].iter().any(|p| matches!(*p, "node_modules" | "target")) { continue; }
        let path = Path::new(&rel);
        let hidden = parts[1..].iter().any(|p| p.starts_with('.'));
        let doc = !hidden && importable(path);
        let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or_default();
        let image = crate::editor::image::FORMATS.iter().any(|kind| ext.eq_ignore_ascii_case(kind));
        if doc || image || parts[1..] == [".obsidian", "app.json"] { initial.insert(rel.clone()); }
        if doc && ext.eq_ignore_ascii_case("xml") { xml.push(rel.clone()); }
        documents += usize::from(doc);
        if documents > IMPORT_FOLDER_LIMIT { return Err(JsValue::from_str("Too many selected documents")); }
        if files.insert(rel, file).is_some() { return Err(invalid_path()); }
    }
    let staging = Staging::new();
    let dir = staging.0.join(root);
    fdl_core::vfs::create_dir_all(&dir).map_err(|error| JsValue::from_str(&error.to_string()))?;
    if documents == 0 { return Ok(Some((staging, dir))); }
    let mut bytes = 0.;
    for rel in initial {
        let file = files.remove(&rel).ok_or_else(invalid_path)?;
        stage_folder_file(&staging, &rel, file, &mut bytes).await?;
    }
    // Markdown import already knows about images beside or above the page.
    // Other sidecars are copied only when a selected DocxXML names them.
    let mut attachments = BTreeSet::new();
    for rel in xml {
        let Ok(mut doc) = read_docxxml(&staging.0.join(&rel)) else { continue };
        doc.map_sources(|src| {
            if let Some(path) = selected_asset(&rel, src, |p| files.contains_key(p) || staging.0.join(p).fs_is_file()) {
                attachments.insert(path);
            }
            None
        });
    }
    for rel in attachments {
        if let Some(file) = files.remove(&rel) { stage_folder_file(&staging, &rel, file, &mut bytes).await?; }
    }
    Ok(Some((staging, dir)))
}

async fn stage_folder_file(staging: &Staging, rel: &str, file: web_sys::File, bytes: &mut f64) -> Result<(), JsValue> {
    use crate::transfer::IMPORT_FILE_LIMIT;
    *bytes += file.size();
    if file.size() > IMPORT_FILE_LIMIT as f64 || *bytes > (2 * IMPORT_FILE_LIMIT) as f64 {
        return Err(JsValue::from_str("Selected folder exceeds the browser import limit"));
    }
    staging.write(&staging.0.join(rel), file).await
}

fn read_docxxml(path: &Path) -> anyhow::Result<Document> {
    let xml = fdl_core::vfs::read_to_string(path)?;
    Ok(fdl_core::docxml::parse(xml.trim_start_matches('\u{feff}'))?)
}

fn selected_asset(rel: &str, src: &str, exists: impl FnMut(&String) -> bool) -> Option<String> {
    let mut dir = fdl_core::paths::selection_parts(rel)?;
    dir.pop()?;
    fdl_core::paths::selection_attachment_paths(&dir, src, true).into_iter().find(exists)
}

impl Staging {
    pub(crate) fn import_docxxml(&self, store: &mut Store, path: &Path, parent: Option<&str>,
        copied: &mut BTreeMap<String, String>) -> anyhow::Result<String> {
        let rel = path.strip_prefix(&self.0)?.to_str().ok_or_else(|| anyhow::anyhow!("Invalid staged path"))?;
        let mut doc = read_docxxml(path)?;
        let mut result = Ok(());
        doc.map_sources(|src| {
            if result.is_err() { return None; }
            let selected = selected_asset(rel, src, |p| self.0.join(p).fs_is_file())?;
            if let Some(path) = copied.get(&selected) { return Some(path.clone()); }
            match store.import_attachment(&self.0.join(&selected)) {
                Ok(path) => { copied.insert(selected, path.clone()); Some(path) }
                Err(error) => { result = Err(error); None }
            }
        });
        result?;
        store.create_from_in(&doc, parent)
    }
}
