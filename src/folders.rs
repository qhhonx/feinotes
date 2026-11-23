use crate::app::{View, Workspace};
use crate::settings::{display_title, tr};
use crate::theme::Theme;
use crate::ui::{TipExt, TitleBarExt, icon, popover, ui_text};
use gpui::{prelude::FluentBuilder, *};

#[derive(Clone, PartialEq)]
pub(crate) enum CreateTarget {
    Library(Option<String>),
    // Sidebar actions target the named section independently of the active page.
    PersonalSidebar(Option<String>),
    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    SharedFolder { room: crate::cloud_sync::shared::Room, parent: String, section: crate::sidebar::SidebarSection },
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    SharedSidebar,
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
    ShareNew,
}

#[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
#[derive(Clone)]
pub(crate) enum SharedFolderCreation {
    Document,
    Folder(Option<String>),
}

impl Workspace {
    pub(crate) fn can_create_in_shared_folder(&self, parent: Option<&str>, cx: &gpui::App) -> bool {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        { crate::cloud_sync::shared::scope(&self.store, cx).is_some_and(|room| room.folder)
            && self.sharing_access(cx).is_some_and(|access| access.can_write())
            && !crate::cloud_sync::shared::editing_paused(&self.store, cx)
            && self.entries.iter().any(|e| Some(e.id.as_str()) == parent && e.meta.folder.is_some()) }
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
        { let _ = (parent, cx); false }
    }

    /// 新建文档/Markdown under `parent` (None = top level), opened and focused.
    /// `markdown` picks the on-disk format; the choice is remembered for ⌘N.
    pub(crate) fn create_doc(&mut self, markdown: bool, parent: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if !self.is_shared() && let Some(id) = parent.clone()
            && let Some(room) = crate::cloud_sync::shared::original::owner_room(&self.entries, &id) {
            #[cfg(target_os = "macos")]
            self.create_in_shared_sidebar(room, id, SharedFolderCreation::Document, crate::sidebar::SidebarSection::Personal, window, cx);
            #[cfg(target_os = "ios")]
            self.with_shared_document(room, window, cx, move |w, _, window, cx| w.create_doc(markdown, Some(id.clone()), window, cx));
            return;
        }
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        if let (Some(room), Some(id)) = (self.current_room(cx), parent.as_ref())
            && self.can_create_in_shared_folder(Some(id), cx) {
            self.create_in_shared_sidebar(room, id.clone(), SharedFolderCreation::Document, self.shared_sidebar_section(), window, cx);
            return;
        }
        if self.is_shared() && !self.can_create_in_shared_folder(parent.as_deref(), cx) { return; }
        let markdown = markdown && !crate::FEI_NOTES;
        self.create_menu = None;
        let (m, p) = (markdown, parent.clone());
        if !self.save_before(cx, move |w, window, cx| w.create_doc(m, p.clone(), window, cx)) { return; }
        if crate::FEI_NOTES && !markdown && !self.is_shared() {
            self.start_draft(parent, window, cx);
            return;
        }
        let file_before = self.store.track_file_changes();
        let made = if parent.is_some() {
            // In a folder: written into that folder's root at once.
            let doc = crate::model::Document::default();
            if markdown { self.store.create_markdown_in(&doc, "", parent.as_deref()) } else { self.store.create_from_in(&doc, parent.as_deref()) }
        } else if markdown {
            self.store.create_markdown(&crate::model::Document::default(), "").and_then(|id| {
                if let Some(p) = &parent { self.store.set_parent(&id, Some(p))?; }
                Ok(id)
            })
        } else {
            self.store.create_from_in(&crate::model::Document::default(), parent.as_deref())
        };
        match made {
            Ok(id) => {
                self.search_open = false;
                self.doc_info = None;
                self.home.sort_menu = false;
                self.entries = self.store.list();
                self.sort();
                match &parent { Some(p) => { self.expanded.insert(p.clone()); } None => self.folded[1] = false }
                #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                if self.is_shared() { self.sharing.expanded.extend(parent.iter().cloned()); }
                self.search.update(cx, |s, cx| s.set_text("", cx));
                self.open(&id, window, cx);
                if let Some(ed) = &self.editor { crate::gpui_compat::focus(window,&ed.read(cx).focus_handle(cx),cx); }
            }
            Err(e) => self.status = crate::app::SaveStatus::Failed(crate::localized_format!("新建失败：{}", "Could not create document: {}", crate::settings::diagnostic(&e.to_string()))),
        }
        self.set_pref(|p| p.last_new = if markdown { "markdown" } else { "doc" }.into(), cx);
        self.remember_files(file_before, crate::settings::tr(if markdown { "新建 Markdown" } else { "新建文档" }, if markdown { "New Markdown" } else { "New document" }));
        cx.notify();
    }

