//! User-confirmed removal retains a recovery copy before changing remote access.
use super::*;

impl Workspace {
    pub(crate) fn open_end_shared(&mut self, room: Room, window: &mut Window, cx: &mut Context<Self>) {
        self.sharing_sheet(Sheet::End(room), window, cx);
    }

    pub(super) fn end_shared_document(&mut self, room: Room, window: &mut Window, cx: &mut Context<Self>) {
        self.end_shared_documents(vec![room], vec![], window, cx);
    }

    pub(super) fn end_shared_documents(&mut self, rooms: Vec<Room>, personal_ids: Vec<String>, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy || rooms.is_empty() || !self.sharing_account_ready(cx) { return; }
        let retry = rooms.clone(); let retry_personal = personal_ids.clone();
        if !self.save_before(cx, move |w, window, cx| w.end_shared_documents(retry, retry_personal, window, cx)) { return; }
        let bulk = matches!(self.sharing.sheet, Some(Sheet::EndMany(..)));
        let epoch = self.begin_sharing_request(cx);
        let personal = crate::store::Store::root_dir();
        let rooms: Vec<_> = rooms.into_iter().map(|room| {
            let lock = shared::prepare_end(&personal, &room, cx); (room, lock)
        }).collect();
        cx.spawn_in(window, async move |this, cx| {
            let root = personal.clone();
            let results = cx.background_executor().spawn(async move {
                rooms.into_iter().map(|(room, lock)| { let result = lock.and_then(|lock| shared::end(&root, &room, lock)); (room, result) }).collect::<Vec<_>>()
            }).await;
            let _ = this.update_in(cx, |w, window, cx| {
                if !w.finish_sharing_request(epoch, None, cx) { return; }
                let mut failed = vec![]; let mut failed_rooms = vec![]; let mut errors = vec![]; let mut removed_current = false;
                for (room, result) in results {
                    match result {
                        Ok(()) => {
                            removed_current |= shared::room_root(&personal, &room.scope.note).is_ok_and(|root| w.store.root() == root);
                            if let Ok(root) = shared::room_root(&personal, &room.scope.note)
                                && let Err(error) = cloud_sync::shared_access_ended(&root, &room.scope.note, cx) {
                                crate::logging::write(&format!("Shared removal local access state: {error:#}"));
                            }
                        }
                        Err(error) => { let _ = shared::store_for(&room, cx); failed.push(room.scope.note.clone()); failed_rooms.push(room); errors.push(settings::diagnostic(&error.to_string())); }
                    }
                }
                if !errors.is_empty() {
                    if removed_current {
                        let message = errors.join("；"); w.close_sharing(window, cx);
                        w.personal_action(window, cx, move |w, window, cx| {
                            w.show_view(View::Shared, cx); w.batch = Some(failed); w.sharing.batch = true;
                            w.sharing_sheet(Sheet::EndMany(failed_rooms, personal_ids), window, cx); w.sharing.message = Some((true, message)); cx.notify();
                        });
                    } else {
                        if bulk { w.batch = Some(failed); w.sharing.batch = true; w.sharing.sheet = Some(Sheet::EndMany(failed_rooms, personal_ids)); }
                        w.sharing.message = Some((true, errors.join("；"))); cx.notify();
                    }
                    return;
                }
                w.batch = None; w.close_sharing(window, cx); cloud_sync::retry(&personal, cx);
                if personal_ids.is_empty() { w.navigate_personal(View::Shared, None, window, cx); }
                else { w.personal_action(window, cx, move |w, window, cx| w.delete_docs(&personal_ids, window, cx)); }
            });
        }).detach();
    }
}
