//! Ordering and personal marks for shared items, stored in the personal catalogue.
use super::*;
use crate::store::{Entry, Store};
use anyhow::Result;
use std::{collections::BTreeMap, path::PathBuf};

pub(crate) const KEY: &str = "shared-personal-preferences";
const RANK_STEP: u64 = 1 << 32;

#[derive(Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Preferences(pub(crate) BTreeMap<String, u64>);

#[derive(serde::Deserialize)]
struct LegacyMarks { pinned: Vec<String>, starred: BTreeMap<String, u64> }

#[derive(serde::Deserialize)]
struct LegacyOrder { ids: Vec<String> }

impl Preferences {
    pub(crate) fn from_legacy(mut read: impl FnMut(&str) -> Result<Option<Vec<u8>>>) -> Result<Self> {
        let mut values = BTreeMap::new();
        if let Some(marks) = read("shared-organization")?.and_then(|data| serde_json::from_slice::<LegacyMarks>(&data).ok()) {
            for id in marks.pinned { values.insert(format!("legacy:pin:{id}"), 1); }
            for (id, time) in marks.starred { values.insert(format!("legacy:star:{id}"), time); }
        }
        if let Some(order) = read("shared-sidebar-order")?.and_then(|data| serde_json::from_slice::<Vec<String>>(&data).ok()) {
            for (i, id) in order.into_iter().enumerate() { values.insert(format!("legacy:rank:{id}"), (i as u64 + 1) * RANK_STEP); }
        }
        if let Some(orders) = read("mobile-personal-list-order")?.and_then(|data| serde_json::from_slice::<BTreeMap<String, LegacyOrder>>(&data).ok()) {
            for (_, order) in orders.into_iter().filter(|(scope, _)| scope.starts_with("shared:")) {
                for (i, id) in order.ids.into_iter().enumerate() { values.insert(format!("legacy:rank:{id}"), (i as u64 + 1) * RANK_STEP); }
            }
        }
        Ok(Self(values))
    }

    fn load() -> Result<Self> {
        match Store::sqlite_setting(KEY)? {
            Some(data) => Ok(serde_json::from_slice(&data)?),
            None => Self::from_legacy(Store::sqlite_setting),
        }
    }

    pub(crate) fn get(&self, field: &str, id: &str) -> u64 {
        self.0.get(&format!("{field}:{id}")).or_else(|| self.0.get(&format!("legacy:{field}:{id}"))).copied().unwrap_or(0)
    }

    pub(super) fn project(&self, entry: &mut Entry) {
        entry.meta.pinned = self.get("pin", &entry.id) != 0;
        entry.meta.starred = self.get("star", &entry.id);
        let rank = self.get("rank", &entry.id);
        if entry.meta.parent.is_some() { entry.meta.order = rank; }
        else { entry.meta.root_order = (rank > 0).then_some(rank); }
    }

    pub(crate) fn toggle(&mut self, id: &str, pin: bool) {
        let field = if pin { "pin" } else { "star" };
        let value = if self.get(field, id) != 0 { 0 } else if pin { 1 } else { crate::time::now_ms() };
        // A clear must remain authoritative even when another device first imports its legacy marks.
        self.0.insert(format!("{field}:{id}"), value);
    }

    pub(crate) fn move_item<'a>(&mut self, mut ids: Vec<&'a str>, id: &'a str, before: Option<&str>) {
        ids.retain(|sibling| *sibling != id);
        let index = before.and_then(|next| ids.iter().position(|sibling| *sibling == next)).unwrap_or(ids.len());
        ids.insert(index, id);
        let left = index.checked_sub(1).map_or(0, |i| self.get("rank", ids[i]));
        let right = ids.get(index + 1).map_or(u64::MAX, |id| self.get("rank", id));
        // Keep independent moves independent when the surrounding entries are already ranked.
        if ids.iter().enumerate().all(|(i, sibling)| i == index || self.get("rank", sibling) != 0)
            && right.saturating_sub(left) > 1 {
            self.0.insert(format!("rank:{id}"), left + (right - left) / 2);
        } else {
            for (i, id) in ids.into_iter().enumerate() {
                let rank = (i as u64 + 1) * RANK_STEP;
                if self.get("rank", id) != rank { self.0.insert(format!("rank:{id}"), rank); }
            }
        }
    }
}

struct PersonalPreferences { root: PathBuf, values: Preferences }
impl Global for PersonalPreferences {}

pub(super) fn install(cx: &mut App) -> bool {
    let root = Store::root_dir();
    if cx.try_global::<PersonalPreferences>().is_some_and(|state| state.root == root) { return false; }
    let values = if Store::sqlite_session() { Preferences::load().unwrap_or_default() } else { Preferences::default() };
    cx.set_global(PersonalPreferences { root, values });
    true
}

pub(super) fn active(cx: &App) -> &Preferences { &cx.global::<PersonalPreferences>().values }

pub(super) fn set(values: Preferences, cx: &mut App) {
    cx.set_global(PersonalPreferences { root: Store::root_dir(), values });
}

pub(super) fn reload(cx: &mut App) -> bool {
    if install(cx) { return true; }
    if let Ok(values) = Preferences::load() && active(cx) != &values { set(values, cx); return true; }
    false
}

impl Workspace {
    pub(crate) fn toggle_shared_organization(&mut self, id: &str, pin: bool, cx: &mut Context<Self>) {
        if self.shared_list_room(id, cx).is_none() { return; }
        let mut values = active(cx).clone();
        values.toggle(id, pin);
        self.save_shared_preferences(values, &[], cx);
        self.row_menu = None;
        cx.notify();
    }

    #[cfg(target_os = "ios")]
    pub(crate) fn reorder_shared_listing(&mut self, id: &str, before: Option<&str>, rows: &[&Entry], local: (&str, &[u8]), cx: &mut Context<Self>) -> bool {
        let mut values = active(cx).clone();
        values.move_item(rows.iter().map(|entry| entry.id.as_str()).collect(), id, before);
        self.save_shared_preferences(values, &[local], cx)
    }

    fn save_shared_preferences(&mut self, values: Preferences, local: &[(&str, &[u8])], cx: &mut Context<Self>) -> bool {
        let result = (|| {
            let bytes = serde_json::to_vec(&values)?;
            let mut updates = Vec::with_capacity(local.len() + 1);
            updates.push((KEY, bytes.as_slice()));
            updates.extend_from_slice(local);
            Store::write_sqlite_settings(&updates)
        })();
        match result {
            Ok(()) => { set(values, cx); super::sidebar::notify(cx); cloud_sync::retry(&Store::root_dir(), cx); cx.notify(); true }
            Err(error) => { self.show_toast_kind(ui::ToastKind::Error, &settings::diagnostic(&error.to_string()), cx); false }
        }
    }
}
