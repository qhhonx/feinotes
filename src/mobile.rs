//! Touch metrics and compact navigation around the shared product editor.
#[cfg(target_os = "ios")]
use gpui::*;

#[cfg(target_os = "ios")]
// Both mobile idioms use UIKit chrome; a wide iPad must not reveal the desktop renderer.
pub fn native_shell(_window: &Window) -> bool { true }
pub fn control(desktop: f32) -> f32 { if cfg!(target_os = "ios") { desktop.max(44.) } else { desktop } }
pub fn label(desktop: f32) -> f32 { if cfg!(target_os = "ios") { desktop.max(16.) } else { desktop } }
pub fn detail(desktop: f32) -> f32 { if cfg!(target_os = "ios") { desktop.max(14.) } else { desktop } }

pub fn ui_font() -> f32 { crate::theme::ui_font() + if cfg!(target_os = "ios") { 3. } else { 0. } }
pub fn ui_font_small() -> f32 { crate::theme::ui_font_small() + if cfg!(target_os = "ios") { 2. } else { 0. } }

#[cfg(target_os="ios")]
pub(crate) fn list_summary(body:&str, query:&str) -> String {
    crate::search::snippet(body,query).map(|(text,_)|text).unwrap_or_else(||body.chars().take(512).collect()).split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(target_os="ios")]
#[derive(Clone)]
pub(crate) struct ReturnPoint {
    pub root: std::path::PathBuf, pub view: crate::app::View, pub current: Option<String>, pub folder: Option<String>,
    pub tab: crate::home::HomeTab, pub query: String, pub key: String,
    pub settings_page: crate::settings::Page, pub settings_root: bool, pub settings_parent: Option<crate::settings::Page>,
}
#[cfg(target_os="ios")]
#[derive(Default)]
pub(crate) struct Navigation { pub links: Vec<ReturnPoint>, pub trash: Option<ReturnPoint> }

#[cfg(target_os = "ios")]
mod shell {
    use super::*;
    use crate::app::{SaveStatus, View, Workspace};
    use crate::settings::{Page, display_title, tr};
    use crate::theme::{self, Theme};
    use crate::ui;
    use gpui::prelude::FluentBuilder;

    fn hide_keyboard() { gpui_mobile::ios::ffi::gpui_ios_hide_keyboard(gpui_mobile::ios::ffi::gpui_ios_get_window()); }

    impl Workspace {
        fn mobile_return_point(&self,cx:&App) -> super::ReturnPoint {
            super::ReturnPoint { root:self.store.root().to_path_buf(),view:self.view,current:self.current.clone(),folder:self.folder_current.clone(),tab:self.home.tab,query:self.search.read(cx).text.clone(),key:self.mobile_document_key(),settings_page:self.settings_page,settings_root:self.mobile_settings_root,settings_parent:self.mobile_settings_parent }
        }
        pub(crate) fn mobile_document_key(&self) -> String {
            let key=self.current.as_ref().map(|id|format!("note:{id}")).unwrap_or_else(||"note:draft".into());
            if self.mobile_navigation.links.iter().any(|point|point.key==key) { format!("{key}:depth:{}",self.mobile_navigation.links.len()) } else { key }
        }
        fn restore_mobile_context(&mut self,point:super::ReturnPoint,cx:&mut Context<Self>) {
            self.folder_current=point.folder; self.home.tab=point.tab;
            self.search.update(cx,|field,cx|field.set_text(&point.query,cx));
            self.settings_page=point.settings_page; self.mobile_settings_root=point.settings_root; self.mobile_settings_parent=point.settings_parent;
            self.show_view(point.view,cx); hide_keyboard();
        }
        pub(crate) fn mobile_open_trash(&mut self,cx:&mut Context<Self>) {
            if self.view==View::Trash { return; }
            if !self.save_before(cx,|w,_,cx|w.mobile_open_trash(cx)) { return; }
            self.mobile_navigation.trash=Some(self.mobile_return_point(cx)); self.show_view(View::Trash,cx);
        }
        pub(crate) fn mobile_follow_link(&mut self,id:&str,ordinal:Option<usize>,window:&mut Window,cx:&mut Context<Self>) {
            if self.entries.iter().any(|entry|entry.id==id && entry.meta.folder.is_some()) {
                self.mobile_navigation.links.clear(); self.open(id,window,cx); return;
            }
            if self.view!=View::Doc || self.is_shared() || self.current.as_deref()==Some(id) {
                if let Some(ordinal)=ordinal { self.open_at(id,ordinal,window,cx); } else { self.open(id,window,cx); }
                return;
            }
            let target=id.to_owned();
            if !self.save_before(cx,move|w,window,cx|w.mobile_follow_link(&target,ordinal,window,cx)) { return; }
            let point=self.mobile_return_point(cx);
            if self.try_open(id,window,cx) {
                if point.current.is_some() { self.mobile_navigation.links.push(point); }
                if let Some(ordinal)=ordinal { self.open_at(id,ordinal,window,cx); }
                cx.notify();
            }
        }
        fn mobile_home(&mut self, cx: &mut Context<Self>) {
            self.folder_current = None;
            self.show_view(View::Home, cx);
            hide_keyboard();
        }

