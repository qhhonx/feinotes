#!/bin/bash
# Build, sign, notarize and package this checkout as a .dmg (the download) and a .zip (the in-app
# updater's package, latest/update-*.json); never launches the app.
# MACOS_TARGET=x86_64-apple-darwin cross-compiles Intel on Apple Silicon.
# SIGNING_IDENTITY=- creates an ad-hoc signed, unnotarized CI package only.
set -euo pipefail
cd "$(dirname "$0")/.."
: "${SIGNING_IDENTITY:?Set SIGNING_IDENTITY to your Developer ID Application identity}"
if [ "$SIGNING_IDENTITY" != - ]; then
  : "${NOTARY_PROFILE:?Set NOTARY_PROFILE to a notarytool keychain profile}"
fi
export MACOSX_DEPLOYMENT_TARGET=13.0
target=${MACOS_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}
case "$target" in
  aarch64-apple-darwin) arch=arm64 ;;
  x86_64-apple-darwin) arch=x86_64 ;;
  *) echo "Unsupported macOS target: $target" >&2; exit 1 ;;
esac
# The dist profile (Cargo.toml): LTO, no symbols, line tables in a dSYM.
binary="${CARGO_TARGET_DIR:-target}/dist/quiet_md"
if [ -n "${MACOS_TARGET:-}" ]; then
  cargo build --profile dist --locked --target "$target"
  binary="${CARGO_TARGET_DIR:-target}/$target/dist/quiet_md"
else
  cargo build --profile dist --locked
fi
# The `quiet` command line tool (crates/cli), shipped inside the app.
cli="$(dirname "$binary")/quiet"
lipo "$binary" -verify_arch "$arch"
lipo "$cli" -verify_arch "$arch"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
out=$(mktemp -d "$PWD/dist-release.XXXXXX")
app="$out/quiet.md.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$cli" "$app/Contents/MacOS/"
# Kept beside the package, not in it: resolves the addresses a panic logs
# (`atos -o <dSYM> -l <base> <addresses>`) and crash reports, matched by UUID.
ditto -c -k --norsrc --noextattr --noqtn --keepParent "$binary.dSYM/" "$out/dSYM-quiet-md-$version-macos-$arch.zip"
cp LICENSE THIRD_PARTY.md "$app/Contents/Resources/"
icons="$out/AppIcon.iconset"
mkdir -p "$icons"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" assets/branding/app-icon.png --out "$icons/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" assets/branding/app-icon.png --out "$icons/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$icons" -o "$app/Contents/Resources/AppIcon.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Quiet.md</string>
<key>CFBundleDisplayName</key><string>Quiet.md</string>
<key>CFBundleIdentifier</key><string>com.s1dashu.quietmd</string>
<key>CFBundleExecutable</key><string>quiet_md</string>
<key>CFBundleIconFile</key><string>AppIcon</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleVersion</key><string>$version</string>
<key>LSMinimumSystemVersion</key><string>13.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSDocumentsFolderUsageDescription</key><string>Quiet.md 将笔记保存在文稿中的 Quiet.md 文件夹。</string>
  <key>CFBundleDocumentTypes</key><array><dict>
    <key>CFBundleTypeName</key><string>Markdown</string>
    <key>CFBundleTypeRole</key><string>Editor</string>
    <key>LSHandlerRank</key><string>Alternate</string>
    <key>LSItemContentTypes</key><array><string>net.daringfireball.markdown</string></array>
    <key>CFBundleTypeExtensions</key><array><string>md</string><string>markdown</string><string>mdown</string></array>
  </dict><dict>
    <key>CFBundleTypeName</key><string>Folder</string>
    <key>CFBundleTypeRole</key><string>Editor</string>
    <key>LSHandlerRank</key><string>Alternate</string>
    <key>LSItemContentTypes</key><array><string>public.folder</string></array>
  </dict></array>
</dict></plist>
PLIST
plutil -lint "$app/Contents/Info.plist"
# The nested tool first: notarization wants every executable signed.
if [ "$SIGNING_IDENTITY" = - ]; then
  codesign --force --sign - "$app/Contents/MacOS/quiet"
  codesign --force --sign - "$app"
else
  codesign --force --options runtime --timestamp --sign "$SIGNING_IDENTITY" "$app/Contents/MacOS/quiet"
  codesign --force --options runtime --timestamp --sign "$SIGNING_IDENTITY" "$app"
