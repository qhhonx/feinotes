//! Snapshot Zed's GPUI crates into `vendor/zed` and apply the local patch
//! series in `vendor/zed/patches`. Usage and workflow: vendor/PATCHES.md.
//!
//!   sync [--rev <sha>] [--keep-going]   rebuild vendor/zed/crates from upstream + patches
//!   check                               vendor/zed/crates matches upstream + patches
//!   save <slug> [--number <n>] [-m <text>]
//!                                       turn edits in vendor/zed/crates into a patch
//!                                       (an existing slug is regenerated in place; a
//!                                       new one takes <n>, or the highest number + 1)
//!   refresh                             rewrite every patch against the current snapshot
//!
//! Patches apply in number order. Numbers are never reassigned: ranges are handed
//! out to lines of work (vendor/PATCHES.md), so gaps are normal.
//!
//! The upstream crates inherit fields and dependencies from Zed's workspace. A
//! copied crate is not a member of ours, so each `Cargo.toml` is rewritten to a
//! self-contained manifest (the upstream form, patched, is kept beside it as
//! `Cargo.toml.orig`; patches always edit that form). Dependencies on Zed crates
//! that are not vendored are dropped together with the features naming them,
//! dev-dependencies are dropped, and git dependencies become crates.io ones.

use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use toml_edit::{DocumentMut, InlineTable, Item, Table, Value};

struct Config { repo: String, rev: String, crates: Vec<String>, drop: Vec<String>, git_versions: BTreeMap<String, String> }

struct Paths { root: PathBuf, vendor: PathBuf, patches: PathBuf, cache: PathBuf, scratch: PathBuf }

fn main() {
    if let Err(e) = run() { eprintln!("vendor-sync: {e:#}"); std::process::exit(1) }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = repo_root()?;
    let vendor = root.join("vendor/zed");
    let cache = std::env::var_os("QUIET_MD_ZED_DIR").map(PathBuf::from)
        .unwrap_or_else(|| dirs_home().join(".cache/quiet-md/zed"));
    let p = Paths { patches: vendor.join("patches"), scratch: root.join("target/vendor-sync"), vendor, cache, root };
    let mut cfg = read_config(&p.vendor.join("upstream.toml"))?;
    let flag = |name: &str| args.iter().any(|a| a == name);
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    match args.first().map(String::as_str) {
        Some("sync") => {
            if let Some(rev) = opt("--rev") { cfg.rev = resolve_rev(&p, &cfg, &rev)?; set_rev(&p, &cfg.rev)? }
            let failed = build(&p, &cfg, None)?;
            if !failed.is_empty() && !flag("--keep-going") {
                bail!("{} patch(es) did not apply: {}\nvendor/zed/crates was left unchanged; rerun with --keep-going to write the rest, fix each failed patch by hand in vendor/zed/crates and `save` it", failed.len(), failed.join(", "))
            }
            install(&p, &cfg)?;
            println!("vendor/zed/crates = zed {} + {} patch(es)", &cfg.rev[..12], patch_files(&p)?.len() - failed.len());
            if !failed.is_empty() { println!("not applied: {}", failed.join(", ")) }
        }
        Some("check") => {
            let failed = build(&p, &cfg, None)?;
            if !failed.is_empty() { bail!("patch(es) did not apply: {}", failed.join(", ")) }
            let out = p.scratch.join("out");
            let _ = fs::remove_dir_all(&out);
            write_crates(&p, &cfg, &out)?;
            let diff = compare_dirs(&out.join("crates"), &p.vendor.join("crates"))?;
            if !diff.is_empty() {
                for f in &diff { eprintln!("  differs: {f}") }
                bail!("vendor/zed/crates does not match upstream + patches ({} file(s)); `save` your edits as a patch", diff.len())
            }
            println!("vendor/zed/crates matches zed {} + {} patch(es)", &cfg.rev[..12], patch_files(&p)?.len());
        }
        Some("save") => {
            let slug = args.get(1).filter(|s| !s.starts_with('-')).context("usage: save <slug> [--number <n>] [-m <description>]")?.clone();
            save(&p, &cfg, &slug, opt("--number"), opt("-m"))?
        }
        Some("refresh") => refresh(&p, &cfg)?,
        _ => bail!("usage: vendor-sync sync [--rev <sha>] [--keep-going] | check | save <slug> [--number <n>] [-m <text>] | refresh"),
    }
    Ok(())
}

fn dirs_home() -> PathBuf { PathBuf::from(std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).unwrap_or_default()) }

fn repo_root() -> Result<PathBuf> {
    let out = git_out(Path::new("."), &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(out.trim()))
}

