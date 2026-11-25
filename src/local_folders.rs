//! 主页's top-level folders from the window's side: 打开文件夹… (or the Finder's Open
//! With, or a folder dropped on the window) adds a top-level folder to the
//! sidebar, edited where it is (a new one is made in the system's open panel); 从侧栏移除
//! forgets one and leaves every file alone. See docs/design/workspaces.md.

use crate::app::{RowMenu, View, Workspace};
use crate::theme::Theme;
use crate::settings::tr;
use crate::ui::ToastKind;
use fdl_core::home::{AddError, RenameError};
use fdl_core::shown::Survey;
use gpui::{prelude::FluentBuilder, *};
use std::path::{Path, PathBuf};

/// "在访达中显示", in the system's words.
pub(crate) fn show_in_files() -> &'static str {
    if cfg!(target_os = "macos") { tr("在访达中显示", "Show in Finder") } else if cfg!(target_os = "windows") { tr("在文件资源管理器中显示", "Show in File Explorer") } else { tr("在文件管理器中显示", "Show in File Manager") }
}

/// Folders to open: the system open panel, or a scripted answer.
fn pick(cx: &mut App) -> impl std::future::Future<Output = Option<Vec<PathBuf>>> + use<> {
    let picked = crate::devtools::take_pick();
    let prompt = picked.is_none().then(|| cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: true, prompt: Some(tr("打开", "Open").into()), extensions: None }));
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
        if self.still_loading(cx) { return }
        let paths = pick(cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(paths) = paths.await else { return };
            let _ = this.update_in(cx, |w, window, cx| w.add_folders(&paths, window, cx));
        })
        .detach();
    }

    /// Add `dirs` as top-level folders; the sidebar shows the last one. A folder
    /// added for the first time is looked at first, off the main thread: a code
    /// project or a very big one is added only once the user says so (`Confirm::AddFolder`).
    pub(crate) fn add_folders(&mut self, dirs: &[PathBuf], window: &mut Window, cx: &mut Context<Self>) {
        let dirs = dirs.to_vec();
        let task = cx.background_executor().spawn(async move {
            dirs.into_iter().map(|d| { let s = if d.join(crate::store::INTERNAL).exists() { Survey::Plain } else { fdl_core::shown::survey(&d) }; (d, s) }).collect::<Vec<_>>()
        });
        cx.spawn_in(window, async move |this, cx| {
            let looked = task.await;
            let _ = this.update_in(cx, |w, window, cx| {
                let (plain, ask): (Vec<_>, Vec<_>) = looked.into_iter().partition(|(_, s)| *s == Survey::Plain);
                w.add_folders_now(&plain.into_iter().map(|(d, _)| d).collect::<Vec<_>>(), window, cx);
                if !ask.is_empty() { w.confirm = Some(crate::app::Confirm::AddFolder(ask)); cx.notify(); }
            });
        }).detach();
    }

    /// 添加 in `Confirm::AddFolder`: the first folder asked about is added, then the next is asked about.
    pub(crate) fn add_asked_folder(&mut self, mut ask: Vec<(PathBuf, Survey)>, window: &mut Window, cx: &mut Context<Self>) {
        if ask.is_empty() { return }
        let (dir, _) = ask.remove(0);
        self.add_folders_now(&[dir], window, cx);
        if !ask.is_empty() { self.confirm = Some(crate::app::Confirm::AddFolder(ask)); }
    }

    fn add_folders_now(&mut self, dirs: &[PathBuf], window: &mut Window, cx: &mut Context<Self>) {
        for d in dirs {
            match self.store.add_folder(d) {
                Ok(key) => {
                    self.folder_added(&key, window, cx);
                    // 打开 is not 导入: say that nothing was copied.
                    let msg = crate::localized_format!("已打开「{}」：在原位置使用，没有复制", "Opened “{}”: used where it is, not copied", self.folder_name(&key));
                    self.show_toast(&msg, cx);
                }
                Err(e) => self.add_failed(d, e, window, cx),
            }
        }
    }

    fn folder_added(&mut self, key: &str, _window: &mut Window, cx: &mut Context<Self>) {
        self.folded_tops.remove(key);
        if self.store.set_active_top(key) { cx.defer(crate::settings::top_switched); }
        // Listed off the main thread, as a big library is at launch (`loading`): here at
        // once when quick, else every window takes it in when done (`Workspace::loaded`).
        if let Some(entries) = crate::loading::begin_added(&mut self.store, key, cx) {
            self.entries.set(entries);
            self.sort();
        }
        self.show_store_warnings_from(&self.store.take_warnings(), cx);
        cx.notify();
    }

    /// A folder that is already open, or inside one: shown there instead.
    fn add_failed(&mut self, dir: &Path, e: AddError, window: &mut Window, cx: &mut Context<Self>) {
        match e {
            AddError::Open(key) => { self.folded_tops.remove(&key); cx.notify(); }
            AddError::Inside(key) => {
                self.folded_tops.remove(&key);
                self.entries.set(self.store.list());
                self.sort();
                let found = self.entries.iter().find(|e| e.top == key && e.meta.folder.is_some() && self.store.folder_dir(Some(&e.id)).is_some_and(|p| p == dir)).map(|e| e.id.clone());
                if let Some(id) = found { self.reveal_in_tree(&id); self.open_folder(&id, window, cx); }
                self.show_toast(tr("这个文件夹已经在侧栏的文件夹里", "This folder is already inside a folder in the sidebar"), cx);
            }
            AddError::Holds(key) => {
                let name = self.store.top_folders().into_iter().find(|r| r.0 == key).map(|r| r.1).unwrap_or_default();
                self.show_toast_kind(ToastKind::Error, &crate::localized_format!("这个文件夹里有已经打开的「{}」，先把它从侧栏移除再打开", "This folder holds “{}”, which is already open; remove it from the sidebar first", name), cx);
            }
            AddError::Other(e) => self.show_toast_kind(ToastKind::Error, &crate::localized_format!("无法打开文件夹：{}", "Could not open folder: {}", crate::settings::diagnostic(&e)), cx),
        }
    }

    /// 从侧栏移除: the top-level folder leaves the sidebar; its folder stays as it is.
    pub(crate) fn remove_folder(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let inside = |w: &Self, id: &Option<String>| id.as_deref().is_some_and(|id| w.store.top_of(id).as_deref() == Some(key));
        if inside(self, &self.current) {
            let retry = key.to_string();
            if !self.save_before(cx, move |w, window, cx| w.remove_folder(&retry, window, cx)) { return; }
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
        if let Err(e) = self.store.remove_folder(key) { self.show_toast_kind(ToastKind::Error, &crate::settings::diagnostic(&e.to_string()), cx); }
        cx.defer(crate::settings::top_switched);
        self.entries.set(self.store.list());
        self.trash = self.store.list_trash();
        self.sort();
        cx.notify();
    }

    /// 重命名 a top-level folder: its folder on disk takes `name` (2.11). The open
    /// document is saved first; ids stay, so it stays open. That is one rename
    /// of one folder, made here: nothing inside is rewritten, and the library
    /// must not be reached at the old path meanwhile. Its rows take the name
    /// at once (paths inside are as they were); listing it again is left to
    /// the background. On failure the name stays as it was, a toast says why
    /// and the row is back in its field with what was typed.
    pub(crate) fn rename_folder_top(&mut self, key: &str, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (k, n) = (key.to_string(), name.to_string());
        if !self.save_before(cx, move |w, window, cx| w.rename_folder_top(&k, &n, window, cx)) { return; }
        self.flush_view_state();
        // Queued moves and renames name pages by id, but are written under the folder's path.
        self.settle_tree_writes(cx);
        for peer in Self::other_workspaces(cx) { peer.update(cx, |w, cx| w.settle_tree_writes(cx)); }
        match self.store.rename_folder_top(key, name) {
            Ok(_) => {
                self.top_renamed(key, cx);
                // Every window shows the top-level folder by its new name.
                let me = window.window_handle().window_id();
                for h in cx.windows().into_iter().filter(|w| w.window_id() != me).filter_map(|w| w.downcast::<Workspace>()) {
                    let _ = h.update(cx, |ws, _, cx| ws.top_renamed(key, cx));
                }
            }
            Err(e) => {
                let msg = match e {
                    RenameError::Taken(n) => crate::localized_format!("已经有一个叫「{}」的文件夹", "There is already a folder named “{}”", n),
                    RenameError::Empty => tr("文件夹名不能为空", "The folder name can't be empty").to_string(),
                    RenameError::Denied => tr("没有权限给这个文件夹改名", "You don't have permission to rename this folder").to_string(),
                    RenameError::Unavailable | RenameError::Fixed => tr("这个文件夹不能改名", "This folder can't be renamed").to_string(),
                    RenameError::Other(e) => crate::localized_format!("文件夹改名失败：{}", "Could not rename the folder: {}", crate::settings::diagnostic(&e)),
                };
                self.show_toast_kind(ToastKind::Error, &msg, cx);
                let row = crate::shared_store::top_row(key);
                if self.rename.is_none() && self.entries.iter().any(|e| e.id == row) {
                    self.start_rename(&row, window, cx);
                    if let Some(field) = self.rename.as_ref().map(|r| r.field.clone()) { field.update(cx, |f, cx| { f.set_text(name, cx); f.select_all_text(cx); }); }
                }
            }
        }
        cx.notify();
    }

    /// Top-level folder `key` has a new name on disk: its row says so now.
    fn top_renamed(&mut self, key: &str, cx: &mut Context<Self>) {
        let row = crate::shared_store::top_row(key);
        if let Some(name) = self.store.top_folders().into_iter().find(|r| r.0 == key).map(|r| r.1) && let Some(e) = self.entries.edit().iter_mut().find(|e| e.id == row) {
            e.meta.folder = Some(name.clone());
            e.title = name;
        }
        self.sort();
        if !self.store.file_task_busy() && !self.store.loading() { self.relist(vec![key.to_string()], cx); }
        cx.notify();
    }

    /// The "…" menu of a top-level folder's row (two or more top-level folders).
    pub(crate) fn top_row_menu(&self, key: &str, m: RowMenu, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let row = |id: &'static str, glyph: &'static str, label: &'static str| crate::design::MenuItem::new(id).icon(glyph).label(label);
        let pos = m.pos;
        let open = self.store.top_folders().into_iter().find(|r| r.0 == key);
        let (name, path, missing) = open.map(|r| (r.1, r.2, r.3.is_some())).unwrap_or_default();
        let (doc, folder, rename, k1, k2) = (m.id.clone(), m.id.clone(), m.id.clone(), key.to_string(), key.to_string());
        let mut menu = crate::design::Popover::new("top-row-menu").menu().min_w(px(200.))
            .on_mouse_down_out(cx.listener(|w, _, _, cx| { w.row_menu = None; cx.notify(); }));
        if missing {
            menu = menu.child(row("rr-retry", "folder", tr("重新查找", "Look again")).on_click(cx.listener(move |w, _, _, cx| {
                w.row_menu = None;
                w.store.retry_folder(&k1);
                w.entries.set(w.store.list());
                w.sort();
                cx.notify();
            })));
        } else {
            menu = menu
                .child(row("rr-new-doc", "doc", tr("新建文档", "New document")).on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.create_doc(false, Some(doc.clone()), window, cx); })))
                .child(row("rr-new-folder", "folder", tr("新建文件夹", "New folder")).on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.new_folder(Some(folder.clone()), window, cx); })))
                .when(self.store.can_rename_folder_top(key), |m| m.child(row("rr-rename", "square-pen", tr("重命名", "Rename")).on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.start_rename(&rename.clone(), window, cx); }))))
                .child(row("rr-reveal", "folder", show_in_files()).on_click(cx.listener(move |w, _, _, cx| { w.row_menu = None; cx.reveal_path(&path); cx.notify(); })));
        }
        menu = menu.child(row("rr-remove", "x", tr("从侧栏移除", "Remove from sidebar")).on_click(cx.listener(move |w, _, _, cx| {
            w.row_menu = None;
            w.confirm = Some(crate::app::Confirm::RemoveFolder(k2.clone(), name.clone()));
            cx.notify();
        })));
        vec![deferred(anchored().anchor(m.corner).position(pos).snap_to_window_with_margin(px(8.)).child(menu.fit(crate::design::room(pos, m.corner)))).with_priority(2).into_any_element()]
    }

    /// The sidebar with no top-level folder open: a way to open one (a new one is made in the open panel).
    pub(crate) fn empty_home(&self, cx: &mut Context<Self>) -> Div {
        let button = |id: &'static str, label: &'static str| crate::design::MenuItem::new(id).label(label);
        div().flex().flex_col().gap(px(2.)).px(px(4.)).pt(px(12.))
            .child(div().px(px(8.)).pb(px(6.)).text_size(px(crate::theme::ui_font())).line_height(px(crate::theme::ui_font() + 7.)).text_color(Theme::text_caption())
                .child(tr("还没有打开任何文件夹。笔记就是文件夹里的文件。", "No folder is open yet. Your notes are the files in a folder.")))
            .child(button("empty-open", tr("打开文件夹…", "Open folder…")).on_click(cx.listener(|w, _, window, cx| w.open_folder_dialog(window, cx))))
    }

    /// The main area with no top-level folder open (2.8), centred: what to do first.
    pub(crate) fn empty_main(&self, cx: &mut Context<Self>) -> Div {
        div().flex_1().flex().flex_col().items_center().justify_center().pb(px(64.))
            .child(img("illus/empty-list.svg").size(px(120.)))
            .child(div().mt(px(16.)).text_size(px(13.)).line_height(px(20.)).text_color(Theme::text_caption()).child(tr("还没有打开任何文件夹。笔记就是文件夹里的文件。", "No folder is open yet. Your notes are the files in a folder.")))
            .child(div().mt(px(20.)).flex().gap(px(8.))
                .child(crate::design::Button::primary("empty-main-open").label(tr("打开文件夹…", "Open folder…")).on_click(cx.listener(|w, _, window, cx| w.open_folder_dialog(window, cx)))))
    }

    /// What the top of the tree is called where nothing names a top-level folder: the
    /// one top-level folder's folder name, else the top-level folder in front's.
    pub(crate) fn home_name(&self) -> String {
        let tops = self.store.top_folders();
        let key = if tops.len() == 1 { Some(tops[0].0.clone()) } else { self.store.active_top() };
        key.and_then(|k| tops.into_iter().find(|r| r.0 == k)).map(|r| r.1).unwrap_or_else(|| tr("我的文档", "My documents").to_string())
    }

    /// What top-level folder `key` is called: its folder's name.
    pub(crate) fn folder_name(&self, key: &str) -> String {
        self.store.top_folders().into_iter().find(|r| r.0 == key).map(|r| r.1).unwrap_or_default()
    }

    /// The rows of 本机文件: single files opened where they are, last opened first.
    pub(crate) fn local_rows(&self) -> Vec<(&crate::store::Entry, usize)> {
        self.local_entries().into_iter().filter(|e| e.meta.folder.is_none()).map(|e| (e, 0)).collect()
    }
}

#[cfg(test)]
mod tests {
    /// What `Confirm::AddFolder` says, by its Chinese key.
    const TEXTS: [&str; 5] = ["添加「{}」？", "这个文件夹看起来是代码项目。", "这个文件夹里有 {} 多个文件。", "Quiet.md 只显示其中的 Markdown 文件和 Quiet.md 文档，不进入依赖、构建产物和 .gitignore 忽略的文件夹；添加后，文件夹里会多一个隐藏的 .quiet 文件夹。", "添加"];

    #[test]
    fn every_language_asks_before_adding_a_code_folder() {
        for l in crate::settings::Lang::ALL.into_iter().filter(|l| !matches!(l, crate::settings::Lang::Zh | crate::settings::Lang::En)) {
            for k in TEXTS { assert!(crate::l10n::get(l, k).is_some(), "{l:?}: {k}"); }
        }
    }
}
