//! A selected library can change only after its stores and sync journal close.
use anyhow::{Result, ensure};
use std::{path::{Path, PathBuf}, sync::{Arc, RwLock}};

struct Selected { root: PathBuf, sqlite: bool, active: Arc<()> }

pub(super) struct Registry(RwLock<Option<Selected>>);

pub struct SessionLease { _active: Arc<()> }

impl Registry {
    pub(super) const fn new() -> Self { Self(RwLock::new(None)) }

    pub(super) fn select(&self, root: PathBuf, sqlite: bool) -> Result<()> {
        let mut selected = self.0.write().expect("library session lock");
        ensure!(selected.is_none(), "Library already selected");
        *selected = Some(Selected { root, sqlite, active: Arc::new(()) });
        Ok(())
    }

    pub(super) fn root(&self) -> Option<PathBuf> {
        self.0.read().expect("library session lock").as_ref().map(|selected| selected.root.clone())
    }

    pub(super) fn sqlite(&self) -> bool {
        self.0.read().expect("library session lock").as_ref().is_some_and(|selected| selected.sqlite)
    }

    pub(super) fn lease(&self, root: &Path) -> Option<SessionLease> {
        self.0.read().expect("library session lock").as_ref().filter(|selected| selected.root == root)
            .map(|selected| SessionLease { _active: selected.active.clone() })
    }

    pub(super) fn quiescent(&self, expected: &Path) -> Result<bool> {
        let selected = self.0.read().expect("library session lock");
        let selected = selected.as_ref().ok_or_else(|| anyhow::anyhow!("No library selected"))?;
        ensure!(selected.sqlite && selected.root == expected, "The selected library changed");
        Ok(Arc::strong_count(&selected.active) == 1)
    }

    pub(super) fn replace(&self, expected: &Path, root: PathBuf) -> Result<()> {
        let mut selected = self.0.write().expect("library session lock");
        let previous = selected.as_ref().ok_or_else(|| anyhow::anyhow!("No library selected"))?;
        ensure!(previous.sqlite && previous.root == expected, "The selected library changed");
        ensure!(Arc::strong_count(&previous.active) == 1, "Close the previous library's stores and sync journal before switching");
        *selected = Some(Selected { root, sqlite: true, active: Arc::new(()) });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handoff_requires_all_previous_owners_and_the_expected_library() -> Result<()> {
        let sessions = Registry::new();
        let first = PathBuf::from("/library/first");
        let second = PathBuf::from("/library/second");
        sessions.select(first.clone(), true)?;
        let editor = sessions.lease(&first).unwrap();
        let journal = sessions.lease(&first).unwrap();
        assert!(sessions.replace(&first, second.clone()).is_err());
        drop(editor);
        assert!(sessions.replace(&first, second.clone()).is_err());
        drop(journal);
        assert!(sessions.replace(&second, second.clone()).is_err());
        assert_eq!(sessions.root(), Some(first.clone()));
        sessions.replace(&first, second.clone())?;
        assert_eq!(sessions.root(), Some(second.clone()));
        assert!(sessions.lease(&first).is_none());
        let new_editor = sessions.lease(&second).unwrap();
        assert!(sessions.replace(&second, first.clone()).is_err());
        drop(new_editor);
        sessions.replace(&second, first.clone())?;
        assert_eq!(sessions.root(), Some(first));
        Ok(())
    }

    #[test]
    fn process_selected_file_libraries_do_not_gain_a_switching_path() -> Result<()> {
        let sessions = Registry::new();
        let root = PathBuf::from("/library/files");
        sessions.select(root.clone(), false)?;
        assert!(!sessions.sqlite());
        assert!(sessions.select(root.clone(), true).is_err());
        assert!(sessions.replace(&root, PathBuf::from("/library/sqlite")).is_err());
        assert_eq!(sessions.root(), Some(root));
        Ok(())
    }
}
