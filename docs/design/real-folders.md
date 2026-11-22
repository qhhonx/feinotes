# Design draft: real directories for library folders

Status at the time of this draft: phase 1 (format and migration) was implemented. See [Storage format](../FORMAT.md). Phases 2–4 were pending. Origin: #151. Related: #112 and #125.

## Problem

The pre-migration layout used virtual folders:

- Document bodies lived in a flat `docs/<id>.xml` or `docs/<id>.md` directory with IDs as filenames.
- Each folder was a Meta record in `index.json`. Its `folder` field held the name and `parent` identified its parent. There was no corresponding directory.
- Document links used `<cite type="doc" doc-id="…">` and located the target by matching its ID to its filename. Markdown documents and exports retained only the link title.

Finder, AI tools, sync drives and Git could not see the hierarchy. An AI writing into the library had no obvious destination, and exporting documents elsewhere broke their links.

## Principles

1. **The filesystem is authoritative.** Sidebar folders mirror directories, and filenames are titles, e.g. `Work/Weekly report.md` and `Work/Proposal.xml`. Moving and renaming in the app operates on real files. The app also recognizes external changes.
2. **Treat XML and Markdown equally.** Both use title-based names in user directories. DocxXML is tagged plain text that AI tools can read and write even if users never edit it directly.
3. **Store relative links in the document.** The links survive copying the entire folder and resolve independently of the index.
4. **Keep the index rebuildable.** Removing `index.json` preserves content, structure and links. Only extra information such as pins, stars and manual order is lost.
5. **Keep application data in a hidden directory.** User directories expose the user's own files.

## Proposed layout (FORMAT 4)

```text
Quiet.md/
├── Work/
│   ├── Weekly report.md
│   ├── Proposal.xml
│   └── Subfolder/…
├── Notes.md
└── .quiet/
    ├── format.json      {"format": 4}
    ├── index.json       Relative path → attached metadata
    ├── settings.json
    ├── .lock
    ├── trash/           Trash location under discussion (see Open questions)
    └── thumbs/          File preview thumbnails
```

Scan recursively for `.md` and `.xml`, skipping dot-prefixed files and directories (including `.quiet/`, `.git/` and `.obsidian/`) and image directories (see Open questions).

## Index

Key `index.json` by relative path. Store `pinned`, `starred`, `opened`, `order`, `width`, `reading`, `catalogue_closed`, `info`, `refs_hidden` and manual folder ordering. Derive `folder`, `parent`, `name` and `markdown` from the path.

After an external rename or move, match disappeared paths to newly discovered ones by file identity (Unix inode or Windows file ID, falling back to a content hash). Transfer metadata to the matched path. Discard unmatched old entries and treat unmatched new files as new documents. At worst this loses metadata such as pins. File contents survive.

The proposal allows each document a runtime ID in memory for editing, windows and undo. It proposes keeping that ID out of files and resolving links independently of it.

## Links

- **Markdown.** Write standard links such as `[Weekly report](../Work/Weekly%20report.md)`, relative to the current file and navigable in GitHub, Obsidian and VS Code. Accept Obsidian-style `[[Weekly report]]` on read by looking up the filename in the library. Write standard links on save.
- **DocxXML.** Use relative paths, e.g. `<cite type="doc" href="../Work/Weekly report.md">Weekly report</cite>`. Continue reading legacy `doc-id` links.
- **In-app moves and renames.** Use existing backlink data (`Document::doc_links`) to find referring documents, rewrite the paths and save, like Obsidian's automatic internal link updates.
- **External renames.** Match by file identity on scan and update referring documents. If that fails, try a unique filename match within the library. If unresolved, mark the broken link in the body and keep its text.
- **Export.** For a single-document export, rewrite links relative to the destination. If the target falls outside the export, fall back to the title and count it in export warnings.

## Watching external changes

- Use filesystem notifications, e.g. the `notify` crate with macOS FSEvents, Windows and Linux inotify. Coalesce bursts and rescan the affected directories.
- Keep the existing foreground rescan as a fallback.
- When the open document changes externally, preserve the on-disk content and offer the existing reload or save-copy conflict handling.

## Migration (FORMAT 3 → 4)

1. Acquire the write lock and back up all of `index.json` and `docs/`, e.g. in `.quiet/before-folders-4/`.
2. Build directories from `folder` and `parent`. Move each `docs/<id>.*` to `<folder-path>/<title>.<extension>` using the escaping rules below.
3. Rewrite all `doc-id` links to relative paths.
4. Generate the new `index.json` and write format version 4 to `format.json`.
5. Roll back to the backup on failure at any stage.

