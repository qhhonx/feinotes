//! [`Http`] through the system curl, as the app already does for updates and
//! fonts. Everything about the request goes in on stdin as a curl config
//! (`--config -`), so the token never shows in the process list; the body goes
//! through a temporary file only this user can read, and so do the headers
//! that come back.

use super::{Error, Http, Request, Response, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub struct CurlHttp {
    /// A proxy (`-x`) and the hosts that skip it (`--noproxy`): the system's,
    /// which curl doesn't read itself (the app's `update::system_proxy`).
    pub proxy: Option<(String, String)>,
    /// The longest a whole request may take, in seconds (a tarball takes long).
    pub max_time: u32,
    /// `http://` allowed too (tests against a local server).
    plain: bool,
    /// Where the body and header files go; none: the system's temporary folder.
    temp: Option<PathBuf>,
}

impl Default for CurlHttp {
    fn default() -> Self { CurlHttp { proxy: None, max_time: 600, plain: false, temp: None } }
}

impl Http for CurlHttp {
    fn send(&self, req: Request) -> Result<Response> {
        let dir = self.temp.clone().unwrap_or_else(crate::vfs::temp_dir);
        let id = uuid::Uuid::new_v4();
        let (body, head) = (dir.join(format!("quiet-curl-{id}.body")), dir.join(format!("quiet-curl-{id}.head")));
        let out = (|| {
            if let Some(b) = &req.body { private_file(&body, b)? }
            private_file(&head, b"")?;
            let config = config(&req, req.body.is_some().then_some(&*body), &head, self);
            let mut child = self.command().spawn().map_err(|e| Error::Other(format!("curl: {e}")))?;
            child.stdin.take().expect("piped").write_all(config.as_bytes()).map_err(|e| Error::Other(format!("curl: {e}")))?;
            let out = child.wait_with_output().map_err(|e| Error::Other(format!("curl: {e}")))?;
            let headers = std::fs::read(&head).unwrap_or_default();
            Ok((out, headers))
        })();
        let _ = std::fs::remove_file(&body);
        let _ = std::fs::remove_file(&head);
        let (out, headers) = out?;
        if !out.status.success() {
            return Err(failure(out.status.code(), String::from_utf8_lossy(&out.stderr).trim()));
        }
        let (status, headers) = last_response(&String::from_utf8_lossy(&headers)).ok_or_else(|| Error::Other("curl: no response".into()))?;
        Ok(Response { status, headers, body: out.stdout })
    }
}

impl CurlHttp {
    #[cfg(test)]
    pub(super) fn plain_in(temp: &Path) -> Self { CurlHttp { plain: true, temp: Some(temp.to_path_buf()), ..Default::default() } }

    /// curl with nothing on its command line but where to read the rest.
    fn command(&self) -> Command {
        #[allow(unused_mut)]
        let mut c = Command::new("curl");
        #[cfg(target_os = "windows")]
        { use std::os::windows::process::CommandExt; c.creation_flags(0x0800_0000); }
        c.args(["--config", "-"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        c
    }
}

/// A file for the body or the headers, readable by this user only.
fn private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    { use std::os::unix::fs::OpenOptionsExt; o.mode(0o600); }
    o.open(path).and_then(|mut f| f.write_all(bytes)).map_err(|e| Error::Local(format!("{}: {e}", path.display())))
}

/// The curl config for one request (`curl --config -`). No `--fail`: every
/// status comes back, with its body (GitHub explains errors there).
fn config(req: &Request, body: Option<&Path>, head: &Path, c: &CurlHttp) -> String {
    let proto = if c.plain { "=https,http" } else { "=https" };
    let mut lines = vec![
        "silent".to_string(),
        "show-error".to_string(),
        "location".to_string(),
        format!("proto = {}", quote(proto)),
        format!("proto-redir = {}", quote(proto)),
        "connect-timeout = 15".to_string(),
        format!("max-time = {}", c.max_time),
        format!("request = {}", quote(&req.method)),
        format!("url = {}", quote(&req.url)),
        format!("dump-header = {}", quote(&head.to_string_lossy())),
    ];
    for (k, v) in &req.headers { lines.push(format!("header = {}", quote(&format!("{k}: {v}")))) }
    if let Some(b) = body { lines.push(format!("data-binary = {}", quote(&format!("@{}", b.display())))) }
    if let Some((proxy, bypass)) = &c.proxy {
        lines.push(format!("proxy = {}", quote(proxy)));
        lines.push(format!("noproxy = {}", quote(bypass)));
    }
    lines.join("\n") + "\n"
}

/// A config value in quotes; a line break can't be in a header, so it goes.
fn quote(s: &str) -> String {
    let mut q = String::from('"');
    for c in s.chars() {
        match c {
            '"' => q.push_str("\\\""),
            '\\' => q.push_str("\\\\"),
            '\r' | '\n' => {}
            c => q.push(c),
        }
    }
    q.push('"');
    q
}

/// The status and headers of the last response in a header dump (redirects
/// come first, each its own block).
fn last_response(dump: &str) -> Option<(u16, Vec<(String, String)>)> {
    let mut last = None;
    for line in dump.lines().map(str::trim_end) {
        if line.starts_with("HTTP/") {
            let status = line.split_whitespace().nth(1)?.parse().ok()?;
            last = Some((status, vec![]));
        } else if let (Some((_, headers)), Some((k, v))) = (last.as_mut(), line.split_once(':')) {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    last
}

/// curl's exit code in the terms sync uses (`man curl`, EXIT CODES).
fn failure(code: Option<i32>, msg: &str) -> Error {
    match code {
        // Couldn't resolve the proxy / host, connect, timed out, TLS handshake,
        // send / receive, empty reply.
        Some(5 | 6 | 7 | 28 | 35 | 52 | 55 | 56) => Error::Offline(msg.to_string()),
        Some(n) => Error::Other(format!("curl {n}: {msg}")),
        None => Error::Other(format!("curl: {msg}")),
    }
}

#[cfg(test)]
pub(super) fn config_for_test(req: &Request) -> (Vec<String>, String) {
    let c = CurlHttp::default();
    let args = c.command().get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    (args, config(req, Some(Path::new("/tmp/b")), Path::new("/tmp/h"), &c))
}

#[cfg(test)]
pub(super) fn parse_for_test(dump: &str) -> Option<(u16, Vec<(String, String)>)> { last_response(dump) }

#[cfg(test)]
pub(super) fn failure_for_test(code: i32) -> Error { failure(Some(code), "x") }
