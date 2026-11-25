"""数据表's 笔记 column, end to end, in a scratch library.

A table file of its own gets a 笔记 column; an empty cell makes a note
(titled as its record, in 「书单 · 笔记」 beside the table, which stays where it is) and opens it in 记录详情, where it is written in and
saved; undo and redo take the note to the 回收站 and back; deleting the
record asks about its note and undo restores both; a cell is pointed at a
note there already is; a new title renames the note's file and the cell
follows; 整页打开 opens the note as a page; a search finds what was written;
a note renamed in the sidebar stays through the undo of other edits.

Build with --release --features devtools, then run this file.
FDL_BIN selects another build; TABLE_NOTE_SHOTS a folder to copy the screenshots to.
"""
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import tempfile

here = Path(__file__).resolve().parent
work = Path(tempfile.mkdtemp(prefix="quiet-table-note-"))
source = work / "source"
(source / "docs").mkdir(parents=True)
doc_id = "aaaaaaaa-0000-0000-0000-000000000001"
(source / "index.json").write_text(json.dumps({doc_id: {"created": 1, "modified": 1, "opened": 1}}))
(source / "preferences.json").write_text(json.dumps({"appearance": "light", "english": False, "seen_version": "9.9.9", "auto_update": False, "quick_hotkey": False}))
(source / "docs" / f"{doc_id}.xml").write_text("<title>已有的笔记</title>\n<p>一段正文</p>\n")
db = sqlite3.connect(source / "书单.sqlite")
db.executescript("""CREATE TABLE "书单" ("书名" TEXT, "作者" TEXT); INSERT INTO "书单" VALUES ('三体','刘慈欣'),('球状闪电','刘慈欣'),('',NULL);""")
# a 数据表 file of ours (a library lists no other SQLite file): its bookkeeping tables, empty; the
# table it holds is taken in as one written by another program
db.executescript("""CREATE TABLE _tables (id INTEGER PRIMARY KEY, name TEXT, position INTEGER, next_auto INTEGER);
CREATE TABLE _fields (id INTEGER PRIMARY KEY, table_id INTEGER, table_name TEXT, name TEXT, position INTEGER, type TEXT, options TEXT, description TEXT);
CREATE TABLE _views (id INTEGER PRIMARY KEY, table_id INTEGER, table_name TEXT, name TEXT, position INTEGER, type TEXT, config TEXT);
CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT); INSERT INTO _meta VALUES ('format', '2'), ('app', 'Quiet.md');""")
db.commit()
db.close()

# where things are in a 1300×850 window (light theme, Chinese); the sheet starts at the header's foot (48)
TABLE_ROW = "62 166"            # 书单 in the sidebar: the first row, its notes under it once it has some
ADD_FIELD = "828 103"            # 「+」: a 文本 column at once, its name ready to type
NEW_TYPE = "845 103"            # the new column's type, after its name: its settings
TYPE_BOX = "1002 249"
TYPE_NOTE = "897 551"           # 笔记, the type list scrolled to its end
NOTE_1, NOTE_2 = "845 141", "845 177"   # the 笔记 cells of rows 1 and 2
PANEL_CLOSE = "1265 116"
NOTE_BODY, NOTE_TITLE = "885 573", "830 215"
NOTE_FULL = "1229 116"
UNDO = "720 69"                 # the table's own ↶ (⌘Z after a rename in the sidebar undoes the rename)

OPEN = f"""click {TABLE_ROW}
wait 1200
"""
ADD_COLUMN = f"""click {ADD_FIELD}
wait 500
type 读书笔记
key enter
wait 300
click {NEW_TYPE}
wait 500
click {TYPE_BOX}
wait 300
move 1002 404
wheel 1002 404 600
wait 100
wheel 1002 404 600
wait 300
shot types
click {TYPE_NOTE}
wait 300
key enter
wait 500
"""
WRITE = f"""dclick {NOTE_1}
wait 1200
move 1200 800
shot note-new
click {NOTE_BODY}
wait 300
type 黑暗森林法则
key enter
type 第二段 /
wait 400
shot note-slash
key escape
key backspace
wait 1500
click {PANEL_CLOSE}
wait 600
move 1200 800
shot cell-title
"""


def run(name, commands, library):
    folder = work / name
    script = work / f"{name}.txt"
    script.write_text(f"resize 1300 850\nwait 1500\nmove 1200 800\n{commands}\nwait 600\nmove 1200 800\nshot end\n")
    subprocess.run([sys.executable, str(here / "run.py"), str(script), str(library), str(folder)], env=dict(os.environ, FDL_NOBLINK="1"), check=True, timeout=120)
    return folder / "lib"


def cells(lib):
    with sqlite3.connect(lib / "书单.sqlite") as c:
        return c.execute('select "书名", "读书笔记" from "书单" order by _order').fetchall()


def notes(lib):
    """The documents of the library, by their paths from its top."""
    return sorted(str(p.relative_to(lib)) for p in lib.rglob("*.xml") if ".quiet" not in p.parts)


def trashed(lib):
    """The titles of the documents in the 回收站 (kept there under their ids)."""
    return sorted(p.read_text().split("</title>")[0].removeprefix("<title>") for p in (lib / ".quiet" / "trash").rglob("*.xml"))


