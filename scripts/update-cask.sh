#!/bin/bash
# Point the Homebrew cask at release v<version>: read both macOS ZIPs' checksums from the release's
# .sha256 files on GitHub and rewrite version and sha256. Run after the GitHub release is published:
#   scripts/update-cask.sh 0.1.14 [cask.rb ...]
# Default cask: packaging/homebrew/quietmd.rb. Pass the tap's copy too (e.g. ../homebrew-tap/Casks/quietmd.rb).
set -euo pipefail
cd "$(dirname "$0")/.."
[ $# -ge 1 ] || { sed -n '2,5p' "$0" | sed 's/^# \{0,1\}//'; exit 1; }
version=${1#v}; shift
[[ $version =~ ^[0-9]+(\.[0-9]+)+$ ]] || { echo "Not a version: $version" >&2; exit 1; }
casks=("$@"); [ ${#casks[@]} -gt 0 ] || casks=(packaging/homebrew/quietmd.rb)
base=https://github.com/s1dashu/quiet-md/releases/download/v$version

sum() {  # arch → checksum from quiet-md-<version>-macos-<arch>.zip.sha256 ("<hex>  <file>")
  local file=quiet-md-$version-macos-$1.zip s
  s=$(curl -fsSL "$base/$file.sha256") || { echo "No $file.sha256 in release v$version" >&2; exit 1; }
  read -r s name <<<"$s"
  [[ $s =~ ^[0-9a-f]{64}$ && $name == "$file" ]] || { echo "Unexpected $file.sha256: $s $name" >&2; exit 1; }
  echo "$s"
}
arm=$(sum arm64); intel=$(sum x86_64)

for cask in "${casks[@]}"; do
  [ -f "$cask" ] || { echo "Missing $cask" >&2; exit 1; }
  sed -i '' -E \
    -e "s/^(  version \")[^\"]*\"/\1$version\"/" \
    -e "s/^(  sha256 arm: +\")[0-9a-f]{64}\"/\1$arm\"/" \
    -e "s/^( +intel: +\")[0-9a-f]{64}\"/\1$intel\"/" "$cask"
  grep -q "version \"$version\"" "$cask" && grep -q "$arm" "$cask" && grep -q "$intel" "$cask" \
    || { echo "Could not rewrite $cask; check its version/sha256 lines" >&2; exit 1; }
  echo "$cask → $version (arm64 ${arm:0:12}…, x86_64 ${intel:0:12}…)"
done
