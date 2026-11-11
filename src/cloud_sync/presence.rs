// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Presence expires locally. It has its own CloudKit zone and never enters
//! the document journal or the history used to restore a library.

use anyhow::{Result, ensure};
use gpui::{App, Global, Hsla, SharedString};
use loro::cursor::Cursor;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, ffi::{CStr, CString, c_char}, path::PathBuf, time::{Duration, Instant}};
use crate::{app, editor::core::{Pos, Selection}, model::BlockId};
use super::{Bytes, Sync, content};
use unicode_segmentation::UnicodeSegmentation;

const POLL: Duration = Duration::from_secs(2);
const HEARTBEAT: Duration = Duration::from_secs(8);
const TTL: Duration = Duration::from_secs(24);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemoteSelection {
    pub selection: Selection,
    pub collaborator: Collaborator,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Collaborator {
    pub name: Option<SharedString>,
    pub initials: SharedString,
    color: usize,
}

const COLORS: [&str; 4] = ["green", "purple", "orange", "red"];

impl Collaborator {
    pub fn color(&self) -> Hsla {
        crate::editor_theme::text_color(COLORS[self.color]).unwrap_or_else(crate::editor_theme::Theme::caret)
    }
}

pub(crate) fn badge_ink(color: Hsla) -> Hsla {
    let rgb = color.to_rgb();
    let linear = |value: f32| if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) };
    let luminance = 0.2126 * linear(rgb.r) + 0.7152 * linear(rgb.g) + 0.0722 * linear(rgb.b);
    if luminance > 0.179 { gpui::black() } else { gpui::white() }
}

pub(crate) enum SelectionSource {
    Idle,
    Editing(String, Selection),
    Waiting,
}

#[derive(Default, PartialEq, Eq)]
pub(crate) struct View {
    pub devices: Vec<String>,
    pub notes: HashMap<String, Vec<RemoteSelection>>,
    pub collaborators: HashMap<String, Vec<Collaborator>>,
}
impl Global for View {}

unsafe extern "C" { fn quiet_cloud_presence(json: *const c_char) -> *mut c_char; }

#[derive(Serialize)]
struct Request {
    account: String,
    channel: Option<String>,
    peer: String,
    token: Option<Bytes>,
    state: Option<Bytes>,
    room: Option<super::shared::Scope>,
}

#[derive(Serialize, Deserialize)]
struct CursorRef {
    block: u64,
    cursor: Bytes,
}

impl CursorRef {
    fn new(doc: &loro::LoroDoc, pos: Pos) -> Option<Self> {
        let cursor = content::anchor(doc, pos)?;
        Some(Self { block: pos.block.0, cursor: Bytes(cursor.encode()) })
    }

    fn resolve(&self, doc: &loro::LoroDoc) -> Option<Pos> {
        let cursor = Cursor::decode(&self.cursor.0).ok()?;
        content::resolve(doc, BlockId(self.block), &cursor)
    }
}

#[derive(Default, Serialize, Deserialize)]
struct State {
    note: Option<String>,
    anchor: Option<CursorRef>,
    head: Option<CursorRef>,
}

