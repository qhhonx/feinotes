# Upstream documentation boundaries

Fei Notes is a fork of [Quiet.md](https://github.com/s1dashu/quiet-md). The published product starts from upstream commit `57884b9c8e49d22effe18885c6d717d8acb67278`.

## Preserved conventions

- Root AGENTS.md, CLAUDE.md and CONTRIBUTING.md retain the complete upstream text with only an appended Fei Notes reference.
- docs/THEMES.md retains the complete upstream guide and links to the separate Fei Notes extension reference.
- README.ja.md, README.es.md, README.fr.md and README.ko.md retain the complete upstream READMEs with a notice identifying its scope and upstream download links.
- LICENSE remains byte-for-byte unchanged from this baseline. docs/FORMAT.md, docs/ARCHITECTURE.md and vendor/PATCHES.md track the upstream updates recorded below.
- tools/harness/README.md keeps the upstream guide and adds the implemented product test notes.

## Product-specific documentation

- README.md and README.zh-CN.md introduce Fei Notes, as the product entry points. The upstream versions remain available at the immutable links below.
- FEINOTES-AGENTS.md and FEINOTES-CONTRIBUTING.md add product rules without replacing the upstream conventions.
- FEINOTES-THEMES.md documents the implemented product extensions; MAINTENANCE.md and APPLE-RELEASE.md describe product integration and distribution.
- THIRD_PARTY.md retains the upstream dependency inventory and updates asset notices to describe the actual Fei Notes bundle. Do not restore removed asset attributions as if those assets were still shipped. LICENSES.md maps the applicable file-level licenses.

Keep product additions separate when importing upstream changes. Preserve original notices and documentation unless an implemented source change requires an accurate update. Record those updates and their scope instead of replacing unrelated upstream content.

## Imported upstream updates

- 2026-11-19 independent candidate, through `86e575c3d088622c7f91f79564705dba6804a74e`: adds 22 upstream commits after `159e3d4`, including native clipboard image-file references, hidden attachment lookup, external Markdown editing, localized untitled filenames, last-read startup preference, menu toggles/translation widths, window placement and import cost. The candidate also includes the committed product recovery through `7e174eb`. File-only refresh and last-save behavior stay outside SQLite; SQLite retains revision checks, cloud transactions and shared permissions. External Markdown refresh waits for IME composition and stays within the selected library. Pasted images use the editor-selected asset root. GPUI clipboard patch 0035 coexists with the renumbered Fei Notes universal-link patch 0036. The prior local-history backup/restore path is unchanged. This entry describes a candidate only; product fast-forward still requires the user's approval of the tested revision.

- 2026-11-18 integration candidate, through `159e3d491760a5160980c059979010005a05e8ee`: incorporates 20 pending commits, including preservation of unknown file-index kinds, table vertical navigation, GPUI's Zed snapshot/patch layout, browser demo support, Linux emoji and board buffer/threshold improvements. Product startup, drafts, SQLite schema 7, Loro 1.17.1, sharing, selected-library assets, themes and website/release boundaries are retained. Universal-link invitations are ported to `gpui_macos` in patch 0035. The pinned mobile GPUI keeps explicit element conversions and its existing board path renderer; it does not adopt the desktop snapshot. Browser sources stay in the upstream `site/` and `web/` paths, separate from the product `website/`. This entry records the independent candidate; advancing `product/quiet-sync` still requires the maintenance guards and a clean product workspace.

  The candidate passes both repository check scripts and 2000 random-edit seeds. Isolated native regressions cover board persistence, IME/history, sharing invitations/permissions, table paste/import and scroll/selection intent. Browser and iOS simulator Rust targets compile; browser runtime, mobile Swift/device acceptance and live multi-account CloudKit remain separate. The macOS dependency-notice generator succeeds with the snapshot Apache notice and exact-revision supplemental notices for newly selected dependencies. The subsequent candidate refresh incorporates the product's committed iPad navigation and TestFlight availability/notice updates through `7c9a81c`; it does not advance the product. The updated maintenance process requires a separate non-TestFlight local experience app and explicit approval of the tested candidate before a fast-forward product update.

- 2026-11-17, through `e9cb5408d0ffcad5bf5b31814e10e1b2f275347f`: imported 18 pending commits, including Excalidraw boards, owned copies of pasted web images, sidebar row/menu fixes and plural forms. Boards remain DocxXML block payloads in SQLite and the existing cloud protocol, while file libraries retain standalone `.excalidraw` files. Board creation, asset enumeration/import, version recovery and backups use their selected backend and library root. Shared read-only/history previews cannot open an editable board; permission loss or an incoming document discards the stale board editor. Product drafts, shared navigation, focused sidebar colors, branding and licensing remain intact. The shell board icon uses Lucide rather than the upstream Nucleo asset; Excalidraw MIT and font notices are preserved.

- 2026-11-12, through `a6a188bd315076fff83dca8f027fdbc67a72eab6`: imported upstream clipboard, table, creation-menu and rendering changes. `vendor/PATCHES.md` includes upstream's macOS clipboard patch record.
- 2026-11-13, through `7ead2356ef2c82609bdae9ad084b26dbd877e65d`: imported reading positions, literal paste, interface preferences and pointer/scroll fixes. `docs/ARCHITECTURE.md` now preserves upstream's distinction between caret reveal and viewport preservation. Fei Notes keeps its shared-note access checks, responsive settings rows, cloud/release panels and Notes theme tokens. Incoming cloud documents clear the local wrapped-row caret hint; the new base64-image test initializes the product's table identity extension through `Default`. Inline Markdown images create the assets directory on first use, including in a fresh library.

- 2026-11-15, through `a15b5a7f7432353baef11f73bcd498f359972717`: imported all 127 pending upstream commits, including FORMAT 4 real folders, local version history, onboarding, additional interface languages, Markdown imports/tables and image/GPU memory improvements. `docs/FORMAT.md` describes the upstream file backend; Fei Notes retains the separate SQLite backend, schema 7 and existing CloudKit/Loro protocol. File-library onboarding and migration never run for the account-selected product library. FORMAT 3/4 imports stage a copy, preserve the source and normalize attachments into the cloud `assets/` namespace. History restore uses the normal atomic cloud-save path and enforces shared-note access. Root conventions and non-product READMEs retain upstream text with scope notices; product themes, branding and license boundaries remain in place. See [FEINOTES-STORAGE.md](FEINOTES-STORAGE.md) for backend and history scope.

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
