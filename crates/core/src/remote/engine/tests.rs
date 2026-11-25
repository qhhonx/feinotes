use super::*;
use crate::remote::FakeRemote;
use std::cell::RefCell;

// ------------------------------------------------------------ rules

#[test]
fn only_quiet_mds_files_are_synced() {
    for p in [
        "周报.xml", "a/b/提案.md", "白板.excalidraw", "表.sqlite", "README.MD", ".gitignore",
        ".assets/a1b2.png", ".assets/x/y.pdf", ".quiet/format.json", ".quiet/meta/device-1.json",
        ".quiet/meta/device-1/device.json", ".quiet/meta/device-1/3f.json", ".quiet/meta/device-1/07 2.json",
        ".quiet/trash/周报.xml", ".quiet/trash/a/b.png", ".quiet/themes/暖色.json",
    ] { assert!(synced(p), "{p}") }
    for p in [
        ".quiet/history/周报/1.xml", ".quiet/history-trash/x.xml", ".quiet/index.json", ".quiet/settings.json", ".quiet/.lock",
        ".quiet/meta/x.txt", ".quiet/trash", ".assets/thumbs/a.png", ".assets", "表.sqlite-journal", "周报.xml.1234.tmp",
        ".DS_Store", "a/.DS_Store", "Thumbs.db", ".git/config", ".obsidian/x.md", "a/.hidden/x.xml", "photo.png", "notes.txt",
        "a/.gitignore", "x.lock", ".quiet/trash/index.json", ".quiet/trash/index.json.v5",
        ".quiet/meta/device-1.json.single", ".quiet/meta/device-1.tmp/07.json", ".quiet/meta/device-1.tmp/device.json",
        ".quiet/meta/device-1/07.json.bak", ".quiet/meta/device-1/07.json.1234.tmp", ".quiet/meta/device-1/x/07.json",
    ] { assert!(!synced(p), "{p}") }
    // A conversion's folder is not even looked into.
    assert!(skipped_dir(".quiet/meta/device-1.tmp") && !skipped_dir(".quiet/meta/device-1"));
}

#[test]
fn gitignore_keeps_the_persons_rules() {
    let block = gitignore("");
    assert!(block.starts_with("# >>> Quiet.md") && block.ends_with("# <<< Quiet.md\n") && block.contains("\n.quiet/history/\n"), "{block}");
    // Added after what is there.
    assert_eq!(gitignore("node_modules/"), format!("node_modules/\n\n{block}"));
    assert_eq!(gitignore("a\n"), format!("a\n\n{block}"));
    // An old part replaced, the rest left as it was.
    let old = "a\n# >>> Quiet.md（旧）\nold-rule\n# <<< Quiet.md\nb\n";
    assert_eq!(gitignore(old), format!("a\n{block}b\n"));
    assert_eq!(gitignore(&gitignore(old)), gitignore(old));
    // An end marker gone: the part ran to the end.
    assert_eq!(gitignore("a\n# >>> Quiet.md\nold\n"), format!("a\n{block}"));
}

#[test]
fn titles_of_the_documents() {
    let paths = ["方案/方案.xml", "周报.xml", ".assets/a.png", "周报.xml", ".quiet/meta/d.json", "白板.excalidraw"].map(String::from);
    assert_eq!(titles(&paths), ["方案", "周报", "白板"]);
}

// ------------------------------------------------------------ two devices