fn read_config(path: &Path) -> Result<Config> {
    let doc: DocumentMut = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?.parse()?;
    let s = |k: &str| doc.get(k).and_then(|v| v.as_str()).map(String::from).with_context(|| format!("upstream.toml: missing `{k}`"));
    let list = |k: &str| doc.get(k).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
    let mut git_versions = BTreeMap::new();
    if let Some(t) = doc.get("git-to-crates-io").and_then(|t| t.as_table_like()) {
        for (k, v) in t.iter() { git_versions.insert(k.to_string(), v.as_str().unwrap_or_default().to_string()); }
    }
    Ok(Config { repo: s("repo")?, rev: s("rev")?, crates: list("crates"), drop: list("drop"), git_versions })
}

fn set_rev(p: &Paths, rev: &str) -> Result<()> {
    let path = p.vendor.join("upstream.toml");
    let mut doc: DocumentMut = fs::read_to_string(&path)?.parse()?;
    doc["rev"] = toml_edit::value(rev);
    fs::write(&path, doc.to_string())?;
    Ok(())
}

// ---- git -------------------------------------------------------------------

/// Git, cut loose from any repository named in the environment. A git hook
/// (pre-push running scripts/check.py) sets GIT_DIR and friends; inherited,
/// they point `git -C <tree>` at the outer repository, so `init` rewrote its
/// config (core.bare = true) instead of making the scratch tree. The list is
/// `git rev-parse --local-env-vars`, what git itself clears for submodules.
fn git_cmd() -> Command {
    let mut c = Command::new("git");
    for v in ["GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_CONFIG", "GIT_CONFIG_PARAMETERS", "GIT_CONFIG_COUNT", "GIT_OBJECT_DIRECTORY", "GIT_DIR", "GIT_WORK_TREE",
        "GIT_IMPLICIT_WORK_TREE", "GIT_GRAFT_FILE", "GIT_INDEX_FILE", "GIT_NO_REPLACE_OBJECTS", "GIT_REPLACE_REF_BASE", "GIT_PREFIX", "GIT_SHALLOW_FILE",
        "GIT_COMMON_DIR", "GIT_CEILING_DIRECTORIES"] { c.env_remove(v); }
    c
}

fn git(dir: &Path, args: &[&str]) -> Result<()> {
    let st = git_cmd().arg("-C").arg(dir).args(GIT_ID).args(args).status().context("running git")?;
    if !st.success() { bail!("git {} failed in {}", args.join(" "), dir.display()) }
    Ok(())
}

fn git_out(dir: &Path, args: &[&str]) -> Result<String> {
    let out = git_cmd().arg("-C").arg(dir).args(GIT_ID).args(args).stderr(Stdio::inherit()).output().context("running git")?;
    if !out.status.success() { bail!("git {} failed in {}", args.join(" "), dir.display()) }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Runs git and returns (success, stderr) without failing.
fn git_try(dir: &Path, args: &[&str]) -> Result<(bool, String)> {
    let out = git_cmd().arg("-C").arg(dir).args(GIT_ID).args(args).output().context("running git")?;
    Ok((out.status.success(), String::from_utf8_lossy(&out.stderr).into_owned()))
}

const GIT_ID: &[&str] = &["-c", "user.name=vendor-sync", "-c", "user.email=vendor-sync@localhost", "-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"];

/// A blobless clone of the Zed repository, checked out sparsely at `rev`
/// (the sparse checkout downloads the needed blobs in one batch).
fn ensure_cache(p: &Paths, cfg: &Config, rev: &str) -> Result<()> {
    if !p.cache.join(".git").exists() {
        fs::create_dir_all(p.cache.parent().unwrap())?;
        println!("cloning {} (blobless) into {}", cfg.repo, p.cache.display());
        let st = git_cmd().args(["clone", "--filter=blob:none", "--no-checkout", "--quiet", &cfg.repo]).arg(&p.cache).status()?;
        if !st.success() { bail!("git clone failed") }
    }
    if !git_try(&p.cache, &["cat-file", "-e", &format!("{rev}^{{commit}}")])?.0 {
        git(&p.cache, &["fetch", "--quiet", "--filter=blob:none", "origin", rev])?;
    }
    let mut sparse = vec!["sparse-checkout", "set", "--no-cone", "/Cargo.toml", "/LICENSE-APACHE"];
    let pats: Vec<String> = cfg.crates.iter().map(|c| format!("/{c}/")).collect();
    sparse.extend(pats.iter().map(String::as_str));
    git(&p.cache, &sparse)?;
    git(&p.cache, &["checkout", "--quiet", "--detach", rev])?;
    Ok(())
}

fn resolve_rev(p: &Paths, cfg: &Config, rev: &str) -> Result<String> {
    ensure_cache(p, cfg, &cfg.rev)?;
    git(&p.cache, &["fetch", "--quiet", "--filter=blob:none", "origin", rev]).ok();
    let target = if rev == "main" || rev == "origin/main" { "FETCH_HEAD" } else { rev };
    Ok(git_out(&p.cache, &["rev-parse", &format!("{target}^{{commit}}")])?.trim().to_string())
}

// ---- building upstream + patches --------------------------------------------

fn patch_files(p: &Paths) -> Result<Vec<PathBuf>> {
    let mut v: Vec<PathBuf> = fs::read_dir(&p.patches).map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).collect()).unwrap_or_default();
    v.retain(|f| f.extension().is_some_and(|e| e == "patch"));
    v.sort_by_key(|f| (number_of(f), f.clone()));
    Ok(v)
}

