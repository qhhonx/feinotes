//! Shared-note navigation, creation and contextual access management.
//! Cloud libraries and invitation permissions remain owned by cloud_sync::shared.

use crate::{app::{View, Workspace}, cloud_sync::{self, shared::{self, Access, Audience, Reply, Room}},
    input::{TextField, TextFieldEvent}, settings::{self, tr, display_title}, theme::Theme, ui::{self, TipExt, TitleBarExt}};
use gpui::{prelude::FluentBuilder, *};
#[cfg(target_os = "ios")]
pub(crate) mod mobile;

#[cfg(feature = "devtools")]
pub(crate) mod qa;
#[cfg(feature = "devtools")]
mod qa_parity;
mod navigation;
#[cfg(target_os = "macos")]
pub(crate) use navigation::SharedFolderEdit;
mod lifecycle;
pub(crate) use lifecycle::SharedTrashAction;
mod list;
mod sidebar;
pub(crate) mod organizer;
pub(crate) mod preferences;
mod actions;
pub(crate) mod links;

#[derive(Clone)]
pub(crate) enum Sheet { MoveInto(Vec<String>, String, Room), RenameCollection(String), Create(String), CreateNew(bool, Option<String>), Accept, Access(Room), Members(Room), Invite(Room), End(Room), EndMany(Vec<Room>, Vec<String>) }

#[cfg(target_os = "ios")]
pub(crate) struct OpeningShared { pub target: String, pub title: String }

pub(crate) fn new_shared_title(folder: bool) -> &'static str {
    if folder { tr("新建共享文件夹", "New shared folder") }
    else { tr("新建共享文档", "New shared document") }
}

pub(crate) fn shared_access_title(folder: bool) -> &'static str {
    if folder { tr("文件夹共享权限", "Folder sharing access") }
    else { tr("文档共享权限", "Document sharing access") }
}

pub(crate) struct Sharing {
    #[cfg(target_os = "ios")]
    pub(crate) opening: Option<OpeningShared>,
    pub(crate) link: Entity<TextField>,
    pub(crate) email: Entity<TextField>,
    pub(crate) folder_name: Entity<TextField>,
    pub(crate) collection: Option<String>,
    pub sheet: Option<Sheet>,
    pub(crate) reply: Option<Reply>,
    prepared: Option<Room>,
    pub(crate) public_link: bool,
    pub(crate) close_link_confirm: bool,
    original: Option<String>,
    pub(crate) pending_open: Option<String>,
    pub(crate) account_open: Option<uuid::Uuid>,
    #[cfg(target_os = "macos")]
    pub(crate) sidebar_load: Option<uuid::Uuid>,
    #[cfg(target_os = "macos")]
    pub(crate) sidebar_waiting: Option<String>,
    copied_text: Option<String>,
    pub(crate) permission: Access,
    member: Option<String>,
    revoke_confirm: bool,
    pub(crate) busy: bool,
    pub(crate) message: Option<(bool, String)>,
    epoch: u64,
    focus_before: Option<FocusHandle>,
    subscriptions: Vec<Subscription>,
    scroll: ScrollHandle,
    pub panel_open: bool,
    pub(crate) sync_panel_open: bool,
    permissions_only: bool,
    pub(crate) folded: bool,
    pub(crate) expanded: std::collections::HashSet<String>,
    pub(crate) sidebar: crate::sidebar_drag::SidebarDrag,
    pub(crate) batch: bool,
    personal_route: Option<Box<navigation::PersonalRoute>>,
    accept_at: Option<&'static str>,
    share_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    toolbar_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
}

impl Sharing {
    pub fn new(cx: &mut App) -> Self {
        if preferences::install(cx) || !cx.has_global::<organizer::Organizer>() {
            cx.set_global(organizer::Organizer::load().unwrap_or_default());
        }
        Self {
            #[cfg(target_os = "ios")]
            opening: None,
            #[cfg(target_os = "macos")]
            sidebar_load: None,
            #[cfg(target_os = "macos")]
            sidebar_waiting: None,
            link: cx.new(|cx| TextField::new(tr("粘贴飞记邀请链接", "Paste a Fei Notes invitation link"), cx)),
            collection: None, folder_name: cx.new(|cx| TextField::new(tr("名称", "Name"), cx)),
            email: cx.new(|cx| TextField::new(tr("对方的 Apple 账户邮箱", "Their Apple Account email"), cx)),
            sheet: None, reply: None, prepared: None, public_link: false, close_link_confirm: false, permission: Access::ReadOnly, member: None, revoke_confirm: false,
            busy: false, message: None, epoch: 0, original: None, pending_open: None, account_open: None, copied_text: None, focus_before: None, subscriptions: vec![],
            scroll: ScrollHandle::new(), panel_open: false, sync_panel_open: false, permissions_only: false, folded: true, expanded: Default::default(), sidebar: Default::default(), batch: false, personal_route: None, accept_at: None, share_bounds: Default::default(), toolbar_bounds: Default::default() }
    }
}

fn sync_badge() -> Div {
    let (fill, ink) = settings::sync_status::marker_colors();
    div().absolute().right(px(1.)).top(px(1.)).size(px(14.)).rounded_full()
        .bg(fill).text_color(ink).text_size(px(10.)).line_height(px(11.))
        .font_weight(FontWeight::BOLD).border_1().border_color(Theme::bg_body())
        .flex().items_center().justify_center().child("!")
}

fn invitation_text(title: &str, link: &str) -> String {
    format!("{}\n{link}", crate::localized_format!("邀请你在飞记中协作《{}》", "Join me in Fei Notes: {}", display_title(title)))
}

pub(crate) fn link_audience(audience: Audience) -> &'static str {
    match audience {
        Audience::Invited => tr("仅受邀者", "Only invited people"),
        Audience::LinkDraft => tr("公开链接尚未启用", "Public link is off"),
        Audience::LinkReadOnly => tr("任何持有链接的人 · 仅查看", "Anyone with the link · View only"),
        Audience::LinkReadWrite => tr("任何持有链接的人 · 可编辑", "Anyone with the link · Can edit"),
        Audience::LinkClosed => tr("公开链接已关闭", "Public link closed"),
    }
}

pub(crate) fn public_link_requirements() -> &'static str {
    tr("持有链接的人可以在飞记中加入，链接可以转发。对方需登录 iCloud 并有有效的云功能权限（正式版需订阅或试用）。",
        "Anyone with the link can join in Fei Notes, including people it is forwarded to. They need to sign in to iCloud and have cloud access (a subscription or trial in the App Store version).")
}

pub(crate) fn private_link_requirements() -> &'static str {
    tr("指定邀请只对受邀账户有效。公开链接可在首次共享单篇文档时选择。",
        "Account invitations work only for the accounts you invite. A public link can be chosen when first sharing a single document.")
}

pub(crate) fn close_link_warning() -> &'static str {
    tr("通过此链接加入的人将失去访问权限。已下载或导出的副本仍可能保留。",
        "People who joined through this link will lose access. Downloaded or exported copies may remain.")
}

fn caption(text: impl Into<SharedString>) -> Div {
    div().w_full().min_w_0().text_size(px(13.)).line_height(px(20.)).text_color(Theme::text_caption()).child(ui::ui_para(text.into()))
}

fn action(id: impl Into<ElementId>, label: &'static str, enabled: bool) -> Stateful<Div> {
    settings::button(id, label).when(!enabled, |d| d.opacity(0.45).cursor_default())
}

/// Sharing control used by the Lark interface.
fn feishu_share_button(id: &'static str, enabled: bool) -> Stateful<Div> {
    use crate::feishu_theme::ud;
    let ink = white();
    div().id(id).no_drag().flex_none().h(px(32.)).min_w(px(80.)).px(px(10.)).rounded(px(6.))
        .flex().items_center().justify_center().gap(px(5.))
        .bg(rgb(ud::B600)).text_color(ink).text_size(px(14.))
        .child(ui::icon("share", 16., ink))
        .child(tr("分享", "Share"))
        .when(enabled, |d| d.cursor_pointer().hover(|s| s.bg(rgb(ud::B700))).active(|s| s.bg(rgb(ud::B800))))
        .when(!enabled, |d| d.opacity(0.45))
}

fn invitation_accent() -> Hsla {
    if settings::interface_feishu() { rgb(crate::feishu_theme::ud::B600).into() }
    else { crate::themes::interface().token("primary_500") }
}

fn anchor_control(button: Stateful<Div>, bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>) -> Div {
    div().relative().child(canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {}).absolute().inset_0()).child(button)
}

pub(crate) fn role(access: Option<Access>) -> &'static str {
    match access { Some(Access::Owner) => tr("所有者", "Owner"), Some(Access::ReadWrite) => tr("可编辑", "Can edit"),
        Some(Access::ReadOnly) => tr("只读", "Can view"), Some(Access::Revoked) => tr("访问已结束", "Access ended"),
        None => tr("正在确认权限", "Checking access") }
}