        pub(crate) fn mobile_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            if self.view == View::Doc && self.preview.is_some() { self.exit_preview(cx); return; }
            if self.view == View::Doc && self.history.is_some() { self.close_history(cx); return; }
            if self.view == View::Settings {
                if self.mobile_settings_root { self.mobile_close_settings(cx); }
                else if let Some(parent) = self.mobile_settings_parent.take() { self.open_settings(parent,cx); }
                else { self.mobile_settings_root = true; cx.notify(); }
                return;
            }
            if let Some(editor) = &self.editor { editor.update(cx, |ed, cx| { ed.cancel_background_composition(cx); }); }
            if !self.save_before(cx, |w, window, cx| w.mobile_back(window,cx)) { return; }
            if self.view==View::Trash && let Some(point)=self.mobile_navigation.trash.take() {
                if point.root==self.store.root() { self.restore_mobile_context(point,cx); return; }
            }
            if self.view==View::Doc && let Some(point)=self.mobile_navigation.links.last().cloned() {
                if point.root!=self.store.root() { self.mobile_navigation.links.clear(); }
                else if let Some(id)=&point.current {
                    if self.try_open(id,window,cx) { self.mobile_navigation.links.pop(); self.restore_mobile_context(point,cx); }
                    return;
                }
            }
            if self.is_shared() {
                if crate::cloud_sync::shared::scope(&self.store,cx).is_some_and(|scope|scope.folder) {
                    if self.view == View::Doc { self.show_view(View::Folder,cx); hide_keyboard(); return; }
                    let parent = self.folder_current.as_ref().and_then(|id|self.entries.iter().find(|e|&e.id==id)).and_then(|e|e.meta.parent.clone());
                    if let Some(parent) = parent { self.folder_current=Some(parent); self.show_view(View::Folder,cx); hide_keyboard(); return; }
                }
                self.navigate_personal(View::Home,None,window,cx); return;
            }
            if self.home.tab == crate::home::HomeTab::Shared && self.mobile_collection_up(cx) { return; }
            if self.view == View::Home && self.home.tab == crate::home::HomeTab::Shared { self.home.tab = crate::home::HomeTab::Recent; }
            if self.view == View::Doc && self.folder_current.is_some() {
                self.show_view(View::Folder, cx);
                hide_keyboard();
            } else if self.view == View::Folder {
                self.folder_current = self.folder_current.as_ref()
                    .and_then(|id| self.entries.iter().find(|entry| &entry.id == id))
                    .and_then(|entry| entry.meta.parent.clone());
                self.show_view(if self.folder_current.is_some() { View::Folder } else { View::Home }, cx);
                hide_keyboard();
            } else { self.mobile_home(cx); }
        }

        pub(crate) fn mobile_open_settings(&mut self, cx: &mut Context<Self>) {
            if let Some(editor) = &self.editor { editor.update(cx, |ed, cx| { ed.cancel_background_composition(cx); }); }
            if !self.save_before(cx, |w, _, cx| w.mobile_open_settings(cx)) { return; }
            self.mobile_settings_return = self.view.clone();
            self.mobile_settings_root = true;
            self.mobile_settings_parent = None;
            self.open_settings(Page::General, cx);
            hide_keyboard();
        }

        pub(crate) fn mobile_close_settings(&mut self, cx: &mut Context<Self>) {
            let previous = self.mobile_settings_return.clone();
            self.show_view(previous, cx);
            hide_keyboard();
        }

