"""Content-aware Markdown table widths, through the native paste/import paths.

Build with --release --features devtools, then run this file. Needs Pillow.
FDL_BIN selects another build. Only a fake clipboard and scratch libraries are used.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

from PIL import Image

here = Path(__file__).resolve().parent
work = Path(tempfile.mkdtemp(prefix="quiet-table-paste-"))
source = work / "source"
(source / "docs").mkdir(parents=True)
doc_id = "aaaaaaaa-0000-0000-0000-000000000001"
(source / "index.json").write_text(json.dumps({doc_id: {"created": 1, "modified": 1, "opened": 1}}))
(source / "preferences.json").write_text(json.dumps({
    "appearance": "light", "english": True, "seen_version": "0.1.10",
    "auto_update": False, "quick_hotkey": False,
}))
original = "<title>Markdown table widths</title>\n<p>PASTE HERE</p>\n"
(source / "docs" / f"{doc_id}.xml").write_text(original)
markdown = """| ID | Status | Description |
| ---: | :---: | --- |
| 1 | OK | A longer description should get more room than a short identifier or status. |
| 2 | Done | 中文内容也应得到足够的空间，避免每一行只有几个字。 |
"""
clipboard = work / "table.md"
clipboard.write_text(markdown)


def run(name, commands, library=source):
    folder = work / name
    script = work / f"{name}.txt"
    script.write_text(f"""resize 1200 850
wait 1200
move 1100 700
{commands}
wait 800
move 1100 700
shot result
""")
    subprocess.run([sys.executable, str(here / "run.py"), str(script), str(library), str(folder)],
                   env=dict(os.environ, FDL_NOBLINK="1"), check=True, timeout=60)
    return folder


def visible_files(folder, suffix):
    library = folder / "lib"
    return [p for p in library.rglob("*" + suffix) if not any(c.startswith(".") for c in p.relative_to(library).parts)]


def tables(folder):
    result = []
    for path in visible_files(folder, ".xml"):
        doc = ET.fromstring("<doc>" + path.read_text() + "</doc>")
        result.extend(doc.findall("table"))
    return result


def verify(folder, expected=None):
    found = tables(folder)
    assert len(found) == 1, f"expected one table; see {folder}"
    table = found[0]
    widths = [int(c.get("width")) for c in table.findall("colgroup/col")]
    assert len(widths) == 3 and 50 <= widths[0] < widths[1] < 200 < widths[2] <= 480, widths
    assert sum(widths) <= 720, widths
    text = "".join(table.itertext())
    assert "A longer description" in text and "中文内容" in text, text
    if expected is not None:
        assert ET.canonicalize(ET.tostring(table, encoding="unicode"), strip_text=True) == expected
    print(f"{folder.name}: saved widths {widths}", flush=True)
    return ET.canonicalize(ET.tostring(table, encoding="unicode"), strip_text=True)


print(f"Artifacts: {work}", flush=True)
paste = f"""key cmd-f
type PASTE HERE
key escape
key backspace
wait 200
clip text {clipboard}
key cmd-v
wait 500
"""
pasted = run("paste", paste)
expected = verify(pasted)
verify(run("reopen", "", pasted / "lib"), expected)
undone = run("undo", paste + "key cmd-z")
assert not tables(undone), f"undo left a table; see {undone}"
doc = ET.fromstring("<doc>" + visible_files(undone, ".xml")[0].read_text() + "</doc>")
assert all(not "".join(p.itertext()) for p in doc.findall("p"))
print("undo: restored empty paste target", flush=True)
verify(run("redo", paste + "key cmd-z\nwait 200\nkey cmd-shift-z"), expected)
imported = run("import", f"pick {clipboard}\nkey cmd-shift-i")
files = visible_files(imported, ".md")
assert len(files) == 1 and files[0].read_bytes() == clipboard.read_bytes()
# Markdown files store text, not column widths. Check the rendered grid's
# vertical borders in a blank strip below the first data row's text.
im = Image.open(imported / "result.png").convert("RGB")
scale = im.width / 1200
runs = []
for x in range(round(330 * scale), round(1000 * scale)):
    if min(im.getpixel((x, round(265 * scale)))) < 250:
        if runs and runs[-1][-1] == x - 1:
            runs[-1].append(x)
        else:
            runs.append([x])
assert len(runs) == 4, f"expected four column edges; see {imported}"
edges = [runs[0][-1] / scale] + [r[0] / scale for r in runs[1:]]
assert all(abs(edges[i + 1] - edges[i] - w) <= 2 for i, w in enumerate([50, 72, 480])), edges
print("import: rendered content-aware widths", flush=True)

resized = run("resize", "move 396 230\nwait 200\ndrag 396 230 426 230", pasted / "lib")
assert [c.get("width") for c in tables(resized)[0].findall("colgroup/col")] == ["80", "72", "480"]
reopened = run("resize_reopen", "", resized / "lib")
assert [c.get("width") for c in tables(reopened)[0].findall("colgroup/col")] == ["80", "72", "480"]
print("resize: manual width persisted after reopening", flush=True)

# An explicitly sized table remains unchanged by save/reopen.
explicit = source / "docs" / f"{doc_id}.xml"
explicit.write_text('<title>Explicit widths</title>\n<table><colgroup><col width="85"/><col width="420"/></colgroup>'
                    '<tbody><tr><td><p>Short</p></td><td><p>Long content stays at its saved width.</p></td></tr></tbody></table>\n')
folder = run("explicit", "")
assert [c.get("width") for c in tables(folder)[0].findall("colgroup/col")] == ["85", "420"]
print("explicit: preserved saved widths", flush=True)