fn line(title: impl Into<SharedString>, detail: impl Into<SharedString>, controls: impl IntoElement) -> Div {
    div().w_full().py(px(12.)).flex().flex_wrap().items_center().gap(px(10.)).border_b_1().border_color(Theme::line_divider())
        .child(div().flex_1().min_w_0().flex().flex_col().gap(px(3.))
            .child(div().w_full().min_w_0().text_size(px(14.)).child(ui::ui_para(title.into()))).child(caption(detail)))
        .child(controls)
}

pub(crate) fn member_label(name: &str, id: &str, index: usize) -> String {
    let name = name.trim();
    if name.is_empty() || name == id || matches!(name, "..." | "…") || name.chars().any(char::is_control) {
        crate::localized_format!("参与者 {}", "Participant {}", index + 1)
    } else { name.chars().take(100).collect() }
}

impl Workspace {
    pub(crate) fn confirm_shared_move(&mut self, ids: &[String], target: Option<&str>, cx: &mut Context<Self>) -> bool {
        if self.is_shared() { return false; }
        let Some((target, room)) = target.and_then(|t| shared::original::owner_room(&self.entries, t).map(|r| (t, r))).filter(|(_, r)| r.scope.folder) else { return false; };
        if !self.sharing.busy {
            #[cfg(target_os = "ios")]
            self.mobile_present_sharing(cx);
            self.sharing.sheet = Some(Sheet::MoveInto(ids.to_vec(), target.into(), room));
            self.sharing.message = None; self.row_menu = None;
            cx.notify();
        }
        true
    }

    pub(crate) fn stopping_original(&self, room: &Room, cx: &App) -> bool {
        room.scope.owner.is_none() && self.personal_navigation_entries(cx).iter().any(|e| e.id == room.scope.note && e.meta.share_root.as_ref() == Some(&e.id))
    }

