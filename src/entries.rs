//! `Workspace.entries`: every page and folder the window shows. Read as a
//! slice; changed only through `set`, `edit` and `take`, each of which gives
//! it a new `generation`. The sidebar's rows and Home's cards are rebuilt when `generation`
//! moves, instead of hashing every entry each frame to find out.
use crate::store::Entry;
use std::sync::atomic::{AtomicU64, Ordering};

/// One counter for every list, so a list replaced whole never comes back to a
/// number a cache has seen.
static GEN: AtomicU64 = AtomicU64::new(1);
fn next() -> u64 { GEN.fetch_add(1, Ordering::Relaxed) }

pub struct Entries { list: Vec<Entry>, generation: u64 }

impl Default for Entries { fn default() -> Self { Entries { list: vec![], generation: next() } } }

impl From<Vec<Entry>> for Entries { fn from(list: Vec<Entry>) -> Self { Entries { list, generation: next() } } }

impl Entries {
    /// Moves with every change: equal means the same entries.
    pub fn generation(&self) -> u64 { self.generation }
    pub fn set(&mut self, list: Vec<Entry>) { self.list = list; self.generation = next(); }
    /// To change them in place; counts as a change whether or not one is made.
    pub fn edit(&mut self) -> &mut Vec<Entry> { self.generation = next(); &mut self.list }
    pub fn take(&mut self) -> Vec<Entry> { self.generation = next(); std::mem::take(&mut self.list) }
}

impl std::ops::Deref for Entries {
    type Target = [Entry];
    fn deref(&self) -> &[Entry] { &self.list }
}

impl<'a> IntoIterator for &'a Entries {
    type Item = &'a Entry;
    type IntoIter = std::slice::Iter<'a, Entry>;
    fn into_iter(self) -> Self::IntoIter { self.list.iter() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> Entry { Entry { id: id.into(), meta: Default::default(), title: String::new(), icon: None, body: "".into(), reminders: vec![], links: vec![], note: None, top: String::new() } }

    #[test]
    fn every_change_moves_the_generation() {
        let mut e = Entries::default();
        let mut seen = vec![e.generation()];
        let mut moved = |e: &Entries| { assert!(!seen.contains(&e.generation()), "a generation came back"); seen.push(e.generation()); };
        e.set(vec![entry("a"), entry("b")]); moved(&e);
        e.edit().push(entry("c")); moved(&e);
        e.edit()[0].title = "A".into(); moved(&e);
        e.edit().retain(|x| x.id != "b"); moved(&e);
        let all = e.take(); moved(&e);
        assert_eq!(all.len(), 2);
        // A list made anew does not repeat an old number either.
        let f = Entries::default(); moved(&f);
        // Reading changes nothing.
        let g = e.generation();
        let _ = (e.len(), e.iter().count(), e.first());
        assert_eq!(e.generation(), g);
    }
}
