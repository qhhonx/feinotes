use crate::app::{View, Workspace};
use crate::settings::{display_title, tr};
use crate::theme::Theme;
use crate::ui::{TipExt, TitleBarExt, icon, popover, ui_text};
use gpui::{prelude::FluentBuilder, *};

impl Workspace {
    /// 新建文档/Markdown under `parent` (None = top level), opened and focused.
    /// `markdown` picks the on-disk format; the choice is remembered for ⌘N.
    pub(crate) fn create_doc(&mut self, markdown: bool, parent: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if crate::in_shared_note() { return; }
        self.create_menu = None;
        let (m, p) = (markdown, parent.clone());
        if !self.save_before(cx, move |w, window, cx| w.create_doc(m, p.clone(), window, cx)) { return; }
        let file_before = self.store.track_file_changes();
        let made = if markdown {
            self.store.create_markdown(&crate::model::Document::default(), "").and_then(|id| {
                if let Some(p) = &parent { self.store.set_parent(&id, Some(p))?; }
                Ok(id)
            })
        } else {
            self.store.create().and_then(|(id, _)| {
                if let Some(p) = &parent { self.store.set_parent(&id, Some(p))?; }
                Ok(id)
            })
        };
        match made {
            Ok(id) => {
                self.search_open = false;
                self.doc_info = None;
                self.home.sort_menu = false;
                self.entries = self.store.list();
                self.sort();
                match &parent { Some(p) => { self.expanded.insert(p.clone()); } None => self.folded[1] = false }
                self.search.update(cx, |s, cx| s.set_text("", cx));
                self.open(&id, window, cx);
                if let Some(ed) = &self.editor { ed.read(cx).focus(window); }
            }
            Err(e) => self.status = crate::app::SaveStatus::Failed(crate::localized_format!("新建失败：{}", "Could not create document: {}", crate::settings::diagnostic(&e.to_string()))),
        }
        self.set_pref(|p| p.last_new = if markdown { "markdown" } else { "doc" }.into(), cx);
        self.remember_files(file_before, crate::settings::tr(if markdown { "新建 Markdown" } else { "新建文档" }, if markdown { "New Markdown" } else { "New document" }));
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
        if self.view == View::Folder { self.folder_current.clone() }
        else if self.view == View::Doc { self.current.as_ref().and_then(|id| self.entries.iter().find(|e| &e.id == id)).and_then(|e| e.meta.parent.clone()) }
        else { None }
    }

