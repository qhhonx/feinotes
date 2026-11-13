# Upstream documentation boundaries

Fei Notes is a fork of [Quiet.md](https://github.com/s1dashu/quiet-md). The published product starts from upstream commit `57884b9c8e49d22effe18885c6d717d8acb67278`.

## Preserved conventions

- Root AGENTS.md, CLAUDE.md and CONTRIBUTING.md retain the complete upstream text with only an appended Fei Notes reference.
- docs/THEMES.md retains the complete upstream guide and links to the separate Fei Notes extension reference.
- README.ja.md retains the complete upstream README with a notice identifying its scope and upstream download links.
- LICENSE and docs/FORMAT.md remain byte-for-byte unchanged from this baseline. docs/ARCHITECTURE.md and vendor/PATCHES.md track the upstream updates recorded below.
- tools/harness/README.md keeps the upstream guide and adds one implemented test environment variable.

## Product-specific documentation

- README.md and README.zh-CN.md introduce Fei Notes, as the product entry points. The upstream versions remain available at the immutable links below.
- FEINOTES-AGENTS.md and FEINOTES-CONTRIBUTING.md add product rules without replacing the upstream conventions.
- FEINOTES-THEMES.md documents the implemented product extensions; MAINTENANCE.md and APPLE-RELEASE.md describe product integration and distribution.
- THIRD_PARTY.md retains the upstream dependency inventory and updates asset notices to describe the actual Fei Notes bundle. Do not restore removed asset attributions as if those assets were still shipped. LICENSES.md maps the applicable file-level licenses.

Keep product additions separate when importing upstream changes. Preserve original notices and documentation unless an implemented source change requires an accurate update. Record those updates and their scope instead of replacing unrelated upstream content.

## Imported upstream updates

- 2026-11-12, through `a6a188bd315076fff83dca8f027fdbc67a72eab6`: imported upstream clipboard, table, creation-menu and rendering changes. `vendor/PATCHES.md` includes upstream's macOS clipboard patch record.
- 2026-11-13, through `7ead2356ef2c82609bdae9ad084b26dbd877e65d`: imported reading positions, literal paste, interface preferences and pointer/scroll fixes. `docs/ARCHITECTURE.md` now preserves upstream's distinction between caret reveal and viewport preservation. Fei Notes keeps its shared-note access checks, responsive settings rows, cloud/release panels and Notes theme tokens. Incoming cloud documents clear the local wrapped-row caret hint; the new base64-image test initializes the product's table identity extension through `Default`. Inline Markdown images create the assets directory on first use, including in a fresh library.

## Immutable upstream references

- [AGENTS.md](https://github.com/s1dashu/quiet-md/blob/57884b9c8e49d22effe18885c6d717d8acb67278/AGENTS.md)
- [CLAUDE.md](https://github.com/s1dashu/quiet-md/blob/57884b9c8e49d22effe18885c6d717d8acb67278/CLAUDE.md)
- [CONTRIBUTING.md](https://github.com/s1dashu/quiet-md/blob/57884b9c8e49d22effe18885c6d717d8acb67278/CONTRIBUTING.md)
- [README.md](https://github.com/s1dashu/quiet-md/blob/57884b9c8e49d22effe18885c6d717d8acb67278/README.md)
- [README.zh-CN.md](https://github.com/s1dashu/quiet-md/blob/57884b9c8e49d22effe18885c6d717d8acb67278/README.zh-CN.md)
- [README.ja.md](https://github.com/s1dashu/quiet-md/blob/57884b9c8e49d22effe18885c6d717d8acb67278/README.ja.md)
- [LICENSE](https://github.com/s1dashu/quiet-md/blob/57884b9c8e49d22effe18885c6d717d8acb67278/LICENSE)
- [THIRD_PARTY.md](https://github.com/s1dashu/quiet-md/blob/57884b9c8e49d22effe18885c6d717d8acb67278/THIRD_PARTY.md)
- [docs/ARCHITECTURE.md](https://github.com/s1dashu/quiet-md/blob/57884b9c8e49d22effe18885c6d717d8acb67278/docs/ARCHITECTURE.md)
- [docs/FORMAT.md](https://github.com/s1dashu/quiet-md/blob/57884b9c8e49d22effe18885c6d717d8acb67278/docs/FORMAT.md)
- [docs/THEMES.md](https://github.com/s1dashu/quiet-md/blob/57884b9c8e49d22effe18885c6d717d8acb67278/docs/THEMES.md)
- [tools/harness/README.md](https://github.com/s1dashu/quiet-md/blob/57884b9c8e49d22effe18885c6d717d8acb67278/tools/harness/README.md)
- [vendor/PATCHES.md](https://github.com/s1dashu/quiet-md/blob/57884b9c8e49d22effe18885c6d717d8acb67278/vendor/PATCHES.md)