    /// 新建 › 画板: a standalone `.excalidraw` file in `parent`, opened at once.
    pub(crate) fn create_board(&mut self, parent: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_shared() { return; }
        self.create_menu = None;
        let p = parent.clone();
        if !self.save_before(cx, move |w, window, cx| w.create_board(p.clone(), window, cx)) { return; }
        let file_before = self.store.track_file_changes();
        match self.store.create_board_in(crate::settings::text("未命名画板"), &crate::whiteboard::Board::default(), parent.as_deref()) {
            Ok(id) => {
                self.search_open = false;
                self.doc_info = None;
                self.home.sort_menu = false;
                self.entries = self.store.list();
                self.sort();
                match &parent { Some(p) => { self.expanded.insert(p.clone()); } None => self.folded[1] = false }
                self.search.update(cx, |s, cx| s.set_text("", cx));
                self.open(&id, window, cx);
            }
            Err(e) => self.status = crate::app::SaveStatus::Failed(crate::localized_format!("新建失败：{}", "Could not create document: {}", crate::settings::diagnostic(&e.to_string()))),
        }
        self.remember_files(file_before, tr("新建画板", "New board"));
        cx.notify();
    }

    /// 新建 › 数据表: a standalone `.sqlite` file in `parent`, opened at once.
    pub(crate) fn create_table(&mut self, parent: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if !crate::DATATABLE || self.is_shared() { return; }
        self.create_menu = None;
        let p = parent.clone();
        if !self.save_before(cx, move |w, window, cx| w.create_table(p.clone(), window, cx)) { return; }
        let file_before = self.store.track_file_changes();
        match self.store.create_table_in(crate::settings::text("未命名数据表"), &crate::table::blank_base(), parent.as_deref()) {
            Ok(id) => {
                self.search_open = false;
                self.doc_info = None;
                self.home.sort_menu = false;
                self.entries = self.store.list();
                self.sort();
                match &parent { Some(p) => { self.expanded.insert(p.clone()); } None => self.folded[1] = false }
                self.search.update(cx, |s, cx| s.set_text("", cx));
                self.open(&id, window, cx);
            }
            Err(e) => self.status = crate::app::SaveStatus::Failed(crate::localized_format!("新建失败：{}", "Could not create document: {}", crate::settings::diagnostic(&e.to_string()))),
        }
        self.remember_files(file_before, tr("新建数据表", "New data table"));
        cx.notify();
    }

    pub(crate) fn copy_folder(&mut self, id: &str, cx: &mut Context<Self>) -> Option<String> {
        let ids = crate::tree::Tree::new(&self.entries).subtree(id);
        let mut copied = std::collections::HashMap::<String, String>::new();
        for key in ids {
            let Some(entry) = self.entries.iter().find(|e| e.id == key).cloned() else { continue };
            let parent = entry.meta.parent.as_ref().map(|p| copied.get(p).cloned().unwrap_or_else(|| p.clone()));
            let result = if entry.meta.folder.is_some() {
                let title = if key == id { crate::localized_format!("{} 副本", "{} copy", entry.title) } else { entry.title };
                self.store.create_folder(&title, parent.as_deref())
            } else {
                self.store.load(&key).and_then(|doc| self.store.create_from(&doc)).and_then(|new| {
                    self.store.set_parent(&new, parent.as_deref())?;
                    self.store.set_width(&new, entry.meta.width)?;
                    Ok(new)
                })
            };
            match result {
                Ok(new) => { copied.insert(key, new); }
                Err(e) => { self.show_toast_kind(crate::ui::ToastKind::Error, &e.to_string(), cx); return None; }
            }
        }
        copied.remove(id)
    }