/// The number a patch file name starts with ("0046-foo.patch" → 46).
fn number_of(f: &Path) -> Option<u32> {
    f.file_name()?.to_string_lossy().split_once('-')?.0.parse().ok()
}

fn slug_of(f: &Path) -> String {
    let stem = f.file_stem().unwrap().to_string_lossy();
    stem.split_once('-').map(|(_, s)| s.to_string()).unwrap_or_else(|| stem.to_string())
}

/// Recreates `target/vendor-sync/tree` as a git repository holding the upstream
/// crates (commit "upstream") and applies the patches in order, one commit each.
/// `skip` leaves one patch out. Returns the file names of patches that failed.
fn build(p: &Paths, cfg: &Config, skip: Option<&Path>) -> Result<Vec<String>> {
    ensure_cache(p, cfg, &cfg.rev)?;
    let tree = p.scratch.join("tree");
    let _ = fs::remove_dir_all(&tree);
    fs::create_dir_all(&tree)?;
    for c in &cfg.crates {
        // Nested crates (refineable/derive_refineable) are copied with their parent.
        if cfg.crates.iter().any(|o| o != c && Path::new(c).starts_with(o)) { continue }
        copy_dir(&p.cache.join(c), &tree.join(c), &cfg.drop, true)?;
    }
    git(&tree, &["init", "--quiet"])?;
    git(&tree, &["add", "-A"])?;
    git(&tree, &["commit", "--quiet", "-m", &format!("upstream {}", cfg.rev)])?;
    let mut failed = Vec::new();
    for f in patch_files(p)? {
        if skip == Some(f.as_path()) { continue }
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let fs_ = f.to_string_lossy();
        let (ok, err) = git_try(&tree, &["apply", "--index", "--3way", "--whitespace=nowarn", &fs_])?;
        let conflicts = ok && !git_out(&tree, &["diff", "--name-only", "--diff-filter=U"])?.trim().is_empty();
        if !ok || conflicts {
            eprintln!("✗ {name} does not apply:\n{}", indent(&err));
            failed.push(name);
            git(&tree, &["reset", "--quiet", "--hard"])?;
            git(&tree, &["clean", "--quiet", "-fd"])?;
            continue;
        }
        git(&tree, &["commit", "--quiet", "--allow-empty", "-m", &name])?;
    }
    Ok(failed)
}

fn indent(s: &str) -> String { s.lines().map(|l| format!("    {l}\n")).collect() }

/// Copies a directory, skipping `.git`, dropped subdirectories (only at the top
/// of a crate) and symlinks (the per-crate LICENSE-APACHE links; one copy of the
/// license lives in vendor/zed).
fn copy_dir(from: &Path, to: &Path, drop: &[String], crate_root: bool) -> Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from).with_context(|| format!("reading {}", from.display()))? {
        let e = e?;
        let name = e.file_name();
        let ft = e.file_type()?;
        if name == ".git" || ft.is_symlink() { continue }
        if ft.is_dir() {
            if crate_root && drop.iter().any(|d| name == d.as_str()) { continue }
            // A nested crate directory is a crate root of its own.
            let nested = e.path().join("Cargo.toml").exists();
            copy_dir(&e.path(), &to.join(&name), drop, nested)?;
        } else {
            fs::copy(e.path(), to.join(&name))?;
        }
    }
    Ok(())
}

