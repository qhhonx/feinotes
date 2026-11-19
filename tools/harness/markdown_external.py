"""External Markdown saves defer during IME and refresh a clean file editor.

Use a devtools build through FDL_BIN. All input, files and captures stay in a
fresh temporary file library. No system clipboard or real library is used.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
from PIL import Image, ImageChops

repo = Path(__file__).resolve().parents[2]
binary = Path(os.environ.get("FDL_BIN", repo / "target/release/quiet_md"))
work = Path(tempfile.mkdtemp(prefix="quiet-markdown-external-", dir="/private/tmp"))


def run(name, steps, barrier, after_barrier):
    root = work / name
    internal = root / ".quiet"
    internal.mkdir(parents=True)
    (internal / "format.json").write_text('{"format":4}')
    (internal / "preferences.json").write_text(json.dumps({"english": True, "auto_update": False, "quick_hotkey": False}))
    note = root / "Note.md"
    note.write_text("ORIGINAL\n")
    script = root / "steps.txt"
    script.write_text("resize 1200 850\nwait 1000\nmove 1100 800\n" + steps + "\nquit\n")
    env = {"HOME": os.path.expanduser("~"), "PATH": "/usr/bin:/bin", "QUIET_MD_HOME": str(root),
           "FDL_SCRIPT": str(script), "FDL_SHOTS": str(root), "FDL_NOBLINK": "1"}
    with (root / "log").open("w") as log:
        process = subprocess.Popen([binary], cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            def wait_for(image):
                deadline = time.monotonic() + 20
                while not (root / image).exists():
                    assert process.poll() is None and time.monotonic() < deadline, str(root / "log")
                    time.sleep(0.05)
            wait_for(barrier)
            note.write_text("EXTERNAL\n")
            after_barrier(root, note, wait_for)
            assert process.wait(timeout=30) == 0, str(root / "log")
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=10)
    return note.read_text()


def composing(root, note, wait_for):
    wait_for("deferred.png")
    assert note.read_text() == "EXTERNAL\n", "Uncommitted IME wrote to disk"
    before = Image.open(root / "composing.png").convert("RGB")
    after = Image.open(root / "deferred.png").convert("RGB")
    assert ImageChops.difference(before, after).getbbox() is None, "External refresh replaced composing text"


saved = run("ime", "key cmd-down\nkey enter\nmark 拼\nshot composing\nwait 3500\nshot deferred\nwait 1000\ncommit 拼\ntype LOCAL\nwait 1800", "composing.png", composing)
assert "ORIGINAL" in saved and "拼LOCAL" in saved and "EXTERNAL" not in saved, saved
print("external_markdown_ime_deferral: passed", flush=True)
saved = run("clean", "shot clean\nwait 3500\nshot refreshed\nkey cmd-down\nkey enter\ntype AFTERREFRESH\nwait 1800", "clean.png", lambda *_: None)
assert "EXTERNAL" in saved and "AFTERREFRESH" in saved and "ORIGINAL" not in saved, saved
print("external_markdown_clean_refresh: passed", flush=True)
print("Artifacts:", work, flush=True)
