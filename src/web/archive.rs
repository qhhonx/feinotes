//! Prepare only the selected document's local, lazily loaded assets.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use fdl_core::{board::types::FileData, markdown, model::{BlockKind, Document}, paths, store, vfs::{self, PathExt as _}};
use std::{collections::BTreeSet, path::{Path, PathBuf}, time::Duration};

pub(crate) fn request_assets(doc: &Document, root: &Path) -> BTreeSet<PathBuf> {
    let mut pending = BTreeSet::new();
    let mut request = |src: &str| {
        if store::export_path_in(root, src).is_some() { return; }
        let raw = src.strip_prefix("file://").unwrap_or(src);
        if raw.is_empty() || raw.contains("://") || raw.starts_with("data:") { return; }
        let path = paths::lexical(&paths::resolve(raw, root));
        if path.starts_with(root) && vfs::pending(&path) { pending.insert(path); }
    };
    for src in doc.asset_sources() { request(&src); }
    doc.for_each_block(|block| if let BlockKind::Board(board) = &block.kind {
        for file in board.files.values().filter(|f| f.data_url.is_none()) {
            if let Some(src) = &file.path { request(src); }
        }
    });
    pending
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