        pub(crate) fn mobile_visible_entries(&self, cx: &App) -> Vec<crate::store::Entry> {
            if !self.query(cx).is_empty() { return self.search_results(cx); }
            if !self.is_shared() && self.home.tab == crate::home::HomeTab::Shared { let mut entries=self.sorted_entries(self.mobile_shared_listing(cx)); entries.sort_by_key(|e|!e.meta.pinned); return entries; }
            let mut entries = if self.is_shared() { self.entries.clone() } else { self.personal_navigation_entries(cx) };
            if self.home.tab == crate::home::HomeTab::Starred {
                entries.extend(self.shared_entries(cx));
                entries.retain(|entry| entry.meta.folder.is_none() && entry.meta.starred > 0);
            } else { entries.retain(|entry| entry.meta.parent == self.folder_current); }
            let mut entries = self.sorted_entries(entries);
            entries.sort_by_key(|e|!e.meta.pinned);
            entries
        }

        pub(crate) fn mobile_open_entry(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
            self.mobile_navigation.links.clear(); self.mobile_navigation.trash=None;
            if !self.is_shared() && self.mobile_collection(id,cx) { return; }
            if let Some(room) = self.shared_list_room(id,cx) { self.open_shared_note(&room,window,cx); }
            else if self.is_shared() && !self.entries.iter().any(|e|e.id==id) { self.navigate_personal(View::Home,Some(id.into()),window,cx); }
            else { self.open(id,window,cx); }
        }

        fn mobile_notes(&self, cx: &mut Context<Self>) -> AnyElement {
            let query = self.query(cx);
            let entries = self.mobile_visible_entries(cx);
            let empty = entries.is_empty();
            let notes = entries.into_iter().map(|entry| {
                let id = entry.id.clone();
                let folder = entry.meta.folder.is_some();
                let menu_id = id.clone();
                let summary = if folder { tr("文件夹", "Folder").to_owned() }
                    else { crate::search::snippet(&entry.body, &query).map(|(text, _)| text)
                        .unwrap_or_else(|| entry.body.chars().take(70).collect::<String>().replace('\n', " ")) };
                div().id(SharedString::from(format!("mobile-note-{id}"))).w_full().min_h(px(84.)).px(px(18.)).py(px(15.))
                    .border_b_1().border_color(Theme::line_divider()).flex().items_center().gap(px(12.)).cursor_pointer()
                    .active(|s| s.bg(Theme::fill_pressed()))
                    .when(folder, |d| d.child(ui::icon("folder", 22., Theme::primary())))
                    .child(div().flex_1().min_w_0().flex().flex_col().gap(px(5.))
                        .child(div().flex().items_center().gap(px(6.)).child(div().flex_1().min_w_0().text_size(px(18.)).line_height(px(24.)).font_weight(FontWeight::SEMIBOLD).truncate().child(display_title(&entry.title)))
                            .when(entry.meta.pinned,|d|d.child(ui::icon("pin-filled",16.,Theme::pinned()))))
                        .child(div().text_size(px(15.)).line_height(px(21.)).text_color(Theme::text_caption()).truncate().child(summary)))
                    .child(div().id(SharedString::from(format!("mobile-actions-{id}"))).size(px(44.)).flex_none().flex().items_center().justify_center()
                        .child(ui::icon("ellipsis", 22., Theme::text_caption())).on_click(cx.listener(move |_, _, _, cx| {
                            cx.stop_propagation(); crate::ipad::present_item_actions(&menu_id);
                        })))
                    .on_click(cx.listener(move |w, _, window, cx| { w.open(&id, window, cx); }))
            }).collect::<Vec<_>>();
            let tabs = (self.view == View::Home && query.is_empty()).then(|| {
                div().h(px(50.)).px(px(16.)).flex_none().flex().items_center().gap(px(8.)).children([
                    (crate::home::HomeTab::Recent,tr("我的文档","My notes")),
                    (crate::home::HomeTab::Starred,tr("收藏","Favorites")),
                ].into_iter().enumerate().map(|(index,(tab,label))| {
                    div().id(("mobile-tab",index)).min_h(px(44.)).px(px(12.)).rounded(px(9.)).flex().items_center().text_size(px(16.))
                        .when(self.home.tab == tab, |d| d.bg(Theme::fill_hover_strong())).child(label)
                        .on_click(cx.listener(move |w,_,_,cx| { w.home.tab = tab; cx.notify(); }))
                }))
            });
            div().flex_1().min_h_0().flex().flex_col()
                .child(div().id("mobile-search-field").h(px(52.)).flex_none().mx(px(16.)).my(px(10.)).px(px(12.)).rounded(px(12.)).bg(Theme::fill_hover())
                    .on_click(cx.listener(|w,_,window,cx| { crate::gpui_compat::focus(window,&w.search.focus_handle(cx),cx); gpui_mobile::show_keyboard(); }))
                    .flex().items_center().gap(px(10.)).child(ui::icon("search", 19., Theme::text_caption())).child(self.search.clone())
                    .when(!self.search.read(cx).text.is_empty(),|d|d.child(div().id("mobile-search-clear").size(px(44.)).flex_none().flex().items_center().justify_center().child(ui::icon("x",18.,Theme::text_caption()))
                        .on_click(cx.listener(|w,_,window,cx| { w.search.update(cx,|field,cx|field.set_text("",cx)); crate::gpui_compat::focus(window,&w.search.focus_handle(cx),cx); cx.notify(); })))))
                .children(tabs)
                .child(div().id("mobile-notes-list").flex_1().min_h_0().overflow_y_scroll().children(notes)
                    .when(empty, |d| d.child(div().p(px(24.)).text_size(px(17.)).text_color(Theme::text_caption())
                        .child(if !query.is_empty() { tr("没有找到匹配的结果，尝试其他关键词进行搜索","No matching notes. Try another keyword.") }
                            else if self.home.tab == crate::home::HomeTab::Starred && self.view == View::Home { tr("暂无收藏笔记","No favorite notes yet") }
                            else { tr("点右上角新建笔记", "Tap New to start a note") })) ))
                .into_any_element()
        }

