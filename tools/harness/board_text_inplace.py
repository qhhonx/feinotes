"""A board text keeps its place when its editor opens: for each case the text
is shot as the board draws it and again in the editor (the caret moved to the
line's start), and the two must match pixel for pixel but for the caret.

Cases: Chinese in a rounded rectangle, English in a sharp one, two mixed lines
in an ellipse, Chinese in a diamond, a two-line standalone text, an arrow's
label, a wrapping container text, a rectangle at 120% and, last (new texts
take the font picked last), Nunito rather than the hand-drawn font; each in
the dark and the light theme. Xiaolai (the hand-drawn font's Chinese) comes
from a cache folder holding the repo's copy.

Build with --release --features devtools, then run this file (FDL_BIN selects
another build). Shots land in the work folder printed at the end (or
BOARD_TEXT_OUT); it exits 1 when a case moved.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from PIL import Image, ImageChops

here = Path(__file__).resolve().parent
repo = here.parent.parent
font = repo / "assets/fonts/excalidraw/Xiaolai-Regular.ttf.zlib"
name = next(e for e in json.loads((repo / "packaging/fonts.json").read_text())["fonts"] if e["id"] == "xiaolai")["files"][0]["name"]
work = Path(os.environ.get("BOARD_TEXT_OUT") or tempfile.mkdtemp(prefix="quiet-board-text-"))
work.mkdir(parents=True, exist_ok=True)

# Board coordinates (`origin board`): the properties panel's Nunito and sharp
# Edges buttons for a selected rectangle with a text, and zoom in. The window is
# made 860 tall first so the panel shows its Font family row without scrolling.
NUNITO, SHARP, ZOOM_IN = "84 509", "43 442", "130 718"
# (name, steps that make the text, where to double-click it)
CASES = [
    ("rrect-zh", "key r\ndrag 300 200 600 340\nkey enter\ntype 画板里的一行中文", "450 270"),
    ("rect-en", f"key r\ndrag 300 200 600 340\nkey enter\ntype Hello board text\nkey escape\nclick {SHARP}", "450 270"),
    ("ellipse-lines", "key o\ndrag 300 180 660 380\nkey enter\ntype 第一行 line one\nkey enter\ntype second 第二行", "480 280"),
    ("diamond-zh", "key d\ndrag 300 160 660 400\nkey enter\ntype 菱形里的字", "480 280"),
    ("text-lines", "key t\nclick 320 250\ntype Standalone 独立文本\nkey enter\ntype line two", "340 262"),
    ("arrow-label", "key a\ndrag 300 300 800 300\nkey enter\ntype label 标签", "550 300"),
    ("wrap", "key r\ndrag 300 200 480 380\nkey enter\ntype A longer sentence that wraps inside the box 中文也换行", "390 290"),
    ("zoom", f"click {ZOOM_IN}\nclick {ZOOM_IN}\nwait 300\nkey r\ndrag 300 200 600 340\nkey enter\ntype 缩放后编辑 zoomed", "450 270"),
    ("nunito", f"key r\ndrag 300 200 600 340\nkey enter\ntype 普通字体 Normal font\nkey escape\nclick {NUNITO}", "450 270"),
]


def script(tag):
    s = "resize 1040 860\nwait 500\nmove 700 700\nwait 1500\nrow lib\nwait 800\nanchor create-folder\nwait 300\nrow Board\nwait 1200\nkey cmd-]\nwait 600\norigin board\n"
    for case, make, at in CASES:
        steps = make.split("\n") + ["key escape", "click 700 480", "move 700 480", "wait 250", f"shot {tag}-{case}-a"]
        steps += [f"dclick {at}", "key cmd-left", "move 700 480", "wait 250", f"shot {tag}-{case}-b", "key escape", "key cmd-a", "key backspace", "wait 400"]
        # a step at a time: run back to back, a step can land before the last is done with;
        # the clearing gets longer (a case left behind threw every later one off)
        s += "".join(f"{l}\nwait 150\n" for l in steps)
    return s


def run(tag, appearance, cache):
    lib = work / f"lib-{tag}"
    shutil.rmtree(lib, ignore_errors=True)
    shutil.copytree(here / "lib_clean", lib)
    (lib / "preferences.json").write_text(json.dumps({"appearance": appearance, "english": True, "seen_version": "9.9.9"}))
    out = work / tag
    out.mkdir(exist_ok=True)
    (out / "script.txt").write_text(script(tag))
    # nothing listens at the URL: Xiaolai comes from the cache
    env = {**os.environ, "QUIET_MD_FONT_URL": "http://127.0.0.1:9/", "QUIET_MD_FONT_DIR": str(cache), "FDL_NOBLINK": "1"}
    subprocess.run([sys.executable, str(here / "run.py"), str(out / "script.txt"), str(lib), str(out)], env=env, check=True)
    return out


def moved(a, b):
    """The difference beyond the caret (a column at most 4px wide: a 2pt caret at 2x), as a box."""
    # the canvas right of the properties panel and its shadow (the panel
    # shows while editing), below the hint, above the zoom bar; the sidebar is
    # closed first (`key cmd-]`), so the board starts at the window's left
    box = (520, 280, a.width, a.height - 60)
    d = ImageChops.difference(a.convert("RGB").crop(box), b.convert("RGB").crop(box)).convert("L").point(lambda v: 255 if v else 0)
    bb = d.getbbox()
    if bb is None or bb[2] - bb[0] <= 4:
        return None
    return (bb[0] + box[0], bb[1] + box[1], bb[2] + box[0], bb[3] + box[1])


def main():
    cache = work / "cache"
    cache.mkdir(exist_ok=True)
    shutil.copy(font, cache / name)
    bad = 0
    for tag, appearance in (("dark", "dark"), ("light", "light")):
        out = run(tag, appearance, cache)
        for case, _, _ in CASES:
            a, b = out / f"{tag}-{case}-a.png", out / f"{tag}-{case}-b.png"
            if not (a.exists() and b.exists()):
                print(f"{tag}-{case}: missing shot"); bad += 1; continue
            m = moved(Image.open(a), Image.open(b))
            print(f"{tag}-{case}: {'same' if m is None else f'moved, differs in {m}'}")
            bad += m is not None
    print(work)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
