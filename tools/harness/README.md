# UI script harness

Use `FDL_SCRIPT` to drive a window and take screenshots with a line-oriented script. Each run first copies `lib_dir` to `work_dir/lib` and points `QUIET_MD_HOME` at the copy to keep the user's library safe.

```sh
cargo build --release --features devtools   # Production builds omit the script driver
python3 tools/harness/run.py tools/harness/examples/scroll.txt            # Uses lib_clean and writes to /tmp/fdl-run
python3 tools/harness/run.py my.txt /path/to/lib /tmp/my-run              # Custom input library and output directory
FDL_BIN=/other/quiet_md python3 tools/harness/run.py my.txt
```

- Screenshots go to `<work_dir>/<name>.png`. An offscreen `shot` draws the transparent sidebar on the window background, as in fullscreen, without the desktop or system glass. Use `deskshot` to capture the actual glass. Comparing against an older binary with a black sidebar will show sidebar differences.
- Logs go to `<work_dir>/log`. Run `grep -a bench log` for benchmark results.
- `lib_clean/` contains a review document covering all block styles and several example images.
- Run `swift winid.swift` to list on-screen quiet_md window IDs for system screenshots.

## Screenshot regression checks

```sh
python3 tools/harness/compare.py tools/harness/examples/menus.txt <old-binary> target/release/quiet_md
```

