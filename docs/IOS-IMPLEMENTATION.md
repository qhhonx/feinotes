# Fei Notes iOS implementation and acceptance

## Current feedback candidate, 2026-11-18

Local candidate **0.1.9 (49)** addresses the latest nine build-48 findings:
neutral batch selection, Home with three peer scopes, native collapsing titles,
context-preserving Trash/document-link returns, mobile theme containers, preview
Close, the unified inline content catalog and consistent Compose accents. A
targeted iPad regression also fixes the stale reading guard after expanding
content. The current simulator acceptance selectors and evidence boundaries are
in [build 49](design/mobile-feedback-20261117/build49/README.md). This candidate
has not been installed on physical devices or uploaded to TestFlight. The
installation statements below are historical checkpoints, not build-49 claims.

## Current two-device installation, 2026-11-17

Product **0.1.9 (44)** has been reinstalled in place on both physical iPhone and
iPad at the user's request. Both version readbacks confirm build 44, both apps
launched and both processes were confirmed running. No uninstall or data reset
was performed. This is installation/startup verification; user interaction
acceptance remains open. See the [two-device installation record](design/iphone-keyboard-20261117/reinstallation44-two-devices.json).

## iPhone docked-keyboard follow-up, 2026-11-17

Product **0.1.9 (44)** is installed in place on the iPhone, with version,
launch and running-process verification. It follows the actual keyboard layout guide for toolbar
state and placement, settles unfinished native composition before ending input
or opening Format/Insert, and restores input after Format only for the same
writable note/scope. Short editor viewports reduce title top spacing so a docked
landscape keyboard does not spend the remaining text area on header padding.
All three final simulator UI cases and 232 Rust regression tests passed; the
full UI rerun and device installation outcome are in the
[build-44 evidence record](design/iphone-keyboard-20261117/README.md).
Physical XCUITest still failed at runner bootstrap, before executing cases.
The iPad retains build 42. User device acceptance remains open.

## iPhone interaction polish delivery, 2026-11-17

Product **0.1.9 (43)** is installed in place on the iPhone, with version,
launch and running-process verification. It refines native search clear/cancel,
query and logical-anchor restoration, top Undo/Share/More/End Typing, and the
reading/editing bottom tool groups. Two isolated simulator UI cases passed,
including Search software-keyboard geometry, landscape and large-text dark
controls. Physical manual QA passed the core search/navigation/toolbar flow;
physical XCUITest failed during runner bootstrap and is not counted as a pass.
Editor accessory geometry above a docked software keyboard remains unverified.
The iPad retains build 42. User acceptance remains open. See the
[implementation and evidence record](design/iphone-polish-20261117/README.md).

## Native iPad workspace delivery, 2026-11-17

The iPad candidate is **0.1.9 (42)** with native folders/list/document columns,
an independent searchable note list, native sidebar controls and document focus.
The same editor is retained across fit and orientation changes. Physical iPad
XCUITest passed against an isolated synthetic local library, including search,
selection, empty detail, focus and rotation. Product build 42 was installed in
place on the iPad; metadata and launch/process checks passed. At that delivery,
the iPhone remained at build 41. User acceptance remains open. See the [implementation and evidence
record](design/ipad-native-workspace-20261117/README.md) for tested scope and
remaining device/layout coverage.

## Current product integration, 2026-11-17

The mobile source is aligned with product snapshot
`0c7640eb39d912da151f7b9823074d9ce79b8a1c`, retaining the native iPhone/iPad
adaptations and the build-40 Swift/Rust namespace safeguard. The latest local Mac
shared-document implementation and theme/format research were reviewed separately
from committed product code. See the [integration and validation record](design/mobile-product-integration-20261117/README.md)
for source boundaries, portable theme fixes, session adaptation and the remaining
mobile sharing-interface target.

The preceding installed candidate was **0.1.9 (41)** on both physical devices,
installed in place on 2026-11-17 from the merged source. Both device version
readbacks confirm build 41. Both apps launched and their processes were confirmed alive after the user
unlocked the devices.
The user will perform theme/search/toolbar acceptance. See the [installation record](design/mobile-product-integration-20261117/installation41.json).
The build-38 statements and numbered sections below are historical records.

Original mobile application baseline: `83583bbc9c1e0c86a3ddbd7024f2cb4c390ed6ed`.
Integrated shared source baseline: `product/quiet-sync` at `3802182e524958c46d7859651216327f9657ad9f`.
Latest signed candidate: build 38, with shared source through `3802182e524958c46d7859651216327f9657ad9f`; installed over Wi-Fi with device metadata and launch verification.
Implementation branch: `mobile/iphone-layout`. Changes remain uncommitted.
Current Mac product head checked for build 38: `574a5dd87175403ec7fe7cc6495ca1069a6f8a1a`.
The integrated application baseline remains frozen at 3802182; the committed settings and shortcuts files have no delta between these revisions.
The 166-path committed delta through 3802182 is applied with mobile adapters.
Native local version history is implemented in build 34. Build 35 adds the legacy-library import entry and mounts the shared language/preference menus. The Mac worktree has separate in-progress changes that are not imported. User-reported iPhone interaction issues
take priority, while product synchronization has been accepted by the user.
The Mac application delta detected during build 23 packaging is integrated in
build 24. The mobile Git HEAD remains the original commit with uncommitted source;
the package records the integration manifest and complete source fingerprint.
First delivery under this objective: Fei Notes 0.1.9, build 4, for the existing registered iPhone.
Latest device delivery: build 38, installed over Wi-Fi on 2026-11-16; metadata confirms 0.1.9 (38), and launch succeeded.
Production synchronization was explicitly accepted by the user on build 30.
Physical interaction acceptance of the new toolbar remains open.
Public remote publication is still awaiting its separate authorization.
The sections below retain each build's evidence;
the initial matrix is a historical build-4 snapshot, not current completion status.

## Current alignment status, build 38; device build 38

Implementation, local interaction evidence and live-service acceptance are
tracked separately. This table does not convert a compiled feature into a
device- or service-verified result.

| Mac product area | Shared implementation and iOS mapping | Current evidence and open gate |
| --- | --- | --- |
| Mac code baseline | Shared source through 3802182 imported; native history entry points added | Mac and iOS compilation pass with mobile adapters. Build 35 includes native local history, old-library import and working language/preference menus; full user acceptance remains open |
| Product identity | Formal Fei Notes icon, 飞记 display name and existing installed bundle identity | Compiled icon and signed bundle checked; build 36 About reads the actual bundle version/build; no user-facing prototype label |
| Notes and folders | Existing create, rename, move, duplicate, pin, favorite and trash commands; native menus and folder picker | Create/rename/move, duplicate, pin/favorite, native delete/cancel, restore, purge and empty Trash verified locally; native batch selection/copy/move/delete and folder subtree copy/delete verified in build 23; document/Markdown creation, remembered type and new-folder child placement verified in build 24; native manual row dragging and restart persistence verified in build 26; physical interactions remain |
| Search | Existing product matching, ordering and snippets | iPhone search keyboard, case-insensitive matching, title-before-body ranking, snippets, clear and empty results verified; physical and hardware-keyboard modal acceptance remain |
| Text editing | Existing GPUI editor, Core, selections, marks, paragraph conversion and undo | Word/range selection, rich Copy/Paste, marks, colors, alignment, headings and Undo/Redo verified locally; physical feel and remaining rich-block controls remain |
| Find and replace | Existing matches, highlights, reveal, replace-one/all and Undo through UIFindInteraction | Native result navigation, mode switching, one/all replacements, Undo, reading restrictions and rotation verified; current-match visibility with the landscape software keyboard and navigation restoration verified in build 14; physical feel, hardware shortcuts and live concurrent replacement remain |
| Software Return | GPUI Mobile dispatches the existing Enter action for committed Return | Paragraph separation, list continuation/exit, code indentation and search-field Submit verified with SQLite/XML readback; physical keyboard/IME combinations remain separate |
| Insertion catalogue | Shared Mac slash items, grouping, translations, Markdown restrictions and existing insert operations | Complete native catalogue exposed; callout input, two-column creation/first-column input and code input verified; equation/button forms verified in build 16 and reminder/countdown forms in build 17; media controls and advanced container interaction remain |
| Equations and buttons | Shared equation source writes, button draft/validation/confirm, action definitions and palette; native forms and content list | Reopen/live equation input, action/default-label changes, visible URL errors, one palette selection, cancel and shared Undo verified; physical interaction, long source/IME and remaining object controls remain |
| Reminders and countdowns | Shared live reminder transforms, advance options, countdown normalization/confirm and palette operation; system date/number/toggle controls | Whole-day/year/advance/reopen, duration/end modes, cancel, visible past-end error, one palette color and numeric limits verified; color-only clock preservation source-reviewed; date-reminder permission, cancellation, delivery after termination and notification cold-open verified in build 21; countdown retains its original in-app bubble; denial/clock changes and device acceptance remain |
| Code blocks | Existing caption/language/wrap/fold/copy operations; native code form and shared language search | Name/language/wrap save and reopen, session fold and reading restrictions verified locally; header-entry feel, clipboard output and physical acceptance remain |
| Tables | Existing renderer and Core table operations; native controls and full shared palettes | Row/column changes, merge/split, cell text styles, column widths, deletion/Undo and read-only selection verified; complex spans, sticky headers and long-table scrolling remain |
| Outline | Existing catalogue entries and jump/unfold behavior in a native list | Heading navigation and reading-mode access verified; iPhone portrait/landscape no longer show the desktop overlay |
| Links | Existing link selection, validator, normalization, submit/remove and Undo; native text/URL form | Selected-word linking, invalid URL feedback, cancel, remove/Undo and bare-host normalization verified; external URL opening was intentionally not exercised |
| Images and attachments | Existing asset import/GPUI renderer and caption/size/alignment/crop/reset/history paths; native forms, crop gestures, Files and Quick Look/share | PNG insertion, caption/alignment/width, crop cancel/save/reset/Undo and crop rotation verified; original bytes unchanged; text attachment preview/share cancellation and reading access verified; system Photo Library cancel/PNG/HEIC import verified in build 20; web-image HTTP/HTTPS, redirect, retry, manual-only loading and typed cache preview verified in build 22; iCloud-only Photos, large/EXIF/animated media, gallery details and physical feel remain |
| Document information | Existing flush, live document counts, metadata times and asset-aware size; native system alert | Mixed Chinese/Latin text and table counts, rename refresh and reading-mode access verified in build 27; physical acceptance remains |
| iCloud version history | Existing journal version listing and restore-as-copy operations; native version selector | Two cached versions, disabled-until-selected, cancel/reset, older-version copy and local-library hiding verified in build 25; live history and device acceptance remain |
| Local version history | Shared SQLite snapshots, preview state, restore/copy/labels/filter and retention controller; native iPhone list and controls | Build 34 simulator verifies naming, named filter, preview/return, cancel/confirm restore, independent copy and cancel/confirm deletion. Synthetic SQLite verifies both bodies; retention 20 to 21 persists after relaunch. Shared cloud restore rollback/access checks pass; device interaction acceptance remains |
| Cloud-aware history | Existing CloudUndo event and CRDT-aware history path | Existing concurrent-text/style test passed; real iPhone concurrent-edit acceptance remains |
| Library sessions | Shared store/runtime leases, local/cloud selection and durable queues | Local session switching, cached-session isolation and recovery verified; live Apple Account changes and queue convergence remain |
| iCloud sync | Shared engine, journal, status and retry; build 28 uses the same Production container as the installed Mac | Build 27 used a Development QA channel and could not read product notes. Build 30 uses the product App ID with a separate Production journal. The user confirmed Mac-note visibility and explicitly accepted iCloud sync on 2026-11-15; no independent new round-trip measurement is claimed |
| Shared notes | Shared invitation validation, permissions, CRDT and native settings/session entry | Cached read-only/writable sessions verified; cross-account invitation, permission changes and revocation remain live-service gates |
| Cloud access and subscription | Shared Rust controller and Swift StoreKit bridge | Local SDK service tests passed with dialogs suppressed; native confirmation remains unverified; shipped paid product IDs remain empty like the inspected Mac beta |
| About and support | Shared product About page and configured URLs; native component catalogue and paged reading | Build 36 verifies component search, complete bounded pages, next/previous, page-local system Find and return; packaged notice hashes and actual version checked; device feel remains |
| Settings and themes | All seven categories use native grouped UIKit pages and choice screens; Rust owns preferences and existing operations | Build 38 verifies immediate language changes, toggle persistence after relaunch, default type, search engine, appearance/size/motion/theme selection, native preview, library/cloud/shortcut navigation and About/notices. CSS editing remains disabled by the current shared product gate. Live sharing/purchase service checks and device preference acceptance remain |
| Import and export | Product codecs and serializers with Files dialogs | XML import, Markdown export, native PDF, PNG insertion and text attachment import verified; build 35 imports a synthetic FORMAT 4 library through the native folder picker, retaining four IDs, parent/link/attachment/Trash/front matter and source bytes; build 37 confirms multi-file cancel/reopen, single-file sheet dismissal/reopen, image selection and original PNG bytes, and export dismissal/reopen; device/provider acceptance remains |
| Backups and restore | Shared portable backup preparation, validation and pending-edit safety | Local backup export/restore through the shared native confirmation and preservation checks passed; live-cloud recovery acceptance remains |
| Printing | Product HTML and native AirPrint | Presentation/cancellation verified; no physical printer job claimed |
| Navigation and layout | Native stack/sheets with shared GPUI views | Build 27 user feedback failed navigation, redundant labels, popup placement, keyboard and list feel. Builds 30/31 replace those entry points; later settings/history fixes have focused simulator evidence. Fresh user device acceptance remains pending |
| Distribution | Versioned, signed registered-iPhone IPA with source/content provenance | Build 27 installed/launched and product-identity build 30 installed directly over Wi-Fi; build 38 installed over Wi-Fi with metadata and successful launch verification; user accepted sync on 30; UI acceptance and remote delivery remain open |

