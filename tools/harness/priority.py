"""Drive the priority-fix scenarios in a disposable library (FDL_BIN: devtools build)."""
from pathlib import Path
import json
import os
import subprocess
import sys
import tempfile

here = Path(__file__).resolve().parent
root = Path(tempfile.mkdtemp(prefix="quiet-priority-"))
source = root / "source"
(source / "docs").mkdir(parents=True)
uid = "aaaaaaaa-0000-0000-0000-000000000001"
(source / "docs" / f"{uid}.xml").write_text("<title>Menu fixture</title>\n<p>Text</p>\n")
(source / "index.json").write_text(json.dumps({uid: {"created": 1, "modified": 1, "opened": 1}}))
(source / "preferences.json").write_text(json.dumps({"language":"fr","quick_hotkey":False,"auto_update":False}))
(source / "settings.json").write_text(json.dumps({"window_origin":[-4000,-4000],"window_size":[1100,750]}))
work = root / "run"
(work / "中文").mkdir(parents=True)
(work / "中文/Other Note.md").write_text("# Heading\n\nTarget\n")
(work / "Source.md").write_text("START [[中文/Other Note#Heading|alias]] and [other](中文/Other%20Note.md#Heading)\n\noutside-one\n")
for i in range(600):
    dest = work / "import-me" / f"d{i % 253}" / f"import-note-{i}.md"
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text("paragraph " * 100)
script = root / "steps"
script.write_text("""wait 750
check-priority frame
wait 500
shot menu
check-priority menu
wait 300
check-priority wiki
wait 300
check-priority target
check-priority relative
wait 300
check-priority target
check-priority refresh
wait 3000
check-priority refreshed
check-priority mirror
wait 300
key cmd-f
type START
key escape
key left
type MARK
wait 1000
check-priority external
key cmd-s
wait 500
check-priority local-won
wait 3000
check-priority disk-won
type AFTER
wait 1000
check-priority start-import
wait 10000
check-priority import
""")
subprocess.run([sys.executable, str(here / "run.py"), str(script), str(source), str(work)],
               env=dict(os.environ, FDL_NOPARK="1"), check=True, timeout=90)
log = (work / "log").read_text(errors="replace")
print(work, log[-2500:], flush=True)
assert "priority import: PASS" in log
