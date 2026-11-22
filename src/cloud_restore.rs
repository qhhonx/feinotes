// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Startup boundary for the signed macOS library-restore build.

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::ffi::{CStr, CString, c_char};
use std::path::{Path, PathBuf};

unsafe extern "C" {
    fn quiet_cloud_restore(action: i32, path: *const c_char, scope: *const c_char) -> *mut c_char;
    fn quiet_cloud_restore_free(reply: *mut c_char);
    fn quiet_cloud_restore_alert(message: *const c_char);
}

#[derive(Deserialize)]
pub(super) struct Reply {
    path: Option<PathBuf>,
    source: Option<String>,
    #[cfg(any(not(feature = "cloudkit-sync"), feature = "devtools"))]
    count: Option<usize>,
    error: Option<String>,
    #[cfg(feature = "cloudkit-sync")]
    #[serde(rename = "fallbackReason")]
    fallback_reason: Option<String>,
}

fn request(action: i32, path: Option<&Path>, scope: Option<&str>) -> Result<Reply> {
    let path = path.map(|p| CString::new(p.as_os_str().as_encoded_bytes())).transpose()?;
    // Swift copies the input before starting its asynchronous worker.
    let scope = scope.map(CString::new).transpose()?;
    let ptr = unsafe { quiet_cloud_restore(action, path.as_ref().map_or(std::ptr::null(), |p| p.as_ptr()),
        scope.as_ref().map_or(std::ptr::null(), |p| p.as_ptr())) };
    ensure!(!ptr.is_null(), "CloudKit returned no response");
    let result = unsafe { serde_json::from_slice::<Reply>(CStr::from_ptr(ptr).to_bytes()) };
    unsafe { quiet_cloud_restore_free(ptr) };
    let reply = result?;
    #[cfg(feature = "cloudkit-sync")]
    if reply.error.is_some() && reply.fallback_reason.as_deref() == Some("subscriptionRequired") {
        anyhow::bail!("{}", crate::cloud_trial::access_required());
    }
    if let Some(message) = &reply.error { anyhow::bail!("{message}") }
    Ok(reply)
}

#[cfg(feature = "cloudkit-sync")]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnableCheck { Ready, SignedOut, AccessRequired, Unavailable }

#[cfg(feature = "cloudkit-sync")]
pub(crate) fn check_enable() -> EnableCheck {
    #[cfg(feature = "devtools")]
    let channel = crate::cloud_trial::test_channel().ok().flatten();
    #[cfg(not(feature = "devtools"))]
    let channel: Option<String> = None;
    match request(11,None,channel.as_deref()).ok().and_then(|reply|reply.source).as_deref() {
        Some("ready") => EnableCheck::Ready,
        Some("signedOut") => EnableCheck::SignedOut,
        Some("accessRequired") => EnableCheck::AccessRequired,
        _ => EnableCheck::Unavailable,
    }
}

