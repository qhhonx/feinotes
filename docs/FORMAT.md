# Storage format

## Library directory (FORMAT 6)

The default library is a folder users can browse and back up at `~/Documents/Quiet.md`. Users can select a different location on first launch. The app stores it in `quiet-md/library.json` in the local application data directory. The `store::Location` record currently has one field, `path`. Add `#[serde(default)]` to future fields. Set `QUIET_MD_HOME` for a temporary override and always use it for tests.

The sidebar mirrors the filesystem. Folders are directories and each document is a file named after its title. See `layout::file_name_for` for escaping. A duplicate gets a suffix such as ` (2)`; ` 2` from older versions is still read as one. The design is described in [Real folders](design/real-folders.md).

```text
Quiet.md/
├── Work/
│   ├── Weekly report.xml   DocxXML document
│   └── Proposal.md         Markdown document
├── Notes.xml
├── .assets/                Pasted or inserted images and attachments, with previews in .assets/thumbs
└── .quiet/                 Application data
    ├── format.json         {"format": 6}, the layout version (store::FORMAT)
    ├── meta/<device id>/   Attached metadata, a folder per device (below): device.json and shards 00.json … 3f.json
    ├── meta/<device id>/<nn>.json.bak  That shard's previous version
    ├── meta/<device id>.json  FORMAT 5: the device's metadata in one file (another device not yet on shards)
    ├── meta/<device id>.json.single  A development FORMAT 6 file, kept 30 days after its move to shards
    ├── index.json.v4       The FORMAT 4 index, kept after the move to 5
    ├── settings.json       Window UI settings, including sidebar width
    ├── preferences.json, shortcuts.json, emoji.json, themes/
    ├── .lock               Left by older versions; FORMAT 5 and later ignore it (the write lock is local, below)
    ├── trash/              Deleted files; their records are `trashed` fields in meta/ (FORMAT 6)
    ├── trash/index.json.v5 The FORMAT 5 trash index, kept after the move to 6
    ├── history/<id>/       Document body snapshots
    ├── history-trash/<id>/ Snapshots for trashed documents
    └── before-folders-4/   Pre-migration layout backup, removed after 30 days
```

The scanner recursively finds the files a folder shows (below), including files added in Finder. It skips names beginning with a dot, including `.quiet`, `.assets`, `.git` and `.obsidian`. Window dimensions belong to the device and live in local application data. Standalone whiteboards use official Excalidraw JSON and take their title from the filename. Older versions skip this extension. The write lock allows multiple app instances. It is local to the computer, in the application data folder (`quiet-md/locks/<library id>.lock`, the id in `format.json`, so every copy of the app and the CLI opening the folder share it), and is held briefly for each write: a lock inside the library would only sync to other devices and lock nothing there. A FORMAT 4 library also takes `.quiet/.lock`, which older versions use.

### What a folder shows

One list for the whole app, in `crates/core/src/shown.rs`; a folder added from anywhere (a code checkout, a home folder) shows only what Quiet.md opens, and nothing else about it is read or written down.

