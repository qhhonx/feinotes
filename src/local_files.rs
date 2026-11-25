//! 本机文件: Markdown files opened where they are (打开文件…, the Finder),
//! edited in place and listed under 本机文件, last opened first (folders
//! opened the same way are `local_folders.rs`). The library
//! only remembers them: 从列表中移除 never touches the file, and 导入到文档库
//! makes a copy.

use fdl_core::vfs::PathExt as _;
use crate::app::{RowMenu, Workspace};
use crate::settings::tr;
use crate::store::Entry;
use crate::ui::ToastKind;
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
    let extensions = Some(MARKDOWN_EXTS.iter().map(|e| SharedString::from(*e)).collect());
    let prompt = picked.is_none().then(|| cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some(tr("打开", "Open").into()), extensions }));
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
        if !dirs.is_empty() { self.add_folders(&dirs, window, cx); }
        let mut last = None;
        for p in &md {
            match self.store.open_local(p) {
                Ok(id) => last = Some(id),
                Err(e) => self.show_toast_kind(ToastKind::Error, &crate::localized_format!("无法打开文件：{}", "Could not open file: {}", crate::settings::diagnostic(&e.to_string())), cx),
            }
        }
        self.entries.set(self.store.list());
        self.sort();
        if let Some(id) = last {
            self.folded[2] = false;
            self.open(&id, window, cx);
        }
        cx.notify();
    }

    /// A relative link (`[周报](../工作/周报.md)`, `[报告](附件/报告.pdf)`,
    /// `[[page]]`): a document of the library, or a Markdown file, opens
    /// here; any other file opens in its app, and a folder, or a file that
    /// would run a program, shows in the Finder.
    pub(crate) fn open_relative(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = self.current.as_deref().and_then(|id| self.store.file_path(id)) else { return };
        // A move may have just rewritten the link on disk.
        let url = &self.follow_moved_links(cx).and_then(|m| m.get(url).cloned()).unwrap_or_else(|| url.to_string());
        let wiki = url.strip_prefix("quiet-wiki:");
        let (path, anchor) = match wiki {
            // `[[page]]`, looked up across the vault the way Obsidian does.
            Some(raw) => {
                let (name, anchor) = crate::markdown::wiki_parts(raw);
                let vault = crate::markdown::wiki_vault(&source, self.store.folder_dir(None).as_deref(), &self.store.top_dirs());
                (crate::markdown::resolve_wiki(name, &source, &vault).unwrap_or_default(), anchor.to_string())
            }
            None => {
                let (name, anchor) = url.split_once('#').unwrap_or((url, ""));
                let name = crate::markdown::percent_decode(name);
                let Some(dir) = source.parent() else { return };
                (if name.is_empty() { source.clone() } else { dir.join(&name) }, crate::markdown::percent_decode(anchor))
            }
        };
        let Some(path) = path.fs_canonicalize().ok().filter(|p| p.fs_exists()) else {
            self.show_toast_kind(ToastKind::Error, tr("文件不存在", "File not found"), cx);
            return;
        };
        let existing = self.entries.iter().find(|e| self.store.file_path(&e.id)
            .is_some_and(|p| p.file_name() == path.file_name() && p.fs_canonicalize().is_ok_and(|p| p == path))).map(|e| e.id.clone());
        if existing.is_none() && !(is_markdown_path(&path) && path.fs_is_file()) {
            if path.fs_is_file() && !runs_program(&path) { cx.open_with_system(&path); } else { cx.reveal_path(&path); }
            return;
        }
        let id = existing.or_else(|| self.store.open_local(&path).ok());
        let Some(id) = id else { return };
        self.entries.set(self.store.list());
        self.sort();
        if self.try_open(&id, window, cx) && !anchor.is_empty() && let Some(ed) = &self.editor {
            let key = anchor.to_lowercase();
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

    /// A copy `id` of 本机文件 `src` keeps its times, as an import does.
    pub(crate) fn keep_times(&mut self, id: &str, src: &std::path::Path) {
        let _ = self.store.with(Some(id), |s| { crate::transfer::keep_times(s, id, src); Ok(()) });
    }

    /// 导入到文档库: a Markdown copy at the top of 我的文档.
    pub(crate) fn import_local(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.row_menu = None;
        let Some(doc) = self.doc_of(id, cx) else { return };
        let front = self.store.front_matter_of(id);
        match self.store.create_markdown(&doc, &front) {
            Ok(new) => {
                if let Some(src) = self.store.local_path(id) { self.keep_times(&new, &src); }
                self.entries.set(self.store.list());
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
        let row = |id: &'static str, glyph: &'static str, label: &'static str| crate::design::MenuItem::new(id).icon(glyph).label(label);
        let pos = m.pos;
        let shown = crate::local_folders::show_in_files();
        vec![deferred(anchored().anchor(m.corner).position(pos).snap_to_window_with_margin(px(8.)).child(crate::design::Popover::new("local-row-menu").menu().fit(crate::design::room(pos, m.corner)).min_w(px(200.))
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

/// Opening it would run something (an app, a script, an installer): a link
/// in a note someone sent must not, so it only shows in the Finder.
fn runs_program(p: &std::path::Path) -> bool {
    const RUN: &[&str] = &["app", "command", "tool", "terminal", "sh", "bash", "zsh", "csh", "fish", "py", "pyw", "pl", "rb", "php", "js", "jse",
        "scpt", "applescript", "workflow", "action", "pkg", "mpkg", "dmg", "jar", "exe", "com", "bat", "cmd", "msi", "msix", "ps1", "vbs", "vbe",
        "wsf", "wsh", "scr", "pif", "hta", "cpl", "reg", "lnk", "desktop", "appimage", "run", "url", "webloc", "inetloc", "fileloc"];
    // No extension: it may be a program.
    p.extension().and_then(|e| e.to_str()).is_none_or(|e| RUN.contains(&e.to_ascii_lowercase().as_str()))
}

/// Files the system asked us to open (Finder's Open With, a double-click,
/// the command line), waiting for a main window.
#[derive(Clone, Default)]
pub struct Incoming(std::rc::Rc<std::cell::RefCell<Vec<PathBuf>>>);

impl Incoming {
    /// Markdown files and folders named on the command line (Windows and Linux "Open with").
    pub fn from_args() -> Self {
        let me = Self::default();
        me.0.borrow_mut().extend(std::env::args_os().skip(1).map(PathBuf::from).filter(|p| (is_markdown_path(p) && p.fs_is_file()) || p.fs_is_dir()));
        me
    }

    pub fn push_urls(&self, urls: Vec<String>) {
        let paths = urls.iter().filter_map(|u| u.strip_prefix("file://")).map(|p| PathBuf::from(crate::markdown::percent_decode(p.trim_end_matches('/'))));
        self.0.borrow_mut().extend(paths.filter(|p| is_markdown_path(p) || p.fs_is_dir()));
    }

    /// Hand waiting files to the front main window, now and whenever more arrive.
    pub fn deliver(self, cx: &mut App) {
        cx.spawn(async move |cx| loop {
            if !self.0.borrow().is_empty() {
                cx.update(|cx| {
                    let target = cx.active_window().and_then(|w| w.downcast::<Workspace>())
                        .or_else(|| cx.windows().into_iter().find_map(|w| w.downcast::<Workspace>()));
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

#[cfg(test)]
mod tests {
    #[test]
    fn programs_only_show() {
        for p in ["a/报告.pdf", "幻灯片.PPTX", "页.html", "表.xlsx"] { assert!(!super::runs_program(std::path::Path::new(p)), "{p}"); }
        for p in ["安装.pkg", "x.command", "run.SH", "a.app", "setup.exe", "无扩展名"] { assert!(super::runs_program(std::path::Path::new(p)), "{p}"); }
    }
}
