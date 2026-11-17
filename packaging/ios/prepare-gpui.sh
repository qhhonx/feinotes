#!/bin/sh
set -eu

ios_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
revision=7548ebc4b24f00494af9976af8852c7163ded9ce
upstream=${1:-https://github.com/longbridge/gpui-mobile.git}
patch_file="$ios_dir/patches/gpui-mobile-ios17-font-traits.patch"
staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT

git init --quiet "$staging/upstream"
git -C "$staging/upstream" fetch --quiet --depth=1 "$upstream" "$revision"
git -C "$staging/upstream" archive "$revision" -- Cargo.toml README.md LICENSE-APACHE LICENSE-GPL LICENSE-AGPL src > "$staging/source.tar"
mkdir "$staging/source"
tar -xf "$staging/source.tar" -C "$staging/source"
cd "$staging/source"
git apply "$patch_file"
git apply "$ios_dir/patches/gpui-mobile-ios-clipboard.patch"
git apply "$ios_dir/patches/gpui-mobile-ios-url.patch"
git apply "$ios_dir/patches/gpui-mobile-ios-return.patch"
git apply "$ios_dir/patches/gpui-mobile-ios-gallery.patch"
git apply "$ios_dir/patches/gpui-mobile-ios-picker-dismissal.patch"
git apply "$ios_dir/patches/gpui-mobile-ios-font-variants.patch"
git apply "$ios_dir/patches/gpui-mobile-ios-board-outlines.patch"
mkdir -p "$ios_dir/.gpui-mobile"
cp -R . "$ios_dir/.gpui-mobile/"

python3 "$ios_dir/prepare-gpui-core.py"
