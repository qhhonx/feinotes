//! 发版自检: `quiet_md --self-test <empty dir> [report.json]` checks a shipped
//! build end to end. Unlike `devtools` (FDL_SCRIPT, development builds only) it
//! takes no script: the scenarios are compiled in, so a release can run them
//! and nothing else. See tools/harness/README.md › Release self-test.
//!
//! The command itself opens no window. It makes a library in `<dir>`, then
//! launches this executable twice with `QUIET_MD_SELF_TEST=1|2` and
//! `QUIET_MD_HOME` pointing there (the real library is never opened). Each
//! launch starts the app as usual (GPU, window, fonts, assets) and runs its
//! scenarios in-process, waiting on library files and app state, never on a
//! fixed delay or on pixels. Launch 1 quits through the app's own quit path;
//! launch 2 checks what it left. The report has every scenario's result and
//! time; a failed scenario adds a PNG of the window beside it. Exit code 0
//! means everything passed.

use crate::app::Workspace;
use gpui::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use web_time::Instant;

const PHASE_ENV: &str = "QUIET_MD_SELF_TEST";
const DIR_ENV: &str = "QUIET_MD_SELF_TEST_DIR";
/// Seconds one launch may take before it counts as hung (default 180).
const TIMEOUT_ENV: &str = "QUIET_MD_SELF_TEST_TIMEOUT";

/// Words the scenarios look for; unique enough not to occur in the guides.
const TITLE: &str = "Self-test 自检";
const BODY: &str = "Quick brown fox 敏捷的狐狸 selftestneedle";
/// What 复制为 Markdown makes of that document.
const MARKDOWN: &str = "# Self-test 自检\n\nQuick brown fox 敏捷的狐狸 selftestneedle\n";

/// This run's launch (1 or 2), when it is one.
fn phase() -> Option<u8> {
    static PHASE: OnceLock<Option<u8>> = OnceLock::new();
    *PHASE.get_or_init(|| std::env::var(PHASE_ENV).ok().and_then(|p| p.parse().ok()).filter(|p| (1..=2).contains(p)))
}

/// A self-test launch: like a scripted run, other apps, dialogs, the
/// global shortcut and the real updater stay out of it.
pub fn active() -> bool { phase().is_some() }

#[derive(Serialize, Deserialize, Clone)]
struct Outcome {
    name: String,
    ok: bool,
    ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot: Option<String>,
}

/// One launch's results, rewritten after every scenario: `running` names the
/// one under way if the process dies.
#[derive(Serialize, Deserialize, Default)]
struct PhaseReport { scenarios: Vec<Outcome>, running: Option<String>, done: bool }

/// What launch 1 leaves for launch 2.
#[derive(Serialize, Deserialize, Default)]
struct Carry { doc: String, folder: String, entries: usize }

// ---- the command

/// `--self-test <dir> [report]`: runs both launches; the exit code, if asked.
pub fn command() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    if args.next()? != "--self-test" { return None }
    let Some(dir) = args.next().map(PathBuf::from) else {
        eprintln!("usage: quiet_md --self-test <empty directory> [report.json]");
        return Some(2);
    };
    let report = args.next().map(PathBuf::from).unwrap_or_else(|| dir.join("report.json"));
    Some(match drive(&dir, &report) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(e) => { eprintln!("self-test: {e:#}"); 2 }
    })
}