- **Files** (`shown::LIBRARY_FILES`, by extension in any case): `.md` / `.markdown` / `.mdown`, `.xml` that is DocxXML, `.excalidraw`, `.sqlite` that is a 数据表. A `.xml` is DocxXML when, after a byte order mark, an `<?xml?>` declaration and comments, its first element is `<title>`, a block (`<p>`, `<h1>` …) or `<document>` / `<doc>` / `<fragment>`; only the first KB is read, and an empty file counts (a page being written). `pom.xml`, Android layouts, plists and HTML do not. A `.sqlite` is a 数据表 when it holds the `_tables`, `_fields`, `_views` and `_meta` tables; another program's database is not, with the `datatable` feature on or off. A file still only in iCloud is taken by its name, since reading it would download it. What `list` read before (its parse cache) is not looked at again; what was not ours is looked at again once it changes (`Store::checked`, for the session).
- **Attachments** (`shown::ATTACHMENTS`: png, jpg, jpeg, gif, webp, bmp, pdf) are what a document previews in place. A folder does not list them; the list is there for the day it does.
- **Folders not gone into** (`shown::skips_dir`): by ecosystem, version control (`.git`, `.hg`, `.svn`); JavaScript (`node_modules`, `bower_components`, `jspm_packages`, `.next`, `.nuxt`, `.svelte-kit`, `.turbo`, `.parcel-cache`, `.vite`, `.angular`, `.nyc_output`, `storybook-static`); C / C++ (`CMakeFiles`, `cmake-build-*`); Python (`__pycache__`, `.venv`, `venv`, `.tox`, `.nox`, `.pytest_cache`, `.mypy_cache`, `.ruff_cache`, `.ipynb_checkpoints`, `*.egg-info`, `.eggs`, `site-packages`); JVM and Apple (`.gradle`, `.idea`, `DerivedData`, `Pods`, `Carthage`, `.build`, `xcuserdata`); others (`.dart_tool`, `.pub-cache`, `elm-stuff`, `.terraform`, `.bundle`, `.cache`, `.sass-cache`, `.docusaurus`, `.expo`, `.serverless`); the system's (`.Trash`, `.Trashes`, `.Spotlight-V100`, `.fseventsd`, `$RECYCLE.BIN`, `System Volume Information`, `Library/Caches`). Names a person may give a folder of notes are skipped only beside the project file that makes them build output: `target` (Cargo.toml, pom.xml, build.sbt), `build` (package.json, Gradle, CMakeLists.txt, setup.py, pyproject.toml, pubspec.yaml, Package.swift, `*.xcodeproj`, meson.build), `out` (package.json, tsconfig.json, Gradle, CMakeLists.txt, `*.xcodeproj`, `*.iml`), `dist` (package.json, tsconfig.json, pyproject.toml, setup.py, setup.cfg), `env` (pyproject.toml, setup.py, setup.cfg, requirements.txt, Pipfile), `vendor` (go.mod, composer.json, Gemfile, Cargo.toml), `coverage` (package.json, pyproject.toml, setup.py, Cargo.toml, go.mod). A folder called `build` with no such file beside it shows as usual.
- **`.gitignore`**: every one at any level, and `.git/info/exclude` beside a `.git`, a deeper file having the last word. Supported: `#` comments, `!` to take a path back, a trailing `/` for folders only, a pattern with a `/` in it relative to its file's folder (else a name at any depth), `*`, `?`, `[a-z]` / `[!a-z]`, `**` and `\` escapes. Not read: git's global excludes file. As in git, nothing inside an ignored folder can be taken back.
- **Folders** show when something listed is in them, at any depth, or when the user made them: made in Quiet.md (its id is not the one its path gives, `meta::path_id`), renamed there, pinned or starred. Folders found empty are not written down.
- **Entries written before** (by an older build) that these rules now hide stay in `meta/` as they were and in memory, only not listed: nothing is read again for them, and a file that shows again (its project file gone, the `.gitignore` line removed) comes back with its id and settings.

Nothing that is not shown gets an entry, so adding a code checkout writes a few hundred entries, not one per folder of `node_modules`. `.quiet/` itself is still made when a folder is added.

### Child pages: a folder's own document

A document can have child pages without a second kind of tree. **A folder holding a document with the folder's own name is that document, and the rest of the folder are its child pages:**

```text
Quiet.md/
└── Plan/
    ├── Plan.xml        The page "Plan" (the folder's own document)
    ├── Budget.md       A child page of "Plan" (a Markdown file, which has no child pages of its own)
    └── Schedule/
        ├── Schedule.xml   A child page of "Plan" with child pages of its own
        └── Week 1.xml
```

