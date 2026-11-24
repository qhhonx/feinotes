# Fei Notes storage and the Quiet.md file library

Fei Notes keeps two storage implementations behind `Store`. The upstream-compatible build uses title-named files, real folders, `.quiet/` metadata and `.assets/` attachments. FORMAT 5 stores file-library metadata in per-device files. Eligible local FORMAT 4 libraries upgrade when opened. Synced folders keep FORMAT 4 under the upstream compatibility rules. `docs/FORMAT.md` documents the file backend and its migrations.

The macOS product (`cloudkit-sync`, including its local-only mode) continues to use the startup-selected SQLite library. Its schema stays at version 7, with stable block/table identities and the existing CloudKit/Loro protocol. No file-layout migration, file-library picker or upstream onboarding may replace this selected root. iCloud is synchronization on top of SQLite, not a third document backend.

## Library settings

Mac startup first checks the explicit local-only preference and any previously
verified cache bound to the current device's iCloud Drive identity. A cache is
opened immediately only when the selected and verified account markers agree,
the database and restore marker exist, and no account revalidation is pending.
CloudKit still verifies the account for every exchange. Drive identity alone
does not authorize a library or any cloud request.
Failed or timed-out background discovery uses the same cache checks; without
that confirmation it falls back to the local library and preserves old caches.

When unresolved, an interactive home opens before account discovery and restore
run in the background. Creating or opening a local note replaces that startup
view with the normal local workspace; a late discovery reply cannot replace it
or switch an active editor's library. This implicit local session does not set
the explicit local-only preference. A user-selected local-only preference is
retained without starting discovery. Diagnostic/shared-room command launches
keep their synchronous startup contract. No schema or cloud protocol changes.

Settings > Documents keeps upstream library rows together (location, Trash and local version-history retention). Fei Notes appends separate Import, iCloud Sync, and Backup and Restore groups through `product_library_groups`. Keep future product controls out of the upstream row list. Cloud library selection, sync status, sync issues and cloud-access controls are shown directly on this page; there is no separate iCloud settings page. Entering or reopening Documents refreshes cloud-access status, and purchase changes continue to refresh the visible controls.

## Import and recovery

FORMAT 3, 4 and 5 file libraries are imported through an isolated staged copy. The source is never rewritten. Existing document IDs are checked for duplicates. The file backend resolves paths and links, then the import rebases attachments into an import-specific `assets/legacy-*` directory. Imported live and trashed documents and their metadata commit together in SQLite. Symlinks and unsupported files are rejected.

The upstream version drawer works with both backends. File snapshots retain upstream raw text. SQLite snapshots serialize the prior committed document after the body and sync transaction succeeds; snapshots are ancillary local files under `.quiet/history/` and are not CloudKit records. SQLite remains the authoritative document store. Existing cloud recovery history and durable queues are preserved.

Personal Trash shows offline recovery copies for permanently deleted shared-folder documents when the local archive contains edits from this replica beyond the deletion's version. The original share keeps its permanent deletion and version archive. An owner can also recover pending edits after a standalone share ends. Participants' ended shares keep their queued edits locally. Saving a recovery verifies locally stored attachments, rebases their paths, and creates an independent personal document with a fresh ID. The selected personal iCloud account must match the source cache.

The personal journal creates `shared_recovery_resolutions` on open. The row recording a save or dismissal is local to that account's library, and saving commits the receipt with the new document. During backup restore, shared archives retain their original peer in `library_values` before rotating the current peer. Previous builds can continue to read the library and merge the new personal document through the existing sync format.

Portable `.feinotesbackup` archives now include local snapshot bodies, version labels and history belonging to trashed notes, for both the personal library and shared-note libraries. Restoring an archive installs that history with the database and attachments; the previous library and its history remain in the rollback folder. New archives use backup format 2, and this build still reads format 1 backups (which contain no local snapshots). Older builds do not read format 2 archives. SQLite schema and CloudKit protocol versions are unchanged.

