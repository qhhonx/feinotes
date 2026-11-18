//! 导入 / 导出: 下载为 → PDF / Markdown (with the 导出 Markdown 设置
//! dialog), 打印, and 导入为文档 (主页's 上传, the app menu): local Markdown
//! or DocxXML files picked straight away. The Word / PDF settings and
//! 打印选项 only choose what happens to comments, so they are left out.

use crate::app::Workspace;
use crate::settings::display_title;
use crate::model::{Block, BlockKind, Document, RichText};
use crate::printing::{self, Job};
use crate::theme::Theme;
use crate::trash::{close_button, dialog, primary_button, scrim, secondary_button};
use crate::ui::{TipExt, ToastKind};
use gpui::{prelude::FluentBuilder, *};
#[cfg(target_os = "ios")]
use gpui::Anchor as Corner;
use std::path::{Path, PathBuf};

/// The open 导出 Markdown 设置 dialog: the page, and 所有内容 picked.
pub struct ExportMd {
    pub id: String,
    pub all: bool,
}

const MARKDOWN_EXTS: [&str; 4] = ["md", "markdown", "mark", "txt"];

/// What the header's 导入 menu picks.
#[derive(Clone, Copy)]
pub(crate) enum ImportKind { Markdown, Folder, DocxXml }

/// 导入文件夹 refuses folders with more documents than this.
pub(crate) const IMPORT_FOLDER_LIMIT: usize = 2000;
/// Bigger files are not documents.
const IMPORT_FILE_LIMIT: u64 = 32 * 1024 * 1024;

/// What to do with a folder 导入文件夹 brought in (a drop puts it in its slot).
pub(crate) type AfterImport = Box<dyn FnOnce(&mut Workspace, &str)>;

/// What a background 导入文件夹 did.
enum Imported { Empty, TooMany(usize), Done { top: Option<String>, failed: usize, saved: anyhow::Result<()>, changes: Vec<crate::store::FileDelta> } }
struct ImportedFiles { last: Option<String>, failed: bool, changes: Vec<crate::store::FileDelta> }

/// A folder to import: its documents and the sub-folders that hold some.
pub(crate) struct Scanned { pub(crate) name: String, dir: PathBuf, dirs: Vec<Scanned>, docs: Vec<PathBuf>, pub(crate) files: usize }

/// Markdown and DocxXML files under `dir`, by name; hidden folders,
/// dependency folders and symlinks are skipped.
pub(crate) fn scan_folder(dir: &Path, depth: usize) -> Option<Scanned> {
    let name = dir.file_name()?.to_string_lossy().into_owned();
    let mut t = Scanned { name, dir: dir.to_path_buf(), dirs: vec![], docs: vec![], files: 0 };
    if depth > 32 { return Some(t); }
    let mut items: Vec<_> = std::fs::read_dir(dir).ok()?.flatten().collect();
    items.sort_by_key(|e| e.file_name().to_string_lossy().to_lowercase());
    for e in items {
        let p = e.path();
        let n = e.file_name().to_string_lossy().into_owned();
        let Ok(ft) = e.file_type() else { continue };
        if n.starts_with('.') || ft.is_symlink() { continue; }
        if ft.is_dir() {
            if matches!(n.as_str(), "node_modules" | "target") { continue; }
            if let Some(sub) = scan_folder(&p, depth + 1).filter(|s| s.files > 0) { t.files += sub.files; t.dirs.push(sub); }
        } else if crate::local_files::is_markdown_path(&p) || p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xml")) {
            t.docs.push(p);
            t.files += 1;
        }
        if t.files > IMPORT_FOLDER_LIMIT { break; }
    }
    Some(t)
}

/// A Markdown file as a new Markdown document inside `parent`; `root`: the
/// folder being imported, where `![[picture.png]]` embeds are also looked up.
pub(crate) fn import_markdown(store: &mut crate::store::Store, path: &Path, root: Option<&Path>, parent: Option<&str>) -> anyhow::Result<String> {
    let bytes = crate::store::read_bounded(path, IMPORT_FILE_LIMIT)?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{feff}');
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().trim().to_string();
    let mut doc = crate::markdown::parse_file(text, &stem, path.parent());
    if let Some(dir) = path.parent() { crate::markdown::wiki_images(&mut doc.blocks, dir, root); }
    crate::editor::image::localize_pasted_images(store.library_root(), &mut doc.blocks);
    if doc.blocks.is_empty() { doc.blocks.push(Block::text("")); }
    store.create_markdown_in(&doc, crate::markdown::front_matter(text).0, parent)
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
        let made = if crate::local_files::is_markdown_path(p) { import_markdown(store, p, Some(root), Some(&id)) } else { store.import_into(p, Some(&id)) };
        if made.is_err() { *failed += 1; }
    }
    Some(id)
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
    let extensions = Some(MARKDOWN_EXTS.iter().chain(&["xml"]).map(|e| SharedString::from(*e)).collect());
    let prompt = picked.is_none().then(|| cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some(crate::settings::text("导入").into()), #[cfg(not(target_os = "ios"))] extensions }));
    async move {
        match prompt {
            Some(p) => p.await.ok().and_then(Result::ok).flatten(),
            None => picked,
        }
    }
}