- **The rule reads only the disk.** The folder `X/` and the file `X/X.xml` must have the same name (exact, after the escaping of `layout::file_name_for`). Only a document (`.xml`) is ever the folder's document: `X/X.md` is an ordinary Markdown file in an ordinary folder `X/`, and a `.excalidraw` or `.sqlite` file never is it either. Markdown files take no child pages, so 新建子文档 is not offered on them. `index.json` has no field for this and `store::FORMAT` is unchanged, so older builds read the same files as a folder holding a document.
- **Shown as a document.** The sidebar and Home show the folder as one document row or card with the document's title and icon, and an expand arrow when there are other entries in it. Opening it opens `X/X.xml`. The document is not repeated among the children, search and @-mentions list it once (as the document), and its breadcrumbs stop above the folder.
- **Adding the first child page** (新建子文档 in the row and document "…" menus; document rows have no hover "+") creates `X/` where `X.xml` was, moves the document into it as `X/X.xml` (first in the folder's order, the folder taking the document's place), then creates the new page last in the folder. Links to the moved document and from it are rewritten as for any move. The steps are one entry in the file-operation undo history. A document already in its folder gets the new page directly. The folder is named after the file (`Plan (2).xml` gives `Plan (2)/Plan (2).xml`), numbered when the name is taken.
- **The pair keeps one name.** Renaming the document (in the editor or the sidebar) renames the folder and then the file after the folder. A child page that already holds that name inside the folder is numbered first. Moving, dragging and deleting the row act on the whole folder, and dropping something on the row puts it in the folder.
- **Without children the folder stays.** When the last child page is moved out or deleted, `X/X.xml` is left as it is and still shows as a plain document. Undoing 新建子文档 moves the document back out and removes the empty folder.
- **Existing libraries.** The FORMAT 2 and FORMAT 4 migrations turned pages with child pages into exactly this shape (`migrate::pages_to_folders`), so those pages have their children again.

The same mapping (a page with children ⇄ a folder with its own document) is meant for importing and exporting Feishu wikis and for the command line.

### Links and images

- Document links are relative to the current file. DocxXML uses `<cite type="doc" href="../Work/Weekly report.xml">` and Markdown uses standard links. On read, the app resolves paths to runtime IDs. On write, it converts them back, so copying the whole folder preserves the links. The reader still accepts legacy `doc-id` links.
- **Every path a file in the library keeps is relative and uses `/`, on every system**, so a library written on Windows reads the same on a Mac and the other way round. What it is relative to depends on the format:
  - **DocxXML pages** (our own format): relative to the library folder. Image `src`, attachment `<source path>` and `<title cover>` are `.assets/…`. A board's `files[].path` is the same.
  - **Markdown files in the library**: relative to the file's own folder, as Obsidian, Typora and GitHub read them: `.assets/x.png` from the top, `../.assets/x.png` from a folder. Moving the file rewrites them. Reading, a relative source is looked for beside the file first, then in the library (`.assets/…`, as older builds wrote it); a picture found only that way is written again relative to the file.
  - Document and plain file links are relative to the current file in both formats (below).