/// Writes the patched tree as vendor-ready crates into `out`: every Cargo.toml
/// is normalized and the upstream form kept as Cargo.toml.orig.
fn write_crates(p: &Paths, cfg: &Config, out: &Path) -> Result<()> {
    let tree = p.scratch.join("tree");
    let ws: DocumentMut = fs::read_to_string(p.cache.join("Cargo.toml"))?.parse()?;
    let vendored: BTreeMap<String, String> = cfg.crates.iter().map(|c| Ok((crate_name(&tree.join(c))?, c.clone()))).collect::<Result<_>>()?;
    copy_dir(&tree.join("crates"), &out.join("crates"), &[], false)?;
    for c in &cfg.crates {
        let orig = fs::read_to_string(tree.join(c).join("Cargo.toml"))?;
        let norm = normalize(&orig, &ws, c, &vendored, cfg).with_context(|| format!("normalizing {c}/Cargo.toml"))?;
        let dst = out.join(c);
        fs::write(dst.join("Cargo.toml.orig"), &orig)?;
        fs::write(dst.join("Cargo.toml"), norm)?;
    }
    // vendor/zed/crates holds crates only; the license sits one level up.
    Ok(())
}

fn install(p: &Paths, cfg: &Config) -> Result<()> {
    let out = p.scratch.join("out");
    let _ = fs::remove_dir_all(&out);
    write_crates(p, cfg, &out)?;
    let dst = p.vendor.join("crates");
    let _ = fs::remove_dir_all(&dst);
    copy_dir(&out.join("crates"), &dst, &[], false)?;
    fs::copy(p.cache.join("LICENSE-APACHE"), p.vendor.join("LICENSE-APACHE"))?;
    Ok(())
}

fn crate_name(dir: &Path) -> Result<String> {
    let doc: DocumentMut = fs::read_to_string(dir.join("Cargo.toml"))?.parse()?;
    Ok(doc["package"]["name"].as_str().context("package.name")?.to_string())
}

fn compare_dirs(a: &Path, b: &Path) -> Result<Vec<String>> {
    let (fa, fb) = (list_files(a)?, list_files(b)?);
    let mut diff = Vec::new();
    for f in fa.union(&fb) {
        let (x, y) = (fs::read(a.join(f)).ok(), fs::read(b.join(f)).ok());
        if x != y { diff.push(f.clone()) }
    }
    Ok(diff)
}

fn list_files(dir: &Path) -> Result<BTreeSet<String>> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeSet<String>) -> Result<()> {
        if !dir.exists() { return Ok(()) }
        for e in fs::read_dir(dir)? {
            let e = e?;
            if e.file_type()?.is_dir() { walk(base, &e.path(), out)? } else if e.file_name() != ".DS_Store" {
                out.insert(e.path().strip_prefix(base)?.to_string_lossy().replace('\\', "/"));
            }
        }
        Ok(())
    }
    let mut out = BTreeSet::new();
    walk(dir, dir, &mut out)?;
    Ok(out)
}

// ---- save / refresh ---------------------------------------------------------

fn save(p: &Paths, cfg: &Config, slug: &str, number: Option<String>, msg: Option<String>) -> Result<()> {
    let files = patch_files(p)?;
    let existing = files.iter().find(|f| slug_of(f) == slug).cloned();
    let name = patch_name(&files, slug, number.as_deref())?;
    let n = number_of(Path::new(&name)).unwrap_or(0);
    let failed = build(p, cfg, existing.as_deref())?;
    if !failed.is_empty() { eprintln!("note: left out of the base because they do not apply: {}", failed.join(", ")) }
    let tree = p.scratch.join("tree");
    // Overlay vendor/zed/crates (upstream manifest form) onto the patched tree.
    let _ = fs::remove_dir_all(tree.join("crates"));
    copy_dir(&p.vendor.join("crates"), &tree.join("crates"), &[], false)?;
    for c in &cfg.crates {
        let d = tree.join(c);
        fs::rename(d.join("Cargo.toml.orig"), d.join("Cargo.toml")).with_context(|| format!("{c}/Cargo.toml.orig missing"))?;
    }
    git(&tree, &["add", "-A"])?;
    git(&tree, &["commit", "--quiet", "--allow-empty", "-m", "edits"])?;
    let diff = diff_at(&tree, n)?;
    if diff.trim().is_empty() { bail!("no changes in vendor/zed/crates relative to upstream + the other patches") }
    let header = match &existing {
        Some(f) => msg.map(|m| header_text(&m)).unwrap_or_else(|| read_header(f)),
        None => header_text(&msg.unwrap_or_else(|| format!("{slug}\n\nTODO: describe what this changes and why."))),
    };
    let path = p.patches.join(&name);
    let old = existing.as_ref().map(fs::read).transpose()?;
    fs::create_dir_all(&p.patches)?;
    fs::write(&path, format!("{header}\n{diff}"))?;
    // The series with the new patch in its place must reproduce the edits.
    let failed_now = build(p, cfg, None)?;
    let out = p.scratch.join("out");
    let _ = fs::remove_dir_all(&out);
    let differs = if failed_now.contains(&name) { vec![name.clone()] } else {
        write_crates(p, cfg, &out)?;
        compare_dirs(&out.join("crates"), &p.vendor.join("crates"))?
    };
    if !differs.is_empty() && failed.is_empty() {
        match old { Some(old) => fs::write(&path, old)?, None => fs::remove_file(&path)? }
        bail!("{name} in its place does not reproduce vendor/zed/crates ({}); nothing was written", differs.join(", "))
    }
    if !differs.is_empty() { eprintln!("note: with patches left out, could not confirm {name} reproduces vendor/zed/crates") }
    println!("wrote {}", path.strip_prefix(&p.root).unwrap_or(&path).display());
    Ok(())
}

