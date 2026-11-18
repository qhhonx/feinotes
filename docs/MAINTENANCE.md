# Maintaining Fei Notes

## Branches

- `product/quiet-sync` is the product and default branch. It contains the macOS application, documentation, release automation and `website/`.
- `main` remains the upstream mirror. The `upstream` remote points at `s1dashu/quiet-md`.
- `gh-pages` retains the website publication history. GitHub Pages also deploys `website/` from the product branch.
- Use short-lived feature, contribution and integration branches. Keep public source release tags immutable.

## Documentation boundaries

Preserve the upstream conventions in root AGENTS.md, CLAUDE.md and CONTRIBUTING.md. Put product additions in FEINOTES-AGENTS.md and FEINOTES-CONTRIBUTING.md. Root theme documentation retains upstream text; product theme additions live in FEINOTES-THEMES.md. See UPSTREAM.md for the baseline and immutable source links.

## Integrating Quiet.md

Prepare upstream integrations only in an independent dated candidate branch and worktree. Never stage unrelated product changes. The product worktree may continue its own work: use its committed branch snapshot, record dirty files or active Git operations, and leave that worktree untouched. Reuse an existing pending candidate when possible; distinguish upstream commits missing from the product from commits already integrated into the candidate.

```sh
git fetch upstream main
git worktree add -b integrate/quiet-main-YYYYMMDD ../feinotes-upstream-YYYYMMDD product/quiet-sync
cd ../feinotes-upstream-YYYYMMDD
git merge --no-ff upstream/main
scripts/check.sh
scripts/check-cloud-sync.sh
```

Review semantic interactions even when Git reports no conflict. Pay particular attention to document creation, editor selections and IME state, table identities, SQLite transactions, file import and shared-note permissions. Use targeted regression checks for paths changed by the integration.

Keep the file and SQLite backends, cloud functionality and local version history in backup/restore intact. Run 2000 random-edit seeds when editor structure changes. If the product branch advances, merge its committed changes into the candidate, resolve conflicts there, and repeat the affected checks. A fast-forward product update avoids a merge conflict at that step; it does not eliminate conflicts during candidate integration.

After the checks, commit the candidate and build a separate local experience app:

```sh
python3 packaging/macos/build-upstream-preview.py
```

The builder requires a clean candidate checkout and records its exact source revision. Without cloud arguments it builds the local-only **Fei Notes Upstream Preview**, using `local.feinotes.upstream-preview`, ad-hoc signing and temporary synthetic SQLite data. Preserve this earlier app and its data when adding the cloud preview. Neither variant may use `app.feinotes.mac` or another TestFlight/Store identity.

For the three-device cloud preview, run the same builder with `--cloud-profile /private/path/mac-cloudlab.provisionprofile --identity APPLE_DEVELOPMENT_SHA1`. It builds **Fei Notes Upstream Cloud Preview** using `app.quietmd.cloudlab`, a Mac device-limited Apple Development profile and only Development CloudKit permissions. The application pins the mobile preview's exact channel and keeps data under its sandbox's `Data/tmp/quiet-sync-qa-upstream-cloud-CHANNEL`; it never opens another CloudLab library or the Production container. Existing CloudLab test apps remain untouched. Ship the engine as the signed main executable with the required bridge and resources. Verify the actual signature, embedded profile, Development entitlements, source revision and channel. Do not register production URL handlers, use shared groups or access real notes. Use the offscreen driver through `--qa-script FILENAME`, with scripts inside that isolated root. Never infer live synchronization from package validation alone.

Provide macOS, iPhone and iPad experience packages from the same candidate revision. The mobile builder is `packaging/ios/build-upstream-preview.py`; its cloud-enabled preview uses **`app.quietmd.cloudlab`**, configuration `UpstreamPreview` and local Apple Development signing with the matching CloudLab device profiles. It must never use the TestFlight identity `app.feinotes.mac` or its Production container. Use only `Development:iCloud.app.quietmd.cloudlab`, with the fixed `FeiUpstreamPreviewCloudChannel` from UpstreamPreview-Info.plist on both devices. This is a separate test library, not the user's production cloud library. Preserve the product's silent asynchronous startup and usable local fallback; a separate App ID is not a reason to disable cloud behavior. Keep the app sandbox separate and do not register the production invitation scheme or shared groups. Verify the actual extracted IPA signature, Development cloud entitlements, exact source revision and test channel. Device-specific development profiles may require separately signed universal IPAs. Keep signed IPAs and signing metadata in ignored private output directories.

