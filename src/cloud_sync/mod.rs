// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
pub(crate) mod activity;
mod assets;
pub(crate) mod backup;
mod catalogue;
mod content;
mod history;
mod journal;
mod paragraphs;
pub(crate) mod presence;
pub(crate) mod shared;
mod table;
#[cfg(feature = "devtools")]
pub mod qa;

use anyhow::{Result, ensure};
use base64::{Engine, prelude::BASE64_STANDARD};
use gpui::{App, Global};
use serde::{Deserialize, Serialize};
use std::{ffi::{CStr, CString, c_char}, path::{Path, PathBuf}, time::Duration};
use crate::{app, editor::core::{Core, Pos}, model::Document, shared_store::SharedStore, store::{Meta, Store}};
use journal::Journal;
pub(crate) use assets::AssetProblem;

pub(crate) struct NoteChange {
    pub id: String,
    pub document: Document,
    pub positions: Vec<(Pos, Pos)>,
}

pub(crate) struct SyncIssue {
    pub id: String,
    pub title: String,
    pub trashed: bool,
    pub held: bool,
    pub attachments: Vec<AttachmentIssue>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct AttachmentIssue {
    pub source: String,
    pub reason: Option<AssetProblem>,
    pub stored_here: bool,
    pub recovering: bool,
}

pub(crate) fn issues(cx: &App) -> Result<Vec<SyncIssue>> {
    cx.try_global::<Sync>().map_or_else(|| Ok(vec![]), |sync| sync.0.issues())
}

unsafe extern "C" {
    fn quiet_cloud_exchange(json: *const c_char) -> *mut c_char;
    fn quiet_cloud_restore_free(reply: *mut c_char);
    fn quiet_cloud_preview(path: *const c_char) -> bool;
    fn quiet_cloud_watch_sync_resume(signal: extern "C" fn(i32));
    fn quiet_cloud_account_revision() -> u64;
    #[cfg(feature = "devtools")]
    fn quiet_cloud_simulate_sync_resume(kind: i32);
}

#[derive(Clone)]
struct Bytes(Vec<u8>);
impl Serialize for Bytes {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&BASE64_STANDARD.encode(&self.0))
    }
}
impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        BASE64_STANDARD.decode(String::deserialize(deserializer)?).map(Self).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Edit { id: String, note: String, data: Bytes,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    collaborator: bool }

#[derive(Serialize)]
struct Exchange { account: String, channel: Option<String>, token: Option<Bytes>, edits: Vec<Edit>, legacy: bool,
    room: Option<shared::Scope>,
    #[serde(rename = "activityOnly", skip_serializing_if = "std::ops::Not::not")]
    activity_only: bool,
    #[cfg(feature = "devtools")]
    #[serde(rename = "expireCursorForQA", skip_serializing_if = "std::ops::Not::not")]
    expire_cursor_for_qa: bool,
    #[cfg(feature = "devtools")]
    #[serde(rename = "uploadFaultForQA", skip_serializing_if = "Option::is_none")]
    upload_fault_for_qa: Option<String> }

#[derive(Deserialize)]
struct Changes { uploaded: Vec<String>, edits: Vec<Edit>, token: Option<Bytes>, more: bool, legacy: bool,
    #[serde(rename = "roomAccess")]
    room_access: Option<shared::Access>,
    #[serde(rename = "uploadQuotaExceeded", default)]
    upload_quota_exceeded: bool,
    #[cfg(feature = "devtools")]
    #[serde(rename = "cursorReplayed", default)]
    cursor_replayed: bool }

