"""Board scenes for the 画板 renderer (docs/design/whiteboard-excalidraw.md, 渲染方案).

usage:
  python3 tools/perf/board_gen.py perf <n> <lib_dir>    n rounded rectangles with bound text on a grid,
                                                        an arrow bound between horizontal neighbours,
                                                        n/4 freedraw strokes of 60 points
  python3 tools/perf/board_gen.py showcase <lib_dir>    one of each rendering case: images (frame, rotated,
                                                        flipped, cropped, rounded), a rotated frame clipping its
                                                        children, arrow labels, dashes, fills, rotated text

Writes `<lib_dir>/bench-<n>.excalidraw` (or `showcase.excalidraw`) plus `.quiet/format.json`, so `lib_dir`
is a FORMAT 4 library for tools/harness/run.py. bench_board.py runs the pan / zoom benchmark over these.
"""
import base64, json, math, os, random, struct, sys, zlib

def base(id, ty, x, y, w, h, **kw):
    e = {"id": id, "type": ty, "x": x, "y": y, "width": w, "height": h, "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent",
         "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 0, "opacity": 100, "groupIds": [], "frameId": None,
         "roundness": None, "seed": random.randint(1, 2**31), "version": 1, "versionNonce": random.randint(1, 2**31), "isDeleted": False,
         "boundElements": [], "updated": 1731576000000, "link": None, "locked": False}
    e.update(kw)
    return e

def text(id, x, y, w, h, s, container=None, size=20, align="center", valign="middle", **kw):
    return base(id, "text", x, y, w, h, text=s, originalText=s, fontSize=size, fontFamily=5, textAlign=align, verticalAlign=valign,
                containerId=container, autoResize=True, lineHeight=1.25, **kw)

def arrow(id, x, y, pts, start=None, end=None, **kw):
    xs, ys = [p[0] for p in pts], [p[1] for p in pts]
    b = lambda el, fp: {"elementId": el, "fixedPoint": fp, "mode": "orbit"} if el else None
    return base(id, "arrow", x, y, max(xs) - min(xs), max(ys) - min(ys), points=pts, startBinding=b(start, [1, 0.5]), endBinding=b(end, [0, 0.5]),
                startArrowhead=None, endArrowhead="arrow", elbowed=False, roundness={"type": 2}, **kw)

def write(lib, name, elements, files=None, bg="#ffffff"):
    os.makedirs(os.path.join(lib, ".quiet"), exist_ok=True)
    open(os.path.join(lib, ".quiet", "format.json"), "w").write('{"format": 4}')
    for i, e in enumerate(elements): e["index"] = f"a{i:06d}"
    doc = {"type": "excalidraw", "version": 2, "source": "board_gen.py", "elements": elements, "appState": {"viewBackgroundColor": bg, "gridSize": 20}, "files": files or {}}
    open(os.path.join(lib, name), "w").write(json.dumps(doc))

