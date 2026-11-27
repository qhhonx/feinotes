//! Shared-note navigation, creation and contextual access management.
//! Cloud libraries and invitation permissions remain owned by cloud_sync::shared.

use crate::{app::{View, Workspace}, cloud_sync::{self, shared::{self, Access, Audience, NewSharedItem, Reply, Room}},
    input::{TextField, TextFieldEvent}, settings::{self, tr, display_title}, theme::Theme, ui::{self, TipExt, TitleBarExt}};
use gpui::{prelude::FluentBuilder, *};
#[cfg(target_os = "ios")]
pub(crate) mod mobile;

#[cfg(feature = "devtools")]
pub(crate) mod qa;
#[cfg(feature = "devtools")]
mod qa_parity;
mod navigation;
mod object_link;
#[cfg(target_os = "macos")]
pub(crate) use navigation::SharedFolderEdit;
mod lifecycle;
pub(crate) use lifecycle::SharedTrashAction;
mod list;
mod sidebar;
#[cfg(target_os = "macos")]
pub(crate) use sidebar::SharedFolderInsertion;
pub(crate) mod organizer;
pub(crate) mod preferences;
mod actions;
pub(crate) mod links;

#[derive(Clone)]
pub(crate) enum Sheet { MoveInto(Vec<String>, String, Room), RenameCollection(String), Create(String), CreateNew(NewSharedItem, Option<String>), Accept, Access(Room), Members(Room), Invite(Room), End(Room), EndMany(Vec<Room>, Vec<String>) }

#[cfg(target_os = "ios")]
pub(crate) struct OpeningShared { pub target: String, pub title: String }

pub(crate) fn new_shared_title(kind: NewSharedItem) -> &'static str {
    match kind {
        NewSharedItem::Document => tr("新建共享文档", "New shared document"),
        NewSharedItem::Board => tr("新建共享画板", "New shared board"),
        NewSharedItem::Folder => tr("新建共享文件夹", "New shared folder"),
    }
}

pub(crate) fn shared_access_title(folder: bool) -> &'static str {
    if folder { tr("文件夹共享权限", "Folder sharing access") }
    else { tr("文档共享权限", "Document sharing access") }
}

const LOADING_NOTICE_DELAY: std::time::Duration = std::time::Duration::from_millis(500);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublicLinkFailure { Region, Other }

pub(crate) struct Sharing {
    #[cfg(target_os = "ios")]
    pub(crate) opening: Option<OpeningShared>,
    #[cfg(target_os = "ios")]
    pub(crate) mobile_invitation_result: bool,
    invitation_parent: Option<(u64, Reply)>,
    pub(crate) public_link_failure: Option<PublicLinkFailure>,
    #[cfg(target_os = "ios")]
    mobile_access_lookup: mobile::AccessLookup,
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
    pub(crate) open_notice: Option<&'static str>,
    pub(crate) busy_feedback: Option<&'static str>,
    pub(crate) sidebar_load: Option<uuid::Uuid>,
    #[cfg(target_os = "macos")]
    pub(crate) sidebar_waiting: Option<String>,
    pub(crate) copied_text: Option<String>,
    #[cfg(target_os = "ios")]
    pub(crate) mobile_toast: Option<mobile::SharingToast>,
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
            #[cfg(target_os = "ios")]
            mobile_toast: None,
            #[cfg(target_os = "ios")]
            mobile_invitation_result: false,
            invitation_parent: None,
            public_link_failure: None,
            #[cfg(target_os = "ios")]
            mobile_access_lookup: mobile::AccessLookup::Idle,
            sidebar_load: None,
            #[cfg(target_os = "macos")]
            sidebar_waiting: None,
            link: cx.new(|cx| TextField::new(tr("粘贴飞记邀请链接", "Paste a Fei Notes invitation link"), cx)),
            collection: None, folder_name: cx.new(|cx| TextField::new(tr("名称", "Name"), cx)),
            email: cx.new(|cx| TextField::new(tr("对方的 Apple 账户邮箱", "Their Apple Account email"), cx)),
            sheet: None, reply: None, prepared: None, public_link: false, close_link_confirm: false, permission: Access::ReadOnly, member: None, revoke_confirm: false,
            busy: false, message: None, epoch: 0, original: None, pending_open: None, account_open: None, open_notice: None, busy_feedback: None, copied_text: None, focus_before: None, subscriptions: vec![],
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
    let title = display_title(title);
    let title = title.replace(['\r', '\n'], " ");
    format!("{}\n{link}", crate::localized_format!("邀请你在飞记中协作《{}》", "Join me in Fei Notes: {}", title))
}

pub(crate) fn create_invitation_label() -> &'static str {
    tr("创建邀请", "Create invitation")
}

pub(crate) fn copy_invitation_label(public: bool) -> &'static str {
    if public { tr("复制公开邀请", "Copy public invitation") } else { tr("复制邀请", "Copy invitation") }
}

pub(crate) fn enable_public_link_label(failure: Option<PublicLinkFailure>) -> &'static str {
    if failure == Some(PublicLinkFailure::Other) { tr("重试启用", "Retry enabling link") }
    else { tr("启用公开链接", "Enable public link") }
}

pub(crate) fn invite_specific_account_label() -> &'static str {
    tr("邀请指定账户", "Invite specific accounts")
}

pub(crate) fn link_audience(audience: Audience) -> &'static str {
    match audience {
        Audience::Invited => tr("仅受邀者", "Only invited people"),
        Audience::LinkDraft => invited_link_scope(),
        Audience::LinkReadOnly => tr("任何持有链接的人 · 仅查看", "Anyone with the link · View only"),
        Audience::LinkReadWrite => tr("任何持有链接的人 · 可编辑", "Anyone with the link · Can edit"),
        Audience::LinkClosed => tr("公开链接已关闭", "Public link closed"),
    }
}

pub(crate) fn public_link_requirements() -> &'static str {
    tr("链接可转发。任何持有链接的人都可在飞记中加入。",
        "Anyone with the link can join in Fei Notes, including people it is forwarded to.")
}

pub(crate) fn invitation_account_requirements() -> &'static str {
    tr("指定账户邀请请使用受邀的 Apple 账户。",
        "For an account invitation, use the Apple Account that was invited.")
}

pub(crate) fn public_edit_warning(folder: bool) -> &'static str {
    if folder { tr("任何持链接加入的人都可以修改此文件夹中的内容。", "Anyone who joins with the link can edit the contents of this folder.") }
    else { tr("任何拿到链接并加入的人都可以修改这篇文档。", "Anyone who gets the link and joins can edit this document.") }
}

pub(crate) fn invited_link_scope() -> &'static str {
    tr("仅受邀的 Apple 账户", "Only invited Apple Accounts")
}

pub(crate) fn private_invitation_delivery() -> &'static str {
    tr("创建邀请后，请把链接发给对方。链接仅对受邀账户有效。",
        "After creating the invitation, send the link to the recipient. It only works for the invited account.")
}

pub(crate) fn sharing_target_label(folder: bool) -> &'static str {
    if folder { tr("文件夹", "Folder") } else { tr("文档", "Document") }
}

pub(crate) fn sharing_folder_scope() -> &'static str {
    tr("成员可访问整个文件夹，包括子文件夹和今后加入的内容。",
        "Members can access the entire folder, including subfolders and anything added later.")
}

pub(crate) fn close_link_warning() -> &'static str {
    tr("通过此链接加入的人将失去访问权限。已下载或导出的副本仍可能保留。",
        "People who joined through this link will lose access. Downloaded or exported copies may remain.")
}

pub(crate) fn closed_link_detail(has_original: bool) -> Option<&'static str> {
    has_original.then(|| tr("如需创建新邀请，请先停止共享，再从原件发起共享。",
        "To create a new invitation, stop sharing, then share the original again."))
}

fn shared_trash_description() -> &'static str {
    tr("已下载的内容会保留在你的回收站，可恢复为普通文档。", "Downloaded content stays in your Trash and can be restored as a personal document.")
}

fn caption(text: impl Into<SharedString>) -> Div {
    div().w_full().min_w_0().text_size(px(13.)).line_height(px(20.)).text_color(Theme::text_caption()).child(ui::ui_para(text.into()))
}

fn sharing_error(message: &str) -> Stateful<Div> {
    div().id("sharing-message").text_size(px(13.)).line_height(px(20.))
        .text_color(crate::design::color::destructive()).child(ui::ui_para(message.to_owned()))
}

fn action(id: impl Into<ElementId>, label: &'static str, enabled: bool) -> crate::design::Button {
    crate::design::Button::outline(id).sized(crate::design::ButtonSize::Sm).label(label).disabled(!enabled)
}

/// Sharing toolbar control for both interface styles.
fn sharing_toolbar_button(window: &Window, id: &'static str, glyph: &'static str, label: Option<&'static str>, open: bool, primary: bool) -> Stateful<Div> {
    let accent = invitation_accent();
    let (fill, hover, pressed, ink) = if primary {
        (accent, accent.blend(gpui::black().opacity(0.09)), accent.blend(gpui::black().opacity(0.16)), white())
    } else {
        (Theme::fill_hover(), Theme::fill_pressed(), Theme::fill_pressed(), Theme::text())
    };
    let size = px(crate::app::HEADER_ACTIONS_H);
    div().id(id).no_drag().flex_none().h(size).min_w(size).rounded(px(6.))
        .flex().items_center().justify_center().gap(px(5.))
        .bg(if open { hover } else { fill }).text_color(ink)
        .text_size(px(14.)).font_weight(FontWeight::NORMAL).line_height(px(20.))
        .child(ui::icon(glyph, 16., ink))
        .when_some(label, |d, label| {
            let style = window.text_style().highlight(HighlightStyle { font_weight: Some(FontWeight::NORMAL), ..Default::default() });
            let label_width = [tr("共享", "Share"), tr("已共享", "Shared")].into_iter()
                .map(|text| window.text_system().shape_line(text.into(), px(14.), &[style.to_run(text.len())], None).width)
                .fold(px(0.), Pixels::max);
            d.w(label_width + px(35.)).whitespace_nowrap().child(label)
        })
        .cursor_pointer().hover(move |s| s.bg(hover)).active(move |s| s.bg(pressed))
}