fn exchange(request: Exchange) -> Result<Changes> {
    let revision = account_revision();
    #[cfg(feature = "devtools")]
    let request = {
        let mut request = request;
        if request.room.is_some() { request.channel = crate::cloud_trial::test_channel()?; }
        request
    };
    let json = CString::new(serde_json::to_vec(&request)?)?;
    let ptr = unsafe { quiet_cloud_exchange(json.as_ptr()) };
    ensure!(!ptr.is_null(), "CloudKit exchange returned no response");
    let result = unsafe { serde_json::from_slice::<serde_json::Value>(CStr::from_ptr(ptr).to_bytes()) };
    unsafe { quiet_cloud_restore_free(ptr) };
    let value = result?;
    check_account_revision(revision)?;
    if let Some(error) = value.get("error").and_then(serde_json::Value::as_str) {
        let phase = match value.get("errorKind").and_then(serde_json::Value::as_str) {
            Some("offline") => SyncPhase::Offline,
            Some("account") => SyncPhase::Account,
            Some("accountChecking") => SyncPhase::AccountChecking,
            Some("accountChanged") => SyncPhase::AccountChanged,
            Some("signedOut") => SyncPhase::SignedOut,
            Some("quota") => SyncPhase::Quota,
            Some("subscriptionRequired") => SyncPhase::SubscriptionRequired,
            _ => SyncPhase::Failed,
        };
        return Err(CloudFailure { phase, message: error.into() }.into());
    }
    let page: Changes = serde_json::from_value(value)?;
    ensure!(page.legacy == request.legacy, "CloudKit returned a page from the wrong sync zone");
    ensure!(page.room_access.is_some() == request.room.is_some(), "CloudKit returned the wrong sharing scope");
    ensure!(!page.upload_quota_exceeded || !request.edits.is_empty(), "CloudKit reported upload quota for a download-only request");
    ensure!(page.uploaded.iter().all(|id| request.edits.iter().any(|edit| &edit.id == id)), "CloudKit acknowledged an edit that was not sent");
    Ok(page)
}

pub fn test_root() -> Result<Option<PathBuf>> {
    if !cfg!(feature = "devtools") { return Ok(None); }
    let root = std::env::var_os("QUIET_SYNC_QA_ROOT").map(PathBuf::from);
    if let Some(root) = &root {
        let app_temp = Path::new("Library/Containers").join(crate::APP_ID).join("Data/tmp");
        let temporary = root.parent().is_some_and(|parent| parent == Path::new("/private/tmp")
            || parent.ends_with(&app_temp));
        ensure!(root.is_absolute() && temporary
            && root.file_name().is_some_and(|name| name.to_string_lossy().starts_with("quiet-sync-qa-"))
            && !root.components().any(|part| part == std::path::Component::ParentDir),
            "Use a fresh quiet-sync-qa directory inside the app's temporary folder");
    }
    Ok(root)
}

struct Sync(Journal);
impl Global for Sync {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SyncPhase {
    #[default]
    Connecting,
    Ready,
    Offline,
    Account,
    AccountChecking,
    AccountChanged,
    SignedOut,
    Quota,
    SubscriptionRequired,
    Failed,
}

#[derive(Default, PartialEq, Eq)]
pub(crate) struct SyncStatus {
    pub phase: SyncPhase,
    pub uploads: u32,
    pub downloads: u32,
    pub held: u32,
    pub attachments: u32,
    pub access_paused: bool,
}
impl Global for SyncStatus {}

struct Wake {
    sender: async_channel::Sender<()>,
    retry_attachments: bool,
}
impl Global for Wake {}

static SYNC_RESUME: std::sync::OnceLock<async_channel::Sender<()>> = std::sync::OnceLock::new();
static ACCOUNT_WATCH: std::sync::OnceLock<(PathBuf, async_channel::Sender<()>)> = std::sync::OnceLock::new();

fn protect_offline_library(marker: &Path) {
    if let Err(error) = std::fs::OpenOptions::new().write(true).create_new(true).open(marker).and_then(|file| file.sync_all())
        && error.kind() != std::io::ErrorKind::AlreadyExists {
        crate::logging::write(&format!("Could not protect the previous iCloud library on restart: {error:#}"));
    }
}

fn account_revision() -> u64 { unsafe { quiet_cloud_account_revision() } }

fn check_account_revision(revision: u64) -> Result<()> {
    if account_revision() == revision { return Ok(()); }
    Err(CloudFailure { phase: SyncPhase::AccountChecking, message: "The iCloud account status changed during this request.".into() }.into())
}

extern "C" fn sync_resumed(_kind: i32) {
    if _kind == 3 && let Some((marker, sender)) = ACCOUNT_WATCH.get() {
        protect_offline_library(marker);
        let _ = sender.try_send(());
    }
    if let Some(sender) = SYNC_RESUME.get() { let _ = sender.try_send(()); }
    #[cfg(feature = "devtools")]
    if test_root().is_ok_and(|root| root.is_some()) {
        crate::logging::write(match _kind {
            1 => "CloudKit sync wake signal", 2 => "CloudKit sync network restored signal", _ => "CloudKit account change signal",
        });
    }
}

#[cfg(feature = "devtools")]
pub(crate) fn simulate_resume(kind: &str) -> Result<()> {
    ensure!(test_root()?.is_some() && SYNC_RESUME.get().is_some(), "Use an isolated iCloud test library");
    let kind = match kind { "wake" => 1, "network" => 2, "account" => 3, _ => anyhow::bail!("Unknown sync resume event") };
    unsafe { quiet_cloud_simulate_sync_resume(kind) };
    Ok(())
}

#[derive(Debug)]
struct CloudFailure { phase: SyncPhase, message: String }

impl std::fmt::Display for CloudFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.message) }
}
impl std::error::Error for CloudFailure {}