#[cfg(all(test, feature = "cloudkit-sync"))]
#[test]
fn local_fallback_keeps_the_reason_from_the_swift_bridge() -> Result<()> {
    let reply: Reply = serde_json::from_str(r#"{"path":"/tmp/local-only","source":"localOnly","fallbackReason":"signedOut"}"#)?;
    assert_eq!(reply.source.as_deref(), Some("localOnly"));
    assert_eq!(reply.fallback_reason.as_deref(), Some("signedOut"));
    Ok(())
}

#[cfg(all(test, feature = "cloudkit-sync", feature = "devtools"))]
#[test]
fn startup_preflight_only_opens_the_selected_verified_cache() -> Result<()> {
    use sha2::{Digest, Sha256};
    let parent = std::env::temp_dir().join(format!("quiet-sync-qa-{}",uuid::Uuid::new_v4()));
    let key = "a".repeat(64);
    let root = parent.join(&key);
    std::fs::create_dir_all(&root)?;
    let lookup = || request(8,Some(&parent),None);
    assert!(lookup()?.path.is_none());
    std::fs::write(parent.join("qa-startup-account"),"bound-cache")?;
    std::fs::write(parent.join("active-account"),&key)?;
    std::fs::write(parent.join("verified-account"),&key)?;
    std::fs::write(root.join(".cloud-restore-ready"),format!("{key}\nfixture"))?;
    std::fs::write(root.join(crate::store::SQLITE_FILE),[])?;
    assert!(lookup()?.path.is_none());
    std::fs::write(parent.join("qa-startup-account"),"slow-unavailable")?;
    assert_eq!(request(10,Some(&parent),None)?.source.as_deref(),Some("localOnly"));
    std::fs::write(parent.join("qa-startup-account"),"bound-cache")?;
    std::fs::write(root.join(".device-identity"),format!("{:x}",Sha256::digest(b"QA identity")))?;
    assert_eq!(lookup()?.path.as_deref(),Some(root.as_path()));
    std::fs::write(root.join(".device-identity"),"another-device-identity")?;
    assert!(lookup()?.path.is_none());
    std::fs::write(root.join(".device-identity"),format!("{:x}",Sha256::digest(b"QA identity")))?;
    std::fs::write(parent.join("verified-account"),"b".repeat(64))?;
    assert!(lookup()?.path.is_none());
    std::fs::write(parent.join("verified-account"),&key)?;
    std::fs::write(root.join("require-account-verification"),[])?;
    assert!(lookup()?.path.is_none());
    std::fs::remove_file(root.join("require-account-verification"))?;
    std::fs::write(parent.join("verified-account"),"signed-out")?;
    assert!(lookup()?.path.is_none());
    std::fs::write(parent.join("use-local-library"),[])?;
    assert_eq!(lookup()?.source.as_deref(),Some("localOnly"));
    assert_eq!(request(10,Some(&parent),None)?.source.as_deref(),Some("localOnly"));
    std::fs::remove_dir_all(parent)?;
    Ok(())
}

pub fn startup() {
    #[cfg(all(feature = "cloudkit-sync", feature = "devtools"))]
    if std::env::args().nth(1).as_deref() == Some("--cloud-sync-prepare-copy") {
        match crate::cloud_sync::qa::prepare_legacy() {
            Ok(()) => std::process::exit(0),
            Err(error) => crate::logging::fatal("Quiet Cloud migration QA", &error),
        }
    }
    #[cfg(all(feature = "cloudkit-sync", feature = "devtools"))]
    if std::env::args().nth(1).as_deref() == Some("--cloud-sync-publish-copy") {
        match publish_copy() {
            Ok(()) => std::process::exit(0),
            Err(error) => crate::logging::fatal("Quiet Cloud migration QA", &error),
        }
    }
    #[cfg(feature = "cloudkit-sync")]
    {
        #[cfg(feature = "devtools")]
        if std::env::args().nth(1).is_some_and(|arg| matches!(arg.as_str(), "--cloud-restore-publish" | "--cloud-restore-check")) {
            legacy_startup();
            return;
        }
        if let Err(error) = sync_startup() { startup_error(&error); }
    }
    #[cfg(not(feature = "cloudkit-sync"))]
    legacy_startup();
}

#[cfg(feature = "cloudkit-sync")]
pub(super) fn startup_error(error: &anyhow::Error) -> ! {
    if !crate::devtools::scripted() && let Ok(message) = CString::new(format!("{error:#}")) {
        unsafe { quiet_cloud_restore_alert(message.as_ptr()) };
    }
    crate::logging::fatal("Quiet Cloud Sync",error)
}

#[cfg(any(not(feature = "cloudkit-sync"), feature = "devtools"))]
fn legacy_startup() {
    let mut args = std::env::args_os().skip(1);
    let command = args.next();
    let command = command.as_ref().and_then(|value| value.to_str());
    let path = args.next().map(PathBuf::from);
    let diagnostic = matches!(command, Some("--cloud-restore-publish" | "--cloud-restore-check"));
    let result = match command {
        Some("--cloud-restore-publish") => publish(path.as_deref()),
        Some("--cloud-restore-check") => restore(path.as_deref(), true),
        _ => restore(None, false),
    };
    if let Err(error) = result {
        if !diagnostic && let Ok(message) = CString::new(format!("{error:#}")) {
            unsafe { quiet_cloud_restore_alert(message.as_ptr()) };
        }
        crate::logging::fatal("Quiet Cloud Restore", &error);
    }
    if diagnostic { std::process::exit(0) }
}

#[cfg(feature = "cloudkit-sync")]
static LOCAL_LOCK: std::sync::OnceLock<std::fs::File> = std::sync::OnceLock::new();

#[cfg(feature = "cloudkit-sync")]
pub fn local_only() -> bool { LOCAL_LOCK.get().is_some() }

#[cfg(feature = "cloudkit-sync")]
static STARTUP_FALLBACK: std::sync::OnceLock<String> = std::sync::OnceLock::new();

#[cfg(feature = "cloudkit-sync")]
pub(crate) fn startup_fallback() -> Option<&'static str> {
    use crate::settings::tr;
    Some(match STARTUP_FALLBACK.get()?.as_str() {
        "signedOut" => tr("当前未登录 iCloud，已打开本机文档库。", "You’re signed out of iCloud. The local library is open."),
        "restricted" => tr("当前账户无法使用 iCloud，已打开本机文档库。", "iCloud is restricted for this account. The local library is open."),
        "accountChanged" => tr("iCloud 账户已切换。已打开本机文档库，请重新连接 iCloud。",
            "The iCloud account changed. The local library is open. Reconnect to iCloud to switch libraries."),
        "restoreFailed" => tr("暂时无法打开 iCloud 文档库，已打开本机文档库。", "The iCloud library could not be opened. The local library is open."),
        "subscriptionRequired" => tr("已打开本机文档库。开始试用或订阅后可从 iCloud 恢复。",
            "The local library is open. Start a trial or subscribe to restore from iCloud."),
        _ => tr("暂时无法连接 iCloud，已打开本机文档库。", "iCloud is temporarily unavailable. The local library is open."),
    })
}

