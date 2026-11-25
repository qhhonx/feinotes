//! A remote in memory: one branch, a linear history, blobs by their git id.
//! For tests and for building the interface before a real repository exists.
//! It can play another device pushing first ([`FakeRemote::race`]) and being
//! offline ([`FakeRemote::set_offline`]).

use super::{Change, Entry, Error, Result, Sha, Status, SyncRemote, blob_sha};
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

#[derive(Default)]
pub struct FakeRemote(Mutex<Inner>);

#[derive(Default)]
struct Inner {
    /// Each commit's files (path → blob), by commit id.
    commits: HashMap<Sha, BTreeMap<String, Sha>>,
    blobs: HashMap<Sha, Vec<u8>>,
    heads: HashMap<String, Sha>,
    /// The commits in order, oldest first.
    log: Vec<(Sha, String)>,
    offline: bool,
    /// Another device's changes, committed just before the next commit is made.
    race: Vec<(String, Vec<(String, Option<Vec<u8>>)>)>,
    calls: usize,
}

impl FakeRemote {
    pub fn new() -> Self { Self::default() }

    /// Every call fails as offline until set back.
    pub fn set_offline(&self, offline: bool) { self.0.lock().unwrap().offline = offline }

    /// Another device pushes `changes` to `branch` just before the next
    /// `commit`, so that one finds the branch moved.
    pub fn race(&self, branch: &str, changes: Vec<(String, Option<Vec<u8>>)>) { self.0.lock().unwrap().race.push((branch.to_string(), changes)) }

    /// Whether a [`race`](Self::race) is still waiting for a commit.
    pub fn racing(&self) -> bool { !self.0.lock().unwrap().race.is_empty() }

    /// The files at the branch's head.
    pub fn files(&self, branch: &str) -> BTreeMap<String, Vec<u8>> {
        let i = self.0.lock().unwrap();
        let Some(head) = i.heads.get(branch) else { return BTreeMap::new() };
        i.commits[head].iter().map(|(p, s)| (p.clone(), i.blobs[s].clone())).collect()
    }

    /// The commit messages, oldest first.
    pub fn messages(&self) -> Vec<String> { self.0.lock().unwrap().log.iter().map(|(_, m)| m.clone()).collect() }

    /// How many calls were made.
    pub fn calls(&self) -> usize { self.0.lock().unwrap().calls }

    /// Puts `changes` on `branch` as someone else would (another device).
    pub fn push(&self, branch: &str, changes: &[(String, Option<Vec<u8>>)], message: &str) -> Sha {
        let mut i = self.0.lock().unwrap();
        let parent = i.heads.get(branch).cloned();
        i.apply(branch, parent.as_deref(), changes, message)
    }
}

impl Inner {
    fn enter(&mut self) -> Result<()> {
        self.calls += 1;
        if self.offline { Err(Error::Offline("fake remote is offline".into())) } else { Ok(()) }
    }

    fn files(&self, commit: &str) -> Result<&BTreeMap<String, Sha>> {
        self.commits.get(commit).ok_or_else(|| Error::NotFound(format!("commit {commit}")))
    }

    fn apply(&mut self, branch: &str, parent: Option<&str>, changes: &[(String, Option<Vec<u8>>)], message: &str) -> Sha {
        let mut files = parent.map(|p| self.commits[p].clone()).unwrap_or_default();
        for (path, bytes) in changes {
            match bytes {
                Some(b) => {
                    let sha = blob_sha(b);
                    self.blobs.insert(sha.clone(), b.clone());
                    files.insert(path.clone(), sha);
                }
                None => { files.remove(path); }
            }
        }
        // A commit id: the parent, the files and the message, hashed.
        let mut h = sha1_smol::Sha1::new();
        h.update(format!("{parent:?}{files:?}{message}{}", self.log.len()).as_bytes());
        let sha = h.digest().to_string();
        self.commits.insert(sha.clone(), files);
        self.log.push((sha.clone(), message.to_string()));
        self.heads.insert(branch.to_string(), sha.clone());
        sha
    }
}

impl SyncRemote for FakeRemote {
    fn head(&self, branch: &str) -> Result<Option<Sha>> {
        let mut i = self.0.lock().unwrap();
        i.enter()?;
        Ok(i.heads.get(branch).cloned())
    }

    fn changed(&self, from: &str, to: &str) -> Result<Vec<Change>> {
        let mut i = self.0.lock().unwrap();
        i.enter()?;
        let (a, b) = (i.files(from)?, i.files(to)?);
        let mut out = vec![];
        for (path, sha) in b {
            match a.get(path) {
                None => out.push(Change { path: path.clone(), status: Status::Added, sha: Some(sha.clone()), previous: None }),
                Some(old) if old != sha => out.push(Change { path: path.clone(), status: Status::Modified, sha: Some(sha.clone()), previous: None }),
                _ => {}
            }
        }
        out.extend(a.keys().filter(|p| !b.contains_key(*p)).map(|p| Change { path: p.clone(), status: Status::Removed, sha: None, previous: None }));
        Ok(out)
    }

    fn tree(&self, commit: &str) -> Result<Vec<Entry>> {
        let mut i = self.0.lock().unwrap();
        i.enter()?;
        let files = i.files(commit)?;
        Ok(files.iter().map(|(p, s)| Entry { path: p.clone(), sha: s.clone(), size: i.blobs[s].len() as u64 }).collect())
    }

    fn blob(&self, sha: &str) -> Result<Vec<u8>> {
        let mut i = self.0.lock().unwrap();
        i.enter()?;
        i.blobs.get(sha).cloned().ok_or_else(|| Error::NotFound(format!("blob {sha}")))
    }

    fn snapshot(&self, commit: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let mut i = self.0.lock().unwrap();
        i.enter()?;
        Ok(i.files(commit)?.iter().map(|(p, s)| (p.clone(), i.blobs[s].clone())).collect())
    }

    fn commit(&self, branch: &str, parent: Option<&str>, changes: &[(String, Option<Vec<u8>>)], message: &str) -> Result<Sha> {
        let mut i = self.0.lock().unwrap();
        i.enter()?;
        for (b, c) in std::mem::take(&mut i.race) {
            let p = i.heads.get(&b).cloned();
            i.apply(&b, p.as_deref(), &c, "another device");
        }
        if i.heads.get(branch).map(String::as_str) != parent { return Err(Error::Rejected) }
        Ok(i.apply(branch, parent, changes, message))
    }
}
