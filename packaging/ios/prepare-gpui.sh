#!/bin/sh
set -eu

ios_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
revision=7548ebc4b24f00494af9976af8852c7163ded9ce
upstream=${1:-https://github.com/longbridge/gpui-mobile.git}
prepared="$ios_dir/.gpui-mobile"
stamp="$prepared/.prepared-sha256"

# Apply in this order. A few patches touch the same source file.
patches='gpui-mobile-ios17-font-traits.patch
gpui-mobile-ios-clipboard.patch
gpui-mobile-ios-url.patch
gpui-mobile-ios-return.patch
gpui-mobile-ios-gallery.patch
gpui-mobile-ios-picker-dismissal.patch
gpui-mobile-ios-font-variants.patch
gpui-mobile-ios-emoji-bounds.patch
gpui-mobile-ios-board-outlines.patch
gpui-mobile-ios-text-context.patch
gpui-mobile-ios-editor-metal-layer.patch'

tree_digest() (
    cd "$1"
    find Cargo.toml README.md LICENSE-* src -type f -exec shasum -a 256 {} + |
        LC_ALL=C sort | shasum -a 256
)

recipe_digest=$(
    cd "$ios_dir"
    printf '%s\n' "$revision"
    shasum -a 256 prepare-gpui.sh
    for patch in $patches; do shasum -a 256 "patches/$patch"; done
)
fingerprint() {
    printf '%s\n' "$recipe_digest"
    tree_digest "$1"
}

if [ "$#" -eq 0 ] && [ -f "$stamp" ] && [ -f "$prepared/Cargo.toml" ] &&
    [ "$(fingerprint "$prepared")" = "$(cat "$stamp")" ]; then
    echo "Pinned GPUI mobile and patch set already verified."
else
    staging=$(mktemp -d "$ios_dir/.gpui-prepare.XXXXXX")
    trap 'rm -rf "$staging"' EXIT
    git init --quiet "$staging/upstream"
    git -C "$staging/upstream" fetch --quiet --depth=1 "$upstream" "$revision"
    git -C "$staging/upstream" archive "$revision" -- Cargo.toml README.md LICENSE-APACHE LICENSE-GPL LICENSE-AGPL src > "$staging/source.tar"
    mkdir "$staging/source"
    tar -xf "$staging/source.tar" -C "$staging/source"
    (
        cd "$staging/source"
        export GIT_CEILING_DIRECTORIES="$staging"
        for patch in $patches; do git apply "$ios_dir/patches/$patch"; done
    )
    fingerprint "$staging/source" > "$staging/source/.prepared-sha256"
    if [ -d "$prepared" ]; then mv "$prepared" "$staging/previous"; fi
    mv "$staging/source" "$prepared"
    echo "Prepared pinned GPUI mobile with the complete patch set."
fi

python3 "$ios_dir/prepare-gpui-core.py"
