// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
use anyhow::{Context, Result};
use gpui::{App, Global};
use serde::Deserialize;
use std::ffi::{CStr, c_char};
use std::sync::OnceLock;

unsafe extern "C" {
    fn quiet_cloud_subscription_available() -> bool;
    fn quiet_cloud_watch_subscription(on_change: extern "C" fn());
    fn quiet_cloud_subscription(action: i32, context: usize, complete: extern "C" fn(usize, *const c_char));
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Plan { Annual, Monthly }

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActiveSubscription {
    plan: Plan,
    valid_until: u64,
}

impl ActiveSubscription {
    pub fn remaining(self) -> Option<std::time::Duration> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?;
        std::time::Duration::from_millis(self.valid_until).checked_sub(now).filter(|duration| !duration.is_zero())
    }

    pub fn plan_now(self) -> Option<Plan> { self.remaining().map(|_| self.plan) }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Offer {
    pub plan: Plan,
    pub price: String,
    pub equivalent_monthly: Option<String>,
    pub saving_percent: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Outcome { Loaded, Purchased, Pending, Cancelled, Restored }

#[derive(Clone, Copy)]
pub(crate) enum Action { Load, Purchase(Plan), Restore, Status }

#[derive(Deserialize)]
pub(crate) struct Reply {
    pub offers: Option<Vec<Offer>>,
    pub active: Option<ActiveSubscription>,
    pub outcome: Option<Outcome>,
    pub error: Option<String>,
}

pub(crate) fn available() -> bool { unsafe { quiet_cloud_subscription_available() } }

pub(crate) struct PurchaseChanged;
impl Global for PurchaseChanged {}

static PURCHASE_EVENTS: OnceLock<async_channel::Sender<()>> = OnceLock::new();

extern "C" fn purchase_changed() {
    if let Some(sender) = PURCHASE_EVENTS.get() { let _ = sender.try_send(()); }
}

pub(crate) fn watch(cx: &mut App) {
    if !available() { return; }
    let (sender, receiver) = async_channel::bounded(1);
    if PURCHASE_EVENTS.set(sender).is_err() { return; }
    // AppKit starts the StoreKit observer on the main thread.
    unsafe { quiet_cloud_watch_subscription(purchase_changed) };
    cx.spawn(async move |cx| {
        while receiver.recv().await.is_ok() {
            if cx.update(|cx| {
                crate::cloud_sync::access_changed(cx);
                cx.set_global(PurchaseChanged);
            }).is_err() { break; }
        }
    }).detach();
}

extern "C" fn complete(context: usize, json: *const c_char) {
    // The bridge returns the context exactly once and keeps the JSON alive through this callback.
    let sender = unsafe { Box::from_raw(context as *mut async_channel::Sender<Result<Reply>>) };
    let reply = if json.is_null() { Err(anyhow::anyhow!("StoreKit returned no response")) }
        else { unsafe { serde_json::from_slice(CStr::from_ptr(json).to_bytes()) }.map_err(Into::into) };
    let _ = sender.try_send(reply);
}

pub(crate) async fn request(action: Action) -> Result<Reply> {
    let action = match action { Action::Load => 0, Action::Purchase(Plan::Annual) => 1, Action::Purchase(Plan::Monthly) => 2,
        Action::Restore => 3, Action::Status => 4 };
    let (sender, receiver) = async_channel::bounded(1);
    let context = Box::into_raw(Box::new(sender)) as usize;
    unsafe { quiet_cloud_subscription(action, context, complete) };
    receiver.recv().await.context("The StoreKit request was interrupted")?
}
