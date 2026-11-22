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
- `python3 tools/harness/table_paste.py` uses a simulated clipboard to verify content-based initial Markdown column widths, including Chinese text, paste, import, save/reopen and undo/redo. It also checks that existing widths survive.
- `python3 tools/harness/checkbox_toggle.py` checks that clicking a task checkbox changes completion while preserving the caret, selection and scroll position. It covers both themes, nesting, callouts, tables, save and undo/redo in a temporary library. Requires Pillow. Set `CHECKBOX_CASES` to a comma-separated list of scenarios.
- `python3 tools/harness/scroll_intent.py` checks that folding and expanding headings preserves the viewport and selection, a hidden caret stays visible after moving to the heading, and typing or arrow keys still follow the caret. It also checks persistence and position when resizing columns or wrapping code in either theme. Requires Pillow. Select scenarios with `SCROLL_CASES`.
- `python3 tools/harness/pointer_properties.py` checks image alignment, attachment cards and previews, callout colors/icons/defaults, countdowns, date reminders, and cell backgrounds and vertical alignment. It verifies viewport and editing position, saves and undo/redo. Requires Pillow. Select comma-separated scenarios with `PROPERTY_CASES`. An isolated undo scenario first verifies its base operation.
- `python3 tools/harness/clipboard_images.py` uses the simulated clipboard for TIFF screenshots, images accompanied by text, multiple images, insertion at the caret, saving, undo/redo and plain or rich text paste. Requires Pillow.
- `python3 tools/harness/gpui_patches.py` checks vendored GPUI behavior (layout, scrollbars, single-axis scrolling, image memory, custom shortcuts, first launch). See the validation section in vendor/PATCHES.md. Set `FDL_REF_BIN` to the pre-upgrade binary to compare line positions and ink coverage. Pass scenario names such as `text`, `scrollbar` or `images` on the command line. Requires Pillow.
- These scripts create legacy libraries (`docs/<id>.xml` and `index.json`). The app migrates them to FORMAT 4 on open, including title-based files, real folders and `.assets/`. Read saved files with `doc_file` in `hlib.py` and convert expected asset paths with `as_migrated`.
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
| `down x y` / `dragto x y` / `dragby dx dy` / `up x y` | Press, drag to an absolute point, drag relative to the last point or release. Use `bench 30 dragby 4 0` to time individual steps. |
| `mods shift cmd` / `mods none` | Hold these modifier keys (shift, cmd, alt, ctrl) for the mouse steps that follow; `none` lets go. |
| `mdown x y` / `mdragto x y` / `mup x y` | Press, drag or release the middle (wheel) button. |
| `wheel x y dy [dx]` | Send one wheel event. Optional horizontal delta simulates a trackpad. Positive values reveal content to the right. |
| `pinch x y delta` | Send one trackpad pinch event at (x, y). A delta of 0.1 zooms in by 10%; negative values zoom out. |
| `shot <name>` / `burst <name> n` | Take one screenshot or n shots about 8 ms apart. |
| `mark <text>` / `commit <text>` | Set or commit IME composition text. |
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
- `FDL_IN_WINDOW_PROMPTS` uses in-window GPUI confirmations for scripted interactions.
- `FDL_NOPARK` keeps the window on screen.
- `FDL_WINDOW_CAPTURE` uses the window server to capture system-drawn window controls.
- `FDL_NOBLINK` disables caret blinking (set by `compare.py`).
- `FDL_RECORD_FPS` sets the recording frame rate (default 30).
- `QUIET_MD_FONT_URL`, `QUIET_MD_FONT_DIR` and `QUIET_MD_TRIAL_FONTS` are passed through: downloadable fonts (the board's Xiaolai, document fonts) are fetched from that folder into that cache, a compiled-in Xiaolai is ignored, and the trial fonts are listed (docs/FONTS.md). `python3 tools/harness/board_font.py` checks the board's font and its download note, `python3 tools/harness/doc_fonts.py` the document font list (`--compare` shoots an article in every font).
- `QUIET_MD_UPDATE_URL` is passed through: the updater reads that folder instead of download.quiet.md. To test forced updates see docs/UPDATES.md. Downloads land in `~/Library/Caches/com.s1dashu.quietmd` of the `HOME` the app sees.

## Rules for running scripts

The harness runs on a computer someone may be using.

- Leave the real ⌘C/⌘V clipboard alone. Use `key cmd-v` only after `clip`.
- Avoid stealing focus. Never use `activate`. By default the harness moves the window offscreen to render it.
- Do not click Show in Finder in Settings › Library, or Open in Finder in the home or folder More menu.
- Do not click links, Web Search or link-opening buttons that launch a browser. Avoid Print (system print panel), Quick Look and `qlmanage -p`.
- Do not use the `open` command. Use `pick a|b` when a script needs to select files.
- Never use the real user library as `lib_dir`. If its contents are essential, make a read-only copy and keep user content out of committed artifacts.

## Fei Notes SQLite regression

`python3 tools/harness/sqlite_history.py` checks first-launch creation, synthetic IME marked text (not persisted before commit), read-only history preview, returning to the live document, and restoring a snapshot into SQLite. Build with `cargo build --features cloudkit-sync,devtools`; `FDL_CLOUD_BIN` selects another binary and `FDL_CLOUD_DYLD_PATH` selects the matching Swift bridge directory. The test creates a fresh `/private/tmp/quiet-sync-qa-history-*` local-only library, uses the existing offscreen driver and never selects an iCloud account or uses the system clipboard.

`python3 tools/harness/sqlite_board.py` uses the same binary overrides and isolated local-only startup to draw a shape, edit board text, paste a synthetic image through the fake clipboard, and verify the persisted scene and image ownership in SQLite. It reopens the note read-only and checks that attempted board edits leave its payload unchanged. Artifacts stay in `/private/tmp/quiet-sync-qa-board-*`.

The table-paste and scroll-intent tests locate visible documents in both the old flat library and FORMAT 4, excluding hidden migration backups.

`python3 tools/harness/sharing_ui.py` checks the contextual shared-note list,
creation and member-management flow with a fresh synthetic library and an
explicit devtools-only offline invitation transport. It injects initial-upload
and invitation failures, checks retry identity and original-note preservation,
then exercises permission changes, confirmed removal and read-only invitation
acceptance. It also verifies collapsible shared navigation, same-window opening,
participant leave, owner deletion and retry after a recoverable deletion failure.
Shared Documents reuses the existing Home document-list renderer. The shared
section starts collapsed above My Documents; captures also check the linked
mail-check invitation controls.
Captures include narrow/dark member management. No live CloudKit,
real notes, system clipboard or foreground activation is used. The existing
`FDL_CLOUD_BIN` and `FDL_CLOUD_DYLD_PATH` overrides apply.

`python3 tools/harness/startup.py` verifies the Mac startup boundary with newly
generated offline fixtures: verified cache, delayed background restore, local
editing retained after a late reply, and explicit local-only preference. It
also exercises the existing New Document shortcut before discovery completes.
An unavailable-account fixture must reject an unverified cache; a fresh install
with a delayed account check must retain a note entered before it completes.
Build with `cloudkit-sync,devtools` and set `FDL_CLOUD_BIN` and
`FDL_CLOUD_DYLD_PATH`; no real account or existing library is used. Account and
device-marker mismatch checks also run in the cloud configuration unit tests.

`python3 tools/harness/markdown_external.py` checks clean external Markdown
refresh and defers replacement/autosave during IME composition. It uses a fresh
file library, synthetic marked text and offscreen captures; `FDL_BIN` selects
the devtools build. SQLite keeps its separate version and transaction checks.
The last-read startup check reads FORMAT 4's temporary local reading state,
separately from the portable document index.