#[cfg(feature = "cloudkit-sync")]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LibrarySelection { Local, Cloud, CopyToCloud }

#[cfg(feature = "cloudkit-sync")]
pub fn select_library(selection: LibrarySelection) -> Result<usize> {
    let cloud = selection != LibrarySelection::Local;
    ensure!(cloud == local_only(), "请从当前文档库切换到另一种模式");
    let test = crate::cloud_sync::test_root()?;
    let scope = migration_scope()?;
    let reply = request(if cloud { 4 } else { 5 }, test.as_deref(), scope.as_deref())?;
    if !cloud { return Ok(0); }
    let root = reply.path.context("Missing iCloud library path")?;
    root.parent().context("Missing iCloud library folder")?;
    let key = root.file_name().and_then(|key| key.to_str()).context("Missing iCloud account key")?;
    ensure!(key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit()), "Invalid iCloud account key");
    let lock = lock_library(&root)?;
    crate::store::Store::initialize_sqlite(&root)?;
    let copied = if selection == LibrarySelection::CopyToCloud {
        drop(lock);
        crate::cloud_sync::copy_local_library(&root, &crate::store::Store::root_dir(), scope)?
    } else { 0 };
    crate::cloud_sync::library::commit(&root)?;
    Ok(copied)
}

#[cfg(feature = "cloudkit-sync")]
pub(crate) fn copy_note_to_local(id: &str) -> Result<String> {
    use crate::store::Store;
    ensure!(!local_only() && !crate::in_shared_note(), "{}", crate::settings::tr(
        "请在个人 iCloud 文档库中复制文档。", "Copy a document from your personal iCloud library."));
    let source = Store::root_dir();
    let parent = source.parent().context("Missing local library folder")?;
    let root = parent.join("local-only");
    std::fs::create_dir_all(&root)?;
    let _lock = lock_library(&root)?;
    ensure!(!parent.join(".restore-local-only").exists(), "{}", crate::settings::tr(
        "请先重开应用以恢复本机备份。", "Reopen the app to restore the local backup first."));
    Store::initialize_local_sqlite(&root)?;
    Store::copy_note_to_local(&root, &source, id)
}

#[cfg(feature = "cloudkit-sync")]
pub(crate) fn lock_library(root: &Path) -> Result<std::fs::File> {
    let lock = std::fs::OpenOptions::new().write(true).create(true).truncate(false).open(root.join("sync.lock"))?;
    lock.try_lock().context("请先退出正在使用该文档库的另一份应用。")?;
    Ok(lock)
}

#[cfg(feature = "cloudkit-sync")]
pub(crate) static BACKUP_RESTORE_ERROR: std::sync::OnceLock<String> = std::sync::OnceLock::new();
#[cfg(feature = "cloudkit-sync")]
pub(crate) static BACKUP_RESTORED: std::sync::OnceLock<()> = std::sync::OnceLock::new();