- The comparison runs the same script against both binaries and compares every screenshot pixel by pixel. It reports the number and bounds of changed pixels and writes differences to `/tmp/fdl-compare/diff-<name>.png`.
- For appearance-preserving refactors such as splitting modules, replacing tokens or sharing components, every image should report `identical`.
- `examples/menus.txt` opens the More menu and page width submenu, edit mode menu, slash menu, block menu, sidebar context menu and help menu in sequence.
- `python3 tools/harness/table_resize.py` checks that resizing columns preserves page position in a temporary library, covering an offscreen caret, a caret in a cell, a drag with no change, saving, undo and redo. Requires Pillow.
- `python3 tools/harness/create_menu_stable.py` opens 我的文档's "+" menu, adds a file to the library while it is open and checks that the menu, its 导入 rows included, looks the same seconds later (every 新建 menu shows them). Requires Pillow.
- `python3 tools/harness/table_paste.py` uses a simulated clipboard to verify content-based initial Markdown column widths, including Chinese text, paste, import, save/reopen and undo/redo. It also checks that existing widths survive.
- `python3 tools/harness/checkbox_toggle.py` checks that clicking a task checkbox changes completion while preserving the caret, selection and scroll position. It covers both themes, nesting, callouts, tables, save and undo/redo in a temporary library. Requires Pillow. Set `CHECKBOX_CASES` to a comma-separated list of scenarios.
- `python3 tools/harness/board_text_inplace.py` checks that a board text stays where it is when its editor opens: the text as drawn and in the editor must match pixel for pixel but for the caret. It covers shapes (rounded and sharp rectangle, ellipse, diamond), free text, an arrow label, several and wrapped lines, Chinese and English, the hand-drawn font with Xiaolai and Nunito, 120% zoom and both themes. Requires Pillow.
- `python3 tools/harness/blank_lines.py` checks that blank lines and spaces typed in a page stay, in a Markdown file and a DocxXML page: Enter pressed several times, a line of only spaces, spaces at a line's start, middle and end. After the autosave the page must look the same when another program changes the file and the app takes it in, when it is opened again and after a relaunch, and each file must hold exactly the expected text. Requires Pillow.
- `python3 tools/harness/scroll_intent.py` checks that folding and expanding headings preserves the viewport and selection, a hidden caret stays visible after moving to the heading, and typing or arrow keys still follow the caret. It also checks persistence and position when resizing columns or wrapping code in either theme. Requires Pillow. Select scenarios with `SCROLL_CASES`.
- `python3 tools/harness/pointer_properties.py` checks image alignment, attachment cards and previews, callout colors/icons/defaults, countdowns, date reminders, and cell backgrounds and vertical alignment. It verifies viewport and editing position, saves and undo/redo. Requires Pillow. Select comma-separated scenarios with `PROPERTY_CASES`. An isolated undo scenario first verifies its base operation.
- `python3 tools/harness/clipboard_images.py` uses the simulated clipboard for TIFF screenshots, images accompanied by text, multiple images, insertion at the caret, saving, undo/redo and plain or rich text paste. Requires Pillow.
- `python3 tools/harness/gpui_patches.py` checks vendored GPUI behavior (layout, scrollbars, single-axis scrolling, image memory, custom shortcuts, first launch). See the validation section in vendor/PATCHES.md. Set `FDL_REF_BIN` to the pre-upgrade binary to compare line positions and ink coverage. Its libraries are pinned to the light look, and it reads the capture scale from the first shot, so it runs on a 1x display as well as a Retina one. Pass scenario names such as `text`, `scrollbar` or `images` on the command line. Requires Pillow.
- These scripts create legacy libraries (`docs/<id>.xml` and `index.json`). The app migrates them to FORMAT 4 on open, including title-based files, real folders and `.assets/`. Read saved files with `doc_file` in `hlib.py` and convert expected asset paths with `as_migrated`.
- `python3 tools/harness/optimistic_rename.py` checks 重命名 written in the background (`src/tree_writes.rs`): the row has the new name before the folder does, a page inside opens and the open one saves in the renamed folder, a second rename before the first lands, a failed rename back in its field with what was typed, 移动到 into another folder (and one that fails), a taken top-level folder name. It holds each write back with `FDL_TREE_WRITE_DELAY`.
- `python3 tools/harness/import_open.py` checks that what an import brings in is in front as soon as it finishes: a Markdown file, a Markdown folder, an Obsidian vault (as a folder and through 从其他应用导入…), a 思源笔记 workspace and a vault the 从其他应用导入… panel finds in a stand-in home folder, each through a folder's "+" menu into a temporary library (an Obsidian vault takes 导入一份副本 when asked). It uses the `showing <title>` step.
- `python3 tools/harness/paste_remote_images.py` uses local HTTP and a simulated clipboard to verify remote images pasted from webpages become library assets (#220). It covers undo/redo, zero network requests when automatic downloads are disabled, and preservation of expired links. Requires Pillow.
- Comparisons disable caret blinking (`FDL_NOBLINK`). Move the pointer to an empty spot at the start of the script so the real cursor position does not trigger hover in the screenshots.
- To build an older binary, run `git worktree add /tmp/old <commit>`, then `CARGO_TARGET_DIR=<repository>/target cargo build --release --features devtools` there. Copy the resulting binary out before building another version.

## Script commands

Write one command per line. Lines beginning with `#` are comments. See the top of `src/devtools.rs` for the full reference.

| Command | Action |
|---|---|
| `wait <ms>` | Wait while drawing frames, even with the screen locked. |
| `key <key>` | Send a key, e.g. `key cmd-b` or `key escape`. |
| `type <text>` | Type text, including trailing spaces. |
| `move x y` / `click x y` / `dclick x y` / `rclick x y` / `drag x1 y1 x2 y2` | Move, click, double-click, right-click or drag in window coordinates. |
| `row <title>` / `rrow <title>` / `hoverrow <title>` | Click, right-click or hover the middle of the sidebar row showing that text, as drawn in a frame drawn for the command: 主页, 我的文档, a document or folder, 设置, a page in 设置's own list. Menu items drawn with `design::MenuItem` count too, by their label: the + menu's 画板, a board's right-click menu. Use the interface language's text (`Home` in English). The topmost row wins if several match. If no such row is on screen, the run stops with exit code 3 and the log lists the rows it could see; `run.py` then exits with an error too. Prefer these to coordinates: the sidebar's rows move when it changes. |
| `anchor <name>` | Click the middle of a control the app tracks by name, e.g. `anchor create-folder` (a folder page's "+"), `create-sidebar` (我的文档's "+"), `doc-font-picker` / `ui-font-picker` (外观's font lists) or `font-del-<id>` (a downloaded font's delete). A missing name stops the run like `row`. |
| `showing <title>` | The document or folder page in front has that title; otherwise the run stops with exit code 3, as `row` does. |
| `origin board` / `origin window` | Count the points of the pointer and file steps that follow (`click`, `drag`, `move`, `rclick`, `filedrag`…) from the open 画板's top-left corner, or from the window's again. The board opens in the note pane, so write board scripts this way rather than in window coordinates. No board open stops the run like `row`. Offsets (`dragby`, wheel deltas) are not moved. |
| `down x y` / `dragto x y` / `dragby dx dy` / `up x y` | Press, drag to an absolute point, drag relative to the last point or release. Use `bench 30 dragby 4 0` to time individual steps. |
| `mods shift cmd` / `mods none` | Hold these modifier keys (shift, cmd, alt, ctrl) for the mouse steps that follow; `none` lets go. |
| `mdown x y` / `mdragto x y` / `mup x y` | Press, drag or release the middle (wheel) button. |
| `wheel x y dy [dx]` | Send one wheel event. Optional horizontal delta simulates a trackpad. Positive values reveal content to the right. |
| `pinch x y delta` | Send one trackpad pinch event at (x, y). A delta of 0.1 zooms in by 10%; negative values zoom out. |
| `shot <name>` / `burst <name> n` | Take one screenshot or n shots about 8 ms apart. |
| `mark <text>` / `commit <text>` | Set or commit IME composition text, after drawing the frame a preceding click asked for: a text field it opens (the board's text tool) takes input once painted. |
| `clip html\|text\|image <file>` / `clip clear` / `clip save <file>` | Set the simulated clipboard for a subsequent `key cmd-v`. Append multiple images or combine text and HTML. Use clear to empty it. `save` writes the text a copy put there to a file. |
| `pick <path>[\|<path>…]` | Supply the next open/save dialog response without opening a system panel. |
| `resize w h` | Resize the window. |
| `place x y` | Move the top-left corner to (x, y) on the main display above other windows. Use with `FDL_NOPARK` for desktop captures. |
| `deskshot <name> [margin]` | Capture the window and underlying desktop with a margin in points (default 48). Place wallpaper behind the window first with `shared-mock/guides/backdrop.swift`. See `shared-mock/guides/shots/shots.sh`. |
| `bench <n> <command>` | Repeat a command n times, synchronously drawing a frame each time and reporting timings. |
| `record <file.mp4>` / `record stop` / `glide` / `scroll` / `caption` | Record demo video. |
| `filedrag x y <path>[\|<path>…]` / `filedrop x y` / `fileleave` | Simulate dragging files from Finder to (x, y), dropping them there or leaving the window. |
| `activate` | Make the window key. This takes focus. Avoid it in scripts on the user's machine. |
| `quit` | Quit. `run.py` appends this automatically. |

### Environment variables

- `FDL_SHOTS` sets the screenshot directory (set by `run.py`).
- `FDL_TRACE` enables event tracing.
- `FDL_NOPARK` keeps the window on screen.
- `FDL_WINDOW_CAPTURE` uses the window server to capture system-drawn window controls.
- `FDL_NOBLINK` disables caret blinking (set by `compare.py`).
- `FDL_RECORD_FPS` sets the recording frame rate (default 30).
- `FDL_RECORD_START=<file.mp4>` records from the window's first frame, before the script starts (it waits about a second for the window to settle): launch and first-open loading. `record stop` ends it.
- `FDL_ANIMATE` makes `wait` draw a frame every 16 ms (and record each one), the main thread spinning in between. Animations ask the system for their frames, which it throttles for an inactive window and stops while the screen is locked, and timers fall behind with the display asleep; set it to record or shoot animations.
- `QUIET_MD_FONT_URL`, `QUIET_MD_FONT_DIR` and `QUIET_MD_TRIAL_FONTS` are passed through: downloadable fonts (the board's Xiaolai, document fonts) are fetched from that folder into that cache, a compiled-in Xiaolai is ignored, and the trial fonts are listed (docs/FONTS.md). `python3 tools/harness/board_font.py` checks the board's font and its download note, `python3 tools/harness/doc_fonts.py` the document font list (`--compare` shoots an article in every font).
- `QUIET_MD_UPDATE_URL` is passed through: the updater reads that folder instead of download.quiet.md. To test forced updates see docs/UPDATES.md. Downloads land in `~/Library/Caches/com.s1dashu.quietmd` of the `HOME` the app sees.
- `FDL_TREE_WRITE_DELAY=<ms>` holds every background write of the tree (a sidebar drop, 重命名) back that long, so a script can act while it is on its way.
- `QUIET_MD_COMPONENTS=1` opens only the 组件库 panel (`src/gallery/`, the same as `quiet_md --components`): no main window and no library; the script plays in the panel.

## Rules for running scripts

The harness runs on a computer someone may be using.

- Leave the real ⌘C/⌘V clipboard alone. Use `key cmd-v` only after `clip`.
- Avoid stealing focus. Never use `activate`. By default the harness moves the window offscreen to render it.
- Do not click Show in Finder in Settings › Library, or Open in Finder in the home or folder More menu.
- Do not click links, Web Search or link-opening buttons that launch a browser. Avoid Print (system print panel), Quick Look and `qlmanage -p`.
- Do not use the `open` command. Use `pick a|b` when a script needs to select files.
- Never use the real user library as `lib_dir`. If its contents are essential, make a read-only copy and keep user content out of committed artifacts.

## Release self-test

Shipped builds have no script driver, so a release is checked by scenarios compiled into the app (`src/selftest.rs`). `quiet_md --self-test <empty dir> [report.json]` takes no script and accepts no other input. It creates a library in `<dir>/文档库 Library` and starts the app twice from the same executable, with `QUIET_MD_HOME` set to that library. It never opens `~/Documents/Quiet.md`. Each launch goes through normal startup, including the GPU, window, fonts and assets, but skips onboarding, the global shortcut and focus stealing, as scripted runs do. It runs its scenarios in-process, then quits through the app's own quit path. Scenarios check library files and app state, not pixels, and between steps they wait on a condition instead of a fixed delay.

- **Launch 1:** seeding the Getting Started guides into an empty library (the window must render something other than a blank frame), a new document that is typed into and saved automatically, Copy as Markdown through the clipboard (the clipboard is restored afterwards), search in Latin and CJK, importing a Markdown file and a folder, moving to the trash and restoring, opening a new board, and checking for updates against a local server (`127.0.0.1`, never the internet). The update manifest must validate and the package must download. The release key must reject the package because it is signed with a throwaway key, which does verify it.
- **Launch 2:** the document from launch 1 reopens with its text, and the library has not been seeded again.

The report records each scenario's result and time in milliseconds. A failed scenario's entry gives the error and the path to `failed-<scenario>.png`, an offscreen render of the window. A crash or hang fails the scenario that was running. Its entry includes the panic from `<dir>/logs/quiet-md.log`. A launch counts as hung after `QUIET_MD_SELF_TEST_TIMEOUT` seconds (default 180). The command exits with 0 when every scenario passes, 1 when one fails, and 2 for usage errors. A full run takes about 2 seconds.

```sh
cargo build --release && target/release/quiet_md --self-test "$(mktemp -d)/run"   # Development build
scripts/self-test-macos.sh dist-release.*/quiet-md-<version>-macos-arm64.zip       # A package: unzipped into a path with spaces and CJK
scripts/self-test-windows.ps1 -Installer quiet-md-<version>-windows-x64-setup.exe  # Installs silently, runs, uninstalls
```

CI runs the macOS script on both packages, with Intel under Rosetta. It runs the Windows script on the installer, installing into a temporary folder whose path contains spaces and CJK. Both jobs upload `self-test/` (reports, logs and screenshots, without the library) as `self-test-macos` / `self-test-windows`. For a release, run the macOS script on each signed, notarized zip from `scripts/release.sh` before uploading it. A package signed with a team identity must also pass `spctl` (Gatekeeper).

To add a scenario, write an `async fn` on `Run` in `src/selftest.rs` that drives the app through `Workspace`/`Editor` methods or `type_text`/`key`, then waits with `until(what, secs, condition)` and returns `Err` with a message on failure. List it with `scenario!` in `phase1` or `phase2`. Pass anything launch 2 needs through `Carry`. Keep each scenario to a few seconds and its fixtures inside the run directory.
