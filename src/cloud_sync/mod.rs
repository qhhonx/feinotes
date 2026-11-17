// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
pub(crate) mod activity;
mod assets;
pub(crate) mod backup;
mod catalogue;
mod organizer;
mod content;
mod history;
mod journal;
mod retention;
mod paragraphs;
pub(crate) mod presence;
pub(crate) mod shared;
mod table;
pub(crate) mod library;
#[cfg(feature = "devtools")]
pub mod qa;

use anyhow::{Result, ensure};
use base64::{Engine, prelude::BASE64_STANDARD};
use gpui::{BorrowAppContext, App, Global};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, ffi::{CStr, CString, c_char}, path::{Path, PathBuf}, time::Duration};
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

pub(crate) fn issues(root: &Path, cx: &App) -> Result<Vec<SyncIssue>> {
    session(root, cx).map_or_else(|| Ok(vec![]), |sync| sync.journal.issues())
}

unsafe extern "C" {
    fn quiet_cloud_exchange(json: *const c_char) -> *mut c_char;
    fn quiet_cloud_restore_free(reply: *mut c_char);
    fn quiet_cloud_preview(path: *const c_char) -> bool;
    fn quiet_cloud_thumbnail(path: *const c_char, destination: *const c_char) -> bool;
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
struct Exchange { #[serde(skip_serializing_if = "Vec::is_empty")] downloads: Vec<String>, #[serde(skip_serializing_if = "Option::is_none")] maintenance: Option<retention::Maintenance>, account: String, channel: Option<String>, token: Option<Bytes>, edits: Vec<Edit>, legacy: bool,
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
struct Changes { #[serde(default)] retention: Option<retention::Receipt>, uploaded: Vec<String>, edits: Vec<Edit>, token: Option<Bytes>, more: bool, legacy: bool,
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
    ensure!(!page.upload_quota_exceeded || !request.edits.is_empty() || request.maintenance.is_some(), "CloudKit reported upload quota for a download-only request");
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

#[derive(Default)]
struct Sync(HashMap<PathBuf, Session>);
impl Global for Sync {}

struct Session {
    journal: Journal,
    store: SharedStore,
    generation: uuid::Uuid,
    sender: async_channel::Sender<()>,
    retry_attachments: bool,
}

fn session<'a>(root: &Path, cx: &'a App) -> Option<&'a Session> { cx.try_global::<Sync>()?.0.get(root) }
fn journal<'a>(root: &Path, cx: &'a App) -> &'a Journal { &session(root, cx).expect("open sync session").journal }
fn journal_mut<'a>(root: &Path, cx: &'a mut App) -> &'a mut Journal { &mut cx.global_mut::<Sync>().0.get_mut(root).expect("open sync session").journal }
fn generation_matches(open: uuid::Uuid, callback: uuid::Uuid) -> bool { open == callback }
fn current(root: &Path, generation: uuid::Uuid, cx: &App) -> bool { session(root, cx).is_some_and(|s| generation_matches(s.generation, generation)) }

#[derive(Default)]
pub(crate) struct SyncStatuses(HashMap<PathBuf, SyncStatus>);
impl Global for SyncStatuses {}
pub(crate) fn status<'a>(root: &Path, cx: &'a App) -> Option<&'a SyncStatus> { cx.try_global::<SyncStatuses>()?.0.get(root) }
pub(crate) fn scope<'a>(root: &Path, cx: &'a App) -> Option<&'a shared::Scope> { session(root, cx)?.journal.room() }

fn release(root: &Path, cx: &mut App) {
    cx.global_mut::<Sync>().0.remove(root);
    cx.update_global::<SyncStatuses, _>(|statuses, _| { statuses.0.remove(root); });
    presence::remove(root, cx);
    if let Some(watch) = ACCOUNT_WATCH.get() { watch.lock().unwrap().remove(root); }
}

