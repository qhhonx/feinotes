"""A 画板 written by another program while it is open (src/devtools/board_reload.rs), in a
disposable library: a shape added outside shows within the 2 s poll with the viewport kept,
a half-written file leaves the board as it was until it is whole, and a change outside
while the board is edited here is not written over (保存失败, then 重新载入). FDL_BIN: a
devtools build."""
from pathlib import Path
import json
import os
import subprocess
import sys
import tempfile

here = Path(__file__).resolve().parent
root = Path(tempfile.mkdtemp(prefix="quiet-board-reload-"))
source = root / "source"
(source / "docs").mkdir(parents=True)
uid = "aaaaaaaa-0000-0000-0000-000000000001"
(source / "docs" / f"{uid}.xml").write_text("<title>Fixture</title>\n<p>Text</p>\n")
(source / "index.json").write_text(json.dumps({uid: {"created": 1, "modified": 1, "opened": 1}}))
(source / "preferences.json").write_text(json.dumps({"quick_hotkey": False, "auto_update": False}))
(source / "settings.json").write_text(json.dumps({"window_size": [1100, 750]}))
work = root / "run"
script = root / "steps"
script.write_text("""wait 750
check-board-reload open
wait 500
check-board-reload opened
wait 3500
shot added
check-board-reload added
wait 3500
check-board-reload half
wait 3500
check-board-reload whole
wait 4000
shot conflict
check-board-reload conflict
wait 500
check-board-reload reloaded
quit
""")
subprocess.run([sys.executable, str(here / "run.py"), str(script), str(source), str(work)], check=True, timeout=120)
log = (work / "log").read_text(errors="replace")
print(work, log[-1500:], flush=True)
assert "board-reload reloaded: PASS" in log