    pub(crate) fn shared_end_label(&self, room: &Room, cx: &App) -> &'static str {
        if self.stopping_original(room,cx) { tr("停止共享", "Stop sharing") }
        else if room.scope.owner.is_some() { tr("离开共享", "Leave share") }
        else if room.scope.folder { tr("删除共享文件夹", "Delete shared folder") }
        else { tr("删除共享文档", "Delete shared document") }
    }

    pub(crate) fn sharing_busy(&self) -> bool { self.sharing.busy }
    pub(crate) fn refresh_shared_entry(&self, cx: &mut Context<Self>) {
        if shared::access(&self.store, cx) == Some(Access::Revoked) { return; }
        let Some(scope) = shared::scope(&self.store, cx).cloned() else { return; };
        if let Some(entry) = self.entries.iter().find(|entry| entry.id == scope.note) {
            let mut entry = entry.clone();
            if self.current.as_ref() == Some(&entry.id) && let Some(editor) = &self.editor {
                entry.body = editor.read(cx).core.doc.plain_text().into();
            }
            cloud_sync::activity::update_entry(&scope, &entry, cx);
        }
        if scope.folder { cloud_sync::activity::update_folder_items(&scope, &self.entries, cx); }
    }

    pub(crate) fn publish_shared_folder_change(&self, cx: &mut Context<Self>) {
        self.refresh_shared_entry(cx);
        if shared::scope(&self.store, cx).is_some_and(|scope| scope.folder) {
            cloud_sync::recheck_account(self.store.root(), cx);
        }
    }

    pub(crate) fn return_to_shared_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_personal(View::Shared, None, window, cx);
    }

    pub(crate) fn close_sharing_for_navigation(&mut self) {
        self.sharing.panel_open = false;
        self.sharing.sync_panel_open = false;
        if !self.sharing.busy { self.sharing.sheet = None; self.sharing.subscriptions.clear(); self.sharing.epoch += 1; }
    }

    pub(crate) fn close_sharing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sharing.panel_open = false;
        self.sharing.sync_panel_open = false;
        if self.sharing.busy { return; }
        self.sharing.sheet = None;
        self.sharing.epoch += 1;
        self.sharing.subscriptions.clear();
        if let Some(focus) = self.sharing.focus_before.take() { crate::gpui_compat::focus(window,&focus,cx); }
        #[cfg(target_os = "ios")]
        if self.view == View::Settings && self.settings_page == settings::Page::Shared { self.mobile_close_settings(window,cx); }
        cx.notify();
    }

    pub(crate) fn sharing_sheet(&mut self, sheet: Sheet, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy { return; }
        #[cfg(target_os = "ios")]
        self.mobile_present_sharing(cx);
        if self.sharing.sheet.is_none() { self.sharing.focus_before = window.focused(cx); }
        self.sharing.epoch += 1;
        self.sharing.sheet = Some(sheet);
        self.sharing.accept_at = None;
        self.sharing.panel_open = false;
        self.sharing.sync_panel_open = false;
        self.sharing.prepared = None;
        self.sharing.public_link = false;
        self.sharing.close_link_confirm = false;
        self.sharing.reply = None;
        self.sharing.member = None;
        self.sharing.revoke_confirm = false;
        self.sharing.message = None;
        self.sharing.permission = Access::ReadOnly;
        self.sharing.scroll.set_offset(point(px(0.), px(0.)));
        self.sharing.email.update(cx, |field, cx| field.set_text("", cx));
        self.sharing.link.update(cx, |field, cx| field.set_text("", cx));
        let input = if matches!(self.sharing.sheet, Some(Sheet::RenameCollection(_) | Sheet::CreateNew(..))) { &self.sharing.folder_name } else if matches!(self.sharing.sheet, Some(Sheet::Accept)) { &self.sharing.link } else { &self.sharing.email };
        if matches!(self.sharing.sheet, Some(Sheet::Access(_) | Sheet::Members(_) | Sheet::End(_) | Sheet::EndMany(..))) { crate::gpui_compat::focus(window,&self.focus,cx); }
        else { crate::gpui_compat::focus(window,&input.focus_handle(cx),cx); }
        self.sharing.subscriptions = vec![cx.subscribe_in(input, window, |w, _, event: &TextFieldEvent, window, cx| {
            match event {
                TextFieldEvent::Submit => w.submit_sharing(window, cx),
                TextFieldEvent::Cancel => w.close_sharing(window, cx),
                _ => cx.notify(),
            }
        })];
        cx.notify();
    }

    pub(crate) fn current_room(&self, cx: &App) -> Option<Room> {
        let scope = shared::scope(&self.store, cx)?.clone();
        let title = self.entries.iter().find(|entry| entry.id == scope.note).map(|entry| entry.title.clone()).unwrap_or_default();
        Some(Room { scope, title })
    }

    pub(crate) fn sharing_access(&self, cx: &App) -> Option<Access> { shared::access(&self.store, cx) }

    fn sharing_account_ready(&self, cx: &App) -> bool {
        !crate::cloud_restore::local_only() && cloud_sync::status(self.sharing_account_root(cx), cx).is_some_and(|s| !matches!(s.phase, cloud_sync::SyncPhase::AccountChecking | cloud_sync::SyncPhase::AccountChanged | cloud_sync::SyncPhase::SignedOut))
    }

    fn sharing_account_root<'a>(&'a self, _cx: &'a App) -> &'a std::path::Path {
        #[cfg(target_os = "macos")]
        if matches!(self.sharing.sheet, Some(Sheet::CreateNew(..)))
            && let Some(personal) = _cx.try_global::<crate::shared_store::SharedStore>() { return personal.root(); }
        self.store.root()
    }

    pub(crate) fn sharing_allowed(&self, cx: &App) -> bool {
        self.sharing_account_ready(cx) && cloud_sync::status(self.sharing_account_root(cx), cx).is_some_and(|status| !status.access_paused)
    }

    fn can_manage_room(&self, room: &Room, cx: &App) -> bool {
        if room.scope.owner.is_some() { return false; }
        #[cfg(target_os = "ios")]
        if self.is_shared() && shared::scope(&self.store,cx) != Some(&room.scope) {
            return self.sharing_account_ready(cx) &&
                cloud_sync::status(&crate::store::Store::root_dir(),cx).is_some_and(|status| !status.access_paused);
        }
        self.sharing_allowed(cx) && (!self.is_shared() || self.sharing_access(cx) == Some(Access::Owner))
    }

    pub(crate) fn render_shared_list(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.home.tab = crate::home::HomeTab::Shared;
        self.render_home(window, cx)
    }

    pub(crate) fn sharing_request_busy(&self) -> bool { self.sharing.busy }

    pub(crate) fn receive_invitation(&mut self, link: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy {
            self.show_toast_kind(ui::ToastKind::Info, tr("请先完成当前共享操作，再打开邀请。", "Finish the current sharing action, then open the invitation again."), cx);
            return;
        }
        match links::branded(&link) {
            Ok(link) => {
                self.create_menu = None; self.row_menu = None;
                self.sharing_sheet(Sheet::Accept, window, cx);
                self.sharing.link.update(cx, |field, cx| field.set_text(&link, cx));
            }
            Err(_) => self.show_toast_kind(ui::ToastKind::Error, tr("邀请链接无效，请向邀请者索取新的链接。", "This invitation link is invalid. Ask the sender for a new link."), cx),
        }
    }

    pub(crate) fn accept_invitation_open(&self) -> bool { matches!(self.sharing.sheet, Some(Sheet::Accept)) }

    pub(crate) fn open_accept_invitation_from(&mut self, anchor: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.open_accept_invitation(window, cx);
        if self.accept_invitation_open() { self.sharing.accept_at = Some(anchor); }
    }

    pub(crate) fn open_accept_invitation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.accept_invitation_open() { self.close_sharing(window, cx); }
        else { self.create_menu = None; self.row_menu = None; self.sharing_sheet(Sheet::Accept, window, cx); }
    }

    pub(crate) fn sharing_toolbar(&self, window: &Window, cx: &mut Context<Self>) -> Option<Div> {
        if !self.is_shared() {
            let id = self.current.clone()?;
            if self.store.local_path(&id).is_some() { return None; }
            let button = if settings::interface_feishu() {
                feishu_share_button("doc-create-share", true)
            } else {
                ui::grouped_toolbar_control(div().id("doc-create-share").no_drag().size(px(crate::app::HEADER_ACTIONS_H))
                    .flex().items_center().justify_center().cursor_pointer(), 8., Theme::fill_hover, Some(Theme::fill_pressed))
                    .child(ui::share_toolbar_icon())
            };
            return Some(anchor_control(button.tip(tr("共享", "Share"), None)
                .on_click(cx.listener(move |w, _, window, cx| w.create_shared_note(&id, window, cx))), self.sharing.share_bounds.clone()));
        }
        let people = self.online_collaborators(cx);
        let cap = if window.viewport_size().width < px(500.) { 0 } else { 2 };
        let bounds = self.sharing.toolbar_bounds.clone();
        let feishu = settings::interface_feishu();
        let presence = ui::grouped_toolbar_control(div().id("doc-share").no_drag().flex_none().min_w(px(crate::app::HEADER_ACTIONS_H)).h(px(crate::app::HEADER_ACTIONS_H))
            .px(px(5.)).flex().items_center().justify_center().gap(px(8.)).cursor_pointer(), 8., Theme::fill_hover, Some(Theme::fill_pressed))
            .when(self.sharing.panel_open && !self.sharing.permissions_only, |d| d.bg(Theme::fill_hover()))
            .when(cap == 0, |d| d.child(div().size(px(7.)).rounded_full().bg(crate::editor_theme::text_color("green").unwrap_or_else(Theme::primary))))
            .children(people.iter().take(cap).enumerate().map(|(ix, person)| div().id(("shared-avatar", ix)).relative().size(px(32.)).rounded_full()
                .flex().items_center().justify_center().bg(person.color().opacity(0.14)).text_color(Theme::text())
                .text_size(px(13.)).child(person.initials.clone())
                .child(div().absolute().right(px(-1.)).bottom(px(-1.)).size(px(8.)).rounded_full().bg(person.color()).border_1().border_color(Theme::bg_body()))
                .tip(person.name.as_ref().map_or("", |name| name.as_ref()), None)))
            .when(people.len() > cap, |d| {
                let label = if cap == 0 { people.len().to_string() } else { format!("+{}", people.len() - cap) };
                d.child(div().min_w(px(32.)).h(px(32.)).px(px(4.)).rounded_full().flex().items_center().justify_center()
                    .bg(Theme::fill_hover()).text_color(Theme::text()).text_size(px(13.)).child(label))
            })
            .tip(tr("在线协作者", "Online collaborators"), None)
            .on_click(cx.listener(|w, _, window, cx| {
                w.doc_menu = false;
                w.doc_sub = None;
                w.sharing.sync_panel_open = false;
                w.sharing.panel_open = !w.sharing.panel_open || w.sharing.permissions_only;
                w.sharing.permissions_only = false;
                crate::gpui_compat::focus(window,&w.focus,cx);
                cx.notify();
            }));
        Some(div().flex().flex_none().items_center().gap(px(if feishu { 10. } else { 4. }))
            .when(!people.is_empty(), |d| d.child(anchor_control(presence, bounds)))
            .children(self.shared_access_button(window, cx)))
    }

    pub(crate) fn shared_access_button(&self, window: &Window, cx: &mut Context<Self>) -> Option<Div> {
        let owner = self.current_room(cx)?.scope.owner.is_none();
        let feishu = settings::interface_feishu();
        let attention = settings::sync_status::needs_attention(self,cx);
        let share = ui::grouped_toolbar_control(div().id("doc-share-invite").no_drag().flex_none()
            .h(px(crate::app::HEADER_ACTIONS_H)).min_w(px(crate::app::HEADER_ACTIONS_H)).px(px(if feishu { 9. } else { 0. })),
            8., Theme::fill_hover, Some(Theme::fill_pressed))
            .flex().items_center().justify_center().gap(px(5.)).cursor_pointer()
            .when(self.sharing.sync_panel_open || self.sharing.panel_open && self.sharing.permissions_only, |d| {
                d.bg(ui::toolbar_color("toolbar.button.hover").unwrap_or_else(Theme::fill_hover))
            })
            .child(ui::participants_icon(true))
            .when(feishu && window.viewport_size().width >= px(500.), |d| d.child(tr("已共享", "Shared")))
            .when(attention, |d| d.relative().child(sync_badge()))
            .on_click(cx.listener(move |w, _, _, cx| {
                w.doc_menu = false;
                w.doc_sub = None;
                if !std::mem::take(&mut w.sharing.sync_panel_open) {
                    w.sharing.panel_open = !w.sharing.panel_open || !w.sharing.permissions_only;
                    w.sharing.permissions_only = true;
                }
                cx.notify();
            }))
            .tip(if attention { tr("同步需要处理", "Sync needs attention") }
                else if owner { tr("成员与权限", "Members and access") } else { tr("查看共享权限", "View sharing access") }, None);
        Some(anchor_control(share, self.sharing.share_bounds.clone()))
    }

    pub(crate) fn online_collaborators<'a>(&self, cx: &'a App) -> &'a [cloud_sync::presence::Collaborator] {
        self.current.as_ref().and_then(|id| cloud_sync::presence::view(self.store.root(), cx)?.collaborators.get(id))
            .map(Vec::as_slice).unwrap_or_default()
    }

    pub(crate) fn render_sharing_toolbar_panel(&mut self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.sharing.sync_panel_open { return self.shared_sync_popover(window,cx); }
        let permissions_only = self.sharing.permissions_only;
        if !self.sharing.panel_open || !(self.view == View::Doc || self.view == View::Folder && permissions_only) { return None; }
        let room = self.current_room(cx)?;
        let people = self.online_collaborators(cx);
        if !permissions_only && people.is_empty() { self.sharing.panel_open = false; return None; }
        let anchor = if permissions_only { self.sharing.share_bounds.get()? } else { self.sharing.toolbar_bounds.get()? };
        let access = self.sharing_access(cx);
        let status = cloud_sync::status(self.store.root(), cx);
        let state = if access == Some(Access::Revoked) { tr("共享访问已结束", "Shared access has ended") }
            else if !self.sharing_allowed(cx) { tr("协作已暂停，请检查 iCloud 状态。", "Collaboration is paused. Check iCloud status.") }
            else if status.is_some_and(|s| s.phase == cloud_sync::SyncPhase::Offline) { tr("当前离线，显示本机内容。", "Offline. Showing local content.") }
            else if status.is_some_and(|s| matches!(s.phase, cloud_sync::SyncPhase::Failed | cloud_sync::SyncPhase::Account | cloud_sync::SyncPhase::Quota)) { tr("同步遇到问题，请查看 iCloud 状态。", "Sync needs attention. Check iCloud status.") }
            else if status.is_some_and(|s| s.phase == cloud_sync::SyncPhase::Ready && s.downloads == 0 && s.uploads == 0 && s.held == 0 && s.attachments == 0) {
                tr("内容已同步", "Content is synced")
            } else { tr("正在获取或同步更新", "Checking or syncing updates") };
        let feishu = settings::interface_feishu();
        let owner = room.scope.owner.is_none();
        let heading = if !permissions_only { tr("在线协作者", "Online collaborators") }
            else if owner { tr("成员与权限", "Members and access") }
            else { tr("共享权限", "Sharing access") };
        let mut body = div().flex().flex_col().p(px(14.)).gap(px(8.))
            .child(div().flex().items_center().gap(px(7.))
                .when(!feishu, |d| d.child(ui::participants_icon(false)))
                .child(div().text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).child(heading))
                .when(feishu && !permissions_only, |d| d.child(div().rounded(px(5.)).px(px(5.)).bg(Theme::fill_hover()).text_size(px(12.)).child(people.len().to_string()))))
            .child(div().truncate().text_size(px(13.)).text_color(Theme::text_caption()).child(display_title(&room.title)));
        if permissions_only && owner {
            body = body.child(caption(tr("所有者 · 你", "Owner · You")));
        } else if permissions_only {
            body = body.child(caption(shared::owner_display_name(self.store.root()).map_or_else(
                || tr("与你共享", "Shared with you").to_owned(),
                |name| crate::localized_format!("{}共享给你", "Shared with you by {}", name),
            ))).child(caption(role(access)));
        } else {
            body = body.child(caption(format!("{} · {state}", role(access))));
            let row_height = if feishu { 36. } else { 28. };
            let list_height = (row_height * people.len() as f32).min(row_height * 5.5);
            let list = div().id("sharing-present-people").h(px(list_height))
                .max_h((window.viewport_size().height - anchor.bottom() - px(210.)).max(px(row_height)))
                .overflow_y_scroll().children(people.iter().enumerate().map(|(index, person)| {
                    let name = person.name.as_ref().map_or("", |s| s.as_ref());
                    let mark = if feishu {
                        div().size(px(26.)).rounded_full().flex().items_center().justify_center()
                            .bg(person.color().opacity(0.14)).text_size(px(12.)).child(person.initials.clone())
                    } else {
                        div().size(px(8.)).rounded_full().bg(person.color())
                    };
                    div().h(px(row_height)).flex_none().flex().items_center().gap(px(9.))
                        .child(mark.flex_none()).child(div().min_w_0().flex_1().truncate().text_size(px(13.)).child(member_label(name, "", index)))
                        .child(div().text_size(px(12.)).text_color(Theme::text_caption()).child(tr("在线", "Online")))
                }));
            body = body.child(list);
        }
        body = body.child(div().h(px(1.)).bg(Theme::line_divider()));
        let members = room.clone();
        let access_title = if owner { tr("成员与权限…", "Members and access…") } else { tr("查看共享权限…", "View sharing access…") };
        body = body.child(action("collab-members", access_title, !self.sharing.busy)
            .on_click(cx.listener(move |w, _, window, cx| {
                if owner { w.open_member_management(members.clone(),window,cx); }
                else { w.sharing_sheet(Sheet::Access(members.clone()),window,cx); }
            })));
        if permissions_only {
            let mut sync = action("collab-cloud-settings", tr("同步状态", "Sync status"), true);
            if settings::sync_status::needs_attention(self,cx) {
                sync = sync.child(div().ml(px(6.)).font_weight(FontWeight::SEMIBOLD).text_color(settings::sync_status::marker_colors().0)
                    .child(tr("需处理", "Needs attention")));
            }
            body = body.child(sync.on_click(cx.listener(|w, _, window, cx| w.open_shared_sync(window,cx))));
        }
        let end_room = room.clone();
        body = body.child(div().h(px(1.)).bg(Theme::line_divider()))
            .child(action("collab-end", self.shared_end_label(&room,cx), self.sharing_account_ready(cx))
                .on_click(cx.listener(move |w, _, window, cx| w.open_end_shared(end_room.clone(), window, cx))));
        Some(deferred(anchored().anchor(Anchor::TopRight).position(point(anchor.right(), anchor.bottom() + px(4.)))
            .snap_to_window_with_margin(px(8.)).child(ui::popover().id("doc-sharing-panel").occlude()
                .w((window.viewport_size().width - px(16.)).min(px(if feishu { 340. } else { 300. })))
                .max_h((window.viewport_size().height - anchor.bottom() - px(12.)).max(px(100.)))
                .overflow_y_scroll().child(body).on_mouse_down_out(cx.listener(move |w, event: &MouseDownEvent, _, cx| {
                    if !anchor.contains(&event.position) { w.sharing.panel_open = false; cx.notify(); }
                })))).with_priority(2).into_any_element())
    }

    fn open_shared_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.view, View::Doc | View::Folder) || self.current_room(cx).is_none() { return; }
        self.close_sharing(window,cx);
        self.sharing.sync_panel_open = true;
        cx.notify();
    }

    fn shared_sync_popover(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        use settings::sync_status::Action;
        if !matches!(self.view, View::Doc | View::Folder) { return None; }
        let anchor = self.sharing.share_bounds.get()?;
        let state = settings::sync_status::shared(self,cx)?;
        let root = self.store.root().to_path_buf();
        let (warning, _) = settings::sync_status::marker_colors();
        let close = action("shared-sync-close", tr("关闭", "Close"), true)
            .on_click(cx.listener(|w, _, _, cx| { w.sharing.sync_panel_open = false; cx.notify(); }));
        let mut body = div().p(px(14.)).flex().flex_col().gap(px(8.))
            .child(div().flex().justify_between().items_center().gap(px(8.))
                .child(div().font_weight(FontWeight::SEMIBOLD).child(tr("同步状态", "Sync status"))).child(close))
            .child(caption(state.scope))
            .child(div().font_weight(FontWeight::SEMIBOLD).when(state.attention, |d| d.text_color(warning)).child(state.title))
            .when(!state.detail.is_empty(), |d| d.child(caption(state.detail)))
            .children(state.rows.into_iter().map(|(title,value)| div().flex().justify_between().gap(px(8.))
                .border_t_1().border_color(Theme::line_divider()).pt(px(5.)).text_size(px(12.))
                .child(title).child(value)));
        if let Some(operation) = state.action {
            let label = match operation {
                Action::Retry => tr("立即同步", "Sync now"),
                Action::Account | Action::Plans => tr("前往 iCloud 设置", "Open iCloud Settings"),
            };
            body = body.child(div().flex().justify_end().child(action("shared-sync-action", label, true)
                .on_click(cx.listener(move |w, _, window, cx| {
                    if w.store.root() != root || !w.sharing.sync_panel_open { return; }
                    if settings::sync_status::shared(w,cx).and_then(|s|s.action) != Some(operation) { return; }
                    if operation == Action::Retry {
                        cloud_sync::retry_shared(&root,cx);
                        cx.notify();
                    } else { w.open_cloud_settings_from_share(window,cx); }
                }))));
        }
        Some(deferred(anchored().anchor(Anchor::TopRight).position(point(anchor.right(), anchor.bottom()+px(4.)))
            .snap_to_window_with_margin(px(8.)).child(ui::popover().id("shared-sync-popover").occlude()
                .w((window.viewport_size().width-px(16.)).min(px(330.)))
                .max_h((window.viewport_size().height-anchor.bottom()-px(12.)).max(px(100.)))
                .overflow_y_scroll().child(body)
                .on_mouse_down_out(cx.listener(move |w, ev: &MouseDownEvent, _, cx| {
                    if !anchor.contains(&ev.position) { w.sharing.sync_panel_open = false; cx.notify(); }
                })))).with_priority(5).into_any_element())
    }

    pub(crate) fn new_and_share(&mut self, folder: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy || !self.home.shared() || self.sharing.collection.is_some() || crate::cloud_restore::local_only() { return; }
        #[cfg(target_os = "ios")]
        if self.is_shared() && self.mobile_personal_browser(cx) {
            self.personal_action(window, cx, move |w, window, cx| w.new_and_share(folder, window, cx));
            return;
        }
        self.open_new_shared_item(folder, window, cx);
    }

    pub(crate) fn open_new_shared_item(&mut self, folder: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy || crate::cloud_restore::local_only() || !self.flush(cx) { return; }
        #[cfg(target_os = "ios")]
        if self.is_shared() { return; }
        self.create_menu = None;
        self.row_menu = None;
        self.sharing.folder_name.update(cx, |field, cx| field.set_text("", cx));
        self.sharing_sheet(Sheet::CreateNew(folder, None), window, cx);
    }

    fn submit_new_shared_item(&mut self, folder: bool, id: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let email = self.sharing.email.read(cx).text.trim().to_owned();
        let title = self.sharing.folder_name.read(cx).text.trim().to_owned();
        if email.is_empty() || title.is_empty() || !self.flush(cx) { return; }
        let id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        self.sharing.sheet = Some(Sheet::CreateNew(folder, Some(id.clone())));
        let permission = self.sharing.permission;
        let personal = crate::store::Store::root_dir();
        let revision = cloud_sync::account_revision();
        let epoch = self.begin_sharing_request(cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                let room = shared::original::prepare_new(&personal, id, title, folder)?;
                cloud_sync::check_account_revision(revision)?;
                let reply = shared::original::invite_new(&personal, &room, &email, permission, revision)?;
                let row = cloud_sync::activity::row(&personal, room.clone())?
                    .ok_or_else(|| anyhow::anyhow!("The new shared item is unavailable"))?;
                Ok::<_, anyhow::Error>((room, row, reply))
            }).await;
            let _ = this.update_in(cx, |w, window, cx| match result {
                Ok((room, row, reply)) => {
                    if !w.finish_sharing_request(epoch, None, cx) || cloud_sync::account_revision() != revision { return; }
                    w.close_sharing(window, cx);
                    if !w.is_shared() { w.entries = w.store.list(); w.sort(); }
                    cloud_sync::activity::publish_row(row, cx);
                    w.navigate_new_shared_item(room, reply, window, cx);
                }
                Err(error) => { w.finish_sharing_request(epoch, Some(error), cx); }
            });
        }).detach();
    }

    pub(crate) fn create_shared_note(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_shared() || self.store.local_path(id).is_some() || self.sharing.busy || !self.flush(cx) { return; }
        if let Some(room) = shared::original::owner_room(&self.entries, id) {
            if shared::original::awaiting_publication(self.store.root(), &room) {
                self.sharing_sheet(Sheet::Create(room.scope.note.clone()), window, cx);
                self.sharing.public_link = shared::public_link_creation_enabled() && shared::public_link_pending(self.store.root(), &room);
                self.sharing.prepared = Some(room);
                return;
            }
            self.open_member_management(room, window, cx);
            return;
        }
        self.sharing_sheet(Sheet::Create(id.into()), window, cx);
    }

    pub(crate) fn can_choose_link(&self, id: &str) -> bool {
        shared::public_link_creation_enabled() && self.sharing.prepared.is_none()
            && self.entries.iter().any(|entry| entry.id == id && entry.meta.folder.is_none())
    }

    pub(crate) fn set_public_link(&mut self, public_link: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy || !matches!(&self.sharing.sheet, Some(Sheet::Create(id)) if self.can_choose_link(id)) { return; }
        self.sharing.public_link = public_link;
        self.sharing.message = None;
        if public_link { crate::gpui_compat::focus(window, &self.focus, cx); }
        cx.notify();
    }

    pub(crate) fn can_retry_public_link(&self, room: &Room, cx: &App) -> bool {
        shared::public_link_creation_enabled() && !self.sharing.busy && self.can_manage_room(room, cx)
            && self.sharing.reply.as_ref().is_some_and(|reply| reply.audience == Audience::LinkDraft)
            && shared::public_link_pending(&crate::store::Store::root_dir(), room)
    }

    pub(crate) fn retry_public_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(room) = self.sheet_room().filter(|room| self.can_retry_public_link(room, cx)).cloned() else { return; };
        let root = crate::store::Store::root_dir();
        let epoch = self.begin_sharing_request(cx);
        cx.spawn_in(window, async move |this, cx| {
            let target = room.clone();
            let outcome = cx.background_executor().spawn(async move { shared::publish_link(&root, &target) }).await;
            let _ = this.update_in(cx, |w, window, cx| w.finish_public_link(epoch, room, outcome, false, window, cx));
        }).detach();
    }

    fn finish_public_link(&mut self, epoch: u64, room: Room, outcome: shared::LinkPublication, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.finish_sharing_request(epoch, None, cx) { return; }
        self.entries = self.store.list();
        match outcome.result {
            Err(error) => {
                self.sharing_sheet(Sheet::Members(room), window, cx);
                self.sharing.reply = outcome.reply;
                self.sharing.message = Some((true, settings::diagnostic(&error.to_string())));
            }
            Ok(link) if open => {
                self.close_sharing(window, cx);
                self.navigate_shared(room, None, window, cx);
                // Navigation can replace the workspace; report success in the destination.
                if let Some(handle) = window.window_handle().downcast::<Workspace>() {
                    cx.defer(move |cx| { let _ = handle.update(cx, |w, _, cx| w.public_link_feedback(link, cx)); });
                }
            }
            Ok(link) => {
                self.sharing.reply = outcome.reply;
                self.public_link_feedback(link, cx);
            }
        }
    }

    fn public_link_feedback(&mut self, link: Option<String>, cx: &mut Context<Self>) {
        if let Some(link) = link { self.copy_link_to_clipboard(link, true, cx); }
        else { self.show_toast(link_audience(Audience::LinkClosed), cx); }
    }

    pub(crate) fn can_close_link(&self, room: &Room, cx: &App) -> bool {
        room.scope.owner.is_none() && !room.scope.folder && self.sharing_account_ready(cx)
            && (!self.is_shared() || shared::scope(&self.store, cx) == Some(&room.scope))
            && self.sharing.reply.as_ref().is_some_and(|reply| reply.audience.link_active())
    }

    pub(crate) fn confirm_close_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy || !self.sheet_room().is_some_and(|room| self.can_close_link(room, cx)) { return; }
        if self.sharing.close_link_confirm { self.manage_shared_note("closeLink", String::new(), None, window, cx); }
        else { self.sharing.close_link_confirm = true; cx.notify(); }
    }

    fn open_invitation(&mut self, room: Room, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_manage_room(&room, cx) { return; }
        self.sharing_sheet(Sheet::Invite(room), window, cx);
    }

    pub(crate) fn open_member_management(&mut self, room: Room, window: &mut Window, cx: &mut Context<Self>) {
        if room.scope.owner.is_some() { return; }
        #[cfg(target_os = "ios")]
        if !self.save_before(cx, {
            let room = room.clone();
            move |w,window,cx| w.open_member_management(room,window,cx)
        }) { return; }
        self.sharing_sheet(Sheet::Members(room), window, cx);
        self.manage_shared_note("members", String::new(), None, window, cx);
    }

    pub(crate) fn sheet_room(&self) -> Option<&Room> {
        match &self.sharing.sheet { Some(Sheet::Access(room) | Sheet::Members(room) | Sheet::Invite(room) | Sheet::End(room)) => Some(room), _ => None }
    }

    pub(crate) fn render_sharing_sheet(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let sheet = self.sharing.sheet.as_ref()?;
        let busy = self.sharing.busy;
        let invitation = matches!(sheet, Sheet::Create(_) | Sheet::CreateNew(..) | Sheet::Invite(_));
        let feishu = settings::interface_feishu();
        let accent = invitation_accent();
        let title = match sheet { Sheet::MoveInto(..) => tr("移入共享文件夹", "Move into shared folder"), Sheet::RenameCollection(_) => tr("重命名个人文件夹", "Rename personal folder"), Sheet::Create(id) if self.entries.iter().any(|e| &e.id == id && e.meta.folder.is_some()) => tr("共享文件夹", "Share folder"), Sheet::Create(_) => tr("共享文档", "Share document"), Sheet::Accept => tr("接受邀请", "Accept invitation"),
            Sheet::CreateNew(folder, _) => new_shared_title(*folder),
            Sheet::Access(room) => shared_access_title(room.scope.folder),
            Sheet::Members(_) => tr("成员与权限", "Members and access"),
            Sheet::Invite(_) => tr("共享文档", "Share document"),
            Sheet::EndMany(..) => tr("移除共享文档", "Remove shared documents"),
            Sheet::End(room) => self.shared_end_label(room,cx) };
        let mut body = div().id("sharing-sheet-body").track_scroll(&self.sharing.scroll).overflow_y_scroll().flex_1().min_h_0()
            .px(px(24.)).pb(px(20.)).flex().flex_col().gap(px(14.));
        match sheet {
            Sheet::MoveInto(ids, _, room) => {
                body = body.child(caption(crate::localized_format!("将 {} 项移入「{}」。这个文件夹的所有成员都能访问移入的文档及子文件夹。", "Move {} items into “{}”. Everyone with access to this folder can access the moved documents and subfolders.", ids.len(), room.title)));
            }
            Sheet::RenameCollection(_) => {
                body = body.child(TextField::framed(&self.sharing.folder_name, px(360.), window, cx))
                    .child(caption(tr("只有你能看到这个分类。", "Only you can see this folder.")));
            }
            Sheet::EndMany(rooms, personal) => {
                if !personal.is_empty() { body = body.child(caption(crate::localized_format!("将 {} 篇个人文档移到回收站。", "Move {} personal documents to Trash.", personal.len()))); }
                let owned = rooms.iter().filter(|room| room.scope.owner.is_none()).count(); let received = rooms.len() - owned;
                if owned > 0 { body = body.child(caption(crate::localized_format!("删除你创建的 {} 篇共享文档，所有参与者都将无法访问。", "Delete {} documents shared by you. Everyone will lose access.", owned))); }
                if received > 0 { body = body.child(caption(crate::localized_format!("离开别人分享的 {} 篇文档，其他人不受影响。", "Leave {} documents shared with you. Other people keep access.", received))); }
                body = body.child(caption(tr("已下载的内容会保留在你的回收站，可恢复为普通文档。", "Downloaded content stays in your Trash and can be restored as a personal document.")));
                for room in rooms.iter().take(5) { body = body.child(caption(display_title(&room.title))); }
            }
            Sheet::End(room) => {
                body = body.child(div().text_size(px(16.)).child(ui::ui_para(display_title(&room.title))))
                    .child(caption(if self.stopping_original(room, cx) { tr("成员将失去访问权限。最新内容会保留在你的原文件夹中。", "Members will lose access. The latest content stays in your original folder.") } else if room.scope.owner.is_some() {
                        tr("离开后，你将失去访问权限。其他成员的权限不变。", "You will lose access. Other members keep their access.")
                    } else if room.scope.folder {
                        tr("删除后，所有成员都将无法访问这个文件夹及其内容。", "Everyone will lose access to this folder and its contents.")
                    } else {
                        tr("删除后，所有人都将无法访问这篇共享文档。", "After deletion, no one will be able to access this shared document.")
                    }))
                    .when(!self.stopping_original(room, cx) && !room.scope.folder, |d| d.child(caption(tr("已下载的内容会保留在你的回收站，可恢复为普通文档。", "Downloaded content stays in your Trash and can be restored as a personal document."))));
            }
            Sheet::CreateNew(folder, id) => {
                body = body.child(caption(if *folder {
                    tr("受邀成员可以访问文件夹及以后添加的内容。", "Invited members can access this folder and anything added later.")
                } else {
                    tr("受邀成员可以访问这篇新文档。", "Invited members can access this new document.")
                })).child(if id.is_some() {
                    caption(self.sharing.folder_name.read(cx).text.clone()).into_any_element()
                } else {
                    TextField::framed(&self.sharing.folder_name, px(360.), window, cx).w_full().max_w_full().into_any_element()
                }).child(self.invitation_fields(window, cx));
            }
            Sheet::Create(id) => {
                let title = self.entries.iter().find(|entry| &entry.id == id).map(|e| display_title(&e.title)).unwrap_or_default();
                body = body.child(div().truncate().text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).child(title))
                    .child(div().text_size(px(14.)).child(tr("与他人协作", "Collaborate with others")))
                    .child(caption(if self.entries.iter().any(|e| &e.id == id && e.meta.folder.is_some()) {
                        tr("受邀成员可访问这个文件夹及其子文件夹。今后放入的文档也对成员可见。", "Members can access this folder, its subfolders, and items added later.")
                    } else { tr("共享这篇原文。你可以继续从原文件夹打开它。", "Share this document in place. You can still open it from its original folder.") }));
                if self.can_choose_link(id) {
                    body = body.child(div().flex().flex_wrap().gap(px(8.)).children([false, true].into_iter().map(|public_link| {
                        action(if public_link { "share-anyone" } else { "share-invited" },
                            if public_link { tr("任何持有链接的人", "Anyone with the link") } else { link_audience(Audience::Invited) }, !busy)
                            .when(self.sharing.public_link == public_link, |d| d.bg(accent.opacity(0.16)).border_color(accent))
                            .on_click(cx.listener(move |w, _, window, cx| w.set_public_link(public_link, window, cx)))
                    })));
                }
                body = if self.sharing.public_link {
                    body.child(caption(link_audience(Audience::LinkReadOnly))).child(caption(public_link_requirements()))
                } else { body.child(self.invitation_fields(window, cx)) };
                if crate::cloud_restore::local_only() {
                    body = body.child(action("share-enable-cloud", tr("开启 iCloud 同步…", "Enable iCloud sync…"), !busy)
                        .on_click(cx.listener(|w, _, window, cx| { w.close_sharing(window, cx); w.open_settings(settings::Page::Library, cx); })));
                }
                if self.sharing.prepared.is_some() { body = body.child(caption(tr("共享准备已保存，可以在原文上重试。", "Sharing is prepared. Retry on the same document."))); }
                let export = id.clone();
                body = body.child(div().h(px(1.)).bg(Theme::line_divider()))
                    .child(action("share-send-copy", tr("发送静态副本…", "Send a static copy…"), !busy)
                        .on_click(cx.listener(move |w, _, window, cx| { if w.sharing.busy { return; } w.close_sharing(window, cx); w.open_export_md(&export, cx); })))
                    .child(caption(tr("导出 Markdown 副本，接收者不会收到后续更新。", "Export a Markdown copy. Recipients will not receive later updates.")));
            }
            Sheet::Accept => {
                body = body.child(caption(tr("请登录 iCloud 并启用飞记云功能。指定邀请需使用受邀的 Apple 账户。", "Sign in to iCloud and enable Fei Notes cloud access. For an account invitation, use the Apple Account that was invited.")))
                    .child(TextField::framed(&self.sharing.link, px(360.), window, cx).w_full().max_w_full());
            }
            Sheet::Invite(room) => {
                body = body.child(div().flex_none().truncate().text_size(px(16.)).line_height(px(22.)).child(display_title(&room.title)))
                    .child(self.invitation_fields(window, cx));
            }
            Sheet::Access(room) => {
                let (owner,access) = shared::access_summary(room,cx);
                body = body.child(caption(display_title(&room.title)))
                    .child(line(tr("共享来源", "Shared by"),owner.unwrap_or_else(||tr("与你共享", "Shared with you").into()),div()))
                    .child(line(tr("你的权限", "Your access"),role(access),div()));
            }
            Sheet::Members(room) => {
                let allowed = self.can_manage_room(room, cx);
                body = body.child(div().truncate().text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).child(display_title(&room.title)))
                    .child(line(tr("你", "You"), tr("所有者", "Owner"), div()));
                if let Some(reply) = &self.sharing.reply {
                    let invited = reply.audience.can_invite();
                    body = body.child(caption(link_audience(reply.audience)));
                    if reply.audience == Audience::LinkDraft {
                        if self.can_retry_public_link(room, cx) {
                            body = body.child(action("retry-public-link", tr("重试启用并复制链接", "Retry and copy link"), true)
                                .on_click(cx.listener(|w, _, window, cx| w.retry_public_link(window, cx))));
                        }
                        body = body.child(caption(tr("仍可从共享列表打开这篇文档，也可以邀请指定账户。", "You can still open this document from the shared list or invite specific accounts.")));
                    }
                    if invited {
                        body = body.child(action("members-add", tr("添加参与者…", "Add participants…"), allowed && !busy)
                            .on_click(cx.listener(|w, _, window, cx| {
                                if let Some(room) = w.sheet_room().cloned() { w.open_invitation(room, window, cx); }
                            })));
                        if reply.members.is_empty() { body = body.child(caption(tr("尚未邀请其他参与者", "No other participants invited yet"))); }
                    }
                    for (index, member) in reply.members.iter().enumerate() {
                        let id = member.id.clone();
                        let selected = self.sharing.member.as_deref() == Some(&member.id);
                        let status = if member.accepted { tr("已加入", "Joined") } else { tr("待接受", "Invitation pending") };
                        let mark = if invited { ui::icon("chevron-right", 14., Theme::icon()).into_any_element() } else { div().into_any_element() };
                        body = body.child(div().id(SharedString::from(format!("share-member-{}", member.id)))
                            .child(line(member_label(&member.name, &member.id, index), format!("{} · {status}", role(Some(member.permission))), mark))
                            .when(invited, |d| d.cursor_pointer().on_click(cx.listener(move |w, _, _, cx| {
                                if w.sharing.busy { return; }
                                w.sharing.member = if selected { None } else { Some(id.clone()) }; w.sharing.revoke_confirm = false; cx.notify();
                            }))));
                        if invited && selected {
                            let id = member.id.clone(); let remove = id.clone();
                            let next = if member.permission == Access::ReadWrite { Access::ReadOnly } else { Access::ReadWrite };
                            let permission = if next == Access::ReadOnly { tr("改为只读", "Allow viewing") } else { tr("允许编辑", "Allow editing") };
                            let revoke = if self.sharing.revoke_confirm { tr("确认移除访问", "Confirm removal") }
                                else if member.accepted { tr("移除参与者", "Remove participant") } else { tr("取消邀请", "Cancel invitation") };
                            body = body.child(div().flex().flex_wrap().gap(px(8.))
                                .child(action("member-permission", permission, allowed && !busy)
                                    .on_click(cx.listener(move |w, _, window, cx| w.manage_shared_note("permission", id.clone(), Some(next), window, cx))))
                                .child(action("member-remove", revoke, allowed && !busy).text_color(Theme::danger())
                                    .on_click(cx.listener(move |w, _, window, cx| {
                                        w.confirm_shared_removal(remove.clone(), window, cx);
                                    }))));
                        }
                    }
                    if reply.url.is_some() {
                        let copy = room.clone();
                        let label = if invited { tr("复制邀请链接", "Copy invitation link") } else { tr("复制公开链接", "Copy public link") };
                        body = body.child(action("copy-share-link", label, !busy)
                            .on_click(cx.listener(move |w, _, window, cx| w.copy_shared_link(copy.clone(), window, cx))));
                    }
                    if reply.audience.link_active() {
                        let confirming = self.sharing.close_link_confirm;
                        body = body.child(caption(if confirming { close_link_warning() } else { public_link_requirements() }))
                            .child(div().flex().flex_wrap().gap(px(8.))
                                .child(action("close-public-link", if confirming { tr("确认关闭链接", "Confirm close link") } else { tr("关闭公开链接…", "Close public link…") },
                                    !busy && self.can_close_link(room, cx)).text_color(Theme::danger())
                                    .on_click(cx.listener(|w, _, window, cx| w.confirm_close_link(window, cx))))
                                .when(confirming, |d| d.child(action("cancel-close-public-link", tr("取消", "Cancel"), !busy)
                                    .on_click(cx.listener(|w, _, _, cx| { if !w.sharing.busy { w.sharing.close_link_confirm = false; cx.notify(); } })))));
                    } else if reply.audience == Audience::Invited {
                        body = body.child(caption(private_link_requirements()));
                    }
                }
                let remove = room.clone();
                body = body.child(action("members-delete-document", self.shared_end_label(room,cx), !busy)
                    .text_color(Theme::danger()).on_click(cx.listener(move |w, _, window, cx| w.open_end_shared(remove.clone(), window, cx))));
                body = body.child(action("members-refresh", tr("刷新成员", "Refresh members"), !busy)
                        .on_click(cx.listener(|w, _, window, cx| w.manage_shared_note("members", String::new(), None, window, cx))));
            }
        }
        if !matches!(sheet, Sheet::RenameCollection(_)) && !self.sharing_allowed(cx) {
            body = body.child(caption(tr("协作已暂停。请在文档库设置中检查 iCloud 账户和云功能。", "Collaboration is paused. Check your iCloud account and cloud access in Documents settings.")));
        }
        let feedback = div().px(px(24.)).flex_none()
            .when(busy, |d| d.child(caption(tr("正在连接 iCloud，请稍候…", "Contacting iCloud. Please wait…"))).pb(px(10.)))
            .when_some(self.sharing.message.as_ref().filter(|(error, _)| *error), |d, (error, message)| d.pb(px(10.)).child(
                div().id("sharing-message").text_size(px(13.)).line_height(px(20.))
                    .text_color(if *error { Theme::danger() } else { Theme::text_caption() }).child(ui::ui_para(message.clone()))));
        let submit = match sheet { Sheet::MoveInto(..) => Some(tr("移入并共享", "Move and share")), Sheet::RenameCollection(_) => Some(tr("保存", "Save")), Sheet::Create(_) if self.sharing.public_link => Some(tr("创建并复制链接", "Create and copy link")), Sheet::Create(_) | Sheet::CreateNew(..) => Some(tr("创建并邀请", "Create and invite")), Sheet::Accept => Some(tr("接受并打开", "Accept and open")),
            Sheet::Invite(_) => Some(tr("邀请", "Invite")), Sheet::EndMany(..) => Some(tr("确认移除", "Remove documents")), Sheet::End(room) => Some(self.shared_end_label(room,cx)), _ => None };
        let input_ok = if matches!(sheet, Sheet::RenameCollection(_)) { !self.sharing.folder_name.read(cx).text.trim().is_empty() } else if matches!(sheet, Sheet::End(_) | Sheet::EndMany(..) | Sheet::MoveInto(..)) { true } else if matches!(sheet, Sheet::Accept) { !self.sharing.link.read(cx).text.trim().is_empty() }
            else if let Sheet::CreateNew(_, id) = sheet {
                (id.is_some() || !self.sharing.folder_name.read(cx).text.trim().is_empty()) && !self.sharing.email.read(cx).text.trim().is_empty()
            } else { matches!(sheet, Sheet::Create(_)) && self.sharing.public_link || !self.sharing.email.read(cx).text.trim().is_empty() };
        let footer = div().flex_none().px(px(24.)).py(px(14.)).flex().justify_end().gap(px(8.)).border_t_1().border_color(Theme::line_divider())
            .children(matches!(sheet, Sheet::Invite(_)).then(|| action("invite-back", tr("成员与权限", "Members and access"), !busy)
                .on_click(cx.listener(|w, _, window, cx| { if let Some(room) = w.sheet_room().cloned() { w.open_member_management(room, window, cx); } }))))
            .child(action("sharing-close", if submit.is_some() { tr("取消", "Cancel") } else { tr("完成", "Done") }, !busy)
                .on_click(cx.listener(|w, _, window, cx| w.close_sharing(window, cx))))
            .children(submit.map(|label| (if invitation {
                    div().id("sharing-submit").h(px(30.)).px(px(16.)).rounded(px(if feishu { 6. } else { 9. }))
                        .flex().items_center().justify_center().cursor_pointer().bg(accent)
                        .text_color(cloud_sync::presence::badge_ink(accent)).text_size(px(13.)).font_weight(FontWeight::SEMIBOLD)
                        .hover(move |s| s.bg(accent.opacity(0.88))).active(move |s| s.bg(accent.opacity(0.76))).child(label)
                } else { ui::primary_button("sharing-submit", label) })
                .when(busy || !input_ok || !(if matches!(sheet, Sheet::RenameCollection(_)) { true } else if matches!(sheet, Sheet::End(_) | Sheet::EndMany(..)) { self.sharing_account_ready(cx) } else { self.sharing_allowed(cx) }), |d| d.opacity(0.45).cursor_default())
                .on_click(cx.listener(|w, _, window, cx| w.submit_sharing(window, cx)))));
        if matches!(sheet, Sheet::Accept) && let Some(bounds) = self.sharing.accept_at.and_then(|key| self.home.anchor(key)) {
            let pos = point(bounds.left(), bounds.bottom() + px(4.));
            let form = ui::popover().id("accept-invitation-popover").occlude().w(px(400.)).max_w(window.viewport_size().width - px(16.))
                .max_h(window.viewport_size().height - px(32.)).flex().flex_col()
                .on_mouse_down_out(cx.listener(|w, event: &MouseDownEvent, window, cx| {
                    if !w.accept_invitation_open() { return; }
                    if ["accept-home", "accept-sidebar"].iter().any(|key| w.home.anchor(key).is_some_and(|b| b.contains(&event.position))) { return; }
                    w.close_sharing(window, cx);
                }))
                .child(div().px(px(24.)).py(px(20.)).text_size(px(17.)).font_weight(FontWeight::SEMIBOLD).child(title))
                .child(body).child(feedback).child(footer);
            return Some(deferred(anchored().anchor(Anchor::TopLeft).position(pos).snap_to_window_with_margin(px(8.)).child(form)).with_priority(5).into_any_element());
        }
        if invitation && feishu && self.view == View::Doc
            && let Some(bounds) = self.sharing.share_bounds.get()
        {
            let available = (window.viewport_size().height - bounds.bottom() - px(12.)).max(px(100.));
            let height = if matches!(sheet, Sheet::Create(_)) { 486. } else { 335. };
            let form = ui::popover().id("share-document-popover").occlude().flex().flex_col()
                .w(px(480.)).max_w(window.viewport_size().width - px(16.)).h(available.min(px(height)))
                .child(div().px(px(24.)).py(px(16.)).text_size(px(17.)).font_weight(FontWeight::SEMIBOLD).child(title))
                .child(body).child(feedback).child(footer);
            // Consume the dismissing click so toolbar actions cannot reopen the form.
            let popup = anchored().anchor(Anchor::TopRight).position(point(bounds.right(), bounds.bottom() + px(4.)))
                .snap_to_window_with_margin(px(8.)).child(form);
            return Some(deferred(div().absolute().inset_0()
                .child(div().id("share-dismiss").absolute().inset_0().occlude()
                    .on_mouse_down(MouseButton::Left, cx.listener(|w, _, window, cx| { cx.stop_propagation(); w.close_sharing(window, cx); })))
                .child(popup)).with_priority(5).into_any_element());
        }
        let height = match sheet { Sheet::MoveInto(..) => 250., Sheet::RenameCollection(_) => 235., Sheet::Create(_) | Sheet::CreateNew(..) => 510., Sheet::Accept | Sheet::Access(_) => 290., Sheet::Members(_) => 580., Sheet::Invite(_) => 345., Sheet::End(_) => 290., Sheet::EndMany(..) => 380. };
        let modal = ui::dialog("sharing-sheet").h((window.viewport_size().height - px(40.)).min(px(height))).w((window.viewport_size().width - px(32.)).min(px(if invitation && !feishu { 440. } else { 520. })))
            .max_h(window.viewport_size().height - px(40.)).flex().flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(div().px(px(24.)).py(px(18.)).flex().items_center().gap(px(9.))
                .when(invitation && !feishu, |d| d.child(div().size(px(30.)).rounded(px(8.)).bg(accent.opacity(0.16))
                    .flex().items_center().justify_center().child(ui::icon("share", 18., Theme::text()))))
                .text_size(px(17.)).font_weight(FontWeight::SEMIBOLD).child(title))
            .child(body).child(feedback).child(footer);
        Some(deferred(div().id("sharing-mask").absolute().inset_0().occlude().flex().items_center().justify_center().bg(ui::scrim())
            .on_mouse_down(MouseButton::Left, cx.listener(|w, _, window, cx| { cx.stop_propagation(); w.close_sharing(window, cx); }))
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation()).child(modal)).with_priority(5).into_any_element())
    }

    fn invitation_fields(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let enabled = !self.sharing.busy;
        div().flex_none().flex().flex_col().gap(px(10.))
            .child(caption(tr("邀请指定 Apple 账户", "Invite a specific Apple Account")))
            .child(TextField::framed(&self.sharing.email, px(360.), window, cx).w_full().max_w_full()
                .when(self.sharing.email.read(cx).is_focused(window), |d| d.border_color(invitation_accent())))
            .child(div().flex().gap(px(8.)).children([Access::ReadOnly, Access::ReadWrite].into_iter().map(|access| {
                action(if access == Access::ReadOnly { "invite-reader" } else { "invite-writer" }, role(Some(access)), enabled)
                    .when(self.sharing.permission == access, |d| d.bg(invitation_accent().opacity(0.16)).border_color(invitation_accent()).text_color(Theme::text()))
                    .on_click(cx.listener(move |w, _, _, cx| { if !w.sharing.busy { w.sharing.permission = access; cx.notify(); } }))
            })))
            .child(caption(tr("只有这个账户可使用邀请链接。请把链接发给对方，飞记不会自动发送通知。", "Only this account can use the invitation link. Send it to them; Fei Notes does not send a notification automatically.")))
    }

    pub(crate) fn confirm_shared_removal(&mut self, member: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(room) = self.sheet_room() else { return };
        if self.sharing.busy || !self.can_manage_room(room, cx) || self.sharing.member.as_deref() != Some(&member) { return; }
        if !self.sharing.revoke_confirm { self.sharing.revoke_confirm = true; cx.notify(); }
        else { self.manage_shared_note("revoke", member, None, window, cx); }
    }

    pub(crate) fn submit_sharing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy { return; }
        if let Some(Sheet::MoveInto(ids, target, room)) = self.sharing.sheet.clone() {
            if !self.sharing_allowed(cx) || !self.flush(cx) { return; }
            let personal = self.store.root().to_owned();
            let epoch = self.begin_sharing_request(cx);
            cx.spawn_in(window, async move |this, cx| {
                let result = cx.background_executor().spawn(async move { shared::original::move_into(&personal, &room, &ids, &target) }).await;
                let _ = this.update_in(cx, |ws, window, cx| { if ws.finish_sharing_request(epoch, result.err(), cx) && ws.sharing.message.is_none() {
                    ws.entries = ws.store.list(); ws.close_sharing(window, cx); cloud_sync::retry(ws.store.root(), cx);
                } });
            }).detach();
            return;
        }
        if let Some(Sheet::RenameCollection(id)) = self.sharing.sheet.clone() { self.save_collection_name(&id, window, cx); return; }
        if let Some(Sheet::EndMany(rooms, personal)) = self.sharing.sheet.clone() {
            if self.sharing_account_ready(cx) { self.end_shared_documents(rooms, personal, window, cx); }
            return;
        }
        if let Some(Sheet::End(room)) = self.sharing.sheet.clone() {
            if self.sharing_account_ready(cx) { self.end_shared_document(room, window, cx); }
            return;
        }
        if !self.sharing_allowed(cx) { return; }
        match self.sharing.sheet.clone() {
            Some(Sheet::Create(id)) => self.publish_shared_copy(&id, window, cx),
            Some(Sheet::CreateNew(folder, id)) => self.submit_new_shared_item(folder, id, window, cx),
            Some(Sheet::Accept) => self.accept_shared_note(window, cx),
            Some(Sheet::Invite(_)) => {
                let email = self.sharing.email.read(cx).text.trim().to_string();
                if !email.is_empty() { self.manage_shared_note("invite", email, Some(self.sharing.permission), window, cx); }
            }
            _ => {}
        }
    }

    fn begin_sharing_request(&mut self, cx: &mut Context<Self>) -> u64 {
        self.sharing.busy = true; self.sharing.message = None; self.sharing.epoch += 1; cx.notify(); self.sharing.epoch
    }

    fn finish_sharing_request(&mut self, epoch: u64, error: Option<anyhow::Error>, cx: &mut Context<Self>) -> bool {
        if self.sharing.epoch != epoch { return false; }
        self.sharing.busy = false;
        if let Some(error) = error { self.sharing.message = Some((true, settings::diagnostic(&error.to_string()))); }
        cx.notify(); true
    }

    pub(crate) fn open_shared_note(&mut self, room: &Room, window: &mut Window, cx: &mut Context<Self>) {
        self.open_shared_item_interactive(room.clone(), room.scope.note.clone(), None, window, cx);
    }

    pub(crate) fn accept_shared_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let link = self.sharing.link.read(cx).text.trim().to_owned();
        if link.is_empty() { return; }
        let epoch = self.begin_sharing_request(cx);
        let root = crate::store::Store::root_dir();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_executor().spawn(async move { shared::accept(&root, &link) }).await;
            let _ = this.update_in(cx, |w, window, cx| match result {
                Ok(room) => {
                    if w.finish_sharing_request(epoch, None, cx) { w.close_sharing(window, cx); w.navigate_shared(room, None, window, cx); }
                }
                Err(error) => { w.finish_sharing_request(epoch, Some(error), cx); }
            });
        }).detach();
    }

    pub(crate) fn manage_shared_note(&mut self, action: &'static str, person: String, access: Option<Access>, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy { return; }
        let Some(room) = self.sheet_room().cloned() else { return };
        if action == "members" {
            if room.scope.owner.is_some() { return; }
        } else if action == "closeLink" {
            if !self.sharing.close_link_confirm || !self.can_close_link(&room, cx) { return; }
        } else if !self.can_manage_room(&room, cx) { return; }
        let epoch = self.begin_sharing_request(cx);
        let root = crate::store::Store::root_dir();
        cx.spawn_in(window, async move |this, cx| {
            let request_room = room.clone();
            let result = cx.background_executor().spawn(async move { shared::manage(&root, &request_room, action, &person, access) }).await;
            let _ = this.update_in(cx, |w, window, cx| match result {
                Ok(reply) => {
                    if !w.finish_sharing_request(epoch, None, cx) { return; }
                    let link_closed = reply.audience == Audience::LinkClosed;
                    w.sharing.reply = Some(reply); w.sharing.member = None; w.sharing.revoke_confirm = false;
                    w.sharing.close_link_confirm = false;
                    if action == "closeLink" {
                        if link_closed { w.show_toast(link_audience(Audience::LinkClosed), cx); }
                        else { w.sharing.message = Some((true, tr("iCloud 尚未确认关闭链接，请刷新后重试。", "iCloud has not confirmed closure. Refresh and try again.").into())); }
                    }
                    if action == "invite" { w.show_invitation_result(room, window, cx); }
                }
                Err(error) => { w.finish_sharing_request(epoch, Some(error), cx); }
            });
        }).detach();
    }

    fn show_invitation_result(&mut self, room: Room, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.original.is_some() && self.sharing.copied_text.is_none()
            && let Some(link) = self.sharing.reply.as_ref().and_then(|reply| reply.url.as_deref()).and_then(|url| links::branded(url).ok()) {
            let text = invitation_text(&room.title, &link);
            if !crate::devtools::scripted() { cx.write_to_clipboard(ClipboardItem::new_string(text.clone())); }
            self.sharing.copied_text = Some(text);
        }
        self.sharing.sheet = Some(Sheet::Members(room));
        crate::gpui_compat::focus(window, &self.focus, cx);
        self.sharing.email.update(cx, |field, cx| field.set_text("", cx));
        self.show_toast(if self.sharing.copied_text.is_some() { tr("邀请文案和链接已复制", "Invitation message and link copied") }
            else { tr("邀请已准备好，请复制链接发给对方。", "Invitation ready. Copy the link and send it.") }, cx);
    }

    pub(crate) fn publish_shared_copy(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let email = self.sharing.email.read(cx).text.trim().to_owned();
        let public_link = shared::public_link_creation_enabled() && self.sharing.public_link;
        if (!public_link && email.is_empty()) || !self.flush(cx) { return; }
        let Some(title) = self.personal_navigation_entries(cx).iter().find(|entry| entry.id == id).map(|entry| display_title(&entry.title)) else { return };
        let root = crate::store::Store::root_dir();
        let prepared = self.sharing.prepared.clone();
        let permission = self.sharing.permission;
        let original = id.to_owned();
        let epoch = self.begin_sharing_request(cx);
        cx.spawn_in(window, async move |this, cx| {
            let copy_root = root.clone();
            let source = original.clone();
            let staged = cx.background_executor().spawn(async move {
                prepared.map_or_else(|| shared::original::prepare(&copy_root, &source, title, public_link), Ok)
            }).await;
            let room = match staged {
                Ok(room) => room,
                Err(error) => { let _ = this.update_in(cx, |w, _, cx| { w.finish_sharing_request(epoch, Some(error), cx); }); return; }
            };
            // Keep the room before publishing. A later upload failure retries this exact copy.
            if this.update_in(cx, |w, _, _| { w.sharing.prepared = Some(room.clone()); }).is_err() { return; }
            let upload_room = room.clone();
            let upload_root = root.clone();
            let uploaded = cx.background_executor().spawn(async move {
                // A draft staged before the pause can still finish as an account invitation.
                anyhow::ensure!(!shared::public_link_creation_enabled()
                    || public_link == shared::public_link_pending(&upload_root, &upload_room),
                    "{}", tr("共享范围已改变，请重新打开共享面板。", "The access scope changed. Reopen the sharing panel."));
                shared::ensure_created(&upload_root, &upload_room)
            }).await;
            if let Err(error) = uploaded {
                let _ = this.update_in(cx, |w, _, cx| { w.finish_sharing_request(epoch, Some(error), cx); });
                return;
            }
            if public_link {
                let target = room.clone();
                let outcome = cx.background_executor().spawn(async move { shared::publish_link(&root, &target) }).await;
                let _ = this.update_in(cx, |w, window, cx| w.finish_public_link(epoch, room, outcome, true, window, cx));
            } else {
                let _ = this.update_in(cx, |w, window, cx| {
                    if !w.finish_sharing_request(epoch, None, cx) { return; }
                    w.close_sharing(window, cx);
                    w.entries = w.store.list();
                    w.sharing.original = Some(original);
                    w.navigate_shared(room, Some((email, permission)), window, cx);
                });
            }
        }).detach();
    }
}
