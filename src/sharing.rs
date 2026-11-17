//! Shared-note navigation, creation and contextual access management.
//! Cloud libraries and invitation permissions remain owned by cloud_sync::shared.

use crate::{app::{View, Workspace}, cloud_sync::{self, shared::{self, Access, Reply, Room}},
    input::{TextField, TextFieldEvent}, settings::{self, tr, display_title}, theme::Theme, ui::{self, TipExt, TitleBarExt}};
use gpui::{prelude::FluentBuilder, *};

#[cfg(feature = "devtools")]
pub(crate) mod qa;
#[cfg(feature = "devtools")]
mod qa_parity;
mod navigation;
mod lifecycle;
mod list;
mod sidebar;
mod actions;
pub(crate) mod links;

#[derive(Clone)]
pub(crate) enum Sheet { Create(String), Accept, Members(Room), Invite(Room), End(Room), EndMany(Vec<Room>, Vec<String>) }

pub(crate) struct Sharing {
    link: Entity<TextField>,
    email: Entity<TextField>,
    pub sheet: Option<Sheet>,
    reply: Option<Reply>,
    prepared: Option<Room>,
    original: Option<String>,
    copied_text: Option<String>,
    permission: Access,
    member: Option<String>,
    revoke_confirm: bool,
    busy: bool,
    message: Option<(bool, String)>,
    epoch: u64,
    focus_before: Option<FocusHandle>,
    subscriptions: Vec<Subscription>,
    scroll: ScrollHandle,
    pub panel_open: bool,
    pub(crate) folded: bool,
    pub(crate) sidebar: crate::sidebar_drag::SidebarDrag,
    pub(crate) batch: bool,
    personal_route: Option<Box<navigation::PersonalRoute>>,
    accept_at: Option<&'static str>,
    share_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    toolbar_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
}

impl Sharing {
    pub fn new(cx: &mut App) -> Self {
        sidebar::install(cx);
        Self { link: cx.new(|cx| TextField::new(tr("粘贴飞记邀请链接", "Paste a Fei Notes invitation link"), cx)),
            email: cx.new(|cx| TextField::new(tr("对方的 Apple 账户邮箱", "Their Apple Account email"), cx)),
            sheet: None, reply: None, prepared: None, permission: Access::ReadOnly, member: None, revoke_confirm: false,
            busy: false, message: None, epoch: 0, original: None, copied_text: None, focus_before: None, subscriptions: vec![],
            scroll: ScrollHandle::new(), panel_open: false, folded: true, sidebar: Default::default(), batch: false, personal_route: None, accept_at: None, share_bounds: Default::default(), toolbar_bounds: Default::default() }
    }
}

fn invitation_text(title: &str, link: &str) -> String {
    format!("{}\n{link}", crate::localized_format!("邀请你在飞记中协作《{}》", "Join me in Fei Notes: {}", display_title(title)))
}

fn caption(text: impl Into<SharedString>) -> Div {
    div().w_full().min_w_0().text_size(px(13.)).line_height(px(20.)).text_color(Theme::text_caption()).child(ui::ui_para(text.into()))
}

fn action(id: impl Into<ElementId>, label: &'static str, enabled: bool) -> Stateful<Div> {
    settings::button(id, label).when(!enabled, |d| d.opacity(0.45).cursor_default())
}

/// A lock reflects Fei's invite-only CloudKit share.
fn feishu_share_button(id: &'static str, enabled: bool) -> Stateful<Div> {
    use crate::feishu_theme::ud;
    let ink = white();
    div().id(id).no_drag().flex_none().h(px(32.)).min_w(px(80.)).px(px(10.)).rounded(px(6.))
        .flex().items_center().justify_center().gap(px(5.))
        .bg(rgb(ud::B600)).text_color(ink).text_size(px(14.))
        .child(ui::icon("excalidraw/locked", 16., ink))
        .child(tr("分享", "Share"))
        .when(enabled, |d| d.cursor_pointer().hover(|s| s.bg(rgb(ud::B700))).active(|s| s.bg(rgb(ud::B800))))
        .when(!enabled, |d| d.opacity(0.45))
}

