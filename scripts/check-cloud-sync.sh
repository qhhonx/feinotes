#!/bin/sh
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Copyright 2026 Fei Notes contributors. See LICENSES.md.
# Local regression for the macOS CloudKit build. No iCloud account is needed.
set -eu
cd "$(dirname "$0")/.."
if [ "$(uname -s)" != Darwin ]; then
    echo "The CloudKit build must be checked on macOS." >&2
    exit 1
fi
cargo test --locked --workspace --features cloudkit-sync,devtools
cargo clippy --locked --workspace --all-targets --features cloudkit-sync,devtools -- -D warnings
