"""What the vendored GPUI patches give the app (vendor/PATCHES.md), checked in
the running app: text layout, scrollbars, scrolling, image memory, custom
shortcuts and first launch.

usage: python3 tools/harness/gpui_patches.py [case ...]
  FDL_BIN      the build under test (default target/release/quiet_md, built with --features devtools)
  FDL_REF_BIN  a reference build, e.g. the one before an upstream sync: every
               case is run with both and their layouts compared
Without FDL_REF_BIN only the checks that need no reference run. Needs Pillow.
Scratch libraries only; no system clipboard, no dialogs, no focus stealing.
"""
import json, os, subprocess, sys, tempfile, threading, time
from pathlib import Path
import xml.etree.ElementTree as ET

from hlib import doc_file
from PIL import Image, ImageChops, ImageOps

here = Path(__file__).resolve().parent
repo = here.parent.parent
BIN = os.environ.get("FDL_BIN") or str(repo / "target/release/quiet_md")
REF = os.environ.get("FDL_REF_BIN")
work = Path(tempfile.mkdtemp(prefix="quiet-gpui-patches-"))
doc_id = "aaaaaaaa-0000-0000-0000-000000000001"
# Device pixels per pt in the captures: the screen's backing scale (2 on a Retina display, 1 on others),
# read from the first run's 1280pt-wide `scale` shot.
S = None


def prefs(lib, appearance="light"):
    """Light unless asked (the ink checks read dark pixels on a light page, whatever the system look); no
    "updated to" card over the sidebar, no updater, no global hotkey."""
    (lib / "preferences.json").write_text(json.dumps({"appearance": appearance, "seen_version": "9.9.9", "auto_update": False, "quick_hotkey": False}))


def library(name, xml, extra=None):
    """A one-document library (old layout; the app moves it to FORMAT 4)."""
    lib = work / "src" / name
    (lib / "docs").mkdir(parents=True, exist_ok=True)
    prefs(lib)
    (lib / "index.json").write_text(json.dumps({doc_id: {"created": 1, "modified": 1, "opened": 1}}))
    (lib / "docs" / f"{doc_id}.xml").write_text(xml)
    if extra: extra(lib)
    return lib


