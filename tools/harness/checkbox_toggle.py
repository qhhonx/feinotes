"""Task clicks preserve the text selection and viewport.

Build with --release --features devtools, then run this file. Needs Pillow.
FDL_BIN selects another build; CHECKBOX_CASES selects comma-separated cases.
All fixtures, scripts and screenshots are generated in a temporary directory.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

from PIL import Image, ImageChops

here = Path(__file__).resolve().parent
work = Path(tempfile.mkdtemp(prefix="quiet-checkbox-"))
doc_id = "aaaaaaaa-0000-0000-0000-000000000001"
marker = "CARET_MARKER"


def run_script(name, source, steps):
    folder = work / name
    script = work / f"{name}.txt"
    script.write_text(f"resize 1200 850\nwait 1500\nmove 1150 820\n{steps}\n")
    subprocess.run([sys.executable, str(here / "run.py"), str(script), str(source), str(folder)],
                   env=dict(os.environ, FDL_NOBLINK="1"), check=True, timeout=60)
    assert (folder / "before.png").exists(), f"No frame captured; see {folder / 'log'}"
    return folder


def checkbox_at(path):
    """Find the single empty task's square outline, independent of theme metrics."""
    image = Image.open(path).convert("RGB")
    sx, sy = image.width / 1200, image.height / 850
    # Exclude the sidebar, title bar and scrollbar. The fixture contains only
    # one checkbox; its square is wider than the individual text glyphs.
    box = (round(330 * sx), round(80 * sy), round(680 * sx), round(780 * sy))
    pixels = image.load()
    dark = {(x, y) for y in range(box[1], box[3]) for x in range(box[0], box[2])
            if max(pixels[x, y]) < 235}
    candidates = []
    while dark:
        start = dark.pop()
        stack, component = [start], [start]
        while stack:
            x, y = stack.pop()
            for dx in (-1, 0, 1):
                for dy in (-1, 0, 1):
                    point = (x + dx, y + dy)
                    if point in dark:
                        dark.remove(point)
                        stack.append(point)
                        component.append(point)
        xs, ys = zip(*component)
        left, top, right, bottom = min(xs), min(ys), max(xs), max(ys)
        width, height = (right - left + 1) / sx, (bottom - top + 1) / sy
        center = ((left + right) // 2, (top + bottom) // 2)
        if 13 <= width <= 20 and 13 <= height <= 20 and abs(width - height) < 2 \
                and min(pixels[center]) > 220:
            candidates.append(((left + right) / (2 * sx), (top + bottom) / (2 * sy)))
    assert len(candidates) == 1, f"Expected one task box, found {candidates}; see {path}"
    return candidates[0]


def unchanged_view(folder, task_y):
    before = Image.open(folder / "before.png").convert("RGB")
    after = Image.open(folder / "after.png").convert("RGB")
    assert before.size == after.size
    sx, sy = before.width / 1200, before.height / 850
    # Compare surrounding paragraphs, clear of the task's container and its
    # floating controls (e.g. a callout's palette can appear after clicking).
    for top, bottom in [(80, task_y - 150), (task_y + 90, 780)]:
        if bottom - top < 30:
            continue
        box = tuple(round(v * (sx if i % 2 == 0 else sy))
                    for i, v in enumerate((380, top, 1000, bottom)))
        a, b = before.crop(box), after.crop(box)
        assert a.getextrema()[0][0] < 150, f"Empty anchor region; see {folder}"
        assert ImageChops.difference(a, b).getbbox() is None, f"Page moved after task click; see {folder}"


def run(name, theme="craft", context="body", selection=False, below=False, finish="", done=True):
    source = work / f"source-{name}"
    (source / "docs").mkdir(parents=True)
    (source / "index.json").write_text(json.dumps({doc_id: {"created": 1, "modified": 1, "opened": 1}}))
    (source / "preferences.json").write_text(json.dumps({
        "theme": theme, "appearance": "light", "english": True, "seen_version": "0.1.10",
        "auto_update": False, "quick_hotkey": False,
    }))
    task = '<checkbox done="false">TASK TARGET</checkbox>'
    if context == "nested":
        task = '<ul><li>Parent task list' + task + '</li></ul>'
    elif context == "callout":
        task = '<callout emoji="💡" background-color="light-blue"><p>Callout label</p>' + task + '</callout>'
    elif context == "table":
        task = '<table><colgroup><col width="480"/></colgroup><tbody><tr><td><p>Cell label</p>' + task + '</td></tr></tbody></table>'
    xml = '<title>Task click regression</title>\n<p>CARET_ANCHOR</p>\n'
    xml += ''.join(f'<p>Before task paragraph {i:02d}</p>\n' for i in range(20))
    xml += task + '\n'
    xml += ''.join(f'<p>After task paragraph {i:02d}</p>\n' for i in range(20))
    xml += '<p>AFTER_ANCHOR</p>\n'
    xml += ''.join(f'<p>Trailing paragraph {i:02d}</p>\n' for i in range(20))
    (source / "docs" / f"{doc_id}.xml").write_text(xml)
    anchor = "AFTER_ANCHOR" if below else "CARET_ANCHOR"
    setup = f"key cmd-f\ntype {anchor}\nkey escape\n"
    if not selection:
        setup += "key left\n"
    setup += f"wheel 1100 500 {'-700' if below else '700'}\nwait 500\nmove 1150 820\nwait 200\nshot before"
    # Calibrate using a disposable run; the second run starts from the same
    # untouched fixture so finding a task never changes the old text caret.
    probe = run_script(name + "-probe", source, setup)
    x, y = checkbox_at(probe / "before.png")
    folder = run_script(name, source, setup + f"\nclick {x} {y}\nmove 1150 820\nwait 600\nshot after\n"
                        + finish + f"\ntype {marker}\nwait 700\nkey cmd-s\nwait 300")
    unchanged_view(folder, y)
    saved = ET.fromstring("<doc>" + (folder / "lib/docs" / f"{doc_id}.xml").read_text() + "</doc>")
    expected = ET.fromstring("<doc>" + xml + "</doc>")
    expected.find('.//checkbox').set('done', 'true' if done else 'false')
    for paragraph in expected.findall('p'):
        if paragraph.text == anchor:
            paragraph.text = marker if selection else marker + anchor
    assert ET.canonicalize(ET.tostring(saved, encoding="unicode"), strip_text=True) == \
        ET.canonicalize(ET.tostring(expected, encoding="unicode"), strip_text=True), \
        f"Task state, caret/selection, undo history or document content changed unexpectedly; see {folder}"
    print(f"{name}: task persisted; viewport and {'selection' if selection else 'caret'} preserved", flush=True)


cases = {
    'body_craft': {},
    'body_feishu': {'theme': 'feishu'},
    'nested': {'context': 'nested'},
    'callout': {'context': 'callout'},
    'table': {'context': 'table'},
    'caret_below': {'below': True},
    'selection': {'selection': True},
    'undo': {'finish': 'key cmd-z\nwait 200', 'done': False},
    'redo': {'finish': 'key cmd-z\nwait 200\nkey cmd-shift-z\nwait 200'},
}
print(f"Artifacts: {work}", flush=True)
for case in os.environ.get('CHECKBOX_CASES', ','.join(cases)).split(','):
    run(case, **cases[case])
