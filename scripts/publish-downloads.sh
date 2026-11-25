#!/bin/bash
# Upload release packages to the download.quiet.md R2 bucket: an immutable copy under v<version>/ and the
# fixed latest/ link the site and README point at. Pass any of the packages, e.g. a Mac's DMG and ZIP and the installer:
#   scripts/publish-downloads.sh dist-release.XXXX/quiet-md-0.1.2-macos-arm64.{dmg,zip} dist/windows/signed/quiet-md-0.1.2-windows-x64-setup.exe
# On macOS the DMG is the download (latest/quiet-md-macos-<arch>.dmg) and the ZIP the updater's package: only the
# ZIP writes latest/update-macos-*.json, so publish both (the ZIP's latest/ link stays for older links).
# Each package is signed with minisign (MINISIGN_KEY, default ~/.minisign/quiet-md.key) and checked against
# packaging/update-key.pub; latest/update-<platform>.json, written last, is what the app's updater reads.
# The key's password comes from the login keychain when stored there once, else minisign asks:
#   security add-generic-password -U -s quiet-md-minisign -a "$USER" -w
# DRY_RUN=1 prints the uploads without signing or sending them. WRANGLER overrides the wrangler command.
# --policy instead of packages signs and uploads latest/policy.json, the forced update (docs/UPDATES.md):
#   MIN_VERSION=0.1.15 MESSAGE_ZH='…' MESSAGE_EN='…' scripts/publish-downloads.sh --policy
#   PLATFORM_MIN='windows-x64=0.1.16' (per platform, in place of MIN_VERSION); neither set clears the policy.
# --fonts uploads the downloadable fonts (packaging/fonts.json: the board's Xiaolai, 设置's document fonts) as
# fonts/<name>-<hash>.<ext>, built by tools/fonts/stage.py (skipped when already served). Copies fetch them
# when first needed, so they must be there before a release: publishing packages does this step first.
set -euo pipefail
export PYTHONIOENCODING=utf-8
cd "$(dirname "$0")/.."
[ $# -ge 1 ] || { sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'; exit 1; }
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

sign() {  # file trusted-comment signature
  if [ -n "$password" ]; then printf '%s\n' "$password" | minisign -S -s "$key" -m "$1" -x "$3" -t "$2" >/dev/null
  else minisign -S -s "$key" -m "$1" -x "$3" -t "$2"; fi
  minisign -V -q -p "$pub" -m "$1" -x "$3" || { echo "$(basename "$1"): signature does not match $pub" >&2; exit 1; }
}

# Downloadable fonts (packaging/fonts.json, docs/FONTS.md): tools/fonts/stage.py builds each file from its
# upstream release and checks it against the catalog, whose SHA-256 the app has compiled in; a file's name
# carries its hash, so a file is never replaced (a changed font is a new name). Already served: skipped.
# Each font's license goes along as fonts/<id>-LICENSE.txt. The fonts marked trial (listed only by preview
# builds) are left out unless named: FONTS='id …' publishes just those.
publish_fonts() {
  local staged="$work/fonts" list name size
  python3 tools/fonts/stage.py --out "$staged" $([ -n "${FONTS:-}" ] && echo "--only ${FONTS// /,}" || echo --official) >"$work/fonts.txt" || { echo "tools/fonts/stage.py failed" >&2; exit 1; }
  list=$(cut -f1 "$work/fonts.txt"; (cd "$staged" && ls ./*-LICENSE.txt | sed 's|^\./||'))
  for name in $list; do
    size=$(stat -f %z "$staged/$name")
    if curl -sfI "$host/fonts/$name" | tr -d '\r' | grep -qi "^content-length: $size$"; then echo "$host/fonts/$name is already served"; continue; fi
    case "$name" in *.txt) type="text/plain; charset=utf-8" ;; *.otf) type=font/otf ;; *.ttf) type=font/ttf ;; *) type=application/octet-stream ;; esac
    put "fonts/$name" "$staged/$name" "$type" "public, max-age=31536000, immutable"
    [ -n "${DRY_RUN:-}" ] && continue
    curl -sfI "$host/fonts/$name" | tr -d '\r' | grep -qi "^content-length: $size$" || { echo "fonts/$name is not served" >&2; exit 1; }
    echo "$host/fonts/$name ($size bytes)"
  done
}
if [ "$1" = --fonts ]; then
  [ $# -eq 1 ] || { echo "--fonts takes no packages" >&2; exit 1; }
  publish_fonts; exit 0
fi

# The policy text is signed as is; its trusted comment names it by SHA-256, so the signature fits no other
# text and is never mistaken for a package's. Refused when a platform's newest package is older than required.
if [ "$1" = --policy ]; then
  [ $# -eq 1 ] || { echo "--policy takes no packages" >&2; exit 1; }
  body="$work/policy.txt"
  python3 - "${MIN_VERSION:-}" "${PLATFORM_MIN:-}" "${MESSAGE_ZH:-}" "${MESSAGE_EN:-}" > "$body" <<'PYTHON'
import json, re, sys, datetime
min_version, platform_min, zh, en = sys.argv[1:]
version = re.compile(r"^\d+(\.\d+)*$")
platforms = dict(p.split("=", 1) for p in platform_min.split())
for v in [min_version, *platforms.values()]:
    if v and not version.match(v): sys.exit(f"Not a version: {v}")
for p in platforms:
    if p not in ("macos-arm64", "macos-x86_64", "windows-x64", "linux-x86_64"): sys.exit(f"Unknown platform: {p}")
policy = {"min_version": min_version, "platforms": platforms, "message_zh": zh, "message_en": en,
          "issued": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")}
sys.stdout.write(json.dumps(policy, ensure_ascii=False, separators=(",", ":"), sort_keys=True))
PYTHON
  echo "Policy: $(cat "$body")"
  [ -z "${MIN_VERSION:-}${PLATFORM_MIN:-}" ] && echo "(no version required: this clears the forced update)"
  newer() { python3 -c 'import sys; p=lambda v:[int(x) for x in v.split(".")]; a,b=p(sys.argv[1]),p(sys.argv[2]); n=max(len(a),len(b)); sys.exit(0 if a+[0]*(n-len(a))>b+[0]*(n-len(b)) else 1)' "$1" "$2"; }
  for platform in macos-arm64 macos-x86_64 windows-x64 linux-x86_64; do
    need=$(sed -n "s/.*$platform=\([0-9.]*\).*/\1/p" <<<"${PLATFORM_MIN:-}"); need=${need:-${MIN_VERSION:-}}
    [ -n "$need" ] || continue
    have=$(curl -sf "$host/latest/update-$platform.json?check=$RANDOM" | python3 -c 'import json,sys; print(json.load(sys.stdin)["version"])' || true)
    [ -n "$have" ] || { echo "Could not read latest/update-$platform.json" >&2; [ -n "${DRY_RUN:-}" ] && continue; exit 1; }
    if newer "$need" "$have"; then echo "$platform: latest is $have, older than the required $need; publish the packages first" >&2; exit 1; fi
    echo "$platform: copies before $need update to $have"
  done
  comment="quiet-md-policy sha256:$(shasum -a 256 "$body" | cut -d' ' -f1)"
  if [ -z "${DRY_RUN:-}" ]; then sign "$body" "$comment" "$body.minisig"
  else echo "minisign -S -s $key -m $body -x $body.minisig -t '$comment'"; : > "$body.minisig"; fi
  python3 - "$body" "$body.minisig" > "$work/policy.json" <<'PYTHON'
import json, sys
print(json.dumps({"policy": open(sys.argv[1], encoding="utf-8").read(), "signature": open(sys.argv[2]).read()}, ensure_ascii=False, indent=2))
PYTHON
  put "latest/policy.json" "$work/policy.json" "application/json" "public, max-age=300"
  [ -n "${DRY_RUN:-}" ] || echo "$host/latest/policy.json updated"
  exit 0
fi

# Before any package: a copy that cannot fetch a font falls back (boards: the system's CJK font).
publish_fonts
for file in "$@"; do
  name=$(basename "$file")
  case "$name" in
    quiet-md-*-macos-arm64.dmg) platform=; latest=quiet-md-macos-arm64.dmg; type=application/x-apple-diskimage ;;
    quiet-md-*-macos-x86_64.dmg) platform=; latest=quiet-md-macos-x86_64.dmg; type=application/x-apple-diskimage ;;
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
    sign "$file" "$name" "$sig"
  else
    echo "minisign -S -s $key -m $file -x $sig -t $name"; : > "$sig"
  fi
  put "v$version/$name" "$file" "$type" "public, max-age=31536000, immutable"
  [ -f "$file.sha256" ] && put "v$version/$name.sha256" "$file.sha256" "text/plain; charset=utf-8" "public, max-age=31536000, immutable"
  put "v$version/$name.minisig" "$sig" "text/plain; charset=utf-8" "public, max-age=31536000, immutable"
  # latest/ keeps a stable URL; the download still saves under the versioned file name.
  put "latest/$latest" "$file" "$type" "public, max-age=300" "attachment; filename=\"$name\""
  size=$(stat -f %z "$file")
  if [ -z "$platform" ]; then  # A download only: the updater never reads it.
    [ -n "${DRY_RUN:-}" ] && continue
    curl -sfI "$host/latest/$latest?check=$RANDOM" | tr -d '\r' | grep -qi "^content-length: $size$" \
      && curl -sfI "$host/v$version/$name" | tr -d '\r' | grep -qi "^content-length: $size$" \
      || { echo "$name is not served as expected" >&2; exit 1; }
    echo "$host/latest/$latest -> $name ($size bytes)"; continue
  fi
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