#[derive(Deserialize)]
struct Device {
    peer: String,
    name: String,
    state: Bytes,
    age: f64,
    identity: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct Page {
    devices: Vec<Device>,
    removed: Vec<String>,
    token: Option<Bytes>,
    more: bool,
    replay: bool,
    #[serde(rename = "editableIdentities")]
    editable_identities: Option<Vec<String>>,
    error: Option<String>,
}

fn exchange(request: Request) -> Result<Page> {
    let json = CString::new(serde_json::to_vec(&request)?)?;
    let ptr = unsafe { quiet_cloud_presence(json.as_ptr()) };
    ensure!(!ptr.is_null(), "CloudKit presence returned no response");
    let page = unsafe { serde_json::from_slice::<Page>(CStr::from_ptr(ptr).to_bytes()) };
    unsafe { super::quiet_cloud_restore_free(ptr) };
    let page = page?;
    ensure!(page.error.is_none(), "{}", page.error.as_deref().unwrap_or_default());
    Ok(page)
}

fn local_state(cx: &mut App) -> Result<Option<State>> {
    let (id, selection) = match app::cloud_active_selection(cx) {
        SelectionSource::Idle => return Ok(Some(State::default())),
        SelectionSource::Waiting => return Ok(None),
        SelectionSource::Editing(id, selection) => (id, selection),
    };
    let doc = cx.global::<Sync>().0.text_state(&id)?;
    let anchor = doc.as_ref().and_then(|doc| CursorRef::new(doc, selection.anchor));
    let head = doc.as_ref().and_then(|doc| CursorRef::new(doc, selection.head));
    Ok(Some(State { note: Some(id), anchor, head }))
}

struct Online {
    name: String,
    identity: Option<String>,
    display_name: Option<String>,
    alias: usize,
    state: State,
    until: Instant,
}

fn display_name(name: Option<&str>) -> Option<SharedString> {
    let name = name?.trim();
    if name.is_empty() || name.contains('@') || name.chars().any(|ch| ch.is_control() || matches!(ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')) { return None; }
    Some(name.graphemes(true).take(20).collect::<String>().into())
}

#[cfg(test)]
#[test]
fn shared_names_omit_addresses_and_controls() {
    assert_eq!(display_name(Some(" 林夏 ")).unwrap().to_string(), "林夏");
    assert!(display_name(Some("person@example.test")).is_none());
    assert!(display_name(Some("a\u{202e}b")).is_none());
    assert!(display_name(Some("\n")).is_none());
    let name = "👩‍💻".repeat(21);
    assert_eq!(display_name(Some(&name)).unwrap().graphemes(true).count(), 20);
}

fn publish(online: &mut HashMap<u64, Online>, room_note: Option<&str>, cx: &mut App) {
    let now = Instant::now();
    online.retain(|_, device| device.until > now);
    let mut devices: Vec<_> = online.iter().collect();
    devices.sort_by_key(|entry| *entry.0);
    let mut view = View::default();
    let mut participants = HashMap::new();
    for (&peer, device) in devices {
        view.devices.push(device.name.clone());
        let Some(note) = &device.state.note else { continue; };
        if room_note.is_some_and(|id| id != note) { continue; }
        let collaborator = if room_note.is_some() {
            let key = (note, device.identity.as_deref(), device.identity.is_none().then_some(peer));
            participants.entry(key).or_insert_with(|| {
                let name = device.identity.as_ref().and_then(|_| display_name(device.display_name.as_deref()));
                let initials = name.as_ref().and_then(|name| name.graphemes(true).next()).map(|value| value.to_owned().into())
                    .unwrap_or_else(|| device.alias.to_string().into());
                let name = name.unwrap_or_else(|| crate::localized_format!("协作者 {}", "Collaborator {}", device.alias).into());
                let collaborator = Collaborator { name: Some(name), initials, color: (device.alias - 1) % COLORS.len() };
                view.collaborators.entry(note.clone()).or_default().push(collaborator.clone());
                collaborator
            }).clone()
        } else {
            Collaborator { name: None, initials: "".into(), color: (peer % COLORS.len() as u64) as usize }
        };
        if let Ok(Some(doc)) = cx.global::<Sync>().0.text_state(note)
            && let Some(anchor) = device.state.anchor.as_ref().and_then(|pos| pos.resolve(&doc))
            && let Some(head) = device.state.head.as_ref().and_then(|pos| pos.resolve(&doc))
        {
            view.notes.entry(note.clone()).or_default().push(RemoteSelection { selection: Selection { anchor, head }, collaborator });
        }
    }
    if cx.global::<View>() != &view { cx.set_global(view); }
}

pub(super) fn start(cx: &mut App, pause: Option<PathBuf>) -> Result<()> {
    let (account, channel, peer) = cx.global::<Sync>().0.identity();
    let (account, channel) = (account.to_owned(), channel.map(str::to_owned));
    let room = cx.global::<Sync>().0.room().cloned();
    #[cfg(feature = "devtools")]
    let channel = if room.is_some() { crate::cloud_trial::test_channel()? } else { channel };
    cx.set_global(View::default());
    cx.spawn(async move |cx| {
        let mut delay = Duration::ZERO;
        let mut online = HashMap::new();
        let mut next_alias = 1;
        let room_note = room.as_ref().map(|room| room.note.as_str());
        let mut token = None;
        let mut last: Option<(Instant, Vec<u8>)> = None;
        loop {
            cx.background_executor().timer(delay).await;
            let Ok(request) = cx.update(|cx| {
                let status = cx.global::<super::SyncStatus>();
                if status.access_paused || matches!(status.phase, super::SyncPhase::AccountChecking
                    | super::SyncPhase::AccountChanged | super::SyncPhase::SignedOut) {
                    online.clear();
                    publish(&mut online, room_note, cx);
                    return None;
                }
                if cx.global::<Sync>().0.access() == Some(super::shared::Access::Revoked) { online.clear(); }
                publish(&mut online, room_note, cx);
                Some((cx.global::<Sync>().0.access(),
                    local_state(cx).and_then(|state| state.map(|state| serde_json::to_vec(&state).map_err(Into::into)).transpose())))
            }) else { break };
            let Some((access, state)) = request else { delay = POLL; continue; };
            let Ok(state) = state else { delay = POLL; continue };
            let state = state.or_else(|| last.as_ref().map(|(_, previous)| previous.clone()));
            if access == Some(super::shared::Access::Revoked) || pause.as_ref().is_some_and(|path| path.exists()) {
                if access.is_some() { online.clear(); }
                delay = POLL;
                continue;
            }
            let update = state.filter(|state| access.is_none_or(super::shared::Access::can_write)
                && last.as_ref().is_none_or(|(time, old)| old != state || time.elapsed() >= HEARTBEAT));
            let request = Request { account: account.clone(), channel: channel.clone(), peer: peer.to_string(),
                token: token.clone(), state: update.clone().map(Bytes), room: room.clone() };
            let revision = super::account_revision();
            match cx.background_executor().spawn(async move { exchange(request) }).await {
                Ok(page) if super::account_revision() == revision => {
                    if let Some(state) = update { last = Some((Instant::now(), state)); }
                    if page.replay { online.clear(); }
                    token = page.token;
                    for id in page.removed.iter().filter_map(|id| id.parse::<u64>().ok()) { online.remove(&id); }
                    if let Some(editors) = page.editable_identities {
                        online.retain(|_, device| device.identity.as_ref().is_none_or(|key| editors.contains(key)));
                    }
                    for device in page.devices {
                        let Ok(id) = device.peer.parse::<u64>() else { continue };
                        if id == peer || !(0.0..TTL.as_secs_f64()).contains(&device.age) { continue; }
                        if let Ok(state) = serde_json::from_slice(&device.state.0) {
                            let member_alias = device.identity.as_ref().and_then(|identity| online.values()
                                .find(|old| old.identity.as_ref() == Some(identity)).map(|old| old.alias));
                            let alias = member_alias.or_else(|| online.get(&id).map(|old| old.alias)).unwrap_or_else(|| {
                                let alias = next_alias;
                                next_alias += 1;
                                alias
                            });
                            online.insert(id, Online { name: device.name, identity: device.identity, display_name: device.display_name, alias, state,
                                until: Instant::now() + TTL - Duration::from_secs_f64(device.age) });
                        }
                    }
                    if cx.update(|cx| publish(&mut online, room_note, cx)).is_err() { break; }
                    delay = if page.more { Duration::ZERO } else { POLL };
                }
                _ => delay = (delay * 2).clamp(POLL, Duration::from_secs(8)),
            }
        }
    }).detach();
    Ok(())
}
