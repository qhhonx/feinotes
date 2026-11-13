"""Pointer state changes preserve the viewport; text navigation still reveals the caret.

Build with --release --features devtools, then run this file. Needs Pillow.
FDL_BIN selects another build; SCROLL_CASES selects comma-separated cases.
All documents, scripts and screenshots stay in a temporary directory.
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
work = Path(tempfile.mkdtemp(prefix="quiet-scroll-intent-"))
doc_id = "aaaaaaaa-0000-0000-0000-000000000001"
marker = "CARET_MARKER"


def fixture(name, target, theme="craft"):
    source = work / (name + "-source")
    (source / "docs").mkdir(parents=True)
    (source / "index.json").write_text(json.dumps({doc_id: {"created": 1, "modified": 1, "opened": 1}}))
    (source / "preferences.json").write_text(json.dumps({
        "theme": theme, "appearance": "light", "english": True, "seen_version": "0.1.10",
        "auto_update": False, "quick_hotkey": False,
    }))
    xml = '<title>Scroll intent regression</title>\n<p>CARET_ANCHOR</p>\n'
    xml += ''.join(f'<p>Before target paragraph {i:02d}</p>\n' for i in range(20))
    xml += target + '\n'
    xml += ''.join(f'<p>After target paragraph {i:02d}</p>\n' for i in range(40))
    (source / "docs" / f"{doc_id}.xml").write_text(xml)
    return source, ET.fromstring("<doc>" + xml + "</doc>")


def run_script(name, source, steps):
    folder = work / name
    script = work / (name + ".txt")
    script.write_text("resize 1200 850\nwait 1500\nmove 1150 820\n" + steps + "\n")
    subprocess.run([sys.executable, str(here / "run.py"), str(script), str(source), str(folder)],
                   env=dict(os.environ, FDL_NOBLINK="1"), check=True, timeout=60)
    assert (folder / "before.png").exists(), f"No frame captured; see {folder / 'log'}"
    return folder


def crop(folder, frame, box=(380, 80, 1000, 280)):
    im = Image.open(folder / (frame + ".png")).convert("RGB")
    sx, sy = im.width / 1200, im.height / 850
    return im.crop(tuple(round(v * (sx if i % 2 == 0 else sy)) for i, v in enumerate(box)))


def stable(folder, frames=("after",)):
    before = crop(folder, "before")
    assert before.getextrema()[0][0] < 150, f"Empty anchor region; see {folder}"
    for frame in frames:
        assert ImageChops.difference(before, crop(folder, frame)).getbbox() is None, \
            f"Viewport moved {frame}; see {folder}"


def saved_matches(folder, expected):
    saved = ET.fromstring("<doc>" + (folder / "lib/docs" / f"{doc_id}.xml").read_text() + "</doc>")
    canonical = lambda tree: ET.canonicalize(ET.tostring(tree, encoding="unicode"), strip_text=True)
    assert canonical(saved) == canonical(expected), f"Unexpected document/selection change; see {folder}"


def setup(selection=False):
    return ("key cmd-f\ntype CARET_ANCHOR\nkey escape\n" + ("" if selection else "key left\n")
            + "wheel 1100 500 700\nwait 500\nmove 1150 820\nwait 200\nshot before\n")


save = "wait 700\nkey cmd-s\nwait 300\n"
fold_click = "move 400 339\nwait 200\nmove 335 339\nwait 200\nclick 335 339\nmove 1150 820\nwait 700\n"
heading = '<h1>FOLD TARGET</h1>\n' + ''.join(f'<p>Inside section {i:02d}</p>\n' for i in range(12)) + '<h1>Next section</h1>'


def fold(name, selection=False, expand=False, hidden=False):
    target = heading.replace("<h1>", '<h1 folded="true">', 1) if expand else heading
    source, expected = fixture(name, target)
    steps = setup(selection)
    if hidden:
        # Put the caret in the section, then move its heading close to the
        # bottom edge. Collapsing must relocate and reveal the hidden caret.
        steps += "click 450 380\nwheel 1100 500 -470\nwait 500\nshot before\n"
        steps += fold_click.replace("339", "809")
    else:
        steps += fold_click
    steps += "shot after\ntype " + marker + "\n" + save + "shot typed\n"
    folder = run_script(name, source, steps)
    if hidden:
        expected.find('h1').text += marker
        # Revealing a heading caret near the edge must actually move it
        # into the viewport, rather than globally suppressing autoscroll.
        assert ImageChops.difference(crop(folder, 'before'), crop(folder, 'after')).getbbox(), \
            f"Hidden caret was not revealed; see {folder}"
    else:
        stable(folder)
        expected.find('p').text = marker if selection else marker + "CARET_ANCHOR"
        assert ImageChops.difference(crop(folder, 'after'), crop(folder, 'typed')).getbbox(), \
            f"Typing did not reveal the old caret; see {folder}"
    if expand:
        del expected.find('h1').attrib['folded']
    else:
        expected.find('h1').set('folded', 'true')
    saved_matches(folder, expected)
    print(f"{name}: fold persisted; selection and scroll intent correct", flush=True)


def keyboard(name, typing=False):
    source, expected = fixture(name, heading)
    steps = setup() + ("" if typing else "key right\nwait 400\n")
    steps += "type " + marker + "\n" + save + "shot after\n"
    folder = run_script(name, source, steps)
    expected.find('p').text = marker + "CARET_ANCHOR" if typing else "C" + marker + "ARET_ANCHOR"
    saved_matches(folder, expected)
    assert ImageChops.difference(crop(folder, 'before'), crop(folder, 'after')).getbbox(), \
        f"Caret was not revealed; see {folder}"
    print(f"{name}: editing/navigation revealed the offscreen caret", flush=True)


def grid_resize(name):
    target = '<grid><column width-ratio="0.5"><p>LEFT COLUMN<br/>Second line<br/>Third line<br/>Fourth line</p></column><column width-ratio="0.5"><p>RIGHT COLUMN</p></column></grid>'
    source, expected = fixture(name, target)
    # A multiline column puts the grip below the floating text-block handle.
    steps = setup() + "move 740 380\nwait 200\ndown 740 380\ndragto 800 380\nwait 300\nshot during\nup 800 380\nmove 1150 820\nwait 500\nshot after\ntype " + marker + "\n" + save
    folder = run_script(name, source, steps)
    stable(folder, ('during', 'after'))
    expected.find('p').text = marker + "CARET_ANCHOR"
    saved = ET.fromstring("<doc>" + (folder / "lib/docs" / f"{doc_id}.xml").read_text() + "</doc>")
    ratios = [float(c.get('width-ratio')) for c in saved.findall('.//column')]
    assert len(ratios) == 2 and ratios[0] > .5 and abs(sum(ratios) - 1) < .001, f"Resize missed; see {folder}"
    for col, ratio in zip(expected.findall('.//column'), saved.findall('.//column')):
        col.set('width-ratio', ratio.get('width-ratio'))
    saved_matches(folder, expected)
    print(f"{name}: ratios saved; viewport and caret preserved", flush=True)


def code_wrap(name, theme="craft"):
    source, expected = fixture(name, '<pre lang="rust"><code>let value = 42;</code></pre>', theme)
    steps = setup() + "move 1000 305\nwait 200\nclick 1000 305\nmove 1150 820\nwait 500\nshot after\ntype " + marker + "\n" + save
    folder = run_script(name, source, steps)
    stable(folder)
    expected.find('p').text = marker + "CARET_ANCHOR"
    expected.find('pre').set('wrap', 'true')
    saved_matches(folder, expected)
    print(f"{name}: wrap persisted; viewport and caret preserved", flush=True)


cases = {
    'fold': lambda: fold('fold'),
    'fold_selection': lambda: fold('fold_selection', selection=True),
    'expand': lambda: fold('expand', expand=True),
    'fold_hidden_caret': lambda: fold('fold_hidden_caret', hidden=True),
    'typing': lambda: keyboard('typing', typing=True),
    'navigation': lambda: keyboard('navigation'),
    'grid_resize': lambda: grid_resize('grid_resize'),
    'code_wrap': lambda: code_wrap('code_wrap'),
    'code_wrap_feishu': lambda: code_wrap('code_wrap_feishu', 'feishu'),
}
print(f"Artifacts: {work}", flush=True)
for name in os.environ.get('SCROLL_CASES', ','.join(cases)).split(','):
    cases[name]()
