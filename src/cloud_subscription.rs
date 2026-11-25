// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
use anyhow::{Context, Result};
use gpui::{App, Global};
use serde::Deserialize;
use std::ffi::{CStr, c_char};
use std::sync::OnceLock;

unsafe extern "C" {
    fn quiet_cloud_subscription_available() -> bool;
    fn quiet_cloud_subscription_sheet_available() -> bool;
    #[cfg(target_os = "ios")]
    fn quiet_cloud_show_subscription_sheet(chinese: bool, dark: bool, red: f32, green: f32, blue: f32);
    fn quiet_cloud_watch_subscription(on_change: extern "C" fn());
    fn quiet_cloud_subscription(action: i32, context: usize, complete: extern "C" fn(usize, *const c_char));
    #[cfg(target_os = "macos")]
    fn quiet_cloud_subscription_from_view(action: i32, context: usize, view: usize, complete: extern "C" fn(usize, *const c_char));
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
pub(crate) struct Reply {
    pub active: Option<ActiveSubscription>,
    #[cfg(target_os = "macos")]
    offers: Option<Vec<panel::Offer>>,
    #[cfg(target_os = "macos")]
    next: Option<Plan>,
    #[cfg(target_os = "macos")]
    outcome: Option<Outcome>,
    #[cfg(target_os = "macos")]
    error_kind: Option<ErrorKind>,
    pub error: Option<String>,
}

pub(crate) fn available() -> bool { unsafe { quiet_cloud_subscription_available() } }
pub(crate) fn sheet_available() -> bool { unsafe { quiet_cloud_subscription_sheet_available() } }

#[cfg(target_os = "ios")]
pub(crate) fn show_sheet() {
    let chinese = crate::settings::lang() == crate::settings::Lang::Zh;
    let accent = crate::ipad::mobile_sharing_accent().to_rgb();
    unsafe { quiet_cloud_show_subscription_sheet(chinese, crate::theme::dark(), accent.r, accent.g, accent.b) };
}

#[cfg(target_os = "macos")]
pub(crate) mod panel;

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Outcome { Loaded, Purchased, Scheduled, Pending, Cancelled, Restored }

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ErrorKind { Unavailable, MissingWindow, Unverified, Busy }

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
    // Start the shared StoreKit observer on the platform's main thread.
    unsafe { quiet_cloud_watch_subscription(purchase_changed) };
    cx.spawn(async move |cx| {
        while receiver.recv().await.is_ok() {
            if crate::gpui_compat::async_update(cx, |cx| {
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

pub(crate) async fn status() -> Result<Reply> {
    let (context, receiver) = response_channel();
    unsafe { quiet_cloud_subscription(4, context, complete) };
    receive(receiver).await
}

#[cfg(target_os = "macos")]
fn request_from_window(action: i32, window: &gpui::Window) -> async_channel::Receiver<Result<Reply>> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let view = match HasWindowHandle::window_handle(window).ok().map(|handle| handle.as_raw()) {
        Some(RawWindowHandle::AppKit(handle)) => handle.ns_view.as_ptr() as usize,
        _ => 0,
    };
    let (context, receiver) = response_channel();
    // Swift resolves the native handle on the UI thread before returning.
    unsafe { quiet_cloud_subscription_from_view(action, context, view, complete) };
    receiver
}

fn response_channel() -> (usize, async_channel::Receiver<Result<Reply>>) {
    let (sender, receiver) = async_channel::bounded(1);
    (Box::into_raw(Box::new(sender)) as usize, receiver)
}

async fn receive(receiver: async_channel::Receiver<Result<Reply>>) -> Result<Reply> {
    receiver.recv().await.context("The StoreKit request was interrupted")?
}
