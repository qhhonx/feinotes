//! Replace only a window's workspace when navigating between isolated libraries.
use super::{*, actions::SharedAction};
use crate::sidebar::SidebarEntry;

#[derive(Clone)]
struct SidebarNavigation {
    entry: Option<SidebarEntry>,
    prepared: Option<crate::shared_store::SharedStore>,
    verified_access: Option<shared::VerifiedRoomAccess>,
    new_invitation: Option<Reply>,
    published_invitation: Option<String>,
    action: Option<SharedAction>,
    #[cfg(target_os = "macos")]
    created: Option<SidebarCreatedItem>,
    #[cfg(target_os = "macos")]
    folder_edit: Option<SharedFolderEdit>,
}

#[derive(Clone)]
struct OpenSource {
    started: std::time::Instant,
    entry: Option<SidebarEntry>,
    verified_access: Option<shared::VerifiedRoomAccess>,
    action: Option<SharedAction>,
}

impl OpenSource {
    fn new(entry: Option<SidebarEntry>, action: Option<SharedAction>) -> Self {
        Self { started: std::time::Instant::now(), entry, verified_access: None, action }
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
pub(crate) enum SharedFolderEdit { Rename, Move }

#[cfg(target_os = "macos")]
#[derive(Clone)]
struct SidebarCreatedItem {
    kind: crate::folders::SharedFolderCreation,
    changes: Vec<crate::store::FileDelta>,
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

pub(super) fn shared_account_changed() -> &'static str {
    tr("iCloud 账户状态已变化。请重试。", "Your iCloud account status changed. Try again.")
}

fn shared_item_ready(store: &crate::shared_store::SharedStore, target: &str, cx: &App) -> bool {
    matches!(shared::access(store, cx), Some(Access::Owner | Access::ReadWrite | Access::ReadOnly))
        && !shared::editing_paused(store, cx)
        && shared::has_materialized_item(store, target, cx)
}

impl Workspace {
    #[cfg(target_os = "ios")]
    pub(crate) fn mobile_personal_browser(&self, cx: &App) -> bool {
        self.sharing.personal_route.is_some() && shared::scope(&self.store,cx).is_some_and(|scope| !scope.folder)
    }

    #[cfg(target_os = "ios")]
    pub(crate) fn mobile_shared_list_context(&self,cx: &App) -> bool {
        if let Some(scope) = shared::scope(&self.store,cx).filter(|scope|scope.folder) {
            return scope.owner.is_some() || self.sharing.personal_route.as_ref().is_none_or(|route|route.home.shared());
        }
        self.home.shared()
    }

    #[cfg(target_os = "ios")]
    pub(crate) fn mobile_folder_share_mark(&self,entry: &crate::store::Entry,cx: &App) -> bool {
        entry.meta.folder.is_some() && !self.mobile_shared_list_context(cx) && (
            entry.meta.share_root.is_some()
            || self.current_room(cx).is_some_and(|room|room.scope.folder && room.scope.owner.is_none()) && !self.mobile_personal_browser(cx)
        )
    }

    #[cfg(target_os = "ios")]
    pub(crate) fn mobile_return_from_share(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if crate::in_shared_note() { return crate::ipad_session::return_to_personal(cx); }
        let view = if self.sharing.personal_route.as_ref().is_some_and(|route| route.folder.is_some() && !route.home.shared()) {
            View::Folder
        } else { View::Home };
        self.navigate_personal(view, None, window, cx);
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
        if invitation.is_none() {
            self.open_shared_item_interactive(room, target, None, window, cx);
        } else {
            self.cancel_shared_open();
            self.navigate_shared_item_using(room, target, invitation, None, window, cx);
        }
    }

    pub(super) fn navigate_new_shared_item(&mut self, room: Room, reply: Reply, window: &mut Window, cx: &mut Context<Self>) {
        let target = room.scope.note.clone();
        let sidebar = SidebarNavigation {
            entry: Some(SidebarEntry::new(crate::sidebar::SidebarSection::Shared, &target)),
            prepared: None,
            verified_access: None,
            new_invitation: Some(reply), published_invitation: None, action: None,
            #[cfg(target_os = "macos")]
            created: None,
            #[cfg(target_os = "macos")]
            folder_edit: None,
        };
        self.navigate_shared_item_using(room, target, None, Some(sidebar), window, cx);
    }

    pub(super) fn navigate_published_invitation(&mut self, room: Room, text: String, window: &mut Window, cx: &mut Context<Self>) {
        let target = room.scope.note.clone();
        let sidebar = SidebarNavigation {
            entry: None, prepared: None, verified_access: None, new_invitation: None, published_invitation: Some(text), action: None,
            #[cfg(target_os = "macos")]
            created: None,
            #[cfg(target_os = "macos")]
            folder_edit: None,
        };
        self.navigate_shared_item_using(room, target, None, Some(sidebar), window, cx);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn shared_sidebar_section(&self) -> crate::sidebar::SidebarSection {
        use crate::sidebar::SidebarSection;
        if self.sidebar_entry.as_ref().is_some_and(|entry| entry.section == SidebarSection::Personal) {
            SidebarSection::Personal
        } else { SidebarSection::Shared }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn reveal_shared_sidebar_entry(&mut self, entry: &SidebarEntry, cx: &App) {
        use crate::sidebar::SidebarSection;
        let rows = match entry.section {
            SidebarSection::Personal => self.personal_navigation_entries(cx),
            SidebarSection::Shared => self.shared_sidebar_entries(cx),
            SidebarSection::Local | SidebarSection::Pinned => return,
        };
        let mut parents = std::collections::HashSet::new();
        let mut id = Some(entry.id.as_str());
        while let Some(row) = id.and_then(|id| rows.iter().find(|row| row.id == id && (row.meta.folder.is_some() || id == entry.id))) {
            if row.meta.folder.is_some() && !parents.insert(row.id.clone()) { break; }
            id = row.meta.parent.as_deref();
        }
        match entry.section {
            SidebarSection::Personal => { self.folded[1] = false; self.expanded.extend(parents); }
            SidebarSection::Shared => { self.sharing.folded = false; self.sharing.expanded.extend(parents); }
            SidebarSection::Local | SidebarSection::Pinned => {}
        }
    }

    pub(crate) fn cancel_sidebar_open(&mut self) {
        if self.sharing.sidebar_load.take().is_some() { self.sharing.open_notice = None; }
    }

    pub(crate) fn cancel_shared_open(&mut self) {
        self.cancel_sidebar_open();
        self.sharing.account_open = None;
        self.sharing.open_notice = None;
    }

    pub(crate) fn navigate_shared_sidebar(&mut self, room: Room, entry: SidebarEntry, window: &mut Window, cx: &mut Context<Self>) {
        self.open_shared_item_interactive(room, entry.id.clone(), Some(entry), window, cx);
    }

    pub(crate) fn open_shared_item_interactive(&mut self, room: Room, target: String, entry: Option<SidebarEntry>, window: &mut Window, cx: &mut Context<Self>) {
        self.open_shared_item(room, target, OpenSource::new(entry, None), window, cx);
    }

    pub(super) fn open_verified_shared_item(&mut self, room: Room, target: String, access: shared::VerifiedRoomAccess, window: &mut Window, cx: &mut Context<Self>) {
        let source = OpenSource { verified_access: Some(access), ..OpenSource::new(None, None) };
        self.open_shared_item(room, target, source, window, cx);
    }

    #[cfg(target_os = "ios")]
    pub(super) fn open_shared_room_for_action(&mut self, room: Room, action: SharedAction, window: &mut Window, cx: &mut Context<Self>) {
        let target = room.scope.note.clone();
        self.open_shared_item_for_action(room, target, None, action, window, cx);
    }

    pub(super) fn open_shared_item_for_action(&mut self, room: Room, target: String, entry: Option<SidebarEntry>, action: SharedAction,
        window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy { return; }
        let (retry_room, retry_target, retry_entry, retry_action) = (room.clone(), target.clone(), entry.clone(), action.clone());
        if !self.save_before(cx, move |ws, window, cx| ws.open_shared_item_for_action(retry_room, retry_target, retry_entry, retry_action, window, cx)) { return; }
        self.open_shared_item(room, target, OpenSource::new(entry, Some(action)), window, cx);
    }

    fn open_shared_item(&mut self, room: Room, target: String, source: OpenSource, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_shared_open();
        self.sharing.pending_open = None;
        if self.sharing.busy { return; }
        if !self.is_shared() && shared::original::awaiting_publication(self.store.root(), &room) {
            self.create_shared_note(&room.scope.note, window, cx);
            return;
        }
        if self.defer_shared_open(&room, &target, &source, window, cx) { return; }
        if source.verified_access.is_none() && self.viewing_shared_item(&room, &target, cx) {
            #[cfg(target_os = "macos")]
            if let Some(entry) = &source.entry { self.reveal_shared_sidebar_entry(entry, cx); }
            self.sidebar_entry = source.entry;
            #[cfg(target_os = "macos")]
            self.finish_mac_shared_waiting(window, cx);
            self.run_shared_open_action(source.action, &target, window, cx);
            cx.notify();
            return;
        }
        let request = uuid::Uuid::new_v4();
        let origin = self.shared_open_origin(cx);
        let workspace = cx.entity_id();
        let handle = WindowHandle::<Self>::new(window.window_handle().window_id());
        let requested = room.clone();
        let started = source.started;
        let revision = cloud_sync::account_revision();
        let progress = if room.scope.folder && target == room.scope.note {
            tr("正在打开共享文件夹…", "Opening shared folder…")
        } else if !room.scope.folder {
            tr("正在打开共享文档…", "Opening shared document…")
        } else { tr("正在打开共享项目…", "Opening shared item…") };
        let load = shared::load_sidebar_store(room, cx, Box::new(move |result, cx| {
            let _ = handle.update(cx, |ws, window, cx| {
                if cx.entity_id() != workspace || ws.sharing.sidebar_load != Some(request) { return; }
                let had_notice = ws.sharing.open_notice.is_some();
                ws.cancel_sidebar_open();
                if ws.shared_open_origin(cx) != origin || ws.sharing.sheet.is_some() || ws.confirm.is_some() {
                    cx.notify(); return;
                }
                if cloud_sync::account_revision() != revision {
                    ws.show_toast_kind(ui::ToastKind::Error, shared_account_changed(), cx);
                    return;
                }
                match result {
                    Ok(store) => {
                        let sidebar = SidebarNavigation {
                            entry: source.entry, prepared: Some(store), verified_access: source.verified_access,
                            new_invitation: None, published_invitation: None, action: source.action,
                            #[cfg(target_os = "macos")]
                            created: None,
                            #[cfg(target_os = "macos")]
                            folder_edit: None,
                        };
                        if had_notice { ws.sharing.open_notice = Some(progress); }
                        ws.open_materialized_item(requested, target, sidebar, (progress, started), window, cx);
                    }
                    Err(error) => if !ws.defer_shared_open(&requested, &target, &source, window, cx) {
                        ws.report_shared_open_error(&error, cx);
                    }
                }
            });
        }));
        match load {
            Ok(()) => self.begin_sidebar_loading(request, progress, started, cx),
            Err(error) => self.report_shared_open_error(&error.to_string(), cx),
        }
    }

    fn begin_sidebar_loading(&mut self, request: uuid::Uuid, progress: &'static str, started: std::time::Instant, cx: &mut Context<Self>) {
        self.sharing.sidebar_load = Some(request);
        self.delay_shared_open_notice(request, progress, started, cx);
    }

    fn delay_shared_open_notice(&self, request: uuid::Uuid, progress: &'static str, started: std::time::Instant, cx: &mut Context<Self>) {
        let remaining = LOADING_NOTICE_DELAY.saturating_sub(started.elapsed());
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(remaining).await;
            let _ = this.update(cx, |ws, cx| {
                if (ws.sharing.sidebar_load == Some(request) || ws.sharing.account_open == Some(request))
                    && ws.sharing.open_notice.is_none() {
                    ws.sharing.open_notice = Some(progress); cx.notify();
                }
            });
        }).detach();
    }

    fn open_materialized_item(&mut self, room: Room, target: String, sidebar: SidebarNavigation,
        (progress, started): (&'static str, std::time::Instant), window: &mut Window, cx: &mut Context<Self>) {
        let store = sidebar.prepared.as_ref().expect("Loaded shared library").clone();
        if shared_item_ready(&store, &target, cx) {
            self.cancel_shared_open();
            self.navigate_shared_item_using(room, target, None, Some(sidebar), window, cx);
            cx.notify();
            return;
        }
        let request = uuid::Uuid::new_v4();
        let revision = cloud_sync::account_revision();
        let personal = crate::store::Store::root_dir();
        let origin = self.shared_open_origin(cx);
        self.sharing.account_open = Some(request);
        self.delay_shared_open_notice(request, progress, started, cx);
        cloud_sync::recheck_account(store.root(), cx);
        cx.spawn_in(window, async move |this, cx| {
            let start = std::time::Instant::now();
            loop {
                let elapsed = start.elapsed();
                let done = this.update_in(cx, |ws, window, cx| {
                    if ws.sharing.account_open != Some(request) { return true; }
                    if ws.shared_open_origin(cx) != origin || ws.library_switching || ws.sharing.busy || ws.confirm.is_some()
                        || ws.sharing.sheet.is_some() || crate::store::Store::root_dir() != personal
                        || !window.root::<Workspace>().flatten().is_some_and(|root| root.entity_id() == cx.entity_id()) {
                        ws.cancel_shared_open(); cx.notify(); return true;
                    }
                    use cloud_sync::SyncPhase::*;
                    let status = cloud_sync::status(store.root(), cx);
                    let phase = status.map(|s| s.phase);
                    let error = if cloud_sync::account_revision() != revision {
                        Some(shared_account_changed())
                    } else if phase == Some(Revoked) || shared::access(&store, cx) == Some(Access::Revoked) {
                        Some(tr("你已无法访问此共享项目。", "You no longer have access to this shared item."))
                    } else if shared_item_ready(&store, &target, cx) {
                        ws.cancel_shared_open(); cx.notify();
                        ws.navigate_shared_item_using(room.clone(), target.clone(), None, Some(sidebar.clone()), window, cx);
                        return true;
                    } else {
                        match phase {
                            Some(SignedOut | AccountChanged) => Some(tr("请在文档库设置中重新连接 iCloud。", "Reconnect to iCloud in Library settings.")),
                            Some(SubscriptionRequired) => Some(shared::sharing_unavailable()),
                            Some(TestFlightVerification) => Some(tr("请确认 TestFlight 安装状态后重试。", "Check the TestFlight installation and try again.")),
                            Some(Offline) if elapsed.as_secs() >= 1 =>
                                Some(tr("无法连接 iCloud。联网后再次打开。", "Could not connect to iCloud. Open it again when online.")),
                            Some(Quota) => Some(tr("iCloud 空间不足。释放空间后再次打开。", "iCloud storage is full. Free some space and open the item again.")),
                            Some(Failed) | None => Some(tr("暂时无法打开共享项目。请重试。", "Could not open the shared item. Try again.")),
                            _ if elapsed.as_secs() >= 45 || elapsed.as_secs() >= 3 && status.is_some_and(|s| s.stalled.is_some()) =>
                                Some(tr("iCloud 未能完成加载。请再次打开。", "iCloud could not finish loading. Open the item again.")),
                            _ => None,
                        }
                    };
                    if let Some(message) = error {
                        ws.cancel_shared_open();
                        ws.show_toast_kind(ui::ToastKind::Error, message, cx);
                        return true;
                    }
                    false
                }).unwrap_or(true);
                if done { break; }
                cx.background_executor().timer(std::time::Duration::from_millis(250)).await;
            }
        }).detach();
    }

    fn shared_open_origin(&self, cx: &App) -> (View, Option<String>, Option<String>, crate::home::HomeTab, Option<String>, String) {
        (self.view, self.current.clone(), self.folder_current.clone(), self.home.tab, self.sharing.collection.clone(), self.query(cx))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn edit_shared_folder(&mut self, room: Room, entry: SidebarEntry, edit: SharedFolderEdit, window: &mut Window, cx: &mut Context<Self>) {
        self.row_menu = None;
        self.cancel_shared_open();
        if self.sharing.busy || !self.shared_sidebar_folder_writable(&room, cx) { return; }
        if shared::scope(&self.store, cx) == Some(&room.scope) {
            self.finish_shared_folder_edit(entry, edit, window, cx);
            return;
        }
        let request = uuid::Uuid::new_v4();
        let workspace = cx.entity_id();
        let handle = WindowHandle::<Self>::new(window.window_handle().window_id());
        let requested = room.clone();
        match shared::load_sidebar_store(room, cx, Box::new(move |result, cx| {
            let _ = handle.update(cx, |ws, window, cx| {
                if cx.entity_id() != workspace || ws.sharing.sidebar_load != Some(request) { return; }
                ws.cancel_sidebar_open();
                match result {
                    Ok(store) if shared::access(&store, cx).is_some_and(Access::can_write)
                        && !shared::editing_paused(&store, cx) => ws.navigate_shared_item_using(requested, entry.id.clone(), None,
                            Some(SidebarNavigation { entry: Some(entry), prepared: Some(store), verified_access: None, new_invitation: None, published_invitation: None, action: None,
                                created: None, folder_edit: Some(edit) }), window, cx),
                    Ok(_) => ws.show_toast_kind(ui::ToastKind::Error, tr("此共享文件夹当前不可写入。", "You cannot write to this shared folder right now."), cx),
                    Err(error) => ws.report_shared_open_error(&error, cx),
                }
            });
        })) {
            Ok(()) => self.sharing.sidebar_load = Some(request),
            Err(error) => self.report_shared_open_error(&error.to_string(), cx),
        }
        cx.notify();
    }

    #[cfg(target_os = "macos")]
    fn finish_shared_folder_edit(&mut self, entry: SidebarEntry, edit: SharedFolderEdit, window: &mut Window, cx: &mut Context<Self>) {
        if !self.sharing_access(cx).is_some_and(Access::can_write) || shared::editing_paused(&self.store, cx)
            || !self.entries.iter().any(|row| row.id == entry.id && row.meta.folder.is_some()) { return; }
        match edit {
            SharedFolderEdit::Rename => {
                self.start_rename_in_sidebar(&entry.id, Some(entry.section), window, cx);
            }
            SharedFolderEdit::Move => self.open_move(vec![entry.id], window, cx),
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn create_in_shared_sidebar(&mut self, room: Room, parent: String,
        kind: crate::folders::SharedFolderCreation, section: crate::sidebar::SidebarSection, window: &mut Window, cx: &mut Context<Self>) {
        self.create_menu = None;
        self.row_menu = None;
        self.cancel_shared_open();
        if self.sharing.busy { return; }

        let origin = SidebarEntry::new(section, &parent);
        if shared::scope(&self.store, cx) == Some(&room.scope) {
            self.commit_shared_sidebar_creation(room, origin, kind, self.store.clone(), window, cx);
            return;
        }
        let request = uuid::Uuid::new_v4();
        let workspace = cx.entity_id();
        let handle = WindowHandle::<Self>::new(window.window_handle().window_id());
        let requested = room.clone();
        let progress = match kind {
            crate::folders::SharedFolderCreation::Document => tr("正在新建共享文档…", "Creating shared document…"),
            crate::folders::SharedFolderCreation::Folder(_) => tr("正在新建共享文件夹…", "Creating shared folder…"),
        };
        let load = shared::load_sidebar_store(room, cx, Box::new(move |result, cx| {
            let _ = handle.update(cx, |ws, window, cx| {
                if cx.entity_id() != workspace || ws.sharing.sidebar_load != Some(request) { return; }
                ws.cancel_sidebar_open();
                match result {
                    Ok(store) => ws.commit_shared_sidebar_creation(requested, origin, kind, store, window, cx),
                    Err(error) => ws.report_shared_open_error(&error, cx),
                }
            });
        }));
        match load {
            Ok(()) => self.begin_sidebar_loading(request, progress, std::time::Instant::now(), cx),
            Err(error) => self.report_shared_open_error(&error.to_string(), cx),
        }
        cx.notify();
    }

    #[cfg(target_os = "macos")]
    fn commit_shared_sidebar_creation(&mut self, room: Room, origin: SidebarEntry,
        kind: crate::folders::SharedFolderCreation, store: crate::shared_store::SharedStore, window: &mut Window, cx: &mut Context<Self>) {
        use crate::folders::SharedFolderCreation;
        if self.sharing.busy || self.library_switching { return; }
        let (retry_room, retry_origin, retry_kind, retry_store) = (room.clone(), origin.clone(), kind.clone(), store.clone());
        if !self.save_before(cx, move |w, window, cx|
            w.commit_shared_sidebar_creation(retry_room, retry_origin, retry_kind, retry_store, window, cx)) { return; }

        let made = (|| -> anyhow::Result<_> {
            let mut store = shared::check_opened_store(store, &room, cx)?;
            anyhow::ensure!(room.scope.folder && shared::access(&store, cx).is_some_and(Access::can_write)
                && !shared::editing_paused(&store, cx),
                "{}", tr("此共享文件夹当前不可写入。", "You cannot write to this shared folder right now."));
            anyhow::ensure!(store.list().iter().any(|e| e.id == origin.id && e.meta.folder.is_some()),
                "{}", tr("找不到要存入的文件夹。", "The destination folder could not be found."));
            let history = store.track_file_changes();
            let id = match &kind {
                SharedFolderCreation::Document => store.create_from_in(&crate::model::Document::default(), Some(&origin.id))?,
                SharedFolderCreation::Folder(name) =>
                    store.create_folder(name.as_deref().unwrap_or_else(|| tr("未命名文件夹", "Untitled folder")), Some(&origin.id))?,
            };
            Ok((store, id, history.take()))
        })();
        let (store, id, changes) = match made {
            Ok(made) => made,
            Err(error) => {
                self.show_toast_kind(ui::ToastKind::Error, &crate::localized_format!("新建失败：{}", "Could not create item: {}", settings::diagnostic(&error.to_string())), cx);
                return;
            }
        };
        let created = SidebarCreatedItem { kind, changes };
        let entry = SidebarEntry::new(origin.section, &id);
        // The current pane stays intact until the new item is ready to open.
        if self.store.root() == store.root() {
            self.entries = self.store.list();
            self.sort();
            if self.try_open(&id, window, cx) {
                self.sidebar_entry = Some(entry);
                self.finish_sidebar_creation(&id, created, window, cx);
            }
        } else {
            self.navigate_shared_item_using(room, id, None, Some(SidebarNavigation {
                entry: Some(entry), prepared: Some(store), verified_access: None, new_invitation: None, published_invitation: None, action: None, created: Some(created), folder_edit: None,
            }), window, cx);
        }
    }

    #[cfg(target_os = "macos")]
    fn finish_sidebar_creation(&mut self, id: &str, created: SidebarCreatedItem, window: &mut Window, cx: &mut Context<Self>) {
        use crate::folders::SharedFolderCreation;
        self.publish_shared_folder_change(cx);
        if let Some(entry) = self.sidebar_entry.clone() { self.reveal_shared_sidebar_entry(&entry, cx); }
        let label = match created.kind {
            SharedFolderCreation::Document => {
                self.set_pref(|p| p.last_new = "doc".into(), cx);
                if let Some(editor) = &self.editor {
                    crate::gpui_compat::focus(window, &editor.read(cx).focus_handle(cx), cx);
                }
                tr("新建文档", "New document")
            }
            SharedFolderCreation::Folder(name) => {
                if name.is_none() {
                    let section = self.sidebar_entry.as_ref().map(|entry| entry.section);
                    self.start_rename_in_sidebar(id, section, window, cx);
                }
                tr("新建文件夹", "New folder")
            }
        };
        self.remember_file_changes(created.changes, label);
        cx.notify();
    }

    fn defer_shared_open(&mut self, room: &Room, target: &str, source: &OpenSource,
        window: &mut Window, cx: &mut Context<Self>) -> bool {
        use shared::RoomAccount::*;
        let personal = crate::store::Store::root_dir();
        match shared::room_account(&personal, cx) {
            Ready | Unavailable => return false,
            Verifying => {}
            issue => { self.shared_account_message(issue, cx); return true; }
        }

        let request = uuid::Uuid::new_v4();
        let revision = cloud_sync::account_revision();
        let route = self.shared_open_origin(cx);
        let room = room.clone();
        let target = target.to_owned();
        let mut open_source = source.clone();
        self.sharing.account_open = Some(request);
        self.delay_shared_open_notice(request, tr("正在确认 iCloud 账户…", "Checking your iCloud account…"), source.started, cx);
        cloud_sync::recheck_account(&personal, cx);
        cx.spawn_in(window, async move |this, cx| {
            let checkpoint = |ws: &Workspace, window: &Window, cx: &Context<Workspace>| {
                ws.sharing.account_open == Some(request)
                    && window.root::<Workspace>().flatten().is_some_and(|root| root.entity_id() == cx.entity_id())
                    && crate::store::Store::root_dir() == personal
                    && !ws.library_switching && !ws.sharing.busy && ws.confirm.is_none()
                    && ws.sharing.sheet.is_none() && route == ws.shared_open_origin(cx)
            };
            let start = std::time::Instant::now();
            loop {
                let state = this.update_in(cx, |ws, window, cx| {
                    if !checkpoint(ws, window, cx) {
                        if ws.sharing.account_open == Some(request) { ws.cancel_shared_open(); cx.notify(); }
                        return None;
                    }
                    Some(shared::room_account(&personal, cx))
                }).ok().flatten();
                match state {
                    Some(Ready) if cloud_sync::account_revision() == revision => break,
                    Some(Verifying) if cloud_sync::account_revision() == revision && start.elapsed().as_secs() < 30 => {}
                    Some(issue) => {
                        let _ = this.update_in(cx, |ws, _, cx| {
                            if ws.sharing.account_open != Some(request) { return; }
                            ws.cancel_shared_open();
                            ws.shared_account_message(if matches!(issue, Ready | Verifying) { Unavailable } else { issue }, cx);
                        });
                        return;
                    }
                    None => return,
                }
                cx.background_executor().timer(std::time::Duration::from_millis(250)).await;
            }

            // A fresh CloudKit permission check is required before exposing the cached room.
            let source = personal.clone();
            let requested = room.clone();
            let allowed = cx.background_executor().spawn(async move { shared::verify_room_access(&source, &requested) }).await;
            let _ = this.update_in(cx, |ws, window, cx| {
                let current = checkpoint(ws, window, cx);
                if ws.sharing.account_open == Some(request) { ws.cancel_shared_open(); cx.notify(); }
                if !current { return; }
                if cloud_sync::account_revision() != revision || shared::room_account(&personal, cx) != Ready {
                    ws.shared_account_message(Unavailable, cx); return;
                }
                match allowed {
                    Ok(access) => {
                        open_source.verified_access = Some(access);
                        ws.open_shared_item(room, target, open_source, window, cx);
                    }
                    Err(error) => {
                        crate::logging::write(&format!("Verify shared document access after reconnect: {error:#}"));
                        ws.show_toast_kind(ui::ToastKind::Info, tr("暂时无法确认此共享的访问权限。请稍后重试。",
                            "Could not confirm access to this share. Try again later."), cx);
                    }
                }
            });
        }).detach();
        true
    }

    fn shared_account_message(&mut self, issue: shared::RoomAccount, cx: &mut Context<Self>) {
        use shared::RoomAccount::*;
        let message = match issue {
            SignedOut => tr("请在系统设置中登录原 iCloud 账户，再打开共享文档。", "Sign in to your previous iCloud account in System Settings, then open the shared document."),
            Changed => tr("iCloud 账户已切换。请在「文档库 › iCloud」查看。", "The iCloud account changed. See Library › iCloud."),
            AccessRequired => shared::sharing_unavailable(),
            _ => tr("暂时无法确认 iCloud 账户，请稍后重试。", "Could not verify the iCloud account. Try again later."),
        };
        self.show_toast_kind(ui::ToastKind::Info, message, cx);
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
        self.show_toast_kind(ui::ToastKind::Error, tr("暂时无法打开共享项目。请重试。", "Could not open the shared item. Try again."), cx);
    }

    fn navigate_shared_item_using(&mut self, room: Room, target: String, invitation: Option<(String, Access)>, sidebar: Option<SidebarNavigation>, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_sidebar_open();
        if self.sharing.busy { return; }
        #[cfg(target_os = "ios")]
        if self.sharing.opening.as_ref().is_some_and(|opening| opening.target == target)
            && shared::scope(&self.store, cx) == Some(&room.scope) { return; }
        if !self.is_shared() && shared::original::awaiting_publication(self.store.root(), &room) {
            self.create_shared_note(&room.scope.note, window, cx);
            return;
        }
        if invitation.is_none() && sidebar.as_ref().is_none_or(|s| s.verified_access.is_none() && s.published_invitation.is_none())
            && self.viewing_shared_item(&room, &target, cx) {
            if let Some(sidebar) = sidebar {
                self.sidebar_entry = sidebar.entry;
                self.run_shared_open_action(sidebar.action, &target, window, cx);
            } else { self.sidebar_entry = None; }
            cx.notify();
            return;
        }
        self.close_history(cx);
        let retry_room = room.clone(); let retry_invitation = invitation.clone();
        let retry_target = target.clone(); let retry_sidebar = sidebar.clone();
        if !self.save_before(cx, move |w, window, cx| w.navigate_shared_item_using(retry_room, retry_target, retry_invitation, retry_sidebar, window, cx)) { return; }
        #[cfg(target_os = "macos")]
        let created = sidebar.as_ref().and_then(|sidebar| sidebar.created.clone());
        #[cfg(target_os = "macos")]
        let folder_edit = sidebar.as_ref().and_then(|sidebar| sidebar.folder_edit);
        let (entry, prepared, verified_access, new_invitation, published_invitation, action) = sidebar.map(|sidebar|
            (sidebar.entry, sidebar.prepared, sidebar.verified_access, sidebar.new_invitation, sidebar.published_invitation, sidebar.action)).unwrap_or_default();
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
        if let Some(access) = verified_access && let Err(error) = shared::apply_verified_room_access(&store, access, cx) {
            self.report_shared_open_error(&format!("{error:#}"), cx);
            return;
        }
        self.remember_scroll(cx);
        if self.is_shared() { self.refresh_shared_entry(cx); }
        let route = if self.is_shared() { self.sharing.personal_route.take() } else { Some(Box::new(PersonalRoute {
            document: self.current.clone(), selection: self.editor.as_ref().map(|editor| editor.read(cx).core.sel),
            query: self.search.read(cx).text.clone(), folder: self.folder_current.clone(), collection: self.sharing.collection.clone(),
            home: std::mem::take(&mut self.home), modified: self.current.as_ref().and_then(|id| self.entries.iter().find(|entry| &entry.id == id).map(|entry| entry.meta.modified)),
        })) };
        let sidebar = self.sidebar_open; let width = self.sidebar_width; let folded = self.sharing.folded; let personal_folded = self.folded;
        let expanded = self.expanded.clone(); let shared_expanded = std::mem::take(&mut self.sharing.expanded);
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
            let target = note.clone();
            let mut ws = Workspace::build_window(false, Some(note), Some(store), window, cx);
            #[cfg(target_os = "macos")]
            { ws.native_sidebar = native_sidebar; }
            ws.sidebar_entry = entry;
            #[cfg(target_os = "macos")]
            if ws.view == View::Home { ws.sharing.sidebar_waiting = Some(waiting.clone()); }
            ws.sidebar_open = sidebar; ws.sidebar_width = width; ws.folded = personal_folded;
            ws.expanded = expanded; ws.sharing.expanded = shared_expanded; ws.folder_drag = folder_drag; ws.sharing.sidebar = shared_drag; ws.list_scroll = scroll;
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
            #[cfg(target_os = "macos")]
            if let Some(created) = created { ws.finish_sidebar_creation(&waiting, created, window, cx); }
            else {
                if room.scope.folder { ws.refresh_shared_entry(cx); }
                if let Some(entry) = ws.sidebar_entry.clone() { ws.reveal_shared_sidebar_entry(&entry, cx); }
            }
            #[cfg(target_os = "macos")]
            if let Some(edit) = folder_edit && let Some(entry) = ws.sidebar_entry.clone() {
                ws.finish_shared_folder_edit(entry, edit, window, cx);
            }
            if let Some(reply) = new_invitation {
                ws.sharing_sheet(Sheet::Members(room.clone()), window, cx);
                ws.sharing.reply = Some(reply);
                ws.sharing.original = Some(room.scope.note.clone());
                ws.show_invitation_result(room, window, cx);
            } else if let Some((email, permission)) = invitation {
                ws.open_invitation(room, window, cx);
                ws.sharing.email.update(cx, |field, cx| field.set_text(&email, cx));
                ws.sharing.permission = permission;
                ws.sharing.original = original;
                ws.manage_shared_note("invite", email, Some(permission), window, cx);
            } else if action.is_some() {
                // Run after the new root is installed so callbacks can use normal window commands.
                cx.defer_in(window, move |ws, window, cx| ws.run_shared_open_action(action, &target, window, cx));
            }
            if let Some(text) = published_invitation {
                ws.sharing.copied_text = Some(text);
                ws.public_invitation_copied(cx);
            }
            ws
        });
        cx.defer(cloud_sync::release_unused);
    }

    fn run_shared_open_action(&mut self, action: Option<SharedAction>, target: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_shared() || !(self.view == View::Doc && self.current.as_deref() == Some(target)
            || self.view == View::Folder && self.folder_current.as_deref() == Some(target)) { return; }
        let action = action.and_then(|slot| slot.borrow_mut().take());
        if let Some(action) = action { action(self, target, window, cx); }
    }

    pub(crate) fn navigate_personal(&mut self, view: View, document: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_personal_from(view, document, None, window, cx);
    }

    pub(crate) fn navigate_personal_from(&mut self, view: View, document: Option<String>, entry: Option<SidebarEntry>, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_personal_route(view, document, entry, None, window, cx);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn navigate_personal_home_tab(&mut self, tab: crate::home::HomeTab, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_personal_route(View::Home, None, None, Some(tab), window, cx);
    }

    fn navigate_personal_route(&mut self, view: View, document: Option<String>, entry: Option<SidebarEntry>, tab: Option<crate::home::HomeTab>, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_shared_open();
        if !self.is_shared() {
            if let Some(id) = document {
                if self.try_open(&id, window, cx) { self.sidebar_entry = entry; cx.notify(); }
            } else {
                if let Some(tab) = tab { self.home.tab = tab; }
                self.show_view(view, cx);
            }
            return;
        }
        if self.sharing.busy { return; }
        self.close_history(cx);
        let retry = document.clone(); let retry_entry = entry.clone();
        if !self.save_before(cx, move |w, window, cx| w.navigate_personal_route(view, retry, retry_entry, tab, window, cx)) { return; }
        self.remember_scroll(cx);
        self.refresh_shared_entry(cx);
        let route = self.sharing.personal_route.take();
        let sidebar = self.sidebar_open; let width = self.sidebar_width; let folded = self.sharing.folded; let personal_folded = self.folded;
        let expanded = self.expanded.clone(); let shared_expanded = std::mem::take(&mut self.sharing.expanded);
        let folder_drag = std::mem::take(&mut self.folder_drag); let shared_drag = std::mem::take(&mut self.sharing.sidebar); let scroll = self.list_scroll.clone();
        #[cfg(target_os = "macos")]
        let native_sidebar = std::mem::take(&mut self.native_sidebar);
        let returning_to_listing = cfg!(target_os = "ios") && document.is_none() && matches!(view, View::Home | View::Folder);
        let start = (!returning_to_listing).then(|| document.clone().or_else(|| route.as_ref().and_then(|route| route.document.clone()))).flatten();
        window.replace_root(cx, |window, cx| {
            let mut ws = Workspace::build_window_with_recent(false, start, None, !returning_to_listing, window, cx);
            #[cfg(target_os = "macos")]
            { ws.native_sidebar = native_sidebar; }
            ws.sidebar_open = sidebar; ws.sidebar_width = width; ws.folded = personal_folded; ws.sharing.folded = folded;
            ws.expanded = expanded; ws.sharing.expanded = shared_expanded; ws.folder_drag = folder_drag; ws.sharing.sidebar = shared_drag; ws.list_scroll = scroll;
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
            else {
                if let Some(tab) = tab { ws.home.tab = tab; }
                ws.show_view(view, cx);
            }
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
            if self.try_open(&target, window, cx) {
                if let Some(entry) = &entry { self.reveal_shared_sidebar_entry(entry, cx); }
                self.sidebar_entry = entry;
                self.sharing.sidebar_waiting = None;
            }
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
