// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! A share owns one isolated SQLite library. No personal catalogue, peer
//! history, settings, or unrelated attachments enter its CloudKit zone.

use anyhow::{Context, Result, ensure};
use gpui::AppContext;
use serde::{Deserialize, Serialize};
use std::{ffi::{CStr, CString, c_char}, fs, path::{Path, PathBuf}, sync::OnceLock};
use crate::{model::Document, store::Store};
use super::{Journal, assets, content};

pub(crate) mod original;

static ACTIVE: OnceLock<Scope> = OnceLock::new();
#[cfg(feature = "devtools")]
pub(crate) mod session_qa;
#[cfg(feature = "devtools")]
mod ui_qa;

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
            && self.owner.as_ref().is_none_or(|owner| !owner.is_empty() && owner.len() <= 256), "Invalid shared note");
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

pub(crate) fn active() -> Option<&'static Scope> { ACTIVE.get() }

fn account(root: &Path) -> Result<String> {
    let marker = fs::read_to_string(root.join(".cloud-restore-ready"))?;
    let key = marker.lines().next().context("Missing library account")?;
    ensure!(key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit()), "Invalid library account");
    Ok(key.to_owned())
}

pub(super) fn descriptor(root: &Path) -> Result<Option<Room>> {
    let room = match fs::read(root.join(MARKER)) {
        Ok(bytes) => serde_json::from_slice::<Room>(&bytes)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    room.scope.validate()?;
    Ok(Some(room))
}

pub(crate) fn room_root(personal: &Path, note: &str) -> Result<PathBuf> {
    ensure!(uuid::Uuid::parse_str(note)?.to_string() == note, "Invalid shared note identifier");
    let parent = personal.join("shared-notes");
    let root = parent.join(note);
    for path in [&parent, &root] {
        ensure!(!fs::symlink_metadata(path).is_ok_and(|meta| meta.is_symlink()), "Invalid shared note directory");
    }
    Ok(root)
}

pub(crate) fn startup(personal: &Path, note: &str) -> Result<PathBuf> {
    let root = room_root(personal, note)?;
    let room = descriptor(&root)?.context("Shared note has not been accepted on this Mac")?;
    ensure!(room.scope.note == note && account(personal)? == account(&root)?, "Shared note belongs to another library");
    ACTIVE.set(room.scope).map_err(|_| anyhow::anyhow!("A shared note is already open"))?;
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
            && account(&root)? == account(personal)?, "Conflicting shared note");
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
    #[cfg(feature = "devtools")]
    #[serde(rename = "accessChannel")]
    access_channel: Option<String>,
}

impl<'a> Command<'a> {
    fn new(account: &'a str, action: &'a str, scope: Option<&'a Scope>) -> Result<Self> {
        Ok(Self { account, action, scope, title: None, email: None, permission: None, participant: None, url: None,
            #[cfg(feature = "devtools")]
            access_channel: crate::cloud_trial::test_channel()? })
    }

    fn send(&self) -> Result<Reply> {
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
        ensure!(reply.error_kind.as_deref() != Some("subscriptionRequired"), "{}", crate::cloud_trial::access_required());
        ensure!(reply.error.is_none(), "{}", reply.error.as_deref().unwrap_or_default());
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

#[derive(Serialize, Deserialize)]
pub(crate) struct Reply {
    scope: Option<Scope>,
    title: Option<String>,
    pub url: Option<String>,
    pub members: Vec<Member>,
    error: Option<String>,
    #[serde(rename = "errorKind")]
    error_kind: Option<String>,
}

#[cfg(feature = "devtools")]
pub(crate) fn create(personal: &Path, title: String, doc: &Document, front: &str) -> Result<Room> {
    let room = prepare_copy(personal, title, doc, front)?;
    ensure_created(personal, &room)?;
    Ok(room)
}

#[cfg(any(test, feature = "devtools"))]
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
    let mut command = Command::new(&key, "create", Some(&room.scope))?;
    command.title = Some(&room.title);
    command.send()?;
    let mut journal = Journal::open(&root, None)?;
    let start = std::time::Instant::now();
    while journal.counts()?.0 > 0 {
        ensure!(start.elapsed() < std::time::Duration::from_secs(240), "Initial shared note upload timed out");
        let page = super::exchange(journal.request()?)?;
        let quota_exceeded = page.upload_quota_exceeded;
        journal.receive(page)?;
        ensure!(journal.access() == Some(Access::Owner), "Shared note is no longer available");
        journal.merge(&Default::default())?;
        ensure!(!quota_exceeded, "{}", crate::settings::tr("iCloud 空间不足。释放空间后再试。", "iCloud is full. Free up space and try again."));
    }
    fs::write(root.join("seed-published"), b"1")?;
    Ok(())
}

pub(crate) fn accept(personal: &Path, url: &str) -> Result<Room> {
    let key = account(personal)?;
    let mut command = Command::new(&key, "accept", None)?;
    let url = crate::sharing::links::cloud(url)?;
    command.url = Some(&url);
    let reply = command.send()?;
    let scope = reply.scope.context("Missing accepted shared note")?;
    let root = room_root(personal, &scope.note)?;
    if root.join("shared-note-ended").exists() {
        let _lock = crate::cloud_restore::lock_library(&root)?;
        let retired = personal.join("retired-shared-notes"); fs::create_dir_all(&retired)?;
        fs::rename(&root, retired.join(format!("{}-{}", scope.note, uuid::Uuid::new_v4())))?;
    }
    let room = descriptor(&root)?.unwrap_or_else(|| Room { scope: scope.clone(), title: reply.title.unwrap_or_default() });
    ensure!(room.scope == scope, "Conflicting shared note");
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

pub(crate) fn prepare_end(personal: &Path, room: &Room, cx: &mut gpui::App) -> Result<Option<std::fs::File>> {
    if !original::is_original(personal, room)? { return Ok(None); }
    let root = room_root(personal, &room.scope.note)?;
    if super::session(&root, cx).is_some() { super::release(&root, cx); }
    Ok(Some(crate::cloud_restore::lock_library(&root)?))
}

pub(crate) fn end(personal: &Path, room: &Room, lock: Option<std::fs::File>) -> Result<()> {
    let root = room_root(personal, &room.scope.note)?;
    ensure!(descriptor(&root)?.is_some_and(|saved| saved.scope == room.scope) && account(&root)? == account(personal)?, "Shared note belongs to another account");
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
    if let Some(saved) = descriptor(personal)? {
        ensure!(saved.scope == room.scope && saved.scope.owner.is_none(), "Only the owner can manage this invitation.");
    } else { ensure_created(personal, room)?; }
    let key = account(personal)?;
    let mut command = Command::new(&key, action, Some(&room.scope))?;
    command.permission = access;
    if action == "invite" { command.email = Some(person.trim()); }
    else { command.participant = Some(person); }
    command.send()
}

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
        "Shared note belongs to another library");
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
        bar.title = Some(format!("{} · {}", crate::app_name(), crate::settings::tr("共享笔记", "Shared Note")).into());
    }
    let note = room.scope.note.clone();
    cx.open_window(options, |window, cx| {
        #[cfg(feature = "devtools")]
        crate::devtools::park_window(window, cx);
        cx.new(|cx| crate::app::Workspace::new_shared_window(store, note, window, cx))
    })?;
    Ok(())
}