def run(binary, tag, lib, steps, env=None):
    """Run `steps` against a fresh copy of `lib`; returns the folder and the peak / settled (median of the
    last second) RSS in MB."""
    folder = work / tag
    script = folder.with_suffix(".txt")
    script.write_text("resize 1280 800\nwait 1500\nmove 1270 790\nwait 300\nshot scale\n" + steps + "\n")
    proc = subprocess.Popen([sys.executable, str(here / "run.py"), str(script), str(lib), str(folder)],
                            env=dict(os.environ, FDL_BIN=binary, FDL_NOBLINK="1", **(env or {})))
    rss = []
    while proc.poll() is None:
        out = subprocess.run(["pgrep", "-f", "-n", binary], capture_output=True, text=True).stdout.split()
        if out:
            kb = subprocess.run(["ps", "-o", "rss=", "-p", out[0]], capture_output=True, text=True).stdout.strip()
            if kb and int(kb) > 0: rss.append(int(kb) / 1024)
        time.sleep(0.1)
    assert proc.returncode == 0, f"{tag}: run.py failed"
    global S
    S = S or Image.open(folder / "scale.png").width // 1280
    tail = sorted(rss[-10:])
    return folder, (max(rss) if rss else 0, tail[len(tail) // 2] if tail else 0)


def bands(path, x0=280):
    """Rows of ink in the document pane (left of x0 pt is the sidebar): [(top, bottom, left, right, ink)] in device px; ink in fully dark pixels
    (light ones on a dark page)."""
    im = Image.open(path).convert("L")
    if im.getpixel((im.width - 4, im.height - 4)) < 128: im = ImageOps.invert(im)
    s = S
    box = (round(x0 * s), round(60 * s), im.width - round(14 * s), im.height)
    px = im.crop(box).load()
    w, h = box[2] - box[0], box[3] - box[1]
    rows, out = [], []
    for y in range(h):
        xs = [x for x in range(w) if px[x, y] < 200]
        # Ink is total darkness, not a pixel count: anti-aliased edges move a
        # little with sub-pixel glyph positions.
        rows.append((xs[0], xs[-1], sum(255 - px[x, y] for x in range(w)) // 255) if xs else None)
    y = 0
    while y < h:
        if rows[y] is None: y += 1; continue
        top = y
        l, r, ink = rows[y]
        while y < h and rows[y] is not None:
            l, r, ink = min(l, rows[y][0]), max(r, rows[y][1]), ink + rows[y][2]
            y += 1
        out.append((top, y, l, r, ink))
    # A row cut by the pane's top or the window's bottom edge depends on where the clip lands; leave it out.
    if out and out[-1][1] >= h - 1: out.pop()
    if out and out[0][0] == 0: out.pop(0)
    return out


def same_layout(a, b, tol=3):
    """Layouts match: same rows of text, each within `tol` device px and 4% of ink (20 px for a few glyphs).
    Rows at a fractional vertical position (table cells) differ by up to 3% across the upgrade: upstream
    no longer rasterizes vertical sub-pixel glyph variants."""
    ba, bb = bands(a), bands(b)
    if len(ba) != len(bb): return f"{len(ba)} rows vs {len(bb)}"
    for i, (p, q) in enumerate(zip(ba, bb)):
        if abs(p[0] - q[0]) > tol or abs(p[2] - q[2]) > tol or abs(p[3] - q[3]) > tol:
            return f"row {i}: {p[:4]} vs {q[:4]}"
        if abs(p[4] - q[4]) > max(0.04 * max(p[4], q[4]), 20):
            return f"row {i}: ink {p[4]} vs {q[4]}"
    return None


def ink(path, box):
    im = Image.open(path).convert("L").crop([round(v * S) for v in box])
    return sum(255 - v for v in im.getdata())


results = []


def report(case, patches, ok, detail):
    results.append((case, patches, ok, detail))
    print(f"{'PASS' if ok else 'FAIL'} {case} [{patches}] {detail}", flush=True)


def compare(case, patches, shots, steps, lib, env=None, extra_check=None):
    """Run `steps` with the build (and the reference); compare the named shots' layouts."""
    new, mem = run(BIN, case + "-new", lib, steps, env)
    detail = []
    ok = True
    if extra_check:
        err = extra_check(new)
        if err: ok = False; detail.append(err)
    if REF:
        old, ref_mem = run(REF, case + "-ref", lib, steps, env)
        for s in shots:
            err = same_layout(old / f"{s}.png", new / f"{s}.png")
            if err: ok = False; detail.append(f"{s}: {err}")
        if extra_check:
            err = extra_check(old)
            if err: detail.append(f"(reference fails too: {err})")
    report(case, patches, ok, "; ".join(detail) or f"{len(shots)} shots match" + ("" if REF else " (no reference)"))
    return new


WIDTHS = "shot w1280\nresize 1000 800\nwait 800\nmove 990 790\nwait 300\nshot w1000\nresize 760 760\nwait 800\nmove 750 750\nwait 300\nshot w760"
W = ["w1280", "w1000", "w760"]

TEXT = {
    # 0001: big title glyphs keep their left edge (optical-size switch).
    "title": ("0001", '<title>Wj fjord Typography</title>\n<h1>Wj Large Title 大标题 Typography</h1>\n'
              '<h2>fjord Ágape 副标题</h2>\n<p>Body text under the titles.</p>\n'),
    # 0002: KaTeX symbol faces without an "m".
    "katex": ("0002", '<title>Formulas</title>\n'
              '<p><latex>\\mathcal{L}\\{f\\}(s) + \\mathscr{F} + \\mathbb{R}^n + \\mathfrak{g}</latex></p>\n'
              '<p><latex>\\left( \\sum_{i=1}^{n} \\frac{a_i}{b_i} \\right)^{2} = \\int_0^1 \\sqrt{x^2+1}\\,dx</latex></p>\n'
              '<p><latex>\\Bigg\\{ \\bigg[ \\Big( \\big( x \\big) \\Big) \\bigg] \\Bigg\\}</latex></p>\n'),
    # 0003 / 0016 / 0017: CJK fallback face and stem weight.
    "cjk": ("0003 0016 0017", '<title>中文排版</title>\n'
            '<p>这是一段中文正文，用来比较字形和笔画粗细：永和九年，岁在癸丑，暮春之初，会于会稽山阴之兰亭，修禊事也。</p>\n'
            '<p><b>加粗的中文正文，群贤毕至，少长咸集。</b></p>\n'
            '<p><span text-color="gray">灰色的中文正文：此地有崇山峻岭，茂林修竹。</span></p>\n'
            '<h2>二级标题 中文</h2>\n'),
    # 0004: synthetic oblique for PingFang.
    "italic": ("0004", '<title>Italic</title>\n<p><em>中文斜体文字与汉字</em></p>\n<p>中文斜体文字与汉字</p>\n'
               '<p><b><em>粗斜体中文</em></b> <em>English italic</em></p>\n'),
    # 0005: explicit column widths.
    "table": ("0005", '<title>Table</title>\n<table><colgroup><col width="85"/><col width="300"/><col width="150"/></colgroup>'
              '<tbody><tr><td><p>A</p></td><td><p>B</p></td><td><p>C</p></td></tr>'
              '<tr><td><p>1</p></td><td><p>2</p></td><td><p>3</p></td></tr></tbody></table>\n'),
    # 0006: rows with an inline formula are taller.
    "rows": ("0006", '<title>Rows</title>\n<p>A paragraph long enough to wrap onto several rows, with a tall inline formula '
             '<latex>\\frac{\\sum_{i=1}^{n} a_i}{\\prod_{j=1}^{m} b_j}</latex> in the middle of it, so that the row holding it '
             'is taller than the rows above and below. More words follow to fill another row or two at every width we try, '
             'and a second formula <latex>\\sqrt{\\frac{x}{y}}</latex> near the end.</p>\n<p>Next paragraph.</p>\n'),
    # 0019: Korean wraps by word.
    "korean": ("0019", '<title>한국어</title>\n<p>한국어 문서를 편집합니다. 이 문장은 줄 바꿈이 어절 단위로 되는지 확인하기 위한 아주 긴 문장입니다. '
               'Quiet.md를 써요. 다시 한 번 더 긴 문장을 써서 여러 줄로 나뉘는지 확인합니다.</p>\n'),
    # 0020 (editor rows in reading order) and #31 (words stay whole).
    "rtl": ("0020 #31", '<title>RTL</title>\n<p>هذا نص عربي طويل لاختبار التفاف الأسطر في المحرر وفي واجهة البرنامج، ويجب أن تبقى الكلمات كاملة عند الالتفاف. '
            'ونضيف جملة أخرى حتى يلتف النص على أكثر من سطرين في كل عرض.</p>\n'
            '<p>שלום עולם, זהו טקסט עברי ארוך כדי לבדוק גלישת שורות בעורך ובממשק, ועוד משפט כדי שיהיו כמה שורות.</p>\n'),
    # #4: closing punctuation never starts a row, opening punctuation never ends one.
    "kinsoku": ("#4", '<title>避头尾</title>\n'
                '<p>这是一段很长的中文段落，用来测试避头尾规则是否生效：句号、逗号和右括号不应出现在行首，左括号（例如这里）不应出现在行尾。'
                '“引号里的内容”也一样……最后再加一些文字让它多折几行，看看换行位置是否和以前一样。</p>\n'
                '<p>一二三四五六七八九十，一二三四五六七八九十。一二三四五六七八九十」一二三四五六七八九十）一二三四五六七八九十、'
                '一二三四五六七八九十！一二三四五六七八九十？一二三四五六七八九十；一二三四五六七八九十：「一二三四五六七八九十（一二三四五六七八九十。</p>\n'),
}


def italic_leans(folder):
    """The italic row's ink sits further right at the top than the upright copy's (a 14° shear)."""
    b = bands(folder / "w1280.png")
    if len(b) < 3: return f"expected the title and two rows, got {len(b)}"
    im = Image.open(folder / "w1280.png").convert("L")
    def lean(band):
        top, bot, l, r, _ = band
        mid = (top + bot) // 2
        x0 = 280 * S
        def first(y0, y1):
            xs = [x for y in range(y0, y1) for x in range(x0 + l, x0 + r + 1) if im.getpixel((x, y + 60 * S)) < 200]
            return sum(xs) / len(xs) if xs else 0
        return first(top, mid) - first(mid, bot)
    it, up = lean(b[1]), lean(b[2])
    return None if it - up > S else f"italic row leans {it:.1f}px, upright {up:.1f}px"


def column_widths(folder):
    """Vertical table borders 85 / 300 / 150 pt apart."""
    im = Image.open(folder / "w1280.png").convert("RGB")
    b = [x for x in bands(folder / "w1280.png") if x[1] - x[0] > 10]
    y = b[1][1] + 60 * S + 2 * S  # in the first row's padding, under its text
    xs = [x for x in range(280 * S, im.width - 20 * S) if max(im.getpixel((x, y))) < 235 and min(im.getpixel((x, y))) > 150]
    runs = []
    for x in xs:
        if runs and runs[-1][-1] >= x - 1: runs[-1].append(x)
        else: runs.append([x])
    edges = [r[0] / S for r in runs]
    gaps = [round(edges[i + 1] - edges[i]) for i in range(len(edges) - 1)]
    want = [85, 300, 150]
    for i in range(len(gaps) - 2):
        if all(abs(gaps[i + k] - want[k]) <= 2 for k in range(3)): return None
    return f"border gaps {gaps}, want {want}"


def text_cases():
    for name, (patches, xml) in TEXT.items():
        check = {"italic": italic_leans, "table": column_widths}.get(name)
        compare(name, patches, W, WIDTHS, library(name, xml), extra_check=check)


def dark_cjk():
    """0016: the same CJK text in the dark theme (smoothing does not depend on the colour)."""
    lib = library("cjk-dark", TEXT["cjk"][1], lambda l: prefs(l, "dark"))
    compare("cjk-dark", "0016", ["w1280"], "shot w1280", lib)


def home_rtl():
    """0021: a wrapped right-to-left description on Home sits against the right edge."""
    xml = TEXT["rtl"][1]
    compare("home-rtl", "0021", ["home"], "click 66 67\nwait 1200\nmove 1270 790\nwait 300\nshot home", library("home-rtl", xml))


# --- behaviour -------------------------------------------------------------

def long_doc():
    import random
    rnd = random.Random(7)
    words = "the quick brown fox jumps over a lazy dog while 文档 编辑器 测试 滚动条 长度 估计".split()
    paras = []
    for i in range(400):
        n = rnd.choice([3, 8, 20, 60, 120])
        paras.append(f"<p>{i}. " + " ".join(rnd.choice(words) for _ in range(n)) + "</p>")
        if i % 37 == 5: paras.append(f"<h2>Section {i}</h2>")
    return "<title>Long</title>\n" + "\n".join(paras) + "\n"


def thumb(path):
    """The scrollbar thumb: (top, height) in pt, from the column near the right edge."""
    im = Image.open(path).convert("L")
    best = None
    for x in range(1270 * S, 1280 * S):
        ys = [y for y in range(0, im.height - 4) if im.getpixel((x, y)) < 230]
        if len(ys) > 20 and (best is None or len(ys) > best[1] - best[0]):
            best = (ys[0], ys[-1])
    return None if best is None else (round(best[0] / S), round((best[1] - best[0]) / S))


def scrollbar():
    """0007: the thumb of a long document (rows measured only once seen) has a usable size and position;
    dragging it scrolls to the estimated offset."""
    lib = library("scrollbar", long_doc())
    # The thumb starts at the top of the pane (y 52-84); drag it down 280pt.
    steps = ("wheel 700 400 200\nwait 150\nshot s0\nmove 1275 68\nwait 300\nshot s1\n"
             "drag 1275 68 1275 348\nwait 600\nshot s2\nwheel 700 400 200\nwait 150\nshot s3")
    def check(folder):
        t = [thumb(folder / f"s{i}.png") for i in range(4)]
        if any(v is None for v in t[1:3]): return f"no thumb found: {t}"
        if not (8 <= t[1][1] <= 400): return f"thumb height {t[1][1]}"
        if abs(t[2][0] - (t[1][0] + 280)) > 12: return f"thumb did not follow the drag: {t}"
        return None
    new, _ = run(BIN, "scrollbar-new", lib, steps)
    err = check(new)
    detail = [f"thumbs {[thumb(new / f's{i}.png') for i in range(4)]}"]
    if REF:
        old, _ = run(REF, "scrollbar-ref", lib, steps)
        to = [thumb(old / f"s{i}.png") for i in range(4)]
        tn = [thumb(new / f"s{i}.png") for i in range(4)]
        detail.append(f"reference {to}")
        for a, b in zip(to, tn):
            if a and b and (abs(a[0] - b[0]) > 6 or abs(a[1] - b[1]) > 6): err = err or f"thumb {b} vs reference {a}"
        for s in ("s2",):
            e = same_layout(old / f"{s}.png", new / f"{s}.png")
            if e: err = err or f"{s}: {e}"
    report("scrollbar", "0007", err is None, (err + "; " if err else "") + "; ".join(detail))


def wide_table():
    """0022: over a table wider than the page, a vertical wheel (with a little sideways drift)
    scrolls the page, not the table; a sideways gesture scrolls the table."""
    cols = "".join(f'<col width="220"/>' for _ in range(10))
    cells = lambda r: "".join(f"<td><p>R{r}C{c}</p></td>" for c in range(10))
    rows = "".join(f"<tr>{cells(r)}</tr>" for r in range(6))
    filler = "\n".join(f"<p>Line {i} below the table.</p>" for i in range(60))
    xml = f"<title>Wide</title>\n<p>Above.</p>\n<table><colgroup>{cols}</colgroup><tbody>{rows}</tbody></table>\n{filler}\n"
    steps = ("shot a0\nwheel 700 260 -40\nwait 200\nwheel 700 260 60 4\nwait 300\nshot a1\n"
             "wheel 700 200 0 300\nwait 300\nshot a2\nwheel 700 200 0 0\nwait 100\nwheel 700 200 0 -120\nwait 300\nshot a3")
    def first_cell_x(path):
        # Left edge of the "R0C0" / first visible cell text: the leftmost ink in the table rows.
        b = [x for x in bands(path) if x[3] - x[2] > 300]
        return b[1][2] if len(b) > 1 else None
    def check(folder):
        b0, b1 = bands(folder / "a0.png"), bands(folder / "a1.png")
        if b0[0][0] == b1[0][0]: return "the vertical wheel did not scroll the page"
        x1, x2 = first_cell_x(folder / "a1.png"), first_cell_x(folder / "a2.png")
        if x1 is None or x2 is None: return "table rows not found"
        if x2 == x1 and same_layout(folder / "a1.png", folder / "a2.png") is None: return "the sideways gesture did not scroll the table"
        return None
    compare("wide-table", "0022", ["a1", "a2", "a3"], steps, library("wide", xml), extra_check=check)


def images():
    """0013 / 0014: a document of 30 large images: peak and settled memory while scrolling through it."""
    import random
    def extra(lib):
        (lib / "assets").mkdir(exist_ok=True)
        rnd = random.Random(3)
        for i in range(30):
            im = Image.new("RGB", (2400, 1600), (rnd.randrange(256), rnd.randrange(256), rnd.randrange(256)))
            for k in range(40):
                x, y = rnd.randrange(2400), rnd.randrange(1600)
                im.paste((rnd.randrange(256), rnd.randrange(256), rnd.randrange(256)), (x, y, min(2400, x + 300), min(1600, y + 200)))
            im.save(lib / "assets" / f"img{i}.png")
    xml = "<title>Images</title>\n" + "\n".join(f'<p>Image {i}</p>\n<img src="assets/img{i}.png" width="600" height="400"/>' for i in range(30)) + "\n"
    steps = "wait 1500\n" + "\n".join("wheel 700 400 400\nwait 120" for _ in range(60)) + "\nwait 1500\nshot bottom\n" + \
            "\n".join("wheel 700 400 -400\nwait 120" for _ in range(60)) + "\nwait 2500\nshot top"
    lib = library("images", xml, extra)
    def colourful(path):
        # Share of saturated pixels: the images drew (not "File not found" placeholders).
        im = Image.open(path).convert("HSV").resize((320, 200))
        return sum(1 for h, sat, v in im.getdata() if sat > 80) / (320 * 200)
    new, mem = run(BIN, "images-new", lib, steps)
    detail = f"peak {mem[0]:.0f} MB, settled {mem[1]:.0f} MB"
    ok = all(colourful(new / f"{s}.png") > 0.1 for s in ("top", "bottom"))
    if not ok: detail += "; images did not draw"
    if REF:
        old, ref_mem = run(REF, "images-ref", lib, steps)
        detail += f"; reference peak {ref_mem[0]:.0f} MB, settled {ref_mem[1]:.0f} MB"
        # Either build holds two or three window drawables (16 MB each at 2560x1600) from run
        # to run, so allow one drawable on top of 10%.
        if mem[0] > ref_mem[0] * 1.1 + 17 or mem[1] > ref_mem[1] * 1.1 + 17: ok = False
        for s in ("top", "bottom"):
            e = same_layout(old / f"{s}.png", new / f"{s}.png")
            if e: ok = False; detail += f"; {s}: {e}"
    report("images", "0013 0014 #21 #33", ok, detail)


def home_images():
    """0014: Home cards of 12 documents, each with images below its card's clip: memory and the cards' look."""
    import random
    lib = work / "src" / "home-images"
    (lib / "docs").mkdir(parents=True, exist_ok=True)
    (lib / "assets").mkdir(exist_ok=True)
    rnd = random.Random(5)
    index = {}
    for d in range(12):
        did = f"bbbbbbbb-0000-0000-0000-{d:012d}"
        index[did] = {"created": 1 + d, "modified": 1 + d, "opened": 0}
        imgs = []
        for k in range(4):
            name = f"d{d}k{k}.png"
            Image.new("RGB", (2400, 1600), (rnd.randrange(256), rnd.randrange(256), rnd.randrange(256))).save(lib / "assets" / name)
            imgs.append(f'<p>Paragraph {k} of document {d}.</p>\n<img src="assets/{name}" width="600" height="400"/>')
        (lib / "docs" / f"{did}.xml").write_text(f"<title>Doc {d}</title>\n" + "\n".join(imgs) + "\n")
    (lib / "index.json").write_text(json.dumps(index))
    prefs(lib)
    steps = "click 66 67\nwait 3000\nmove 1270 790\nwait 300\nshot home\nwheel 700 400 600\nwait 1500\nshot home2"
    new, mem = run(BIN, "home-images-new", lib, steps)
    detail, ok = f"peak {mem[0]:.0f} MB, settled {mem[1]:.0f} MB", True
    if REF:
        old, ref_mem = run(REF, "home-images-ref", lib, steps)
        detail += f"; reference peak {ref_mem[0]:.0f} MB, settled {ref_mem[1]:.0f} MB"
        if mem[0] > ref_mem[0] * 1.1 + 17 or mem[1] > ref_mem[1] * 1.1 + 17: ok = False
        for s in ("home", "home2"):
            e = same_layout(old / f"{s}.png", new / f"{s}.png")
            if e: ok = False; detail += f"; {s}: {e}"
    report("home-images", "0014", ok, detail)


def lists():
    """uniform_list padding (GPUI 0.2 counted it twice): Home's grid and a long catalogue end where they did."""
    xml = "<title>Outline</title>\n" + "\n".join(f"<h2>Heading {i}</h2>\n<p>Text {i}.</p>" for i in range(80)) + "\n"
    def extra(lib):
        idx = json.loads((lib / "index.json").read_text()); idx[doc_id]["catalogue_closed"] = False
        (lib / "index.json").write_text(json.dumps(idx))
    steps = "shot c0\nwheel 340 500 3000\nwait 400\nwheel 340 500 3000\nwait 400\nmove 1100 300\nwait 600\nshot c1"
    compare("catalogue", "uniform_list", ["c0", "c1"], steps, library("catalogue", xml, extra))


def shortcuts():
    """0028: a custom shortcut from shortcuts.json is parsed as written (Bold on cmd-shift-k)."""
    def extra(lib):
        (lib / "shortcuts.json").write_text(json.dumps({"editor::Bold": "cmd-shift-k"}))
    xml = "<title>Keys</title>\n<p>plain words</p>\n"
    steps = "click 420 186\nwait 200\nkey cmd-a\nkey cmd-shift-k\nwait 300\nkey escape\nwait 600\nshot k"
    def check(folder):
        doc = ET.fromstring("<doc>" + doc_file(folder / "lib").read_text() + "</doc>")
        return None if doc.find("p/b") is not None else "custom Bold shortcut did nothing: " + ET.tostring(doc, encoding="unicode")[:200]
    compare("shortcuts", "0028", [], steps, library("keys", xml, extra), extra_check=check)


def first_launch():
    """0010: first launch replaces the onboarding view with the workspace in the same window;
    afterwards the window is found as the workspace: ⌘Q right after typing saves the new
    document through `app::quit`, which looks the window up with downcast::<Workspace>()."""
    lib = work / "src" / "empty"
    lib.mkdir(parents=True, exist_ok=True)
    steps = ("shot o0\n" + "\n".join("key enter\nwait 600" for _ in range(8)) + "\nwait 1500\nshot o1\n"
             # Keys reach the workspace once a click has focused it (both builds).
             "click 66 67\nwait 800\nkey cmd-n\nwait 800\ntype FIRSTLAUNCH\nkey cmd-q\nwait 2000")
    def check(folder):
        found = [p for p in (folder / "lib").rglob("*.xml") if ".quiet" not in p.parts and "FIRSTLAUNCH" in p.read_text()]
        return None if found else "the document typed before ⌘Q was not saved"
    compare("first-launch", "0010", ["o1"], steps, lib, env={"QUIET_MD_ONBOARDING": "1"}, extra_check=check)


def nested_deferred():
    """A deferred inside a deferred (#327): the board's canvas background picker opens from the main menu.
    GPUI 0.2 asserted "cannot call defer_draw during deferred drawing" and the app aborted."""
    xml = "<title>Board host</title>\n<p>text</p>\n"
    # sidebar + › Board (opens the new board in the note pane), the main menu, the picker's trigger: the last
    # of 画布背景's row of swatches, after the five presets. Coordinates, not `anchor`: the reference build
    # (FDL_REF_BIN) may predate it.
    steps = ("click 250 136\nwait 300\nclick 295 236\nwait 1200\nclick 314 74\nwait 400\nshot n0\n"
             "move 505 227\nwait 1200\nclick 505 227\nwait 500\nshot n1\nwait 300")
    def check(folder):
        log = (folder / "log").read_text(errors="replace")
        if "PANIC" in log: return "panicked: " + log[log.index("PANIC"):][:200]
        if not (folder / "n1.png").exists(): return "the app quit before the picker showed"
        # the picker's island (the hex field) right of the menu
        box = (520, 200, 700, 280)
        return None if abs(ink(folder / "n1.png", box) - ink(folder / "n0.png", box)) > 2000 else "the picker did not open"
    compare("nested-deferred", "upstream GPUI", [], steps, library("board", xml), extra_check=check)


CASES = {"text": text_cases, "dark": dark_cjk, "home": home_rtl, "scrollbar": scrollbar, "table": wide_table,
         "images": images, "home_images": home_images, "lists": lists, "shortcuts": shortcuts, "first_launch": first_launch,
         "nested_deferred": nested_deferred}

if __name__ == "__main__":
    for name in sys.argv[1:] or CASES:
        CASES[name]()
    print(f"\n{sum(r[2] for r in results)}/{len(results)} passed; shots in {work}")
    sys.exit(0 if all(r[2] for r in results) else 1)
