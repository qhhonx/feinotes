"""An attachment Quiet.md edits itself opens in a window of its own (scratch library).

A Markdown file and a PDF are inserted into a page as attachments (视频或文件). Then:
the Markdown card, clicked, opens in a second window that edits the attachment's own
file; saving there changes that file and the size on the card in the first window; the
card clicked again opens no third window; its eye still asks for Quick Look; the PDF
still asks for Quick Look and opens no window; with the second window closed, a double
click on the card opens it again. Quick Look is only logged in a scripted run.

Build with --release --features devtools. Needs Pillow. FDL_BIN selects another build.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from PIL import Image, ImageChops

here = Path(__file__).resolve().parent
work = Path(tempfile.mkdtemp(prefix="quiet-attachment-window-"))
doc_id = "aaaaaaaa-0000-0000-0000-000000000001"
W, H = 1200, 850
# The Markdown card (its name and size, its eye), the PDF's preview under it, the line above them.
CARD, EYE, PDF, AWAY = "480 248", "708 248", "600 520", "800 192"

source = work / "source"
(source / "docs").mkdir(parents=True)
(source / "index.json").write_text(json.dumps({doc_id: {"created": 1, "modified": 1, "opened": 1}}))
(source / "preferences.json").write_text(json.dumps({
    "theme": "craft", "appearance": "light", "english": True, "seen_version": "0.1.10",
    "auto_update": False, "quick_hotkey": False, "remote_images": False,
}))
(source / "docs" / (doc_id + ".xml")).write_text("<title>Attachments</title>\n<p>BODY</p>\n")
files = work / "files"
files.mkdir()
notes = "# Meeting notes\n\nFirst line.\n"
(files / "meeting-notes.md").write_text(notes)
(files / "report.pdf").write_bytes(
    b"%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n"
    b"3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF\n")

steps = f"""resize {W} {H}
wait 1500
move 1150 820
# the caret after BODY, then both attachments on lines of their own
key cmd-f
type BODY
key escape
key right
key enter
pick {files / 'meeting-notes.md'}
type /file
wait 400
key enter
wait 1200
pick {files / 'report.pdf'}
type /file
wait 400
key enter
wait 2500
key cmd-s
wait 500
click {AWAY}
move 1150 820
wait 400
shot card
windows 1
# the first click picks the card, the second opens it
click {CARD}
wait 700
click {CARD}
wait 1500
windows 2
window 1
wait 800
showing meeting-notes
shot window
key cmd-f
type First line.
key escape
key right
type  EDITED-IN-QUIET
wait 300
key cmd-s
wait 800
shot window-edited
window 0
wait 800
click {AWAY}
move 1150 820
wait 400
shot card-after
# open already: no third window
wait 700
click {CARD}
wait 700
click {CARD}
wait 1000
windows 2
# the eye is Quick Look as before
click {EYE}
wait 500
windows 2
# a PDF is not ours to edit: Quick Look, no window
click {AWAY}
wait 700
click {PDF}
wait 700
click {PDF}
wait 800
windows 2
# closed, then a double click
window 1
key cmd-w
wait 800
windows 1
window 0
click {AWAY}
wait 700
dclick {CARD}
wait 1500
windows 2
window 1
wait 500
showing meeting-notes
quit
"""
script = work / "steps.txt"
script.write_text(steps)
run = work / "run"
print(f"Artifacts: {work}", flush=True)
subprocess.run([sys.executable, str(here / "run.py"), str(script), str(source), str(run)],
               env=dict(os.environ, FDL_NOBLINK="1"), check=True, timeout=120)
log = (run / "log").read_text(errors="replace")

# The window edited the attachment where it is; nothing else of that name was made.
found = [p for p in (run / "lib").rglob("meeting-notes*") if ".quiet" not in p.parts]
assert len(found) == 1 and ".assets" in found[0].parts, f"expected the attachment only, found {found}"
edited = found[0].read_text()
assert edited == notes.replace("First line.", "First line. EDITED-IN-QUIET"), f"attachment not edited in place: {edited!r}"
assert (files / "meeting-notes.md").read_text() == notes, "the file the attachment was copied from changed"

# The card's size line follows the save; the rest of the card stays.
def crop(name, box):
    im = Image.open(run / (name + ".png")).convert("RGB")
    sx, sy = im.width / W, im.height / H
    return im.crop((round(box[0] * sx), round(box[1] * sy), round(box[2] * sx), round(box[3] * sy)))
size_line, name_line = (410, 250, 560, 268), (410, 230, 560, 249)
assert ImageChops.difference(crop("card", size_line), crop("card-after", size_line)).getbbox(), f"card size not refreshed; see {run}"
assert ImageChops.difference(crop("card", name_line), crop("card-after", name_line)).getbbox() is None, f"card moved; see {run}"

# Quick Look was asked for the eye and the PDF only (scripted runs log it instead of opening it).
looks = [line.split("/")[-1] for line in log.splitlines() if line.startswith("quicklook ")]
assert looks == ["meeting-notes.md", "report.pdf"], f"Quick Look asked for {looks}; see {run / 'log'}"

# Nothing of this run is left running.
left = subprocess.run(["pgrep", "-fl", "quiet_md"], capture_output=True, text=True).stdout
assert str(run / "lib") not in left and str(script) not in left, f"still running: {left}"
print(f"ok: {run / 'card.png'} {run / 'card-after.png'} {run / 'window.png'} {run / 'window-edited.png'}")
