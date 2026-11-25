"""The 新建 menu of a folder's "+" (its page's header) keeps its rows while it stays open.

Every 新建 menu ends with the 导入 rows. They once depended on how many
top-level folders there were and on where the "+" sat, worked out again every
frame, so a refresh behind the open menu could take them away under the
pointer. Here the menu is opened, a file appears in the library while it is
open (the background check lists it), and the menu must look the same seconds
later.

Build with --release --features devtools, then run this file. Needs Pillow.
FDL_BIN selects another build. Only a scratch library is used.
"""
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

from PIL import Image

here = Path(__file__).resolve().parent
work = Path(tempfile.mkdtemp(prefix="quiet-create-menu-"))
source = work / "source"
(source / "Inbox").mkdir(parents=True)
(source / "Inbox" / "Start.md").write_text("# Start\n\nstart\n")
run_dir = work / "run"
script = work / "script.txt"
script.write_text("""move 900 600
wait 2500
row Inbox
wait 800
anchor create-folder
wait 300
move 900 600
wait 600
shot open
wait 6000
shot later
""")

app = subprocess.Popen([sys.executable, str(here / "run.py"), str(script), str(source), str(run_dir)])
# run.py copies the library to run/lib; once the open menu is shot, a new document shows up in it.
deadline = time.time() + 60
while not (run_dir / "open.png").exists():
    assert time.time() < deadline and app.poll() is None, f"no open.png; see {run_dir / 'log'}"
    time.sleep(0.1)
(run_dir / "lib" / "Inbox" / "Added while the menu is open.md").write_text("# Added while the menu is open\n")
assert app.wait() == 0, f"the run failed; see {run_dir / 'log'}"

# The menu alone (it hangs under the header's "+", right-aligned, from window point 1032 across and
# 44 down), at 2x: inside its rounded edge and shadow, where a card arriving under it shows through.
box = (1510, 110, 2040, 680)
open_, later = (Image.open(run_dir / f"{n}.png").convert("RGB").crop(box) for n in ("open", "later"))
assert open_.tobytes() == later.tobytes(), f"the 新建 menu changed while open: {run_dir}/open.png vs later.png"
# The new document reached the sidebar while the menu stayed open (the refresh did happen).
assert (run_dir / "lib" / "Inbox" / "Added while the menu is open.md").exists()
print(f"create menu: stable while open ({run_dir})")
