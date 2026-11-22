# Whiteboard rendering benchmarks

Generate a scene with `board_gen.py perf <n>`: n rounded rectangles with text, arrows bound between neighbors and n/4 freehand strokes of 60 points each (about 3.25n elements). Run `python3 tools/perf/board_bench.py` to replay `board_bench.txt`. Set `FDL_BOARD_RENDER=gpui|vello|auto` to choose a backend and `FDL_BOARD_PROF=1` to log backend, visible count and duration per draw.

The initial measurements are devtools `bench` frame P50 in ms: CPU time for a synchronous draw, excluding GPU rasterization. M5 Pro, 2× display, 1280×800 window, 2026-11-14.

- Overview pan: fit to content at startup (10% minimum, so only part of large scenes is visible, about 3000–7000 elements from n=1000). Pan back and forth vertically and horizontally. Average four groups.
- Hover: move across the canvas for hit testing and redraw. Average two groups. See the November 15 correction below before interpreting this column.
- 100% pan: reset with ⌘0, then scroll back and forth with a few dozen elements visible. Average two groups.
- Zoom: average one ⌘= and two ⌘- groups.

| Version | n | Overview pan | Hover | 100% pan | Zoom |
|---|---|---|---|---|---|
| Baseline | 200 | 4.6 | 5.0 | 3.8 | 7.6 |
| Baseline | 1000 | 20.4 | 24.2 | 11.1 | 14.3 |
| Baseline | 3000 | 69.2 | 76.1 | 34.0 | 37.6 |
| Baseline | 10000 | 324.0 | 324.8 | 250.7 | 254.8 |
| GPUI | 200 | 3.0 | 3.5 | 2.4 | 6.1 |
| GPUI | 1000 | 10.2 | 16.0 | 1.1 | 4.0 |
| GPUI | 3000 | 28.8 | 33.7 | 1.9 | 5.5 |
| GPUI | 10000 | 47.1 | 47.0 | 3.0 | 5.3 |
| vello | 200 | 3.3 | 1.8 | 3.6 | 6.5 |
| vello | 1000 | 5.2 | 1.9 | 4.3 | 5.7 |
| vello | 3000 | 8.3 | 1.6 | 3.8 | 5.1 |
| vello | 10000 | 11.7 | 1.2 | 3.5 | 5.5 |
| Auto | 200 | 3.8 | 3.9 | 2.3 | 5.9 |
| Auto | 1000 | 5.4 | 1.8 | 2.7 | 5.7 |
| Auto | 3000 | 8.7 | 1.5 | 2.4 | 5.4 |
| Auto | 10000 | 11.7 | 1.8 | 2.7 | 5.1 |

Two costs dominated the baseline:

1. When a standalone `.excalidraw` file was open, the hidden thumbnail in the underlying document redrew the full scene every frame (~250 ms at n=10000). Covered thumbnails now skip drawing. Visible thumbnails switch to vello at 64 visible elements and reuse the previous frame while unchanged, including during document scrolling.
2. Rebuilding paint order and looking up every cached element cost ~3.3 ms per frame at n=10000. With unchanged scenes (pan, zoom, hover), reusing order and bounds leaves only bounds culling (~0.2 ms).

The initial thresholds switched to vello at ≥800 visible elements and back to GPUI below 500 to prevent oscillation. CPU times converged at ~650 visible elements (n=200 overview). Above that, GPUI added ~3–5 µs per element, vello ~1.5 µs. GPUI also incurred unmeasured 4× MSAA GPU rasterization. Vello's full-screen rasterization and IOSurface copy cost ~1.5 ms per frame even with few elements. Unchanged scenes such as hover could reuse the previous vello frame.

Zoom key handling (step only, excluding draw) previously scaled linearly: ~11.7 ms at n=10000 and ~3.5 ms at n=3000. For every action, `run.rs` copied the entire element array and committed viewport changes through the store, comparing whole-scene snapshots. Zoom actions now update only the viewport, taking 0.02 ms at n=10000.

## Comparison with upstream Excalidraw (2026-11-15)

Upstream ran the production npm build of `@excalidraw/excalidraw` 0.18.0 with React 19 in embedded Chromium, using `board_upstream.html` at 1280×800 and 2×. Quiet.md used the experiment branch with rough.js rendering, bundled fonts and automatic backend selection. Both ran the same `board_gen.py perf` scenes with roughness set to 1 (the default sketch style). Zoom and steps matched `board_bench.txt`. Overview zoom was 12% at n=1000 and 10% otherwise.

