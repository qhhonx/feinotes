// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
use super::{ErrorKind, Outcome, Plan, Reply};
use crate::{
    app::Workspace,
    design::{self, Badge, Button, Card, Dialog, Item, Radio, Separator},
    settings::{self, tr},
    ui::ui_para,
};
use gpui::{prelude::FluentBuilder, *};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Offer {
    plan: Plan,
    price: String,
    saving_percent: Option<u32>,
}

pub(crate) struct Panel {
    offers: Vec<Offer>,
    active: Option<Plan>,
    next: Option<Plan>,
    selected: Plan,
    working: bool,
    message: Option<(&'static str, bool)>,
    preview: bool,
}

#[derive(Clone, Copy)]
enum Action { Load, Buy(Plan), Restore }

impl Action {
    fn code(self) -> i32 {
        match self { Self::Load => 0, Self::Buy(Plan::Annual) => 1, Self::Buy(Plan::Monthly) => 2, Self::Restore => 3 }
    }

    fn pending_label(self) -> &'static str {
        match self {
            Self::Load => tr("正在查询 App Store…", "Checking the App Store…"),
            Self::Buy(_) => tr("正在等待 App Store 确认…", "Waiting for App Store confirmation…"),
            Self::Restore => tr("正在恢复购买…", "Restoring purchases…"),
        }
    }
}

impl Plan {
    fn name(self) -> &'static str {
        match self { Self::Annual => tr("年付", "Annual"), Self::Monthly => tr("月付", "Monthly") }
    }
}

impl Panel {
    fn new(preview: bool) -> Self {
        let offers = if preview {
            vec![
                Offer { plan: Plan::Annual, price: "¥12.00".into(), saving_percent: Some(50) },
                Offer { plan: Plan::Monthly, price: "¥2.00".into(), saving_percent: None },
            ]
        } else { Vec::new() };
        Self { offers, active: None, next: None, selected: Plan::Annual, working: false, message: None, preview }
    }

    fn selected_offer(&self) -> Option<&Offer> {
        self.offers.iter().find(|offer| offer.plan == self.selected)
    }

    fn can_buy(&self) -> bool {
        !self.working && self.selected_offer().is_some() && Some(self.selected) != self.active && Some(self.selected) != self.next
    }

    fn purchase_label(&self) -> String {
        let name = self.selected.name();
        if Some(self.selected) == self.active { return tr("当前方案", "Current plan").into(); }
        if Some(self.selected) == self.next { return crate::localized_format!("已安排切换{}", "{} change scheduled", name); }
        if self.active.is_some() { return crate::localized_format!("切换到{}", "Switch to {}", name); }
        tr("通过 App Store 继续", "Continue with the App Store").into()
    }

    fn billing(&self, offer: &Offer) -> String {
        if Some(offer.plan) == self.active {
            return tr("可在 App Store 查看续订状态或管理订阅。", "Check renewal status or manage your plan in the App Store.").into();
        }
        let renewal = match offer.plan {
            Plan::Annual => crate::localized_format!("{} 每年自动续订，可在 App Store 管理或取消。", "{} per year, auto-renewing. Manage or cancel in the App Store.", offer.price),
            Plan::Monthly => crate::localized_format!("{} 每月自动续订，可在 App Store 管理或取消。", "{} per month, auto-renewing. Manage or cancel in the App Store.", offer.price),
        };
        if self.active == Some(Plan::Monthly) && offer.plan == Plan::Annual && self.next != Some(Plan::Annual) {
            let change = tr("当前月付继续有效，年付预计在下次续订时生效。以 App Store 确认为准。",
                "Monthly stays active. Annual is expected to start at your next renewal, as confirmed by the App Store.");
            format!("{change}\n{renewal}")
        } else { renewal }
    }

