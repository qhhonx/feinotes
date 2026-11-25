//! `quiet help` and `quiet help COMMAND`: the overview, and each command
//! with its options and an example, so an agent need not read the whole
//! guide (`quiet skill`) to call one.

use crate::fail::{R, fail};

pub const HELP: &str = "quiet — read, search, edit and script a Quiet.md library

  quiet ls [FOLDER] [-r] [--kind doc|md|board|table|folder]
  quiet find WORDS… [--in FOLDER] [--limit N]
  quiet outline DOC                      headings with refs and sizes
  quiet read DOC[#REF] [--section] [--as md|text|xml|json] [--max-chars N]
  quiet blocks DOC[#REF] [--kind K] [--grep TEXT] [--done|--undone]
  quiet links DOC                        mentions out of and into DOC
  quiet check                            broken links, unreadable files, unused assets

  quiet new TITLE [--in FOLDER] [--md TEXT|-] [--kind doc|md|board|table]
  quiet mkdir NAME [--in FOLDER]
  quiet insert DOC [--after|--before|--into|--end-of REF | --start | --end] --md TEXT|-
  quiet replace DOC#REF [--section] --md TEXT|-
  quiet delete DOC#REF [--section]
  quiet move DOC#REF --after|--before|--into REF [--section]
  quiet set DOC#REF [--text TEXT] [--done|--undone] [--kind K]
  quiet rename DOC TITLE | quiet mv DOC FOLDER | quiet rm DOC

  quiet table DOC[#REF]                  a 数据表's rows;  table add|update|delete …
  quiet board DOC[#REF]                  a 画板's elements; board add|rm …
  quiet history DOC [VERSION]            saved versions, or what changed since one
  quiet restore DOC VERSION              put a document back as it was
  quiet trash | quiet untrash NAME       the trash, and bringing a document back
  quiet import FILE… [--in FOLDER]       Markdown, DocxXML, CSV, .excalidraw

  quiet run [FILE|-] [-e CODE]           a JavaScript script, as one transaction
  quiet call OP [JSON]                   any operation by name (what run's lib calls)
  quiet mcp                              serve these as MCP tools on stdio
  quiet skill                            the full guide for agents (script API included)
  quiet skill install                    the agent skill, for Claude Code and Codex
  quiet help COMMAND                     one command in detail
  quiet telemetry [on|off|status]        the anonymous daily usage counts

Writes take --dry-run (show the diff, write nothing) and --expect-rev REV.
Paths start at 主页 (Home): the first part is one of the folders Quiet.md
has open on this computer (`quiet ls` lists them), e.g. Quiet.md/工作/周报.
DOC is such a path (extension optional), a path or title only one document
has, or an id. REF is what outline/blocks print (#3~k7f2: position and
fingerprint), a position (#2.1) or a heading's text. New things go to the
--in folder, else the first folder. Global: --library DIR (that folder on its
own, paths starting in it), --json.";

/// (names, usage and notes).
const COMMANDS: &[(&[&str], &str)] = &[
    (&["ls"], "quiet ls [FOLDER] [-r] [--kind doc|md|board|table|folder]

Lists a folder; with none, 主页: the folders Quiet.md has open, where each
is on disk and whether it opens (two of one name: use its key, which --json
shows, as the first part of a path). -r goes into subfolders. Each line:
path, [kind], last modified.

  quiet ls 工作 -r --kind doc"),
    (&["find", "search"], "quiet find WORDS… [--in FOLDER] [--limit N]

Documents holding every word (any case) in their title or text, best first,
each with up to 5 matching blocks and their refs. Default limit 20.

  quiet find 季度 预算 --in 工作"),
    (&["outline"], "quiet outline DOC

The document's rev, size and headings, each with its ref and section size;
a document without headings lists its top-level blocks instead. Read this
before reading or editing a long document.

  quiet outline 周报"),
    (&["read", "cat"], "quiet read DOC[#REF] [--section|--no-heading] [--as md|text|xml|json] [--max-chars N]

The document (or the block REF; --section: the heading and its section;
--no-heading: the section without its heading) as
Markdown, plain text, DocxXML or JSON blocks. Prints at most 40000
characters unless --max-chars says otherwise (0: everything).

  quiet read 周报#'本周进展' --section"),
    (&["blocks"], "quiet blocks DOC[#REF] [--kind K] [--grep TEXT] [--done|--undone]

Blocks with ref, kind and text, filtered: --kind text|h1…h9|heading|bullet|
ordered|todo|code|quote|callout|table|image|board|datatable…, --grep (any
case), --done / --undone for todos. With #REF, only that block and its children.

  quiet blocks 发布计划 --kind todo --undone"),
    (&["links"], "quiet links DOC

@mentions of other documents in DOC (broken ones marked) and the documents
that mention DOC."),
    (&["check"], "quiet check

The whole library: files that do not parse, mentions of documents that are
gone, empty documents, pictures and files no document uses."),
    (&["new"], "quiet new TITLE [--in FOLDER] [--md TEXT|-] [--kind doc|md|board|table] [--fields A,B,… | --csv TEXT|-]

A new document (DocxXML; --kind md or --markdown for a .md file), its body
from Markdown; an empty board; or a 数据表 with the columns --fields names,
or from CSV text whose first row names them (types guessed from the values).

  quiet new 会议纪要 --in 会议 --md '# 议题\\n- [ ] 预算'
  quiet new 书单 --kind table --fields 书名,作者,状态
  quiet new 架构图 --kind board"),
    (&["mkdir"], "quiet mkdir NAME [--in FOLDER]"),
    (&["insert", "add"], "quiet insert DOC [--after|--before|--into|--end-of REF | --start | --end] --md TEXT|-

New blocks from Markdown; at the end by default. DOC#REF alone means
--after REF. --into puts them inside REF (a list item, quote, callout,
column) after its last child; --end-of at the end of heading REF's section.
A document that is one empty line is replaced. [[path or title]] mentions
another document: in its own top-level folder first, then in the others (several
there: ambiguous; write [[id]]).

  quiet insert 周报 --after '#3~k7f2' --md '- [ ] 补测试'
  quiet insert 方案 --end-of 风险 --md '- 许可证需要审核，见 [[法务/清单]]'"),
    (&["replace"], "quiet replace DOC#REF [--section] --md TEXT|-

The block (--section: the heading and its section) replaced by the Markdown."),
    (&["delete"], "quiet delete DOC#REF [--section]"),
    (&["move"], "quiet move DOC#REF --after|--before|--into|--end-of REF | --start | --end [--section]

  quiet move 方案#'风险' --section --after '#2~m4qa'"),
    (&["set"], "quiet set DOC#REF [--text TEXT] [--done|--undone] [--kind text|h1…h6|bullet|ordered|todo|quote|code]

Changes one block in place: its text (inline Markdown), a todo's state, its kind.

  quiet set 发布计划#'#7~p2ka' --done"),
    (&["rename"], "quiet rename DOC TITLE

The file follows the title. Mentions in other documents keep working."),
    (&["mv"], "quiet mv DOC FOLDER

Into FOLDER, a path from 主页 within the document's own top folder (its name
alone: its top). Links to it in other documents are rewritten; moving to
another top folder is not supported yet."),
    (&["rm"], "quiet rm DOC

To the trash, where the app keeps it 30 days; `quiet untrash` brings it back."),
    (&["table"], "quiet table DOC[#REF] [--table NAME] [--limit N] [--offset N]
quiet table add DOC[#REF] --set Field=Value … | --rows JSON|-
quiet table update DOC[#REF] ROW --set Field=Value … | --rows JSON|-
quiet table delete DOC[#REF] ROW…

A 数据表 file, or the 数据表 block REF in a document. Rows have ids that last
(r12). Values are text as the app shows them, and are written as if pasted:
numbers, dates, checkboxes (true/false), select options (new ones are added),
several options joined with commas. Computed fields cannot be written.
--rows takes a list of {\"Field\": value} objects; for update each needs \"_id\".

  quiet table add 书单 --set 书名=三体 --set 状态=在读
  quiet table update 书单 r12 --set 状态=读完
  quiet table add 书单 --rows '[{\"书名\":\"A\"},{\"书名\":\"B\"}]'"),
    (&["board"], "quiet board DOC[#REF] [--full]
quiet board add DOC[#REF] --elements JSON|- [--replace]
quiet board rm DOC[#REF] ID…

A 画板 file, or the board block REF in a document. `board` lists elements
(id, type, position, size, text or label, arrow ends) and their bounds;
--full adds the scene as Excalidraw JSON. `board add` takes Excalidraw
element skeletons, as convertToExcalidrawElements does: type (rectangle,
ellipse, diamond, text, arrow, line, frame), x, y, optional width, height,
label: {text}, text (for text), strokeColor, backgroundColor, id (to point
arrows at), start/end: {id} for arrows. New elements go on top; --replace
clears the board first.

  quiet board add 架构图 --elements '[{\"type\":\"rectangle\",\"id\":\"a\",\"x\":0,\"y\":0,\"label\":{\"text\":\"App\"}},{\"type\":\"rectangle\",\"id\":\"b\",\"x\":300,\"y\":0,\"label\":{\"text\":\"CLI\"}},{\"type\":\"arrow\",\"x\":100,\"y\":35,\"start\":{\"id\":\"a\"},\"end\":{\"id\":\"b\"}}]'"),
    (&["history"], "quiet history DOC [VERSION]

The versions kept of DOC, newest first (each save keeps the text it
replaced). With VERSION (its name, or 1 for the newest, 2 …): what changed
from it to now."),
    (&["restore"], "quiet restore DOC VERSION

DOC put back as it was at VERSION. What it has now is kept as a version
first, so a restore can itself be undone."),
    (&["trash"], "quiet trash

What is in the trash (kept 30 days), with ids for `quiet untrash`. Each top-level
folder has its own; untrash puts a document back where it was."),
    (&["untrash"], "quiet untrash NAME

Brings the document called NAME (or with that id) back from the trash, to where it was."),
    (&["import"], "quiet import FILE… [--in FOLDER]

Copies files into the library: Markdown (.md, .txt; pictures beside it are
copied in), DocxXML (.xml), CSV/TSV (as a 数据表), .excalidraw (as a 画板).
The originals are left alone."),
    (&["run"], "quiet run [FILE|-] [-e CODE] [--timeout SECS] [--dry-run]

A JavaScript script against `lib` (see `quiet skill`, \"Script API\"). The
whole script is one transaction: if it throws or times out (60 s), nothing
is written. Top-level await and return work; a returned value is printed.

  quiet run -e 'return lib.find(\"预算\").results.map(r => r.path)'"),
    (&["call"], "quiet call OP [JSON|-]

Any operation by its name with JSON arguments, as `lib.call` in a script.
Operations: ls find open outline read blocks links check history trash_list
(reading); insert replace remove move set create mkdir rename mv trash
restore untrash import table table_add table_update table_delete board
board_add board_remove. Arguments are the commands' options by name, the
document as \"doc\" and a block as \"at\".

  quiet call blocks '{\"doc\": \"周报\", \"kind\": \"todo\", \"done\": false}'"),
    (&["mcp"], "quiet mcp [--library DIR]

An MCP server on stdin/stdout with two tools: `quiet` (a command line) and
`quiet_run` (a script). For agents that speak MCP but have no shell. E.g. in
an MCP client's settings: {\"command\": \"quiet\", \"args\": [\"mcp\"]}"),
    (&["telemetry"], "quiet telemetry [on|off|status]

quiet counts how often its commands run: per day, how many ran and failed and
which commands (by name), under a random id for this computer. Never the
library, documents, titles or arguments. Finished days are sent once a day to
stats.quiet.md, with quiet's version and the system (macos/windows, arm or x86).
`status` shows what is kept and waiting; `off` stops it and forgets the id.
Nothing is counted while DO_NOT_TRACK or QUIET_NO_TELEMETRY is set, in CI, or
while 自动检查更新 (Check for updates automatically) is off in Quiet.md."),
    (&["skill", "guide"], "quiet skill
quiet skill install [--dir DIR]… [--force]

The full guide for agents: addressing, every command, the script API, examples.
`install` writes the agent skill (DIR/quiet/SKILL.md and references/guide.md),
by default into ~/.claude/skills and ~/.codex/skills where those agents are
installed, so they reach for quiet when a task is about the user's notes. Run it
again after updating quiet. A skill called quiet that it did not write is
left alone unless --force."),
];

pub fn of(cmd: Option<&str>) -> R<String> {
    let Some(c) = cmd else { return Ok(format!("{HELP}\n")) };
    COMMANDS.iter().find(|(names, _)| names.contains(&c)).map(|(_, t)| format!("{t}\n"))
        .ok_or_else(|| fail("usage", format!("no command {c:?}")).hint("`quiet help` lists them"))
}