fn drive(dir: &Path, report: &Path) -> anyhow::Result<bool> {
    // Only ever a fresh folder: the scenarios write, move and delete in it.
    if dir.exists() && std::fs::read_dir(dir)?.next().is_some() { anyhow::bail!("{} is not empty", dir.display()) }
    std::fs::create_dir_all(dir)?;
    let dir = std::path::absolute(dir)?;
    // A library path with spaces and CJK, as users' folders have.
    let library = dir.join("文档库 Library");
    let exe = std::env::current_exe()?;
    let timeout = std::env::var(TIMEOUT_ENV).ok().and_then(|s| s.parse().ok()).unwrap_or(180);
    let start = Instant::now();
    let mut all = vec![];
    let mut ok = true;
    for phase in 1..=2u8 {
        let file = dir.join(format!("phase-{phase}.json"));
        let out = std::fs::File::create(dir.join(format!("phase-{phase}.log")))?;
        let mut child = std::process::Command::new(&exe)
            .env(PHASE_ENV, phase.to_string()).env(DIR_ENV, &dir).env("QUIET_MD_HOME", &library)
            .env("QUIET_MD_LOG_DIR", dir.join("logs"))
            .env_remove("FDL_SCRIPT").env_remove("QUIET_MD_UPDATE_URL").env_remove("QUIET_MD_ONBOARDING")
            .stdin(std::process::Stdio::null()).stdout(out.try_clone()?).stderr(out)
            .spawn()?;
        let t0 = Instant::now();
        let status = loop {
            if let Some(s) = child.try_wait()? { break Some(s) }
            if t0.elapsed() > Duration::from_secs(timeout) { let _ = child.kill(); let _ = child.wait(); break None }
            std::thread::sleep(Duration::from_millis(100));
        };
        let mut got: PhaseReport = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let clean = status.is_some_and(|s| s.success()) && got.done;
        if !clean {
            let why = match status { None => format!("launch {phase} hung for {timeout}s"), Some(s) => format!("launch {phase} ended ({s}) before finishing") };
            let tail = log_tail(&dir.join("logs"));
            let name = got.running.take().unwrap_or_else(|| format!("launch-{phase}"));
            got.scenarios.push(Outcome { name, ok: false, ms: t0.elapsed().as_millis() as u64, error: Some(format!("{why}\n{tail}")), screenshot: None });
        }
        ok &= clean && got.scenarios.iter().all(|s| s.ok);
        all.extend(got.scenarios);
        // Launch 2 checks launch 1's work: nothing to check after a crash.
        if !clean { break }
    }
    let json = serde_json::json!({
        "app": "quiet.md", "version": env!("CARGO_PKG_VERSION"), "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
        "exe": exe, "library": library, "ok": ok, "ms": start.elapsed().as_millis() as u64, "scenarios": all,
    });
    if let Some(p) = report.parent() { std::fs::create_dir_all(p)?; }
    std::fs::write(report, serde_json::to_vec_pretty(&json)?)?;
    for s in &all {
        println!("{} {:>6} ms  {}{}", if s.ok { "ok  " } else { "FAIL" }, s.ms, s.name, s.error.as_deref().map(|e| format!(": {e}")).unwrap_or_default());
    }
    println!("self-test {} in {} ms; report {}", if ok { "passed" } else { "FAILED" }, start.elapsed().as_millis(), report.display());
    Ok(ok)
}

/// The end of the app's log (a panic's message and place are written there).
fn log_tail(logs: &Path) -> String {
    let text = std::fs::read_to_string(logs.join("quiet-md.log")).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    // From the last panic's message, else the last lines.
    let from = lines.iter().rposition(|l| l.contains("PANIC ")).unwrap_or(lines.len().saturating_sub(20));
    lines[from..lines.len().min(from + 8)].join("\n")
}

// ---- inside a launch

pub fn start(window: &mut Window, cx: &mut App) {
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let Some(phase) = phase() else { return };
    let Some(dir) = std::env::var_os(DIR_ENV).map(PathBuf::from) else { return };
    if STARTED.swap(true, std::sync::atomic::Ordering::Relaxed) { return }
    let any = window.window_handle();
    let Some(ws) = any.downcast::<Workspace>() else { return };
    cx.spawn(async move |cx| {
        let mut run = Run { cx: cx.clone(), any, ws, file: dir.join(format!("phase-{phase}.json")), dir, report: PhaseReport::default(), carry: Carry::default() };
        if phase == 1 { run.phase1().await } else { run.phase2().await }
        run.report.done = true;
        run.save();
        cx.update(crate::app::quit);
    }).detach();
}

type Step = Result<(), String>;

struct Run {
    cx: AsyncApp,
    any: AnyWindowHandle,
    ws: WindowHandle<Workspace>,
    dir: PathBuf,
    file: PathBuf,
    report: PhaseReport,
    carry: Carry,
}

