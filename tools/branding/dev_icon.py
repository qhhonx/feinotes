#!/usr/bin/env python3
"""The preview build's icon: the app icon with a blue "dev" ribbon across its
top-right corner, so a hand-test build is never taken for the installed app.

    python3 tools/branding/dev_icon.py   # writes assets/branding/app-icon-dev.png

Needs Pillow. The ribbon is clipped to the icon's own rounded square."""
import math, sys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parents[2]
SRC, OUT = ROOT / "assets/branding/app-icon.png", ROOT / "assets/branding/app-icon-dev.png"
BLUE = (10, 132, 255, 255)   # macOS system blue (#0A84FF), the "dev" ribbon
S = 2                        # drawn at twice the size, then reduced, for smooth edges

def font(size):
    for path, index in (("/System/Library/Fonts/SFNSRounded.ttf", 0), ("/System/Library/Fonts/SFNS.ttf", 0), ("/System/Library/Fonts/HelveticaNeue.ttc", 1)):
        try:
            f = ImageFont.truetype(path, size, index=index)
            try: f.set_variation_by_name("Bold")
            except Exception: pass
            return f
        except OSError: continue
    sys.exit("no system font found")

icon = Image.open(SRC).convert("RGBA")
w, h = icon.size
left, top, right, bottom = icon.getchannel("A").getbbox()
side = right - left
W, H = w * S, h * S
# The band runs at 45° across the top-right corner, between two lines x - y = c.
corner = (right - top) * S
near, far = corner - 0.26 * side * S, corner - 0.60 * side * S
band = Image.new("L", (W, H), 0)
ImageDraw.Draw(band).polygon([(near, 0), (far, 0), (W, W - far), (W, W - near)], fill=255)
mask = Image.composite(band, Image.new("L", (W, H), 0), icon.getchannel("A").resize((W, H), Image.LANCZOS))
# A soft shadow under the ribbon, inside the icon too.
shadow = mask.filter(ImageFilter.GaussianBlur(10 * S)).point(lambda v: v * 0.28)
shadow = Image.composite(shadow, Image.new("L", (W, H), 0), icon.getchannel("A").resize((W, H), Image.LANCZOS))
layer = Image.new("RGBA", (W, H), (0, 0, 0, 0))
layer.paste((0, 0, 0, 255), (0, 6 * S), shadow.crop((0, 0, W, H - 6 * S)))
layer.paste(BLUE, (0, 0), mask)
# "dev" centred on the visible stretch of the band's middle line (from the
# icon's top edge to its right edge), centred by the ink, not the font's box.
mid = (near + far) / 2
f = font(int(0.22 * side * S))
x0, y0, x1, y1 = ImageDraw.Draw(Image.new("L", (1, 1))).textbbox((0, 0), "dev", font=f)
word = Image.new("RGBA", (x1 - x0, y1 - y0), (0, 0, 0, 0))
ImageDraw.Draw(word).text((-x0, -y0), "dev", font=f, fill=(255, 255, 255, 255))
word = word.rotate(-45, resample=Image.BICUBIC, expand=True)
cx = (mid + top * S + right * S) / 2
cy = cx - mid
text = Image.new("RGBA", (W, H), (0, 0, 0, 0))
text.paste(word, (round(cx - word.width / 2), round(cy - word.height / 2)), word)
layer = Image.alpha_composite(layer, text)
out = Image.alpha_composite(icon, layer.resize((w, h), Image.LANCZOS))
out.save(OUT, optimize=True)
print(OUT.relative_to(ROOT))
