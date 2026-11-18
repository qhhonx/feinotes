//! Replace only a window's workspace when navigating between isolated libraries.
use super::*;

pub(super) struct PersonalRoute {
    document: Option<String>,
    selection: Option<crate::editor::core::Selection>,
    home: crate::home::Home,
    modified: Option<u64>,
    query: String,
    folder: Option<String>,
}

impl Workspace {
    pub(crate) fn navigate_shared(&mut self, room: Room, invitation: Option<(String, Access)>, window: &mut Window, cx: &mut Context<Self>) {
        let target = room.scope.note.clone();
        self.navigate_shared_item(room, target, invitation, window, cx);
    }

    pub(crate) fn navigate_shared_item(&mut self, room: Room, target: String, invitation: Option<(String, Access)>, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy { return; }
        if !self.is_shared() && shared::original::awaiting_publication(self.store.root(), &room) {
            self.create_shared_note(&room.scope.note, window, cx);
            return;
        }
        if invitation.is_none() && self.view == View::Doc && self.current.as_deref() == Some(&target)
            && shared::scope(&self.store, cx) == Some(&room.scope) { return; }
        self.close_history(cx);
        let retry_room = room.clone(); let retry_invitation = invitation.clone();
        let retry_target = target.clone();
        if !self.save_before(cx, move |w, window, cx| w.navigate_shared_item(retry_room, retry_target, retry_invitation, window, cx)) { return; }
        let store = match shared::store_for(&room, cx) {
            Ok(store) => store,
            Err(error) => { self.show_toast_kind(ui::ToastKind::Error, &settings::diagnostic(&error.to_string()), cx); return; }
        };
        self.remember_scroll(cx);
        let route = if self.is_shared() { self.sharing.personal_route.take() } else { Some(Box::new(PersonalRoute {
            document: self.current.clone(), selection: self.editor.as_ref().map(|editor| editor.read(cx).core.sel),
            query: self.search.read(cx).text.clone(), folder: self.folder_current.clone(),
            home: std::mem::take(&mut self.home), modified: self.current.as_ref().and_then(|id| self.entries.iter().find(|entry| &entry.id == id).map(|entry| entry.meta.modified)),
        })) };
        let sidebar = self.sidebar_open; let width = self.sidebar_width; let folded = self.sharing.folded; let personal_folded = self.folded;
        let expanded = self.expanded.clone();
        let folder_drag = std::mem::take(&mut self.folder_drag); let shared_drag = std::mem::take(&mut self.sharing.sidebar); let scroll = self.list_scroll.clone();
        let original = self.sharing.original.take();
        let browser_tab = route.as_ref().map(|r|r.home.tab).unwrap_or(self.home.tab);
        let browser_query = route.as_ref().map(|r|r.query.clone()).unwrap_or_default();
        let browser_folder = route.as_ref().and_then(|r|r.folder.clone());
        let folder_scope = room.scope.folder;
        let note = target;
        window.replace_root(cx, |window, cx| {
            let mut ws = Workspace::build_window(false, Some(note), Some(store), window, cx);
            ws.sidebar_open = sidebar; ws.sidebar_width = width; ws.folded = personal_folded;
            ws.expanded = expanded; ws.folder_drag = folder_drag; ws.sharing.sidebar = shared_drag; ws.list_scroll = scroll;
            ws.folder_drag.personal_rows = None;
            #[cfg(target_os = "ios")]
            { if !folder_scope { ws.home.tab = browser_tab; ws.folder_current = browser_folder; ws.search.update(cx,|field,cx|field.set_text(&browser_query,cx)); } }
            #[cfg(not(target_os = "ios"))]
            let _ = (browser_tab,browser_query,browser_folder,folder_scope);
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
        if !self.is_shared() {
            if let Some(id) = document { self.open(&id, window, cx); } else { self.show_view(view, cx); }
            return;
        }
        if self.sharing.busy { return; }
        self.close_history(cx);
        let retry = document.clone();
        if !self.save_before(cx, move |w, window, cx| w.navigate_personal(view, retry, window, cx)) { return; }
        self.remember_scroll(cx);
        let route = self.sharing.personal_route.take();
        let sidebar = self.sidebar_open; let width = self.sidebar_width; let folded = self.sharing.folded; let personal_folded = self.folded;
        let expanded = self.expanded.clone();
        let folder_drag = std::mem::take(&mut self.folder_drag); let shared_drag = std::mem::take(&mut self.sharing.sidebar); let scroll = self.list_scroll.clone();
        let start = document.clone().or_else(|| route.as_ref().and_then(|route| route.document.clone()));
        window.replace_root(cx, |window, cx| {
            let mut ws = Workspace::build_window(false, start, None, window, cx);
            ws.sidebar_open = sidebar; ws.sidebar_width = width; ws.folded = personal_folded; ws.sharing.folded = folded;
            ws.expanded = expanded; ws.folder_drag = folder_drag; ws.sharing.sidebar = shared_drag; ws.list_scroll = scroll;
            ws.folder_drag.personal_rows = None;
            if let Some(route) = route {
                ws.home = route.home;
                #[cfg(target_os = "ios")]
                { ws.folder_current = route.folder; ws.search.update(cx,|field,cx|field.set_text(&route.query,cx)); }
                if document.is_none() && let (Some(selection), Some(editor)) = (route.selection, &ws.editor) {
                    let unchanged = ws.current.as_ref().and_then(|id| ws.entries.iter().find(|entry| &entry.id == id).map(|entry| entry.meta.modified)) == route.modified;
                    editor.update(cx, |editor, cx| { editor.core.sel = if unchanged { selection } else { crate::editor::core::Selection::caret(selection.head) }; editor.core.fix_selection(); cx.notify(); });
                }
            }
            if let Some(id) = document { ws.open(&id, window, cx); }
            else { ws.show_view(view, cx); }
            ws
        });
        cx.defer(cloud_sync::release_unused);
    }
}

impl Workspace {
    #[cfg(feature = "devtools")]
    pub(super) fn personal_sidebar_rows(&self, cx: &App) -> Vec<(crate::store::Entry, usize)> {
        let entries = cx.global::<crate::shared_store::SharedStore>().clone().list();
        crate::tree::Tree::new(&entries).rows(|id| self.expanded.contains(id)).into_iter().map(|(entry, depth)| (entry.clone(), depth)).collect()
    }

}
