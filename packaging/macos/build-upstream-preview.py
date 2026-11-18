#!/usr/bin/env python3
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Copyright 2026 Fei Notes contributors. See LICENSES.md.
"""Build a local, ad-hoc upstream candidate without TestFlight identity or data."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shlex
import shutil
import subprocess
import tempfile

REPO = Path(__file__).resolve().parents[2]
BUNDLE = "local.feinotes.upstream-preview"
CONTAINER = "iCloud.local.feinotes.upstream-preview"
NAME = "Fei Notes Upstream Preview"


def run(*args, **kwargs):
    return subprocess.check_output(args, cwd=REPO, text=True, **kwargs).strip()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify(app):
    info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
    assert info["CFBundleIdentifier"] == BUNDLE
    assert info["CFBundleName"] == info["CFBundleDisplayName"] == NAME
    assert "CFBundleURLTypes" not in info and "CFBundleDocumentTypes" not in info
    assert not list(app.rglob("*provision*")), "A preview cannot contain a provisioning profile"
    assert not any(path.is_symlink() for path in app.rglob("*")), "Ship real files, not cache symlinks"
    run("codesign", "--verify", "--deep", "--strict", str(app))
    signed = subprocess.run(["codesign", "-d", "--verbose=4", str(app)], capture_output=True, text=True, check=True).stderr
    assert f"Identifier={BUNDLE}\n" in signed and "Signature=adhoc" in signed
    result = subprocess.run(["codesign", "-d", "--entitlements", ":-", str(app)], capture_output=True, check=True)
    entitlements = plistlib.loads(result.stdout) if result.stdout.strip() else {}
    assert not entitlements, "The local preview must have no cloud, group or keychain entitlements"
    metadata = json.loads((app / "Contents/Resources/upstream-preview.json").read_text())
    assert metadata["bundle_id"] == BUNDLE and metadata["cloud_namespace"] == f"Development:{CONTAINER}"
    assert metadata["testflight"] is False and re.fullmatch(r"[a-f0-9]{40}", metadata["source_revision"])
    root = Path(metadata["library_parent"])
    assert root.parent == Path("/private/tmp") and root.name.startswith("quiet-sync-qa-upstream-preview-")
    for relative, expected in metadata["sha256"].items():
        assert digest(app / relative) == expected, f"Changed preview artifact: {relative}"
    print(json.dumps({"app": str(app), "bundle_id": BUNDLE, "source_revision": metadata["source_revision"],
                      "library": str(root / "local-only"), "signature": "ad-hoc", "entitlements": entitlements}, indent=2))


def smoke(app):
    verify(app)
    metadata = json.loads((app / "Contents/Resources/upstream-preview.json").read_text())
    root = Path(metadata["library_parent"])
    with (root / "preview-smoke.log").open("w") as log:
        subprocess.run([str(app / "Contents/MacOS/quiet_md"), "--preview-smoke"],
                       stdout=log, stderr=subprocess.STDOUT, timeout=45, check=True)
    log = (root / "preview-smoke.log").read_text()
    assert "PANIC" not in log and "ERROR" not in log
    import sqlite3
    with sqlite3.connect(f"file:{root}/local-only/library.sqlite3?mode=ro", uri=True) as db:
        assert db.execute("SELECT 1 FROM library_documents WHERE title = 'Upstream preview smoke test'").fetchone()
    print(f"PREVIEW_SMOKE_OK launcher, isolated SQLite, create/save: {root}")


def build(output):
    assert not run("git", "status", "--porcelain"), "Commit the candidate before building its preview"
    revision = run("git", "rev-parse", "HEAD")
    branch = run("git", "branch", "--show-current")
    assert branch.startswith("integrate/"), "Build from an independent integration branch"
    output = (output or REPO / "dist-preview" / f"upstream-{revision[:12]}").absolute()
    assert not output.exists(), "Use a new output directory; never replace an installed or existing app"
    output.mkdir(parents=True)
    env = {key: value for key, value in os.environ.items() if not key.startswith(("FEI_", "QUIET_", "FDL_", "DYLD_"))}
    env.update(FEI_BUNDLE_ID=BUNDLE, FEI_CLOUD_CONTAINER=CONTAINER, FEI_CLOUD_ENVIRONMENT="Development")
    process = subprocess.Popen(["cargo", "build", "--locked", "--features", "cloudkit-sync,devtools",
                                "--message-format=json-render-diagnostics"], cwd=REPO, env=env, stdout=subprocess.PIPE, text=True)
    binary = bridge = configuration = None
    with (output / "cargo-build.jsonl").open("w") as log:
        for line in process.stdout:
            log.write(line)
            item = json.loads(line)
            if item.get("reason") == "compiler-artifact" and item.get("executable") and item["target"]["name"] == "quiet_md":
                binary = Path(item["executable"])
            if item.get("reason") == "build-script-executed" and item["package_id"].split("#")[-1].startswith("quiet_md@"):
                bridge = Path(item["out_dir"]) / "libQuietCloudRestore.dylib"
                configuration = dict(item["env"])
    assert process.wait() == 0, "Candidate build failed; see cargo-build.jsonl"
    assert binary and bridge and bridge.is_file() and configuration
    assert configuration["FEI_BUNDLE_ID"] == BUNDLE and configuration["FEI_TESTFLIGHT_BETA"] == "0"
    assert configuration["FEI_CLOUD_NAMESPACE"] == f"Development:{CONTAINER}"
    assert configuration["FEI_SOURCE_REVISION"] == revision[:12]
    target = run("rustc", "-vV", env=env).split("host: ", 1)[1].splitlines()[0]
    notices = output / "notices"
    run("python3", "scripts/fei-release-licenses.py", target, str(notices), env=env)
    app = output / "Fei Notes.app"
    macos, frameworks, resources = (app / "Contents" / part for part in ("MacOS", "Frameworks", "Resources"))
    for directory in (macos, frameworks, resources):
        directory.mkdir(parents=True)
    shutil.copy2(binary, macos / "quiet_md.bin")
    shutil.copy2(bridge, frameworks / bridge.name)
    info = {"CFBundleIdentifier": BUNDLE, "CFBundleExecutable": "quiet_md", "CFBundleName": NAME,
            "CFBundleDisplayName": NAME, "CFBundlePackageType": "APPL",
            "CFBundleShortVersionString": re.search(r'^version = "([^"]+)"', (REPO / "Cargo.toml").read_text(), re.M)[1],
            "CFBundleVersion": "1", "LSMinimumSystemVersion": "14.0", "NSHighResolutionCapable": True,
            "NSDocumentsFolderUsageDescription": "This isolated preview only uses synthetic test notes.",
            "FeiNotesSourceRevision": revision}
    plist = app / "Contents/Info.plist"
    plist.write_bytes(plistlib.dumps(info))
    notice_file = notices / "Open Source Notices.txt"
    assert notice_file.is_file(), "Missing generated dependency notices"
    run("sh", "packaging/macos/brand-app.sh", str(app), BUNDLE, str(notice_file), env=env)
    info = plistlib.loads(plist.read_bytes())
    info.update(CFBundleName=NAME, CFBundleDisplayName=NAME)
    plist.write_bytes(plistlib.dumps(info))
    for language in ("en", "zh-Hans"):
        (resources / f"{language}.lproj/InfoPlist.strings").write_text(
            f'"CFBundleName" = "{NAME}";\n"CFBundleDisplayName" = "{NAME}";\n')
    app = app.rename(output / f"{NAME}.app")
    root = Path(tempfile.mkdtemp(prefix=f"quiet-sync-qa-upstream-preview-{revision[:12]}-", dir="/private/tmp"))
    # Finder launches cannot inherit a product library, QA channel or release environment.
    launcher = app / "Contents/MacOS/quiet_md"
    launcher.write_text("""#!/bin/sh
