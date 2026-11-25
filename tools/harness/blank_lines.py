"""Blank lines and spaces typed in a page stay (#77): in a Markdown file and a
DocxXML page, Enter pressed a few times, a line of only spaces, spaces at a
line's start, middle and end. After the autosave the page must look the same
when another program changes the file (here the first line, as a sync or the
`quiet` CLI would: the app takes the file in), when it is opened again and
after a relaunch; the files must hold exactly what `EXPECTED` says.

Build with --release --features devtools, then run this file (FDL_BIN selects
another build). Shots land in the work folder printed at the end (or
BLANK_LINES_OUT); it exits 1 when something was lost.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from PIL import Image, ImageChops

here = Path(__file__).resolve().parent
work = Path(os.environ.get("BLANK_LINES_OUT") or tempfile.mkdtemp(prefix="quiet-blank-lines-"))
work.mkdir(parents=True, exist_ok=True)

DOCS = {"Notes MD": ".md", "Notes XML": ".xml"}
EXPECTED = {
    ".md": "第二段\n\n&nbsp;\n\n&nbsp;\n\n  &#32;\n\n   空格开头\n\n中间   空格  &#32;\n\n末段\n",
    ".xml": "<title>Notes XML</title>\n<p>第二段</p>\n<p></p>\n<p></p>\n<p>   </p>\n<p>   空格开头</p>\n<p>中间   空格   </p>\n<p>末段</p>\n",
}
# The page's text below the first line (which the other program changes),
# and the caret at the end of the last line, the pointer away from it.
PAGE, LAST = (300, 200, 1280, 560), "click 900 391\nkey cmd-right\nmove 1200 700\nwait 300"


# Three spaces, spelled out: an editor saving this file may strip a line's end.
SP = " " * 3


def typing(name):
    return f"""row {name}
wait 800
click 600 187
key cmd-right
key enter
key enter
key enter
type {SP}
key enter
type    空格开头
key enter
type 中间   空格{SP}
key enter
type 末段
wait 4000
{LAST}
shot {name}-typed
wait 4000
{LAST}
shot {name}-taken-in
"""


def again(name, tag):
    return f"row {name}\nwait 1000\n{LAST}\nshot {name}-{tag}\n"


def run(src, out, script, watch=False):
    """`script` on a copy of `src` (run.py's, `out`/lib), which it returns."""
    out.mkdir(parents=True, exist_ok=True)
    lib = out / "lib"
    (out / "script.txt").write_text(script)
    stop = threading.Event()

    def other_program():
        # Once a page is saved and shot, change its first line the way another program would.
        done = set()
        while not stop.is_set() and len(done) < len(DOCS):
            for name, ext in DOCS.items():
                f = lib / f"{name}{ext}"
                if name not in done and (out / f"{name}-typed.png").exists() and "末段" in f.read_text():
                    f.write_text(f.read_text().replace("第一段", "第二段"))
                    done.add(name)
            time.sleep(0.2)

    t = threading.Thread(target=other_program) if watch else None
    if t: t.start()
    try:
        subprocess.run([sys.executable, str(here / "run.py"), str(out / "script.txt"), str(src), str(out)], env={**os.environ, "FDL_NOBLINK": "1"}, check=True)
    finally:
        stop.set()
        if t: t.join()
    return lib


def differs(a, b):
    d = ImageChops.difference(Image.open(a).convert("RGB").crop(PAGE), Image.open(b).convert("RGB").crop(PAGE))
    return d.getbbox()


def main():
    lib = work / "lib"
    shutil.rmtree(lib, ignore_errors=True)
    shutil.copytree(here / "lib_clean", lib)
    (lib / "preferences.json").write_text(json.dumps({"english": True, "seen_version": "9.9.9"}))
    (lib / "Notes MD.md").write_text("第一段\n")
    (lib / "Notes XML.xml").write_text("<title>Notes XML</title>\n<p>第一段</p>\n")
    head = "move 700 700\nwait 1500\n"
    # Typed, taken in from the other program, then each opened again after the other.
    lib = run(lib, work / "first", head + "".join(typing(n) for n in DOCS) + "".join(again(n, "reopened") for n in DOCS), watch=True)
    lib = run(lib, work / "relaunch", head + "".join(again(n, "relaunched") for n in DOCS))
    bad = 0
    for name, ext in DOCS.items():
        text = (lib / f"{name}{ext}").read_text()
        ok = text == EXPECTED[ext]
        print(f"{name}{ext}: {'as expected' if ok else f'unexpected {text!r}'}")
        bad += not ok
        typed = work / "first" / f"{name}-typed.png"
        for shot in (work / "first" / f"{name}-taken-in.png", work / "first" / f"{name}-reopened.png", work / "relaunch" / f"{name}-relaunched.png"):
            box = differs(typed, shot) if shot.exists() else "missing"
            print(f"  {shot.stem}: {'same' if box is None else f'differs {box}'}")
            bad += box is not None
    print(work)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
