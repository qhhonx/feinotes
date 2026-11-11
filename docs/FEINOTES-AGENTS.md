# Working on Fei Notes

Read the upstream conventions in ../AGENTS.md and ../CONTRIBUTING.md first, then ../README.md, FEINOTES-CONTRIBUTING.md and MAINTENANCE.md. This document contains Fei Notes-specific additions. The upstream text remains in the root AGENTS.md and CLAUDE.md with only a link to this supplement.

## Relationship to upstream

Common editor, style, testing and user-data rules remain governed by the upstream conventions. For the Fei Notes product only, this supplement defines the product name, cloud library, branded icon provenance and Apple release process. In particular, use Lucide or original shell icons for Fei Notes instead of the upstream Nucleo recommendation. The branded app version is independent of the unchanged upstream Cargo versions. Upstream packaging instructions describe Quiet.md; use APPLE-RELEASE.md for Fei Notes. Theme extensions are documented in FEINOTES-THEMES.md. See UPSTREAM.md for the original revision and documentation boundaries.

## Product and scope

Fei Notes is a native macOS Markdown notes app maintained by the Fei Notes contributors. Optional iCloud sync and collaboration use CloudKit and Loro. Source is public and self-builds are supported under the applicable licenses; preserve the upstream MIT permissions and the reserved commercial rights of the designated Fei Notes modules.

Preserve the existing editor layout, keyboard shortcuts and interaction patterns. Use the existing Theme tokens, popover containers and controls. The Notes theme changes appearance through the theme system. Chinese and English are supported UI languages. Write new developer documentation and comments in English.

## Architecture and editing

- Keep the document engine in `crates/core` (`fdl_core`), independent of GPUI. Put application UI in the root crate.
- Change document structure through `editor::Core`, using checkpoint, the Core operation and `Editor::changed()`. Add structural operations to the existing random-edit driver in `fuzz_tests.rs`.
- Preserve stable document, block, row and column identities during sync. Keep changes to the materialized document, CRDT state and durable queues consistent within their SQLite transaction.
- Preserve unknown or unsupported rich content during import, sync and recovery. Keep `crates/core/tests/fixtures/all-blocks.xml` round-tripping byte for byte.
- Respect the cloud protocol and SQLite schema versions. Add defaults to new serialized metadata fields. Include a migration whenever an existing library needs one.
- Follow the compact style in surrounding Rust code. Do not run repository-wide rustfmt.
- Use Theme semantic colors. Choose Lucide or original artwork for new shell icons, record provenance and update THIRD_PARTY.md.
- Record modifications to vendored dependencies in vendor/PATCHES.md.

## User data and credentials

- Use fresh temporary libraries for tests. `QUIET_MD_HOME` selects an upstream-compatible test library. Cloud builds use their startup-selected SQLite library and the documented isolated QA mode.
- The default upstream library `~/Documents/Quiet.md` and Fei Notes application containers can contain real notes. Do not modify, delete, stage or upload their contents during development.
- Stop only processes started for this checkout's test run. Use isolated test accounts and synthetic attachments.
- Keep Apple keys, signing identities, provisioning profiles, account contact details, tokens and logs containing personal data out of Git. Release credentials belong in the protected GitHub environments.
- Public website copy, metadata and assets must use the Fei Notes product identity and its support address. Do not publish the maintainer’s personal name, address, phone number, private email or account details on the website. Keep this rule when editing support and privacy pages.
- Retain pending edits on sync failure, cloud-access changes and account changes. Download progress is a cache and must be recoverable.

## Checks

Run `scripts/check.sh` before committing. For cloud changes, also run `scripts/check-cloud-sync.sh`. For editor structure changes, use `FDL_FUZZ_SEEDS=2000 cargo test --release random_edits`.

For a visual-preserving refactor, compare the before and after UI with the existing harness. Its rules are in tools/harness/README.md. Use its synthetic clipboard and file picker, keep the window offscreen and avoid external links, Finder, printing and Quick Look.

## Delivery and upstream

Create commits and push when the user requests them. Keep logical changes reviewable. Preserve the Quiet.md attribution and all applicable licenses. Follow MAINTENANCE.md for upstream integrations and APPLE-RELEASE.md for distribution.

Run pull-request checks without release credentials. TestFlight distribution may run on the protected product branch. Store submission and publication have separate release gates. Verify the exact uploaded source revision, version and build number before distributing. Distribution packages include arm64 and x86_64 slices for both the executable and Swift bridge. Intel has no hardware validation; report compilation and package checks separately from runtime testing.