pub(crate) fn retry(cx: &mut App) {
    if cx.try_global::<Wake>().is_none() { return; }
    let wake = cx.global_mut::<Wake>();
    wake.retry_attachments = true;
    let _ = wake.sender.try_send(());
    update_status(cx, SyncPhase::Connecting);
}

pub(crate) fn access_changed(cx: &App) {
    if let Some(wake) = cx.try_global::<Wake>() { let _ = wake.sender.try_send(()); }
}

fn update_status(cx: &mut App, phase: SyncPhase) { update_status_after_exchange(cx, phase, false); }

fn update_status_after_exchange(cx: &mut App, phase: SyncPhase, authorized: bool) {
    let ((uploads, downloads, held, attachments), phase) = match cx.global::<Sync>().0.counts() {
        Ok(counts) => (counts, phase),
        Err(_) => ((0, 0, 0, 0), SyncPhase::Failed),
    };
    let before = cx.global::<SyncStatus>();
    let access_paused = !authorized && (phase == SyncPhase::SubscriptionRequired || before.access_paused);
    let changed = before.access_paused != access_paused;
    let status = SyncStatus { phase, uploads, downloads, held, attachments, access_paused };
    if before != &status { cx.set_global(status); }
    if changed && shared::active().is_some() { app::cloud_notes_changed(&[], false, cx); }
}