## Objective and completion gate

Deliver the existing Fei Notes product on iPhone, then adapt the same application
for iPad. Match applicable Mac product behavior through shared Rust code. UIKit
owns navigation, interactive back, sheets, naming forms and system file dialogs;
Longbridge GPUI Mobile runs the existing editor and product views. Use the Fei
Notes icon and product name throughout the app and installer.

Full Mac feature alignment is the completion gate. Build 4 is an interaction and
branding acceptance delivery toward that gate; it does not complete the cloud
account, collaboration or platform-service work listed below. A compiled path,
a visible control and a device/service-verified feature are different evidence.

## Feature acceptance

| Area | Required result | Build 4 implementation and evidence |
| --- | --- | --- |
| Branding | Product icon/name; no prototype label in app or installer | Original Fei Notes artwork adapted to the iOS icon set; display name is 飞记; simulator Home Screen inspected; device asset catalog compiled |
| Shared source | Product model, Core, persistence and sync protocol | Same product source; `crates/core` has no changes; desktop GPUI and mobile GPUI use explicit adapters |
| Notes and folders | Listing, creation, rename, move, duplicate, pin, favorite and trash | Uses product `home_entries`, `level_entries`, sorting, create/rename/move/copy/pin/star and trash commands. System menu and folder picker expose them. Simulator UI verified create, rename and move; SQLite checked afterward. Duplicate, pin/star and restore/purge still require interaction acceptance |
| Search | Mac matching, title-first order and snippets | Calls existing `search_results` and `search::snippet`; no independent mobile matching algorithm. Search interaction acceptance remains open |
| Editing | Existing block editor and formatting behavior | Original GPUI editor and Core; native formatting menus dispatch existing actions. Workspace command bindings extracted once and used by both shells. Text input and export of the saved text verified; complete rich-content touch acceptance remains open |
| Cloud undo/redo | Existing CRDT-aware undo path | iOS now emits the same CloudUndo events and installs history/presence state. Existing `undo_redo_keeps_remote_text_and_independent_style` test passed; real iPhone concurrent-edit acceptance remains open |
| Cloud library | Local/cloud selection, account changes, status and durable queue | Existing Development test library and persistent channel selection preserved; shared sync status/retry and issue details available. Consumer library switching remains pending |
| Collaboration | Invitations, permissions and received notes | Pending iOS session lifecycle and invitation presentation; Mac code remains in the product branch |
| Subscription/access | Same configured access rules and purchase/restore behavior | Pending iOS StoreKit product configuration and purchase presentation. Development test sync must not be represented as the consumer access flow |
| Settings | Same applicable preferences, themes and status | Reuses General, Appearance, Shortcuts, About and cloud status; SQLite preference persistence shared, legacy JSON fallback retained. Theme menu is rendered by the mobile shell. CSS edit/file actions need an iOS document workflow |
| Import/export | Product formats with system file access | Existing import entry and Markdown serializer; system Files export supports text or the Markdown folder with attachments. Simulator exported a Markdown file and its title/body were read back. Attachment-folder export and import interaction acceptance remain open |
| Backups | Portable format, pending changes and restore safety | Existing backup creator runs on a background executor and exports through Files; simulator UI exported a backup. Existing backup restore test passed, including pending edits, Trash and attachment. In-app iOS restore is pending |
| PDF/print | Same HTML content through platform facilities | Pending iOS WebKit/PDF and AirPrint presentation |
| Layout | Native navigation and adequate touch targets | Native UINavigationController transitions, interactive back, system action sheets and naming form; keyboard-safe shared editor. Portrait and landscape navigation checked on one iPhone simulator. iPad layout refinement remains a subsequent phase |
| Tables | Preserve product column dimensions | GPUI Mobile grid adapter uses content-sized tracks and explicit sizing cells instead of equal-width columns; compile checked, unequal/merged-cell visual acceptance remains open |
| Distribution | Versioned signed IPA, provenance and HTTPS installer | Device build signed with existing registered-iPhone profile; stable app bundle identifier preserves the installed library. Artifact hashes and live channel are recorded outside Git |

## Verification performed

- One iPhone simulator, normal UI entry points: new folder, new note, text input,
  native rename and saved title, native folder move, Markdown export to Files,
  library backup export to Files, settings and iCloud status, return, landscape.
- Synthetic SQLite and exported Markdown readback corroborate UI success.
- Cloud-enabled Mac product compile passed. Existing cloud-aware undo and
  portable backup restore tests passed; no broad device matrix was run.
- The signed device bundle's identifier, version/build, icon, signature,
  registered-device profile and Development CloudKit entitlements are checked
  before the IPA is exposed through the installation channel.
- Physical interaction, iPhone/Mac sync and interactive-back feel are user
  acceptance items. Simulator evidence does not establish those results.

## Initial implementation targets, historical build 4

The current alignment table above supersedes the original pending list below.

1. **Library session lifecycle:** the Mac process-restart boundary must become a
   safe iOS session boundary before exposing local/cloud and account switching.
   Preserve and close the old queue/store, select the new account/library, then
   recreate editor and sync state; never change a live global path underneath an
   active editor or sync worker. Existing startup-selected session locks need a
   deliberate shared abstraction, not an iOS-only copy of storage logic.
2. **Sharing and access:** reuse invitation validation, permission checks and
   shared CRDT logic behind that lifecycle. Present invitations and purchases
   through iOS, and verify actual account/product configuration separately.
3. **Platform output and restore:** use existing HTML/export/backup logic with
   native PDF/print, backup selection/verification/restore and theme-file editing.
4. **Touch completeness:** validate rich blocks, selection handles, undo during
   concurrent edits and all note-management entry points on the existing iPhone.
5. **iPad:** reuse the completed commands and services with an adaptive split
   layout; keep the existing iPad as the only hardware target for that phase.

Finder reveal, Mac menu-bar shortcuts and desktop window management map to
appropriate iOS entry points where relevant. No elapsed-time estimate is inferred
from model training dates; subsequent effort follows the remaining concrete
implementation and device findings.

## Installed data and development identifiers

Preserve `app.quietmd.cloudlab`, the existing Development container, channel and
cached library. Historical environment keys, UserDefaults keys and sandbox
folder identifiers are migration details and are retained for data continuity;
they are not product-facing names. Do not delete or rename an installed user's
library to remove an internal identifier. Use fresh synthetic data for tests.

## Build 5 session and recovery implementation

Build 5 extends the same worktree; build 4's delivered IPA remains immutable.

- A selected SQLite session now has leases held by its stores, journal and file
  generation jobs. Replacing the root requires the expected previous root and
  no remaining owners. The content model, document codec, SQLite schema and
  sync protocol are unchanged.
- The shared sync runtime owns its tasks. Closing it invalidates a runtime token
  before cancellation; delayed responses cannot update a subsequent library.
  Resume/account callbacks point to the current runtime rather than a one-time
  process-global sender.
- iOS replaces the Workspace entity inside the existing GPUI window. It flushes,
  closes services and stores, waits for leases, selects the verified target,
  reloads library preferences/themes/shortcuts and creates the next Workspace.
  A failed preparation keeps the current session. An activation failure attempts
  to reopen the previous library. Mac keeps its existing restart presentation.
- The historical flat local library is copied into `local-only` and the original
  remains intact. Atomic directory publication prevents a partial copy from
  becoming the selected library. Cloud caches remain siblings of the local
  library, so restoring the latter does not relocate unrelated cloud queues.
- Backup selection uses GPUI Mobile's native file picker. The system confirmation
  uses the same verified backup summary as Mac. Restore reuses the portable
  backup verifier and atomic installer, preserves the former library, and
  recreates the editor without restarting the app.
- Native library choices expose local, cloud and copy-local-to-cloud behavior.
  Explicit cloud preparation requires the account/access check, as on Mac.
  Startup can fall back to the local library while preserving the original
  cloud cache, and offers reconnect. Real account/access transition acceptance
  remains separate from offline-cache tests.
- Navigation publications are deduplicated and numbered. UIKit ignores older
  publications, including while a transition buffers newer state.

### Evidence

The session lease tests, isolated two-library round trip and runtime cancellation
check passed. The round trip kept the original note identity, peer and pending
edit, rejected switching while owners remained, and rejected an old callback
against the next library.

An upgrade on the existing iPhone simulator copied five synthetic notes and seven
entries. Documents, entries, Trash, blocks, search fragments and library values
matched, excluding the expected last-opened timestamp. The legacy database was
retained and SQLite integrity passed.

A UI check generated its own backup, added another note, restored through Files
and the system confirmation, then created and saved a new note. Data readback
confirmed matching original note identities and bodies, retained the post-backup
note only in the preserved former library, and saved the new post-restore edit.
The cloud-enabled Mac compile and existing backup format/queue test passed.

Offline cloud-runtime-to-local UI acceptance passed with a synthetic cached
account and a paused transport. Readback confirmed that the cloud note and its
queued update were unchanged, the old journal lock was released, a new local
edit was saved only to the local library, and the selected mode persisted. This
does not establish live CloudKit upload, account login, subscription access or
cross-account sharing. Simulator
CloudKit initialization uses Xcode's simulated-entitlements section with the
configured container. Xcode signs the simulator host with empty actual
entitlements; adding restricted CloudKit keys to the actual host signature can
prevent process launch. Build 25 records this distinction explicitly.

### Remaining completion gates after this implementation

- Validate both directions of live iPhone/Mac sync and account/library changes on
  the existing registered iPhone; confirm safe behavior on service errors.
- Adapt shared-note session selection, invitation handling and permission UI.
- Connect StoreKit product configuration and native purchase/restore presentation.
- Implement PDF/AirPrint and native theme-file editing using the product content.
- Complete rich-block touch interaction and the planned iPad adaptive layout.

The earlier build-4 matrix records that delivery's evidence. Build-5 session and
backup work supersedes its pending implementation entries; service and physical
acceptance items remain open until their own evidence is available.

## Build 6 native output and theme editing

Build 6 keeps the delivered session lifecycle and the same product baseline.
It adds the iOS backends for existing PDF and print commands, and a native editor
for the theme stylesheet selected by the existing Appearance page.

### Implementation

- The Rust document-to-HTML generator is unchanged. Headings, lists, tables,
  equations and embedded/cached images follow the same product serializer.
- A temporary nonpersistent WKWebView lays out that HTML. UIKit paginates it on
  A4 with the existing 48 pt horizontal and 40 pt vertical margins. Preparation
  waits for fonts and images with bounded timeouts. PDF export uses the native
  Files picker; printing presents the same PDF through UIPrintInteractionController.
  Printer selection, paper/range controls and cancellation belong to iOS.
- Completion owns its Rust callback state independently of the requesting view.
  User cancellation and failure release the native job. The editor is retained.
- The app exposes its Documents directory through the standard Files integration;
  library databases remain in Application Support. Chinese and English are
  declared as supported localizations for system presentation.
- Edit CSS and Duplicate still use the existing Rust theme file preparation.
  iOS presents a UIKit text view with code-friendly keyboard settings and native
  Save/Cancel. Saves are atomic and refuse to overwrite a concurrently changed
  file. The existing Rust watcher/parser reports rules and refreshes styles.

### Evidence

One iPhone simulator imported a synthetic Markdown document through Files,
exported PDF through the normal menu, presented the system print preview, then
cancelled back to the still-running editor. The exported PDF has seven A4 pages;
all 96 numbered paragraphs are present in order, with both boundary markers.
Its embedded 240 x 120 image retains all four sampled source colors exactly.
All seven pages were rendered and inspected. A physical printer was not used.

The Appearance -> Edit CSS UI check opened the native editor, inserted a unique
comment and saved back to settings. Readback matched the complete bundled
stylesheet after removing that test comment. No theme format or parser changed.
The cloud-enabled Mac product compile passed. No broad regression suite or
additional iPad was used.

### Remaining gates

Physical iPhone interaction, live iPhone/Mac sync and real account/access changes
remain acceptance work. Sharing invitations and permission sessions, StoreKit
configuration/purchase presentation, complete rich-block touch behavior and iPad
layout remain implementation or acceptance targets. Printer output and broader
custom-theme behavior need their own device evidence. The earlier delivery
matrices remain historical; this section supersedes their pending PDF/print and
CSS editor implementation entries.

## Build 7 shared sessions and access backend

Build 7 retains the same product baseline, Fei Notes icon, installed bundle
identifier and Development container. It is an iPhone delivery; iPad refinement
and full physical service acceptance remain separate completion gates.

### Shared sessions

- The existing shared-note scope can be replaced at an iOS session boundary.
  Before opening a cached room, the product validates its canonical identity,
  descriptor and personal-library account. The old Workspace, journal and sync
  runtime close before the root and active scope change. Returning opens the
  original personal cache, including while offline, rather than preparing a
  new cloud account. Recovery reinstalls the previous root and scope.
- Shared sessions use the existing room transport without a personal QA channel.
  Their CRDT, attachment projection, durable queues and access checks are the
  same Rust/CloudKit implementation as Mac. The personal account selection is
  committed from the personal root, never from the room directory.
- The existing Shared Notes settings/controller now serves iOS. Accepting a
  pasted invitation, creating a share, fetching members, inviting view/edit
  access and changing/revoking access call the same product operations. Native
  menus expose Shared Notes, Share this note and Return to my library.
- Read-only or paused rooms keep the shared editor read-only, hide formatting
  and destructive catalogue actions, and reject stale native edit commands.
  Switching cannot proceed while an invitation-management operation is active;
  those jobs hold session leases. General text fields use the existing mobile
  metrics adapter for readable text and 44 pt touch height.

