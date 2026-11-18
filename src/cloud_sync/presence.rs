// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Presence expires locally. It has its own CloudKit zone and never enters
//! the document journal or the history used to restore a library.

use anyhow::{Result, ensure};
use gpui::{BorrowAppContext, App, Global, Hsla, SharedString};
use loro::cursor::Cursor;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, ffi::{CStr, CString, c_char}, path::{Path, PathBuf}, sync::{Arc, Weak}, time::{Duration, Instant}};
use crate::{app, editor::core::{Pos, Selection}, model::BlockId};
use super::{Bytes, content};
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
    Editing(String, Option<Selection>),
}

#[derive(Default, PartialEq, Eq)]
pub(crate) struct View {
    pub devices: Vec<String>,
    pub notes: HashMap<String, Vec<RemoteSelection>>,
    pub collaborators: HashMap<String, Vec<Collaborator>>,
}
#[derive(Default)]
pub(crate) struct Views(HashMap<PathBuf, View>);
impl Global for Views {}
pub(crate) fn view<'a>(root: &std::path::Path, cx: &'a App) -> Option<&'a View> { cx.try_global::<Views>()?.0.get(root) }
pub(crate) fn wake(root: &Path, cx: &App) {
    if let Some(session) = super::session(root, cx) { let _ = session.presence_wake.try_send(()); }
}
#[cfg(target_os = "ios")]
pub(crate) fn wake_all(cx: &mut App) {
    if let Some(sync) = cx.try_global::<super::Sync>() {
        for session in sync.0.values() { let _ = session.presence_wake.try_send(()); }
    }
}
pub(super) fn remove(root: &std::path::Path, cx: &mut App) {
    if !cx.has_global::<Views>() { return; }
    cx.update_global::<Views, _>(|views, _| { views.0.remove(root); });
}

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

impl State {
    fn editing(note: String, selection: Option<Selection>, doc: Option<&loro::LoroDoc>) -> Self {
        let (anchor, head) = selection.zip(doc).map_or((None, None), |(sel, doc)|
            (CursorRef::new(doc, sel.anchor), CursorRef::new(doc, sel.head)));
        Self { note: Some(note), anchor, head }
    }