pub fn start(cx: &mut App) -> Result<()> {
    let channel = if cfg!(feature = "devtools") && shared::active().is_none() { std::env::var("QUIET_SYNC_QA_CHANNEL").ok() } else { None };
    if let Some(channel) = &channel {
        uuid::Uuid::parse_str(channel)?;
        ensure!(test_root()?.is_some(), "A QA CloudKit channel requires a separate local QA root");
    }
    let journal = Journal::open(&Store::root_dir(), channel)?;
    let room = journal.room().is_some();
    let account = journal.identity().0.to_owned();
    let marker = journal.account_verification_marker()?;
    cx.set_global(Sync(journal));
    cx.set_global(SyncStatus::default());
    let (retry_tx, retry_rx) = async_channel::bounded(1);
    let (account_tx, account_rx) = async_channel::bounded(1);
    ensure!(SYNC_RESUME.set(retry_tx.clone()).is_ok(), "The iCloud sync worker is already running");
    cx.set_global(Wake { sender: retry_tx, retry_attachments: false });
    ensure!(ACCOUNT_WATCH.set((marker, account_tx)).is_ok(), "The iCloud account watcher is already running");
    // GPUI starts the workspace on the main thread, as required by NSWorkspace.
    unsafe { quiet_cloud_watch_sync_resume(sync_resumed) };
    update_status(cx, SyncPhase::Connecting);
    let pause = test_root()?.map(|root| root.join("pause-cloud"));
    #[cfg(feature = "devtools")]
    let upload_fault_fixture = test_root()?.map(|root| root.join("qa-upload-fault"));
    presence::start(cx, pause.clone())?;
    if !room { activity::start(cx, Store::root_dir(), account, pause.clone()); }
    cx.spawn(async move |cx| {
        while account_rx.recv().await.is_ok() {
            if cx.update(|cx| update_status(cx, SyncPhase::AccountChecking)).is_err() { break; }
        }
    }).detach();
    cx.spawn(async move |cx| {
        let mut last_error = None;
        let mut account_notice = None;
        let mut legacy_checked: Option<std::time::Instant> = None;
        let mut upload_retry_at: Option<std::time::Instant> = None;
        let mut delay = Duration::from_secs(2);
        loop {
            let mut timer = std::pin::pin!(cx.background_executor().timer(delay));
            let mut wake = std::pin::pin!(retry_rx.recv());
            std::future::poll_fn(|cx| {
                use std::task::Poll;
                if wake.as_mut().poll(cx).is_ready() || timer.as_mut().poll(cx).is_ready() { Poll::Ready(()) }
                else { Poll::Pending }
            }).await;
            let result = cx.update(|cx| {
                if std::mem::take(&mut cx.global_mut::<Wake>().retry_attachments) {
                    upload_retry_at = None;
                    if cx.global_mut::<Sync>().0.retry_attachments()? { legacy_checked = None; }
                }
                let legacy = !room && legacy_checked.is_none_or(|time| time.elapsed() >= Duration::from_secs(60));
                merge(cx)?;
                update_status(cx, cx.global::<SyncStatus>().phase);
                let allow_upload = !legacy && upload_retry_at.is_none_or(|time| std::time::Instant::now() >= time);
                let journal = &cx.global::<Sync>().0;
                let request = if legacy { journal.legacy_request()? } else { journal.request_with_uploads(allow_upload)? };
                #[cfg(feature = "devtools")]
                let request = {
                    let mut request = request;
                    if request.channel.is_some() && !request.edits.is_empty() {
                        request.upload_fault_for_qa = upload_fault_fixture.as_ref().filter(|path| path.is_file())
                            .and_then(|path| std::fs::read_to_string(path).ok()).map(|id| id.trim().to_owned());
                    }
                    request
                };
                Ok((legacy, allow_upload, request))
            });
            let Ok(request) = result else { break };
            let revision = account_revision();
            let (legacy, allow_upload) = request.as_ref().map(|(legacy, allow_upload, _)| (*legacy, *allow_upload)).unwrap_or_default();
            let result = match request {
                Ok(_) if pause.as_ref().is_some_and(|path| path.exists()) => {
                    delay = Duration::from_secs(2);
                    continue;
                }
                Ok((_, _, request)) => cx.background_executor().spawn(async move { exchange(request) }).await,
                Err(error) => Err(error),
            };
            let Ok(result) = cx.update(|cx| -> Result<_> {
                check_account_revision(revision)?;
                let page = result?;
                let (more, quota_exceeded) = (page.more, page.upload_quota_exceeded);
                if !page.uploaded.is_empty() || !page.edits.is_empty() {
                    crate::logging::write(&format!("CloudKit sync uploaded={} downloaded={}", page.uploaded.len(), page.edits.len()));
                }
                if cx.global_mut::<Sync>().0.receive(page)? { app::cloud_notes_changed(&[], true, cx); }
                merge(cx)?;
                Ok((more, quota_exceeded))
            }) else { break };
            match result {
                Ok((more, quota_exceeded)) => {
                    account_notice = None;
                    if legacy && !more { legacy_checked = Some(std::time::Instant::now()); }
                    if quota_exceeded { upload_retry_at = Some(std::time::Instant::now() + Duration::from_secs(30)); }
                    else if allow_upload { upload_retry_at = None; }
                    let (pending, collaborating) = cx.update(|cx| {
                        update_status_after_exchange(cx, if upload_retry_at.is_some() { SyncPhase::Quota }
                            else if more { SyncPhase::Connecting } else { SyncPhase::Ready }, true);
                        let collaborating = match app::cloud_active_selection(cx) {
                            presence::SelectionSource::Editing(id, _) => cx.global::<presence::View>().notes.contains_key(&id),
                            _ => false,
                        };
                        let writable = cx.global::<Sync>().0.access().is_none_or(shared::Access::can_write);
                        (cx.global::<SyncStatus>().uploads > 0 && writable, collaborating)
                    }).unwrap_or_default();
                    last_error = None;
                    delay = if legacy || more || (pending && upload_retry_at.is_none()) { Duration::ZERO } else if collaborating { Duration::from_secs(1) }
                        else { Duration::from_secs(3) };
                }
                Err(error) => {
                    let phase = error.downcast_ref::<CloudFailure>().map_or(SyncPhase::Failed, |error| error.phase);
                    let _ = cx.update(|cx| {
                        if matches!(phase, SyncPhase::AccountChanged | SyncPhase::SignedOut) {
                            if let Some((marker, _)) = ACCOUNT_WATCH.get() { protect_offline_library(marker); }
                            if account_notice != Some(phase) {
                                app::cloud_account_notice(phase, cx);
                                account_notice = Some(phase);
                            }
                        }
                        update_status(cx, phase);
                    });
                    let error = format!("{error:#}");
                    if last_error.as_ref() != Some(&error) { crate::logging::write(&format!("CloudKit sync: {error}")); }
                    last_error = Some(error);
                    delay = if phase == SyncPhase::AccountChecking { Duration::ZERO }
                        else { Duration::from_secs(6).max((delay * 2).min(Duration::from_secs(30))) };
                }
            }
        }
    }).detach();
    Ok(())
}

