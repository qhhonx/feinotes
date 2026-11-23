# Architecture

Quiet.md is a single-process GPUI application with roughly 90,000 lines of Rust, including roughly 26,000 for whiteboards. A document is a tree of blocks in memory (`model.rs`) and a DocxXML file on disk (`docxml.rs`). The editor renders the tree as a virtualized list.

The Cargo workspace contains two crates:

- **`crates/core` (`fdl_core`, about 36,000 lines, including 20,000 for whiteboards)** contains the document model, DocxXML, editing and undo, the on-disk library, Markdown and HTML import, Markdown export, and whiteboard models and algorithms. It builds and tests independently of GPUI with `cargo test -p fdl_core`. The fuzzer lives here too.
- **The root crate (`quiet_md`)** owns windows, views, rendering, menus and overlays. `main.rs` re-exports the core modules at the crate root, and `editor/mod.rs` re-exports `editor::core` and `editor::table_ops`. The application can therefore use paths such as `crate::model` and `super::core`.

Dependencies flow from the application to core. Within the workspace, core may reference only core modules.

```text
main.rs — window
  ├─ onboarding.rs — first launch, replaced by Workspace in the same window after library selection
  └─ app.rs — Workspace (window root view)
       ├─ store.rs — library directories and .quiet/index.json (see FORMAT.md) [core]
       ├─ sidebar / home / search / trash / tree / docmenu / docinfo / modes
       └─ editor::Editor (view.rs) — open document
            └─ editor::Core (core.rs) — document, selection and undo stack [core]
```

## Layers

| Layer | Files | Responsibility |
|---|---|---|
| Model | `model.rs` | Plain data: `Document`, `Block`, `BlockKind`, `RichText` (text and style runs), and `Inline { kind, xml }`. `BlockKind::nests()` controls nesting. |
| Format | `docxml.rs` | Converts between DocxXML and `Document`. Keeps unknown elements as `Raw`, retains inline XML, parses its type once on read and produces stable output. See FORMAT.md. |
| Editing engine | `editor/core.rs`, `editor/table_ops.rs` | Typing, deletion, merging, indentation, conversion, dragging, tables, folding, inline objects and undo. Lives in `fdl_core` for unit testing and fuzzing without GPUI. |
| Editor view | `editor/view.rs` and other `editor/*` modules | Translates keyboard and pointer events into Core calls and handles overlays, menus and block interactions. |
| Rendering | `editor/render.rs`, `editor/text.rs` | Reads blocks and creates GPUI elements. `text.rs` draws rich text. |
| Application | `app.rs` and other top-level modules | Sidebar, home, search, trash, document tree, import, export and printing. Uses `Store` for disk I/O. |

## Editing flow

1. On an edit event the view calls `core.checkpoint(kind)`. This takes an undo snapshot, increments `generation` and sets `dirty`. Consecutive typing in the same block coalesces when keystrokes are less than 700 ms apart.
2. A Core method updates `core.doc` and/or `core.sel`.
3. Choose the notification according to where the viewport should go:
   - Typing and navigation follow the caret. `Editor::changed()` calls `core.reveal_caret()` to unfold headings or list items hiding the selection and requests a scroll to the caret.
   - Actions on independent targets, such as toggling a checkbox or resizing a column, use `changed_preserving_viewport()` to preserve the caret, selection and viewport. If folding moves the caret to a heading, use `changed()` to reveal it.
   - Both paths emit `EditorEvent::Changed` for actual changes and refresh find results, the slash menu and the mention panel.
4. `Workspace` receives `Changed` and calls `schedule_save`. After 500 ms, `flush()` writes to disk. On failure the document stays unsaved, the top bar shows the error and the app prompts before switching documents, creating one or quitting. The dialog offers retry, reload from disk or save a recovery copy. Reload resets editor state across windows showing the document. Quit checks all windows again after each resolved conflict and waits while conflicts remain. If a save still fails during system shutdown, the app writes the body to local app data in `quiet-md/recovery/<library-id>/`. Reopening that library creates a new recovery document and preserves the original on disk.

After a successful write, `Store::save_now` snapshots the previous in-memory body in `history/<id>/`. Automatic snapshots are throttled to one every five minutes, with 1–30 snapshots per document and a 200 MiB library budget. A snapshot failure logs a warning without failing the save. Restoring a version first forces a snapshot of the current body, then uses the regular save path.