/// A device: its library folder and the folder its sync state lives in.
struct Device { name: &'static str, root: PathBuf, state: PathBuf }

fn temp(what: &str) -> PathBuf {
    let p = vfs::temp_dir().join(format!("quiet-sync-{what}-{}", uuid::Uuid::new_v4()));
    vfs::create_dir_all(&p).unwrap();
    p
}

fn copy_name(name: &str, path: &str) -> String {
    match path.rsplit_once('.') { Some((s, e)) => format!("{s}（冲突 · {name}）.{e}"), None => format!("{path}（冲突 · {name}）") }
}

impl Device {
    fn new(name: &'static str) -> Self { Device { name, root: temp(name), state: temp(&format!("{name}-state")) } }
    fn folder(&self) -> Dir { Dir(self.root.clone()) }
    fn write(&self, p: &str, s: &str) { self.folder().write(p, s.as_bytes()).unwrap() }
    fn read(&self, p: &str) -> Option<String> { vfs::read(self.root.join(p)).ok().map(|b| String::from_utf8(b).unwrap()) }
    fn remove(&self, p: &str) { self.folder().remove(p).unwrap() }
    fn files(&self) -> BTreeMap<String, String> {
        let f = self.folder();
        f.list().unwrap().into_iter().map(|l| { let s = String::from_utf8(f.read(&l.path).unwrap()).unwrap(); (l.path, s) }).collect()
    }
    fn state(&self) -> State { State::load(&self.state).unwrap() }
    fn sync_with(&self, remote: &FakeRemote, edit: impl FnOnce(&mut Options)) -> Result<Outcome> { self.sync_by(remote, &KeepBoth, edit) }
    fn sync_by(&self, remote: &FakeRemote, merger: &dyn Merger, edit: impl FnOnce(&mut Options)) -> Result<Outcome> {
        if !self.state.join(STATE).fs_exists() { State::new("o/r", "main").save(&self.state).unwrap() }
        let copy = |p: &str| copy_name(self.name, p);
        let message = |paths: &[String]| format!("{} ({})", titles(paths).join("、"), self.name);
        let mut o = Options::new(&copy, &message);
        o.merger = merger;
        o.first = First::Merge;
        edit(&mut o);
        sync(&self.folder(), remote, &self.state, &o)
    }
    fn sync(&self, remote: &FakeRemote) -> Outcome { self.sync_with(remote, |_| {}).unwrap() }
}

impl Drop for Device {
    fn drop(&mut self) {
        let _ = vfs::remove_dir_all(&self.root);
        let _ = vfs::remove_dir_all(&self.state);
    }
}

fn remote_text(r: &FakeRemote) -> BTreeMap<String, String> {
    r.files("main").into_iter().map(|(p, b)| (p, String::from_utf8(b).unwrap())).collect()
}

fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> { pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect() }

#[test]
fn edits_to_different_files_meet() {
    let (r, a, b) = (FakeRemote::new(), Device::new("MacBook"), Device::new("iPad"));
    a.write("周报.xml", "1");
    a.write("提案.xml", "1");
    let o = a.sync(&r);
    assert_eq!((o.uploaded.len(), o.commit.is_some()), (2, true));
    assert_eq!(b.sync(&r).downloaded, ["周报.xml", "提案.xml"].map(String::from));
    a.write("周报.xml", "2");
    b.write("提案.xml", "2");
    b.write("新/笔记.md", "new");
    a.sync(&r);
    let o = b.sync(&r);
    assert_eq!((o.downloaded.clone(), o.uploaded.clone()), (vec!["周报.xml".to_string()], vec!["提案.xml".to_string(), "新/笔记.md".to_string()]));
    a.sync(&r);
    let want = map(&[("周报.xml", "2"), ("提案.xml", "2"), ("新/笔记.md", "new")]);
    assert_eq!((a.files(), b.files(), remote_text(&r)), (want.clone(), want.clone(), want));
    // One commit per sync that had something to push, named after its documents.
    assert_eq!(r.messages(), ["周报、提案 (MacBook)", "周报 (MacBook)", "提案、笔记 (iPad)"]);
    // Nothing new: nothing pushed, nothing fetched beyond the head.
    let calls = r.calls();
    assert_eq!(a.sync(&r), Outcome::default());
    assert_eq!(r.calls(), calls + 1);
}

#[test]
fn both_changing_a_file_leaves_a_conflict_copy_here() {
    let (r, a, b) = (FakeRemote::new(), Device::new("MacBook"), Device::new("iPad"));
    a.write("周报.xml", "base");
    a.sync(&r);
    b.sync(&r);
    a.write("周报.xml", "mac");
    b.write("周报.xml", "ipad");
    a.sync(&r);
    let o = b.sync(&r);
    let copy = "周报（冲突 · iPad）.xml".to_string();
    assert_eq!(o.conflicts, vec![("周报.xml".to_string(), copy.clone(), 1)]);
    // Ours stays and goes up; theirs is the copy, here only.
    assert_eq!(b.files(), map(&[("周报.xml", "ipad"), ("周报（冲突 · iPad）.xml", "mac")]));
    assert_eq!(remote_text(&r), map(&[("周报.xml", "ipad")]));
    assert!(b.state().held.contains(&copy));
    b.sync(&r);
    assert_eq!(remote_text(&r), map(&[("周报.xml", "ipad")]));
    // Kept as a document of its own: it goes up like any other.
    release(&b.state, &copy).unwrap();
    assert_eq!(b.sync(&r).uploaded, vec![copy.clone()]);
    a.sync(&r);
    assert_eq!(a.files(), b.files());
    // A second conflict on the same file takes the next free name.
    a.write("周报.xml", "mac2");
    b.write("周报.xml", "ipad2");
    a.sync(&r);
    assert_eq!(b.sync(&r).conflicts, vec![("周报.xml".to_string(), "周报（冲突 · iPad） (2).xml".to_string(), 1)]);
    // A copy the person deleted is no longer held.
    b.remove("周报（冲突 · iPad） (2).xml");
    b.sync(&r);
    assert!(b.state().held.is_empty());
}

#[test]
fn a_change_outlives_a_removal() {
    let (r, a, b) = (FakeRemote::new(), Device::new("MacBook"), Device::new("iPad"));
    a.write("x.xml", "1");
    a.write("y.xml", "1");
    a.sync(&r);
    b.sync(&r);
    // Removed here first, changed there: the change comes back.
    a.remove("x.xml");
    b.write("x.xml", "2");
    a.sync(&r);
    b.sync(&r);
    a.sync(&r);
    // Changed here first, removed there: the change is pushed back up.
    a.write("y.xml", "2");
    b.remove("y.xml");
    a.sync(&r);
    b.sync(&r);
    a.sync(&r);
    let want = map(&[("x.xml", "2"), ("y.xml", "2")]);
    assert_eq!((a.files(), b.files(), remote_text(&r)), (want.clone(), want.clone(), want));
    // A plain removal goes through, folder and all.
    b.write("f/z.xml", "z");
    b.sync(&r);
    a.sync(&r);
    a.remove("f/z.xml");
    a.sync(&r);
    b.sync(&r);
    assert!(!b.root.join("f").exists() && !remote_text(&r).contains_key("f/z.xml"));
}

#[test]
fn a_rename_is_a_removal_and_an_addition() {
    let (r, a, b) = (FakeRemote::new(), Device::new("MacBook"), Device::new("iPad"));
    a.write("旧/甲.xml", "甲");
    a.sync(&r);
    b.sync(&r);
    vfs::create_dir_all(a.root.join("新")).unwrap();
    vfs::rename(a.root.join("旧/甲.xml"), a.root.join("新/甲.xml")).unwrap();
    vfs::remove_dir(a.root.join("旧")).unwrap();
    assert_eq!(a.sync(&r).uploaded, ["新/甲.xml", "旧/甲.xml"].map(String::from));
    b.sync(&r);
    assert_eq!(b.files(), map(&[("新/甲.xml", "甲")]));
}

#[test]
fn pushed_first_elsewhere_pulls_and_tries_again() {
    let (r, a) = (FakeRemote::new(), Device::new("MacBook"));
    a.write("周报.xml", "1");
    a.sync(&r);
    a.write("周报.xml", "2");
    r.race("main", vec![("提案.xml".into(), Some(b"iPad".to_vec()))]);
    let o = a.sync(&r);
    assert_eq!((o.uploaded, o.downloaded), (vec!["周报.xml".to_string()], vec!["提案.xml".to_string()]));
    assert_eq!(remote_text(&r), map(&[("周报.xml", "2"), ("提案.xml", "iPad")]));
    assert_eq!(r.messages(), ["周报 (MacBook)", "another device", "周报 (MacBook)"]);
    // The same file changed there in the meantime: a conflict, nothing lost.
    a.write("周报.xml", "3");
    r.race("main", vec![("周报.xml".into(), Some(b"iPad".to_vec()))]);
    let o = a.sync(&r);
    assert_eq!(o.conflicts.len(), 1);
    assert_eq!(a.read("周报（冲突 · MacBook）.xml").as_deref(), Some("iPad"));
    assert_eq!(remote_text(&r)["周报.xml"], "3");
    // It keeps happening: give up after the retries, with the edits still here.
    a.write("周报.xml", "4");
    for i in 0..5 { r.race("main", vec![(format!("{i}.xml"), Some(b"x".to_vec()))]) }
    // They all land just before the next commit, refusing it.
    let fake_retries = |o: &mut Options| o.retries = 0;
    assert_eq!(a.sync_with(&r, fake_retries), Err(Error::Rejected));
    assert_eq!(a.read("周报.xml").as_deref(), Some("4"));
    a.sync(&r);
    assert_eq!(remote_text(&r)["周报.xml"], "4");
}

#[test]
fn offline_edits_go_up_together_later() {
    let (r, a) = (FakeRemote::new(), Device::new("MacBook"));
    a.write("周报.xml", "1");
    a.sync(&r);
    r.set_offline(true);
    a.write("周报.xml", "2");
    a.write("提案.xml", "1");
    let before = a.state();
    assert!(matches!(a.sync_with(&r, |_| {}), Err(Error::Offline(_))));
    a.write("笔记.md", "1");
    assert!(matches!(a.sync_with(&r, |_| {}), Err(Error::Offline(_))));
    assert_eq!(a.state().commit, before.commit);
    r.set_offline(false);
    let o = a.sync(&r);
    assert_eq!(o.uploaded.len(), 3);
    assert_eq!(r.messages().len(), 2);
}

#[test]
fn first_sync_three_ways() {
    // The remote is empty: up it all goes.
    let (r, a) = (FakeRemote::new(), Device::new("MacBook"));
    a.write("周报.xml", "1");
    a.write(".assets/a.png", "png");
    assert_eq!(a.sync_with(&r, |o| o.first = First::Ask).unwrap().uploaded.len(), 2);
    // This side is empty (but for files sync leaves alone): down it all comes.
    let b = Device::new("iPad");
    b.write(".DS_Store", "x");
    let o = b.sync_with(&r, |o| o.first = First::Ask).unwrap();
    assert_eq!((o.downloaded.len(), o.needs_choice), (2, false));
    assert_eq!(b.read("周报.xml").as_deref(), Some("1"));
    // Both have files: ask, doing nothing.
    let c = Device::new("Windows");
    c.write("周报.xml", "other");
    c.write("自己的.md", "mine");
    let o = c.sync_with(&r, |o| o.first = First::Ask).unwrap();
    assert!(o.needs_choice && o.commit.is_none() && o.downloaded.is_empty());
    assert_eq!(c.files().len(), 2);
    assert_eq!(c.state().commit, None);
    // Told to merge: both kept, the one both have differently as a conflict copy.
    let o = c.sync(&r);
    assert_eq!(o.conflicts, vec![("周报.xml".to_string(), "周报（冲突 · Windows）.xml".to_string(), 1)]);
    assert_eq!(c.files(), map(&[("周报.xml", "other"), ("周报（冲突 · Windows）.xml", "1"), ("自己的.md", "mine"), (".assets/a.png", "png")]));
    assert_eq!(remote_text(&r), map(&[("周报.xml", "other"), ("自己的.md", "mine"), (".assets/a.png", "png")]));
}

/// FORMAT 6: each device's shards go both ways, a shard per file as any
/// other; the copy a conversion keeps and its half-made folder stay here.
#[test]
fn metadata_shards_sync_and_the_conversions_leftovers_stay() {
    let (r, a, b) = (FakeRemote::new(), Device::new("MacBook"), Device::new("iPad"));
    a.write(".quiet/format.json", "{\"format\":6}");
    a.write(".quiet/meta/aaaa/device.json", "{\"id\":\"aaaa\"}");
    a.write(".quiet/meta/aaaa/07.json", "{\"v\":1}");
    a.write(".quiet/meta/aaaa/07.json.bak", "old");
    a.write(".quiet/meta/aaaa.json.single", "whole");
    a.write(".quiet/meta/aaaa.tmp/07.json", "half");
    a.sync(&r);
    b.write(".quiet/meta/bbbb/3f.json", "{\"v\":1}");
    b.sync(&r);
    a.sync(&r);
    let want = map(&[(".quiet/format.json", "{\"format\":6}"), (".quiet/meta/aaaa/device.json", "{\"id\":\"aaaa\"}"), (".quiet/meta/aaaa/07.json", "{\"v\":1}"), (".quiet/meta/bbbb/3f.json", "{\"v\":1}")]);
    assert_eq!((remote_text(&r), b.files()), (want.clone(), want));
    assert_eq!(a.read(".quiet/meta/bbbb/3f.json").as_deref(), Some("{\"v\":1}"));
    // A later write of one shard moves that shard alone.
    a.write(".quiet/meta/aaaa/07.json", "{\"v\":1,\"items\":{}}");
    assert_eq!(a.sync(&r).uploaded, [".quiet/meta/aaaa/07.json"].map(String::from));
}

#[test]
fn only_managed_files_move_and_the_rest_of_the_remote_stays() {
    let (r, a, b) = (FakeRemote::new(), Device::new("MacBook"), Device::new("iPad"));
    r.push("main", &[("README.txt".into(), Some(b"hi".to_vec())), (".github/x.yml".into(), Some(b"y".to_vec()))], "made on github.com");
    a.write("周报.xml", "1");
    for p in [".DS_Store", ".quiet/history/周报/1.xml", ".quiet/index.json", ".assets/thumbs/t.png", "photo.png", ".git/HEAD"] { a.write(p, "local") }
    a.sync(&r);
    let want = map(&[("README.txt", "hi"), (".github/x.yml", "y"), ("周报.xml", "1")]);
    assert_eq!(remote_text(&r), want);
    b.sync(&r);
    assert_eq!(b.files(), map(&[("周报.xml", "1")]));
    // A name this device can't use stays on the remote, listed.
    r.push("main", &[("a:b.xml".into(), Some(b"x".to_vec()))], "made on Linux");
    assert_eq!(b.sync(&r).skipped, vec!["a:b.xml".to_string()]);
    assert!(remote_text(&r).contains_key("a:b.xml"));
}

#[test]
fn files_too_large_stay_here() {
    let (r, a) = (FakeRemote::new(), Device::new("MacBook"));
    a.write(".assets/big.mov", &"x".repeat(2000));
    a.write(".assets/mid.mov", &"x".repeat(1000));
    a.write("周报.xml", "1");
    let small = |o: &mut Options| { o.too_large = 1500; o.large = 500 };
    let o = a.sync_with(&r, small).unwrap();
    assert_eq!(o.too_large, vec![(".assets/big.mov".to_string(), 2000)]);
    assert_eq!(o.large, vec![(".assets/mid.mov".to_string(), 1000)]);
    assert_eq!(remote_text(&r).keys().collect::<Vec<_>>(), [".assets/mid.mov", "周报.xml"]);
    // Listed again each time, never lost.
    assert_eq!(a.sync_with(&r, small).unwrap().too_large.len(), 1);
    assert!(a.read(".assets/big.mov").is_some());
}

#[test]
fn a_branch_gone_is_filled_again() {
    let (r, a) = (FakeRemote::new(), Device::new("MacBook"));
    a.write("周报.xml", "1");
    a.sync(&r);
    // As if the repository were made anew.
    let fresh = FakeRemote::new();
    assert_eq!(a.sync(&fresh).uploaded, vec!["周报.xml".to_string()]);
    assert_eq!(a.files(), map(&[("周报.xml", "1")]));
}

/// A merger for `.md` that records the base it got and puts both sides together.
struct Joining(RefCell<Vec<Option<String>>>);

impl Merger for Joining {
    fn mergeable(&self, path: &str) -> bool { path.ends_with(".md") }
    fn merge(&self, _: &str, base: Option<&[u8]>, ours: &[u8], theirs: &[u8]) -> Merged {
        self.0.borrow_mut().push(base.map(|b| String::from_utf8(b.to_vec()).unwrap()));
        Merged::Clean([ours, b"+", theirs].concat())
    }
}

#[test]
fn the_base_is_the_last_synced_version() {
    let (r, a, b) = (FakeRemote::new(), Device::new("MacBook"), Device::new("iPad"));
    let m = Joining(RefCell::new(vec![]));
    a.write("笔记.md", "base");
    a.sync_by(&r, &m, |_| {}).unwrap();
    b.sync_by(&r, &m, |_| {}).unwrap();
    a.write("笔记.md", "mac");
    b.write("笔记.md", "ipad");
    a.sync_by(&r, &m, |_| {}).unwrap();
    // The base came from b's cache, not the remote.
    let calls = r.calls();
    let o = b.sync_by(&r, &m, |_| {}).unwrap();
    assert_eq!(o.merged, vec!["笔记.md".to_string()]);
    assert_eq!(m.0.borrow().as_slice(), [Some("base".to_string())]);
    assert_eq!(b.read("笔记.md").as_deref(), Some("ipad+mac"));
    // head, changed, theirs, commit: no blob fetch for the base.
    assert_eq!(r.calls() - calls, 4);
    a.sync_by(&r, &m, |_| {}).unwrap();
    // Without the cache it is fetched by its id.
    a.write("笔记.md", "mac2");
    b.write("笔记.md", "ipad2");
    a.sync_by(&r, &m, |_| {}).unwrap();
    vfs::remove_dir_all(b.state.join(BASES)).unwrap();
    b.sync_by(&r, &m, |_| {}).unwrap();
    assert_eq!(m.0.borrow().last().cloned(), Some(Some("ipad+mac".to_string())));
    a.sync_by(&r, &m, |_| {}).unwrap();
    assert_eq!(a.read("笔记.md").as_deref(), Some("ipad2+mac2"));
    // Only the current bases are kept.
    let kept = vfs::read_dir(b.state.join(BASES)).unwrap().count();
    assert_eq!(kept, 1);
}

// ------------------------------------------------------------ documents merged by block

/// A document as DocxXML writes it.
fn xml(paras: &[&str]) -> String {
    let body: String = paras.iter().map(|p| format!("<p>{p}</p>\n")).collect();
    crate::docxml::serialize(&crate::docxml::parse(&format!("<title>周报</title>\n{body}")).unwrap())
}

#[test]
fn edits_to_different_paragraphs_of_a_document_both_land() {
    let (r, a, b) = (FakeRemote::new(), Device::new("MacBook"), Device::new("iPad"));
    let m = DocMerger;
    a.write("周报.xml", &xml(&["一", "二", "三", "四"]));
    a.write("提案.md", "# 提案\n\n第一段\n\n第二段\n\n- 甲\n- 乙\n\n第三段\n");
    a.sync_by(&r, &m, |_| {}).unwrap();
    b.sync_by(&r, &m, |_| {}).unwrap();
    a.write("周报.xml", &xml(&["一 mac", "二", "三", "四"]));
    a.write("提案.md", "# 提案\n\n第一段 mac\n\n第二段\n\n- 甲\n- 乙\n\n第三段\n");
    b.write("周报.xml", &xml(&["一", "二", "三", "四 ipad", "五"]));
    b.write("提案.md", "# 提案\n\n第一段\n\n第二段\n\n- 甲\n- 乙\n- 丙\n\n第三段 ipad\n");
    a.sync_by(&r, &m, |_| {}).unwrap();
    let o = b.sync_by(&r, &m, |_| {}).unwrap();
    assert_eq!((o.merged, o.conflicts), (vec!["周报.xml".to_string(), "提案.md".to_string()], vec![]));
    a.sync_by(&r, &m, |_| {}).unwrap();
    let want = map(&[
        ("周报.xml", xml(&["一 mac", "二", "三", "四 ipad", "五"]).as_str()),
        ("提案.md", "# 提案\n\n第一段 mac\n\n第二段\n\n- 甲\n- 乙\n- 丙\n\n第三段 ipad\n"),
    ]);
    // No conflict copy on either side, both edits in both.
    assert_eq!((a.files(), b.files(), remote_text(&r)), (want.clone(), want.clone(), want));
}

#[test]
fn both_editing_one_paragraph_keeps_the_merge_here_and_theirs_as_a_copy() {
    for (p, base, mac, ipad, merged) in [
        ("周报.xml", xml(&["一", "二", "三"]), xml(&["一 mac", "二", "三 mac"]), xml(&["一 ipad", "二", "三"]), xml(&["一 ipad", "二", "三 mac"])),
        ("提案.md", "第一段\n\n第二段\n\n第三段\n".into(), "第一段 mac\n\n第二段\n\n第三段 mac\n".into(), "第一段 ipad\n\n第二段\n\n第三段\n".into(), "第一段 ipad\n\n第二段\n\n第三段 mac\n".into()),
    ] {
        let (r, a, b) = (FakeRemote::new(), Device::new("MacBook"), Device::new("iPad"));
        a.write(p, &base);
        a.sync(&r);
        b.sync(&r);
        a.write(p, &mac);
        b.write(p, &ipad);
        a.sync_by(&r, &DocMerger, |_| {}).unwrap();
        let o = b.sync_by(&r, &DocMerger, |_| {}).unwrap();
        // Here: ours where both edited, theirs merged in elsewhere; theirs whole in the copy.
        let copy = copy_name("iPad", p);
        assert_eq!(o.conflicts, vec![(p.to_string(), copy.clone(), 1)], "{p}");
        assert_eq!(b.files(), map(&[(p, merged.as_str()), (copy.as_str(), mac.as_str())]), "{p}");
        assert_eq!(remote_text(&r), map(&[(p, merged.as_str())]), "{p}");
        a.sync_by(&r, &DocMerger, |_| {}).unwrap();
        assert_eq!(a.files(), map(&[(p, merged.as_str())]), "{p}");
    }
}

#[test]
fn other_files_are_not_merged_by_contents() {
    let (r, a, b) = (FakeRemote::new(), Device::new("MacBook"), Device::new("iPad"));
    a.write("白板.excalidraw", "base");
    a.sync(&r);
    b.sync(&r);
    a.write("白板.excalidraw", "mac");
    b.write("白板.excalidraw", "ipad");
    a.sync_by(&r, &DocMerger, |_| {}).unwrap();
    let o = b.sync_by(&r, &DocMerger, |_| {}).unwrap();
    assert_eq!(o.conflicts, vec![("白板.excalidraw".to_string(), "白板（冲突 · iPad）.excalidraw".to_string(), 1)]);
    assert_eq!(b.read("白板.excalidraw").as_deref(), Some("ipad"));
}

// ------------------------------------------------------------ random two-device runs

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize { (self.next() % n.max(1) as u64) as usize }
}

fn seeds() -> u64 { std::env::var("FDL_FUZZ_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(20) }

const NAMES: &[&str] = &["周报.xml", "提案.md", "a/甲.xml", "a/乙.md", ".assets/i.png", "b/c/丙.xml", ".DS_Store", ".quiet/history/h.xml"];

/// Two devices edit, rename, remove, sync, get pushed ahead of and go
/// offline, at random. Every sync loses nothing: what this device changed is
/// still here after, and whatever of the remote's it overwrote is in a
/// conflict copy here. Once both have synced twice more they hold the same
/// files (but for conflict copies, which stay where they were made).
#[test]
fn random_two_device_syncs_lose_nothing() {
    for seed in 0..seeds() {
        let mut rng = Rng(seed * 7919 + 1);
        let r = FakeRemote::new();
        let devices = [Device::new("A"), Device::new("B")];
        let mut n = 0;
        for _ in 0..30 {
            let d = &devices[rng.below(2)];
            n += 1;
            match rng.below(10) {
                0..=3 => d.write(NAMES[rng.below(NAMES.len())], &format!("{}-{n}", d.name)),
                4 => { let p = NAMES[rng.below(NAMES.len())]; if d.read(p).is_some() { d.remove(p) } }
                5 => {
                    let (from, to) = (NAMES[rng.below(6)], NAMES[rng.below(6)]);
                    if let Some(s) = d.read(from) && from != to { d.remove(from); d.write(to, &s) }
                }
                6 => r.race("main", vec![(NAMES[rng.below(6)].to_string(), Some(format!("race-{n}").into_bytes()))]),
                7 => r.set_offline(rng.below(3) == 0),
                _ => checked_sync(d, &r, seed),
            }
        }
        r.set_offline(false);
        for _ in 0..2 { for d in &devices { checked_sync(d, &r, seed) } }
        let shared = |d: &Device| {
            let held = d.state().held;
            d.files().into_iter().filter(|(p, _)| synced(p) && !held.contains(p)).collect::<BTreeMap<_, _>>()
        };
        assert_eq!(shared(&devices[0]), shared(&devices[1]), "seed {seed}");
        assert_eq!(shared(&devices[0]), remote_text(&r), "seed {seed}");
    }
}

/// A sync, checked to keep everything: this device's changes since its last
/// sync, and every remote file it replaced or removed (in a conflict copy, or
/// it was this device's own last version).
fn checked_sync(d: &Device, r: &FakeRemote, seed: u64) {
    let state = d.state();
    let mine: Vec<String> = d.files().into_iter().filter(|(p, s)| synced(p) && state.files.get(p) != Some(&blob_sha(s.as_bytes()))).map(|(_, s)| s).collect();
    let before = remote_text(r);
    let known: HashSet<String> = state.files.values().cloned().collect();
    // Another device's push landing mid-sync replaces files too; only a quiet remote tells who did.
    let racing = r.racing();
    match d.sync_with(r, |o| o.retries = 2) {
        Ok(_) | Err(Error::Offline(_)) | Err(Error::Rejected) => {}
        Err(e) => panic!("seed {seed}: {e}"),
    }
    let here: HashSet<String> = d.files().into_values().collect();
    for s in &mine { assert!(here.contains(s), "seed {seed}: lost {s} of {}", d.name) }
    let after = remote_text(r);
    for (p, s) in &before {
        // A file of the remote's this device never saw, replaced or removed by it.
        if !racing && after.get(p) != Some(s) && !known.contains(&blob_sha(s.as_bytes())) && !after.values().any(|v| v == s) {
            assert!(here.contains(s), "seed {seed}: {p} = {s} overwritten by {} and not kept", d.name);
        }
    }
}

const DOCS: &[&str] = &["周报.xml", "a/甲.xml", "b/c/丙.xml"];

/// Two devices edit documents block by block (the generator of
/// `merge/tests.rs`), remove them, sync, get pushed ahead of and go offline,
/// at random, merged by [`DocMerger`]: no sync fails, every document here
/// stays well formed, each conflict leaves its copy, and two more rounds
/// leave both devices and the remote with the same files.
#[test]
fn random_document_edits_merge_and_meet() {
    use crate::docxml::{parse, serialize};
    use crate::merge::tests::{NESTED, Rng as DocRng, edited};
    let base = parse(NESTED).unwrap();
    for seed in 0..seeds() {
        let mut rng = DocRng(seed * 7919 + 1);
        let r = FakeRemote::new();
        let devices = [Device::new("A"), Device::new("B")];
        for _ in 0..30 {
            let d = &devices[rng.below(2)];
            let p = DOCS[rng.below(DOCS.len())];
            match rng.below(10) {
                0..=3 => { let now = d.read(p).map_or(base.clone(), |s| parse(&s).unwrap()); d.write(p, &serialize(&edited(&now, &mut rng))) }
                4 => if d.read(p).is_some() { d.remove(p) },
                5 => {
                    let now = r.files("main").into_iter().find(|(q, _)| q == p).map_or(base.clone(), |(_, b)| parse(std::str::from_utf8(&b).unwrap()).unwrap());
                    r.race("main", vec![(p.to_string(), Some(serialize(&edited(&now, &mut rng)).into_bytes()))]);
                }
                6 => r.set_offline(rng.below(3) == 0),
                _ => merged_sync(d, &r, seed),
            }
        }
        r.set_offline(false);
        for _ in 0..2 { for d in &devices { merged_sync(d, &r, seed) } }
        let shared = |d: &Device| {
            let held = d.state().held;
            d.files().into_iter().filter(|(p, _)| synced(p) && !held.contains(p)).collect::<BTreeMap<_, _>>()
        };
        assert_eq!(shared(&devices[0]), shared(&devices[1]), "seed {seed}");
        assert_eq!(shared(&devices[0]), remote_text(&r), "seed {seed}");
    }
}

fn merged_sync(d: &Device, r: &FakeRemote, seed: u64) {
    match d.sync_by(r, &DocMerger, |o| o.retries = 2) {
        Ok(o) => for (p, copy, n) in &o.conflicts { assert!(*n >= 1 && d.read(copy).is_some(), "seed {seed}: {p} → {copy} ({n})") }
        Err(Error::Offline(_)) | Err(Error::Rejected) => {}
        Err(e) => panic!("seed {seed}: {e}"),
    }
    for (p, s) in d.files() {
        let doc = crate::docxml::parse(&s).unwrap_or_else(|e| panic!("seed {seed}: {p} of {}: {e}", d.name));
        assert_eq!(crate::docxml::serialize(&doc), s, "seed {seed}: {p} of {} does not round-trip", d.name);
    }
}
