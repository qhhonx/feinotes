//! 本机文件: Markdown files opened where they are (打开文件…, the Finder),
//! edited in place and listed under 本机文件, last opened first (folders
//! opened the same way are `local_folders.rs`). The library
//! only remembers them: 从列表中移除 never touches the file, and 导入到文档库
//! makes a copy.

use crate::app::{RowMenu, Workspace};
use crate::settings::tr;
use crate::store::Entry;
use crate::theme::Theme;
use crate::ui::{ToastKind, icon, popover};
use fdl_core::vfs::PathExt as _;
use gpui::*;
use std::path::PathBuf;

/// What 打开文件… offers: Markdown, edited in place.
pub const MARKDOWN_EXTS: [&str; 3] = ["md", "markdown", "mdown"];

pub fn is_markdown_path(p: &std::path::Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| MARKDOWN_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Markdown files to open: the system open panel, or a scripted answer.
fn pick(cx: &mut App) -> impl std::future::Future<Output = Option<Vec<PathBuf>>> + use<> {
    let picked = crate::devtools::take_pick();
    #[cfg(not(target_os = "ios"))]
    let extensions = Some(MARKDOWN_EXTS.iter().map(|e| SharedString::from(*e)).collect());
    let prompt = picked.is_none().then(|| cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some(tr("打开", "Open").into()), #[cfg(not(target_os = "ios"))] extensions }));
    async move {
        match prompt {
            Some(p) => p.await.ok().and_then(Result::ok).flatten(),
            None => picked,
        }
    }
}

impl Workspace {
    /// 本机文件 (single files and opened folders, not what is in them), last opened first.
    pub(crate) fn local_entries(&self) -> Vec<&Entry> {
        let mut v: Vec<&Entry> = self.entries.iter().filter(|e| e.meta.local() && e.meta.parent.is_none()).collect();
        v.sort_by(|a, b| b.meta.opened.cmp(&a.meta.opened).then_with(|| a.id.cmp(&b.id)));
        v
    }

    /// 打开文件…: pick Markdown files and open them in place.
    pub fn open_file_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.row_menu = None;
        let paths = pick(cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(paths) = paths.await else { return };
            let _ = this.update_in(cx, |w, window, cx| w.open_paths(&paths, window, cx));
        })
        .detach();
    }

    /// Open Markdown files and folders where they are (the last file shows);
    /// anything else is imported as before.
    pub fn open_paths(&mut self, paths: &[PathBuf], window: &mut Window, cx: &mut Context<Self>) {
        let (dirs, files): (Vec<PathBuf>, Vec<PathBuf>) = paths.iter().cloned().partition(|p| p.fs_is_dir());
        let (md, other): (Vec<PathBuf>, Vec<PathBuf>) = files.into_iter().partition(|p| is_markdown_path(p));
        if !other.is_empty() { self.import_paths(&other, None, true, window, cx); }
        if !dirs.is_empty() { self.mount_dirs(&dirs, window, cx); }
        let mut last = None;
        for p in &md {
            match self.store.open_local(p) {
                Ok(id) => last = Some(id),
                Err(e) => self.show_toast_kind(ToastKind::Error, &crate::localized_format!("无法打开文件：{}", "Could not open file: {}", crate::settings::diagnostic(&e.to_string())), cx),
            }
        }
        self.entries = self.store.list();
        self.sort();
        if let Some(id) = last {
            self.folded[2] = false;
            self.open(&id, window, cx);
        }
        cx.notify();
    }

    pub(crate) fn open_relative(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_shared() { return; }
        let Some(source) = self.current.as_deref().and_then(|id| self.store.file_path(id)) else { return };
        let wiki = url.strip_prefix("quiet-wiki:");
        let raw = wiki.map(|s| s.split('|').next().unwrap_or(s)).unwrap_or(url);
        let (name, anchor) = raw.split_once('#').unwrap_or((raw, ""));
        let name = crate::markdown::percent_decode(name);
        let Some(dir) = source.parent() else { return };
        let mut path = if name.is_empty() { source.clone() } else { dir.join(&name) };
        if wiki.is_some() && path.extension().is_none() { path.set_extension("md"); }
        if wiki.is_some() && !path.fs_is_file() && let Some(root) = self.store.folder_dir(None) {
            let mut from_root = root.join(&name);
            if from_root.extension().is_none() { from_root.set_extension("md"); }
            if from_root.fs_is_file() { path = from_root; }
        }
        let Some(path) = path.fs_canonicalize().ok().filter(|p| p.fs_is_file()) else {
            self.show_toast_kind(ToastKind::Error, tr("文件不存在", "File not found"), cx);
            return;
        };
        if !is_markdown_path(&path) {
            if matches!(path.extension().and_then(|s| s.to_str()).map(str::to_ascii_lowercase).as_deref(), Some("pdf" | "html" | "htm" | "ppt" | "pptx" | "docx" | "xlsx" | "txt" | "png" | "jpg" | "jpeg")) {
                cx.open_with_system(&path);
            } else { cx.reveal_path(&path); }
            return;
        }
        let existing = self.entries.iter().find(|e| self.store.file_path(&e.id).is_some_and(|p| p == path)).map(|e| e.id.clone());
        let id = existing.or_else(|| self.store.open_local(&path).ok());
        let Some(id) = id else { return };
        self.entries = self.store.list();
        self.sort();
        if self.try_open(&id, window, cx) && !anchor.is_empty() && let Some(ed) = &self.editor {
            let key = crate::markdown::percent_decode(anchor).to_lowercase();
            ed.update(cx, |e, cx| {
                let mut found = None;
                e.core.doc.for_each_block(|b| {
                    if found.is_none() && matches!(b.kind, crate::model::BlockKind::Heading { .. }) {
                        let label = b.text.text.trim().to_lowercase();
                        if label == key || label.replace(' ', "-") == key { found = Some(b.id); }
                    }
                });
                if let Some(id) = found { e.scroll_to_block(id, cx); }
            });
        }
    }

    /// 导入到文档库: a Markdown copy at the top of 我的文档.
    pub(crate) fn import_local(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.row_menu = None;
        let Some(doc) = self.doc_of(id, cx) else { return };
        let front = self.store.front_matter_of(id);
        match self.store.create_markdown(&doc, &front) {
            Ok(new) => {
                self.entries = self.store.list();
                self.sort();
                self.open(&new, window, cx);
                self.show_toast_kind(ToastKind::Info, tr("已导入到文档库，原文件未改动", "Imported into the library; the original file is unchanged"), cx);
            }
            Err(e) => self.show_toast_kind(ToastKind::Error, &crate::localized_format!("导入失败：{}", "Could not import: {}", crate::settings::diagnostic(&e.to_string())), cx),
        }
        cx.notify();
    }

    /// 从列表中移除: forget the file; it stays where it is.
    fn forget_local(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.row_menu = None;
        self.delete_docs(&[id.to_string()], window, cx);
    }

    /// The "…" menu of a 本机文件 row.
    pub(crate) fn local_row_menu(&self, m: RowMenu, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (import, rename, reveal, forget) = (m.id.clone(), m.id.clone(), m.id.clone(), m.id.clone());
        let row = |id: &'static str, glyph: &'static str, label: &'static str| crate::ui::menu_row(id, SharedString::from(id)).child(icon(glyph, 16., Theme::icon())).child(label);
        let pos = if m.pos.x < px(0.) { point(-m.pos.x, m.pos.y + px(22.)) } else { m.pos };
        let shown = crate::local_folders::show_in_files();
        vec![deferred(anchored().position(pos).snap_to_window_with_margin(px(8.)).child(popover().id("local-row-menu").occlude().min_w(px(200.)).py(px(crate::ui::MENU_PAD_Y))
            .on_mouse_down_out(cx.listener(|w, _, _, cx| { w.row_menu = None; cx.notify(); }))
            .child(row("lr-import", "folder-input", tr("导入到文档库", "Import into library")).on_click(cx.listener(move |w, _, window, cx| w.import_local(&import, window, cx))))
            .child(row("lr-rename", "square-pen", tr("重命名", "Rename")).on_click(cx.listener(move |w, _, window, cx| w.start_rename(&rename, window, cx))))
            .child(row("lr-reveal", "folder", shown).on_click(cx.listener(move |w, _, _, cx| {
                w.row_menu = None;
                if let Some(p) = w.store.local_path(&reveal) { cx.reveal_path(&p); }
                cx.notify();
            })))
            .child(row("lr-forget", "x", tr("从列表中移除", "Remove from list")).on_click(cx.listener(move |w, _, window, cx| w.forget_local(&forget, window, cx))))))
            .with_priority(2).into_any_element()]
    }
}