The earlier `local.feinotes.upstream-preview.ios` package was a local-only preview. Do not silently repurpose it for production notes or delete its test notes. Before first installing the CloudLab preview, check that an unrelated app using that test identity is not already installed. Only update this task's own CloudLab preview thereafter. Real cloud checks use synthetic notes exclusively within the selected test channel; never infer success from compilation or unsigned simulator startup.

The user authorizes installing these separate preview apps on the local Mac and paired acceptance iPhone/iPad whenever device and signing conditions permit. Preserve installed product/TestFlight apps and their data. Verify the destination bundle identity before an update, install only the preview, and record device bundle/version readback and launch results separately. A locked or unavailable device is a reported installation/launch limitation, not an app test pass. Do not uninstall other apps, reset libraries, change device settings or publish remote install channels to overcome a blocker.

Deliver the candidate commit, verification results, app path and isolated data location to the user. **Daily automation stops here. Do not update `product/quiet-sync` until the user tests and explicitly approves that exact candidate commit.** A changed candidate invalidates the approval: rebuild, retest and obtain a fresh confirmation.

After approval, check that the product worktree is clean, no Git operation is in progress, the candidate matches the approved tested revision, and the product is an ancestor of that revision. Only then update the product worktree:

```sh
git merge-base --is-ancestor product/quiet-sync APPROVED_CANDIDATE_SHA
git -C ../quiet-md merge --ff-only APPROVED_CANDIDATE_SHA
```

If any guard fails, leave the product untouched. Refresh the independent candidate with committed product changes, retest, rebuild and ask the user to approve the new exact revision. Never reset, rebase or force-update the product to manufacture a fast-forward. Do not push or publish under the daily task. Send the authorized main-task handoff only after the approved fast-forward actually succeeds, once per resulting product commit.

The first integration preview against upstream `fcd2aff` found text conflicts in `src/app.rs` and `src/settings.rs`. Preserve upstream's common creation entry point and quick-new preference, and retain Fei Notes' shared-note guards and save behavior. Refresh the preview against the actual upstream revision before merging.

Generic contributions start on a clean upstream branch with only the independent fix. Reconcile an upstream-accepted patch during the next product merge.

## Data compatibility

Keep stable identities and pending operations through upgrades. Treat CloudKit cursors as recoverable download progress. Publish a verified compatible cloud checkpoint before deleting any history it replaces. Retain causal history needed by offline devices and the permanent-deletion protection. Cloud history compaction is tracked in [FEI-1](https://linear.app/unidrive-studio/issue/FEI-1).

Document SQLite and cloud protocol migrations with their exact supported versions. Backups must include pending changes and referenced attachments and must preserve the original library during recovery.

## Releases and website

Follow [APPLE-RELEASE.md](APPLE-RELEASE.md). Each build must be traceable to one source commit, app version and Apple build number. The TestFlight channel includes cloud access after verification of Apple's installed TestFlight signature or a sandbox app transaction. The Store channel uses its separate production access checks.

Keep release secrets in the protected GitHub environments. Run pull-request checks without them. Use an ephemeral signing keychain on the release runner and remove it after the job. Public TestFlight requires Apple's beta review and a public group invitation.

Website source is in `website/` and deploys to [feinotes.app](https://feinotes.app/) with GitHub Pages. Update the Chinese and English pages together. Download links must refer to the actual TestFlight invitation and describe its current availability.

## Licensing and provenance

Preserve upstream and third-party copyright notices. Check [LICENSES.md](../LICENSES.md) whenever adding or moving files. Keep independently authored product modules separate from upstream-derived files where practical. Verify asset provenance and generated dependency notices before publishing a release.
