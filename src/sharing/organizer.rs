//! The folder tree used only by the current user's Shared Documents view.
use super::*;
use crate::store::{Entry, Meta, Store};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const KEY: &str = "shared-personal-folders";
#[derive(Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Placement {
    pub(crate) title: Option<String>,
    pub(crate) parent: Option<String>,
}
#[derive(Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Organizer(pub BTreeMap<String, Placement>);
impl Global for Organizer {}

impl Organizer {
    pub(super) fn load() -> anyhow::Result<Self> {
        Ok(Store::sqlite_setting(KEY)?.map(|bytes| serde_json::from_slice(&bytes)).transpose()?.unwrap_or_default())
    }
    pub(crate) fn place(&mut self, id: &str, parent: Option<&str>) -> anyhow::Result<()> {
        let mut visited = BTreeSet::from([id]);
        let mut next = parent;
        while let Some(folder) = next {
            anyhow::ensure!(visited.insert(folder), "A folder cannot contain itself");
            let row = self.0.get(folder).filter(|r| r.title.is_some()).ok_or_else(|| anyhow::anyhow!("Folder unavailable"))?;
            next = row.parent.as_deref();
        }
        self.0.entry(id.to_string()).or_default().parent = parent.map(str::to_string);
        Ok(())
    }
    pub(super) fn folders(&self) -> impl Iterator<Item=Entry> + '_ {
        self.0.iter().filter_map(|(id, placement)| {
            let title = placement.title.clone()?;
            Some(Entry { id: id.clone(), title: title.clone(), meta: Meta { folder: Some(title), parent: placement.parent.clone(), ..Default::default() },
                icon: None, body: "".into(), reminders: vec![], links: vec![] })
        })
    }
}

pub(crate) fn reload(cx: &mut App) {
    if let Ok(tree) = Organizer::load() && cx.try_global::<Organizer>() != Some(&tree) { cx.set_global(tree); }
}

impl Workspace {
    pub(crate) fn rename_collection(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let title = cx.global::<Organizer>().0.get(&id).and_then(|p| p.title.clone()).unwrap_or_default();
        self.personal_collection(None, window, cx); self.sharing.edit_collection = Some(id);
        self.sharing.folder_name.update(cx, |field, cx| field.set_text(&title, cx));
    }
    pub(crate) fn remove_collection(&mut self, id: &str, cx: &mut Context<Self>) {
        self.update_organizer(cx, |tree| {
            if let Some(old) = tree.0.remove(id) {
                for placement in tree.0.values_mut().filter(|p| p.parent.as_deref() == Some(id)) { placement.parent = old.parent.clone(); }
            }
            Ok(())
        });
        self.row_menu = None; cx.notify();
    }

    pub(crate) fn personal_collection(&mut self, parent: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.sharing_sheet(Sheet::Collection(parent), window, cx);
        self.sharing.folder_name.update(cx, |field, cx| field.set_text("", cx));
    }
    pub(super) fn save_collection(&mut self, parent: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.sharing.folder_name.read(cx).text.trim().to_owned();
        if title.is_empty() { return; }
        let editing = self.sharing.edit_collection.take();
        let id = editing.clone().unwrap_or_else(|| format!("collection-{}", uuid::Uuid::new_v4()));
        let result = self.update_organizer(cx, |tree| {
            if editing.is_none() { tree.place(&id, parent.as_deref())?; }
            anyhow::ensure!(tree.0.contains_key(&id), "Folder unavailable");
            tree.0.get_mut(&id).unwrap().title = Some(title);
            Ok(())
        });
        if result {
            self.sharing.collection = parent.clone();
            if let Some(parent) = parent { self.expanded.insert(parent); }
            self.sharing.folded = false;
            self.close_sharing(window, cx);
        }
    }
    pub(super) fn update_organizer(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Organizer) -> anyhow::Result<()>) -> bool {
        let result = (|| {
            let mut tree = Organizer::load()?;
            change(&mut tree)?;
            Store::write_sqlite_setting(KEY, &serde_json::to_vec(&tree)?)?;
            Ok::<_, anyhow::Error>(tree)
        })();
        match result {
            Ok(tree) => { cx.set_global(tree); sidebar::notify(cx); cloud_sync::retry(&Store::root_dir(), cx); true }
            Err(error) => { self.show_toast_kind(ui::ToastKind::Error, &settings::diagnostic(&error.to_string()), cx); false }
        }
    }
}