pub(crate) fn copy_local_library(target: &Path, source: &Path, channel: Option<String>) -> Result<usize> {
    // Initialize the cloud catalogue before importing. Otherwise the first
    // journal open would mistake these copies for the server's initial state.
    let mut journal = Journal::open(target, channel)?;
    let count = Store::copy_local_library(target, source)?;
    journal.merge(&Default::default())?;
    Ok(count)
}

pub(crate) fn displayed_shared_updates(rendered: &Document, cx: &App) -> Result<Vec<String>> {
    cx.try_global::<Sync>().map_or_else(|| Ok(vec![]), |sync| sync.0.displayable_updates(rendered))
}

pub(crate) fn mark_shared_updates_read(updates: &[String], cx: &mut App) -> Result<()> {
    if cx.try_global::<Sync>().is_some() { cx.global_mut::<Sync>().0.mark_updates_read(updates)?; }
    Ok(())
}

fn merge(cx: &mut App) -> Result<()> {
    let Some(positions) = app::cloud_open_positions(cx) else { return Ok(()); };
    let (library_changed, changed) = cx.global_mut::<Sync>().0.merge(&positions)?;
    if library_changed || !changed.is_empty() { app::cloud_notes_changed(&changed, true, cx); }
    Ok(())
}

fn wake_after_save(cx: &mut App) {
    update_status(cx, cx.global::<SyncStatus>().phase);
    let status = cx.global::<SyncStatus>();
    if status.uploads > 0 && matches!(status.phase, SyncPhase::Ready | SyncPhase::Connecting) {
        let _ = cx.global::<Wake>().sender.try_send(());
    }
}

pub fn save(store: &mut SharedStore, id: &str, core: &Core, cx: &mut App) -> Result<(Meta, bool)> {
    if crate::cloud_restore::local_only() { return store.save(id, &core.doc).map(|meta| (meta, false)); }
    let result = cx.global_mut::<Sync>().0.save(store, id, core)?;
    wake_after_save(cx);
    Ok(result)
}

pub fn undo(store: &mut SharedStore, id: &str, core: &Core, redo: bool, cx: &mut App) -> Result<Option<NoteChange>> {
    let result = cx.global_mut::<Sync>().0.undo(store, id, core, redo)?;
    wake_after_save(cx);
    Ok(result)
}

pub fn versions(id: &str, cx: &App) -> Result<Vec<(i64, String, String)>> {
    cx.global::<Sync>().0.versions(id)
}

pub(crate) fn offline_recoveries(cx: &App) -> Result<Vec<(String, String)>> {
    cx.try_global::<Sync>().map_or_else(|| Ok(vec![]), |sync| sync.0.offline_recoveries())
}

pub(crate) fn resolve_offline_recovery(id: &str, keep: bool, cx: &mut App) -> Result<Option<String>> {
    let copy = cx.global_mut::<Sync>().0.resolve_offline_recovery(id, keep)?;
    if copy.is_some() && let Some(wake) = cx.try_global::<Wake>() { let _ = wake.sender.try_send(()); }
    Ok(copy)
}

pub fn version(id: &str, sequence: i64, cx: &App) -> Result<(Document, String)> {
    cx.global::<Sync>().0.version(id, sequence)
}

pub fn preview(path: &Path) -> bool {
    CString::new(path.as_os_str().as_encoded_bytes()).is_ok_and(|path| unsafe { quiet_cloud_preview(path.as_ptr()) })
}