The existing version guard makes older versions refuse FORMAT 4. A round-trip test like `library_round_trips` should verify document bodies, parent folders and link targets before and after migration.

## Open questions

1. **Filenames.** How should we escape `/ \ : * ? " < > |`, leading dots, Windows reserved names such as `CON`, and long titles? Use a suffix such as ` 2` for sibling duplicates. Should title edits immediately rename the file? The preference was to rename it and rewrite referring links immediately. Markdown titles currently live in `Meta.name` and would become filenames after migration.
2. **Images.** Share a single `assets/` directory across the library or use a per-note `<title>.assets/` directory as for local files? The latter is easier to copy with a single document but must follow renames.
3. **Trash.** Use `.quiet/trash/` for in-app recovery or the more familiar system trash?
4. **Sync conflicts.** iCloud, Dropbox and Nutstore can create files like `Weekly report (conflicted copy).md`. Git merges can leave conflict markers. How should scanning detect and report these?
5. **Local files (`Meta.path`).** Keep opening Markdown files outside the library as before. The Open Local Folder request (#112) could reuse scanning and watching with an external directory as a second library root.
6. **Performance.** Measure the cost of recursive scanning and watching for libraries with thousands of documents. Preserve the `list()` cache that skips unchanged files based on modification time and size.

## Additional requirements (2026-11-12)

- **Finder mirrors the sidebar.** The directory in Settings › Library should open directly onto the sidebar folders and title-named files. Keep pins and ordering in `.quiet/`, with a separate entry point in Settings. Move the existing root-level `preferences.json`, `shortcuts.json`, `themes/` and `format.json` into `.quiet/`.
- **Self-contained notes.** A single file reveals its title (filename), body and outgoing links (relative paths). Removing `.quiet/` leaves the library usable, losing only metadata such as pins, stars and manual order.
- **Sync support.** First launch allows a library in iCloud Drive. Keep device-specific state (`opened` time and window dimensions) in local application data keyed by library path. Merge `.quiet/index.json` entry by entry using relative paths so edits to different documents on two devices coexist. Recognize iCloud placeholders (`.<name>.icloud`) during scanning and trigger downloads as needed.

## Suggested resolutions

1. **Filenames.** Replace illegal characters with full-width equivalents or hyphens, strip leading dots, append an underscore to Windows reserved names and truncate at 120 bytes. Resolve collisions with ` 2`, ` 3`, etc. Rename and update referring links on title changes, coalescing edits on the 500 ms save schedule.
2. **Images.** Use a hidden `.assets/` per folder to keep images with copied content, avoid renaming asset directories when titles change and keep Finder tidy.
3. **Trash.** Use `.quiet/trash/` to keep in-app restore and the existing trash UI. Permanently delete files when the trash is emptied.

## Phases

1. Library format and migration: layout, scan, path-keyed index, migration, rollback and round-trip tests.
2. Map sidebar moves, renames and deletes to real files. Use relative links and rewrite them automatically.
3. Watch external changes and surface conflict copies and broken links.
4. Open local folders (#112) and move imported files into the library (#125).

## Future work: an app-owned iCloud container

Defer iCloud messaging during first launch. A future managed-sync option could use iCloud Documents. An official build signed with the iCloud entitlement could place the library in `~/Library/Mobile Documents/iCloud~<container>/Documents` as ordinary Markdown and XML files. CloudKit's cloud records would conflict with the filesystem model.

The design leaves room for this without a breaking change:

- The only location input is `path` in `library.json`. Older versions see the container library as a regular directory. A future container flag could be a new `#[serde(default)]` field on `Location` that older versions ignore.
- Layout and format are independent of location. Device-specific state such as `opened` and window dimensions stays outside the library.
- Placeholder detection and on-demand downloading also apply inside the container.

Future implementation would require signing with the container entitlement and provisioning profile, checking `ubiquityIdentityToken` at startup and prompting if the user is signed out or iCloud is disabled for the app. Coordinate reads and writes with `NSFileCoordinator`, discover remote changes with `NSMetadataQuery` and handle conflicts with `NSFileVersion` alongside phase 3 conflict reporting. Self-built, Windows and Linux builds would continue using ordinary folders without the entitlement.
