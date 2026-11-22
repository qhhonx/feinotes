#!/usr/bin/env python3
"""How much simplified Chinese a font covers: its cmap against the 3500
first-level characters of 通用规范汉字表 (data/tongyong-level1.txt), the 6763
of GB 2312 (from Python's gb2312 codec) and common Chinese punctuation.

usage: python3 tools/fonts/coverage.py <font>… (TrueType / OpenType; .zlib inflated)
Needs fontTools. tools/fonts/stage.py refuses a Chinese font (group cjk) in
packaging/fonts.json whose first-level coverage is under 100%.
"""
import io, sys, zlib
from pathlib import Path
from fontTools.ttLib import TTFont

here = Path(__file__).resolve().parent
LEVEL1 = [l.strip() for l in (here / "data/tongyong-level1.txt").read_text(encoding="utf-8").splitlines() if l.strip() and not l.startswith("#")]
GB2312 = [bytes([a, b]).decode("gb2312") for a in range(0xB0, 0xF8) for b in range(0xA1, 0xFF) if not (a == 0xD7 and b > 0xF9)]
PUNCT = list("，。、；：？！“”‘’（）《》〈〉【】「」『』—…·～")
assert len(LEVEL1) == 3500 and len(GB2312) == 6763


def cmap(path):
    data = Path(path).read_bytes()
    if path.endswith(".zlib"):
        data = zlib.decompress(data)
    return set(TTFont(io.BytesIO(data), lazy=True).getBestCmap())


def report(path):
    have = cmap(path)
    out = {}
    for name, chars in (("level1", LEVEL1), ("gb2312", GB2312), ("punct", PUNCT)):
        missing = [c for c in chars if ord(c) not in have]
        out[name] = (100 * (len(chars) - len(missing)) / len(chars), missing)
    return out


if __name__ == "__main__":
    for p in sys.argv[1:]:
        r = report(p)
        print(f"{Path(p).name}: 一级字 {r['level1'][0]:.2f}% (缺 {len(r['level1'][1])})  GB2312 {r['gb2312'][0]:.2f}% (缺 {len(r['gb2312'][1])})  标点 {r['punct'][0]:.0f}% (缺 {''.join(r['punct'][1]) or '无'})")
        if r["level1"][1]:
            print("  一级字缺:", "".join(r["level1"][1][:50]))
