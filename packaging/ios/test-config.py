# SPDX-License-Identifier: MIT
# Copyright 2026 Fei Notes contributors. See LICENSES.md.
"""Exercise the Rust prebuild's production/QA boundaries without compiling."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--ios-dir', type=Path, default=Path(__file__).resolve().parent)
args = parser.parse_args()
with tempfile.TemporaryDirectory() as directory:
    cargo = Path(directory) / 'cargo'
    cargo.write_text('#!/bin/sh\nprintf "beta=%s cloud=%s preview=%s\\n" "$FEI_TESTFLIGHT_BETA" "$FEI_IOS_PRODUCT_CLOUD" "$FEI_IOS_UPSTREAM_PREVIEW"\nprintf "%s\\n" "$@"\n')
    cargo.chmod(0o700)
    def run(config, profile='debug', qa='0'):
        env = dict(os.environ, CONFIGURATION=config, RUST_PROFILE=profile, FEI_MOBILE_QA=qa,
                   CARGO=str(cargo), RUST_TARGET='aarch64-apple-ios', SRCROOT=str(args.ios_dir))
        return subprocess.run(['sh', str(args.ios_dir / 'build-rust.sh')], env=env, capture_output=True, text=True)
    beta = run('TestFlight', 'release')
    assert beta.returncode == 0 and 'beta=1 cloud=1' in beta.stdout
    assert '--release' in beta.stdout and 'mobile-qa' not in beta.stdout
    local = run('Debug', qa='1')
    assert local.returncode == 0 and 'beta=0 cloud=0' in local.stdout and 'cloudkit-sync,mobile-qa' in local.stdout
    product = run('Product')
    assert product.returncode == 0 and 'beta=0 cloud=1' in product.stdout
    preview = run('UpstreamPreview')
    assert preview.returncode == 0 and 'beta=0 cloud=0 preview=1' in preview.stdout
    for config, profile, qa in [('TestFlight','debug','0'), ('TestFlight','release','1'), ('Product','debug','1')]:
        assert run(config, profile, qa).returncode != 0, (config, profile, qa)
print('7 Rust prebuild namespace, isolated preview, optimized beta and QA exclusion scenarios passed.')
