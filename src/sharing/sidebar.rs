//! Shared navigation uses the personal sidebar's drag and animation engine.
use super::*;
use crate::{cloud_sync::activity::ActivityView, sidebar::{SidebarEntry, SidebarSection}, store::{Entry, Store}};

pub(super) fn notify(cx: &mut App) {
    cx.defer(|cx| {
        for handle in cx.windows().into_iter().filter_map(|handle| handle.downcast::<Workspace>()) {
            let _ = handle.update(cx, |_, _, cx| cx.notify());
        }
    });
}

impl Workspace {
    fn viewing_shared_folder(&self, id: &str, cx: &App) -> bool {
        matches!(self.view, View::Folder | View::Doc)
            && shared::scope(&self.store, cx).is_some_and(|scope| scope.folder && scope.note == id)
    }

    pub(crate) fn shared_entries(&self, cx: &App) -> Vec<Entry> {
        let tree = cx.global::<organizer::Organizer>();
        let prefs = preferences::active(cx);
        let personal;
        let entries = if self.is_shared() {
            personal = self.personal_navigation_entries(cx);
            &personal
        } else {
            &self.entries
        };
        let originals: std::collections::HashMap<_, _> = entries.iter()
            .filter(|entry| entry.meta.share_root.as_deref() == Some(&entry.id))
            .map(|entry| (entry.id.as_str(), &entry.meta)).collect();
        cx.try_global::<ActivityView>().into_iter().flat_map(|view| &view.0).map(|row| {
            let id = row.room.scope.note.clone();
            let parent = tree.parent(&id);
            let mut entry = Entry { id, title: row.room.title.clone(), icon: row.icon.clone(), body: row.body.clone(),
                meta: crate::store::Meta { parent, folder: row.room.scope.folder.then(|| row.room.title.clone()),
                    created: row.created, modified: row.modified, opened: row.opened, ..Default::default() },
                reminders: vec![], links: vec![], note: None, root: String::new() };
            prefs.project(&mut entry);
            if row.room.scope.owner.is_none() && let Some(meta) = originals.get(entry.id.as_str()) {
                entry.meta.pinned = meta.pinned;
                entry.meta.starred = meta.starred;
            }
            entry
        }).collect()
    }

    pub(crate) fn shared_shortcut_row(&self, entry: &Entry, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = entry.id.clone();
        let title = display_title(&entry.title);
        div().id(SharedString::from(format!("shared-pinned-{id}"))).group("doc-row").h(px(crate::sidebar::ROW_H)).mb(px(crate::sidebar::ROW_GAP))
            .pl(px(6.)).flex().items_center().gap(px(9.)).rounded(px(crate::sidebar::ROW_R)).cursor_pointer()
            .text_size(px(crate::theme::ui_font())).text_color(crate::sidebar::text_color()).hover(|s| s.bg(crate::sidebar::hover()))
            .when(self.sidebar_row_selected(SidebarSection::Pinned, &id), |d| d.bg(crate::sidebar::selection_fill()))
            .child(crate::sidebar::entry_icon(entry, self.viewing_shared_folder(&id, cx), true, cx))
            .child(div().flex_1().min_w_0().flex().items_center()
                .child(crate::ui::document_title(&title).child(crate::ui::ui_text(title)))
                .when(self.document_has_shared_status(entry, cx), |d| {
                    d.child(crate::sidebar::shared_document_badge().ml(px(6.)))
                }))
            .child(self.more_button(entry, "shared-pinned", cx))
            .on_mouse_down(MouseButton::Right, cx.listener({ let id = id.clone(); move |w, event: &MouseDownEvent, _, cx| {
                w.row_menu = Some(crate::app::RowMenu { id: format!("shared:{id}"), pos: event.position, corner: Anchor::TopLeft, sub: false, batch: false }); cx.notify();
            }}))
            .on_click(cx.listener(move |w, _, window, cx| { if let Some(room) = w.shared_list_room(&id, cx).or_else(|| w.shared_child_room(&id, cx)) {
                w.navigate_shared_sidebar(room, SidebarEntry::new(SidebarSection::Pinned, &id), window, cx);
            } }))
    }

