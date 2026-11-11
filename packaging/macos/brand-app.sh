#!/bin/sh
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Copyright 2026 Fei Notes contributors. See LICENSES.md.
# Apply the Fei Notes resources to a staged CloudKit app before signing.
set -eu

app=${1:?Expected a staged Fei Notes.app bundle}
rust_notices=${3:-}
test "$(basename "$app")" = 'Fei Notes.app'
plist="$app/Contents/Info.plist"
test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$plist")" = "${2:-app.quietmd.cloudlab}"
source_dir=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$source_dir/../.." && pwd)
resources="$app/Contents/Resources"
mkdir -p "$resources/en.lproj" "$resources/zh-Hans.lproj"

temp=$(mktemp -d)
trap 'rm -rf "$temp"' EXIT
iconset="$temp/FeiNotes.iconset"
mkdir "$iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$source_dir/fei-notes.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" "$source_dir/fei-notes.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$resources/FeiNotes.icns"
old_icon=$(plutil -extract CFBundleIconFile raw "$plist" 2>/dev/null || true)
case "$old_icon" in app-icon|app-icon.icns) rm -f "$resources/app-icon.icns" ;; esac

plutil -replace CFBundleName -string 'Fei Notes' "$plist"
plutil -replace CFBundleDisplayName -string 'Fei Notes' "$plist"
plutil -replace CFBundleDevelopmentRegion -string en "$plist"
plutil -replace CFBundleIconFile -string FeiNotes "$plist"
plutil -replace LSHasLocalizedDisplayName -bool YES "$plist"
if plutil -extract CFBundleIconName raw "$plist" >/dev/null 2>&1; then plutil -remove CFBundleIconName "$plist"; fi
plutil -replace NSDocumentsFolderUsageDescription -string 'Fei Notes opens and imports documents you select.' "$plist"

cat > "$resources/en.lproj/InfoPlist.strings" <<'STRINGS'
"CFBundleName" = "Fei Notes";
"CFBundleDisplayName" = "Fei Notes";
"NSDocumentsFolderUsageDescription" = "Fei Notes opens and imports documents you select.";
STRINGS
cat > "$resources/zh-Hans.lproj/InfoPlist.strings" <<'STRINGS'
"CFBundleName" = "飞记";
"CFBundleDisplayName" = "飞记";
"NSDocumentsFolderUsageDescription" = "飞记会打开和导入你选择的文档。";
STRINGS
cp "$repo/LICENSE" "$resources/Quiet.md-LICENSE"
cp "$repo/THIRD_PARTY.md" "$resources/THIRD_PARTY.md"
cp "$repo/LICENSES.md" "$resources/LICENSES.md"
cp "$repo/LICENSE-FEINOTES.md" "$resources/LICENSE-FEINOTES.md"
licenses="$resources/Licenses"
mkdir -p "$licenses"
cp "$repo/assets/icons/LICENSE-lucide" "$licenses/Lucide-Interface.txt"
cp "$repo/assets/editor-icons/LICENSE-lucide" "$licenses/Lucide-Editor.txt"
cp "$repo/assets/fonts/katex/LICENSE" "$licenses/KaTeX.txt"
{
    printf 'Fei Notes / 飞记\nOpen Source Licenses / 开源许可\n\nQuiet.md — MIT License\n\n'
    cat "$resources/Quiet.md-LICENSE"
    printf '\n\nFei Notes Source License Scope\n\n'
    cat "$resources/LICENSES.md"
    printf '\n\nFei Notes — PolyForm Noncommercial 1.0.0\n\n'
    cat "$resources/LICENSE-FEINOTES.md"
    printf '\n\nRepository Notices / 仓库第三方声明\n\n'
    cat "$resources/THIRD_PARTY.md"
    for license in "$licenses"/*.txt; do
        printf '\n\n%s\n\n' "$(basename "$license")"
        cat "$license"
    done
    if [ -n "$rust_notices" ]; then
        printf '\n\nRust Dependencies in This Build / 本构建的 Rust 依赖\n\n'
        cat "$rust_notices"
    fi
} > "$resources/Open Source Licenses.txt"
plutil -lint "$plist" "$resources/en.lproj/InfoPlist.strings" "$resources/zh-Hans.lproj/InfoPlist.strings"