- `fdl_core::paths` holds the one set of functions for this: `portable` / `portable_in` write a path relative to a folder (the library, or the file's), and `resolve` reads one back, either `/` or `\` taken.
  - In memory a page's own files are absolute paths on this computer; `Store` and the Markdown writer convert when they read and write, so editing, copying and exporting need not.
  - A file outside the library keeps its absolute path, as written.
  - Pages from before the rule keep absolute paths, this system's or another's (`C:\Users\…\.assets\x.png` on a Mac, `/Users/…/.assets/x.png` on Windows). The reader finds the same file under this library's `.assets/` (the part after the path's last `assets` / `.assets` folder), and the next save writes it relative.
  - Older versions already read a DocxXML page's `.assets/…` relative to the library, so the rule did not change FORMAT. (A Markdown file's `../.assets/…` in a folder, written relative to the file, older builds do not find.)
  - `no_writer_keeps_a_platform_path` (store tests) scans what every writer leaves (DocxXML, Markdown imported, in a folder, moved, and outside the library, 思源导入, boards, the library's records) for a `\`, a drive letter or the library's absolute path, and checks each Markdown file's sources from its own folder.
- Plain links to files, such as `[Report](attachments/Report.pdf)` or `[Weekly report](../Work/Weekly%20report.md)` in Markdown and `<a href>` in DocxXML, are relative to the current file. Moving or renaming a document, the file a link points at or a folder holding either rewrites them like document links; a link that leaves the library still points at the same file. Clicking one opens a library document or a Markdown file in the app, opens another file in its system app, and only reveals a folder or a file that would run a program.

### Metadata by device (.quiet/meta/, FORMAT 5 and 6)

What the files cannot say is split by device so a synced library never has two computers writing one file ([sync design](design/sync.md) 3.3, `crates/core/src/meta.rs`). Each device writes only its own files: FORMAT 5 one file, `meta/<device id>.json`; FORMAT 6 a folder of shards, `meta/<device id>/` (below). The device id is made once per computer and kept in the local application data folder (`quiet-md/device.json`), shared by every top-level folder. A FORMAT 5 file:

```json
{ "v": 1, "device": { "id": "9b2f…", "name": "", "platform": "macos" },
  "items": {
    "3e5c…": { "path": ["Work/Weekly report.xml", 1763800000123], "pinned": [true, 1763800000456], "scroll": [[3, 12.5], 1763800001000] },
    "7d1a…": { "path": ["Work/", 1763700000000] },
    "_root": { "history_limit": [7, 1763700000000], "look.theme": ["ink", 1763700000000] } } }
```

- **Every field is `[value, stamp]`.** The stamp is a hybrid logical clock in milliseconds: this computer's time, or one past the latest stamp seen when the clock is behind. Reading merges all devices' files and takes, per item and field, the value with the latest stamp (ties: the larger device id). `null` puts a field back to its default.
- **Items are ids**, with the entry's place in `path` (folders end in `/`). Renaming or moving rewrites `path`; removing a document in the app writes `path: null`. A file that is not on disk is left out of the list but keeps its entry, so metadata that arrives before its file (another device's sync) is not lost.
- **A write stamps only what changed** since the store last read the files (`Store::begin_write` re-reads them, as it re-read `index.json`), so a value another device wrote in between is not overwritten. A device keeps in its file only the fields it still wins.
- **Files found on disk** with no entry get the id derived from their NFC relative path (`meta::path_id`), the same on every device. Two ids claiming one path merge into the older one.
- **`_root`** holds the folder's own settings (one top-level folder on 主页; the name is from before the rename and stays on disk; `meta::FOLDER_ITEM` in code): `history_limit` and the folder's look (`look.theme`, `look.feishu`, `look.doc_family`, …), each field on its own. `scroll` (the reading position) is a field of its document. Fields this build does not know are kept.
- **Moving from FORMAT 4.** `Store::open_at` writes `index.json` and the folder-level parts of `settings.json` as this device's file, every field stamped at that moment, then `format.json` (last, so a crash redoes it), and renames `index.json` to `index.json.v4`. A library in a synced folder (`migrate::sync_of`) stays FORMAT 4 for now, since another computer may still run a build that cannot open 5; asking first is left to the sync work. A FORMAT 4 store still open when another copy moves the library to 5 stops writing.
- **The merged view is cached** in memory with each device file's name, size, modification time and a mark of each write (inode and change time on Unix, a hash of the contents on Windows: a sync can put back the time a file had where it was written); a write or a listing re-reads only when one of them changed, and in FORMAT 6 only the shards that changed (every device's file of those shards). A change time moves in ticks (a few milliseconds on Linux, a second on HFS+, two on FAT), so a second write in the same tick may leave all of them as they were: a file seen within two seconds of its change time (or with one ahead of this computer's clock) also keeps a hash of its contents, and is unchanged only while they hash the same (`meta::unchanged`). Once it is seen two seconds after its change, its metadata alone is trusted again. While none did, and memory is as this device last read or wrote it, the index is not rebuilt either.

### Shards (FORMAT 6)

A device's metadata is a folder, so a change writes about 1/64 of it instead of the whole (4.5 MB for 15,000 entries).

```text
.quiet/meta/9b2f…/device.json   { "id": "9b2f…", "name": "", "platform": "macos" }
.quiet/meta/9b2f…/2b.json       { "v": 1, "items": { "3e5c…": { "path": ["Work/Weekly report.xml", 1763800000123] } } }
```

- **Which shard**: `nn` is the first byte of the SHA-1 of the item id (its UTF-8 bytes), low six bits (`& 0x3f`), as two lowercase hex digits `00` … `3f` (`meta::shard_of`). Every device puts an item in the same shard. `_root` is an item like any other.
- **A shard** is the `items` part of a FORMAT 5 file, unchanged: `{ "v": 1, "items": { id: { field: [value, stamp] } } }`. An empty shard has no file.
- **Writing** rewrites only the shards holding a changed item, each through a temporary file and a rename, its previous version kept as `<nn>.json.bak` (not synced to the disk: a backup costs a shard no second sync), the folder synced once. A shard is cleaned as it is written (below), keeping only the fields this device still wins.
- **Reading** merges every device's shards, and the whole files of devices still on FORMAT 5's layout. When one device has both, its folder counts. `<id>.tmp/`, `.single`, `.bak`, `.corrupt-` and temporary files are not read. A shard named `<nn> 2.json` (a sync's copy) is read as that device's shard `nn`.
- **From the development builds' single file.** Libraries made by development builds of FORMAT 6 have `meta/<device id>.json`; the format stays 6. On opening, under the write lock, a device converts its own file only (never another's, so a synced folder stays readable throughout): it copies it to `<id>.json.single`, writes `device.json` and the shards to `<id>.tmp/` (each synced), renames that to `<id>/` and removes `<id>.json` and its backup `<id>.json.bak` (the `.single` copy is one now). Values and stamps are unchanged. A crash leaves a state the next opening finishes: a `<id>.tmp/` is made again; with `<id>/` and `<id>.json` both there only the removal is left. To go back within 30 days: remove `<id>/` and rename the `.single` copy to `<id>.json`. The day's clean-up (below) removes the copy 30 days after the conversion. A new top-level folder writes shards from the start, and the folder appears with the device's first field.
- **This computer's own record** (`meta::Here`, in the application data folder as `libraries/<key>.<device id>.meta.json`, never synced): the path each item's file was seen at here, for `gone`, when the shards were last cleaned, and when this device's file became shards (while its `.single` copy is kept).

### Deletions outside the app and clean-up (FORMAT 6)

- **`gone`**: `{"at": <seconds>, "from": "<the path it was at>"}`, written when a file is deleted outside the app. Only a device that saw the file here (its record above, at the path the item still has) writes it, when a whole scan (`list`, no folder unreadable) finds it missing and no iCloud placeholder is there; a device that never had the file writes nothing, since "not here yet" is not a deletion. It is stamped later than every field of the item, and `path: null` goes with it at the same stamp.
- **Deleted**: an item whose latest `purged` or `gone` is later than its latest `path` (or whose `path` is `null` at that stamp) is not listed and not waited for. A `path` written later (the file added back) brings it back.
- **Put back where it was**: a file found with no entry at the `from` of a `gone` stamped in the last 30 days (a git branch switched back, a folder moved back) is that item again: `list` writes its `path`, later than the tombstone, and it keeps its id and every field (pins, order …) instead of getting a new one from `meta::path_id`. Of several, the latest tombstone; one in the 回收站 is not brought back.
- **Clean-up**, each device of its own fields only, when it writes a shard and once a day for all its shards (about 30 ms for 15,000 entries, so not on opening: with the second `list` after it, the first one drawn already):
  - an item deleted as above goes once its tombstone is 30 days old and every active device (one that wrote any shard in the last 90 days) has a latest stamp later than the tombstone, so has seen it; a device silent for more than 90 days holds nothing back. An item whose every field is `null` (`meta::has_user_data`) goes by the same rule, by its latest field;
  - an item with no tombstone goes when none of its fields changed for 30 days and its file is not here (it never arrived), unless it is in the 回收站.

### Deletions and hand order (FORMAT 6)

FORMAT 6 moves the last shared files into the device files, so two computers deleting or reordering at once no longer write one file ([sync design](design/sync.md) 3.3).

- **`trashed`**: `{"from": "Work/Plan.xml", "at": <seconds>, "parent"?: <folder id>, "root"?: <id>, "meta": {…}}`. `from` is where it was (a folder's ends in `/`), `at` when it was deleted, `parent` the folder it goes back into, `root` the page it was deleted with (a folder's contents), `meta` the shared index fields it had (pins, order …; nothing of this computer's own, like 最近打开). Restoring writes `trashed: null`; the later write wins when one device deletes and another restores.
- **`purged`**: seconds, written by 彻底删除 and 30-day expiry. It hides the item everywhere. It goes by the clean-up rules above (in FORMAT 5's single files: thirty days after its stamp, the device that wrote it drops all its own fields for that item on its next write; the other devices drop theirs as they stop winning them).
- **The 回收站** lists items with `trashed` set and no `purged`, whose file (or folder) is in `.quiet/trash/`. A record whose file has not synced yet is kept and not shown; a file with no record waits for it (FORMAT 5 adopted such files into the index).
- **`order` and `root_order`** (`Meta::top_order` in code; the name on disk stays) are fractional index strings (`a0`, `a0V`, `a1` …, `board::fractional_index`), compared as bytes, equal keys by id. A move writes a new key for the moved item only, between its new neighbours; siblings are not renumbered. If nothing fits between two equal keys, that folder gets fresh keys throughout. The store works with ranks (1, 2 …) in memory: keys become ranks on reading (`meta::to_ranks`), and on writing the longest run of siblings still in their read order keeps its keys (`meta::to_keys`). FORMAT 5 numbers are read as before.
- **Moving from FORMAT 5** (`meta::upgrade6`, opened under the write locks, the format checked again inside): order numbers become keys in the same order and `trash/index.json` becomes this device's `trashed` fields, written to this device's file first; then `format.json` says 6; then `trash/index.json` is renamed `index.json.v5`. A crash midway redoes it from the files left and gives the same keys. A FORMAT 4 library moves 4 → 5 → 6 in one opening. As before, a library in a synced folder stays at its format. A store opened as 5 stops writing once another copy moves the library to 6, and builds that know only 5 refuse 6 (`format > FORMAT`). `Store::restore_backup` moves the whole `trash/` folder, `.v5` included.

### Index fields (.quiet/index.json in FORMAT 4)

In FORMAT 4 keys are relative paths. Folder keys end in a slash (e.g. `"Work/"`). Each value is an `Attached` record. FORMAT 5 keeps the same fields per item, with the path as `path`.

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

The sidebar and home page share `Meta::place()` for manual ordering. Unpositioned items come first, newest modification first. Manually positioned items follow by order number (FORMAT 6: by key, read as ranks). The ID breaks ties.

Older versions refuse to open FORMAT 4, builds that know only 4 refuse 5, and builds that know only 5 refuse 6.

### Reliability

- **Atomic writes.** Write a temporary file (`<name>.tmp`) and replace the destination with `rename`. After a crash, the destination contains either the old or the new contents.
- **Index recovery.** Before saving `index.json` (in FORMAT 5, this device's `meta/` file), copy the previous file to `.bak`. If parsing fails on read, keep the damaged file as `<name>.corrupt-<seconds>`, load the backup and notify the user once. If the backup is unusable, start with an empty index. Documents and folders remain on disk, but pins, stars and manual order may be lost.
- **Version checks.** If `format.json` specifies a newer format, refuse to open the library and prompt for an upgrade. Recheck after acquiring the lock for every write. Increment `FORMAT` whenever older versions would misinterpret a layout change.
- **Files added externally.** Markdown and XML files copied into the library appear in the document list, including DocxXML exported from Feishu.
- **Save errors.** Report failures to the user.

## Markdown files

A Markdown page is written as standard Markdown (`markdown::write_file`): one block after another, a blank line between them. A file is not rewritten while what it says is unchanged, and blocks that did not change keep their lines as they are on disk. Some things a reader drops are written so that they read back as typed:

- **An empty paragraph** is a line holding only `&nbsp;` (`markdown::BLANK`): a blank line only separates blocks, and Obsidian, Typora and GitHub show this line as an empty one. A file's own `&nbsp;` lines read as empty paragraphs too. Empty paragraphs at the very end are not written: they show nothing, and the editor adds one when you click below the text.
- **Spaces at the end of a line** (and a paragraph of only spaces, a no-break space, a tab) keep the last one as a character reference: `  &#32;` for three spaces, `&#160;`. Spaces at the start of a line are written as they are and read as they are.
- **A line break inside a block** (Shift + Enter) is `<br>`; `<br/>` and `<br />` read the same.
- **Text that would read as one of these** is escaped: a paragraph saying `&nbsp;` is `\&nbsp;`, `&#32;` in text is `\&#32;`, `<br>` is `\<br>`.

Reading, `&#…;` and `&#x…;` anywhere in a line are the characters they name. Older builds read these lines as the text they spell (`&nbsp;`, `&#32;`, `<br>`); the library layout is unchanged, so `store::FORMAT` is too.

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
