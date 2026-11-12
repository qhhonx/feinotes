"""Screenshot paste regression, using a fake clipboard and temporary libraries.

Build with --release --features devtools, then run this file. Needs Pillow.
FDL_BIN selects another build. No system clipboard or user library is accessed.
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
work = Path(tempfile.mkdtemp(prefix="quiet-clipboard-images-"))
source = work / "source"
(source / "docs").mkdir(parents=True)
doc_id = "aaaaaaaa-0000-0000-0000-000000000001"
(source / "index.json").write_text(json.dumps({doc_id: {"created": 1, "modified": 1, "opened": 1}}))
(source / "preferences.json").write_text(json.dumps({
    "appearance": "light", "english": True, "seen_version": "0.1.9",
    "auto_update": False, "quick_hotkey": False,
}))
original = "<title>Screenshot paste</title>\n<p>BeforeAfter</p>\n"
(source / "docs" / f"{doc_id}.xml").write_text(original)
pixels = Image.new("RGBA", (64, 32), (30, 120, 220, 180))
pixels.save(work / "screenshot.tiff")
pixels.save(work / "screenshot.png")
(work / "text.txt").write_text("screenshot.png")
(work / "rich.html").write_text("<p><b>Rich text</b></p>")


def run(name, clipboard, finish="", plain=False, expected_images=1, expected_text=None):
    folder = work / name
    script = work / f"{name}.txt"
    # The only document opens automatically. Select its body through
    # Find so the insertion position does not depend on font metrics.
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
{clipboard}
key {"cmd-shift-v" if plain else "cmd-v"}
wait 700
{finish}
wait 700
move 1100 700
shot result
""")
    subprocess.run([sys.executable, str(here / "run.py"), str(script), str(source), str(folder)],
                   env=dict(os.environ, FDL_NOBLINK="1"), check=True, timeout=60)
    saved = (folder / "lib" / "docs" / f"{doc_id}.xml").read_text()
    doc = ET.fromstring("<doc>" + saved + "</doc>")
    images = doc.findall(".//img")
    assert len(images) == expected_images, f"{name}: expected {expected_images} images; {saved}; see {folder}"
    if images:
        paragraphs = ["".join(p.itertext()) for p in doc.findall("p")]
        assert paragraphs == ["Before", "After"], f"{name}: caret text changed: {paragraphs}"
        for image in images:
            path = Path(image.attrib["src"])
            if not path.is_absolute():
                path = folder / "lib" / path
            assert path.suffix == ".png", f"{name}: image is not portable PNG"
            assert list(Image.open(path).convert("RGBA").getdata()) == list(pixels.getdata()), f"{name}: pixels changed"
    elif expected_text is not None:
        paragraphs = "".join("".join(p.itertext()) for p in doc.findall("p"))
        assert paragraphs == expected_text, f"{name}: unexpected text: {paragraphs}"
    print(f"{name}: passed", flush=True)


print(f"Artifacts: {work}", flush=True)
tiff = f"clip image {work / 'screenshot.tiff'}"
png_text = f"clip image {work / 'screenshot.png'}\nclip text {work / 'text.txt'}"
run("tiff", tiff)
run("png_with_text", png_text)
run("undo", tiff, finish="key cmd-z", expected_images=0, expected_text="BeforeAfter")
run("redo", tiff, finish="key cmd-z\nkey cmd-shift-z")
run("multiple", tiff + "\n" + png_text, expected_images=2)
run("plain", png_text, plain=True, expected_images=0, expected_text="Beforescreenshot.pngAfter")
run("rich_html", png_text + f"\nclip html {work / 'rich.html'}",
    expected_images=0, expected_text="BeforeRich textAfter")