The version history drawer follows the Google Docs/Feishu pattern. Selecting a snapshot previews it read-only in place, with optional change highlighting. The workspace's `preview: Option<Preview>` holds the original `Core` (document, caret, selection and undo stack) and reinstates it on return. During preview, `flush`, `schedule_save` and `sync_document` short-circuit to avoid writing historical content or sending it to sibling windows as current content.

`changes.rs` computes a block-level longest common subsequence (LCS) to highlight new and rewritten blocks. `diff.rs` computes a line-level LCS for the drawer's added/deleted line counts. See [Storage format](FORMAT.md).

After each random edit, `crates/core/src/editor/fuzz_tests.rs` checks these invariants:

- Both selection endpoints are visible caret stops in the same scope (one table cell or the body).
- Block IDs are unique. Runs stay in bounds and do not overlap. Each inline object run contains only U+FFFC.
- Column proportions sum to one and merged table cells do not overlap.
- Only `nests()` blocks and containers (quotes, callouts, column groups and columns) have children.
- Code blocks have no styles or inline objects.
- Saving and reloading preserves the document.
- Undoing and then redoing every step returns to the same state.

### Direct property changes

Core owns all structural edits, including adding, removing and moving blocks and merging or splitting text. A few view modules set individual block properties directly with `core.block_mut(id)`, bracketed by `checkpoint` and `changed`:

- Image size, crop and description (`image.rs`)
- Countdown settings (`countdown.rs`)
- Code block name (`caption.rs`)
- Icon and cover (`cover.rs`)
- Title changes (`view.rs`)
- Some checkbox and fold toggles (`view.rs`, `render.rs`)

These preserve tree structure. Implement new structural operations in Core and add a corresponding random operation to the fuzzer's `step`.

## Undo

Each `Snapshot` holds a complete `Document`. A `VecDeque` retains up to 500 steps with an approximate 256 MB budget and a minimum of 20 steps even for long documents. During undo and redo, `keep_outside_history` preserves the current document icon and fold state. Every change, including folding, increments `generation` so rendering caches, the table of contents and word counts can detect stale results.

This approach copies the entire document at each history step. A historical benchmark measured 5.06 ms median synchronous frame processing while typing in a million-character document. See the [method and build](PERFORMANCE.md) for measurement details, including the exclusion of display presentation latency.

## Rendering and virtualization

- A GPUI `List` contains the title area (`Item::Header`), visible top-level blocks and the references section (`Item::Below`).
- Children of text, heading and list blocks get separate rows (`flattens`). Containers draw children within the container row.
- `sync_items` uses `hash_block` as the row key. It compares keys with the previous frame and uses `splice` on the interval between the unchanged prefix and suffix. Only changed rows need new height measurements.
- `hash_block` hashes text length. The list remeasures visible rows each frame. A same-length change such as applying ⌘B to a paragraph may leave an offscreen cached height temporarily stale. Hashing the text itself added about 3 ms per keystroke for a million-character document. See the measurements in the comments in `render.rs`.
- The scrollbar uses the vendored GPUI method `ListState::estimated_scroll_metrics` to estimate unmeasured rows from average row height. See vendor/PATCHES.md.

## Memory and caches

- A shared cache protects document images visible in any window. Above 64 MiB of offscreen decoded images it evicts the least recently used first. Other offscreen entries expire after 30 seconds. A sweep every two seconds also cleans the GPU atlas. Up to two decodes run concurrently and stale queued requests are skipped. Original images retain full resolution and animated images keep their decoding and playback behavior.
- Images with explicit dimensions start loading when actually painted in the visible region. Images clipped out of home cards avoid early decoding. Home caches up to 128 recently used document previews.
- The parse cache, window lists and file history share full-text search strings via `Arc<str>`. After a successful save when switching documents, the app releases the old document's full XML conflict-detection baseline if no other window uses it. Reopening reestablishes the baseline.
- Metal and DirectX allocate intermediate path textures on demand and reclaim them on subsequent frames without paths. Apple GPUs use on-chip memory for multisampling at the same antialiasing sample count.

## Module map

### Document engine (`crates/core/src/`)

