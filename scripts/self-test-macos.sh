#!/bin/bash
# Release self-test of a macOS package: unpacks the zip scripts/release.sh
# made into a fresh folder (spaces and CJK in its path, as users' have) and
# runs the app's built-in checks (src/selftest.rs) on a temporary library.
# Never touches ~/Documents/Quiet.md. Usage:
#   scripts/self-test-macos.sh <quiet-md-…-macos-<arch>[-unsigned].zip> [output dir]
# The output dir (default: a new temporary one) gets report.json, logs and,
# for a failed scenario, its screenshot. Exit code 0: every scenario passed.
set -euo pipefail
zip=${1:?usage: self-test-macos.sh <package.zip> [output dir]}
out=${2:-$(mktemp -d "${TMPDIR:-/tmp}/quiet-md-self-test.XXXXXX")}
mkdir -p "$out"
out=$(cd "$out" && pwd)
place=$(mktemp -d "${TMPDIR:-/tmp}/quiet-md 自检 安装.XXXXXX")
trap 'rm -rf "$place"' EXIT
ditto -x -k "$zip" "$place"
app="$place/quiet.md.app"
codesign --verify --deep --strict "$app"
# A notarized package also passes Gatekeeper as downloaded.
if codesign -dv "$app" 2>&1 | grep -q '^TeamIdentifier=[A-Z0-9]'; then
  spctl --assess --type execute "$app"
fi
status=0
"$app/Contents/MacOS/quiet_md" --self-test "$out/run" "$out/report.json" || status=$?
echo "self-test output: $out"
exit "$status"
