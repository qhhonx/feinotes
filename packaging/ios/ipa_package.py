#!/usr/bin/env python3
"""Package signed iOS apps without macOS metadata; verify the actual IPA payload."""
import argparse
import hashlib
import json
import os
import plistlib
import re
import shutil
import stat
import subprocess
import tempfile
import zipfile
from pathlib import Path, PurePosixPath


def verify_ipa(path):
    with zipfile.ZipFile(path) as archive, tempfile.TemporaryDirectory(prefix="fei-ipa-check-") as temporary:
        entries = archive.infolist()
        for entry in entries:
            parts = PurePosixPath(entry.filename).parts
            if entry.filename.startswith("/") or ".." in parts or not parts or parts[0] != "Payload":
                raise ValueError("IPA must contain only safe Payload paths")
            if any(part.startswith("._") or part == "__MACOSX" for part in parts):
                raise ValueError("AppleDouble metadata inside an IPA can invalidate its signature: " + entry.filename)
        names = [entry.filename for entry in entries if re.fullmatch(r"Payload/[^/]+\.app/Info\.plist", entry.filename)]
        if len(names) != 1:
            raise ValueError("Expected exactly one iOS app")
        info = plistlib.loads(archive.read(names[0]))
        root = Path(temporary).resolve()
        for entry in entries:
            target = root / entry.filename
            target.parent.mkdir(parents=True, exist_ok=True)
            mode = entry.external_attr >> 16
            if entry.is_dir():
                target.mkdir(exist_ok=True)
            elif stat.S_ISLNK(mode):
                destination = archive.read(entry).decode()
                if not (target.parent / destination).resolve().is_relative_to(root):
                    raise ValueError("Bundle symlink escapes Payload")
                target.symlink_to(destination)
            else:
                if not target.resolve().is_relative_to(root):
                    raise ValueError("Bundle path escapes Payload")
                with archive.open(entry) as source, target.open("wb") as output:
                    shutil.copyfileobj(source, output)
                target.chmod(stat.S_IMODE(mode) or 0o644)
        app = root / names[0].rsplit("/", 1)[0]
        subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True, capture_output=True)
        return info


def package_app(app, output):
    app = app.resolve(strict=True)
    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True, capture_output=True)
    with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED) as archive:
        for path in [app, *sorted(app.rglob("*"))]:
            name = "Payload/" + str(path.relative_to(app.parent))
            if path.is_symlink():
                entry = zipfile.ZipInfo(name)
                entry.create_system = 3
                entry.external_attr = (stat.S_IFLNK | 0o777) << 16
                archive.writestr(entry, os.readlink(path))
            else:
                archive.write(path, name)
    return verify_ipa(output)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("path", type=Path)
    parser.add_argument("--output", type=Path, help="Package a signed .app, then verify the resulting IPA")
    args = parser.parse_args()
    info = package_app(args.path, args.output) if args.output else verify_ipa(args.path)
    path = args.output or args.path
    print(json.dumps({"bundle": info["CFBundleIdentifier"], "version": info["CFBundleShortVersionString"],
                      "build": info["CFBundleVersion"], "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}))
