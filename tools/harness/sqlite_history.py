"""Fei Notes SQLite startup, IME and local-history UI regression.

Build with cargo build --features cloudkit-sync,devtools. FDL_CLOUD_BIN selects
another binary; FDL_CLOUD_DYLD_PATH selects its Swift bridge directory. Uses a
fresh local-only QA root, offscreen windows and synthetic text, never CloudKit.
"""
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import time

repo = Path(__file__).resolve().parents[2]
binary = Path(os.environ.get("FDL_CLOUD_BIN", repo / "target/debug/quiet_md"))
work = Path(tempfile.mkdtemp(prefix="quiet-sync-qa-history-", dir="/private/tmp"))
(work / "use-local-library").touch()
database = work / "local-only/library.sqlite3"
env = {"HOME": os.path.expanduser("~"), "PATH": "/usr/bin:/bin",
       "QUIET_SYNC_QA_ROOT": str(work), "QUIET_SYNC_QA_LOCAL_FIRST": "1",
       "FDL_SHOTS": str(work), "FDL_NOBLINK": "1",
       "DYLD_LIBRARY_PATH": os.environ.get("FDL_CLOUD_DYLD_PATH", str(binary.parent))}
print(f"Artifacts: {work}", flush=True)


def rows(query):
    with sqlite3.connect(database) as db:
        return db.execute(query).fetchall()


def body():
    return str(rows("SELECT payload FROM library_blocks"))


def run(name, steps, composing=False):
    script = work / (name + ".txt")
    script.write_text("resize 1200 850\nwait 1200\nmove 1100 800\n" + steps + "\nquit\n")
    with (work / (name + ".log")).open("w") as log:
        process = subprocess.Popen([binary], cwd=work, env=dict(env, FDL_SCRIPT=str(script)), stdout=log, stderr=subprocess.STDOUT)
        try:
            if composing:
                deadline = time.monotonic() + 20
                while not (work / "composing.png").exists():
                    assert process.poll() is None and time.monotonic() < deadline, "IME barrier was not reached"
                    time.sleep(0.05)
                saved = body()
                assert "BODY_ORIGINAL" in saved and "拼" not in saved, saved
            assert process.wait(timeout=30) == 0, f"See {work / (name + '.log')}"
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()


run("create-ime", """key cmd-n
wait 300
type Cloud UI synthetic
key enter
type BODY_ORIGINAL
wait 1000
key cmd-f
type BODY_ORIGINAL
key escape
key right
mark 拼
wait 800
shot composing
wait 1200
commit 拼音
wait 1000
key cmd-s
wait 300
shot note
""", composing=True)
notes = rows("SELECT id, title FROM library_documents WHERE trashed = 0")
assert len(notes) == 1 and notes[0][1] == "Cloud UI synthetic", notes
assert "BODY_ORIGINAL拼音" in body(), body()
print("create/IME: committed text persisted; marked text stayed out of SQLite", flush=True)

# Seed an independently known snapshot, so preview and restore are checked
# against expected content rather than against the implementation's own output.
history = database.parent / ".quiet/history" / notes[0][0]
history.mkdir(parents=True, exist_ok=True)
(history / f"{int(time.time() * 1000) + 1000}-aabbccdd.xml").write_text(
    "<title>Cloud UI synthetic</title>\n<p>SNAPSHOT_EXPECTED</p>\n")
open_history = "click 1140 24\nwait 250\nclick 1030 444\nwait 500\n"
run("preview", open_history + "shot preview\ntype SHOULD_NOT_SAVE\nwait 700\nkey escape\nwait 300\nshot back\n")
assert "BODY_ORIGINAL拼音" in body() and "SHOULD_NOT_SAVE" not in body(), body()
print("preview: refused input and returned to the current body without writing history", flush=True)
run("restore", open_history + "click 844 68\nwait 900\nshot restored\n")
assert "SNAPSHOT_EXPECTED" in body() and "BODY_ORIGINAL" not in body(), body()
print("restore: selected snapshot persisted in SQLite", flush=True)
