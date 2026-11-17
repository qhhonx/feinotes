// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Opt-in offline invitation transport for fresh, synthetic native UI regressions.
use super::*;

fn root() -> Option<PathBuf> {
    std::env::var_os("QUIET_SYNC_QA_SHARING_UI")?;
    assert!(crate::devtools::scripted(), "Sharing UI QA requires the script driver");
    let root = super::super::test_root().expect("isolated QA root").expect("isolated QA root");
    assert!(root.join("qa-session-rooms.json").is_file() && root.join("pause-cloud").is_file(), "Use fresh synthetic session fixtures");
    Some(root)
}

pub(super) fn enabled() -> bool { root().is_some() }

fn reply() -> Reply { Reply { scope: None, title: None, url: None, members: vec![], error: None, error_kind: None } }

pub(super) fn command(command: &Command<'_>) -> Option<Result<Reply>> {
    let root = root()?;
    Some((|| {
        if command.action == "checkAccess" { return Ok(reply()); }
        if command.action == "accept" {
            ensure!(command.url == Some("https://www.icloud.com/share/synthetic-fei-ui"), "Invalid synthetic invitation link");
            let rooms: Vec<Room> = serde_json::from_slice(&fs::read(root.join("qa-session-rooms.json"))?)?;
            let mut result = reply(); result.scope = Some(rooms[1].scope.clone()); result.title = Some(rooms[1].title.clone());
            return Ok(result);
        }
        let scope = command.scope.context("Missing synthetic scope")?;
        if command.action == "delete" || command.action == "leave" {
            ensure!((command.action == "delete") == scope.owner.is_none(), "Synthetic removal role mismatch");
            if root.join("fail-end-once").exists() { fs::remove_file(root.join("fail-end-once"))?; anyhow::bail!("Synthetic removal failed. Recovery copy retained."); }
            return Ok(reply());
        }
        ensure!(scope.owner.is_none(), "Only synthetic owners manage invitations");
        let path = root.join(format!("qa-members-{}.json", scope.note));
        let mut result = if path.is_file() { serde_json::from_slice(&fs::read(&path)?)? } else { reply() };
        result.url = Some("https://www.icloud.com/share/synthetic-fei-ui".into());
        match command.action {
            "members" => {},
            "invite" => {
                if root.join("fail-invite-once").is_file() { fs::remove_file(root.join("fail-invite-once"))?; anyhow::bail!("Synthetic invitation failed. Retry this shared note."); }
                result.members.push(Member { id: "synthetic-participant".into(), name: "".into(), permission: command.permission.context("Missing permission")?, accepted: false });
            }
            "permission" => result.members.iter_mut().find(|member| Some(member.id.as_str()) == command.participant)
                .context("Missing synthetic participant")?.permission = command.permission.context("Missing permission")?,
            "revoke" => result.members.retain(|member| Some(member.id.as_str()) != command.participant),
            _ => anyhow::bail!("Unexpected synthetic share command"),
        }
        fs::write(path, serde_json::to_vec(&result)?)?;
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
            edits: vec![], token: Some(super::super::Bytes(b"synthetic-ui-cursor".to_vec())), more: false, legacy: false, room_access: Some(Access::Owner), cursor_replayed: false, upload_quota_exceeded: false })?;
        fs::write(path.join("seed-published"), b"synthetic UI regression")?;
        Ok(())
    })())
}