    pub(crate) fn organized_shared_entries(&self, cx: &App) -> Vec<Entry> {
        let tree = cx.global::<organizer::Organizer>();
        let mut rows = self.shared_entries(cx);
        rows.extend(tree.folders().map(|mut entry| {
            entry.meta.parent = tree.parent(&entry.id);
            preferences::active(cx).project(&mut entry);
            entry
        }));
        // Display identical ranks in ID order even when the devices' local dates differ.
        rows.sort_by(|a, b| {
            let rank = |entry: &Entry| { let (_, rank, _) = entry.meta.place(); rank };
            rank(a).cmp(&rank(b)).then_with(|| a.id.cmp(&b.id))
        });
        for (i, entry) in rows.iter_mut().filter(|entry| entry.meta.place().0).enumerate() {
            let rank = i as u64 + 1;
            if entry.meta.parent.is_some() { entry.meta.order = rank; }
            else { entry.meta.root_order = Some(rank); }
        }
        rows
    }

    pub(crate) fn shared_collection_entries(&self, cx: &App) -> Vec<Entry> {
        let folder = self.sharing.collection.as_deref().filter(|id| cx.global::<organizer::Organizer>().is_folder(id));
        self.organized_shared_entries(cx).into_iter().filter(|entry| entry.meta.parent.as_deref() == folder).collect()
    }

    #[cfg(all(target_os = "macos", feature = "devtools"))]
    pub(crate) fn qa_shared_root_order(&self, cx: &App) -> Vec<String> {
        let entries = self.organized_shared_entries(cx);
        crate::tree::Tree::new(&entries).children(None).iter().map(|entry| entry.id.clone()).collect()
    }

    pub(crate) fn shared_child_room(&self, id: &str, cx: &App) -> Option<Room> {
        if let Some(room) = self.current_room(cx).filter(|room| room.scope.folder && room.scope.note != id)
            && self.entries.iter().any(|entry| entry.id == id && entry.meta.folder.is_none()) { return Some(room); }
        let entries = self.shared_sidebar_entries(cx);
        let entry = entries.iter().find(|entry| entry.id == id && entry.meta.folder.is_none())?;
        let root = entry.meta.share_root.as_deref().filter(|root| *root != id)?;
        self.shared_list_room(root, cx)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn shared_folder_context(&self, id: &str, cx: &App) -> Option<(Room, String)> {
        if let Some(room) = self.current_room(cx).filter(|room| room.scope.folder)
            && let Some(entry) = self.entries.iter().find(|entry| entry.id == id && entry.meta.folder.is_some()) {
            return Some((room, entry.title.clone()));
        }
        if let Some(room) = self.shared_list_room(id, cx).filter(|room| room.scope.folder) {
            return Some((room.clone(), room.title));
        }
        let entries = self.shared_sidebar_entries(cx);
        let entry = entries.iter().find(|entry| entry.id == id && entry.meta.folder.is_some())?;
        let room = self.shared_list_room(entry.meta.share_root.as_deref()?, cx)?;
        Some((room, entry.title.clone()))
    }

    fn projected_folder_items<'a>(&self, row: &'a cloud_sync::activity::SharedRow, cx: &App) -> std::borrow::Cow<'a, [cloud_sync::activity::FolderItem]> {
        let scope = &row.room.scope;
        if shared::scope(&self.store, cx) == Some(scope) && self.entries.iter().any(|entry| entry.id == scope.note) {
            if shared::access(&self.store, cx) == Some(Access::Revoked) { return std::borrow::Cow::Borrowed(&[]); }
            return std::borrow::Cow::Owned(cloud_sync::activity::folder_items(scope, &self.entries));
        }
        std::borrow::Cow::Borrowed(&row.folder_items)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn project_owned_sidebar_folders(&self, entries: &mut Vec<Entry>, cx: &App) {
        let Some(view) = cx.try_global::<ActivityView>() else { return; };
        let originals: std::collections::HashSet<_> = entries.iter().filter(|entry|
            entry.meta.folder.is_some() && entry.meta.share_root.as_ref() == Some(&entry.id)).map(|entry| entry.id.clone()).collect();
        for row in view.0.iter().filter(|row| row.room.scope.owner.is_none() && row.room.scope.folder && originals.contains(&row.room.scope.note)) {
            let scope = &row.room.scope;
            let projected = self.projected_folder_items(row, cx);
            let mut children: std::collections::HashMap<_, _> = projected.iter().map(|item| {
                let entry = item.entry(scope);
                (entry.id.clone(), entry)
            }).collect();
            entries.retain_mut(|entry| {
                if entry.meta.share_root.as_ref() != Some(&scope.note) { return true; }
                if entry.id == scope.note {
                    entry.title = row.room.title.clone();
                    entry.meta.folder = Some(entry.title.clone());
                    return true;
                }
                let Some(current) = children.remove(&entry.id) else { return false; };
                entry.title = current.title;
                entry.icon = current.icon;
                entry.meta.folder = current.meta.folder;
                entry.meta.parent = current.meta.parent;
                entry.meta.order = current.meta.order;
                true
            });
            entries.extend(children.into_values());
        }
    }