### Subscription backend

The iOS unavailable stub is replaced by the existing Rust subscription controller
and shared Swift StoreKit backend. iOS supplies its attached foreground view
controller (or scene on older supported systems). Product matching, verified
transactions, pending/cancellation handling, restore and entitlement refresh are
shared. Optional bundle product IDs remain empty in this distributed build,
matching the inspected Mac beta configuration. Local test product IDs belong
only to the StoreKitTesting configuration, not the signed user package.

A dedicated local StoreKit test plan verifies annual purchase, current status,
restore and pending-without-access through the actual SDK and shared bridge,
with purchase dialogs suppressed. Both service tests pass. Native confirmation
and cancellation remain unverified: the SDK awaits purchase without presenting
its sheet on this simulator, both in Xcode and the CLI. A separate UIKit-only
scene application reproduced the same result. This narrows the investigation
but does not establish the cause or a successful real purchase.

The application now shuts down the GPUI renderer explicitly on the main thread,
before thread-local destruction. This removes the renderer shutdown crash seen
when the hosted StoreKit tests ended.

### Evidence and remaining gates

The cloud-enabled Mac compile and iOS simulator/device builds pass. The targeted
room-selection/account test, existing permission/revocation/queue test and
collaborator-read-version test pass. A separate explicitly requested synthetic
fixture uses actual room staging, CRDT projection and read-only merge.

On the existing iPhone simulator, normal menus opened the cached read-only room,
kept the keyboard closed and hid edit/delete actions, returned to the personal
library, opened the writable room, entered a body marker and returned again.
A new personal note remained editable and saved after both switches. SQLite
readback confirmed unchanged read-only title/body and zero queued updates;
exactly one writable body marker with unchanged title; queue growth from two to
three while retaining both previous updates byte for byte; unchanged original
personal content; stable room/document identities; and no shared document or
body in the personal library. Integrity checks pass. A prior title-input check
is preserved separately; it is not substituted for the body-input evidence.

The test uses a cached synthetic account and a paused transport. It does not
establish cross-account invitation delivery, live permission changes, revocation
from another device, live CloudKit upload or physical iPhone/Mac convergence.
Those remain acceptance gates, along with native StoreKit presentation, complete
rich-block touch behavior and iPad layout. The existing signed installation
channel delivers build 7 for the registered iPhone. Build 6 remains immutable.

## Build 8 touch selection and rich clipboard

Build 8 keeps the same product baseline, Fei Notes branding, iPhone bundle
identifier and Development CloudKit container. It adds touch selection to the
existing editor without introducing another document view or editing model.

- GPUI Mobile long presses select a word using the existing Core word boundary
  logic. Long-press movement and 44 pt endpoint touch targets extend Core's
  selection; GPUI draws the selected range and endpoint knobs. Selection stays
  inside the same table-cell scope. Cancelled gestures restore their prior
  selection. Remote document replacement or a reading/permission transition
  invalidates old gesture and menu state.
- UIEditMenuInteraction supplies native Cut, Copy, Paste, Select All and text
  style actions. These dispatch the existing editor actions. Menu requests are
  checked against the requesting editor's generation and exact text/block/cell
  selection. Queued presentations are invalidated when their requester closes.
  Reading mode exposes Copy and Select All without opening the keyboard.
- A UIKit pan recognizer accepts only contacts within 22 pt of the published
  selection endpoints and leaves the screen's leading 8 pt to navigation. It
  reserves that contact against the system menu's dismissal recognizers;
  cancelsTouchesInView is false. GPUI continues to receive and own the actual
  phased drag. This addresses the observed raw-touch cancellation while the
  native edit menu was closing; it does not implement another selection engine.
- The pinned GPUI Mobile clipboard patch constructs NSString with explicit
  UTF-8 byte lengths, preserves GPUI JSON fragment metadata in a private
  pasteboard flavor, and transfers supported encoded images. The existing
  product HTML import path can read the iOS HTML flavor. Binary reads are
  bounded. These capabilities are separate from their acceptance coverage.
- The transient Saving indicator overlays the page's top edge, so routine
  autosave does not move the text and selection. Save failures retain a visible
  message. No document format, schema, CRDT or Core editing operation changes.

### Evidence

On the same iPhone simulator, the two targeted UI flows pass on the final
source: word selection and native rich Copy/Paste followed by Undo/Redo; and
endpoint extension followed by read-only Copy/Select All and paste into another
note. SQLite readback verifies the source and copied fragment retain their
bold XML mark, and endpoint extension copies exactly `Bravo Charlie`.
Read-only full selection and its Copy menu are visible in the captured UI.
The test first copies only one word before Select All, so retaining an older
full-range clipboard cannot substitute for the final copy. The shared Undo
handler was observed changing body length 5 -> 0 and Redo 0 -> 5; settled
screenshots also show the empty and restored body. Temporary instrumentation
is removed from the distributed source.

The Mac cloud build and iOS simulator/device builds pass. Both tracked platform
patches reconstruct the staged dependency files from the pinned source.
No broad regression suite or additional iPad was run. External-app HTML/image
paste, every rich block's touch controls, long-document drag autoscroll and
physical iPhone interaction still need dedicated acceptance. Live CloudKit,
account transitions, cross-account sharing, native StoreKit confirmation and
iPad layout retain their earlier completion gates. This delivery does not
represent those gates as complete. Builds 6 and 7 remain immutable.

## Build 9 native table controls

Build 9 keeps the product baseline, shared GPUI editor, formal Fei Notes icon,
installed bundle identity and Development CloudKit configuration. The editor's
More menu now exposes Table controls while the caret or selection is in a table.
Selection-context observation updates this entry without treating caret movement
as a document edit.

UIKit presents an accessible grouped sheet with system fonts and touch-sized
rows. It offers cell, row, column and table selection; right/down range expansion;
row/column insertion, deletion and movement; merge/split; header row/column;
column distribution and width adjustment; vertical alignment; and clear/light-blue
cell backgrounds. Existing Core operations perform every document mutation.
The desktop toolbar and native sheet share the table-command dispatcher for
insertion, merging, splitting, deletion and cell styles. No Core operation,
document format, SQLite schema or sync protocol was added in this build.

The native sheet checks its requesting editor's generation and exact text,
block and cell selection before executing a command. Remote document replacement
and reading/permission transitions invalidate it. Reading mode exposes selection
without mutations or a keyboard. Desktop hover pins, 10 pt bars and column-resize
hot zones are disabled on iOS; table content and horizontal scrolling stay in
the shared renderer. Native formatting dispatch waits for its menu to dismiss;
table insertion then restores the keyboard using GPUI Mobile's existing iOS API.

### Evidence and remaining work

The targeted final-build flow passes on the same iPhone simulator: insert a
table, enter cell text, add a row and column, select and merge two cells, split
them, change backgrounds/vertical alignment/header/column width, move rows and
columns, delete the table and Undo, then enter reading mode and select the table.
Settled screenshots show the merged span, unequal column widths and the
selection-only native sheet. SQLite readback confirms a restored 4-by-4 table,
widths 100/132/100/100, one header row, the two styled cells, exactly one original
cell marker and no remaining merged spans. The integrity check passes.

Cloud-enabled Mac compilation, iOS simulator/device compilation and device
signature checks pass. The packaged icon matches build 8. No broad regression
suite or additional device was used. Native table controls still need physical
iPhone acceptance. The complete cell-color palette, rich-text cell controls,
complex rowspan/sticky-header behavior and long-table scrolling remain explicit
touch-alignment work; existing unsupported values are preserved. Live iPhone/Mac
CloudKit convergence, cross-account sharing, native StoreKit confirmation and
iPad layout retain their earlier gates. Builds 6 through 8 remain immutable.

## Build 10 shared formatting and native outline

The existing native table controller becomes one reusable editor-controls sheet.
Native navigation pushes its style, paragraph and palette pages. The existing
GPUI view remains the only document view. The system edit menu exposes More
styles for the selected word; the editor More menu also opens formatting.

All five text marks, nine heading levels, text/list/checklist/code/quote types,
three paragraph alignments, eight text-color choices and sixteen highlights
dispatch the existing `Editor::toggle_mark`, `turn`, `align` and `apply_color`
methods. Table cell selections use these same methods, including whole-row and
multi-cell formatting. All sixteen table background choices come from the same
palette as the desktop. Unsupported color/alignment options stay hidden for
Markdown-backed notes; their supported text and paragraph operations remain.
Redundant mobile shortcut dispatch branches are removed.

Native requests share a purpose-tagged generation/selection stamp. Stale requests
or a read-only/permission transition cannot apply formatting to another context.
Reading mode hides mutation controls while retaining native Copy, Select All and
Outline. No document model, codec, Core operation, schema or sync protocol changes.

The heading outline moves to the same system sheet on iPhone. It uses the existing
catalogue entries and heading-jump/unfold behavior. The desktop floating catalogue
previously covered the text as soon as a heading existed in a narrow note; iPhone
now uses the mobile page layout in both orientations and the explicit Outline
entry. The Mac catalogue remains unchanged. Larger iPad layout still has its
separate adaptation gate.

Navigation commands wait for an open system alert/menu to dismiss. Navigation
items keep their identity when only note state changes; only their enabled state
and menu contents update. This fixes the reproduced unintended Back after a
reading-mode update. Temporary diagnostics are removed from the prepared package.

### Evidence and remaining gates

On the same iPhone simulator, the targeted body flow verifies selected-word
formatting, complete palettes, heading conversion, Undo/Redo, portrait/landscape,
native outline jumps and reading-mode navigation. The table flow verifies the
full background palette and selected-row text formatting in addition to the
build-9 operations. The existing endpoint/read-only Copy and Select All flow
also passes. SQLite/XML readback corroborates the selected word's exact marks
and colors, paragraph alignment, restored table structure, styled cells and
unchanged original markers. Device and Mac cloud-enabled builds are checked
separately from runtime and service acceptance.

This build does not establish physical iPhone/Mac convergence, cross-account
CloudKit sharing, actual StoreKit confirmation, every rich block's touch
controls, complex table scrolling/sticky headers or final iPad layout. Images,
attachments, links and the remaining block-specific controls still need their
own applicable Mac-to-iOS interaction checks. The registered iPhone is not
locally reachable; public installation-channel publication remains pending
explicit authorization. Builds 6 through 9 remain immutable.


## Build 11 insertion, links and attachment preview

The same editor-controls sheet now exposes image, file, link, table and divider
insertion. The keyboard bar offers full formatting and insertion through 44pt
controls rather than separate abbreviated formatting logic. Reading mode has an
explicit Edit note entry and keeps insertion controls hidden.

The native link form uses the existing Rust input models, URL validator,
normalization, submit, remove and Undo operations. It preserves the requesting
selection and reports validation errors without discarding the form or keyboard.
Interactive dismissal cancels the shared link context. A long press or handle
drag clears a pending link click so selecting a link does not open it.

Image and attachment insertion reuse GPUI Mobile's pinned document picker, which
imports a sandbox copy. Shared asset construction runs on the background
executor. iOS callbacks retain their library session lease and validate the
requesting editor generation, selection and permission before inserting. A
changed context rejects insertion; an imported but unreferenced cache asset may
remain. No document codec, schema, sync protocol or Core operation is added.

A native attachment list reads the current shared document, including table
contents, and remains available in reading mode. Missing local files are marked
unavailable. Existing file preview and export commands map to Quick Look and the
native Files workflow on iOS. The preview controller is embedded in a separate
native navigation container: presenting/dismissing the share sheet otherwise
caused Quick Look to replace the app's Done button. Editing is disabled in Quick
Look so a preview cannot overwrite a library asset outside the document engine.

The third tracked GPUI Mobile patch fixes URL construction from a Rust string
slice and supplies UIKit with a valid options dictionary. Reconstructing the
pinned revision with all three patches reproduces all 179 staged source/license
files byte for byte. This is separate from exercising an external URL.

### Evidence and remaining gates

The same iPhone simulator passed the focused link flow: selected-word linking,
shared validation errors, cancel, remove and Undo, plus a new link whose bare host
was normalized by the shared implementation. SQLite/XML readback confirms the
exact linked text, URL and unchanged surrounding marker.

A second focused flow imports a synthetic PNG and text attachment through normal
Files UI, displays the image in the shared editor, opens native preview/share,
cancels sharing, returns to the editor and previews the text attachment's actual
content. Reading mode retains both attachments and exposes no insertion entry.
Readback verifies one image and one attachment in that note, the original file
name and exact asset hashes. PNG and UTF-8 text are the formats exercised here;
this is not acceptance of every decoder, Photos, large video/audio or all inline
media controls.

The cloud-enabled Mac check, iOS Rust builds and signed iPhone package are
verified separately. The formal icon, display name and existing installed bundle
identity are preserved. Public Cloudflare publication still awaits explicit
authorization; no new channel was started. Physical iPhone/Mac convergence,
cross-account sharing, StoreKit confirmation, remaining block controls and the
adaptive iPad layout remain open. Builds 6 through 10 remain immutable.


## Build 12 shared confirmations and search input

Mac and iOS now obtain confirmation text and execute confirmed actions through
one shared `Workspace` implementation. The existing Mac confirmation view keeps
its controls and behavior. iPhone uses a system alert for delete, restore, purge,
empty Trash, backup restoration and the other applicable confirmation variants.
Choice rows preserve the shared reference-list and copy-to-cloud flags; save
failure keeps Retry, Save a copy and Reload. Native confirmation callbacks carry
a serial checked against the current library, target and confirmation state.
Each accepted response consumes its serial, so a renewed identical confirmation
can be presented as a new request. Replacing or cancelling the context dismisses
an obsolete native alert. Empty
Trash also includes the current Trash identities in its request context.