    pub(crate) fn open_folder(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let target = id.to_string();
        if !self.save_before(cx, move |w, window, cx| { w.open_folder(&target, window, cx); }) { return false; }
        self.editor = None;
        self.current = None;
        if self.folder_current.as_deref() != Some(id) { self.reset_folder_scroll(); }
        if let Some(meta) = self.store.touch(id) && let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) { entry.meta = meta; }
        if self.manual_home_sort() { self.prepare_manual_order(cx); }
        self.folder_current = Some(id.to_string());
        self.view = View::Folder;
        self.row_menu = None;
        self.create_menu = None;
        self.doc_menu = false;
        self.expanded.insert(id.to_string());
        self.reveal_in_tree(id);
        window.focus(&self.focus);
        cx.notify();
        true
    }

    pub(crate) fn new_folder(&mut self, parent: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let file_before = self.store.track_file_changes();
        self.create_menu = None;
        match self.store.create_folder(tr("未命名文件夹", "Untitled folder"), parent.as_deref()) {
            Ok(id) => {
                self.entries = self.store.list(); self.sort();
                if let Some(p) = parent { self.expanded.insert(p); }
                self.folded[1] = false;
                // Select and open the new folder (like 新建文档), then focus its name
                // field. open_folder focuses the pane, so start_rename must follow it.
                if self.open_folder(&id, window, cx) {
                    self.start_rename(&id, window, cx);
                }
            }
            Err(e) => self.show_toast_kind(crate::ui::ToastKind::Error, &e.to_string(), cx),
        }
        self.remember_files(file_before, crate::settings::tr("新建文件夹", "New folder"));
        cx.notify();
    }

    pub(crate) fn render_create_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (parent, pos) = self.create_menu.clone()?;
        let folder_parent = parent.clone();
        let row = |id: &'static str, glyph: &'static str, label: &'static str| crate::ui::menu_row(id, SharedString::from(id)).child(icon(glyph, 16., Theme::icon())).child(label);
        let (doc_parent, md_parent) = (parent.clone(), parent.clone());
        Some(deferred(anchored().anchor(Corner::TopLeft).position(pos).snap_to_window_with_margin(px(8.)).child(
            popover().id("create-items").occlude().w(px(180.)).py(px(crate::ui::MENU_PAD_Y))
                .on_mouse_down_out(cx.listener(|w, _, _, cx| { w.create_menu = None; cx.notify(); }))
                .child(row("create-document", "doc", tr("文档", "Document")).on_click(cx.listener(move |w, _, window, cx| w.create_doc(false, doc_parent.clone(), window, cx))))
                .child(row("create-markdown", "doc-md", "Markdown").on_click(cx.listener(move |w, _, window, cx| w.create_doc(true, md_parent.clone(), window, cx))))
                .child(row("create-folder", "folder", tr("文件夹", "Folder")).on_click(cx.listener(move |w, _, window, cx| w.new_folder(folder_parent.clone(), window, cx))))
        )).with_priority(3).into_any_element())
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
                .child(ui_text(label))
                .on_click(cx.listener(move |w, _, window, cx| match &target { Some(t) => w.open(t, window, cx), None => w.show_view(View::Home, cx) }))
        };
        let path = self.ancestors(&id);
        let mut crumbs = div().min_w_0().flex().items_center().text_size(px(17.)).line_height(px(24.))
            .child(link("home".into(), tr("我的文档", "My documents").into(), None));
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
        crumbs = crumbs.child(sep()).child(div().min_w_0().px(px(4.)).overflow_hidden().whitespace_nowrap().text_ellipsis()
            .font_weight(FontWeight::SEMIBOLD).text_color(Theme::text()).child(ui_text(title)));
        let doc = id.clone();
        let header = crate::trash::page_header_with(self, crumbs, 26., cx)
            .child(self.import_button("folder-import", Some(id.clone()), cx))
            .child(crate::ui::header_icon("folder-new", "plus")
                .child(self.home.track("create-folder"))
                .when(self.create_menu.is_some(), |d| d.bg(Theme::fill_hover()))
                .tip(tr("新建", "New"), Some(crate::shortcuts::hint("workspace::NewDoc", "⌘ + N")))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |w, ev: &ClickEvent, _, cx| { let pos = w.home.anchor("create-folder").map_or(ev.position(), |b| point(b.left(), b.bottom() + px(4.))); let p = Some(doc.clone()); w.create_menu = if w.create_menu.is_some() { None } else { Some((p, pos)) }; cx.notify(); })))
            .child(self.view_switch(cx).no_drag().ml(px(12.)));
        let children = self.level_entries(Some(&id));
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
        let row = |id: &'static str, glyph: &'static str, label: &'static str| crate::ui::menu_row(id, SharedString::from(id)).child(icon(glyph, 16., Theme::icon())).child(label);
        let pos = if m.pos.x < px(0.) { point(-m.pos.x, m.pos.y + px(22.)) } else { m.pos };
        vec![deferred(anchored().position(pos).snap_to_window_with_margin(px(8.)).child(popover().id("folder-row-menu").occlude().w(px(190.)).py(px(crate::ui::MENU_PAD_Y))
            .on_mouse_down_out(cx.listener(|w, _, _, cx| { w.row_menu = None; cx.notify(); }))
            .child(row("fr-new-doc", "doc", tr("新建文档", "New document")).on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.new_child(&doc, window, cx); })))
            .child(row("fr-new-folder", "folder", tr("新建文件夹", "New folder")).on_click(cx.listener(move |w, _, window, cx| { w.row_menu = None; w.new_folder(Some(folder.clone()), window, cx); })))
            .child(row("fr-rename", "square-pen", tr("重命名", "Rename")).on_click(cx.listener(move |w, _, window, cx| w.start_rename(&rename, window, cx))))
            .child(row("fr-move", "folder-input", tr("移动到", "Move to")).on_click(cx.listener(move |w, _, window, cx| w.open_move(vec![mv.clone()], window, cx))))
            .child(row("fr-delete", "bin", tr("删除", "Delete")).on_click(cx.listener(move |w, _, _, cx| w.ask_delete(&del, cx))))))
            .with_priority(2).into_any_element()]
    }
}