def perf(n, lib):
    random.seed(1)
    cols = max(1, int(math.sqrt(n * 1.6)))
    els = []
    for i in range(n):
        x, y = (i % cols) * 260, (i // cols) * 180
        r = base(f"r{i}", "rectangle", x, y, 200, 120, backgroundColor="#e1eaff", strokeColor="#4e83fd", roundness={"type": 3},
                 boundElements=[{"id": f"t{i}", "type": "text"}])
        els.append(r)
        els.append(text(f"t{i}", x + 50, y + 47.5, 100, 25, f"节点 {i}", container=f"r{i}"))
    for i in range(n):
        if (i + 1) % cols and i + 1 < n:
            x, y = (i % cols) * 260 + 205, (i // cols) * 180 + 60
            els.append(arrow(f"a{i}", x, y, [[0, 0], [50, 0]], start=f"r{i}", end=f"r{i + 1}", strokeColor="#bbbfc4"))
            els[2 * i]["boundElements"].append({"id": f"a{i}", "type": "arrow"})
            els[2 * i + 2]["boundElements"].append({"id": f"a{i}", "type": "arrow"})
    for k in range(n // 4):
        cx, cy = random.uniform(0, cols * 260), random.uniform(0, max(1, n // cols) * 180)
        pts = [[j * 3, 20 * math.sin(j / 5 + k)] for j in range(60)]
        ys = [p[1] for p in pts]
        els.append(base(f"f{k}", "freedraw", cx, cy, 177, max(ys) - min(ys), strokeColor="#f14241", points=pts, pressures=[], simulatePressure=True))
    write(lib, f"bench-{n}.excalidraw", els)
    print(n, "shapes,", len(els), "elements")

def png(w, h):
    """An asymmetric test picture: four colored quadrants, a dark corner
    block top-left and a grid, so flips and crops show."""
    rows = []
    for y in range(h):
        row = bytearray([0])
        for x in range(w):
            q = (x >= w // 2) + 2 * (y >= h // 2)
            c = [(240, 80, 70), (70, 160, 240), (90, 200, 110), (250, 200, 60)][q]
            if x < w // 4 and y < h // 4: c = (30, 30, 40)
            if x % 40 == 0 or y % 40 == 0: c = (255, 255, 255)
            row += bytes(c)
        rows.append(bytes(row))
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(b"".join(rows))) + chunk(b"IEND", b"")

def showcase(lib):
    random.seed(2)
    data = png(320, 200)
    url = "data:image/png;base64," + base64.b64encode(data).decode()
    os.makedirs(os.path.join(lib, ".assets"), exist_ok=True)
    open(os.path.join(lib, ".assets", "pic.png"), "wb").write(data)
    files = {"inline": {"mimeType": "image/png", "id": "inline", "dataURL": url, "created": 1},
             "onDisk": {"mimeType": "image/png", "id": "onDisk", "path": ".assets/pic.png", "created": 1}}
    img = lambda id, x, y, w, h, f="inline", **kw: base(id, "image", x, y, w, h, **{"strokeColor": "transparent", "fileId": f, "status": "saved", "scale": [1, 1], "crop": None, **kw})
    els = [
        # row 1: plain, rotated, flipped x, flipped y, cropped
        img("i1", 0, 0, 160, 100), img("i2", 200, 0, 160, 100, "onDisk", angle=0.4), img("i3", 400, 0, 160, 100, scale=[-1, 1]),
        img("i4", 600, 0, 160, 100, scale=[1, -1]),
        img("i5", 800, 0, 160, 100, crop={"x": 0, "y": 0, "width": 160, "height": 100, "naturalWidth": 320, "naturalHeight": 200}),
        img("i6", 1000, 0, 160, 100, roundness={"type": 3}, angle=-0.3, scale=[-1, -1]),
        img("i9", 1250, 180, 160, 100, angle=0.3, crop={"x": 80, "y": 50, "width": 160, "height": 100, "naturalWidth": 320, "naturalHeight": 200}),
        # a rotated frame clipping an image and a rectangle that stick out
        base("fr", "frame", 0, 200, 360, 240, angle=0.25, name="Rotated frame"),
        img("i7", 200, 260, 220, 140, frameId="fr"),
        base("r1", "rectangle", -40, 380, 160, 100, backgroundColor="#ffc9c9", fillStyle="solid", frameId="fr"),
        # an upright frame with a cropped, flipped image
        base("fr2", "frame", 480, 200, 300, 200, name="Group A"),
        img("i8", 500, 240, 320, 140, frameId="fr2", scale=[1, -1], crop={"x": 40, "y": 20, "width": 240, "height": 105, "naturalWidth": 320, "naturalHeight": 200}),
        # arrow with a label: the line must break behind the label
        arrow("ar", 850, 300, [[0, 0], [300, 0]], boundElements=[{"id": "al", "type": "text"}], strokeWidth=2),
        text("al", 960, 287.5, 80, 25, "label", container="ar"),
        arrow("ar2", 850, 380, [[0, 0], [150, 80], [300, 0]], boundElements=[{"id": "al2", "type": "text"}], strokeStyle="dashed", strokeColor="#1971c2"),
        text("al2", 950, 427.5, 100, 25, "curved", container="ar2"),
        # styles
        base("s1", "rectangle", 0, 520, 160, 100, backgroundColor="#a5d8ff", fillStyle="hachure", roundness={"type": 3}),
        base("s2", "ellipse", 200, 520, 160, 100, backgroundColor="#b2f2bb", fillStyle="cross-hatch", strokeStyle="dashed", strokeWidth=4),
        base("s3", "diamond", 400, 520, 160, 100, backgroundColor="#ffec99", fillStyle="solid", strokeStyle="dotted", strokeColor="#e03131"),
        base("s4", "rectangle", 600, 520, 160, 100, backgroundColor="#d0bfff", fillStyle="zigzag", opacity=50),
        text("tx", 800, 540, 200, 50, "Rotated text\n中文 文字", angle=0.35, align="left", valign="top", size=20),
        text("tx2", 1050, 540, 160, 25, "Upright 你好", align="left", valign="top"),
        base("st", "stickynote", 0, 680, 200, 200, backgroundColor="#ffdf6b", roundness={"type": 2}, baseHeight=200),
    ]
    write(lib, "showcase.excalidraw", els, files)
    print("showcase:", len(els), "elements")

if __name__ == "__main__":
    if sys.argv[1] == "perf": perf(int(sys.argv[2]), sys.argv[3])
    elif sys.argv[1] == "showcase": showcase(sys.argv[2])
    else: sys.exit(__doc__)
