# Fei Notes Apple release

The release workflow follows BackupDuck's practice of checking the exact source revision before distribution and identifying each release with an immutable tag. Fei Notes uses its existing Rust and Swift packager on a macOS runner. Xcode uploads both channels and App Store Connect is the source of truth for their processing and distribution state.

| Release step | Trigger | Result |
| --- | --- | --- |
| Public TestFlight | An app change pushed to `product/quiet-sync`, a `public-testflight/vVERSION/build-NUMBER` tag, or `public-testflight` dispatch | Signed beta build with free cloud access, Apple processing, beta review submission, and assignment to the public beta group |
| App Store candidate | `store/v1.0.0/build-5` tag or `store` dispatch with an unused build number | Signed StoreKit build, Apple processing, and attachment to the matching macOS App Store draft with manual release |
| Submit the first subscriptions | App Store Connect after the store job and approval to submit | Add the selected macOS version, the subscription group, and both plans to one review draft and submit it in the web interface |
| Submit after the first subscription approval | `submit` dispatch and confirmation matching `submit:VERSION:BUILD` | Submit the exact selected App Store build and applicable review items |
| Publish an approved release | `publish` dispatch and confirmation matching `publish:1.0.0` | Request publication when Apple reports the version as pending developer release |

Use the actual release version and available build number. Use a new number after Apple rejects an upload or a build changes. A retry resumes an existing processed or processing upload by exact version, platform, build number, and audience. Avoid restarting the upload while Apple reports an incomplete upload; check its receipt first. Do not move an existing tag. A successful upload is followed by Apple processing and, where applicable, review. Internal testing still requires the intended users to be enrolled and a real Mac installation to be checked.

Advance the TestFlight marketing version together on Mac, iPhone, and iPad once the current version has passed external beta review. While that review is pending, a replacement build can use the same marketing version with a new build number.

Set `FEI_TESTFLIGHT_VERSION` and `FEI_TESTFLIGHT_IOS_BUILD` in the repository before dispatching either platform. Select an unused even build number for iPhone and iPad. Mac uses that number plus one. Both workflows check these settings. Check both Apple upload records and keep both workflow dispatches on the same source commit.

When enabled, the Mac branch-push workflow proposes the odd build number `1001 + 2 * github.run_number`. It proceeds only if the adjacent even number matches the configured iOS build. A website-only change does not trigger an Apple build.

