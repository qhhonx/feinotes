# Editor performance benchmarks

The README reports results from this procedure. Use the same scripts to reproduce it on your own machine.

## Environment

| Item | Value |
|---|---|
| Date | 2026-11-03 |
| Source | `0e92b51`, built with `cargo build --release --locked` |
| Machine | Apple M5 Pro, 48 GiB, macOS 26.5 |
| Window | 1280 × 800 pt at 2× scale, Craft theme |
| Document | 1,001,307 characters in 549 chapters with paragraphs, lists, tables and code, created by the [sample generator](../tools/perf/gen_library.py) |

## Method

The [harness](../tools/harness/README.md) replays [readme-performance.txt](../tools/harness/examples/readme-performance.txt). It opens the document, types 100 individual `x` characters at the end of the first paragraph, presses Backspace 100 times, then scrolls down 100 times.

After each operation, `bench` synchronously calls `window.draw` and records the frame processing time as `frame`. P50 is the median and P95 the 95th percentile. These figures measure in-app processing on one machine. End-to-end keyboard-to-screen latency also includes presentation time. No other apps were benchmarked.

## Results

| Operation | Frame P50 | Frame P95 | Maximum |
|---|---:|---:|---:|
| Typing | 5.06 ms | 5.78 ms | 6.94 ms |
| Backspacing | 4.99 ms | 5.55 ms | 5.69 ms |
| Scrolling | 2.08 ms | 4.14 ms | 9.23 ms |

Raw output in milliseconds:

```text
bench "type x" x100: step [p50 0.03 p95 0.05 max 5.29] frame [p50 5.06 p95 5.78 max 6.94] editor render [p50 3.77 p95 4.31 max 4.99] ms
bench "key backspace" x100: step [p50 0.02 p95 0.05 max 3.91] frame [p50 4.99 p95 5.55 max 5.69] editor render [p50 3.64 p95 4.12 max 4.24] ms
bench "wheel 900 500 40" x100: step [p50 0.01 p95 0.02 max 0.02] frame [p50 2.08 p95 4.14 max 9.23] editor render [p50 0.02 p95 0.03 max 1.83] ms
```

## Reproduce the benchmark

The test uses a temporary library and leaves `~/Documents/Quiet.md` untouched.

```sh
cargo build --release --locked --features devtools
python3 tools/perf/gen_library.py /tmp/quiet-perf/library
printf '{"english":true,"theme":"craft"}\n' > /tmp/quiet-perf/library/preferences.json
python3 tools/harness/run.py tools/harness/examples/readme-performance.txt /tmp/quiet-perf/library /tmp/quiet-perf/run
grep -a bench /tmp/quiet-perf/run/log
```

## Dragging the scrollbar

[scrollbar-drag.txt](../tools/harness/examples/scrollbar-drag.txt) opens the 1,000,000-character compilation, presses the page thumb and drags it 4 pt a step: 90 steps to halfway, 90 to the end, then 180 back to the top, with `bench` drawing a frame after each step. Every step lands on rows that may not have been laid out yet, so this is the editor's most expensive scrolling. `mean` is the average frame and `drops` the frames over 16.7 ms (one 60 Hz frame); screenshots at halfway, the end and the top check that the thumb is under the pointer and the page where the thumb says.

Two documents: the compilation as generated, and with `--tall-start` (three images, a 60-row table and two 120-line code blocks first). The thumb's length estimate counts rows not yet laid out at the average height of those on screen, so a tall first screen overestimates the document many times over.

Measured 2026-11-25 on an Apple M6 Mac mini (16 GiB, macOS 27.2.1), 1280 × 800 pt window, Craft theme, `--features devtools` release builds. Frame mean / drops (of 90) per quarter of the drag:

| Build | Document | Down 1 | Down 2 | Up 1 | Up 2 | Thumb under the pointer |
|---|---|---:|---:|---:|---:|---|
| v0.1.14 | as generated | 12.96 / 8 | 13.72 / 12 | 11.50 / 3 | 14.96 / 28 | yes |
| v0.1.14 | tall start | 5.85 / 6 | 3.03 / 0 | 3.00 / 0 | 6.34 / 9 | no: the page reaches its end at 1/10 of the track, the thumb stops there |
| 6df2232 | as generated | 14.21 / 21 | 14.90 / 19 | 12.03 / 6 | 11.16 / 1 | yes |
| 6df2232 | tall start | 3.25 / 5 | 3.35 / 0 | 1.84 / 0 | 3.47 / 4 | no (as v0.1.14) |
| 67d6145 | as generated | 16.19 / 33 | 23.19 / 79 | 11.02 / 4 | 10.83 / 7 | yes |
| 67d6145 | tall start | 16.70 / 24 | 26.23 / 83 | 23.57 / 75 | 15.83 / 32 | yes |
| this change | as generated | 12.77 / 7 | 15.20 / 23 | 11.88 / 6 | 9.81 / 0 | yes |
| this change | tall start | 13.10 / 8 | 14.93 / 19 | 11.18 / 4 | 10.26 / 1 | yes |

- With a tall start, v0.1.14 and 6df2232 are fast only because the page stops moving: holding the length estimated at the press, it reaches its end a tenth of the way down, and the rest of the drag redraws the same rows.
- 67d6145 made the page follow the thumb to the end, and aimed the page again on every frame in which newly measured rows had changed the estimate. During a drag that is nearly every frame, so each one laid out a second set of rows nobody saw: about 10 ms more a frame going down, and most frames dropped.
- Now a move aims the page by the latest estimate, and it is aimed again only once the pointer rests (80 ms) or the thumb is let go, the thumb held in place until the page is there. Frames are back to v0.1.14's; the thumb stays under the pointer, the end of the track is the end of the document, and letting go mid-drag leaves the thumb where it was (same pixels before and after).
- The rest of the cost, about 12 ms a frame going down, is laying out a screen of rows not seen before (text shaping); it has not changed since v0.1.14. The editor's own `render` stays under 0.2 ms.

Reproduce (the same temporary libraries; `run.py` copies them per run):

```sh
cargo build --release --locked --features devtools
python3 tools/perf/gen_library.py /tmp/quiet-perf/library
python3 tools/perf/gen_library.py --tall-start /tmp/quiet-perf/tall
for lib in library tall; do
  printf '{"english":true,"theme":"craft"}\n' > /tmp/quiet-perf/$lib/preferences.json
  python3 tools/harness/run.py tools/harness/examples/scrollbar-drag.txt /tmp/quiet-perf/$lib /tmp/quiet-perf/drag-$lib
  grep -a bench /tmp/quiet-perf/drag-$lib/log
done
```

### Toward 120 Hz

The same replay, aiming at 8.3 ms (one 120 Hz frame). A sampling profile of the drag (`sample`, main thread) put about 60% of the frame in `CTLineCreateWithAttributedString`, and most of that not in shaping but in CoreText setting up fonts: `TFont::InitShapingGlyphs`, `InitASCIIDataCache` and variable-font advances (`HVAR`), work CoreText keeps on each CTFont object. GPUI made a new sized CTFont for every run of every line, and CoreText made new fallback faces for the Chinese text of every line. Three changes:

1. Each font is made once per size (vendor patch 0080 reuse-coretext-fonts). The ASCII and HVAR setup left the profile.
2. The face CoreText fell back to for a CJK character is kept and set on that character in later lines (same patch), so CoreText no longer falls back, and makes new faces, line after line. Shaping went from about 415 to 105 samples in the same 10 s of dragging. It is the face CoreText chose itself: one picked with `CTFontCreateForString` instead lost a bold heading's weight.
3. While the thumb is held the page lays out no rows beyond the window (`ListState::set_overdraw`, vendor patch 0081; 600 pt above and below otherwise), and goes back to 600 pt once the thumb is let go and the page has settled.

Measured 2026-11-25 on the same Mac mini, but with another build of the app busy in the background (30-40% CPU) the whole time: main (c6498c2) ran 1.3-1.5× slower than in the table above, and single runs varied by up to 2×. Each build was therefore run three times, alternating with the other. Per quarter of the drag, over the three runs: frame mean, mean P95, and frames over 8.3 ms and over 16.7 ms (of 270):

