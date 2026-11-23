# Storage format

## Library directory (FORMAT 4)

The default library is a folder users can browse and back up at `~/Documents/Quiet.md`. Users can select a different location on first launch. The app stores it in `quiet-md/library.json` in the local application data directory. The `store::Location` record currently has one field, `path`. Add `#[serde(default)]` to future fields. Set `QUIET_MD_HOME` for a temporary override and always use it for tests.

The sidebar mirrors the filesystem. Folders are directories and each document is a file named after its title. See `layout::file_name_for` for escaping. A duplicate gets a suffix such as ` 2`. The design is described in [Real folders](design/real-folders.md).

```text
Quiet.md/
├── Work/
│   ├── Weekly report.xml   DocxXML document
│   └── Proposal.md         Markdown document
├── Notes.xml
├── .assets/                Pasted or inserted images and attachments, with previews in .assets/thumbs
└── .quiet/                 Application data
    ├── format.json         {"format": 4}, the layout version (store::FORMAT)
    ├── index.json          Relative path → attached metadata
    ├── index.json.bak      Previous index.json
    ├── settings.json       Window UI settings, including sidebar width
    ├── preferences.json, shortcuts.json, emoji.json, themes/
    ├── .lock               Write lock (flock), held briefly when updating the index
    ├── trash/              Deleted files and an index.json mapping IDs to Trashed records
    ├── history/<id>/       Document body snapshots
    ├── history-trash/<id>/ Snapshots for trashed documents
    └── before-folders-4/   Pre-migration layout backup, removed after 30 days
```

The scanner recursively finds `.md`, `.xml` and `.excalidraw` files, including files added in Finder. It skips names beginning with a dot, including `.quiet`, `.assets`, `.git` and `.obsidian`. Window dimensions belong to the device and live in local application data. Standalone whiteboards use official Excalidraw JSON and take their title from the filename. Older versions skip this extension. The write lock allows multiple app instances.

### Child pages: a folder's own document

A document can have child pages without a second kind of tree. **A folder holding a document with the folder's own name is that document, and the rest of the folder are its child pages:**

```text
Quiet.md/
└── Plan/
    ├── Plan.xml        The page "Plan" (the folder's own document)
    ├── Budget.md       A child page of "Plan"
    └── Schedule/
        ├── Schedule.xml   A child page of "Plan" with child pages of its own
        └── Week 1.xml
```