    fn apply(&mut self, action: Action, result: anyhow::Result<Reply>) {
        self.working = false;
        self.message = None;
        let Ok(reply) = result else {
            self.message = Some((request_error(action, None), true));
            return;
        };
        if reply.outcome == Some(Outcome::Cancelled) { return; }
        if matches!(action, Action::Load) {
            self.offers = reply.offers.unwrap_or_default();
            self.selected = reply.next.unwrap_or(Plan::Annual);
            if self.offers.is_empty() { self.message = Some((request_error(action, None), true)); }
        }
        if reply.error.is_some() && !matches!(action, Action::Load) {
            self.message = Some((request_error(action, reply.error_kind), true));
            return;
        }
        self.active = reply.active.and_then(|active| active.plan_now());
        self.next = reply.next;
        match reply.outcome {
            Some(Outcome::Purchased) => self.message = Some((tr("购买已确认。", "Purchase confirmed."), false)),
            Some(Outcome::Pending) => self.message = Some((tr("购买正在等待批准。", "Purchase pending approval."), false)),
            Some(Outcome::Scheduled) => {
                if let Action::Buy(plan) = action && self.active != Some(plan) && self.next.is_none() { self.next = Some(plan); }
            }
            Some(Outcome::Restored) => self.message = Some((if self.active.is_some() {
                tr("已恢复有效订阅。", "Active subscription restored.")
            } else { tr("当前 App Store 账户没有有效订阅。", "No active subscription for this App Store account.") }, false)),
            _ => {}
        }
    }

    fn preview_action(&mut self, action: Action) {
        self.message = None;
        match action {
            Action::Buy(plan) if self.active.is_none() => self.active = Some(plan),
            Action::Buy(plan) => self.next = Some(plan),
            Action::Restore => self.message = Some((tr("本地预览不查询 App Store 购买记录。", "The local preview does not query App Store purchases."), false)),
            Action::Load => {}
        }
    }
}

fn request_error(action: Action, kind: Option<ErrorKind>) -> &'static str {
    if matches!(action, Action::Load) { return tr("暂时无法获取订阅方案。", "Plans are temporarily unavailable."); }
    match kind {
        Some(ErrorKind::Unavailable) => tr("该方案暂不可用。", "This plan is temporarily unavailable."),
        Some(ErrorKind::MissingWindow) => tr("请重新打开订阅弹窗后再试。", "Reopen the subscription sheet and try again."),
        Some(ErrorKind::Unverified) => tr("暂时无法确认购买结果。可在 App Store 查看订阅。", "The purchase could not be confirmed. Check your subscription in the App Store."),
        Some(ErrorKind::Busy) => tr("正在处理另一笔订阅请求。", "Another subscription request is in progress."),
        None => tr("暂时无法连接 App Store，请重试。", "Could not connect to the App Store. Try again."),
    }
}

impl Workspace {
    pub(crate) fn open_mac_subscription(&mut self, preview: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.subscription.panel.is_some() { return; }
        self.subscription.panel = Some(Panel::new(preview));
        if !preview { self.subscription_action(Action::Load, window, cx); }
        cx.notify();
    }

    fn close_mac_subscription(&mut self, cx: &mut Context<Self>) {
        if self.subscription.panel.as_ref().is_some_and(|panel| panel.working) { return; }
        self.subscription.panel = None;
        cx.notify();
    }

    fn subscription_action(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = &mut self.subscription.panel else { return };
        if panel.working || matches!(action, Action::Buy(plan) if plan != panel.selected || !panel.can_buy()) { return; }
        if panel.preview {
            panel.preview_action(action);
            cx.notify();
            return;
        }
        panel.working = true;
        panel.message = Some((action.pending_label(), false));
        cx.notify();
        let receiver = super::request_from_window(action.code(), window);
        cx.spawn_in(window, async move |this, cx| {
            let result = super::receive(receiver).await;
            let _ = this.update_in(cx, |w, _, cx| {
                if let Some(panel) = &mut w.subscription.panel { panel.apply(action, result); }
                cx.notify();
            });
        }).detach();
    }

