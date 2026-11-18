"""Native column-resize regression. Build with --release --features devtools.

Run with `python3 tools/harness/table_resize.py`; FDL_BIN selects another build.
Needs Pillow. All documents, scripts and screenshots stay in a temporary folder.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

from hlib import doc_file, docs
from PIL import Image, ImageChops

here = Path(__file__).resolve().parent
work = Path(tempfile.mkdtemp(prefix="quiet-table-resize-"))
source = work / "source"
(source / "docs").mkdir(parents=True)
doc_id = "aaaaaaaa-0000-0000-0000-000000000001"
(source / "index.json").write_text(json.dumps({doc_id: {"created": 1, "modified": 1, "opened": 1}}))
(source / "preferences.json").write_text(json.dumps({
    "appearance": "light", "english": True, "seen_version": "0.1.7",
    "auto_update": False, "quick_hotkey": False,
}))
xml = "<title>Table resize regression</title>\n"
xml += "".join(f"<p>Before table paragraph {i:02d}</p>\n" for i in range(20))
xml += '<p>TABLE ANCHOR</p>\n<table><colgroup><col width="240"/><col width="240"/></colgroup><tbody>'
xml += "".join(f"<tr><td><p>Row {i}: text wraps when this column gets narrower.</p></td>"
               f"<td><p>Right {i}</p></td></tr>" for i in range(6))
xml += "</tbody></table>\n"
xml += "".join(f"<p>After table paragraph {i:02d}</p>\n" for i in range(30))
(source / "docs" / f"{doc_id}.xml").write_text(xml)


def shot(folder, name, box):
    im = Image.open(folder / f"{name}.png").convert("RGB")
    sx, sy = im.width / 1200, im.height / 850
    return im.crop(tuple(round(v * (sx if i % 2 == 0 else sy)) for i, v in enumerate(box)))


def run(name, setup, end_x=526, finish="", width=180):
    folder = work / name
    script = work / f"{name}.txt"
    script.write_text(f"""resize 1200 850
wait 1500
move 1100 700
{setup}
move 1100 700
wait 200
move 586 320
wait 200
shot before
down 586 320
wait 100
dragto {end_x} 320
wait 200
shot during
up {end_x} 320
move 1100 700
wait 400
shot after
{finish}
wait 700
""")
    subprocess.run([sys.executable, str(here / "run.py"), str(script), str(source), str(folder)],
                   env=dict(os.environ, FDL_NOBLINK="1"), check=True)
    # Text above the table must stay at exactly the same window coordinates,
    # both while wrapping changes and after the mouse is released.
    box = (350, 80, 820, 275)
    before = shot(folder, "before", box)
    assert before.getextrema()[0][0] < 100, f"{name}: anchor region is empty"
    for frame in ("during", "after"):
        diff = ImageChops.difference(before, shot(folder, frame, box))
        assert diff.getbbox() is None, f"{name}: page moved {frame} resize; see {folder}"
    # Verify that the gesture hit the resizer, persisted only its width, and
    # left all document content intact (also catches no-op / undo regressions).
    saved = ET.fromstring("<doc>" + doc_file(folder / "lib").read_text() + "</doc>")
    original = ET.fromstring("<doc>" + xml + "</doc>")
    widths = [int(c.get("width")) for c in saved.findall(".//col")]
    assert widths == [width, 240], f"{name}: expected {[width, 240]}, saved {widths}; see {folder}"
    saved.find(".//col").set("width", "240")
    assert ET.canonicalize(ET.tostring(saved, encoding="unicode"), strip_text=True) == \
        ET.canonicalize(ET.tostring(original, encoding="unicode"), strip_text=True), f"{name}: document content changed"
    print(f"{name}: stable during drag and on release; saved width {width}")


print(f"Artifacts: {work}", flush=True)
scroll = "wheel 1100 500 700\nwait 400"
run("caret_above", "click 500 100\nwait 200\n" + scroll)
run("caret_below", scroll + "\nclick 450 705\nwait 200")
run("caret_in_table", scroll + "\nclick 450 630\nwait 200")
run("no_change", "click 500 100\nwait 200\n" + scroll, end_x=586, width=240)
run("undo", "click 500 100\nwait 200\n" + scroll, finish="key cmd-z", width=240)
run("redo", "click 500 100\nwait 200\n" + scroll, finish="key cmd-z\nwait 200\nkey cmd-shift-z")
