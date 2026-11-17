# Shared-note sessions in one macOS process

Shared documents open in the current application window. Navigation replaces
the workspace with one bound to the selected room's immutable store; it never
changes a store's root. Returning to personal documents restores the personal
home state, sidebar layout and valid editor selection. The app does not launch
itself with command-line arguments or create another user-facing window.

## Ownership

The startup-selected library remains the personal library and the source of
application preferences, themes and shortcuts. It is not switched when a shared
document becomes active. Each `Workspace` owns a `SharedStore` with an immutable
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

## Sharing originals and organizing folders

Sharing an existing note keeps its ID and its position in My Documents. Both
entries open the same room. Stopping sharing copies the latest collaboration
back into the personal library under the original ID. Existing copy-based shares
retain their existing removal and recovery behavior.

Personal folders created from Shared Documents organize the current account's
shortcuts. A private catalogue map syncs folder titles and placements across
that account's devices. Creating a personal subfolder or filing a shortcut there
does not grant access.

Choosing Share Folder on a library folder grants access to its subtree, including
future documents. Each folder invitation has a dedicated zone with the prefix
`QuietSharedFolder-v1-`. The shared catalogue omits the private parent and
unrelated siblings. Moving a private document into the folder requires confirming
the expanded audience. Read-only members can open nested contents and editors
can add documents and folders.

## Lifetime and failures

Leaving the last workspace for a shared store releases its session and lock after
the workspace is dropped. The sync loop also removes unused sessions. Saved edits
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

SQLite schema 7, cloud journal protocol 4 and backup format 2 are retained.
Folder zones are distinct from older single-note zones.
Existing shared-note descriptors, IDs, attachments and pending edits are reused.
The old explicit shared-room startup path and devtools-only window helper remain
for diagnostic commands; user-facing navigation does not use them.

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

## Contextual sharing interface

The macOS cloud build exposes Shared Documents as a collapsible sidebar section
above My Documents and a tab on Home. The section starts collapsed and lists each
room and its unread state when expanded. Both sidebar headers use the same batch
and plus buttons. The shared plus menu offers New Personal Folder and Accept
Invitation. The invitation opens the same form as the Home header button. Home reuses the My
Documents list renderer, including document icons, time columns, row sizing and
ellipsis controls. Shared sorting is independent and never changes personal
manual order. The contextual menu provides members, deletion or leaving. Account and synchronization
controls remain in Documents settings.

Personal note toolbar and More-menu actions open the same creation form. It
shares the selected original while preserving its personal catalogue position.
Static-copy delivery uses the existing Markdown export path. A locally staged
room is retained before initial publication, so a failed upload retries the same
identity. After publication, the current window navigates to the shared room and
owns the invitation form. A failed invitation retains its
recipient and permissions there; it never recreates the note. Adding permission
does not send an email or message: the owner must send the invitation URL.

The shared-note toolbar keeps a collaboration entry visible while access is
being checked or paused. Its small popover shows access, actual sync status and
active editors. Owners open Invite Participants or Members and Access from
there. Contextual member management also opens from the shared list. Member
rows expand to permission and removal actions; removal requires a second,
explicit confirmation. Only owners manage membership in this first stage.
Missing CloudKit names fall back to numbered participants, without exposing an
opaque participant identifier. Reader/writer inspection of full membership,
automatic invitation notifications and an activity timeline remain separate
features.

Owners can delete a shared document; participants can leave it. These actions
are available in the Home list and collaboration panel, with owner deletion
also in Members and Access. Confirmation states the effect on other users.
Before remote removal, cached content and attachments are saved as an independent
copy in the personal Trash. Failure to save recovery blocks the remote action;
remote failure retains the room and supports retry without duplicate recovery
copies. Successful removal hides the room and retires local access while retaining
its cache and durable queues. Rejoining moves that retired cache aside and starts
a fresh session, preventing old queued edits from being uploaded into the rejoined
share. No cloud subscription is required to remove existing access.

Forms retain inline failures and block duplicate requests. Asynchronous results
are scoped to their form epoch, and live share responses check the CloudKit
account revision. The cloud protocol, SQLite schema, existing room identities,
attachments and pending queues are unchanged.

### Offline interface validation

`python3 tools/harness/sharing_ui.py` uses fresh synthetic session libraries and
an explicit `QUIET_SYNC_QA_SHARING_UI` transport fixture. That fixture exists only
in devtools builds, requires the offscreen script driver, a validated temporary
QA root, prepared session descriptors and the cloud-pause marker. It does not
contact CloudKit. It covers initial publication failure, retrying the same copy,
invitation failure and retry in the current window, member permission changes,
confirmed revocation, invitation acceptance with read-only access, preservation
of the personal original, collapsible navigation, same-window opening, leaving,
owner deletion, recoverable deletion failure, and narrow/dark management presentation. Screenshots
and logs are written to its temporary artifact directory. This is not a new
real-account or signed-distribution acceptance test.

### Shared / personal interface parity — 2026-11-17

The separate shared-session sidebar renderer has been removed. The ordinary
sidebar renders personal navigation in both contexts, including Pinned, section
folding, document menus, creation and batch entry points. Personal actions resolve
the personal workspace before using its store; shared actions resolve their room.
Only navigation / animation state moves across the same-window replacement.
Neither store nor document entries are temporarily exchanged between libraries.

| Surface / operation | Shared document behavior |
| --- | --- |
| Sidebar rows | Same spring / lift / landing / removal renderer, title field, ellipsis and selection controls; separate manual order |
| Home toolbar | Same batch, sort and grid/list controls; same cards, list rows and menu primitives |
| Pinned and Favorites | Include both document kinds in either context; shared organization is personal-local SQLite state, never shared metadata |
| Search and recent search | Personal entries plus all active cached shared entries, correct per-result labels and same-window routing; complete cached body instead of the 160-character list summary |
| New window | Explicit menu action preserves room isolation and read-only access |
| Duplicate | Shared sources are copied into My Documents with fresh document IDs and attachment paths; source and membership remain unchanged |
| Rename | Owner / editable participants only; shared sidebar displays the same title field and updates the activity model immediately |
| Download | Existing Markdown and PDF flows after resolving the source room |
| Bulk selection | Same checkboxes, select-all, count, exit and action bars; mixed Favorites dispatch to each source library |
| Remove | Confirmation distinguishes personal Trash, owner deletion for everyone, and participant leave; cached shared recovery remains in personal Trash |
| Failure / retry | Keep failed shared selections and retry only failed rooms; a removed current room returns to a personal context |
| Move / file path | Shared documents have no personal folder or personal file path; these personal-only actions are omitted |

These organization and lifecycle rules were explicitly approved by the user.
The shared-copy creation mechanism, region-preserving branded invitation links,
CloudKit permission model, original-document cleanup confirmation and member
management are retained. UI consistency does not convert a shared note into a
personal note or change another participant's permissions.

`tools/harness/sharing_parity.py` uses only fresh temporary libraries and the
existing offline transport fixture. It exercises actual pinned clicks from both
contexts, cross-library full-body search and result opening, list/grid switching,
sidebar ellipsis, shared rename, read-only rejection, explicit new-window access,
local organization, role-aware bulk cancellation, copying into personal storage,
and partial removal failure followed by retry without duplicate recovery copies.
The existing sharing harness continues to cover creation, invitation, permissions,
revocation, dragging, recovery and collapsed-folder preservation. These checks do
not replace real two-Apple-Account CloudKit collaboration acceptance or mobile UI
implementation.