/// Files the system asked us to open (Finder's Open With, a double-click,
/// the command line), waiting for a main window.
#[derive(Clone, Default)]
pub struct Incoming(std::rc::Rc<std::cell::RefCell<Vec<PathBuf>>>,
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))] std::rc::Rc<std::cell::RefCell<Vec<String>>>);

impl Incoming {
    /// Markdown files and folders named on the command line (Windows and Linux "Open with").
    pub fn from_args() -> Self {
        let me = Self::default();
        me.0.borrow_mut().extend(std::env::args_os().skip(1).map(PathBuf::from).filter(|p| (is_markdown_path(p) && p.fs_is_file()) || (!crate::FEI_NOTES && p.fs_is_dir())));
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        me.1.borrow_mut().extend(std::env::args().skip(1).filter(|url| crate::sharing::links::incoming(url)));
        me
    }

    pub fn push_urls(&self, urls: Vec<String>) {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        {
            let mut invitations = self.1.borrow_mut(); let available = 8usize.saturating_sub(invitations.len());
            invitations.extend(urls.iter().filter(|url| crate::sharing::links::incoming(url)).take(available).cloned());
        }
        let paths = urls.iter().filter_map(|u| u.strip_prefix("file://")).map(|p| PathBuf::from(crate::markdown::percent_decode(p.trim_end_matches('/'))));
        self.0.borrow_mut().extend(paths.filter(|p| is_markdown_path(p) || (!crate::FEI_NOTES && p.fs_is_dir())));
    }

