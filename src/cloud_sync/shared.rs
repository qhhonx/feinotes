// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! A share owns one isolated SQLite library. No personal catalogue, peer
//! history, settings, or unrelated attachments enter its CloudKit zone.

use anyhow::{Context, Result, ensure};
use gpui::AppContext;
use serde::{Deserialize, Serialize};
use std::{ffi::{CStr, CString, c_char}, fs, path::{Path, PathBuf}, sync::Mutex};
use crate::{model::Document, store::Store};
use super::{Journal, assets, content};

pub(crate) mod original;

static ACTIVE: Mutex<Option<Scope>> = Mutex::new(None);
#[cfg(all(feature = "devtools", target_os = "macos"))]
pub(crate) mod session_qa;
#[cfg(feature = "devtools")]
mod ui_qa;

const LINK_PENDING: &str = "public-link-pending";

pub(super) const MARKER: &str = "shared-note.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Scope {
    pub note: String,
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub folder: bool,
}

impl Scope {
    fn validate(&self) -> Result<()> {
        ensure!(uuid::Uuid::parse_str(&self.note)?.to_string() == self.note
            && self.owner.as_ref().is_none_or(|owner| !owner.is_empty() && owner.len() <= 256), "Invalid shared document");
        Ok(())
    }

    pub fn permits(&self, scope: &str) -> bool { scope == self.note || scope == assets::SCOPE || self.folder && uuid::Uuid::parse_str(scope).is_ok_and(|id| id.to_string() == scope) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Access { Owner, ReadWrite, ReadOnly, Revoked }

impl Access {
    pub fn can_write(self) -> bool { matches!(self, Self::Owner | Self::ReadWrite) }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Room {
    pub scope: Scope,
    pub title: String,
}

pub(crate) fn active() -> Option<Scope> { ACTIVE.lock().expect("shared session lock").clone() }

#[cfg(target_os = "ios")]
pub(crate) fn select_session(root: &Path) -> Result<()> {
    let scope = descriptor(root)?.map(|room| room.scope);
    *ACTIVE.lock().expect("shared session lock") = scope;
    Ok(())
}

fn account(root: &Path) -> Result<String> {
    let marker = fs::read_to_string(root.join(".cloud-restore-ready"))?;
    let key = marker.lines().next().context("Missing library account")?;
    ensure!(key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit()), "Invalid library account");
    Ok(key.to_owned())
}

pub(crate) fn descriptor(root: &Path) -> Result<Option<Room>> {
    let room = match fs::read(root.join(MARKER)) {
        Ok(bytes) => serde_json::from_slice::<Room>(&bytes)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    room.scope.validate()?;
    Ok(Some(room))
}

// Staging writes this marker once, before a share can appear in the list.
pub(super) fn registered_at(root: &Path) -> u64 {
    fs::metadata(root.join(MARKER)).and_then(|meta| meta.modified()).ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |time| time.as_secs())
}

pub(crate) fn room_root(personal: &Path, note: &str) -> Result<PathBuf> {
    ensure!(uuid::Uuid::parse_str(note)?.to_string() == note, "Invalid shared document identifier");
    let parent = personal.join("shared-notes");
    let root = parent.join(note);
    for path in [&parent, &root] {
        ensure!(!fs::symlink_metadata(path).is_ok_and(|meta| meta.is_symlink()), "Invalid shared document directory");
    }
    Ok(root)
}

pub(crate) fn startup(personal: &Path, note: &str) -> Result<PathBuf> {
    let root = session_root(personal,note)?;
    let scope = descriptor(&root)?.context("Missing shared document")?.scope;
    let mut active = ACTIVE.lock().expect("shared session lock");
    ensure!(active.is_none(), "A shared document is already open");
    *active = Some(scope);
    Ok(root)
}

pub(crate) fn session_root(personal: &Path, note: &str) -> Result<PathBuf> {
    let root = room_root(personal, note)?;
    let room = descriptor(&root)?.context("Shared document has not been accepted on this device")?;
    ensure!(room.scope.note == note && account(personal)? == account(&root)?, "Shared document belongs to another library");
    Ok(root)
}

fn stage(personal: &Path, room: &Room, seed: Option<(&Document, &str)>) -> Result<PathBuf> {
    room.scope.validate()?;
    stage_with(personal, room, |staged| {
        if let Some((doc, front)) = seed {
            copy_assets(personal, staged, doc)?;
            Journal::open(staged, None)?.seed_shared(doc, front)?;
        }
        Ok(())
    })
}

fn stage_with(personal: &Path, room: &Room, seed: impl FnOnce(&Path) -> Result<()>) -> Result<PathBuf> {
    room.scope.validate()?;
    let root = room_root(personal, &room.scope.note)?;
    if root.exists() {
        ensure!(descriptor(&root)?.is_some_and(|saved| saved.scope == room.scope)
            && account(&root)? == account(personal)?, "Conflicting shared document");
        return Ok(root);
    }
    let parent = root.parent().unwrap();
    fs::create_dir_all(parent)?;
    let staged = parent.join(format!(".stage-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&staged)?;
    let result = (|| {
        let account = account(personal)?;
        Store::initialize_shared_sqlite(&staged, &account)?;
        fs::write(staged.join(MARKER), serde_json::to_vec(room)?)?;
        seed(&staged)?;
        fs::rename(&staged, &root)?;
        Ok(root)
    })();
    if result.is_err() { let _ = fs::remove_dir_all(&staged); }
    result
}

fn copy_assets(personal: &Path, staged: &Path, doc: &Document) -> Result<()> {
    for source in content::local_assets(doc) {
        let bytes = crate::store::read_bounded(&assets::path(personal, &source)?, assets::LIMIT as u64)
            .with_context(|| format!("Could not copy attachment {source}"))?;
        let destination = assets::path(staged, &source)?;
        fs::create_dir_all(destination.parent().context("Missing attachment folder")?)?;
        if destination.exists() {
            ensure!(crate::store::read_bounded(&destination, assets::LIMIT as u64)? == bytes, "Conflicting attachment {source}");
        } else {
            let temp = destination.with_file_name(format!(".asset-{}", uuid::Uuid::new_v4()));
            fs::write(&temp, bytes)?;
            let linked = fs::hard_link(&temp, &destination);
            fs::remove_file(&temp)?;
            if let Err(error) = linked {
                if error.kind() != std::io::ErrorKind::AlreadyExists { return Err(error.into()); }
                ensure!(fs::read(&destination)? == fs::read(assets::path(personal, &source)?)?, "Conflicting attachment {source}");
            }
        }
    }
    Ok(())
}

unsafe extern "C" { fn quiet_cloud_share(json: *const c_char) -> *mut c_char; }

#[derive(Clone, Serialize)]
struct ShareTrace {
    sample: &'static str,
    attempt: String,
}

impl ShareTrace {
    fn for_room(room: &Room) -> Option<Self> {
        if room.scope.folder || room.scope.owner.is_some() { return None; }
        let token = std::env::var("FEI_SHARE_TRACE_TOKEN").ok()?;
        let sample = Self::sample(&room.title, &token)?;
        Some(Self { sample, attempt: uuid::Uuid::new_v4().to_string() })
    }

    fn sample(title: &str, token: &str) -> Option<&'static str> {
        if uuid::Uuid::parse_str(token).ok()?.to_string() != token { return None; }
        match title.strip_prefix("Fei share trace ")?.strip_prefix(token)? {
            " public" => Some("publicLink"),
            " private" => Some("privateInvite"),
            _ => None,
        }
    }
}

#[derive(Serialize)]
struct Command<'a> {
    account: &'a str,
    action: &'a str,
    scope: Option<&'a Scope>,
    title: Option<&'a str>,
    email: Option<&'a str>,
    permission: Option<Access>,
    participant: Option<&'a str>,
    url: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trace: Option<ShareTrace>,
    #[serde(rename = "knownOwned", skip_serializing_if = "Vec::is_empty")]
    known_owned: Vec<String>,
    #[cfg(feature = "devtools")]
    #[serde(rename = "accessChannel")]
    access_channel: Option<String>,
}

impl<'a> Command<'a> {
    fn new(account: &'a str, action: &'a str, scope: Option<&'a Scope>) -> Result<Self> {
        Ok(Self { account, action, scope, title: None, email: None, permission: None, participant: None, url: None, trace: None, known_owned: vec![],
            #[cfg(feature = "devtools")]
            access_channel: crate::cloud_trial::test_channel()? })
    }

    fn receive(&self) -> Result<Reply> {
        #[cfg(feature = "devtools")]
        if let Some(result) = ui_qa::command(self) { return result; }
        let revision = super::account_revision();
        let json = CString::new(serde_json::to_vec(self)?)?;
        let ptr = unsafe { quiet_cloud_share(json.as_ptr()) };
        ensure!(!ptr.is_null(), "CloudKit sharing returned no response");
        let reply = unsafe { serde_json::from_slice::<Reply>(CStr::from_ptr(ptr).to_bytes()) };
        unsafe { super::quiet_cloud_restore_free(ptr) };
        let reply = reply?;
        super::check_account_revision(revision)?;
        Ok(reply)
    }

    fn send(&self) -> Result<Reply> {
        let reply = self.receive()?;
        use crate::settings::tr;
        if let Some(error) = reply.error.as_deref() {
            match reply.error_kind.as_deref() {
                Some("linkRegionRestricted") => return Err(LinkWriteError::Region.into()),
                Some("linkWriteRejected") => return Err(LinkWriteError::Rejected.into()),
                Some("linkWriteUnconfirmed") => return Err(LinkWriteError::Unconfirmed.into()),
                Some("shareTimeout") if self.action == "publishLink" => return Err(LinkWriteError::Unconfirmed.into()),
                _ => {},
            }
            let message = match reply.error_kind.as_deref() {
                Some("subscriptionRequired") => crate::cloud_trial::access_required(),
                Some("unavailableLink") => tr("此链接无效或已关闭。", "This share link is invalid or has been closed."),
                Some("deniedLink") => tr("此账户无法使用该链接。链接可能已关闭，或邀请发给了其他 Apple 账户。", "This account cannot use this link. It may have been closed or sent to a different Apple Account."),
                Some("signedOut") => tr("请在系统设置中登录 iCloud，再重新连接飞记。", "Sign in to iCloud in System Settings, then reconnect Fei Notes."),
                Some("accountMismatch") => tr("当前 iCloud 账户与文档库不一致。请在文档库设置中重新连接。", "The iCloud account does not match this library. Reconnect in Library settings."),
                Some("shareConflict") => tr("其他设备修改了共享权限。请刷新后重试。", "Sharing permissions changed on another device. Refresh and try again."),
                Some("invitationOnly") => tr("这篇文档已使用指定账户邀请。现有权限无法切换成公开链接。", "This document uses account invitations. Its existing access cannot be changed to a public link."),
                Some("linkOnly") => tr("这篇文档使用链接权限，无法逐人邀请或改权。", "This document uses link access. Individual invitations and permissions are unavailable."),
                _ => error,
            };
            anyhow::bail!("{message}");
        }
        Ok(reply)
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Member {
    pub id: String,
    pub name: String,
    pub permission: Access,
    pub accepted: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Audience { #[default] Invited, LinkDraft, LinkReadOnly, LinkReadWrite, LinkClosed }

impl Audience {
    pub fn link_active(self) -> bool { matches!(self, Self::LinkReadOnly | Self::LinkReadWrite) }
    pub fn can_invite(self) -> bool { matches!(self, Self::Invited | Self::LinkDraft) }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Reply {
    scope: Option<Scope>,
    title: Option<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub audience: Audience,
    pub members: Vec<Member>,
    error: Option<String>,
    #[serde(rename = "errorKind")]
    error_kind: Option<String>,
    #[serde(rename = "ownedRooms")]
    owned_rooms: Option<Vec<Room>>,
}

#[cfg(feature = "devtools")]
pub(crate) fn create(personal: &Path, title: String, doc: &Document, front: &str) -> Result<Room> {
    let room = prepare_copy(personal, title, doc, front)?;
    ensure_created(personal, &room)?;
    Ok(room)
}

#[cfg(feature = "devtools")]
pub(crate) fn prepare_copy(personal: &Path, title: String, doc: &Document, front: &str) -> Result<Room> {
    Command::new(&account(personal)?, "checkAccess", None)?.send()?;
    let room = Room { scope: Scope { note: uuid::Uuid::new_v4().to_string(), owner: None, folder: false }, title };
    stage(personal, &room, Some((doc, front)))?;
    Ok(room)
}

pub(crate) fn ensure_created(personal: &Path, room: &Room) -> Result<()> {
    #[cfg(feature = "devtools")]
    if let Some(result) = ui_qa::publish(personal, room) { return result; }
    let root = room_root(personal, &room.scope.note)?;
    if root.join("seed-published").is_file() { return Ok(()); }
    let key = account(personal)?;
    let action = if public_link_pending(personal, room) { "createLinkDraft" } else { "create" };
    let mut command = Command::new(&key, action, Some(&room.scope))?;
    command.title = Some(&room.title);
    command.trace = ShareTrace::for_room(room);
    command.send()?;
    let mut journal = Journal::open(&root, None)?;
    let start = std::time::Instant::now();
    while journal.counts()?.0 > 0 {
        ensure!(start.elapsed() < std::time::Duration::from_secs(240), "Initial shared document upload timed out");
        let page = super::exchange(journal.request()?)?;
        let quota_exceeded = page.upload_quota_exceeded;
        journal.receive(page)?;
        ensure!(journal.access() == Some(Access::Owner), "Shared document is no longer available");
        journal.merge(&Default::default())?;
        ensure!(!quota_exceeded, "{}", crate::settings::tr("iCloud 空间不足。释放空间后再试。", "iCloud is full. Free up space and try again."));
    }
    fs::write(root.join("seed-published"), b"1")?;
    Ok(())
}

pub(crate) fn public_link_pending(personal: &Path, room: &Room) -> bool {
    room.scope.owner.is_none() && !room.scope.folder && room_root(personal, &room.scope.note)
        .is_ok_and(|root| root.join(LINK_PENDING).is_file() && !root.join("shared-note-ended").exists())
}

#[derive(Debug)]
enum LinkWriteError { Region, Rejected, Unconfirmed }

impl std::fmt::Display for LinkWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::settings::tr;
        f.write_str(match self {
            Self::Region => tr("iCloud 报告地区限制，未能启用公开链接。", "iCloud reported a regional restriction and could not enable the public link."),
            Self::Rejected => tr("iCloud 拒绝启用公开链接。", "iCloud rejected the public link change."),
            Self::Unconfirmed => tr("暂时无法确认公开链接是否已启用。请刷新共享权限后再试。", "Could not confirm whether the public link is enabled. Refresh access and try again."),
        })
    }
}

impl std::error::Error for LinkWriteError {}

pub(crate) struct LinkPublication {
    pub reply: Option<Reply>,
    pub result: Result<Option<String>>,
}

// Public link creation is paused in product builds while the CloudKit rejection is investigated.
pub(crate) fn public_link_creation_enabled() -> bool {
    #[cfg(feature = "devtools")]
    { ui_qa::enabled() }
    #[cfg(not(feature = "devtools"))]
    { false }
}

// Only the initiating device can retry its staged public link after uploading the document.
pub(crate) fn publish_link(personal: &Path, room: &Room) -> LinkPublication {
    if !public_link_creation_enabled() {
        return LinkPublication { reply: None, result: Err(anyhow::anyhow!("{}", crate::settings::tr(
            "暂时无法创建公开链接。你仍可邀请指定账户。", "Public link creation is paused. You can still invite specific accounts."))) };
    }
    let revision = super::account_revision();
    let trace = ShareTrace::for_room(room);
    let outcome = (|| {
        ensure!(public_link_pending(personal, room) && room_root(personal, &room.scope.note)?.join("seed-published").is_file(),
            "{}", crate::settings::tr("请先完成文档上传，再启用公开链接。", "Finish uploading the document before enabling its public link."));
        manage_traced(personal, room, "publishLink", "", None, trace.clone())
    })();
    let reply = match outcome {
        Ok(reply) => reply,
        Err(error) => {
            // Fetch independently of the failed write, including after a bridge timeout.
            // A definite rejection never causes automatic copying, even if another device enabled it.
            let observed = super::check_account_revision(revision)
                .and_then(|()| manage_traced(personal, room, "members", "", None, trace)).ok();
            match observed {
                Some(reply) if matches!(error.downcast_ref::<LinkWriteError>(), Some(LinkWriteError::Unconfirmed))
                    && matches!(reply.audience, Audience::LinkReadOnly | Audience::LinkClosed) => reply,
                reply => return LinkPublication { reply, result: Err(error) },
            }
        }
    };
    let result = match reply.audience {
        Audience::LinkReadOnly => reply.url.as_deref().context(LinkWriteError::Unconfirmed)
            .and_then(crate::sharing::links::branded).map(Some),
        Audience::LinkClosed => Ok(None),
        _ => Err(LinkWriteError::Unconfirmed.into()),
    };
    let result = super::check_account_revision(revision).and(result);
    if result.is_ok() && let Ok(root) = room_root(personal, &room.scope.note) { clear_pending_link(&root); }
    LinkPublication { reply: Some(reply), result }
}

fn clear_pending_link(root: &Path) {
    if let Err(error) = fs::remove_file(root.join(LINK_PENDING))
        && error.kind() != std::io::ErrorKind::NotFound {
        crate::logging::write("Could not clear the local pending public link flag");
    }
}

pub(crate) fn accept(personal: &Path, url: &str) -> Result<Room> {
    let key = account(personal)?;
    let mut command = Command::new(&key, "accept", None)?;
    let url = crate::sharing::links::cloud(url)?;
    command.url = Some(&url);
    let reply = command.send()?;
    let scope = reply.scope.context("Missing accepted shared document")?;
    let root = room_root(personal, &scope.note)?;
    if root.join("shared-note-ended").exists() {
        let _lock = crate::cloud_restore::lock_library(&root)?;
        let retired = personal.join("retired-shared-notes"); fs::create_dir_all(&retired)?;
        fs::rename(&root, retired.join(format!("{}-{}", scope.note, uuid::Uuid::new_v4())))?;
    }
    let room = descriptor(&root)?.unwrap_or_else(|| Room { scope: scope.clone(), title: reply.title.unwrap_or_default() });
    ensure!(room.scope == scope, "Conflicting shared document");
    stage(personal, &room, None)?;
    Ok(room)
}

pub(crate) fn rooms(personal: &Path) -> Result<Vec<Room>> {
    let dir = personal.join("shared-notes");
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };
    let mut rooms = vec![];
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        if !entry.file_type()?.is_dir() || uuid::Uuid::parse_str(&name.to_string_lossy()).is_err() { continue; }
        if !entry.path().join("shared-note-ended").exists() && let Some(room) = descriptor(&entry.path())? { rooms.push(room); }
    }
    rooms.sort_by(|a, b| a.title.cmp(&b.title).then_with(|| a.scope.note.cmp(&b.scope.note)));
    Ok(rooms)
}

pub(super) fn discover_owned(personal: &Path, key: &str) -> Result<()> {
    let mut command = Command::new(key, "discoverOwned", None)?;
    command.known_owned = rooms(personal)?.into_iter().filter(|room| room.scope.owner.is_none())
        .map(|room| room.scope.note).collect();
    let revision = super::account_revision();
    let found = command.send()?.owned_rooms.context("Missing owned shares")?;
    for room in found {
        super::check_account_revision(revision)?;
        stage_discovered_owner(personal, &room)?;
    }
    Ok(())
}

fn stage_discovered_owner(personal: &Path, room: &Room) -> Result<()> {
    ensure!(room.scope.owner.is_none(), "Invalid discovered share owner");
    let root = room_root(personal, &room.scope.note)?;
    // An ended share or an unpublished local copy is managed by its original workflow.
    if root.exists() { return Ok(()); }
    stage_with(personal, room, |staged| {
        fs::write(staged.join("seed-published"), b"Confirmed owned CloudKit share")?;
        Ok(())
    })?;
    Ok(())
}
pub(crate) fn prepare_end(personal: &Path, room: &Room, cx: &mut gpui::App) -> Result<Option<std::fs::File>> {
    if !original::is_original(personal, room)? { return Ok(None); }
    let root = room_root(personal, &room.scope.note)?;
    if super::session(&root, cx).is_some() { super::release(&root, cx); }
    Ok(Some(crate::cloud_restore::lock_library(&root)?))
}

pub(crate) fn end(personal: &Path, room: &Room, lock: Option<std::fs::File>) -> Result<()> {
    let root = room_root(personal, &room.scope.note)?;
    ensure!(descriptor(&root)?.is_some_and(|saved| saved.scope == room.scope) && account(&root)? == account(personal)?, "Shared document belongs to another account");
    if let Some(lock) = lock {
        let mut journal = Journal::open_locked(&root, None, lock)?;
        #[cfg(feature = "devtools")]
        let simulated = ui_qa::enabled();
        #[cfg(not(feature = "devtools"))]
        let simulated = false;
        if !simulated {
            journal.merge(&Default::default())?;
            let start = std::time::Instant::now();
            loop {
                ensure!(start.elapsed().as_secs() < 240, "Timed out syncing the shared folder before stopping");
                let page = super::exchange(journal.request()?)?;
                ensure!(page.room_access == Some(Access::Owner) && !page.upload_quota_exceeded, "Unable to finish syncing before stopping sharing");
                let more = page.more;
                journal.receive(page)?;
                journal.merge(&Default::default())?;
                let (uploads, pending, held, warnings) = journal.counts()?;
                ensure!(warnings == 0 && held == 0, "Resolve attachment errors before stopping sharing");
                if !more && uploads == 0 {
                    ensure!(pending == 0, "Some shared content has not downloaded yet");
                    break;
                }
            }
        }
        original::restore(personal, room, || {
            Command::new(&account(personal)?, "delete", Some(&room.scope))?.send()?;
            Ok(())
        })?;
        fs::write(root.join("shared-note-ended"), b"Stopped sharing original")?;
        return Ok(());
    }
    let mut cache = Store::open_sqlite_at(root.clone())?;
    if !room.scope.folder && cache.list().iter().any(|entry| entry.id == room.scope.note) {
        Store::archive_note_to_trash(personal, &root, &room.scope.note)?;
    }
    let key = account(personal)?;
    Command::new(&key, if room.scope.owner.is_none() { "delete" } else { "leave" }, Some(&room.scope))?.send()?;
    fs::write(root.join("shared-note-ended"), b"Access ended; local cache retained for recovery")?;
    Ok(())
}

pub(crate) fn manage(personal: &Path, room: &Room, action: &str, person: &str, access: Option<Access>) -> Result<Reply> {
    manage_traced(personal, room, action, person, access, ShareTrace::for_room(room))
}

fn manage_traced(personal: &Path, room: &Room, action: &str, person: &str, access: Option<Access>, trace: Option<ShareTrace>) -> Result<Reply> {
    let open_room = descriptor(personal)?;
    if let Some(saved) = &open_room {
        ensure!(saved.scope == room.scope && saved.scope.owner.is_none(), "Only the owner can manage this invitation.");
    }
    let key = account(personal)?;
    let mut command = Command::new(&key, action, Some(&room.scope))?;
    command.permission = access;
    command.trace = trace;
    if action == "invite" { command.email = Some(person.trim()); }
    else { command.participant = Some(person); }
    let reply = command.send()?;
    if action == "invite" && reply.audience == Audience::Invited {
        if open_room.is_some() { clear_pending_link(personal); }
        else if let Ok(root) = room_root(personal, &room.scope.note) { clear_pending_link(&root); }
    }
    Ok(reply)
}

#[cfg(target_os = "ios")]
pub(crate) fn launch(room: &Room, cx: &mut gpui::App) -> gpui::Task<Result<()>> {
    let room = room.clone();
    cx.defer(move |cx| {
        if let Some(handle) = cx.windows().into_iter().find_map(|w| w.downcast::<crate::app::Workspace>()) {
            let _ = handle.update(cx,|w,window,cx| w.open_shared_note(&room,window,cx));
        }
    });
    gpui::Task::ready(Ok(()))
}

#[cfg(target_os = "macos")]
pub(crate) fn open_window(room: &Room, cx: &mut gpui::App) -> Result<()> {
    room.scope.validate()?;
    let personal = Store::root_dir();
    ensure!(!crate::cloud_restore::local_only() && !personal.join("require-account-verification").exists(),
        "{}", crate::settings::tr("请先在文档库设置中重新连接 iCloud。", "Reconnect to iCloud in Documents settings first."));
    ensure!(super::status(&personal, cx).is_some_and(|s| !s.access_paused && !matches!(s.phase,
        super::SyncPhase::AccountChecking | super::SyncPhase::AccountChanged | super::SyncPhase::SignedOut)),
        "{}", crate::settings::tr("请先确认 iCloud 账户和云功能状态。", "Check your iCloud account and cloud access first."));
    let root = room_root(&personal, &room.scope.note)?;
    if !root.exists() && original::is_original(&personal, room)? {
        stage(&personal, room, None)?;
        fs::write(root.join("seed-published"), b"Owner catalogue reference")?;
    }
    ensure!(!root.join("shared-note-ended").exists(), "This shared document has been removed");
    ensure!(descriptor(&root)?.is_some_and(|saved| saved.scope == room.scope) && account(&root)? == account(&personal)?,
        "Shared document belongs to another library");
    for window in cx.windows().into_iter().filter_map(|w| w.downcast::<crate::app::Workspace>()) {
        if window.read(cx).is_ok_and(|ws| ws.store.root() == root) {
            window.update(cx, |_, window, cx| {
                if !crate::devtools::scripted() { window.activate_window(); cx.activate(true); }
            })?;
            return Ok(());
        }
    }
    let store = store_for(room, cx)?;
    let mut options = crate::main_window_options(crate::devtools::scripted(), None, cx);
    if let Some(bar) = &mut options.titlebar {
        bar.title = Some(format!("{} · {}", crate::app_name(), crate::settings::tr("共享文档", "Shared document")).into());
    }
    let note = room.scope.note.clone();
    cx.open_window(options, |window, cx| {
        #[cfg(feature = "devtools")]
        crate::devtools::park_window(window, cx);
        cx.new(|cx| crate::app::Workspace::new_shared_window(store, note, window, cx))
    })?;
    Ok(())
}

fn check_room_account(personal: &Path, cx: &gpui::App) -> Result<()> {
    ensure!(!crate::cloud_restore::local_only() && !personal.join("require-account-verification").exists(),
        "{}", crate::settings::tr("请先在文档库设置中重新连接 iCloud。", "Reconnect to iCloud in Documents settings first."));
    ensure!(super::status(personal, cx).is_some_and(|s| !s.access_paused && !matches!(s.phase,
        super::SyncPhase::AccountChecking | super::SyncPhase::AccountChanged | super::SyncPhase::SignedOut)),
        "{}", crate::settings::tr("请先确认 iCloud 账户和云功能状态。", "Check your iCloud account and cloud access first."));
    Ok(())
}

fn check_room(personal: &Path, root: &Path, room: &Room) -> Result<()> {
    ensure!(!root.join("shared-note-ended").exists(), "This shared document has been removed");
    ensure!(descriptor(root)?.is_some_and(|saved| saved.scope == room.scope) && account(root)? == account(personal)?,
        "Shared document belongs to another library");
    ensure!(!original::awaiting_publication(personal, room), "{}", crate::settings::tr("请从原位置打开并完成共享邀请。", "Open the original item to finish sharing."));
    Ok(())
}

fn prepare_room(personal: &Path, root: &Path, room: &Room) -> Result<()> {
    if !root.exists() && original::is_original(personal, room)? {
        stage(personal, room, None)?;
        fs::write(root.join("seed-published"), b"Owner catalogue reference")?;
    }
    check_room(personal, root, room)
}

pub(crate) fn check_opened_store(store: crate::shared_store::SharedStore, room: &Room, cx: &gpui::App) -> Result<crate::shared_store::SharedStore> {
    let personal = Store::root_dir();
    check_room_account(&personal, cx)?;
    ensure!(store.root() == room_root(&personal, &room.scope.note)?, "The personal library changed while opening the shared document");
    check_room(&personal, store.root(), room)?;
    ensure!(scope(&store, cx) == Some(&room.scope) && access(&store, cx) != Some(Access::Revoked), "Shared access has ended or changed");
    Ok(store)
}

pub(crate) fn store_for(room: &Room, cx: &mut gpui::App) -> Result<crate::shared_store::SharedStore> {
    room.scope.validate()?;
    let personal = Store::root_dir();
    check_room_account(&personal, cx)?;
    let root = room_root(&personal, &room.scope.note)?;
    prepare_room(&personal, &root, room)?;
    let store = if let Some(session) = super::session(&root, cx) { session.store.clone() }
        else {
            let (store, journal) = open_library(root)?;
            let store = crate::shared_store::SharedStore::from_shared_store(store);
            super::install_session(store.clone(), journal, cx)?;
            store
        };
    Ok(store)
}

fn open_library(root: PathBuf) -> Result<(Store, Journal)> {
    // Claim the room before Store can migrate SQLite or recover pending content.
    let lock = crate::cloud_restore::lock_library(&root)?;
    let store = Store::open_sqlite_at(root.clone())?;
    let journal = Journal::open_locked(&root, None, lock)?;
    Ok((store, journal))
}

#[cfg(target_os = "macos")]
type SidebarWaiter = Box<dyn FnOnce(Result<crate::shared_store::SharedStore, String>, &mut gpui::App)>;

#[cfg(target_os = "macos")]
#[derive(Default)]
struct SidebarLoads(std::collections::HashMap<PathBuf, Vec<SidebarWaiter>>);

#[cfg(target_os = "macos")]
impl gpui::Global for SidebarLoads {}

#[cfg(target_os = "macos")]
pub(crate) fn load_sidebar_store(room: Room, cx: &mut gpui::App, ready: SidebarWaiter) -> Result<()> {
    let personal = Store::root_dir();
    check_room_account(&personal, cx)?;
    room.scope.validate()?;
    let root = room_root(&personal, &room.scope.note)?;
    if let Some(session) = super::session(&root, cx) {
        check_room(&personal, &root, &room)?;
        let store = session.store.clone();
        cx.defer(move |cx| ready(Ok(store), cx));
        return Ok(());
    }
    if !cx.has_global::<SidebarLoads>() { cx.set_global(SidebarLoads::default()); }
    let loads = &mut cx.global_mut::<SidebarLoads>().0;
    if let Some(waiters) = loads.get_mut(&root) { waiters.push(ready); return Ok(()); }
    loads.insert(root.clone(), vec![ready]);
    let revision = super::account_revision();
    let library_generation = super::session(&personal, cx).map(|session| session.generation);
    cx.spawn(async move |cx| {
        let source = personal.clone(); let path = root.clone(); let requested = room.clone();
        let opened = cx.background_executor().spawn(async move {
            prepare_room(&source, &path, &requested)?;
            open_library(path)
        }).await;
        let _ = crate::gpui_compat::async_update(cx, |cx| {
            let waiters = cx.global_mut::<SidebarLoads>().0.remove(&root).unwrap_or_default();
            let store = (|| -> Result<_> {
                ensure!(Store::root_dir() == personal && super::session(&personal, cx).map(|session| session.generation) == library_generation,
                    "The personal library changed while opening the shared document");
                super::check_account_revision(revision)?;
                check_room_account(&personal, cx)?;
                check_room(&personal, &root, &room)?;
                if let Some(session) = super::session(&root, cx) {
                    return check_opened_store(session.store.clone(), &room, cx);
                }
                let (store, journal) = opened?;
                let store = crate::shared_store::SharedStore::from_shared_store(store);
                super::install_session(store.clone(), journal, cx)?;
                Ok(store)
            })().map_err(|error| format!("{error:#}"));
            for waiter in waiters { waiter(store.clone(), cx); }
            cx.defer(super::release_unused);
        });
    }).detach();
    Ok(())
}

pub(crate) fn scope<'a>(store: &crate::shared_store::SharedStore, cx: &'a gpui::App) -> Option<&'a Scope> { super::scope(store.root(), cx) }

pub(crate) fn access(store: &crate::shared_store::SharedStore, cx: &gpui::App) -> Option<Access> {
    super::session(store.root(), cx).and_then(|session| session.journal.access())
}

pub(crate) fn editing_paused(store: &crate::shared_store::SharedStore, cx: &gpui::App) -> bool {
    store.is_shared() && super::status(store.root(), cx).is_some_and(|status| status.access_paused
        || matches!(status.phase, super::SyncPhase::AccountChecking | super::SyncPhase::AccountChanged | super::SyncPhase::SignedOut))
}

#[cfg(target_os = "ios")]
pub(crate) fn can_edit_in(root: &Path, cx: &gpui::App) -> bool {
    let Some(session) = super::session(root,cx) else { return root == Store::root_dir() };
    !session.store.is_shared() || (!editing_paused(&session.store,cx) && access(&session.store,cx).is_some_and(Access::can_write))
}

#[cfg(any(feature = "devtools", test))]
fn sample(personal: &Path) -> Result<Document> {
    use crate::model::{Block, BlockKind, FileBlock, FileView, RichText};
    let asset = "assets/files/shared-fixture.txt";
    fs::create_dir_all(personal.join("assets/files"))?;
    fs::write(personal.join(asset), b"shared attachment fixture")?;
    fs::write(personal.join("assets/files/private-fixture.txt"), b"unrelated attachment fixture")?;
    Ok(Document { title: RichText::plain("共享合成笔记"),
        blocks: vec![Block::text("共享正文"), Block::new(BlockKind::File(FileBlock {
            src_kind: "path".into(), src: asset.into(), name: "shared-fixture.txt".into(), view: FileView::Card,
        }), RichText::default())], ..Default::default() })
}

#[cfg(feature = "devtools")]
pub(super) fn live_owner_smoke(personal: &Path) -> Result<()> {
    ensure!(super::test_root()?.is_some(), "Use an isolated sharing QA root");
    let doc = sample(personal)?;
    let room = create(personal, doc.title.text.clone(), &doc, "")?;
    let reply = manage(personal, &room, "members", "", None)?;
    ensure!(reply.url.is_some() && reply.members.is_empty(), "Expected an uninvited private share");
    let room_root = room_root(personal, &room.scope.note)?;
    ensure!(room_root.join("assets/files/shared-fixture.txt").is_file()
        && !room_root.join("assets/files/private-fixture.txt").exists(), "Unexpected shared files");
    let journal = Journal::open(&room_root, None)?;
    ensure!(journal.counts()?.0 == 0 && journal.access() == Some(Access::Owner), "Expected an uploaded owner note");
    // Exercise the same metadata/acceptance entry point using the owner.
    let reopened = accept(personal, reply.url.as_deref().unwrap())?;
    ensure!(reopened.scope == room.scope, "Owner link did not reopen the same room");
    fs::write(personal.join("qa-share-link"), reply.url.unwrap())?;
    println!("SHARE_OWNER_OK room={} attachments=1 participants=0", room.scope.note);
    Ok(())
}

#[cfg(feature = "devtools")]
pub(super) fn live_accept_smoke(personal: &Path, expect_read_only: bool) -> Result<()> {
    ensure!(super::test_root()?.is_some(), "Use an isolated sharing QA root");
    let link = if expect_read_only {
        let mut link = String::new();
        std::io::stdin().read_line(&mut link)?;
        link
    } else { fs::read_to_string(personal.join("qa-share-link"))? };
    let room = accept(personal, link.trim())?;
    ensure!(!expect_read_only || room.scope.owner.is_some(), "当前 Apple 账户是笔记所有者，请切到登录第二个 Apple 账户的 macOS 测试用户");
    let root = room_root(personal, &room.scope.note)?;
    let mut journal = Journal::open(&root, None)?;
    let start = std::time::Instant::now();
    loop {
        ensure!(start.elapsed() < std::time::Duration::from_secs(180), "Shared note download timed out");
        let page = super::exchange(journal.request()?)?;
        let more = page.more;
        journal.receive(page)?;
        ensure!(journal.access() != Some(Access::Revoked), "Shared note is no longer available");
        journal.merge(&Default::default())?;
        if !more && journal.counts()?.1 == 0 { break; }
    }
    let mut store = Store::open_sqlite_at(root.clone())?;
    ensure!(store.list().iter().map(|entry| entry.id.as_str()).eq([room.scope.note.as_str()]), "Unexpected shared catalogue");
    ensure!(store.load(&room.scope.note)?.plain_text().contains("共享正文"), "Shared text was not restored");
    ensure!(fs::read(root.join("assets/files/shared-fixture.txt"))? == b"shared attachment fixture"
        && !root.join("assets/files/private-fixture.txt").exists(), "Unexpected restored attachments");
    let access = journal.access().context("Missing share permission")?;
    ensure!(!expect_read_only || access == Access::ReadOnly, "受邀账户没有只读权限");
    println!("SHARE_RESTORED role={access:?} note=1 attachments=1");
    if expect_read_only { println!("SHARE_ROOM_ID {}", room.scope.note); }
    Ok(())
}

#[cfg(feature = "devtools")]
pub(super) fn live_member_change(personal: &Path, operation: &str) -> Result<()> {
    ensure!(super::test_root()?.is_some(), "Use an isolated sharing QA root");
    let link = fs::read_to_string(personal.join("qa-share-link"))?;
    let room = accept(personal, &link)?;
    ensure!(room.scope.owner.is_none() && room.title == "共享合成笔记", "Only manage the synthetic owner room");
    let reply = match operation {
        "share-members" => manage(personal, &room, "members", "", None)?,
        "share-permit-write" | "share-revoke" => {
            let reply = manage(personal, &room, "members", "", None)?;
            let [member] = reply.members.as_slice() else { anyhow::bail!("Expected one synthetic test participant") };
            ensure!(member.accepted, "The test participant has not accepted the invitation");
            if operation == "share-revoke" { manage(personal, &room, "revoke", &member.id, None)? }
            else { manage(personal, &room, "permission", &member.id, Some(Access::ReadWrite))? }
        }
        _ => {
            let email = fs::read_to_string(personal.join("qa-share-email"))?;
            let permission = if operation == "share-invite-read" { Access::ReadOnly } else { Access::ReadWrite };
            manage(personal, &room, "invite", &email, Some(permission))?
        }
    };
    println!("SHARE_MEMBERS count={}", reply.members.len());
    for member in reply.members { println!("SHARE_MEMBER accepted={} permission={:?}", member.accepted, member.permission); }
    Ok(())
}

#[cfg(feature = "devtools")]
pub(crate) fn live_room_smoke(operation: &str) -> Result<()> {
    use crate::editor::core::{Core, Pos, Selection};
    ensure!(super::test_root()?.is_some(), "Use an isolated sharing QA root");
    let room = active().context("Expected a shared note process")?;
    let root = Store::root_dir();
    let mut journal = Journal::open(&root, None)?;
    let mut store = crate::shared_store::SharedStore::open()?;
    match operation {
        "read-only" | "edit-a" | "edit-b" | "revoke-edit" => {
            let doc = store.load(&room.note)?;
            ensure!(doc.title.text == "共享合成笔记", "Expected the synthetic note");
            let mut core = Core::new(doc);
            let first = &core.doc.blocks[0];
            let end = if matches!(operation, "edit-b" | "revoke-edit") { first.text.len() } else { 0 };
            core.sel = Selection::caret(Pos::new(first.id, end));
            core.insert_text(match operation { "edit-b" => "客端乙", "revoke-edit" => "撤销前待传", _ => "所有者甲" });
            let result = journal.save(&mut store, &room.note, &core);
            if operation == "read-only" {
                ensure!(journal.access() == Some(Access::ReadOnly) && result.is_err() && journal.counts()?.0 == 0,
                    "A read-only collaborator queued an edit");
            } else {
                result?;
                ensure!(journal.counts()?.0 > 0, "The shared edit did not reach the durable queue");
            }
        }
        "sync" | "revoked" => {
            let queued = journal.counts()?.0;
            let start = std::time::Instant::now();
            loop {
                ensure!(start.elapsed() < std::time::Duration::from_secs(180), "Shared edit replay timed out");
                let page = super::exchange(journal.request()?)?;
                let more = page.more;
                journal.receive(page)?;
                journal.merge(&Default::default())?;
                if journal.access() == Some(Access::Revoked) || (!more && journal.counts()?.0 == 0 && journal.counts()?.1 == 0) {
                    break;
                }
            }
            if operation == "revoked" {
                ensure!(journal.access() == Some(Access::Revoked) && queued > 0 && journal.counts()?.0 == queued
                    && store.list().is_empty(), "Revocation did not block queued uploads and hide the note");
            } else { ensure!(journal.access().is_some_and(Access::can_write), "The shared note is not editable"); }
        }
        "verify" => {
            let text = store.load(&room.note)?.plain_text();
            ensure!(text.matches("所有者甲").count() == 1 && text.matches("客端乙").count() == 1 && !text.contains("撤销前待传"),
                "The two offline edits did not converge");
        }
        _ => anyhow::bail!("Unknown shared note QA command"),
    }
    println!("SHARE_ROOM_OK operation={operation}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud_sync::{Bytes, Changes};

    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }

    #[test]
    fn share_tracing_selects_only_the_two_exact_session_samples() {
        let token = "01234567-89ab-4def-8123-456789abcdef";
        let prefix = format!("Fei share trace {token}");
        assert_eq!(ShareTrace::sample(&format!("{prefix} public"), token), Some("publicLink"));
        assert_eq!(ShareTrace::sample(&format!("{prefix} private"), token), Some("privateInvite"));
        assert!(ShareTrace::sample(&format!("{prefix} public notes"), token).is_none());
        assert!(ShareTrace::sample("Fei share trace wrong public", token).is_none());
        assert!(ShareTrace::sample("Fei share trace wrong public", "wrong").is_none());
    }

    #[test]
    fn discovered_owner_creates_a_published_cache_without_seeding_or_reopening_ended_shares() -> Result<()> {
        let temp = Fixture(std::env::temp_dir().join(format!("quiet-share-discovery-{}", uuid::Uuid::new_v4())));
        fs::create_dir_all(&temp.0)?;
        fs::write(temp.0.join(".cloud-restore-ready"), format!("{}\n", "a".repeat(64)))?;
        let room = Room { scope: Scope { note: uuid::Uuid::new_v4().to_string(), owner: None, folder: false }, title: "Owned share from another device".into() };
        stage_discovered_owner(&temp.0, &room)?;
        let root = session_root(&temp.0, &room.scope.note)?;
        assert!(root.join("seed-published").is_file());
        assert_eq!(rooms(&temp.0)?, vec![room.clone()]);
        {
            let journal = Journal::open(&root, None)?;
            assert_eq!(journal.counts()?.0, 0);
        }
        fs::write(root.join("shared-note-ended"), b"ended")?;
        stage_discovered_owner(&temp.0, &room)?;
        assert!(rooms(&temp.0)?.is_empty());
        assert!(root.join("shared-note-ended").exists());
        Ok(())
    }

    #[test]
    fn unpublished_link_does_not_gate_uploaded_document_on_either_owner_device() -> Result<()> {
        let temp = Fixture(std::env::temp_dir().join(format!("fei-link-owner-{}", uuid::Uuid::new_v4())));
        let first = temp.0.join("first");
        let second = temp.0.join("second");
        for library in [&first, &second] {
            Store::initialize_local_sqlite(library)?;
            fs::write(library.join(".cloud-restore-ready"), format!("{}\n", "a".repeat(64)))?;
        }
        let doc = crate::docxml::parse("<title>Same original</title><p>Synthetic shared body</p>")?;
        let id = Store::open_sqlite_at(first.clone())?.create_from(&doc)?;
        let room = original::prepare_local(&first, &id, doc.title.text.clone(), true)?;
        let first_root = room_root(&first, &id)?;
        assert!(original::awaiting_publication(&first, &room) && public_link_pending(&first, &room));
        let owner = Journal::open(&first_root, None)?;
        let edits = owner.request()?.edits;
        fs::write(first_root.join("seed-published"), b"synthetic upload confirmed")?;
        stage_discovered_owner(&second, &room)?;
        let second_root = room_root(&second, &id)?;
        let mut mirror = Journal::open(&second_root, None)?;
        mirror.receive(Changes { retention: None, uploaded: vec![], edits,
            token: Some(Bytes(vec![1])), more: false, legacy: false,
            room_access: Some(Access::Owner), #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false })?;
        mirror.merge(&Default::default())?;
        for library in [&first, &second] {
            assert!(!original::awaiting_publication(library, &room));
            assert_eq!(rooms(library)?, vec![room.clone()]);
        }
        assert!(public_link_pending(&first, &room) && !public_link_pending(&second, &room));
        let store = Store::open_sqlite_at(second_root)?;
        assert_eq!(store.load(&id)?.plain_text(), doc.plain_text());
        Ok(())
    }

    #[test]
    fn shared_list_first_receive_preserves_dates_and_records_open_separately() -> Result<()> {
        use super::super::activity;
        use crate::model::{Block, RichText};
        use std::time::{Duration, UNIX_EPOCH};

        let temp = Fixture(std::env::temp_dir().join(format!("quiet-share-first-receive-{}", uuid::Uuid::new_v4())));
        let personal = temp.0.join("guest");
        let sender = temp.0.join("sender");
        for (path, key) in [(&personal, "a"), (&sender, "b")] {
            fs::create_dir_all(path)?;
            fs::write(path.join(".cloud-restore-ready"), format!("{}\n", key.repeat(64)))?;
        }
        let doc = Document { title: RichText::plain("Delayed shared fixture"),
            blocks: vec![Block::text("Synthetic searchable body")], ..Default::default() };
        let owner = Room { scope: Scope { note: uuid::Uuid::new_v4().to_string(), owner: None, folder: false }, title: doc.title.text.clone() };
        let owner_root = stage(&sender, &owner, Some((&doc, "")))?;
        let source = Journal::open(&owner_root, None)?;
        let guest = Room { scope: Scope { owner: Some("synthetic-sender".into()), ..owner.scope }, title: owner.title };
        let guest_root = stage(&personal, &guest, None)?;
        fs::OpenOptions::new().write(true).open(guest_root.join(MARKER))?.set_modified(UNIX_EPOCH + Duration::from_secs(100))?;
        let other = Room { scope: Scope { note: uuid::Uuid::new_v4().to_string(), ..guest.scope.clone() }, title: "Later shared fixture".into() };
        let other_root = stage(&personal, &other, None)?;
        fs::OpenOptions::new().write(true).open(other_root.join(MARKER))?.set_modified(UNIX_EPOCH + Duration::from_secs(200))?;

        let before = activity::row(&personal, guest.clone())?.context("Missing title-only row")?;
        let later = activity::row(&personal, other)?.context("Missing other row")?;
        assert_eq!(before.room.title, doc.title.text);
        assert!(before.body.is_empty());
        assert_eq!((before.created, before.modified, before.opened), (100, 100, 0));
        assert!(before.created < later.created && before.modified < later.modified);

        // The transport can withhold the first payload across arbitrary list reads.
        let mut journal = Journal::open(&guest_root, None)?;
        journal.merge(&Default::default())?;
        assert!(activity::row(&personal, guest.clone())? == Some(before.clone()));
        journal.receive(Changes { retention: None, uploaded: vec![], edits: source.request()?.edits,
            token: Some(Bytes(vec![1])), more: false, legacy: false, room_access: Some(Access::ReadOnly),
            #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false })?;
        journal.merge(&Default::default())?;
        assert_eq!(journal.access(), Some(Access::ReadOnly));
        let received = activity::row(&personal, guest.clone())?.context("Missing received row")?;
        assert!(received.body.contains("Synthetic searchable body"));
        assert_eq!((received.created, received.modified, received.opened), (before.created, before.modified, 0));

        let mut store = Store::open_sqlite_at(guest_root)?;
        assert_eq!(store.load(&guest.scope.note)?.title.text, doc.title.text);
        let touched = store.touch(&guest.scope.note).context("Missing opened document")?;
        let opened = activity::row(&personal, guest)?.context("Missing opened row")?;
        assert_eq!((opened.created, opened.modified), (before.created, before.modified));
        assert_eq!(opened.opened, touched.opened);
        assert!(opened.opened > later.modified);
        Ok(())
    }

    #[test]
    fn room_selection_requires_its_library_account_and_descriptor() -> Result<()> {
        let temp = Fixture(std::env::temp_dir().join(format!("quiet-share-selection-{}",uuid::Uuid::new_v4())));
        fs::create_dir_all(&temp.0)?;
        fs::write(temp.0.join(".cloud-restore-ready"),format!("{}\n","a".repeat(64)))?;
        let room = Room { scope: Scope { note: uuid::Uuid::new_v4().to_string(),owner: Some("synthetic-owner".into()), folder: false },title: "Selected note".into() };
        let root = stage(&temp.0,&room,None)?;
        assert_eq!(session_root(&temp.0,&room.scope.note)?,root);
        fs::write(root.join(".cloud-restore-ready"),format!("{}\nshared\n","b".repeat(64)))?;
        assert!(session_root(&temp.0,&room.scope.note).is_err());
        fs::write(root.join(".cloud-restore-ready"),format!("{}\nshared\n","a".repeat(64)))?;
        let other = Room { scope: Scope { note: uuid::Uuid::new_v4().to_string(),..room.scope.clone() },..room.clone() };
        fs::write(root.join(MARKER),serde_json::to_vec(&other)?)?;
        assert!(session_root(&temp.0,&room.scope.note).is_err());
        assert!(session_root(&temp.0,"../personal").is_err());
        Ok(())
    }

    #[test]
    #[ignore = "Creates an explicit synthetic library for the iOS session UI check"]
    fn prepare_mobile_sharing_fixture() -> Result<()> {
        let output = PathBuf::from(std::env::var("FEI_NOTES_SHARE_FIXTURE_ROOT")?);
        ensure!(output.is_absolute() && (output.starts_with(std::env::temp_dir()) || output.starts_with("/private/tmp")) && !output.exists(), "Use a new temporary fixture directory");
        fs::create_dir(&output)?;
        let personal = output.join("personal");
        let source = personal.join(".restore-source");
        fs::create_dir_all(&source)?;
        fs::write(source.join(".cloud-restore-ready"),format!("{}\n","a".repeat(64)))?;
        let mut store = Store::open_at(source)?;
        let personal_note = store.create_from(&Document { title: crate::model::RichText::plain("FEI_PERSONAL_SESSION"),..Default::default() })?;
        drop(store);
        Store::initialize_sqlite(&personal)?;
        let mut personal_journal = Journal::open(&personal,Some("44ecb219-6dec-4b51-ae62-bcd575b86813".into()))?;
        personal_journal.merge(&Default::default())?;
        drop(personal_journal);

        let owner = output.join("owner");
        fs::create_dir(&owner)?;
        fs::write(owner.join(".cloud-restore-ready"),format!("{}\n","a".repeat(64)))?;
        let mut doc = sample(&owner)?;
        doc.title = crate::model::RichText::plain("FEI_SHARED_READONLY");
        let room = Room { scope: Scope { note: uuid::Uuid::new_v4().to_string(),owner: None, folder: false },title: doc.title.text.clone() };
        let owner_root = stage(&owner,&room,Some((&doc,"")))?;
        let owner_journal = Journal::open(&owner_root,None)?;
        let guest = Room { scope: Scope { owner: Some("synthetic-owner".into()),..room.scope.clone() },..room.clone() };
        let guest_root = stage(&personal,&guest,None)?;
        let mut guest_journal = Journal::open(&guest_root,None)?;
        guest_journal.receive(Changes { retention: None, uploaded: vec![],edits: owner_journal.request()?.edits,token: Some(Bytes(vec![1])),more: false,legacy: false,
            room_access: Some(Access::ReadOnly),#[cfg(feature = "devtools")] cursor_replayed: false,upload_quota_exceeded: false })?;
        guest_journal.merge(&Default::default())?;
        drop(guest_journal);
        doc.title = crate::model::RichText::plain("FEI_SHARED_WRITABLE");
        let writable = Room { scope: Scope { note: uuid::Uuid::new_v4().to_string(),owner: None, folder: false },title: doc.title.text.clone() };
        // Copy only the shared attachment into this room, using the same product staging path.
        let personal_asset = assets::path(&personal,"assets/files/shared-fixture.txt")?;
        fs::create_dir_all(personal_asset.parent().unwrap())?;
        fs::write(personal_asset,b"shared attachment fixture")?;
        let writable_root = stage(&personal,&writable,Some((&doc,"")))?;
        fs::write(writable_root.join("seed-published"),b"1")?;
        fs::write(output.join("fixture.json"),serde_json::to_vec(&serde_json::json!({ "personal_note": personal_note,"readonly": guest,"writable": writable }))?)?;
        Ok(())
    }

    #[test]
    fn shared_library_claim_precedes_opening_its_database() -> Result<()> {
        let temp = Fixture(std::env::temp_dir().join(format!("quiet-share-lock-{}", uuid::Uuid::new_v4())));
        fs::create_dir_all(&temp.0)?;
        fs::write(temp.0.join(crate::store::SQLITE_FILE), b"database must not be opened while another process owns the room")?;
        let _owner = crate::cloud_restore::lock_library(&temp.0)?;
        let error = match open_library(temp.0.clone()) { Ok(_) => anyhow::bail!("Opened an owned room"), Err(error) => error };
        ensure!(error.to_string().contains("请先退出正在使用该文档库"), "The database was opened before the room lock was checked: {error:#}");
        ensure!(fs::read(temp.0.join(crate::store::SQLITE_FILE))? == b"database must not be opened while another process owns the room");
        Ok(())
    }

    #[test]
    fn shared_activity_requires_the_collaborator_version_to_be_displayed() -> Result<()> {
        use crate::cloud_sync::{Edit, activity, content};
        let temp = Fixture(std::env::temp_dir().join(format!("quiet-share-activity-{}", uuid::Uuid::new_v4())));
        let personal = temp.0.join("personal");
        let guest = temp.0.join("guest");
        fs::create_dir_all(&personal)?;
        fs::create_dir_all(&guest)?;
        fs::write(personal.join(".cloud-restore-ready"), format!("{}\n", "a".repeat(64)))?;
        fs::write(guest.join(".cloud-restore-ready"), format!("{}\n", "b".repeat(64)))?;
        let doc = sample(&personal)?;
        let id = uuid::Uuid::new_v4().to_string();
        let room = Room { scope: Scope { note: id.clone(), owner: None, folder: false }, title: doc.title.text.clone() };
        let owner_root = stage(&personal, &room, Some((&doc, "")))?;
        let owner = Journal::open(&owner_root, None)?;
        let guest_room = Room { scope: Scope { owner: Some("synthetic-owner".into()), ..room.scope }, title: room.title };
        let guest_root = stage(&guest, &guest_room, None)?;
        let mut guest = Journal::open(&guest_root, None)?;
        let db = rusqlite::Connection::open(guest_root.join(crate::store::SQLITE_FILE))?;
        let receive = |journal: &mut Journal, edits| journal.receive(Changes {
            retention: None, uploaded: vec![], edits, token: Some(Bytes(vec![1])), more: false, legacy: false,
            room_access: Some(Access::ReadOnly), #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false,
        });
        let base = owner.request()?.edits.into_iter().map(|edit| Edit { collaborator: true, ..edit }).collect();
        receive(&mut guest, base)?;
        guest.merge(&Default::default())?;
        let store = Store::open_sqlite_at(guest_root)?;
        let loro = owner.text_state(&id)?.context("Missing initial shared note")?;
        let mut current = store.load(&id)?;
        let edit = |title, record: &str, collaborator, current: &mut Document| -> Result<Edit> {
            current.title = crate::model::RichText::plain(title);
            let update = content::edit(&loro, 123, current)?.context("Missing shared edit")?;
            Ok(Edit { id: record.into(), note: id.clone(), data: Bytes(update), collaborator })
        };

        let same_account = edit("另一台 Mac", "self", false, &mut current)?;
        receive(&mut guest, vec![same_account])?;
        guest.merge(&Default::default())?;
        let cached = store.load(&id)?;
        assert!(guest.displayable_updates(&cached)?.is_empty());
        assert!(guest.displayable_updates(&current)?.is_empty());

        let first = edit("协作者的第一次编辑", "remote-1", true, &mut current)?;
        activity::observe(&db, &id, std::slice::from_ref(&first))?;
        assert!(guest.displayable_updates(&cached)?.is_empty());
        receive(&mut guest, vec![first.clone()])?;
        guest.merge(&Default::default())?;
        assert!(guest.displayable_updates(&cached)?.is_empty());
        let rendered = store.load(&id)?;
        assert_eq!(rendered.title.text, "协作者的第一次编辑");
        let displayed = guest.displayable_updates(&rendered)?;
        assert_eq!(displayed, ["remote-1"]);

        let second = edit("协作者的第二次编辑", "remote-2", true, &mut current)?;
        receive(&mut guest, vec![second.clone()])?;
        guest.merge(&Default::default())?;
        guest.mark_updates_read(&displayed)?;
        activity::observe(&db, &id, &[first])?;
        assert_eq!(guest.displayable_updates(&store.load(&id)?)?, ["remote-2"]);
        guest.mark_updates_read(&["remote-2".into()])?;
        assert!(guest.displayable_updates(&store.load(&id)?)?.is_empty());

        // An upgraded library may already contain the body before the
        // sidebar's independent cursor discovers the collaborator record.
        db.execute("DELETE FROM shared_activity WHERE id = 'remote-2'", [])?;
        activity::observe(&db, &id, &[second])?;
        assert_eq!(guest.displayable_updates(&store.load(&id)?)?, ["remote-2"]);
        Ok(())
    }

    #[test]
    fn share_projects_one_note_and_pauses_upload_after_access_is_removed() -> Result<()> {
        let temp = Fixture(std::env::temp_dir().join(format!("quiet-share-test-{}", uuid::Uuid::new_v4())));
        let personal = temp.0.join("personal");
        let guest = temp.0.join("guest");
        fs::create_dir_all(&personal)?;
        fs::create_dir_all(&guest)?;
        fs::write(personal.join(".cloud-restore-ready"), format!("{}\n", "a".repeat(64)))?;
        fs::write(guest.join(".cloud-restore-ready"), format!("{}\n", "b".repeat(64)))?;
        let doc = sample(&personal)?;
        let room = Room { scope: Scope { note: uuid::Uuid::new_v4().to_string(), owner: None, folder: false }, title: doc.title.text.clone() };
        let owner_root = stage(&personal, &room, Some((&doc, "")))?;
        let owner = Journal::open(&owner_root, None)?;
        let request = owner.request()?;
        assert_eq!(request.room, Some(room.scope.clone()));
        assert_eq!(request.edits.len(), 2);
        assert!(request.edits.iter().all(|edit| room.scope.permits(&edit.note)));
        assert!(!owner_root.join("assets/files/private-fixture.txt").exists());
        assert!(owner.legacy_request().is_err());

        let guest_room = Room { scope: Scope { owner: Some("synthetic-owner".into()), ..room.scope.clone() }, title: room.title };
        let guest_root = stage(&guest, &guest_room, None)?;
        let mut guest = Journal::open(&guest_root, None)?;
        guest.receive(Changes { retention: None, uploaded: vec![], edits: request.edits, token: Some(Bytes(vec![1])), more: false, legacy: false,
            room_access: Some(Access::ReadOnly), #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false })?;
        guest.merge(&Default::default())?;
        let mut store = Store::open_sqlite_at(guest_root.clone())?;
        assert_eq!(store.list().iter().map(|entry| entry.id.as_str()).collect::<Vec<_>>(), [room.scope.note.as_str()]);
        assert!(store.load(&room.scope.note)?.plain_text().contains("共享正文"));
        assert_eq!(fs::read(guest_root.join("assets/files/shared-fixture.txt"))?, b"shared attachment fixture");

        let db = rusqlite::Connection::open(guest_root.join(crate::store::SQLITE_FILE))?;
        super::super::journal::enqueue(&db, &room.scope.note, vec![1])?;
        assert!(guest.request()?.edits.is_empty());
        guest.receive(Changes { retention: None, uploaded: vec![], edits: vec![], token: Some(Bytes(vec![2])), more: false, legacy: false,
            room_access: Some(Access::ReadWrite), #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false })?;
        assert_eq!(guest.request()?.edits.len(), 1);
        guest.receive(Changes { retention: None, uploaded: vec![], edits: vec![], token: None, more: false, legacy: false,
            room_access: Some(Access::Revoked), #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false })?;
        assert!(guest.request()?.edits.is_empty());
        assert_eq!(guest.counts()?.0, 1);
        guest.merge(&Default::default())?;
        assert!(store.list().is_empty());
        Ok(())
    }
}
