use crate::store::*;
use crate::model::Document;
use anyhow::Result;
use std::{cell::RefCell, rc::Rc, sync::{atomic::{AtomicBool, Ordering}, Arc}};

#[derive(Clone)]
pub struct SharedStore(Rc<RefCell<Store>>, Arc<AtomicBool>, std::path::PathBuf, bool);
pub struct FileTaskGuard(Arc<AtomicBool>);

impl Drop for FileTaskGuard {
    fn drop(&mut self) { self.0.store(false, Ordering::Release); }
}

impl gpui::Global for SharedStore {}
impl SharedStore {
    #[cfg(all(test, target_os = "macos", feature = "cloudkit-sync"))]
    pub(crate) fn for_test(store: Store) -> Self { Self::from_store(store, false) }
    pub fn open() -> Result<Self> { Ok(Self::from_store(Store::open()?, crate::in_shared_note())) }
    fn from_store(store: Store, shared: bool) -> Self {
        let root = store.library_root().to_path_buf();
        Self(Rc::new(RefCell::new(store)), Arc::new(AtomicBool::new(false)), root, shared)
    }
    pub fn root(&self) -> &std::path::Path { &self.2 }
    pub fn is_shared(&self) -> bool { self.3 }
    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    pub(crate) fn open_shared(root: std::path::PathBuf) -> Result<Self> { Ok(Self::from_store(Store::open_sqlite_at(root)?, true)) }
    #[cfg(all(target_os = "macos", feature = "cloudkit-sync"))]
    pub(crate) fn only_session_reference(&self) -> bool { Rc::strong_count(&self.0) == 1 }
    pub fn file_task_busy(&self) -> bool { self.1.load(Ordering::Acquire) }
    pub fn begin_file_task(&self) -> Option<FileTaskGuard> {
        self.1.compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed).is_ok().then(|| FileTaskGuard(self.1.clone()))
    }
    pub fn with<R>(&self, f: impl FnOnce(&mut Store) -> R) -> R { f(&mut self.0.borrow_mut()) }
    pub fn take_warnings(&self) -> Vec<String> { std::mem::take(&mut self.0.borrow_mut().warnings) }
    pub fn adopt_listing(&self, entries: &[Entry]) { self.0.borrow_mut().adopt_listing(entries); }
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
    pub fn create_from_in(&mut self, doc: &Document, parent: Option<&str>) -> Result<String> { self.0.borrow_mut().create_from_in(doc, parent) }
    pub fn create_from(&mut self, doc: &Document) -> Result<String> { self.0.borrow_mut().create_from(doc) }
    pub fn size_of(&self, id: &str, doc: &Document) -> u64 { self.0.borrow_mut().size_of(id, doc) }
    pub fn list_trash(&mut self) -> Vec<TrashEntry> { self.0.borrow_mut().list_trash() }
    pub fn restore(&mut self, id: &str) -> Result<bool> { self.0.borrow_mut().restore(id) }
    pub fn set_width(&mut self, id: &str, width: PageWidth) -> Result<()> { self.0.borrow_mut().set_width(id, width) }
    pub fn update_meta(&mut self, id: &str, f: impl FnOnce(&mut Meta)) -> Result<()> { self.0.borrow_mut().update_meta(id, f) }
    pub fn set_catalogue(&mut self, id: &str, pinned: bool) -> Result<()> { self.0.borrow_mut().set_catalogue(id, pinned) }
    pub fn settings(&self) -> Settings { self.0.borrow_mut().settings() }
    pub fn save_settings(&self, s: &Settings) -> Result<()> { self.0.borrow_mut().save_settings(s) }
    pub fn versions(&self, id: &str) -> Vec<fdl_core::versions::Version> { self.0.borrow().versions(id) }
    pub fn version_diff(&self, id: &str, name: &str, baseline: Option<&str>) -> Result<Vec<fdl_core::diff::Line>> { self.0.borrow().version_diff(id, name, baseline) }
    pub fn version_document(&self, id: &str, name: &str) -> Result<Document> { self.0.borrow().version_document(id, name) }
    pub fn set_version_label(&self, id: &str, name: &str, label: Option<&str>) -> Result<()> { self.0.borrow().set_version_label(id, name, label) }
    pub fn delete_version(&self, id: &str, name: &str) -> Result<()> { self.0.borrow().delete_version(id, name) }
    pub fn restore_version(&self, id: &str, name: &str) -> Result<Document> { self.0.borrow().restore_version(id, name) }
    pub fn save_version_now(&self, id: &str) -> Result<bool> { self.0.borrow().save_version_now(id) }
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
    pub fn create_board_in(&mut self, title: &str, board: &crate::whiteboard::Board, parent: Option<&str>) -> Result<String> { self.0.borrow_mut().create_board_in(title, board, parent) }
    pub fn front_matter_of(&self, id: &str) -> String { self.0.borrow().front_matter_of(id) }
    pub fn open_local(&mut self, path: &std::path::Path) -> Result<String> { self.0.borrow_mut().open_local(path) }
    pub fn local_path(&self, id: &str) -> Option<std::path::PathBuf> { self.0.borrow().local_path(id) }
    pub fn folder_dir(&self, id: Option<&str>) -> Option<std::path::PathBuf> { self.0.borrow().folder_dir(id) }
    pub fn file_path(&self, id: &str) -> Option<std::path::PathBuf> { self.0.borrow().file_path(id) }
}
