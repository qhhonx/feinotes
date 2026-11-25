//! 导入 / 导出: 下载为 → PDF / Markdown (with the 导出 Markdown 设置
//! dialog), 打印, and 导入为文档 (主页's 上传, the app menu): local Markdown
//! or DocxXML files picked straight away. The Word / PDF settings and
//! 打印选项 only choose what happens to comments, so they are left out.

use fdl_core::vfs::KindExt as _;
#[cfg(not(target_os = "ios"))]
use fdl_core::vfs::PathExt as _;
use crate::app::Workspace;
use crate::settings::display_title;
use crate::model::{Block, BlockKind, Document, RichText};
use crate::printing::{self, Job};
use crate::design::{Button, Dialog, scrim};
use crate::ui::{TipExt, ToastKind};
use gpui::{prelude::FluentBuilder, *};
use std::path::{Path, PathBuf};

/// The open 导出 Markdown 设置 dialog: the page, and 所有内容 picked.
pub struct ExportMd {
    pub id: String,
    pub all: bool,
}

const MARKDOWN_EXTS: [&str; 4] = ["md", "markdown", "mark", "txt"];

/// What the header's 导入 menu picks.
#[derive(Clone, Copy)]
pub(crate) enum ImportKind { Markdown, Folder, DocxXml, Csv }

/// 导入文件夹 refuses folders with more documents than this.
pub(crate) const IMPORT_FOLDER_LIMIT: usize = 2000;
/// Bigger files are not documents.
const IMPORT_FILE_LIMIT: u64 = 32 * 1024 * 1024;

/// What to do with a folder 导入文件夹 brought in (a drop puts it in its slot).
pub(crate) type AfterImport = Box<dyn FnOnce(&mut Workspace, &str)>;

/// What a background 导入文件夹 did.
/// `cloud`: what was skipped as only in iCloud (`Scanned::cloud`).
enum Imported { Empty { cloud: usize }, TooMany(usize), Done { top: Option<String>, failed: usize, cloud: usize, saved: anyhow::Result<()>, changes: Vec<crate::store::FileDelta> } }
struct ImportedFiles { last: Option<String>, failed: bool, changes: Vec<crate::store::FileDelta> }

/// A folder to import: its documents and the sub-folders that hold some;
/// `cloud`: the documents and folders under it left out as only in iCloud.
pub(crate) struct Scanned { pub(crate) name: String, dir: PathBuf, dirs: Vec<Scanned>, docs: Vec<PathBuf>, pub(crate) files: usize, pub(crate) cloud: usize }

/// Markdown and DocxXML files under `dir`, by name; hidden folders,
/// dependency folders and symlinks are skipped, and so is what is only in
/// iCloud (`cloud_only`), without starting its download.
pub(crate) fn scan_folder(dir: &Path, depth: usize) -> Option<Scanned> {
    let name = dir.file_name()?.to_string_lossy().into_owned();
    let mut t = Scanned { name, dir: dir.to_path_buf(), dirs: vec![], docs: vec![], files: 0, cloud: 0 };
    if cloud_only(dir) { t.cloud = 1; return Some(t); }
    if depth > 32 { return Some(t); }
    let mut items: Vec<_> = fdl_core::vfs::read_dir(dir).ok()?.flatten().collect();
    items.sort_by_key(|e| e.file_name().to_string_lossy().to_lowercase());
    for e in items {
        let p = e.path();
        let n = e.file_name().to_string_lossy().into_owned();
        let Ok(ft) = e.file_type() else { continue };
        if n.starts_with('.') || ft.is_symlink() {
            // An older macOS's stand-in for a note left in iCloud: `.Note.md.icloud`.
            if n.strip_prefix('.').and_then(|n| n.strip_suffix(".icloud")).is_some_and(|n| importable(Path::new(n))) { t.cloud += 1; }
            continue;
        }
        if ft.fs_is_dir() {
            if matches!(n.as_str(), "node_modules" | "target") { continue; }
            if let Some(sub) = scan_folder(&p, depth + 1) {
                t.cloud += sub.cloud;
                if sub.files > 0 { t.files += sub.files; t.dirs.push(sub); }
            }
        } else if importable(&p) {
            if cloud_only(&p) { t.cloud += 1; continue; }
            t.docs.push(p);
            t.files += 1;
        }
        if t.files > IMPORT_FOLDER_LIMIT { break; }
    }
    Some(t)
}

