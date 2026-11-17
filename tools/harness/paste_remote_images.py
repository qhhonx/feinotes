"""Web images pasted from a page (Feishu, Notion…) become owned assets (#220).

Build with --release --features devtools, then run this file. Needs Pillow.
FDL_BIN selects another build. A local HTTP server stands in for the web
page's image host; the fake clipboard carries the HTML. No system clipboard
or user library is accessed.
"""
import http.server
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import xml.etree.ElementTree as ET

from PIL import Image

here = Path(__file__).resolve().parent
work = Path(tempfile.mkdtemp(prefix="quiet-paste-remote-"))
serve = work / "serve"
serve.mkdir()
pixels = Image.new("RGBA", (48, 24), (200, 60, 40, 255))
pixels.save(serve / "pic.png")
hits = []


class Host(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *a, **k):
        super().__init__(*a, directory=str(serve), **k)

    def do_GET(self):
        hits.append(self.path)
        # An expired signed link: the host answers 403.
        if "expired" in self.path:
            self.send_error(403)
            return
        super().do_GET()

    def log_message(self, *a):
        pass


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Host)
threading.Thread(target=server.serve_forever, daemon=True).start()
base = f"http://127.0.0.1:{server.server_address[1]}"
doc_id = "aaaaaaaa-0000-0000-0000-000000000001"


def library(remote_images):
    source = work / f"source-{remote_images}"
    if source.exists():
        return source
    (source / "docs").mkdir(parents=True)
    (source / "index.json").write_text(json.dumps({doc_id: {"created": 1, "modified": 1, "opened": 1}}))
    (source / "preferences.json").write_text(json.dumps({
        "appearance": "light", "english": True, "seen_version": "0.1.9",
        "auto_update": False, "quick_hotkey": False, "remote_images": remote_images,
    }))
    (source / "docs" / f"{doc_id}.xml").write_text("<title>Remote paste</title>\n<p>BeforeAfter</p>\n")
    return source


def saved(folder):
    for p in (folder / "lib").rglob("*.xml"):
        text = p.read_text(errors="ignore")
        if "Remote paste" in text:
            return text
    sys.exit(f"saved document not found in {folder}")


def run(name, src, finish="", remote_images=True):
    folder = work / name
    script = work / f"{name}.txt"
    (work / f"{name}.html").write_text(f'<p>Copied from a web page</p><img src="{src}"><p>Tail</p>')
    # The only document opens automatically; Find puts the caret mid-word
    # without depending on font metrics.
    script.write_text(f"""resize 1200 850
wait 1200
key cmd-f
type BeforeAfter
key escape
key left
key right
key right
key right
key right
key right
key right
clip clear
clip html {work / f"{name}.html"}
key cmd-v
wait 2500
{finish}
wait 1500
move 1100 700
shot result
""")
    hits.clear()
    subprocess.run([sys.executable, str(here / "run.py"), str(script), str(library(remote_images)), str(folder)],
                   env=dict(os.environ, FDL_NOBLINK="1"), check=True, timeout=90)
    text = saved(folder)
    images = ET.fromstring("<doc>" + text + "</doc>").findall(".//img")
    assert len(images) == 1, f"{name}: expected one image; {text}; see {folder}"
    return images[0].attrib.get("src") or images[0].attrib.get("href"), folder


def owned(name, src, folder):
    assert not src.startswith("http"), f"{name}: still a web link: {src}; see {folder}"
    path = Path(src)
    if not path.is_absolute():
        path = folder / "lib" / path
    assert path.suffix == ".png" and path.is_file(), f"{name}: no owned PNG at {path}"
    assert folder / "lib" in path.parents, f"{name}: asset outside the library: {path}"
    assert list(Image.open(path).convert("RGBA").getdata()) == list(pixels.getdata()), f"{name}: pixels changed"
    print(f"{name}: passed ({path.name})", flush=True)


print(f"Artifacts: {work}", flush=True)
url = f"{base}/pic.png?code=abc123&expires=1"
src, folder = run("adopt", url)
owned("adopt", src, folder)
assert hits, "adopt: the image host was never asked"

# Undo then redo the paste: the download, not the expiring link, comes back.
src, folder = run("undo_redo", url, finish="key cmd-z\nwait 300\nkey cmd-shift-z")
owned("undo_redo", src, folder)

# 网络图片 off: nothing goes out, the link stays a link.
src, folder = run("offline", url, remote_images=False)
assert src == url and not hits, f"offline: {src}, requests {hits}; see {folder}"
print("offline: passed", flush=True)

# A link that has already expired stays a link (shown as failed).
bad = f"{base}/pic.png?expired=1"
src, folder = run("expired", bad)
assert src == bad, f"expired: {src}; see {folder}"
print("expired: passed", flush=True)
server.shutdown()
