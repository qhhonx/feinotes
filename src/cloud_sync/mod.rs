// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
pub(crate) mod activity;
mod assets;
pub(crate) mod backup;
mod catalogue;
mod organizer;
pub(crate) mod conflict_ui;
pub(crate) use journal::conflict::ConflictVersions;
mod content;
mod history;
mod journal;
mod retention;
mod paragraphs;
pub(crate) mod presence;
mod progress;
pub(crate) mod shared;
mod table;
pub(crate) mod library;
#[cfg(feature = "devtools")]
pub mod qa;

use anyhow::{Result, ensure};
use base64::{Engine, prelude::BASE64_STANDARD};
use gpui::{BorrowAppContext, App, Global};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, ffi::{CStr, CString, c_char}, path::{Path, PathBuf}, time::{Duration, Instant}};
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
    pub title: Option<String>,
    pub trashed: bool,
    pub held: bool,
    pub conflict: bool,
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

pub(crate) fn document_conflict(root: &Path, id: &str, cx: &App) -> bool {
    status(root, cx).is_some_and(|status| status.conflicts > 0)
        && session(root, cx).is_some_and(|session| session.journal.document_conflict(id))
}

pub(crate) fn can_review_conflict(root: &Path, cx: &App) -> bool {
    session(root, cx).is_some_and(|session| session.account_revision == account_revision()
        && session.journal.can_review_conflict())
        && !root.join("require-account-verification").exists()
        && status(root, cx).is_some_and(|s| !matches!(s.phase, SyncPhase::Account | SyncPhase::AccountChecking
            | SyncPhase::AccountChanged | SyncPhase::SignedOut | SyncPhase::Revoked))
}

pub(crate) fn conflict_versions(root: &Path, id: &str, cx: &App) -> Result<ConflictVersions> {
    ensure!(can_review_conflict(root, cx), "This iCloud document is unavailable");
    journal(root, cx).conflict_versions(id)
}

pub(crate) fn copy_conflict_versions(root: &Path, id: &str, selected: &[(bool, [u8; 32])], cx: &mut App) -> Result<Vec<String>> {
    ensure!(can_review_conflict(root, cx), "This iCloud document is unavailable");
    let revision = session(root, cx).unwrap().account_revision;
    let copies = journal_mut(root, cx).copy_conflict_versions(id, selected, || {
        check_account_revision(revision)?;
        ensure!(!root.join("require-account-verification").exists(), "The iCloud account requires verification.");
        Ok(())
    })?;
    let _ = session(root, cx).unwrap().sender.try_send(());
    Ok(copies)
}

pub(crate) fn retry_document(root: &Path, id: &str, cx: &mut App) -> Result<()> {
    ensure!(can_review_conflict(root, cx), "This iCloud document is unavailable");
    journal_mut(root, cx).retry_document(id)?;
    let _ = session(root, cx).unwrap().sender.try_send(());
    Ok(())
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

fn exchange(request: Exchange) -> Result<Changes> { exchange_page(request).map(|(page, _)| page) }

fn exchange_page(request: Exchange) -> Result<(Changes, bool)> {
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
            Some("testflightVerification") => SyncPhase::TestFlightVerification,
            _ => match value.get("errorStage").and_then(serde_json::Value::as_str) {
                Some("uploading") => SyncPhase::Uploading,
                Some("downloading") => SyncPhase::Downloading,
                _ => SyncPhase::Connecting,
            },
        };
        return Err(CloudFailure { phase, message: error.into() }.into());
    }
    let cursor_advanced = value.get("cursorAdvanced").and_then(serde_json::Value::as_bool).unwrap_or(false);
    let page: Changes = serde_json::from_value(value)?;
    ensure!(page.legacy == request.legacy, "CloudKit returned a page from the wrong sync zone");
    ensure!(page.room_access.is_some() == request.room.is_some(), "CloudKit returned the wrong sharing scope");
    ensure!(!page.upload_quota_exceeded || !request.edits.is_empty() || request.maintenance.is_some(), "CloudKit reported upload quota for a download-only request");
    ensure!(page.uploaded.iter().all(|id| request.edits.iter().any(|edit| &edit.id == id)), "CloudKit acknowledged an edit that was not sent");
    Ok((page, cursor_advanced))
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
    presence_wake: async_channel::Sender<()>,
    retry_attachments: bool,
    progress: progress::Monitor,
    account_revision: u64,
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
        update_status(root, cx, SyncPhase::Revoked);
    }
    activity::remove_room(note, cx);
    app::cloud_notes_changed(root, &[], false, cx);
    Ok(())
}