set -eu
umask 077
library=ROOT
if [ ! -e "$library" ]; then mkdir -m 700 "$library"; fi
[ -d "$library" ] && [ ! -L "$library" ]
[ "$(/usr/bin/stat -f %u "$library")" = "$(/usr/bin/id -u)" ]
[ -z "$(/usr/bin/find "$library" -type l -print -quit)" ]
: > "$library/use-local-library"
macos=$(CDPATH= cd -P "$(dirname "$0")" && pwd)
[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$macos/../Info.plist")" = 'local.feinotes.upstream-preview' ]
cd "$library"
if [ "${1:-}" = '--preview-smoke' ]; then
  exec /usr/bin/env -i HOME="$HOME" PATH=/usr/bin:/bin:/usr/sbin:/sbin \\
    QUIET_SYNC_QA_ROOT="$library" QUIET_SYNC_QA_LOCAL_FIRST=1 QUIET_MD_LOG_DIR="$library/logs" \\
    FDL_SCRIPT="$macos/../Resources/preview-smoke.txt" FDL_SHOTS="$library" FDL_NOBLINK=1 "$macos/quiet_md.bin"
fi
exec /usr/bin/env -i HOME="$HOME" PATH=/usr/bin:/bin:/usr/sbin:/sbin \\
  QUIET_SYNC_QA_ROOT="$library" QUIET_SYNC_QA_LOCAL_FIRST=1 \\
  QUIET_MD_LOG_DIR="$library/logs" "$macos/quiet_md.bin"
""".replace("ROOT", shlex.quote(str(root)), 1))
    launcher.chmod(0o755)
    (app / "Contents/Resources/preview-smoke.txt").write_text(
        "resize 1200 850\nwait 1500\nkey cmd-n\nwait 400\ntype Upstream preview smoke test\n"
        "key enter\ntype Synthetic local notes only\nwait 1000\nkey cmd-s\nwait 500\nshot preview-smoke\nquit\n")
    metadata = {"source_revision": revision, "source_branch": branch, "bundle_id": BUNDLE,
                "cloud_namespace": f"Development:{CONTAINER}", "testflight": False, "library_parent": str(root),
                "architecture": target, "features": ["cloudkit-sync", "devtools"],
                "scope": "Local synthetic SQLite/editor preview; no live CloudKit or TestFlight acceptance"}
    for relative in ("Contents/MacOS/quiet_md.bin", "Contents/Frameworks/libQuietCloudRestore.dylib"):
        run("codesign", "--force", "--sign", "-", "--timestamp=none", str(app / relative))
    metadata["sha256"] = {relative: digest(app / relative) for relative in (
        "Contents/MacOS/quiet_md", "Contents/MacOS/quiet_md.bin", "Contents/Frameworks/libQuietCloudRestore.dylib",
        "Contents/Resources/preview-smoke.txt")}
    (app / "Contents/Resources/upstream-preview.json").write_text(json.dumps(metadata, indent=2) + "\n")
    run("codesign", "--force", "--sign", "-", "--identifier", BUNDLE, "--timestamp=none", str(app))
    assert run("git", "rev-parse", "HEAD") == revision and not run("git", "status", "--porcelain"), "Candidate changed during build"
    verify(app)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--verify", type=Path, metavar="APP")
    parser.add_argument("--smoke", type=Path, metavar="APP", help="Verify and run the bundled offscreen SQLite smoke test")
    args = parser.parse_args()
    if args.smoke: smoke(args.smoke.absolute())
    elif args.verify: verify(args.verify.absolute())
    else: build(args.output)