fn invitation_accent() -> Hsla {
    if settings::feishu() { rgb(crate::feishu_theme::ud::B600).into() }
    else { crate::editor_theme::Theme::primary_500() }
}

fn anchor_control(button: Stateful<Div>, bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>) -> Div {
    div().relative().child(canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {}).absolute().inset_0()).child(button)
}

fn role(access: Option<Access>) -> &'static str {
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

fn member_label(name: &str, id: &str, index: usize) -> String {
    let name = name.trim();
    if name.is_empty() || name == id || matches!(name, "..." | "…") || name.chars().any(char::is_control) {
        crate::localized_format!("参与者 {}", "Participant {}", index + 1)
    } else { name.chars().take(100).collect() }
}

impl Workspace {
    pub(crate) fn sharing_busy(&self) -> bool { self.sharing.busy }
    fn review_original(&mut self, original: String, window: &mut Window, cx: &mut Context<Self>) {
                                if self.sharing.busy { return; }
                                let id = original; let window_id = window.window_handle().window_id();
                                self.close_sharing(window, cx); self.navigate_personal(View::Doc, Some(id.clone()), window, cx);
                                cx.defer(move |cx| {
                                    if let Some(handle) = cx.windows().into_iter().find(|handle| handle.window_id() == window_id).and_then(|handle| handle.downcast::<Workspace>()) {
                                        let _ = handle.update(cx, |ws, _, cx| { if !ws.is_shared() && ws.entries.iter().any(|entry| entry.id == id) { ws.ask_delete(&id, cx); } });
                                    }
                                });
    }

    pub(crate) fn refresh_shared_entry(&self, cx: &mut Context<Self>) {
        if shared::access(&self.store, cx) == Some(Access::Revoked) { return; }
        if let Some(scope) = shared::scope(&self.store, cx).cloned()
            && let Some(entry) = self.entries.iter().find(|entry| entry.id == scope.note) {
            let mut entry = entry.clone();
            if let Some(editor) = &self.editor { entry.body = editor.read(cx).core.doc.plain_text().into(); }
            cloud_sync::activity::update_entry(&scope, &entry, cx);
        }
    }

