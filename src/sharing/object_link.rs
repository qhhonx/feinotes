// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.

use super::*;
use crate::cloud_sync::object_link::{self, Kind, Link};
use crate::shared_store::SharedStore;
use std::time::{Duration, Instant};
use uuid::Uuid;

impl Workspace {
    pub(crate) fn object_link_available(&self, cx: &App) -> bool {
        // Keep Production copy actions hidden until the HTTPS landing page is available.
        env!("FEI_CLOUD_NAMESPACE").starts_with("Development:") && !crate::cloud_restore::local_only()
            && cx.try_global::<SharedStore>().is_some_and(|store| object_link::identity(store.root(), cx).is_some())
    }

    pub(crate) fn copy_object_link(&mut self, id: &str, room: Option<Room>, personal: bool, cx: &mut Context<Self>) {
        if !self.object_link_available(cx) { return; }
        self.row_menu = None;
        self.doc_menu = false;
        let result = (|| {
            let mut store = cx.try_global::<SharedStore>()?.clone();
            let (account, channel) = object_link::identity(store.root(), cx)?;
            if shared::room_account(store.root(), cx) != shared::RoomAccount::Ready { return None; }
            let object = Uuid::parse_str(id).ok()?;
            let room = room.or_else(|| if personal {
                shared::original::owner_room(&store.list(), id)
            } else {
                self.current_room(cx)
            });
            let (kind, scope) = if let Some(room) = room {
                if self.current_room(cx).is_some_and(|current| current.scope == room.scope)
                    && self.sharing_access(cx) == Some(Access::Revoked) { return None; }
                (Kind::Shared, object_link::room_scope(&account, &room)?)
            } else {
                if !personal { return None; }
                if !store.list().iter().any(|entry| entry.id == id && !entry.meta.local()) { return None; }
                (Kind::Personal, object_link::personal_scope(&account, channel.as_deref()))
            };
            Some(Link { kind, scope, object }.url())
        })();
        if let Some(url) = result {
            if !crate::devtools::scripted() { cx.write_to_clipboard(ClipboardItem::new_string(url)); }
            self.show_toast(tr("已复制链接", "Link copied"), cx);
        } else {
            self.show_toast_kind(ui::ToastKind::Info, tr("暂时无法复制链接，请确认 iCloud 账户和访问权限。",
                "Could not copy this link. Check your iCloud account and access."), cx);
        }
    }

    fn object_link_unavailable(&mut self, cx: &mut Context<Self>) {
        self.show_toast_kind(ui::ToastKind::Info, tr("无法打开此链接。请确认 iCloud 账户、同步状态和访问权限。",
            "Could not open this link. Check your iCloud account, sync status and access."), cx);
    }

    pub(super) fn receive_object_link(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(link) = Link::parse(url) else { self.object_link_unavailable(cx); return; };
        let Some(mut store) = cx.try_global::<SharedStore>().cloned() else { self.object_link_unavailable(cx); return; };
        let personal = store.root().to_path_buf();
        let route = (self.view, self.current.clone(), self.folder_current.clone());
        let request = Uuid::new_v4();
        self.sharing.account_open = Some(request);
        let revision = cloud_sync::account_revision();
        let id = link.object.to_string();
        cx.spawn_in(window, async move |this, cx| {
            let started = Instant::now();
            let current = |w: &Workspace, window: &Window, cx: &Context<Workspace>| {
                w.sharing.account_open == Some(request) && !w.sharing.busy && !w.library_switching
                    && cloud_sync::account_revision() == revision
                    && route == (w.view, w.current.clone(), w.folder_current.clone())
                    && cx.try_global::<SharedStore>().is_some_and(|store| store.root() == personal)
                    && window.root::<Workspace>().flatten().is_some_and(|root| root.entity_id() == cx.entity_id())
            };
            let account = loop {
                let observed = this.update_in(cx, |w, window, cx| current(w, window, cx).then(||
                    (shared::room_account(&personal,cx), object_link::identity(&personal,cx)))).ok().flatten();
                match observed {
                    Some((shared::RoomAccount::Ready, Some((account, channel)))) => {
                        if link.kind == Kind::Shared { break Some(account); }
                        if object_link::personal_scope(&account, channel.as_deref()) != link.scope { break None; }
                        if store.list().iter().any(|entry| entry.id == id && !entry.meta.local()) { break Some(account); }
                    }
                    Some((shared::RoomAccount::Verifying, _)) => {}
                    Some(_) => break None,
                    None => return,
                }
                if started.elapsed() >= Duration::from_secs(15) { break None; }
                cx.background_executor().timer(Duration::from_millis(250)).await;
            };
            let resolved = if link.kind == Kind::Shared {
                if let Some(account) = &account {
                    let account = account.clone(); let source = personal.clone();
                    cx.background_executor().spawn(async move { object_link::find_room(&source, &account, link) }).await.ok().flatten()
                } else { None }
            } else { None };
            let _ = this.update_in(cx, |w, window, cx| {
                if !current(w, window, cx) { return; }
                w.sharing.account_open = None;
                if account.is_none() || shared::room_account(&personal,cx) != shared::RoomAccount::Ready {
                    w.object_link_unavailable(cx);
                    return;
                }
                if let Some((room, access)) = resolved {
                    w.open_verified_shared_item(room, id, access, window, cx);
                } else if link.kind == Kind::Personal {
                    w.personal_action(window, cx, move |w, window, cx| {
                        if cloud_sync::account_revision() != revision { return; }
                        match w.entries.iter().find(|entry| entry.id == id && !entry.meta.local()).map(|entry| entry.meta.folder.is_some()) {
                            Some(true) => { w.open_folder(&id, window, cx); }
                            Some(false) => w.open(&id, window, cx),
                            None => w.object_link_unavailable(cx),
                        }
                    });
                } else {
                    w.object_link_unavailable(cx);
                }
            });
        }).detach();
    }
}
