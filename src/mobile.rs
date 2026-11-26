//! Touch metrics and compact navigation around the shared product editor.
#[cfg(target_os = "ios")]
use gpui::*;

#[cfg(target_os = "ios")]
// Both mobile idioms use UIKit chrome; a wide iPad must not reveal the desktop renderer.
pub fn native_shell(_window: &Window) -> bool { true }
pub fn control(desktop: f32) -> f32 { if cfg!(target_os = "ios") { desktop.max(44.) } else { desktop } }
pub fn label(desktop: f32) -> f32 { if cfg!(target_os = "ios") { desktop.max(16.) } else { desktop } }

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
    use crate::settings::{Page, tr};
    use crate::theme::{self, Theme};
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
            self.cancel_shared_open();
            cx.notify();
            if self.view == View::Doc && self.preview.is_some() { self.exit_preview(cx); return; }
            if self.view == View::Doc && self.history.is_some() { self.close_history(cx); return; }
            if self.view == View::Settings {
                if !self.mobile_settings_root && self.settings_page==Page::Shared { self.close_sharing(window,cx); }
                else if self.mobile_settings_root || self.settings_page == Page::SharedSync { self.mobile_close_settings(window,cx); }
                else if self.settings_page == Page::CloudAccess { self.close_settings(cx); }
                else if let Some(parent) = self.mobile_settings_parent.take() { self.open_settings(parent,cx); }
                else if self.cloud_settings_from_share { self.mobile_close_settings(window,cx); }
                else {
                    if self.settings_page == Page::Cloud { self.cancel_cloud_enable(); }
                    self.mobile_settings_root = true; cx.notify();
                }
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
                    let folder = self.folder_current.as_ref().and_then(|id|self.entries.iter()
                        .find(|entry| &entry.id == id && entry.meta.folder.is_some()));
                    // A document can be opened directly from a personal list
                    // (for example from Favorites), with no folder on the stack.
                    if self.view == View::Doc && folder.is_some() { self.show_view(View::Folder,cx); hide_keyboard(); return; }
                    let parent = folder.and_then(|entry|entry.meta.parent.clone());
                    if let Some(parent) = parent { self.folder_current=Some(parent); self.show_view(View::Folder,cx); hide_keyboard(); return; }
                }
                self.mobile_return_from_share(window,cx); return;
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
            self.mobile_open_settings_for(false, cx);
        }

        pub(crate) fn mobile_open_settings_for(&mut self, shared_library: bool, cx: &mut Context<Self>) {
            if let Some(editor) = &self.editor { editor.update(cx, |ed, cx| { ed.cancel_background_composition(cx); }); }
            if !self.save_before(cx, move |w, _, cx| w.mobile_open_settings_for(shared_library, cx)) { return; }
            self.mobile_settings_return = self.view.clone();
            self.mobile_settings_root = !shared_library;
            self.mobile_settings_parent = None;
            self.open_settings(if shared_library { Page::SharedSync } else { Page::General }, cx);
            hide_keyboard();
        }

        pub(crate) fn mobile_close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            let previous = self.mobile_settings_return;
            let restore_shared_sync = self.take_cloud_settings_return();
            self.show_view(previous, cx);
            self.restore_shared_sync(restore_shared_sync,cx);
            self.complete_shared_open(window,cx);
            hide_keyboard();
        }

        pub(crate) fn mobile_visible_entries(&self, cx: &App) -> Vec<crate::store::Entry> {
            let personal = !self.is_shared() || self.mobile_personal_browser(cx);
            if !self.query(cx).is_empty() { return self.search_results(cx); }
            if personal && self.home.tab == crate::home::HomeTab::Shared { return self.mobile_order_entries(self.sorted_shared_entries(self.shared_collection_entries(cx))); }
            let mut entries = if personal { self.personal_navigation_entries(cx) } else { self.entries.clone() };
            if !personal && self.current_room(cx).is_some_and(|room| room.scope.folder) {
                entries.retain(|entry| entry.meta.parent == self.folder_current && self.folder_current.is_some());
                // Personal pins remain available in shortcuts and menus. The
                // shared folder itself has a single order for all members.
                for entry in &mut entries { entry.meta.pinned = false; }
                return Self::shared_folder_order(entries);
            }
            if self.home.tab == crate::home::HomeTab::Starred {
                entries = self.favorite_entries(cx);
            } else { entries.retain(|entry| entry.meta.parent == self.folder_current); }
            for entry in &mut entries {
                if let Some((pinned, starred)) = self.shared_child_marks(&entry.id, cx) {
                    entry.meta.pinned = pinned;
                    entry.meta.starred = starred;
                }
            }
            self.mobile_order_entries(self.sorted_entries(entries))
        }

        pub(crate) fn mobile_open_entry(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
            self.mobile_navigation.links.clear(); self.mobile_navigation.trash=None;
            if !self.is_shared() && self.mobile_collection(id,cx) { return; }
            if let Some(room) = self.shared_list_room(id,cx) { self.open_shared_note(&room,window,cx); }
            else if self.home.tab == crate::home::HomeTab::Starred && let Some(room) = self.shared_child_room(id,cx) {
                self.open_shared_item_interactive(room,id.into(),None,window,cx);
            }
            else if self.is_shared() && (self.mobile_personal_browser(cx) || !self.entries.iter().any(|e|e.id==id)) { self.navigate_personal(View::Home,Some(id.into()),window,cx); }
            else { self.open(id,window,cx); }
        }

        pub(crate) fn mobile_contextual_sharing(&self) -> bool {
            self.view == View::Settings && self.settings_page == Page::Shared && !self.mobile_settings_root
                && matches!(self.mobile_settings_return,View::Doc | View::Home | View::Folder | View::Shared)
        }

        pub(crate) fn mobile_list_notice(&self) -> Option<(&'static str, &str)> {
            use crate::ui::ToastKind;
            self.toast.as_ref().filter(|(_, _, time)| time.elapsed().as_secs_f32() < 3.)
                .map(|(kind, text, _)| (match kind { ToastKind::Success => "success", ToastKind::Error => "error", ToastKind::Info => "info" }, text.as_ref()))
                .or_else(|| self.sharing_status_notice().map(|text| ("info", text)))
        }

        pub(crate) fn render_mobile(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
            crate::ipad::publish_navigation(self,cx);
            let body = match self.view {
                _ if self.view == View::Doc || self.mobile_contextual_sharing() && self.mobile_settings_return == View::Doc => {
                    let failure = match &self.status {
                        SaveStatus::Failed(message) => Some(message.clone()),
                        _ => None,
                    };
                    div().flex_1().min_h_0().relative().flex().flex_col()
                        .children(failure.map(|text| div().px(px(18.)).text_size(px(14.)).text_color(Theme::danger()).child(text)))
                        .map(|d| {
                            #[cfg(feature = "cloudkit-sync")]
                            let d = d.children(self.render_conflict_banner(cx));
                            d
                        })
                        .child(div().flex_1().min_h_0().children(self.editor.clone()))
                        .into_any_element()
                }
                _ => div().into_any_element(),
            };
            let export = self.render_export_md(cx);
            let search = self.render_search(window, cx);
            let busy = (self.library_switching || self.store.file_task_busy() || self.file_history.busy).then(|| deferred(div().absolute().inset_0().occlude()
                .bg(Theme::bg_body()).flex().items_center().justify_center().text_size(px(17.))
                .child(if self.library_switching { tr("正在打开文档库…","Opening library…") } else { tr("正在处理…","Working…") })
                .on_mouse_down(MouseButton::Left,|_,_,cx| cx.stop_propagation())).with_priority(30));
            #[cfg(feature = "cloudkit-sync")]
            let conflict_review = self.render_conflict_review(window, cx);
            let theme_menu = self.render_theme_menu(cx);
            let lang_menu = self.render_lang_menu(cx);
            let pref_menu = self.render_pref_menu(cx);
            let move_dialog = self.render_move_dialog(cx);
            div().id("mobile-workspace").key_context("Workspace").track_focus(&self.focus)
                .size_full().flex().flex_col().bg(Theme::bg_body()).text_color(Theme::text()).font_family(theme::SYSTEM_FONT)
                .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                .map(|d| self.workspace_actions(d,cx))
                .child(canvas(|_, _, _| (), |_, _, _, _| crate::editor::touch::clear_scroll_hitboxes()).absolute().size_full())
                .child(body).children(export).children(search).children(theme_menu).children(lang_menu).children(pref_menu).children(move_dialog).children(busy)
                .map(|d| {
                    #[cfg(feature = "cloudkit-sync")]
                    let d = d.children(conflict_review);
                    d
                }).into_any_element()
        }
    }
}