| Module | Contents |
|---|---|
| `model.rs` | Document model, inline object types and `display_title` |
| `docxml.rs` | DocxXML parsing and serialization, including `to_inline` for inline objects |
| `store.rs` | Library I/O, index recovery, trash, settings and `image_path` |
| `paste.rs` | HTML and Markdown to blocks, shared by paste and import |
| `markdown.rs` | Markdown export |
| `lang.rs` | Code language table |
| `spacing.rs` | Where a space goes between 汉字 and Latin letters or digits (盘古之白), for typing with the preference on and Edit › 整理本文空格 |
| `palette.rs` | RGBA text, background and callout palettes independent of GPUI. `theme.rs` converts them to GPUI colors. |
| `time.rs` | Date conversion, local timezone and current time |
| `editor/core.rs` | Editing engine |
| `editor/table_ops.rs` | Table operations and cell selection (`CellSel`) |
| `editor/fuzz_tests.rs` | Random edit fuzzer |

Whiteboard and history modules in core:

| Module | Contents |
|---|---|
| `whiteboard.rs` | Board blocks (`BlockKind::Board`), standalone `.excalidraw` I/O, SVG and searchable text |
| `board/` model | Excalidraw ports: `types`, `new_element`, `mutate`, `scene`, `fractional_index`, `app_state`, `consts`, `random`, `color`, `utils`. I/O: `restore`, `json`, `encode`, `clipboard`, `transform`. See [Whiteboard design](design/whiteboard-excalidraw.md). |
| Geometry | `math`, `bounds`, `collision`, `deconstruct`, `shape`, `hachure`, `freehand`, `handles`, `size_helpers`, `viewport` |
| Editing | `drag`, `resize`, `selection`, `groups`, `zindex`, `duplicate`, `align`, `snapping`, `lasso`, `eraser`, `frame`, `crop`, `image`, `sticky`, `flowchart` |
| Lines and text | `linear`, `binding`, `elbow`, `heading`, `arrow_text`, `text`, `text_wrapping`, `text_measure`, `text_edit`, `list_marker` |
| Undo and actions | `history` (store, delta, history), `actions.rs` and `actions/` (one function per upstream action) |
| Output | `display` (shared backend display list), `export_svg` |
| `versions.rs` | Directory-indexed document snapshots, throttling, quotas and trash integration |
| `diff.rs` / `changes.rs` | Dependency-free line LCS and block-level LCS for snapshot highlighting |
| `merge.rs` | Three-way merge of two versions of a document by block, for sync (docs/design/sync.md §3.2); not wired into the app yet |

### Application (`src/`)