pub(crate) fn shared_access_ended(root: &Path, note: &str, cx: &mut App) -> Result<()> {
    if let Some(session) = cx.global_mut::<Sync>().0.get_mut(root) {
        session.journal.receive(Changes { retention: None, uploaded: vec![], edits: vec![], token: None, more: false, legacy: false,
            room_access: Some(shared::Access::Revoked), upload_quota_exceeded: false,
            #[cfg(feature = "devtools")] cursor_replayed: false })?;
    }
    cx.update_global::<activity::ActivityView, _>(|view, _| view.0.retain(|row| row.room.scope.note != note));
    app::cloud_notes_changed(root, &[], false, cx);
    Ok(())
}

pub(crate) fn release_unused(cx: &mut App) {
    let roots = cx.try_global::<Sync>().into_iter().flat_map(|sync| &sync.0)
        .filter(|(_, session)| session.store.is_shared() && session.store.only_session_reference())
        .map(|(root, _)| root.clone()).collect::<Vec<_>>();
    for root in roots { release(&root, cx); }
}

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


static ACCOUNT_WATCH: std::sync::OnceLock<std::sync::Mutex<HashMap<PathBuf, (PathBuf, async_channel::Sender<()>, async_channel::Sender<()>)>>> = std::sync::OnceLock::new();

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

extern "C" fn sync_resumed(kind: i32) {
    if let Some(watch) = ACCOUNT_WATCH.get() {
        for (marker, sender, account) in watch.lock().unwrap().values() {
            if kind == 3 { protect_offline_library(marker); let _ = account.try_send(()); }
            let _ = sender.try_send(());
        }
    }
    #[cfg(feature = "devtools")]
    if test_root().is_ok_and(|root| root.is_some()) {
        crate::logging::write(match kind {
            1 => "CloudKit sync wake signal", 2 => "CloudKit sync network restored signal", _ => "CloudKit account change signal",
        });
    }
}

#[cfg(feature = "devtools")]
pub(crate) fn simulate_resume(kind: &str) -> Result<()> {
    ensure!(test_root()?.is_some() && ACCOUNT_WATCH.get().is_some(), "Use an isolated iCloud test library");
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

pub(crate) fn retry(root: &Path, cx: &mut App) {
    if !cx.has_global::<Sync>() { return; }
    let Some(session) = cx.global_mut::<Sync>().0.get_mut(root) else { return };
    session.retry_attachments = true;
    let _ = session.sender.try_send(());
    update_status(root, cx, SyncPhase::Connecting);
}

pub(crate) fn access_changed(_cx: &App) { sync_resumed(0); }

fn update_status(root: &Path, cx: &mut App, phase: SyncPhase) { update_status_after_exchange(root, cx, phase, false); }

fn update_status_after_exchange(root: &Path, cx: &mut App, phase: SyncPhase, authorized: bool) {
    let ((uploads, downloads, held, attachments), phase) = match journal(root, cx).counts() {
        Ok(counts) => (counts, phase),
        Err(_) => ((0, 0, 0, 0), SyncPhase::Failed),
    };
    let before = status(root, cx).expect("session status");
    let access_paused = !authorized && (phase == SyncPhase::SubscriptionRequired || before.access_paused);
    let changed = before.access_paused != access_paused;
    let status = SyncStatus { phase, uploads, downloads, held, attachments, access_paused };
    if before != &status { cx.update_global::<SyncStatuses, _>(|statuses, _| { statuses.0.insert(root.to_path_buf(), status); }); }
    if scope(root, cx).is_some() && (changed || matches!(phase, SyncPhase::AccountChecking | SyncPhase::AccountChanged | SyncPhase::SignedOut)) { app::cloud_notes_changed(root, &[], false, cx); }
}

#[cfg(target_os = "macos")]
pub fn start(cx: &mut App) -> Result<()> {
    let store = cx.global::<SharedStore>().clone();
    start_library(store, cx)
}

#[cfg(target_os = "ios")]
pub(crate) fn start_personal_session(cx: &mut App, channel: Option<String>, parent: PathBuf) -> Result<()> {
    let store = cx.global::<SharedStore>().clone();
    let opened = Journal::open(store.root(), channel)?;
    install_session_in(store, opened, Some(parent), cx)
}

#[cfg(target_os = "ios")]
pub(crate) fn start_test_channel(cx: &mut App, channel: uuid::Uuid, parent: PathBuf) -> Result<()> {
    start_personal_session(cx, Some(channel.to_string()), parent)
}

#[cfg(target_os = "ios")]
pub(crate) fn start_shared_session(cx: &mut App, parent: PathBuf) -> Result<()> {
    start_personal_session(cx, None, parent)
}

#[cfg(target_os = "ios")]
pub(crate) fn stop(cx: &mut App) {
    let roots = cx.try_global::<Sync>().map(|sync| sync.0.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
    for root in roots { release(&root, cx); }
    if cx.has_global::<Sync>() { drop(cx.remove_global::<Sync>()); }
    if cx.has_global::<activity::ActivityView>() { drop(cx.remove_global::<activity::ActivityView>()); }
}

#[cfg(target_os = "ios")]
pub(crate) fn history_available(root: &Path, cx: &App) -> bool { session(root, cx).is_some() && scope(root, cx).is_none() }

fn start_library(store: SharedStore, cx: &mut App) -> Result<()> {
    let root = store.root().to_path_buf();
    if session(&root, cx).is_some() { return Ok(()); }
    let channel = if cfg!(feature = "devtools") && !store.is_shared() { std::env::var("QUIET_SYNC_QA_CHANNEL").ok() } else { None };
    if let Some(channel) = &channel {
        uuid::Uuid::parse_str(channel)?;
        ensure!(test_root()?.is_some(), "A QA CloudKit channel requires a separate local QA root");
    }
    let opened = Journal::open(&root, channel)?;
    install_session(store, opened, cx)
}

fn install_session(store: SharedStore, opened: Journal, cx: &mut App) -> Result<()> {
    install_session_in(store, opened, test_root()?, cx)
}

fn install_session_in(store: SharedStore, opened: Journal, parent: Option<PathBuf>, cx: &mut App) -> Result<()> {
    let root = store.root().to_path_buf();
    let pause = parent.as_ref().map(|root| root.join("pause-cloud"));
    #[cfg(feature = "devtools")]
    let upload_fault_fixture = test_root()?.map(|root| root.join("qa-upload-fault"));
    let room = opened.room().is_some();
    let account = opened.identity().0.to_owned();
    let marker = opened.account_verification_marker()?;
    let generation = uuid::Uuid::new_v4();
    let (retry_tx, retry_rx) = async_channel::bounded(1);
    let (account_tx, account_rx) = async_channel::bounded(1);
    if !cx.has_global::<Sync>() { cx.set_global(Sync::default()); }
    if !cx.has_global::<SyncStatuses>() { cx.set_global(SyncStatuses::default()); }
    cx.global_mut::<Sync>().0.insert(root.clone(), Session { journal: opened, store, generation, sender: retry_tx.clone(), retry_attachments: false });
    cx.update_global::<SyncStatuses, _>(|statuses, _| { statuses.0.insert(root.clone(), SyncStatus::default()); });
    let watch = ACCOUNT_WATCH.get_or_init(Default::default);
    let first = { let mut watch = watch.lock().unwrap(); let first = watch.is_empty(); watch.insert(root.clone(), (marker, retry_tx, account_tx)); first };
    let local_session_qa = cfg!(feature = "devtools") && crate::devtools::scripted()
        && std::env::var_os("QUIET_SYNC_QA_SHARED_SESSIONS").is_some();
    if first && !local_session_qa { unsafe { quiet_cloud_watch_sync_resume(sync_resumed) }; }
    update_status(&root, cx, SyncPhase::Connecting);
    if let Err(error) = presence::start(cx, root.clone(), generation, pause.clone()) { release(&root, cx); return Err(error); }
    if !room { activity::start(cx, root.clone(), account, pause.clone(), generation); }
    let account_root = root.clone();
    cx.spawn(async move |cx| {
        while account_rx.recv().await.is_ok() {
            if !crate::gpui_compat::async_update(cx, |cx| {
                if !current(&account_root, generation, cx) { return false; }
                update_status(&account_root, cx, SyncPhase::AccountChecking);
                true
            }).unwrap_or(false) { break; }
        }
    }).detach();
    cx.spawn(async move |cx| {
        let mut observed_revision = account_revision();
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
            if !crate::gpui_compat::async_update(cx, |cx| {
                if !current(&root, generation, cx) { return false; }
                if room && session(&root, cx).unwrap().store.only_session_reference() { release(&root, cx); return false; }
                true
            }).unwrap_or(false) { break; }
            let result = crate::gpui_compat::async_update(cx, |cx| {
                if observed_revision != account_revision() {
                    observed_revision = account_revision();
                    update_status(&root, cx, SyncPhase::AccountChecking);
                }
                if std::mem::take(&mut cx.global_mut::<Sync>().0.get_mut(&root).unwrap().retry_attachments) {
                    upload_retry_at = None;
                    if journal_mut(&root, cx).retry_attachments()? { legacy_checked = None; }
                }
                let legacy = !room && legacy_checked.is_none_or(|time| time.elapsed() >= Duration::from_secs(60));
                merge(&root, cx)?;
                update_status(&root, cx, status(&root, cx).unwrap().phase);
                let allow_upload = !legacy && upload_retry_at.is_none_or(|time| std::time::Instant::now() >= time);
                let journal = &journal(&root, cx);
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
            if !crate::gpui_compat::async_update(cx, |cx| current(&root, generation, cx)).unwrap_or(false) { break; }
            let Ok(result) = crate::gpui_compat::async_update(cx, |cx| -> Result<_> {
                check_account_revision(revision)?;
                let page = result?;
                let (more, quota_exceeded) = (page.more, page.upload_quota_exceeded);
                if !page.uploaded.is_empty() || !page.edits.is_empty() {
                    crate::logging::write(&format!("CloudKit sync uploaded={} downloaded={}", page.uploaded.len(), page.edits.len()));
                }
                if journal_mut(&root, cx).receive(page)? { app::cloud_notes_changed(&root, &[], true, cx); }
                merge(&root, cx)?;
                Ok((more, quota_exceeded))
            }) else { break };
            match result {
                Ok((more, quota_exceeded)) => {
                    account_notice = None;
                    if legacy && !more { legacy_checked = Some(std::time::Instant::now()); }
                    if quota_exceeded { upload_retry_at = Some(std::time::Instant::now() + Duration::from_secs(30)); }
                    else if allow_upload { upload_retry_at = None; }
                    let (pending, collaborating) = crate::gpui_compat::async_update(cx, |cx| {
                        update_status_after_exchange(&root, cx, if upload_retry_at.is_some() { SyncPhase::Quota }
                            else if more { SyncPhase::Connecting } else { SyncPhase::Ready }, true);
                        let collaborating = match app::cloud_active_selection(&root, cx) {
                            presence::SelectionSource::Editing(id, _) => presence::view(&root, cx).expect("presence view").notes.contains_key(&id),
                            _ => false,
                        };
                        let writable = journal(&root, cx).access().is_none_or(shared::Access::can_write);
                        (status(&root, cx).unwrap().uploads > 0 && writable, collaborating)
                    }).unwrap_or_default();
                    last_error = None;
                    delay = if legacy || more || (pending && upload_retry_at.is_none()) { Duration::ZERO } else if collaborating { Duration::from_secs(1) }
                        else { Duration::from_secs(3) };
                }
                Err(error) => {
                    let phase = error.downcast_ref::<CloudFailure>().map_or(SyncPhase::Failed, |error| error.phase);
                    let _ = crate::gpui_compat::async_update(cx, |cx| {
                        if matches!(phase, SyncPhase::AccountChanged | SyncPhase::SignedOut) {
                            if let Ok(marker) = journal(&root, cx).account_verification_marker() { protect_offline_library(&marker); }
                            if account_notice != Some(phase) {
                                app::cloud_account_notice(&root, phase, cx);
                                account_notice = Some(phase);
                            }
                        }
                        update_status(&root, cx, phase);
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

pub(crate) fn displayed_shared_updates(root: &Path, rendered: &Document, cx: &App) -> Result<Vec<String>> {
    session(root, cx).map_or_else(|| Ok(vec![]), |sync| sync.journal.displayable_updates(rendered))
}

pub(crate) fn mark_shared_updates_read(root: &Path, updates: &[String], cx: &mut App) -> Result<()> {
    if session(root, cx).is_some() { journal_mut(root, cx).mark_updates_read(updates)?; }
    Ok(())
}

fn merge(root: &Path, cx: &mut App) -> Result<()> {
    let Some(positions) = app::cloud_open_positions(root, cx) else { return Ok(()); };
    let (library_changed, changed) = journal_mut(root, cx).merge(&positions)?;
    if library_changed || !changed.is_empty() {
        if let Some(scope) = scope(root, cx) { shared::original::refresh_owner(root, scope, None)?; }
        app::cloud_notes_changed(root, &changed, true, cx);
    }
    Ok(())
}

fn wake_after_save(root: &Path, cx: &mut App) {
    update_status(root, cx, status(root, cx).unwrap().phase);
    let status = status(root, cx).unwrap();
    if status.uploads > 0 && matches!(status.phase, SyncPhase::Ready | SyncPhase::Connecting) {
        let _ = session(root, cx).unwrap().sender.try_send(());
    }
}

pub fn save(store: &mut SharedStore, id: &str, core: &Core, cx: &mut App) -> Result<(Meta, bool)> {
    let root = store.root().to_path_buf();
    if session(&root, cx).is_none() {
        ensure!(!store.is_shared() && crate::cloud_restore::local_only(), "This library has no active sync session");
        return store.save(id, &core.doc).map(|meta| (meta, false));
    }
    let result = journal_mut(&root, cx).save(store, id, core)?;
    if let Some(scope) = scope(&root, cx)
        && let Err(error) = shared::original::refresh_owner(&root, scope, Some(id)) { crate::logging::write(&format!("Private shared index refresh: {error:#}")); }
    wake_after_save(&root, cx);
    Ok(result)
}

pub fn undo(store: &mut SharedStore, id: &str, core: &Core, peers: &[Pos], redo: bool, cx: &mut App) -> Result<Option<(NoteChange, Option<crate::editor::core::Selection>)>> {
    let root = store.root().to_path_buf();
    let result = journal_mut(&root, cx).undo(store, id, core, peers, redo)?;
    if let Some(scope) = scope(&root, cx)
        && let Err(error) = shared::original::refresh_owner(&root, scope, Some(id)) { crate::logging::write(&format!("Private shared index refresh: {error:#}")); }
    wake_after_save(&root, cx);
    Ok(result)
}

pub fn versions(root: &Path, id: &str, cx: &App) -> Result<Vec<(i64, String, String)>> {
    journal(root, cx).versions(id)
}

pub(crate) fn offline_recoveries(root: &Path, cx: &App) -> Result<Vec<(String, String)>> {
    session(root, cx).map_or_else(|| Ok(vec![]), |sync| sync.journal.offline_recoveries())
}

pub(crate) fn resolve_offline_recovery(root: &Path, id: &str, keep: bool, cx: &mut App) -> Result<Option<String>> {
    let copy = journal_mut(root, cx).resolve_offline_recovery(id, keep)?;
    if copy.is_some() && let Some(session) = session(root, cx) { let _ = session.sender.try_send(()); }
    Ok(copy)
}

pub fn version(root: &Path, id: &str, sequence: i64, cx: &App) -> Result<(Document, String)> {
    journal(root, cx).version(id, sequence)
}

pub fn preview(path: &Path) -> bool {
    CString::new(path.as_os_str().as_encoded_bytes()).is_ok_and(|path| unsafe { quiet_cloud_preview(path.as_ptr()) })
}

#[cfg(target_os = "macos")]
/// Generate an attachment preview with the sandbox-aware native service.
/// This blocks and must only run on a background executor.
pub fn thumbnail(path: &Path, destination: &Path) -> bool {
    let Ok(path) = CString::new(path.as_os_str().as_encoded_bytes()) else { return false };
    let Ok(destination) = CString::new(destination.as_os_str().as_encoded_bytes()) else { return false };
    unsafe { quiet_cloud_thumbnail(path.as_ptr(), destination.as_ptr()) }
}

#[cfg(test)]
mod session_tests {
    use super::*;

    #[test]
    fn library_handoff_preserves_queued_edits_and_isolates_late_callbacks() -> Result<()> {
        const CHILD: &str = "FEI_SESSION_HANDOFF_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe()?)
                .args(["--exact","cloud_sync::session_tests::library_handoff_preserves_queued_edits_and_isolates_late_callbacks","--nocapture"])
                .env(CHILD,"1").output()?;
            ensure!(output.status.success(),"{}\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
            return Ok(());
        }
        struct Temporary(PathBuf);
        impl Drop for Temporary { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
        let temporary = Temporary(std::env::temp_dir().join(format!("fei-session-check-{}",uuid::Uuid::new_v4())));
        std::fs::create_dir(&temporary.0)?;
        let first = temporary.0.join("cloud");
        let second = temporary.0.join("local");
        let staged = first.join(".restore-source");
        let mut legacy = Store::open_at(staged.clone())?;
        let document = crate::docxml::parse("<title>Session source</title>\n<p>Original</p>")?;
        let note = legacy.create_from(&document)?;
        std::fs::write(staged.join(".cloud-restore-ready"),format!("{}\n","a".repeat(64)))?;
        drop(legacy);
        Store::initialize_sqlite(&first)?;
        std::fs::create_dir(&second)?;
        Store::initialize_local_sqlite(&second)?;
        Store::set_sqlite_session_root(first.clone())?;
        let mut editor_store = SharedStore::open()?;
        let mut journal = Journal::open(&first,Some("61d4fa9f-8938-468b-a920-fdb41ba3afe0".into()))?;
        let mut core = Core::new(editor_store.load(&note)?);
        let block = &core.doc.blocks[0];
        core.sel = crate::editor::core::Selection::caret(Pos::new(block.id,block.text.len()));
        core.insert_text(" · Pending offline edit");
        journal.save(&mut editor_store,&note,&core)?;
        ensure!(journal.counts()?.0 > 0,"The source edit was not queued");
        let queued = journal.counts()?.0;
        let peer = journal.identity().2;
        let delayed = uuid::Uuid::new_v4();
        assert!(Store::switch_sqlite_session_root(&first,second.clone()).is_err());
        drop(journal);
        assert!(Store::switch_sqlite_session_root(&first,second.clone()).is_err());
        let export = Store::session_lease(&first).unwrap();
        drop(editor_store);
        assert!(Store::switch_sqlite_session_root(&first,second.clone()).is_err());
        drop(export);
        Store::switch_sqlite_session_root(&first,second.clone())?;
        let mut local = SharedStore::open()?;
        assert!(local.list().is_empty());
        assert!(!generation_matches(uuid::Uuid::new_v4(), delayed));
        assert!(local.list().is_empty());
        local.create_from(&crate::docxml::parse("<title>Local session</title>\n<p>Separate</p>")?)?;
        drop(local);
        Store::switch_sqlite_session_root(&second,first.clone())?;
        let mut reopened = SharedStore::open()?;
        let journal = Journal::open(&first,Some(uuid::Uuid::new_v4().to_string()));
        assert!(journal.is_err());
        let db = rusqlite::Connection::open(first.join(crate::store::SQLITE_FILE))?;
        let channel: String = db.query_row("SELECT value FROM state WHERE key='channel'",[],|row| row.get(0))?;
        drop(db);
        let journal = Journal::open(&first,Some(channel))?;
        assert_eq!(journal.counts()?.0,queued);
        assert_eq!(journal.identity().2,peer);
        assert!(reopened.load(&note)?.plain_text().contains("Pending offline edit"));
        assert_eq!(reopened.list().len(),1);
        drop(journal); drop(reopened);
        if let Some(output) = std::env::var_os("FEI_SESSION_FIXTURE_OUTPUT") {
            let output = PathBuf::from(output);
            ensure!(output.is_absolute() && output.starts_with("/private/tmp/"),"Use a private temporary fixture folder");
            std::fs::create_dir(&output)?;
            for name in [crate::store::SQLITE_FILE,".cloud-restore-ready"] { std::fs::copy(first.join(name),output.join(name))?; }
            std::fs::write(output.join("fixture.json"),serde_json::to_vec(&serde_json::json!({
                "channel":"61d4fa9f-8938-468b-a920-fdb41ba3afe0","note":note,"pending":queued,"peer":peer
            }))?)?;
        }
        Ok(())
    }

    #[test]
    fn replaced_session_rejects_the_previous_generation() {
        let old = uuid::Uuid::new_v4();
        let next = uuid::Uuid::new_v4();
        assert!(generation_matches(old, old));
        assert!(!generation_matches(next, old));
        assert!(generation_matches(next, next));
    }
}