pub(crate) fn store_for(room: &Room, cx: &mut gpui::App) -> Result<crate::shared_store::SharedStore> {
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
        "Shared note belongs to another library");
    ensure!(!original::awaiting_publication(&personal, room), "{}", crate::settings::tr("请从原位置打开并完成共享邀请。", "Open the original item to finish sharing."));
    let store = if let Some(session) = super::session(&root, cx) { session.store.clone() }
        else {
            let (store, journal) = open_library(root)?;
            super::install_session(store.clone(), journal, cx)?;
            store
        };
    Ok(store)
}

fn open_library(root: PathBuf) -> Result<(crate::shared_store::SharedStore, Journal)> {
    // Claim the room before Store can migrate SQLite or recover pending content.
    let lock = crate::cloud_restore::lock_library(&root)?;
    let store = crate::shared_store::SharedStore::open_shared(root.clone())?;
    let journal = Journal::open_locked(&root, None, lock)?;
    Ok((store, journal))
}

pub(crate) fn scope<'a>(store: &crate::shared_store::SharedStore, cx: &'a gpui::App) -> Option<&'a Scope> { super::scope(store.root(), cx) }

pub(crate) fn access(store: &crate::shared_store::SharedStore, cx: &gpui::App) -> Option<Access> {
    super::session(store.root(), cx).and_then(|session| session.journal.access())
}

pub(crate) fn editing_paused(store: &crate::shared_store::SharedStore, cx: &gpui::App) -> bool {
    store.is_shared() && super::status(store.root(), cx).is_some_and(|status| status.access_paused
        || matches!(status.phase, super::SyncPhase::AccountChecking | super::SyncPhase::AccountChanged | super::SyncPhase::SignedOut))
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
            uploaded: vec![], edits, token: Some(Bytes(vec![1])), more: false, legacy: false,
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
        guest.receive(Changes { uploaded: vec![], edits: request.edits, token: Some(Bytes(vec![1])), more: false, legacy: false,
            room_access: Some(Access::ReadOnly), #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false })?;
        guest.merge(&Default::default())?;
        let mut store = Store::open_sqlite_at(guest_root.clone())?;
        assert_eq!(store.list().iter().map(|entry| entry.id.as_str()).collect::<Vec<_>>(), [room.scope.note.as_str()]);
        assert!(store.load(&room.scope.note)?.plain_text().contains("共享正文"));
        assert_eq!(fs::read(guest_root.join("assets/files/shared-fixture.txt"))?, b"shared attachment fixture");

        let db = rusqlite::Connection::open(guest_root.join(crate::store::SQLITE_FILE))?;
        super::super::journal::enqueue(&db, &room.scope.note, vec![1])?;
        assert!(guest.request()?.edits.is_empty());
        guest.receive(Changes { uploaded: vec![], edits: vec![], token: Some(Bytes(vec![2])), more: false, legacy: false,
            room_access: Some(Access::ReadWrite), #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false })?;
        assert_eq!(guest.request()?.edits.len(), 1);
        guest.receive(Changes { uploaded: vec![], edits: vec![], token: None, more: false, legacy: false,
            room_access: Some(Access::Revoked), #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false })?;
        assert!(guest.request()?.edits.is_empty());
        assert_eq!(guest.counts()?.0, 1);
        guest.merge(&Default::default())?;
        assert!(store.list().is_empty());
        Ok(())
    }
}
