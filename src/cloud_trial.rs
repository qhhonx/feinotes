// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
use anyhow::{Result, ensure};
use serde::Deserialize;
use std::ffi::{CStr, CString, c_char};

unsafe extern "C" {
    fn quiet_cloud_trial(start: bool, scope: *const c_char) -> *mut c_char;
    fn quiet_cloud_restore_free(reply: *mut c_char);
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Trial {
    started_at: u64,
    pub expires_at: u64,
}

impl Trial {
    pub fn active_at(self, millis: u64) -> bool { millis < self.expires_at }
}

#[derive(Deserialize)]
struct Reply { trial: Option<Trial>, error: Option<String> }

#[cfg(feature = "devtools")]
pub(crate) fn test_channel() -> Result<Option<String>> {
    if crate::cloud_sync::test_root()?.is_some() {
        let id = std::env::var("QUIET_SYNC_QA_CHANNEL")?;
        uuid::Uuid::parse_str(&id)?;
        Ok(Some(id))
    } else { Ok(None) }
}

pub(crate) fn request(start: bool) -> Result<Option<Trial>> {
    if testflight() { return Ok(None); }
    #[cfg(feature = "devtools")]
    let scope = test_channel()?.map(CString::new).transpose()?;
    #[cfg(not(feature = "devtools"))]
    let scope: Option<CString> = None;
    let reply = unsafe { quiet_cloud_trial(start, scope.as_ref().map_or(std::ptr::null(), |scope| scope.as_ptr())) };
    ensure!(!reply.is_null(), "iCloud returned no trial status");
    let decoded = unsafe { serde_json::from_slice::<Reply>(CStr::from_ptr(reply).to_bytes()) };
    unsafe { quiet_cloud_restore_free(reply) };
    let reply = decoded?;
    if let Some(error) = reply.error { anyhow::bail!("{error}"); }
    if let Some(trial) = reply.trial {
        ensure!(trial.expires_at.checked_sub(trial.started_at) == Some(7 * 24 * 60 * 60 * 1_000), "Invalid iCloud trial period");
    }
    Ok(reply.trial)
}

pub(crate) fn access_required() -> &'static str {
    use crate::settings::tr;
    if testflight() {
        tr("无法验证 TestFlight 安装。请联网后从 TestFlight 打开飞记。", "Couldn’t verify the TestFlight installation. Connect to the internet and open Fei Notes from TestFlight.")
    } else if crate::cloud_subscription::available() {
        tr("请开始试用或订阅以使用 iCloud 同步和协作。", "Start a trial or subscribe to use iCloud sync and collaboration.")
    } else { tr("请在 iCloud 同步设置中查看试用状态。", "Check your trial in iCloud Sync settings.") }
}

pub(crate) fn testflight() -> bool { env!("FEI_TESTFLIGHT_BETA") == "1" }

pub(crate) fn available() -> bool {
    crate::cloud_subscription::available() || env!("FEI_CLOUD_NAMESPACE").starts_with("Production:")
}

pub(crate) fn now_ms() -> u64 {
    #[cfg(feature = "devtools")]
    if test_channel().is_ok_and(|channel| channel.is_some())
        && let Ok(value) = std::env::var("QUIET_SYNC_QA_TRIAL_TIME_MS") && let Ok(millis) = value.parse() { return millis; }
    crate::time::now_ms()
}

#[cfg(feature = "devtools")]
pub(crate) fn qa(start: bool) -> Result<()> {
    use anyhow::Context;
    ensure!(crate::cloud_sync::test_root()?.is_some(), "Use an isolated iCloud trial test");
    if let Some(trial) = request(start)? {
        let now = std::env::var("QUIET_SYNC_QA_TRIAL_TIME_MS").ok().map(|value| value.parse::<u64>())
            .transpose().context("Invalid test time")?.unwrap_or_else(crate::time::now_ms);
        println!("TRIAL_STATUS started={} expires={} active={}", trial.started_at, trial.expires_at, trial.active_at(now));
    } else { println!("TRIAL_AVAILABLE"); }
    Ok(())
}