The iPhone Trash page gains its missing native Empty Trash action, disabled when
there are no items. Note rows display the existing pinned icon. The search bar
has a 44pt clear target and explicit empty-result/favorite states. Its entire
background can receive a tap. Shared single-line text inputs request the GPUI
Mobile software keyboard on iOS, fixing the reproduced search-field tap that
focused the field without presenting a keyboard. Mac input is unchanged.

### Evidence and remaining gates

One targeted management flow on the existing iPhone simulator creates and
duplicates a note, pins/favorites the copy, cancels and confirms deletion,
restores it with both flags intact, then cancels/confirms permanent deletion and
empty Trash using only notes made by that flow. The preflight snapshot confirmed
Trash was empty. Final SQLite readback confirms distinct original/copy document
identities, identical body text, retained pinned/favorite metadata, no purged or
emptied note, and an empty Trash. All 61 pre-existing synthetic documents and
their block payloads match the preflight snapshot exactly.

The focused search flow creates an older title match and a newer body match,
searches with different letter case and opens the title match first. Screenshots
show both ordered results, the shared body snippet, the empty-result caption and
clear/reopen behavior. The keyboard is observed after tapping the search bar.
The generic confirmation bridge also passes a selected backup/restore flow:
restored and new edits remain usable, while the post-backup note exists only in
the preserved former library. The shared restore format is unchanged.

The cloud-enabled Mac check, both iOS Rust builds, native compilation, signing,
formal icon and existing bundle/profile identity are checked separately. This
flow does not exercise every confirmation variant: real trial/account/library
changes, copy-to-cloud, reference-list choices and save-failure recovery remain
separate acceptance items. Folder subtree/batch management, hardware-keyboard
search, remaining rich-block touch controls and the earlier physical/service
gates remain open. The registered iPhone is still not locally reachable, and
public installation publication still awaits the pending Cloudflare approval.
Builds 6 through 11 remain immutable.


## Build 13 native document find and software Return

The iPhone document-find entry now presents UIKit's `UIFindInteraction` navigator
with a custom `UIFindSession`. The existing GPUI editor remains the document
view. Shared find state supplies matches, highlights, reveal, counts, previous/
next and replacement; the native session does not scan or rewrite document text.
The mode menu exposes only the shared case-insensitive contains behavior and
Find/Find and replace, rather than offering unsupported matching options.

One logical Rust find session retains one native session across navigator mode
changes. A new document/find session gets a fresh interaction. UIKit can retain
its active session while toggling modes, so creating a second native object made
replacements persist while the displayed count stayed stale. Mode transitions
now wait for the prior navigator to end and reuse the logical session object.
Native callbacks and UIKit presentation are deferred across GPUI app leases,
avoiding the reproduced reentrant borrow during mode switching. Temporary trace
logging is removed from the prepared build.

Requests carry the find-session identity and document generation. The editor
rejects stale replacement requests and checks current reading/shared permissions.
Reading mode keeps find navigation but does not offer replacement. Leaving the
note or changing permission closes the native context. The GPUI formatting bar
is hidden while the system navigator is active.

A fourth tracked GPUI Mobile patch maps a committed software Return to the
existing GPUI Enter key event path. The shared editor intentionally ignores an
isolated newline text commit because paragraph/list/code behavior belongs to
Enter; previously the mobile backend sent only the text commit. The legacy
global text callback and marked-text commit paths are preserved. All 179 staged
dependency files reproduce exactly from the pinned source and four tracked
patches. There is no new Core operation, document format, schema or sync protocol.

### Evidence and remaining gates

The existing iPhone simulator passes the focused native find flow: four matches
across title/body, previous/next, mode switch, one replacement, all replacements,
updated result counts, Undo, reading-mode options and both orientations. XML
readback after Undo confirms the restored title and one retained individual
replacement in the original two paragraphs. The Return flow verifies shared
list continuation and exit, exact code indentation and single-line search Submit.
All 74 pre-existing synthetic documents and their block payloads match the
previous package's readback.

Cloud-enabled Mac and iOS builds, signing and formal identity are checked
separately from runtime acceptance. The landscape software keyboard leaves a small document viewport; unobscured
match positioning there remains a layout follow-up. Physical interaction,
hardware-keyboard/IME combinations, live concurrent replacement, remaining block
controls and the previous service/iPad gates remain open. The public installation update still
awaits the pending Cloudflare authorization. Builds 6 through 12 remain immutable.


## Build 14 native find viewport and navigation restoration

The shared find engine and replacement operations are unchanged. On iOS, reveal
places the current match near the top of the actual editor viewport and repeats
that reveal when the virtualized list viewport changes size. The desktop find
positioning policy remains unchanged.

With the landscape software keyboard and native replacement navigator, the
original safe-area layout left only 14 points for document content. During
landscape find, the app fades the existing navigation bar out, disables its touch
and accessibility targets, and extends the same GPUI editor to the top of its
parent view. Portrait orientation or ending find restores the saved bar state and
safe-area constraint. UIKit retains the original bar and back item throughout.
Calling the navigation-controller bar-hide API instead reproduced an empty back
item after the full mode-switch flow, so that approach is not in this build.
Navigation titles now update only when their values change.

The existing iPhone simulator passes both focused flows: a short find/replace
rotation and return sequence, and a 45-paragraph note with three distant matches,
search/replace mode switching, all match positions, both orientations, closing
find and returning to the note list. Eight screenshots were checked against the
actual editor and native control frames. Each current-match highlight is a full
24-point line. Landscape search has a 134.33-point editor viewport; replacement
has 92 points, with 42 points between the current line and the nearest native
find control.

SQLite/XML readback verifies the exact 45 paragraphs and three markers in each
synthetic probe note, with no changes from find navigation. All 93 documents and
block payloads in the previous final snapshot are unchanged. Cloud-enabled Mac
and iOS Rust builds, the signed device bundle, formal product identity and the
179 staged GPUI Mobile files plus four patch digests are checked separately.

Build 14 is prepared locally. No new installation channel is started and older
packages remain immutable. Remote publication still awaits explicit Cloudflare
authorization. Physical iPhone interaction, live iPhone/Mac synchronization,
remaining rich-block controls and the service/iPad gates in the current table
remain open; this focused viewport check does not complete feature alignment.


## Build 15 shared insertion catalogue

The iPhone insertion catalogue now derives its contents directly from Mac's
`SLASH_ITEMS` and `GROUPS`. The five existing quick entries remain, while the
other unique kinds appear in native category lists. Labels use the shared
translations and Markdown filtering uses the existing `markdown_ok` predicate.
There is no independent mobile capability list or document mutation routine.
The selected item dispatches the existing `run_slash` operation, with the native
selection-generation stamp and current edit permission checked before dispatch.
Link dispatch also enforces the same title/code restrictions shown by its row.

This exposes the previously omitted text/headings/lists, code, quote, callout,
columns, equation, button, date reminder and countdown entries through the native
menu. It does not replace the existing specialized configuration panels; those
panels still need their own touch adaptation and acceptance.

The first direct-input check reproduced a missing keyboard after dismissing the
insert sheet: GPUI focus was already on the editor, but UIKit had resigned its
text input. For inserted editable document content, the adapter now requests
the keyboard through GPUI Mobile's public, deferred `show_keyboard` API. Native
link and file pickers retain their own presentation and input handling.

The existing iPhone simulator passes a focused catalogue/insertion flow: category
navigation, presence of the previously missing specialized entries, inserting a
callout, inserting two columns and entering the first column, inserting a code
block with software Return, native rename, and returning to the note list. The
final screenshots show native lists and the shared GPUI content renderer.
SQLite readback checks exact text, code newlines, parent block relationships, two
0.5-width columns and the untouched second-column paragraph. All 101 documents
and block payloads from build 14's final snapshot are unchanged.

Cloud-enabled Mac/iOS builds, formal icon/name, signing, registered-device profile
and the pinned GPUI Mobile source with four unchanged patches are checked. Build
15 is prepared locally; older packages and the existing remote channel remain
unchanged. Public installation still awaits Cloudflare authorization. Specialized
block configuration, physical interaction, live iPhone/Mac sync and the remaining
service/iPad gates remain open.


## Build 16 native equation and button forms

A reusable UIKit object form presents text/source fields and native option lists.
The existing GPUI editor still draws the document and equation/button previews.
`More > Content settings` lists equations, buttons, reminders and countdowns by
stable block/inline position, so existing content has a touch-sized edit entry.
Equation and button entries now open native forms; the date-related entries
still open their existing panels pending subsequent adaptation.

Equations retain Mac's live-update semantics. The original source-write sequence
is extracted into one editor helper used by both the desktop field and native
source changes. Done or swipe dismissal leaves the current source in place.
Button edits retain the existing draft, validation, confirm and cancel behavior;
native actions call the same field-change handler, action setter and submit
method. Shared action definitions and all 16 palette entries populate native
choice lists. The native form does not implement URL validation, normalization,
text limits or document mutation separately.

Callbacks use a logical form serial and an editor selection/generation stamp.
The adapter refreshes its stamp after its own accepted changes, rejects stale or
newly read-only contexts, and ignores callbacks from an older form. UIKit
presentation, updates and callbacks cross the GPUI lease boundary asynchronously.
Unchanged option rows retain their cells during typing; repeated row replacement
had interrupted a reproduced color-selection tap. Validation expands the sheet
so the shared error message is visible above its lower edge.

The existing iPhone simulator passes one focused complete flow: equation input
and reopen, shared equation rendering, button action/default-label changes, an
invalid URL with a visible error, bare-host normalization using Mac's existing
`http://` rule, a deep-green palette choice, reopening, cancelling a changed
draft, switching to DuplicatePage, shared Undo, native rename and returning to
the note list. The check does not execute the URL or duplicate-page action.
SQLite readback confirms `<latex>x^2+1</latex>` and the original button text,
OpenLink action, normalized URL and deep-green color after cancel/Undo. All 105
prior documents and block payloads match build 15's final snapshot.

Mac/iOS compilation, formal product identity, signing and the registered-device
profile are checked separately. There is no Core/model/schema/protocol change in
this build. The pinned GPUI Mobile source and its four patches remain unchanged.
Build 16 is prepared locally; the installation channel still awaits explicit
Cloudflare publication authorization. Reminder/countdown forms, code/media
controls, physical interaction and the previously listed live-service/iPad gates
remain open.


## Build 17 native reminder and countdown settings

The shared object form now supports system date wheels, switches and numeric
fields. Reminder settings use the existing live `toggle`, `on_day`, `at_time`,
`with_jet` and `apply_reminder` paths. The advance list and its label come from
the shared definitions, including a friendly label for a stored custom advance.
Countdown settings retain the original draft, mode switch, numeric normalization,
validation and confirmation methods. Native inputs read the existing GPUI field
length limit; numeric characters and length are enforced before UIKit accepts
the edit. Maximum duration values still normalize in the shared Rust controller.

Date wheels use the Gregorian calendar and the offset supplied by the existing
time module, explicitly shown beside the field. The verified run used GMT+08:00.
This does not establish device-zone changes or DST behavior. Native dates are
passed back as instants and transformed through the same local-day/time helpers.
The date-dependent form rows update in place; changing whole-day or notification
mode updates the existing reminder options rather than defining a mobile rule.

Countdown colors come from the existing seven-color palette. A color-only save
closes the draft and calls the original palette operation, preserving its running
clock. A time/notification edit confirms through the original countdown method,
then applies a selected color through the palette operation. The two existing
commands retain their separate undo behavior. Color-only clock preservation was
source-reviewed and compiled; it was not separately timed through the UI.

One existing iPhone simulator passes the complete date-form flow: whole-day mode,
changing the reminder year, toggling notifications, selecting the prior-day 09:00
advance, closing/reopening, countdown duration input, color selection, cancelling
a changed duration, rejecting a past end with a visible shared error, changing
the end with native wheels, confirming and reopening. A separate focused numeric
flow verifies two-digit entry and shared normalization from 99 to 23 hours.
No additional device matrix or unrelated regression suite is run.

SQLite readback confirms the reminder's 23:59:59 due time and prior-day 09:00
notification instant in the shared offset. The countdown preserves the original
1:02:03 duration fields after cancelling the nine-hour draft, saves blue and a
future end, and has a duration consistent with the shared calculation and existing
XML float32 formatting. The numeric probe saves 23 hours. All 111 prior documents
and block payloads match build 16's final snapshot.

Mac/iOS compilation, formal icon/name, signing/profile and the unchanged pinned
GPUI Mobile source with four patches are checked. There is no Core/model/schema/
protocol change. Build 17 is prepared locally; older packages and the live channel
remain unchanged while explicit Cloudflare publication authorization is pending.
System notification permission/delivery and background scheduling have not been
verified by these configuration checks. Code/media controls, physical interaction
and the remaining service/iPad gates remain open.


## Build 18 native code-block controls

Both desktop code-header styles now call the same code wrap, fold and copy
helpers. Caption changes share the original checkpoint/write/change-event path;
selecting a language calls the existing `set_lang`. The native form therefore
does not define another code-block persistence or history implementation. Code
language search uses the existing `filter_languages`, preserving prefix-before-
contains ordering and the original language names.

`More > Content settings` includes code blocks. Tapping a code-header language
uses the same form with its native language picker initially open. UIKit owns
presentation, native option navigation/search, touch-sized switches and fields.
The logical form identity, generation/selection and permission checks remain in
Rust. Search updates the option list without replacing the search controller.
The copy action dispatches the shared GPUI clipboard method; no system clipboard
write is performed by the focused test.

