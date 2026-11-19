"""#234: the main window returns to the document read last, or the home page.

Build with --release --features devtools, then run this file. Two documents
with faked `opened` stamps; whichever the app opens gets `opened` refreshed,
so the index tells the story without reading pixels. The document modified
most recently must not win: `opened` picks the document read last.
No system clipboard or user library is accessed.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import uuid

here = Path(__file__).resolve().parent
work = Path(tempfile.mkdtemp(prefix="quiet-start-view-"))
source = work / "source"
(source / "docs").mkdir(parents=True)

A, B = str(uuid.uuid4()), str(uuid.uuid4())
(source / "docs" / f"{A}.xml").write_text("<title>Doc A</title>\n<p>AAAA</p>\n")
(source / "docs" / f"{B}.xml").write_text("<title>Doc B</title>\n<p>BBBB</p>\n")


def index():
    return {id: json.loads(v) for id, v in {
        A: '{"created":1,"modified":900,"opened":100}',
        B: '{"created":1,"modified":10,"opened":200}',
    }.items()}


def run(start_home, name):
    (source / "index.json").write_text(json.dumps(index()))
    (source / "preferences.json").write_text(json.dumps({
        "appearance": "light", "english": True, "seen_version": "0.1.9",
        "auto_update": False, "quick_hotkey": False, "start_home": start_home,
    }))
    folder = work / name
    script = work / f"{name}.txt"
    script.write_text("resize 1200 850\nwait 1500\nmove 1100 700\nshot start\n")
    local_cache = Path(tempfile.gettempdir()) / "quiet-md-local/libraries"
    previous = set(local_cache.glob("*.json"))
    subprocess.run([sys.executable, str(here / "run.py"), str(script), str(source), str(folder)],
                   env=dict(os.environ, FDL_NOBLINK="1"), check=True, timeout=60)
    lib = folder / "lib"
    index_file = lib / ".quiet/index.json"
    if not index_file.exists(): index_file = lib / "index.json"
    entries = json.loads(index_file.read_text())
    records = {meta.get("id", key): meta for key, meta in entries.items()}
    states = [json.loads(path.read_text()).get("opened", {}) for path in set(local_cache.glob("*.json")) - previous]
    opened = next(state for state in states if A in state and B in state)
    for id, value in opened.items():
        if id in records: records[id]["opened"] = value
    return records, folder / "start.png"


lib, _ = run(False, "last")
assert lib[B]["opened"] > 1_000, f"B.opened was not refreshed: {lib}"
assert lib[A]["opened"] == 100, f"the modified-most document won over the read-last one: {lib}"
print("last_document: passed", flush=True)

lib, _ = run(True, "home")
assert lib[A]["opened"] == 100 and lib[B]["opened"] == 200, f"something opened on start-home: {lib}"
print("start_home: passed", flush=True)
print(f"start_view: passed; artifacts: {work}")