    pub(crate) fn return_to_shared_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_personal(View::Shared, None, window, cx);
    }

    pub(crate) fn close_sharing_for_navigation(&mut self) {
        self.sharing.panel_open = false;
        if !self.sharing.busy { self.sharing.sheet = None; self.sharing.subscriptions.clear(); self.sharing.epoch += 1; }
    }

    pub(crate) fn close_sharing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sharing.panel_open = false;
        if self.sharing.busy { return; }
        self.sharing.sheet = None;
        self.sharing.epoch += 1;
        self.sharing.subscriptions.clear();
        if let Some(focus) = self.sharing.focus_before.take() { window.focus(&focus); }
        cx.notify();
    }

    fn sharing_sheet(&mut self, sheet: Sheet, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy { return; }
        if self.sharing.sheet.is_none() { self.sharing.focus_before = window.focused(cx); }
        self.sharing.epoch += 1;
        self.sharing.sheet = Some(sheet);
        self.sharing.accept_at = None;
        self.sharing.panel_open = false;
        self.sharing.prepared = None;
        self.sharing.reply = None;
        self.sharing.member = None;
        self.sharing.revoke_confirm = false;
        self.sharing.message = None;
        self.sharing.permission = Access::ReadOnly;
        self.sharing.scroll.set_offset(point(px(0.), px(0.)));
        self.sharing.email.update(cx, |field, cx| field.set_text("", cx));
        self.sharing.link.update(cx, |field, cx| field.set_text("", cx));
        let input = if matches!(self.sharing.sheet, Some(Sheet::Accept)) { &self.sharing.link } else { &self.sharing.email };
        if matches!(self.sharing.sheet, Some(Sheet::Members(_) | Sheet::End(_) | Sheet::EndMany(..))) { window.focus(&self.focus); }
        else { window.focus(&input.focus_handle(cx)); }
        self.sharing.subscriptions = vec![cx.subscribe_in(input, window, |w, _, event: &TextFieldEvent, window, cx| {
            match event {
                TextFieldEvent::Submit => w.submit_sharing(window, cx),
                TextFieldEvent::Cancel => w.close_sharing(window, cx),
                _ => cx.notify(),
            }
        })];
        cx.notify();
    }

    fn current_room(&self, cx: &App) -> Option<Room> {
        let scope = shared::scope(&self.store, cx)?.clone();
        let title = self.entries.iter().find(|entry| entry.id == scope.note).map(|entry| entry.title.clone()).unwrap_or_default();
        Some(Room { scope, title })
    }

    pub(crate) fn sharing_access(&self, cx: &App) -> Option<Access> { shared::access(&self.store, cx) }

    fn sharing_account_ready(&self, cx: &App) -> bool {
        !crate::cloud_restore::local_only() && cloud_sync::status(self.store.root(), cx).is_some_and(|s| !matches!(s.phase, cloud_sync::SyncPhase::AccountChecking | cloud_sync::SyncPhase::AccountChanged | cloud_sync::SyncPhase::SignedOut))
    }

    fn sharing_allowed(&self, cx: &App) -> bool {
        self.sharing_account_ready(cx) && cloud_sync::status(self.store.root(), cx).is_some_and(|status| !status.access_paused)
    }

    fn can_manage_room(&self, room: &Room, cx: &App) -> bool {
        room.scope.owner.is_none() && self.sharing_allowed(cx) && (!self.is_shared()
            || shared::scope(&self.store, cx) == Some(&room.scope) && self.sharing_access(cx) == Some(Access::Owner))
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
            let button = if settings::feishu() {
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
        let cap = if window.viewport_size().width < px(500.) { 0 } else { 3 };
        let bounds = self.sharing.toolbar_bounds.clone();
        let feishu = settings::feishu();
        let presence = ui::grouped_toolbar_control(div().id("doc-share").no_drag().flex_none().min_w(px(crate::app::HEADER_ACTIONS_H)).h(px(crate::app::HEADER_ACTIONS_H))
            .px(px(5.)).flex().items_center().justify_center().gap(px(8.)).cursor_pointer(), 8., Theme::fill_hover, Some(Theme::fill_pressed))
            .when(self.sharing.panel_open, |d| d.bg(Theme::fill_hover()))
            .when(!feishu || people.is_empty() || cap == 0, |d| d.child(ui::participants_icon()))
            .when(feishu, |d| d.children(people.iter().take(cap).enumerate().map(|(ix, person)| div().id(("shared-avatar", ix)).relative().size(px(32.)).rounded_full()
                .flex().items_center().justify_center().bg(person.color().opacity(0.14)).text_color(Theme::text())
                .text_size(px(13.)).child(person.initials.clone())
                .child(div().absolute().right(px(-1.)).bottom(px(-1.)).size(px(8.)).rounded_full().bg(person.color()).border_1().border_color(Theme::bg_body()))
                .tip(person.name.as_ref().map_or("", |name| name.as_ref()), None))))
            .when(feishu && people.len() > cap, |d| {
                let label = if cap == 0 { people.len().to_string() } else { format!("+{}", people.len() - cap) };
                d.child(div().min_w(px(32.)).h(px(32.)).px(px(4.)).rounded_full().flex().items_center().justify_center()
                    .bg(Theme::fill_hover()).text_color(Theme::text()).text_size(px(13.)).child(label))
            })
            .tip(tr("当前参与者", "Current participants"), None)
            .on_click(cx.listener(|w, _, window, cx| {
                w.sharing.panel_open = !w.sharing.panel_open;
                window.focus(&w.focus);
                cx.notify();
            }));
        let anchor = anchor_control(presence, bounds);
        let mut row = div().flex().flex_none().items_center().gap(px(if feishu { 10. } else { 4. })).child(anchor);
        if let Some(room) = self.current_room(cx).filter(|r| r.scope.owner.is_none()) {
            let enabled = !self.sharing.busy && self.can_manage_room(&room, cx);
            let share = if feishu {
                feishu_share_button("doc-share-invite", enabled)
            } else {
                div().id("doc-share-invite").no_drag().flex_none().size(px(crate::app::HEADER_ACTIONS_H))
                    .flex().items_center().justify_center().rounded(px(7.)).child(ui::share_toolbar_icon())
                    .when(enabled, |d| d.cursor_pointer()).when(!enabled, |d| d.opacity(0.45))
            };
            let share = share.when(enabled, |d| d.on_click(cx.listener(move |w, _, window, cx| w.open_invitation(room.clone(), window, cx))))
                .tip(tr("邀请参与者", "Invite participants"), None);
            row = row.child(anchor_control(share, self.sharing.share_bounds.clone()));
        }
        Some(row)
    }

    fn online_collaborators<'a>(&self, cx: &'a App) -> &'a [cloud_sync::presence::Collaborator] {
        self.current.as_ref().and_then(|id| cloud_sync::presence::view(self.store.root(), cx)?.collaborators.get(id))
            .map(Vec::as_slice).unwrap_or_default()
    }

    pub(crate) fn render_sharing_toolbar_panel(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.sharing.panel_open || self.view != View::Doc { return None; }
        let room = self.current_room(cx)?;
        let anchor = self.sharing.toolbar_bounds.get()?;
        let access = self.sharing_access(cx);
        let people = self.online_collaborators(cx);
        let status = cloud_sync::status(self.store.root(), cx);
        let state = if access == Some(Access::Revoked) { tr("共享访问已结束", "Shared access has ended") }
            else if !self.sharing_allowed(cx) { tr("协作已暂停，请检查 iCloud 状态。", "Collaboration is paused. Check iCloud status.") }
            else if status.is_some_and(|s| s.phase == cloud_sync::SyncPhase::Offline) { tr("当前离线，显示本机内容。", "Offline. Showing local content.") }
            else if status.is_some_and(|s| matches!(s.phase, cloud_sync::SyncPhase::Failed | cloud_sync::SyncPhase::Account | cloud_sync::SyncPhase::Quota)) { tr("同步遇到问题，请查看 iCloud 状态。", "Sync needs attention. Check iCloud status.") }
            else if status.is_some_and(|s| s.phase == cloud_sync::SyncPhase::Ready && s.downloads == 0 && s.uploads == 0 && s.held == 0 && s.attachments == 0) {
                tr("内容已同步", "Content is synced")
            } else { tr("正在获取或同步更新", "Checking or syncing updates") };
        let feishu = settings::feishu();
        let heading = if feishu { tr("当前参与者", "Current participants") } else { tr("共享备忘录", "Shared note") };
        let mut body = div().flex().flex_col().p(px(14.)).gap(px(8.))
            .child(div().flex().items_center().gap(px(7.))
                .when(!feishu, |d| d.child(ui::participants_icon()))
                .child(div().text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).child(heading))
                .when(feishu, |d| d.child(div().rounded(px(5.)).px(px(5.)).bg(Theme::fill_hover()).text_size(px(12.)).child(people.len().to_string()))))
            .child(div().truncate().text_size(px(13.)).text_color(Theme::text_caption()).child(display_title(&room.title)))
            .child(caption(format!("{} · {state}", role(access))));
        if !feishu {
            body = body.child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD)
                .child(crate::localized_format!("当前参与者（{}）", "Current participants ({})", people.len())));
        }
        if people.is_empty() {
            body = body.child(caption(tr("当前没有其他协作者在编辑", "No other collaborators are editing now")));
        } else {
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
                        .child(div().text_size(px(12.)).text_color(Theme::text_caption()).child(tr("正在编辑", "Editing")))
                }));
            body = body.child(list);
        }
        body = body.child(div().h(px(1.)).bg(Theme::line_divider()));
        if room.scope.owner.is_none() {
            let invite = room.clone();
            let members = room.clone();
            body = body.child(div().flex().gap(px(8.)).items_center()
                .child(action("collab-invite", tr("邀请参与者…", "Invite participants…"), self.can_manage_room(&invite, cx))
                    .on_click(cx.listener(move |w, _, window, cx| w.open_invitation(invite.clone(), window, cx))))
                .child(action("collab-members", tr("成员与权限…", "Members and access…"), !self.sharing.busy)
                    .on_click(cx.listener(move |w, _, window, cx| w.open_member_management(members.clone(), window, cx)))));
        } else {
            let end_room = room.clone();
            body = body.child(caption(tr("成员权限由所有者管理。", "The owner manages member access.")))
                .child(action("collab-end", tr("离开共享…", "Leave share…"), self.sharing_account_ready(cx))
                    .on_click(cx.listener(move |w, _, window, cx| w.open_end_shared(end_room.clone(), window, cx))));
        }
        if !self.sharing_allowed(cx) {
            body = body.child(action("collab-cloud-settings", tr("查看 iCloud 状态", "View iCloud status"), true)
                .on_click(cx.listener(|w, _, _, cx| { w.sharing.panel_open = false; w.open_settings(settings::Page::Library, cx); })));
        }
        Some(deferred(anchored().anchor(Corner::TopRight).position(point(anchor.right(), anchor.bottom() + px(4.)))
            .snap_to_window_with_margin(px(8.)).child(ui::popover().id("doc-sharing-panel").occlude()
                .w((window.viewport_size().width - px(16.)).min(px(if feishu { 340. } else { 300. })))
                .max_h((window.viewport_size().height - anchor.bottom() - px(12.)).max(px(100.)))
                .overflow_y_scroll().child(body).on_mouse_down_out(cx.listener(move |w, event: &MouseDownEvent, _, cx| {
                    if !anchor.contains(&event.position) { w.sharing.panel_open = false; cx.notify(); }
                })))).with_priority(2).into_any_element())
    }

    pub(crate) fn create_shared_note(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_shared() || self.store.local_path(id).is_some() || self.sharing.busy || !self.flush(cx) { return; }
        self.sharing_sheet(Sheet::Create(id.into()), window, cx);
    }

    fn open_invitation(&mut self, room: Room, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_manage_room(&room, cx) { return; }
        self.sharing_sheet(Sheet::Invite(room), window, cx);
    }

    fn open_member_management(&mut self, room: Room, window: &mut Window, cx: &mut Context<Self>) {
        if room.scope.owner.is_some() { return; }
        self.sharing_sheet(Sheet::Members(room), window, cx);
        self.manage_shared_note("members", String::new(), None, window, cx);
    }

    fn sheet_room(&self) -> Option<&Room> {
        match &self.sharing.sheet { Some(Sheet::Members(room) | Sheet::Invite(room) | Sheet::End(room)) => Some(room), _ => None }
    }

    pub(crate) fn render_sharing_sheet(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let sheet = self.sharing.sheet.as_ref()?;
        let busy = self.sharing.busy;
        let invitation = matches!(sheet, Sheet::Create(_) | Sheet::Invite(_));
        let feishu = settings::feishu();
        let accent = invitation_accent();
        let title = match sheet { Sheet::Create(_) => tr("共享笔记", "Share note"), Sheet::Accept => tr("接受邀请", "Accept invitation"),
            Sheet::Members(_) => tr("成员与权限", "Members and access"), Sheet::Invite(_) if feishu => tr("分享文档", "Share document"),
            Sheet::Invite(_) => tr("共享备忘录", "Share note"),
            Sheet::EndMany(..) => tr("移除共享文档", "Remove shared documents"),
            Sheet::End(room) => if room.scope.owner.is_none() { tr("删除共享文档", "Delete shared document") } else { tr("离开共享", "Leave share") } };
        let mut body = div().id("sharing-sheet-body").track_scroll(&self.sharing.scroll).overflow_y_scroll().flex_1().min_h_0()
            .px(px(24.)).pb(px(20.)).flex().flex_col().gap(px(14.));
        match sheet {
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
                    .child(caption(if room.scope.owner.is_none() {
                        tr("删除后，所有人都将无法访问这篇共享文档。", "After deletion, no one will be able to access this shared document.")
                    } else {
                        tr("离开后，你将无法访问这篇文档，其他人不受影响。", "You will lose access to this document. Other people will keep access.")
                    }))
                    .child(caption(tr("已下载的内容会保留在你的回收站，可恢复为普通文档。", "Downloaded content stays in your Trash and can be restored as a personal document.")));
            }
            Sheet::Create(id) => {
                let title = self.entries.iter().find(|entry| &entry.id == id).map(|e| display_title(&e.title)).unwrap_or_default();
                body = body.child(div().truncate().text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).child(title))
                    .child(div().text_size(px(14.)).child(tr("与他人协作", "Collaborate with others")))
                    .child(caption(tr("将创建一篇独立的共享笔记。之后请在共享笔记中继续编辑，原笔记保留。", "This creates a separate shared note. Continue editing there; your original note is kept.")))
                    .child(self.invitation_fields(window, cx));
                if crate::cloud_restore::local_only() {
                    body = body.child(action("share-enable-cloud", tr("开启 iCloud 同步…", "Enable iCloud sync…"), !busy)
                        .on_click(cx.listener(|w, _, window, cx| { w.close_sharing(window, cx); w.open_settings(settings::Page::Library, cx); })));
                }
                if self.sharing.prepared.is_some() { body = body.child(caption(tr("共享副本已保留。重试会继续使用这一篇。", "The shared copy is kept. Retrying will use the same note."))); }
                let export = id.clone();
                body = body.child(div().h(px(1.)).bg(Theme::line_divider()))
                    .child(action("share-send-copy", tr("发送静态副本…", "Send a static copy…"), !busy)
                        .on_click(cx.listener(move |w, _, window, cx| { if w.sharing.busy { return; } w.close_sharing(window, cx); w.open_export_md(&export, cx); })))
                    .child(caption(tr("导出 Markdown 副本，接收者不会收到后续更新。", "Export a Markdown copy. Recipients will not receive later updates.")));
            }
            Sheet::Accept => {
                body = body.child(caption(tr("粘贴邀请链接。请使用受邀的 Apple 账户。", "Paste the invitation link and use the invited Apple Account.")))
                    .child(TextField::framed(&self.sharing.link, px(360.), window, cx).w_full().max_w_full());
            }
            Sheet::Invite(room) => {
                body = body.child(div().flex_none().truncate().text_size(px(16.)).line_height(px(22.)).child(display_title(&room.title)))
                    .child(self.invitation_fields(window, cx));
            }
            Sheet::Members(room) => {
                let allowed = self.can_manage_room(room, cx);
                body = body.child(div().truncate().text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).child(display_title(&room.title)))
                    .child(line(tr("你", "You"), tr("所有者", "Owner"), div()))
                    .child(action("members-add", tr("添加参与者…", "Add participants…"), allowed && !busy)
                        .on_click(cx.listener(|w, _, window, cx| {
                            if let Some(room) = w.sheet_room().cloned() { w.open_invitation(room, window, cx); }
                        })));
                if let Some(reply) = &self.sharing.reply {
                    if reply.members.is_empty() { body = body.child(caption(tr("尚未邀请其他参与者", "No other participants invited yet"))); }
                    for (index, member) in reply.members.iter().enumerate() {
                        let id = member.id.clone();
                        let selected = self.sharing.member.as_deref() == Some(&member.id);
                        let status = if member.accepted { tr("已加入", "Joined") } else { tr("待接受", "Invitation pending") };
                        body = body.child(div().id(SharedString::from(format!("share-member-{}", member.id))).cursor_pointer()
                            .child(line(member_label(&member.name, &member.id, index), format!("{} · {status}", role(Some(member.permission))), ui::icon("chevron-right", 14., Theme::icon())))
                            .on_click(cx.listener(move |w, _, _, cx| {
                                if w.sharing.busy { return; }
                                w.sharing.member = if selected { None } else { Some(id.clone()) }; w.sharing.revoke_confirm = false; cx.notify();
                            })));
                        if selected {
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
                    if let Some(link) = reply.url.as_deref().and_then(|url| links::branded(url).ok()) {
                        body = body.child(action("copy-share-link", tr("复制邀请链接", "Copy invitation link"), !busy)
                            .on_click(cx.listener(move |w, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
                                w.show_toast(tr("已复制邀请链接", "Invitation link copied"), cx);
                            })));
                    }
                }
                if let Some(original) = self.sharing.original.clone() {
                    body = body.child(caption(tr("原文档仍在“我的文档”中。之后请在共享文档中继续编辑。", "Your original is still in My documents. Continue editing the shared document.")))
                        .child(action("review-original", tr("将原文档移到回收站…", "Move original to Trash…"), !busy)
                            .on_click(cx.listener(move |w, _, window, cx| {
                                w.review_original(original.clone(), window, cx);
                            })));
                }
                let remove = room.clone();
                body = body.child(action("members-delete-document", tr("删除共享文档…", "Delete shared document…"), !busy)
                    .text_color(Theme::danger()).on_click(cx.listener(move |w, _, window, cx| w.open_end_shared(remove.clone(), window, cx))));
                body = body.child(caption(tr("仅受邀用户可访问。转发链接不会授予其他人访问权限。", "Only invited people can access this note. Forwarding the link does not grant access.")))
                    .child(action("members-refresh", tr("刷新成员", "Refresh members"), !busy)
                        .on_click(cx.listener(|w, _, window, cx| w.manage_shared_note("members", String::new(), None, window, cx))));
            }
        }
        if !self.sharing_allowed(cx) {
            body = body.child(caption(tr("协作已暂停。请在文档库设置中检查 iCloud 账户和云功能。", "Collaboration is paused. Check your iCloud account and cloud access in Documents settings.")));
        }
        let feedback = div().px(px(24.)).flex_none()
            .when(busy, |d| d.child(caption(tr("正在连接 iCloud，请稍候…", "Contacting iCloud. Please wait…"))).pb(px(10.)))
            .when_some(self.sharing.message.as_ref().filter(|(error, _)| *error), |d, (error, message)| d.pb(px(10.)).child(
                div().id("sharing-message").text_size(px(13.)).line_height(px(20.))
                    .text_color(if *error { Theme::danger() } else { Theme::text_caption() }).child(ui::ui_para(message.clone()))));
        let submit = match sheet { Sheet::Create(_) => Some(tr("创建并邀请", "Create and invite")), Sheet::Accept => Some(tr("接受并打开", "Accept and open")),
            Sheet::Invite(_) => Some(tr("邀请", "Invite")), Sheet::EndMany(..) => Some(tr("确认移除", "Remove documents")), Sheet::End(room) => Some(if room.scope.owner.is_none() { tr("删除共享文档", "Delete shared document") } else { tr("离开共享", "Leave share") }), _ => None };
        let input_ok = if matches!(sheet, Sheet::End(_) | Sheet::EndMany(..)) { true } else if matches!(sheet, Sheet::Accept) { !self.sharing.link.read(cx).text.trim().is_empty() }
            else { !self.sharing.email.read(cx).text.trim().is_empty() };
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
                .when(busy || !input_ok || !(if matches!(sheet, Sheet::End(_) | Sheet::EndMany(..)) { self.sharing_account_ready(cx) } else { self.sharing_allowed(cx) }), |d| d.opacity(0.45).cursor_default())
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
            return Some(deferred(anchored().anchor(Corner::TopLeft).position(pos).snap_to_window_with_margin(px(8.)).child(form)).with_priority(5).into_any_element());
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
            let popup = anchored().anchor(Corner::TopRight).position(point(bounds.right(), bounds.bottom() + px(4.)))
                .snap_to_window_with_margin(px(8.)).child(form);
            return Some(deferred(div().absolute().inset_0()
                .child(div().id("share-dismiss").absolute().inset_0().occlude()
                    .on_mouse_down(MouseButton::Left, cx.listener(|w, _, window, cx| { cx.stop_propagation(); w.close_sharing(window, cx); })))
                .child(popup)).with_priority(5).into_any_element());
        }
        let height = match sheet { Sheet::Create(_) => 510., Sheet::Accept => 290., Sheet::Members(_) => 580., Sheet::Invite(_) => 345., Sheet::End(_) => 290., Sheet::EndMany(..) => 380. };
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

    fn confirm_shared_removal(&mut self, member: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(room) = self.sheet_room() else { return };
        if self.sharing.busy || !self.can_manage_room(room, cx) || self.sharing.member.as_deref() != Some(&member) { return; }
        if !self.sharing.revoke_confirm { self.sharing.revoke_confirm = true; cx.notify(); }
        else { self.manage_shared_note("revoke", member, None, window, cx); }
    }

    fn submit_sharing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy { return; }
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
        self.navigate_shared(room.clone(), None, window, cx);
    }

    fn accept_shared_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn manage_shared_note(&mut self, action: &'static str, person: String, access: Option<Access>, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy { return; }
        let Some(room) = self.sheet_room().cloned() else { return };
        if action == "members" {
            if room.scope.owner.is_some() { return; }
        } else if !self.can_manage_room(&room, cx) { return; }
        let epoch = self.begin_sharing_request(cx);
        let root = self.store.root().to_path_buf();
        cx.spawn_in(window, async move |this, cx| {
            let request_room = room.clone();
            let result = cx.background_executor().spawn(async move { shared::manage(&root, &request_room, action, &person, access) }).await;
            let _ = this.update_in(cx, |w, window, cx| match result {
                Ok(reply) => {
                    if !w.finish_sharing_request(epoch, None, cx) { return; }
                    w.sharing.reply = Some(reply); w.sharing.member = None; w.sharing.revoke_confirm = false;
                    if action == "invite" {
                        if w.sharing.original.is_some() && w.sharing.copied_text.is_none()
                            && let Some(link) = w.sharing.reply.as_ref().and_then(|reply| reply.url.as_deref()).and_then(|url| links::branded(url).ok()) {
                            let text = invitation_text(&room.title, &link);
                            if !crate::devtools::scripted() { cx.write_to_clipboard(ClipboardItem::new_string(text.clone())); }
                            w.sharing.copied_text = Some(text);
                        }
                        w.sharing.sheet = Some(Sheet::Members(room));
                        window.focus(&w.focus);
                        w.sharing.email.update(cx, |field, cx| field.set_text("", cx));
                        w.show_toast(if w.sharing.copied_text.is_some() { tr("邀请文案和链接已复制", "Invitation message and link copied") } else { tr("邀请已准备好，请复制链接发给对方。", "Invitation ready. Copy the link and send it.") }, cx);
                    }
                }
                Err(error) => { w.finish_sharing_request(epoch, Some(error), cx); }
            });
        }).detach();
    }

    fn publish_shared_copy(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let email = self.sharing.email.read(cx).text.trim().to_owned();
        if email.is_empty() || !self.flush(cx) { return; }
        let Ok(doc) = self.store.load(id) else { return };
        let root = crate::store::Store::root_dir();
        let prepared = self.sharing.prepared.clone();
        let permission = self.sharing.permission;
        let original = id.to_owned();
        let epoch = self.begin_sharing_request(cx);
        cx.spawn_in(window, async move |this, cx| {
            let copy_root = root.clone();
            let staged = cx.background_executor().spawn(async move {
                prepared.map_or_else(|| shared::prepare_copy(&copy_root, crate::model::display_title(&doc.title.text), &doc, ""), Ok)
            }).await;
            let room = match staged {
                Ok(room) => room,
                Err(error) => { let _ = this.update_in(cx, |w, _, cx| { w.finish_sharing_request(epoch, Some(error), cx); }); return; }
            };
            // Keep the room before publishing. A later upload failure retries this exact copy.
            if this.update_in(cx, |w, _, _| { w.sharing.prepared = Some(room.clone()); }).is_err() { return; }
            let upload_room = room.clone();
            let published = cx.background_executor().spawn(async move { shared::ensure_created(&root, &upload_room) }).await;
            if let Err(error) = published {
                let _ = this.update_in(cx, |w, _, cx| { w.finish_sharing_request(epoch, Some(error), cx); }); return;
            }
            let _ = this.update_in(cx, |w, window, cx| {
                if !w.finish_sharing_request(epoch, None, cx) { return; }
                w.close_sharing(window, cx);
                w.sharing.original = Some(original);
                w.navigate_shared(room, Some((email, permission)), window, cx);
            });
        }).detach();
    }
}