    /// Hand waiting files to the front main window, now and whenever more arrive.
    pub fn deliver(self, cx: &mut App) {
        cx.spawn(async move |cx| loop {
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            if !self.1.borrow().is_empty() {
                cx.update(|cx| {
                    let target = cx.active_window().and_then(|w| w.downcast::<Workspace>())
                        .or_else(|| cx.windows().into_iter().find_map(|w| w.downcast::<Workspace>()));
                    let target = match target {
                        Some(target) => target,
                        None => { crate::open_main_window(false, None, cx); return; }
                    };
                    if target.read(cx).is_ok_and(|w| w.sharing_request_busy()) { return; }
                    let link = self.1.borrow_mut().remove(0);
                    let _ = target.update(cx, |w, window, cx| {
                        window.activate_window();
                        w.receive_invitation(link, window, cx);
                    });
                });
            }
            if !self.0.borrow().is_empty() {
                cx.update(|cx| {
                    let personal = |w: &WindowHandle<Workspace>| w.read(cx).is_ok_and(|ws| !ws.is_shared());
                    let target = cx.active_window().and_then(|w| w.downcast::<Workspace>()).filter(personal)
                        .or_else(|| cx.windows().into_iter().filter_map(|w| w.downcast::<Workspace>()).find(personal));
                    let Some(target) = target else { return };
                    let paths = std::mem::take(&mut *self.0.borrow_mut());
                    let _ = target.update(cx, |w, window, cx| {
                        window.activate_window();
                        w.open_paths(&paths, window, cx);
                    });
                });
            }
            cx.background_executor().timer(std::time::Duration::from_millis(250)).await;
        })
        .detach();
    }
}
