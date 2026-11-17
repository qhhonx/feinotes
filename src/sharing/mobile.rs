//! UIKit presents the product sharing state; CloudKit requests stay in sharing.rs.
use super::*;

impl Workspace {
    pub(crate) fn mobile_present_sharing(&mut self, cx: &mut Context<Self>) {
        if self.view != View::Settings { self.mobile_settings_return = self.view.clone(); }
        self.mobile_settings_root = false;
        self.open_settings(settings::Page::Shared,cx);
    }
    pub(crate) fn mobile_shared_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(room) = self.current_room(cx) {
            self.sharing.sheet = Some(Sheet::Members(room.clone()));
            self.sharing.reply = None;
            if self.sharing_access(cx) == Some(Access::Owner) { self.manage_shared_note("members",String::new(),None,window,cx); }
        }
        cx.notify();
    }
}

impl Workspace {
    pub(crate) fn mobile_shared_listing(&self, cx: &App) -> Vec<crate::store::Entry> {
        let tree = cx.global::<organizer::Organizer>();
        let parent = self.sharing.collection.as_deref().filter(|id| tree.0.contains_key(*id));
        let belongs = |id: &str| tree.0.get(id).and_then(|p| p.parent.as_deref()).filter(|id| tree.0.contains_key(*id)) == parent;
        self.shared_entries(cx).into_iter().filter(|e| belongs(&e.id)).chain(tree.folders().filter(|e| belongs(&e.id))).collect()
    }
    pub(crate) fn mobile_collection(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        if !cx.global::<organizer::Organizer>().0.get(id).is_some_and(|p| p.title.is_some()) { return false; }
        self.sharing.collection = Some(id.into()); self.home.tab = crate::home::HomeTab::Shared;
        self.show_view(View::Home, cx); true
    }
    pub(crate) fn mobile_collection_up(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(id) = self.sharing.collection.take() else { return false; };
        self.sharing.collection = cx.global::<organizer::Organizer>().0.get(&id).and_then(|p|p.parent.clone());
        cx.notify(); true
    }
    pub(crate) fn mobile_collection_entries(&self, cx: &App) -> Vec<crate::store::Entry> { cx.global::<organizer::Organizer>().folders().collect() }
    pub(crate) fn mobile_collection_path(&self, cx: &App) -> Vec<(String,String)> {
        let tree=cx.global::<organizer::Organizer>(); let mut current=self.sharing.collection.clone(); let mut out=vec![];
        while let Some(id)=current { if out.iter().any(|(seen,_)|seen==&id) { break; } let Some(row)=tree.0.get(&id) else {break;};
            out.push((id,row.title.clone().unwrap_or_default())); current=row.parent.clone(); }
        out.reverse();out
    }
    pub(crate) fn mobile_move_shortcut(&mut self, id: &str, parent: Option<String>, cx: &mut Context<Self>) -> bool {
        if self.shared_list_room(id,cx).is_none() && !cx.global::<organizer::Organizer>().0.contains_key(id) { return false; }
        if !self.is_shared() { self.update_organizer(cx,|tree|tree.place(id,parent.as_deref())); }
        true
    }

}
