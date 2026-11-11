# Contributing

Code style and commit conventions are in [AGENTS.md](AGENTS.md) (the same as CLAUDE.md). Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) before you start.

## Building from source

macOS needs the Xcode Command Line Tools; Windows needs the Visual Studio 2022 C++ Build Tools and the Windows SDK. The Rust version is pinned in `rust-toolchain.toml`, and rustup picks it up automatically.

```sh
cargo build --release --locked
./target/release/quiet_md                          # uses ~/Documents/Quiet.md
QUIET_MD_HOME=/tmp/lib ./target/release/quiet_md   # uses another library
```

While developing, point `QUIET_MD_HOME` at a scratch directory; don't test against your real library.

The repository is a Cargo workspace: `crates/core` is the document engine (`fdl_core`), which does not depend on GPUI, and the root crate is the app. GPUI, taffy and xattr are locally patched copies in `vendor/`; the changes are listed in [vendor/PATCHES.md](vendor/PATCHES.md). `target/debug` can grow past 10 GB; if you only build release, `cargo clean --profile dev` clears it.

## Testing

```sh
scripts/check.sh                                        # tests + clippy with zero warnings; run before committing
cargo test --release -p fdl_core                        # the document engine only, without building GPUI
FDL_FUZZ_SEEDS=2000 cargo test --release random_edits   # more random edits, for the editing core
```

- The editing core has a random test: starting from a document with every block type, it makes random selections, typing, structural, style, table and undo edits, checks the model's invariants after each step, and saves and reloads every 25 steps.
- The document format has a regression test: `crates/core/tests/fixtures/all-blocks.xml` must read and write back byte for byte.
- UI screenshots and scripted runs are covered in [tools/harness/README.md](tools/harness/README.md).

To run the checks on every commit: `git config core.hooksPath scripts/hooks`.

## Documentation

| Document | Contents |
|---|---|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Layers, the editing flow, invariants, undo, rendering, the module map |
| [docs/FORMAT.md](docs/FORMAT.md) | Library layout, index recovery, DocxXML read/write rules and local extensions |
| [docs/THEMES.md](docs/THEMES.md) | Writing document themes in CSS: selectors, properties, named colors |
| [AGENTS.md](AGENTS.md) | Code style, pre-commit checks, testing conventions |

Most of these, AGENTS.md included, are written in Chinese for now.

## Building release packages

Before a release, `scripts/check.sh` must pass and CI must be green on that commit. After packaging, verify the signatures, notarization (macOS) and the SHA-256 of the downloads. The Windows CI build does not replace installing, uninstalling and editing on a real machine.

The declared macOS minimum is 13, not yet verified on real hardware. Intel packages can be cross-compiled on Apple Silicon; real Intel hardware validation is still needed.

### macOS

`scripts/release.sh` builds for the local architecture, generates the app icon, signs with a Developer ID Application certificate, submits for Apple notarization, staples the ticket and runs a Gatekeeper check, then writes a ZIP and its SHA-256. It does not upload to GitHub or launch the app.

Install the signing certificate, with its private key, in the login keychain and set up a notarytool keychain profile, then run:

```sh
SIGNING_IDENTITY="Developer ID Application: …" NOTARY_PROFILE="quiet-md" scripts/release.sh
```

The script stops if signing or notarization fails; whatever is left in the failed run's directory must not be shipped.

To build both architectures sequentially on one Apple Silicon Mac, install the Intel Rust target once, then run the same signing and notarization flow for each architecture:

```sh
rustup target add x86_64-apple-darwin
export SIGNING_IDENTITY="Developer ID Application: …" NOTARY_PROFILE="quiet-md"
scripts/release.sh
MACOS_TARGET=x86_64-apple-darwin scripts/release.sh
```

Each run keeps its output in a separate `dist-release.*` directory. Intel ZIPs use `macos-x86_64`; Apple Silicon ZIPs keep `macos-arm64`. CI builds both on its existing ARM runner and verifies the binary architectures and signatures. `SIGNING_IDENTITY=-` skips notarization and creates ad-hoc signed `-unsigned.zip` validation packages, which must not be distributed and are rejected by `publish-downloads.sh`.

### Windows

Uses Inno Setup 7. In a Visual Studio x64 Developer PowerShell:

```powershell
# An unsigned installer for local testing; nothing is uploaded or published
.\scripts\package-windows.ps1 -Iscc 'C:\path\to\ISCC.exe'

# Once the certificate is issued and connected through a hardware token or a cloud signing client,
# pick the current user's certificate by thumbprint
.\scripts\package-windows.ps1 -Iscc 'C:\path\to\ISCC.exe' `
  -CertificateThumbprint '<40-character certificate thumbprint>' `
  -TimestampUrl 'http://time.certum.pl'
```

