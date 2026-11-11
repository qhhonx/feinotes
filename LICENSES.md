# Source licensing and attribution

Fei Notes is maintained by the Fei Notes contributors and is based on Quiet.md. The upstream MIT notice in [LICENSE](LICENSE) is retained verbatim.

## License scope

| Files | License |
| --- | --- |
| Upstream Quiet.md code, mixed modifications to upstream files, `crates/core/`, and files outside the explicit Fei Notes list below | MIT, subject to bundled third-party exceptions |
| Independently authored Fei Notes files explicitly listed below | PolyForm Noncommercial License 1.0.0 |
| Vendored dependencies, fonts and third-party icons | Their existing licenses, as recorded in THIRD_PARTY.md and each component's notices |
| Fei Notes name and application icon | Product branding; no trademark rights are granted by the code licenses |

The PolyForm license applies to these Fei Notes files in this release:

- `src/cloud_restore.rs`, `src/cloud_trial.rs`, `src/cloud_subscription.rs`
- All Rust files in `src/cloud_sync/`
- `packaging/macos/CloudRestore.swift`, `CloudSync.swift`, `CloudShare.swift`, `CloudTrial.swift`, `CloudSubscription.swift`
- `packaging/macos/apple-release.py`, `run-apple-release.sh`, `test-apple-release.py`, `build-cloud-app.sh`, `brand-app.sh`
- `scripts/check-cloud-sync.sh`, `scripts/fei-release-licenses.py`
- `.github/workflows/fei-apple-release.yml`, `.github/workflows/fei-website.yml`, `.github/workflows/fei-beta-status.yml`

These files carry an SPDX identifier. New files inherit MIT unless explicitly added to this list with a matching file notice. Upstream-derived and mixed files retain MIT. Existing third-party licenses and previously granted permissions continue to apply.

Copyright 2026 Fei Notes contributors for the independently authored Fei Notes files. See [LICENSE-FEINOTES.md](LICENSE-FEINOTES.md) for the unmodified PolyForm Noncommercial License 1.0.0 text. Commercial licenses for these files are available from [support@feinotes.app](mailto:support@feinotes.app).

The source is public, and personal, noncommercial self-builds are permitted under the applicable licenses. Commercial rights in the designated Fei Notes files are reserved. This does not restrict the rights granted by the upstream MIT license.
