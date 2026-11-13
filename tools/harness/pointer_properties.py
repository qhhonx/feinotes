"""Native regressions for pointer property controls using an isolated library.

Build with --release --features devtools. Needs Pillow.
PROPERTY_CASES selects comma-separated cases; FDL_BIN selects another build.
"""
import copy
from datetime import datetime
import time
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

from PIL import Image, ImageChops

here = Path(__file__).resolve().parent
work = Path(tempfile.mkdtemp(prefix="quiet-properties-"))
doc_id = "aaaaaaaa-0000-0000-0000-000000000001"
marker = "CARET_MARKER"


def fixture(name, target):
    source = work / (name + "-source")
    (source / "docs").mkdir(parents=True)
    (source / "assets").mkdir()
    Image.new("RGB", (300, 120), (70, 140, 180)).save(source / "assets/sample.png")
    (source / "assets/sample.txt").write_text("ATTACHMENT PREVIEW\nSecond line\nThird line\n")
    (source / "index.json").write_text(json.dumps({doc_id: {"created": 1, "modified": 1, "opened": 1}}))
    (source / "preferences.json").write_text(json.dumps({
        "theme": "craft", "appearance": "light", "english": True, "seen_version": "0.1.10",
        "auto_update": False, "quick_hotkey": False, "remote_images": False,
    }))
    xml = '<title>Pointer property regression</title>\n<p>CARET_ANCHOR</p>\n'
    xml += ''.join(f'<p>Anchor {i:02d}: Before target paragraph</p>\n' for i in range(20))
    xml += target + '\n'
    xml += ''.join(f'<p>After target paragraph {i:02d}</p>\n' for i in range(40))
    (source / "docs" / (doc_id + ".xml")).write_text(xml)
    return source, ET.fromstring("<doc>" + xml + "</doc>")


def run_script(name, source, steps):
    folder = work / name
    script = work / (name + ".txt")
    script.write_text("resize 1200 850\nwait 1500\nmove 1150 820\n" + steps + "\n")
    subprocess.run([sys.executable, str(here / "run.py"), str(script), str(source), str(folder)],
                   env=dict(os.environ, FDL_NOBLINK="1"), check=True, timeout=60)
    assert (folder / "before.png").exists(), f"No frame captured; see {folder / 'log'}"
    return folder


def stable(folder):
    # Left-hand text anchors stay clear of property panels and toolbars.
    def anchor(frame):
        im = Image.open(folder / (frame + ".png")).convert("RGB")
        sx, sy = im.width / 1200, im.height / 850
        return im.crop((round(350*sx), round(80*sy), round(420*sx), round(210*sy)))
    a, b = anchor("before"), anchor("after")
    assert a.getextrema()[0][0] < 150, f"Empty anchor region; see {folder}"
    assert ImageChops.difference(a, b).getbbox() is None, f"Viewport moved; see {folder}"


def read_doc(folder):
    return ET.fromstring("<doc>" + (folder / "lib/docs" / (doc_id + ".xml")).read_text() + "</doc>")


def same_doc(folder, expected, saved=None):
    saved = read_doc(folder) if saved is None else saved
    for tree in (expected, saved):
        for time in tree.findall('.//time'):
            time.text = None  # The serializer derives the displayed date label.
    canonical = lambda tree: ET.canonicalize(ET.tostring(tree, encoding="unicode"), strip_text=True)
    (folder / "expected.xml").write_text(ET.tostring(expected, encoding="unicode"))
    assert canonical(saved) == canonical(expected), f"Unexpected state, caret or document content; see {folder}"


def setup(activate=""):
    return ("key cmd-f\ntype CARET_ANCHOR\nkey escape\nkey left\n"
            "wheel 1100 500 700\nwait 500\n" + activate
            + "move 1150 820\nwait 200\nshot before\n")


