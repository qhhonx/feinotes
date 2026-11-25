# Development guidelines for contributors and agents

Read README.md and docs/ARCHITECTURE.md first. Keep AGENTS.md and CLAUDE.md identical whenever you edit either file.

## Current focus: preparing the first open-source release

Maintain the code, bundled guides, documentation and assets in this repository. The implementation and its `Theme` tokens are the reference for visual details. Resolve discrepancies in favor of the code.

## Product principles

- Preserve existing elements, their order, behavior, shortcuts and wording. Use the existing semantic Theme tokens.
- The sidebar sits directly on the window background without a panel, border or shadow. The note is a flat white surface flush with the top, right and bottom edges of the window, without gutters, rounded corners or borders; its left edge casts one faint shadow (`theme::shadow_note`) to set it apart from the sidebar. This is the only structural exception.

## Code

- **Do not run rustfmt.** Match the deliberately compact style and comment density of the surrounding code.
- **The document engine lives in `crates/core` (`fdl_core`) and is independent of GPUI.** Keep the model, format, editing engine, library, import and Markdown export there. Core modules may reference only other core modules within the workspace. UI code belongs in the root crate. See docs/ARCHITECTURE.md.
- **Change document structure through `editor::Core`.** Call `checkpoint`, invoke the Core method, then call `Editor::changed()`. Add each new structural operation to the random choices in `step` in `fuzz_tests.rs`.
- Only blocks for which `BlockKind::nests()` is true and containers (quotes, callouts, column groups and columns) can have children. Code blocks contain neither styles nor inline objects.
- **Keep DocxXML output stable.** After reader or writer changes, `crates/core/tests/fixtures/all-blocks.xml` must still round-trip byte for byte. Add the canonical representation of each new block type to this fixture.
- Use `Theme::*` semantic color tokens first, then the `theme::craft` scale (e.g. `c(craft::N300)`). Use a hex value only for a measured color and cite its source. Use `theme::shadow_*` for shadows and `ui::popover` / `ui::menu_row` for menus.
- Quiet.md's design system is `crate::design` (`src/design/`): shadcn/ui with Nova's colours and Vega's geometry, popovers and cards edged the affectUI way (one 0.5pt ring and a soft drop: `shadow::popover` / `shadow::frame`, not a border plus a shadow), settled 2026-11-22 in docs/design/DESIGN_SYSTEM.md. Corners hang off one base radius, `design::R`; to round the look, move a component up a step (`R - 2.` → `R + 2.`), not `R`. New and migrated UI uses its components (`Button`, `Popover`, `MenuItem`, `Field`, `Switch`, `Dialog`… — docs/design/COMPONENTS.md) and tokens (`design::{color, chip, geo, shadow}`), not `Theme` / `craft` or `ui::popover` / `ui::menu_row`; express states with `selected`, `disabled`, `destructive` rather than restyling hover. A missing value comes from shadcn (Nova for colour, Vega for size), not invented; add new components to the 组件库 panel (`src/gallery/`, opened alone with `scripts/components.sh`) with at least one live instance, and check them there, light and dark. The whole app is moved over except the board's Excalidraw chrome and a few one-off pieces (COMPONENTS.md lists them); `theme.rs` stays for the document, the sidebar and list rows.
- Look for new icons in Nucleo UI Outline 18 first, to match the existing set. If none fits, draw one and mark it as project artwork in a comment at the top of the SVG. Update the list and counts in THIRD_PARTY.md when adding or removing icons. The Nucleo license allows up to 100 icons in an open-source project.
- The vendored Zed tree is a pinned upstream snapshot plus patches. After editing it, run the `save` command in `tools/vendor-sync` to record the patch and update vendor/PATCHES.md. Patches keep their numbers; a new one takes `--number` from your line's range (see vendor/PATCHES.md). A failed `check` indicates unrecorded changes.
- Increment `store::FORMAT` if older versions would misread a library layout change. Add `#[serde(default)]` to all new Meta fields.

## Before committing

```sh
scripts/check.sh        # release tests and clippy on all targets with zero warnings
```

After changes to the editing engine, run more random seeds with `FDL_FUZZ_SEEDS=2000 cargo test --release random_edits`.

For refactors that only move code, compare before-and-after screenshots with `tools/harness/compare.py`. They must match pixel for pixel. See tools/harness/README.md.

To check every commit automatically, run `git config core.hooksPath scripts/hooks`.

Update both Cargo.toml version numbers together when releasing. Create commits only when the user explicitly requests one.

## Protect user data

- Quiet.md runs as `quiet_md`. The default library at `~/Documents/Quiet.md` contains real user data. Never commit, delete or modify its files. Leave other applications' processes and data alone. When terminating a process, match only a binary built from this repository.
- Launch or update the local preview with `scripts/preview.sh`. Its bundle is `dist-preview/Quiet.md Preview.app` so users can distinguish it from the installed app. It also uses the default library. On first launch, `src/guides.rs` populates an empty library with the compiled-in Getting Started guides and preserves existing libraries.
- Generate `shared-mock/library` from `shared-mock/guides/src/*.xml` with `python3 shared-mock/guides/build.py <empty-directory>`. After guide changes, regenerate it and verify the round trip with `FDL_ROUNDTRIP_DIR=<directory> cargo test --release library_round_trips -- --ignored`. The Simplified Chinese guides (seeded when the interface is in Simplified Chinese) live in `shared-mock/guides/src/zh/` with the same file names and structure; build them into `shared-mock/library-zh` with `build.py --lang zh` (no `.assets`: they use the English library's images) and round-trip that directory too. Change both sets together. Document only implemented features and omit emoji icons from guide titles.
- After changing UI shown in the guides, retake the screenshots with `shared-mock/guides/shots/shots.sh`. A window floats in the center of the screen for about a minute without taking focus.
- Always use a temporary library for tests and set `QUIET_MD_HOME` to its path.

## UI scripts

See tools/harness/README.md for the full rules.

- Use the simulated clipboard. Send `key cmd-v` only after `clip`.
- Avoid `activate`, which takes focus.
- Do not click Show in Finder under Settings › Library, Open in Finder in the home or folder menu, links, Web Search, Print or Quick Look.
- Do not use the `open` command. Use `pick` for file selection.
- Leave no test app running. A script quits the app when it ends (`FDL_KEEP_OPEN` keeps it); after a run, check that none of your own `quiet_md` processes are left.

## Environment

- The shell is zsh. Quote glob arguments such as `'--include=*.rs'` because unmatched globs raise errors.
- `grep` resolves to ugrep.
- On macOS, use `sed -i ''` for in-place editing.
- The `timeout` command is unavailable.
