# Fei Notes storage after upstream FORMAT 4

Fei Notes keeps two storage implementations behind `Store`. The upstream-compatible build uses the file backend: FORMAT 4 title-named files, real folders, `.quiet/` metadata and `.assets/` attachments. Old file libraries retain upstream migration consent and rollback behavior. `docs/FORMAT.md` documents this backend.

The macOS product (`cloudkit-sync`, including its local-only mode) continues to use the startup-selected SQLite library. Its schema stays at version 7, with stable block/table identities and the existing CloudKit/Loro protocol. No file-layout migration, file-library picker or upstream onboarding may replace this selected root. iCloud is synchronization on top of SQLite, not a third document backend.

## Library settings

Settings > Documents keeps upstream library rows together (location, Trash and local version-history retention). Fei Notes appends separate Import, iCloud Sync, and Backup and Restore groups through `product_library_groups`. Keep future product controls out of the upstream row list. Cloud library selection, sync status, sync issues and cloud-access controls are shown directly on this page; there is no separate iCloud settings page. Entering or reopening Documents refreshes cloud-access status, and purchase changes continue to refresh the visible controls.

## Import and recovery

FORMAT 3 and FORMAT 4 libraries are imported through an isolated staged copy. The source is never rewritten. Existing document IDs are checked for duplicates; paths and links are resolved by the file backend, then attachments are rebased into an import-specific `assets/legacy-*` directory. Imported live and trashed documents and their metadata commit together in SQLite. Symlinks and unsupported files are rejected.

The upstream version drawer works with both backends. File snapshots retain upstream raw text. SQLite snapshots serialize the prior committed document after the body and sync transaction succeeds; snapshots are ancillary local files under `.quiet/history/` and are not CloudKit records. SQLite remains the authoritative document store. Existing cloud recovery history and durable queues are preserved.

Portable `.feinotesbackup` archives now include local snapshot bodies, version labels and history belonging to trashed notes, for both the personal library and shared-note libraries. Restoring an archive installs that history with the database and attachments; the previous library and its history remain in the rollback folder. New archives use backup format 2, and this build still reads format 1 backups (which contain no local snapshots). Older builds do not read format 2 archives. SQLite schema and CloudKit protocol versions are unchanged.

Restoring a snapshot uses the normal product cloud-save transaction, including CRDT state and the durable outbox. Queue failure rolls back the materialized body; shared-note read-only or revoked access rejects restoration. Local-only saves use SQLite directly. Returning from a preview restores the editor state and never saves the preview. Retention is the upstream limit (1-30 versions per note, 200 MiB total); deletion, undo/redo, restoration and purge move or remove ancillary history with the note.

## Regression coverage

`fdl_core` tests cover both file layouts, staged imports, attachment and document links, trash, optimistic revisions, history and transaction rollback. Cloud tests cover restoring local snapshots into CRDT/outbox state, rollback after an injected queue failure and shared-note read-only/revoked access. The native `tools/harness/sqlite_history.py` regression checks empty-library shortcuts, pending IME text, preview refusal and persisted restoration in an isolated local-only SQLite library. Table import and scroll-intent harnesses cover background metadata adoption, manual widths, undo/redo, selections and viewport preservation. Background listings update Store metadata without releasing open-editor conflict baselines. Run the two repository check scripts and 2000 random-edit seeds before advancing the product branch.