/// The file name for `slug`: an existing patch keeps its own; a new one takes
/// `number`, or the highest number in the series + 1, and never an existing one.
fn patch_name(files: &[PathBuf], slug: &str, number: Option<&str>) -> Result<String> {
    let n = match number {
        Some(s) => Some(s.parse::<u32>().ok().filter(|n| (1..=9999).contains(n)).with_context(|| format!("--number {s}: expected 1–9999"))?),
        None => None,
    };
    if let Some(f) = files.iter().find(|f| slug_of(f) == slug) {
        if n.is_some_and(|n| number_of(f) != Some(n)) { bail!("{} already exists; patches are not renumbered", f.file_name().unwrap().to_string_lossy()) }
        return Ok(f.file_name().unwrap().to_string_lossy().into_owned());
    }
    let n = n.unwrap_or_else(|| files.iter().filter_map(|f| number_of(f)).max().unwrap_or(0) + 1);
    if let Some(f) = files.iter().find(|f| number_of(f) == Some(n)) { bail!("number {n:04} is taken by {}", f.file_name().unwrap().to_string_lossy()) }
    Ok(format!("{n:04}-{slug}.patch"))
}

/// `tree` holds upstream, one commit per patch (named by its file) and the
/// edits on top. Returns the diff the patch numbered `n` needs so that, applied
/// after the patches numbered below `n`, the later ones still lead to the edits.
fn diff_at(tree: &Path, n: u32) -> Result<String> {
    let log = git_out(tree, &["log", "--reverse", "--format=%H %s"])?;
    let commits: Vec<(&str, &str)> = log.lines().filter_map(|l| l.split_once(' ')).collect();
    let later: Vec<&(&str, &str)> = commits.iter().filter(|(_, s)| number_of(Path::new(s)).is_some_and(|m| m > n)).collect();
    for (sha, name) in later.iter().rev() {
        if git_try(tree, &["diff", "--quiet", &format!("{sha}^"), sha])?.0 { continue }
        if !git_try(tree, &["revert", "--no-edit", sha])?.0 {
            let _ = git_try(tree, &["revert", "--abort"]);
            bail!("the edits overlap {name}, which comes later in the series; save them with a number after it (--number), or fold them into it")
        }
    }
    let base = commits.iter().take_while(|(_, s)| *s != "edits" && number_of(Path::new(s)).is_none_or(|m| m < n)).last().context("empty tree")?.0;
    git_out(tree, &["diff", "--binary", "--full-index", base, "HEAD"])
}

/// Patch files start with free text (git apply skips everything before the
/// first `diff --git`).
fn header_text(msg: &str) -> String { let mut s = msg.trim_end().to_string(); s.push('\n'); s }

fn read_header(f: &Path) -> String {
    let text = fs::read_to_string(f).unwrap_or_default();
    let end = text.find("\ndiff --git ").map(|i| i + 1).unwrap_or(0);
    let h = text[..end].trim_end();
    header_text(h)
}

fn refresh(p: &Paths, cfg: &Config) -> Result<()> {
    let failed = build(p, cfg, None)?;
    if !failed.is_empty() { bail!("fix these first: {}", failed.join(", ")) }
    let tree = p.scratch.join("tree");
    let log = git_out(&tree, &["log", "--reverse", "--format=%H %s"])?;
    let commits: Vec<(&str, &str)> = log.lines().filter_map(|l| l.split_once(' ')).collect();
    for w in commits.windows(2) {
        let (prev, (sha, name)) = (w[0].0, w[1]);
        let f = p.patches.join(name);
        let diff = git_out(&tree, &["diff", "--binary", "--full-index", prev, sha])?;
        let new = format!("{}\n{diff}", read_header(&f));
        if fs::read_to_string(&f).ok().as_deref() != Some(new.as_str()) { fs::write(&f, new)?; println!("refreshed {name}") }
    }
    Ok(())
}

