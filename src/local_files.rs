//! 本机文件: Markdown files opened where they are (打开文件…, the Finder),
//! edited in place and listed under 我的文档, last opened first. The library
//! only remembers them: 从列表中移除 never touches the file, and 导入到文档库
//! makes a copy.

use crate::app::{RowMenu, Workspace};
use crate::settings::tr;
use crate::store::Entry;
use crate::theme::Theme;
use crate::ui::{ToastKind, icon, popover};
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
    /// 本机文件, last opened first.
    pub(crate) fn local_entries(&self) -> Vec<&Entry> {
        let mut v: Vec<&Entry> = self.entries.iter().filter(|e| e.meta.local()).collect();
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

    /// Open Markdown files where they are (the last one shows); anything
    /// else is imported as before.
    pub fn open_paths(&mut self, paths: &[PathBuf], window: &mut Window, cx: &mut Context<Self>) {
        let (md, other): (Vec<PathBuf>, Vec<PathBuf>) = paths.iter().cloned().partition(|p| is_markdown_path(p));
        if !other.is_empty() { self.import_paths(&other, None, true, window, cx); }
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

    /// 导入到文档库: a Markdown copy at the top of 我的文档.
    fn import_local(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
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
        let shown = if cfg!(target_os = "macos") { tr("在访达中显示", "Show in Finder") } else if cfg!(target_os = "windows") { tr("在文件资源管理器中显示", "Show in File Explorer") } else { tr("在文件管理器中显示", "Show in File Manager") };
        vec![deferred(anchored().position(pos).snap_to_window_with_margin(px(8.)).child(popover().id("local-row-menu").occlude().w(px(200.)).py(px(crate::ui::MENU_PAD_Y))
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
pub struct Incoming(std::rc::Rc<std::cell::RefCell<Vec<PathBuf>>>);

impl Incoming {
    /// Markdown files named on the command line (Windows and Linux "Open with").
    pub fn from_args() -> Self {
        let me = Self::default();
        me.0.borrow_mut().extend(std::env::args_os().skip(1).map(PathBuf::from).filter(|p| is_markdown_path(p) && p.is_file()));
        me
    }

    pub fn push_urls(&self, urls: Vec<String>) {
        let paths = urls.iter().filter_map(|u| u.strip_prefix("file://")).map(|p| PathBuf::from(crate::markdown::percent_decode(p)));
        self.0.borrow_mut().extend(paths.filter(|p| is_markdown_path(p)));
    }

    /// Hand waiting files to the front main window, now and whenever more arrive.
    pub fn deliver(self, cx: &mut App) {
        cx.spawn(async move |cx| loop {
            if !self.0.borrow().is_empty() {
                let _ = cx.update(|cx| {
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
