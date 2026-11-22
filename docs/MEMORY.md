# Memory experiments

[Issue #173](https://github.com/s1dashu/quiet-md/issues/173) reported about 33 MB at startup on Windows and over 400 MB after opening a few sample documents. Another report reproduced the increase simply by opening Home.

## Results

| Scenario (macOS footprint, MB) | M5 Pro before | M5 Pro after | M1 Pro before | M1 Pro after |
|---|---:|---:|---:|---:|
| Photo library startup | 230 | 184 | 156 | 110 |
| Photo library Home (8 cards with 4032×3024 photos) | **1160** | **207** | **1082** | **130** |
| Open photo document | 873 | 222 | 797 | 147 |
| Return to text-only document | 825 | 222 | 749 | 147 |
| After 35 seconds | 377 | 202 | 208 | 127 |
| Guide library startup | 304 | 221 | 214 | 128 |
| Guide library Home | 324 | 228 | 239 | 138 |
| After viewing all guides in the script | 328 | 255 | 236 | 179 |
| Guide library after 35 seconds | 298 | 233 | 222 | 157 |
| After editing and scrolling a million-character document (M5) | 346 | 281 | — | — |

In a production build without devtools, leaving the M5 Pro on the document page after startup gave 106 → 76 MB for the photo library and 165 → 109 MB for the guide library.

The main cost was decoding document images and home card thumbnails at full resolution. A 4032×3024 photo takes 48.8 MB in BGRA and another copy in GPU texture memory, roughly 98 MB total, even when the card displays it at about 190 pt wide. Previously, `DocImages` released offscreen images after 30 seconds and imposed no size limit on visible images.

Frame times for typing, backspacing and scrolling the same million-character document stayed within measurement noise (experiment 4).

Windows measurements used Task Manager's Memory column (private working set), the same photo library and a keyboard-driven script. Two runs agreed. The table reports the second run in MB.

| Scenario | Private working set before | After | Commit size before | After | Dedicated GPU before | After |
|---|---:|---:|---:|---:|---:|---:|
| 6 seconds after launch (home cards decoding) | 497 | 35 | 1007 | 172 | 138 | 138 |
| Home (8 photo cards) | **406** | **34** | 914 | 172 | **516** | **146** |
| Open photo document | 129 | 46 | 352 | 195 | 233 | 158 |
| Return to text-only document | 82 | 34 | 305 | 183 | 233 | 158 |
| After 35 seconds | 35 | 33 | 163 | 160 | 138 | 138 |

The original Home measurement reproduced the reported 400 MB. After the changes, Home and startup had similar private working sets.

## Measurement method

[mem_probe.py](../tools/perf/mem_probe.py) replays a harness script in a temporary library and measures the macOS process with `footprint` at each `shot`.

- **Photos.** [gen_photo_library.py](../tools/perf/gen_photo_library.py) creates eight documents each containing one phone-sized 4032×3024 photo, plus a text-only Notes document. [photos.txt](../tools/perf/mem/photos.txt) goes from startup to Home (eight cards), opens Photo 1, opens Notes and waits 35 seconds for cache expiry.
- **Guides.** Build the bundled guides with `shared-mock/guides/build.py`. [guides.txt](../tools/perf/mem/guides.txt) goes from startup to Home, opens the welcome page and five Writing and Formatting guides, opens a text-only guide and waits 35 seconds.

Columns in MB:

- `total`: total footprint.
- `pixels`: MALLOC_LARGE plus untagged VM, including decoded images and other large buffers. Before the changes this also included freed blocks retained by malloc. Directly mmapped large blocks now appear under untagged VM.
- `small`: MALLOC_SMALL plus TINY.
- `gpu`: IOAccelerator plus owned unmapped (graphics), covering textures and buffers.
- `surface`: the window's IOSurface swap chain.
- `cg`: CG image.

The M5 Pro had 48 GB and ran macOS 26.5. The M1 Pro had 16 GB and ran macOS 27.2, where names such as “Malloc Large” use different capitalization, supported by the script. Both used a 1280×800 pt window at 2× and a devtools build with offscreen rendering. Repeated runs of the same binary varied by up to ±2 MB, apart from `surface` alternating between 32 and 48 MB for two or three swap-chain images. Offscreen rendering itself uses about 70 MB extra GPU memory on M5, accounting for higher devtools startup figures.

```sh
cargo build --release --features devtools
python3 tools/perf/gen_photo_library.py /tmp/q-photos
python3 tools/perf/mem_probe.py tools/perf/mem/photos.txt /tmp/q-photos /tmp/q-run/photos
```

## Experiments

All figures below are M5 Pro devtools `total` MB. Each column includes the preceding changes.

| Scenario | Baseline | 1a | 1b | 2 | 3 | 4 | 5 |
|---|---:|---:|---:|---:|---:|---:|---:|
| Photos startup | 212 | 214 | 229 | 229 | 199 | 184 | 184 |
| Photos Home | 1157 | 309 | 262 | 257 | 226 | 211 | 208 |
| Photo document | 871 | 352 | 331 | 287 | 255 | 225 | 222 |
| Photos after 35 s | 376 | 261 | 250 | 256 | 219 | 205 | 202 |
| Guides Home | 335 | 288 | 276 | 276 | — | 228 | 228 |
| After opening guides | 340 | 333 | 334 | 330 | — | 257 | 257 |

1a decoded for display size. 1b added JPEG IDCT scaling. Column 2 discarded CPU pixels after upload and included the 15% IDCT tolerance described below, which accounts for most of the photo document's 331 → 287 MB drop. Columns 3–5 changed the app icon, mmapped large allocations and reduced thumbnail size respectively.

### 1. Decode for display size (adopted)

`editor::image::picture(path, long_edge)` replaced `img(path).doc_picture()`.

- The cache key includes the long edge times window scale, rounded up in 128 px steps below 1024 and 512 px steps above it. Dragging to resize an image can reuse decodes across frames.
- After a background decode, `thumbnail_exact` downsamples by area averaging. `RenderImage::with_scale_factor` preserves the natural size reported to layout.
- Body images target their displayed frame, covers their cover frame, home images their size inside the card, and file previews their preview frame. The lightbox follows the current zoom up to original size.
- While a new size decodes, an existing size of the same image fills in to prevent flashing. Once a different size is visible, the next sweep releases the old one.
- Animated GIFs still use the GPUI loader with all frames and their original dimensions.

The first 1a Home measurement was still 611 MB. Without an explicit thumbnail width, `thumb_blocks` used the original width (4032). Clamping to the 690-wide body column fixed this. After 1a, `pixels` still held about 70 MB in two freed full-size RGB decode buffers of about 35 MB each. They stayed in malloc's large cache despite `malloc_zone_pressure_relief`.

In 1b, `jpeg-decoder` scales during IDCT and emits half-, quarter- or eighth-size output directly. This avoids the full-size buffer and speeds photo decoding. IDCT offers discrete size steps. Allowing a result up to 15% below target lets a 2048 target use 2016 pixels. Without that tolerance it fell back to full size. MallocStackLogging traced a 36 MB block to `jpeg_decoder::compute_image`. PNG, GIF, CMYK and 16-bit JPEG still use `image`. The added dependency was `jpeg-decoder 0.3` (MIT/Apache-2.0, rayon disabled, no transitive dependencies).

Text in home card screenshots became sharper. Previously the GPU used bilinear filtering on a texture 4.5 times larger, producing jagged edges. In document bodies, 1600 px guide screenshots shown at about 1380 px remained nearly pixel-identical, with a mean difference of 0.02/255.

### 2. Release CPU pixels after GPU upload (adopted)

GPUI reads pixels when first inserting an image into the atlas. Subsequent frames need only its dimensions. [Vendor patch 24](../vendor/PATCHES.md) implements this:

- `RenderImage::hollow()` retains the ImageId and dimensions without the pixel data.
- Once all windows have drawn an image, the next cache sweep replaces it with the hollow version.
- If a window lacks the atlas entry (first draw in a new window or an atlas rebuilt after Windows device loss), `paint_image` skips it and records it for `Window::take_missing_images`. The cache decodes it again on the next frame.
- Animated images and images shown in windows that do not report frames retain their pixels.

Temporary logging confirmed that all eight Home thumbnails and the 2016×1512 photo document image became hollow. On macOS this change alone left footprint unchanged: the freed 12 MB appeared in vmmap as `MALLOC_LARGE (empty)`. Experiment 4 addressed this. The Windows heap returns blocks above 1 MB directly to the OS, and Task Manager's Memory column measures this private CPU memory, making Windows the main beneficiary of this step.

### 3. Application icon (adopted)

MallocStackLogging traced a persistent 32 MB CG image at startup to `set_app_icon` → `-[NSApplication setApplicationIconImage:]`. AppKit treated the 1024 px PNG as 1024 pt and rendered a 2048² bitmap on Retina. Setting the NSImage size to 256 pt (maximum Dock magnification size) reduced it to 2 MB.

### 4. Use mmap for large allocations (adopted on macOS and Linux)

The global allocator in [big_alloc.rs](../src/big_alloc.rs) uses mmap/munmap for blocks of 1 MiB or more with alignment up to 4096. Other allocations use the system allocator. This returns freed pages immediately, as the Windows heap does. macOS malloc and glibc retain freed large blocks for reuse, keeping footprint high after images, decode buffers and large text allocations are released. With mmap, `pixels` fell nearly to zero and the existing `release_freed` mechanism remained useful for small blocks.

Frame processing on the million-character document with `readme-performance.txt` (P50 / P95 in ms):

| Build | Typing | Backspacing | Scrolling |
|---|---|---|---|
| Baseline | 5.14 / 6.22 | 5.10 / 5.71 | 3.69 / 7.92 |
| 1–3 | 5.36 / 6.18 | 5.38 / 5.88 | 3.23 / 6.47 |
| 1–4 | 5.30 / 6.04 | 5.26 / 5.88 | 3.22 / 6.49 |

Other builds were running on the machine during measurement. Differences were within the noise across these runs.

### 5. Smaller thumbnails (adopted)

Home cards decode at 0.75 times their drawn size (`THUMB` in `home.rs`). The slightly softer result was indistinguishable on the cards. With experiment 2 already releasing CPU copies, the savings on macOS were about 3 MB of GPU textures.

### 6. Downsample PNGs during decoding (adopted)

Previously `image` decoded the whole PNG before `thumbnail_exact`. A 3000×1600 image occupied 8 MB of file data plus 14 MB of RGB, adding about 50 MB transiently with two parallel decoders (`DECODERS`). `decode_png` now streams rows using `png` and box-averages to the target size, weighting by alpha where present. It retains one source row and one accumulator row. Interlaced PNGs and images that need no downsampling still use `image`. The new direct dependency is `png 0.18`, already used by `image`.

- Concurrency changed from two decoders to half the core count, clamped to 2–4. Each uses a row and its reduced result. Requests run in paint order (top to bottom). If the latest window frame no longer drew the image, discard the slot and requeue when visible again.
- On a 15-core machine with four decoders, time from opening a folder to its last card decoding was 182–191 → 180–192 ms. Two large scrolls took 160–170 / 121–149 → 149–168 / 134–145 ms.
- For 120 synthetic PNGs (1.2–6.6 MP, 990 MB total), two threads decoding to 512 reduced peak RSS from 68 to 16 MB. Time went from 1.40 to 1.50 s.
- With the harness opening a 40-card folder and scrolling to the bottom (`FDL_NOPARK`, footprint sampled every 0.25 s), peaks went from 291/331 to 289–300 MB and settled values from 216–225 to 216–233 MB. The earlier isolated decode spikes disappeared.
- With the same library's images scaled to 1/20, peaks still reached 220–260 MB and settled at 141–152 MB. About 80 MB of “Owned physical footprint (unmapped) (graphics)” appeared while painting and became reclaimable after one or two idle seconds. Its source was unresolved. The MSAA path texture was Memoryless, zero bytes and allocated only once. The roughly 70 MB higher settled footprint of the large library came from the 120 thumbnails' atlas textures (IOAccelerator 31 → 87–95 MB), retained up to 30 seconds after leaving the screen.

### Alternatives considered

- **Two swap-chain images instead of three.** Saves 16 MB at 2560×1600. At 120 Hz the CPU may have to wait for a drawable while encoding the next frame, potentially dropping frames. Offscreen harness timings cannot measure that effect. A physical display test would be needed.
- **Small malloc blocks.** About 40 MB at guide library startup. Grouping by call stack found 82 MB in live allocations overall, largely one-time window, font and menu structures plus GPUI's per-frame text layout cache. No individual app-level owner stood out. The 40 MB “keyboard layout” in malloc_history was actually an mmap of ICU data (clean pages excluded from footprint).

## Windows measurements

The machine ran Windows 11 build 26200 on an i5-13600KF with an RTX 4070 Ti, 32 GB RAM and a 1920×1080 display. Both builds used `cargo build --release --locked --features devtools`. The baseline was pre-merge main and the comparison was the experiment branch.

A scheduled task launched the app in the desktop session, since windows launched from SSH were not visible. The keyboard-only `FDL_SCRIPT` waited 7 seconds, opened Home with `ctrl-alt-home`, jumped to Photo 1 and then Notes with `ctrl-j`, and waited 40 seconds. Samples at 6, 13, 21, 28 and 61 seconds recorded:

- Private working set via the `Process\Working Set - Private` counter (Task Manager's Memory column).
- Commit size via `PrivateMemorySize64`.
- Dedicated and shared usage via `GPU Process Memory`.

New libraries open on Home, so the six-second sample already included its cards. Screenshots confirmed both thumbnails and body photos rendered after the changes. The results were consistent with experiments 1 and 2 working on Windows. Experiments 3 and 4 concern macOS/Linux.

## Device loss

Restarting the Windows graphics device with `pnputil /restart-device` could not isolate the cache behavior in this experiment. Both builds crashed on the first device loss. A failed swap-chain resize in `directx_renderer.rs` left `render_target_view` empty and it was unwrapped. The existing GPUI issue was tracked separately.

To isolate recovery of hollow images when the atlas is cleared, the devtools command `forget-uploads` removes every cached hollow image from the window atlas and draws one frame, simulating an atlas rebuild.

- Initially, all eight Home cards were blank even 2.5 seconds later. Frame N recorded the missing image, but decoding required frame N+1. Home had no blinking caret or other source of frame requests, so the cards waited for mouse movement.
- After `paint_image` began requesting the next frame when recording a miss, all eight cards and the body photo recovered in about one second on both macOS and Windows.
