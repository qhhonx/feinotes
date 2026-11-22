//! Open product CloudKit libraries separately from isolated development channels.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{ffi::{CStr, CString, c_char}, path::{Path, PathBuf}};

unsafe extern "C" {
    fn fei_ios_product_cloud() -> bool;
    fn quiet_cloud_restore(action: i32, path: *const c_char, scope: *const c_char) -> *mut c_char;
    fn quiet_cloud_restore_free(reply: *mut c_char);
}

#[derive(Deserialize)]
struct RestoreReply {
    path: Option<PathBuf>,
    source: Option<String>,
    error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibrarySelection { Local, Cloud, CopyToCloud }

pub(crate) fn startup_fallback() -> Option<&'static str> { crate::ipad_session::startup_fallback() }

pub(crate) fn production() -> bool { env!("FEI_CLOUD_NAMESPACE") == "Production:iCloud.app.feinotes.mac" }
pub(crate) fn configuration_matches() -> bool { production() == unsafe { fei_ios_product_cloud() } }
pub(crate) fn parent(base: &Path, channel: uuid::Uuid) -> PathBuf {
    if production() { base.parent().expect("application support parent").join("Fei Notes Library").join("Production").join("iCloud.app.feinotes.mac") }
    else { base.join(channel.to_string()) }
}
pub(crate) fn scope(channel: uuid::Uuid) -> Option<String> { (!production()).then(||channel.to_string()) }

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnableCheck { Ready, SignedOut, AccessRequired, Unavailable }

pub(crate) fn check_enable() -> EnableCheck { crate::ipad_session::check_cloud_enable() }

pub(crate) fn check_account(parent: &Path, channel: uuid::Uuid) -> EnableCheck {
    #[cfg(feature = "mobile-qa")]
    if std::env::var("FEI_QA_CLOUD_SWITCH_FAILURE").as_deref() == Ok("1") { return EnableCheck::Ready; }
    match request(parent,channel,11).ok().and_then(|reply|reply.source).as_deref() {
        Some("ready") => EnableCheck::Ready,
        Some("signedOut") => EnableCheck::SignedOut,
        Some("accessRequired") => EnableCheck::AccessRequired,
        _ => EnableCheck::Unavailable,
    }
}

pub(crate) fn cached(parent: &Path, channel: uuid::Uuid) -> Result<Option<PathBuf>> {
    let reply = request(parent,channel,8)?;
    if reply.path.is_none() || reply.source.as_deref() == Some("localOnly") { return Ok(None); }
    checked_root(parent,reply).map(Some)
}

pub(crate) fn prepare(parent: &Path, channel: uuid::Uuid) -> Result<PathBuf> {
    checked_root(parent,request(parent,channel,10)?)
}

pub(crate) fn prepare_selection(parent: &Path, channel: uuid::Uuid) -> Result<PathBuf> {
    #[cfg(feature = "mobile-qa")]
    ensure!(std::env::var("FEI_QA_CLOUD_SWITCH_FAILURE").as_deref() != Ok("1"), "Cloud switch QA failure; local notes are kept");
    checked_root(parent,request(parent,channel,4)?)
}

fn request(parent: &Path, channel: uuid::Uuid, action: i32) -> Result<RestoreReply> {
    #[cfg(feature = "mobile-qa")]
    ensure!(std::env::var("FEI_QA_CLOUD_SWITCH_FAILURE").as_deref() != Ok("1"), "Cloud switch QA failure; local notes are kept");
    ensure!(configuration_matches(), "The native and Rust CloudKit configurations differ");
    ensure!(parent.is_absolute() && (production() || parent.file_name().is_some_and(|name| name == channel.to_string().as_str())),
        "Use an isolated CloudKit QA library for the requested channel");
    let path = CString::new(parent.as_os_str().as_encoded_bytes())?;
    let scope = scope(channel).map(CString::new).transpose()?;
    let ptr = unsafe { quiet_cloud_restore(action, if production() { std::ptr::null() } else { path.as_ptr() }, scope.as_ref().map_or(std::ptr::null(),|scope|scope.as_ptr())) };
    ensure!(!ptr.is_null(), "CloudKit returned no response");
    let decoded = unsafe { serde_json::from_slice::<RestoreReply>(CStr::from_ptr(ptr).to_bytes()) };
    unsafe { quiet_cloud_restore_free(ptr) };
    let reply = decoded?;
    if let Some(error) = reply.error { anyhow::bail!("{error}"); }
    Ok(reply)
}

fn checked_root(parent: &Path, reply: RestoreReply) -> Result<PathBuf> {
    ensure!(reply.source.as_deref() != Some("localOnly"), "The CloudKit account is unavailable");
    let root = reply.path.context("CloudKit returned no library path")?;
    ensure!(root.parent() == Some(parent), "CloudKit returned a different library");
    crate::store::Store::initialize_sqlite(&root)?;
    Ok(root)
}

pub(crate) fn local_only() -> bool { !crate::ipad::has_cloud() }

pub(crate) fn lock_library(root: &Path) -> Result<std::fs::File> {
    let lock = std::fs::OpenOptions::new().write(true).create(true).truncate(false).open(root.join("sync.lock"))?;
    lock.try_lock().context("The test library is already open")?;
    Ok(lock)
}

// Copy into the existing local library without changing the active cloud session.
pub(crate) fn copy_note_to_local(source: &Path, id: &str) -> Result<String> {
    ensure!(!local_only() && !crate::in_shared_note(), "Open a personal iCloud note first");
    let root = crate::ipad_session::base_path()?.join("local-only");
    std::fs::create_dir_all(&root)?;
    let _lock = lock_library(&root)?;
    crate::store::Store::initialize_local_sqlite(&root)?;
    crate::store::Store::copy_note_to_local(&root, source, id)
}