#[cfg(feature = "cloudkit-sync")]
fn sync_startup() -> Result<()> {
    #[cfg(feature = "devtools")]
    if std::env::var_os("QUIET_SYNC_QA_SHARED_SESSIONS").is_some() {
        ensure!(crate::devtools::scripted(), "Shared-session QA requires the offscreen harness");
        let root = crate::cloud_sync::test_root()?.context("Shared-session QA requires an isolated root")?;
        return crate::store::Store::set_sqlite_session_root(crate::cloud_sync::shared::session_qa::prepare(&root)?);
    }
    let test = crate::cloud_sync::test_root()?;
    let scope = migration_scope()?;
    let shared = std::env::args().nth(1).as_deref() == Some("--shared-note-room");
    let local_qa = crate::cloud_sync::preview_channel().is_some() || cfg!(feature = "devtools") && std::env::var_os("QUIET_SYNC_QA_LOCAL_FIRST").is_some();
    let action = if shared { 7 } else if test.is_some() && !local_qa { 6 } else { 2 };
    let reply = request(action, test.as_deref(), scope.as_deref())?;
    install_startup(reply,shared)
}

#[cfg(feature = "cloudkit-sync")]
pub(super) fn install_startup(reply: Reply, shared: bool) -> Result<()> {
    let root = reply.path.context("Missing sync library path")?;
    let root = if shared {
        let note = std::env::args().nth(2).context("Missing shared note")?;
        crate::cloud_sync::shared::startup(&root, &note)?
    } else { root };
    let local = reply.source.as_deref() == Some("localOnly");
    if local { std::fs::create_dir_all(&root)?; }
    let mut lock = lock_library(&root)?;
    if !shared {
        match crate::cloud_sync::backup::install(&root) {
            Ok(Some(restored)) => {
                lock = restored;
                let _ = BACKUP_RESTORED.set(());
            }
            Ok(None) => {}
            Err(error) => {
                crate::logging::write(&format!("Library backup restore: {error:#}"));
                let _ = BACKUP_RESTORE_ERROR.set(error.to_string());
            }
        }
    }
    if local {
        crate::store::Store::initialize_local_sqlite(&root)?;
        LOCAL_LOCK.set(lock).map_err(|_| anyhow::anyhow!("Local library is already open"))?;
        if let Some(reason) = reply.fallback_reason {
            crate::logging::write(&format!("iCloud startup fallback={reason}"));
            let _ = STARTUP_FALLBACK.set(reason);
        }
    } else { crate::store::Store::initialize_sqlite(&root)?; }
    crate::store::Store::set_sqlite_session_root(root)?;
    crate::logging::write(&format!("CloudKit sync library source={}", reply.source.unwrap_or_default()));
    Ok(())
}

// Interactive startup separates choosing a library from installing the
// process-wide Store root. Diagnostics and shared-room launches keep their
// dedicated synchronous startup contract.
#[cfg(feature = "cloudkit-sync")]
fn startup_request(action: i32) -> Result<Reply> {
    request(action,crate::cloud_sync::test_root()?.as_deref(),migration_scope()?.as_deref())
}

#[cfg(feature = "cloudkit-sync")]
pub(super) fn cached_startup() -> Result<Option<Reply>> {
    let reply = startup_request(8)?;
    Ok(reply.path.is_some().then_some(reply))
}

#[cfg(feature = "cloudkit-sync")]
pub(super) fn discover() -> Result<Reply> {
    let reply = startup_request(10)?;
    if reply.source.as_deref() != Some("localOnly") && let Some(root) = reply.path.as_deref()
        && root.join(".restore-source").is_dir() && !crate::cloud_sync::backup::scheduled(root) {
        // Import a downloaded snapshot off the UI thread before attaching it.
        let _lock = lock_library(root)?;
        crate::store::Store::initialize_sqlite(root)?;
    }
    Ok(reply)
}

#[cfg(feature = "cloudkit-sync")]
pub(super) fn choose_local() -> Result<Reply> { startup_request(9) }

