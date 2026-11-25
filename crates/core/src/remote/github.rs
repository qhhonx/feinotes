//! [`SyncRemote`] over GitHub's REST API: the Git Data API (refs, commits,
//! trees, blobs) to read and to commit, compare for what changed, the tarball
//! for a whole snapshot. The API address is a setting, for GitHub Enterprise
//! (`https://<host>/api/v3`).

use super::{Change, Entry, Error, Http, Request, Response, Result, Sha, Status, SyncRemote};
use base64::Engine;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub struct GitHub<H: Http> {
    http: H,
    api: String,
    /// `owner/name`.
    repo: String,
    token: String,
}

/// Compare lists this many files at most; a longer list is cut short.
const COMPARE_FILES: usize = 300;

impl<H: Http> GitHub<H> {
    pub fn new(http: H, api: &str, repo: &str, token: &str) -> Self {
        GitHub { http, api: api.trim_end_matches('/').to_string(), repo: repo.to_string(), token: token.to_string() }
    }

    /// The public GitHub's API.
    pub fn github_com(http: H, repo: &str, token: &str) -> Self { Self::new(http, "https://api.github.com", repo, token) }

    fn send(&self, method: &str, path: &str, body: Option<Value>) -> Result<Response> {
        let mut headers = vec![
            ("Accept".to_string(), "application/vnd.github+json".to_string()),
            ("X-GitHub-Api-Version".to_string(), "2022-11-28".to_string()),
            ("User-Agent".to_string(), "Quiet.md".to_string()),
            ("Authorization".to_string(), format!("Bearer {}", self.token)),
        ];
        if body.is_some() { headers.push(("Content-Type".to_string(), "application/json".to_string())) }
        let url = if path.is_empty() { format!("{}/repos/{}", self.api, self.repo) } else { format!("{}/repos/{}/{path}", self.api, self.repo) };
        self.http.send(Request { method: method.to_string(), url, headers, body: body.map(|b| b.to_string().into_bytes()) })
    }