The measurements have different boundaries. Quiet.md timed CPU event processing plus synchronous draw (step + frame). Upstream measured main-thread busy time within 150 ms after dispatch, including React, canvas drawing and GC. Both exclude GPU rasterization. Values are group averages of P50 times in ms.

| n (elements) | Scenario | Upstream | Quiet.md | Ratio |
|---|---|---|---|---|
| 1000 (3225) | Overview pan | 12.7 | 4.6 | 2.8× |
| | Hover | 4.2 | 1.0 | 4× |
| | 100% pan | 2.8 | 1.4 | 2.0× |
| | Zoom | 5.9 | 2.9 | 2.0× |
| 3000 (9706) | Overview pan | 26.4 | 10.2 | 2.6× |
| | Hover | 9.4 | 1.5 | 6× |
| | 100% pan | 8.9 | 1.5 | 6× |
| | Zoom | 19.1 | 3.2 | 6× |
| 10000 (32420) | Overview pan | 47.1 | 13.2 | 3.6× |
| | Hover | 29.9 | 2.2 | 14× |
| | 100% pan | 20.4 | 1.6 | 13× |
| | Zoom | 50.9 | 3.4 | 15× |

Upstream scanned all elements every frame for visibility and React rendering. Even 100% panning with only dozens visible slowed with scene size. Quiet.md reused paint order and culled by bounds, keeping those frames nearly independent of element count. Upstream P95 often reached 50–130 ms during GC and element canvas cache rebuilds. Quiet.md P95 stayed within 20 ms. Roughness 0 and 1 performed similarly because rough paths are cached per element.

The comparison uses corrected hover measurements from November 15. Previously `bench move` dispatched system mouse events while the window was borrowed by the synchronous update. Dropped events timed an empty draw. The **hover column in the opening table is unreliable** for the same reason. Benchmark pointer moves now call `Window::dispatch_event` directly in `src/devtools.rs`.

## Drag latency (2026-11-15)

At 100%, `board_drag.txt` holds a rectangle with arrows bound on both sides and drags it back and forth by 4 pt (four groups of 30 steps), then selects all with ⌘A and drags. Upstream's `runDrag()` in `board_upstream.html` uses the same rectangle and steps. Figures are single-step P50 ms with P95 in parentheses. See the timing correction below.

On November 17 we found that the old pointer-down position landed between rectangles in the current layout. That measured marquee selection (~4 ms at n=10000 while `get_elements_within_selection` computed all bounds). Moving the point inside the rectangle gave ~1.9–2.1 ms for single-element drags at n=1000, 3000 and 10000, consistent with round two.

| n (elements) | Upstream | Before optimization | After |
|---|---|---|---|
| 1000 (3225) | 5.1 (18–27) | 3.2 | 1.9 (2.5) |
| 3000 (9706) | 8.0 (33–39) | 5.9 | 2.0 (2.5) |
| 10000 (32420) | 17.2 (60–130) | 17.8 | 2.1 (2.6) |

Correction: the Before optimization and After columns above measure frame time only. Step took ~0.08 ms for a single-element drag. Round two below uses step + frame.

Dragging all elements upstream took 65 ms, 395 ms and a freeze of several seconds at 32,420 elements. After round one Quiet.md took 23 / 64 / 243 ms (step + frame).

Three costs previously scaled with the total element count on every drag step:

1. The properties panel repeatedly scanned for selected elements and built full maps more than ten times per frame, about half the cost at n=10000. `BoardView::selection_scope` now uses selected elements, their labels and label containers, and text being edited. Selections above 256 still use the whole scene. `BoardView::selected_elements` looks up IDs and sorts by scene order.
2. Each step copied the scene for snapping even when the cache already existed. Copy only when `SnapCache::wants` requests a rebuild.
3. Previously each scene change (every drag frame) rebuilt all paint order, filtering visibility and querying caches. When membership, order and bindings stay stable, `Order` in `render.rs` updates only changed elements, their labels/containers, caches and bounds. Frame changes affecting child clips or changes to more than one eighth of the scene still rebuild the whole list.

### Round two (2026-11-15)

The benchmark now uses separate updates for processing the event (including effects and observers, as in a real event) and drawing the frame. Step includes the first update. Figures are step + frame P50 ms with P95 in parentheses.

| n (elements) | Drag one | Drag all | Hover (10000, pointer moving) |
|---|---|---|---|
| 1000 (3225) | 1.7 (2.6) | 5.7 (7–9) | |
| 3000 (9706) | 2.0 (2.4–5.8) | 10.4 (12–15) | |
| 10000 (32420) | 1.9 (2.6–8) | 25.4 (28) | 0.9 (1.3), after round one 2.4 |