| Build | Document | Down 1 | Down 2 | Up 1 | Up 2 | Whole drag |
|---|---|---|---|---|---|---|
| main | as generated | 16.6 / 35.2 / 267 / 72 | 20.2 / 37.4 / 266 / 147 | 18.6 / 33.2 / 244 / 138 | 16.5 / 30.6 / 244 / 108 | mean 18.0 ms; 95% over 8.3, 43% over 16.7 |
| this change | as generated | 6.1 / 14.5 / 55 / 10 | 4.7 / 9.0 / 44 / 0 | 6.2 / 10.9 / 94 / 0 | 4.6 / 8.8 / 34 / 0 | mean 5.4 ms; 21% over 8.3, 0.9% over 16.7 |
| main | tall start | 14.3 / 27.8 / 247 / 62 | 20.4 / 32.1 / 270 / 184 | 18.9 / 28.1 / 260 / 162 | 18.2 / 28.3 / 244 / 159 | mean 17.9 ms; 95% over 8.3, 53% over 16.7 |
| this change | tall start | 7.1 / 14.3 / 80 / 9 | 5.9 / 10.7 / 82 / 0 | 7.1 / 10.8 / 118 / 0 | 4.6 / 9.0 / 56 / 0 | mean 6.2 ms; 31% over 8.3, 0.8% over 16.7 |

- Frames are about three times faster, and 60 Hz drops are nearly gone. The 120 Hz target is met on average but not yet at P95 (9-11 ms past the first quarter, on this busy machine). What is left is shaping characters seen for the first time (their fallback face too), taffy layout of the new rows and painting.
- The thumb stays under the pointer, the end of the track is the end of the document, and letting go mid-drag leaves the thumb where it was, on both documents.
- Rendering is unchanged: screenshots of the compilation (top and three wheel positions), the guides (Writing and formatting, Welcome, Playground), and a Chinese page with quotes in the default face, Songti SC, PingFang SC, Hiragino Sans GB and Heiti SC are identical to main's, pixel for pixel (`tools/harness/compare.py`).
- Wheel scrolling did not slow down: five alternating runs of 300 steps of 40 pt each way, mean 3.48 / 2.90 ms (down / up) against main's 3.57 / 2.92, with fewer frames over 8.3 ms (65 against 160). Opening the compilation, first frame 30.8-34.0 ms against 33.2-35.8. Typing and backspacing ([readme-performance.txt](../tools/harness/examples/readme-performance.txt)) are unchanged within the noise.

## Dragging blocks

[block-drag.txt](../tools/harness/examples/block-drag.txt) opens the 1,000,000-character compilation, presses the handle of its first paragraph and drags it 4 pt a step: 120 steps down (480 pt, past 14 blocks), 120 back up, then 120 frames held at the window's bottom edge, where the page scrolls under it. `bench` draws a frame after each step. Dragged blocks now open a grey slot their size where they would land and the blocks between slide aside on springs; the slides are drawn moved (`Shift`, an offset at prepaint), not laid out again, so no row is measured again while it slides.

Measured 2026-11-25 on an Apple M6 Mac mini (16 GiB, macOS 27.2.1), 1280 × 800 pt window, Craft theme, `--features devtools` release builds, three runs each, alternating. Means of the three: frame mean, frames over 16.7 ms and over 8.3 ms (of 120), and the p50 of the step (the drag event: finding the drop), of the frame and of the editor's own `render`:

| Build | Phase | Frame mean | Over 16.7 / 8.3 | Step p50 | Frame p50 | Render p50 |
|---|---|---:|---:|---:|---:|---:|
| main (cc0a4c0) | down | 14.9 ms | 59 / 101 | 5.6 ms | 16.5 ms | 14.2 ms |
| main | up | 18.6 ms | 100 / 118 | 6.9 ms | 19.4 ms | 16.4 ms |
| main | held at the edge | 20.0 ms | 100 / 118 | 6.9 ms | 20.8 ms | 16.3 ms |
| this change | down | 4.1 ms | 0 / 0 | 0.04 ms | 4.1 ms | 0.07 ms |
| this change | up | 4.2 ms | 0 / 0 | 0.05 ms | 4.3 ms | 0.08 ms |
| this change | held at the edge | 6.9 ms | 1 / 27 | 0.04 ms | 6.4 ms | 0.09 ms |