    /// A call that must succeed, as JSON.
    fn call(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value> {
        let r = self.send(method, path, body)?;
        if !(200..300).contains(&r.status) { return Err(status_error(&r)) }
        serde_json::from_slice(&r.body).map_err(|e| Error::Other(format!("{method} {path}: {e}")))
    }

    /// The tree of a commit.
    fn commit_tree(&self, commit: &str) -> Result<Sha> {
        let c = self.call("GET", &format!("git/commits/{commit}"), None)?;
        string(&c["tree"]["sha"])
    }

    /// Every blob under `tree`, a folder at a time (when the recursive list was cut short).
    fn walk(&self, tree: &str, prefix: &str, out: &mut Vec<Entry>) -> Result<()> {
        let t = self.call("GET", &format!("git/trees/{tree}"), None)?;
        for e in t["tree"].as_array().into_iter().flatten() {
            let path = format!("{prefix}{}", string(&e["path"])?);
            match e["type"].as_str() {
                Some("tree") => self.walk(&string(&e["sha"])?, &format!("{path}/"), out)?,
                _ => if let Some(e) = entry(e, path) { out.push(e) },
            }
        }
        Ok(())
    }

    /// The files of two commits compared (compare could not say).
    fn tree_diff(&self, from: &str, to: &str) -> Result<Vec<Change>> {
        let map = |v: Vec<Entry>| v.into_iter().map(|e| (e.path, e.sha)).collect::<BTreeMap<_, _>>();
        let (a, b) = (map(self.tree(from)?), map(self.tree(to)?));
        let mut out = vec![];
        for (path, sha) in &b {
            match a.get(path) {
                None => out.push(Change { path: path.clone(), status: Status::Added, sha: Some(sha.clone()), previous: None }),
                Some(old) if old != sha => out.push(Change { path: path.clone(), status: Status::Modified, sha: Some(sha.clone()), previous: None }),
                _ => {}
            }
        }
        out.extend(a.keys().filter(|p| !b.contains_key(*p)).map(|p| Change { path: p.clone(), status: Status::Removed, sha: None, previous: None }));
        Ok(out)
    }

    /// Blobs, a tree over `base` (None: from nothing), a commit, and the branch moved to it.
    fn commit_on(&self, branch: &str, parent: Option<&str>, changes: &[(String, Option<Vec<u8>>)], message: &str) -> Result<Sha> {
        let mut tree = vec![];
        for (path, bytes) in changes {
            let sha = match bytes {
                Some(b) => {
                    let blob = self.call("POST", "git/blobs", Some(json!({ "content": b64(b), "encoding": "base64" })))?;
                    Value::String(string(&blob["sha"])?)
                }
                // Removed from the base tree.
                None => Value::Null,
            };
            tree.push(json!({ "path": path, "mode": "100644", "type": "blob", "sha": sha }));
        }
        let mut body = json!({ "tree": tree });
        if let Some(p) = parent { body["base_tree"] = Value::String(self.commit_tree(p)?) }
        let tree = string(&self.call("POST", "git/trees", Some(body))?["sha"])?;
        let parents: Vec<&str> = parent.into_iter().collect();
        let commit = string(&self.call("POST", "git/commits", Some(json!({ "message": message, "tree": tree, "parents": parents })))?["sha"])?;
        // Not forced: a branch that moved since `parent` refuses (422), and so
        // does a branch that came to exist when this was its first commit.
        let r = match parent {
            Some(_) => self.send("PATCH", &format!("git/refs/heads/{}", enc(branch)), Some(json!({ "sha": commit, "force": false })))?,
            None => self.send("POST", "git/refs", Some(json!({ "ref": format!("refs/heads/{branch}"), "sha": commit })))?,
        };
        match r.status {
            200 | 201 => Ok(commit),
            409 | 422 => Err(Error::Rejected),
            _ => Err(status_error(&r)),
        }
    }

    /// The first commit of an empty repository. The Git Data API refuses an
    /// empty repository (409 "Git Repository is empty"), so the first file
    /// goes in through the contents API, which makes the repository's first
    /// commit and the branch; the rest follow as a normal commit on top.
    /// When someone else's commit got there first, the file lands on top of
    /// it: that is told by the new commit having a parent, and answered with
    /// Rejected, so the caller pulls (the file is the same as theirs).
    fn first_commit(&self, branch: &str, changes: &[(String, Option<Vec<u8>>)], message: &str) -> Result<Sha> {
        let Some(first) = changes.iter().position(|(_, b)| b.is_some()) else { return Err(Error::Other("nothing to commit".into())) };
        let (path, bytes) = (&changes[first].0, changes[first].1.as_deref().unwrap_or_default());
        let body = json!({ "message": message, "content": b64(bytes), "branch": branch });
        let r = self.send("PUT", &format!("contents/{}", enc(path)), Some(body))?;
        let made = match r.status {
            200 | 201 => serde_json::from_slice::<Value>(&r.body).map_err(|e| Error::Other(format!("contents: {e}")))?,
            // The file is there already (no `sha` given): not empty after all.
            409 | 422 => return Err(Error::Rejected),
            _ => return Err(status_error(&r)),
        };
        if made["commit"]["parents"].as_array().is_some_and(|p| !p.is_empty()) { return Err(Error::Rejected) }
        let commit = string(&made["commit"]["sha"])?;
        let rest: Vec<_> = changes.iter().enumerate().filter(|(i, _)| *i != first).map(|(_, c)| c.clone()).collect();
        if rest.is_empty() { return Ok(commit) }
        self.commit_on(branch, Some(&commit), &rest, message)
    }
}

impl<H: Http> SyncRemote for GitHub<H> {
    fn head(&self, branch: &str) -> Result<Option<Sha>> {
        let r = self.send("GET", &format!("git/ref/heads/{}", enc(branch)), None)?;
        match r.status {
            200 => {
                let v: Value = serde_json::from_slice(&r.body).map_err(|e| Error::Other(format!("ref: {e}")))?;
                Ok(Some(string(&v["object"]["sha"])?))
            }
            // "Git Repository is empty."
            409 => Ok(None),
            // No such branch, or no such repository (or one this sign-in
            // can't see): ask the repository to tell the two apart.
            404 => self.call("GET", "", None).map(|_| None),
            _ => Err(status_error(&r)),
        }
    }

    fn changed(&self, from: &str, to: &str) -> Result<Vec<Change>> {
        let v = self.call("GET", &format!("compare/{from}...{to}"), None)?;
        let files = v["files"].as_array().cloned().unwrap_or_default();
        // Compare shows changes since the merge base: that is `from` only when
        // `to` is ahead of it (a forced push elsewhere makes them diverge). And
        // past COMPARE_FILES the list is cut short. Either way, the two trees.
        if !matches!(v["status"].as_str(), Some("ahead" | "identical")) || files.len() >= COMPARE_FILES { return self.tree_diff(from, to) }
        let mut out = vec![];
        for f in files {
            let path = string(&f["filename"])?;
            let sha = f["sha"].as_str().map(str::to_string);
            let status = match f["status"].as_str() {
                Some("added" | "copied") => Status::Added,
                Some("removed") => Status::Removed,
                Some("renamed") => Status::Renamed,
                Some("unchanged") => continue,
                _ => Status::Modified,
            };
            let previous = f["previous_filename"].as_str().map(str::to_string);
            out.push(Change { path, sha: if status == Status::Removed { None } else { sha }, status, previous });
        }
        Ok(out)
    }