fn importable(p: &Path) -> bool { crate::local_files::is_markdown_path(p) || p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xml")) }

/// Kept only in iCloud (or another File Provider): 优化存储 took the file or
/// folder off this Mac (#352) and reading it would start a download, which
/// can take hours. SF_DATALESS (<sys/stat.h>) says so without reading.
#[cfg(target_os = "macos")]
fn cloud_only(p: &Path) -> bool {
    use std::os::macos::fs::MetadataExt;
    std::fs::symlink_metadata(p).is_ok_and(|m| m.st_flags() & 0x4000_0000 != 0)
}
#[cfg(not(target_os = "macos"))]
fn cloud_only(_: &Path) -> bool { false }

/// While alive, files only in iCloud fail to read on this thread instead of
/// downloading: around reading what a folder import brings in (a note's
/// pictures, the vault's settings), never around the library's own files.
struct NoCloudDownloads { #[cfg(target_os = "macos")] was: i32 }

// <sys/resource.h>: IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES, IOPOL_SCOPE_THREAD, IOPOL_MATERIALIZE_DATALESS_FILES_OFF.
#[cfg(target_os = "macos")]
const DATALESS: i32 = 3;
#[cfg(target_os = "macos")]
const THREAD: i32 = 1;
#[cfg(target_os = "macos")]
const OFF: i32 = 1;
#[cfg(target_os = "macos")]
unsafe extern "C" { fn getiopolicy_np(iotype: i32, scope: i32) -> i32; fn setiopolicy_np(iotype: i32, scope: i32, policy: i32) -> i32; }

impl NoCloudDownloads {
    #[cfg(target_os = "macos")]
    fn new() -> Self {
        let was = unsafe { getiopolicy_np(DATALESS, THREAD) };
        unsafe { setiopolicy_np(DATALESS, THREAD, OFF) };
        Self { was }
    }
    #[cfg(not(target_os = "macos"))]
    fn new() -> Self { Self {} }
}

#[cfg(target_os = "macos")]
impl Drop for NoCloudDownloads {
    fn drop(&mut self) { if self.was >= 0 { unsafe { setiopolicy_np(DATALESS, THREAD, self.was) }; } }
}

/// A Markdown file as a new Markdown document inside `parent`; `root`: the
/// folder being imported, where `![[picture.png]]` embeds are also looked up.
pub(crate) fn import_markdown(store: &mut crate::store::Store, path: &Path, root: Option<&Path>, parent: Option<&str>) -> anyhow::Result<String> {
    let (mut doc, front) = read_markdown(path, root)?;
    crate::editor::image::localize_pasted_images(store.library_root(), &mut doc.blocks);
    store.create_markdown_in(&doc, &front, parent)
}

/// `import_markdown`'s document and front matter, before the library is touched.
fn read_markdown(path: &Path, root: Option<&Path>) -> anyhow::Result<(Document, String)> {
    let bytes = crate::store::read_bounded(path, IMPORT_FILE_LIMIT)?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{feff}');
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().trim().to_string();
    let mut doc = crate::markdown::parse_file(text, &stem, path.parent());
    if let Some(dir) = path.parent() { crate::markdown::wiki_images(&mut doc.blocks, dir, root); }
    if doc.blocks.is_empty() { doc.blocks.push(Block::text("")); }
    Ok((doc, crate::markdown::front_matter(text).0.to_string()))
}

/// One scanned folder as a library folder under `parent`; its id. Needs no
/// window, so 欢迎页 can import on a background thread before the main window.
pub(crate) fn import_tree(store: &mut crate::store::Store, t: &Scanned, parent: Option<&str>, failed: &mut usize) -> Option<String> {
    tree(store, t, &t.dir, parent, failed)
}

fn tree(store: &mut crate::store::Store, t: &Scanned, root: &Path, parent: Option<&str>, failed: &mut usize) -> Option<String> {
    let id = match store.create_folder(&t.name, parent) {
        Ok(id) => id,
        Err(_) => { *failed += t.files; return None; }
    };
    // In the folder's order, as its documents are: made together, unplaced
    // folders would fall back to random ids.
    if parent.is_some() { let _ = store.set_parent(&id, parent); }
    for sub in &t.dirs { tree(store, sub, root, Some(&id), failed); }
    for p in &t.docs {
        let made = if crate::local_files::is_markdown_path(p) {
            let read = { let _local = NoCloudDownloads::new(); read_markdown(p, Some(root)) };
            read.and_then(|(doc, front)| store.create_markdown_in(&doc, &front, Some(&id)))
        } else { store.import_into(p, Some(&id)) };
        if made.is_err() { *failed += 1; }
    }
    Some(id)
}

/// What a folder import left behind, for its toast.
pub(crate) fn import_problems(failed: usize, cloud: usize) -> Option<String> {
    let failed = (failed > 0).then(|| crate::localized_count!(failed, "有 {failed} 个文件未能导入", "{failed} file could not be imported", "{failed} files could not be imported"));
    let parts: Vec<String> = failed.into_iter().chain((cloud > 0).then(|| cloud_skipped(cloud))).collect();
    (!parts.is_empty()).then(|| parts.join(crate::settings::tr("；", "; ")))
}

fn cloud_skipped(cloud: usize) -> String {
    crate::localized_count!(cloud, "{cloud} 项只在 iCloud 中、没有下载到这台 Mac，已跳过；在访达中下载后可再导入",
        "Skipped {cloud} item that is only in iCloud; download it in Finder, then import again",
        "Skipped {cloud} items that are only in iCloud; download them in Finder, then import again")
}

fn downloads() -> PathBuf {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Downloads")).unwrap_or_default()
}

/// Where to save `name`: the system save panel (Downloads first), or a
/// scripted answer.
fn save_path(name: &str, cx: &mut App) -> impl std::future::Future<Output = Option<PathBuf>> + use<> {
    let picked = crate::devtools::take_pick().and_then(|p| p.into_iter().next());
    let prompt = picked.is_none().then(|| cx.prompt_for_new_path(&downloads(), Some(name)));
    async move {
        match prompt {
            Some(p) => p.await.ok().and_then(Result::ok).flatten(),
            None => picked,
        }
    }
}

/// Markdown and DocxXML files to import: the system open panel, or a
/// scripted answer.
fn open_paths(cx: &mut App) -> impl std::future::Future<Output = Option<Vec<PathBuf>>> + use<> {
    let picked = crate::devtools::take_pick();
    #[cfg(not(target_os = "ios"))]
    let extensions = Some(MARKDOWN_EXTS.iter().chain(&["xml", "csv", "tsv"]).filter(|e| crate::DATATABLE || !matches!(**e, "csv" | "tsv")).map(|e| SharedString::from(*e)).collect());
    let prompt = picked.is_none().then(|| cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some(crate::settings::text("导入").into()), #[cfg(not(target_os = "ios"))] extensions }));
    async move {
        match prompt {
            Some(p) => p.await.ok().and_then(Result::ok).flatten(),
            None => picked,
        }
    }
}

/// A CSV or TSV file, imported as a 数据表 (only while they are offered).
pub(crate) fn is_csv_path(p: &Path) -> bool { crate::DATATABLE && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("csv") || x.eq_ignore_ascii_case("tsv")) }

/// A CSV or TSV file as a new 数据表 file named after it: the first row
/// names the columns, each column's type is guessed from its cells.
pub(crate) fn import_csv(store: &mut crate::store::Store, path: &Path, parent: Option<&str>) -> anyhow::Result<String> {
    let bytes = crate::store::read_bounded(path, IMPORT_FILE_LIMIT)?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{feff}');
    let name = path.file_stem().and_then(|s| s.to_str()).filter(|s| !s.trim().is_empty()).unwrap_or(crate::settings::text("未命名数据表"));
    let mut base = crate::table::Base::default();
    let t = crate::table::csv::import(&mut base, name, text);
    base.tables.push(t);
    store.create_table_in(name, &base, parent)
}

/// The page title as a file name.
fn file_name(doc: &Document) -> String {
    display_title(&doc.title.text).replace(['/', ':', '\\'], "_")
}

/// The modal mask (the window dimmed) and its dialog.
fn mask(id: &'static str, dialog: impl IntoElement, close: impl Fn(&mut Workspace, &mut Context<Workspace>) + 'static, cx: &mut Context<Workspace>) -> AnyElement {
    deferred(
        scrim()
            .id(id)
            .on_mouse_down(MouseButton::Left, cx.listener(move |w, _, _, cx| close(w, cx)))
            .child(dialog),
    )
    .with_priority(3)
    .into_any_element()
}

/// The radio: `design::Radio`.
pub(crate) fn radio(on: bool) -> crate::design::Radio { crate::design::Radio::new(on) }

/// Percent-escapes in a Markdown link (`%20` and friends).
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// Relative and `file://` image links of an imported file, made absolute.
fn resolve_images(blocks: &mut [Block], base: &Path) {
    for b in blocks.iter_mut() {
        resolve_images(&mut b.children, base);
        if let BlockKind::Table(t) = &mut b.kind {
            for cell in t.rows.iter_mut().flatten() {
                resolve_images(&mut cell.blocks, base);
            }
        }
        let BlockKind::Image { src, .. } = &mut b.kind else { continue };
        if src.starts_with("http://") || src.starts_with("https://") || src.starts_with("data:") {
            continue;
        }
        let path = match src.strip_prefix("file://") {
            Some(p) => PathBuf::from(percent_decode(p)),
            None => base.join(percent_decode(src)),
        };
        *src = path.to_string_lossy().to_string();
    }
}

/// A Markdown / TXT file as a page: titled after the file, its leading
/// `# 标题` dropped when it repeats the name.
pub fn markdown_document(path: &Path) -> std::io::Result<Document> {
    let bytes = fdl_core::vfs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{feff}');
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().trim().to_string();
    let off = crate::paste::Off { file: true, ..Default::default() };
    let mut blocks = crate::paste::markdown_to_blocks(text, off);
    if blocks.first().is_some_and(|b| matches!(b.kind, BlockKind::Heading { level: 1, .. }) && b.text.text.trim() == stem) {
        blocks.remove(0);
    }
    if let Some(dir) = path.parent() {
        resolve_images(&mut blocks, dir);
        crate::markdown::wiki_images(&mut blocks, dir, None);
    }
    crate::editor::image::localize_pasted_images(&crate::store::Store::root_dir(), &mut blocks);
    if blocks.is_empty() {
        blocks.push(Block::text(""));
    }
    Ok(Document { title: RichText::plain(stem), icon: None, cover: None, blocks })
}

fn write_markdown_export(root: &Path, doc: &Document, subs: &[String], path: &Path, all: bool) -> anyhow::Result<usize> {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("doc");
    let assets = format!("{stem}.assets");
    let out = crate::markdown::export(doc, &crate::markdown::Options { library_root: Some(root), sub_pages: subs, assets: all.then_some(assets.as_str()), ..Default::default() });
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut missing = out.missing;
    fdl_core::vfs::write(path, &out.text)?;
    for (rel, text) in &out.generated {
        let to = dir.join(rel);
        if let Some(parent) = to.parent() { fdl_core::vfs::create_dir_all(parent)?; }
        fdl_core::vfs::write(to, text)?;
    }
    for (src, rel) in &out.files {
        let to = dir.join(rel);
        if let Some(parent) = to.parent() { fdl_core::vfs::create_dir_all(parent)?; }
        if crate::store::export_path_in(root, &src.to_string_lossy()).ok_or_else(|| anyhow::anyhow!("Asset outside library"))
            .and_then(|p| crate::store::read_bounded(&p, crate::store::EXPORT_FILE_LIMIT))
            .and_then(|bytes| Ok(fdl_core::vfs::write(&to, bytes)?)).is_err() { missing += 1; }
    }
    Ok(missing)
}

impl Workspace {
    fn close_menus(&mut self) {
        self.doc_menu = false;
        self.doc_sub = None;
        self.row_menu = None;
    }

    /// Titles of `id`'s child pages, for its 子页面列表 blocks.
    fn sub_page_titles(&self, id: &str) -> Vec<String> {
        crate::tree::Tree::new(&self.entries).children(Some(id)).iter().map(|e| e.title.clone()).collect()
    }

    /// 下载为 → Markdown: the 导出 Markdown 设置 first.
    pub fn open_export_md(&mut self, id: &str, cx: &mut Context<Self>) {
        self.close_menus();
        self.export_md = Some(ExportMd { id: id.to_string(), all: false });
        cx.notify();
    }

    /// 导出: the .md where the user picks; 所有内容 copies local images into
    /// `<name>.assets` beside it.
    fn export_markdown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ExportMd { id, all }) = self.export_md.take() else { return };
        cx.notify();
        let Some(doc) = self.doc_of(&id, cx) else { return };
        let subs = self.sub_page_titles(&id);
        let root = self.store.document_root(&id);
        let path = save_path(&format!("{}.md", file_name(&doc)), cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(path) = path.await else { return };
            let written = write_markdown_export(&root, &doc, &subs, &path, all);
            let missing = written.as_ref().copied().unwrap_or_default();
            if written.is_err() {
                let _ = this.update(cx, |w, cx| w.show_toast_kind(ToastKind::Error, crate::settings::text("导出失败，请重试"), cx));
            } else if missing > 0 {
                let _ = this.update(cx, |w, cx| w.show_toast_kind(ToastKind::Error, &crate::localized_count!(missing, "已导出，但有 {missing} 个图片或附件未能复制", "Exported, but {missing} image or attachment could not be copied", "Exported, but {missing} images or attachments could not be copied"), cx));
            }
        })
        .detach();
    }

    #[cfg(target_os = "ios")]
    pub(crate) fn mobile_export(&mut self, id: &str, all: bool, cx: &mut Context<Self>) {
        if !self.save_before(cx, { let id = id.to_owned(); move |w, _, cx| w.mobile_export(&id, all, cx) }) { return; }
        let Some(doc) = self.doc_of(id, cx) else { return };
        let subs = self.sub_page_titles(id);
        let root = self.store.root().to_path_buf();
        let session = crate::store::Store::session_lease(&root);
        cx.spawn(async move |this,cx| {
            let result = cx.background_executor().spawn(async move {
                let _session = session;
                let folder = crate::ipad::export_folder()?;
                let path = folder.join(format!("{}.md", file_name(&doc)));
                let result = write_markdown_export(&root, &doc, &subs, &path, all).and_then(|missing| {
                    anyhow::ensure!(missing == 0, "{}", crate::localized_format!("有 {missing} 个附件未能导出", "{missing} attachments could not be exported"));
                    Ok(if all { folder.clone() } else { path })
                });
                if result.is_err() { let _ = std::fs::remove_dir_all(&folder); }
                result
            }).await;
            let _ = this.update(cx, |workspace,cx| match result {
                Ok(path) => crate::ipad::present_export(&path),
                Err(error) => workspace.show_toast_kind(ToastKind::Error,&error.to_string(),cx),
            });
        }).detach();
    }

    /// 下载为 → PDF: the page laid out on A4 by WebKit, saved where the user picks.
    pub fn export_pdf(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(target_os = "ios")]
        { let _ = window; self.mobile_export_pdf(id,cx); return; }
        #[cfg(not(target_os = "ios"))]
        {
        self.close_menus();
        if !printing::SUPPORTED {
            self.show_toast_kind(ToastKind::Error, crate::settings::text("当前系统不支持"), cx);
            return;
        }
        cx.notify();
        let Some(doc) = self.doc_of(id, cx) else { return };
        let library_root = self.store.document_root(id);
        let subs = self.sub_page_titles(id);
        let name = file_name(&doc);
        let path = save_path(&format!("{name}.pdf"), cx);
        let view = printing::parent_of(window);
        cx.spawn_in(window, async move |this, cx| {
            let Some(mut path) = path.await else { return };
            // The save panel lets the name lose its `.pdf`; add it back, but never
            // replace a file the panel didn't ask about.
            if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
                let mut p = path.into_os_string();
                p.push(".pdf");
                path = PathBuf::from(p);
                if path.fs_exists() {
                    let msg = crate::localized_format!("已有同名文件：{}", "A file with this name already exists: {}", path.display());
                    let _ = this.update(cx, |w, cx| w.show_toast_kind(ToastKind::Error, &msg, cx));
                    return;
                }
            }
            let _ = this.update(cx, |w, cx| w.show_toast_kind(ToastKind::Info, crate::settings::text("正在导出文档"), cx));
            let html = crate::html::to_html(&doc, &crate::html::Options { library_root: &library_root, sub_pages: &subs });
            let _ = fdl_core::vfs::remove_file(&path);
            let res = printing::run(html, name, Job::Pdf(path.clone()), view, cx).await;
            let ok = matches!(res, Ok(true)) && path.fs_is_file();
            let _ = this.update(cx, |w, cx| {
                if ok {
                    w.toast = None;
                    cx.notify();
                } else {
                    w.show_toast_kind(ToastKind::Error, crate::settings::text("导出失败，请重试"), cx);
                }
            });
        })
        .detach();
        }
    }

    #[cfg(target_os = "ios")]
    fn mobile_export_pdf(&mut self, id: &str, cx: &mut Context<Self>) {
        self.close_menus();
        let selected = id.to_owned();
        if !self.save_before(cx,move |workspace,_,cx| workspace.mobile_export_pdf(&selected,cx)) { return; }
        let Some(doc) = self.doc_of(id,cx) else { return };
        let subs = self.sub_page_titles(id);
        let name = file_name(&doc);
        let html = crate::html::to_html(&doc,&crate::html::Options { sub_pages: &subs, library_root: self.store.root() });
        let folder = match crate::ipad::export_folder() {
            Ok(folder) => folder,
            Err(error) => { self.show_toast_kind(ToastKind::Error,&error.to_string(),cx); return; }
        };
        let path = folder.join(format!("{name}.pdf"));
        self.show_toast_kind(ToastKind::Info,crate::settings::text("正在导出文档"),cx);
        cx.spawn(async move |this,cx| {
            let result = printing::run(html,name,Job::Pdf(path.clone()),0,cx).await;
            let ok = matches!(result,Ok(true)) && path.is_file();
            let updated = this.update(cx,|workspace,cx| {
                if ok { workspace.toast = None; cx.notify(); crate::ipad::present_export(&path); }
                else { workspace.show_toast_kind(ToastKind::Error,result.as_ref().err().map(String::as_str)
                    .unwrap_or_else(|| crate::settings::text("导出失败，请重试")),cx); }
            });
            if !ok || updated.is_err() { let _ = fdl_core::vfs::remove_dir_all(folder); }
        }).detach();
    }

    /// 打印: the system print panel over the same A4 layout as PDF.
    pub fn print_doc(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        if !printing::SUPPORTED {
            self.show_toast_kind(ToastKind::Error, crate::settings::text("当前系统不支持"), cx);
            return;
        }
        let Some(doc) = self.doc_of(id, cx) else { return };
        let subs = self.sub_page_titles(id);
        let name = file_name(&doc);
        let html = crate::html::to_html(&doc, &crate::html::Options { library_root: &self.store.document_root(id), sub_pages: &subs });
        let view = printing::parent_of(window);
        self.show_toast_kind(ToastKind::Info, crate::settings::text("文档打印中"), cx);
        cx.spawn_in(window, async move |this, cx| {
            let res = printing::run(html, name, Job::Print, view, cx).await;
            let _ = this.update(cx, |w, cx| match res {
                Ok(_) => {
                    w.toast = None;
                    cx.notify();
                }
                Err(e) => w.show_toast_kind(ToastKind::Error, &e, cx),
            });
        })
        .detach();
    }

    /// 上传 / 导入为文档: pick Markdown or DocxXML files, each becoming a
    /// document; the last one opens.
    pub fn open_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        cx.notify();
        let paths = open_paths(cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(paths) = paths.await else { return };
            let _ = this.update_in(cx, |w, window, cx| w.import_paths(&paths, None, true, window, cx));
        })
        .detach();
    }

    /// Import files into `parent` (a folder; None: the top of 我的文档); with
    /// `open`, the last one opens (a drop on a list stays on the list).
    pub(crate) fn import_paths(&mut self, paths: &[PathBuf], parent: Option<String>, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.store.file_task_busy() || self.file_history.busy || self.tree_writes_pending() { return; }
        let (again, retry) = (paths.to_vec(), parent.clone());
        if !self.save_before(cx, move |w, window, cx| w.import_paths(&again, retry.clone(), open, window, cx)) {
            return;
        }
        let Some((key, mut writer)) = self.store.tree_writer(parent.as_deref()) else { return self.show_toast_kind(ToastKind::Error, &crate::shared_store::no_root().to_string(), cx) };
        let Some(task_guard) = self.store.begin_file_task() else { return };
        let parent = self.store.real_parent(parent.as_deref()).map(str::to_string);
        let paths = paths.to_vec();
        let task = cx.background_executor().spawn(async move {
            let journal = writer.track_file_changes();
            let mut last = None;
            let mut failed = false;
            let batch = writer.begin_batch().is_ok();
            for p in &paths {
                let ext = p.extension().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
                let made = match ext.as_str() {
                    _ if crate::local_files::is_markdown_path(p) => import_markdown(&mut writer, p, None, parent.as_deref()),
                    e if MARKDOWN_EXTS.contains(&e) => markdown_document(p).map_err(anyhow::Error::from).and_then(|doc| writer.create_from_in(&doc, parent.as_deref())),
                    "xml" => writer.import_into(p, parent.as_deref()),
                    "csv" | "tsv" if crate::DATATABLE => import_csv(&mut writer, p, parent.as_deref()),
                    _ => Err(anyhow::anyhow!("文件格式不支持")),
                };
                match made { Ok(id) => last = Some(id), Err(_) => failed = true }
            }
            if batch && writer.end_batch().is_err() { failed = true; }
            let entries = writer.list();
            (ImportedFiles { last, failed, changes: journal.take() }, entries)
        });
        self.show_toast_kind(ToastKind::Info, crate::settings::tr("正在导入…", "Importing…"), cx);
        cx.spawn_in(window, async move |this, cx| {
            let (imported, entries) = task.await;
            drop(task_guard);
            let _ = this.update_in(cx, |w, window, cx| {
                let entries = w.store.merge_entries(&key, entries);
                w.sync_peers(cx, &entries, None);
                w.entries = entries;
                w.sort();
                w.remember_file_changes(imported.changes, crate::settings::tr("导入", "Import"));
                if let Some(id) = imported.last.filter(|_| open) { w.open(&id, window, cx); }
                if imported.failed { w.show_toast_kind(ToastKind::Error, crate::settings::text("导入失败，请检查文件格式"), cx); }
                else if w.toast.as_ref().is_some_and(|t| matches!(t.0, ToastKind::Info) && t.1.as_ref() == crate::settings::tr("正在导入…", "Importing…")) { w.toast = None; }
                cx.notify();
            });
        }).detach();
    }

    /// The header's 导入 menu (主页, a folder's page): what it imports goes
    /// into the folder shown, or the top of 我的文档.
    pub(crate) fn open_import_kind(&mut self, kind: ImportKind, parent: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.import_menu = None;
        cx.notify();
        let picked = crate::devtools::take_pick();
        let (folders, file_extensions): (bool, &[&str]) = match kind {
            ImportKind::Markdown => (false, &["md", "markdown", "mdown"]),
            ImportKind::Folder => (true, &[]),
            ImportKind::DocxXml => (false, &["xml"]),
            ImportKind::Csv => (false, &["csv", "tsv"]),
        };
        #[cfg(target_os = "ios")]
        let _ = file_extensions;
        let options = PathPromptOptions {
            files: !folders, directories: folders, multiple: !folders, prompt: Some(crate::settings::text("导入").into()),
            #[cfg(not(target_os = "ios"))] extensions: (!folders).then(|| file_extensions.iter().map(|e| SharedString::from(*e)).collect()),
        };
        let prompt = picked.is_none().then(|| cx.prompt_for_paths(options));
        cx.spawn_in(window, async move |this, cx| {
            let paths = match prompt {
                Some(p) => p.await.ok().and_then(Result::ok).flatten(),
                None => picked,
            };
            let Some(paths) = paths.filter(|p| !p.is_empty()) else { return };
            let _ = this.update_in(cx, |w, window, cx| match kind {
                ImportKind::Folder => w.import_folder(&paths[0], parent.clone(), true, window, cx),
                _ => w.import_paths(&paths, parent.clone(), true, window, cx),
            });
        })
        .detach();
    }

    /// The 导入 menu under the header icon.
    pub(crate) fn render_import_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (parent, pos) = self.import_menu.clone()?;
        let row = |id: &'static str, glyph: &'static str, label: &'static str| crate::design::MenuItem::new(id).icon(glyph).label(label);
        let item = |id: &'static str, glyph: &'static str, label: &'static str, kind: ImportKind| {
            let parent = parent.clone();
            row(id, glyph, label).on_click(cx.listener(move |w, _, window, cx| w.open_import_kind(kind, parent.clone(), window, cx)))
        };
        Some(deferred(anchored().anchor(Anchor::TopRight).position(pos).snap_to_window_with_margin(px(8.)).child(
            crate::design::Popover::new("import-items").menu().min_w(px(210.))
                .on_mouse_down_out(cx.listener(|w, _, _, cx| { w.import_menu = None; cx.notify(); }))
                .child(item("import-md", "doc-md", crate::settings::tr("导入 Markdown 文件…", "Import Markdown Files…"), ImportKind::Markdown))
                .child(item("import-folder", "folder", crate::settings::tr("导入文件夹…", "Import Folder…"), ImportKind::Folder))
                .child(item("import-xml", "doc", crate::settings::tr("导入 DocxXML…", "Import DocxXML…"), ImportKind::DocxXml))
                .when(crate::DATATABLE, |d| d.child(item("import-csv", "datatable", crate::settings::tr("从 CSV 新建数据表…", "New Data Table from CSV…"), ImportKind::Csv))),
        )).with_priority(3).into_any_element())
    }

    /// The header's … menu: open this folder (None: the library) in the
    /// system's file manager.
    pub(crate) fn render_place_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (folder, pos) = self.place_menu.clone()?;
        let label = if cfg!(target_os = "macos") { crate::settings::tr("在访达中打开", "Open in Finder") }
            else if cfg!(target_os = "windows") { crate::settings::tr("在文件资源管理器中打开", "Open in File Explorer") }
            else { crate::settings::tr("在文件管理器中打开", "Open in File Manager") };
        Some(deferred(anchored().anchor(Anchor::TopRight).position(pos).snap_to_window_with_margin(px(8.)).child(
            crate::design::Popover::new("place-items").menu().w(px(210.))
                .on_mouse_down_out(cx.listener(|w, _, _, cx| { w.place_menu = None; cx.notify(); }))
                .child(crate::design::MenuItem::new("place-open").icon("folder").label(label)
                    .on_click(cx.listener(move |w, _, _, cx| {
                        w.place_menu = None;
                        if let Some(dir) = w.store.folder_dir(folder.as_deref()) { cx.open_with_system(&dir); }
                        cx.notify();
                    }))),
        )).with_priority(3).into_any_element())
    }

    /// The header's … icon, left of 导入; its menu hangs below it, right-aligned.
    pub(crate) fn place_button(&self, id: &'static str, folder: Option<String>, cx: &mut Context<Self>) -> crate::design::Button {
        let open = self.place_menu.is_some();
        crate::ui::header_icon(id, "ellipsis")
            .selected(open)
            .when(!open, |d| d.tip(crate::settings::tr("更多", "More"), None))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |w, ev: &ClickEvent, _, cx| {
                w.place_menu = if w.place_menu.is_some() { None } else { Some((folder.clone(), ev.position() + point(px(18.), px(20.)))) };
                cx.notify();
            }))
    }

    /// The header's 导入 icon; its menu hangs below it, right-aligned.
    pub(crate) fn import_button(&self, id: &'static str, parent: Option<String>, cx: &mut Context<Self>) -> crate::design::Button {
        let open = self.import_menu.is_some();
        crate::ui::header_icon(id, "import")
            .selected(open)
            .tip(crate::settings::tr("导入", "Import"), None)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |w, ev: &ClickEvent, _, cx| {
                w.import_menu = if w.import_menu.is_some() { None } else { Some((parent.clone(), ev.position() + point(px(18.), px(20.)))) };
                cx.notify();
            }))
    }

    /// 导入文件夹: pick a folder; it lands at the top of 我的文档 as a folder,
    /// its sub-folders as folders, its Markdown and DocxXML files as documents.
    pub fn open_import_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_import_kind(ImportKind::Folder, None, window, cx);
    }

    /// Import `dir` as a folder inside `parent` (None: the top of 我的文档).
    pub(crate) fn import_folder(&mut self, dir: &Path, parent: Option<String>, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.import_folder_then(dir, parent, open, None, window, cx);
    }

    /// `import_folder`, then `then` with the new folder once it is in.
    pub(crate) fn import_folder_then(&mut self, dir: &Path, parent: Option<String>, open: bool, then: Option<AfterImport>, window: &mut Window, cx: &mut Context<Self>) {
        if self.store.file_task_busy() || self.file_history.busy || self.tree_writes_pending() { return; }
        let held = std::rc::Rc::new(std::cell::Cell::new(then));
        let (again, retry, later) = (dir.to_path_buf(), parent.clone(), held.clone());
        if !self.save_before(cx, move |w, window, cx| w.import_folder_then(&again, retry.clone(), open, later.take(), window, cx)) {
            return;
        }
        let then = held.take();
        // Scanning and writing hundreds of files takes seconds (an fsync each),
        // so it runs on its own store off the window's thread; the library
        // lock keeps it one write, as `tree_writer` does for drags.
        let Some((key, mut writer)) = self.store.tree_writer(parent.as_deref()) else { return self.show_toast_kind(ToastKind::Error, &crate::shared_store::no_root().to_string(), cx) };
        let Some(task_guard) = self.store.begin_file_task() else { return };
        let parent = self.store.real_parent(parent.as_deref()).map(str::to_string);
        let dir = dir.to_path_buf();
        let task = cx.background_executor().spawn(async move {
            let imported = match scan_folder(&dir, 0) {
                None => Imported::Empty { cloud: 0 },
                Some(tree) if tree.files == 0 => Imported::Empty { cloud: tree.cloud },
                Some(tree) if tree.files > IMPORT_FOLDER_LIMIT => Imported::TooMany(tree.files),
                Some(tree) => {
            let journal = writer.track_file_changes();
            let mut failed = 0;
            // One write for the whole folder: each file is written where it
            // goes and the index once at the end (120 notes took 7 s one by one).
            // Without the batch each file is still written on its own.
            let batch = writer.begin_batch().is_ok();
            let top = import_tree(&mut writer, &tree, parent.as_deref(), &mut failed);
            let saved = if batch { writer.end_batch() } else { Ok(()) };
            Imported::Done { top, failed, cloud: tree.cloud, saved, changes: journal.take() }
                }
            };
            match imported {
                Imported::Empty { .. } | Imported::TooMany(_) => (imported, Vec::new()),
                Imported::Done { .. } => (imported, writer.list()),
            }
        });
        self.show_toast_kind(ToastKind::Info, crate::settings::tr("正在导入…", "Importing…"), cx);
        cx.spawn_in(window, async move |this, cx| {
            let (imported, entries) = task.await;
            drop(task_guard);
            let _ = this.update_in(cx, |w, window, cx| { let entries = if entries.is_empty() { entries } else { w.store.merge_entries(&key, entries) }; w.finish_import_folder(imported, entries, open, then, window, cx) });
        }).detach();
    }

    fn finish_import_folder(&mut self, imported: Imported, entries: Vec<crate::store::Entry>, open: bool, then: Option<AfterImport>, window: &mut Window, cx: &mut Context<Self>) {
        let (top, failed, cloud, saved, changes) = match imported {
            Imported::Empty { cloud } => {
                let msg = if cloud > 0 { cloud_skipped(cloud) } else { crate::settings::tr("这个文件夹里没有 Markdown 或 XML 文件", "This folder has no Markdown or XML files").to_string() };
                self.show_toast_kind(ToastKind::Error, &msg, cx);
                return;
            }
            Imported::TooMany(files) => {
                self.show_toast_kind(ToastKind::Error, &crate::localized_format!("文件夹里有 {} 个文件，一次最多导入 {} 个", "The folder has {} files; up to {} can be imported at once", files, IMPORT_FOLDER_LIMIT), cx);
                return;
            }
            Imported::Done { top, failed, cloud, saved, changes } => (top, failed, cloud, saved, changes),
        };
        self.sync_peers(cx, &entries, None);
        self.entries = entries;
        self.sort();
        self.remember_file_changes(changes, crate::settings::tr("导入文件夹", "Import folder"));
        if let Some(id) = top {
            if let Some(then) = then { then(self, &id); }
            self.reveal_in_tree(&id);
            if open { self.open(&id, window, cx); }
        }
        if let Err(e) = saved {
            self.show_toast_kind(ToastKind::Error, &crate::localized_format!("导入未能完成：{}", "Import could not finish: {}", crate::settings::diagnostic(&e.to_string())), cx);
        } else if let Some(msg) = import_problems(failed, cloud) {
            self.show_toast_kind(ToastKind::Error, &msg, cx);
        } else if self.toast.as_ref().is_some_and(|t| matches!(t.0, ToastKind::Info) && t.1.as_ref() == crate::settings::tr("正在导入…", "Importing…")) { self.toast = None; }
        cx.notify();
    }

    /// 导出 Markdown 设置 as a `Dialog`: its title, the options, the buttons
    /// on the right.
    pub(crate) fn render_export_md(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let all = self.export_md.as_ref()?.all;
        let option = |id: &'static str, on: bool, label: &'static str, value: bool| {
            div()
                .id(id)
                .flex()
                .items_center()
                .cursor_pointer()
                .child(radio(on))
                .child(div().ml(px(8.)).child(label))
                .on_click(cx.listener(move |w, _, _, cx| {
                    if let Some(e) = w.export_md.as_mut() {
                        e.all = value;
                        cx.notify();
                    }
                }))
        };
        let ws = cx.entity().downgrade();
        let ws2 = ws.clone();
        let dialog = Dialog::new("export-md")
            .keys(&self.dialog_focus, move |_, cx| { let _ = ws.update(cx, |w, cx| { w.export_md = None; cx.notify(); }); },
                Some(move |window: &mut Window, cx: &mut App| { let _ = ws2.update(cx, |w, cx| w.export_markdown(window, cx)); }))
            .relative()
            .w(px(720.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .title(crate::settings::text("导出 Markdown 设置"))
            .closable(cx.listener(|w, _, _, cx| {
                w.export_md = None;
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(div().mr(px(40.)).text_color(crate::design::color::muted_foreground()).child(crate::settings::text("内容范围")))
                    .child(
                        div()
                            .flex()
                            .gap(px(24.))
                            .child(option("export-md-text", !all, crate::settings::text("仅文本（不含图片和附件）"), false))
                            .child(option("export-md-all", all, crate::settings::text("所有内容"), true)),
                    ),
            )
            .footer(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(Button::outline("export-md-cancel").label(crate::settings::text("取消")).min_w(px(80.)).on_click(cx.listener(|w, _, _, cx| {
                        w.export_md = None;
                        cx.notify();
                    })))
                    .child(Button::primary("export-md-ok").label(crate::settings::text("导出")).min_w(px(80.)).on_click(cx.listener(|w, _, window, cx| w.export_markdown(window, cx)))),
            );
        Some(mask(
            "export-md-mask",
            dialog,
            |w, cx| {
                w.export_md = None;
                cx.notify();
            },
            cx,
        ))
    }
}

#[cfg(feature = "devtools")]
impl Workspace {
    pub(crate) fn check_export_safety(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::model::*;
        let root = crate::store::Store::root_dir();
        assert!(std::env::var_os("FDL_SCRIPT").is_some() && std::env::var_os("QUIET_MD_HOME").is_some());
        fdl_core::vfs::create_dir_all(root.join("assets")).unwrap();
        fdl_core::vfs::write(root.join("assets/report.pdf"), b"attachment-content").unwrap();
        let outside = root.parent().unwrap().join("outside-private.txt");
        fdl_core::vfs::write(&outside, b"PRIVATE-MUST-NOT-EXPORT").unwrap();
        let file = FileBlock { src_kind: "path".into(), src: "assets/report.pdf".into(), name: "Report".into(), view: FileView::Card };
        let mut text = RichText::plain("Attachment: ");
        text.push("\u{fffc}", Marks { inline: Some(crate::docxml::to_inline(InlineKind::File(file))), ..Default::default() });
        let mut doc = Document { title: RichText::plain("Export safety"), blocks: vec![Block::new(BlockKind::Text, text)], ..Default::default() };
        let image = crate::docxml::parse(&format!("<img href=\"{}\"/>", outside.display())).unwrap();
        assert!(image.blocks.iter().any(|b| matches!(b.kind, BlockKind::Image { .. })));
        doc.blocks.extend(image.blocks);
        let html = crate::html::to_html(&doc, &crate::html::Options { library_root: self.store.root(), sub_pages: &[] });
        assert!(!html.contains("PRIVATE-MUST-NOT-EXPORT"));
        assert!(!html.contains("UFJJVkFURS1NVVNULU5PVC1FWFBPUlQ="));
        assert!(!html.contains(&*outside.to_string_lossy()));
        let id = self.store.create_from(&doc).unwrap();
        self.entries = self.store.list();
        self.open(&id, window, cx);
        self.export_md = Some(ExportMd { id, all: true });
        self.export_markdown(window, cx);
        eprintln!("export-safety: HTML rejected outside file; Markdown export dispatched");
    }
}


#[cfg(test)]
mod tests {
    use super::{BlockKind, markdown_document, percent_decode};

    #[test]
    fn importing_many_small_notes_preserves_the_tree() {
        let tmp = fdl_core::vfs::temp_dir().join(format!("quiet-many-notes-{}", uuid::Uuid::new_v4()));
        let source = tmp.join("input");
        for n in 0..600 {
            let dir = source.join(format!("dir-{}", n % 253));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("note-{n}.md")), format!("body {n}")).unwrap();
        }
        let mut store = crate::store::Store::open_at(tmp.join("library")).unwrap();
        let tree = super::scan_folder(&source, 0).unwrap();
        let mut failed = 0;
        store.batch(|s| super::import_tree(s, &tree, None, &mut failed)).unwrap().unwrap();
        assert_eq!(failed, 0);
        let entries = store.list();
        for n in 0..600 {
            let title = format!("note-{n}");
            let entry = entries.iter().find(|e| e.title == title).unwrap();
            let path = store.file_path(&entry.id).unwrap();
            assert!(path.ends_with(std::path::Path::new(&format!("dir-{}", n % 253)).join(format!("{title}.md"))));
            assert_eq!(std::fs::read_to_string(path).unwrap().trim(), format!("body {n}"));
        }
        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn notes_left_in_icloud_are_counted_not_read() {
        let tmp = fdl_core::vfs::temp_dir().join(format!("quiet-icloud-{}", uuid::Uuid::new_v4()));
        let source = tmp.join("vault");
        std::fs::create_dir_all(source.join("journals")).unwrap();
        std::fs::write(source.join("here.md"), "here").unwrap();
        // An older macOS's stand-ins for notes iCloud kept: a few bytes of plist, not the note.
        std::fs::write(source.join(".gone.md.icloud"), "bplist").unwrap();
        std::fs::write(source.join("journals/.2021_03_01.md.icloud"), "bplist").unwrap();
        std::fs::write(source.join(".photo.png.icloud"), "bplist").unwrap();
        std::fs::write(source.join(".hidden.md"), "hidden").unwrap();
        let tree = super::scan_folder(&source, 0).unwrap();
        assert_eq!((tree.files, tree.cloud), (1, 2));
        // A folder with only such notes adds nothing to import but still counts them.
        let journals = super::scan_folder(&source.join("journals"), 0).unwrap();
        assert_eq!((journals.files, journals.cloud), (0, 1));
        assert!(super::import_problems(0, 0).is_none());
        assert!(super::import_problems(0, 2).is_some_and(|m| m.contains('2')));
        assert!(super::import_problems(3, 2).is_some_and(|m| m.contains('3') && m.contains('2')));
        std::fs::remove_dir_all(tmp).unwrap();
    }

    /// The guard turns iCloud downloads off for its thread only while it lives.
    #[cfg(target_os = "macos")]
    #[test]
    fn reads_skip_icloud_downloads_while_guarded() {
        use super::{getiopolicy_np, DATALESS, OFF, THREAD};
        let before = unsafe { getiopolicy_np(DATALESS, THREAD) };
        assert_ne!(before, OFF);
        {
            let _local = super::NoCloudDownloads::new();
            assert_eq!(unsafe { getiopolicy_np(DATALESS, THREAD) }, OFF);
            std::thread::spawn(move || assert_eq!(unsafe { getiopolicy_np(DATALESS, THREAD) }, before)).join().unwrap();
        }
        assert_eq!(unsafe { getiopolicy_np(DATALESS, THREAD) }, before);
    }

    #[test]
    fn imports_markdown_file() {
        let dir = fdl_core::vfs::temp_dir().join(format!("fdl-import-{}", fdl_core::vfs::process_id()));
        fdl_core::vfs::create_dir_all(&dir).unwrap();
        let path = dir.join("周报.md");
        fdl_core::vfs::write(&path, "\u{feff}# 周报\n\n第一段\n\n- a\n- b\n\n![图](https://e.com/a.png)\n\n## 小结\n").unwrap();
        let doc = markdown_document(&path).unwrap();
        assert_eq!(doc.title.text, "周报");
        let kinds: Vec<_> = doc.blocks.iter().map(|b| std::mem::discriminant(&b.kind)).collect();
        assert_eq!(kinds.len(), 5);
        assert_eq!(doc.blocks[0].text.text, "第一段");
        assert!(matches!(doc.blocks[1].kind, BlockKind::Bullet { .. }));
        assert!(matches!(&doc.blocks[3].kind, BlockKind::Image { src, .. } if src == "https://e.com/a.png"));
        assert!(matches!(doc.blocks[4].kind, BlockKind::Heading { level: 2, .. }));
        assert_eq!(percent_decode("a%20b%E5%9B%BE.png"), "a b图.png");
        let _ = fdl_core::vfs::remove_dir_all(dir);
    }
}