- Before, a drag cost about 16 ms of `render` a frame whatever moved, from three walks of the whole document: the selection's word count and the set of selected blocks each listed the selected blocks again every frame (`block_sel_ids` walks every block twice and indexes their paths), and the word count built an index of every block. A dragged block is selected, so every frame paid it. Both are now kept until the selection or the document changes. The same cost was paid by any frame with blocks selected, not only while dragging.
- Finding the drop under the pointer (each drag event and each frame) walked every top-level block (tens of thousands) and indexed every block: 5-7 ms. It now walks only the top-level blocks the page drew rows of.
- Held at the edge, the page scrolls 600 pt/s and lays out rows it has not shown yet; that is what is left (as when scrolling).
- Pages not being dragged draw as before: screenshots of the test document (still, hovered, block menu, selected, scrolled), the compilation (top and two wheel positions) and [menus.txt](../tools/harness/examples/menus.txt) are identical to main's, pixel for pixel (`tools/harness/compare.py`).

Reproduce (the same temporary library as above):

```sh
cargo build --release --locked --features devtools
python3 tools/harness/run.py tools/harness/examples/block-drag.txt /tmp/quiet-perf/library /tmp/quiet-perf/block-drag
grep -a bench /tmp/quiet-perf/block-drag/log
```

To watch the slides themselves in a scripted run, set `FDL_ANIMATE=1` (tools/harness/README.md): a scripted window's animation frames are otherwise throttled.

## Large libraries

A user who imported an Obsidian vault of more than 15,000 Markdown notes found browsing almost unusable. This section measures that case before and after the fix on `perf/big-library`.

### Environment

| Item | Value |
|---|---|
| Date | 2026-11-25 |
| Source | `ce2dd1d` (before) and `perf/big-library` on top of it (after), `cargo build --release --locked --features devtools` |
| Machine | Mac mini, Apple M6 (2 performance and 4 efficiency cores), 16 GiB, macOS 27.2 |
| Window | 1280 × 800 pt at 2× scale |
| Library | `gen_library.py --vault 15000`: 15,000 Markdown notes in 174 folders up to three deep, one flat `Inbox` of 3,016 notes, 200 images, wiki links, relative Markdown links, tags and front matter, plus `.obsidian/` |

### Method

[big-library.txt](../tools/harness/examples/big-library.txt) opens the vault and times idle frames with `bench 60 noop` in seven views, each 4 seconds after getting there so that card previews have loaded. `Inbox` changed last, so the window opens on its page with `Inbox` open in the sidebar:

| View | What is on screen |
|---|---|
| Inbox open | The `Inbox` page of 3,016 cards, `Inbox` open in the sidebar (about 3,500 rows) |
| Inbox closed | The same page, `Inbox` closed in the sidebar (about 460 rows) |
| Opened again | As the first |
| Sidebar scrolled | The same, the sidebar scrolled |
| Home | 主页 with the `Inbox` folder card |
| Home scrolled | The same, 主页 scrolled |
| Document | A note from `Inbox` |

Each build ran the script five times, the two builds alternating (`ce2dd1d` has no `row` step yet, so both were driven by the same clicks given as coordinates). On this machine the main thread runs either on a performance core or, when those are busy, on an efficiency core, where every frame takes two to three times as long; a run can switch between the two at any point, and a library of 300 notes shows the same two levels. The frame table gives the best P50 of the five runs, which is the frame on a performance core, and the median of the five P50s.

Because frame times move with the machine's load, four views were also profiled with `sample` for 8 seconds of `bench 1500 noop`: the main thread's share of samples in `Window::draw`, `Workspace::render` and `render_sidebar`, times the mean frame, is the time each of them takes in a frame. These numbers change much less from run to run.

### Results

Frame P50 in milliseconds, best of five runs (median of five):

| View | Before | After | 300 notes, before |
|---|---:|---:|---:|
| Inbox open | 15.8 (16.0) | 9.5 (10.5) | 6.4 |
| Inbox closed | 21.1 (28.7) | 10.1 (20.6) | 6.8 |
| Opened again | 23.6 (29.7) | 7.2 (20.8) | 8.6 |
| Sidebar scrolled | 24.6 (29.0) | 7.7 (22.6) | 10.3 |
| Home | 29.5 (30.4) | 9.6 (23.9) | 8.4 |
| Home scrolled | 27.2 (29.2) | 10.2 (22.1) | 13.5 |
| Document | 24.8 (26.8) | 2.7 (11.3) | 7.3 |

The last column is the same script on `gen_library.py --vault 300` (best of three). After the fix a frame in the 15,000-note vault costs about what it costs in a library of 300 notes.

Main-thread time per frame in milliseconds, before → after (`sample`):

