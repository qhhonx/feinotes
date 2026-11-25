use super::*;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::VecDeque;

// ------------------------------------------------------------ a recorded GitHub

/// Answers by `METHOD path` (after `/repos/o/r/`), in order; the last answer repeats.
#[derive(Default)]
struct Recorded {
    routes: RefCell<Vec<(String, VecDeque<Response>)>>,
    log: RefCell<Vec<Request>>,
}

const API: &str = "https://ghe.example/api/v3";

fn answer(status: u16, body: Value) -> Response {
    Response { status, headers: vec![("Content-Type".into(), "application/json".into())], body: body.to_string().into_bytes() }
}

impl Recorded {
    fn on(self, route: &str, status: u16, body: Value) -> Self { self.with(route, answer(status, body)) }
    fn with(self, route: &str, r: Response) -> Self {
        {
            let mut routes = self.routes.borrow_mut();
            match routes.iter_mut().find(|(k, _)| k == route) {
                Some((_, q)) => q.push_back(r),
                None => routes.push((route.to_string(), VecDeque::from([r]))),
            }
        }
        self
    }
    /// The requests made to `route`, their JSON bodies.
    fn bodies(&self, route: &str) -> Vec<Value> {
        self.log.borrow().iter().filter(|r| key(r) == route).map(|r| serde_json::from_slice(r.body.as_deref().unwrap_or(b"null")).unwrap()).collect()
    }
}

fn key(r: &Request) -> String {
    let path = r.url.strip_prefix(&format!("{API}/repos/o/r")).unwrap_or(&r.url);
    format!("{} {}", r.method, path.strip_prefix('/').unwrap_or(path))
}

impl Http for &Recorded {
    fn send(&self, req: Request) -> Result<Response> {
        let k = key(&req);
        self.log.borrow_mut().push(req);
        let mut routes = self.routes.borrow_mut();
        let (_, q) = routes.iter_mut().find(|(r, _)| *r == k).unwrap_or_else(|| panic!("no answer recorded for {k}"));
        Ok(if q.len() > 1 { q.pop_front().unwrap() } else { q[0].clone() })
    }
}

fn hub(r: &Recorded) -> GitHub<&Recorded> { GitHub::new(r, &format!("{API}/"), "o/r", "ghs_secret") }

const C1: &str = "1111111111111111111111111111111111111111";
const C2: &str = "2222222222222222222222222222222222222222";

#[test]
fn head_reads_the_branch_and_tells_empty_from_missing() {
    let r = Recorded::default().on("GET git/ref/heads/main", 200, json!({ "ref": "refs/heads/main", "object": { "sha": C1, "type": "commit" } }));
    assert_eq!(hub(&r).head("main"), Ok(Some(C1.to_string())));
    let req = r.log.borrow()[0].clone();
    assert_eq!(req.url, format!("{API}/repos/o/r/git/ref/heads/main"));
    for (k, v) in [("Accept", "application/vnd.github+json"), ("X-GitHub-Api-Version", "2022-11-28"), ("Authorization", "Bearer ghs_secret"), ("User-Agent", "Quiet.md")] {
        assert!(req.headers.iter().any(|(a, b)| a == k && b == v), "{k}: {:?}", req.headers);
    }
    // An empty repository.
    let r = Recorded::default().on("GET git/ref/heads/main", 409, json!({ "message": "Git Repository is empty." }));
    assert_eq!(hub(&r).head("main"), Ok(None));
    // No such branch in a repository that is there.
    let r = Recorded::default().on("GET git/ref/heads/notes%20v2", 404, json!({ "message": "Not Found" })).on("GET ", 200, json!({ "full_name": "o/r" }));
    assert_eq!(hub(&r).head("notes v2"), Ok(None));
    // No such repository (or none this sign-in may see).
    let r = Recorded::default().on("GET git/ref/heads/main", 404, json!({ "message": "Not Found" })).on("GET ", 404, json!({ "message": "Not Found" }));
    assert_eq!(hub(&r).head("main"), Err(Error::NotFound("Not Found".into())));
}

