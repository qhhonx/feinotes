# Fei Notes invitation links

Public links directly map a CloudKit share identifier to the static invitation
page. Chinese-region links use `https://feinotes.app/share/?c=<id>` and global
links use `https://feinotes.app/share/?g=<id>`. The Mac app maps `c` to
`https://www.icloud.com.cn/share/<id>` and `g` to
`https://www.icloud.com/share/<id>`. No lookup database, encoded original URL or
Vercel service is needed. The earlier unreleased v1 transport is intentionally
not supported. Raw Apple invitation URLs can still be pasted into the app.

`src/sharing/links.rs` and `website/share/links.mjs` validate the mapping. The
identifier is limited to URL-safe ASCII letters, digits, underscores and hyphens.
Unknown regions, additional parameters, fragments and other destinations are
rejected. CloudKit still validates the recipient's Apple Account and membership;
changing the domain grants no new access. Ending the underlying share ends
access through the Fei Notes link too.

## Opening

The Mac app also registers `feinotes://share/?c=<id>` and `?g=<id>`. Both this
fallback and an HTTPS Universal Link enter the same bounded invitation queue,
reuse the current window and show a confirmation. Opening a link never accepts
an invitation automatically. Busy requests retain their queued URL.

The landing page reuses the home page background, typography and download
button together, without the document card, outline or long help sections. A valid page
tries to open the app once, then retains a manual button and Get Fei Notes / Copy
Link options. A hidden page does not attempt to open the app. Returning from the
app does not trigger another attempt. Browsers may require user activation or
confirmation; the website cannot guarantee automatic launch.

No document content or remote destination is fetched by this page. It has no
analytics, advertising or external script. The identifier in the requested URL
is visible to the hosting provider. Referrer and indexing restrictions are
specified in HTML rather than a hosting-specific headers configuration.

## GitHub Pages

`website/` is the source for the existing `.github/workflows/fei-website.yml`
publication workflow. The `/share/` directory has a real `index.html`, so every
invitation receives the same static page with HTTP 200; no dynamic path rewrite
or custom 404 handling is required. Preserve the custom domain `feinotes.app`,
HTTPS enforcement and the existing email DNS records during the hosting change.

`website/.well-known/apple-app-site-association` lists the Mac App ID and
`/share/*`. Preserve `.nojekyll` and include the dot-directory in the Pages
artifact. GitHub Pages controls response content types; validate the deployed
extensionless file and Apple's association service before claiming Universal
Link acceptance. A fallback button/one-shot browser attempt is separate from
OS-verified Universal Links. The signed Mac app includes
`com.apple.developer.associated-domains = ["applinks:feinotes.app"]`; its profile
must authorize this entitlement.

Changing App ID capabilities invalidates old profiles for future builds.
Regenerate affected development and distribution profiles before the next
release. Never put credentials in Git. Do not publish a TestFlight beta as part
of this website change.

The current collaboration client is macOS. iOS must deliver user activities and
custom URLs to the invitation handler before it is advertised as supported.
Web editing is not released. A future web client can use the same identifier
and region without changing the CloudKit access model.

## Verification

Run `node tools/test-share-links.mjs` for region mapping, invalid URLs, automatic
opening once, blocked-launch fallback, bilingual status and copy behavior. The
offline `tools/harness/sharing_ui.py` additionally verifies that personal sidebar
order and expanded children survive shared navigation, shared titles update
before saving, shared drag order persists without touching personal metadata, and
member / deletion recovery flows remain isolated. Pointer drags and deletion
with a collapsed folder at the top are included.

Check installed signed-app cold/warm start, no-window reopening, invalid links
and two-account CloudKit access separately from these synthetic regressions.
Record the exact website snapshot, Pages run, DNS and app artifact in the local
release evidence. Previous Vercel deployment evidence remains historical, not
proof that the GitHub Pages transition is complete.

Apple references: [Associated domains](https://developer.apple.com/documentation/xcode/supporting-associated-domains),
[Universal Link debugging](https://developer.apple.com/documentation/technotes/tn3155-debugging-universal-links).

## Creation and local updates — 2026-11-17

First successful invitation copies a localized message, document title and link.
Failed requests do not copy a success message. Scripted QA stores this text in
memory instead of writing the system clipboard. Management offers moving the
original into personal Trash through the existing confirmation; cancelling
leaves both documents intact. In-place conversion requires cross-library
identity/reference, attachment and pending-CRDT migration with failure rollback;
this pass retains the separate shared-copy mechanism.

Both contexts reuse the same section header. Local edits update title, icon,
modification time and ordering before saving. Opened/new shared documents publish
metadata directly to the shared list, including the first row without waiting
for polling. Open-document metadata is retained across background polls. Remote
discovery still uses bounded CloudKit polling; received changes update the UI.
The invitation page now uses the home page visual context and versioned scripts.

Shared sidebar rows use the same drag, spring, lift, landing and removal renderer
as My Documents. Their flat manual order is a separate local SQLite setting
(`shared-sidebar-order`), independent of document metadata and CloudKit content.
Successful invitation creation copies only a short invitation line and the link;
copy feedback uses the existing transient toast above the member sheet. Errors
remain in the sheet and all management controls remain in its scrollable body.
Automatic selection after deletion uses a remaining visible document, preserving
folder expansion; if none is visible, it returns Home.
