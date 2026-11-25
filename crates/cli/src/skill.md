---
name: quiet
description: Read, search, edit and organize the user's Quiet.md notes library (documents, Markdown notes, boards, 数据表) with the `quiet` command line tool. Use it whenever a task touches the user's notes or knowledge base — "my notes", "my library", "Quiet.md", 笔记, 笔记库, 文档库, 知识库, 周报, 会议纪要, 待办 — to find or summarize something in them, write a new note, update or reorganize existing ones, collect todos, fill a data table or draw a diagram on a board. Edit the library through `quiet`, not by writing its files directly.
---

# quiet: the user's Quiet.md library

Quiet.md keeps notes as plain files in the folders the user opened in it
(`quiet ls` lists them; paths start with one: `Quiet.md/工作/周报`): `.xml` documents, `.md`
notes, `.excalidraw` boards and `.sqlite` 数据表. The user
edits them in the Quiet.md app; you work on the same files with `quiet`, which
keeps them valid, rewrites links when a document moves, keeps every replaced
text as a version, and refuses to overwrite a change made since you read.

## Before the first use in a session

1. Check it is there: `quiet version`. If not found, it ships inside the app:
   `/Applications/quiet.md.app/Contents/MacOS/quiet` on macOS, `quiet.exe`
   beside `quiet_md.exe` on Windows. Use that path, or suggest the user links it
   onto their PATH (`ln -s /Applications/quiet.md.app/Contents/MacOS/quiet /usr/local/bin/quiet`).
2. For anything beyond the basics below (the script API, 数据表, boards,
   history), read `references/guide.md` next to this file, or run `quiet skill`.
   `quiet help COMMAND` shows one command.

## Basics

```
quiet ls [FOLDER] [-r]                 what is there
quiet find WORDS…                      documents holding every word, with matching blocks and refs
quiet outline DOC                      headings with refs: read this before a long document
quiet read DOC#REF --section           one section (refs look like #3~k7f2)
quiet blocks DOC --kind todo --undone  blocks by kind, text, state
quiet new TITLE [--in FOLDER] --md '…' a new document from Markdown
quiet insert DOC --end-of HEADING --md '…'   add to the end of a section
quiet set DOC#REF --text '…' | --done  change one block
quiet run script.js                    a JavaScript script against `lib`: one transaction
```

A document is its path (`工作/周报`, extension optional) or title. New content is
Markdown; `[[path or title]]` links to another document (this folder's first,
then the other folders'; `[[id]]` when a name is ambiguous). Every write takes
`--dry-run`. Use `quiet run` instead of many commands when the steps do not
need your judgement in between.

## Working in someone's library

These are the user's own notes; they may have the app open while you work.

- Read before you write: `outline`, then only the sections you need.
- Prefer adding over rewriting. Do not delete, move, rename, restore or
  rewrite whole sections of existing documents unless the user asked for it.
- For a change across several documents, or a large one, run it with
  `--dry-run` first and show the user the diff when it is not clearly what they asked.
- Put new documents where the user said; if they did not say, where related
  documents are, and tell them where.
- After writing, tell the user what changed: the paths, and what was added or changed.
- If you got it wrong, undo it: `quiet history DOC`, then `quiet restore DOC 1`.
- On `stale_ref`, `stale_rev` or `conflict` the document changed since you read
  it (the user may be typing in it): read it again and redo the edit; never force it.
- Never write the library's files directly, and leave its `.quiet/` folder alone.