| View | `Window::draw` | `Workspace::render` | of which `render_sidebar` |
|---|---:|---:|---:|
| Inbox open | 7.2 → 0.9 | 6.7 → 0.6 | 6.7 → 0.5 |
| Inbox closed | 8.1 → 2.7 | 6.7 → 1.2 | 6.1 → 0.9 |
| Home | 7.9 → 1.9 | 5.2 → 0.5 | 5.1 → 0.4 |
| Document | 5.0 → 1.4 | 4.8 → 0.9 | 4.7 → 0.8 |

Before the fix, most of each frame went to work that grows with the library and was redone on every frame, even with nothing changing:

- The sidebar copied all 15,000 entries and sorted them to order the pinned ones, rebuilt the whole page tree, hashing every id, twice (once more for a drag that was not happening), and copied the entry of every row in the tree, links and reminders included.
- A folder card on Home walked every entry once for each folder under it and copied every document below it, to paint the newest four.
- Home and a folder's page copied and sorted their whole level, 3,016 entries for `Inbox`, and copied it once more for the grid.

Now only the pinned entries are sorted, and the rest is built once and kept until the entries change, so an unchanged frame does none of it. The tree build itself looks up only the folders instead of every id. Screenshots of every view above, the row menu and `examples/menus.txt` are pixel-identical before and after (`compare.py`).

The rows and cards were first kept by a hash of every field they show (`tree::shape_key`), computed a few times a frame, because the window's entries were assigned in some 70 places and nothing said when they changed. They are now an `Entries` ([entries.rs](../src/entries.rs)): read as a slice, changed only through `set`, `edit` and `take`, each giving them a new `generation` from one counter (a list replaced whole never repeats a number). The sidebar's rows, Home's level and the folder cards' papers are rebuilt when it moves; assigning a `Vec` to the field no longer compiles. Browsing the views above moves it six times in all (the load, the first sort, opening a document). Measured 2026-11-26 against `032e2fb`, three runs alternating, `bench 600 noop` with a 4 s `sample` in each view, medians, ms a frame:

| View | `Window::draw` | `Workspace::render` | of which `render_sidebar` | `shape_key` |
|---|---:|---:|---:|---:|
| Inbox open | 2.81 → 1.56 | 0.93 → 0.30 | 0.67 → 0.28 | 0.49 → 0 |
| Inbox closed | 2.18 → 0.88 | 0.62 → 0.10 | 0.43 → 0.08 | 0.41 → 0 |
| Sidebar scrolled | 2.96 → 1.48 | 1.16 → 0.37 | 0.90 → 0.35 | 0.50 → 0 |
| Home | 3.35 → 0.92 | 1.01 → 0.21 | 0.78 → 0.20 | 1.17 → 0 |
| Home scrolled | 3.64 → 1.78 | 0.97 → 0.46 | 0.75 → 0.41 | 1.28 → 0 |
| Document | 1.21 → 0.85 | 0.68 → 0.41 | 0.66 → 0.38 | 0.22 → 0 |

Home hashed the entries three times a frame (its level, the folder cards, the sidebar). Frame P50 from the same runs jumps with the core the main thread lands on (performance or efficiency, above) and says little; the samples are steadier. The screenshots of the seven views in all three runs, `examples/menus.txt`, and a script that adds a page, opens folders and goes to Home are pixel-identical before and after, except the new page's row, which differs by up to 4/255 between two runs of the same build too.

### Still on the main thread

These grow with the library too. Times are for the 15,000-note vault, with a library of 300 notes in parentheses, taken directly from `Store` ([store_bench.rs](../crates/core/examples/store_bench.rs), median of 15):

| Operation | When | Before | After |
|---|---|---:|---:|
| `Store::list`, the first time | Opening a library for the first time: every note is read | 2.0 s | off the main thread (below) |
| `Store::list` | Every time the window comes forward (`app.rs`, window activation), and 2 s after files change on disk | 70-78 ms (4 ms) | 38-44 ms (3 ms) |
| `Store::save_editor` | Every autosave while typing | 52-57 ms (8 ms) | 12 ms (8 ms) |

