#!/usr/bin/env python3
"""Build the 开始指南 sample library from shared-mock/guides/src.

usage: python3 shared-mock/guides/build.py <out_library_dir>

Each src/*.xml is one document (file name order = reading order). Placeholders:
  {{doc:key}}                       @文档 link to the guide whose KEYS entry is key
  {{reminder:YYYY-MM-DD HH:MM}}     timed date reminder, notified when it is due
  {{reminder-day:YYYY-MM-DD}}       whole-day reminder, notified 09:00 that day
  {{countdown:YYYY-MM-DD HH:MM}}    countdown block to that moment (blue)
  {{button:Action|url|text|color}}  inline button (OpenLink / DuplicatePage)
Assets under guides/assets are copied to <out>/.assets/guides.

The output is a FORMAT 4 library: folders on disk, each document a file named
after its title (store::file_name_for), links as paths relative to the file,
and what the files cannot say in .quiet/index.json, keyed by path.
"""
import datetime as dt, json, re, shutil, sys, time, uuid
from pathlib import Path

here = Path(__file__).resolve().parent
out = Path(sys.argv[1]).resolve()
NS = uuid.UUID("6f7c1e9a-9d0b-4a51-8c7e-2a1d3b4c5e6f")
# Folders in the guide library: key -> title, a folder on disk.
FOLDERS = {"writing-guides": "Writing and formatting", "examples": "Examples"}
# The top level, in order; every entry gets its place, so the sidebar and
# Home's manual order (the default) read the same.
TOP = ["welcome", "writing-guides", "examples", "organize", "data", "personalize", "practice"]
# file stem -> (key, parent key, pinned); a parent is a key of FOLDERS
KEYS = {
    "01-welcome": ("welcome", None, False),
    "02-writing": ("writing", "writing-guides", False),
    "03-blocks": ("blocks", "writing-guides", False),
    "04-tables": ("tables", "writing-guides", False),
    "05-code": ("code", "writing-guides", False),
    "06-media": ("media", "writing-guides", False),
    "07-organize": ("organize", None, False),
    "07a-reading-list": ("reading-list", "examples", False),
    "07b-weekly": ("weekly", "examples", False),
    "08-data": ("data", None, False),
    "09-personalize": ("personalize", None, False),
    "10-practice": ("practice", None, False),
}
ids = {k: str(uuid.uuid5(NS, k)) for k, _, _ in KEYS.values()}
ids.update({k: str(uuid.uuid5(NS, k)) for k in FOLDERS})
srcs = {KEYS[p.stem][0]: p.read_text() for p in sorted((here / "src").glob("*.xml"))}
missing = set(ids) - set(srcs) - set(FOLDERS)
assert not missing, f"no source for {missing}"

def esc(s): return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
def plain_title(x):
    t = re.search(r"<title[^>]*>(.*?)</title>", x, re.S).group(1)
    return re.sub(r"<[^>]+>", "", t).replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
titles = {k: plain_title(x) for k, x in srcs.items()}
local = lambda s: dt.datetime.strptime(s, "%Y-%m-%d %H:%M").astimezone()
ms = lambda d: int(d.timestamp() * 1000)
now = int(time.time())

ILLEGAL = dict(zip('/\\:*?"<>|', '／＼：＊？＂＜＞｜'))
def file_name(title):
    # Same rules as store::file_name_for (asserted for these titles by guides.rs).
    s = " ".join("".join(ILLEGAL.get(c, " " if ord(c) < 32 else c) for c in title).split())
    s = s.lstrip(". ").rstrip(". ") or "未命名文档"
    assert len(s.encode()) <= 120 and s.split(".")[0].upper() not in ("CON", "PRN", "AUX", "NUL"), title
    return s
def rel_path(frm, to):
    a, b = [p for p in frm.split("/") if p], [p for p in to.split("/") if p]
    n = 0
    while n < min(len(a), len(b)) and a[n] == b[n]: n += 1
    return "/".join([".."] * (len(a) - n) + b[n:])
# Where each guide goes: <folder>/<title>.xml.
paths = {}
for k, parent, _ in KEYS.values():
    d = file_name(FOLDERS[parent]) if parent else ""
    paths[k] = (d + "/" if d else "") + file_name(titles[k]) + ".xml"
assert len({p.lower() for p in paths.values()}) == len(paths), "two guides with one name"
here_dir = ""