    pub(crate) fn render_mac_subscription(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let panel = self.subscription.panel.as_ref()?;
        let muted = design::color::muted_foreground();
        let mut body = div().id("subscription-body").min_h_0().flex().flex_col().gap(px(12.)).overflow_y_scroll();
        if let Some(active) = panel.active {
            let status = if let Some(next) = panel.next {
                crate::localized_format!("{}当前有效，{}将在下次续订时生效。", "{} is active. {} starts at your next renewal.", active.name(), next.name())
            } else { crate::localized_format!("{}方案正在生效。", "Your {} plan is active.", active.name()) };
            body = body.child(div().text_color(muted).child(ui_para(status)));
        }

        let label = tr("订阅方案", "Subscription plans");
        if panel.offers.is_empty() {
            if !panel.working {
                body = body.child(Button::outline("subscription-reload").label(tr("重试加载方案", "Reload plans"))
                    .on_click(cx.listener(|w, _, window, cx| w.subscription_action(Action::Load, window, cx))));
            }
        } else {
            let mut plans = Card::new("subscription-plans");
            for (index, offer) in panel.offers.iter().enumerate() {
                let plan = offer.plan;
                let selected = panel.selected == plan;
                let price = match plan {
                    Plan::Annual => crate::localized_format!("{} / 年", "{} / year", offer.price),
                    Plan::Monthly => crate::localized_format!("{} / 月", "{} / month", offer.price),
                };
                let detail = if Some(plan) == panel.active { Some(tr("当前方案", "Current plan")) }
                    else if Some(plan) == panel.next { Some(tr("下次续订生效", "Starts at next renewal")) } else { None };
                if index > 0 { plans = plans.child(Separator::new()); }
                let row = div().id(("subscription-plan", index)).relative().flex().items_center().pl(px(14.))
                    .child(Radio::new(selected).disabled(panel.working))
                    .child(Item::new(plan.name()).flex_1().min_w_0().description(detail)
                        .children(offer.saving_percent.map(|percent| Badge::new(crate::localized_format!("省 {}%", "Save {}%", percent))))
                        .child(div().text_color(muted).child(price)))
                    .when(!panel.working, |d| d.cursor_pointer().hover(|s| s.bg(design::color::accent())).on_click(cx.listener(move |w, _, _, cx| {
                        if let Some(panel) = &mut w.subscription.panel && !panel.working { panel.selected = plan; panel.message = None; cx.notify(); }
                    })))
                    .children(crate::devtools::row_probe(plan.name()));
                plans = plans.child(row);
            }
            body = body.child(div().font_weight(FontWeight::SEMIBOLD).child(label)).child(plans)
                .children(panel.selected_offer().map(|offer| div().text_color(muted).child(ui_para(panel.billing(offer)))));
        }
        body = body.children(panel.message.map(|(message, error)| div()
            .text_color(if error { design::color::destructive() } else { muted }).child(ui_para(message))));
        if panel.preview {
            body = body.child(div().text_color(muted).child(tr("本地预览 · 不会产生购买", "Local preview · No purchases")));
        }

        let accent = if settings::interface_feishu() { rgb(crate::feishu_theme::ud::B600).into() }
            else { crate::themes::interface().token("primary_500") };
        let buy = panel.selected;
        let restore = tr("恢复购买", "Restore purchases");
        let mut links = div().flex().flex_wrap().gap(px(8.));
        for (key, fallback, title) in [
            (c"FeiPrivacyURL", "https://feinotes.app/privacy/", tr("隐私政策", "Privacy policy")),
            (c"FeiTermsURL", "https://www.apple.com/legal/internet-services/itunes/dev/stdeula/", tr("使用条款", "Terms of use")),
        ] {
            if let Some(url) = settings::release_url(key).or_else(|| panel.preview.then(|| fallback.to_owned())) {
                links = links.child(Button::link(title).label(title).disabled(panel.working)
                    .when(!panel.working, |button| button.on_click(move |_, _, cx| cx.open_url(&url))));
            }
        }
        let manage = tr("管理订阅", "Manage subscriptions");
        let purchase_label = panel.purchase_label();
        let actions = div().flex().flex_wrap().items_center().gap(px(8.))
            .child(Button::outline("subscription-restore").label(restore).disabled(panel.working)
                .children(crate::devtools::row_probe(restore))
                .on_click(cx.listener(|w, _, window, cx| w.subscription_action(Action::Restore, window, cx))))
            .when(panel.active.is_some(), |d| d.child(Button::link("subscription-manage").label(manage).disabled(panel.working)
                .on_click(cx.listener(|w, _, _, cx| {
                    if let Some(panel) = &mut w.subscription.panel && !panel.working {
                        if panel.preview {
                            panel.message = Some((tr("本地预览不会更改 App Store 订阅。", "The local preview does not change App Store subscriptions."), false));
                            cx.notify();
                        } else { cx.open_url("https://apps.apple.com/account/subscriptions"); }
                    }
                }))))
            .child(div().flex_1())
            .when(!panel.offers.is_empty(), |d| d.child(Button::primary("subscription-buy").label(purchase_label.clone())
                .accent(accent, crate::cloud_sync::presence::badge_ink(accent)).disabled(!panel.can_buy())
                .children(crate::devtools::row_probe(purchase_label))
                .on_click(cx.listener(move |w, _, window, cx| w.subscription_action(Action::Buy(buy), window, cx)))));
        let ws = cx.entity().downgrade();
        let dialog = Dialog::new("subscription-dialog")
            .title(tr("飞记云功能", "Fei Notes Cloud"))
            .description(tr("在 Mac、iPhone 和 iPad 间同步文档，并邀请他人共同编辑。", "Sync documents across Mac, iPhone, and iPad and invite others to edit together."))
            .w(px(520.)).max_w(window.viewport_size().width - px(32.)).max_h(window.viewport_size().height - px(32.))
            .when(!panel.working, |d| d.closable(cx.listener(|w, _, _, cx| w.close_mac_subscription(cx))))
            .keys(&self.dialog_focus, move |_, cx| {
                let _ = ws.update(cx, |w, cx| w.close_mac_subscription(cx));
            }, None::<fn(&mut Window, &mut App)>)
            .child(body).footer(div().flex().flex_col().gap(px(12.)).child(actions).child(links));
        Some(deferred(design::scrim().id("subscription-mask").occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation()).child(dialog)).with_priority(10).into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, Outcome, Panel, Plan, Reply};