    pub(crate) fn shared_sidebar_entries(&self, cx: &App) -> Vec<Entry> {
        let mut rows = self.organized_shared_entries(cx);
        let Some(view) = cx.try_global::<ActivityView>() else { return rows; };
        let mut seen: std::collections::HashSet<String> = rows.iter().map(|entry| entry.id.clone()).collect();
        for row in view.0.iter().filter(|row| row.room.scope.folder) {
            let scope = &row.room.scope;
            let items = self.projected_folder_items(row, cx);
            // Only attach descendants reachable from this room's own root.
            let mut entries: Vec<_> = items.iter().map(|item| item.entry(scope)).collect();
            let Some(root) = rows.iter().find(|entry| entry.id == scope.note) else { continue; };
            let mut root = root.clone();
            root.meta.parent = None;
            entries.push(root);
            let tree = crate::tree::Tree::new(&entries);
            let mut next = tree.children(Some(&scope.note)).to_vec();
            while let Some(entry) = next.pop() {
                if !seen.insert(entry.id.clone()) { continue; }
                let mut preview = entry.clone();
                let values = preferences::active(cx);
                preview.meta.pinned = values.get("pin", &entry.id) != 0;
                preview.meta.starred = values.get("star", &entry.id);
                rows.push(preview);
                next.extend(tree.children(Some(&entry.id)));
            }
        }
        rows
    }

