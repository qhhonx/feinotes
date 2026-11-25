//! The browser supplies copies of files and the paths below a selected folder.
//! Stage them in a temporary VFS tree for the regular importers.

use std::{collections::{BTreeMap, BTreeSet}, path::{Path, PathBuf}, sync::Arc};
use fdl_core::{browser_import::Kind, model::Document, paths, store::Store, vfs::{self, PathExt as _}};
use wasm_bindgen::{JsCast, JsValue, prelude::wasm_bindgen};
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen(module = "/src/web/file_picker.js")]
extern "C" {
    #[wasm_bindgen(js_name = pickBrowserFiles)]
    fn pick_import(accept: &str, directory: bool, input: &JsValue) -> js_sys::Promise;
}

/// Open the picker while the click still has browser user activation.
pub(crate) fn select(accept: Option<&str>) -> JsFuture {
    JsFuture::from(pick_import(accept.unwrap_or_default(), accept.is_none(), &JsValue::UNDEFINED))
}

pub(crate) struct Staging {
    root: PathBuf,
    separate_roots: bool,
}

impl Staging {
    fn new(separate_roots: bool) -> Arc<Self> {
        let root = PathBuf::from("/.quiet-web-import").join(uuid::Uuid::new_v4().to_string());
        Arc::new(Self { root, separate_roots })
    }

    async fn write(&self, path: &Path, file: web_sys::File) -> Result<(), JsValue> {
        let data = JsFuture::from(file.array_buffer()).await?;
        let parent = path.parent().ok_or_else(invalid_path)?;
        fdl_core::vfs::create_dir_all(parent).and_then(|()| fdl_core::vfs::write(path, js_sys::Uint8Array::new(&data).to_vec()))
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }
}

impl Drop for Staging {
    fn drop(&mut self) { let _ = fdl_core::vfs::remove_dir_all(&self.root); }
}

fn invalid_path() -> JsValue { JsValue::from_str("Invalid selected path") }

/// Stage individual documents as given in the browser's file list.
pub(crate) async fn stage_documents(files: JsFuture) -> Result<Option<(Arc<Staging>, Vec<PathBuf>)>, JsValue> {
    use crate::transfer::{IMPORT_FILE_LIMIT, IMPORT_FOLDER_LIMIT};
    let files = files.await?.dyn_into::<js_sys::Array>()?;
    if files.length() == 0 { return Ok(None); }
    if files.length() as usize > IMPORT_FOLDER_LIMIT { return Err(JsValue::from_str("Too many selected documents")); }
    let staging = Staging::new(false);
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
        let path = staging.root.join(index.to_string()).join(name);
        staging.write(&path, file).await?;
        paths.push(path);
    }
    Ok(Some((staging, paths)))
}

/// Use one directory for all selected files, so relative links work.
pub(crate) async fn stage_folder(files: JsFuture) -> Result<Option<(Arc<Staging>, PathBuf)>, JsValue> {
    let files = files.await?.dyn_into::<js_sys::Array>()?.iter().map(|item| {
        let file = item.dyn_into::<web_sys::File>()?;
        let rel = js_sys::Reflect::get(&file, &"webkitRelativePath".into())?.as_string().ok_or_else(invalid_path)?;
        Ok((rel, file))
    }).collect::<Result<Vec<_>, JsValue>>()?;
    stage_tree(files, false).await
}

pub(crate) async fn stage_drop(files: JsFuture) -> Result<Option<(Arc<Staging>, PathBuf)>, JsValue> {
    let files = files.await?.dyn_into::<js_sys::Array>()?.iter().map(|entry| {
        let pair = entry.dyn_into::<js_sys::Array>()?;
        let rel = pair.get(0).as_string().ok_or_else(invalid_path)?;
        Ok((format!("selection/{rel}"), pair.get(1).dyn_into::<web_sys::File>()?))
    }).collect::<Result<Vec<_>, JsValue>>()?;
    stage_tree(files, true).await
}