#[test]
fn compare_lists_changes() {
    let files = json!([
        { "filename": "周报.xml", "status": "modified", "sha": "a1" },
        { "filename": "新.md", "status": "added", "sha": "a2" },
        { "filename": "旧.md", "status": "removed", "sha": "a3" },
        { "filename": "b/甲.xml", "status": "renamed", "sha": "a4", "previous_filename": "a/甲.xml" },
    ]);
    let r = Recorded::default().on(&format!("GET compare/{C1}...{C2}"), 200, json!({ "status": "ahead", "files": files }));
    let got = hub(&r).changed(C1, C2).unwrap();
    assert_eq!(got, vec![
        Change { path: "周报.xml".into(), status: Status::Modified, sha: Some("a1".into()), previous: None },
        Change { path: "新.md".into(), status: Status::Added, sha: Some("a2".into()), previous: None },
        Change { path: "旧.md".into(), status: Status::Removed, sha: None, previous: None },
        Change { path: "b/甲.xml".into(), status: Status::Renamed, sha: Some("a4".into()), previous: Some("a/甲.xml".into()) },
    ]);
}

/// Two commits' trees, for the comparisons compare can't make.
fn two_trees(r: Recorded, n: usize) -> Recorded {
    let a: Vec<Value> = (0..n).map(|i| json!({ "path": format!("{i}.xml"), "mode": "100644", "type": "blob", "sha": format!("old{i}"), "size": 1 })).collect();
    let mut b: Vec<Value> = (1..n).map(|i| json!({ "path": format!("{i}.xml"), "mode": "100644", "type": "blob", "sha": if i == 1 { "new1".into() } else { format!("old{i}") }, "size": 1 })).collect();
    b.push(json!({ "path": "加.md", "mode": "100644", "type": "blob", "sha": "add", "size": 1 }));
    r.on(&format!("GET git/commits/{C1}"), 200, json!({ "tree": { "sha": "t1" } }))
        .on(&format!("GET git/commits/{C2}"), 200, json!({ "tree": { "sha": "t2" } }))
        .on("GET git/trees/t1?recursive=1", 200, json!({ "sha": "t1", "tree": a, "truncated": false }))
        .on("GET git/trees/t2?recursive=1", 200, json!({ "sha": "t2", "tree": b, "truncated": false }))
}

#[test]
fn compare_cut_short_compares_trees() {
    // 300 files is as many as compare lists: there may be more.
    let files: Vec<Value> = (0..300).map(|i| json!({ "filename": format!("{i}.xml"), "status": "modified", "sha": "x" })).collect();
    let r = two_trees(Recorded::default().on(&format!("GET compare/{C1}...{C2}"), 200, json!({ "status": "ahead", "files": files })), 400);
    let mut got = hub(&r).changed(C1, C2).unwrap();
    got.sort_by(|a, b| a.path.cmp(&b.path));
    assert_eq!(got, vec![
        Change { path: "0.xml".into(), status: Status::Removed, sha: None, previous: None },
        Change { path: "1.xml".into(), status: Status::Modified, sha: Some("new1".into()), previous: None },
        Change { path: "加.md".into(), status: Status::Added, sha: Some("add".into()), previous: None },
    ]);
    // Diverged (a forced push): compare counts from the merge base, so trees again.
    let r = two_trees(Recorded::default().on(&format!("GET compare/{C1}...{C2}"), 200, json!({ "status": "diverged", "files": [] })), 3);
    assert_eq!(hub(&r).changed(C1, C2).unwrap().len(), 3);
}

