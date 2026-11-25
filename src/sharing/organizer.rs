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
    pub(super) fn is_folder(&self, id: &str) -> bool { self.0.get(id).is_some_and(|row| row.title.is_some()) }

    pub(super) fn parent(&self, id: &str) -> Option<String> {
        self.0.get(id).and_then(|row| row.parent.as_ref()).filter(|parent| *parent != id && self.is_folder(parent)).cloned()
    }

    pub(super) fn folders(&self) -> impl Iterator<Item=Entry> + '_ {
        self.0.iter().filter_map(|(id, placement)| {
            let title = placement.title.clone()?;
            Some(Entry { id: id.clone(), title: title.clone(), meta: Meta { folder: Some(title), parent: placement.parent.clone(), ..Default::default() },
                icon: None, body: "".into(), reminders: vec![], links: vec![], note: None, top: String::new() })
        })
    }
}

pub(crate) fn reload(cx: &mut App) {
    let mut changed = super::preferences::reload(cx);
    if let Ok(tree) = Organizer::load() && cx.try_global::<Organizer>() != Some(&tree) {
        cx.set_global(tree);
        changed = true;
    }
    if changed { sidebar::notify(cx); }
}

impl Workspace {
    pub(crate) fn rename_collection(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.sharing.busy { return; }
        let Some(title) = cx.global::<Organizer>().0.get(&id).and_then(|p| p.title.clone()) else { return; };
        self.sharing_sheet(Sheet::RenameCollection(id), window, cx);
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

    pub(super) fn save_collection_name(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.sharing.folder_name.read(cx).text.trim().to_owned();
        if title.is_empty() { return; }
        let result = self.update_organizer(cx, |tree| {
            let name = tree.0.get_mut(id).and_then(|p| p.title.as_mut()).ok_or_else(|| anyhow::anyhow!("Folder unavailable"))?;
            *name = title;
            Ok(())
        });
        if result { self.close_sharing(window, cx); }
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