def fill(x):
    def doc(m):
        k = m.group(1)
        assert k in ids, f"unknown doc {k}"
        return f'<cite type="doc" href="{esc(rel_path(here_dir, paths[k])).replace(chr(34), "&quot;")}" file-type="docx">{esc(titles[k])}</cite>'
    def rem(m):
        d = local(m.group(1))
        return f'<time expire-time="{ms(d)}" notify-time="{ms(d)}" should-notify="true">{d:%Y-%m-%d %H:%M}</time>'
    def rem_day(m):
        d = local(m.group(1) + " 23:59")
        n = local(m.group(1) + " 09:00")
        return f'<time expire-time="{ms(d)}" notify-time="{ms(n)}" should-notify="true" is-whole-day="true">{d:%Y-%m-%d}</time>'
    def countdown(m):
        d = local(m.group(1))
        start = now * 1000
        dur = (ms(d) - start) // 1000 + 1
        return f'<countdown duration="{dur}" start-time="{start}" notify="true" color="#4E83FD" timing="date" date="{d:%Y-%m-%d}" time="{d:%H:%M}"/>'
    def button(m):
        action, url, text, color = m.group(1).split("|")
        src = f' src="{esc(url)}"' if url else ""
        return f'<button action="{action}"{src} background-color="{color}">{esc(text)}</button>'
    x = re.sub(r"\{\{doc:([a-z-]+)\}\}", doc, x)
    x = re.sub(r"\{\{reminder:([0-9-]+ [0-9:]+)\}\}", rem, x)
    x = re.sub(r"\{\{reminder-day:([0-9-]+)\}\}", rem_day, x)
    x = re.sub(r"\{\{countdown:([0-9-]+ [0-9:]+)\}\}", countdown, x)
    x = re.sub(r"\{\{button:([^}]+)\}\}", button, x)
    assert "{{" not in x, re.search(r"\{\{[^}]*\}\}", x).group(0)
    # A cover is a file in the library (the app copies a gallery pick into
    # assets): bring the app's gallery image along.
    def cover(m):
        name = m.group(1)
        covers.add(name)
        return f'cover=".assets/guides/covers/{name}"'
    x = re.sub(r'cover="covers/([^"]+)"', cover, x)
    return re.sub(r'(src|path)="assets/', r'\1=".assets/', x)

covers = set()

if out.exists() and any(out.iterdir()):
    sys.exit(f"{out} is not empty")
out.mkdir(parents=True, exist_ok=True)
(out / ".quiet").mkdir()
index = {}
order = list(KEYS.values())
inside = {}
for k, title in FOLDERS.items():
    (out / file_name(title)).mkdir()
    index[file_name(title) + "/"] = {"id": ids[k], "created": now - 3600, "modified": now}
for i, (k, parent, pinned) in enumerate(order):
    here_dir = paths[k].rpartition("/")[0]
    (out / paths[k]).write_text(fill(srcs[k]))
    # Reading order is written out: TOP places the top level, and a folder's
    # pages are numbered in file order.
    t = now - 60 * i
    meta = {"id": ids[k], "created": now - 3600 + i, "modified": t}
    if pinned: meta["pinned"] = True
    if parent:
        assert parent in FOLDERS, f"{k}: pages live in folders, not under {parent}"
        inside[parent] = inside.get(parent, 0) + 1
        meta["order"] = inside[parent]
    else:
        assert k in TOP, f"{k} is missing from TOP"
    index[paths[k]] = meta
assert set(TOP) == {k for k, p, _ in KEYS.values() if not p} | set(FOLDERS), "TOP must list every top-level entry"
key_of = {**{k: paths[k] for k in paths}, **{k: file_name(t) + "/" for k, t in FOLDERS.items()}}
for i, k in enumerate(TOP):
    index[key_of[k]]["root_order"] = i + 1
(out / ".quiet" / "index.json").write_text(json.dumps(dict(sorted(index.items())), ensure_ascii=False, indent=2) + "\n")
(out / ".quiet" / "format.json").write_text('{\n  "format": 4\n}')
shutil.copytree(here / "assets", out / ".assets" / "guides")
(out / ".assets" / "guides" / "covers").mkdir()
for name in sorted(covers):
    shutil.copy2(here.parent.parent / "assets" / "covers" / name, out / ".assets" / "guides" / "covers" / name)
print(out)
