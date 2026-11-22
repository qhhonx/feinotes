#!/usr/bin/env python3
"""The downloadable fonts' files (packaging/fonts.json, docs/FONTS.md), built
from their upstream releases.

usage: python3 tools/fonts/stage.py [--out DIR] [--cache DIR] [--fill] [--only ID,…] [--official] [--serve PORT]

  --out    where the files go, named as the download site names them, with
           each font's license as <id>-LICENSE.txt (default: /tmp/quiet-md-fonts)
  --cache  where upstream downloads are kept between runs (default: $QUIET_MD_FONT_SOURCES or
           ~/.cache/quiet-md-font-sources)
  --fill   write each file's name, sha256 and size into packaging/fonts.json
           (after adding or updating a font); without it every file must
           match the catalog, or the run fails. Either way a Chinese font
           (group cjk) must cover 通用规范汉字表's 3500 first-level characters
           (checked with fontTools)
  --only   just these fonts; --official: all but the ones marked trial
  --serve  then serve --out on 127.0.0.1:PORT, for QUIET_MD_FONT_URL=http://127.0.0.1:PORT/
           (a preview or a test build fetching from this machine instead of download.quiet.md)

A file's `from` is a URL, a URL#member of a zip archive, or repo:<path> in this
checkout. Upstream archives are fetched with curl; nothing here runs code
from them.
"""
import argparse, hashlib, http.server, json, os, shutil, subprocess, sys, zipfile
from pathlib import Path

repo = Path(__file__).resolve().parents[2]
catalog_path = repo / "packaging/fonts.json"


def fetch(url, cache):
    cache.mkdir(parents=True, exist_ok=True)
    local = cache / (hashlib.sha1(url.encode()).hexdigest()[:10] + "-" + url.rsplit("/", 1)[-1].replace("%20", "_"))
    if not local.exists():
        print(f"fetching {url}", file=sys.stderr)
        part = local.with_suffix(local.suffix + ".part")
        subprocess.run(["curl", "-fsSL", "--retry", "3", "--proto", "=https", "-o", str(part), url], check=True)
        part.rename(local)
    return local


def source(frm, cache):
    """The bytes `from` names."""
    if frm.startswith("repo:"):
        return (repo / frm[5:]).read_bytes()
    url, _, member = frm.partition("#")
    local = fetch(url, cache)
    if not member:
        return local.read_bytes()
    with zipfile.ZipFile(local) as z:
        return z.read(member)


def family_of(data):
    """The family name the font's name table gives (16, else 1), when fontTools is around."""
    try:
        from fontTools.ttLib import TTFont
        import io
    except ImportError:
        return None
    names = TTFont(io.BytesIO(data), lazy=True)["name"]
    for nid in (16, 1):
        n = names.getName(nid, 3, 1, 0x409) or names.getName(nid, 1, 0, 0)
        if n:
            return n.toUnicode()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="/tmp/quiet-md-fonts")
    ap.add_argument("--cache", default=os.environ.get("QUIET_MD_FONT_SOURCES") or os.path.expanduser("~/.cache/quiet-md-font-sources"))
    ap.add_argument("--fill", action="store_true")
    ap.add_argument("--only", default="")
    ap.add_argument("--official", action="store_true", help="leave out the fonts marked trial")
    ap.add_argument("--serve", type=int)
    a = ap.parse_args()
    out, cache = Path(a.out), Path(a.cache)
    out.mkdir(parents=True, exist_ok=True)
    doc = json.loads(catalog_path.read_text())
    only = set(filter(None, a.only.split(",")))
    bad = []
    for e in doc["fonts"]:
        if only and e["id"] not in only or a.official and e.get("trial"):
            continue
        (out / f"{e['id']}-LICENSE.txt").write_bytes(source(e["license_from"], cache))
        for f in e["files"]:
            data = source(f["from"], cache)
            sha = hashlib.sha256(data).hexdigest()
            base = f["from"].split("#")[-1].rsplit("/", 1)[-1].lower()
            ext = "ttf.zlib" if base.endswith(".ttf.zlib") else base.rsplit(".", 1)[1]
            name = f"{f['stem']}-{sha[:12]}.{ext}"
            if e.get("group", "cjk") == "cjk" and (missing := simplified_missing(inflate(data) if f.get("zlib") else data)):
                bad.append(f"{e['id']}: a Chinese font must cover 通用规范汉字表's 3500 first-level characters; {len(missing)} missing, e.g. {''.join(missing[:20])}")
                continue
            if a.fill:
                f.update(name=name, sha256=sha, size=len(data))
                fam = family_of(inflate(data) if f.get("zlib") else data)
                if fam and fam != e["family"]:
                    print(f"{e['id']}: the font calls its family {fam!r}, the catalog {e['family']!r}", file=sys.stderr)
            elif (f["name"], f["sha256"], f["size"]) != (name, sha, len(data)):
                bad.append(f"{f['name']}: built {name} ({len(data)} bytes)")
                continue
            (out / name).write_bytes(data)
            print(f"{name}\t{len(data)}")
    if a.fill:
        catalog_path.write_text(json.dumps(doc, ensure_ascii=False, indent=1) + "\n")
    if bad:
        sys.exit("refused:\n  " + "\n  ".join(bad))
    if a.serve:
        os.chdir(out)
        print(f"serving {out} at http://127.0.0.1:{a.serve}/", file=sys.stderr)
        http.server.ThreadingHTTPServer(("127.0.0.1", a.serve), http.server.SimpleHTTPRequestHandler).serve_forever()


def simplified_missing(data):
    """First-level characters of 通用规范汉字表 the font lacks ([] without fontTools: not checked)."""
    try:
        from fontTools.ttLib import TTFont
        import io
    except ImportError:
        print("fontTools not installed: simplified Chinese coverage not checked", file=sys.stderr)
        return []
    have = set(TTFont(io.BytesIO(data), lazy=True).getBestCmap())
    level1 = [l.strip() for l in (repo / "tools/fonts/data/tongyong-level1.txt").read_text(encoding="utf-8").splitlines() if l.strip() and not l.startswith("#")]
    return [c for c in level1 if ord(c) not in have]


def inflate(data):
    import zlib
    return zlib.decompress(data)


if __name__ == "__main__":
    main()
