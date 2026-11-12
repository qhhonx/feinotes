#!/bin/bash
# Upload release packages to the download.quiet.md R2 bucket: an immutable copy under v<version>/ and the
# fixed latest/ link the site and README point at. Pass the macOS ZIP, the Windows installer, or both:
#   scripts/publish-downloads.sh dist-release.XXXX/quiet-md-0.1.2-macos-arm64.zip dist/windows/signed/quiet-md-0.1.2-windows-x64-setup.exe
# Each package is signed with minisign (MINISIGN_KEY, default ~/.minisign/quiet-md.key) and checked against
# packaging/update-key.pub; latest/update-<platform>.json, written last, is what the app's updater reads.
# The key's password comes from the login keychain when stored there once, else minisign asks:
#   security add-generic-password -U -s quiet-md-minisign -a "$USER" -w
# DRY_RUN=1 prints the uploads without signing or sending them. WRANGLER overrides the wrangler command.
set -euo pipefail
cd "$(dirname "$0")/.."
[ $# -ge 1 ] || { sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'; exit 1; }
bucket=quiet-md-downloads
host=https://download.quiet.md
wrangler=${WRANGLER:-npx -y wrangler@4}
key=${MINISIGN_KEY:-$HOME/.minisign/quiet-md.key}
pub=packaging/update-key.pub
expected=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT

put() {  # key file content-type cache-control [content-disposition]
  local cmd=($wrangler r2 object put "$bucket/$1" --file "$2" --remote --content-type "$3" --cache-control "$4")
  [ -n "${5:-}" ] && cmd+=(--content-disposition "$5")
  if [ -n "${DRY_RUN:-}" ]; then printf '%q ' "${cmd[@]}"; echo; else "${cmd[@]}" >/dev/null; fi
}

if [ -z "${DRY_RUN:-}" ]; then
  command -v minisign >/dev/null || { echo "minisign is not installed (brew install minisign)" >&2; exit 1; }
  grep -q '^RW' "$pub" || { echo "$pub has no public key yet; see the comment in it" >&2; exit 1; }
  [ -f "$key" ] || { echo "Missing signing key $key (set MINISIGN_KEY)" >&2; exit 1; }
  password=$(security find-generic-password -s quiet-md-minisign -a "$USER" -w 2>/dev/null || true)
fi

for file in "$@"; do
  name=$(basename "$file")
  case "$name" in
    quiet-md-*-macos-arm64.zip) platform=macos-arm64; latest=quiet-md-macos-arm64.zip; type=application/zip ;;
    quiet-md-*-macos-x86_64.zip) platform=macos-x86_64; latest=quiet-md-macos-x86_64.zip; type=application/zip ;;
    quiet-md-*-windows-x64-setup.exe) platform=windows-x64; latest=quiet-md-windows-x64-setup.exe; type=application/vnd.microsoft.portable-executable ;;
    quiet-md-*-x86_64.AppImage) platform=linux-x86_64; latest=quiet-md-x86_64.AppImage; type=application/octet-stream ;;
    *) echo "Unrecognised package name: $name" >&2; exit 1 ;;
  esac
  version=${name#quiet-md-}; version=${version%%-*}
  [ "$version" = "$expected" ] || { echo "$name is $version but Cargo.toml says $expected" >&2; exit 1; }
  [ -f "$file" ] || { echo "Missing $file" >&2; exit 1; }
  # Refuse a package that does not match the checksum written next to it by the build scripts.
  if [ -f "$file.sha256" ]; then
    (cd "$(dirname "$file")" && shasum -a 256 -c "$name.sha256" >/dev/null) || { echo "Checksum mismatch: $file" >&2; exit 1; }
  fi
  # The trusted comment is the file name: the app accepts the signature only for the version the manifest names.
  sig="$work/$name.minisig"
  if [ -z "${DRY_RUN:-}" ]; then
    if [ -n "$password" ]; then printf '%s\n' "$password" | minisign -S -s "$key" -m "$file" -x "$sig" -t "$name" >/dev/null
    else minisign -S -s "$key" -m "$file" -x "$sig" -t "$name"; fi
    minisign -V -q -p "$pub" -m "$file" -x "$sig" || { echo "$name: signature does not match $pub" >&2; exit 1; }
  else
    echo "minisign -S -s $key -m $file -x $sig -t $name"; : > "$sig"
  fi
  put "v$version/$name" "$file" "$type" "public, max-age=31536000, immutable"
  [ -f "$file.sha256" ] && put "v$version/$name.sha256" "$file.sha256" "text/plain; charset=utf-8" "public, max-age=31536000, immutable"
  put "v$version/$name.minisig" "$sig" "text/plain; charset=utf-8" "public, max-age=31536000, immutable"
  # latest/ keeps a stable URL; the download still saves under the versioned file name.
  put "latest/$latest" "$file" "$type" "public, max-age=300" "attachment; filename=\"$name\""
  size=$(stat -f %z "$file")
  manifest="$work/update-$platform.json"
  python3 - "$version" "$host/v$version/$name" "$size" "$sig" > "$manifest" <<'PYTHON'
import json, sys
version, url, size, sig = sys.argv[1:]
print(json.dumps({"version": version, "url": url, "size": int(size), "signature": open(sig).read()}, indent=2))
PYTHON
  [ -n "${DRY_RUN:-}" ] && { put "latest/update-$platform.json" "$manifest" "application/json" "public, max-age=300"; continue; }
  headers=$(curl -sfI "$host/latest/$latest?check=$RANDOM" | tr -d '\r')
  grep -qi "^content-length: $size$" <<<"$headers" && grep -qi "filename=\"$name\"" <<<"$headers" \
    || { echo "latest/$latest did not update as expected" >&2; exit 1; }
  curl -sfI "$host/v$version/$name" | tr -d '\r' | grep -qi "^content-length: $size$" \
    || { echo "v$version/$name is not served" >&2; exit 1; }
  # Last: running copies see the new version only once the package it points at is served.
  put "latest/update-$platform.json" "$manifest" "application/json" "public, max-age=300"
  echo "$host/latest/$latest -> $name ($size bytes); $host/latest/update-$platform.json -> $version"
done