// ---- manifest normalization -------------------------------------------------

const DEP_TABLES: &[&str] = &["dependencies", "build-dependencies", "dev-dependencies"];

fn normalize(orig: &str, ws: &DocumentMut, crate_dir: &str, vendored: &BTreeMap<String, String>, cfg: &Config) -> Result<String> {
    let mut doc: DocumentMut = orig.parse()?;
    let wpkg = ws["workspace"]["package"].as_table_like().context("workspace.package")?;
    let wdeps = ws["workspace"]["dependencies"].as_table_like().context("workspace.dependencies")?;
    // [package]: inherited fields; no auto-discovered example or bench targets.
    let pkg = doc["package"].as_table_mut().context("[package]")?;
    let inherited: Vec<String> = pkg.iter().filter(|(_, v)| is_workspace(v)).map(|(k, _)| k.to_string()).collect();
    for k in inherited {
        let v = wpkg.get(&k).and_then(|v| v.as_value()).with_context(|| format!("workspace.package.{k}"))?.clone();
        pkg.insert(&k, Item::Value(v));
    }
    pkg.insert("autoexamples", toml_edit::value(false));
    pkg.insert("autobenches", toml_edit::value(false));
    doc.remove("example");
    doc.remove("bench");
    // Dependencies.
    let mut dropped = BTreeSet::new();
    doc.remove("dev-dependencies");
    for t in ["dependencies", "build-dependencies"] {
        if let Some(tbl) = doc.get_mut(t).and_then(|t| t.as_table_mut()) { fix_deps(tbl, wdeps, crate_dir, vendored, cfg, &mut dropped)? }
    }
    if let Some(targets) = doc.get_mut("target").and_then(|t| t.as_table_mut()) {
        for (_, tt) in targets.iter_mut() {
            let Some(tt) = tt.as_table_mut() else { continue };
            tt.remove("dev-dependencies");
            for t in DEP_TABLES {
                if let Some(tbl) = tt.get_mut(t).and_then(|t| t.as_table_mut()) { fix_deps(tbl, wdeps, crate_dir, vendored, cfg, &mut dropped)? }
            }
            let empty: Vec<String> = tt.iter().filter(|(_, v)| v.as_table().is_some_and(|t| t.is_empty())).map(|(k, _)| k.to_string()).collect();
            for k in empty { tt.remove(&k); }
        }
        let empty: Vec<String> = targets.iter().filter(|(_, v)| v.as_table().is_some_and(|t| t.is_empty())).map(|(k, _)| k.to_string()).collect();
        for k in empty { targets.remove(&k); }
    }
    // Features that name a dropped dependency.
    if let Some(feats) = doc.get_mut("features").and_then(|t| t.as_table_mut()) {
        for (_, v) in feats.iter_mut() {
            let Some(arr) = v.as_array_mut() else { continue };
            arr.retain(|e| {
                let s = e.as_str().unwrap_or_default();
                let dep = s.strip_prefix("dep:").unwrap_or(s);
                let dep = dep.split('/').next().unwrap_or(dep).trim_end_matches('?');
                !dropped.contains(dep)
            });
        }
    }
    // [lints] workspace = true → Zed's workspace lints.
    // Appended as text: the tables keep their positions from Zed's manifest,
    // which would interleave them with ours.
    let mut lints_text = String::new();
    if doc.get("lints").is_some_and(is_workspace) {
        doc.remove("lints");
        let mut lints = ws["workspace"]["lints"].as_table().context("workspace.lints")?.clone();
        lints.set_implicit(true);
        let mut d = DocumentMut::new();
        d.insert("lints", Item::Table(lints));
        lints_text = format!("\n{}", d.to_string().trim_start());
    }
    let header = format!("# Generated by tools/vendor-sync from Cargo.toml.orig (zed {}). Do not edit;\n# edit Cargo.toml.orig and save a patch (vendor/PATCHES.md).\n\n", &cfg.rev[..12]);
    Ok(header + doc.to_string().trim_end() + "\n" + &lints_text)
}

fn is_workspace(item: &Item) -> bool {
    item.as_table_like().and_then(|t| t.get("workspace")).and_then(|v| v.as_bool()) == Some(true)
}

