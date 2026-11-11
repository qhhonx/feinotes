use crate::store::*;
use crate::model::Document;
use anyhow::Result;
use std::{rc::Rc, cell::RefCell};

#[derive(Clone)]
pub struct SharedStore(Rc<RefCell<Store>>);
impl gpui::Global for SharedStore {}
impl SharedStore {
    pub fn open() -> Result<Self> { Ok(Self(Rc::new(RefCell::new(Store::open()?)))) }
    pub fn take_warnings(&self) -> Vec<String> { std::mem::take(&mut self.0.borrow_mut().warnings) }
    pub fn list(&mut self) -> Vec<Entry> { self.0.borrow_mut().list() }
    pub fn load(&self, id: &str) -> Result<Document> { self.0.borrow_mut().load(id) }
    pub fn load_for_edit(&self, id: &str, shared: bool) -> Result<Document> { self.0.borrow().load_for_edit(id, shared) }
    pub fn forget_loaded(&self, id: &str) { self.0.borrow().forget_loaded(id); }
    pub fn recover_later(&self, doc: &Document) -> Result<()> { self.0.borrow().recover_later(doc) }
    pub fn save(&mut self, id: &str, doc: &Document) -> Result<Meta> { self.0.borrow_mut().save(id, doc) }
    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    pub fn save_sqlite_with<T>(&mut self, id: &str, doc: &Document, sync: impl FnOnce(&rusqlite::Transaction<'_>, i64) -> Result<T>) -> Result<(Meta, T)> {
        self.0.borrow_mut().save_sqlite_with(id, doc, sync)
    }
    pub fn create(&mut self) -> Result<(String, Document)> { self.0.borrow_mut().create() }
    pub fn create_from(&mut self, doc: &Document) -> Result<String> { self.0.borrow_mut().create_from(doc) }
    pub fn size_of(&self, id: &str, doc: &Document) -> u64 { self.0.borrow_mut().size_of(id, doc) }
    pub fn delete(&mut self, id: &str, root: Option<&str>) -> Result<()> { self.0.borrow_mut().delete(id, root) }
    pub fn list_trash(&mut self) -> Vec<TrashEntry> { self.0.borrow_mut().list_trash() }
    pub fn restore(&mut self, id: &str) -> Result<bool> { self.0.borrow_mut().restore(id) }
    pub fn purge(&mut self, id: &str) -> Result<()> { self.0.borrow_mut().purge(id) }
    pub fn import(&mut self, path: &std::path::Path) -> Result<String> { self.0.borrow_mut().import(path) }
    pub fn set_width(&mut self, id: &str, width: PageWidth) -> Result<()> { self.0.borrow_mut().set_width(id, width) }
    pub fn update_meta(&mut self, id: &str, f: impl FnOnce(&mut Meta)) -> Result<()> { self.0.borrow_mut().update_meta(id, f) }
    pub fn set_catalogue(&mut self, id: &str, pinned: bool) -> Result<()> { self.0.borrow_mut().set_catalogue(id, pinned) }
    pub fn settings(&self) -> Settings { self.0.borrow_mut().settings() }
    pub fn save_settings(&self, s: &Settings) -> Result<()> { self.0.borrow_mut().save_settings(s) }
    pub fn rename(&mut self, id: &str, title: &str) -> Result<Meta> { self.0.borrow_mut().rename(id, title) }
    pub fn touch(&mut self, id: &str) -> Option<Meta> { self.0.borrow_mut().touch(id) }
    pub fn create_folder(&mut self, title: &str, parent: Option<&str>) -> Result<String> { self.0.borrow_mut().create_folder(title, parent) }
    pub fn accept_file_title(&self, id: &str, title: &str) -> Result<()> { self.0.borrow().accept_file_title(id, title) }
    pub fn initialize_manual_order(&mut self) -> Result<()> { self.0.borrow_mut().initialize_manual_order() }
    pub fn track_file_changes(&mut self) -> FileChanges { self.0.borrow_mut().track_file_changes() }
    pub fn tree_writer(&self) -> Store { self.0.borrow().tree_writer() }
    pub fn set_parent(&mut self, id: &str, parent: Option<&str>) -> Result<Meta> { self.0.borrow_mut().set_parent(id, parent) }
    pub fn set_starred(&mut self, id: &str, on: bool) -> Result<u64> { self.0.borrow_mut().set_starred(id, on) }
    pub fn set_pinned(&mut self, id: &str, pinned: bool) -> Result<()> { self.0.borrow_mut().set_pinned(id, pinned) }
    pub fn is_markdown(&self, id: &str) -> bool { self.0.borrow().is_markdown(id) }
    pub fn create_markdown(&mut self, doc: &Document, front: &str) -> Result<String> { self.0.borrow_mut().create_markdown(doc, front) }
    pub fn front_matter_of(&self, id: &str) -> String { self.0.borrow().front_matter_of(id) }
    pub fn open_local(&mut self, path: &std::path::Path) -> Result<String> { self.0.borrow_mut().open_local(path) }
    pub fn local_path(&self, id: &str) -> Option<std::path::PathBuf> { self.0.borrow().local_path(id) }
}
