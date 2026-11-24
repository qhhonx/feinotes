#!/bin/zsh
# Build and open the design system's 组件库 panel (src/gallery) on its own: no main window, no library.
# DEVTOOLS=1 builds the script driver in (for tools/harness/run.py with QUIET_MD_COMPONENTS=1).
set -e
cd "${0:A:h}/.."
cargo build --release ${DEVTOOLS:+--features devtools}
bin="$PWD/target/release/quiet_md"
pkill -f "^$bin --components" || true
(nohup "$bin" --components >/tmp/quiet-md-components.log 2>&1 &)
echo "组件库面板已打开：$bin --components"
