//! Dropping onto the sidebar's 我的文档: files from the Finder are imported
//! into the folder under the pointer, or between the rows there (a document
//! row only opens a slot beside it, #418), and a 本机文件 row carried up into
//! the tree is imported there.
use fdl_core::vfs::PathExt as _;
use crate::app::Workspace;
use crate::settings::tr;
use crate::theme::{self, Theme};
use crate::ui::{icon, ui_text};
use gpui::*;
use std::path::PathBuf;

/// A 本机文件 row being carried: GPUI's drag payload, drawn as the row lifted.
#[derive(Clone)]
pub(crate) struct LocalRowDrag { pub id: String, pub title: String, pub folder: bool }

impl Render for LocalRowDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().h(px(crate::sidebar::ROW_H)).w(px(220.)).pl(px(6.)).pr(px(10.)).flex().items_center().rounded(px(crate::sidebar::ROW_R))
            .bg(theme::raised(Theme::bg_body())).shadow(theme::shadow_menu()).opacity(0.96)
            .child(div().size(px(20.)).flex_none().flex().items_center().justify_center().child(icon(if self.folder { "folder" } else { "doc-md" }, crate::sidebar::ROW_ICON, Theme::icon())))
            .child(div().pl(px(9.)).flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(px(13.)).text_color(Theme::text())
                .child(ui_text(self.title.clone())))
    }
}

/// Where a drop on the sidebar lands: in `parent` (None: the top of 我的文档),
/// before `before` when it opens a slot between rows (index, depth), or last
/// in a folder row it is over.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SidebarDropAt { pub parent: Option<String>, pub slot: Option<(usize, usize)>, pub before: Option<String> }

impl SidebarDropAt {
    /// The top of 我的文档 with no tree shown to open a slot in.
    pub(crate) fn top() -> Self { Self { parent: None, slot: None, before: None } }
}

fn importable(p: &std::path::Path) -> bool {
    p.fs_is_dir() || crate::local_files::is_markdown_path(p) || p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xml")) || crate::transfer::is_csv_path(p)
}

impl Workspace {
    /// Something dragged over the sidebar at `at`: where it would land
    /// (`Some(None)`: the top of 我的文档). A 本机文件 row only lands in the tree
    /// itself, not back in its own section.
    pub(crate) fn sidebar_drop_at(&mut self, at: Point<Pixels>, local: bool, cx: &mut Context<Self>) -> bool {
        let target = if self.view == crate::app::View::Settings || self.view == crate::app::View::Trash { None } else { self.sidebar_target(at, local) };
        if target != self.sidebar_drop {
            if let Some(f) = target.as_ref().filter(|t| t.slot.is_none()).and_then(|t| t.parent.as_ref()) { self.folder_drag.kick(f, 3.2); }
            self.sidebar_drop = target;
            cx.notify();
        }
        // Over the sidebar, the page's own drop veil steps back.
        if self.sidebar_drop.is_some() && let Some(d) = self.home.folders.file_drop.as_mut() { d.leave(); cx.notify(); }
        self.sidebar_drop.is_some()
    }

    /// Files from the Finder let go over the sidebar.
    pub(crate) fn sidebar_files_dropped(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let Some(target) = self.sidebar_drop.take() else { return };
        let (dirs, files): (Vec<_>, Vec<_>) = paths.into_iter().filter(|p| importable(p)).partition(|p| p.fs_is_dir());
        if dirs.is_empty() && files.is_empty() {
            self.show_toast_kind(crate::ui::ToastKind::Error, tr("这些文件不能导入：支持 Markdown、DocxXML 文件和文件夹", "These files can't be imported: Markdown, DocxXML files and folders are supported"), cx);
            cx.notify();
            return;
        }
        self.ask_import(crate::app::DropImport::Files(dirs.into_iter().chain(files).collect()), target, cx);
    }

    /// 是否导入: asked before anything from outside comes into the library.
    fn ask_import(&mut self, what: crate::app::DropImport, target: SidebarDropAt, cx: &mut Context<Self>) {
        self.row_menu = None;
        self.confirm = Some(crate::app::Confirm::Import { what, target });
        cx.notify();
    }

    /// The place a drop goes, for the dialog and the toast.
    pub(crate) fn drop_place(&self, target: &SidebarDropAt) -> String {
        target.parent.as_ref().and_then(|t| self.entries.iter().find(|e| &e.id == t)).map(|e| crate::settings::display_title(&e.title))
            .unwrap_or_else(|| self.home_name())
    }

    /// 导入 confirmed: bring it in where it was dropped.
    pub(crate) fn import_dropped(&mut self, what: crate::app::DropImport, target: SidebarDropAt, window: &mut Window, cx: &mut Context<Self>) {
        match what {
            crate::app::DropImport::Files(paths) => self.import_dropped_files(paths, target, window, cx),
            crate::app::DropImport::Local(id) => self.import_dropped_local(&id, target, window, cx),
        }
    }