After round one, dragging the entire selection took 23 / 64 / 243 ms. For a board opened in a long document (six repetitions of `all-blocks`), dragging one element improved from 1.5 to 0.9 ms.

Changes in round two:

1. Reuse shapes under translation. `Scene::translate_element` updates x/y and records the translated version range in `Scene::translation`. If element, label, container and frame all moved equally, the cache translates bounds and clipping, deferring the shape origin adjustment until draw (`moved_by` / `settle` in `render.rs`). Upstream similarly invalidates `ShapeCache` only on size or point changes.
2. `drag_selected_elements` avoids four copies per element, computes original bounds once per drag and reads the clock once per step (`with_one_timestamp`). It skips snap data when snapping is off.
3. Cache the properties panel until selection, app state or selected elements change, excluding translations (`properties::cached_model`, `BoardView::scope_stamp`).
4. Build and draw selection boxes only for elements visible on the canvas.
5. Hit testing uses a 256×256 grid (`canvas::HitIndex`), built on the second hit test after a scene change. A drag with one test per step avoids rebuilding.
6. `chrome::ChromeView` caches toolbar, panels and footer layout and paint across hover and selection drags if app state and selected elements (apart from translation) are unchanged. Other changes redraw. A/B single-element drag frame time: 2.76 → 2.45 ms.
7. When a board covers the window, skip drawing underlying document and sidebar rows. Reserve sidebar width and keep document layout. Single-element drags saved ~0.5 ms for the sidebar and ~0.65 ms for long documents per frame.
8. GPUI Metal wrote path vertices directly to the instance buffer (vendor patch 36), reducing frame submission from ~0.84 to ~0.21 ms. This runs in a later system callback outside `bench` and was measured with `sample`.

## Shared backend across three platforms (2026-11-16)

Originally vello used macOS IOSurface pixel buffers and the Metal surface pipeline. All three platforms then copied frames into `PixelFrame` and displayed them as GPUI sprites with `Window::paint_pixels`, rewriting the atlas tile in place (legacy vendor patch 36). Glyph outlines came from CoreText / DirectWrite / swash (patch 37). macOS screenshots matched pixel for pixel. The extra atlas copy added ~0.5–0.9 ms when panning with vello.

Windows, i5-13600KF, RTX 4070 Ti, 1280×800 window. Same `board_bench.txt` procedure, step + frame P50 ms:

| Scenario | Visible elements | Before (GPUI only) | After (Auto) |
|---|---|---|---|
| n=1000 Overview pan | ~3100 | 13.5–16.3 | 8.5–13.3 |
| n=3000 Overview pan | ~5300 | 20.7–41.2 | 12.1–22.1 |
| n=10000 Overview pan | ~5300 | 40.3–46.3 | 14.2–19.6 |
| n=3000 / 10000 Hover | ~5300 | 37.9–39.2 | 1.5–2.3 |
| 100% pan, Zoom, board with images | 50–400 | 2–7 | 2–7 (still GPUI) |

Vello's fixed frame cost on Windows was ~5–6 ms (~3.5 ms on macOS), favoring GPUI for sparse scenes. Thresholds stayed unchanged. Linux ran under WSL2/WSLg with X11 and lavapipe software Vulkan. The smaller window showed ~1190 elements in overview. Panning at n=3000 / 10000 took 4.7–5.8 ms on GPUI and 1.1–2.0 ms on vello.

## After upgrading GPUI (2026-11-17)

Upstream GPUI (Zed e2f00a4) bins curves into tiles on the CPU and draws by coverage on the GPU without triangulation or MSAA. The board still caches paths per element and reuses them across pan and zoom with `Path::transformed`, but visible paths need binning every frame. Vello now creates a `RenderImage` whenever the scene changes. Legacy patch 36 overwrote the same atlas tile and was dropped.

Same machine and script, CPU-only frame P50 ms, backend forced with `FDL_BOARD_RENDER`. Old refers to the contemporary main build (patched GPUI 0.2.2).

