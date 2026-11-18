//! Shared navigation uses the personal sidebar's drag and animation engine.
use super::*;
use crate::{cloud_sync::activity::ActivityView, store::{Entry, Store}};

#[derive(Default)]
struct SharedOrder(Vec<String>);
impl Global for SharedOrder {}

fn order(cx: &mut App) -> &SharedOrder {
    if !cx.has_global::<SharedOrder>() {
        let saved = if Store::sqlite_session() { Store::sqlite_setting("shared-sidebar-order").ok().flatten()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default() } else { vec![] };
        cx.set_global(SharedOrder(saved));
    }
    cx.global::<SharedOrder>()
}

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
struct Organization { pinned: std::collections::HashSet<String>, starred: std::collections::HashMap<String, u64> }
impl Global for Organization {}

pub(super) fn install(cx: &mut App) {
    let _ = order(cx);
    if !cx.has_global::<Organization>() {
        let saved = if Store::sqlite_session() { Store::sqlite_setting("shared-organization").ok().flatten().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default() } else { Organization::default() };
        cx.set_global(saved);
    }
}

pub(super) fn notify(cx: &mut App) {
    cx.defer(|cx| {
        for handle in cx.windows().into_iter().filter_map(|handle| handle.downcast::<Workspace>()) {
            let _ = handle.update(cx, |_, _, cx| cx.notify());
        }
    });
}

fn move_before(ids: &mut Vec<String>, id: &str, before: Option<&str>) {
    if !ids.iter().any(|item| item == id) || before == Some(id) { return; }
    ids.retain(|item| item != id);
    let index = before.and_then(|before| ids.iter().position(|item| item == before)).unwrap_or(ids.len());
    ids.insert(index, id.to_string());
}

impl Workspace {
    fn viewing_shared_folder(&self, id: &str, cx: &App) -> bool {
        matches!(self.view, View::Folder | View::Doc)
            && shared::scope(&self.store, cx).is_some_and(|scope| scope.folder && scope.note == id)
    }

    pub(crate) fn shared_entries(&self, cx: &App) -> Vec<Entry> {
        let organization = cx.global::<Organization>(); let order = &cx.global::<SharedOrder>().0;
        cx.try_global::<ActivityView>().into_iter().flat_map(|view| &view.0).map(|row| Entry {
            id: row.room.scope.note.clone(), title: row.room.title.clone(), icon: row.icon.clone(), body: row.body.clone(),
            meta: crate::store::Meta { folder: row.room.scope.folder.then(|| row.room.title.clone()), created: row.created, modified: row.modified, opened: row.opened,
                pinned: organization.pinned.contains(&row.room.scope.note), starred: organization.starred.get(&row.room.scope.note).copied().unwrap_or(0),
                root_order: order.iter().position(|id| id == &row.room.scope.note).map(|i| i as u64 + 1), ..Default::default() }, reminders: vec![], links: vec![],
        }).collect()
    }

    pub(crate) fn toggle_shared_organization(&mut self, id: &str, pin: bool, cx: &mut Context<Self>) {
        let mut organization = cx.global::<Organization>().clone();
        if pin { if !organization.pinned.remove(id) { organization.pinned.insert(id.to_string()); } }
        else if organization.starred.remove(id).is_none() { organization.starred.insert(id.to_string(), crate::time::now_ms()); }
        match serde_json::to_vec(&organization).map_err(anyhow::Error::from).and_then(|bytes| Store::write_sqlite_setting("shared-organization", &bytes)) {
            Ok(()) => { cx.set_global(organization); notify(cx); self.row_menu = None; cx.notify(); }
            Err(error) => self.show_toast_kind(ui::ToastKind::Error, &settings::diagnostic(&error.to_string()), cx),
        }
    }

    pub(crate) fn shared_shortcut_row(&self, entry: &Entry, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = entry.id.clone();
        div().id(SharedString::from(format!("shared-pinned-{id}"))).group("doc-row").h(px(crate::sidebar::ROW_H)).mb(px(crate::sidebar::ROW_GAP))
            .pl(px(6.)).flex().items_center().gap(px(9.)).rounded(px(crate::sidebar::ROW_R)).cursor_pointer()
            .text_size(px(crate::theme::ui_font())).text_color(crate::sidebar::text_color()).hover(|s| s.bg(crate::sidebar::hover()))
            .when(self.is_shared() && self.current.as_ref() == Some(&id), |d| d.bg(crate::sidebar::selection_fill()))
            .child(crate::sidebar::entry_icon(entry, self.viewing_shared_folder(&id, cx), cx)).child(div().flex_1().min_w_0().truncate().child(display_title(&entry.title)))
            .child(self.more_button(entry, "shared-pinned", cx))
            .on_mouse_down(MouseButton::Right, cx.listener({ let id = id.clone(); move |w, event: &MouseDownEvent, _, cx| {
                w.row_menu = Some(crate::app::RowMenu { id: format!("shared:{id}"), pos: event.position, corner: Corner::TopLeft, sub: false, batch: false }); cx.notify();
            }}))
            .on_click(cx.listener(move |w, _, window, cx| { if let Some(room) = w.shared_list_room(&id, cx) { w.open_shared_note(&room, window, cx); } }))
    }