        fn mobile_trash(&self, cx: &mut Context<Self>) -> AnyElement {
            div().id("mobile-trash").flex_1().min_h_0().overflow_y_scroll().children(self.trash.iter().map(|entry| {
                let id = entry.id.clone();
                div().id(SharedString::from(format!("mobile-trash-{}",entry.id))).min_h(px(72.)).px(px(18.)).py(px(12.))
                    .flex().items_center().gap(px(12.)).border_b_1().border_color(Theme::line_divider()).text_size(px(17.))
                    .child(div().flex_1().min_w_0().truncate().child(display_title(&entry.title)))
                    .child(div().size(px(44.)).flex().items_center().justify_center().child(ui::icon("ellipsis",22.,Theme::primary())))
                    .on_click(cx.listener(move |_,_,_,_| crate::ipad::present_item_actions(&id)))
            })).when(self.trash.is_empty(),|d| d.child(div().p(px(24.)).text_size(px(17.)).text_color(Theme::text_caption()).child(tr("回收站暂无内容","Trash is empty"))))
                .into_any_element()
        }

        pub(crate) fn render_mobile(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
            crate::ipad::publish_navigation(self,cx);
            let body = match self.view {
                View::Doc => {
                    let failure = match &self.status {
                        SaveStatus::Failed(message) => Some(message.clone()),
                        _ => None,
                    };
                    div().flex_1().min_h_0().relative().flex().flex_col()
                        .children(failure.map(|text| div().px(px(18.)).text_size(px(14.)).text_color(Theme::danger()).child(text)))
                        .child(div().flex_1().min_h_0().children(self.editor.clone()))
                        .into_any_element()
                }
                View::Home if self.is_shared() => self.render_shared_waiting(cx),
                View::Settings => div().into_any_element(),
                View::Trash => self.mobile_trash(cx),
                _ => self.mobile_notes(cx),
            };
            let export = self.render_export_md(cx);
            let search = self.render_search(window, cx);
            let busy = (self.library_switching || self.store.file_task_busy() || self.file_history.busy).then(|| deferred(div().absolute().inset_0().occlude()
                .bg(Theme::bg_body()).flex().items_center().justify_center().text_size(px(17.))
                .child(if self.library_switching { tr("正在打开文档库…","Opening library…") } else { tr("正在处理…","Working…") })
                .on_mouse_down(MouseButton::Left,|_,_,cx| cx.stop_propagation())).with_priority(30));
            let toast = self.render_toast(window);
            let theme_menu = self.render_theme_menu(cx);
            let lang_menu = self.render_lang_menu(cx);
            let pref_menu = self.render_pref_menu(cx);
            let move_dialog = self.render_move_dialog(cx);
            div().id("mobile-workspace").key_context("Workspace").track_focus(&self.focus)
                .size_full().flex().flex_col().bg(Theme::bg_body()).text_color(Theme::text()).font_family(theme::SYSTEM_FONT)
                .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                .map(|d| self.workspace_actions(d,cx))
                .child(body).children(export).children(search).children(theme_menu).children(lang_menu).children(pref_menu).children(move_dialog).children(toast).children(busy).into_any_element()
        }
    }
}