settle = "move 1150 820\nwait 600\n"
save = "wait 700\nkey cmd-s\nwait 300\n"
callout = '<callout emoji="💡" background-color="light-orange" border-color="orange"><p>CALLOUT TARGET</p></callout>'
image = '<img src="assets/sample.png" width="300" height="120" align="left"/>'
file_card = '<figure view-type="Card"><source path="assets/sample.txt" name="sample.txt"/></figure>'
countdown = '<countdown duration="0" start-time="0" notify="false" color="#FF8800" timing="duration"/>'
expire = int(datetime(2030, 1, 1, 16).timestamp() * 1000)
reminder = f'<p>REMINDER <time expire-time="{expire}" notify-time="{expire}" should-notify="true">Date</time></p>'
table = '<table><colgroup><col width="240"/><col width="240"/></colgroup><tbody><tr><td><p>LEFT CELL</p></td><td><p>RIGHT CELL</p></td></tr><tr><td><p>Lower left</p></td><td><p>Lower right</p></td></tr></tbody></table>'


def probe():
    specs = {
        'image': (image, "click 450 350\nwait 300\n"),
        'file': (file_card, "click 450 330\nwait 300\n"),
        'callout': (callout, "move 450 335\nwait 300\nmove 740 270\nwait 400\n"),
        'countdown': (countdown, "move 1000 340\nwait 300\n"),
        'reminder': (reminder, "click 480 314\nwait 400\n"),
        'table': (table, "move 465 300\nwait 300\n"),
    }
    for name in os.environ.get('PROPERTY_PROBE', ','.join(specs)).split(','):
        target, activate = specs[name]
        source, _ = fixture(name, target)
        folder = run_script(name, source, setup() + activate + "shot controls\n")
        print(f"Probe {name}: {folder}", flush=True)



def run(name, target, activate, action, mutate, finish="", caret="anchor", dismiss=True):
    source, original = fixture(name, target)
    expected = copy.deepcopy(original)
    steps = setup(activate) + action + settle + "shot after\n" + ("key escape\n" if dismiss else "")
    steps += finish
    if caret == "atomic":
        # The toolbar is available only with its image/attachment selected.
        # Enter should create a paragraph directly after that same object.
        steps += "key enter\ntype " + marker + "\n"
    elif caret == "cell":
        steps += "type " + marker + "\n"
    else:
        steps += "type " + marker + "\n"
    folder = run_script(name, source, steps + save)
    stable(folder)
    saved = read_doc(folder)
    mutate(expected, saved)
    if caret == "atomic":
        tag = 'img' if expected.find('img') is not None else 'figure'
        at = list(expected).index(expected.find(tag))
        paragraph = ET.Element('p'); paragraph.text = marker
        expected.insert(at + 1, paragraph)
    elif caret == "reminder_undo":
        paragraph = next(p for p in expected.findall('p') if p.find('time') is not None)
        paragraph.text += marker
    elif caret == "cell":
        expected.find('.//td/p').text = marker + "LEFT CELL"
    else:
        expected.find('p').text = marker + "CARET_ANCHOR"
    same_doc(folder, expected, saved)
    print(f"{name}: property/history state, pointer viewport and resulting caret verified", flush=True)


def attribute(path, key, value):
    def mutate(expected, _):
        node = expected.find(path)
        if value is None:
            node.attrib.pop(key, None)
        else:
            node.set(key, value)
    return mutate


palette_open = "move 450 335\nwait 250\nmove 740 270\nwait 300\n"
image_select = "click 450 350\nwait 300\n"
file_select = "click 450 330\nwait 300\n"
reminder_open = "click 480 314\nwait 300\n"


def attributes(path, **values):
    def mutate(expected, saved):
        for key, value in values.items():
            attribute(path, key.replace('_', '-'), value)(expected, saved)
    return mutate


def cell_attribute(key, value):
    def mutate(expected, saved):
        for row in expected.findall('.//tr'):
            node = row.find('td')
            if value is None:
                node.attrib.pop(key, None)
            else:
                node.set(key, value)
    return mutate