Restoring a snapshot uses the normal product cloud-save transaction, including CRDT state and the durable outbox. Queue failure rolls back the materialized body; shared-note read-only or revoked access rejects restoration. Local-only saves use SQLite directly. Returning from a preview restores the editor state and never saves the preview. Retention is the upstream limit (1-30 versions per note, 200 MiB total); deletion, undo/redo, restoration and purge move or remove ancillary history with the note.

## Cloud checkpoints and attachment retention

The journal inventories immutable CloudKit records in SQLite. After 32 records
for a document or catalogue, the owner can publish a full Loro checkpoint,
including causal history and protected recovery versions. A fresh Loro instance
must import it with matching state and version vector before publication. Only
a later verified readback authorizes deleting explicitly covered records, up to
160 per batch. Checkpoints over 16 MiB leave the original updates intact.

Every writer and maintenance request conditionally updates the same per-zone
fence in an atomic CloudKit operation. A stale generation retries after fetching
changes. Shared participants can write within their permissions; only the owner
can compact or collect. Folder rooms checkpoint their catalogue and individual
descendant documents independently.

Attachment headers use `QuietNoteEdit.assetIndex` (BYTES). Restore downloads
referenced chunks instead of all historical assets. Collection protects current
and trashed documents, recoverable sync history, checkpoint versions, named local
versions and crash recovery. Unreferenced chunks must be observed for at least
one hour and all pending work must drain before collection. An offline device
reusing a collected file uploads its local bytes before publishing the reference.
Local cache collection additionally verifies the digest and runs at a new session.

SQLite schema 7 and existing backups are retained; the new inventory tables are
created on open. The checkpoint envelope changes the beta cloud wire format.
Update all beta writers together; mixed writing with earlier beta builds is not
supported. Production distribution requires the additive `assetIndex` schema
field. Code integration and Development probes do not establish Production
schema deployment or real multi-account acceptance.

## Boards

The November 18 GPUI migration changes rendering/platform dependencies, not SQLite schema 7, backup formats or the Loro 1.17.1 synchronization engine. File-backend access goes through `fdl_core::vfs`; it aliases the real filesystem on native builds and isolates browser sample notebooks in memory. The browser demo has no product CloudKit or personal-library access. Mobile continues to use the separately pinned GPUI backend, with conditional drawing and element conversions kept in the shared code.

Standalone boards use `Meta::kind = Board` on both backends. The file backend writes `.excalidraw`; SQLite stores its document and embedded scene through the existing DocxXML block payload, with no schema migration. SQLite history keeps XML snapshots and restores through the normal cloud transaction. Scene image paths participate in attachment enumeration, import rebasing, cloud upload, shared-copy recovery and portable backups. Image downloads and live board views use the selected personal or shared library root.

The cloud protocol treats one board scene as a block property, like other opaque block payloads. Independent paragraph edits merge with a scene edit; concurrent changes inside the same board use the existing block-property conflict behavior rather than an element-by-element board protocol. Older clients preserve unsupported whiteboard XML. Shared read-only notes and history previews show the thumbnail without exposing an editable board; permission changes and incoming document state discard an open board editor so stale scenes cannot be saved over newer state. Live multi-account CloudKit board behavior still requires separate acceptance.

## Regression coverage

`fdl_core` tests cover both file layouts, staged imports, attachment and document links, trash, optimistic revisions, history and transaction rollback. Cloud tests cover restoring local snapshots into CRDT/outbox state, rollback after an injected queue failure and shared-note read-only/revoked access. The native `tools/harness/sqlite_history.py` regression checks empty-library shortcuts, pending IME text, preview refusal and persisted restoration in an isolated local-only SQLite library. Table import and scroll-intent harnesses cover background metadata adoption, manual widths, undo/redo, selections and viewport preservation. Background listings update Store metadata without releasing open-editor conflict baselines. Run the two repository check scripts and 2000 random-edit seeds before advancing the product branch.
