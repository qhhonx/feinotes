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
        cx.try_global::<ActivityView>().into_iter().flat_map(|view| &view.0).map(|row| {
            let id = row.room.scope.note.clone();
            let parent = tree.parent(&id);
            let mut entry = Entry { id, title: row.room.title.clone(), icon: row.icon.clone(), body: row.body.clone(),
                meta: crate::store::Meta { parent, folder: row.room.scope.folder.then(|| row.room.title.clone()),
                    created: row.created, modified: row.modified, opened: row.opened, ..Default::default() },
                reminders: vec![], links: vec![] };
            prefs.project(&mut entry);
            entry
        }).collect()
    }

    pub(crate) fn shared_shortcut_row(&self, entry: &Entry, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = entry.id.clone();
        div().id(SharedString::from(format!("shared-pinned-{id}"))).group("doc-row").h(px(crate::sidebar::ROW_H)).mb(px(crate::sidebar::ROW_GAP))
            .pl(px(6.)).flex().items_center().gap(px(9.)).rounded(px(crate::sidebar::ROW_R)).cursor_pointer()
            .text_size(px(crate::theme::ui_font())).text_color(crate::sidebar::text_color()).hover(|s| s.bg(crate::sidebar::hover()))
            .when(self.sidebar_row_selected(SidebarSection::Pinned, &id), |d| d.bg(crate::sidebar::selection_fill()))
            .child(crate::sidebar::entry_icon(entry, self.viewing_shared_folder(&id, cx), cx)).child(div().flex_1().min_w_0().truncate().child(display_title(&entry.title)))
            .child(self.more_button(entry, "shared-pinned", cx))
            .on_mouse_down(MouseButton::Right, cx.listener({ let id = id.clone(); move |w, event: &MouseDownEvent, _, cx| {
                w.row_menu = Some(crate::app::RowMenu { id: format!("shared:{id}"), pos: event.position, corner: Corner::TopLeft, sub: false, batch: false }); cx.notify();
            }}))
            .on_click(cx.listener(move |w, _, window, cx| { if let Some(room) = w.shared_list_room(&id, cx) {
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

    pub(crate) fn render_shared_sidebar_rows(&mut self, entries: Vec<Entry>, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.sharing.sidebar.shared_rows = Some(entries);
        self.with_sidebar(true, |w| w.render_folder_rows(window, cx))
    }

    pub(crate) fn shared_sidebar_row(&self, entry: &Entry, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = entry.id.clone();
        if entry.meta.folder.is_some() {
            let has_children = self.folder_drag.has_children(&id);
            return div().id(SharedString::from(format!("private-collection-{id}"))).h(px(crate::sidebar::ROW_H)).w_full().group("doc-row")
                .flex().items_center().gap(px(0.)).pl(px(6.)).pr(px(6.)).rounded(px(crate::sidebar::ROW_R)).cursor_pointer()
                .hover(|s| s.bg(crate::sidebar::hover()))
                .child(div().size(px(20.)).flex_none().relative()
                    .child(crate::sidebar::entry_icon(entry, self.expanded.contains(&id), cx)
                        .when(has_children, |d| d.group_hover("doc-row", |s| s.opacity(0.))))
                    .when(has_children, |d| d.child(div().absolute().inset_0().flex().items_center().justify_center()
                        .opacity(0.).group_hover("doc-row", |s| s.opacity(1.))
                        .child(ui::icon(if self.expanded.contains(&id) { "chevron-down" } else { "chevron-right" }, 12., Theme::icon())))
                        .on_mouse_down(MouseButton::Left, cx.listener(|w, ev: &MouseDownEvent, _, _| {
                            w.sharing.sidebar.icon_press = true; w.sharing.sidebar.icon_alt = ev.modifiers.alt;
                        }))))
                .child(self.sidebar_title(entry, self.sharing.collection.as_ref() == Some(&id), false))
                .child(self.more_button(entry, "collection", cx))
                .on_mouse_down(MouseButton::Right, cx.listener({ let id = id.clone(); move |w, event: &MouseDownEvent, _, cx| {
                    w.row_menu = Some(crate::app::RowMenu { id: id.clone(), pos: event.position, corner: Corner::TopLeft, sub: false, batch: false });
                    cx.stop_propagation(); cx.notify();
                }}))
                .on_mouse_down(MouseButton::Left, cx.listener(move |w, event: &MouseDownEvent, _, cx| {
                    w.with_sidebar(true, |w| w.arm_folder_drag(&id, event.position, cx)); cx.stop_propagation();
                }));
        }
        let selected = self.sidebar_row_selected(SidebarSection::Shared, &id);
        let unread = cx.try_global::<ActivityView>().is_some_and(|view| view.0.iter().any(|row| row.room.scope.note == id && row.unread));
        let dot = unread.then(crate::sidebar::shared_unread_dot);
        let more = !self.shared_batch() && !self.rename.as_ref().is_some_and(|rename| rename.id == entry.id);
        let end = if more { Some(self.more_button_with_status(entry, "shared", dot, cx).into_any_element()) }
            else { dot.map(|dot| crate::sidebar::row_action_slot().child(dot).into_any_element()) };
        div().id(SharedString::from(format!("shared-sidebar-{id}"))).h(px(crate::sidebar::ROW_H)).w_full().pl(px(6.)).pr(px(8.))
            .flex().items_center().gap(px(0.)).rounded(px(crate::sidebar::ROW_R)).cursor_pointer()
            .text_size(px(crate::theme::ui_font())).text_color(crate::sidebar::text_color()).group("doc-row")
            .when(self.shared_batch(), |d| d.child(crate::sidebar::checkbox(if self.batch.as_ref().is_some_and(|ids| ids.contains(&id)) { crate::sidebar::Check::On } else { crate::sidebar::Check::Off }, false, "doc-row").mr(px(4.))))
            .when(self.shared_batch() && self.batch.as_ref().is_some_and(|ids| ids.contains(&id)), |d| d.bg(crate::sidebar::selection_fill()))
            .child(crate::sidebar::entry_icon(entry, self.viewing_shared_folder(&id, cx), cx))
            .child(self.sidebar_title(entry, selected, false))
            .children(end)
            .on_mouse_down(MouseButton::Right, cx.listener({ let id = id.clone(); move |w, event: &MouseDownEvent, _, cx| {
                if w.batch.is_some() && !w.batch.as_ref().is_some_and(|ids| ids.contains(&id)) { w.toggle_check(&id); }
                w.row_menu = Some(crate::app::RowMenu { id: format!("shared:{id}"), pos: event.position, corner: Corner::TopLeft, sub: false, batch: w.batch.is_some() });
                cx.stop_propagation(); cx.notify();
            }}))
            .on_mouse_down(MouseButton::Left, cx.listener(move |w, event: &MouseDownEvent, _, cx| {
                if w.shared_batch() || event.modifiers.shift {
                    if !w.shared_batch() { w.batch = Some(vec![]); w.sharing.batch = true; if w.view == View::Home { w.home.tab = crate::home::HomeTab::Shared; } }
                    w.toggle_check(&id); cx.notify();
                } else { w.with_sidebar(true, |w| w.arm_folder_drag(&id, event.position, cx)); }
                cx.stop_propagation();
            }))
    }

    pub(crate) fn reorder_shared_sidebar(&mut self, id: &str, parent: Option<&str>, before: Option<&str>, cx: &mut Context<Self>) {
        let Some(rows) = self.folder_drag.shared_rows.as_ref() else { return; };
        let siblings = crate::tree::Tree::new(rows);
        let ids = siblings.children(parent).iter().map(|entry| entry.id.as_str()).collect();
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
