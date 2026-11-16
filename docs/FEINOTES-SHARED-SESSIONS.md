# Shared-note sessions in one macOS process

Shared notes now open a window in the existing application process. Opening a
room that already has a window activates that window. The app does not launch
itself with command-line arguments, which a sandboxed Launch Services caller
cannot reliably forward.

## Ownership

The startup-selected library remains the personal library and the source of
application preferences, themes and shortcuts. It is not switched when a shared
window becomes active. Each `Workspace` owns a `SharedStore` with an immutable
library root and shared-library flag. Shared stores open the existing
`shared-notes/<note UUID>/library.sqlite3` directly.

The sync registry is keyed by library root. Each session owns its journal and
exclusive lock, store, retry channel, attachment retry flag and generation.
Opening a shared library claims that lock before opening or migrating SQLite;
the same file handle is then transferred to the journal without an unlock gap.
Status and presence views are also keyed by library root. Saved documents,
remote updates, cursor anchors, permissions, history, recovery and cross-window
mirrors use that same root. A document ID alone never selects a session.

An editor receives its workspace's root. Image resolution, attachments,
thumbnails, cover imports, pasted assets, clipboard Markdown and HTML/Markdown
exports use this root explicitly. Personal-library defaults remain available to
the upstream-compatible editor and application-level preferences.

## Lifetime and failures

Closing the last window for a shared store releases its session and lock after
the window is dropped. The sync loop also removes unused sessions. Saved edits
stay in the durable outbox. Reopening creates a new session generation; late
sync and presence results from an older generation are discarded.

Wake, network recovery and account-change signals fan out to the registered
sessions. Account changes protect the personal account's offline-verification
marker and cause every affected session to recheck the account. Shared windows
keep their permission and paused-state guards. Owner management validates the
room descriptor associated with the caller's store.

Ordinary file-open events target personal windows. Shared windows cannot create
unrelated documents, switch libraries, restore a personal backup, or bypass the
shared journal when a session is missing. Backup restoration still requires
shared windows to close; unused sessions are released before it is scheduled.

Unread state keeps its independent activity cursor, account-based author
filter, projection check and per-update read receipts. The next-frame receipt
also checks the same window, library, document view and displayed version.

## Compatibility

There is no database relocation, schema migration or CloudKit protocol change:
SQLite schema 7, cloud journal protocol 4 and backup format 2 remain unchanged.
Existing shared-note descriptors, IDs, attachments and pending edits are reused.
The old explicit shared-room startup path remains for existing diagnostic
commands; user-facing opening no longer calls it or creates another process.

## Verification

Run `scripts/check.sh` and `scripts/check-cloud-sync.sh`. The native regression
is separate because it needs a macOS window server:

```sh
cargo build --locked --features cloudkit-sync,devtools
FDL_CLOUD_BIN=/absolute/path/to/quiet_md \
FDL_CLOUD_DYLD_PATH=/absolute/path/to/matching/swift/bridge \
python3 tools/harness/shared_sessions.py
```

This test creates fresh synthetic libraries, pauses every cloud exchange, and
opens personal, owner and read-only windows in one process. It verifies window
reuse, editor and attachment roots, read-only save rejection, durable queue
isolation, unchanged personal content, lock release, reopen recovery, stale
session-generation rejection and immediate account-change fan-out to all
sessions. It captures all three windows offscreen without
changing the user's clipboard or activating the windows. The special fixture
startup and monitor bypass exist only in `devtools` and require the offscreen
harness plus a validated temporary QA root.

These local checks do not replace acceptance of a signed distribution build
with two real Apple accounts. The product integration also includes the Notes theme adaptation. Its focused
sidebar selection, toolbar grouping, compact outline and system Share symbol
retain these per-window library and permission guards.
