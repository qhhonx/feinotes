// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub(crate) const PREFIX: &str = "https://feinotes.app/o/?id=";
const V1: u8 = 0x10;
const DOMAIN: &[u8] = b"feinotes-object-scope-v1\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind { Personal = 1, Shared = 2 }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Link {
    pub kind: Kind,
    pub scope: [u8; 16],
    pub object: Uuid,
}

pub(crate) fn owner_key(owner_record_name: &str) -> String {
    format!("{:x}", Sha256::digest(owner_record_name.as_bytes()))
}

fn scope(kind: Kind, owner_key: &str, suffix: &[u8]) -> [u8; 16] {
    let mut digest = Sha256::new();
    digest.update(DOMAIN);
    digest.update([kind as u8]);
    digest.update(owner_key.as_bytes());
    digest.update(suffix);
    digest.finalize()[..16].try_into().expect("SHA256 includes 16 bytes")
}

pub(crate) fn personal_scope(owner_key: &str, channel: Option<&str>) -> [u8; 16] {
    scope(Kind::Personal, owner_key, channel.unwrap_or_default().as_bytes())
}

pub(crate) fn shared_scope(owner_key: &str, root: &str, folder: bool) -> Option<[u8; 16]> {
    let root = Uuid::parse_str(root).ok()?;
    let mut suffix = [u8::from(folder); 17];
    suffix[1..].copy_from_slice(root.as_bytes());
    Some(scope(Kind::Shared, owner_key, &suffix))
}

impl Link {
    pub(crate) fn url(self) -> String {
        let mut bytes = [0; 33];
        bytes[0] = V1 | self.kind as u8;
        bytes[1..17].copy_from_slice(&self.scope);
        bytes[17..].copy_from_slice(self.object.as_bytes());
        format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes))
    }

    pub(crate) fn parse(url: &str) -> Option<Self> {
        let encoded = url.strip_prefix(PREFIX)?;
        if encoded.len() != 44 { return None; }
        let mut bytes = [0; 33];
        if URL_SAFE_NO_PAD.decode_slice(encoded, &mut bytes).ok()? != bytes.len() { return None; }
        let kind = match bytes[0] { 0x11 => Kind::Personal, 0x12 => Kind::Shared, _ => return None };
        Some(Self { kind, scope: bytes[1..17].try_into().ok()?, object: Uuid::from_slice(&bytes[17..]).ok()? })
    }
}

pub(crate) fn identity(personal: &std::path::Path, cx: &gpui::App) -> Option<(String, Option<String>)> {
    let (account, channel, _) = super::session(personal, cx)?.journal.identity();
    Some((account.to_owned(), channel.map(str::to_owned)))
}

pub(crate) fn room_scope(account: &str, room: &super::shared::Room) -> Option<[u8; 16]> {
    let owner = room.scope.owner.as_deref().map(owner_key);
    shared_scope(owner.as_deref().unwrap_or(account), &room.scope.note, room.scope.folder)
}

pub(crate) fn find_room(
    personal: &std::path::Path, account: &str, link: Link,
) -> anyhow::Result<Option<(super::shared::Room, super::shared::VerifiedRoomAccess)>> {
    let find = || -> anyhow::Result<Option<super::shared::Room>> {
        let mut rooms = super::shared::rooms(personal)?.into_iter().filter(|room| room_scope(account, room) == Some(link.scope));
        let room = rooms.next();
        anyhow::ensure!(rooms.next().is_none(), "Ambiguous shared scope");
        Ok(room)
    };
    let mut room = find()?;
    if room.is_none() {
        // Discovery only registers existing, authorized shares. It never accepts an invitation.
        let joined = super::shared::discover(personal, account, true);
        room = find()?;
        if room.is_none() {
            let owned = super::shared::discover(personal, account, false);
            room = find()?;
            if room.is_none() { joined?; owned?; }
        }
    }
    let Some(room) = room else { return Ok(None); };
    if !room.scope.folder && room.scope.note != link.object.to_string() { return Ok(None); }
    let access = super::shared::verify_room_access(personal, &room)?;
    Ok(Some((room, access)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_links_match_browser_vectors_and_reject_noncanonical_urls() {
        let owner = owner_key("_syntheticObjectOwner");
        let object = Uuid::parse_str("2b5779cc-07c7-49bb-a58b-703e58793d20").unwrap();
        let root = "34a16d93-954c-427a-a523-c8676d030dff";
        let channel = "500c45bb-fd8d-4f33-b192-afcf82794a7c";
        let shared = super::shared_scope(&owner, root, true).unwrap();
        assert_ne!(shared, super::shared_scope(&owner_key("_otherOwner"), root, true).unwrap());
        for (kind, scope, expected) in [
            (Kind::Personal, personal_scope(&owner, None), "ER6SW-JTRIWZXlgDVjTGZikrV3nMB8dJu6WLcD5YeT0g"),
            (Kind::Personal, personal_scope(&owner, Some(channel)), "ESxibyuzRyG9js7Ayh7U1oMrV3nMB8dJu6WLcD5YeT0g"),
            (Kind::Shared, shared, "EpuKKYPY_gt4OG13H5JOTrwrV3nMB8dJu6WLcD5YeT0g"),
        ] {
            let link = Link { kind, scope, object };
            let url = link.url();
            assert_eq!(url, format!("{PREFIX}{expected}"));
            assert_eq!(Link::parse(&url), Some(link));
            assert!(Link::parse(&format!("{url}&type=folder")).is_none());
            assert!(Link::parse(&url.replace("feinotes.app", "feinotes.app.example")).is_none());
        }
    }
}
