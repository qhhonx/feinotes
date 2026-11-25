#!/usr/bin/env python3
"""Shared macOS, Windows and Linux checks; run inside the platform build environment."""
import os
from pathlib import Path
import subprocess
import sys

os.chdir(Path(__file__).resolve().parent.parent)
# A failing step doesn't stop the rest: every step runs, then the failed ones are listed.
failed = []
def cargo(*args, env=None):
    if subprocess.run(["cargo", *args], env=env).returncode != 0:
        failed.append(" ".join(["cargo", *args]))

cargo("test", "--release", "--workspace", "--locked", "--no-fail-fast")
# tools/vendor-sync is its own workspace (the patch series tool, vendor/PATCHES.md); a cold build takes seconds.
cargo("test", "--locked", "--manifest-path", "tools/vendor-sync/Cargo.toml", "--target-dir", "target/tools", "--no-fail-fast")
for library in ["shared-mock/library", "shared-mock/library-zh"]:
    cargo("test", "--release", "--locked", "--no-fail-fast", "-p", "fdl_core", "library_round_trips", "--", "--ignored",
          env={**os.environ, "FDL_ROUNDTRIP_DIR": str(Path(library).resolve())})
source = Path("shared-mock/guides/assets")
bundled = Path("shared-mock/library/.assets/guides")
for image in source.rglob("*"):
    if image.is_file():
        target = bundled / image.relative_to(source)
        if not target.is_file() or target.read_bytes() != image.read_bytes():
            failed.append(f"Bundled guide asset is stale: {target}")
cargo("clippy", "--release", "--workspace", "--locked", "--all-targets", "--", "-D", "warnings")
# The harness build (script driver compiled in) must stay clean too.
cargo("clippy", "--release", "--workspace", "--locked", "--all-targets", "--features", "devtools", "--", "-D", "warnings")
# 数据表 is off by default (Cargo feature `datatable`); the build with it on must stay clean too.
cargo("clippy", "--release", "--workspace", "--locked", "--all-targets", "--features", "devtools,datatable", "--", "-D", "warnings")

if failed:
    print("\nFailed:\n" + "\n".join(f"  {f}" for f in failed), file=sys.stderr)
    sys.exit(1)
print("All checks passed")
