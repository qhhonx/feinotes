"""Isolated SQLite board editing, image ownership, reopen and read-only regression.

Build with cloudkit-sync,devtools. FDL_CLOUD_BIN and FDL_CLOUD_DYLD_PATH
select the test binary and bridge. Uses synthetic data and the fake clipboard.
"""
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import xml.etree.ElementTree as ET

from PIL import Image

repo = Path(__file__).resolve().parents[2]
binary = Path(os.environ.get("FDL_CLOUD_BIN", repo / "target/debug/quiet_md"))
work = Path(tempfile.mkdtemp(prefix="quiet-sync-qa-board-", dir="/private/tmp"))
(work / "use-local-library").touch()
database = work / "local-only/library.sqlite3"
image = work / "synthetic.png"
Image.new("RGBA", (40, 30), (40, 120, 220, 255)).save(image)
env = dict(os.environ, QUIET_SYNC_QA_ROOT=str(work), QUIET_SYNC_QA_LOCAL_FIRST="1",
           FDL_SHOTS=str(work), FDL_NOBLINK="1",
           DYLD_LIBRARY_PATH=os.environ.get("FDL_CLOUD_DYLD_PATH", str(binary.parent)))
print(f"Artifacts: {work}", flush=True)


def run(name, steps):
    script = work / (name + ".txt")
    script.write_text("resize 1200 850\nwait 1200\nmove 1100 800\n" + steps + "\nquit\n")
    with (work / (name + ".log")).open("w") as log:
        subprocess.run([binary], cwd=work, env=dict(env, FDL_SCRIPT=str(script)),
                       stdout=log, stderr=subprocess.STDOUT, check=True, timeout=40)


def scene():
    with sqlite3.connect(database) as db:
        rows = db.execute("SELECT payload FROM library_blocks ORDER BY document, position").fetchall()
    for (payload,) in rows:
        root = ET.fromstring(json.loads(payload)["xml"])
        if root.tag == "whiteboard":
            return json.loads(root.text), payload
    raise AssertionError("No persisted board")


run("edit", f"""key cmd-n
type SQLite board QA
key enter
type /hb
key enter
wait 900
key r
drag 600 250 800 360
key t
click 500 450
type BOARD_LABEL
key cmd-enter
wait 300
move 750 550
clip image {image}
key cmd-v
wait 1200
shot board-edited""")
board, saved = scene()
assert any(e["type"] == "rectangle" for e in board["elements"]), board
assert any("BOARD_LABEL" in e.get("text", "") for e in board["elements"]), board
assert board["files"], board
for file in board["files"].values():
    assert file["path"].startswith("assets/"), file
    owned = work / "local-only" / file["path"]
    assert owned.is_file(), owned
    assert Image.open(owned).convert("RGBA").tobytes() == Image.open(image).tobytes()
with sqlite3.connect(database) as db:
    for note, raw in db.execute("SELECT id, meta FROM library_entries").fetchall():
        meta = json.loads(raw)
        meta["reading"] = True
        meta["kind"] = "board"
        db.execute("UPDATE library_entries SET meta = ? WHERE id = ?", (json.dumps(meta), note))
run("readonly-reopen", "dclick 500 300\nkey r\ndrag 500 300 700 450\nwait 900\nshot board-readonly")
assert scene()[1] == saved, "Read-only board changed after reopening"
print("SQLite board scene, text, owned image, reopen and read-only checks passed", flush=True)
