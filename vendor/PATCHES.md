# Vendored patches

Quiet.md takes GPUI from the [Zed repository](https://github.com/zed-industries/zed). The tree in `vendor/zed` combines a pinned upstream snapshot and an ordered local patch series. `vendor/zed/upstream.toml` records the commit and selected crates. Numbered patches in `vendor/zed/patches`, applied in number order, produce `vendor/zed/crates`, which the app consumes through Cargo path dependencies. Patched sites carry a `quiet.md:` comment.

Vendoring the selected crates avoids fetching the entire Zed repository for a contributor's first build or CI run.

## Layout

| Path | Contents |
|---|---|
| `vendor/zed/upstream.toml` | Repository, commit, crates to copy, excluded directories (examples and benches), and crates.io replacements for upstream Git dependencies |
| `vendor/zed/patches/NNNN-name.patch` | One patch with a description of what changed and why, followed by a diff relative to the Zed root |
| `vendor/zed/crates/…` | Generated tree preserving Zed paths such as `crates/gpui` and `crates/refineable/derive_refineable` |
| `vendor/zed/crates/*/Cargo.toml.orig` | Patched upstream manifests. Apply manifest edits here. |
| `vendor/zed/crates/*/Cargo.toml` | Standalone manifests generated from the originals. Expand inherited workspace fields and dependencies, remove dev dependencies, omitted crates and their features, replace Git dependencies with crates.io packages and substitute Zed's lint rules. |
| `vendor/zed/LICENSE-APACHE` | Zed license. These crates use Apache-2.0. |
| `tools/vendor-sync` | Synchronization tool |

The root Cargo.toml excludes the vendored crates from workspace tests and clippy. Save local edits as patches. The sync tool's check reports unrecorded changes.

## Sync workflow

```sh
# Define a shell function so the same commands work in sh and zsh.
vendor_sync() { cargo run -q --manifest-path tools/vendor-sync/Cargo.toml -- "$@"; }
vendor_sync check                       # Compare the tree against upstream plus all patches
vendor_sync save <name> -m "<message>"  # New patch with the current edits, numbered highest + 1
vendor_sync save <name> --number 0062 -m "<message>" # New patch with a number from your range
vendor_sync save <existing-name>        # Regenerate a patch in place, retain its number and message
vendor_sync sync                        # Rebuild vendor/zed/crates from upstream.toml
vendor_sync sync --rev <commit-or-main> # Update the pinned revision and report failed patches
vendor_sync sync --rev main --keep-going # Skip failed patches and write the remaining results
vendor_sync refresh                     # Refresh diff line numbers and context at the current snapshot
```

The tool keeps a blob-filtered Zed clone in `~/.cache/quiet-md/zed` (override with `QUIET_MD_ZED_DIR`) and sparsely checks out the selected crates. Intermediate files go in `target/vendor-sync`. Patches apply in number order via `git apply --3way`, with one commit per patch.

## Patch numbers

A patch keeps its number and file name for good: `save` and `refresh` never renumber the series, so gaps are normal. Branches that add patches at the same time take numbers from separate ranges, so their patches never collide when merged:

| Range | Line of work |
|---|---|
| 0046–0059 | Component library (design system) |
| 0060–0079 | Sync and iOS (mobile line) |
| 0080 and up | Performance |

Patches 0001–0045 came before the ranges were handed out. A new patch takes `--number <n>` from its line's range. Without `--number` it takes the highest number in the series + 1. `save` refuses a number that is already taken and never overwrites another patch, and it refuses to renumber an existing patch.

A patch numbered before others is saved so that the later patches still apply after it: `save` takes the edits relative to every other patch, then removes the later patches' changes from the diff. If the edits touch the same lines as a later patch, `save` stops and writes nothing; save them with a number after that patch, or fold them into it. After writing, `save` rebuilds the series and confirms it reproduces `vendor/zed/crates`.

For a new patch, edit files under `vendor/zed/crates` (use `Cargo.toml.orig` for manifest edits), run `vendor_sync save <name> -m "<message>"`, then `vendor_sync sync` to regenerate manifests and `vendor_sync check`. Save one related change at a time: each save includes all outstanding edits in the tree.

To upgrade upstream:

1. Run `vendor_sync sync --rev main --keep-going` and record failed patches.
2. Reimplement each failed patch from its description in the vendored tree and run `vendor_sync save <existing-name>` before starting the next. If upstream now provides the behavior, remove that patch file and document it below.
3. Run `vendor_sync refresh`, `vendor_sync check` and `cargo build`. Fix application build errors.
4. Run the validation procedure below, compare screenshots and update this file.

## Patch series

Snapshot: Zed `e2f00a41ac67` (2026-11-15). Legacy numbers refer to the GPUI 0.2.2 patch table.

| Patch | Legacy # | Files | Change and purpose |
|---|---|---|---|
| 0001 glyph-bounds-at-size | 2 | `gpui_apple/text_system.rs` | Measure glyph bounds with the CTFont at rasterization size. System optical sizes vary (SF Text → SF Display), previously clipping the left edge of large heading glyphs. |
| 0002 katex-fonts | 3 | Same as above | Load `KaTeX_*` fonts without an `m` glyph (Size1–4, AMS, Caligraphic, Script). |
| 0003 fallback-font-identity | 14 | Same as above | Map PostScript names to multiple FontIds and match CoreText fallback by glyph count. System PingFangUI.ttc and downloaded PingFang.ttc both provide PingFangSC-Regular with different glyph maps. Name-only lookup drew wrong characters. See `editor::text::tests::cjk_fallback_glyphs`. |
| 0004 synthetic-italic | 12 | Same as above | For missing italic faces, including fallback faces, use a slanted twin FontId, shear 14° during rasterization (WebKit's angle) and widen bounds. This supports browser-like Chinese italics. |
| 0005 grid-col-widths | 6 | `gpui/style.rs`, `styled.rs`, `taffy.rs` | `grid_col_widths(Vec<f32>)` overrides `grid_cols` with pixel widths for table columns. |
| 0006 paint-rows | 5 | `gpui/text_system/line.rs` | `WrappedLine::paint_rows` / `paint_background_rows` accept explicit row tops for lines of different heights containing formulas or buttons. |
| 0007 list-scroll-estimates | 7 | `gpui/elements/list.rs` | `ListState::estimated_scroll_metrics` and `scroll_to_estimated_offset` estimate unmeasured rows from average height for long-document scrollbars and dragging. |
| 0008 path-prompt-extensions | 8 | `gpui/platform.rs`, `gpui_macos/platform.rs` | `PathPromptOptions.extensions` filters NSOpenPanel selection to Markdown, text and XML on import. |
| 0009 offscreen-capture | 10 | `gpui/window.rs`, `platform.rs`, `gpui_macos/window.rs`, `gpui_apple/metal_renderer.rs` | Compile upstream `Window::render_to_image` outside `test-support`. Add `Window::render_offscreen` (run next-frame callbacks and draw offscreen without presenting) and `Window::needs_draw` for devtools capture while obscured or locked. |
| 0010 replace-root-type | 26 | `gpui/window.rs` | Update the root type in the handle during `Window::replace_root`. After onboarding replaces the root, `downcast::<Workspace>()` can find the workspace in the same window. |
| 0011 window-min-size | 27 | `gpui/window.rs`, `platform.rs`, `gpui_macos/window.rs`, `gpui_windows/window.rs`, `events.rs` | `Window::set_window_min_size` updates minimum content size after creation via macOS setContentMinSize or Windows min_size in WM_GETMINMAXINFO. The minimum width follows sidebar visibility and width. |
| 0012 present-in-transaction | 28 | `gpui/window.rs`, `platform.rs`, `gpui_macos/window.rs` | `Window::present_next_frame_in_transaction` routes the next frame through `PlatformWindow::draw_in_transaction`. On macOS, presentsWithTransaction presents native glass changes and drawing in the same Core Animation transaction. |
| 0013 hollow-images | 24 | `gpui/assets.rs`, `window.rs` | `RenderImage::hollow`, `is_hollow` and `with_scale_factor` preserve ID and dimensions while releasing uploaded CPU pixels. If the atlas lacks a hollow image, `paint_image` skips it, records it for `Window::take_missing_images` and requests the next frame. See docs/MEMORY.md. |
| 0014 load-visible-only | 20 | `gpui/elements/img.rs`, `image_cache.rs` | `ImageCache::peek` neither loads nor marks an entry used. `Img::load_visible_only` decodes explicitly sized images on visible paint, avoiding images clipped from cards. |
| 0015 hide-cursor | New | `gpui/app.rs` | Add `App::hide_cursor_until_mouse_moves`. Upstream replaced `CursorStyle::None` with `CursorHideMode` for typing. The app sets the latter to `Never` and calls the new method while typing in the editor or inputs and while idle in the image viewer. |
| 0016 font-smoothing | 1 | `gpui_apple/text_system.rs` | Use the strongest stem darkening for all text colors with `glyph_dilation_for_color` fixed at zero. Upstream varies smoothing by color. The app matches native AppKit text (disabling smoothing costs ~15% ink). Honor a disabled AppleFontSmoothing preference. |
| 0017 fallback-by-name | New | `gpui_apple/open_type.rs` | Build `FontFallbacks` descriptors by name with `CTFontDescriptorCreateWithNameAndSize`, as in 0.2. Family plus weight/slant selected PingFang.ttc (49,533 glyphs) instead of the system UI face (47,098). Different outlines made Chinese about 2% lighter. |
| 0018 clipboard-text-and-image | 22 | `gpui_macos/pasteboard.rs` | Read text and one image together (prefer PNG over TIFF) and skip empty image data. The editor can paste a screenshot while text fields can read accompanying text. Private pasteboard tests live in `pasteboard::tests`. |
| 0019 korean-word-wrap | 23 | `gpui/text_system/line_wrapper.rs` | Upstream UAX #14 (icu_segmenter) allows breaks between Hangul syllables. Suppress breaks inside Hangul/alphabetic words, including mixed words such as Quiet.md를. Oversize words still wrap by character. `test_break_indices_quiet_md` also covers Chinese punctuation and whole Arabic/Hebrew words. |
| 0020 rtl-wrap | 30 | `gpui/text_system/line_layout.rs` | For wrapped RTL paragraphs, `wrap_bidi` uses character widths in reading order to choose breaks, shapes each line separately, concatenates layouts and marks the first glyph of each line. Exclude trailing spaces from shaping. Breaking in visual order reversed multiline Arabic. |
| 0021 rtl-align | 32 | `gpui/text_system/line.rs` | Default to right alignment for wrapped paragraphs starting with an RTL letter. Preserve single lines, explicit alignment and `paint_rows`. |
| 0022 single-axis-scroll | 17 | `gpui/elements/div.rs` | Single-axis scroll containers accept only events dominated by their axis. Vertical scrolling across a wide table moves the page and horizontal scrolling moves the table. Upstream `restrict_scroll_to_axis` needs per-element opt-in and locks trackpad gestures only with tracked scroll handles. |
| 0023 windows-clipboard | 16 | `gpui_windows/clipboard.rs` | Upstream reads text, one image (including CF_DIB) and copied files together. Prefer PNG, otherwise convert bitmaps to PNG to reduce stored size. Correct upstream's extra 12 mask bytes for V4/V5 DIB headers. If all copied files are images, load them as images. Otherwise paste one path per line if there is no text. |
| 0024 windows-device-lost | 25 | `gpui_windows/directx_renderer.rs`, `events.rs`, `platform.rs`, `window.rs` | Upstream uses optional resources, propagates errors, invalidates failed resizes and tolerates missing atlas textures, but panics on failed recovery. Log platform errors and retry at next vsync. Keep the new device on window renderer failure and retry drawing later, skipping paints meanwhile and rebuilding the swap chain at current size. Reapply acrylic blur after recovery by switching off then on so an identical setting is honored. |
| 0025 windows-shader-path | 15 | `gpui_windows/directx_renderer.rs` | Pass debug shader paths as wide strings to D3DCompileFromFile, replacing `to_str().unwrap()`. |
| 0026 linux-appearance-reborrow | 18 | `gpui_linux/wayland/client.rs`, `x11/client.rs` | Copy window handles and release the client borrow before appearance callbacks reenter the platform to update menus, avoiding a borrow panic. |
| 0027 wgpu-30-crates-io | New | `gpui_wgpu/wgpu_context.rs`, `wgpu_renderer.rs` | Replace Zed's Git wgpu with crates.io 30.0.2 via `[git-to-crates-io]` in upstream.toml. Its DeviceDescriptor lacks default_queue and its has_srgb_suffix is named is_srgb. Linux only. |
| 0028 keybinding-load-unmapped | New | `gpui/keymap/binding.rs` | `KeyBinding::load_unmapped` parses keys for the specified OS without an App or keyboard layout mapping, like 0.2 load plus DummyKeyboardMapper. Upstream load requires &App and rewrites keys on Windows. Preserve Windows/Linux bindings and custom shortcuts. |
| 0029 web-system-font | New | `gpui_web/platform.rs` | `WebPlatform::new_with_system_font` selects the .SystemUIFont family (hardcoded upstream to IBM Plex Sans). The web app loads Inter in src/web.rs. |
| 0030 web-diagonal-cursors | New | `gpui_web/platform.rs` | Correct swapped diagonal resize cursors: ResizeUpLeftDownRight maps to CSS nwse-resize. Image corner cursors previously pointed the wrong way on web. |
| 0031 board-paths | 34, 35 | `gpui/path_builder.rs`, `window.rs` | Add `Path::transformed` (exact uniform scale and translation for monotonic quadratics), empty, is_empty, from_polygons and PathBuilder::build_polygons (flatten contours to tolerance, retaining each contour). Expose Window::with_element_opacity. Boards reuse per-element paths while panning/zooming. Frame clips and arrow label cutouts use flattened contours in src/board/render_geom.rs since upstream paths retain neither contour order nor triangles. |
| 0032 cosmic-bundled-fonts | 43, 38 | `gpui_wgpu/cosmic_text_system.rs` | Make add_fonts replace existing faces with the same family and style (bundled CBDT Noto Color Emoji overrides Fedora's COLRv1). Load Xiaolai without an m glyph. Linux and web. |
| 0033 glyph-outline | 37, 38 | `gpui/platform.rs`, `text_system.rs`, `gpui_apple/text_system.rs`, `gpui_windows/direct_write.rs` (adds `Win32_Graphics_Direct2D_Common`), `gpui_wgpu/cosmic_text_system.rs`, `gpui_web/text_system.rs` | `WindowTextSystem::glyph_outline` and GlyphOutlineCmd return outlines in font units, y upward. Use font-kit on macOS (including italic shear), DirectWrite GetGlyphRunOutline on Windows and swash on Linux/web. Web canvas fallback has no outlines. Board CPU rendering and rotated text use these outlines. Load Xiaolai without an m glyph on macOS. |
| 0034 image-into-frames | New | `gpui/assets.rs` | `RenderImage::into_frames` retrieves image frames. Vello creates an image on each board change. After drop_image, reuse the previous buffer to draw the next frame directly (src/board/render_vello.rs), avoiding a 12 MB allocation and copy per frame. |
| 0035 macos-file-image-clipboard | New | `gpui_macos/pasteboard.rs` | When a single file is copied, keep the bitmap the pasteboard carries with it, so the editor can paste pictures from apps such as WeChat. |
| 0036 web-focus-reentry | New | `gpui_web/events.rs`, `gpui_web/window.rs` | A browser focus or blur can fire synchronously while GPUI is handling another event (focusing the input-method mirror on a press, or the site moving focus into the iframe). Refreshing the window's active state then re-entered the window update and panicked with "attempted to read a window that is already on the stack", after which the app stopped drawing and taking input (the site's live notebook froze on the first click). Refresh the active state on the next turn instead. |
| 0037 rtl-decorations | New | `gpui/text_system/line.rs` | `paint_line` and `paint_line_background` look a glyph's decoration run up again from the start when its byte index runs backwards. Right-to-left glyphs come in visual order, so the forward-only lookup painted a coloured, underlined or highlighted Arabic span with the previous run's style. Lines without such scripts never go backwards and paint as before. |
| 0038 windows-rtl | New | `gpui_windows/direct_write.rs` | DirectWrite lists a right-to-left run's glyphs in reading order and advances them leftwards from the run's right edge. Place them left to right and return them in visual order, as CoreText does. Lines whose first letter is right-to-left get a right-to-left reading direction with a finite width and no wrapping, so run origins stay within the line and neutrals and embedded Latin words sit where macOS puts them. Laid out in reading order from the left, Arabic came out reversed and its joining forms fell apart (the same bug users report in Zed). Left-to-right runs in left-to-right lines take the unchanged path. |
| 0039 ios-font-traits | New | `gpui_apple/text_system.rs` | iOS leaves trait numbers out of some faces (Helvetica and Arial lack them, some CJK fallback faces lack the slant). font-kit's `properties()` unwraps them, so those families failed to load and `is_italic` panicked during layout. Read the italic bit and the traits with defaults (`font_properties`). macOS faces report every trait; their result is unchanged. See docs/design/mobile-spike.md. |
| 0040 ios-key-events | New | `gpui_ios/text_input.rs`, `gpui_ios/window.rs`, `gpui_ios/Cargo.toml` | gpui_ios sent no key events. `pressesBegan` turns hardware keys GPUI binds (⌘/⌃ combinations, Return, Delete, Tab, Esc, arrows, Home/End, Page Up/Down) into KeyDown/KeyUp, leaving plain typing and composition to UIKit. The soft keyboard's Return (outside composition) and Delete at the start of the editable range become `enter` and `backspace`, so an editor whose input handler covers one block can split and merge blocks. |
| 0041 ios-scene-window | New | `gpui_ios/window.rs` | Let the scene size its window instead of setting the screen's bounds. iPadOS 26 runs apps in resizable windows; with a screen-sized frame the app drew past its window and ignored rotation and resizing. |
| 0045 windows-render-to-image | New | `gpui_windows/window.rs`, `gpui_windows/directx_renderer.rs` | Offscreen `render_to_image` is no longer limited to `test-support` builds, as on macOS. The release self-test (`src/selftest.rs`) checks that the window draws and saves a PNG of a failing scenario. |
| 0046 shadow-spread-corners | New | `gpui/window.rs` | GPUI kept a drop shadow's corner radii when the spread grew it, so a ring drawn by a spread shadow (a field's 3pt focus halo) was thicker at the corners than along the sides. Grow the radii with the spread as CSS does: the radius plus the spread, easing to sharp for a sharp corner and shrinking to zero for a negative spread. |
| 0047 continuous-corners | New | `gpui_apple/shaders.metal` | Experiment (2026-11-23, not on main): draw every rounded corner as a continuous corner, as Apple does. A corner of radius r starts 1.528 r from the corner instead of r and is a superellipse in that box, its exponent chosen so the curve crosses the diagonal where the circle of radius r does; curvature eases in from the straight sides. When the box must shrink to fit (a pill) the exponent falls back to 2, the circle. The p-norm is divided by its gradient's length so antialiasing stays one pixel wide. Metal only; the Windows and wgpu shaders still draw circles. |
| 0060 ios-image-tile-edge | New | `gpui_apple/shaders.metal` | The polychrome sprite fragment clamps its atlas sample to half a texel inside the sprite's tile. `img()` rasterizes an SVG at 2x, so on a 3x screen (iPhone) it is drawn larger than its pixels: the edge pixels' samples fell less than half a texel inside the tile and the linear filter mixed in the neighbouring tile's edge, a grey 1px line down the left side of 收藏's empty-state picture. Images at 1:1 or smaller already sample at least half a texel in, so the desktop's 2x SVGs and photos are unchanged. Patches 0060–0079 belong to the mobile line (feat/mobile). |
| 0080 reuse-coretext-fonts | New | `gpui_apple/text_system.rs` | `layout_line` made a new CTFont for every run of every line, and CoreText fell back to new CJK faces for every line too; CoreText keeps a font's shaping tables, ASCII advances and fallback faces on the CTFont, so each line set them all up again. Each font is now made once per size, and the face CoreText fell back to for a CJK character (Han, kana, CJK and full-width punctuation) is kept per font, size and character and set on that character in later lines. It is CoreText's own face, so weight and variation stay as CoreText chose them. About a third of the time laying out new text (docs/PERFORMANCE.md, "Dragging the scrollbar"); glyphs, positions and faces are unchanged (`kept_cjk_fallbacks_shape_like_coretext`). |
| 0081 list-set-overdraw | New | `gpui/elements/list.rs` | `ListState::set_overdraw` changes after construction how much is laid out beyond the visible area. The editor sets none while its scrollbar thumb is held, where every frame lands somewhere new and rows nobody sees were most of the frame, and puts it back when the thumb is let go. Lists that never call it (iOS included) behave as before. |
| 0085 windows-font-family-names | New | `gpui_windows/direct_write.rs` | A DirectWrite font set matches a family by its name in the system's language only: on a Chinese Windows "Microsoft YaHei" and "SimSun" found nothing (微软雅黑 and 宋体 did), so a document font saved on a Mac or an English system, or a family named in code, quietly fell back to the default face. When a name finds nothing, `matching_fonts` tries the family's names in its other languages: the collection's `FindFamilyName`, else every family's names (`other_names`, unit-tested). Font fallback lists match the same way. Numbered 0085: 0048–0084 are taken by other branches. |
| 0086 web-canvas-viewport-scale | New | `gpui_web/window.rs` | Derive layout from CSS-pixel bounds and use the browser's effective DPR for the backing store when ResizeObserver reports device pixels at an inconsistent scale. |

## Retired patches

| Legacy # | Original change | Upstream behavior or replacement |
|---|---|---|
| 4 | Chinese punctuation wrapping | UAX #14 already keeps closing punctuation off line starts and opening punctuation off line ends. See `test_break_indices_quiet_md`. |
| 9 | Disable mouseDownCanMoveWindow and defer start_window_move | Main and quick-note windows enable upstream `WindowOptions::app_owns_titlebar_drag` and use `Window::start_window_move`. AppKit leaves dragging and double-click handling to the app. |
| 11 | Public DispatchEventResult | Public upstream. |
| 13 | Include aligned glyphs in text layer bounds | Upstream computes layer bounds from actual drawn primitives. |
| 19 | On-demand, batch-sized path textures | Upstream draws analytic tiles directly into the target without intermediate or MSAA textures. Apple GPU depth textures use Memoryless storage. |
| 21 | Remove tile index before releasing image | The generic atlas removes from tiles_by_key before release. |
| 29 | Clip shadows using blur extent | Upstream shadow bounds include three blur radii. |
| 31 | Treat Hebrew and Arabic letters as word characters | UAX #14 classifies these letters as AL and preserves words. |
| 33 | Limit CAMetalLayer to two drawables | Upstream CAMetalDisplayLink supplies drawables ahead of time. Two may drop frames. Scripted memory was ~95 MB both before and after. Measure real display frame rates before pursuing the 16 MB saving. |
| 36 | PixelFrame and Window::paint_pixels wrote CPU frames directly to an atlas tile | Board vello frames now use ordinary RenderImages, replacing each changed frame and calling drop_image on the previous image on the next display (src/board/render_vello.rs). |
| 39 | Compare fonts when merging runs in layout_line | Upstream compares fonts. |
| 40 | Write Metal path vertices directly into the instance buffer | Upstream tiled drawing eliminates that step. |
| 41 | Always listen for release on elements with an active style | Upstream registers unconditionally. |
| 42 | Register rightMouseDragged: and otherMouseDragged: | Registered upstream. |
| Windows VK_PROCESSKEY | Pass keys under composition to IME | Upstream cannot parse these as GPUI keys and passes them to TranslateMessage and IME. Check Backspace and Enter with Weasel on Windows for duplicate handling. |

## Changes retained from upstream

- **Pixel alignment.** Upstream aligns absolute taffy layout bounds, borders and clipping to device pixels. Glyphs use whole-pixel vertical positions and nearest quarter-pixel horizontal positions (previously rounded down). Text and icon edges change slightly. A block with fractional device-pixel height, such as a paragraph with inline math, can shift subsequent content by one device pixel.
- **Stationary hover.** Upstream recomputes hover when content changes. A search button covered by the search overlay now loses its background and tooltip immediately, previously only on mouse movement.
- **Uniform list padding.** Version 0.2.2 subtracted padding from the content area and then counted it again in child placement and content height. To preserve positions after the upstream fix, Home grid padding changed from pt(14)/pb(16) to pt(28)/pb(32), and scrollable contents lists from pb(22) to pb(44). A list that fits no longer scrolls an extra 22 pt.
- **Window origin.** A half-point origin creates a window one point taller upstream. Round main and quick-note window origins to whole points.

## Validation

After each upstream upgrade, use temporary libraries (`QUIET_MD_HOME`), simulated clipboard and file dialogs, and leave keyboard focus with the user.

```sh
cargo build --release --features devtools  # Build the reference binary separately as described in tools/harness/README.md
FDL_BIN=target/release/quiet_md FDL_REF_BIN=<old-binary> python3 tools/harness/gpui_patches.py
for f in table_paste table_resize clipboard_images checkbox_toggle scroll_intent pointer_properties; do
  FDL_BIN=target/release/quiet_md python3 tools/harness/$f.py; done
scripts/check.sh
```

- `gpui_patches.py` runs scenarios for patch behavior on each binary. Each document text row's four edges must match within three device pixels and ink coverage (sum of darkness) within 4%. These tolerances allow subpixel changes while detecting wrapping, alignment, line-height and scroll changes. Absolute checks include slanted italics, 85/300/150 column widths, a draggable scrollbar, visible images, working custom shortcuts, saving text typed before ⌘Q and a deferred popup opened inside another (the board's canvas background picker in its main menu, #327: GPUI 0.2 aborted on nested deferred draws). RSS is sampled every 0.1 s.
- The six other harness scripts exercise functional behavior without a reference build.
- Pixel comparisons (`compare.py`) and menu/shadow checks for legacy 13 and 29 used a temporary inspection script during the upgrade. It covered documents, menus, slash menus, search, Home, settings, scrolling, narrow windows and dark mode with Chinese. The script was not retained.
- GPUI unit tests such as `test_break_indices_quiet_md` depend on fonts, example images and util_macros in the Zed repository, so they cannot build in the vendored copy. To run macOS pasteboard tests, copy vendor/zed/crates elsewhere, append an empty [workspace] to gpui_macos/Cargo.toml and run `cargo test --features test-support --lib pasteboard`.
- Cross-check Windows from macOS with `cargo check --target x86_64-pc-windows-msvc` and a stub rc.exe/llvm-rc that writes empty output. Linux uses `--target x86_64-unknown-linux-gnu`, a C cross-compiler wrapper and `RUST_FONTCONFIG_DLOPEN=on`.

### Results on 2026-11-15 (macOS arm64, reference main a15b5a7)

Of 40 checks, 28 passed on macOS (including the macOS portion of 0028), two were partial (0011 macOS and legacy 9), three needed manual observation (0008, 0012, 0015), five awaited sida-windows (0011 Windows, 0023–0025 and VK_PROCESSKEY), and two compiled but awaited CI (0026–0027). No failures were recorded. Old/new below are measurements. Scenario names refer to gpui_patches.py.

| Item | Risk | Check | Result |
|---|---|---|---|
| 0001 glyph-bounds-at-size | Clipped left edge on large headings | title with overhanging Wj, fjord in title/H1/H2 at widths 1280/1000/760 | Passed at all three widths |
| 0002 katex-fonts | Incorrect fallback for script, blackboard bold and large brackets | katex with mathcal, mathscr, mathbb, mathfrak, left/right brackets and Bigg braces at three widths | Passed |
| 0003 fallback-font-identity | Wrong Chinese glyphs | cjk at three widths and editor::text::tests::cjk_fallback_glyphs in check.sh | Passed |
| 0004 synthetic-italic | Upright Chinese italics | italic compares upper ink shifted right against upright text at three widths | Both slanted and matched layout |
| 0005 grid-col-widths | Saved widths ignored | table measures rules at 85/300/150 and three widths | Within ±2 pt and matched reference |
| 0006 paint-rows | Inline math overlaps other lines | rows with fractions and radicals in a long paragraph at three widths | Passed |
| 0007 list-scroll-estimates | Incorrect or jumping scrollbar | scrollbar: 400 variable paragraphs, scroll then drag from y=68 down 280 pt | Both had (52,32) → (340,32) → (344,32) after scrolling and matching page content |
| 0008 path-prompt-extensions | Import lists all files | Scripts use pick and cannot open real panels. Code passes extensions and sets allowedContentTypes. | Unverified, inspect Import in preview manually |
| 0009 offscreen-capture | Capture fails while locked or obscured | All scenarios capture offscreen or obscured via render_offscreen | Hundreds of captures passed |
| 0010 replace-root-type | Post-onboarding window missing for quit saves and Finder opens | first-launch with empty library, QUIET_MD_ONBOARDING, eight Enters, create/type/immediate ⌘Q. app::quit uses downcast to Workspace. | Both reached workspace and saved |
| 0011 window-min-size (macOS) | Minimum width ignores sidebar changes | Accessibility returned -25205 for GPUI window size and lldb attachment required authorization. Exercised sidebar toggle path. | Partial: minimum size unmeasured |
| 0011 window-min-size (Windows) | Same issue in WM_GETMINMAXINFO | — | Awaiting sida-windows |
| 0012 present-in-transaction | Sidebar glass flickers one frame ahead or behind | Scripted toggles ran successfully. Offscreen capture cannot measure presentation. | Unverified, observe or record toggles and resizing on screen |
| 0013 hollow-images | Uploaded CPU pixels retained | images scrolls down and back past 30 images at 2400×1600, samples RSS and verifies display | Passed over three runs: new/old peaks 366–378 / 365–377 MB, settled 300–306 / 299 MB |
| 0014 load-visible-only | Decode images clipped from home cards | home-images scrolls to bottom of 12 documents with four large images each, only two visible per card | Passed, visuals matched. New/old peaks 135–145 / 142–150 MB, settled 129–130 / 134–135 MB |
| 0015 hide-cursor | Pointer remains visible while typing | Offscreen cursor cannot be observed and CGCursorIsVisible is unavailable/unreliable. Scripted typing succeeded. | Unverified, observe real-window typing |
| 0016 font-smoothing | Dark text thinner and light text paler | cjk and cjk-dark compare ink in light and dark themes | Passed, body row ink difference ≤0.5% |
| 0017 fallback-by-name | Chinese about 2% lighter | cjk compares row ink with 4% tolerance | Passed, measured body difference ≤0.5% |
| 0018 clipboard-text-and-image | Screenshot with text pastes only text | gpui_macos pasteboard::tests uses a private pasteboard (nine tests, three patch-specific). clipboard_images.py covers only the app's simulated side. | Passed 9/9 |
| 0019 korean-word-wrap | Breaks between Hangul syllables | korean at three widths against reference word wrapping | Passed, upstream test unavailable in vendor copy |
| 0020 rtl-wrap | Reversed Arabic line order and split words | rtl with long Arabic/Hebrew paragraphs at three widths | Passed |
| 0021 rtl-align | Left-aligned multiline RTL excerpts | home-rtl with Arabic/Hebrew home card excerpts | Passed |
| 0022 single-axis-scroll | Vertical scroll with drift moves wide table horizontally | wide-table, ten 220 pt columns, vertical scroll with 4 pt horizontal delta followed by horizontal ±300/120 | Passed, page and table scrolled independently, three screenshots matched |
| 0023 windows-clipboard | BMP output, V4/V5 misalignment, copied images fail to paste | — | Awaiting sida-windows |
| 0024 windows-device-lost | Panic after driver restart or wake | — | Awaiting sida-windows |
| 0025 windows-shader-path | Debug build crashes on Chinese paths | — | Awaiting sida-windows |
| 0026 linux-appearance-reborrow | Panic on appearance change | Cross-platform clippy from macOS | Compiled, awaiting CI |
| 0027 wgpu-30-crates-io | Cannot compile against crates.io wgpu | Same cross-check | Compiled, awaiting CI |
| 0028 keybinding-load-unmapped | Layout remaps user shortcuts or Windows/Linux keys | shortcuts sets bold to cmd-shift-k in .quiet/shortcuts.json, selects all, invokes it and checks saved `<b>` markup | Both macOS builds passed, Windows awaiting sida-windows |
| Legacy 4, Chinese punctuation | Closing punctuation starts a line | kinsoku compares dense Chinese punctuation at three widths | Passed |
| Legacy 31, RTL words | Arabic/Hebrew break midword | rtl | Passed |
| Legacy 9, title-bar drag | Buttons miss clicks or dragging/double-clicking fails | Scripted search, More, sidebar and Home clicks succeeded. Real pointer needed for drag and double-click zoom. | Buttons passed, drag and double-click unverified |
| Legacy 11, public DispatchEventResult | Build failure | Compile | Passed |
| Legacy 13, text layer bounds | Missing or clipped glyphs | Layouts matched across text scenarios and upgrade screenshots | Passed |
| Legacy 19, path textures | Large textures allocated for rounded cards or checkboxes | Home, table and checkbox captures matched (home-images, checkbox_toggle). Memory in 0013/0014. | Passed |
| Legacy 21, atlas removal | Crash on tile deletion | images scrolls past 30 images and back to discard and reupload hollow images | Passed, no crashes and images displayed |
| Legacy 29, shadow clipping | Shadows cropped at edges | Checked menus, slash menus and popups along edges during upgrade | Passed, matched reference |
| Legacy 33, two drawables | Extra drawable costs 16 MB at 2560×1600 | Patch omitted. Both versions alternated between two and three drawables, e.g. 110/126 MB for an image-free document. | Passed, distributions matched |
| Windows VK_PROCESSKEY | IME handles Backspace/Enter twice | — | Awaiting sida-windows with Weasel |
| uniform_list padding | Missing 16/22 pt at bottom of home grid/long contents | home-images scrolled to bottom and catalogue with 80 headings scrolled to bottom | Passed after adjustment |

## Upstream crates omitted

None of the platform backends are omitted. gpui_web serves the website's live notebooks (see [Web build](../web/README.md)). gpui_ios, added on 2026-11-22 at the same snapshot for the iOS/iPadOS work in [docs/design/mobile.md](../docs/design/mobile.md), builds only for `target_os = "ios"`; desktop and web builds do not compile it.

## Other upstream capabilities and measurements

- AccessKit platform adapters are wired up. Exposing elements to screen readers requires roles such as `.id(...).role(...)` or custom `a11y_role` implementations (gpui/src/_accessibility.rs). The app did not yet annotate elements at the time of the upgrade.
- Clean macOS arm64 builds on 2026-11-15: release build time 66.8 → 45.3 s and binary size 36.6 → 35.1 MB. Dist with LTO took 210 → 161 s and shrank from 24.97 to 24.27 MB. Scripted memory usage stayed around 95 MB.

## Removed vendor directories

- `vendor/taffy` patched the beyond-limits stage in 0.9.0 with a fix backported from 0.13. Upstream GPUI uses taffy 0.15, which includes the fix.
- `vendor/xattr` patched Linux ENOATTR. The new dependency tree no longer includes xattr.