    pub(crate) fn organized_shared_entries(&self, cx: &App) -> Vec<Entry> {
        let mut rows = self.shared_entries(cx);
        let tree = cx.global::<organizer::Organizer>();
        for row in &mut rows { row.meta.parent = tree.0.get(&row.id).and_then(|p| p.parent.clone()); }
        rows.extend(tree.folders());
        rows
    }

    pub(crate) fn render_shared_sidebar_rows(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = self.organized_shared_entries(cx);
        let order = &order(cx).0;
        rows.sort_by_key(|row| order.iter().position(|id| id == &row.id));
        self.sharing.sidebar.shared_rows = Some(rows);
        self.with_sidebar(true, |w| w.render_folder_rows(window, cx))
    }

    pub(crate) fn shared_sidebar_row(&self, entry: &Entry, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = entry.id.clone();
        if entry.meta.folder.is_some() {
            let child = id.clone();
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
                .child(div().id(SharedString::from(format!("sub-{id}"))).size(px(22.)).p(px(3.)).rounded(px(6.))
                    .opacity(0.).group_hover("doc-row", |s| s.opacity(1.)).hover(|s| s.bg(crate::sidebar::selection_fill())).child(ui::icon("folder-plus", 16., Theme::icon()))
                    .tip(tr("新建个人子文件夹", "New personal subfolder"), None)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |w, _, window, cx| { cx.stop_propagation(); w.personal_collection(Some(child.clone()), window, cx); })))
                .on_mouse_down(MouseButton::Right, cx.listener({ let id = id.clone(); move |w, event: &MouseDownEvent, _, cx| {
                    w.row_menu = Some(crate::app::RowMenu { id: id.clone(), pos: event.position, corner: Corner::TopLeft, sub: false, batch: false });
                    cx.stop_propagation(); cx.notify();
                }}))
                .on_mouse_down(MouseButton::Left, cx.listener(move |w, event: &MouseDownEvent, _, cx| {
                    w.with_sidebar(true, |w| w.arm_folder_drag(&id, event.position, cx)); cx.stop_propagation();
                }));
        }
        let selected = self.is_shared() && (self.current.as_ref() == Some(&id) || self.folder_current.as_ref() == Some(&id));
        let unread = cx.try_global::<ActivityView>().is_some_and(|view| view.0.iter().any(|row| row.room.scope.note == id && row.unread));
        div().id(SharedString::from(format!("shared-sidebar-{id}"))).h(px(crate::sidebar::ROW_H)).w_full().pl(px(6.)).pr(px(8.))
            .flex().items_center().gap(px(0.)).rounded(px(crate::sidebar::ROW_R)).cursor_pointer()
            .text_size(px(crate::theme::ui_font())).text_color(crate::sidebar::text_color()).group("doc-row")
            .when(self.shared_batch(), |d| d.child(crate::sidebar::checkbox(if self.batch.as_ref().is_some_and(|ids| ids.contains(&id)) { crate::sidebar::Check::On } else { crate::sidebar::Check::Off }, false, "doc-row").mr(px(4.))))
            .when(self.shared_batch() && self.batch.as_ref().is_some_and(|ids| ids.contains(&id)), |d| d.bg(crate::sidebar::selection_fill()))
            .child(crate::sidebar::entry_icon(entry, self.viewing_shared_folder(&id, cx), cx))
            .child(self.sidebar_title(entry, selected, false))
            .when(unread, |d| d.child(div().size(px(7.)).rounded_full().bg(Theme::primary())))
            .when(!self.shared_batch() && !self.rename.as_ref().is_some_and(|rename| rename.id == entry.id), |d| d.child(self.more_button(entry, "shared", cx)))
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
        if !self.update_organizer(cx, |tree| tree.place(id, parent)) { return; }
        if let Some(parent) = parent { self.expanded.insert(parent.into()); }
        let Some(rows) = self.folder_drag.shared_rows.as_ref() else { return };
        let mut ids = rows.iter().map(|entry| entry.id.clone()).collect();
        move_before(&mut ids, id, before);
        match serde_json::to_vec(&ids).map_err(anyhow::Error::from).and_then(|bytes| Store::write_sqlite_setting("shared-sidebar-order", &bytes)) {
            Ok(()) => { cx.set_global(SharedOrder(ids)); notify(cx); cx.notify(); }
            Err(error) => self.show_toast_kind(ui::ToastKind::Error, &settings::diagnostic(&error.to_string()), cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::move_before;
    #[test]
    fn shared_reorder_preserves_ids_and_cancels_unknown_items() {
        let mut ids = vec!["a".into(), "b".into(), "c".into()];
        move_before(&mut ids, "c", Some("a")); assert_eq!(ids, ["c", "a", "b"]);
        move_before(&mut ids, "c", None); assert_eq!(ids, ["a", "b", "c"]);
        move_before(&mut ids, "missing", None); move_before(&mut ids, "b", Some("b")); assert_eq!(ids, ["a", "b", "c"]);
    }
}
