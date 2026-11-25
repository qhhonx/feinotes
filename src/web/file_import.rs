//! The browser supplies copies of files and the paths below a selected folder.
//! Stage them in a temporary VFS tree for the regular importers.

use std::{path::{Path, PathBuf}, sync::Arc};
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

/// Use one directory for all selected files, so relative image links work.
pub(crate) async fn stage_folder(files: JsFuture) -> Result<Option<(Arc<Staging>, PathBuf)>, JsValue> {
    use crate::transfer::{IMPORT_FILE_LIMIT, IMPORT_FOLDER_LIMIT, importable};
    let files = files.await?.dyn_into::<js_sys::Array>()?;
    if files.length() == 0 { return Ok(None); }
    let mut root = String::new();
    let mut bytes = 0.;
    let mut documents = 0;
    let mut selected = Vec::new();
    for item in files.iter() {
        let file = item.dyn_into::<web_sys::File>()?;
        let rel = js_sys::Reflect::get(&file, &"webkitRelativePath".into())?.as_string().ok_or_else(invalid_path)?;
        let parts = fdl_core::paths::selection_parts(&rel).filter(|parts| parts.len() >= 2).ok_or_else(invalid_path)?;
        if root.is_empty() { root = parts[0].to_owned(); }
        if root != parts[0] { return Err(invalid_path()); }
        if parts[1..].iter().any(|p| matches!(*p, "node_modules" | "target")) { continue; }
        let path = Path::new(&rel);
        let hidden = parts[1..].iter().any(|p| p.starts_with('.'));
        let doc = !hidden && importable(path);
        let image = path.extension().and_then(|ext| ext.to_str())
            .is_some_and(|ext| crate::editor::image::FORMATS.iter().any(|kind| ext.eq_ignore_ascii_case(kind)));
        if !doc && !image && parts[1..] != [".obsidian", "app.json"] { continue; }
        documents += usize::from(doc);
        bytes += file.size();
        if documents > IMPORT_FOLDER_LIMIT || file.size() > IMPORT_FILE_LIMIT as f64 || bytes > (2 * IMPORT_FILE_LIMIT) as f64 {
            return Err(JsValue::from_str("Selected folder exceeds the browser import limit"));
        }
        selected.push((rel, file));
    }
    let staging = Staging::new();
    let dir = staging.0.join(root);
    fdl_core::vfs::create_dir_all(&dir).map_err(|error| JsValue::from_str(&error.to_string()))?;
    if documents > 0 {
        for (rel, file) in selected { staging.write(&staging.0.join(rel), file).await?; }
    }
    Ok(Some((staging, dir)))
}