    fn import_dropped_files(&mut self, paths: Vec<PathBuf>, target: SidebarDropAt, window: &mut Window, cx: &mut Context<Self>) {
        let (dirs, files): (Vec<_>, Vec<_>) = paths.into_iter().partition(|p| p.fs_is_dir());
        let known: std::collections::HashSet<String> = self.entries.iter().map(|e| e.id.clone()).collect();
        let parent = target.parent.clone();
        self.landed_in(parent.as_deref());
        if !files.is_empty() { self.import_paths(&files, parent.clone(), false, window, cx); }
        // Into the slot it showed: what came in at this level goes before the row below it.
        let new: Vec<String> = self.entries.iter().filter(|e| !known.contains(&e.id) && e.meta.parent == parent).map(|e| e.id.clone()).collect();
        self.place_dropped(&new, &target);
        // Folders come in off the window's thread; each takes its slot once it is in.
        for dir in &dirs {
            let target = target.clone();
            self.import_folder_then(dir, parent.clone(), false, Some(Box::new(move |w, id| w.place_dropped(&[id.to_string()], &target))), window, cx);
        }
        cx.notify();
    }

    /// A 本机文件 row let go over the tree: a Markdown copy lands there; the
    /// file itself stays where it is and in 本机文件.
    pub(crate) fn local_row_dropped(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(target) = self.sidebar_drop.take() else { return };
        self.ask_import(crate::app::DropImport::Local(id.to_string()), target, cx);
    }

    fn import_dropped_local(&mut self, id: &str, target: SidebarDropAt, window: &mut Window, cx: &mut Context<Self>) {
        // A folder under 本机文件: copied in like any folder.
        if self.entries.iter().any(|e| e.id == id && e.meta.folder.is_some()) {
            let Some(dir) = self.store.local_path(id) else { return };
            self.landed_in(target.parent.as_deref());
            let parent = target.parent.clone();
            self.import_folder_then(&dir, parent, false, Some(Box::new(move |w, id| w.place_dropped(&[id.to_string()], &target))), window, cx);
            return;
        }
        let Some(doc) = self.doc_of(id, cx) else { return };
        let front = self.store.front_matter_of(id);
        match self.store.create_markdown_in(&doc, &front, target.parent.as_deref()) {
            Ok(new) => {
                if let Some(src) = self.store.local_path(id) { self.keep_times(&new, &src); }
                self.place_dropped(&[new], &target);
                self.landed_in(target.parent.as_deref());
                let place = self.drop_place(&target);
                self.show_toast_kind(crate::ui::ToastKind::Info, &crate::localized_format!("已导入到「{}」，原文件未改动", "Imported into “{}”; the original file is unchanged", place), cx);
            }
            Err(e) => self.show_toast_kind(crate::ui::ToastKind::Error, &crate::localized_format!("导入失败：{}", "Could not import: {}", crate::settings::diagnostic(&e.to_string())), cx),
        }
        cx.notify();
    }

    /// Put what a drop brought in into the slot it showed (a folder row: as imported).
    fn place_dropped(&mut self, ids: &[String], target: &SidebarDropAt) {
        let journal = self.store.track_file_changes();
        if target.slot.is_some() {
            for id in ids { let _ = self.store.move_in_tree(id, target.parent.as_deref(), target.before.as_deref()); }
        }
        self.entries.set(self.store.list());
        self.sort();
        // Part of the import: undo checks every place it recorded.
        self.amend_file_record(ids, journal.take());
    }

    /// The folder that took a drop opens and bounces.
    fn landed_in(&mut self, folder: Option<&str>) {
        if let Some(f) = folder { self.expanded.insert(f.to_string()); self.folder_drag.kick(f, 4.5); }
    }

    /// Nothing is being dragged any more: forget the drop target.
    pub(crate) fn settle_sidebar_drop(&mut self, cx: &App) {
        if self.sidebar_drop.is_some() && !cx.has_active_drag() && !crate::devtools::file_drag() { self.sidebar_drop = None; }
    }

    /// The 我的文档 heading while a drop would land at the top.
    pub(crate) fn sidebar_drop_top(&self) -> bool { self.sidebar_drop.as_ref().is_some_and(|t| t.parent.is_none() && t.slot.is_none()) }

    /// The sidebar list's handlers for both kinds of drag.
    pub(crate) fn sidebar_drop_zone(&self, list: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        list.on_drag_move(cx.listener(|w, ev: &DragMoveEvent<ExternalPaths>, _, cx| {
                let at = ev.event.position;
                if ev.bounds.contains(&at) { w.sidebar_drop_at(at, false, cx); } else if w.sidebar_drop.take().is_some() { cx.notify(); }
            }))
            .on_drag_move(cx.listener(|w, ev: &DragMoveEvent<LocalRowDrag>, _, cx| {
                let at = ev.event.position;
                if ev.bounds.contains(&at) { w.sidebar_drop_at(at, true, cx); } else if w.sidebar_drop.take().is_some() { cx.notify(); }
            }))
            // Where it is let go decides, not the last move heard.
            .on_drop(cx.listener(|w, paths: &ExternalPaths, window, cx| { w.sidebar_drop_at(window.mouse_position(), false, cx); w.sidebar_files_dropped(paths.paths().to_vec(), cx) }))
            .on_drop(cx.listener(|w, d: &LocalRowDrag, window, cx| { w.sidebar_drop_at(window.mouse_position(), true, cx); w.local_row_dropped(&d.id, cx) }))
    }
}