#[test]
fn a_tree_cut_short_is_read_a_folder_at_a_time() {
    let r = Recorded::default()
        .on(&format!("GET git/commits/{C1}"), 200, json!({ "tree": { "sha": "root" } }))
        .on("GET git/trees/root?recursive=1", 200, json!({ "sha": "root", "tree": [], "truncated": true }))
        .on("GET git/trees/root", 200, json!({ "sha": "root", "tree": [
            { "path": "a.xml", "mode": "100644", "type": "blob", "sha": "s1", "size": 5 },
            { "path": "run.sh", "mode": "100755", "type": "blob", "sha": "s2", "size": 6 },
            { "path": "link", "mode": "120000", "type": "blob", "sha": "s3", "size": 1 },
            { "path": "sub", "mode": "160000", "type": "commit", "sha": "s4" },
            { "path": "方案", "mode": "040000", "type": "tree", "sha": "t" },
        ] }))
        .on("GET git/trees/t", 200, json!({ "sha": "t", "tree": [{ "path": "方案.xml", "mode": "100644", "type": "blob", "sha": "s5", "size": 7 }] }));
    assert_eq!(hub(&r).tree(C1).unwrap(), vec![
        Entry { path: "a.xml".into(), sha: "s1".into(), size: 5 },
        Entry { path: "run.sh".into(), sha: "s2".into(), size: 6 },
        Entry { path: "方案/方案.xml".into(), sha: "s5".into(), size: 7 },
    ]);
}

#[test]
fn blobs_come_base64_in_lines() {
    let r = Recorded::default().on("GET git/blobs/b1", 200, json!({ "content": "5ZGo5oql\nCg==\n", "encoding": "base64", "size": 7 }));
    assert_eq!(hub(&r).blob("b1").unwrap(), "周报\n".as_bytes());
}

/// A commit's calls up to moving the branch.
fn committing(r: Recorded) -> Recorded {
    r.on("POST git/blobs", 201, json!({ "sha": "blob1" }))
        .on(&format!("GET git/commits/{C1}"), 200, json!({ "tree": { "sha": "t1" } }))
        .on("POST git/trees", 201, json!({ "sha": "t2" }))
        .on("POST git/commits", 201, json!({ "sha": C2 }))
}

#[test]
fn a_commit_goes_on_top_of_its_parent() {
    let r = committing(Recorded::default()).on("PATCH git/refs/heads/main", 200, json!({ "object": { "sha": C2 } }));
    let changes = vec![("周报.xml".to_string(), Some(b"<p>x</p>".to_vec())), ("旧.md".to_string(), None)];
    assert_eq!(hub(&r).commit("main", Some(C1), &changes, "周报 (MacBook)"), Ok(C2.to_string()));
    assert_eq!(r.bodies("POST git/blobs"), vec![json!({ "content": "PHA+eDwvcD4=", "encoding": "base64" })]);
    assert_eq!(r.bodies("POST git/trees"), vec![json!({ "base_tree": "t1", "tree": [
        { "path": "周报.xml", "mode": "100644", "type": "blob", "sha": "blob1" },
        { "path": "旧.md", "mode": "100644", "type": "blob", "sha": null },
    ] })]);
    assert_eq!(r.bodies("POST git/commits"), vec![json!({ "message": "周报 (MacBook)", "tree": "t2", "parents": [C1] })]);
    assert_eq!(r.bodies("PATCH git/refs/heads/main"), vec![json!({ "sha": C2, "force": false })]);
}

#[test]
fn a_moved_branch_refuses_the_commit() {
    let r = committing(Recorded::default()).on("PATCH git/refs/heads/main", 422, json!({ "message": "Update is not a fast forward" }));
    let changes = vec![("周报.xml".to_string(), Some(b"x".to_vec()))];
    assert_eq!(hub(&r).commit("main", Some(C1), &changes, "m"), Err(Error::Rejected));
    // A first commit on a branch someone just made.
    let r = Recorded::default().on("POST git/blobs", 201, json!({ "sha": "blob1" })).on("POST git/trees", 201, json!({ "sha": "t2" }))
        .on("POST git/commits", 201, json!({ "sha": C2 })).on("POST git/refs", 422, json!({ "message": "Reference already exists" }));
    assert_eq!(hub(&r).commit("main", None, &changes, "m"), Err(Error::Rejected));
    assert_eq!(r.bodies("POST git/refs"), vec![json!({ "ref": "refs/heads/main", "sha": C2 })]);
    assert_eq!(r.bodies("POST git/trees"), vec![json!({ "tree": [{ "path": "周报.xml", "mode": "100644", "type": "blob", "sha": "blob1" }] })]);
}