    pub(crate) fn creation_parent(&self) -> Option<String> {
        if let Some(draft) = &self.draft { draft.parent.clone() }
        else if self.view == View::Folder { self.folder_current.clone() }
        else if self.view == View::Doc { self.current.as_ref().and_then(|id| self.entries.iter().find(|e| &e.id == id)).and_then(|e| e.meta.parent.clone()) }
        else { None }
    }

    pub(crate) fn open_folder(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        { self.sharing.account_open = None; }
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        self.cancel_sidebar_open();
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if !self.is_shared() && crate::cloud_sync::shared::original::owner_room(&self.entries, id).is_some() {
            return self.try_open(id, window, cx);
        }
        // A folder with a 正文 has no page of its own: it is that document (#357).
        if let Some(note) = self.entries.iter().find(|e| e.id == id).and_then(|e| e.note.clone()) { return self.try_open(&note, window, cx); }
        let target = id.to_string();
        if !self.save_before(cx, move |w, window, cx| { w.open_folder(&target, window, cx); }) { return false; }
        self.remember_scroll(cx);
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        self.close_sharing_for_navigation();
        self.draft = None;
        self.editor_sub = None;
        self.editor = None;
        self.current = None;
        if self.folder_current.as_deref() != Some(id) { self.reset_folder_scroll(); }
        if let Some(meta) = self.store.touch(id) && let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) { entry.meta = meta; }
        self.schedule_view_state(cx);
        if self.manual_home_sort() { self.prepare_manual_order(cx); }
        self.folder_current = Some(id.to_string());
        self.sidebar_entry = None;
        self.view = View::Folder;
        self.row_menu = None;
        self.create_menu = None;
        self.doc_menu = false;
        if !cfg!(target_os = "macos") || !self.is_shared() {
            self.expanded.insert(id.to_string());
            self.reveal_in_tree(id);
        }
        crate::gpui_compat::focus(window, &self.focus, cx);
        cx.notify();
        true
    }

    pub(crate) fn can_create_folder(&self, target: &CreateTarget, cx: &App) -> bool {
        match target {
            CreateTarget::Library(parent) if self.is_shared() => self.can_create_in_shared_folder(parent.as_deref(), cx),
            CreateTarget::Library(parent) if self.view != View::Home || !self.home.shared() =>
                parent.as_ref().is_none_or(|id| self.entries.iter().any(|e| &e.id == id && e.meta.folder.is_some())),
            CreateTarget::PersonalSidebar(parent) => parent.as_ref().is_none_or(|id|
                self.folder_drag.personal_rows.as_deref().unwrap_or(&self.entries).iter().any(|e| &e.id == id && e.meta.folder.is_some())),
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            CreateTarget::SharedFolder { room, .. } => self.shared_sidebar_folder_writable(room, cx),
            _ => false,
        }
    }

    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    pub(crate) fn sidebar_create_target(&self, entry: &crate::store::Entry) -> CreateTarget {
        let personal = self.folder_drag.personal_rows.as_deref().unwrap_or(&self.entries);
        if entry.meta.share_root.is_some()
            && let Some(room) = crate::cloud_sync::shared::original::owner_room(personal, &entry.id) {
            return CreateTarget::SharedFolder { room, parent: entry.id.clone(), section: crate::sidebar::SidebarSection::Personal };
        }
        CreateTarget::PersonalSidebar(Some(entry.id.clone()))
    }

    pub(crate) fn new_folder(&mut self, target: CreateTarget, name: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_create_folder(&target, cx) { return; }
        let retry_target = target.clone();
        let parent = match target {
            CreateTarget::PersonalSidebar(_) if self.is_shared() => {
                self.create_menu = None;
                self.personal_action(window, cx, move |w, window, cx| w.new_folder(target, name, window, cx));
                return;
            }
            CreateTarget::Library(parent) | CreateTarget::PersonalSidebar(parent) => parent,
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            CreateTarget::SharedFolder { room, parent, section } => {
                self.create_in_shared_sidebar(room, parent, SharedFolderCreation::Folder(name), section, window, cx);
                return;
            }
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            CreateTarget::SharedSidebar | CreateTarget::ShareNew => return,
        };
        #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
        if let (Some(room), Some(id)) = (self.current_room(cx), parent.as_ref()) {
            self.create_in_shared_sidebar(room, id.clone(), SharedFolderCreation::Folder(name), self.shared_sidebar_section(), window, cx);
            return;
        }
        self.create_menu = None;
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if !self.is_shared() && let Some(id) = parent.clone()
            && let Some(room) = crate::cloud_sync::shared::original::owner_room(&self.entries, &id) {
            #[cfg(target_os = "macos")]
            self.create_in_shared_sidebar(room, id, SharedFolderCreation::Folder(name), crate::sidebar::SidebarSection::Personal, window, cx);
            #[cfg(target_os = "ios")]
            self.with_shared_document(room, window, cx, move |w, _, window, cx| w.new_folder(CreateTarget::Library(Some(id)), name, window, cx));
            return;
        }
        let retry_name = name.clone();
        if !self.save_before(cx, move |w, window, cx| w.new_folder(retry_target, retry_name, window, cx)) { return; }
        let file_before = self.store.track_file_changes();
        match self.store.create_folder(name.as_deref().unwrap_or_else(|| tr("未命名文件夹", "Untitled folder")), parent.as_deref()) {
            Ok(id) => {
                self.entries = self.store.list(); self.sort();
                if let Some(p) = parent { self.expanded.insert(p); }
                self.folded[1] = false;
                // Select and open the new folder (like 新建文档), then focus its name
                // field. open_folder focuses the pane, so start_rename must follow it.
                if self.open_folder(&id, window, cx) && name.is_none() {
                    self.start_rename(&id, window, cx);
                }
            }
            Err(e) => self.show_toast_kind(crate::ui::ToastKind::Error, &e.to_string(), cx),
        }
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        self.publish_shared_folder_change(cx);
        self.remember_files(file_before, crate::settings::tr("新建文件夹", "New folder"));
        cx.notify();
    }

    pub(crate) fn render_create_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (target, pos) = self.create_menu.clone()?;
        let can_create_folder = self.can_create_folder(&target, cx);
        let folder_target = target.clone();
        let personal = matches!(target, CreateTarget::PersonalSidebar(_));
        let row = |id: &'static str, glyph: &'static str, label: &'static str| crate::ui::menu_row(id, SharedString::from(id)).child(icon(glyph, 16., Theme::icon())).child(label);
        let menu = popover().id("create-items").occlude().min_w(px(180.)).py(px(crate::ui::MENU_PAD_Y))
            .on_mouse_down_out(cx.listener(move |w, event: &MouseDownEvent, _, cx| {
                let row_slot = match &w.create_menu {
                    Some((CreateTarget::PersonalSidebar(Some(id)), _)) => Some(format!("add-doc-false-{id}")),
                    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
                    Some((CreateTarget::SharedFolder { parent, section, .. }, _)) => {
                        let key = if *section == crate::sidebar::SidebarSection::Shared { "shared-folder" } else { "doc" };
                        Some(format!("add-{key}-false-{parent}"))
                    }
                    _ => None,
                };
                if row_slot.is_some_and(|slot| w.more_bounds.borrow().get(&slot).is_some_and(|bounds|
                    point(bounds.left(), bounds.bottom() + px(4.)) == pos && bounds.contains(&event.position))) { return; }
                let key = match &w.create_menu {
                    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                    Some((CreateTarget::SharedSidebar, _)) => "create-shared-sidebar",
                    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                    Some((CreateTarget::ShareNew, _)) => "create-home",
                    Some((CreateTarget::Library(None), _)) => "create-home",
                    Some((CreateTarget::Library(Some(_)), _)) => "create-folder",
                    _ => "create-sidebar",
                };
                if !w.home.anchor(key).is_some_and(|bounds| bounds.contains(&event.position)) { w.create_menu = None; cx.notify(); }
            }));
        let menu = match target {
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
            CreateTarget::SharedSidebar | CreateTarget::ShareNew => menu.children([false, true].into_iter().map(|folder| {
                let sidebar = target == CreateTarget::SharedSidebar;
                row(if folder { "new-shared-folder" } else { "new-shared-document" }, if folder { "folder" } else { "doc" },
                    crate::sharing::new_shared_title(folder))
                    .on_click(cx.listener(move |w, _, window, cx| {
                        if sidebar {
                            #[cfg(target_os = "macos")]
                            w.open_new_shared_item(folder, window, cx);
                            #[cfg(target_os = "ios")]
                            w.personal_action(window, cx, move |w, window, cx| w.open_new_shared_item(folder, window, cx));
                        } else { w.new_and_share(folder, window, cx); }
                    }))
            })),
            #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
            CreateTarget::SharedFolder { room, parent, section } => {
                if !can_create_folder { return None; }
                menu.child(row("create-document", "doc", tr("文档", "Document")).on_click(cx.listener(move |w, _, window, cx|
                    w.create_in_shared_sidebar(room.clone(), parent.clone(), SharedFolderCreation::Document, section, window, cx))))
                    .child(row("create-folder", "folder", tr("文件夹", "Folder")).on_click(cx.listener(move |w, _, window, cx|
                        w.new_folder(folder_target.clone(), None, window, cx))))
            }
            CreateTarget::Library(parent) | CreateTarget::PersonalSidebar(parent) => {
                let table_parent = parent.clone();
                let (doc_parent, md_parent, board_parent) = (parent.clone(), parent.clone(), parent);
                let document = move |w: &mut Workspace, markdown, parent, window: &mut Window, cx: &mut Context<Self>| {
                    w.create_menu = None;
                    if personal { w.personal_action(window, cx, move |w, window, cx| w.create_doc(markdown, parent, window, cx)); }
                    else { w.create_doc(markdown, parent, window, cx); }
                };
                menu
                .child(row("create-document", "doc", tr("文档", "Document")).on_click(cx.listener(move |w, _, window, cx| document(w, false, doc_parent.clone(), window, cx))))
                .when(!crate::FEI_NOTES, |d| d.child(row("create-markdown", "doc-md", "Markdown").on_click(cx.listener(move |w, _, window, cx| document(w, true, md_parent.clone(), window, cx)))))
                .when(personal || !self.is_shared(), |d| d.child(row("create-board", "whiteboard", crate::settings::text("画板")).on_click(cx.listener(move |w, _, window, cx| {
                    w.create_menu = None;
                    let parent = board_parent.clone();
                    if personal { w.personal_action(window, cx, move |w, window, cx| w.create_board(parent, window, cx)); }
                    else { w.create_board(parent, window, cx); }
                }))))
                .when(crate::DATATABLE && !self.is_shared(), |d| {
                    let table_parent = table_parent.clone();
                    d.child(row("create-table", "table", crate::settings::text("数据表")).on_click(cx.listener(move |w, _, window, cx| {
                        w.create_menu = None;
                        w.create_table(table_parent.clone(), window, cx);
                    })))
                })
                .when(can_create_folder, |d| d.child(row("create-folder", "folder", tr("文件夹", "Folder")).on_click(cx.listener(move |w, _, window, cx| w.new_folder(folder_target.clone(), None, window, cx)))))
            }
        };
        Some(deferred(anchored().anchor(Anchor::TopLeft).position(pos).snap_to_window_with_margin(px(8.)).child(menu)).with_priority(3).into_any_element())
    }

    /// A folder's page: one header row (the path on the left, the actions on
    /// the right), then its contents in the view 主页 shows.
    pub(crate) fn render_folder(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let id = self.folder_current.clone().unwrap_or_default();
        let title = self.entries.iter().find(|e| e.id == id).map(|e| display_title(&e.title)).unwrap_or_else(|| tr("文件夹", "Folder").to_string());
        // 我的文档 › … › parent › this folder; a long path folds its middle into "…".
        let sep = || div().mx(px(4.)).flex_none().child(icon("chevron-right", 12., Theme::icon()));
        let link = |key: String, label: String, target: Option<String>| {
            div().id(SharedString::from(format!("folder-crumb-{key}"))).no_drag().flex_none().max_w(px(220.)).px(px(4.)).rounded(px(6.))
                .overflow_hidden().whitespace_nowrap().text_ellipsis().cursor_pointer()
                .text_color(Theme::text_caption()).hover(|s| s.text_color(Theme::text()))
                .when(key == "home" && cfg!(all(target_os = "macos", feature = "devtools")), |d| {
                    d.relative().child(self.home.track("folder-breadcrumb-root"))
                })
                .child(ui_text(label))
                .on_click(cx.listener(move |w, _, window, cx| match &target {
                    Some(t) => w.open(t, window, cx),
                    None => w.navigate_breadcrumb_library(window, cx),
                }))
        };
        // A root's own row (two or more roots) is the first crumb, by name.
        let path: Vec<_> = self.ancestors(&id).into_iter().filter(|e| crate::shared_store::root_of_row(&e.id).is_none()).collect();
        // The path starts at its root folder, by that folder's name (W5).
        let root = self.entries.iter().find(|e| e.id == id).map(|e| e.root.clone()).unwrap_or_default();
        let root_title = if crate::FEI_NOTES { self.breadcrumb_library_title().to_string() } else { self.root_name(&root) };
        let first = link("home".into(), root_title, None).into_any_element();
        // A root's own page is the root: its name is the page's title, not a crumb before it.
        let own = crate::shared_store::root_of_row(&id).is_some();
        let mut crumbs = div().min_w_0().flex().items_center().text_size(px(17.)).line_height(px(24.)).when(!own, |d| d.child(first));
        for (i, e) in path.iter().enumerate() {
            if path.len() > 2 && i > 0 && i < path.len() - 1 {
                if i == 1 {
                    let hidden: Vec<String> = path[1..path.len() - 1].iter().map(|e| display_title(&e.title)).collect();
                    crumbs = crumbs.child(sep()).child(div().id("folder-crumb-more").flex_none().px(px(4.)).text_color(Theme::text_caption()).tip(&hidden.join(" / "), None).child("…"));
                }
                continue;
            }
            crumbs = crumbs.child(sep()).child(link(e.id.clone(), display_title(&e.title), Some(e.id.clone())));
        }
        crumbs = crumbs.when(!own, |d| d.child(sep())).child(div().min_w_0().px(px(4.)).overflow_hidden().whitespace_nowrap().text_ellipsis()
            .font_weight(FontWeight::SEMIBOLD).text_color(Theme::text()).child(ui_text(title)));
        let doc = id.clone();
        let header = crate::trash::page_header_with(self, crumbs, 26., cx);
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let header = if self.is_shared() {
            header.when_some(self.shared_access_button(window, cx), |d, share| {
                d.child(crate::ui::toolbar_group(div().flex_none().child(share)))
            })
        } else {
            let target = id.clone();
            header.child(crate::ui::header_icon("folder-create-share", "share").tip(tr("共享", "Share"), None)
                .on_click(cx.listener(move |w, _, window, cx| w.create_shared_note(&target,window,cx))))
        };
        let writable = !self.is_shared() || self.can_create_in_shared_folder(Some(&id), cx);
        let header = header.when(writable, |header| header
            .when(!self.is_shared(), |d| d.child(self.place_button("folder-more", Some(id.clone()), cx)))
            .child(self.import_button("folder-import", Some(id.clone()), cx))
            .child(crate::ui::header_icon("folder-new", "plus")
                .child(self.home.track("create-folder"))
                .when(self.create_menu.as_ref().is_some_and(|(_, pos)| self.home.anchor("create-folder").is_some_and(|b| point(b.left(), b.bottom() + px(4.)) == *pos)), |d| d.bg(Theme::fill_hover()))
                .tip(tr("新建", "New"), Some(crate::shortcuts::hint("workspace::NewDoc", "⌘ + N")))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |w, ev: &ClickEvent, _, cx| { let pos = w.home.anchor("create-folder").map_or(ev.position(), |b| point(b.left(), b.bottom() + px(4.))); let p = Some(doc.clone()); w.create_menu = if w.create_menu.is_some() { None } else { Some((CreateTarget::Library(p), pos)) }; cx.notify(); }))))
            .child(self.view_switch(cx).no_drag().ml(px(12.)));
        let children = self.level_entries(Some(&id));
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        let children = children.into_iter().map(|mut entry| {
            if let Some((pinned, starred)) = self.shared_child_marks(&entry.id, cx) {
                entry.meta.pinned = pinned;
                entry.meta.starred = starred;
            }
            entry
        }).collect::<Vec<_>>();
        let body = if children.is_empty() {
            crate::trash::empty_state("illus/empty-list.svg", tr("这个文件夹还是空的", "This folder is empty")).into_any_element()
        } else if self.home.grid {
            self.render_home_grid(&children, true, window, cx)
        } else {
            self.render_home_list(&children, true, cx)
        };
        div().size_full().flex().flex_col().child(header).child(div().h(px(8.)).flex_none()).child(body).into_any_element()
    }

    pub(crate) fn folder_row_menu(&self, m: crate::app::RowMenu, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (doc, folder, rename, mv, del) = (m.id.clone(), m.id.clone(), m.id.clone(), m.id.clone(), m.id.clone());
        let personal = !self.is_shared() || !self.entries.iter().any(|e| e.id == m.id && e.meta.folder.is_some());
        let folder = if personal { CreateTarget::PersonalSidebar(Some(folder)) } else { CreateTarget::Library(Some(folder)) };
        let row = |id: &'static str, glyph: &'static str, label: &'static str| crate::ui::menu_row(id, SharedString::from(id)).child(icon(glyph, 16., Theme::icon())).child(label);
        let pos = if m.pos.x < px(0.) { point(-m.pos.x, m.pos.y + px(22.)) } else { m.pos };
        vec![deferred(anchored().position(pos).snap_to_window_with_margin(px(8.)).child(popover().id("folder-row-menu").occlude().min_w(px(190.)).py(px(crate::ui::MENU_PAD_Y))
            .on_mouse_down_out(cx.listener(|w, _, _, cx| { w.row_menu = None; cx.notify(); }))
            .child(row("fr-new-doc", "doc", tr("新建文档", "New document")).on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None;
                let doc = doc.clone();
                if personal { w.personal_action(window, cx, move |w, window, cx| w.new_child(&doc, window, cx)); }
                else { w.new_child(&doc, window, cx); } })))
            .when(self.can_create_folder(&folder, cx), |d| d.child(row("fr-new-folder", "folder", tr("新建文件夹", "New folder")).on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.new_folder(folder.clone(), None, window, cx); }))))
            .child(row("fr-rename", "square-pen", tr("重命名", "Rename")).on_click(cx.listener(move |w, _, window, cx| {
                let rename = rename.clone();
                if personal { w.personal_action(window, cx, move |w, window, cx| w.start_rename(&rename, window, cx)); }
                else { w.start_rename(&rename, window, cx); }
            })))
            .child(row("fr-move", "folder-input", tr("移动到", "Move to")).on_click(cx.listener(move |w, _, window, cx| {
                let ids = vec![mv.clone()];
                if personal { w.personal_action(window, cx, move |w, window, cx| w.open_move(ids, window, cx)); }
                else { w.open_move(ids, window, cx); }
            })))
            .children({
                #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
                { let target = m.id.clone(); personal.then(|| row("share-folder", "share", tr("共享", "Share"))
                    .on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None;
                        let target = target.clone();
                        w.personal_action(window, cx, move |w, window, cx| w.create_shared_note(&target, window, cx)); }))) }
                #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync")))]
                { None::<Stateful<Div>> }
            })
            .child(row("fr-delete", "bin", tr("删除", "Delete")).on_click(cx.listener(move |w, _, _, cx| w.ask_delete(&del, cx))))))
            .with_priority(2).into_any_element()]
    }
}