fn idle_shared_session(session: &Session) -> bool {
    session.store.only_session_reference() &&
        !(session.journal.access().is_some_and(shared::Access::can_write) && session.journal.has_queued_catalogue().unwrap_or(true))
}

pub(crate) fn release_unused(cx: &mut App) {
    let roots = cx.try_global::<Sync>().into_iter().flat_map(|sync| &sync.0)
        .filter(|(_, session)| session.store.is_shared() && idle_shared_session(session))
        .map(|(root, _)| root.clone()).collect::<Vec<_>>();
    for root in roots { release(&root, cx); }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SyncPhase {
    #[default]
    Connecting,
    Uploading,
    Downloading,
    Applying,
    ReadOnly,
    Revoked,
    Ready,
    Offline,
    Account,
    AccountChecking,
    AccountChanged,
    SignedOut,
    Quota,
    SubscriptionRequired,
    TestFlightVerification,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub(crate) struct ApplyWait {
    pub catalogue_predecessor: bool,
    pub documents_awaiting_catalogue: u32,
    pub documents_awaiting_predecessor: u32,
    pub documents_awaiting_attachment: u32,
    pub documents_held: u32,
    pub local_edit: bool,
    pub local_error: bool,
}

#[derive(Default, PartialEq, Eq)]
pub(crate) struct SyncStatus {
    pub phase: SyncPhase,
    pub uploads: u32,
    pub downloads: u32,
    pub held: u32,
    pub attachments: u32,
    pub conflicts: u32,
    pub catalogue_paused: bool,
    pub access_paused: bool,
    pub access_verified: bool,
    pub stalled: Option<SyncPhase>,
    pub apply_wait: ApplyWait,
    pub retry_requested: bool,
    last_success: Option<u64>,
}


static ACCOUNT_WATCH: std::sync::OnceLock<std::sync::Mutex<HashMap<PathBuf, (PathBuf, async_channel::Sender<()>, async_channel::Sender<()>)>>> = std::sync::OnceLock::new();

fn protect_offline_library(marker: &Path) {
    if let Err(error) = std::fs::OpenOptions::new().write(true).create_new(true).open(marker).and_then(|file| file.sync_all())
        && error.kind() != std::io::ErrorKind::AlreadyExists {
        crate::logging::write(&format!("Could not protect the previous iCloud library on restart: {error:#}"));
    }
}

pub(crate) fn account_revision() -> u64 { unsafe { quiet_cloud_account_revision() } }

pub(crate) fn check_account_revision(revision: u64) -> Result<()> {
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
}

pub(crate) fn retry_shared(root: &Path,cx: &mut App) {
    if scope(root,cx).is_none() { return; }
    cx.update_global::<SyncStatuses,_>(|statuses,_| {
        if let Some(status) = statuses.0.get_mut(root) { status.retry_requested = true; }
    });
    retry(root,cx);
}

// Wake account verification without changing the durable upload queue or retrying held edits.
pub(crate) fn recheck_account(root: &Path, cx: &App) {
    if let Some(session) = session(root, cx) { let _ = session.sender.try_send(()); }
}

pub(crate) fn account_is_current(root: &Path, cx: &App) -> bool {
    session(root, cx).is_some_and(|session| session.account_revision == account_revision())
        && !root.join("require-account-verification").exists()
}

// The Swift bridge compared the current account with this personal session before exchanging
// data. Serialize marker removal with the native account-change callback.
fn complete_account_check(root: &Path, revision: u64) -> Result<()> {
    let _guard = ACCOUNT_WATCH.get_or_init(Default::default).lock().unwrap();
    check_account_revision(revision)?;
    let marker = root.join("require-account-verification");
    match std::fs::remove_file(&marker) {
        Ok(()) => {
            if let Err(error) = check_account_revision(revision) {
                protect_offline_library(&marker);
                return Err(error);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

pub(crate) fn access_changed(_cx: &App) { sync_resumed(0); }

pub(crate) fn last_success(root: &Path, cx: &App) -> Option<u64> {
    let status = status(root, cx)?;
    if !account_is_current(root, cx)
        || matches!(status.phase, SyncPhase::AccountChecking | SyncPhase::AccountChanged | SyncPhase::SignedOut | SyncPhase::Account | SyncPhase::Revoked)
    { return None; }
    status.last_success
}

fn update_status(root: &Path, cx: &mut App, phase: SyncPhase) { update_status_after_exchange(root, cx, phase, false); }

#[cfg(any(feature = "devtools", target_os = "ios"))]
#[cfg_attr(target_os = "ios", cfg(feature = "mobile-qa"))]
pub(crate) fn fixture_status(root: &Path, phase: SyncPhase, cx: &mut App) {
    let Some(status) = status(root,cx) else { return; };
    let (uploads, stalled) = match phase {
        SyncPhase::Failed => (2, Some(SyncPhase::Uploading)),
        SyncPhase::Uploading | SyncPhase::Offline => (2, None),
        _ => (0, None),
    };
    let personal = scope(root,cx).is_none();
    let replacement = SyncStatus { phase, uploads, stalled,
        access_verified: personal && matches!(phase, SyncPhase::Ready | SyncPhase::Offline),
        access_paused: phase == SyncPhase::SubscriptionRequired,
        last_success: status.last_success.or((personal && matches!(phase, SyncPhase::Ready | SyncPhase::Offline)).then(crate::time::now)),
        ..Default::default() };
    cx.update_global::<SyncStatuses,_>(|statuses,_| { statuses.0.insert(root.to_path_buf(),replacement); });
}

#[cfg(any(feature = "devtools", target_os = "ios"))]
#[cfg_attr(target_os = "ios", cfg(feature = "mobile-qa"))]
pub(crate) fn personal_status_fixture(name: &str) -> Option<SyncPhase> {
    Some(match name {
        "ready" => SyncPhase::Ready,
        "offline" => SyncPhase::Offline,
        "verifying" => SyncPhase::AccountChecking,
        "signed-out" => SyncPhase::SignedOut,
        "changed" => SyncPhase::AccountChanged,
        "no-access" => SyncPhase::SubscriptionRequired,
        _ => return None,
    })
}

fn update_status_after_exchange(root: &Path, cx: &mut App, phase: SyncPhase, authorized: bool) {
    #[cfg(target_os = "ios")]
    #[cfg(feature = "mobile-qa")]
    if crate::ipad_session::sync_badge_fixture() && (scope(root,cx).is_some()
        || std::env::var("FEI_QA_PERSONAL_SYNC_STATE").ok().is_some_and(|name|personal_status_fixture(&name).is_some())) { return; }

    let journal = journal(root, cx);
    let ((uploads, downloads, held, attachments), conflicts, catalogue_paused, phase) =
        match (journal.counts(), journal.pending_work(), journal.catalogue_paused()) {
        (Ok(counts), Ok((_, _, conflicts)), Ok(paused)) => (counts, conflicts, paused, phase),
        _ => ((0, 0, 0, 0), 0, false, SyncPhase::Failed),
    };
    let before = status(root, cx).expect("session status");
    let access_paused = !authorized && (phase == SyncPhase::SubscriptionRequired || before.access_paused);
    let access_verified = authorized || before.access_verified && !access_paused
        && !matches!(phase, SyncPhase::Account | SyncPhase::AccountChecking | SyncPhase::AccountChanged
            | SyncPhase::SignedOut | SyncPhase::TestFlightVerification);
    let refresh_access = before.access_paused != access_paused || before.phase != phase
        && matches!(phase, SyncPhase::AccountChecking | SyncPhase::AccountChanged | SyncPhase::SignedOut);
    let progress = &session(root, cx).unwrap().progress;
    let status = SyncStatus { phase, uploads, downloads, held, attachments, conflicts, catalogue_paused, access_paused,
        access_verified, stalled: progress.stalled(), apply_wait: journal.apply_wait, retry_requested: false, last_success: progress.last_success };
    if before != &status {
        #[cfg(target_os = "ios")]
        if scope(root, cx).is_none() && std::env::var("FEI_LOCAL_ACCEPTANCE_MODE").as_deref() == Ok("inhouse") {
            // Keep this bounded and free of document data, account IDs and raw error strings.
            let summary = serde_json::json!({
                "phase": format!("{:?}", status.phase),
                "stalled": status.stalled.map(|p| format!("{p:?}")),
                "uploads": status.uploads, "downloads": status.downloads,
                "conflicts": status.conflicts, "catalogue_paused": status.catalogue_paused,
                "attachments": status.attachments, "wait": status.apply_wait,
                "last_success": status.last_success,
            });
            let folder = std::env::temp_dir();
            let staging = folder.join("feinotes-sync-stage.tmp");
            if std::fs::write(&staging, summary.to_string()).is_ok() {
                let _ = std::fs::rename(staging, folder.join("feinotes-sync-summary.json"));
            }
        }
        cx.update_global::<SyncStatuses, _>(|statuses, _| { statuses.0.insert(root.to_path_buf(), status); });
    }
    if scope(root, cx).is_some() && refresh_access { app::cloud_notes_changed(root, &[], false, cx); }
}

fn record_success(session: &mut Session) {
    if !session.progress.should_record_success() { return; }
    let time = crate::time::now();
    if time == 0 { return; }
    match session.journal.record_success(time) {
        Ok(()) => session.progress.record_success(time),
        Err(error) => crate::logging::write(&format!("Could not save last successful iCloud sync: {error:#}")),
    }
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
    let discover_owned = opened.identity().1.is_none() && test_root()?.is_none();
    let marker = opened.account_verification_marker()?;
    let generation = uuid::Uuid::new_v4();
    let (retry_tx, retry_rx) = async_channel::bounded(1);
    let (account_tx, account_rx) = async_channel::bounded(1);
    let (presence_wake, presence_rx) = async_channel::bounded(1);
    if !cx.has_global::<Sync>() { cx.set_global(Sync::default()); }
    if !cx.has_global::<SyncStatuses>() { cx.set_global(SyncStatuses::default()); }
    let last_success = opened.last_success().unwrap_or_else(|error| {
        crate::logging::write(&format!("Could not read last successful iCloud sync: {error:#}"));
        None
    });
    cx.global_mut::<Sync>().0.insert(root.clone(), Session { journal: opened, store, generation, sender: retry_tx.clone(), presence_wake,
        retry_attachments: false, progress: progress::Monitor::new(last_success), account_revision: account_revision() });
    cx.update_global::<SyncStatuses, _>(|statuses, _| { statuses.0.insert(root.clone(), SyncStatus::default()); });
    let watch = ACCOUNT_WATCH.get_or_init(Default::default);
    let first = { let mut watch = watch.lock().unwrap(); let first = watch.is_empty(); watch.insert(root.clone(), (marker, retry_tx, account_tx)); first };
    let local_session_qa = cfg!(feature = "devtools") && crate::devtools::scripted()
        && std::env::var_os("QUIET_SYNC_QA_SHARED_SESSIONS").is_some();
    if first && !local_session_qa { unsafe { quiet_cloud_watch_sync_resume(sync_resumed) }; }
    update_status(&root, cx, SyncPhase::Connecting);
    if let Err(error) = presence::start(cx, root.clone(), generation, pause.clone(), presence_rx) { release(&root, cx); return Err(error); }
    if !room {
        activity::start(cx, root.clone(), account, pause.clone(), generation, discover_owned && !local_session_qa);
    }
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
        let mut replayed_stalled_apply = false;
        let mut delay = Duration::from_secs(2);
        let mut reported_ready = false;
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
                if room && idle_shared_session(session(&root, cx).unwrap()) { release(&root, cx); return false; }
                true
            }).unwrap_or(false) { break; }
            let mut round = progress::Round::default();
            let mut failure_phase = SyncPhase::Applying;
            let result = crate::gpui_compat::async_update(cx, |cx| {
                if observed_revision != account_revision() {
                    observed_revision = account_revision();
                    update_status(&root, cx, SyncPhase::AccountChecking);
                }
                if std::mem::take(&mut cx.global_mut::<Sync>().0.get_mut(&root).unwrap().retry_attachments) {
                    upload_retry_at = None;
                    let retry_apply = !room && status(&root, cx).is_some_and(|s| s.stalled == Some(SyncPhase::Applying));
                    let journal = journal_mut(&root, cx);
                    journal.retry_conflicts()?;
                    let replay = journal.retry_attachments()?;
                    if replay || retry_apply && journal.replay_pending_apply()? {
                        legacy_checked = None;
                        replayed_stalled_apply |= retry_apply;
                    }
                }
                let legacy = !room && legacy_checked.is_none_or(|time| time.elapsed() >= Duration::from_secs(60));
                round.legacy = legacy;
                merge(&root, &mut round, cx)?;
                update_status(&root, cx, status(&root, cx).unwrap().phase);
                failure_phase = SyncPhase::Failed;
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
            let (legacy, allow_upload, wanted) = request.as_ref().map(|(legacy, allow_upload, request)|
                (*legacy, *allow_upload, request.downloads.clone())).unwrap_or_default();
            let result = match request {
                Ok(_) if pause.as_ref().is_some_and(|path| path.exists()) => {
                    delay = Duration::from_secs(2);
                    continue;
                }
                Ok((_, _, request)) => {
                    failure_phase = SyncPhase::Connecting;
                    cx.background_executor().spawn(async move { exchange_page(request) }).await
                }
                Err(error) => Err(error),
            };
            if !crate::gpui_compat::async_update(cx, |cx| current(&root, generation, cx)).unwrap_or(false) { break; }
            let Ok(result) = crate::gpui_compat::async_update(cx, |cx| -> Result<_> {
                check_account_revision(revision)?;
                let (page, cursor_advanced) = result?;
                if !room { complete_account_check(&root, revision)?; }
                round.connected = true;
                let (more, quota_exceeded) = (page.more, page.upload_quota_exceeded);
                failure_phase = SyncPhase::Applying;
                let scope_session = cx.global_mut::<Sync>().0.get_mut(&root).unwrap();
                scope_session.account_revision = revision;
                if !page.uploaded.is_empty() || !page.edits.is_empty() {
                    crate::logging::write(&format!("CloudKit sync uploaded={} downloaded={}", page.uploaded.len(), page.edits.len()));
                }
                let (access_changed, uploaded) = scope_session.journal.receive_page(page)?;
                round.uploaded = uploaded;
                round.download_advanced = more && cursor_advanced;
                if access_changed { app::cloud_notes_changed(&root, &[], true, cx); }
                merge(&root, &mut round, cx)?;
                let journal = journal(&root, cx);
                let (uploads, _, held, attachments) = journal.counts()?;
                let (active_uploads, active_downloads, conflicts) = journal.pending_work()?;
                let access = journal.access();
                let revoked = access == Some(shared::Access::Revoked);
                let read_only = access == Some(shared::Access::ReadOnly);
                let next_downloads = if legacy || more || revoked { vec![] } else { journal.attachment_downloads()? };
                round.download_advanced |= !more && round.applied && wanted.iter().any(|id| !next_downloads.contains(id));
                round.downloading = more || !next_downloads.is_empty();
                round.applying = !revoked && active_downloads > 0;
                round.uploading = !revoked && !read_only && !more && active_uploads > 0;
                Ok((more, quota_exceeded, uploads == 0 && held == 0 && attachments == 0 && conflicts == 0 && !journal.catalogue_paused()?, revoked, read_only && uploads > 0))
            }) else { break };
            match result {
                Ok((more, quota_exceeded, uploads_clean, revoked, read_only_pending)) => {
                    account_notice = None;
                    if legacy && !more { legacy_checked = Some(std::time::Instant::now()); }
                    if quota_exceeded { upload_retry_at = Some(std::time::Instant::now() + Duration::from_secs(30)); }
                    else if allow_upload { upload_retry_at = None; }
                    let (pending, collaborating, settled, stalled_apply) = crate::gpui_compat::async_update(cx, |cx| {
                        let phase = if revoked { SyncPhase::Revoked }
                            else if upload_retry_at.is_some() { SyncPhase::Quota }
                            else if round.downloading { SyncPhase::Downloading }
                            else if round.applying { SyncPhase::Applying }
                            else if read_only_pending { SyncPhase::ReadOnly }
                            else if round.uploading { SyncPhase::Uploading }
                            else { SyncPhase::Ready };
                        let scope_session = cx.global_mut::<Sync>().0.get_mut(&root).unwrap();
                        scope_session.progress.completed(Instant::now(), round);
                        if !legacy && uploads_clean && phase == SyncPhase::Ready && !round.apply_blocked_by_local_edits
                            && account_revision() == revision {
                            record_success(scope_session);
                        }
                        update_status_after_exchange(&root, cx, phase, true);
                        let collaborating = match app::cloud_active_selection(&root, cx) {
                            presence::SelectionSource::Editing(id, _) => presence::view(&root, cx).expect("presence view").notes.contains_key(&id),
                            _ => false,
                        };
                        let writable = journal(&root, cx).access().is_none_or(shared::Access::can_write);
                        let status = status(&root, cx).unwrap();
                        (status.uploads > 0 && writable, collaborating,
                            status.phase == SyncPhase::Ready && status.uploads == 0 && status.downloads == 0 && status.conflicts == 0 && !status.catalogue_paused,
                            phase == SyncPhase::Applying && status.stalled == Some(SyncPhase::Applying))
                    }).unwrap_or_default();
                    if !room && stalled_apply && !replayed_stalled_apply {
                        // One automatic inventory replay per session. A missing
                        // predecessor can block imports even on a healthy connection.
                        replayed_stalled_apply = true;
                        let replay = crate::gpui_compat::async_update(cx, |cx| {
                            if !current(&root, generation, cx) || account_revision() != revision { return Ok(false); }
                            journal(&root, cx).replay_pending_apply()
                        });
                        match replay {
                            Ok(Ok(true)) => {
                                legacy_checked = None;
                                delay = Duration::ZERO;
                                continue;
                            }
                            Ok(Err(error)) => crate::logging::write(&format!("Could not restart stalled iCloud apply: {error:#}")),
                            _ => {}
                        }
                    }
                    if !legacy && settled && !reported_ready {
                        crate::logging::write("CloudKit sync ready: no pending uploads or downloads");
                        reported_ready = true;
                    }
                    last_error = None;
                    delay = if (legacy && !more) || (round.downloading && round.download_advanced)
                        || (pending && upload_retry_at.is_none() && round.uploaded) { Duration::ZERO }
                        else if collaborating { Duration::from_secs(1) }
                        else { Duration::from_secs(3) };
                }
                Err(error) => {
                    reported_ready = false;
                    let phase = error.downcast_ref::<CloudFailure>().map_or(failure_phase, |error| error.phase);
                    let _ = crate::gpui_compat::async_update(cx, |cx| {
                        if matches!(phase, SyncPhase::AccountChanged | SyncPhase::SignedOut) {
                            if let Ok(marker) = journal(&root, cx).account_verification_marker() { protect_offline_library(&marker); }
                            if account_notice != Some(phase) {
                                app::cloud_account_notice(&root, phase, cx);
                                account_notice = Some(phase);
                            }
                        }
                        cx.global_mut::<Sync>().0.get_mut(&root).unwrap().progress.failed(Instant::now(), phase, round);
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

fn merge(root: &Path, round: &mut progress::Round, cx: &mut App) -> Result<()> {
    let before = journal(root, cx).inbox_count()?;
    let Some(positions) = app::cloud_open_positions(root, cx) else {
        round.apply_blocked_by_local_edits = true;
        journal_mut(root, cx).apply_wait = ApplyWait { local_edit: true, ..Default::default() };
        return Ok(());
    };
    round.apply_blocked_by_local_edits = false;
    let journal = journal_mut(root, cx);
    let (library_changed, changed) = journal.merge(&positions).inspect_err(|_| {
        journal.apply_wait.local_error = true;
    })?;
    round.applied |= journal.inbox_count()? < before;
    if library_changed || !changed.is_empty() {
        if let Some(scope) = scope(root, cx) { shared::original::refresh_owner(root, scope, None)?; }
        app::cloud_notes_changed(root, &changed, true, cx);
    }
    Ok(())
}

fn wake_after_save(root: &Path, cx: &mut App) {
    update_status(root, cx, status(root, cx).unwrap().phase);
    let status = status(root, cx).unwrap();
    if status.uploads > 0 && matches!(status.phase, SyncPhase::Ready | SyncPhase::Connecting | SyncPhase::Uploading) {
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
    let Some(sync) = session(root, cx) else { return Ok(vec![]); };
    let mut rows = sync.journal.offline_recoveries()?;
    if recovery_account_ready(root, cx) {
        let shared = sync.journal.shared_recoveries()?;
        if recovery_account_ready(root, cx) { rows.extend(shared); }
    }
    Ok(rows)
}

pub(crate) fn resolve_offline_recovery(root: &Path, id: &str, keep: bool, cx: &mut App) -> Result<Option<String>> {
    ensure!(session(root, cx).is_some(), "This library has no active sync session");
    let copy = if journal::recovery::is_shared(id) {
        ensure!(recovery_account_ready(root, cx), "{}", crate::settings::tr(
            "请先确认 iCloud 账户。", "Verify your iCloud account first."));
        let revision = session(root, cx).unwrap().account_revision;
        journal_mut(root, cx).resolve_shared_recovery(id, keep, || {
            check_account_revision(revision)?;
            ensure!(!root.join("require-account-verification").exists(), "The iCloud account requires verification.");
            Ok(())
        })?
    } else { journal_mut(root, cx).resolve_offline_recovery(id, keep)? };
    if copy.is_some() && let Some(session) = session(root, cx) { let _ = session.sender.try_send(()); }
    Ok(copy)
}

fn recovery_account_ready(root: &Path, cx: &App) -> bool {
    root == Store::root_dir() && !crate::cloud_restore::local_only()
        && !root.join("require-account-verification").exists()
        && session(root, cx).is_some_and(|sync| sync.journal.room().is_none() && sync.account_revision == account_revision())
        && status(root, cx).is_some_and(|s| !matches!(s.phase,
            SyncPhase::Account | SyncPhase::AccountChecking | SyncPhase::AccountChanged | SyncPhase::SignedOut))
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
    fn only_the_current_account_revision_can_clear_the_verification_marker() -> Result<()> {
        let root = std::env::temp_dir().join(format!("fei-account-verification-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root)?;
        let marker = root.join("require-account-verification");
        let result = (|| -> Result<()> {
            std::fs::write(&marker, [])?;
            let revision = account_revision();
            assert!(complete_account_check(&root, revision.wrapping_add(1)).is_err());
            assert!(marker.is_file());
            complete_account_check(&root, revision)?;
            assert!(!marker.exists());
            complete_account_check(&root, revision)?;
            Ok(())
        })();
        std::fs::remove_dir_all(root)?;
        result
    }

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