In reading mode `More > Code tools` lists only code blocks. Name, language and
wrap are disabled in UIKit and rejected by the Rust permission guard. Fold and
copy remain available as on the desktop. Folding preserves the original local
view-state behavior; it is not introduced into the document format or restart
persistence. Code forms avoid initially focusing the caption field.

One existing iPhone simulator passes `CodeFormUITests.testNativeCodeControls`
(64.625 seconds): insert and type a three-line code body, rename the code, search
for Rust through the native search field, select it, enable wrap, fold, close and
reopen, then verify reading restrictions and fold access. Three screenshots are
inspected. SQLite readback confirms the exact body/newlines, `lang="rust"`, the
caption, wrap and search-index marker. All 115 previous documents and 603 block
payloads match build 17's package snapshot. No additional device or unrelated
regression suite is used.

Mac compile and both iOS target builds pass. The signed build retains the formal
Fei Notes name/icon, registered-iPhone identity and Development CloudKit container.
No Core/model/schema/protocol file changes relative to build 17. The fixed GPUI
Mobile source and four patches remain unchanged. Build 18 is staged locally;
previous packages and the remote channel remain unchanged pending explicit
Cloudflare publication authorization. This does not establish physical feel,
clipboard output, notification delivery, live sync, full media controls or iPad
layout acceptance.


## Build 19 native image settings and crop gestures

`More > Content settings` now lists image blocks. On iOS a selected editable
image opens the same native settings instead of the desktop lightbox toolbar;
reading access continues through the system preview, also available from the
attachment list. Desktop image toolbars and resize handles are hidden on iOS.
The existing GPUI editor still renders document images and stored crops.

The form exposes the original caption writer, Core alignment operation and
proportional resize path through a native text field, option list and slider.
The slider retains one resize checkpoint and finishes through `end_resize`.
Cropping uses `start_crop`, `start_crop_drag`, `crop_drag_to`, `save_crop`,
`cancel_crop` and `reset_crop`. UIKit supplies a preview and 44-point gesture
handles; normalized touch positions map to the shared displayed-image geometry.
Minimum size, bounds, rectangle rounding, saved dimensions and undo therefore
remain governed by the original Rust implementation. The preview sends commands
and redraws the returned rectangle; it does not crop or rewrite the source file.
Permission and selection/generation guards also cover image commands.

The first targeted flow exposed integer overflow in the shared crop overlay:
random 64-bit block IDs were multiplied to derive handle/mask IDs in debug builds.
Those identities now combine the block and control name without arithmetic.
The subsequent flow corrected a test locator to use the actual accessible handle
rather than an unexposed parent container. The final targeted flow passes on the
same iPhone simulator (87.880 seconds): PNG import, description/alignment/width,
reopen, crop drag/cancel, landscape/portrait, crop/save, reset and shared Undo.
Four screenshots are inspected. No extra device or unrelated regression matrix
is run.

SQLite confirms a 227-point proportional width before cropping, the caption and
right alignment, and crop `0.0851,0.1036,1,1`. Saved width/height match the original
crop geometry within XML formatting precision. The image bytes retain the exact
fixture SHA-256. All 116 previous documents and 604 block payloads match build
18's package snapshot. Crop remains non-destructive and cancel leaves no crop.

Mac and iOS builds, signature/profile, formal icon/name and Development CloudKit
entitlements are checked. No Core/model/schema/protocol change relative to build
18. The GPUI Mobile pin and four patches are unchanged. Build 19 is local; previous
packages and the live channel remain unchanged pending explicit Cloudflare
publication authorization. Photos access, large/EXIF/animated images, preview
gallery details, VoiceOver crop controls, physical interaction, real iPhone/Mac
sync and the remaining service/iPad gates are not established by the PNG flow.


## Build 20 GPUI Mobile Photo Library entry

The native insertion catalogue adds Photo Library alongside the existing Files
image entry. Fei Notes enables GPUI Mobile's existing `image_picker` package and
uses the tracked `gpui-mobile-ios-gallery.patch` to expose a non-blocking gallery
receiver. The legacy synchronous APIs remain unchanged. The new PHPicker path
schedules UIKit work on the main queue, retains picker/delegate ownership and
rejects an overlapping request. A logical request serial prevents a late file
callback from completing a later picker. Cancellation during loading, stale
callback cleanup and receiver-drop cleanup are source-reviewed and compiled;
those timing races are not separately forced through the UI.

The picker accesses only the user's selected item. It copies the selected
provider representation before the provider URL expires, preserves supported
PNG/GIF/JPEG/WebP/BMP representations and requests compatible JPEG for an
otherwise unsupported photo format. The verified HEIC sample is exported by the
system as JPEG. This does not add a HEIC decoder to the shared document engine.
The app then calls the same background image validator, asset writer and insert
operation as the Files entry, with selection/generation/permission checks and
the existing library-session lease. Its owned temporary copy is removed after
processing. There is no second mobile image-import or document implementation.

The same iPhone simulator passes native picker presentation/cancellation and
selection of two synthetic photos, preview, rename and return. A follow-up
selection flow passes with the final cleanup code (55.413 seconds). Five relevant
screenshots are inspected: the system private-access picker plus PNG/HEIC
previews and shared editor rendering. No real photos, extra device or unrelated
regression matrix are used.

SQLite and image readback confirm one image in each selected-photo note and no
image in the cancelled note. PNG remains exactly 4,107 bytes with the source
SHA-256. HEIC imports as a valid 33,818-byte JPEG; both images are 640 by 360.
Source fixtures are unchanged and the owned picker temporary directory is empty.
All 119 previous documents and 610 block payloads match build 19's snapshot.

Mac/iOS builds, formal branding, signature/profile and Development CloudKit
entitlements are checked. The fixed GPUI Mobile revision now reconstructs to
180 files with five recorded patches. No Core/model/schema/protocol files change
relative to build 19. Build 20 is staged locally; older packages and the remote
installation channel remain unchanged pending explicit Cloudflare publication
authorization. iCloud-only Photos, large/EXIF/animated samples, hardware feel and
real iPhone/Mac sync remain acceptance gates.


## Build 21 native date-reminder delivery

The audit found that the shared reminder poll still called macOS `osascript` on
iOS. The workspace now has one shared `visit_reminders` collector: it reads the
open document live, including unsaved edits, and the remaining entries from the
same cached reminder metadata as before. Mac retains its original 15-second
active-app filter and notification path. iOS uses the same enabled flag, absolute
notify instant, document title and localized reminder body with User Notifications.
No Core/model/schema/protocol changes are introduced.