/// Runs one scenario: its result and time go in the report (a PNG of the
/// window when it fails).
macro_rules! scenario {
    ($run:ident, $name:literal, $body:expr) => {{
        $run.report.running = Some($name.into());
        $run.save();
        let t0 = Instant::now();
        let r: Step = $body.await;
        let screenshot = r.is_err().then(|| $run.shot($name)).flatten();
        $run.report.running = None;
        $run.report.scenarios.push(Outcome { name: $name.into(), ok: r.is_ok(), ms: t0.elapsed().as_millis() as u64, error: r.err(), screenshot });
        $run.save();
    }};
}

impl Run {
    async fn phase1(&mut self) {
        scenario!(self, "first-launch-guides", self.first_launch());
        scenario!(self, "new-document-save", self.new_document());
        scenario!(self, "copy-as-markdown", self.copy_markdown());
        scenario!(self, "search", self.search());
        scenario!(self, "import-markdown-and-folder", self.import());
        scenario!(self, "trash-and-restore", self.trash_restore());
        scenario!(self, "board-opens", self.board());
        scenario!(self, "update-check-and-signature", self.update());
        self.carry.entries = self.ws(|w, _, _| w.entries.len()).unwrap_or(0);
        let _ = std::fs::write(self.dir.join("carry.json"), serde_json::to_vec(&self.carry).unwrap_or_default());
    }