fn fix_deps(tbl: &mut Table, wdeps: &dyn toml_edit::TableLike, crate_dir: &str, vendored: &BTreeMap<String, String>, cfg: &Config, dropped: &mut BTreeSet<String>) -> Result<()> {
    let names: Vec<String> = tbl.iter().map(|(k, _)| k.to_string()).collect();
    for name in names {
        let item = tbl.get(&name).unwrap();
        let mut dep = to_inline(item);
        if is_workspace(item) {
            let base = wdeps.get(&name).with_context(|| format!("workspace dependency `{name}`"))?;
            let mut merged = to_inline(base);
            merged.remove("workspace");
            for (k, v) in dep.iter() {
                match k {
                    "workspace" => {}
                    "features" => {
                        let mut all: Vec<String> = merged.get("features").and_then(|f| f.as_array()).map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
                        for f in v.as_array().into_iter().flatten().filter_map(|v| v.as_str()) { if !all.iter().any(|x| x == f) { all.push(f.to_string()) } }
                        merged.insert("features", Value::Array(all.iter().collect()));
                    }
                    _ => { merged.insert(k, v.clone()); }
                }
            }
            dep = merged;
        }
        if let Some(path) = dep.get("path").and_then(|v| v.as_str()).map(String::from) {
            // A Zed crate: workspace paths are relative to the Zed root.
            let pkg = dep.get("package").and_then(|v| v.as_str()).unwrap_or(&name).to_string();
            let Some(target) = vendored.get(&pkg) else { dropped.insert(name.clone()); tbl.remove(&name); continue };
            let _ = path;
            dep.insert("path", Value::from(relative(crate_dir, target)));
        } else if dep.contains_key("git") {
            for k in ["git", "rev", "branch", "tag"] { dep.remove(k); }
            if !dep.contains_key("version") {
                let pkg = dep.get("package").and_then(|v| v.as_str()).unwrap_or(&name).to_string();
                let v = cfg.git_versions.get(&pkg).with_context(|| format!("git dependency `{pkg}` has no version; add it to [git-to-crates-io] in upstream.toml"))?;
                dep.insert("version", Value::from(v.as_str()));
            }
        }
        let item = if dep.len() == 1 && dep.contains_key("version") { Item::Value(dep.get("version").unwrap().clone()) } else { dep.fmt(); Item::Value(Value::InlineTable(dep)) };
        tbl.insert(&name, item);
    }
    Ok(())
}

fn to_inline(item: &Item) -> InlineTable {
    match item {
        Item::Value(Value::String(s)) => { let mut t = InlineTable::new(); t.insert("version", Value::from(s.value().as_str())); t }
        _ => {
            let mut t = InlineTable::new();
            if let Some(tl) = item.as_table_like() {
                for (k, v) in tl.iter() { if let Some(v) = v.as_value() { let mut v = v.clone(); v.decor_mut().clear(); t.insert(k, v); } }
            }
            t
        }
    }
}