Signing uses `signtool.exe` from the Windows SDK; `-SignTool` sets its path. The timestamp URL should be the certificate authority's RFC 3161 service (Certum in the example). The script signs the app, the installer and the uninstaller separately, checks the publisher, the trusted signature and the timestamp, and stops on any failure. The final installer and its SHA-256 go to `dist/windows/signed/` or `dist/windows/unsigned/`; the intermediate directories are not for release.

The installer lets you choose the location and an optional desktop shortcut, and by default installs for the current user only. Uninstalling keeps your documents. On Windows, documents live in `Quiet.md` inside the system Documents folder.

On every platform, never commit credentials, private keys or certificate files.

### Uploads and in-app updates

`scripts/publish-downloads.sh <package>…` uploads packages to `v<version>/` and `latest/` on `download.quiet.md`, signs them with minisign, and finally writes `latest/update-<platform>.json` (`macos-arm64`, `macos-x86_64`, `windows-x64`). That manifest is what the app reads. It checks 10 seconds after launch and every 12 hours after that, and again when a window comes to the front more than an hour after the last check (this can be turned off in Settings › General); **Check for Updates…** in the menu checks right away. The download starts only when the user clicks. The app verifies the minisign signature (its trusted comment must be the package's file name); on macOS the new package must also be signed by the same team as the running app. It then quits, swaps the package and reopens. See `src/update.rs`.

Generate the update signing key once (`brew install minisign`), keep the private key outside the repository and commit the public key:

```sh
minisign -G -f -p packaging/update-key.pub -s ~/.minisign/quiet-md.key
```

The script verifies each freshly signed package against `packaging/update-key.pub` and stops if the keys don't match. A lost private key can only be replaced together with the public key, and installed copies of older versions then stop receiving updates, so keep a backup.

To try the update flow locally, point `QUIET_MD_UPDATE_URL=http://127.0.0.1:<port>` at a directory holding a manifest and packages, and build with `QUIET_MD_UPDATE_PUBKEY=<test public key>`; with that variable set, unsigned builds check for updates too.

For a new version, bump the version in both `Cargo.toml` files together, and write the version's highlights in `src/changelog.rs`: after updating, existing users see them once at the bottom of the sidebar.

### Linux (experimental)

Install Rust through rustup, then run `scripts/linux-deps.sh` on Ubuntu/Debian or Arch-based systems. Run `scripts/preview-linux.sh` for an isolated temporary library, or set `QUIET_MD_HOME` to reuse a test library. `scripts/package-linux.sh` produces a tarball and SHA-256 in `dist/linux`; it includes a desktop entry and a 512×512 icon generated with ImageMagick. The binary is dynamically linked; build on the oldest distribution you intend to support and check `runtime-libraries.txt` on the target system.

The Linux build and full checks run on Ubuntu 22.04. `scripts/package-appimage.sh` also produces an x86_64 AppImage in `dist/linux`, using a pinned, SHA-256-verified linuxdeploy. It bundles shared dependencies and checks the payload against a glibc 2.35 baseline; graphics drivers remain system-provided. SHA-256 and dependency reports are generated alongside it. AppImage can be run after `chmod +x`; if FUSE is unavailable, use `--appimage-extract-and-run`. Linux updates are manual. Interactive testing has been done on Omarchy 4.5 with Wayland, floating windows and 250% scaling, including Chinese title input. GNOME Wayland, X11 and systems without Noto fonts still need interactive verification; this is not a claim of support for every desktop distribution. Software Vulkan (`vulkan-swrast` on Arch) is useful in VMs or when no hardware Vulkan driver is available. Global quick-note shortcuts and automatic updates are not supported on Linux yet.

Linux support started in [PR #7](https://github.com/s1dashu/quiet-md/pull/7) by [VJ-2303](https://github.com/VJ-2303); maintainer follow-ups add desktop integration and runtime fixes.

## Contributing to the Fei Notes fork

The Quiet.md instructions above are preserved from upstream. For Fei Notes-specific build configuration, contribution licensing and Apple distribution, see [docs/FEINOTES-CONTRIBUTING.md](docs/FEINOTES-CONTRIBUTING.md) and [docs/FEINOTES-AGENTS.md](docs/FEINOTES-AGENTS.md). The upstream release and download endpoints above belong to Quiet.md. See [docs/UPSTREAM.md](docs/UPSTREAM.md) for the source revision.