fn invitation_accent() -> Hsla { crate::design::color::control_fill() }

fn sharing_panel_title(title: &'static str) -> Div {
    div().min_w_0().text_size(px(17.)).font_weight(FontWeight::SEMIBOLD).child(ui::ui_para(title))
}

fn sharing_popover(id: &'static str) -> crate::design::Popover {
    crate::design::Popover::new(id)
        .bg(crate::theme::raised(Theme::popover_bg())).text_color(Theme::text())
}

fn anchor_control(button: impl IntoElement, bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>) -> Div {
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

fn participant_row(name: impl Into<SharedString>, role: &'static str, pending: bool, controls: impl IntoElement) -> Div {
    div().w_full().min_h(px(36.)).py(px(6.)).flex().items_center().gap(px(8.)).border_b_1().border_color(Theme::line_divider())
        .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.))
            .child(div().truncate().text_size(px(14.)).child(name.into()))
            .when(pending, |d| d.child(div().text_size(px(12.)).text_color(Theme::text_caption()).child(tr("待接受", "Invitation pending")))))
        .child(div().min_w_0().max_w(relative(0.5)).text_size(px(13.)).text_color(crate::design::color::muted_foreground()).child(ui::ui_para(role)))
        .child(div().flex_none().w(px(14.)).child(controls))
}