    fn tree(&self, commit: &str) -> Result<Vec<Entry>> {
        let tree = self.commit_tree(commit)?;
        let t = self.call("GET", &format!("git/trees/{tree}?recursive=1"), None)?;
        // Past 100,000 entries or 7 MB the list is cut short: a folder at a time instead.
        if t["truncated"].as_bool() == Some(true) {
            let mut out = vec![];
            self.walk(&tree, "", &mut out)?;
            return Ok(out);
        }
        let mut out = vec![];
        for e in t["tree"].as_array().into_iter().flatten() {
            if e["type"].as_str() == Some("blob") && let Some(e) = entry(e, string(&e["path"])?) { out.push(e) }
        }
        Ok(out)
    }

    fn blob(&self, sha: &str) -> Result<Vec<u8>> {
        let v = self.call("GET", &format!("git/blobs/{sha}"), None)?;
        let text: String = v["content"].as_str().unwrap_or_default().chars().filter(|c| !c.is_ascii_whitespace()).collect();
        base64::engine::general_purpose::STANDARD.decode(text).map_err(|e| Error::Other(format!("blob {sha}: {e}")))
    }

    fn snapshot(&self, commit: &str) -> Result<Vec<(String, Vec<u8>)>> {
        // A redirect to codeload, which curl follows (without the token: it is in the new address).
        let r = self.send("GET", &format!("tarball/{commit}"), None)?;
        if r.status != 200 { return Err(status_error(&r)) }
        let tar = super::tar::gunzip(&r.body).map_err(|e| Error::Other(format!("tarball: {e}")))?;
        super::tar::files(&tar).map_err(|e| Error::Other(format!("tarball: {e}")))
    }

    fn commit(&self, branch: &str, parent: Option<&str>, changes: &[(String, Option<Vec<u8>>)], message: &str) -> Result<Sha> {
        match parent {
            Some(_) => self.commit_on(branch, parent, changes, message),
            // A new branch in a repository that has commits, or else an empty repository.
            None => match self.commit_on(branch, None, changes, message) {
                Err(Error::Other(m)) if m.starts_with("409") => self.first_commit(branch, changes, message),
                r => r,
            },
        }
    }
}

/// A regular file of a tree listing (no links, no submodules).
fn entry(e: &Value, path: String) -> Option<Entry> {
    if e["type"].as_str() != Some("blob") || !matches!(e["mode"].as_str(), Some("100644" | "100755")) { return None }
    Some(Entry { path, sha: e["sha"].as_str()?.to_string(), size: e["size"].as_u64().unwrap_or(0) })
}

fn string(v: &Value) -> Result<String> {
    v.as_str().map(str::to_string).ok_or_else(|| Error::Other(format!("unexpected answer: {v}")))
}

fn b64(b: &[u8]) -> String { base64::engine::general_purpose::STANDARD.encode(b) }

/// A path or branch in a URL: everything but letters, digits, `-._~` and `/` escaped.
fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) { out.push(b as char) } else { out.push_str(&format!("%{b:02X}")) }
    }
    out
}

/// A failed status in sync's terms. GitHub says "rate limit" with 403 or 429
/// and `x-ratelimit-remaining: 0` (the hourly limit) or `retry-after` (the
/// secondary one); any other 403 is a sign-in without the permission.
fn status_error(r: &Response) -> Error {
    let msg = serde_json::from_slice::<Value>(&r.body).ok().and_then(|v| v["message"].as_str().map(str::to_string)).unwrap_or_default();
    let limited = r.header("x-ratelimit-remaining") == Some("0") || r.header("retry-after").is_some() || msg.to_lowercase().contains("rate limit");
    match r.status {
        403 | 429 if limited => {
            let reset = r.header("x-ratelimit-reset").and_then(|v| v.parse().ok()).or_else(|| {
                let wait: u64 = r.header("retry-after")?.parse().ok()?;
                Some(web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).ok()?.as_secs() + wait)
            });
            Error::RateLimited { reset }
        }
        401 | 403 => Error::Auth(msg),
        404 => Error::NotFound(msg),
        s => Error::Other(format!("{s} {msg}")),
    }
}
