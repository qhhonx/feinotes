//! Prepare only the selected document's local, lazily loaded assets.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use fdl_core::{board::types::FileData, markdown, model::Document, paths, store, vfs::{self, PathExt as _}};
use std::{collections::BTreeSet, path::{Path, PathBuf}, time::Duration};

pub(crate) fn request_assets(doc: &Document, root: &Path) -> BTreeSet<PathBuf> {
    request_sources(doc.asset_sources().iter().map(String::as_str), root)
}

pub(crate) fn request_sources<'a>(sources: impl IntoIterator<Item = &'a str>, root: &Path) -> BTreeSet<PathBuf> {
    sources.into_iter().filter_map(|src| {
        if store::export_path_in(root, src).is_some() { return None; }
        let raw = src.strip_prefix("file://").unwrap_or(src);
        if raw.is_empty() || raw.contains("://") || raw.starts_with("data:") { return None; }
        let path = paths::lexical(&paths::resolve(raw, root));
        (path.starts_with(root) && vfs::pending(&path)).then_some(path)
    }).collect()
}

pub(crate) fn encoded_image(src: &str, root: &Path) -> Option<String> {
    let path = store::export_path_in(root, src)?;
    let bytes = store::read_bounded(&path, 20 * 1024 * 1024).ok()?;
    Some(STANDARD.encode(bytes))
}

pub(crate) fn board_image(file: &FileData, root: &Path) -> Option<String> {
    crate::html::browser_board_image(file, &|src| encoded_image(src, root))
}

pub(crate) async fn wait_for_assets(mut pending: BTreeSet<PathBuf>, executor: &gpui::BackgroundExecutor) {
    // A stalled host must not hold up every other file in the export.
    let started = web_time::Instant::now();
    pending.retain(|path| !path.fs_is_file() && vfs::pending(path));
    while !pending.is_empty() && started.elapsed() < Duration::from_secs(10) {
        executor.timer(Duration::from_millis(100)).await;
        pending.retain(|path| !path.fs_is_file() && vfs::pending(path));
    }
}

pub(crate) fn pack(doc: &Document, root: &Path, subs: &[String], title: &str) -> anyhow::Result<markdown::archive::Archive> {
    let resolve = |src: &str| store::export_path_in(root, src);
    let read = |path: &Path| store::read_bounded(path, store::EXPORT_FILE_LIMIT);
    let board_asset = |file: &FileData| {
        if let Some(data) = &file.data_url { return Some(data.clone()); }
        let path = resolve(file.path.as_deref()?)?;
        let bytes = read(&path).ok()?;
        Some(format!("data:{};base64,{}", file.mime_type, STANDARD.encode(bytes)))
    };
    markdown::archive::pack(doc, title, markdown::Options {
        sub_pages: subs, library_dir: Some(root), archive_asset: Some(&resolve), board_asset: Some(&board_asset), ..Default::default()
    }, read)
}