    fn active(plan: Plan) -> super::super::ActiveSubscription {
        super::super::ActiveSubscription { plan, valid_until: 4_102_444_800_000 }
    }

    #[test]
    fn scheduled_plan_does_not_replace_current_entitlement() {
        let mut panel = Panel::new(true);
        panel.working = true;
        panel.apply(Action::Buy(Plan::Annual), Ok(Reply {
            active: Some(active(Plan::Monthly)), offers: None, next: None,
            outcome: Some(Outcome::Scheduled), error: None, error_kind: None,
        }));
        assert!(panel.active == Some(Plan::Monthly) && panel.next == Some(Plan::Annual));
        assert!(!panel.can_buy());

        panel.apply(Action::Buy(Plan::Annual), Ok(Reply {
            active: None, offers: None, next: None,
            outcome: Some(Outcome::Cancelled), error: None, error_kind: None,
        }));
        assert!(panel.active == Some(Plan::Monthly) && panel.next == Some(Plan::Annual));
    }

    #[test]
    fn restored_entitlement_is_projected_into_the_panel() {
        let mut panel = Panel::new(true);
        panel.apply(Action::Restore, Ok(Reply {
            active: Some(active(Plan::Annual)), offers: None, next: None,
            outcome: Some(Outcome::Restored), error: None, error_kind: None,
        }));
        assert!(panel.active == Some(Plan::Annual));
        assert!(!panel.can_buy());
        assert!(panel.message.is_some_and(|(_, error)| !error));
    }
}