    fn visible_note(&self, room: Option<&super::shared::Scope>) -> Option<&str> {
        let note = self.note.as_deref()?;
        room.is_none_or(|room| note != super::assets::SCOPE && room.permits(note)).then_some(note)
    }
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

fn local_state(root: &Path, cx: &mut App) -> State {
    let (id, selection) = match app::cloud_active_selection(root, cx) {
        SelectionSource::Idle => return State::default(),
        SelectionSource::Editing(id, selection) => (id, selection),
    };
    let doc = selection.and_then(|_| super::journal(root, cx).text_state(&id).ok().flatten());
    State::editing(id, selection, doc.as_ref())
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

#[cfg(test)]
#[test]
fn editing_presence_can_precede_anchors_and_tracks_the_document_within_a_folder() -> Result<()> {
    use crate::model::{Block, Document};
    use super::shared::Scope;

    let note = uuid::Uuid::new_v4().to_string();
    let folder = Scope { note: uuid::Uuid::new_v4().to_string(), owner: None, folder: true };
    let other = Scope { folder: false, ..folder.clone() };
    let doc = Document { blocks: vec![Block::text("甲乙丙丁")], ..Default::default() };
    let id = doc.blocks[0].id;
    let selection = Selection { anchor: Pos::new(id, "甲".len()), head: Pos::new(id, "甲乙丙".len()) };
    let pending = State::editing(note.clone(), None, None);
    assert_eq!(pending.visible_note(Some(&folder)), Some(note.as_str()));
    assert_eq!(pending.visible_note(Some(&other)), None);
    assert_eq!(State::default().visible_note(Some(&folder)), None);
    assert!(pending.anchor.is_none() && pending.head.is_none());

    let loro = content::open(&content::seed(&doc)?)?;
    let active = State::editing(note, Some(selection), Some(&loro));
    let payload = serde_json::to_string(&active)?;
    assert!(!payload.contains("乙丙"));
    let received: State = serde_json::from_str(&payload)?;
    assert_eq!(received.anchor.and_then(|anchor| anchor.resolve(&loro)), Some(selection.anchor));
    assert_eq!(received.head.and_then(|head| head.resolve(&loro)), Some(selection.head));
    Ok(())
}

fn publish(root: &Path, online: &mut HashMap<u64, Online>, room: Option<&super::shared::Scope>, cx: &mut App) {
    let now = Instant::now();
    online.retain(|_, device| device.until > now);
    let mut devices: Vec<_> = online.iter().collect();
    devices.sort_by_key(|entry| *entry.0);
    let mut view = View::default();
    let mut participants = HashMap::new();
    for (&peer, device) in devices {
        view.devices.push(device.name.clone());
        let Some(note) = device.state.visible_note(room) else { continue; };
        let collaborator = if room.is_some() {
            let key = (note, device.identity.as_deref(), device.identity.is_none().then_some(peer));
            participants.entry(key).or_insert_with(|| {
                let name = device.identity.as_ref().and_then(|_| display_name(device.display_name.as_deref()));
                let initials = name.as_ref().and_then(|name| name.graphemes(true).next()).map(|value| value.to_owned().into())
                    .unwrap_or_else(|| device.alias.to_string().into());
                let name = name.unwrap_or_else(|| crate::localized_format!("协作者 {}", "Collaborator {}", device.alias).into());
                let collaborator = Collaborator { name: Some(name), initials, color: (device.alias - 1) % COLORS.len() };
                view.collaborators.entry(note.to_owned()).or_default().push(collaborator.clone());
                collaborator
            }).clone()
        } else {
            Collaborator { name: None, initials: "".into(), color: (peer % COLORS.len() as u64) as usize }
        };
        if let (Some(anchor), Some(head)) = (&device.state.anchor, &device.state.head)
            && let Ok(Some(doc)) = super::journal(root, cx).text_state(note)
            && let (Some(anchor), Some(head)) = (anchor.resolve(&doc), head.resolve(&doc))
        {
            view.notes.entry(note.to_owned()).or_default().push(RemoteSelection { selection: Selection { anchor, head }, collaborator });
        }
    }
    if self::view(root, cx) != Some(&view) { cx.update_global::<Views, _>(|views, _| { views.0.insert(root.to_path_buf(), view); }); }
}

// A reopened room reuses its CloudKit peer record. Serialize exchanges with
// the retiring generation so its last update cannot overtake the new editor.
#[derive(Default)]
struct Transports(HashMap<PathBuf, Weak<async_lock::Mutex<()>>>);
impl Global for Transports {}

pub(super) fn start(cx: &mut App, root: PathBuf, generation: uuid::Uuid, pause: Option<PathBuf>, wake: async_channel::Receiver<()>) -> Result<()> {
    let (account, channel, peer) = super::journal(&root, cx).identity();
    let (account, channel) = (account.to_owned(), channel.map(str::to_owned));
    let room = super::journal(&root, cx).room().cloned();
    #[cfg(feature = "devtools")]
    let channel = if room.is_some() { crate::cloud_trial::test_channel()? } else { channel };
    let transports = &mut cx.default_global::<Transports>().0;
    let transport = transports.get(&root).and_then(Weak::upgrade).unwrap_or_default();
    transports.retain(|_, transport| transport.strong_count() > 0);
    transports.insert(root.clone(), Arc::downgrade(&transport));
    if !cx.has_global::<Views>() { cx.set_global(Views::default()); }
    cx.update_global::<Views, _>(|views, _| { views.0.insert(root.clone(), View::default()); });
    cx.spawn(async move |cx| {
        let mut delay = Duration::ZERO;
        let mut online = HashMap::new();
        let mut next_alias = 1;
        let mut token = None;
        let mut last: Option<(Instant, Vec<u8>)> = None;
        let mut announced_editing = false;
        loop {
            let mut timer = std::pin::pin!(cx.background_executor().timer(delay));
            let mut woken = std::pin::pin!(wake.recv());
            std::future::poll_fn(|cx| {
                use std::task::Poll;
                if woken.as_mut().poll(cx).is_ready() || timer.as_mut().poll(cx).is_ready() { Poll::Ready(()) }
                else { Poll::Pending }
            }).await;
            let _transport = transport.lock().await;
            if !crate::gpui_compat::async_update(cx, |cx| super::current(&root, generation, cx)).unwrap_or(false) { break; }
            let Ok(request) = crate::gpui_compat::async_update(cx, |cx| {
                let status = super::status(&root, cx).unwrap();
                if status.access_paused || matches!(status.phase, super::SyncPhase::AccountChecking
                    | super::SyncPhase::AccountChanged | super::SyncPhase::SignedOut) {
                    online.clear();
                    publish(&root, &mut online, room.as_ref(), cx);
                    return None;
                }
                if super::journal(&root, cx).access() == Some(super::shared::Access::Revoked) { online.clear(); }
                publish(&root, &mut online, room.as_ref(), cx);
                Some((super::journal(&root, cx).access(), local_state(&root, cx)))
            }) else { break };
            let Some((access, state)) = request else { delay = POLL; continue; };
            if access == Some(super::shared::Access::Revoked) || pause.as_ref().is_some_and(|path| path.exists()) {
                if access.is_some() { online.clear(); }
                delay = POLL;
                continue;
            }
            let editing = state.note.is_some();
            let update = serde_json::to_vec(&state).ok().filter(|state| access.is_none_or(super::shared::Access::can_write)
                && last.as_ref().is_none_or(|(time, old)| old != state || (room.is_none() || editing) && time.elapsed() >= HEARTBEAT));
            if editing && update.is_some() { announced_editing = true; }
            let request = Request { account: account.clone(), channel: channel.clone(), peer: peer.to_string(),
                token: token.clone(), state: update.clone().map(Bytes), room: room.clone() };
            let revision = super::account_revision();
            match cx.background_executor().spawn(async move { exchange(request) }).await {
                Ok(page) if super::account_revision() == revision => {
                    if let Some(state) = update { last = Some((Instant::now(), state)); announced_editing = editing; }
                    if !crate::gpui_compat::async_update(cx, |cx| super::current(&root, generation, cx)).unwrap_or(false) { break; }
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
                    if crate::gpui_compat::async_update(cx, |cx| publish(&root, &mut online, room.as_ref(), cx)).is_err() { break; }
                    delay = if page.more { Duration::ZERO } else { POLL };
                }
                _ => delay = (delay * 2).clamp(POLL, Duration::from_secs(8)),
            }
        }
        if announced_editing && pause.as_ref().is_none_or(|path| !path.exists()) {
            let _transport = transport.lock().await;
            let retired = crate::gpui_compat::async_update(cx, |cx| super::session(&root, cx)
                .is_none_or(|session| session.journal.identity().2 != peer)).unwrap_or(false);
            if retired && let Ok(state) = serde_json::to_vec(&State::default()) {
                let request = Request { account, channel, peer: peer.to_string(), token, state: Some(Bytes(state)), room };
                let _ = cx.background_executor().spawn(async move { exchange(request) }).await;
            }
        }
    }).detach();
    Ok(())
}