def countdown_settings(expected, saved):
    node = saved.find('countdown')
    assert int(time.time()*1000) - 120000 < int(node.get('start-time')) <= int(time.time()*1000), 'Countdown did not restart'
    assert node.get('duration') == '3601' and node.get('hours') == '1', 'Duration field was not applied'
    assert node.get('timing') == 'duration' and node.get('notify') == 'false'
    assert datetime.strptime(node.get('date'), '%Y-%m-%d')
    assert datetime.strptime(node.get('time'), '%H:%M')
    expected.find('countdown').attrib.update({
        'duration': '3601', 'start-time': node.get('start-time'),
        'days': '0', 'hours': '1', 'minutes': '0', 'seconds': '0',
        'date': node.get('date'), 'time': node.get('time'),
    })



def random_callout(expected, saved):
    node = saved.find('callout')
    hue = node.get('border-color')
    assert hue in ('gray', 'red', 'orange', 'yellow', 'green', 'blue', 'purple')
    assert node.get('background-color') == 'light-' + hue
    expected.find('callout').set('border-color', hue)
    expected.find('callout').set('background-color', 'light-' + hue)


def countdown_date(expected, saved):
    node = saved.find('countdown')
    start = int(node.get('start-time'))
    assert int(time.time()*1000) - 120000 < start <= int(time.time()*1000)
    assert node.get('timing') == 'date' and node.get('date') == '2030-01-01' and node.get('time') == '16:00'
    assert abs(float(node.get('duration')) - ((expire-start)/1000+1)) < 10
    expected.find('countdown').attrib.update({'timing': 'date', 'duration': node.get('duration'), 'start-time': str(start)})


count_hover = 'move 1000 340\nwait 200\n'
count_color = count_hover + 'move 1112 304\nwait 250\nclick 916 342\n'
count_settings = count_hover + 'click 1080 304\nwait 250\nclick 448 382\nkey cmd-a\ntype 1\nkey tab\nclick 852 567\n'
cell_select = 'move 450 332\nwait 200\nclick 465 307\nwait 250\n'
cell_align = 'click 465 270\nwait 250\n'
cell_bg = 'click 354 270\nwait 250\n'

# Each spec has one native gesture, an exact state assertion, and a marker
# that proves the old caret (or selected atomic object/cells) survived.
specs = {
    'image_center': (image, image_select, 'click 537 281\n', attribute('img', 'align', 'center'), 'atomic', True),
    'image_right': (image, image_select, 'click 569 281\n', attribute('img', 'align', 'right'), 'atomic', True),
    'image_left': (image.replace('align="left"', 'align="center"'), 'click 740 350\nwait 300\n', 'click 749 281\n', attribute('img', 'align', 'left'), 'atomic', True),
    'file_preview': (file_card, file_select, 'click 615 266\n', attribute('figure', 'view-type', 'Preview'), 'atomic', True),
    'file_card': (file_card.replace('Card', 'Preview'), file_select, 'click 777 187\n', attribute('figure', 'view-type', 'Card'), 'atomic', True),
    'callout_text': (callout, '', palette_open+'click 700 350\n', attribute('callout', 'text-color', 'red'), 'anchor', True),
    'callout_border': (callout, '', palette_open+'click 804 408\n', attribute('callout', 'border-color', 'blue'), 'anchor', True),
    'callout_fill': (callout, '', palette_open+'click 804 465\n', attribute('callout', 'background-color', 'light-blue'), 'anchor', True),
    'callout_emoji': (callout, '', 'click 373 331\nwait 300\ntype rocket\nwait 300\nkey enter\n', attribute('callout', 'emoji', '🚀'), 'anchor', False),
    'callout_reset': (callout.replace('light-orange','light-blue').replace('border-color="orange"','border-color="blue" text-color="red"'), '', palette_open+'click 795 532\n', attributes('callout', background_color='light-orange', border_color='orange', text_color=None), 'anchor', True),
    'countdown_color': (countdown, '', count_color, attribute('countdown', 'color', '#F76964'), 'anchor', False),
    'countdown_settings': (countdown, '', count_settings, countdown_settings, 'anchor', False),
    'reminder_notify': (reminder, '', reminder_open+'click 377 766\n', attribute('.//time', 'should-notify', 'false'), 'anchor', True),
    'reminder_date': (reminder, '', reminder_open+'click 500 448\n', attributes('.//time', expire_time=str(expire+86400000), notify_time=str(expire+86400000)), 'anchor', True),
    'reminder_time': (reminder, '', reminder_open+'click 440 723\nkey cmd-a\ntype 10:00\nkey enter\n', attributes('.//time', expire_time=str(expire-6*3600000), notify_time=str(expire-6*3600000)), 'anchor', True),
    'reminder_all_day': (reminder, '', reminder_open+'click 377 690\n', attributes('.//time', expire_time=str(expire+28799000), notify_time=str(expire-7*3600000), is_whole_day='true'), 'anchor', True),
    'table_bg': (table, cell_select, cell_bg+'click 417 343\n', cell_attribute('background-color', 'light-red'), 'cell', True),
    'table_valign': (table, cell_select, cell_align+'click 490 445\n', cell_attribute('vertical-align', 'middle'), 'cell', True),
}