fn member_removal_label(accepted: bool) -> &'static str {
    if accepted { tr("移除参与者", "Remove participant") } else { tr("取消邀请", "Cancel invitation") }
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

    pub(crate) fn shared_end_effect(&self, room: &Room, cx: &App) -> &'static str {
        match (self.stopping_original(room,cx),room.scope.folder,room.scope.owner.is_some()) {
            (true,true,_) => tr("成员将失去这个文件夹及其内容的访问权限，已开启的公开链接也会失效。你保留原文件夹和最新内容。", "Members lose access to the folder and its contents. Any public link stops working. You keep the original folder and its latest contents."),
            (true,false,_) => tr("成员将失去访问权限，已开启的公开链接也会失效。你保留原文及最新内容。", "Members lose access. Any public link stops working. You keep the original document and its latest contents."),
            (_,true,true) => tr("你将失去这个文件夹及其内容的访问权限。其他成员的权限不变。", "You lose access to the folder and its contents. Other members keep their access."),
            (_,false,true) => tr("你将失去这篇文档的访问权限。其他成员的权限不变。", "You lose access to the document. Other members keep their access."),
            (_,true,false) => tr("删除后，所有成员都将失去这个文件夹及其内容的访问权限。", "Deleting the folder removes everyone's access to it and its contents."),
            (_,false,false) => tr("删除后，所有成员都将失去这篇文档的访问权限。", "Deleting the document removes everyone's access to it."),
        }
    }

    fn sharing_folder_contains_current_item(&self, room: &Room, cx: &App) -> bool {
        room.scope.folder && shared::scope(&self.store,cx) == Some(&room.scope)
            && self.current.as_ref().is_some_and(|id| id != &room.scope.note)
    }

    pub(crate) fn sharing_scope_detail(&self, room: &Room, cx: &App) -> Option<&'static str> {
        if self.sharing_folder_contains_current_item(room,cx) {
            Some(tr("当前项目的访问权限由此文件夹决定。", "This folder controls access to the current item."))
        } else { room.scope.folder.then(sharing_folder_scope) }
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
        self.sharing.invitation_parent = None;
        self.sharing.epoch += 1;
        self.sharing.subscriptions.clear();
        if let Some(focus) = self.sharing.focus_before.take() { crate::gpui_compat::focus(window,&focus,cx); }
        #[cfg(target_os = "ios")]
        if self.view == View::Settings && self.settings_page == settings::Page::Shared { self.mobile_close_settings(window,cx); }
        cx.notify();
    }

    pub(crate) fn sharing_sheet(&mut self, sheet: Sheet, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy { return; }
        self.cancel_shared_open();
        #[cfg(target_os = "ios")]
        {
            self.mobile_present_sharing(cx);
            self.sharing.mobile_invitation_result = false;
            self.sharing.mobile_access_lookup = mobile::AccessLookup::Idle;
        }
        self.sharing.invitation_parent = None;
        if self.sharing.sheet.is_none() { self.sharing.focus_before = window.focused(cx); }
        self.sharing.epoch += 1;
        self.sharing.sheet = Some(sheet);
        self.sharing.accept_at = None;
        self.sharing.panel_open = false;
        self.sharing.sync_panel_open = false;
        self.sharing.prepared = None;
        self.sharing.public_link = false;
        self.sharing.public_link_failure = None;
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

    pub(crate) fn sharing_account_ready(&self, cx: &App) -> bool {
        !crate::cloud_restore::local_only() && cloud_sync::status(self.sharing_account_root(cx), cx).is_some_and(|s| !matches!(s.phase, cloud_sync::SyncPhase::AccountChecking | cloud_sync::SyncPhase::AccountChanged | cloud_sync::SyncPhase::SignedOut))
    }

    fn sharing_account_root<'a>(&'a self, _cx: &'a App) -> &'a std::path::Path {
        #[cfg(target_os = "macos")]
        if matches!(self.sharing.sheet, Some(Sheet::CreateNew(..)))
            && let Some(personal) = _cx.try_global::<crate::shared_store::SharedStore>() { return personal.root(); }
        #[cfg(target_os = "ios")]
        if self.is_shared() && (matches!(self.sharing.sheet, Some(Sheet::Create(_)))
            || self.sheet_room().is_some_and(|room| shared::scope(&self.store,_cx) != Some(&room.scope))) {
            return _cx.global::<crate::shared_store::SharedStore>().root();
        }
        self.store.root()
    }

    pub(crate) fn sharing_allowed(&self, cx: &App) -> bool {
        self.sharing_account_ready(cx) && cloud_sync::status(self.sharing_account_root(cx), cx).is_some_and(|status| !status.access_paused)
    }

    pub(crate) fn can_manage_room(&self, room: &Room, cx: &App) -> bool {
        if room.scope.owner.is_some() { return false; }
        #[cfg(target_os = "macos")]
        if self.is_shared() && shared::scope(&self.store, cx) != Some(&room.scope) {
            return self.shared_list_room(&room.scope.note, cx).is_some_and(|listed| listed.scope == room.scope)
                && shared::room_account(&crate::store::Store::root_dir(), cx) == shared::RoomAccount::Ready;
        }
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

    pub(crate) fn sharing_status_notice(&self) -> Option<&'static str> {
        self.sharing.open_notice.or(self.sharing.busy_feedback.filter(|_| self.sharing.sheet.is_none()))
    }

    pub(crate) fn receive_invitation(&mut self, link: String, window: &mut Window, cx: &mut Context<Self>) {
        if link.starts_with("https://feinotes.app/o/") {
            self.receive_object_link(&link, window, cx);
            return;
        }
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
            let button = sharing_toolbar_button(window, "doc-create-share", "share",
                self.header_action_labels(window).then(|| tr("共享", "Share")), false, true);
            return Some(self.anchor_share_control(button.tip(tr("共享", "Share"), None)
                .on_click(cx.listener(move |w, _, window, cx| w.create_shared_note(&id, window, cx)))));
        }
        let people = self.online_collaborators(cx);
        let cap = if window.viewport_size().width < px(500.) { 0 } else { 2 };
        let bounds = self.sharing.toolbar_bounds.clone();
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
        Some(div().flex().flex_none().items_center().gap(px(10.))
            .when(!people.is_empty(), |d| d.child(anchor_control(presence, bounds)))
            .children(self.shared_access_button(window, cx)))
    }

    pub(crate) fn shared_access_button(&self, window: &Window, cx: &mut Context<Self>) -> Option<Div> {
        let owner = self.current_room(cx)?.scope.owner.is_none();
        let attention = settings::sync_status::needs_attention(self,cx);
        let open = self.sharing.sync_panel_open || self.sharing.panel_open && self.sharing.permissions_only;
        let button = sharing_toolbar_button(window, "doc-share-invite", "shared-document",
            self.header_action_labels(window).then(|| tr("已共享", "Shared")), open, false);
        let share = button
            .when(attention, |d| d.relative().child(sync_badge()))
            .on_click(cx.listener(|w, _, window, cx| {
                w.doc_menu = false;
                w.doc_sub = None;
                if w.sharing.sync_panel_open { w.close_sharing(window, cx); return; }
                w.sharing.panel_open = !w.sharing.panel_open || !w.sharing.permissions_only;
                w.sharing.permissions_only = true;
                cx.notify();
            }))
            .tip(if attention { tr("同步需要处理", "Sync needs attention") }
                else if owner { tr("成员与权限", "Members and access") } else { tr("查看共享权限", "View sharing access") }, None);
        Some(self.anchor_share_control(share))
    }

    pub(crate) fn anchor_share_control(&self, button: impl IntoElement) -> Div {
        anchor_control(button, self.sharing.share_bounds.clone())
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
        let owner = room.scope.owner.is_none();
        let heading = if !permissions_only { tr("在线协作者", "Online collaborators") }
            else if owner { tr("成员与权限", "Members and access") }
            else { tr("共享权限", "Sharing access") };
        let mut body = div().flex().flex_col().p(px(14.)).gap(px(8.))
            .child(div().flex().items_center().gap(px(7.))
                .child(sharing_panel_title(heading))
                .when(!permissions_only, |d| d.child(div().rounded(px(5.)).px(px(5.)).bg(Theme::fill_hover()).text_size(px(12.)).child(people.len().to_string())))
                .child(div().flex_1())
                .child(crate::design::Button::close("collab-close")
                    .on_click(cx.listener(|w, _, _, cx| { w.sharing.panel_open = false; cx.notify(); }))))
            .child(div().truncate().text_size(px(13.)).text_color(Theme::text_caption()).child(display_title(&room.title)));
        if permissions_only {
            body = body.children(self.sharing_scope_detail(&room,cx).map(caption));
        }
        if permissions_only && owner {
            body = body.child(caption(tr("所有者 · 你", "Owner · You")));
        } else if permissions_only {
            body = body.child(caption(shared::owner_display_name(self.store.root()).map_or_else(
                || tr("与你共享", "Shared with you").to_owned(),
                |name| crate::localized_format!("{}共享给你", "Shared with you by {}", name),
            ))).child(caption(role(access)));
        } else {
            body = body.child(caption(format!("{} · {state}", role(access))));
        }
        if !people.is_empty() && !permissions_only {
            let row_height = 36.;
            let list_height = (row_height * people.len() as f32).min(row_height * 5.5);
            let list = div().id("sharing-present-people").h(px(list_height))
                .max_h((window.viewport_size().height - anchor.bottom() - px(210.)).max(px(row_height)))
                .overflow_y_scroll().children(people.iter().enumerate().map(|(index, person)| {
                    let name = person.name.as_ref().map_or("", |s| s.as_ref());
                    let mark = div().size(px(26.)).rounded_full().flex().items_center().justify_center()
                        .bg(person.color().opacity(0.14)).text_size(px(12.)).child(person.initials.clone());
                    div().h(px(row_height)).flex_none().flex().items_center().gap(px(9.))
                        .child(mark.flex_none()).child(div().min_w_0().flex_1().truncate().text_size(px(13.)).child(member_label(name, "", index)))
                        .child(div().text_size(px(12.)).text_color(Theme::text_caption()).child(tr("在线", "Online")))
                }));
            body = body.child(list);
        }
        use crate::design::{MenuItem, MenuSeparator};
        let members = room.clone();
        let access_title = if owner { tr("成员与权限…", "Members and access…") } else { tr("查看共享权限…", "View sharing access…") };
        let busy = self.sharing.busy;
        let mut actions = div().flex().flex_col()
            .child(MenuSeparator::new())
            .child(MenuItem::new("collab-members").icon("shared-document").label(access_title).disabled(busy)
                .on_click(cx.listener(move |w, _, window, cx| {
                    if owner { w.open_member_management(members.clone(),window,cx); }
                    else { w.sharing_sheet(Sheet::Access(members.clone()),window,cx); }
                })));
        if permissions_only {
            let attention = settings::sync_status::needs_attention(self,cx);
            actions = actions.child(MenuItem::new("collab-cloud-settings").icon("cloud").label(tr("同步状态", "Sync status"))
                .when(attention, |row| row.description(tr("需处理", "Needs attention")))
                .on_click(cx.listener(move |w, _, window, cx| w.open_shared_sync(window,cx))));
        }
        body = body.child(actions.child(MenuSeparator::new())
            .child(MenuItem::new("collab-end").label(self.shared_end_label(&room,cx)).destructive(true)
                .disabled(!self.sharing_account_ready(cx) || busy)
                .on_click(cx.listener(move |w, _, window, cx| w.open_end_shared(room.clone(), window, cx)))));
        Some(deferred(anchored().anchor(Anchor::TopRight).position(point(anchor.right(), anchor.bottom() + px(4.)))
            .snap_to_window_with_margin(px(8.)).child(sharing_popover("doc-sharing-panel")
                .w((window.viewport_size().width - px(16.)).min(px(340.)))
                .max_h((window.viewport_size().height - anchor.bottom() - px(12.)).max(px(100.)))
                .overflow_y_scroll().child(body).on_mouse_down_out(cx.listener(move |w, event: &MouseDownEvent, _, cx| {
                    if !anchor.contains(&event.position) { w.sharing.panel_open = false; cx.notify(); }
                })))).with_priority(2).into_any_element())
    }

    fn open_shared_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.view, View::Doc | View::Folder) || self.current_room(cx).is_none() { return; }
        self.close_sharing(window,cx);
        self.sharing.focus_before = window.focused(cx);
        self.sharing.sync_panel_open = true;
        crate::gpui_compat::focus(window, &self.focus, cx);
        cx.notify();
    }

    fn shared_sync_popover(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        use settings::sync_status::Action;
        if !matches!(self.view, View::Doc | View::Folder) { return None; }
        let anchor = self.sharing.share_bounds.get()?;
        let state = settings::sync_status::shared(self,cx)?;
        let root = self.store.root().to_path_buf();
        let (warning, _) = settings::sync_status::marker_colors();
        let close = crate::design::Button::close("shared-sync-close")
            .on_click(cx.listener(|w, _, window, cx| w.close_sharing(window, cx)));
        use crate::design::{Separator, color};
        let muted = color::muted_foreground();
        let supporting_text = |text: SharedString| div().text_color(muted).child(ui::ui_para(text));
        let mut body = div().flex().flex_col().gap(px(8.))
            .child(div().flex().justify_between().items_center().gap(px(8.))
                .child(sharing_panel_title(tr("同步状态", "Sync status")).flex_1()).child(close))
            .child(supporting_text(state.scope.into()))
            .child(div().font_weight(FontWeight::SEMIBOLD).when(state.attention, |d| d.text_color(warning)).child(ui::ui_para(state.title)))
            .when(!state.detail.is_empty(), |d| d.child(supporting_text(state.detail.into())))
            .children(state.rows.into_iter().map(|(title,value)| div().flex().flex_col().gap(px(8.))
                .child(Separator::new())
                .child(div().flex().justify_between().gap(px(8.))
                    .child(div().flex_1().min_w_0().text_color(muted).child(ui::ui_para(title)))
                    .child(div().min_w_0().child(ui::ui_para(value))))));
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
            .snap_to_window_with_margin(px(8.)).child(sharing_popover("shared-sync-popover").padded()
                .w((window.viewport_size().width-px(16.)).min(px(330.)))
                .max_h((window.viewport_size().height-anchor.bottom()-px(12.)).max(px(100.)))
                .overflow_y_scroll().child(body)
                .on_mouse_down_out(cx.listener(move |w, ev: &MouseDownEvent, window, cx| {
                    if !anchor.contains(&ev.position) { w.close_sharing(window, cx); }
                })))).with_priority(5).into_any_element())
    }

    pub(crate) fn new_and_share(&mut self, kind: NewSharedItem, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy || !self.home.shared() || self.sharing.collection.is_some() || crate::cloud_restore::local_only() { return; }
        #[cfg(target_os = "ios")]
        if self.is_shared() && self.mobile_personal_browser(cx) {
            self.personal_action(window, cx, move |w, window, cx| w.new_and_share(kind, window, cx));
            return;
        }
        self.open_new_shared_item(kind, window, cx);
    }

    pub(crate) fn open_new_shared_item(&mut self, kind: NewSharedItem, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy || crate::cloud_restore::local_only() || !self.flush(cx) { return; }
        #[cfg(target_os = "ios")]
        if self.is_shared() { return; }
        self.create_menu = None;
        self.row_menu = None;
        self.sharing.folder_name.update(cx, |field, cx| field.set_text("", cx));
        self.sharing_sheet(Sheet::CreateNew(kind, None), window, cx);
    }

    fn submit_new_shared_item(&mut self, kind: NewSharedItem, id: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let email = self.sharing.email.read(cx).text.trim().to_owned();
        let title = self.sharing.folder_name.read(cx).text.trim().to_owned();
        let permission = self.sharing.permission;
        let public = self.sharing.public_link.then_some(permission);
        if (public.is_none() && email.is_empty()) || title.is_empty() || !self.flush(cx) { return; }
        let id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        self.sharing.sheet = Some(Sheet::CreateNew(kind, Some(id.clone())));
        let personal = crate::store::Store::root_dir();
        let revision = cloud_sync::account_revision();
        let origin = (self.view, self.settings_page);
        let epoch = self.begin_sharing_request(cx);
        enum Invitation { Named(Reply), Public(shared::LinkPublication) }
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                let room = shared::original::prepare_new(&personal, id, title, kind, public)?;
                cloud_sync::check_account_revision(revision)?;
                anyhow::ensure!(shared::pending_public_access(&personal, &room) == public,
                    "{}", tr("共享范围已改变，请重新打开共享面板。", "The access scope changed. Reopen the sharing panel."));
                let named = if public.is_some() {
                    shared::ensure_created(&personal, &room)?;
                    shared::original::register_new(&personal, &room, revision)?;
                    None
                } else {
                    Some(shared::original::invite_new(&personal, &room, &email, permission, revision)?)
                };
                let row = cloud_sync::activity::row(&personal, room.clone())?
                    .ok_or_else(|| anyhow::anyhow!("The new shared item is unavailable"))?;
                let invitation = named.map_or_else(|| Invitation::Public(shared::publish_link(&personal, &room, None)), Invitation::Named);
                Ok::<_, anyhow::Error>((room, row, invitation))
            }).await;
            let _ = this.update_in(cx, |w, window, cx| match result {
                Ok((room, row, invitation)) => {
                    if cloud_sync::account_revision() != revision {
                        w.finish_sharing_request(epoch, Some(anyhow::anyhow!(navigation::shared_account_changed())), cx);
                        return;
                    }
                    cloud_sync::activity::publish_row(row, cx);
                    let stayed = (w.view, w.settings_page) == origin;
                    match invitation {
                        Invitation::Public(outcome) if stayed => w.finish_public_link(epoch, room, outcome, true, window, cx),
                        Invitation::Public(_) => {
                            if w.finish_sharing_request(epoch, None, cx) { w.close_sharing(window, cx); }
                        }
                        Invitation::Named(reply) => {
                            if !w.finish_sharing_request(epoch, None, cx) { return; }
                            if !w.is_shared() { w.entries = w.store.list(); w.sort(); }
                            #[cfg(target_os = "ios")]
                            if stayed { w.sharing.reply = Some(reply); w.show_invitation_result(room, window, cx); return; }
                            w.close_sharing(window, cx);
                            #[cfg(not(target_os = "ios"))]
                            if stayed { w.navigate_new_shared_item(room, reply, window, cx); }
                        }
                    }
                }
                Err(error) => { w.finish_sharing_request(epoch, Some(error), cx); }
            });
        }).detach();
    }

    pub(crate) fn personal_row_can_share(&self, id: &str, cx: &App) -> bool {
        if crate::cloud_restore::local_only() { return false; }
        let entries = self.personal_navigation_entries(cx);
        let Some(row) = entries.iter().find(|row| row.id == id && !row.meta.local()
            && row.meta.kind != crate::store::DocKind::Table) else { return false; };
        if row.meta.share_root.as_deref() == Some(id) { return true; }
        let members = crate::tree::Tree::new(&entries).subtree(id);
        entries.iter().filter(|row| members.contains(&row.id)).all(|row|
            row.meta.share_root.is_none() && !row.meta.local() && row.meta.kind != crate::store::DocKind::Table)
    }

    pub(crate) fn create_shared_note(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_shared() || !self.personal_row_can_share(id, cx) || self.sharing.busy || !self.flush(cx) { return; }
        if let Some(room) = shared::original::owner_room(&self.entries, id) {
            if shared::original::awaiting_publication(self.store.root(), &room) {
                self.resume_original_sharing(room, window, cx);
                return;
            }
            self.open_member_management(room, window, cx);
            return;
        }
        self.sharing_sheet(Sheet::Create(id.into()), window, cx);
    }

    fn resume_original_sharing(&mut self, room: Room, window: &mut Window, cx: &mut Context<Self>) {
        let permission = shared::pending_public_access(&crate::store::Store::root_dir(), &room);
        self.sharing_sheet(Sheet::Create(room.scope.note.clone()), window, cx);
        if let Some(permission) = permission {
            self.sharing.public_link = true;
            self.sharing.permission = permission;
        }
        self.sharing.prepared = Some(room);
    }

    pub(crate) fn can_choose_link(&self, id: &str, entries: &[crate::store::Entry]) -> bool {
        self.sharing.prepared.is_none()
            && entries.iter().any(|entry| entry.id == id && !entry.meta.local())
    }

    pub(crate) fn can_choose_sharing_audience(&self, cx: &App) -> bool {
        match self.sharing.sheet.as_ref() {
            Some(Sheet::Create(id)) if self.is_shared() => self.can_choose_link(id, &self.personal_navigation_entries(cx)),
            Some(Sheet::Create(id)) => self.can_choose_link(id, &self.entries),
            Some(Sheet::CreateNew(_, None)) => true,
            _ => false,
        }
    }

    pub(crate) fn set_public_link(&mut self, public_link: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy || !self.can_choose_sharing_audience(cx) || self.sharing.public_link == public_link { return; }
        self.sharing.public_link = public_link;
        self.sharing.permission = Access::ReadOnly;
        self.sharing.message = None;
        if public_link && matches!(self.sharing.sheet, Some(Sheet::Create(_))) { crate::gpui_compat::focus(window, &self.focus, cx); }
        cx.notify();
    }

    pub(crate) fn public_link_retry_access(&self, room: &Room, cx: &App) -> Option<Access> {
        if !self.can_manage_room(room, cx) { return None; }
        let local = shared::pending_public_access(&crate::store::Store::root_dir(), room);
        self.sharing.reply.as_ref()?.draft_public_access(local)
    }

    pub(crate) fn retry_public_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy { return; }
        let Some(room) = self.sheet_room().cloned() else { return; };
        let Some(permission) = self.public_link_retry_access(&room, cx) else { return; };
        let root = crate::store::Store::root_dir();
        let epoch = self.begin_sharing_request(cx);
        cx.spawn_in(window, async move |this, cx| {
            let target = room.clone();
            let outcome = cx.background_executor().spawn(async move { shared::publish_link(&root, &target, Some(permission)) }).await;
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
                let region = matches!(error.downcast_ref::<shared::LinkWriteError>(), Some(shared::LinkWriteError::Region));
                self.sharing.public_link_failure = Some(if region { PublicLinkFailure::Region } else { PublicLinkFailure::Other });
                let message = match error.downcast_ref::<shared::LinkWriteError>() {
                    Some(shared::LinkWriteError::Region) => tr("iCloud 地区限制，未能启用公开链接。", "iCloud regional restriction prevented enabling the public link.").into(),
                    Some(issue) => issue.to_string(),
                    None => tr("未能启用公开链接。请刷新共享权限后再试。", "Could not enable the public link. Refresh sharing access and try again.").into(),
                };
                self.sharing.message = Some((true, message));
            }
            Ok(shared::PublishedLink::Active(Some(link))) if open => {
                let text = invitation_text(&room.title, &link);
                self.write_invitation_to_clipboard(text.clone(), cx);
                #[cfg(target_os = "ios")]
                {
                    self.public_invitation_copied(cx);
                    self.sharing_sheet(Sheet::Members(room), window, cx);
                    self.sharing.reply = outcome.reply;
                    self.sharing.mobile_invitation_result = true;
                }
                #[cfg(not(target_os = "ios"))]
                {
                    self.close_sharing(window, cx);
                    self.navigate_published_invitation(room, text, window, cx);
                }
            }
            Ok(shared::PublishedLink::Active(link)) => {
                if open { self.sharing_sheet(Sheet::Members(room.clone()), window, cx); }
                self.sharing.reply = outcome.reply;
                if let Some(link) = link {
                    self.write_invitation_to_clipboard(invitation_text(&room.title, &link), cx);
                    self.public_invitation_copied(cx);
                } else {
                    self.show_sharing_toast(ui::ToastKind::Info, tr(
                        "公开链接已启用。请点「复制邀请」获取链接。",
                        "The public link is enabled. Select Copy invitation to get the link."), cx);
                }
            }
            Ok(shared::PublishedLink::Closed) => {
                if open { self.sharing_sheet(Sheet::Members(room), window, cx); }
                self.sharing.reply = outcome.reply;
                self.show_sharing_toast(ui::ToastKind::Success, link_audience(Audience::LinkClosed), cx);
            }
        }
    }

    pub(crate) fn can_close_link(&self, room: &Room, cx: &App) -> bool {
        room.scope.owner.is_none() && self.sharing_account_ready(cx)
            && (!self.is_shared() || shared::scope(&self.store, cx) == Some(&room.scope))
            && self.sharing.reply.as_ref().is_some_and(|reply| reply.audience.link_active())
    }

    pub(crate) fn confirm_close_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy || !self.sheet_room().is_some_and(|room| self.can_close_link(room, cx)) { return; }
        if self.sharing.close_link_confirm { self.manage_shared_note("closeLink", String::new(), None, window, cx); }
        else { self.sharing.close_link_confirm = true; cx.notify(); }
    }

    pub(crate) fn open_invitation(&mut self, room: Room, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy || !self.can_manage_room(&room, cx) { return; }
        let parent = if matches!(&self.sharing.sheet, Some(Sheet::Members(current)) if current.scope == room.scope) {
            self.sharing.reply.take().map(|reply| (cloud_sync::account_revision(), reply))
        } else { None };
        self.sharing_sheet(Sheet::Invite(room), window, cx);
        self.sharing.invitation_parent = parent;
    }

    pub(crate) fn return_to_member_management(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Sheet::Invite(room)) = self.sharing.sheet.clone() else { return; };
        if self.sharing.busy { return; }
        let parent = self.sharing.invitation_parent.take().filter(|(revision, _)| {
            *revision == cloud_sync::account_revision() && self.can_manage_room(&room, cx)
        });
        if let Some((_, reply)) = parent {
            let refresh = self.sharing.message.as_ref().is_some_and(|(error, _)| *error);
            self.sharing_sheet(Sheet::Members(room), window, cx);
            self.sharing.reply = Some(reply);
            if refresh { self.manage_shared_note("members", String::new(), None, window, cx); }
        } else { self.open_member_management(room, window, cx); }
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

    fn sharing_sheet_anchor(&self, sheet: &Sheet, cx: &App) -> Option<Bounds<Pixels>> {
        let id = match self.view {
            View::Doc => self.current.as_deref(),
            View::Folder => self.folder_current.as_deref(),
            _ => None,
        }?;
        let contextual = match sheet {
            Sheet::Create(target) => id == target && !self.is_shared(),
            Sheet::Members(room) | Sheet::Invite(room) | Sheet::Access(room) => {
                if self.is_shared() { self.current_room(cx).is_some_and(|current| current.scope == room.scope) }
                else { room.scope.owner.is_none() && room.scope.note == id }
            }
            _ => false,
        };
        contextual.then(|| self.sharing.share_bounds.get()).flatten()
    }

    pub(crate) fn render_sharing_sheet(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let sheet = self.sharing.sheet.as_ref()?;
        let busy = self.sharing.busy;
        let invitation = matches!(sheet, Sheet::Create(_) | Sheet::CreateNew(..) | Sheet::Invite(_));
        let accent = invitation_accent();
        let title = match sheet { Sheet::MoveInto(..) => tr("移入共享文件夹", "Move into shared folder"), Sheet::RenameCollection(_) => tr("重命名个人文件夹", "Rename personal folder"), Sheet::Create(id) if self.entries.iter().any(|e| &e.id == id && e.meta.folder.is_some()) => tr("共享文件夹", "Share folder"),
            Sheet::Create(id) if self.entries.iter().any(|e| &e.id == id && e.meta.kind == crate::store::DocKind::Board) => tr("共享画板", "Share board"),
            Sheet::Create(_) => tr("共享文档", "Share document"), Sheet::Accept => tr("接受邀请", "Accept invitation"),
            Sheet::CreateNew(kind, _) => new_shared_title(*kind),
            Sheet::Access(room) => shared_access_title(room.scope.folder),
            Sheet::Members(_) => tr("成员与权限", "Members and access"),
            Sheet::Invite(_) => tr("邀请参与者", "Invite participant"),
            Sheet::EndMany(..) => tr("移除共享文档", "Remove shared documents"),
            Sheet::End(room) => self.shared_end_label(room,cx) };
        let members = matches!(sheet, Sheet::Members(_));
        let anchor = self.sharing_sheet_anchor(sheet, cx);
        let inviting = matches!(sheet, Sheet::Invite(_));
        let accepting = matches!(sheet, Sheet::Accept);
        let header_close = invitation || members || accepting || matches!(sheet, Sheet::Access(_));
        let has_footer = !matches!(sheet, Sheet::Access(_));
        let public_link_failure = self.sharing.public_link_failure;
        let error_by_field = matches!(sheet, Sheet::Invite(_) | Sheet::Accept)
            && self.sharing.message.as_ref().is_some_and(|(error,_)| *error);
        let mut body = div().id("sharing-sheet-body").track_scroll(&self.sharing.scroll).overflow_y_scroll().min_h_0()
            .when(!members || anchor.is_some(), |d| d.flex_1())
            .px(px(24.)).pb(px(if members { 12. } else { 20. })).flex().flex_col()
            .gap(px(if members { 10. } else { 14. }));
        match sheet {
            Sheet::MoveInto(ids, _, room) => {
                body = body.child(caption(crate::localized_format!("将 {} 项移入「{}」。这个文件夹的所有成员都能访问移入的文档及子文件夹。", "Move {} items into “{}”. Everyone with access to this folder can access the moved documents and subfolders.", ids.len(), room.title)));
            }
            Sheet::RenameCollection(_) => {
                body = body.child(crate::design::Field::new("private-collection-name").w_full().focus_color(accent)
                    .focused(self.sharing.folder_name.read(cx).is_focused(window)).child(self.sharing.folder_name.clone()))
                    .child(caption(tr("只有你能看到这个分类。", "Only you can see this folder.")));
            }
            Sheet::EndMany(rooms, personal) => {
                let mut details = Vec::new();
                if !personal.is_empty() { details.push(crate::localized_format!("将 {} 篇个人文档移到回收站。", "Move {} personal documents to Trash.", personal.len())); }
                let owned = rooms.iter().filter(|room| room.scope.owner.is_none()).count(); let received = rooms.len() - owned;
                if owned > 0 { details.push(crate::localized_format!("删除你创建的 {} 篇共享文档，所有参与者都将无法访问。", "Delete {} documents shared by you. Everyone will lose access.", owned)); }
                if received > 0 { details.push(crate::localized_format!("离开别人共享的 {} 篇文档，其他人不受影响。", "Leave {} documents shared with you. Other people keep access.", received)); }
                details.push(shared_trash_description().to_owned());
                details.extend(rooms.iter().take(5).map(|room| display_title(&room.title)));
                return Some(self.render_shared_end_confirmation(title, tr("确认移除", "Remove documents"), details.join("\n"), window, cx));
            }
            Sheet::End(room) => {
                let mut details = vec![display_title(&room.title), self.shared_end_effect(room,cx).to_owned()];
                if !self.stopping_original(room, cx) && !room.scope.folder { details.push(shared_trash_description().to_owned()); }
                return Some(self.render_shared_end_confirmation(title, title, details.join("\n"), window, cx));
            }
            Sheet::CreateNew(kind, id) => {
                body = body.child(caption(match kind {
                    NewSharedItem::Folder => tr("受邀成员可以访问文件夹及以后添加的内容。", "Invited members can access this folder and anything added later."),
                    NewSharedItem::Board => tr("受邀成员可以访问这张新画板。", "Invited members can access this new board."),
                    NewSharedItem::Document => tr("受邀成员可以访问这篇新文档。", "Invited members can access this new document."),
                })).child(if id.is_some() {
                    caption(self.sharing.folder_name.read(cx).text.clone()).into_any_element()
                } else {
                    crate::design::Field::new("new-shared-name").w_full().focus_color(accent)
                        .focused(self.sharing.folder_name.read(cx).is_focused(window)).child(self.sharing.folder_name.clone()).into_any_element()
                }).children(self.can_choose_sharing_audience(cx).then(|| self.sharing_audience_fields(cx)))
                    .child(if self.sharing.public_link { self.public_link_fields(kind.is_folder(), cx) } else { self.invitation_fields(window, cx) });
            }
            Sheet::Create(id) => {
                let entry = self.entries.iter().find(|entry| &entry.id == id);
                let folder = entry.is_some_and(|e| e.meta.folder.is_some());
                let title = entry.map(|e| display_title(&e.title)).unwrap_or_default();
                body = body.child(Self::sharing_target(folder,title))
                    .child(caption(if folder { sharing_folder_scope() } else {
                        tr("与对方协作这篇原文。你仍可从原文件夹打开它。", "Collaborate on this document in its original folder.")
                    }));
                if self.can_choose_sharing_audience(cx) { body = body.child(self.sharing_audience_fields(cx)); }
                body = if self.sharing.public_link {
                    body.child(self.public_link_fields(folder, cx))
                } else { body.child(self.invitation_fields(window, cx)) };
                if crate::cloud_restore::local_only() {
                    body = body.child(action("share-enable-cloud", tr("开启 iCloud 同步…", "Enable iCloud sync…"), !busy)
                        .on_click(cx.listener(|w, _, window, cx| { w.close_sharing(window, cx); w.open_settings(settings::Page::Library, cx); })));
                }
                if self.sharing.prepared.is_some() { body = body.child(caption(tr("共享准备已保存，可以在原文上重试。", "Sharing is prepared. Retry on the same document."))); }
                if !folder && !crate::FEI_NOTES {
                    let export = id.clone();
                    body = body.child(div().h(px(1.)).bg(Theme::line_divider()))
                        .child(div().flex().child(action("share-send-copy", tr("导出 Markdown 副本…", "Export Markdown copy…"), !busy)
                            .on_click(cx.listener(move |w, _, window, cx| { if w.sharing.busy { return; } w.close_sharing(window, cx); w.open_export_md(&export, cx); }))))
                        .child(caption(tr("副本不会随原文更新。", "The copy doesn't update with the original.")));
                }
            }
            Sheet::Accept => {
                body = body.child(caption(invitation_account_requirements()))
                    .child(crate::design::Field::new("sharing-link").w_full()
                        .focus_color(accent).focused(self.sharing.link.read(cx).is_focused(window)).child(self.sharing.link.clone()))
                    .children(self.sharing.message.as_ref().filter(|_| error_by_field).map(|(_,message)| sharing_error(message)));
            }
            Sheet::Invite(room) => {
                body = body.child(Self::sharing_target(room.scope.folder, display_title(&room.title)))
                    .children(self.sharing_scope_detail(room,cx).map(caption)).child(self.invitation_fields(window, cx))
                    .children(self.sharing.message.as_ref().filter(|_| error_by_field).map(|(_,message)| sharing_error(message)));
            }
            Sheet::Access(room) => {
                let (owner,access) = shared::access_summary(room,cx);
                body = body.child(Self::sharing_target(room.scope.folder, display_title(&room.title)))
                    .children(self.sharing_scope_detail(room,cx).map(caption))
                    .child(if let Some(owner) = owner { line(tr("共享来源", "Shared by"),owner,div()) }
                        else { caption(tr("他人与你共享", "Shared with you")) })
                    .child(line(tr("你的权限", "Your access"),role(access),div()));
            }
            Sheet::Members(room) => {
                let allowed = self.can_manage_room(room, cx);
                body = body.child(Self::sharing_target(room.scope.folder, display_title(&room.title)))
                    .children(self.sharing_scope_detail(room,cx).map(caption));
                let mut participants = div().flex_none().flex().flex_col()
                    .child(div().flex().flex_wrap().items_center().justify_between().gap(px(6.))
                        .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child(tr("参与者", "Participants")))
                        .child(crate::design::Button::outline("members-refresh").sized(crate::design::ButtonSize::Sm)
                            .label(tr("刷新成员", "Refresh members")).disabled(busy)
                            .on_click(cx.listener(|w, _, window, cx| w.manage_shared_note("members", String::new(), None, window, cx)))))
                    .child(participant_row(tr("你", "You"), tr("所有者", "Owner"), false, div()));
                if let Some(reply) = &self.sharing.reply {
                    let invited = reply.audience.can_invite();
                    for (index, member) in reply.members.iter().enumerate() {
                        let id = member.id.clone();
                        let selected = self.sharing.member.as_deref() == Some(&member.id);
                        let mark = if invited { ui::icon("chevron-right", 14., Theme::icon()).into_any_element() } else { div().into_any_element() };
                        let name = member_label(&member.name, &member.id, index);
                        participants = participants.child(div().id(SharedString::from(format!("share-member-{}", member.id)))
                            .tip(&name, None)
                            .child(participant_row(name, role(Some(member.permission)), !member.accepted, mark))
                            .when(invited && allowed && !busy, |d| d.cursor_pointer().hover(|d| d.bg(Theme::fill_hover())).on_click(cx.listener(move |w, _, _, cx| {
                                if w.sharing.busy { return; }
                                w.sharing.member = if selected { None } else { Some(id.clone()) }; w.sharing.revoke_confirm = false; cx.notify();
                            }))));
                        if invited && selected {
                            let id = member.id.clone(); let remove = id.clone();
                            let next = if member.permission == Access::ReadWrite { Access::ReadOnly } else { Access::ReadWrite };
                            let permission = if next == Access::ReadOnly { tr("改为只读", "Allow viewing") } else { tr("允许编辑", "Allow editing") };
                            let revoke = member_removal_label(member.accepted);
                            participants = participants.child(div().py(px(6.)).flex().flex_wrap().gap(px(8.))
                                .child(action("member-permission", permission, allowed && !busy)
                                    .on_click(cx.listener(move |w, _, window, cx| w.manage_shared_note("permission", id.clone(), Some(next), window, cx))))
                                .child(crate::design::Button::destructive("member-remove").sized(crate::design::ButtonSize::Sm)
                                    .label(revoke).disabled(!allowed || busy)
                                    .on_click(cx.listener(move |w, _, window, cx| {
                                        w.confirm_shared_removal(remove.clone(), window, cx);
                                    }))));
                        }
                    }
                    if invited {
                        participants = participants.child(crate::design::MenuItem::new("members-add")
                            .mt(px(6.)).icon("plus").label(tr("添加参与者", "Add participant")).submenu(true)
                            .disabled(!allowed || busy)
                            .on_click(cx.listener(|w, _, window, cx| {
                                if let Some(room) = w.sheet_room().cloned() { w.open_invitation(room, window, cx); }
                            })));
                    }
                    let retry = reply.public_draft();
                    let public = reply.audience.link_active();
                    let confirming = public && self.sharing.close_link_confirm;
                    let region_alternative = public_link_failure == Some(PublicLinkFailure::Region) && invited;
                    let private_copy = room.clone();
                    let public_copy = room.clone();
                    let status = if reply.audience.can_invite() { invited_link_scope() } else { link_audience(reply.audience) };
                    let links = div().flex_none().flex().flex_col().gap(px(5.))
                        .child(Self::sharing_section_title(tr("链接访问", "Link access")))
                        .child(div().flex().flex_col().rounded(px(crate::design::R)).bg(Theme::fill_hover())
                            .child(div().flex().items_center().justify_between().gap(px(12.)).px(px(10.)).py(px(8.))
                                .child(div().text_size(px(13.)).text_color(Theme::text_caption()).child(tr("访问范围", "Access scope")))
                                .child(div().min_w_0().text_right().text_size(px(13.)).child(ui::ui_para(status))))
                            .children(public_link_failure.and(self.sharing.message.as_ref()).map(|(_,message)| sharing_error(message).mx(px(10.))))
                            .children(if retry {
                                Some(crate::design::MenuItem::new("retry-public-link").icon("share")
                                    .label(enable_public_link_label(public_link_failure)).disabled(busy || self.public_link_retry_access(room,cx).is_none())
                                    .on_click(cx.listener(|w, _, window, cx| w.retry_public_link(window, cx))))
                            } else if !public && reply.url.is_some() {
                                Some(crate::design::MenuItem::new("copy-share-link").icon("copy")
                                    .label(copy_invitation_label(false)).disabled(busy)
                                    .on_click(cx.listener(move |w, _, window, cx| w.copy_shared_invitation(private_copy.clone(), window, cx))))
                            } else { None })
                            .when(public && !confirming, |d| d.child(div().flex().flex_wrap().items_center().justify_between().gap(px(5.))
                                .px(px(4.)).py(px(3.)).border_t_1().border_color(Theme::line_divider())
                                .child(crate::design::Button::ghost("close-public-link")
                                    .label(tr("关闭公开链接…", "Close public link…")).disabled(busy || !self.can_close_link(room,cx))
                                    .on_click(cx.listener(|w, _, window, cx| w.confirm_close_link(window, cx))))
                                .child(crate::design::Button::ghost("copy-share-link").icon("copy")
                                    .label(copy_invitation_label(false)).disabled(busy)
                                    .on_click(cx.listener(move |w, _, window, cx| w.copy_shared_invitation(public_copy.clone(), window, cx))))))
                            .when(region_alternative, |d| d.child(crate::design::MenuItem::new("invite-specific-account").icon("plus")
                                .label(invite_specific_account_label()).disabled(!allowed || busy)
                                .on_click(cx.listener(|w, _, window, cx| {
                                    if let Some(room) = w.sheet_room().cloned() { w.open_invitation(room,window,cx); }
                                }))))
                            .when(confirming, |d| d.child(div().p(px(10.)).flex().flex_col().gap(px(6.))
                                .child(caption(close_link_warning()))
                                .child(div().flex().justify_end().gap(px(8.))
                                    .child(crate::design::Button::ghost("cancel-close-public-link").label(tr("取消", "Cancel")).disabled(busy)
                                        .on_click(cx.listener(|w, _, _, cx| { if !w.sharing.busy { w.sharing.close_link_confirm = false; cx.notify(); } })))
                                    .child(crate::design::Button::destructive("close-public-link").sized(crate::design::ButtonSize::Sm)
                                        .label(tr("确认关闭链接", "Confirm close link")).disabled(busy || !self.can_close_link(room,cx))
                                        .on_click(cx.listener(|w, _, window, cx| w.confirm_close_link(window, cx))))))));
                    let note = if reply.audience == Audience::LinkClosed { closed_link_detail(self.stopping_original(room,cx)) }
                        else { None };
                    body = body.child(participants).child(links.children(note.map(|text| caption(text).px(px(10.)))));
                } else {
                    body = body.child(participants);
                }
            }
        }
        if !matches!(sheet, Sheet::RenameCollection(_)) && !self.sharing_allowed(cx) {
            body = body.child(caption(shared::sharing_unavailable()));
        }
        let feedback = div().px(px(24.)).flex_none()
            .when_some(self.sharing.busy_feedback, |d, message| d.child(caption(message)).pb(px(10.)))
            .when_some(self.sharing.message.as_ref().filter(|_| !error_by_field && (!members || public_link_failure.is_none() || self.sharing.reply.is_none())), |d, (error, message)| d.pb(px(10.)).child(
                div().id("sharing-message").text_size(px(13.)).line_height(px(20.))
                    .text_color(if *error { crate::design::color::destructive() } else { Theme::text_caption() }).child(ui::ui_para(message.clone()))));
        let submit = match sheet { Sheet::MoveInto(..) => Some(tr("移入并共享", "Move and share")), Sheet::RenameCollection(_) => Some(tr("保存", "Save")), Sheet::Invite(_) if self.sharing.message.as_ref().is_some_and(|(error,_)| *error) => Some(tr("重试创建", "Retry invitation")),
            Sheet::Create(_) | Sheet::CreateNew(..) | Sheet::Invite(_) => Some(create_invitation_label()), Sheet::Accept => Some(tr("接受并打开", "Accept and open")), _ => None };
        let input_ok = if matches!(sheet, Sheet::RenameCollection(_)) { !self.sharing.folder_name.read(cx).text.trim().is_empty() } else if matches!(sheet, Sheet::MoveInto(..)) { true } else if matches!(sheet, Sheet::Accept) { !self.sharing.link.read(cx).text.trim().is_empty() }
            else if let Sheet::CreateNew(_, id) = sheet {
                (id.is_some() || !self.sharing.folder_name.read(cx).text.trim().is_empty()) && (self.sharing.public_link || !self.sharing.email.read(cx).text.trim().is_empty())
            } else { matches!(sheet, Sheet::Create(_)) && self.sharing.public_link || !self.sharing.email.read(cx).text.trim().is_empty() };
        let footer = div().flex_none().px(px(24.)).py(px(14.)).flex().justify_end().gap(px(8.)).border_t_1().border_color(Theme::line_divider())
            .children(if let Sheet::Members(room) = sheet {
                let room = room.clone();
                Some(crate::design::Button::destructive("members-delete-document").sized(crate::design::ButtonSize::Sm)
                    .label(self.shared_end_label(&room,cx)).disabled(busy || !self.sharing_account_ready(cx))
                    .on_click(cx.listener(move |w, _, _, cx| w.ask_end_shared(room.clone(), cx))))
            } else { None })
            .children((!header_close).then(|| crate::design::Button::outline("sharing-close").sized(crate::design::ButtonSize::Sm)
                .label(tr("取消", "Cancel")).disabled(busy)
                .on_click(cx.listener(|w, _, window, cx| w.close_sharing(window, cx)))))
            .children(submit.map(|label| {
                let allowed = match sheet {
                    Sheet::RenameCollection(_) => true,
                    _ => self.sharing_allowed(cx),
                };
                crate::design::Button::primary("sharing-submit").accent(accent, white()).sized(crate::design::ButtonSize::Sm).label(label).disabled(busy || !input_ok || !allowed)
                    .on_click(cx.listener(|w, _, window, cx| w.submit_sharing(window, cx)))
            }));
        let header = div().flex_none().px(px(24.)).py(px(if members || inviting { 14. } else if accepting { 20. } else { 18. })).flex().items_center().gap(px(9.))
            .children(inviting.then(|| crate::design::Button::ghost("invite-back").icon("chevron-left")
                .tip(tr("返回成员与权限", "Back to members and access"),None).disabled(busy)
                .on_click(cx.listener(|w, _, window, cx| w.return_to_member_management(window, cx)))))
            .when(invitation && !inviting, |d| d.child(div().size(px(30.)).rounded(px(8.)).bg(accent)
                .flex().items_center().justify_center().child(ui::icon("share", 18., white()))))
            .child(sharing_panel_title(title).flex_1())
            .when(header_close, |d| d.child(crate::design::Button::close("sharing-close").disabled(busy)
                .on_click(cx.listener(|w, _, window, cx| w.close_sharing(window, cx)))));
        if accepting && let Some(bounds) = self.sharing.accept_at.and_then(|key| self.home.anchor(key)) {
            let pos = point(bounds.left(), bounds.bottom() + px(4.));
            let form = sharing_popover("accept-invitation-popover").w(px(400.)).max_w(window.viewport_size().width - px(16.))
                .max_h(window.viewport_size().height - px(32.)).flex().flex_col()
                .on_mouse_down_out(cx.listener(|w, event: &MouseDownEvent, window, cx| {
                    if !w.accept_invitation_open() { return; }
                    if ["accept-home", "accept-sidebar"].iter().any(|key| w.home.anchor(key).is_some_and(|b| b.contains(&event.position))) { return; }
                    w.close_sharing(window, cx);
                }))
                .child(header)
                .child(body).child(feedback).child(footer);
            return Some(deferred(anchored().anchor(Anchor::TopLeft).position(pos).snap_to_window_with_margin(px(8.)).child(form)).with_priority(5).into_any_element());
        }
        if let Some(bounds) = anchor {
            let available = (window.viewport_size().height - bounds.bottom() - px(12.)).max(px(100.));
            let height = if matches!(sheet, Sheet::Access(_)) { 232. } else { 450. };
            let form = sharing_popover("share-document-popover").flex().flex_col()
                .w(px(480.)).max_w(window.viewport_size().width - px(16.)).h(available.min(px(height)))
                .child(header).child(body).child(feedback).children(has_footer.then_some(footer));
            // Consume the dismissing click so toolbar actions cannot reopen the form.
            let popup = anchored().anchor(Anchor::TopRight).position(point(bounds.right(), bounds.bottom() + px(4.)))
                .snap_to_window_with_margin(px(8.)).child(form);
            return Some(deferred(div().absolute().inset_0()
                .child(div().id("share-dismiss").absolute().inset_0().occlude()
                    .on_mouse_down(MouseButton::Left, cx.listener(|w, _, window, cx| { cx.stop_propagation(); w.close_sharing(window, cx); })))
                .child(popup)).with_priority(5).into_any_element());
        }
        let height = match sheet { Sheet::MoveInto(..) => 250., Sheet::RenameCollection(_) => 235., Sheet::Create(_) | Sheet::CreateNew(..) => 510., Sheet::Accept => 290., Sheet::Access(_) => 345., Sheet::Members(_) => 580., Sheet::Invite(_) => 425., Sheet::End(_) | Sheet::EndMany(..) => unreachable!("End confirmations use AlertDialog") };
        let max_height = window.viewport_size().height - px(40.);
        let modal = crate::design::Dialog::new("sharing-sheet")
            .bg(crate::theme::raised(Theme::bg_body())).text_color(Theme::text()).p(px(0.)).gap(px(0.)).when(!members, |d| d.h(max_height.min(px(height))))
            .w((window.viewport_size().width - px(32.)).min(px(520.)))
            .max_h(if members { max_height.min(px(height)) } else { max_height }).flex().flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(header).child(body).child(feedback).children(has_footer.then_some(footer));
        Some(deferred(crate::design::scrim().id("sharing-mask").occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|w, _, window, cx| { cx.stop_propagation(); w.close_sharing(window, cx); }))
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation()).child(modal)).with_priority(5).into_any_element())
    }

    fn render_shared_end_confirmation(&self, title: &'static str, action: &'static str, description: String, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        use crate::design::{Button, Dialog, color, scrim};
        let buttons = div().flex().justify_end().gap(px(8.))
            .child(Button::outline("sharing-close").label(tr("取消", "Cancel")).disabled(self.sharing.busy)
                .on_click(cx.listener(|w, _, window, cx| w.close_sharing(window, cx))))
            .child(Button::destructive("sharing-submit").label(action).disabled(self.sharing.busy || !self.sharing_account_ready(cx))
                .on_click(cx.listener(|w, _, window, cx| w.submit_sharing(window, cx))));
        let dialog = Dialog::alert("sharing-sheet")
            .max_w(window.viewport_size().width - px(32.))
            .max_h(window.viewport_size().height - px(40.)).overflow_y_scroll()
            .title(title).description(description)
            .children(self.sharing.busy_feedback.map(|message| div().text_color(color::muted_foreground()).child(ui::ui_para(message))))
            .children(self.sharing.message.as_ref().map(|(error, message)|
                div().text_color(if *error { color::destructive() } else { color::muted_foreground() }).child(ui::ui_para(message.clone()))))
            .footer(buttons);
        deferred(scrim().id("sharing-mask").occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(dialog)).with_priority(5).into_any_element()
    }

    fn sharing_target(folder: bool, title: impl Into<SharedString>) -> Div {
        div().flex().flex_col().gap(px(3.))
            .child(caption(sharing_target_label(folder)))
            .child(div().text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).child(ui::ui_para(title.into())))
    }

    fn sharing_section_title(title: &'static str) -> Div {
        div().pt(px(5.)).text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child(title)
    }

    fn sharing_audience_fields(&self, cx: &mut Context<Self>) -> Div {
        let accent = invitation_accent();
        div().flex().flex_wrap().gap(px(8.)).children([false, true].into_iter().map(|public| {
            crate::design::Button::outline(if public { "share-anyone" } else { "share-invited" })
                .sized(crate::design::ButtonSize::Sm)
                .label(if public { tr("任何持有链接的人", "Anyone with the link") } else { link_audience(Audience::Invited) })
                .selected(self.sharing.public_link == public).accent(accent, Theme::text()).disabled(self.sharing.busy)
                .on_click(cx.listener(move |w, _, window, cx| w.set_public_link(public, window, cx)))
        }))
    }

    pub(crate) fn set_public_link_permission(&mut self, access: Access, cx: &mut Context<Self>) {
        if self.sharing.busy || !self.sharing.public_link || !self.can_choose_sharing_audience(cx) { return; }
        if matches!(access, Access::ReadOnly | Access::ReadWrite) { self.sharing.permission = access; cx.notify(); }
    }

    fn public_link_fields(&self, folder: bool, cx: &mut Context<Self>) -> Div {
        self.sharing_permission_fields(true, cx).child(caption(public_link_requirements()))
            .when(self.sharing.permission == Access::ReadWrite, |d| d.child(caption(public_edit_warning(folder))))
    }

    fn sharing_permission_fields(&self, public: bool, cx: &mut Context<Self>) -> Div {
        let enabled = !self.sharing.busy && (!public || self.can_choose_sharing_audience(cx));
        let accent = invitation_accent();
        div().flex_none().flex().flex_col().gap(px(8.))
            .child(Self::sharing_section_title(tr("权限", "Permission")))
            .child(div().flex().flex_wrap().gap(px(8.)).children([Access::ReadOnly, Access::ReadWrite].into_iter().map(|access| {
                let id = match (public, access) { (true, Access::ReadOnly) => "public-reader", (true, _) => "public-writer", (false, Access::ReadOnly) => "invite-reader", _ => "invite-writer" };
                crate::design::Button::outline(id).sized(crate::design::ButtonSize::Sm)
                    .label(role(Some(access))).selected(self.sharing.permission == access).accent(accent, Theme::text()).disabled(!enabled)
                    .on_click(cx.listener(move |w, _, _, cx| {
                        if public { w.set_public_link_permission(access, cx); }
                        else if !w.sharing.busy { w.sharing.permission = access; cx.notify(); }
                    }))
            })))
    }

    fn invitation_fields(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        self.sharing_permission_fields(false, cx)
            .child(Self::sharing_section_title(tr("受邀 Apple 账户邮箱", "Invited Apple Account email")))
            .child(crate::design::Field::new("sharing-email").w_full()
                .focus_color(invitation_accent()).focused(self.sharing.email.read(cx).is_focused(window)).child(self.sharing.email.clone()))
            .child(caption(private_invitation_delivery()))
    }

    pub(crate) fn confirm_shared_removal(&mut self, member: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Sheet::Members(room)) = self.sharing.sheet.as_ref() else { return };
        if self.sharing.busy || self.sharing.revoke_confirm || !self.can_manage_room(room, cx) || self.sharing.member.as_deref() != Some(&member) { return; }
        let Some((index, participant)) = self.sharing.reply.as_ref().filter(|reply| reply.audience.can_invite())
            .and_then(|reply| reply.members.iter().enumerate().find(|(_, person)| person.id == member)) else { return };
        let name = member_label(&participant.name, &member, index);
        let action = member_removal_label(participant.accepted);
        let detail = if participant.accepted { crate::localized_format!("{} 将失去访问权限。", "{} will lose access.", name) }
            else { crate::localized_format!("{} 将无法再使用这份邀请。", "{} will no longer be able to use this invitation.", name) };
        let epoch = self.sharing.epoch;
        let account = cloud_sync::account_revision();
        self.sharing.revoke_confirm = true;
        #[cfg(feature = "devtools")]
        if std::env::var_os("QUIET_SYNC_QA_SHARING_UI").is_some() && std::env::var_os("FDL_IN_WINDOW_PROMPTS").is_none() {
            return;
        }
        let answer = window.prompt(PromptLevel::Warning, action, Some(&detail),
            &[PromptButton::cancel(tr("取消", "Cancel")),PromptButton::new(action)], cx);
        cx.spawn_in(window, async move |this, cx| {
            // AppKit returns NSAlertSecondButtonReturn, while GPUI's own prompt returns its index.
            let confirmed = matches!(answer.await, Ok(1 | 1001));
            let _ = this.update_in(cx, |w, window, cx| {
                if w.sharing.epoch == epoch { w.resolve_shared_removal(member, confirmed && cloud_sync::account_revision() == account, window, cx); }
            });
        }).detach();
    }

    fn resolve_shared_removal(&mut self, member: String, confirmed: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.sharing.revoke_confirm) { return; }
        if confirmed && matches!(self.sharing.sheet, Some(Sheet::Members(_))) && self.sharing.member.as_deref() == Some(&member) {
            self.manage_shared_note("revoke", member, None, window, cx);
        }
        cx.notify();
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
            Some(Sheet::CreateNew(kind, id)) => self.submit_new_shared_item(kind, id, window, cx),
            Some(Sheet::Accept) => self.accept_shared_note(window, cx),
            Some(Sheet::Invite(_)) => {
                let email = self.sharing.email.read(cx).text.trim().to_string();
                if !email.is_empty() { self.manage_shared_note("invite", email, Some(self.sharing.permission), window, cx); }
            }
            _ => {}
        }
    }

    fn begin_sharing_request(&mut self, cx: &mut Context<Self>) -> u64 {
        self.sharing.public_link_failure = None;
        let message = match self.sharing.sheet.as_ref() {
            Some(Sheet::CreateNew(NewSharedItem::Folder, _)) => tr("正在新建共享文件夹…", "Creating shared folder…"),
            Some(Sheet::CreateNew(NewSharedItem::Board, _)) => tr("正在新建共享画板…", "Creating shared board…"),
            Some(Sheet::CreateNew(NewSharedItem::Document, _)) => tr("正在新建共享文档…", "Creating shared document…"),
            Some(Sheet::Create(id)) => {
                if self.personal_navigation_entries(cx).iter().any(|e| &e.id == id && e.meta.folder.is_some()) {
                    tr("正在共享文件夹…", "Sharing folder…")
                } else { tr("正在共享文档…", "Sharing document…") }
            }
            Some(Sheet::Accept) => tr("正在接受邀请…", "Accepting invitation…"),
            Some(Sheet::MoveInto(..)) => tr("正在移入共享文件夹…", "Moving into shared folder…"),
            Some(Sheet::End(_) | Sheet::EndMany(..)) => tr("正在更新共享访问权限…", "Updating shared access…"),
            _ => tr("正在连接 iCloud…", "Connecting to iCloud…"),
        };
        self.sharing.busy = true;
        self.sharing.message = None;
        self.sharing.busy_feedback = None;
        self.sharing.epoch += 1;
        let epoch = self.sharing.epoch;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(LOADING_NOTICE_DELAY).await;
            let _ = this.update(cx, |ws, cx| {
                if ws.sharing.busy && ws.sharing.epoch == epoch {
                    ws.sharing.busy_feedback = Some(message); cx.notify();
                }
            });
        }).detach();
        cx.notify();
        epoch
    }

    fn finish_sharing_request(&mut self, epoch: u64, error: Option<anyhow::Error>, cx: &mut Context<Self>) -> bool {
        if self.sharing.epoch != epoch { return false; }
        self.sharing.busy = false;
        self.sharing.busy_feedback = None;
        if let Some(error) = error { self.sharing.message = Some((true, settings::diagnostic(&error.to_string()))); }
        cx.notify(); true
    }

    pub(crate) fn open_shared_note(&mut self, room: &Room, window: &mut Window, cx: &mut Context<Self>) {
        self.open_shared_item_interactive(room.clone(), room.scope.note.clone(), None, window, cx);
    }

    pub(crate) fn accept_shared_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let link = self.sharing.link.read(cx).text.trim().to_owned();
        if link.is_empty() { return; }
        let Ok(link) = links::pasted_invitation(&link) else {
            self.sharing.message = Some((true, tr("请粘贴有效的邀请链接后重试。", "Paste a valid invitation link and try again.").into()));
            cx.notify();
            return;
        };
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
                        if link_closed { w.show_sharing_toast(ui::ToastKind::Success, link_audience(Audience::LinkClosed), cx); }
                        else { w.sharing.message = Some((true, tr("iCloud 尚未确认关闭链接，请刷新后重试。", "iCloud has not confirmed closure. Refresh and try again.").into())); }
                    }
                    if action == "invite" { w.show_invitation_result(room, window, cx); }
                }
                Err(error) => {
                    if matches!(action, "permission" | "revoke") {
                        if w.finish_sharing_request(epoch, None, cx) {
                            w.show_sharing_toast(ui::ToastKind::Error, &settings::diagnostic(&error.to_string()), cx);
                        }
                    } else {
                        w.finish_sharing_request(epoch, Some(error), cx);
                    }
                }
            });
        }).detach();
    }

    fn show_invitation_result(&mut self, room: Room, window: &mut Window, cx: &mut Context<Self>) {
        self.sharing.copied_text = None;
        let mut copied = false;
        if (self.sharing.original.is_some() || cfg!(target_os = "ios"))
            && let Some(link) = self.sharing.reply.as_ref().and_then(|reply| reply.url.as_deref()).and_then(links::invitation_url) {
            self.write_invitation_to_clipboard(invitation_text(&room.title, &link), cx);
            copied = true;
        }
        self.sharing.sheet = Some(Sheet::Members(room));
        self.sharing.invitation_parent = None;
        #[cfg(target_os = "ios")]
        {
            self.sharing.mobile_invitation_result = true;
        }
        crate::gpui_compat::focus(window, &self.focus, cx);
        self.sharing.email.update(cx, |field, cx| field.set_text("", cx));
        let message = if copied { tr("已复制邀请。请发给对方。", "Invitation copied. Send it to the recipient.") }
            else { tr("邀请已创建。请复制邀请并发给对方。", "Invitation created. Copy it and send it to the recipient.") };
        self.show_sharing_toast(ui::ToastKind::Success, message, cx);
    }

    pub(crate) fn publish_shared_copy(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let email = self.sharing.email.read(cx).text.trim().to_owned();
        let permission = self.sharing.permission;
        let public_link = self.sharing.public_link.then_some(permission);
        if (public_link.is_none() && email.is_empty()) || !self.flush(cx) { return; }
        let Some(title) = self.personal_navigation_entries(cx).iter().find(|entry| entry.id == id).map(|entry| display_title(&entry.title)) else { return };
        let root = crate::store::Store::root_dir();
        #[cfg(target_os = "ios")]
        let revision = cloud_sync::account_revision();
        let prepared = self.sharing.prepared.clone();
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
                anyhow::ensure!(public_link == shared::pending_public_access(&upload_root, &upload_room),
                    "{}", tr("共享范围已改变，请重新打开共享面板。", "The access scope changed. Reopen the sharing panel."));
                shared::ensure_created(&upload_root, &upload_room)
            }).await;
            if let Err(error) = uploaded {
                let _ = this.update_in(cx, |w, _, cx| { w.finish_sharing_request(epoch, Some(error), cx); });
                return;
            }
            #[cfg(target_os = "ios")]
            {
                let catalogue = root.clone();
                let target = room.clone();
                let row = cx.background_executor().spawn(async move { cloud_sync::activity::row(&catalogue, target) }).await;
                if let Ok(Some(row)) = row {
                    let _ = this.update_in(cx, |_, _, cx| cloud_sync::activity::publish_row(row, cx));
                }
            }
            if public_link.is_some() {
                let target = room.clone();
                let outcome = cx.background_executor().spawn(async move { shared::publish_link(&root, &target, None) }).await;
                let _ = this.update_in(cx, |w, window, cx| w.finish_public_link(epoch, room, outcome, true, window, cx));
            } else {
                #[cfg(target_os = "ios")]
                {
                    let target = room.clone();
                    let result = cx.background_executor().spawn(async move {
                        cloud_sync::check_account_revision(revision)?;
                        let reply = shared::manage(&root, &target, "invite", &email, Some(permission))?;
                        cloud_sync::check_account_revision(revision)?;
                        Ok::<_, anyhow::Error>(reply)
                    }).await;
                    let _ = this.update_in(cx, |w, window, cx| match result {
                        Ok(reply) if w.finish_sharing_request(epoch, None, cx) => {
                            if !w.is_shared() { w.entries = w.store.list(); w.sort(); }
                            w.sharing.reply = Some(reply);
                            w.show_invitation_result(room, window, cx);
                        }
                        Err(error) => { w.finish_sharing_request(epoch, Some(error), cx); }
                        _ => {}
                    });
                }
                #[cfg(not(target_os = "ios"))]
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
