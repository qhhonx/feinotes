// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Opt-in offline invitation transport for fresh, synthetic native UI regressions.
use super::*;

fn root() -> Option<PathBuf> {
    #[cfg(all(target_os = "ios", target_abi = "sim", feature = "mobile-qa"))]
    if crate::ipad_session::offline_invitation_fixture() {
        let personal = Store::root_dir();
        let parent = personal.parent()?;
        return parent.join("qa-session-rooms.json").is_file().then(|| parent.to_owned());
    }
    #[cfg(feature = "devtools")]
    {
        std::env::var_os("QUIET_SYNC_QA_SHARING_UI")?;
        assert!(crate::devtools::scripted(), "Sharing UI QA requires the script driver");
        let root = super::super::test_root().expect("isolated QA root").expect("isolated QA root");
        assert!(root.join("qa-session-rooms.json").is_file() && root.join("pause-cloud").is_file(), "Use fresh synthetic session fixtures");
        Some(root)
    }
    #[cfg(not(feature = "devtools"))]
    None
}

pub(super) fn enabled() -> bool { root().is_some() }

fn reply() -> Reply { Reply { scope: None, title: None, url: None, audience: Audience::Invited, draft_permission: None, members: vec![], error: None, error_kind: None, discovered_rooms: None, verified_access: None, owner_name: None, owned_zone_exists: None } }