The iOS bridge is `Reminders.swift`. The pinned GPUI Mobile notification package
offers immediate delivery, without the scheduling/status APIs needed here, so
this platform bridge uses Apple's system API directly. See Apple's
[scheduling guide](https://developer.apple.com/documentation/usernotifications/scheduling-a-notification-locally-from-your-app).
The earliest 60 future reminders are scheduled. Absolute instants become UTC
Gregorian calendar triggers, rounded upward to the system's second granularity;
the stored millisecond values stay unchanged. Existing request identifiers are
updated in place, obsolete owned requests are cancelled, and another library's
pending reminders are removed when the active library changes. Library namespaces
exclude the app-container prefix, preserving identity across reinstall/update.
The native queue is left intact until the initial library plan is available.

Plans refresh at workspace construction, flush, deletion, explicit status-page
entry and the existing 15-second active timer. A switching workspace does not
publish a stale plan. Native reconciliation is serialized/coalesced, with a UIKit background task
covering it as the app leaves the foreground. The task ends on completion or the
system expiration callback. No runtime timer is required for a
notification already handed to the system. Foreground presentation is wired;
notification taps queue until the editor starts and open only the matching active
library/document. No automatic account or library switch is introduced.

General settings and the reminder form expose a native notification status page.
Permission is requested only by its explicit Allow action; denied permission
links to the app's system notification settings. The page queries actual pending
requests and the current contents of Notification Center. It is not a notification
history: responding to a notification removes it from the system center. The
first delivery flow reached the banner and cold-open successfully, then failed
an incorrect test assumption that the consumed notification would remain listed.
The final test checks actual delivery, the exact document route and consumed
pending request instead. Countdown behavior remains the existing in-app bubble,
as on Mac; it is not silently changed into a background system notification.

One existing iPhone simulator passes native permission (22.861 seconds), turning
off a future reminder and removing its system request (40.177 seconds), and the
final real-time delivery/cold-open flow (105.732 seconds). The latter imports a
synthetic note through Files, observes the actual system pending request,
terminates Fei Notes, receives a SpringBoard banner, taps it and opens the exact
note after process restart. Five relevant screenshots are inspected, including
the formal Fei Notes icon in the delivered banner. No extra device or unrelated
regression suite is run.

SQLite confirms the exact notify/expire instants and the disabled reminder's
`should-notify="false"`. All 125 prior documents and 620 block payloads match
build 20's snapshot. Mac and iOS builds, formal branding, signature/profile and
Development CloudKit entitlements are checked. The GPUI Mobile pin and five
patches remain unchanged. Build 21 is local; prior packages and the remote
channel remain unchanged pending explicit Cloudflare publication authorization.
Denied/revoked permission, background-task expiration, clock/timezone changes,
queue capacity, foreground presentation and other-library notification responses are source-reviewed or
wired but not separately forced in the UI. Physical delivery and real iPhone/Mac
sync remain acceptance gates.


## Build 22 web images on iOS

The audit found that the shared remote-image worker invoked external `curl`,
which cannot supply downloads inside the iOS app. Only the transport is adapted:
a small Swift URLSession bridge returns the temporary download to the existing
Rust worker. Mac keeps its current curl path. Shared source URLs, automatic-download
preference, retry cooldown, format/dimension validation, atomic cache commit and
failed-partial cleanup are retained. The ephemeral session has a 60-second limit,
no persistent cookie/credential/cache storage and normal server trust evaluation.
No GPUI Mobile or Core/model/schema/protocol change is added relative to build 21.

The app permits user-supplied HTTP image URLs through NSAllowsArbitraryLoads,
matching the existing Mac capability. This is an app-wide ATS exception; HTTPS
still uses default certificate trust, with no trust override in the bridge. This
exception requires justification for a future App Store submission. See Apple's
[ATS key documentation](https://developer.apple.com/documentation/BundleResources/Information-Property-List/NSAppTransportSecurity/NSAllowsArbitraryLoads).
The pinned GPUI HTTP package supplies a trait, without a configured concrete
client in this app, so the platform transport avoids a second network dependency.

The native image form exposes current download status and a touch-sized load/retry
action. Background completion refreshes only a still-valid matching image form.
Retry invalidates only the derived remote cache file, allowing a corrupt cached
copy to be fetched again; local originals are untouched. The attachment list now
resolves downloaded images through the same shared cache. Screenshot review found
that extensionless cache filenames made Quick Look show generic file information.
The final platform preview makes a type-suffixed temporary image copy, preserving
the original cache, and removes that copy when the preview is released. The final
screenshot shows the actual synthetic image rather than a generic file panel.

The same existing iPhone simulator passes redirected HTTP download, server-failure
retry and invalid-image rejection (80.627 seconds), public HTTPS image download
and shared preference presentation (22.901 seconds), and automatic-download-off,
explicit single-image loading plus corrected preview (38.540 seconds). The local
fixture server binds only to loopback and serves synthetic images. Its request log
confirms that manual mode makes no request until the explicit load action. Actual
status, preference and image-preview screenshots are inspected; no extra device or
unrelated regression suite is used.

SQLite preserves all five source URLs. Three successful synthetic cache entries
match the PNG source byte-for-byte and one matches the public HTTPS source; the
server failure/invalid response leaves no partial or invalid final cache. All
129 previous documents and 624 block payloads match build 21's snapshot.
The preview temporary directory is empty after closing. Mac and iOS compilation,
formal icon/name, local signature/profile and Development CloudKit entitlements
are checked. The same GPUI pin, 180 files and five patches are unchanged. A benign
notification bridge capture warning is removed with an explicit singleton capture.

Build 22 is prepared locally. Previous packages and the remote installation channel
are unchanged while explicit public Cloudflare publication authorization is pending.
Invalid-certificate rejection, corrupt-cache refetch, large/animated images and
physical touch/real iPhone-Mac sync are not claimed as runtime-verified here.


## Build 23 native batch note and folder operations

The audit found that the iPhone library exposed single-item commands but omitted
the Mac batch operations. The library menu now opens a native selection sheet.
It receives exactly the entries shown by the shared mobile list collector: the
current folder, Favorites or existing search results, with existing ordering.
The native sheet shows selection checkmarks and a count, with duplicate, move and
delete fixed in the system bottom toolbar. Empty selections disable those actions.
Select All uses the first BATCH_LIMIT entries as on Mac; extra unchecked rows are
disabled at the same shared limit of 50. Dynamic Type and native sheet/navigation
behavior come from the existing UIKit controls adapter.

The adapter calls the existing toggle_check, batch_copy, ask_batch_delete and
move_docs paths. Folder copy, document copy, parent changes, subtree deletion,
file history and the shared confirmation remain product operations. No second
mobile implementation of those rules, Core operation, schema or sync protocol
is introduced. Selection sessions bind callbacks to a unique serial, active
library root, view, folder, tab and query, reject switching/shared sessions and
prune candidates that have disappeared. The native folder picker is shared with
single-item move, and excludes Trash plus all selected subtrees. Move submission
validates that the destination is still a live folder before calling move_docs.
Cancel closes the selection without executing a batch operation; the optional
close callback also covers an interactive sheet dismissal.

The existing iPhone simulator passes select-all/deselect/cancel, empty-toolbar
restrictions, two-note duplicate and move, folder-subtree duplicate and deletion
with cancel/confirm (85.091 seconds). The first run reached delete confirmation
but failed because XCTest matched both a system button and its nested button;
the final focused flow uses firstMatch with fresh synthetic fixtures and passes.
Actual selection/toolbar, destination-picker and confirmation screenshots are
inspected. No additional device or unrelated regression suite is used.

SQLite verifies the two root note copies, both originals moved into the target
folder, two preserved children inside the folder copy, and the copied folder plus
both children in Trash only after confirmation. All bodies survive. All 134 prior
documents and 629 block payloads match build 22's snapshot; prior entry parent
links are also unchanged. Mac/iOS compilation, formal product icon/name, local
signature/profile and Development CloudKit entitlements are checked. The GPUI
pin, 180 source/license files and five patches are unchanged. No Core/model/schema/
protocol files change relative to build 22.

Mac advanced to a new product head during packaging. The package baseline audit
records that application delta as pending; this build is not represented as the
latest Mac application source.

Build 23 is local, with previous packages and the remote channel unchanged while
public Cloudflare publication authorization remains pending. Selection capacity,
stale callbacks and swipe-dismiss are source-reviewed/compiled rather than
separately forced in runtime. Physical feel, live batch synchronization and the
full iPhone/Mac acceptance remain open gates.


## Build 24 current Mac product integration

Mac advanced to cc023e40c8b7fbcc2a6462b41f82f6d79a8c34cf while build 23 was being
packaged. All 41 changed paths are reconciled with the existing mobile source.
The original dirty files are backed up before integration. Three conflicts are
resolved: new-note creation uses the upstream shared create_doc helper; the native
image adapter and upstream clipboard tests are both retained; sidebar renaming
uses the upstream focus-ring implementation. Thirty-six resolved files match the
integration result exactly. Five have documented platform adjustments: folder
focus/anchor APIs, extra GPUI mobile image-format cases, horizontal scroll-offset
API shape, a touch-sized table scrollbar footer and the iOS default-type label.
The upstream Core editor/Markdown delta is exact, and the existing mobile store
session adapter is preserved. No commit, push or main-worktree mutation is made.

The iOS Cargo lock updates only the local fdl_core package to 0.1.10. The pinned
GPUI Mobile source, 180 files and five patches are unchanged. The build retains
the formal Fei Notes 0.1.9 product version and increments its package build to 24.
The source provenance records the original mobile Git HEAD, current integrated
Mac head and all uncommitted source hashes separately.

The library menu exposes explicit Document and Markdown creation, both calling
the shared helper. Compose uses the same remembered/default type as Mac. General
settings retain the shared preference, with an iOS label explaining Compose and
hardware-keyboard behavior. Newly created folders now open before naming, as on
Mac, and subsequent notes use that folder as their parent. Native formatting in
a Markdown table now reaches the shared column-alignment operation. The new
horizontal table scrollbar uses the GPUI API adapter and a 44-point iOS footer;
the Mac footer keeps its upstream dimensions.

One focused flow on the existing iPhone simulator passes explicit Markdown and
Document creation, remembered Markdown through Compose, folder opening and child
placement, and Markdown column alignment. The initial test looked for an action
inside a still-closed paragraph submenu; the final test enters that submenu and
passes. Screenshots of the native alignment form, shared wide-table rendering
and shared default-type preference are inspected. The targeted upstream Markdown
export/column-alignment roundtrip test passes. No broad regression suite or extra
device is used.

SQLite verifies Markdown flags on explicit and remembered-type notes, ordinary
format on the document and folder child, the child's exact parent, and centered
alignment across all three rows of the selected table column. Other columns stay
left-aligned. All 146 previous documents and 641 block payloads match build 23;
prior parent links and Trash records are unchanged. Mac/iOS compilation and the
local product icon, signature/profile and Development CloudKit configuration are
checked. Build 24 is local, with old packages and the remote channel preserved.
Physical touch, scrollbar dragging, multi-image clipboard behavior, complete
preference coverage, live services and real iPhone/Mac sync remain unverified.
Public Cloudflare publication still awaits the existing authorization request.


## Build 25 native iCloud version history

The delivery audit found a platform omission: the Mac version-history and
restore-copy functions were gated to macOS, with no iPhone entry. The shared
CloudVersions confirmation, listing and restore_cloud_version operation are now
available on iOS. The native More menu exposes history only for an active personal
cloud runtime. Local and shared-note sessions do not expose this entry. The iOS
entry flushes pending edits before reading the existing journal history.

A UIKit version list presents the shared timestamps and snippets, with a checkmark
selection, Cancel and Restore a Copy. Restore is disabled until a version is
selected, and the selection resets on a fresh presentation. The action uses the
same confirmation serial binding as the other native confirmations: active library,
current note and confirmation contents are part of its identity. Selection is
validated against the listed sequences; switching/unavailable cloud runtimes
cannot submit a restore. The native view updates enabled state as runtime availability
changes and closes with its owning confirmation. Restore retains the existing
product behavior of creating a new note and opening it. No Core/model/schema/
protocol change or separate recovery implementation is introduced.

One existing iPhone simulator passes the focused cached-history flow (35.049
seconds): list two versions, verify disabled restore, select/cancel, reopen with
no selection, select the older version, restore/open a copy, switch back to the
local library and verify its menu hides history. Three relevant screenshots are
inspected. The fixture uses a fresh isolated channel and a synthetic cached
account with transport paused. This is local journal/UI evidence, not live
CloudKit history acceptance. No extra device or broad regression suite is used.

The first simulator launch was blocked after adding CloudKit keys to its actual
ad-hoc host signature. Restoring Xcode's empty actual signature and retaining its
simulated-entitlements section fixed launch. The next run completed recovery but
failed cleanup because the adaptive popover has no Cancel row; the final test
dismisses it by tapping outside and passes. The signed device package uses the
normal Apple Development profile and CloudKit entitlements.

SQLite confirms a distinct recovered note containing the older marker only.
The original cloud document/blocks, original pending update and both history rows
are unchanged. Neither cloud note appears in the local library. Switching local
releases the cloud journal lock and retains the transport-pause marker. All 151
prior local documents and 650 block payloads match build 24's snapshot. Mac/iOS
compilation, formal icon/name, local signature/profile and Development CloudKit
container are checked. The Mac product baseline and GPUI revision, 180 files and
five patches remain unchanged.

Build 25 is local; old packages and the remote channel are preserved pending the
existing public Cloudflare authorization request. Interactive dismissal, busy/
stale callbacks, missing/trimmed versions and runtime changes are guarded or
source-reviewed rather than separately forced in the UI. Live version exchange,
recovered-copy upload and physical iPhone/Mac acceptance remain open gates.


## Build 26 native manual ordering

The alignment audit found that iPhone could select Manual Order but had no touch
entry for rearranging the list. Sort now offers Reorder for a full personal-library
level: My Notes or the open folder, with an empty search query. Filtered search,
Favorites, shared-note sessions and switching libraries do not expose reordering.
Entering it uses the existing manual-order initialization and shared saved sort
preference. The current level is shown in a native editing table with system drag
handles, Dynamic Type and the standard animated row movement.

The adapter does not write ordering metadata itself. The existing desktop drag
and move-into-folder path now share queue_tree_move with the native row command.
That helper retains the same optimistic tree update, file-history snapshot,
pending move queue, background tree_writer.move_in_tree transaction, subpage sync,
confirmation and failure rollback. No Core/model/schema/sync operation is added.
Cross-folder relocation continues through the existing Move picker; the native
row gesture reorders siblings at one level.

A reorder session binds callbacks to its serial, library root, view and parent.
Each move validates live source/destination membership. A response counter lets
the native list reconcile accepted or rejected gestures. Row identities remain
stable during a possible gesture, with backend reconciliation after a response.
While a write/response is pending, dragging and Done are disabled and interactive
dismissal is blocked. Context replacement dismisses the page; its close callback
can clear only the matching session. A writer failure keeps shared rollback and
also supplies the native prompt with its diagnostic.

The existing iPhone simulator passes a single focused UI flow (75.554 seconds):
create one synthetic folder with three notes, drag the last row to the first
position and another row to the end using the actual system handles, wait for
writes, close, terminate/relaunch and reopen the list. The persisted list is the
expected reverse of the original. Native handle, result and relaunch screenshots
are inspected. Mac/iOS builds pass. No additional device or unrelated regression
suite is used.

SQLite confirms sibling order 1/2/3 for the expected identities and preserves each
new note's parent and body. All 153 prior documents and 652 block payloads match
build 25; prior parent links and Trash records are unchanged. Relative ordering in
every prior sibling group, including 149 prior root entries, is unchanged after
manual-order initialization. Formal branding, signature/profile and Development
CloudKit entitlements are checked. The Mac baseline and GPUI pin, 180 files and
five patches are unchanged. Build 26 is local; earlier packages and the remote
channel are preserved pending the existing public Cloudflare authorization.

The folder case is runtime-verified. Root dragging, write failure, stale/concurrent
callbacks and interactive dismissal are guarded/source-reviewed but not separately
forced in the UI. Physical gesture feel and live iPhone/Mac order convergence
remain acceptance gates. The audit also notes that the Mac Copy to Local Library
entry is gated by trial/subscription availability; the current Development mobile
configuration must preserve that product gate rather than expose it unconditionally.


## Build 27 native document information

The iPhone note menu now exposes Document Information for personal and shared
notes, including reading mode. It calls the existing open_doc_info operation:
pending edits are flushed and counts are calculated from the current Core
Document, using the shared word_count, char_count, metadata timestamps and
Store::size_of. A UIKit alert presents the result with a standard Done action.
The fixed 840-point desktop dialog is suppressed on iOS. Desktop presentation
and the Core/model/schema/sync implementation are unchanged.

The information session binds to a unique serial, library root and note. It is
hidden during a library switch or confirmation and when the current note/view
changes. Native presentation and close callbacks validate the current generation
or serial. Closing a stale presentation cannot clear a newer information session.

One existing iPhone simulator passes the focused flow in 47.587 seconds: import
synthetic Chinese/Latin text and a table, open information (12 words, 43
characters), rename with an additional Chinese character, reopen (13 words, 44
characters), enable reading mode and reopen without a keyboard. Native screenshots
are inspected. The first assertion expected literal newlines in accessibility
text; UIKit normalizes these to spaces, so the test was corrected without
changing the application statistics. No broad regression or extra device is used.

SQLite confirms the renamed title, reading preference, bold text and table. All
156 prior documents and 655 block payloads are unchanged. Parent links and Trash
are preserved; one previously open note receives its normal startup opened-time
update. Mac, simulator and device-target builds pass; the formal icon/name,
signature/profile and Development CloudKit entitlement are checked. The Mac
product baseline and GPUI pin, 180 files and five patches are unchanged.

Build 27 is prepared locally. Earlier packages and the remote channel remain
unchanged pending the existing explicit public Cloudflare authorization request.
Physical interaction, shared-note information access and live iPhone/Mac sync
remain acceptance gates; this local flow does not establish live-service parity.


## Device acceptance and build 28 product CloudKit correction, 2026-11-15

The registered iPhone was updated over Wi-Fi from build 3 to build 27. Device app
metadata verified the display name and build; process launch succeeded. The user
then rejected the interaction quality: navigation transitions and redundant
button text, popup placement, keyboard presentation and the home list all need
work. The user also reported that Mac-synced notes did not appear. These are open
acceptance failures, not completed items covered by prior simulator checks.

The installed Mac app is app.feinotes.mac 1.0.0 (1006), signed for Production and
container iCloud.app.feinotes.mac. Build 27 instead used Development container
iCloud.app.quietmd.cloudlab and a per-install QA channel. Consequently the two
apps addressed different data. This configuration mismatch is confirmed; it is
not evidence of a generic device network failure.

Build 28 adds a Product build configuration using the existing universal product
App ID app.feinotes.mac, a device-specific iOS development profile whose iCloud
entitlement allows Production, and the Mac's existing Production container. No
new certificate, subscription bypass or CloudKit schema change is introduced.
The regular CloudRestore bridge is called without QA input or channel, and the
sync journal starts with channel None. The production library has a separate
account-scoped directory; no existing QA journal is relabeled or reused. The
normal trial/subscription checks remain in place. Core/schema/protocol are
unchanged. Native and Rust cloud configuration must match before configuration
can succeed.

Mac check and the device build pass. Codesign/profile inspection confirms the
product identifier, registered device, formal icon/name and Production container.
The product app was absent before installation; build 28 was installed and
launched over Wi-Fi without removing or updating the separate old QA app. Both
packages and old data remain available. Actual note visibility is awaiting user
feedback and two-way synchronization is not yet established. Product startup
appears to have fallen back locally; the next diagnostic must identify its
actual account/access/transport error rather than call the corrected signature
proof of successful synchronization.

Implementation and evidence: /private/tmp/fei-ios-product-sync28.9gjtjdve.
The pending Mac integration is staged at /private/tmp/fei-ios-product28.9n9028vj;
it has not been applied. Next work is the startup/access error and then the
reported navigation, popup, keyboard and native-list interaction problems. The
public remote channel remains unchanged while the existing explicit publication
authorization request is unresolved.


## Build 29 startup diagnosis and build 30 interaction correction, 2026-11-15

Build 29 exposes a cloud-startup failure in a native alert rather than silently
falling back to an empty local library. Its dedicated startup-error file contains
only the error, outside note storage. The installed phone returned CKErrorDomain
code 3 (network unavailable) from the corrected Production path. App networking
permission is an unverified possibility, not a confirmed cause. A user question
about the new app's wireless-data permission is pending. Mac note visibility and
bidirectional sync remain unverified. The old cloudlab app is retained.

Build 30 replaces the compact home/folder list's presentation with UITableView,
a native search bar, segmented category control and context menu. Visible rows,
order, filtering, snippets, favorites and all item actions still come from shared
Rust operations. Native diffable rows retain stable document identities. There
is no native store or independent search/sorting algorithm. List commands bind
to the active library/view/parent scope and validate source membership. Shared
waiting pages retain their existing GPUI presentation. Shared product notices
are mirrored into a native label so hidden GPUI list content cannot hide errors.

Each route retains its own native list while the shared GPUI view moves to the
editor. Outgoing content is frozen before native navigation commands, with a
noninteractive snapshot and timeout recovery for commands that do not navigate.
Back buttons use minimal icon display; settings no longer has a redundant Done
alongside Back. Context actions use the system row anchor; fallback action-sheet
anchors use the relevant row or view center instead of the top edge. Delayed
presentation validates the owning route and generation. New-note keyboard
requests occur after navigation completion; gap insertion and switching back to
editing also request the existing GPUI Mobile keyboard path.

The first focused simulator flow passed search, navigation, creation, rename and
context-menu actions, but screenshot review showed that its keyboard-existence
assertion was too weak. The stronger assertion correctly failed with the keyboard
at y=891 below the 874-point app, and also failed for native UISearchBar. Xcode 27
uses Device Hub instead of Simulator.app. Changing its selected simulator's
hardware-keyboard emulation made the native and GPUI software keyboards visible
and hittable at y=590, height 226. The strengthened complete flow then passed.
This is simulator evidence and does not establish the user's physical keyboard
experience. No additional test device or broad regression suite is used.

Build-30 evidence is in /private/tmp/fei-ios-interaction30.n_6j51xt. The final
focused interaction flow passes, including scoped context-menu callbacks. Mac
and device builds pass, and the signed product build is installed over Wi-Fi
on the registered iPhone. Its initial launch failed with the explicit Locked reason, then succeeded after
the user unlocked the phone. Physical interaction acceptance remains open.
The bottom GPUI formatting bar still has text controls and is a remaining touch
presentation issue. The earlier CKError 3 is historical: the user subsequently allowed wireless
data, saw Mac notes and accepted sync as recorded below.


## User sync acceptance, 2026-11-15

The user confirmed that Mac notes appeared on iPhone, allowed the new app to use
wireless data, unlocked the phone, and then explicitly reported: "iCloud sync has
been verified and works." Sync is therefore recorded as user-accepted. No extra
synthetic sync round trip is requested. This is user acceptance evidence; do not
claim independent note hashes, journal/outbox measurements or a new automated
bidirectional test. The subsequent build-30 launch succeeded and Device Hub
visually showed its native home list and settings on the registered physical
iPhone. The earlier Locked and CKError 3 entries remain historical evidence.

Remaining active work is interaction quality, including the old text-based GPUI
formatting strip. Replace that presentation with standard UIKit toolbar buttons
that dispatch the existing Rust editor commands. Core, document operations and
the accepted production synchronization configuration should remain unchanged.


## Build 31 native editing toolbar, 2026-11-15

The compact editor now uses a UIKit toolbar above the keyboard. Image-only Undo,
list, styles, insertion and keyboard buttons replace the GPUI text strip. Redo
is available from Undo's native menu. Reading mode exposes a pencil action;
read-only shared access remains non-editable. Find temporarily hides the strip.
Commands validate the live document and library scope and call the existing
Rust operations. Core, storage schema and sync protocol are unchanged from 30.

A focused simulator test exposed a keyboard state race after rapid body input:
the layout guide briefly reports no keyboard while the editor retains native
input focus. The button now uses UIKit keyboard notifications and the editor's
actual first responder state, so hiding does not incorrectly request another
show. The original fast-input failure sequence passes without adding an input
wait or a second tap. Native format/insertion sheets, reading/editing, Find,
home return and saved body preview pass in the same single-device test.

The final toolbar reserves 52 points so system button shapes are not clipped.
Exported screenshots were inspected, including reading after its repaint and
editing above the visible software keyboard. An early reading screenshot was
captured during repaint; the settled screenshot contains the title and body.
Evidence: /private/tmp/fei-ios-toolbar31.yjlxdoy_/toolbar-final.xcresult.
Physical UI acceptance remains open. The user's accepted iCloud result is
retained; this change does not request or claim a new sync test.

Build 31 was installed and launched successfully over Wi-Fi on the existing
registered iPhone. Both devicectl JSON outcomes are success. The previous QA app
and its data were preserved. No public channel or remote tunnel was changed.


## Build 32 product alignment through 219ad6d, 2026-11-15

The previously staged product delta is now integrated into mobile/iphone-layout.
Before snapshots and an application manifest are retained under
/private/tmp/fei-ios-align32.mlr25lwc. No commit, push or Mac worktree edit is made.

Shared behavior now includes inline Markdown image absorption, wrapped-row caret
affinity, horizontal reveal in unwrapped code, persisted document reading
positions, viewport-preserving property edits and the product interface-size
preference. The touch position adapter preserves its existing Pos API while the
shared pointer implementation retains wrap-row affinity. Table property and code
wrap commands still use the existing shared command helpers, now with the same
viewport intent as Mac. IME cleanup and current access guards are retained.

Interface size uses the same stored Rust preference in the settings page and in
the native home/folder list and search field. UIKit content-size-category support
is retained. iOS omits the desktop-only window-material setting. Native routes,
icon, product name, accepted Production CloudKit configuration and existing
session identity are unchanged. This is an incremental product alignment, not a
claim that every current Mac feature has reached iPhone.

The three inline-image tests and the all-blocks fixture round trip pass. Mac,
simulator and device Rust compilation pass. Only the original iPhone simulator
is used for focused appearance/preference and editor-toolbar flows. No new live
CloudKit test or additional physical-device matrix is requested. Physical UI
acceptance remains open; the phone's last delivered build is still 31.

The remaining committed product delta is recorded in pending-product.json:
17 Core paths, 49 application paths, 16 platform-backend paths, 13 assets and 71
documentation/tool/build paths. The current Mac worktree also contains separate
uncommitted work that is not imported. Later storage/history integration and
applicable mobile entry points remain required by the full objective.

The final focused UI run (alignment-fontbridge.xcresult) passed both flows.
The simulator preference is large after termination/relaunch, the synthetic
body is saved, and exported screenshots show the selected size, native home
rows and search field without the desktop window-material row. Build 32 is
signed and retained as a private candidate; build 31 remains on the phone.


## Development build 33: source through 3802182, 2026-11-15

The 166-path committed delta over 219ad6d is now applied, with before snapshots
under /private/tmp/fei-ios-align33.3fddqobf. The Mac worktree is read only. Three
upstream deletions affect only backed-up repository fixtures/site source. No
commit, push, physical install, certificate change or public upload is made.

The session Registry and leases remain authoritative for the selected SQLite
root; new FORMAT 4 file-location selection rejects an account-selected root.
File onboarding is disabled for the product-selected library on iOS, matching
the product storage boundary. The Store schema remains 7. Local-history restore
on iOS calls the same atomic cloud-save path and shared access checks as Mac.
New asynchronous file tasks disable native commands and cover the GPUI surface
until completion, including the shared file-history busy state.

The image path uses the product thumbnail/cache implementation. A reproducible
212-line, four-file extension to pinned gpui-pre 0.3.10 supplies scale factors,
static-image buffer release, missing-atlas reporting and visible-only loading.
The archive and every prepared source file are checked by the manifest, and a
fresh reconstruction was verified. Darwin anonymous mapping flags are corrected
for iOS in the imported large-allocation allocator. No no-op API substitutes or
dependency-version upgrades are used.

Evidence so far: Mac, simulator and device Rust compilation pass; an independent
mobile-GPUI contract probe confirms static-buffer release without losing image
identity/dimensions and preserves animated frames. Six shared image/clipboard
byte-conversion tests, the all-blocks fixture, SQLite history/gesture transactions
and cloud history restore rollback/read-only/revoked checks pass. All test data
is synthetic; no actual CloudKit sync check is repeated.

A 4096x2048 synthetic PNG is visible with the expected four quadrants and 2:1
aspect after import, background return and note reopen. The saved image bytes
match the source SHA-256. The first Files test selected the older app's folder;
the corrected test handles the two controlled app folders and passes. This is
actual image-display evidence, not a measured memory-footprint result or a
forced GPU-atlas-loss check. Those distinctions remain explicit.

Build 33 is not a signed delivery candidate. Local-history list/preview/restore,
copy/naming/deletion/filter, retention controls and applicable new folder-import
and language entries still need mobile implementation/review. The device stays
on build 31 and the last signed private IPA remains build 32. User acceptance of
Production sync is retained; UI acceptance and remote delivery remain open.

The final image/delete flow also passes with the asynchronous-task interaction
guard enabled (image-delete-final.xcresult): the synthetic note moves to Trash,
its native list row disappears, and the original PNG bytes remain unchanged.
The iOS alert exposes nested accessibility buttons; the test uses the same
first-match selector as existing confirmation checks. Mac, simulator and device
Rust final builds pass. No full physical memory/atlas-loss claim is made.


## Build 34: native local version history, 2026-11-15

The shared baseline stays frozen at 3802182. This build adds presentation and
command adapters around the existing history implementation. The iPhone uses a
native list, filter menu, naming alert and destructive confirmation. The shared
GPUI editor renders the read-only preview. Native back first returns to the
history list, then returns to the exact current document; the close button can
leave history directly. Preview controls expose change marks, an independent
copy and restore, without editing or keyboard controls.

Each native callback carries an ephemeral history token plus library/editor
scope and note identity. Rust checks the current history session and version
membership before dispatching the existing command. Restore retains shared
permission checks and the atomic document/CRDT/outbox save. Native naming and
the desktop text field now share one label-writing method. No Core structural
operation, SQLite migration or CloudKit protocol is introduced by build 34.

The library settings page reuses the shared retention controller (1–30). iOS
settings text buttons have a minimum 44-point width as well as the existing
44-point height; desktop metrics are unchanged.

Focused evidence is under /private/tmp/fei-ios-history34.bq8o59gx. Mac check,
iOS simulator/device Rust builds and the signed-device Xcode build pass. One
90.947-second simulator flow covers naming two versions, filtering, preview,
returning to current content, restart, cancel/confirm restore, independent copy
and cancel/confirm history deletion. Read-only SQLite checks confirm the original
has the first version and the distinct copy retains the second version. A
separate 18.004-second settings flow changes retention from 20 to 21 and reopens
the application; the screenshot and SQLite value both show 21. Tests use the
same simulator and synthetic local data. Core/image/atomic-restore evidence from
build 33 remains applicable because those implementations are unchanged.

Production synchronization remains user accepted on build 30 and is not retested.
Physical acceptance of build 34 and remote installation remain separate gates.
Applicable new legacy-folder import and language entries still need review;
this candidate does not establish complete Mac feature or service acceptance.

Build 34 is signed and retained privately as FeiNotes.ipa (50,175,607 bytes).
Signature, registered-device profile, unchanged formal icon, Production container
and absence of bundled SQLite data were verified. The IPA was installed over
Wi-Fi; device metadata confirms version 0.1.9, build 34, app.feinotes.mac. Launch
was explicitly denied because the device is locked. This is not a launch or
physical interaction pass. Public remote distribution is awaiting the explicit
question presented with this concrete candidate; no new tunnel or upload occurs.


## Build 35: working shared menus and native legacy-library import, 2026-11-15

The source audit found that iOS displayed the new shared language/default-type
buttons without mounting render_lang_menu or render_pref_menu. The mobile root
now mounts both existing overlays. Dropdown triggers and options use at least
44-point touch heights on iOS, while desktop dimensions stay unchanged. The
language list is bounded and scrollable. No duplicate preference model or
language-selection implementation is added.

Library settings now exposes the same old-library import action as Mac. iOS
presents the native folder picker as a sheet. Cancelling or interactively
closing it releases the pending operation; choosing a folder keeps the security
scope and NSFileCoordinator read alive while making a temporary sandbox copy.
This follows [Apple's directory access requirements](https://developer.apple.com/documentation/uikit/providing-access-to-directories).
Rust receives only that owned snapshot and uses Store::import_legacy_copy,
including its existing validation and SQLite transaction. A session lease and
busy guard protect the captured destination; completion refreshes only that
same selected library. Temporary native and Core staging directories are
removed on success/error, including a callback whose requesting future closes.
Source notes are copied rather than opened for migration. Core, schema and
CloudKit protocol/configuration are unchanged from build 34.

Evidence is under /private/tmp/fei-ios-import35.magg9ix6. Mac check, simulator and
device Rust compilation, and the signed-device Xcode build pass. A 20.330-second
simulator flow selects English, restarts, confirms General/Settings remain
English and returns to Chinese. A 27.397-second flow opens and dismisses the
native folder picker, reopens it, imports a synthetic FORMAT 4 library and opens
its nested note. Read-only SQLite/asset/source hashes verify all four IDs,
folder parent, internal note link, body, attachment bytes, Trash and Markdown
front matter. Native and Core temporary stages are empty afterwards. A separate
20.250-second flow chooses Markdown through the shared default-type menu and
creates a synthetic note; SQLite confirms markdown=true and saved new_default.

The first picker script expected a text Cancel button that this iOS system
picker did not provide in full-screen presentation. Sheet presentation plus
interactive-dismissal handling is verified instead. The fixture generator's
first sandboxed run was denied while creating its isolated local state; the
successful run uses a dedicated temporary runtime directory. No personal notes,
extra simulator, live sync retest, Mac source edits, commit or public upload are
part of this build. Native device feel, external file providers and broader
live-service acceptance remain distinct from these local checks.

Build 35 is signed privately (50,198,244 bytes), using the existing registered
profile and unchanged formal icon/Production CloudKit rights. Wi-Fi installation
succeeds and device metadata confirms app.feinotes.mac, 0.1.9 (35). Launch is
explicitly denied because the device is locked; physical UI acceptance remains
pending. The IPA contains no SQLite library. Earlier signed IPAs are preserved.
The existing remote-publication question is still pending; no tunnel/upload or
published-channel pointer is changed. Production sync remains user accepted.


## Build 36: product About and native notices catalogue, 2026-11-15

The platform audit found a real omission in About: iOS displayed the Cargo/Core
version while Mac used the product bundle version. iOS now shows the actual
CFBundleShortVersionString and CFBundleVersion, for example 0.1.9 (36). The same
product view exposes configured support/privacy/terms URLs and notices. Support
and privacy URLs return HTTP 200 in this run; an empty terms setting remains
hidden, matching the current product configuration.

The Mac notice collector accepts an explicit manifest/package/Apple target for
iOS, preserving its existing default graph (458 selected Mac packages before
and after). The locked mobile graph has 362 third-party packages and the same
device/simulator versions. Eight supplemental components reference exact source
commits recorded by their published crates. Generated notices are verified by
source/input/resource hashes during every Xcode build.

The mobile presentation is a native searchable component list with bounded
text pages, Dynamic Type and page-local system Find. Seven application/source
sections and 362 dependency entries produce 369 entries and 564 unique pages,
with at most 6000 Unicode code points per page. Concatenating each entry's pages
reconstructs its full text. Metadata/source URLs and license bodies are retained.
The original complete plain-text resource is also bundled. No document model,
editor structure, SQLite schema or cloud engine is changed.

The initial whole-file Quick Look and UITextView paths exposed very slow system
accessibility queries for the large text. Those intermediate attempts are not
acceptance evidence. The final catalogue limits the text exposed at once and
waits for native search dismissal before pushing its selected reader. The final
focused simulator flow passes in 25.529 seconds: enter About, search an exact
component, read its body, go forward/back with matching restored text, find six
case-insensitive accesskit matches on the page, and dismiss back to About. The
matching screenshot shows the native 1/6 search result and highlighted text.

Evidence is under /private/tmp/fei-ios-about36.ir4k0obg, especially
about-catalogue-navigation.xcresult. Mac, simulator and device Rust builds pass.
The full simulator/device matrix and accepted live CloudKit sync are not rerun.
Physical interaction acceptance and remote-publication authorization remain open.

Build 36 is signed and retained privately as FeiNotes.ipa (50,574,443 bytes).
Signature, existing registered-device profile, unchanged formal icon and
Production container/entitlements were verified. Package readback verifies the
complete prepared notice text/catalogue, all original license bodies/source
URLs, and actual 0.1.9 (36) Info.plist metadata. Wi-Fi installation succeeds;
device metadata confirms app.feinotes.mac, build 36. Launch is denied because
the device is locked. The Mac baseline was reread and still equals 3802182.
User device interaction acceptance and explicit remote-publication permission
remain pending; no upload/tunnel or published-channel pointer was changed.


## Build 37: GPUI Mobile file-picker dismissal, 2026-11-16

Local delivery preparation also passed against the signed build 37 IPA. The
installer page and manifest identify 0.1.9 (37), a complete loopback download
matches the signed artifact SHA-256, and HEAD/range requests and unknown-route
rejection passed. The temporary listener was closed after verification. Evidence:
`/private/tmp/fei-ios-picker37.a6cxgmls/local-install-readiness.json`. This check
started no public tunnel and does not establish remote installation acceptance.

The platform audit distinguishes two native picker modes. Multi-file note import
already has a working Cancel button; its baseline check passes and its import
logic is retained. Single-file backup selection omits Cancel while the backend
forces full-screen presentation, leaving no sheet-dismissal route. The focused
baseline test confirms that missing action. The fix is in the pinned Longbridge
platform adapter, without another import implementation: use a page sheet and
an adaptive presentation delegate to resolve dismissal as Ok(None). Selection,
explicit cancellation and dismissal use the existing pending-picker identity
check and one-shot sender. Scope/Core/SQLite/cloud code is unchanged.

The new sixth tracked patch is gpui-mobile-ios-picker-dismissal.patch. A fresh
archive from pinned commit 7548ebc4b24f00494af9976af8852c7163ded9ce plus all six
ordered patches reconstructs all 180 prepared dependency files exactly. Source
hashes and replay evidence are under /private/tmp/fei-ios-picker37.a6cxgmls.

The same simulator passes a 41.996-second flow that cancels/reopens multi-file
import and dismisses/reopens single-file backup selection. A 57.200-second flow
dismisses/reopens the image selector, chooses a synthetic PNG, saves the note,
and dismisses/reopens Markdown export. SQLite/asset readback confirms the exact
original PNG SHA-256 in the new synthetic note. Export's existing cancellation
handler already works and is unchanged. Two earlier image-selection scripts
failed while navigating the system file provider's locations/history; the final
script waits for the actual My iPhone/app-folder page before continuing.

Simulator and device Rust builds pass. The signed candidate reuses the existing
formal icon, Product App ID and Production container. No Core structures,
protocols, account settings, real notes or Mac source are changed. Accepted live
sync and a broad device/Rust regression matrix are not repeated. Physical UI
acceptance and the existing explicit remote-publication question remain open.

The 37 private IPA is signed (50,576,246 bytes), with the same formal icon,
registered-device profile and Production CloudKit rights. The actual iPhone
metadata confirms app.feinotes.mac, 0.1.9 (37) after Wi-Fi installation. Launch
is denied because the device is locked. Common application Rust/Core, root
Cargo manifest/lock and build script hashes match the signed build 36, so the
Mac compilation is not repeated for this iOS-only dependency patch. The shared
notice resources also match the verified build 36. Earlier signed IPAs remain
unchanged. No public upload or tunnel was started; the existing publication
permission and user acceptance remain pending.


## Build 38: native settings and subpages, 2026-11-16

Settings, General, Appearance, Library, Shortcuts, iCloud, Shared Notes and About
now use inset-grouped UIKit tables. Choice screens use native checkmarks and
push/back transitions. Switches, multiline labels, system typography, safe areas,
shortcut search/recording, invitation input and destructive confirmations belong
to UIKit. Existing native notification settings, system file dialogs and the
component-notice reader remain the platform subpages. Theme preview uses the
selected shared theme's resolved fonts, dimensions and colors. CSS editing is
still hidden by the shared THEME_CSS product gate; this work does not enable it.

`src/settings/mobile.rs` supplies presentation data and validates commands against
the current library/page, selected room and theme scope. It calls the existing
preference writer, theme operations, retention controller, import/backup flows,
CloudKit retry/library switch, sharing and subscription controllers. Shared sync
status and issue descriptions are extracted once for both renderers. The old
mobile GPUI settings shells are removed; the desktop renderer stays in place.
Core, schema, sync engine, session code and Cargo manifests match signed build 37
(50 recorded paths checked). Mac compilation passes; its newer current HEAD has
no committed settings/shortcut delta from the frozen integration baseline.

Native updates no longer depend on drawing the hidden GPUI view. A workspace
observer publishes changes, and settings commands publish their resulting state.
UIKit can retain its transition coordinator inside didShow; buffered updates now
wait for it to release before being applied. This fixes stale labels/values after
returning from a choice screen. Library-to-iCloud navigation retains Library as
the parent. Xcode records the Rust archive as an explicit link-phase dependency.

Focused evidence on the existing iPhone simulator:
- `settings-final.xcresult`: language refresh, 23.581s; library/cloud/shortcut and
  file-picker navigation, 56.769s. The suite's preference case had an obsolete
  expectation that CSS editing was enabled; it is not counted as a pass.
- `settings-prefs-final.xcresult`: preference persistence, language round trip,
  native appearance choices/preview, actual About version and notices return,
  93.690s, passed. The product-disabled CSS entry is asserted absent.
- Rust simulator/device builds, signed device Xcode build and Mac cloud-enabled
  compile pass. Evidence and diagnostics are in
  `/private/tmp/fei-ios-settings38.nficpi3w`.

Build 38 is signed with the existing product identity/profile and formal icon.
An installation attempt returned CoreDevice 4016: trusted device connectivity
was unavailable. Build 37 remains the last confirmed installed build. Device UX,
hardware shortcut recording, cross-account sharing and purchase presentation
are separate acceptance gates. The user's production-sync acceptance is retained;
no new live sync round trip or public installation channel is claimed.

Build 38 device follow-up: after the user unlocked the paired iPhone and rejoined
the same Wi-Fi, installation succeeded. Device metadata confirms app.feinotes.mac
0.1.9 (38), and CoreDevice reports a successful launch. This supersedes the
connectivity failure above; physical interaction acceptance remains pending.
Evidence: install-wifi-ready.json, installed-app-current.json and launch-current.json
in the build 38 run directory.

## Mobile Notes theme and native chrome, 2026-11-16

The selected theme now reaches UIKit navigation, list, settings and editor actions
through a resolved mobile palette. Notes uses existing ink/gold tokens and mobile
body/title metrics, with Dynamic Type scaling. Other theme selections and
light/dark preference still use the existing settings. Desktop metrics are
unchanged; optional mobile roles are documented in FEINOTES-THEMES.md.

The compact list uses native integrated Search and separate Compose. Search owns
its native activation/keyboard/close lifecycle; an explicit native text-field
Clear handler keeps input active. Display text preserves case independently of
the normalized Rust matcher. Search result opening waits for native dismissal;
Back preserves query/results without reopening the keyboard. Native list scope,
operation acknowledgement and existing editor-scope checks protect repeated or
stale commands. Reader and editor tool groups remain distinct, with direct
checklist/table actions and native Share/More.

Wide iPad now keeps the UIKit shell and moves native search/Compose and editor
actions into its top areas. This candidate still uses a navigation stack; the
research's multi-column iPad workspace is not implemented. It adds no Core,
schema, cloud protocol, certificate or release change. The implementation,
validation, source hashes and screenshots are recorded in
[mobile style follow-up](design/mobile-notes-style-20261116/README.md). The retained
screens are Fei Notes simulator output, distinct from the physical Apple Notes
reference. Live Format, complete IME/VoiceOver editing, gallery and native split
workspace remain separate acceptance work.


## Build 40: physical mobile candidate and startup packaging correction

The mobile theme/search/toolbar candidate was requested on the paired iPhone and
iPad. Build 39 installed but failed its pre-library configuration guard on both:
the Swift Product host was linked to a Development Rust archive. No library reset
or data migration was used to address the failure. Build 40 rebuilds the archive
with `FEI_IOS_PRODUCT_CLOUD=1` and preserves the existing Product app identity,
Production container and accepted sync configuration.

`packaging/ios/build-rust.sh` is now an unconditional Xcode pre-build step. Product
selects Production; other configurations select Development. The linked archive
therefore follows the Swift configuration even after a previous simulator/manual
build. A focused Product-configuration startup test enters an isolated local
library, verifies the native shell, then relaunches and verifies the list; it passes
in 13.251s. Simulator/device builds and signing verification pass. Device metadata
confirms 0.1.9 (40) installed on iPhone27.2 and iPad17.7.11. Physical launch and manual
UX are distinguished in the [installation record](design/mobile-notes-style-20261116/installation40.json).
No new certificate, personal-note reset, public upload or live sync acceptance is
claimed. Evidence is in `/private/tmp/fei-mobile-install40-20261116/`.


## Product 45 feedback acceptance (2026-11-17)

Product 0.1.9 (45) includes the initial 17 mobile feedback corrections and the icon asset correction. The initial findings are not the audit boundary. See [the feedback record](design/mobile-feedback-20261117/README.md), [comparison board](design/mobile-feedback-20261117/comparison.html) and [all 50 Mac baseline rows](design/mobile-feedback-20261117/format-matrix.md) for evidence and remaining differences. Shared Core/app tests passed serially (179 + 104); targeted simulator flows and a physical iPad workspace flow passed. Product was installed/launched on iPad, and remote iPhone installation was confirmed by the user. The temporary download tunnel was then closed. No physical iPhone UI pass is claimed. Mobile weight/italic and the reported launch-animation icon whitespace remain explicit visual acceptance items.


## Product 46 semantic main-branch integration

The mobile base is now product `52d391c`, preserving its prior working changes. See [the Product 46 record](design/mobile-product-integration-20261117/build46/README.md) for the completed clear-then-close Search regression, task context recovered from the earlier mobile chat, native shared-list/settings adaptation, scoped room permissions and invitation callbacks. Existing mobile navigation and editor controls remain native. Public source tests, simulator cases and live-service acceptance have separate evidence statuses.

## Mainline integration and TestFlight configuration

The current mobile source is integrated with the Mac product's original-note and
shared-folder navigation, verified CRDT checkpoints and attachment retention.
UIKit keeps its native lists, settings and editor controls. Read-only sessions
remain protected by the common Rust permission checks. Shared-folder creation
controls derive from the current session and write permission, rather than from a
blanket prohibition on shared sessions.

The `TestFlight` scheme uses optimized Rust, the Production container and the
receipt-verified beta cloud-access path. QA fixtures are forbidden in that
configuration. The platform-specific release workflow pins one source revision
and keeps iOS signing separate from Mac signing.

Current iOS builds link Apple's system SQLite, using Rusqlite 0.40.2. This replaces
the historical bundled-SQLite setup described above. The Mac workspace retains
Rusqlite 0.41.1 and bundled SQLite. The shared store's SQL and on-disk format are
unchanged. The core dependency accepts both tested API versions. Device linking
checks exclude SQLite 3.44 client-data imports and third-party filesystem-capacity
probes; the deployment target remains iOS 17. Compiling with a current SDK does
not constitute runtime validation on an iOS 17 device.

The website's phone images are actual simulator screenshots of synthetic notes.
They illustrate the same content as the existing Mac scene and are not presented
as a recording of a live cloud transfer.
