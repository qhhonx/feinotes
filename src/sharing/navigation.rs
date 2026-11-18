//! Replace only a window's workspace when navigating between isolated libraries.
use super::*;
use crate::sidebar::SidebarEntry;

#[derive(Clone)]
struct SidebarNavigation {
    entry: Option<SidebarEntry>,
    prepared: Option<crate::shared_store::SharedStore>,
}

pub(super) struct PersonalRoute {
    document: Option<String>,
    selection: Option<crate::editor::core::Selection>,
    home: crate::home::Home,
    modified: Option<u64>,
    query: String,
    folder: Option<String>,
    collection: Option<String>,
}

impl Workspace {
    #[cfg(target_os = "ios")]
    pub(crate) fn mobile_personal_browser(&self, cx: &App) -> bool {
        self.sharing.personal_route.is_some() && shared::scope(&self.store,cx).is_some_and(|scope| !scope.folder)
    }

    #[cfg(target_os = "ios")]
    pub(crate) fn remember_personal_browser(&mut self, cx: &App) {
        if let Some(route) = &mut self.sharing.personal_route {
            route.home.tab = self.home.tab;
            route.folder = self.folder_current.clone();
            route.collection = self.sharing.collection.clone();
            route.query = self.search.read(cx).text.clone();
        }
    }

    pub(crate) fn navigate_shared(&mut self, room: Room, invitation: Option<(String, Access)>, window: &mut Window, cx: &mut Context<Self>) {
        let target = room.scope.note.clone();
        self.navigate_shared_item(room, target, invitation, window, cx);
    }