#[cfg(feature = "cloudkit-sync")]
pub(super) fn recent_local_notes() -> Result<Vec<(String,String)>> {
    let root = choose_local()?.path.context("Missing local library path")?;
    let file = root.join(crate::store::SQLITE_FILE);
    if !file.is_file() || crate::cloud_sync::backup::scheduled(&root) { return Ok(Vec::new()); }
    // A preview must not migrate the database, install backups or open a Store
    // until the user chooses a note. Read only the titles needed for this frame.
    let db = rusqlite::Connection::open_with_flags(file,rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut stmt = db.prepare("SELECT d.id,d.title FROM library_documents d JOIN library_entries e ON e.id=d.id
        WHERE d.trashed=0 ORDER BY json_extract(e.meta,'$.modified') DESC LIMIT 8")?;
    Ok(stmt.query_map([],|row| Ok((row.get(0)?,row.get(1)?)))?.collect::<rusqlite::Result<_>>()?)
}

#[cfg(feature = "cloudkit-sync")]
fn migration_scope() -> Result<Option<String>> {
    if let Some(channel) = crate::cloud_sync::preview_channel() { return Ok(Some(channel.into())); }
    if !cfg!(feature = "devtools") || std::env::var_os("QUIET_SYNC_QA_MIGRATION").is_none() { return Ok(None); }
    ensure!(crate::cloud_sync::test_root()?.is_some(), "A migration QA run requires an isolated test root");
    let channel = std::env::var("QUIET_SYNC_QA_CHANNEL").context("A migration QA run requires a test channel")?;
    uuid::Uuid::parse_str(&channel)?;
    Ok(Some(channel))
}

#[cfg(all(feature = "cloudkit-sync", feature = "devtools"))]
fn publish_copy() -> Result<()> {
    let channel = migration_scope()?.context("Enable the migration QA scope")?;
    let root = crate::cloud_sync::test_root()?.context("Missing isolated QA root")?;
    let source = root.join("legacy-copy");
    ensure!(source.is_dir() && !source.symlink_metadata()?.is_symlink() && !root.join("active-account").exists(),
        "Expected a staged legacy copy in a fresh QA root");
    let count = request(1, Some(&source), Some(&channel))?.count.context("Missing published file count")?;
    println!("MIGRATION_PUBLISHED files={count}");
    Ok(())
}

#[cfg(any(not(feature = "cloudkit-sync"), feature = "devtools"))]
fn publish(path: Option<&Path>) -> Result<()> {
    let path = path.context("Expected the fixture directory")?;
    let reply = request(1, Some(path), None)?;
    println!("Published {} files", reply.count.context("Missing published file count")?);
    Ok(())
}

#[cfg(any(not(feature = "cloudkit-sync"), feature = "devtools"))]
fn restore(test_root: Option<&Path>, check: bool) -> Result<()> {
    if let Some(root) = test_root {
        ensure!(root.is_absolute() && root.starts_with("/private/tmp"), "Use a fresh path under /private/tmp for the test library");
    }
    let reply = request(0, test_root, None)?;
    let root = reply.path.context("Missing restored library path")?;
    crate::store::Store::set_session_root(root.clone())?;
    let source = reply.source.unwrap_or_default();
    crate::logging::write(&format!("CloudKit restore source={source}"));
    if check { verify_fixture(&root, &source)? }
    Ok(())
}

#[cfg(any(not(feature = "cloudkit-sync"), feature = "devtools"))]
fn verify_fixture(root: &Path, source: &str) -> Result<()> {
    let mut store = crate::store::Store::open()?;
    let entries = store.list();
    ensure!(entries.len() == 3, "Expected one folder and two documents");
    let folder = entries.iter().find(|e| e.meta.folder.is_some()).context("Missing restored folder")?;
    let mut notes: Vec<_> = entries.iter().filter(|e| e.meta.parent.as_deref() == Some(&folder.id)).collect();
    notes.sort_by_key(|e| e.meta.place());
    ensure!(folder.title == "双机验收" && notes.len() == 2, "Restored folder hierarchy mismatch");
    let md = notes[0];
    let xml = notes[1];
    ensure!(md.title == "Markdown 笔记" && md.meta.markdown && md.meta.starred > 0
        && xml.title == "飞书富文本" && !xml.meta.markdown && xml.meta.width == crate::store::PageWidth::Wide,
        "Restored document order or attributes mismatch");
    ensure!(md.body.contains("海盐橙子") && xml.body.contains("蓝鲸地图"), "Search index lacks the restored document text");
    ensure!(store.load(&md.id)?.plain_text().contains("海盐橙子") && store.load(&xml.id)?.plain_text().contains("蓝鲸地图"),
        "Could not open the restored notes in Quiet's document engine");
    let asset = "assets/files/987efcd9-af9b-4113-ae68-3a2eb10d40fa/实验附件.txt";
    ensure!(std::fs::read(root.join(asset))? == "Quiet Cloud Restore · 双机验收附件\n".as_bytes(), "Attachment bytes mismatch");
    ensure!(crate::docxml::serialize(&store.load(&xml.id)?).contains(asset), "Missing attachment link in the restored note");
    ensure!(store.list_trash().is_empty(), "The restored trash should be empty");
    println!("RESTORE_OK source={source} folder=1 notes=2 attachment=1 searchable=true metadata=true order=true");
    Ok(())
}
