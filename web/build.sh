#!/bin/zsh
# Build the web demo (wasm32) into an output folder: quiet_md.js, the optimized
# wasm, fonts/ and index.html. Libraries go into <out>/libraries/ separately
# (web/bundle.py).
# usage: [FAST=1] web/build.sh <out dir>
# Needs: rustup target wasm32-unknown-unknown, wasm-bindgen-cli matching
# Cargo.lock's wasm-bindgen (WASM_BINDGEN=<path> to override), wasm-opt (binaryen).
set -e
here=${0:A:h}; repo=${here:h}
out=${1:?usage: web/build.sh <out dir>}
mkdir -p "$out"; out=${out:A}
ver=$(grep -A1 '^name = "wasm-bindgen"$' "$repo/Cargo.lock" | sed -n 's/version = "\(.*\)"/\1/p')
wb=${WASM_BINDGEN:-$HOME/.local/wasm-bindgen-$ver/bin/wasm-bindgen}
[ -x "$wb" ] || { echo "need wasm-bindgen $ver: cargo install wasm-bindgen-cli --version $ver --root ~/.local/wasm-bindgen-$ver" >&2; exit 1; }
cd "$repo"
# FAST=1: the everyday release profile and no wasm-opt, for quick rebuilds.
profile=${FAST:+release}; profile=${profile:-web}
cargo build --profile $profile --target wasm32-unknown-unknown --bin quiet_md
"$wb" --target web --no-typescript --out-dir "$out" target/wasm32-unknown-unknown/$profile/quiet_md.wasm
if [ -z "$FAST" ] && command -v wasm-opt > /dev/null; then
  wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals --enable-reference-types \
    "$out/quiet_md_bg.wasm" -o "$out/quiet_md_bg.wasm.opt" && mv "$out/quiet_md_bg.wasm.opt" "$out/quiet_md_bg.wasm"
fi
mkdir -p "$out/fonts" "$out/libraries"
cp "$here"/fonts/* "$out/fonts/"
cp "$here/index.html" "$out/"
ls -la "$out/quiet_md_bg.wasm"