    pub(crate) fn navigate_shared_item(&mut self, room: Room, target: String, invitation: Option<(String, Access)>, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_shared_item_using(room, target, invitation, None, window, cx);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn cancel_sidebar_open(&mut self) { self.sharing.sidebar_load = None; }

    pub(crate) fn navigate_shared_sidebar(&mut self, room: Room, entry: SidebarEntry, window: &mut Window, cx: &mut Context<Self>) {
        self.open_shared_item_interactive(room, entry.id.clone(), Some(entry), window, cx);
    }

    pub(crate) fn open_shared_item_interactive(&mut self, room: Room, target: String, entry: Option<SidebarEntry>, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        {
            self.cancel_sidebar_open();
            self.sharing.pending_open = None;
            if self.sharing.busy { return; }
            if !self.is_shared() && shared::original::awaiting_publication(self.store.root(), &room) {
                self.create_shared_note(&room.scope.note, window, cx);
                return;
            }
            if self.viewing_shared_item(&room, &target, cx) {
                self.sidebar_entry = entry;
                self.finish_mac_shared_waiting(window, cx);
                cx.notify();
                return;
            }
            let request = uuid::Uuid::new_v4();
            let workspace = cx.entity_id();
            let handle = WindowHandle::<Self>::new(window.window_handle().window_id());
            let requested = room.clone();
            if let Err(error) = shared::load_sidebar_store(room, cx, Box::new(move |result, cx| {
                let _ = handle.update(cx, |ws, window, cx| {
                    if cx.entity_id() != workspace || ws.sharing.sidebar_load != Some(request) { return; }
                    ws.cancel_sidebar_open();
                    match result {
                        Ok(store) => ws.navigate_shared_item_using(requested, target, None,
                            Some(SidebarNavigation { entry, prepared: Some(store) }), window, cx),
                        Err(error) => {
                            ws.report_shared_open_error(&error, cx);
                            ws.finish_mac_shared_waiting(window, cx);
                        }
                    }
                });
            })) {
                self.report_shared_open_error(&error.to_string(), cx);
                return;
            }
            self.sharing.sidebar_load = Some(request);
        }
        #[cfg(target_os = "ios")]
        self.navigate_shared_item_using(room, target, None, Some(SidebarNavigation { entry, prepared: None }), window, cx);
    }

    fn viewing_shared_item(&self, room: &Room, target: &str, cx: &App) -> bool {
        if shared::scope(&self.store, cx) != Some(&room.scope) { return false; }
        #[cfg(target_os = "macos")]
        if self.view == View::Home && self.sharing.sidebar_waiting.as_deref() == Some(target) { return true; }
        (self.view == View::Doc && self.current.as_deref() == Some(target))
            || (self.view == View::Folder && self.folder_current.as_deref() == Some(target))
    }

    fn report_shared_open_error(&mut self, error: &str, cx: &mut Context<Self>) {
        crate::logging::write(&format!("Open shared document: {error}"));
        self.show_toast_kind(ui::ToastKind::Error, tr("暂时无法打开共享文档。请重试。", "Could not open the shared document. Try again."), cx);
    }

    fn navigate_shared_item_using(&mut self, room: Room, target: String, invitation: Option<(String, Access)>, sidebar: Option<SidebarNavigation>, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        self.cancel_sidebar_open();
        if self.sharing.busy { return; }
        #[cfg(target_os = "ios")]
        if self.sharing.opening.as_ref().is_some_and(|opening| opening.target == target)
            && shared::scope(&self.store, cx) == Some(&room.scope) { return; }
        if !self.is_shared() && shared::original::awaiting_publication(self.store.root(), &room) {
            self.create_shared_note(&room.scope.note, window, cx);
            return;
        }
        if invitation.is_none() && self.viewing_shared_item(&room, &target, cx) {
            self.sidebar_entry = sidebar.and_then(|sidebar| sidebar.entry);
            cx.notify();
            return;
        }
        self.close_history(cx);
        let retry_room = room.clone(); let retry_invitation = invitation.clone();
        let retry_target = target.clone(); let retry_sidebar = sidebar.clone();
        if !self.save_before(cx, move |w, window, cx| w.navigate_shared_item_using(retry_room, retry_target, retry_invitation, retry_sidebar, window, cx)) { return; }
        let (entry, prepared) = sidebar.map(|sidebar| (sidebar.entry, sidebar.prepared)).unwrap_or_default();
        let opened = match prepared {
            Some(store) => shared::check_opened_store(store, &room, cx),
            None => shared::store_for(&room, cx),
        };
        let store = match opened {
            Ok(store) => store,
            Err(error) => {
                self.report_shared_open_error(&format!("{error:#}"), cx);
                return;
            }
        };
        self.remember_scroll(cx);
        let route = if self.is_shared() { self.sharing.personal_route.take() } else { Some(Box::new(PersonalRoute {
            document: self.current.clone(), selection: self.editor.as_ref().map(|editor| editor.read(cx).core.sel),
            query: self.search.read(cx).text.clone(), folder: self.folder_current.clone(), collection: self.sharing.collection.clone(),
            home: std::mem::take(&mut self.home), modified: self.current.as_ref().and_then(|id| self.entries.iter().find(|entry| &entry.id == id).map(|entry| entry.meta.modified)),
        })) };
        let sidebar = self.sidebar_open; let width = self.sidebar_width; let folded = self.sharing.folded; let personal_folded = self.folded;
        let expanded = self.expanded.clone();
        let folder_drag = std::mem::take(&mut self.folder_drag); let shared_drag = std::mem::take(&mut self.sharing.sidebar); let scroll = self.list_scroll.clone();
        let original = self.sharing.original.take();
        #[cfg(target_os = "macos")]
        // The native backdrop belongs to the window. Keep it behind the old frame until the new root draws.
        let native_sidebar = std::mem::take(&mut self.native_sidebar);
        let browser_tab = route.as_ref().map(|r|r.home.tab).unwrap_or(self.home.tab);
        let browser_collection = route.as_ref().and_then(|r|r.collection.clone());
        let browser_query = route.as_ref().map(|r|r.query.clone()).unwrap_or_default();
        let browser_folder = route.as_ref().and_then(|r|r.folder.clone());
        let folder_scope = room.scope.folder;
        let note = target;
        #[cfg(target_os = "ios")]
        let opening = OpeningShared { target: note.clone(), title: room.title.clone() };
        window.replace_root(cx, |window, cx| {
            #[cfg(target_os = "macos")]
            let waiting = note.clone();
            let mut ws = Workspace::build_window(false, Some(note), Some(store), window, cx);
            #[cfg(target_os = "macos")]
            { ws.native_sidebar = native_sidebar; }
            ws.sidebar_entry = entry;
            #[cfg(target_os = "macos")]
            if ws.view == View::Home { ws.sharing.sidebar_waiting = Some(waiting); }
            ws.sidebar_open = sidebar; ws.sidebar_width = width; ws.folded = personal_folded;
            ws.expanded = expanded; ws.folder_drag = folder_drag; ws.sharing.sidebar = shared_drag; ws.list_scroll = scroll;
            ws.folder_drag.personal_rows = None;
            #[cfg(target_os = "ios")]
            {
                if ws.view == View::Home { ws.sharing.opening = Some(opening); }
                if !folder_scope {
                    if let Some(route) = &route { ws.home.inherit_mobile_list_order(&route.home); }
                    ws.home.tab = browser_tab; ws.folder_current = browser_folder; ws.sharing.collection = browser_collection;
                    ws.search.update(cx,|field,cx|field.set_text(&browser_query,cx));
                }
            }
            #[cfg(not(target_os = "ios"))]
            let _ = (browser_tab,browser_query,browser_folder,browser_collection,folder_scope);
            ws.sharing.personal_route = route; ws.sharing.folded = folded;
            if let Some((email, permission)) = invitation {
                ws.open_invitation(room, window, cx);
                ws.sharing.email.update(cx, |field, cx| field.set_text(&email, cx));
                ws.sharing.permission = permission;
                ws.sharing.original = original;
                ws.manage_shared_note("invite", email, Some(permission), window, cx);
            }
            ws
        });
        cx.defer(cloud_sync::release_unused);
    }

    pub(crate) fn navigate_personal(&mut self, view: View, document: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_personal_from(view, document, None, window, cx);
    }

    pub(crate) fn navigate_personal_from(&mut self, view: View, document: Option<String>, entry: Option<SidebarEntry>, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        self.cancel_sidebar_open();
        if !self.is_shared() {
            if let Some(id) = document {
                if self.try_open(&id, window, cx) { self.sidebar_entry = entry; cx.notify(); }
            } else { self.show_view(view, cx); }
            return;
        }
        if self.sharing.busy { return; }
        self.close_history(cx);
        let retry = document.clone(); let retry_entry = entry.clone();
        if !self.save_before(cx, move |w, window, cx| w.navigate_personal_from(view, retry, retry_entry, window, cx)) { return; }
        self.remember_scroll(cx);
        let route = self.sharing.personal_route.take();
        let sidebar = self.sidebar_open; let width = self.sidebar_width; let folded = self.sharing.folded; let personal_folded = self.folded;
        let expanded = self.expanded.clone();
        let folder_drag = std::mem::take(&mut self.folder_drag); let shared_drag = std::mem::take(&mut self.sharing.sidebar); let scroll = self.list_scroll.clone();
        #[cfg(target_os = "macos")]
        let native_sidebar = std::mem::take(&mut self.native_sidebar);
        let start = document.clone().or_else(|| route.as_ref().and_then(|route| route.document.clone()));
        window.replace_root(cx, |window, cx| {
            let mut ws = Workspace::build_window(false, start, None, window, cx);
            #[cfg(target_os = "macos")]
            { ws.native_sidebar = native_sidebar; }
            ws.sidebar_open = sidebar; ws.sidebar_width = width; ws.folded = personal_folded; ws.sharing.folded = folded;
            ws.expanded = expanded; ws.folder_drag = folder_drag; ws.sharing.sidebar = shared_drag; ws.list_scroll = scroll;
            ws.folder_drag.personal_rows = None;
            if let Some(route) = route {
                ws.home = route.home;
                #[cfg(target_os = "ios")]
                { ws.folder_current = route.folder; ws.sharing.collection = route.collection; ws.search.update(cx,|field,cx|field.set_text(&route.query,cx)); }
                if document.is_none() && let (Some(selection), Some(editor)) = (route.selection, &ws.editor) {
                    let unchanged = ws.current.as_ref().and_then(|id| ws.entries.iter().find(|entry| &entry.id == id).map(|entry| entry.meta.modified)) == route.modified;
                    editor.update(cx, |editor, cx| { editor.core.sel = if unchanged { selection } else { crate::editor::core::Selection::caret(selection.head) }; editor.core.fix_selection(); cx.notify(); });
                }
            }
            if let Some(id) = document {
                if ws.try_open(&id, window, cx) { ws.sidebar_entry = entry; }
            }
            else { ws.show_view(view, cx); }
            ws
        });
        cx.defer(cloud_sync::release_unused);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn finish_mac_shared_waiting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.sidebar_load.is_some() || self.view != View::Home || !self.is_shared() { return; }
        let target = self.sharing.sidebar_waiting.clone().or_else(|| shared::scope(&self.store, cx).map(|scope| scope.note.clone()));
        if let Some(target) = target && self.entries.iter().any(|entry| entry.id == target) {
            let entry = self.sidebar_entry.clone();
            if self.try_open(&target, window, cx) { self.sidebar_entry = entry; self.sharing.sidebar_waiting = None; }
        }
    }
}

impl Workspace {
    #[cfg(feature = "devtools")]
    pub(super) fn personal_sidebar_rows(&self, cx: &App) -> Vec<(crate::store::Entry, usize)> {
        let entries = cx.global::<crate::shared_store::SharedStore>().clone().list();
        crate::tree::Tree::new(&entries).rows(|id| self.expanded.contains(id)).into_iter().map(|(entry, depth)| (entry.clone(), depth)).collect()
    }

}