async fn stage_tree(selected: Vec<(String, web_sys::File)>, separate_roots: bool) -> Result<Option<(Arc<Staging>, PathBuf)>, JsValue> {
    use crate::transfer::IMPORT_FOLDER_LIMIT;
    if selected.is_empty() { return Ok(None); }
    let mut root = String::new();
    let mut documents = Vec::new();
    let mut files = BTreeMap::new();
    let mut initial = BTreeSet::new();
    let mut images = BTreeSet::new();
    for (rel, file) in selected {
        let parts = fdl_core::paths::selection_parts(&rel).filter(|parts| parts.len() >= 2).ok_or_else(invalid_path)?;
        if root.is_empty() { root = parts[0].to_owned(); }
        if root != parts[0] { return Err(invalid_path()); }
        if parts[1..].iter().any(|p| matches!(*p, "node_modules" | "target")) { continue; }
        let path = Path::new(&rel);
        let hidden = parts[1..].iter().any(|p| p.starts_with('.'));
        let kind = if hidden { None } else { Kind::of(path) };
        let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or_default();
        let image = crate::editor::image::FORMATS.iter().any(|format| ext.eq_ignore_ascii_case(format));
        let settings = if separate_roots && parts.len() > 2 { &parts[2..] } else { &parts[1..] };
        if let Some(kind) = kind {
            initial.insert(rel.clone());
            documents.push((rel.clone(), kind));
        } else if settings == [".obsidian", "app.json"] {
            initial.insert(rel.clone());
        } else if image {
            images.insert(rel.clone());
        }
        if documents.len() > IMPORT_FOLDER_LIMIT { return Err(JsValue::from_str("Too many selected documents")); }
        if files.insert(rel, file).is_some() { return Err(invalid_path()); }
    }
    let staging = Staging::new(separate_roots);
    let dir = staging.root.join(root);
    fdl_core::vfs::create_dir_all(&dir).map_err(|error| JsValue::from_str(&error.to_string()))?;
    if documents.is_empty() { return Ok(Some((staging, dir))); }
    let mut bytes = 0.;
    for rel in initial {
        let file = files.remove(&rel).ok_or_else(invalid_path)?;
        stage_folder_file(&staging, &rel, file, &mut bytes).await?;
    }
    // Obsidian and SiYuan look up images in the selected folder. Let them see
    // its image names first, then read bytes only for references that resolved.
    for rel in &images {
        let path = staging.root.join(rel);
        let parent = path.parent().ok_or_else(invalid_path)?;
        vfs::create_dir_all(parent).and_then(|()| vfs::write(&path, [])).map_err(|e| JsValue::from_str(&e.to_string()))?;
    }
    let mut attachments = BTreeSet::new();
    for (rel, kind) in documents {
        let staged = staging.root.join(&rel);
        let source_root = staging.markdown_root(&rel);
        let doc = match kind {
            Kind::Markdown => crate::transfer::staged_markdown(&staged, &source_root),
            Kind::DocxXml => read_docxxml(&staged),
        };
        let Ok(mut doc) = doc else { continue };
        doc.map_sources(|src| {
            let found = match kind {
                Kind::DocxXml => staging.selected_asset(&rel, src, |p| files.contains_key(p) || staging.root.join(p).fs_is_file()),
                Kind::Markdown => {
                    let path = paths::lexical(Path::new(src));
                    if !path.starts_with(&source_root) { return None; }
                    path.strip_prefix(&staging.root).ok().and_then(Path::to_str)
                        .and_then(|rel| images.get(rel)).cloned()
                }
            };
            if let Some(path) = found { attachments.insert(path); }
            None
        });
    }
    for rel in images.difference(&attachments) {
        vfs::remove_file(staging.root.join(rel)).map_err(|e| JsValue::from_str(&e.to_string()))?;
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
    staging.write(&staging.root.join(rel), file).await
}

fn read_docxxml(path: &Path) -> anyhow::Result<Document> {
    let xml = fdl_core::vfs::read_to_string(path)?;
    Ok(fdl_core::browser_import::parse_docxxml(&xml)?)
}

impl Staging {
    fn markdown_root(&self, rel: &str) -> PathBuf {
        let mut parts = rel.split('/');
        let root = self.root.join(parts.next().unwrap_or_default());
        if self.separate_roots && let (Some(group), Some(_)) = (parts.next(), parts.next()) {
            return root.join(group);
        }
        root
    }

    fn selected_asset(&self, rel: &str, src: &str, mut exists: impl FnMut(&str) -> bool) -> Option<String> {
        let mut dir = fdl_core::paths::selection_parts(rel)?;
        dir.pop()?;
        // The outer staging directory joins the drop into one import operation.
        // A folder's library-root attachments still belong to that folder.
        let prefix = if self.separate_roots && dir.len() > 1 { dir.remove(0) } else { "" };
        fdl_core::paths::selection_attachment_paths(&dir, src, true).into_iter()
            .map(|path| if prefix.is_empty() { path } else { format!("{prefix}/{path}") })
            .find(|path| exists(path))
    }

    pub(crate) fn import_docxxml(&self, store: &mut Store, path: &Path, parent: Option<&str>,
        copied: &mut BTreeMap<String, String>) -> anyhow::Result<String> {
        let rel = path.strip_prefix(&self.root)?.to_str().ok_or_else(|| anyhow::anyhow!("Invalid staged path"))?;
        let mut doc = read_docxxml(path)?;
        let mut result = Ok(());
        doc.map_sources(|src| {
            if result.is_err() { return None; }
            let selected = self.selected_asset(rel, src, |p| self.root.join(p).fs_is_file())?;
            if let Some(path) = copied.get(&selected) { return Some(path.clone()); }
            match store.import_attachment(&self.root.join(&selected)) {
                Ok(path) => { copied.insert(selected, path.clone()); Some(path) }
                Err(error) => { result = Err(error); None }
            }
        });
        result?;
        store.create_from_in(&doc, parent)
    }
}