#[test]
fn an_empty_repository_starts_through_the_contents_api() {
    let empty = json!({ "message": "Git Repository is empty." });
    let r = Recorded::default()
        .on("POST git/blobs", 409, empty.clone())
        .on("POST git/blobs", 201, json!({ "sha": "blob2" }))
        .on("PUT contents/a/%E5%91%A8%E6%8A%A5.xml", 201, json!({ "commit": { "sha": C1, "parents": [] } }))
        .on(&format!("GET git/commits/{C1}"), 200, json!({ "tree": { "sha": "t1" } }))
        .on("POST git/trees", 201, json!({ "sha": "t2" }))
        .on("POST git/commits", 201, json!({ "sha": C2 }))
        .on("PATCH git/refs/heads/main", 200, json!({}));
    let changes = vec![("a/周报.xml".to_string(), Some(b"one".to_vec())), ("b.md".to_string(), Some(b"two".to_vec()))];
    assert_eq!(hub(&r).commit("main", None, &changes, "开始同步"), Ok(C2.to_string()));
    assert_eq!(r.bodies("PUT contents/a/%E5%91%A8%E6%8A%A5.xml"), vec![json!({ "message": "开始同步", "content": "b25l", "branch": "main" })]);
    assert_eq!(r.bodies("POST git/commits"), vec![json!({ "message": "开始同步", "tree": "t2", "parents": [C1] })]);
    // Only one file: the contents API's commit is all.
    let r = Recorded::default().on("POST git/blobs", 409, empty.clone())
        .on("PUT contents/b.md", 201, json!({ "commit": { "sha": C1, "parents": [] } }));
    assert_eq!(hub(&r).commit("main", None, &changes[1..], "m"), Ok(C1.to_string()));
    // Someone else's first commit got there before: pull.
    let r = Recorded::default().on("POST git/blobs", 409, empty)
        .on("PUT contents/b.md", 201, json!({ "commit": { "sha": C2, "parents": [{ "sha": C1 }] } }));
    assert_eq!(hub(&r).commit("main", None, &changes[1..], "m"), Err(Error::Rejected));
}