- **The rule reads only the disk.** The folder `X/` and the file `X/X.xml` or `X/X.md` must have the same name (exact, after the escaping of `layout::file_name_for`). A `.excalidraw` or `.sqlite` file never is the folder's document. When both `X.xml` and `X.md` exist, `X.xml` is it and `X.md` is an ordinary child page. `index.json` has no field for this and `store::FORMAT` is unchanged, so older builds read the same files as a folder holding a document.
- **Shown as a document.** The sidebar and Home show the folder as one document row or card with the document's title, icon and format, and an expand arrow when there are other entries in it. Opening it opens `X/X.*`. The document is not repeated among the children, search and @-mentions list it once (as the document), and its breadcrumbs stop above the folder.
- **Adding the first child page** (the row's hover "+", or 新建子文档 in the row and document "…" menus) creates `X/` where `X.xml` was, moves the document into it as `X/X.xml` (first in the folder's order, the folder taking the document's place), then creates the new page last in the folder. Links to the moved document and from it are rewritten as for any move. The steps are one entry in the file-operation undo history. A document already in its folder gets the new page directly. The folder is named after the file (`Plan 2.xml` gives `Plan 2/Plan 2.xml`), numbered when the name is taken.
- **The pair keeps one name.** Renaming the document (in the editor or the sidebar) renames the folder and then the file after the folder. A child page that already holds that name inside the folder is numbered first. Moving, dragging and deleting the row act on the whole folder, and dropping something on the row puts it in the folder.
- **Without children the folder stays.** When the last child page is moved out or deleted, `X/X.xml` is left as it is and still shows as a plain document. Undoing 新建子文档 moves the document back out and removes the empty folder.
- **Existing libraries.** The FORMAT 2 and FORMAT 4 migrations turned pages with child pages into exactly this shape (`migrate::pages_to_folders`), so those pages have their children again.

The same mapping (a page with children ⇄ a folder with its own document) is meant for importing and exporting Feishu wikis and for the command line.

### Links and images

- Document links are relative to the current file. DocxXML uses `<cite type="doc" href="../Work/Weekly report.xml">` and Markdown uses standard links. On read, the app resolves paths to runtime IDs. On write, it converts them back, so copying the whole folder preserves the links. The reader still accepts legacy `doc-id` links.
- Image and attachment paths are relative to the library root (`.assets/…`).

### Index (.quiet/index.json)

Keys are relative paths. Folder keys end in a slash (e.g. `"Work/"`). Each value is an `Attached` record.

| Field | Meaning | If omitted |
|---|---|---|
| `id` | Runtime ID for the editor, windows and undo. Links use paths. | Required |
| `created` / `modified` | Unix time in seconds | Required |
| `pinned` | Pinned | false |
| `starred` | Time starred, or 0 if unstarred | 0 |
| `order` | Sibling order within a folder | 0 (automatic) |
| `root_order` | Top-level sibling order | Automatic |
| `width` | Page width, `standard` (820), `wide` (1020) or `full` | standard |
| `catalogue_closed` | Table of contents collapsed | false |
| `reading` | Reading mode | false |
| `info` | Document information line, `{hidden, created, modified}` | Show modification time only |
| `refs_hidden` | Hide references | false |

The path supplies the parent folder, name and Markdown status. Deleting the index preserves content, structure and links but loses metadata such as pins, stars and manual order. New fields must use `#[serde(default)]` and be omitted when they hold their default value.

### Migrating the old layout (FORMAT 3 → 4)

The old layout stored documents in a flat `docs/<id>.xml` directory and represented folders as index records. When `Store::open` encounters an old library:

- A local library migrates silently. The changelog card reports how many documents moved.
- `Store::migration_state` returns `NeedsConsent` for a library on a sync drive (e.g. iCloud or Dropbox), evidence of use on another computer (missing local paths or sync conflict copies), or a previous restoration of the old layout. The UI asks before proceeding. Choosing Later postpones the prompt for one week.
- The app moves the complete old layout into `.quiet/before-folders-4/` first. Failure at any step rolls back the migration. `Store::restore_backup` can restore the old layout.
- The earlier FORMAT 2 migration converted parent documents to folders of the same name and retained `index.before-folders.json` before migration.

The sidebar and home page share `Meta::place()` for manual ordering. Unpositioned items come first, newest modification first. Manually positioned items follow by order number. The ID breaks ties.

Older versions refuse to open FORMAT 4.

### Reliability

- **Atomic writes.** Write a temporary file (`<name>.tmp`) and replace the destination with `rename`. After a crash, the destination contains either the old or the new contents.
- **Index recovery.** Before saving `index.json`, copy the previous file to `.bak`. If parsing fails on read, keep the damaged file as `<name>.corrupt-<seconds>`, load the backup and notify the user once. If the backup is unusable, start with an empty index. Documents and folders remain on disk, but pins, stars and manual order may be lost.
- **Version checks.** If `format.json` specifies a newer format, refuse to open the library and prompt for an upgrade. Recheck after acquiring the lock for every write. Increment `FORMAT` whenever older versions would misinterpret a layout change.
- **Files added externally.** Markdown and XML files copied into the library appear in the document list, including DocxXML exported from Feishu.
- **Save errors.** Report failures to the user.

## DocxXML

DocxXML is Feishu/Lark's XML document format. The canonical representation of every block appears in `crates/core/tests/fixtures/all-blocks.xml`. The `fixture_round_trips` test requires byte-for-byte equality after reading and writing it.

### Writing

- Write each top-level block on its own line. Start children on new lines with two spaces of indentation per level.
- Write blocks inside table cells on a single line without indentation. Encode content newlines (such as lines of code) as `&#10;` to distinguish them from formatting.
- An empty list item with a child paragraph starts with `<li><p></p>…`. Without the empty paragraph, the reader merges the child paragraph into the list item.
- Use `folded="true"` for folded blocks. Store the icon and cover on `<title icon=… cover=… cover-x=… cover-y=…>`.

### Reading

- Discard only source formatting whitespace: a newline with indentation, and spaces or tabs before nested blocks. Other whitespace is content, including the space in `<p> </p>`.
- Preserve unrecognized elements verbatim on output. Unknown block-level elements become `BlockKind::Raw`.
- Each inline object (formula, file, button, date reminder, document mention or unknown inline element) occupies one U+FFFC in the text. Its style run holds `Inline { kind, xml }`. The reader parses `kind` once for display and export. The writer emits the original `xml` verbatim, preserving unknown attributes.
- When creating or editing an object, the editor uses `docxml::to_inline` to build canonical XML and read it back, keeping both fields consistent across saves.

### Local extensions

These constructs extend DocxXML. Other tools may treat them as unknown elements or attributes.

- `<countdown duration start-time notify color timing/>` stores a countdown. Cloud countdowns are widgets outside DocxXML.
- A Mermaid diagram is a code block, `<pre lang="mermaid"><code>…</code></pre>`, so its source survives in any tool and Markdown export writes a ```` ```mermaid ```` fence. Quiet.md draws the diagram; `view="code"` keeps showing the code instead.
- `<time … is-whole-day="true">` stores an all-day reminder. The model has `isWholeDay`, but the original XML has no equivalent attribute. Without it, a reminder at 23:59 local time also counts as all-day.
- Local images and attachments use `path="…"` where the cloud uses `token` / `href`. A video or file is represented by `<figure><source path=… name=…/></figure>`.

- An embedded whiteboard uses `<whiteboard type="excalidraw" height="360">{"elements":[…],"files":{…}}</whiteboard>`. Its body is XML-escaped scene JSON with one element per line, stable key order (`board/json.rs`) and deleted elements omitted. The height is the on-page height. On read, `board/restore.rs` repairs the scene. Unreadable JSON and other types (e.g. Feishu's `type="blank"`) are preserved verbatim as unknown elements.

### Known limitations

- A block with nested children inside a table cell loses trailing spaces or tabs in its text on read. Older versions wrote indentation inside cells, so the reader discards whitespace before nested blocks. The fuzzer applies the same normalization (`trim_before_nested`) in `round_trip`.
- If a block that cannot nest (e.g. code, divider or image) has children in memory, the writer drops them. The editor enforces the [structural invariants](ARCHITECTURE.md) that prevent this.

## Version history

Each file in `.quiet/history/<id>/` contains the on-disk body **before** a save, stored verbatim. Filenames use a 13-digit millisecond timestamp, a hyphen, an eight-digit UUID suffix and the XML or Markdown extension. Fixed-width timestamps sort lexically and UUIDs avoid collisions between devices. A named version has an adjacent `<snapshot-filename>.name` (one plain-text line, up to 40 characters). Clearing the name removes the sidecar. The directory is the index.

- **Capture.** After writing a new body, `Store::save_now` snapshots the previous in-memory text. Skip identical content and automatic snapshots less than five minutes apart. Save Current Version bypasses the interval but still deduplicates identical text. Snapshot failures log warnings while the document save succeeds.
- **Retention.** Keep 20 per document by default, configurable from 1 to 30 in Settings › Library. After each write, also enforce a 200 MiB cap across `.quiet/history/`, evicting oldest first.
- **Trash.** `delete` moves each file, including name sidecars, to `.quiet/history-trash/<id>/`. `restore` moves them back. Purging or 30-day trash expiry permanently deletes them. Retention and budget eviction remove the matching name file too.
- **Naming.** Hover a drawer entry to name it. The named-only filter finds milestones among automatic snapshots.
- **Restore.** Force a snapshot of the current disk content before saving the historical body through the normal editor path, so a restore can itself be reversed. Save a Copy creates a new document ID and puts the version name or time in the title, e.g. “Product notes (First draft)”.
- **Preview.** Selecting a version opens it read-only in the document area (`Editor::set_preview`). The banner identifies the version and offers change highlighting, restore and return. Returning reinstates the entire original editor state, including caret, selection and undo. During preview, `Store::flush` and `sync_document` skip historical content.
- **Highlighting.** `changes::block_changes` compares a version with its **previous** snapshot using a block LCS. New blocks have a light green background and rewritten blocks light yellow. Containers such as quotes, callouts and column groups apply the background to their child rows. View the preceding snapshot to see deleted blocks. IDs are generated during parsing, so highlighting must use the same parse result as rendering.
- **Counts.** The drawer shows lines added and removed from a line-level LCS of adjacent snapshots (`diff`). The oldest version has no predecessor and is labeled accordingly.
- **Compatibility.** Older builds ignore the history directory and read the index and body normally. The layout needs no `store::FORMAT` bump. Saving in an older build simply stops updating history.
- **Identity.** Snapshots use the document's runtime ID from the index. Renaming or moving a document preserves history. Deleting the index assigns new IDs and detaches previous snapshots.
- **Assets.** The app currently retains `.assets/`, so historical references remain usable. Any future garbage collector must scan all snapshots, trashed documents and recovery copies.

## Save failures and recovery

Each write uses a unique temporary file in the destination directory. The app syncs the file before renaming and also syncs the parent directory on Unix. If the library write lock is unavailable, users can still list notes in read-only mode. If another instance has changed the document on disk, the app preserves that version and offers to reload or save a copy.

If a save fails during system shutdown, the app stores the document body in `quiet-md/recovery/<library-id>/` in local application data. When the same library next opens, it creates a recovered copy and removes the recovery file after a successful write. This mechanism handles failed shutdown saves separately from version history.

Export checks canonical paths and rejects files outside the library, including symlinks that escape it. HTML reads up to 20 MiB per embedded image. Markdown copies assets up to 256 MiB each and counts missing, rejected or oversized assets in the export warnings. Both inline and block attachments export as links with copied files.