    pub(crate) fn render_shared_sidebar_rows(&mut self, entries: Vec<Entry>, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.sharing.sidebar.shared_rows = Some(entries);
        self.with_sidebar(true, |w| w.render_folder_rows(window, cx))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn shared_sidebar_folder_writable(&self, room: &Room, cx: &App) -> bool {
        if !room.scope.folder { return false; }
        if let Some(writable) = shared::live_room_writable(room, cx) { return writable; }
        cx.try_global::<ActivityView>().and_then(|view| view.0.iter().find(|row| row.room.scope == room.scope))
            .is_some_and(|row| row.access.map_or(room.scope.owner.is_none(), Access::can_write))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn shared_drag_room(&self, root: &str, cx: &App) -> Option<Room> {
        self.current_room(cx).filter(|room| room.scope.note == root)
            .or_else(|| self.shared_list_room(root, cx))
            .filter(|room| self.shared_sidebar_folder_writable(room, cx))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn shared_drop_allowed(entries: &[Entry], room: &Room, id: &str, target: &str) -> bool {
        if id == room.scope.note || !entries.iter().any(|e| e.id == id && e.meta.parent.as_deref() != Some(target))
            || !entries.iter().any(|e| e.id == target && e.meta.folder.is_some()) { return false; }
        let tree = crate::tree::Tree::new(entries);
        let members = tree.subtree(&room.scope.note);
        members.iter().any(|member| member == id) && members.iter().any(|member| member == target)
            && !tree.subtree(id).iter().any(|member| member == target)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn move_shared_drop(&mut self, room: Room, id: String, target: String, cx: &mut Context<Self>) {
        if !self.shared_sidebar_folder_writable(&room, cx) { return; }
        if shared::scope(&self.store, cx) == Some(&room.scope) {
            self.commit_shared_drop(room, self.store.clone(), &id, &target, cx);
            return;
        }
        // A sidebar drop can update a room without switching away from the current page.
        let workspace = cx.weak_entity();
        let requested = room.clone();
        if let Err(error) = shared::load_sidebar_store(room, cx, Box::new(move |result, cx| {
            let _ = workspace.update(cx, |w, cx| match result {
                Ok(store) => w.commit_shared_drop(requested, store, &id, &target, cx),
                Err(error) => w.report_shared_drop_error(&error, cx),
            });
        })) {
            self.report_shared_drop_error(&error.to_string(), cx);
        }
    }

    #[cfg(target_os = "macos")]
    fn commit_shared_drop(&mut self, room: Room, store: crate::shared_store::SharedStore, id: &str, target: &str, cx: &mut Context<Self>) {
        let mut store = match shared::check_opened_store(store, &room, cx) {
            Ok(store) => store,
            Err(error) => { self.report_shared_drop_error(&error.to_string(), cx); return; }
        };
        // Permissions and parent links may have changed while loading the room.
        if !self.shared_sidebar_folder_writable(&room, cx)
            || !Self::shared_drop_allowed(&store.list(), &room, id, target) { return; }
        self.sharing.expanded.insert(target.to_owned());
        if self.store.root() == store.root() {
            self.move_docs(&[id.to_owned()], Some(target.to_owned()), cx);
        } else {
            match store.set_parent(id, Some(target)) {
                Ok(_) => {
                    cloud_sync::activity::update_folder_items(&room.scope, &store.list(), cx);
                    cloud_sync::recheck_account(store.root(), cx);
                    self.show_toast(settings::text("移动成功"), cx);
                    notify(cx);
                }
                Err(error) => self.report_shared_drop_error(&error.to_string(), cx),
            }
        }
        cx.notify();
    }

    #[cfg(target_os = "macos")]
    fn report_shared_drop_error(&mut self, error: &str, cx: &mut Context<Self>) {
        self.show_toast_kind(ui::ToastKind::Error, &crate::localized_format!("移动失败：{}", "Could not move document: {}", settings::diagnostic(error)), cx);
    }

    pub(crate) fn shared_sidebar_row(&self, entry: &Entry, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = entry.id.clone();
        let member = entry.meta.share_root.is_some();
        let collection = id.starts_with("collection-");
        let selected = self.sidebar_row_selected(SidebarSection::Shared, &id);
        let batch = self.shared_batch();
        if entry.meta.folder.is_some() {
            let has_children = self.folder_drag.has_children(&id);
            let menu_id = if collection { id.clone() } else { format!("shared:{id}") };
            let menu_kind = if collection { "collection" } else if cfg!(target_os = "macos") { "shared-folder" } else { "shared" };
            #[cfg(target_os = "macos")]
            let create = (!batch && !collection && self.sidebar_rename(&id, SidebarSection::Shared).is_none())
                .then(|| self.shared_list_room(entry.meta.share_root.as_deref().unwrap_or(&id), cx)).flatten()
                .filter(|room| self.shared_sidebar_folder_writable(room, cx))
                .map(|room| crate::folders::CreateTarget::SharedFolder { room, parent: id.clone(), section: SidebarSection::Shared });
            #[cfg(target_os = "macos")]
            let controls = (!batch && self.sidebar_rename(&id, SidebarSection::Shared).is_none())
                .then(|| self.folder_row_actions(entry, menu_kind, create.clone(), cx));
            #[cfg(target_os = "macos")]
            let title_reserve = if controls.is_some() && create.is_some() { 74. } else if controls.is_some() { 28. } else { 0. };
            #[cfg(target_os = "ios")]
            let title_reserve = 0.;
            let row = div().id(SharedString::from(format!("shared-sidebar-folder-{id}"))).h(px(crate::sidebar::ROW_H)).w_full().relative().group("doc-row")
                .flex().items_center().gap(px(0.)).pl(px(6.)).pr(px(6.)).rounded(px(crate::sidebar::ROW_R)).cursor_pointer()
                .hover(|s| s.bg(crate::sidebar::hover()))
                .when(batch, |d| d.child(crate::sidebar::batch_check_slot().mr(px(4.))))
                .child(div().size(px(20.)).flex_none().relative()
                    .child(crate::sidebar::entry_icon(entry, self.expanded.contains(&id), false, cx)
                        .when(has_children, |d| d.group_hover("doc-row", |s| s.opacity(0.))))
                    .when(has_children, |d| d.child(div().absolute().inset_0().flex().items_center().justify_center()
                        .opacity(0.).group_hover("doc-row", |s| s.opacity(1.))
                        .child(ui::icon(if self.expanded.contains(&id) { "chevron-down" } else { "chevron-right" }, 12., Theme::icon())))
                        .on_mouse_down(MouseButton::Left, cx.listener(|w, ev: &MouseDownEvent, _, _| {
                            w.sharing.sidebar.icon_press = true; w.sharing.sidebar.icon_alt = ev.modifiers.alt;
                        }))))
                .child(div().flex_1().min_w_0().flex().mr(px(title_reserve))
                    .child(self.sidebar_title(entry, SidebarSection::Shared, selected, self.folder_drop_target(&id), cx)))
                .when(!batch && (!member || cfg!(target_os = "macos")), |d| d
                    .on_mouse_down(MouseButton::Right, cx.listener(move |w, event: &MouseDownEvent, _, cx| {
                        w.row_menu = Some(crate::app::RowMenu { id: menu_id.clone(), pos: event.position, corner: Anchor::TopLeft, sub: false, batch: false });
                        w.create_menu = None;
                        cx.stop_propagation(); cx.notify();
                    })))
                .on_mouse_down(MouseButton::Left, cx.listener(move |w, event: &MouseDownEvent, _, cx| {
                    if !member || !w.shared_batch() { w.with_sidebar(true, |w| w.arm_folder_drag(&id, event.position, cx)); }
                    cx.stop_propagation();
                }));
            #[cfg(target_os = "macos")]
            let row = row.children(controls);
            #[cfg(target_os = "ios")]
            let row = row.when(!member, |row| row.child(self.more_button(entry, menu_kind, cx)));
            return row;
        }
        let checked = self.batch.as_ref().is_some_and(|ids| ids.contains(&id));
        let blocked = self.batch.as_ref().is_some_and(|ids| !checked && ids.len() >= crate::sidebar::BATCH_LIMIT);
        let unread = cx.try_global::<ActivityView>().is_some_and(|view| view.0.iter().any(|row| row.room.scope.note == id && row.unread));
        let dot = unread.then(crate::sidebar::shared_unread_dot);
        let more = (!member || cfg!(target_os = "macos")) && !batch && self.sidebar_rename(&id, SidebarSection::Shared).is_none();
        let end = if more { Some(self.more_button_with_status(entry, "shared", dot, cx).into_any_element()) }
            else { dot.map(|dot| crate::sidebar::row_action_slot().child(dot).into_any_element()) };
        div().id(SharedString::from(format!("shared-sidebar-{id}"))).h(px(crate::sidebar::ROW_H)).w_full().pl(px(6.)).pr(px(8.))
            .flex().items_center().gap(px(0.)).rounded(px(crate::sidebar::ROW_R)).cursor_pointer()
            .text_size(px(crate::theme::ui_font())).text_color(crate::sidebar::text_color()).group("doc-row")
            .when(batch, |d| d.child(crate::sidebar::batch_check_slot().mr(px(4.)).when(!member, |slot|
                slot.child(crate::sidebar::checkbox(if checked { crate::sidebar::Check::On } else { crate::sidebar::Check::Off }, blocked, "doc-row")))))
            .when(batch && !member && checked, |d| d.bg(crate::sidebar::selection_fill()))
            .child(crate::sidebar::entry_icon(entry, false, false, cx))
            .child(self.sidebar_title(entry, SidebarSection::Shared, selected, false, cx))
            .children(end)
            .when(!member, |d| d.on_mouse_down(MouseButton::Right, cx.listener({ let id = id.clone(); move |w, event: &MouseDownEvent, _, cx| {
                if w.batch.is_some() && !w.batch.as_ref().is_some_and(|ids| ids.contains(&id)) { w.toggle_check(&id); }
                w.row_menu = Some(crate::app::RowMenu { id: format!("shared:{id}"), pos: event.position, corner: Anchor::TopLeft, sub: false, batch: w.batch.is_some() });
                cx.stop_propagation(); cx.notify();
            }})))
            .on_mouse_down(MouseButton::Left, cx.listener(move |w, event: &MouseDownEvent, _, cx| {
                if !member && (w.shared_batch() || event.modifiers.shift) {
                    if !w.shared_batch() { w.batch = Some(vec![]); w.sharing.batch = true; if w.view == View::Home { w.home.tab = crate::home::HomeTab::Shared; } }
                    w.toggle_check(&id); cx.notify();
                } else if !w.shared_batch() { w.with_sidebar(true, |w| w.arm_folder_drag(&id, event.position, cx)); }
                cx.stop_propagation();
            }))
    }

    pub(crate) fn reorder_shared_sidebar(&mut self, id: &str, parent: Option<&str>, before: Option<&str>, cx: &mut Context<Self>) {
        let Some(rows) = self.folder_drag.shared_rows.as_ref() else { return; };
        let siblings = crate::tree::Tree::new(rows);
        let ids = siblings.children(parent).iter().filter(|entry| entry.meta.share_root.is_none()).map(|entry| entry.id.as_str()).collect();
        let mut values = preferences::active(cx).clone();
        values.move_item(ids, id, before);
        let result = (|| {
            let mut tree = organizer::Organizer::load()?;
            tree.place(id, parent)?;
            Store::write_sqlite_settings(&[
                (organizer::KEY, &serde_json::to_vec(&tree)?),
                (preferences::KEY, &serde_json::to_vec(&values)?),
            ])?;
            Ok::<_, anyhow::Error>(tree)
        })();
        match result {
            Ok(tree) => {
                if let Some(parent) = parent { self.expanded.insert(parent.to_owned()); }
                cx.set_global(tree); preferences::set(values, cx);
                notify(cx); cloud_sync::retry(&Store::root_dir(), cx); cx.notify();
            }
            Err(error) => self.show_toast_kind(ui::ToastKind::Error, &settings::diagnostic(&error.to_string()), cx),
        }
    }
}