| Module | Contents |
|---|---|
| `main.rs` | Entry point, `FDL_SCRIPT` mode (devtools builds only), `--self-test` and core module re-exports |
| `guides.rs` | Seeds an empty library with the bundled Getting Started guides once |
| `app.rs` | `Workspace`, the two-pane window, open/create/save/delete actions, save state and reminder scheduling |
| `html.rs` / `printing.rs` / `transfer.rs` | HTML/PDF export, printing and import/export UI |
| `clipboard.rs` | System clipboard HTML and the simulated clipboard in script mode |
| `sidebar.rs` / `tree.rs` | Sidebar and document tree, including child pages (a folder's own document, see FORMAT.md and core's `subdocs.rs`) and Move to |
| `home.rs` / `trash.rs` | Home and trash |
| `search.rs` | Search dialog and advanced search |
| `docmenu.rs` / `docinfo.rs` / `modes.rs` | More menu, document information, edit/read mode switch and shortcut panel |
| `theme.rs` | `craft` scale, semantic `Theme::*` colors, named display colors, `shadow_*`, type scale and note panel dimensions |
| `ui.rs` / `input.rs` | Shared menu shells and rows, white pill buttons, hints, toasts and single-line input |
| `highlight.rs` | Code highlighting |
| `mermaid.rs` | Mermaid code blocks drawn as diagrams: bundled mermaid.js in an off-screen web view (macOS, Windows), SVG cached by source and theme |
| `assets.rs` | Embedded icons, fonts and cover gallery |
| `fonts.rs` / `font_classes.rs` / `font_downloads.rs` | Installed families for the font settings and what each is (黑体, 宋体 …, scripts covered: read from the font files); fonts fetched on demand (the board's Xiaolai, downloadable document fonts). See [Downloadable fonts](FONTS.md). |
| `devtools.rs` | Script driver compiled with `--features devtools`. See tools/harness/README.md. |
| `selftest.rs` | Release self-test: fixed end-to-end scenarios that shipped builds run with `--self-test`. See tools/harness/README.md. |
| `history.rs` | Version drawer grouped by day, in-place read-only preview, change highlighting, naming, restore, save a copy and delete |
| `board/` view | `BoardView` replaces the document in the note area. `mod.rs` owns the scene, store and commit loop, `canvas.rs` dispatches pointers, `keys.rs` handles shortcuts, `run.rs` / `action.rs` implement actions and clipboard, `text_editor.rs` edits text and `frame_name.rs` renames frames. `fonts.rs` registers Excalidraw's fonts; `font_hint.rs` shows the CJK one downloading (`font_downloads.rs`, docs/FONTS.md). `editor/board.rs` handles thumbnails and entering/leaving the board. |
| `board/tools/` | Tool state machines: `selection` (drag, handles and marquee), `shape`, `linear`, `freedraw`, `text`, `eraser`, `lasso`, `image`, `crop`, `flowchart` |
| `board/` rendering | GPUI backend in `render.rs` and `render_geom.rs` (CPU clipping and image resampling), vello_cpu in `render_vello.rs`, overlays for selection bounds, handles and snap lines in `overlay.rs` |
| `board/chrome/` | Toolbar, properties, palettes, context menu, zoom controls and shortcut help |

### Editor (`src/editor/`)

| Module | Contents |
|---|---|
| `view.rs` | `Editor` state, keyboard and pointer handlers, and `changed()` |
| `keymap.rs` / `menus.rs` | Key bindings and data for slash and conversion menus |
| `render.rs` / `text.rs` | Page rendering and rich text elements |
| `overlays.rs` | Handles, slash and + menus, block menu and floating toolbar |
| `drag.rs` / `gap.rs` | Block dragging and clicks between blocks |
| `table.rs` / `grid.rs` | Table UI (operations live in core's `table_ops.rs`) and columns |
| `image.rs` / `file.rs` / `cover.rs` | Images, video and files, icons and covers |
| `math.rs` | Formula layout from katex-rs HTML |
| `link.rs` / `links.rs` | Link panels and cards, document mentions and references |
| `button.rs` / `countdown.rs` / `reminder.rs` / `subpages.rs` | Buttons, countdowns, date reminders and subpage lists |
| `emoji.rs` / `emoji_picker.rs` | Emoji data and picker |
| `catalogue.rs` / `find.rs` / `reading.rs` / `scrollbar.rs` / `caption.rs` | Contents, find/replace, reading mode and info line, scrollbar, code block names and image captions |

## Interface languages

`settings::Lang` supports Simplified Chinese, Traditional Chinese, English, Japanese, Korean, Spanish, French, Arabic and Turkish. `Preferences::language` stores the choice. Older files fall back to `Preferences::english`, which the app still writes so older versions show English for other languages. Use `settings::text` / `label` for mapped static strings, `settings::tr` for paired strings and `localized_format!` for parameterized strings. Menu tables keep stable source labels and translate on display. Search matches both English labels and original keywords. Inputs retain their original placeholders and translate when rendered. Switching languages refreshes all windows. Arabic strings are translated, with the UI layout still left-to-right.

Chinese source strings serve as translation keys. The lookup tables live in `src/l10n/zh_tw.rs`, `ja.rs`, `ko.rs`, `es.rs`, `fr.rs`, `ar.rs` and `tr.rs`. The helpers above use the table first and fall back to English, so new strings can ship before all translations are ready. `localized_format!` uses `l10n::refill` to recover arguments from formatted Chinese and fill the translated template without changing callers. Use `settings::localized` when specifying the language without the macro, as in date reminder tests. `settings::english()` means any language other than Chinese. Run `python3 tools/l10n/keys.py --missing ja` (or `zh_tw`, `ko`, `es`, `fr`, `ar`, `tr`) to list missing keys. The `l10n` tests verify that keys still exist in source and placeholders come from the original template.

User titles, body text, attachment names and saved button labels are user content. Only UI defaults such as the empty-title label change language. The application translates core storage diagnostics from known templates, preserving paths, document IDs and OS error text. The document engine stays independent of interface language. Regression checks cover translation keys, menu tables, reminders, user content and diagnostic arguments.

## Undo for folders and file operations

Folders are `Meta::folder` index entries. Only folders can be parents. Core's `store/folders.rs` handles legacy tree migration, hierarchy checks and sibling order. The app's `folders.rs`, `sidebar_drag.rs` and `home_folder.rs` implement folder pages, dragging and animation.

Create, duplicate, rename, move, delete and restore operations have their own undo history. Under the write lock, the store records successful file operations so changes from other instances stay outside the current operation. After an asynchronous drop reaches disk, the actual record replaces the preview record. Undo checks file state and hierarchy and reads current content from disk, leaving older body snapshots out of file-operation undo. When a management action changes a title, it adjusts caret boundaries in the editor and its history snapshots.
