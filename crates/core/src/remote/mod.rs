//! Git sync without git (docs/design/sync.md §2.1, way a): the library folder
//! is not a work tree; a GitHub repository is the remote, reached through its
//! REST API, and every sync is pull → merge here → push one commit whose
//! parent is the remote's head. History stays linear and nothing is rewritten.
//!
//! - [`Http`]: one request, one response. [`CurlHttp`] runs the system curl
//!   on desktop; iOS will bring its own (NSURLSession).
//! - [`SyncRemote`]: what sync needs of a remote. [`GitHub`] is the real one,
//!   [`FakeRemote`] an in-memory one for tests and UI work.
//! - [`engine`]: one [`sync`](engine::sync) of a folder against a remote.

#[cfg(not(any(target_family = "wasm", target_os = "ios")))]
mod curl;
pub mod engine;
mod fake;
mod github;
mod tar;

#[cfg(not(any(target_family = "wasm", target_os = "ios")))]
pub use curl::CurlHttp;
pub use fake::FakeRemote;
pub use github::GitHub;

/// A commit's or blob's id: 40 hex digits.
pub type Sha = String;

/// Why a remote call failed, sorted by what the person can do about it, so
/// the interface can say it in plain words instead of showing HTTP.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// No network, the host could not be found or reached, or it timed out.
    Offline(String),
    /// The sign-in is missing, expired or revoked, or lacks the permission.
    Auth(String),
    /// The repository or what was asked of it is not there (or is hidden from this sign-in).
    NotFound(String),
    /// The branch moved since the commit's parent: pull, merge, push again.
    Rejected,
    /// Too many requests; `reset` is when it lifts (Unix seconds), when known.
    RateLimited { reset: Option<u64> },
    /// The folder on this device could not be read or written.
    Local(String),
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Offline(m) => write!(f, "offline: {m}"),
            Error::Auth(m) => write!(f, "not signed in or not allowed: {m}"),
            Error::NotFound(m) => write!(f, "not found: {m}"),
            Error::Rejected => write!(f, "the branch moved: pull first"),
            Error::RateLimited { reset: Some(t) } => write!(f, "rate limited until {t}"),
            Error::RateLimited { reset: None } => write!(f, "rate limited"),
            Error::Local(m) => write!(f, "this device: {m}"),
            Error::Other(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    /// A header's value, by its name in any case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// HTTP for the remote. `send` fails only when no response came
/// ([`Error::Offline`] / [`Error::Other`]); any status is a response.
pub trait Http {
    fn send(&self, req: Request) -> Result<Response>;
}

/// A file a commit changed (compare `from...to`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub path: String,
    pub status: Status,
    /// The blob at `to`; None when removed.
    pub sha: Option<Sha>,
    /// Renamed: the path it had at `from`.
    pub previous: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status { Added, Modified, Removed, Renamed }

/// A file in a commit's tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub sha: Sha,
    pub size: u64,
}

/// What sync needs of a remote: one branch of one repository, read and
/// moved forward a commit at a time. Paths are relative, with `/`.
pub trait SyncRemote {
    /// The branch's head; None when the branch (or the whole repository) is empty.
    fn head(&self, branch: &str) -> Result<Option<Sha>>;
    /// The files that differ between two commits.
    fn changed(&self, from: &str, to: &str) -> Result<Vec<Change>>;
    /// Every file in a commit (regular files only: no links, no submodules).
    fn tree(&self, commit: &str) -> Result<Vec<Entry>>;
    fn blob(&self, sha: &str) -> Result<Vec<u8>>;
    /// Every file in a commit with its contents, in one go (a new device's first download).
    fn snapshot(&self, commit: &str) -> Result<Vec<(String, Vec<u8>)>>;
    /// A commit on top of `parent` (None: the branch's first) with `changes`
    /// (contents, or None to remove) over the parent's files, and the branch
    /// moved to it without force. [`Error::Rejected`] when the branch is no
    /// longer at `parent`.
    fn commit(&self, branch: &str, parent: Option<&str>, changes: &[(String, Option<Vec<u8>>)], message: &str) -> Result<Sha>;
}

/// Git's id for a file's contents (`git hash-object`), so a file on disk can
/// be compared with the remote's without fetching it.
pub use crate::sync_base::blob_sha;

/// A path from the remote that is safe to write under the folder: relative,
/// no `.` / `..` / empty parts, nothing Windows can't name.
pub fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.split('/').all(|p| {
            !p.is_empty() && p != "." && p != ".." && !p.ends_with(['.', ' '])
                && !p.chars().any(|c| matches!(c, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control())
        })
}

#[cfg(test)]
mod tests;