    async fn phase2(&mut self) {
        self.carry = std::fs::read(self.dir.join("carry.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        scenario!(self, "reopen-keeps-content", self.reopen());
    }

    fn save(&self) {
        if let Ok(b) = serde_json::to_vec_pretty(&self.report) { let _ = std::fs::write(&self.file, b); }
    }

    fn ws<R>(&mut self, f: impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) -> R) -> Result<R, String> {
        self.ws.update(&mut self.cx, f).map_err(|e| format!("window gone: {e}"))
    }

    /// Polls `f` (drawing frames meanwhile, as the display link would) until
    /// it holds, for at most `secs`.
    async fn until(&mut self, what: &str, secs: u64, mut f: impl FnMut(&mut Workspace, &mut Window, &mut Context<Workspace>) -> bool) -> Step {
        let end = Instant::now() + Duration::from_secs(secs);
        loop {
            if self.ws(&mut f)? { return Ok(()) }
            self.draw();
            if Instant::now() > end {
                // What the window said went wrong, if it did.
                let said = self.ws(|w, _, _| match &w.status { crate::app::SaveStatus::Failed(m) => format!(" ({m})"), _ => String::new() }).unwrap_or_default();
                return Err(format!("timed out after {secs}s waiting for {what}{said}"))
            }
            self.cx.background_executor().timer(Duration::from_millis(20)).await;
        }
    }

    fn draw(&mut self) {
        let _ = self.any.update(&mut self.cx, |_, window, cx| if window.needs_draw() { window.draw(cx).clear(cx); });
    }

    /// The window rendered offscreen: it must have drawn something.
    fn render(&mut self) -> Result<image::RgbaImage, String> {
        let img = self.any.update(&mut self.cx, |_, window, cx| window.render_offscreen(cx)).map_err(|e| e.to_string())?.map_err(|e| format!("render: {e}"))?;
        let mut colors = std::collections::HashSet::new();
        for p in img.pixels().step_by(7) { colors.insert(p.0); if colors.len() > 16 { return Ok(img) } }
        Err(format!("the window rendered blank ({} colours)", colors.len()))
    }

    fn shot(&mut self, name: &str) -> Option<String> {
        let img = self.any.update(&mut self.cx, |_, window, cx| window.render_offscreen(cx)).ok()?.ok()?;
        let path = self.dir.join(format!("failed-{name}.png"));
        img.save(&path).ok()?;
        Some(path.to_string_lossy().into_owned())
    }

    fn type_text(&mut self, text: &str) -> Step {
        self.any.update(&mut self.cx, |_, window, cx| {
            for ch in text.chars() {
                let s = ch.to_string();
                window.dispatch_keystroke(Keystroke { modifiers: Modifiers::default(), key: if ch == ' ' { "space".into() } else { s.clone() }, key_char: Some(s) }, cx);
            }
        }).map_err(|e| e.to_string())
    }

    fn key(&mut self, stroke: &str) -> Step {
        let ks = crate::shortcuts::parse_key(stroke).map_err(|e| format!("{stroke}: {e:?}"))?;
        self.any.update(&mut self.cx, |_, window, cx| { window.dispatch_keystroke(ks, cx); }).map_err(|e| e.to_string())
    }

    fn entry_text(&mut self, id: &str) -> Result<String, String> {
        let id = id.to_string();
        self.ws(move |w, _, _| w.store.load(&id).map(|d| format!("{}\n{}", d.title.plain_text(), d.plain_text())).map_err(|e| format!("{e:#}")))?
    }

    fn file_of(&mut self, id: &str) -> Result<PathBuf, String> {
        let i = id.to_string();
        // A background write (import, move) leaves the window's store to
        // reread the index when it next lists.
        self.ws(move |w, _, _| { w.store.list(); w.store.file_path(&i) })?.ok_or_else(|| format!("no file for {id}"))
    }

    // ---- the scenarios

    /// An empty library is seeded with 开始指南, and the window draws.
    async fn first_launch(&mut self) -> Step {
        self.until("the guides in the list", 30, |w, _, _| !w.entries.is_empty()).await?;
        let root = crate::store::Store::active_dir();
        if !crate::guides::seeded(&root).map_err(|e| e.to_string())? { return Err("library not marked as seeded".into()) }
        let ids: Vec<String> = self.ws(|w, _, _| w.entries.iter().filter(|e| e.meta.folder.is_none()).map(|e| e.id.clone()).collect())?;
        if ids.len() < 3 { return Err(format!("only {} guide documents", ids.len())) }
        for id in &ids {
            let f = self.file_of(id)?;
            if !f.is_file() { return Err(format!("{} is missing", f.display())) }
        }
        self.render()?;
        Ok(())
    }

    /// ⌘N's document: typed into, then saved by the app on its own.
    async fn new_document(&mut self) -> Step {
        let before = self.ws(|w, _, _| w.entries.len())?;
        self.ws(|w, window, cx| w.create_doc(false, None, window, cx))?;
        self.until("the new document to open", 10, move |w, _, _| w.entries.len() > before && w.editor.is_some()).await?;
        let id = self.ws(|w, _, _| w.current.clone())?.ok_or("nothing open")?;
        self.type_text(TITLE)?;
        self.key("enter")?;
        self.type_text(BODY)?;
        self.until("the editor to have the text", 5, |w, _, cx| w.editor.as_ref().is_some_and(|e| e.read(cx).core.doc.plain_text().contains("selftestneedle"))).await?;
        self.until("the save", 10, |w, _, _| !w.unsaved).await?;
        let file = self.file_of(&id)?;
        let disk = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        if !disk.contains(TITLE) || !disk.contains(BODY) { return Err(format!("{} lacks the typed text:\n{disk}", file.display())) }
        self.carry.doc = id;
        Ok(())
    }

    /// 复制为 Markdown of the whole document, through the system clipboard
    /// (put back afterwards).
    async fn copy_markdown(&mut self) -> Step {
        let kept = self.cx.update(|cx| cx.read_from_clipboard());
        self.cx.update(|cx| cx.write_to_clipboard(ClipboardItem::new_string(String::new())));
        // A collapsed caret copies the whole document.
        self.ws(|w, window, cx| if let Some(e) = &w.editor { let f = e.read(cx).focus.clone(); window.focus(&f, cx); })?;
        self.any.update(&mut self.cx, |_, window, cx| window.dispatch_action(Box::new(crate::editor::keymap::CopyMarkdown), cx)).map_err(|e| e.to_string())?;
        let mut got = String::new();
        let r = self.until("the clipboard", 5, |_, _, cx| { got = cx.read_from_clipboard().and_then(|c| c.text()).unwrap_or_default(); !got.is_empty() }).await;
        if let Some(item) = kept { self.cx.update(|cx| cx.write_to_clipboard(item)); }
        r?;
        let got = got.replace("\r\n", "\n");
        if got != MARKDOWN { return Err(format!("expected {MARKDOWN:?}, got {got:?}")) }
        Ok(())
    }

    /// The search dialog finds the new document by a word of its body, in
    /// Latin and in CJK.
    async fn search(&mut self) -> Step {
        let doc = self.carry.doc.clone();
        for q in ["selftestneedle", "敏捷的狐狸"] {
            self.ws(move |w, window, cx| { w.open_search(window, cx); w.search.update(cx, |s, cx| s.set_text(q, cx)); })?;
            let d = doc.clone();
            self.until(&format!("a hit for {q}"), 5, move |w, _, cx| w.search_results(cx).iter().any(|e| e.id == d)).await?;
            self.ws(|w, window, cx| w.close_search(window, cx))?;
        }
        Ok(())
    }

    /// A Markdown file and a folder of them (spaces and CJK in every name).
    async fn import(&mut self) -> Step {
        let src = self.dir.join("导入 素材 Import");
        let file = src.join("单个 笔记.md");
        let folder = src.join("文件夹 Folder");
        let write = |p: &Path, s: &str| { std::fs::create_dir_all(p.parent().unwrap()).and_then(|_| std::fs::write(p, s)).map_err(|e| e.to_string()) };
        write(&file, "# 单个 笔记\n\nimportedfilemarker with **bold** and `code`.\n\n- one\n- two\n")?;
        write(&folder.join("甲 a.md"), "importedfoldermarkerA\n")?;
        write(&folder.join("子目录 Sub").join("乙 b.md"), "importedfoldermarkerB\n")?;
        let find = |marker: &'static str| move |w: &mut Workspace| w.entries.iter().find(|e| e.body.contains(marker)).map(|e| e.id.clone());
        self.ws(|w, window, cx| w.import_paths(std::slice::from_ref(&file), None, false, window, cx))?;
        self.until("the imported file", 15, move |w, _, _| find("importedfilemarker")(w).is_some()).await?;
        self.ws(|w, window, cx| w.import_folder(&folder, None, false, window, cx))?;
        self.until("the imported folder", 15, move |w, _, _| find("importedfoldermarkerA")(w).is_some() && find("importedfoldermarkerB")(w).is_some()).await?;
        let (one, a, b, dir) = self.ws(move |w, _, _| (
            find("importedfilemarker")(w), find("importedfoldermarkerA")(w), find("importedfoldermarkerB")(w),
            w.entries.iter().find(|e| e.meta.folder.as_deref() == Some("文件夹 Folder")).map(|e| e.id.clone()),
        ))?;
        let dir = dir.ok_or("no folder 文件夹 Folder in the library")?;
        for (id, marker) in [(one.clone(), "importedfilemarker"), (a, "importedfoldermarkerA"), (b.clone(), "importedfoldermarkerB")] {
            let id = id.ok_or(marker)?;
            let f = self.file_of(&id)?;
            let disk = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            if !disk.contains(marker) { return Err(format!("{} lacks {marker}", f.display())) }
        }
        // The sub-folder came along: b sits two folders down.
        let bf = self.file_of(&b.unwrap())?;
        if !bf.parent().is_some_and(|p| p.ends_with(Path::new("文件夹 Folder").join("子目录 Sub"))) { return Err(format!("{} is not under 文件夹 Folder/子目录 Sub", bf.display())) }
        if !self.entry_text(one.as_deref().unwrap())?.contains("bold") { return Err("the imported file lost its text".into()) }
        self.carry.folder = dir;
        Ok(())
    }

    /// 移到回收站, then 恢复: the file leaves its place and comes back.
    async fn trash_restore(&mut self) -> Step {
        let id = self.ws(|w, _, _| w.entries.iter().find(|e| e.body.contains("importedfilemarker")).map(|e| e.id.clone()))?.ok_or("no imported file")?;
        let file = self.file_of(&id)?;
        let i = id.clone();
        self.ws(move |w, window, cx| w.delete_docs(std::slice::from_ref(&i), window, cx))?;
        let i = id.clone();
        self.until("the document in 回收站", 10, move |w, _, _| w.trash.iter().any(|t| t.id == i) && !w.entries.iter().any(|e| e.id == i)).await?;
        if file.exists() { return Err(format!("{} is still in place after deleting", file.display())) }
        let i = id.clone();
        self.ws(move |w, _, cx| w.restore_doc(&i, cx))?;
        let i = id.clone();
        self.until("the restored document", 10, move |w, _, _| w.entries.iter().any(|e| e.id == i) && !w.trash.iter().any(|t| t.id == i)).await?;
        let back = self.file_of(&id)?;
        if !std::fs::read_to_string(&back).is_ok_and(|s| s.contains("importedfilemarker")) { return Err(format!("{} not restored", back.display())) }
        Ok(())
    }

    /// 新建 › 画板 opens into the board, which draws.
    async fn board(&mut self) -> Step {
        self.ws(|w, window, cx| w.create_board(None, window, cx))?;
        self.until("the board to open", 10, |w, _, cx| w.editor.as_ref().is_some_and(|e| { let e = e.read(cx); e.board_file && e.board_open() })).await?;
        self.settle().await?;
        let id = self.ws(|w, _, _| w.current.clone())?.ok_or("nothing open")?;
        if !self.file_of(&id)?.is_file() { return Err("no board file".into()) }
        self.render()?;
        Ok(())
    }

    /// Frames until nothing more is invalidated (images, layout), at most 2s.
    async fn settle(&mut self) -> Step {
        let end = Instant::now() + Duration::from_secs(2);
        loop {
            let dirty = self.any.update(&mut self.cx, |_, window, cx| { let d = window.needs_draw(); if d { window.draw(cx).clear(cx); } d }).map_err(|e| e.to_string())?;
            if !dirty || Instant::now() > end { return Ok(()) }
            self.cx.background_executor().timer(Duration::from_millis(16)).await;
        }
    }

    /// 检查更新 against a local server: the manifest is read and validated,
    /// the package downloads, and the release key turns down its signature
    /// (made with a throwaway key, which does verify it).
    async fn update(&mut self) -> Step {
        let platform = crate::update::platform().ok_or("no update platform")?;
        let name = crate::update::package_name(UPDATE_VERSION, platform);
        let signature = update_signature(&name).ok_or("no test signature for this platform")?;
        let url = serve_update(&name, &signature).map_err(|e| format!("local server: {e}"))?;
        let dir = self.dir.join("updates");
        crate::update::redirect(url, dir.clone());
        self.cx.update(|cx| crate::update::check(true, cx));
        let mut state = crate::update::State::Idle;
        self.until("the manifest", 20, |_, _, cx| { state = crate::update::state(cx); !matches!(state, crate::update::State::Checking | crate::update::State::Idle) }).await?;
        match &state {
            crate::update::State::Available(m) if m.version == UPDATE_VERSION => {}
            s => return Err(format!("check ended in {s:?}")),
        }
        self.cx.update(crate::update::download);
        self.until("the download", 30, |_, _, cx| { state = crate::update::state(cx); !matches!(state, crate::update::State::Downloading(_) | crate::update::State::Available(_)) }).await?;
        match &state {
            // Turned down twice: the second download goes past caches.
            crate::update::State::Failed(e) if e.contains("signature") => {}
            s => return Err(format!("a package signed by another key ended in {s:?}")),
        }
        let bytes = std::fs::read(dir.join(format!("{name}.part"))).map_err(|e| format!("downloaded package: {e}"))?;
        if bytes != UPDATE_PACKAGE { return Err("the downloaded package differs from the served one".into()) }
        let key = minisign_verify::PublicKey::from_base64(UPDATE_KEY).map_err(|e| e.to_string())?;
        if !crate::update::signed(&bytes, &signature, &name, &key) { return Err("the test key does not verify the package".into()) }
        Ok(())
    }

    /// Launch 2: the document typed in launch 1 opens with its text, and the
    /// library was not seeded again.
    async fn reopen(&mut self) -> Step {
        if self.carry.doc.is_empty() { return Err("launch 1 made no document".into()) }
        let n = self.carry.entries;
        self.until("the library", 30, |w, _, _| !w.entries.is_empty()).await?;
        let have = self.ws(|w, _, _| w.entries.len())?;
        if have != n { return Err(format!("{have} entries after relaunch, {n} before")) }
        let (doc, folder) = (self.carry.doc.clone(), self.carry.folder.clone());
        if !self.ws(move |w, _, _| w.entries.iter().any(|e| e.id == folder))? { return Err("the imported folder is gone".into()) }
        let d = doc.clone();
        self.ws(move |w, window, cx| w.open(&d, window, cx))?;
        let d = doc.clone();
        self.until("the document to open", 10, move |w, _, _| w.current.as_deref() == Some(&d) && w.editor.is_some()).await?;
        let text = self.ws(|w, _, cx| w.editor.as_ref().map(|e| { let c = &e.read(cx).core.doc; format!("{}\n{}", c.title.plain_text(), c.plain_text()) }))?.unwrap_or_default();
        if !text.contains(TITLE) || !text.contains(BODY) { return Err(format!("reopened text: {text:?}")) }
        self.render()?;
        Ok(())
    }
}

// ---- the update server

const UPDATE_VERSION: &str = "99.0.0";
const UPDATE_PACKAGE: &[u8] = b"quiet.md self-test package\n";
/// A throwaway minisign key (not the release key) and its signatures of
/// `UPDATE_PACKAGE` under each platform's package name for `UPDATE_VERSION`.
const UPDATE_KEY: &str = "RWSAXatwncDz4nQwcTTXRM1RbeEkn/+LQLg5kPbhNrWytmawPGQTB7ha";
const UPDATE_SIG: &str = "RUSAXatwncDz4j6nKamMtjGoHaCy11n2Iv+Ny4fuFOFJDVqup2A7n8yKuJ6ZWJKR3hU/sTtSUyC9G3PqntbEdU3nIpsmkajAsQc=";
const UPDATE_GLOBAL: &[(&str, &str)] = &[
    ("quiet-md-99.0.0-macos-arm64.zip", "RTMt7kbuSwZqUNc9Aqh3hasvD8vHz6kLEzykSVnF4UVRAStTBZB8weX87n+yzsLCNLOtPJTwgwXyBdYDJZ4KAw=="),
    ("quiet-md-99.0.0-macos-x86_64.zip", "GjUIl9SPa5CJmv6cvRiXilWjKyTGcuAnZYAXWa8MfH9bqWxESYu4/wa2f1qOSWZsKNNtrtsQ2r9VjL8PNk/LDg=="),
    ("quiet-md-99.0.0-windows-x64-setup.exe", "eufYxDk9CukTSPEeaoeMcgVauDpJis4/2ef0MLOBi87Xm3sRvwu7GcaXTxR5p3yosrrcmwKSb/frX7ew37EPDg=="),
    ("quiet-md-99.0.0-x86_64.AppImage", "bR7dVffRIGbO0y0RBIpHtFqnhlADJcZF8fBkpYc0atxmO3ZIkbTXxITZJhHa+im0vx9rq7z9IcEftZXzGAe+Bg=="),
];

fn update_signature(name: &str) -> Option<String> {
    let (_, global) = UPDATE_GLOBAL.iter().find(|(n, _)| *n == name)?;
    Some(format!("untrusted comment: signature from minisign secret key\n{UPDATE_SIG}\ntrusted comment: {name}\n{global}\n"))
}

/// Serves the manifest and the package on 127.0.0.1 for the rest of the run;
/// the update URL to use.
fn serve_update(name: &str, signature: &str) -> std::io::Result<String> {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let base = format!("http://{}", listener.local_addr()?);
    let platform = crate::update::platform().unwrap_or_default();
    let manifest = serde_json::json!({ "version": UPDATE_VERSION, "url": format!("{base}/v{UPDATE_VERSION}/{name}"), "size": UPDATE_PACKAGE.len(), "signature": signature }).to_string();
    let files = [(format!("/update-{platform}.json"), manifest.into_bytes()), (format!("/v{UPDATE_VERSION}/{name}"), UPDATE_PACKAGE.to_vec())];
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buf = vec![0u8; 4096];
            let mut got = 0;
            while got < buf.len() && !buf[..got].windows(4).any(|w| w == b"\r\n\r\n") {
                match stream.read(&mut buf[got..]) { Ok(0) | Err(_) => break, Ok(n) => got += n }
            }
            let req = String::from_utf8_lossy(&buf[..got]);
            // As a CDN does: the query (the updater's `nocache=` retry) is not part of the file.
            let path = req.split_whitespace().nth(1).unwrap_or("").split('?').next().unwrap_or("");
            let reply = match files.iter().find(|(p, _)| p == path) {
                Some((_, body)) => [format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes(), body.clone()].concat(),
                None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
            };
            let _ = stream.write_all(&reply);
        }
    });
    Ok(base)
}
