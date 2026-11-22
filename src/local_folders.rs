//! 本机文件夹: folders opened where they are (打开文件夹…, the Finder's Open
//! With, a folder dropped on 本机文件), listed under 本机文件 below the single
//! files, each as a tree. Notes and folders in them are made, renamed and
//! moved on disk; deleting moves to the system's trash; 从列表中移除 only
//! forgets the folder. Between them and 我的文档 things are copied, never moved.

use crate::app::{RowMenu, View, Workspace};
use crate::settings::tr;
use crate::store::Entry;
use crate::theme::Theme;
use crate::ui::{ToastKind, icon, popover};
use gpui::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// "在访达中显示", in the system's words.
pub(crate) fn show_in_files() -> &'static str {
    if cfg!(target_os = "macos") { tr("在访达中显示", "Show in Finder") } else if cfg!(target_os = "windows") { tr("在文件资源管理器中显示", "Show in File Explorer") } else { tr("在文件管理器中显示", "Show in File Manager") }
}

/// Folders to open: the system open panel, or a scripted answer.
fn pick(cx: &mut App) -> impl std::future::Future<Output = Option<Vec<PathBuf>>> + use<> {
    let picked = crate::devtools::take_pick();
    let prompt = picked.is_none().then(|| cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: true, prompt: Some(tr("打开", "Open").into()), #[cfg(not(target_os = "ios"))] extensions: None }));
    async move {
        match prompt {
            Some(p) => p.await.ok().and_then(Result::ok).flatten(),
            None => picked,
        }
    }
}

impl Workspace {
    /// 打开文件夹…: pick folders and open them where they are.
    pub fn open_folder_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.row_menu = None;
        let paths = pick(cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(paths) = paths.await else { return };
            let _ = this.update_in(cx, |w, window, cx| w.mount_dirs(&paths, window, cx));
        })
        .detach();
    }

    /// List `dirs` under 本机文件, edited where they are; the last one's page shows.
    pub(crate) fn mount_dirs(&mut self, dirs: &[PathBuf], window: &mut Window, cx: &mut Context<Self>) {
        if crate::FEI_NOTES || self.is_shared() { return; }
        let mut last = None;
        for d in dirs {
            match self.store.mount(d) {
                Ok(id) => last = Some(id),
                Err(e) => self.show_toast_kind(ToastKind::Error, &crate::localized_format!("无法打开文件夹：{}", "Could not open folder: {}", crate::settings::diagnostic(&e.to_string())), cx),
            }
        }
        self.entries = self.store.list();
        self.sort();
        if let Some(id) = last {
            self.folded[2] = false;
            self.expanded.insert(id.clone());
            self.open_folder(&id, window, cx);
        }
        cx.notify();
    }

    /// What is in opened folder (or folder in one) `id`, as the sidebar
    /// lists it: folders first, then by name.
    fn mounted_children<'a>(&'a self, kids: &HashMap<&str, Vec<&'a Entry>>, id: &str) -> Vec<&'a Entry> {
        let mut v = kids.get(id).cloned().unwrap_or_default();
        v.sort_by_cached_key(|e| (e.meta.folder.is_none(), e.title.to_lowercase(), e.id.clone()));
        v
    }

    /// The rows of 本机文件: single files, last opened first, then each
    /// opened folder by name, open as far as it was opened.
    pub(crate) fn local_rows(&self) -> Vec<(&Entry, usize)> {
        let top: Vec<&Entry> = self.local_entries();
        let mut kids: HashMap<&str, Vec<&Entry>> = HashMap::new();
        for e in self.entries.iter().filter(|e| e.meta.mounted()) {
            if let Some(p) = e.meta.parent.as_deref() { kids.entry(p).or_default().push(e); }
        }
        let mut out = vec![];
        for e in top.iter().filter(|e| e.meta.folder.is_none()) { out.push((*e, 0)); }
        let mut roots: Vec<&Entry> = top.iter().filter(|e| e.meta.folder.is_some()).copied().collect();
        roots.sort_by_cached_key(|e| (e.title.to_lowercase(), e.id.clone()));
        let mut stack: Vec<(&Entry, usize)> = roots.into_iter().rev().map(|e| (e, 0)).collect();
        while let Some((e, d)) = stack.pop() {
            out.push((e, d));
            if e.meta.folder.is_some() && self.expanded.contains(&e.id) {
                stack.extend(self.mounted_children(&kids, &e.id).into_iter().rev().map(|c| (c, d + 1)));
            }
        }
        out
    }

    /// Whether a mounted folder row has anything in it to fold.
    pub(crate) fn mounted_has_children(&self, id: &str) -> bool {
        self.entries.iter().any(|e| e.meta.mounted() && e.meta.parent.as_deref() == Some(id))
    }

    /// Fold or unfold a folder under 本机文件.
    pub(crate) fn toggle_mounted(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.expanded.remove(id) { self.expanded.insert(id.to_string()); }
        cx.notify();
    }

    /// The open document or folder page is at or under `path`: closed.
    fn close_inside(&mut self, path: &Path) {
        let inside = |w: &Self, id: &str| w.store.local_path(id).is_some_and(|p| p.starts_with(path));
        if self.current.as_deref().is_some_and(|c| inside(self, c)) {
            self.save_task = None;
            self.unsaved = false;
            self.current = None;
            self.editor = None;
            self.editor_sub = None;
            if self.view == View::Doc { self.view = View::Home; }
        }
        if self.folder_current.as_deref().is_some_and(|f| inside(self, f)) {
            self.folder_current = None;
            if self.view == View::Folder { self.view = View::Home; }
        }
    }

    /// 移到废纸篓: a file or folder in an opened folder goes to the system's trash.
    pub(crate) fn trash_mounted(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.row_menu = None;
        let Some(path) = self.store.local_path(id) else { return };
        if self.current.as_deref().and_then(|c| self.store.local_path(c)).is_some_and(|p| p.starts_with(&path)) {
            let retry = id.to_string();
            if !self.save_before(cx, move |w, window, cx| w.trash_mounted(&retry, window, cx)) { return; }
        }
        match crate::system_trash::trash(&path) {
            Ok(()) => {
                self.close_inside(&path);
                self.store.forget_mounted(id);
                self.entries = self.store.list();
                self.sort();
                let _ = window;
                self.show_toast(if cfg!(target_os = "windows") { tr("已移到回收站", "Moved to the Recycle Bin") } else { tr("已移到废纸篓", "Moved to the Trash") }, cx);
            }
            Err(e) => self.show_toast_kind(ToastKind::Error, &crate::localized_format!("无法删除：{}", "Could not delete: {}", crate::settings::diagnostic(&e.to_string())), cx),
        }
        cx.notify();
    }

    /// 从列表中移除 an opened folder: forgotten, every file left where it is.
    fn forget_mount(&mut self, id: &str, cx: &mut Context<Self>) {
        self.row_menu = None;
        let Some(path) = self.store.local_path(id) else { return };
        if self.current.as_deref().and_then(|c| self.store.local_path(c)).is_some_and(|p| p.starts_with(&path)) && !self.flush(cx) { return; }
        self.close_inside(&path);
        if let Err(e) = self.store.delete(id, None) {
            self.show_toast_kind(ToastKind::Error, &crate::settings::diagnostic(&e.to_string()), cx);
        }
        self.entries = self.store.list();
        self.sort();
        cx.notify();
    }

    /// A row of 本机文件 let go on a folder under it: moved there on disk.
    pub(crate) fn move_mounted(&mut self, id: &str, folder: &str, cx: &mut Context<Self>) {
        if id == folder { return; }
        if !self.store.is_mounted(id) {
            self.show_toast_kind(ToastKind::Info, tr("单独打开的文件不能移进文件夹", "A file opened on its own can't be moved into a folder"), cx);
            return;
        }
        if !self.flush(cx) { return; }
        match self.store.set_parent(id, Some(folder)) {
            Ok(_) => { self.expanded.insert(folder.to_string()); }
            Err(e) => self.show_toast_kind(ToastKind::Error, &crate::localized_format!("移动失败：{}", "Could not move document: {}", crate::settings::diagnostic(&e.to_string())), cx),
        }
        self.entries = self.store.list();
        self.sort();
        cx.notify();
    }

    /// 导入到文档库 a folder under 本机文件: a copy at the top of 我的文档.
    fn import_mounted_folder(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.row_menu = None;
        let Some(path) = self.store.local_path(id) else { return };
        self.import_folder(&path, None, true, window, cx);
    }

    /// The "…" menu of a row in an opened folder (or of the folder itself).
    pub(crate) fn mounted_row_menu(&self, m: RowMenu, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(e) = self.entries.iter().find(|e| e.id == m.id) else { return vec![] };
        let (folder, root) = (e.meta.folder.is_some(), e.meta.parent.is_none());
        let row = |id: &'static str, glyph: &'static str, label: &'static str| crate::ui::menu_row(id, SharedString::from(id)).child(icon(glyph, 16., Theme::icon())).child(label);
        let divider = || div().mx(px(8.)).my(px(5.)).h(px(1.)).bg(Theme::line_border());
        let pos = if m.pos.x < px(0.) { point(-m.pos.x, m.pos.y + px(22.)) } else { m.pos };
        let id = m.id.clone();
        let ids = || (id.clone(), id.clone());
        let mut menu = popover().id("mounted-row-menu").occlude().w(px(210.)).py(px(crate::ui::MENU_PAD_Y))
            .on_mouse_down_out(cx.listener(|w, _, _, cx| { w.row_menu = None; cx.notify(); }));
        if folder {
            let (a, b) = ids();
            menu = menu
                .child(row("mr-new-md", "doc-md", tr("新建 Markdown", "New Markdown")).on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.create_doc(true, Some(a.clone()), window, cx); })))
                .child(row("mr-new-folder", "folder", tr("新建文件夹", "New folder")).on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.new_folder(crate::folders::CreateTarget::Library(Some(b.clone())), None, window, cx); })))
                .child(divider());
        }
        if !root {
            let (a, b) = ids();
            menu = menu
                .child(row("mr-import", "folder-input", tr("导入到文档库", "Import into library")).on_click(cx.listener(move |w, _, window, cx| {
                    if folder { w.import_mounted_folder(&a, window, cx) } else { w.import_local(&a, window, cx) }
                })))
                .child(row("mr-rename", "square-pen", tr("重命名", "Rename")).on_click(cx.listener(move |w, _, window, cx| w.start_rename(&b, window, cx))));
        }
        let (a, b) = ids();
        menu = menu.child(row("mr-reveal", "folder", show_in_files()).on_click(cx.listener(move |w, _, _, cx| {
            w.row_menu = None;
            if let Some(p) = w.store.local_path(&a) { cx.reveal_path(&p); }
            cx.notify();
        })));
        menu = if root {
            menu.child(row("mr-forget", "x", tr("从列表中移除", "Remove from list")).on_click(cx.listener(move |w, _, _, cx| w.forget_mount(&b, cx))))
        } else {
            let label = if cfg!(target_os = "windows") { tr("移到回收站", "Move to Recycle Bin") } else { tr("移到废纸篓", "Move to Trash") };
            menu.child(divider()).child(row("mr-trash", "bin", label).on_click(cx.listener(move |w, _, window, cx| w.trash_mounted(&b, window, cx))))
        };
        vec![deferred(anchored().position(pos).snap_to_window_with_margin(px(8.)).child(menu)).with_priority(2).into_any_element()]
    }
}
