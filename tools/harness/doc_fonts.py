"""设置 › 文档字体's download list (src/font_downloads.rs, docs/FONTS.md).

Build with --release --features devtools, then run this file. Needs Pillow.
FDL_BIN selects another build. The fonts come from a local, deliberately
slow HTTP server (QUIET_MD_FONT_URL) serving tools/fonts/stage.py's output
(FONT_STAGE, default /tmp/quiet-md-fonts: staged on first use, which fetches
the upstream releases); the cache is a temporary folder (QUIET_MD_FONT_DIR).
Screenshots land in the work folder printed at the end (or DOC_FONTS_OUT).

1. Pick 霞鹜文楷: it downloads with progress in the list, becomes the
   document font, the article draws in it (body, headings, bold, full-width
   punctuation; code keeps the code face).
2. Restart: it is loaded from the cache, no request.
3. Cache deleted: the default face stands in, the preference stays, the row
   says it can be downloaded again.
4. Delete from the list: the cache goes, the document font goes back to 跟随主题.

--compare [ids]: the article in every catalog font (trial ones too), one
shot each at the same window size, and compare.png with all of them.
"""
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import time

from PIL import Image, ImageDraw, ImageFont

here = Path(__file__).resolve().parent
repo = here.parent.parent
catalog = json.loads((repo / "packaging/fonts.json").read_text())["fonts"]
work = Path(os.environ.get("DOC_FONTS_OUT") or tempfile.mkdtemp(prefix="quiet-doc-fonts-"))
work.mkdir(parents=True, exist_ok=True)
stage = Path(os.environ.get("FONT_STAGE", "/tmp/quiet-md-fonts"))
hits = []
RATE = 8 << 20  # bytes per second: slow enough to see progress

ARTICLE = """# 阅读字体样张 Reading Sample

## 一、简体常用字

这个问题让我们觉得发展还会继续进步。国务院关于进一步加强义务教育阶段学生作业管理的通知，强调减轻负担、提质增效：「书写要规范，标点要准确！」

## 二、中英混排

Quiet.md 是一款**安静的本地笔记应用**，*Notes live as plain files* on your disk。它支持 `Markdown`、画板和飞书 DocxXML；“引号”‘单引号’（括号）《书名号》——破折号……省略号。

The quick brown fox jumps over the lazy dog. **Bold text** and *italic text* sit beside 0123456789.

## 三、长文段落

读书之法，在循序而渐进，熟读而精思。字求其训，句索其旨，未得乎前，则不敢求其后；未通乎此，则不敢志乎彼。如是，则志定而理明，意诚而心正。
"""


class Host(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *a, **k):
        super().__init__(*a, directory=str(stage), **k)

    def do_GET(self):
        hits.append(self.path)
        path = stage / self.path.lstrip("/")
        if not path.is_file():
            return self.send_error(404)
        data = path.read_bytes()
        self.send_response(200)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        # Paced by the clock (see board_font.py): short sleeps can last several times longer.
        start = time.monotonic()
        for i in range(0, len(data), 1 << 16):
            self.wfile.write(data[i:i + (1 << 16)])
            ahead = start + (i + (1 << 16)) / RATE - time.monotonic()
            if ahead > 0:
                time.sleep(ahead)

    def log_message(self, *a):
        pass


def run(tag, script, lib, cache, url):
    out = work / tag
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir()
    (out / "script.txt").write_text(script)
    env = {**os.environ, "QUIET_MD_FONT_URL": url, "QUIET_MD_FONT_DIR": str(cache), "QUIET_MD_TRIAL_FONTS": "1"}
    subprocess.run([sys.executable, str(here / "run.py"), str(out / "script.txt"), str(lib), str(out)], env=env, check=True)
    assert "panicked" not in (out / "log").read_text(errors="replace"), f"{tag}: panic"
    return out


def prefs(lib):
    return json.loads((lib / ".quiet/preferences.json").read_text())


def doc_family(lib):
    """The document font the app opens `lib` with: the top-level folder's look
    (settings.rs `load`, W21): the newest `look.` fields of `_root` in the
    devices' meta files, when there are any, else preferences.json."""
    look = {}
    for f in (lib / ".quiet/meta").glob("*.json"):
        for k, (v, t) in json.loads(f.read_text()).get("items", {}).get("_root", {}).items():
            if k.startswith("look.") and t >= look.get(k, (None, -1))[1]:
                look[k] = (v, t)
    look = {k[5:]: v for k, (v, t) in look.items() if v is not None}
    return look.get("doc_family", "") if look else prefs(lib).get("doc_family", "")


def base_library():
    """lib_clean as the app migrates it, in Chinese, holding only the article."""
    src = work / "base"
    if src.exists():
        return src
    first = run("migrate", "wait 1500\n", here / "lib_clean", work / "no-cache", "http://127.0.0.1:9/")
    shutil.copytree(first / "lib", src)
    shutil.rmtree(src / ".quiet/before-folders-4", ignore_errors=True)
    for p in src.glob("*.xml"):
        p.unlink()
    p = src / ".quiet/preferences.json"
    p.write_text(json.dumps({**prefs(src), "language": "zh-CN", "english": False}, ensure_ascii=False, indent=2))
    (src / "阅读字体样张.md").write_text(ARTICLE, encoding="utf-8")
    return src