/// The page title as a file name.
fn file_name(doc: &Document) -> String {
    display_title(&doc.title.text).replace(['/', ':', '\\'], "_")
}

/// The modal mask (the window dimmed) and its dialog.
fn mask(id: &'static str, dialog: impl IntoElement, close: impl Fn(&mut Workspace, &mut Context<Workspace>) + 'static, cx: &mut Context<Workspace>) -> AnyElement {
    deferred(
        div()
            .id(id)
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(scrim())
            .on_mouse_down(MouseButton::Left, cx.listener(move |w, _, _, cx| close(w, cx)))
            .child(dialog),
    )
    .with_priority(3)
    .into_any_element()
}

/// 16px radio: the accent with a white dot when checked, else a white disc
/// with the task box's rim.
pub(crate) fn radio(on: bool) -> Div {
    div()
        .size(px(16.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .when(on, |d| d.bg(Theme::primary()).child(div().size(px(6.)).rounded_full().bg(crate::theme::raised(Theme::bg_body()))))
        .when(!on, |d| d.border_1().border_color(Theme::todo_box()).bg(crate::theme::raised(Theme::bg_body())))
}

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
    let bytes = std::fs::read(path)?;
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
    std::fs::write(path, &out.text)?;
    for (rel, text) in &out.generated {
        let to = dir.join(rel);
        if let Some(parent) = to.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(to, text)?;
    }
    for (src, rel) in &out.files {
        let to = dir.join(rel);
        if let Some(parent) = to.parent() { std::fs::create_dir_all(parent)?; }
        if crate::store::export_path_in(root, &src.to_string_lossy()).ok_or_else(|| anyhow::anyhow!("Asset outside library"))
            .and_then(|p| crate::store::read_bounded(&p, crate::store::EXPORT_FILE_LIMIT))
            .and_then(|bytes| Ok(std::fs::write(&to, bytes)?)).is_err() { missing += 1; }
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
        let root = self.store.root().to_path_buf();
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
        let root = crate::store::Store::root_dir();
        let session = crate::store::Store::session_lease(&root);
        cx.spawn(async move |this,cx| {
            let result = cx.background_executor().spawn(async move {
                let _session = session;
                let folder = crate::ipad::export_folder()?;
                let path = folder.join(format!("{}.md", file_name(&doc)));
                let missing = write_markdown_export(&root, &doc, &subs, &path, all)?;
                anyhow::ensure!(missing == 0, "{}", crate::localized_format!("有 {missing} 个附件未能导出", "{missing} attachments could not be exported"));
                Ok::<_,anyhow::Error>(if all { folder } else { path })
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
        let library_root = self.store.root().to_path_buf();
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
                if path.exists() {
                    let msg = crate::localized_format!("已有同名文件：{}", "A file with this name already exists: {}", path.display());
                    let _ = this.update(cx, |w, cx| w.show_toast_kind(ToastKind::Error, &msg, cx));
                    return;
                }
            }
            let _ = this.update(cx, |w, cx| w.show_toast_kind(ToastKind::Info, crate::settings::text("正在导出文档"), cx));
            let html = crate::html::to_html(&doc, &crate::html::Options { library_root: &library_root, sub_pages: &subs });
            let _ = std::fs::remove_file(&path);
            let res = printing::run(html, name, Job::Pdf(path.clone()), view, cx).await;
            let ok = matches!(res, Ok(true)) && path.is_file();
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
            if !ok || updated.is_err() { let _ = std::fs::remove_dir_all(folder); }
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
        let html = crate::html::to_html(&doc, &crate::html::Options { library_root: self.store.root(), sub_pages: &subs });
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
        let Some(task_guard) = self.store.begin_file_task() else { return };
        let mut writer = self.store.tree_writer();
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
        let (folders, extensions): (bool, &[&str]) = match kind {
            ImportKind::Markdown => (false, &["md", "markdown", "mdown"]),
            ImportKind::Folder => (true, &[]),
            ImportKind::DocxXml => (false, &["xml"]),
        };
        let options = PathPromptOptions {
            files: !folders, directories: folders, multiple: !folders, prompt: Some(crate::settings::text("导入").into()),
            #[cfg(not(target_os = "ios"))] extensions: (!folders).then(|| extensions.iter().map(|e| SharedString::from(*e)).collect()),
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
        let row = |id: &'static str, glyph: &'static str, label: &'static str| crate::ui::menu_row(id, SharedString::from(id)).child(crate::ui::icon(glyph, 16., Theme::icon())).child(label);
        let item = |id: &'static str, glyph: &'static str, label: &'static str, kind: ImportKind| {
            let parent = parent.clone();
            row(id, glyph, label).on_click(cx.listener(move |w, _, window, cx| w.open_import_kind(kind, parent.clone(), window, cx)))
        };
        Some(deferred(anchored().anchor(Corner::TopRight).position(pos).snap_to_window_with_margin(px(8.)).child(
            crate::ui::popover().id("import-items").occlude().w(px(210.)).py(px(crate::ui::MENU_PAD_Y))
                .on_mouse_down_out(cx.listener(|w, _, _, cx| { w.import_menu = None; cx.notify(); }))
                .child(item("import-md", "doc-md", crate::settings::tr("导入 Markdown 文件…", "Import Markdown Files…"), ImportKind::Markdown))
                .child(item("import-folder", "folder", crate::settings::tr("导入文件夹…", "Import Folder…"), ImportKind::Folder))
                .child(item("import-xml", "doc", crate::settings::tr("导入 DocxXML…", "Import DocxXML…"), ImportKind::DocxXml)),
        )).with_priority(3).into_any_element())
    }

    /// The header's … menu: open this folder (None: the library) in the
    /// system's file manager.
    pub(crate) fn render_place_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (folder, pos) = self.place_menu.clone()?;
        let label = if cfg!(target_os = "macos") { crate::settings::tr("在访达中打开", "Open in Finder") }
            else if cfg!(target_os = "windows") { crate::settings::tr("在文件资源管理器中打开", "Open in File Explorer") }
            else { crate::settings::tr("在文件管理器中打开", "Open in File Manager") };
        Some(deferred(anchored().anchor(Corner::TopRight).position(pos).snap_to_window_with_margin(px(8.)).child(
            crate::ui::popover().id("place-items").occlude().w(px(210.)).py(px(crate::ui::MENU_PAD_Y))
                .on_mouse_down_out(cx.listener(|w, _, _, cx| { w.place_menu = None; cx.notify(); }))
                .child(crate::ui::menu_row("place-open", SharedString::from("place-open")).child(crate::ui::icon("folder", 16., Theme::icon())).child(label)
                    .on_click(cx.listener(move |w, _, _, cx| {
                        w.place_menu = None;
                        if let Some(dir) = w.store.folder_dir(folder.as_deref()) { cx.open_with_system(&dir); }
                        cx.notify();
                    }))),
        )).with_priority(3).into_any_element())
    }

    /// The header's … icon, left of 导入; its menu hangs below it, right-aligned.
    pub(crate) fn place_button(&self, id: &'static str, folder: Option<String>, cx: &mut Context<Self>) -> Stateful<Div> {
        let open = self.place_menu.is_some();
        crate::ui::header_icon(id, "ellipsis")
            .when(open, |d| d.bg(Theme::fill_hover()))
            .when(!open, |d| d.tip(crate::settings::tr("更多", "More"), None))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |w, ev: &ClickEvent, _, cx| {
                w.place_menu = if w.place_menu.is_some() { None } else { Some((folder.clone(), ev.position() + point(px(18.), px(20.)))) };
                cx.notify();
            }))
    }

    /// The header's 导入 icon; its menu hangs below it, right-aligned.
    pub(crate) fn import_button(&self, id: &'static str, parent: Option<String>, cx: &mut Context<Self>) -> Stateful<Div> {
        let open = self.import_menu.is_some();
        crate::ui::header_icon(id, "import")
            .when(open, |d| d.bg(Theme::fill_hover()))
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
        let Some(task_guard) = self.store.begin_file_task() else { return };
        // Scanning and writing hundreds of files takes seconds (an fsync each),
        // so it runs on its own store off the window's thread; the library
        // lock keeps it one write, as `tree_writer` does for drags.
        let mut writer = self.store.tree_writer();
        let dir = dir.to_path_buf();
        let task = cx.background_executor().spawn(async move {
            let imported = match scan_folder(&dir, 0).filter(|t| t.files > 0) {
                None => Imported::Empty,
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
            Imported::Done { top, failed, saved, changes: journal.take() }
                }
            };
            match imported {
                Imported::Empty | Imported::TooMany(_) => (imported, Vec::new()),
                Imported::Done { .. } => (imported, writer.list()),
            }
        });
        self.show_toast_kind(ToastKind::Info, crate::settings::tr("正在导入…", "Importing…"), cx);
        cx.spawn_in(window, async move |this, cx| {
            let (imported, entries) = task.await;
            drop(task_guard);
            let _ = this.update_in(cx, |w, window, cx| w.finish_import_folder(imported, entries, open, then, window, cx));
        }).detach();
    }

    fn finish_import_folder(&mut self, imported: Imported, entries: Vec<crate::store::Entry>, open: bool, then: Option<AfterImport>, window: &mut Window, cx: &mut Context<Self>) {
        let (top, failed, saved, changes) = match imported {
            Imported::Empty => {
                self.show_toast_kind(ToastKind::Error, crate::settings::tr("这个文件夹里没有 Markdown 或 XML 文件", "This folder has no Markdown or XML files"), cx);
                return;
            }
            Imported::TooMany(files) => {
                self.show_toast_kind(ToastKind::Error, &crate::localized_format!("文件夹里有 {} 个文件，一次最多导入 {} 个", "The folder has {} files; up to {} can be imported at once", files, IMPORT_FOLDER_LIMIT), cx);
                return;
            }
            Imported::Done { top, failed, saved, changes } => (top, failed, saved, changes),
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
        } else if failed > 0 {
            self.show_toast_kind(ToastKind::Error, &crate::localized_count!(failed, "有 {failed} 个文件未能导入", "{failed} file could not be imported", "{failed} files could not be imported"), cx);
        } else if self.toast.as_ref().is_some_and(|t| matches!(t.0, ToastKind::Info) && t.1.as_ref() == crate::settings::tr("正在导入…", "Importing…")) { self.toast = None; }
        cx.notify();
    }

    /// 导出 Markdown 设置 as a modal: white, radius 16, a 15pt semibold
    /// title over a hairline, 13pt options, the buttons on the right.
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
        let dialog = dialog("export-md")
            .relative()
            .w(px(720.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .pt(px(24.))
                    .pb(px(24.))
                    .pl(px(24.))
                    .pr(px(56.))
                    .border_b_1()
                    .border_color(Theme::line_divider())
                    .text_size(px(15.))
                    .line_height(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(crate::settings::text("导出 Markdown 设置")),
            )
            .child(close_button("export-md-close").absolute().top(px(27.)).right(px(25.)).on_click(cx.listener(|w, _, _, cx| {
                w.export_md = None;
                cx.notify();
            })))
            .child(
                div().pt(px(24.)).px(px(24.)).pb(px(16.)).mb(px(24.)).child(
                    div()
                        .flex()
                        .items_center()
                        .text_size(px(13.))
                        .line_height(px(20.))
                        .child(div().mr(px(40.)).text_color(Theme::text_caption()).child(crate::settings::text("内容范围")))
                        .child(
                            div()
                                .flex()
                                .gap(px(24.))
                                .child(option("export-md-text", !all, crate::settings::text("仅文本（不含图片和附件）"), false))
                                .child(option("export-md-all", all, crate::settings::text("所有内容"), true)),
                        ),
                ),
            )
            .child(
                div()
                    .px(px(24.))
                    .pb(px(24.))
                    .flex()
                    .justify_end()
                    .gap(px(12.))
                    .child(secondary_button("export-md-cancel", crate::settings::text("取消")).min_w(px(80.)).on_click(cx.listener(|w, _, _, cx| {
                        w.export_md = None;
                        cx.notify();
                    })))
                    .child(primary_button("export-md-ok", crate::settings::text("导出")).min_w(px(80.)).on_click(cx.listener(|w, _, window, cx| w.export_markdown(window, cx)))),
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
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("assets/report.pdf"), b"attachment-content").unwrap();
        let outside = root.parent().unwrap().join("outside-private.txt");
        std::fs::write(&outside, b"PRIVATE-MUST-NOT-EXPORT").unwrap();
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
    fn imports_markdown_file() {
        let dir = std::env::temp_dir().join(format!("fdl-import-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("周报.md");
        std::fs::write(&path, "\u{feff}# 周报\n\n第一段\n\n- a\n- b\n\n![图](https://e.com/a.png)\n\n## 小结\n").unwrap();
        let doc = markdown_document(&path).unwrap();
        assert_eq!(doc.title.text, "周报");
        let kinds: Vec<_> = doc.blocks.iter().map(|b| std::mem::discriminant(&b.kind)).collect();
        assert_eq!(kinds.len(), 5);
        assert_eq!(doc.blocks[0].text.text, "第一段");
        assert!(matches!(doc.blocks[1].kind, BlockKind::Bullet { .. }));
        assert!(matches!(&doc.blocks[3].kind, BlockKind::Image { src, .. } if src == "https://e.com/a.png"));
        assert!(matches!(doc.blocks[4].kind, BlockKind::Heading { level: 2, .. }));
        assert_eq!(percent_decode("a%20b%E5%9B%BE.png"), "a b图.png");
        let _ = std::fs::remove_dir_all(dir);
    }
}