fi
codesign --verify --deep --strict --verbose=2 "$app"
suffix=
if [ "$SIGNING_IDENTITY" = - ]; then
  suffix=-unsigned
  echo "CI package only: ad-hoc signed, without notarization; do not publish."
else
  ditto -c -k --norsrc --noextattr --noqtn --keepParent "$app" "$out/notarization.zip"
  xcrun notarytool submit "$out/notarization.zip" --keychain-profile "$NOTARY_PROFILE" --wait --output-format json > "$out/notarization.json"
  python3 - "$out/notarization.json" <<'PYTHON'
import json, sys
result = json.load(open(sys.argv[1]))
if result.get("status") != "Accepted":
    raise SystemExit("Notarization was not accepted; see " + sys.argv[1])
print("Notarization accepted:", result["id"])
PYTHON
  xcrun stapler staple "$app"
  xcrun stapler validate "$app"
  spctl --assess --type execute --verbose=2 "$app"
fi
archive="$out/quiet-md-$version-macos-$arch$suffix.zip"
# No extended attributes: ditto stores them as `._*` entries, which unzip and
# other tools unpack as real files inside the bundle, breaking its signature
# ("damaged and can't be opened", #407).
ditto -c -k --norsrc --noextattr --noqtn --keepParent "$app" "$archive"
if unzip -l "$archive" | grep '/\._' >/dev/null; then echo "AppleDouble entries in $archive" >&2; exit 1; fi
(cd "$out" && shasum -a 256 "$(basename "$archive")" > "$(basename "$archive").sha256")
# The download people install from: the same stapled .app beside an /Applications link to drag it onto,
# LZMA-compressed (ULMO, macOS 10.15+), about a fifth smaller than the zip.
# The zip above stays the in-app updater's package (latest/update-*.json); both hold the same bundle.
dmg="$out/quiet-md-$version-macos-$arch$suffix.dmg"
stage="$out/dmg-root"
mkdir "$stage"
ditto --norsrc --noextattr --noqtn "$app" "$stage/quiet.md.app"
ln -s /Applications "$stage/Applications"
# hdiutil now and then fails with "Resource busy" on CI runners; a retry gets through.
for try in 1 2 3; do
  hdiutil create -quiet -volname "Quiet.md" -srcfolder "$stage" -fs HFS+ -format ULMO "$dmg" && break
  [ $try = 3 ] && exit 1; rm -f "$dmg"; sleep 5
done
rm -rf "$stage"
if [ "$SIGNING_IDENTITY" = - ]; then
  codesign --force --sign - "$dmg"
else
  codesign --force --timestamp --sign "$SIGNING_IDENTITY" "$dmg"
  xcrun notarytool submit "$dmg" --keychain-profile "$NOTARY_PROFILE" --wait --output-format json > "$out/notarization-dmg.json"
  python3 - "$out/notarization-dmg.json" <<'PYTHON'
import json, sys
result = json.load(open(sys.argv[1]))
if result.get("status") != "Accepted":
    raise SystemExit("DMG notarization was not accepted; see " + sys.argv[1])
print("DMG notarization accepted:", result["id"])
PYTHON
  xcrun stapler staple "$dmg"
  xcrun stapler validate "$dmg"
  spctl --assess --type open --context context:primary-signature --verbose=2 "$dmg"
fi
codesign --verify --strict --verbose=2 "$dmg"
hdiutil verify -quiet "$dmg"
# Mounted read-only, the copy inside must still pass the strict signature check.
mount=$(mktemp -d)
hdiutil attach -quiet -readonly -nobrowse -noautoopen -mountpoint "$mount" "$dmg"
check=0
codesign --verify --deep --strict "$mount/quiet.md.app" && [ -L "$mount/Applications" ] || check=1
hdiutil detach -quiet "$mount"; rmdir "$mount"
[ $check = 0 ] || { echo "$dmg: the app inside does not verify" >&2; exit 1; }
(cd "$out" && shasum -a 256 "$(basename "$dmg")" > "$(basename "$dmg").sha256")
printf 'Release artifacts: %s (download), %s (in-app update)\nDebug symbols (keep, do not publish): %s\n' "$dmg" "$archive" "$out/dSYM-quiet-md-$version-macos-$arch.zip"