def with_prefs(lib, name, **kv):
    dst = work / name
    shutil.rmtree(dst, ignore_errors=True)
    shutil.copytree(lib, dst)
    p = dst / ".quiet/preferences.json"
    p.write_text(json.dumps({**prefs(dst), **kv}, ensure_ascii=False, indent=2))
    return dst


# 设置 › 外观 › 文档字体's list, searched down to 霞鹜文楷 (it sits below the
# fold otherwise); its row and its delete button, by name
OPEN_LIST = "key cmd-,\nwait 600\nrow 外观\nwait 600\nanchor doc-font-picker\nwait 600\ncommit 霞鹜\nwait 400\n"
WENKAI_ROW, WENKAI_TRASH = "row 霞鹜文楷", "anchor font-del-lxgw-wenkai"
BACK = "key escape\nwait 300\nrow 返回\nwait 800\n"


def flow(url):
    lib = base_library()
    cache = work / "cache"
    shutil.rmtree(cache, ignore_errors=True)
    wenkai = next(e for e in catalog if e["id"] == "lxgw-wenkai")

    a = run("1-download", "move 700 700\nwait 1500\n" + OPEN_LIST + f"shot 1a-list\n{WENKAI_ROW}\nwait 2500\nshot 1b-progress\nwait 6000\nshot 1c-done\n" + BACK + "move 1250 780\nwait 500\nshot 1d-article\n", lib, cache, url)
    assert sorted(hits) == sorted(f"/{f['name']}" for f in wenkai["files"]), hits
    assert doc_family(a / "lib") == "LXGW WenKai", "kept for the next launch"
    assert all((cache / f["name"]).stat().st_size == f["size"] for f in wenkai["files"])

    hits.clear()
    b = run("2-restart", "move 1250 780\nwait 2500\nshot 2-article\n", a / "lib", cache, url)
    assert hits == [], f"the cache answers: {hits}"
    assert "font lxgw-wenkai ready" in (b / "log").read_text(errors="replace")

    gone = work / "cache-gone"
    shutil.rmtree(gone, ignore_errors=True)
    c = run("3-missing", "move 1250 780\nwait 2500\nshot 3a-article\n" + "key cmd-,\nwait 600\nrow 外观\nwait 800\nshot 3b-settings\n", a / "lib", gone, url)
    assert hits == [] and doc_family(c / "lib") == "LXGW WenKai", "kept, not fetched behind the reader's back"

    d = run("4-delete", "move 700 700\nwait 2500\n" + OPEN_LIST + f"{WENKAI_TRASH}\nwait 800\nshot 4-deleted\n", a / "lib", cache, url)
    assert doc_family(d / "lib") == "" and not any(cache.glob("lxgw-wenkai-*"))
    print(f"doc fonts: ok ({work})")


def compare(url, ids):
    lib = base_library()
    cache = work / "cache-all"
    cache.mkdir(exist_ok=True)
    shots = []
    for e in catalog:
        if ids and e["id"] not in ids:
            continue
        for f in e["files"]:
            if not (cache / f["name"]).exists():
                shutil.copy(stage / f["name"], cache / f["name"])
        tag = f"cmp-{e['id']}"
        out = run(tag, "move 1250 780\nwait 3000\nshot article\n", with_prefs(lib, f"lib-{e['id']}", doc_family=e["family"]), cache, url)
        shots.append((e, out / "article.png"))
    sheet(shots)


def sheet(shots):
    """compare.png: each shot's article, labelled, three to a row."""
    face = next((f for f in ("/System/Library/Fonts/Hiragino Sans GB.ttc", "/System/Library/Fonts/STHeiti Medium.ttc") if Path(f).exists()), None)
    label_font = ImageFont.truetype(face, 40) if face else None
    tiles = []
    for e, p in shots:
        im = Image.open(p).convert("RGB")
        w, h = im.size
        crop = im.crop((int(w * 0.27), int(h * 0.1), int(w * 0.95), int(h * 0.75)))
        tile = Image.new("RGB", (crop.width, crop.height + 80), "white")
        tile.paste(crop, (0, 80))
        tag = {"latin": " · 西文", "mono": " · 等宽"}.get(e.get("group"), "") + (" · 试用" if e.get("trial") else "")
        ImageDraw.Draw(tile).text((24, 18), f"{e['zh']}（{e['family']}）{tag}", fill=(200, 40, 40), font=label_font)
        tiles.append(tile.resize((tile.width // 2, tile.height // 2)))
    cols = 3
    tw, th = max(t.width for t in tiles), max(t.height for t in tiles)
    sheet = Image.new("RGB", (cols * tw, -(-len(tiles) // cols) * th), (230, 230, 230))
    for i, t in enumerate(tiles):
        sheet.paste(t, ((i % cols) * tw, (i // cols) * th))
    sheet.save(work / "compare.png")
    print(f"compare: {work / 'compare.png'}")


def main():
    if not any(stage.glob("*.ttf")):
        subprocess.run([sys.executable, str(repo / "tools/fonts/stage.py"), "--out", str(stage)], check=True)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Host)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    url = f"http://127.0.0.1:{server.server_address[1]}/"
    if "--compare" in sys.argv:
        compare(url, set(sys.argv[sys.argv.index("--compare") + 1:]))
    else:
        flow(url)


if __name__ == "__main__":
    main()