Measured 2026-11-25 on the Mac mini above, five runs alternating between `86317e7` and `perf/meta-cache`. Saving waits on the disk (the note and this device's 4.5 MB metadata file, each written with a backup and `fsync`): with other builds busy on the same disk it took 64-143 ms before and 20-79 ms after.

Before, every `list` and every save started with `Store::begin_write` merging every device's metadata again (`load_meta`: a copy of the merged view, the index rebuilt from it, every entry turned into JSON fields and compared), and every save compared the whole index before writing (`save_meta`): about half of `list` and most of a save. In a loop of `list` and save, `begin_write` took 2,083 of 5,093 samples before and 91 of about 5,000 after.

Now memory is known to be what the disk says while nothing has changed since this store last read or wrote it, and then nothing is read again:

- No device file in `.quiet/meta/` was added, removed or rewritten: each is compared by name, size, modification time and, since a sync (iCloud, a git checkout, `rsync -t`) can put a file back with the time it had where it was written, its inode and change time (ctime), which every write or replacing rename moves and nothing sets back. Windows has no change time, so a hash of each file's contents stands in (a placeholder still in the cloud is not read). Any difference reads every file again, as before.
- No read on the way took in another device's write that memory has not (a setting saved outside `begin_write` merges what it finds).
- This computer's file about the library, the index, the entries kept for others and the 回收站 are as they were when last read or written; a change not written (a failed write) reads everything again.
- `list` left nothing out: an entry whose file is not here (another device's, still on its way) is kept on disk but not in memory, so the next write reads everything again and the file keeps its settings when it arrives. A library with such entries is as fast as before, not faster.

A save compares only the entries that changed since the last write and the folders they left or went to (their order keys), not the whole index. Tests check both against the old way: in the test build every skipped read is compared with reading everything again, and every save with comparing the whole index.

What is left of `list` is finding the files (`layout::scan` and a `stat` for each note, most of its 40 ms); of a save, writing this device's metadata file whole, as FORMAT 6 keeps it.

### Opening a big library the first time

The first `list` reads every note (no parse cache yet), and the first folder opened under 手动排序 numbers every page (`initialize_manual_order`, a write of every entry). Both ran on the main thread before the window's first frame, which stayed empty meanwhile.

Now ([loading.rs](../src/loading.rs)) a `tree_writer` of each top-level folder lists it (and numbers its pages) on a background thread, counting into a `ListProgress`, and hands everything back at once; the main store takes over what it read (`Store::adopt`). The window waits up to 150 ms for it before its first frame, so a small or already-read library opens exactly as before. Past that, sidebar and 主页 stay empty until 300 ms, then grey rows and the onboarding's drifting light show how far it got (正在载入 3,214 / 15,000 篇), and fade out over what was found, from the second frame after the hand-back.

Meanwhile search, 新建 and the imports say the library is still loading and do nothing, rather than queue work that would happen by surprise later; a Markdown file opened from the Finder or dropped on the window opens where it is. Listing meanwhile (after such a write) reads no note, and the hand-back then lists once more, with every note already read.

`initialize_manual_order` also no longer rewrites the index when the numbers are already right. A FORMAT 5/6 library never kept `home_manual_initialized` (its settings live in the device files, which hold only some of them), so every folder opened under 手动排序 rewrote the whole index: 184 ms for 15,000 notes.

Cold first open (no parse cache), measured 2026-11-25 on the Mac mini above, [harness](../tools/harness/README.md) with `FDL_RECORD_START`:

| | 300 notes | 15,000 notes | 60,000 notes |
|---|---:|---:|---:|
| Before the first frame | as before (listed within the 150 ms) | 150 ms | 150 ms |
| Listing in the background | — | 0.6-1.0 s | 2.4-7.8 s |
| Main-thread frame while loading, P50 | — | 1-7 ms | 0.6-1.2 ms |
| Taking the result in (one frame) | — | 55-71 ms | 300-420 ms |

An import (Markdown files, a folder, an Obsidian vault, 思源笔记) is written by a `tree_writer` off the main thread the same way; it now lists the library there too, and the window's store takes over the writer (`Store::adopt`) instead of reading the whole library again to find the new pages. Taking an import of 20 notes in, on the main thread (two runs each, against `b9c78d1`): 35-38 ms → 2-3 ms in a 2,000-note library, 173-206 ms → 15-18 ms in the 15,000-note one. `tools/harness/import_open.py` checks that what was imported is in front.

Before, the 15,000-note window showed nothing for the 2.0 s of the `list` plus 1.1 s for the first folder's numbering. A file opened during loading costs one more `list` at the hand-back (264 ms for 60,000 notes). What the hand-back still takes is `begin_write` and `settings()` in opening the start folder, sorting the entries and the first frame laying out the rows.

### Adding a code folder

添加文件夹 on a folder of code checkouts used to stall the window for seconds: `list` went into every `node_modules`, `target` and `.venv`, wrote an entry for each of their folders and read every Markdown file there, and the 2 s check walked the whole tree again on the main thread. Now (see [What a folder shows](FORMAT.md#what-a-folder-shows)) the scan does not go into dependency and build folders or what `.gitignore` leaves out, lists only Markdown, DocxXML, whiteboards and 数据表, and writes no entry for a folder with nothing to show. The listing after adding runs on a background thread under the window's loading layer drawn over what is already shown (the sidebar keeps its rows), and when the 2 s check sees a top-level folder change, that folder is listed in the background and handed back (`SharedStore::begin_relist` / `end_relist`); if the window saved meanwhile, its own store lists again instead. Before adding, a folder with a project file at its top or one level down, or with more than 20,000 files, asks first (`shown::survey`, off the main thread; a folder that already has `.quiet/` does not ask).

A generated tree of 40 checkouts (`tools/perf/gen_code_tree.py`, 315,285 files, 50,406 of them `.md` / `.xml` and folders the old scan took in), measured 2026-11-26 on the Mac mini above against `fdf3684`. Core steps with `add_folder_bench` (two runs each); the window with the harness, a timer on the main thread logging how late it fires:

| | Before | After |
|---|---:|---:|
| First `list()` | 1,091-2,946 ms | 75-82 ms |
| Later `list()` (window activated) | 584-611 ms | 62-66 ms |
| `shape_at` (the 2 s check) | 521-613 ms | 60-114 ms |
| Entries listed | 50,406 | 206 (126 notes, 80 folders) |
| `.quiet/` written | 14 MB | 60 KB |
| Core memory | 324 MB | 8 MB |
| Longest main-thread stall adding it (window) | 1.26 s + 0.6 s (4.6 s cold) | under 100 ms |
| Opening its folder page | 3.7 s, then ~110 ms frames | no stall |
| Window peak memory | 846-852 MB | 152 MB |
| 20 s of builds writing into `target/` | stalls up to 1.05 s, 3.07 s in all; `.quiet/` 28 MB | none over 80 ms; 128 KB |
| 20 s of notes written into `docs/` | stalls up to 1.02 s | none over 80 ms |

The same tree with its `Cargo.toml` / `package.json` / `pyproject.toml` (`--project-files`, the usual case, where `target`, `build` and `dist` are known for build output): `list()` 9-11 ms (28-30 ms the first time), `shape_at` 9-15 ms. The harness window misses its timer by 60-80 ms some 220 times a minute even idle, in both builds (its frame cadence parked off screen), so only longer delays count as stalls above. Activating the window still lists on the main thread: 63 ms at worst on this tree, 9-11 ms with project files.

### Reproduce

```sh
cargo build --release --locked --features devtools
python3 tools/perf/gen_library.py --vault 15000 /tmp/quiet-big/vault
python3 tools/harness/run.py tools/harness/examples/big-library.txt /tmp/quiet-big/vault /tmp/quiet-big/run
grep -a bench /tmp/quiet-big/run/log
# list and save: on a copy, opened once by the app or by the first run
cp -R /tmp/quiet-big/vault /tmp/quiet-big/store
QUIET_MD_HOME=/tmp/quiet-big/store cargo run --release -p fdl_core --example store_bench /tmp/quiet-big/store
# first open: a fresh copy under $TMPDIR (its parse cache goes there too), recorded from the first frame
FDL_RECORD_START=/tmp/quiet-big/launch.mp4 python3 tools/harness/run.py <script> /tmp/quiet-big/vault $TMPDIR/quiet-big-run
grep -a 'library:' $TMPDIR/quiet-big-run/log
# adding a code folder: the tree under $TMPDIR, so what this computer keeps about it goes there too
python3 tools/perf/gen_code_tree.py $TMPDIR/codetree        # --project-files for the usual case
TMPDIR=$TMPDIR cargo run --release -p fdl_core --example add_folder_bench -- $TMPDIR/codetree 3
```