| Backend | n | Version | Overview pan | Hover | 100% pan | Zoom |
|---|---|---|---|---|---|---|
| GPUI | 200 | Old | 3.3 | 3.0 | 2.3 | 4.7 |
| GPUI | 200 | New | 3.7 | 3.8 | 1.7 | 4.0 |
| GPUI | 1000 | Old | 9.1 | 12.6 | 1.9 | 4.5 |
| GPUI | 1000 | New | 11.4 | 15.0 | 2.5 | 5.4 |
| GPUI | 3000 | Old | 18.1 | 21.2 | 1.9 | 5.0 |
| GPUI | 3000 | New | 21.6 | 25.9 | 2.0 | 4.3 |
| vello | 200 | Old | 3.3 | 0.8 | 3.2 | 3.9 |
| vello | 200 | New | 4.6 | 0.6 | 4.5 | 4.7 |
| vello | 1000 | Old | 5.5 | 0.7 | 3.2 | 3.9 |
| vello | 1000 | New | 6.7 | 0.7 | 4.3 | 5.3 |
| vello | 3000 | Old | 9.1 | 1.1 | 3.5 | 3.2 |
| vello | 3000 | New | 10.1 | 0.6 | 4.4 | 4.8 |

The new version took 10–25% longer on CPU. GPUI spent extra time binning every frame, and vello took ~1 ms longer to create and upload full-frame images. GPU differences (old 4× MSAA and an intermediate texture) were unmeasured. Automatic selection (vello at ≥800 visible elements) gave ~10 ms on large scenes. In-place atlas updates on Metal, DirectX and wgpu, as in legacy patch 36, could recover the extra millisecond.

The `showcase` comparison included rotated frame clipping, arrow label cutouts, dashes, fills, rotated text and images. Shapes, clipping and cutouts matched. Differences affected ~1% of pixels at antialiased and glyph edges.

## Remeasuring thresholds and profiling (2026-11-17)

### Removing a vello frame allocation and copy

Previously each frame copied rendered RGBA pixels into a new 12 MB buffer, converted it to BGRA and created a `RenderImage`. After `drop_image`, `RenderImage::into_frames` (patch 0034) now recovers the buffer so vello draws directly into `PixmapMut` and converts to BGRA in place. The copy step fell from ~1.0 to 0.5 ms and screenshots matched pixel for pixel.

With `FDL_BOARD_RENDER=vello`, mean frame P50 for the first four (overview pan) groups in `board_bench.txt`, two runs each, in ms:

| n | At merge | After |
|---|---|---|
| 200 | 4.61 | 3.91 |
| 1000 | 6.78 | 6.10–6.17 |
| 3000 | 10.24 | 9.57–9.76 |

Disabling lossless compression of Metal atlas textures (`allowGPUOptimizedContents = false`) slowed uploads by ~0.8 ms and was discarded.

### Thresholds

Two runs per backend panned the overview for `board_gen.py perf <n>` at n=40–600 (95–1440 visible elements). At merge time the backends converged near the old threshold of 800 visible elements. Faster vello frames shifted the crossover to ~500–600.

| Visible elements | GPUI | vello (After) |
|---|---|---|
| 385 | 3.5 | 3.8–4.0 |
| 478 | 3.4–3.7 | 3.8–3.9 |
| 598 | 3.7–4.2 | 3.6–3.9 |
| 705 | 4.6–4.8 | 4.1–4.2 |
| 964 | 4.8–5.3 | 4.2–4.6 |

macOS now uses vello at ≥600 visible elements and returns to GPUI below 400. Windows and Linux retain 800 / 500 pending a new Windows measurement of the previously observed 5–6 ms fixed vello frame cost.

### Frame profile (sample, n=1000 overview, ~3000 visible elements)

Vello main thread, ~6.4 ms per frame:

- Submitting individual elements to vello_cpu cost ~4 ms (~70% `fill_path`, ~12% `stroke_path`). Almost all of that was inside vello_cpu copying paths, estimating task cost and batching worker submissions. Version 0.3 has no record/replay API, so panning resubmits every path each frame.
- Display cost ~1.25 ms: `paint_image` uploads the entire frame to the atlas via `replaceRegion` with lossless compression by the driver.
- Worker threads rasterize. The main thread waited ~0.5 ms, with another 0.5 ms for BGRA conversion.

GPUI in the same scene:

- Path binning in `PaintedPath::new`: ~47%.
- `BoundsTree::insert` ordering primitives inserted into `Scene`: ~28%.
- `Path::transformed` allocating a curve array per path per frame: ~9%.
- Text layout and painting: ~10%.

These costs are inside GPUI. Candidate optimizations include caching bins across whole-device-pixel translations and batch-inserting board paths in their known order to bypass `BoundsTree`. For vello, a linear texture backed by an `MTLBuffer` could avoid the 1.25 ms upload by letting vello draw into GPU-readable memory. Each platform would need its own implementation.