specs.update({
    'callout_random': (callout.replace('light-orange','medium-gray').replace('border-color="orange"','border-color="gray" text-color="red"'), '', palette_open+'click 685 532\n', random_callout, 'anchor', True),
    'callout_clear_text': (callout.replace('<callout ', '<callout text-color="red" '), '', palette_open+'click 700 350\n', attribute('callout', 'text-color', None), 'anchor', True),
    'callout_clear_border': (callout, '', palette_open+'click 727 408\n', attribute('callout', 'border-color', None), 'anchor', True),
    'callout_clear_fill': (callout, '', palette_open+'click 727 465\n', attribute('callout', 'background-color', None), 'anchor', True),
    'reminder_advance': (reminder, '', reminder_open+'click 450 799\nwait 250\nclick 440 593\n', attribute('.//time', 'notify-time', str(expire-300000)), 'anchor', True),
    'table_bottom': (table, cell_select, cell_align+'click 490 486\n', cell_attribute('vertical-align', 'bottom'), 'cell', True),
    'table_top': (table.replace('<td>', '<td vertical-align="middle">'), cell_select, cell_align+'click 490 422\n', cell_attribute('vertical-align', None), 'cell', True),
    'table_bg_reset': (table.replace('<td>', '<td background-color="light-blue">'), cell_select, cell_bg+'click 460 410\n', cell_attribute('background-color', None), 'cell', True),
    'countdown_date': (countdown.replace('/>', ' date="2030-01-01" time="16:00"/>'), '', count_hover+'click 1080 304\nwait 250\nclick 350 458\nclick 852 567\n', countdown_date, 'anchor', False),
})


verified = set()


def run_case(name):
    base = name.removesuffix('_undo').removesuffix('_redo')
    target, activate, action, mutate, caret, dismiss = specs[base]
    finish = ''
    if name.endswith('_undo'):
        # A filtered undo run must first prove the gesture changes state.
        if base not in verified:
            run_case(base)
        finish = 'key cmd-z\nwait 200\n'
        mutate = lambda expected, saved: None
        # Core::set_object checkpoints at the inline object, then restores
        # the live caret. Undo intentionally returns to that object.
        if base.startswith('reminder_'):
            caret = 'reminder_undo'
    elif name.endswith('_redo'):
        finish = 'key cmd-z\nwait 200\nkey cmd-shift-z\nwait 200\n'
    run(name, target, activate, action, mutate, finish, caret, dismiss)
    if name == base:
        verified.add(base)


cases = list(specs)
for base in ('image_center', 'file_preview', 'callout_text', 'countdown_color', 'countdown_settings', 'reminder_notify', 'table_bg', 'table_valign'):
    cases += [base + '_undo', base + '_redo']

print(f"Artifacts: {work}", flush=True)
if 'PROPERTY_PROBE' in os.environ:
    probe()
else:
    for name in os.environ.get('PROPERTY_CASES', ','.join(cases)).split(','):
        run_case(name)
