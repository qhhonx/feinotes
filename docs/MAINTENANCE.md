# Maintaining Fei Notes

## Branches

- `product/quiet-sync` is the product and default branch. It contains the macOS application, documentation, release automation and `website/`.
- `main` remains the upstream mirror. The `upstream` remote points at `s1dashu/quiet-md`.
- `gh-pages` retains the website publication history. GitHub Pages also deploys `website/` from the product branch.
- Use short-lived feature, contribution and integration branches. Keep public source release tags immutable.

## Documentation boundaries

Preserve the upstream conventions in root AGENTS.md, CLAUDE.md and CONTRIBUTING.md. Put product additions in FEINOTES-AGENTS.md and FEINOTES-CONTRIBUTING.md. Root theme documentation retains upstream text; product theme additions live in FEINOTES-THEMES.md. See UPSTREAM.md for the baseline and immutable source links.

## Integrating Quiet.md

Commit the current product changes before starting an integration. Fetch upstream, then merge in an isolated worktree.

```sh
git fetch upstream main
git worktree add -b integrate/quiet-main ../feinotes-upstream product/quiet-sync
cd ../feinotes-upstream
git merge --no-ff upstream/main
scripts/check.sh
scripts/check-cloud-sync.sh
```

Review semantic interactions even when Git reports no conflict. Pay particular attention to document creation, editor selections and IME state, table identities, SQLite transactions, file import and shared-note permissions. Use targeted regression checks for paths changed by the integration.

The first integration preview against upstream `fcd2aff` found text conflicts in `src/app.rs` and `src/settings.rs`. Preserve upstream's common creation entry point and quick-new preference, and retain Fei Notes' shared-note guards and save behavior. Refresh the preview against the actual upstream revision before merging.

Generic contributions start on a clean upstream branch with only the independent fix. Reconcile an upstream-accepted patch during the next product merge.

## Data compatibility

Keep stable identities and pending operations through upgrades. Treat CloudKit cursors as recoverable download progress. Publish a verified compatible cloud checkpoint before deleting any history it replaces. Retain causal history needed by offline devices and the permanent-deletion protection. Cloud history compaction is tracked in [FEI-1](https://linear.app/unidrive-studio/issue/FEI-1).

Document SQLite and cloud protocol migrations with their exact supported versions. Backups must include pending changes and referenced attachments and must preserve the original library during recovery.

## Releases and website

Follow [APPLE-RELEASE.md](APPLE-RELEASE.md). Each build must be traceable to one source commit, app version and Apple build number. The TestFlight channel includes cloud access after Apple verifies a sandbox app transaction. The Store channel uses its separate production access checks.

Keep release secrets in the protected GitHub environments. Run pull-request checks without them. Use an ephemeral signing keychain on the release runner and remove it after the job. Public TestFlight requires Apple's beta review and a public group invitation.

Website source is in `website/` and deploys to [feinotes.app](https://feinotes.app/) with GitHub Pages. Update the Chinese and English pages together. Download links must refer to the actual TestFlight invitation and describe its current availability.

## Licensing and provenance

Preserve upstream and third-party copyright notices. Check [LICENSES.md](../LICENSES.md) whenever adding or moving files. Keep independently authored product modules separate from upstream-derived files where practical. Verify asset provenance and generated dependency notices before publishing a release.