pub(super) fn command(command: &Command<'_>) -> Option<Result<Reply>> {
    let root = root()?;
    Some((|| {
        if command.action == "checkAccess" { return Ok(reply()); }
        if command.action == "accept" {
            ensure!(matches!(command.url, Some("https://www.icloud.com/share/synthetic-fei-ui"
                | "https://www.icloud.com/share/synthetic-fei-ui?source=qa#Document")), "Invalid synthetic invitation link");
            let rooms: Vec<Room> = serde_json::from_slice(&fs::read(root.join("qa-session-rooms.json"))?)?;
            let room = rooms.get(1).context("Missing synthetic invitation room")?;
            let mut result = reply(); result.scope = Some(room.scope.clone()); result.title = Some(room.title.clone());
            return Ok(result);
        }
        let scope = command.scope.context("Missing synthetic scope")?;
        if command.action == "inspectOwnedZone" {
            let mut result = reply();
            result.scope = Some(scope.clone());
            result.owned_zone_exists = Some(!root.join(format!("qa-deleted-{}", scope.note)).is_file());
            return Ok(result);
        }
        if command.action == "checkRoomAccess" || command.action == "inspectRoomAccess" {
            if command.action == "checkRoomAccess" && fs::remove_file(root.join("qa-delay-room-access-once")).is_ok() {
                std::thread::sleep(std::time::Duration::from_millis(1800));
            }
            if command.action == "inspectRoomAccess" && fs::remove_file(root.join("fail-access-read-once")).is_ok() {
                anyhow::bail!("Synthetic permission check temporarily unavailable");
            }
            let denied = root.join("qa-room-access-denied").is_file();
            ensure!(!denied || command.action == "inspectRoomAccess", "Synthetic shared permission denied");
            let mut result = reply();
            result.scope = Some(scope.clone());
            result.verified_access = Some(if denied { Access::Revoked } else if scope.owner.is_some() { Access::ReadOnly } else { Access::Owner });
            return Ok(result);
        }
        if command.action == "delete" || command.action == "leave" {
            ensure!((command.action == "delete") == scope.owner.is_none(), "Synthetic removal role mismatch");
            if root.join("fail-end-once").exists() { fs::remove_file(root.join("fail-end-once"))?; anyhow::bail!("Synthetic removal failed. Recovery copy retained."); }
            if command.action == "delete" {
                #[cfg(all(target_os = "ios",feature = "mobile-qa"))]
                if std::env::var_os("FEI_QA_STOP_SHARE_DELAY").is_some() { std::thread::sleep(std::time::Duration::from_secs(4)); }
                fs::write(root.join(format!("qa-deleted-{}", scope.note)), b"1")?;
                if fs::remove_file(root.join("fail-end-reply-once")).is_ok() {
                    let mut result = reply();
                    result.error = Some("Synthetic stop response lost after deletion".into());
                    result.error_kind = Some("stopUnconfirmed".into());
                    return Ok(result);
                }
            }
            return Ok(reply());
        }
        ensure!(scope.owner.is_none(), "Only synthetic owners manage invitations");
        let path = root.join(format!("qa-members-{}.json", scope.note));
        let mut result = if path.is_file() { serde_json::from_slice(&fs::read(&path)?)? } else { reply() };
        if !path.is_file() && let Some(permission) = pending_public_access(&root.join("a".repeat(64)),
            &Room { scope: scope.clone(), title: String::new() }) {
            result.audience = Audience::LinkDraft;
            result.draft_permission = Some(permission);
        }
        match command.action {
            "members" => {
                #[cfg(all(target_os = "ios", feature = "mobile-qa"))]
                if std::env::var_os("FEI_QA_SLOW_SHARE_MEMBERS").is_some() {
                    std::thread::sleep(std::time::Duration::from_millis(2600));
                }
            },
            "publishLink" => {
                if fs::remove_file(root.join("fail-link-conflict-once")).is_ok() {
                    result.error = Some("Conflicting share".into());
                    result.error_kind = Some("shareConflict".into());
                    return Ok(result);
                }
                if root.join("fail-link-write-once").is_file() {
                    fs::remove_file(root.join("fail-link-write-once"))?;
                    #[cfg(all(target_os = "ios", feature = "mobile-qa"))]
                    if std::env::var_os("FEI_QA_SHARE_REMOTE_DRAFT").is_some() {
                        // Keep the CloudKit-equivalent draft but remove origin-only state.
                        fs::write(&path, serde_json::to_vec(&result)?)?;
                        clear_pending_link(&room_root(&root.join("a".repeat(64)), &scope.note)?);
                    }
                    result.error = Some("Cannot perform action due to region".into());
                    result.error_kind = Some("linkRegionRestricted".into());
                    return Ok(result);
                }
                ensure!(result.audience.link_active() || result.audience == Audience::LinkClosed
                    || result.audience == Audience::LinkDraft && result.members.is_empty(), "Cannot reopen a private invitation as a public link");
                if result.audience == Audience::LinkDraft {
                    let permission = command.permission.unwrap_or(Access::ReadOnly);
                    ensure!(result.draft_permission.is_none_or(|saved| saved == permission), "Draft permission changed");
                    result.draft_permission = None;
                    result.audience = match permission {
                        Access::ReadOnly => Audience::LinkReadOnly, Access::ReadWrite => Audience::LinkReadWrite,
                        _ => anyhow::bail!("Invalid public link permission"),
                    };
                }
            }
            "closeLink" => {
                ensure!(result.audience.link_active() || result.audience == Audience::LinkClosed, "No public link");
                result.audience = Audience::LinkClosed;
                result.members.clear();
            }
            "invite" => {
                ensure!(result.audience.can_invite(), "Individual invitations require private access");
                if fs::remove_file(root.join("delay-invite-once")).is_ok() {
                    std::thread::sleep(std::time::Duration::from_millis(1100));
                }
                if root.join("fail-invite-once").is_file() { fs::remove_file(root.join("fail-invite-once"))?; anyhow::bail!("Synthetic invitation failed. Retry this shared note."); }
                result.audience = Audience::Invited;
                result.draft_permission = None;
                result.members.push(Member { id: "synthetic-participant".into(), name: "".into(), permission: command.permission.context("Missing permission")?, accepted: false });
            }
            "permission" => result.members.iter_mut().find(|member| Some(member.id.as_str()) == command.participant)
                .context("Missing synthetic participant")?.permission = command.permission.context("Missing permission")?,
            "revoke" => result.members.retain(|member| Some(member.id.as_str()) != command.participant),
            _ => anyhow::bail!("Unexpected synthetic share command"),
        }
        result.url = (result.audience == Audience::Invited || result.audience.link_active())
            .then(|| "https://www.icloud.com/share/synthetic-fei-ui".into());
        #[cfg(all(target_os = "ios", feature = "mobile-qa"))]
        if result.audience.link_active() && std::env::var_os("FEI_QA_SHARE_URL_FRAGMENT").is_some() {
            result.url = Some("https://www.icloud.com.cn/share/synthetic-fei-ui#Synthetic".into());
        }
        fs::write(path, serde_json::to_vec(&result)?)?;
        if command.action == "publishLink" && root.join("fail-link-reply-once").is_file() {
            fs::remove_file(root.join("fail-link-reply-once"))?;
            result.error = Some("Synthetic publication acknowledgement lost".into());
            result.error_kind = Some("shareTimeout".into());
        }
        Ok(result)
    })())
}

pub(super) fn publish(personal: &Path, room: &Room) -> Option<Result<()>> {
    let root = root()?;
    Some((|| {
        if root.join("fail-publish-once").is_file() { fs::remove_file(root.join("fail-publish-once"))?; anyhow::bail!("Synthetic initial upload failed. Retry the retained copy."); }
        let path = room_root(personal, &room.scope.note)?;
        if path.join("seed-published").is_file() { return Ok(()); }
        let mut journal = Journal::open(&path, None)?;
        let request = journal.request()?;
        journal.receive(super::super::Changes { retention: None, uploaded: request.edits.iter().map(|edit| edit.id.clone()).collect(),
            edits: vec![], token: Some(super::super::Bytes(b"synthetic-ui-cursor".to_vec())), more: false, legacy: false, room_access: Some(Access::Owner), #[cfg(feature = "devtools")] cursor_replayed: false, upload_quota_exceeded: false })?;
        fs::write(path.join("seed-published"), b"synthetic UI regression")?;
        Ok(())
    })())
}
