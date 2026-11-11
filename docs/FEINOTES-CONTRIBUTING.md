# Contributing to Fei Notes

Read [upstream conventions](../AGENTS.md) and [Fei Notes additions](FEINOTES-AGENTS.md) and the [architecture reference](ARCHITECTURE.md) before making changes. New developer-facing documentation, comments, issue reports and pull requests should use English. Preserve the supported Chinese and English UI strings.

The root CONTRIBUTING.md preserves the Quiet.md instructions. This supplement describes the Fei Notes product; its Apple distribution instructions apply instead of the upstream Quiet.md release instructions.

## Build and test

Install the Rust toolchain pinned by `rust-toolchain.toml` and Xcode on macOS.

```sh
cargo build --release --locked
scripts/check.sh
scripts/check-cloud-sync.sh
```

The plain build is the upstream-compatible local editor. The branded macOS app uses `cloudkit-sync` and the packager described in [APPLE-RELEASE.md](APPLE-RELEASE.md). Cloud signing and live-container access need the contributor's own approved Apple configuration.

Use synthetic libraries in a fresh temporary directory. `QUIET_MD_HOME=/private/tmp/fei-local-test` selects the local editor's test library. The cloud build has its own isolated QA root rules. Preserve every pending edit when testing retries and account changes.

For structural editor changes, extend the existing random-edit coverage and run `FDL_FUZZ_SEEDS=2000 cargo test --release random_edits`. Keep `crates/core/tests/fixtures/all-blocks.xml` round-tripping byte for byte. For UI refactors, use the existing [screenshot harness](../tools/harness/README.md).

## Pull requests

Keep each change focused and explain the user-visible behavior, validation and known limits. Follow the compact style around the edited code. Avoid repository-wide formatting. Use existing theme tokens and controls for UI changes.

Changes to the database or sync protocol must document migration behavior for existing libraries, partial failures and older devices. Never include real notes, private keys, signing archives, provisioning profiles or personal diagnostic logs in a pull request.

By submitting a contribution, you agree to license it under the applicable file license documented in [LICENSES.md](../LICENSES.md). Upstream-derived files retain MIT. The selected independently authored Fei Notes files use PolyForm Noncommercial 1.0.0. Contact support before contributing code with incompatible licensing or uncertain provenance.

## Quiet.md upstream

Generic fixes that work independently of Fei Notes should be proposed against [Quiet.md](https://github.com/s1dashu/quiet-md). Product sync, platform integration and distribution are maintained here. Record accepted upstream changes and reconcile duplicates on the next integration. See [MAINTENANCE.md](MAINTENANCE.md).

For private security reports, use the process in [SECURITY.md](../SECURITY.md).