# 1. a column, a note made for an empty cell in 「书单 · 笔记」 beside the table, written in, saved
lib = run("write", OPEN + ADD_COLUMN + WRITE, source)
assert cells(lib) == [("三体", "书单 · 笔记/三体.xml"), ("球状闪电", None), (None, None)], cells(lib)
assert notes(lib) == ["书单 · 笔记/三体.xml", "已有的笔记.xml"], notes(lib)
assert (lib / "书单 · 笔记" / "三体.xml").read_text() == "<title>三体</title>\n<p>黑暗森林法则</p>\n<p>第二段 </p>\n", (lib / "书单 · 笔记" / "三体.xml").read_text()
with sqlite3.connect(lib / "书单.sqlite") as c:
    assert c.execute("select type from _fields where name = '读书笔记'").fetchall() == [("note",)]

# 2. reopened: the cell shows the note, the search finds its words
again = run("reopen", "click 62 200\nwait 300\nmove 1200 800\nshot reopened-at-once\n" + "click 62 200\nwait 1200\n" + """move 1200 800
shot reopened
click 1240 24
wait 600
type 黑暗森林
wait 1000
shot search
key escape
""", lib)
assert cells(again) == cells(lib) and notes(again) == notes(lib)

# 3. undo empties the cell and the note goes to the 回收站; redo brings both back
undone = run("undo", OPEN + ADD_COLUMN + WRITE + """click 400 500
key cmd-z
wait 900
""", source)
assert cells(undone)[0] == ("三体", None), cells(undone)
assert notes(undone) == ["已有的笔记.xml"] and trashed(undone) == ["三体"], (notes(undone), trashed(undone))
redone = run("redo", OPEN + ADD_COLUMN + WRITE + """click 400 500
key cmd-z
wait 900
key cmd-shift-z
wait 1000
""", source)
assert cells(redone)[0] == ("三体", "书单 · 笔记/三体.xml") and notes(redone) == ["书单 · 笔记/三体.xml", "已有的笔记.xml"] and trashed(redone) == [], (cells(redone), notes(redone), trashed(redone))

# 4. deleting the record asks about its note; both go, and undo restores both
DELETE = f"""click 400 500
wait 200
rclick {NOTE_1}
wait 400
shot cell-menu
click 908 393
wait 500
shot ask
click 797 464
wait 900
"""
gone = run("delete", OPEN + ADD_COLUMN + WRITE + DELETE, source)
assert [r[0] for r in cells(gone)] == ["球状闪电", None], cells(gone)
assert notes(gone) == ["已有的笔记.xml"] and trashed(gone) == ["三体"], (notes(gone), trashed(gone))
back = run("undelete", OPEN + ADD_COLUMN + WRITE + DELETE + """key cmd-z
wait 1200
""", source)
assert cells(back)[0] == ("三体", "书单 · 笔记/三体.xml") and notes(back) == ["书单 · 笔记/三体.xml", "已有的笔记.xml"] and trashed(back) == [], (cells(back), notes(back), trashed(back))

# 5. a cell pointed at a note there already is; a new title renames the file and the cell follows; 整页打开
more = run("point", OPEN + ADD_COLUMN + WRITE + f"""click {NOTE_2}
wait 200
rclick {NOTE_2}
wait 400
click 921 297
wait 500
shot pick
click 913 298
wait 500
move 1200 800
shot pointed
dclick {NOTE_1}
wait 1000
click {NOTE_TITLE}
wait 200
key end
type  II
wait 1800
move 1200 800
shot renamed
click {NOTE_FULL}
wait 1200
move 1200 800
shot full-page
""", source)
assert cells(more) == [("三体", "书单 · 笔记/三体 II.xml"), ("球状闪电", "已有的笔记.xml"), (None, None)], cells(more)
assert notes(more) == ["书单 · 笔记/三体 II.xml", "已有的笔记.xml"], notes(more)
assert (more / "书单 · 笔记" / "三体 II.xml").read_text().startswith("<title>三体 II</title>\n<p>黑暗森林法则</p>")
assert (more / "已有的笔记.xml").read_text() == "<title>已有的笔记</title>\n<p>一段正文</p>\n"

# 6. the note renamed in the sidebar while the table is open: the cell follows, and the undo of another
#    edit (the table's own ↶), then of the cell following, leaves the note where it is
kept = run("rename", OPEN + ADD_COLUMN + WRITE + f"""move 48 169
wait 700
click 48 169
wait 1000
rclick 100 200
wait 700
click 157 484
wait 500
key cmd-a
type 黑暗森林
key enter
wait 2800
move 1200 800
shot sidebar-renamed
click 634 177
wait 200
type 大刘
key enter
wait 400
click {UNDO}
wait 1200
move 1200 800
shot undone-once
click {UNDO}
wait 2800
""", source)
assert notes(kept) == ["书单 · 笔记/黑暗森林.xml", "已有的笔记.xml"] and trashed(kept) == [], (notes(kept), trashed(kept))
assert cells(kept)[0] == ("三体", "书单 · 笔记/黑暗森林.xml"), cells(kept)
with sqlite3.connect(kept / "书单.sqlite") as c:
    assert c.execute('select "作者" from "书单" order by _order').fetchall()[1] == ("刘慈欣",)

shots = os.environ.get("TABLE_NOTE_SHOTS")
if shots:
    Path(shots).mkdir(parents=True, exist_ok=True)
    for case in ("write", "reopen", "delete", "point", "rename"):
        for png in (work / case).glob("*.png"):
            if png.name != "end.png": shutil.copy(png, Path(shots) / f"{case}-{png.name}")
print("table notes ok:", work)
