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

Branch pushes that change app sources, assets, packaging, or the Apple workflow trigger public TestFlight. The version comes from the repository variable `FEI_TESTFLIGHT_VERSION` and defaults to `1.0.0`. Automatic builds use `1000 + github.run_number`. Use build numbers below 1000 for manually chosen releases while this workflow owns the higher range. A website-only change does not trigger an Apple build.

Apple requires the first subscription to go through the App Store Connect web interface with the app version. The `submit` command checks for an approved subscription before editing any review draft. For 1.0, use the store job to upload and attach the candidate, then assemble the first review draft in App Store Connect. Submit it after the release owner confirms. [Apple subscription submission API](https://developer.apple.com/documentation/appstoreconnectapi/submitting-subscriptions-and-subscription-groups-for-app-review)

Fei Notes uses an independent product version in the app bundle. Keep Cargo versions aligned with the merged Quiet.md source. Use the `feinotes/v1.0.0` namespace for public source releases and record both the actual upstream base commit and the Fei Notes release commit in the release notes. The Apple build number identifies an individual upload.

Both beta channels use production CloudKit permissions and a verified Apple sandbox app transaction for cloud access. `testflight` exports internal-only; `public-testflight` exports an externally eligible beta and submits it for TestFlight review. It has no StoreKit products or trial controls. The App Store build compiles the beta access branch out, includes the configured monthly and annual products, and uses the normal cloud trial and subscription rules.

## Architecture support

Distribution packages are universal macOS apps with arm64 and x86_64 slices in both the Rust executable and Swift bridge. The packager builds each target, checks the deployment target and architecture, combines the slices before signing, and includes dependency notices for both targets. `FEI_MAC_ARCHS` defaults to `arm64 x86_64`; a single architecture may be selected for development. Intel is compiled and packaged without Intel hardware validation. Runtime tests run on Apple silicon only.

## One-time repository setup

Enable Actions and restrict updates to the product branch and creation of release tags to maintainers. Allow the branch `product/quiet-sync` and the `testflight/**` and `public-testflight/**` release tags in the `feinotes-testflight` environment. Merge the workflow into the default branch when manual dispatch is needed. Protect the submission and publication environments with separate approvals. Pull requests run without release credentials.

- All four environments need `ASC_KEY_ID`, `ASC_ISSUER_ID`, and `ASC_PRIVATE_KEY_B64` secrets for an approved App Store Connect credential. Scope access as tightly as the account permits and use a separate release credential if available.
- The build environments `feinotes-testflight` and `feinotes-store` also need secrets `FEI_APP_CERT_B64`, `FEI_INSTALLER_CERT_B64`, `FEI_STORE_PROFILE_B64`, and `FEI_CERT_PASSWORD`. The two certificate files are password-protected PKCS#12 files. Add public SHA-1 signing fingerprints as the `FEI_APP_CERT_SHA1` and `FEI_INSTALLER_CERT_SHA1` variables. The job uses an ephemeral signing keychain and removes it when finished.
- The test environment needs `FEI_PUBLIC_BETA_GROUP_ID` for the external group and `FEI_BETA_GROUP_ID` for the existing internal group. Set `FEI_SUPPORT_URL` to the live support page to include it in the app's About section and the bilingual TestFlight notes. Add testers in App Store Connect.
- Set `FEI_EXPORT_EXEMPT=1` only for a channel whose build and distribution scope are confirmed exempt. Keep the internal beta's decision separate from the public App Store decision.
- The store and submit environments need the `FEI_STOREKIT_MONTHLY_PRODUCT` and `FEI_STOREKIT_ANNUAL_PRODUCT` variables. The submit environment also needs `FEI_SUBSCRIPTION_GROUP_ID`. The store build requires published `FEI_SUPPORT_URL`, `FEI_PRIVACY_URL`, and `FEI_TERMS_URL` values.
- Prepare the matching macOS version draft and its release-specific review materials in App Store Connect. Complete any outstanding export compliance and metadata requirements. The job stops on Apple errors and can resume from a processed build after corrections.

Keep the release credentials out of the repository and all attachments. Production builds use Apple distribution signing. Direct-download Developer ID signing and notarization are separate.

## Website deployment

The approved bilingual site is in `website/`, published at `https://feinotes.app/`. The live website is stored in the existing repository's `gh-pages` branch. `.github/workflows/fei-website.yml` deploys that branch's root, and deploys `website/` when website changes are pushed on `product/quiet-sync`. GitHub Pages uses the Actions source, the `github-pages` environment allows both branches, and HTTPS is enforced. Keep `CNAME` set to `feinotes.app`. The first Actions deployment was verified on commit `a1c5f15cbe56bdd9e04f453a9b043448c52a70de`.

Use `https://feinotes.app/support/` for `FEI_SUPPORT_URL` and `https://feinotes.app/privacy/` for `FEI_PRIVACY_URL`. The public support email is `support@feinotes.app`, with Telegram feedback at `https://t.me/+eKIYrdIFufxmYzJl`. These URLs are set on the applicable GitHub release environments. The old Vercel project is paused. Existing TestFlight build 5 embeds the old support URL, so update that link in the next build. [GitHub Pages custom domains](https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site/managing-a-custom-domain-for-your-github-pages-site)

[Apple internal TestFlight](https://developer.apple.com/help/app-store-connect/test-a-beta-version/add-internal-testers/) · [Apple first subscription submission](https://developer.apple.com/help/app-store-connect/manage-submissions-to-app-review/submit-an-in-app-purchase/) · [GitHub protected environments](https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/manage-environments)

## Public beta availability

The public invitation is https://testflight.apple.com/join/NdcXugMy. Apple's external review must approve the build before it can be downloaded. The `public-testflight` channel includes cloud access and omits subscription products while remaining externally eligible for Xcode upload.

The availability workflow runs after release jobs and every 30 minutes. It queries only the designated public group, publishes `website/testflight-status.json` on state changes, and dispatches the website deployment. The bilingual homepage enables its TestFlight button when Apple reports an available external build. Pending review is shown explicitly. A submission failure or rejection requires checking Apple's returned state and metadata before retrying.

The public website and app support link use feinotes.app. The canonical repository is https://github.com/qhhonx/feinotes.
