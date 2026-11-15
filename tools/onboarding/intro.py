"""Video -> ordered-dither frames -> one PNG of XOR deltas (what the app embeds).
usage: python3 intro.py <video.mp4> <out.png> [--palette bg.png] [--w 480 --h 270 --fps 24]
1-bit: grayscale PNG rows of 0/255. Palette: indexed PNG, frame N = index XOR previous.
Frames are stacked top to bottom; the app knows W and H, so the count is height / H."""
import sys, subprocess, argparse, json, os
import numpy as np
from PIL import Image

ap = argparse.ArgumentParser()
ap.add_argument("video"); ap.add_argument("out")
ap.add_argument("--palette"); ap.add_argument("--colors", type=int, default=16)
ap.add_argument("--w", type=int, default=480); ap.add_argument("--h", type=int, default=270)
ap.add_argument("--fps", type=float, default=24); ap.add_argument("--start", type=float, default=0)
ap.add_argument("--dur", type=float, default=5)
a = ap.parse_args()
W, H = a.w, a.h
raw = subprocess.run(["ffmpeg", "-v", "error", "-ss", str(a.start), "-t", str(a.dur), "-i", a.video,
    "-vf", f"fps={a.fps},scale={W}:{H}:force_original_aspect_ratio=increase:flags=area,crop={W}:{H}", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
    capture_output=True, check=True).stdout
rgb = np.frombuffer(raw, np.uint8).reshape(-1, H, W, 3).astype(np.float32) / 255
n = len(rgb)

def bayer(k):
    m = np.array([[0]])
    for _ in range(k): m = np.block([[4 * m, 4 * m + 2], [4 * m + 3, 4 * m + 1]])
    return (m + 0.5) / m.size
T = np.tile(bayer(3), (H // 8 + 1, W // 8 + 1))[:H, :W]

if a.palette == "self":
    # The clip's own colours: median cut over a strip of its frames.
    strip = Image.fromarray((np.vstack(rgb[:: max(1, n // 8)]) * 255).astype(np.uint8))
    pal_img = strip.quantize(a.colors, method=Image.Quantize.MEDIANCUT)
if a.palette:
    if a.palette != "self": pal_img = Image.open(a.palette).convert("RGB").quantize(a.colors, method=Image.Quantize.MEDIANCUT)
    pal = np.array(pal_img.getpalette()[:a.colors * 3], np.float32).reshape(-1, 3) / 255
    # Spread: roughly the gap between neighbouring palette colours.
    d = np.linalg.norm(pal[:, None] - pal[None], axis=2); d[d == 0] = 9
    spread = float(np.median(d.min(1)))
    def frame(f):
        g = f + (T[..., None] - 0.5) * spread
        idx = np.argmin(((g[..., None, :] - pal) ** 2).sum(-1), axis=-1)
        return idx.astype(np.uint8)
else:
    pal = np.array([[0, 0, 0], [1, 1, 1]], np.float32)
    def frame(f):
        g = f @ np.array([0.299, 0.587, 0.114], np.float32)
        g = np.clip((g - 0.04) / 0.9, 0, 1) ** 0.9
        return (g > T).astype(np.uint8)

frames = [frame(f) for f in rgb]
delta = [frames[0]] + [frames[i] ^ frames[i - 1] for i in range(1, n)]
stack = np.vstack(delta)
if a.palette:
    # Indices as grey levels i * 17 (a 4-bit greyscale PNG); colours live in the app.
    # Row 0 carries the palette as RGB bytes (16 colours × 3 = 48 pixels).
    assert a.colors <= 16
    head = np.zeros((1, W), np.uint8)
    rgbs = (pal * 255).round().astype(np.uint8).reshape(-1)
    head[0, :len(rgbs)] = rgbs
    Image.fromarray(np.vstack([head, (stack * 17).astype(np.uint8)]), "L").save(a.out, optimize=True)
else:
    Image.fromarray(stack * 255).convert("1").save(a.out, optimize=True)
# A plain preview (not shipped): last-frame accumulate check + a gif for eyes.
prev = os.path.splitext(a.out)[0]
cols = (pal * 255).astype(np.uint8)
Image.fromarray(cols[frames[n // 2]]).resize((W * 2, H * 2), Image.NEAREST).save(prev + "-mid.png")
meta = {"w": W, "h": H, "fps": a.fps, "frames": n, "bytes": os.path.getsize(a.out),
        "palette": [[int(c * 255) for c in p] for p in pal] if a.palette else None,
        "changed_per_frame": int(np.mean([(d != 0).sum() for d in delta[1:]]))}
json.dump(meta, open(prev + ".json", "w"))
print(json.dumps({k: v for k, v in meta.items() if k != "palette"}))