#[test]
fn statuses_say_what_went_wrong() {
    let with = |status, headers: &[(&str, &str)], body: Value| {
        let mut a = answer(status, body);
        a.headers.extend(headers.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        Recorded::default().with("GET git/blobs/b", a)
    };
    let r = with(401, &[], json!({ "message": "Bad credentials" }));
    assert_eq!(hub(&r).blob("b"), Err(Error::Auth("Bad credentials".into())));
    let r = with(403, &[("x-ratelimit-remaining", "0"), ("x-ratelimit-reset", "1764000000")], json!({ "message": "API rate limit exceeded for user ID 1." }));
    assert_eq!(hub(&r).blob("b"), Err(Error::RateLimited { reset: Some(1764000000) }));
    let r = with(429, &[("Retry-After", "60")], json!({ "message": "You have exceeded a secondary rate limit." }));
    assert!(matches!(hub(&r).blob("b"), Err(Error::RateLimited { reset: Some(t) }) if t > 1_700_000_000));
    let r = with(403, &[("x-ratelimit-remaining", "4999")], json!({ "message": "Resource not accessible by integration" }));
    assert_eq!(hub(&r).blob("b"), Err(Error::Auth("Resource not accessible by integration".into())));
    let r = with(502, &[], json!({ "message": "Server Error" }));
    assert_eq!(hub(&r).blob("b"), Err(Error::Other("502 Server Error".into())));
}

#[test]
fn a_snapshot_is_the_tarball_unpacked() {
    let long = format!("{}/很长的文件夹名字/{}.xml", "子".repeat(20), "文".repeat(40));
    let deep = format!("{}/x.md", ["folder"; 30].join("/"));
    let files: Vec<(&str, &[u8])> = vec![("周报.xml", b"<p>1</p>"), (".assets/a.png", &[0, 1, 2, 255]), (&long, b"long"), (&deep, b"deep"), ("empty.md", b"")];
    let gz = tar::gzip(&tar::build("o-r-2222222", &files));
    let r = Recorded::default().with(&format!("GET tarball/{C2}"), Response { status: 200, headers: vec![], body: gz });
    let got = hub(&r).snapshot(C2).unwrap();
    assert_eq!(got, files.iter().map(|(p, b)| (p.to_string(), b.to_vec())).collect::<Vec<_>>());
}

#[test]
fn unsafe_paths_are_not_written() {
    for p in ["a.xml", "方案/方案.xml", ".assets/x.png", ".quiet/meta/d.json"] { assert!(safe_path(p), "{p}") }
    for p in ["", "/etc/passwd", "../x.xml", "a/../../x", "a//b", "./a", "a\\b.xml", "c:x.xml", "a?.md", "trail. /x.md", "x.md."] { assert!(!safe_path(p), "{p}") }
}

// ------------------------------------------------------------ curl

#[cfg(not(any(target_family = "wasm", target_os = "ios")))]
mod with_curl {
    use super::super::curl::{config_for_test, failure_for_test, parse_for_test};
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn the_token_goes_in_on_stdin() {
        let req = Request {
            method: "POST".into(),
            url: "https://api.github.com/repos/o/r/git/blobs".into(),
            headers: vec![("Authorization".into(), "Bearer ghs_secret".into()), ("X-Odd".into(), "a\"b\\c\nd".into())],
            body: Some(b"{}".to_vec()),
        };
        let (args, config) = config_for_test(&req);
        assert_eq!(args, ["--config", "-"]);
        assert!(config.contains("header = \"Authorization: Bearer ghs_secret\"\n"), "{config}");
        // Quotes and backslashes escaped; a line break can't smuggle in another option.
        assert!(config.contains("header = \"X-Odd: a\\\"b\\\\cd\"\n"), "{config}");
        assert!(config.contains("request = \"POST\"\n") && config.contains("data-binary = \"@/tmp/b\"\n") && config.contains("proto = \"=https\"\n"), "{config}");
    }

    #[test]
    fn the_last_response_after_redirects() {
        let dump = "HTTP/1.1 302 Found\r\nLocation: https://codeload.github.com/x\r\n\r\nHTTP/2 200 \r\ncontent-type: application/x-gzip\r\nx-ratelimit-remaining: 4999\r\n\r\n";
        let (status, headers) = parse_for_test(dump).unwrap();
        assert_eq!(status, 200);
        assert_eq!(headers, vec![("content-type".into(), "application/x-gzip".into()), ("x-ratelimit-remaining".into(), "4999".into())]);
        assert!(matches!(failure_for_test(6), Error::Offline(_)) && matches!(failure_for_test(28), Error::Offline(_)) && matches!(failure_for_test(3), Error::Other(_)));
    }

    /// curl itself, against a server here: a redirect followed, the body sent,
    /// the status and headers of the last response read back; and a closed
    /// port is offline.
    #[test]
    fn curl_round_trip() {
        let Ok(l) = std::net::TcpListener::bind("127.0.0.1:0") else { return };
        let port = l.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let mut got = vec![];
            for (i, s) in l.incoming().take(2).enumerate() {
                let mut s = s.unwrap();
                let mut req = vec![];
                let mut buf = [0; 4096];
                // Head, then as much body as Content-Length says.
                loop {
                    let n = s.read(&mut buf).unwrap();
                    req.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&req).to_string();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let len = text.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap())).unwrap_or(0);
                        if req.len() >= end + 4 + len || n == 0 { break }
                    }
                    if n == 0 { break }
                }
                got.push(String::from_utf8_lossy(&req).to_string());
                let reply = if i == 0 { format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: http://127.0.0.1:{port}/next\r\nContent-Length: 0\r\n\r\n") }
                    else { "HTTP/1.1 422 Unprocessable\r\nX-Ratelimit-Remaining: 7\r\nContent-Length: 9\r\nConnection: close\r\n\r\n{\"a\":\"b\"}".to_string() };
                s.write_all(reply.as_bytes()).unwrap();
            }
            got
        });
        // Its own temporary folder: other test processes use the system's at the same time.
        let temp = crate::vfs::temp_dir().join(format!("quiet-curl-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&temp).unwrap();
        let http = CurlHttp::plain_in(&temp);
        let r = http.send(Request {
            method: "PATCH".into(),
            url: format!("http://127.0.0.1:{port}/first"),
            headers: vec![("Authorization".into(), "Bearer t0ken".into())],
            body: Some("{\"周\":1}".as_bytes().to_vec()),
        });
        let got = server.join().unwrap();
        let r = r.unwrap();
        assert_eq!((r.status, r.header("x-ratelimit-remaining"), r.body.as_slice()), (422, Some("7"), b"{\"a\":\"b\"}".as_slice()));
        assert!(got[0].starts_with("PATCH /first ") && got[0].contains("Authorization: Bearer t0ken") && got[0].ends_with("{\"周\":1}"), "{}", got[0]);
        // 307 keeps the method and body.
        assert!(got[1].starts_with("PATCH /next "), "{}", got[1]);
        // Nothing left behind in the temporary folder, after a failure either.
        let left = || std::fs::read_dir(&temp).unwrap().count();
        assert_eq!(left(), 0);

        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let r = http.send(Request { method: "GET".into(), url: format!("http://127.0.0.1:{closed}/"), body: Some(b"x".to_vec()), ..Default::default() });
        assert!(matches!(r, Err(Error::Offline(_))), "{r:?}");
        assert_eq!(left(), 0);
        // The files really go there: with the folder gone, the request can't start.
        std::fs::remove_dir(&temp).unwrap();
        let r = http.send(Request { method: "GET".into(), url: format!("http://127.0.0.1:{closed}/"), ..Default::default() });
        assert!(matches!(r, Err(Error::Local(_))), "{r:?}");
    }

    /// Against a real repository (set QUIET_MD_GITHUB_TOKEN and
    /// QUIET_MD_GITHUB_REPO=owner/name, a repository made for this: it gets commits).
    #[test]
    #[ignore]
    fn real_github() {
        let (Ok(token), Ok(repo)) = (std::env::var("QUIET_MD_GITHUB_TOKEN"), std::env::var("QUIET_MD_GITHUB_REPO")) else { return };
        let gh = GitHub::github_com(CurlHttp::default(), &repo, &token);
        let head = gh.head("main").unwrap();
        let name = format!("sync-test/{}.md", uuid::Uuid::new_v4());
        let c = gh.commit("main", head.as_deref(), &[(name.clone(), Some(b"# test\n".to_vec()))], "Quiet.md sync test").unwrap();
        assert_eq!(gh.head("main").unwrap(), Some(c.clone()));
        let tree = gh.tree(&c).unwrap();
        let e = tree.iter().find(|e| e.path == name).unwrap();
        assert_eq!(gh.blob(&e.sha).unwrap(), b"# test\n");
        assert!(gh.snapshot(&c).unwrap().iter().any(|(p, b)| *p == name && b == b"# test\n"));
        if let Some(h) = head { assert!(gh.changed(&h, &c).unwrap().iter().any(|ch| ch.path == name && ch.status == Status::Added)) }
        let c2 = gh.commit("main", Some(&c), &[(name.clone(), None)], "Quiet.md sync test: clean up").unwrap();
        assert_eq!(gh.commit("main", Some(&c), &[(name, None)], "stale"), Err(Error::Rejected));
        assert_eq!(gh.head("main").unwrap(), Some(c2));
    }
}
