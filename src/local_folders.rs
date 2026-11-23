//! 工作区 roots from the window's side: 打开文件夹… (or the Finder's Open
//! With, or a folder dropped on the window) adds a root folder to the
//! sidebar, edited where it is; 新建文件夹… makes one first; 从侧栏移除
//! forgets one and leaves every file alone. See docs/design/workspaces.md.

use crate::app::{RowMenu, View, Workspace};
use crate::theme::Theme;
use crate::ui::{icon, popover};
use crate::settings::tr;
use crate::ui::ToastKind;
use fdl_core::workspace::AddError;
use gpui::*;
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

/// Where a new root folder goes: the system save panel, or a scripted answer.
fn pick_new(cx: &mut App) -> impl std::future::Future<Output = Option<PathBuf>> + use<> {
    let picked = crate::devtools::take_pick().and_then(|v| v.into_iter().next());
    let base = dirs::document_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default());
    let prompt = picked.is_none().then(|| cx.prompt_for_new_path(&base, Some("Quiet.md")));
    async move {
        match prompt {
            Some(p) => p.await.ok().and_then(Result::ok).flatten(),
            None => picked,
        }
    }
}

impl Workspace {
    /// 打开文件夹…: pick folders and add them to the sidebar where they are.
    pub fn open_folder_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.row_menu = None;
        let paths = pick(cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(paths) = paths.await else { return };
            let _ = this.update_in(cx, |w, window, cx| w.add_roots(&paths, window, cx));
        })
        .detach();
    }

    /// 新建文件夹…: make an empty folder and add it to the sidebar.
    pub fn new_root_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.row_menu = None;
        let path = pick_new(cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(path) = path.await else { return };
            let _ = this.update_in(cx, |w, window, cx| {
                match w.store.create_root(&path) {
                    Ok(key) => w.root_added(&key, window, cx),
                    Err(e) => w.add_failed(&path, e, window, cx),
                }
            });
        })
        .detach();
    }

    /// Add `dirs` as root folders; the sidebar shows the last one.
    pub(crate) fn add_roots(&mut self, dirs: &[PathBuf], window: &mut Window, cx: &mut Context<Self>) {
        for d in dirs {
            let fresh = !d.join(crate::store::INTERNAL).exists();
            match self.store.add_root(d) {
                Ok(key) => {
                    self.root_added(&key, window, cx);
                    // A folder of the user's own gets a hidden .quiet: say so once (W2).
                    if fresh { self.show_toast(tr("已在这个文件夹里建了隐藏的 .quiet 文件夹，存放置顶、排序、历史版本和回收站", "Quiet.md keeps pins, order, history and the Trash for this folder in a hidden .quiet folder inside it"), cx); }
                }
                Err(e) => self.add_failed(d, e, window, cx),
            }
        }
    }

    fn root_added(&mut self, key: &str, _window: &mut Window, cx: &mut Context<Self>) {
        self.folded_roots.remove(key);
        if self.store.set_active_root(key) { cx.defer(crate::settings::root_switched); }
        self.entries = self.store.list();
        self.sort();
        self.show_store_warnings_from(&self.store.take_warnings(), cx);
        cx.notify();
    }

    /// A folder that is already open, or inside one: shown there instead.
    fn add_failed(&mut self, dir: &Path, e: AddError, window: &mut Window, cx: &mut Context<Self>) {
        match e {
            AddError::Open(key) => { self.folded_roots.remove(&key); cx.notify(); }
            AddError::Inside(key) => {
                self.folded_roots.remove(&key);
                self.entries = self.store.list();
                self.sort();
                let found = self.entries.iter().find(|e| e.root == key && e.meta.folder.is_some() && self.store.folder_dir(Some(&e.id)).is_some_and(|p| p == dir)).map(|e| e.id.clone());
                if let Some(id) = found { self.reveal_in_tree(&id); self.open_folder(&id, window, cx); }
                self.show_toast(tr("这个文件夹已经在侧栏的文件夹里", "This folder is already inside a folder in the sidebar"), cx);
            }
            AddError::Holds(key) => {
                let name = self.store.roots().into_iter().find(|r| r.0 == key).map(|r| r.1).unwrap_or_default();
                self.show_toast_kind(ToastKind::Error, &crate::localized_format!("这个文件夹里有已经打开的「{}」，先把它从侧栏移除再打开", "This folder holds “{}”, which is already open; remove it from the sidebar first", name), cx);
            }
            AddError::Other(e) => self.show_toast_kind(ToastKind::Error, &crate::localized_format!("无法打开文件夹：{}", "Could not open folder: {}", crate::settings::diagnostic(&e)), cx),
        }
    }

    /// 从侧栏移除: the root leaves the sidebar; its folder stays as it is.
    pub(crate) fn remove_root(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let inside = |w: &Self, id: &Option<String>| id.as_deref().is_some_and(|id| w.store.root_of(id).as_deref() == Some(key));
        if inside(self, &self.current) {
            let retry = key.to_string();
            if !self.save_before(cx, move |w, window, cx| w.remove_root(&retry, window, cx)) { return; }
            self.current = None;
            self.editor = None;
            self.editor_sub = None;
            if self.view == View::Doc { self.view = View::Home; }
        }
        if inside(self, &self.folder_current) {
            self.folder_current = None;
            if self.view == View::Folder { self.view = View::Home; }
        }
        let _ = window;
        if let Err(e) = self.store.remove_root(key) { self.show_toast_kind(ToastKind::Error, &crate::settings::diagnostic(&e.to_string()), cx); }
        cx.defer(crate::settings::root_switched);
        self.entries = self.store.list();
        self.trash = self.store.list_trash();
        self.sort();
        cx.notify();
    }

    /// The "…" menu of a root's row (two or more roots).
    pub(crate) fn root_row_menu(&self, key: &str, m: RowMenu, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let row = |id: &'static str, glyph: &'static str, label: &'static str| crate::ui::menu_row(id, SharedString::from(id)).child(icon(glyph, 16., Theme::icon())).child(label);
        let pos = if m.pos.x < px(0.) { point(-m.pos.x, m.pos.y + px(22.)) } else { m.pos };
        let open = self.store.roots().into_iter().find(|r| r.0 == key);
        let (name, path, missing) = open.map(|r| (r.1, r.2, r.3.is_some())).unwrap_or_default();
        let (doc, folder, k1, k2) = (m.id.clone(), m.id.clone(), key.to_string(), key.to_string());
        let mut menu = popover().id("root-row-menu").occlude().min_w(px(200.)).py(px(crate::ui::MENU_PAD_Y))
            .on_mouse_down_out(cx.listener(|w, _, _, cx| { w.row_menu = None; cx.notify(); }));
        if missing {
            menu = menu.child(row("rr-retry", "folder", tr("重新查找", "Look again")).on_click(cx.listener(move |w, _, _, cx| {
                w.row_menu = None;
                w.store.retry_root(&k1);
                w.entries = w.store.list();
                w.sort();
                cx.notify();
            })));
        } else {
            menu = menu
                .child(row("rr-new-doc", "doc", tr("新建文档", "New document")).on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.create_doc(false, Some(doc.clone()), window, cx); })))
                .child(row("rr-new-folder", "folder", tr("新建文件夹", "New folder")).on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.new_folder(crate::folders::CreateTarget::Library(Some(folder.clone())), None, window, cx); })))
                .child(row("rr-reveal", "folder", show_in_files()).on_click(cx.listener(move |w, _, _, cx| { w.row_menu = None; cx.reveal_path(&path); cx.notify(); })));
        }
        menu = menu.child(row("rr-remove", "x", tr("从侧栏移除", "Remove from sidebar")).on_click(cx.listener(move |w, _, _, cx| {
            w.row_menu = None;
            w.confirm = Some(crate::app::Confirm::RemoveRoot(k2.clone(), name.clone()));
            cx.notify();
        })));
        vec![deferred(anchored().position(pos).snap_to_window_with_margin(px(8.)).child(menu)).with_priority(2).into_any_element()]
    }

    /// The sidebar with no root folder open: a way to open or make one.
    pub(crate) fn empty_roots(&self, cx: &mut Context<Self>) -> Div {
        let button = |id: &'static str, label: &'static str| crate::ui::menu_row(id, SharedString::from(id)).child(label);
        div().flex().flex_col().gap(px(2.)).px(px(4.)).pt(px(12.))
            .child(div().px(px(8.)).pb(px(6.)).text_size(px(crate::theme::ui_font())).line_height(px(crate::theme::ui_font() + 7.)).text_color(Theme::text_caption())
                .child(tr("还没有打开任何文件夹。笔记就是文件夹里的文件。", "No folder is open yet. Your notes are the files in a folder.")))
            .child(button("empty-open", tr("打开文件夹…", "Open folder…")).on_click(cx.listener(|w, _, window, cx| w.open_folder_dialog(window, cx))))
            .child(button("empty-new", tr("新建文件夹…", "New folder…")).on_click(cx.listener(|w, _, window, cx| w.new_root_dialog(window, cx))))
    }

    /// What the top of the tree is called where nothing names a root: the
    /// one root's folder name, else the root in front's.
    pub(crate) fn top_name(&self) -> String {
        let roots = self.store.roots();
        let key = if roots.len() == 1 { Some(roots[0].0.clone()) } else { self.store.active_root() };
        key.and_then(|k| roots.into_iter().find(|r| r.0 == k)).map(|r| r.1).unwrap_or_else(|| tr("我的文档", "My documents").to_string())
    }

    /// What root `key` is called: its folder's name.
    pub(crate) fn root_name(&self, key: &str) -> String {
        self.store.roots().into_iter().find(|r| r.0 == key).map(|r| r.1).unwrap_or_default()
    }

    /// The rows of 本机文件: single files opened where they are, last opened first.
    pub(crate) fn local_rows(&self) -> Vec<(&crate::store::Entry, usize)> {
        self.local_entries().into_iter().filter(|e| e.meta.folder.is_none()).map(|e| (e, 0)).collect()
    }
}
