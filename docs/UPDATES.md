# Updates and forced updates

`src/update.rs` keeps an installed copy current. Everything it trusts is signed with the release minisign key (`packaging/update-key.pub`; the secret key stays with the maintainer).

## Ordinary updates

`scripts/publish-downloads.sh <packages…>` uploads each package to `v<version>/`, signs it (trusted comment: the package's file name) and writes `latest/update-<platform>.json` last. The app reads that manifest 10 seconds after launch and every 12 hours while Settings › About › Automatic updates is on. It downloads only when the user asks. The manifest is unsigned, so it can name only a package on download.quiet.md over HTTPS whose signature carries that package's own name, version included. Only newer versions are offered.

## Forced updates

If a released version has a serious problem, publish a fixed version as usual, then publish a policy that makes older copies update.

```sh
DRY_RUN=1 MIN_VERSION=0.1.15 MESSAGE_ZH='0.1.14 可能丢失刚输入的内容，请立即更新。' MESSAGE_EN='0.1.14 can lose text you just typed. Please update now.' scripts/publish-downloads.sh --policy
MIN_VERSION=0.1.15 MESSAGE_ZH='…' MESSAGE_EN='…' scripts/publish-downloads.sh --policy
```

- `MIN_VERSION`: copies older than this must update. `PLATFORM_MIN='windows-x64=0.1.16 macos-arm64=0.1.15'` sets the version for individual platforms (`macos-arm64`, `macos-x86_64`, `windows-x64`, `linux-x86_64`), overriding `MIN_VERSION` for those platforms. The messages are optional. Chinese interfaces (simplified and traditional) show `MESSAGE_ZH`, the others `MESSAGE_EN`, and each falls back to the other.
- The script refuses a policy that asks for more than a platform's `latest/update-<platform>.json` offers. Publish the packages first.
- Running it with neither variable set publishes an empty policy, which clears the forced update. Copies that already downloaded a package keep it as an ordinary update.
- Only copies that include this feature (the release after 0.1.13 and later) read the policy. Older copies never fetch the file.

### Format

`latest/policy.json`:

```json
{
  "policy": "{\"issued\":\"…\",\"message_en\":\"…\",\"message_zh\":\"…\",\"min_version\":\"0.1.15\",\"platforms\":{}}",
  "signature": "untrusted comment: …\n…\ntrusted comment: quiet-md-policy sha256:<hex SHA-256 of the policy string>\n…\n"
}
```

`policy` is the exact UTF-8 text that was signed. The trusted comment names that text by hash, and minisign's global signature covers the comment. So a policy signature fits no other text, a policy cannot pass as a package (whose trusted comment must be the package name), and a package signature cannot pass as a policy. The client ignores unknown fields in the policy, so later versions can add fields.

### What the app does

- It reads the policy 3 seconds after launch and then hourly, whether or not automatic updates are on. Copies that cannot update themselves (development builds, Linux distribution packages, a Windows copy without its installer) skip the policy entirely.
- The policy applies only when it verifies against the built-in key and requires a version newer than the running one. A policy that fails verification, is malformed, or asks for a version the manifest does not offer yet forces nothing. The last case is logged (`update policy: requires … but the newest package is …`).
- When it applies, the newest package downloads at once with the usual checks: size, minisign signature under its own file name, and on macOS the code signature team. Once it is verified, every window shows a dialog that cannot be dismissed, with the policy's message and one button: Restart and Update. Keys do not reach the window behind it (Return presses the button). The restart saves open documents through the usual quit path first.
- Until the package is there (offline, a failed download), a banner at the foot of the window says an update is required and the documents stay editable. Retry tries again now, and the app retries by itself every 10 minutes. The last verified policy is cached next to the download folder (`policy.json` in the update cache), so a copy launched offline still shows the banner.
- If the restart fails (no permission to replace the app, for example), the dialog gives way to the banner with the reason. Retry tries the restart again.
- Updates only move forward: the required version must be newer than the running one, and the package must be the manifest's newest version. A library opened by a newer version may not open in an older one, so a policy never asks for a downgrade.

### Testing

The client accepts only the key it was built with, so build a copy with a throwaway key:

```sh
minisign -G -W -p /tmp/t/test.pub -s /tmp/t/test.key
QUIET_MD_UPDATE_PUBKEY="$(tail -1 /tmp/t/test.pub)" cargo build --release --features devtools
```

Serve a folder on `127.0.0.1` containing `latest/update-<platform>.json`, `latest/policy.json` and the package it names, each signed with the test key. On macOS the package is a zip of an ad-hoc signed `quiet.md.app`. Run the app with `QUIET_MD_UPDATE_URL=http://127.0.0.1:<port>/latest`, a temporary `QUIET_MD_HOME` and a temporary `HOME`, so downloads and the cached policy stay out of your own cache folder. To produce `policy.json`, run a copy of `publish-downloads.sh` with `host`, `pub`, `MINISIGN_KEY` and `WRANGLER` pointed at the test server, the test key and a stand-in that copies files instead of uploading them. Rebuild without `QUIET_MD_UPDATE_PUBKEY` afterwards. The unit tests in `update.rs` cover verification, version comparison and the misconfiguration case (`cargo test --release --bin quiet_md update::tests`).