/// `to` relative to `from`, both relative to the Zed root ("crates/x").
fn relative(from: &str, to: &str) -> String {
    let f: Vec<&str> = from.split('/').collect();
    let t: Vec<&str> = to.split('/').collect();
    let common = f.iter().zip(&t).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<&str> = vec![".."; f.len() - common];
    parts.extend(&t[common..]);
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(names: &[&str]) -> Vec<PathBuf> { names.iter().map(|n| PathBuf::from("patches").join(n)).collect() }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("vendor-sync-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    const SERIES: &[&str] = &["0001-a.patch", "0039-ios.patch", "0047-corners.patch", "0060-tile.patch", "0085-names.patch"];

    #[test]
    fn existing_patch_keeps_its_number() {
        let f = files(SERIES);
        assert_eq!(patch_name(&f, "tile", None).unwrap(), "0060-tile.patch");
        assert_eq!(patch_name(&f, "tile", Some("0060")).unwrap(), "0060-tile.patch");
        assert!(patch_name(&f, "tile", Some("0061")).is_err());
    }

    #[test]
    fn new_patch_takes_highest_plus_one() {
        assert_eq!(patch_name(&files(SERIES), "new", None).unwrap(), "0086-new.patch");
        assert_eq!(patch_name(&[], "new", None).unwrap(), "0001-new.patch");
    }

    #[test]
    fn new_patch_takes_explicit_number() {
        assert_eq!(patch_name(&files(SERIES), "new", Some("0050")).unwrap(), "0050-new.patch");
        assert_eq!(patch_name(&files(SERIES), "new", Some("62")).unwrap(), "0062-new.patch");
        assert!(patch_name(&files(SERIES), "new", Some("x")).is_err());
        assert!(patch_name(&files(SERIES), "new", Some("0")).is_err());
    }

    #[test]
    fn taken_number_is_an_error() {
        let e = patch_name(&files(SERIES), "new", Some("0047")).unwrap_err().to_string();
        assert!(e.contains("0047-corners.patch"), "{e}");
    }

    #[test]
    fn series_sorts_by_number_and_keeps_gaps() {
        let d = scratch("sort");
        for n in ["0085-names.patch", "0047-corners.patch", "9-x.patch", "0010-y.patch", "notes.md"] { fs::write(d.join(n), "").unwrap() }
        let p = Paths { root: d.clone(), vendor: d.clone(), patches: d.clone(), cache: d.clone(), scratch: d.clone() };
        let names: Vec<String> = patch_files(&p).unwrap().iter().map(|f| f.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["9-x.patch", "0010-y.patch", "0047-corners.patch", "0085-names.patch"]);
        fs::remove_dir_all(&d).unwrap();
    }

    /// A tree with upstream, patches 0010 (line 2) and 0030 (line 18), then edits.
    fn tree(name: &str, edit: &[(usize, &str)]) -> PathBuf {
        let d = scratch(name);
        let mut lines: Vec<String> = (1..=20).map(|i| format!("line {i}")).collect();
        let commit = |lines: &[String], msg: &str| {
            fs::write(d.join("a.txt"), lines.join("\n") + "\n").unwrap();
            git(&d, &["add", "-A"]).unwrap();
            git(&d, &["commit", "--quiet", "--allow-empty", "-m", msg]).unwrap();
        };
        git(&d, &["init", "--quiet"]).unwrap();
        commit(&lines, "upstream e2f00a41ac67");
        lines[1] = "line 2 patched".into();
        commit(&lines, "0010-two.patch");
        lines[17] = "line 18 patched".into();
        commit(&lines, "0030-eighteen.patch");
        for (i, t) in edit { lines[*i - 1] = t.to_string() }
        commit(&lines, "edits");
        d
    }

    #[test]
    fn patch_in_the_middle_leaves_later_patches_applying() {
        let d = tree("middle", &[(10, "line 10 edited")]);
        let diff = diff_at(&d, 20).unwrap();
        assert!(diff.contains("+line 10 edited") && !diff.contains("line 18 patched"), "{diff}");
        // 0010, then the new 0020, then 0030 as it was, give the edits.
        let p30 = git_out(&d, &["diff", "--binary", ":/^0010", ":/^0030"]).unwrap();
        let edits = git_out(&d, &["rev-parse", ":/^edits"]).unwrap();
        let want = git_out(&d, &["show", &format!("{}:a.txt", edits.trim())]).unwrap();
        fs::write(d.join("0020.patch"), &diff).unwrap();
        fs::write(d.join("0030.patch"), &p30).unwrap();
        git(&d, &["checkout", "--quiet", "--detach", ":/^0010"]).unwrap();
        git(&d, &["apply", "0020.patch"]).unwrap();
        git(&d, &["apply", "0030.patch"]).unwrap();
        assert_eq!(fs::read_to_string(d.join("a.txt")).unwrap(), want);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn last_patch_is_the_plain_diff() {
        let d = tree("last", &[(10, "line 10 edited")]);
        assert_eq!(diff_at(&d, 31).unwrap(), git_out(&d, &["diff", "--binary", "--full-index", ":/^0030", ":/^edits"]).unwrap());
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn edits_overlapping_a_later_patch_are_refused() {
        let d = tree("overlap", &[(18, "line 18 edited")]);
        let e = diff_at(&d, 20).unwrap_err().to_string();
        assert!(e.contains("0030-eighteen.patch"), "{e}");
        fs::remove_dir_all(&d).unwrap();
    }

    /// Run by `outer_repository_in_env_is_left_alone` with a hook's environment.
    #[test]
    #[ignore]
    fn in_hook_env() {
        let d = tree("hook", &[(10, "line 10 edited")]);
        assert!(diff_at(&d, 20).unwrap().contains("+line 10 edited"));
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn outer_repository_in_env_is_left_alone() {
        // A git hook runs with GIT_DIR etc. naming the repository being pushed.
        let outer = scratch("outer");
        git(&outer, &["init", "--quiet"]).unwrap();
        let snapshot = || -> Vec<(String, Vec<u8>)> {
            list_files(&outer.join(".git")).unwrap().into_iter().map(|f| { let b = fs::read(outer.join(".git").join(&f)).unwrap(); (f, b) }).collect()
        };
        let before = snapshot();
        let out = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "tests::in_hook_env"])
            .env("GIT_DIR", outer.join(".git")).env("GIT_WORK_TREE", &outer).env("GIT_INDEX_FILE", outer.join(".git/index"))
            .output().unwrap();
        let log = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success() && log.contains("1 passed"), "{log}");
        assert!(before == snapshot(), "the outer repository changed");
        fs::remove_dir_all(&outer).unwrap();
    }
}