Apple requires the first subscription to go through the App Store Connect web interface with the app version. The `submit` command checks for an approved subscription before editing any review draft. For 1.0, use the store job to upload and attach the candidate, then assemble the first review draft in App Store Connect. Submit it after the release owner confirms. [Apple subscription submission API](https://developer.apple.com/documentation/appstoreconnectapi/submitting-subscriptions-and-subscription-groups-for-app-review)

Fei Notes uses an independent product version in the app bundle. Keep Cargo versions aligned with the merged Quiet.md source. Use the `feinotes/v1.0.0` namespace for public source releases and record both the actual upstream base commit and the Fei Notes release commit in the release notes. The Apple build number identifies an individual upload.

Both beta channels use production CloudKit permissions. On macOS, cloud access first verifies the complete installed code signature against Apple's TestFlight Beta Distribution certificate, the Fei Notes bundle and team IDs, and the signed beta entitlement. StoreKit's verified sandbox app transaction remains a fallback. This allows a genuine TestFlight installation to sync when the AppTransaction service is unavailable; plist flags, ad-hoc signatures and developer signatures do not qualify. Verification failures retain their own diagnostic and retry state instead of becoming subscription-required errors. `testflight` exports internal-only; `public-testflight` exports an externally eligible beta and submits it for TestFlight review. It has no StoreKit products or trial controls. The App Store build compiles the beta access branch out, includes the configured monthly and annual products, and uses the normal cloud trial and subscription rules.

## Architecture support

Distribution packages are universal macOS apps with arm64 and x86_64 slices in both the Rust executable and Swift bridge. The packager builds each target, checks the deployment target and architecture, combines the slices before signing, and includes dependency notices for both targets. `FEI_MAC_ARCHS` defaults to `arm64 x86_64`; a single architecture may be selected for development. Intel is compiled and packaged without Intel hardware validation. Runtime tests run on Apple silicon only.

## One-time repository setup

Enable Actions and restrict updates to the product branch and creation of release tags to maintainers. Allow the branch `product/quiet-sync` and the `testflight/**` and `public-testflight/**` release tags in the `feinotes-testflight` environment. Merge the workflow into the default branch when manual dispatch is needed. Protect the submission and publication environments with separate approvals. Pull requests run without release credentials.

- All four environments need `ASC_KEY_ID`, `ASC_ISSUER_ID`, and `ASC_PRIVATE_KEY_B64` secrets for an approved App Store Connect credential. Scope access as tightly as the account permits and use a separate release credential if available.
- The build environments `feinotes-testflight` and `feinotes-store` also need secrets `FEI_APP_CERT_B64`, `FEI_INSTALLER_CERT_B64`, `FEI_STORE_PROFILE_B64`, `FEI_CERT_PASSWORD`, `FEI_APP_CERT_SHA1`, and `FEI_INSTALLER_CERT_SHA1`. The two certificate files are password-protected PKCS#12 files. The job uses an ephemeral signing keychain and removes it when finished.
- Both TestFlight environments use secrets `FEI_PUBLIC_BETA_GROUP_ID` and `FEI_BETA_GROUP_ID` for the external and internal groups. The iOS environment also uses secrets `FEI_IOS_APP_CERT_SHA1` and `FEI_IOS_PROFILE_UUID`. Set `FEI_SUPPORT_URL` to the live support page to include it in the app's About section and the bilingual TestFlight notes. Add testers in App Store Connect.
- Set `FEI_EXPORT_EXEMPT=1` only for a channel whose build and distribution scope are confirmed exempt. Keep the internal beta's decision separate from the public App Store decision.
- The store and submit environments need the `FEI_STOREKIT_MONTHLY_PRODUCT` and `FEI_STOREKIT_ANNUAL_PRODUCT` variables. The submit environment also needs `FEI_SUBSCRIPTION_GROUP_ID`. The store build requires published `FEI_SUPPORT_URL`, `FEI_PRIVACY_URL`, and `FEI_TERMS_URL` values.
- Prepare the matching macOS version draft and its release-specific review materials in App Store Connect. Complete any outstanding export compliance and metadata requirements. The job stops on Apple errors and can resume from a processed build after corrections.

Keep the release credentials out of the repository and all attachments. Production builds use Apple distribution signing. Direct-download Developer ID signing and notarization are separate.

Set and verify the repository's Git author and committer name and GitHub noreply address in a temporary release checkout before creating a commit. Enable `user.useConfigOnly` in that checkout to prevent fallback to the machine's system identity.

## Website deployment

The approved bilingual site is in `website/`, published at `https://feinotes.app/`. The live website is stored in the existing repository's `gh-pages` branch. `.github/workflows/fei-website.yml` deploys that branch's root, and deploys `website/` when website changes are pushed on `product/quiet-sync`. GitHub Pages uses the Actions source, the `github-pages` environment allows both branches, and HTTPS is enforced. Keep `CNAME` set to `feinotes.app`. The first Actions deployment was verified on commit `a1c5f15cbe56bdd9e04f453a9b043448c52a70de`.

Use `https://feinotes.app/support/` for `FEI_SUPPORT_URL` and `https://feinotes.app/privacy/` for `FEI_PRIVACY_URL`. The public support email is `support@feinotes.app`, with Telegram feedback at `https://t.me/+eKIYrdIFufxmYzJl`. These URLs are set on the applicable GitHub release environments. The old Vercel project is paused. Existing TestFlight build 5 embeds the old support URL, so update that link in the next build. [GitHub Pages custom domains](https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site/managing-a-custom-domain-for-your-github-pages-site)

[Apple internal TestFlight](https://developer.apple.com/help/app-store-connect/test-a-beta-version/add-internal-testers/) · [Apple first subscription submission](https://developer.apple.com/help/app-store-connect/manage-submissions-to-app-review/submit-an-in-app-purchase/) · [GitHub protected environments](https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/manage-environments)

## Public beta availability

The public invitation is https://testflight.apple.com/join/NdcXugMy. Apple's external review must approve the build before it can be downloaded. The `public-testflight` channel includes cloud access and omits subscription products while remaining externally eligible for Xcode upload.

The availability workflow runs after release jobs and every 30 minutes. It queries only the designated public group, publishes `website/testflight-status.json` on state changes, and dispatches the website deployment. The bilingual homepage enables its TestFlight button when Apple reports an available external build. Pending review is shown explicitly. A submission failure or rejection requires checking Apple's returned state and metadata before retrying.

When a newer build is in review, keep the latest externally available build downloadable and show the pending version separately. Review, rejection or an internal-only upload must not hide an older valid public build. Disable the download only when no public build is available or the public invitation is disabled. Website availability changes do not require rebuilding the app; publish them without triggering another Apple release.

The public website and app support link use feinotes.app. The canonical repository is https://github.com/qhhonx/feinotes.


## Superseding pending TestFlight builds

Set the repository variable `FEI_EXPIRE_SUPERSEDED_BETAS=1` to enable automatic expiration of obsolete pending builds. The existing availability workflow reconciles the public group after release runs and every 30 minutes; it does not start another app build.

For a replacement in the same platform and marketing-version train, Apple allows only one pending beta review. After the exact new upload is valid and externally eligible, the release job first makes it available to the existing internal group. With expiration enabled, it expires only older pending builds in that same train before submitting the replacement. Approved builds are preserved; processing, failed, expired or internal-only replacements cannot expire a predecessor. Live states are read again before mutation and submission. A later metadata failure can be continued against the existing upload without building or uploading it again.

`python3 -B packaging/macos/apple-release.py supersede-pending` defaults to a read-only plan. Applying requires both `--apply` and the enabled variable. It treats macOS and iOS independently, compares numeric version/build values, and requires a newer valid externally eligible build already in beta review or approved in the same public group. Only older builds still waiting for beta review or in beta review can expire. Already approved, rejected, unsubmitted, internal-only and other-platform builds are preserved. Membership, processing and review states are read again immediately before each change, then Apple's `expired` field is read back. A failed or not-yet-submitted new build cannot retire the current pending build.

Expiration uses Apple's TestFlight `builds.expired` API. It prevents further internal and external installation of that obsolete build; it does not submit or withdraw a formal App Store version. Beta review submission records remain historical evidence. Keep an approved public version downloadable until its replacement is actually approved. Disable the variable to stop automatic expiration.

## Runner recovery

Automatic releases keep using `macos-26` and the triggering commit. For public-TestFlight recovery after a confirmed infrastructure failure, manual dispatch accepts `runner=macos-26-intel` and an optional full 40-character `source` commit. The source must be an ancestor of the workflow commit; both macOS jobs check out and verify that exact source. Runner or source overrides are rejected for Store operations and automatic branch releases.

Before a replacement run, confirm the previous run is terminal and read the exact Apple upload receipt. Keep the authorized version and build only when that number has not been uploaded, or let the existing upload-resume logic handle a verified receipt. Record the workflow commit separately from the application source. Both executable and Swift bridge still require arm64 and x86_64 slices. Intel-hosted compilation does not establish physical Intel application acceptance.


## Fei Notes 1.0.6 source selection

The 1.0.6 TestFlight package selects application source
`431232de1a9e67ae6d22b9da5428a0658e9da95d`, including the 18 upstream commits
through `e9cb5408d0ffcad5bf5b31814e10e1b2f275347f`. That source passed the
ordinary and cloud checks plus the board, owned-image, history and backup
regressions documented in UPSTREAM.md. The release workflow checks that exact
commit again and packages both architectures.

Board scenes retain block-property conflict handling; element-level board
collaboration and real multi-account board acceptance are not established by
these local checks. Keep that boundary in the bilingual beta testing notes.
The release preparation commit uses `[skip ci]` so a single explicit Intel
public-TestFlight dispatch owns this version; do not also dispatch an automatic
branch build. This selection does not submit an App Store version.
