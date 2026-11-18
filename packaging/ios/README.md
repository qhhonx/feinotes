# Fei Notes iOS builds

## Current local candidate, build 49

Build 49 addresses the latest build-48 feedback using the existing shared editor
and native iPhone/iPad hosts. See
`docs/design/mobile-feedback-20261117/build49/README.md` and `verification.json`
for the current simulator selectors, theme matrix and package checks. Physical
acceptance and live cross-account sharing are not implied. Earlier installation
records below remain historical. Generate the project with `project.yml` after
preparing the pinned backend; keep Product and isolated QA cloud namespaces
separate. No signing materials belong in a source handoff.

## iPhone docked-keyboard follow-up, build 44

Build 44 keeps editor tools above the current docked keyboard, derives keyboard
button state from the same layout guide, and commits native marked text before
End Typing or Format/Insert handoff. Format can resume the same editable input
after dismissal. Compact-height title padding follows the actual editor
viewport and title line height. All three final UI cases and 232 Rust regression tests
passed. Product 0.1.9 (44) is installed in place on the iPhone with version,
launch and process verification. Full UI rerun and install details are recorded in
`docs/design/iphone-keyboard-20261117/README.md`. Physical XCTest failed at runner
bootstrap; no physical docked-keyboard pass is inferred. The iPad retains 42.

## iPhone interaction polish delivery, build 43

Build 43 refines iPhone native search restoration, top navigation actions and
reading/editing bottom tools. Product 0.1.9 (43) was installed in place on the
iPhone; version, launch and process checks passed. The isolated local QA app
passed two simulator UI cases and a physical manual core-flow check. Physical
XCTest runner bootstrap failed before executing cases. Search software-keyboard
geometry is verified; editor accessory geometry above it remains open. The iPad
retains build 42. See `docs/design/iphone-polish-20261117/README.md` for evidence.

## Native iPad workspace delivery, build 42

Build 42 adds a native triple-column iPad container: hierarchical folders, an
independent searchable list and the existing editor. Fit selects three columns,
list/detail or compact presentation; document focus expands and restores detail.
The physical iPadOS 17.7.11 synthetic interaction test passed, including search,
selection and orientation changes. Product 0.1.9 (42) was installed in place on
the iPad with version and launch verification. At that delivery the iPhone remained at build 41.
See `docs/design/ipad-native-workspace-20261117/README.md` for evidence and limits.

## Product integration delivery, build 41

Build 41 packages the merged product snapshot `0c7640e` with the native mobile
adapters and subsequent local Mac theme fixes. It was signed using the existing
two-device profile and installed in place on the paired iPhone and iPad. Device
metadata confirms 0.1.9 (41) on both. Both apps launched successfully after the
user unlocked the devices, and both processes were confirmed running. No app data was reset.
The Product namespace build safeguard remains enabled. See
`docs/design/mobile-product-integration-20261117/installation41.json`.

## Mobile theme candidate, build 40

Build 40 installs the Notes mobile theme, native search lifecycle and reading/editing
toolbars on the paired iPhone and iPad. It corrects build 39's Swift/Rust namespace
mismatch. The Product startup/relaunch regression passes using an isolated local
simulator library. Device metadata confirms 0.1.9 (40); user interaction acceptance
remains separate. See `docs/design/mobile-notes-style-20261116/README.md`.

## Native settings candidate, build 38

Build 38 uses native grouped settings and choice pages with shared Rust state and
operations. It is signed and installed over Wi-Fi; device metadata confirms 0.1.9 (38)
and launch succeeded. Physical settings interaction acceptance remains pending. Focused simulator
language/navigation and preference/About checks pass; see IOS-IMPLEMENTATION.md.
Private signed candidates are kept in the ignored `.private-builds/` directory.
This directory must not be published by website or distribution automation.

The Rust archive is an explicit Xcode link-phase dependency, resolved from the
SDK-specific RUST_TARGET directory through LIBRARY_SEARCH_PATHS. Finish the Rust
build before invoking Xcode; an updated archive must trigger relinking even when
Swift sources are unchanged.

## Current product configuration

The `Product` Xcode configuration uses the formal Fei Notes name/icon, universal
App ID `app.feinotes.mac`, Production container `iCloud.app.feinotes.mac` and the
same library scope as the installed Mac product. Xcode's pre-build step runs
`build-rust.sh` with `FEI_IOS_PRODUCT_CLOUD=1` for `Product` and `TestFlight`, and `0` for other
configurations, before linking. This prevents a cached Development Rust archive
from being packaged with the Production Swift host. Direct Cargo builds still
need the matching `FEI_IOS_PRODUCT_CLOUD` value. `Debug` retains the
older isolated Development configuration. Do not use the historical CloudLab
signing instructions below for a product package.

The `TestFlight` configuration and `FeiNotesTestFlight` scheme link an optimized
Rust release archive and the Production Swift bridge. QA/devtools features are
forbidden. Beta access still requires the existing verified TestFlight receipt
path. Use the platform-specific manual workflow; it pins the exact source SHA,
version and build and validates the archive before upload. Release notes live in
`release-notes/1.0.8/`. iOS links system SQLite with Rusqlite 0.40.2, whose base
bindings support the deployment baseline without importing SQLite 3.44 client-data
APIs. The Mac workspace keeps Rusqlite 0.41.1 and bundled SQLite.

`prepare-gpui.sh` prepares the pinned Longbridge backend and GPUI core extension.
The core extension is generated from the checksum-verified gpui-pre 0.3.10 crate
and `patches/gpui-pre-image-cache.patch`. `prepare-gpui-core.py --crate <archive>`
supports an offline setup; without an archive it uses the Cargo cache or fetches
the exact pinned crate. The generated `.gpui-core` directory is ignored by Git.
The checked manifest verifies every source file before use and refuses to replace
a changed directory. This adds thumbnail scale, static-image buffer release,
missing-atlas recovery and visible-only loading to the existing mobile backend.
No dependency-version upgrade is implied. The same checked patch exposes the current
board renderer path, glyph-outline and CPU-frame APIs. iOS uploads changing CPU
frames through its existing image atlas, rather than the desktop-specific upload
primitive. The pinned mobile platform patch supplies CoreText font outlines.

The file prompt adapter also applies `gpui-mobile-ios-picker-dismissal.patch`.
Page-sheet dismissal completes the existing receiver as cancellation, including
single-file modes that omit a Cancel button. Explicit Cancel and successful
selection keep the same receiver/identity handling and sandbox-copy behavior.

Use the existing iPhone simulator for focused checks and synthetic libraries.
Record source fingerprints and keep earlier signed candidates immutable. Build 37
is installed and its device metadata verified; launch awaits device unlock. The user accepted Production sync on build 30.
Do not repeat that live sync check or infer device acceptance from compilation.
Current correspondence and remaining gates are in `docs/IOS-IMPLEMENTATION.md`.

## About and bundled notices

The product About page reads the signed bundle version and iOS build number,
uses the configured support/privacy/terms URLs, and exposes a native component
catalogue with bounded text pages and page-local system Find. It reuses the Mac
notice collector against the locked mobile dependency graph; the Mac collector's
default graph is unchanged. Generate the resource with the same Cargo environment
used for the mobile Rust build:

```sh
python3 packaging/ios/prepare-notices.py
python3 packaging/ios/prepare-notices.py --check
```

Generation is offline. The iOS supplement records the exact upstream commits for
license texts absent from published crates. Device and simulator graphs must
select the same dependency versions. All page fragments reconstruct the complete
source text. Xcode verifies the resource/input hashes on every build and refuses
stale notices after a dependency or source-notice change. Regenerate before
running xcodegen when creating a new checkout's resources.

## Historical development notes

The following sections retain early Development-container and iPad QA procedures.
Their signing, channel and default-mode descriptions are historical.


iOS 工程使用飞记现有 GPUI 工作区和编辑器。UIKit 管理场景和键盘，Longbridge 的 iOS 后端提供 GPUI 窗口、渲染和输入。iOS 单独锁定 GPUI 版本，桌面依赖继续由仓库根的 Cargo 工程管理。

## 构建

在飞记仓库根目录运行。Rust 工具链需要安装对应的 iOS target。

```sh
sh packaging/ios/prepare-gpui.sh
SDKROOT="$(xcrun --sdk iphonesimulator --show-sdk-path)" \
  IPHONEOS_DEPLOYMENT_TARGET=17.0 CARGO_PROFILE_DEV_DEBUG=0 \
  cargo +stable build --locked --manifest-path packaging/ios/Cargo.toml \
  --features cloudkit-sync --target aarch64-apple-ios-sim
xcodegen generate --spec packaging/ios/project.yml --project packaging/ios
```

`prepare-gpui.sh` 提取固定版本的 Longbridge 源码并应用 iPadOS 17 字体兼容补丁。也可以传入本地 Longbridge 仓库的绝对路径。iOS links the platform SQLite on iOS/iPadOS 17 and later. The Mac dependency configuration is unchanged.

用 Xcode 打开生成的工程，选具体的 arm64 iPhone 或 iPad 模拟器。真机用 `iphoneos` SDK 和 `aarch64-apple-ios`，通过具有开发容器权限的 iOS 描述文件签名。测试目标 `FeiNotesEditorProbeUITests` 验证本地模式可点击正文、输入和重开。重开后从测试 App 沙盒只读查询 `library.sqlite3`，确认正文含本次测试标记。

真机签名时覆盖 `PRODUCT_BUNDLE_IDENTIFIER=app.quietmd.cloudlab`，复用 Mac 内测包已有的 CloudLab 测试标识。用 Apple Development 证书与包含验收 iPad 的 iOS 开发描述文件签名，核对成品的容器为 `iCloud.app.quietmd.cloudlab`、环境为 `Development`。需要 iPadOS 17 或以上和开发者模式。

## 连接隔离开发区

首次启动时用 `FEI_NOTES_MVP_CHANNEL` 指定测试区 UUID。模拟器可以在调用 `simctl launch` 时传 `SIMCTL_CHILD_FEI_NOTES_MVP_CHANNEL`。App 仅连接现有 CloudKit 开发容器的该 UUID 测试区，并把本地副本放在测试 App 的沙盒里。成功打开后，App 记住该测试区，从主屏幕重开时继续打开同一副本。首次启动时未指定测试区，或者显式设定 `FEI_NOTES_MVP_MODE=local`，会打开包含合成笔记的本地 SQLite 测试库。iPad 和 Mac 开发包须使用同一 iCloud 账户。

Mac 端用带 devtools 的开发包，指定同一个 `QUIET_SYNC_QA_CHANNEL`、独立的 `QUIET_SYNC_QA_ROOT` 和 `QUIET_SYNC_QA_MIGRATION=1`。已有的 `--cloud-sync-qa structure-seed` 创建一篇带表格的合成笔记，`--cloud-sync-qa sync` 同步测试库。使用新 UUID 时可以从空库复验。

## 真机双向验收

1. Mac 上传合成笔记。在 iPad 首次启动时传入同一测试区 UUID，确认拉取的笔记 ID 和正文与 Mac 一致。
2. iPad 断网，在该笔记里用系统拼音选字。退到主屏幕后结束进程，再从主屏幕重开，核对正文和 SQLite 待上传记录。
3. iPad 恢复网络，在 Mac 拉取并确认同一笔记有且仅有一次 iPad 编辑。Mac 再修改同一笔记并上传，核对 iPad 收到改动。
4. 双端重开，确认笔记 ID 与正文一致，待上传队列归零。

Mac 验收命令 `--cloud-sync-qa mvp-note-check` 用 `QUIET_SYNC_QA_MVP_TEXT` 检查唯一测试笔记中的标记，并输出笔记 ID、正文 SHA-256 和待上传数。`mvp-note-append` 在隔离副本的正文追加同一变量指定的标记，再用 `sync` 上传。

## 当前验收

- iPadOS 27 模拟器已显示飞记原编辑视图。XCUITest 通过点击正文、键盘输入、后台保存与退出重开，随后从 SQLite 核对了正文。
- iPhone 17e 模拟器运行同一套 GPUI 编辑器。XCUITest 验证了本地英文输入和系统拼音选字。退出重开后，SQLite 的同一篇测试笔记包含英文标记和一次「飞机」，没有留存输入中的拼音。iPad 模拟器也通过了拼音选字和重开检查。
- 使用模拟器中的隔离 CloudKit 缓存副本测试了无网络重开。第二次启动没有传入测试区，仍打开同一篇笔记。SQLite 中保留了两次编辑，并为同一笔记保留两条待上传更新。
- iPad 模拟器复现并修复了后台保存写入未确认拼音的问题。使用隔离离线缓存复验后，退到后台时保存了已输入的文字，重开后选中的汉字也保存成功。未确认的拼音未落盘，两次有效编辑为同一笔记新增了两条待上传更新。
- Mac 开发包已上传一篇合成笔记，第二个空 Mac 测试副本从真实 CloudKit 拉回了该笔记，待传队列为零。
- iPad 6（iPadOS 17.7.11）已完成开发配对、安装并启动。真机启动需要内置 SQLite 和 Longbridge 的字体属性兼容补丁。
- 真机重启后，iPad 通过真实 CloudKit 拉到了 Mac 上传的「协同结构验收」。笔记 ID 和正文 SHA-256 与 Mac 副本一致。
- iPad 关闭 Wi-Fi 后用系统拼音输入「飞记离线验收」。结束进程并重开后，原笔记和 5 条待上传更新都保留。恢复 Wi-Fi 后，CloudKit 的资源上传一度返回 `NSURLErrorDomain:-1202`。
- Mac 在同一篇笔记追加并上传「Mac 回传验收」。独立的 Mac 副本确认回写已经到达 CloudKit。
- 2026-11-10 真机复验时，iPad 日志记录 `uploaded=5 downloaded=6`。Mac 从 CloudKit 拉取补传，iPad 结束进程后再次重开。两端各有一篇笔记，两个标记各出现一次，待上传数均为零。
- 笔记 ID 为 `6fc03068-7bda-4406-b5b7-8b1a6021cf18`。最终正文 SHA-256 在两端均为 `bbd6abc2f1af5e57cdeffd3c7b54247ce862b8f8a7c4f23a77688cc4c5487e6d`。
- Apple 系统信任数据检查返回网络错误，设备报告的资产版本保持在 `1003`。CloudKit 在恢复飞记内测包后恢复上传，证书错误的具体恢复原因未确认。
- iPad 模拟器登录 iCloud 时出现网络错误，iPhone 模拟器打开开发区时返回需要登录 Apple 账户。

## iPhone layout preview

This directory is now maintained in the `mobile/iphone-layout` worktree based on
the Fei Notes `product/quiet-sync` commit `83583bb`. The mobile crate directly
compiles the product source and document core. Build 3 adds compact note-list,
editor and settings routes, touch metrics, narrow page margins and a formatting
bar dispatching the existing editor actions.

Use the existing registered-device Development profile and CloudLab test zone.
The app keeps the existing bundle identifier and cached library. Source revision
and build are shown in the mobile About page. System text-selection handles and
the full consumer cloud-library switching flow require subsequent adaptation.

The targeted iPhone simulator check covers list/editor navigation, input,
save-on-back, the More sheet, settings and the cloud status page. Read back the
synthetic SQLite library to confirm input was saved. Mac compatibility is checked
with a cloud-enabled compile. On-device interaction acceptance belongs to the
user's existing iPhone.

`install-channel.py --ipa /absolute/path/FeiNotes.ipa --cloudflared /absolute/path/cloudflared --state /private/tmp/channel.json`
serves this signed development build behind an independent random HTTPS path.
The process must remain running. The state file contains the private installation
URL and must stay outside source control.

Export signed builds through Xcode archive/export. Before hosting, run
`python3 packaging/ios/ipa_package.py /absolute/path/FeiNotes.ipa` to verify the
actual raw extracted payload signature. The install server performs this check
itself. To ZIP an already-signed app without adding macOS extended attributes,
use `ipa_package.py /absolute/path/FeiNotes.app --output /absolute/path/FeiNotes.ipa`.
Default macOS-aware ZIP packaging can inject AppleDouble `._` entries inside the
app and invalidate its resource seal, even when the input app passed codesign.
Product 46 exposed this regression; the known-good 45, broken 46 and repaired
archive are compared in `docs/design/mobile-product-integration-20261117/build46`.


## Native navigation acceptance, build 4

Build 4 uses the original Fei Notes icon and the 飞记 display name. UIKit owns
navigation, back transitions, action sheets, naming forms and file export. The
single live GPUI view is moved between native route controllers; inactive routes
keep snapshots during transitions. Rust remains the owner of product state and
commands. The display link remains owned by the host while the GPUI controller
moves to another route, and drawing is skipped while the app is inactive.

Both shells now use the shared workspace action bindings. Mobile search, sort,
note management, CRDT-aware undo, preferences, Markdown export and backup creation
use the product implementations. File generation runs away from the main UI
thread. The iOS interface exports through UIDocumentPicker instead of the
unimplemented GPUI Mobile save-path dialog.

See ../../docs/IOS-IMPLEMENTATION.md for the objective, evidence and remaining
Mac feature alignment gates. Build 4 is a device acceptance delivery, not a claim
that consumer account switching, sharing, StoreKit, restore or PDF/print are
complete. The app's About page uses the common product view; source provenance is
recorded with each development artifact outside the repository.

## Library sessions and restore, build 5

The app can replace its library session without replacing the GPUI Mobile window.
Shared stores and the sync journal hold leases on the selected root. Services and
old views must close before selection changes. Owned sync tasks use a cancellation
token so a delayed CloudKit response cannot enter the next library.

Local storage uses `FeiNotesEditorProbe/local-only`; a historical flat library is
copied atomically and retained at its original location. The application data
identifier remains a migration detail. Separating this local root from the cloud
cache makes the existing atomic backup installer safe to use without moving
another library's pending queue.

Native library choices route to existing product preparation and copy behavior.
Backup restore uses the GPUI Mobile file picker, shared validation and summary,
a native confirmation, and the existing backup installer. The Workspace is then
recreated inside the same window. New data readback, not only menu presence, is
required for session acceptance.

The selected simulator restore check creates its own backup and verifies the
editor can still save after restoring. The offline cloud-session check requires
a synthetic cached-account fixture and `pause-cloud`; it does not contact a real
CloudKit zone. Use `CODE_SIGNING_ALLOWED=YES CODE_SIGN_IDENTITY=-` with the configured
entitlements for that simulator check. A UI-only unsigned app cannot initialize
CKContainer. Test fixtures and their private paths stay outside Git.

Current source and acceptance evidence are tracked in
`../../docs/IOS-IMPLEMENTATION.md`. The signed build-4 IPA already delivered to the
user is independent of these later worktree changes.

## Native output and CSS editing, build 6

The original document HTML serializer feeds a temporary WKWebView and A4
UIPrintPageRenderer. PDF uses the same Files exporter as other formats, while
AirPrint receives the generated PDF through UIPrintInteractionController. The
callback owns its completion state even if the original Rust view closes.
Preparation timeouts do not impose a timeout on a user choosing a printer.

The normal Appearance page opens its existing editable theme file in a UIKit
text editor. Atomic saves are read by the existing Rust theme watcher and parser;
there is no separate mobile theme format. The Documents folder is visible in
Files, while the library and its pending queue stay in Application Support.

The selected PDF acceptance requires FEI-PDF-ACCEPT6.md in the app's Documents
folder, a synthetic multi-page fixture containing 96 numbered paragraphs, a
small data-URI image and a table. Its export is verified outside the repository
for page size, paragraph order and image pixels. Printer cancellation is a UI
check, not evidence that a physical printer completed a job. The CSS acceptance
inserts only a unique comment into a synthetic library's theme and checks the
rest of the stylesheet byte for byte.


## Native table controls, build 9

The editor More menu exposes Table controls for the current table. UIKit supplies
a grouped sheet while the existing GPUI renderer and Core retain the document,
selection, table commands and Undo history. The sheet covers selection, range
expansion, insertion/deletion/movement, merge/split, headers, column widths,
vertical alignment and clear/light-blue cell backgrounds. Reading mode exposes
selection only. Menu actions validate their requesting editor and selection;
remote replacement and permission transitions invalidate stale requests.

One existing iPhone simulator passed the focused table flow, including table
deletion and Undo plus the reading-mode sheet. SQLite readback verifies the
restored dimensions, unequal widths, header, styles and original cell text.
Physical acceptance and the full remaining Mac/touch feature gates are tracked
in ../../docs/IOS-IMPLEMENTATION.md. Builds 6 through 8 remain immutable.


## Shared formatting and outline, build 10

One native editor-controls controller now presents text marks, all nine heading
levels, paragraph alignment, complete shared text/highlight palettes and the
full cell background palette. Body and table selections dispatch the existing
Editor methods. Context stamps include their purpose, generation and selection;
reading/permission changes invalidate mutation controls. The GPUI editor remains
the document view.

On iPhone, Outline uses the existing catalogue entries and jump/unfold behavior
through the system sheet. The mobile page layout applies in both orientations,
so the desktop floating catalogue does not cover a heading. Navigation buttons
keep their identity across note-state updates, and menu commands run after the
system sheet dismisses. Runtime evidence, remaining Mac alignment gates and
remote-publication status are tracked in ../../docs/IOS-IMPLEMENTATION.md.


## Insertion, links and attachment preview, build 11

The existing editor-controls sheet presents insertion actions and the keyboard
bar exposes full formatting and insertion. GPUI Mobile's Files picker feeds the
shared image/file import logic; asynchronous completion checks the original
editor context and retains the library session while preparing assets.

A native link form dispatches the shared validation, normalization, submit,
remove and Undo methods. Native Quick Look previews current note attachments,
with a stable parent navigation bar and editing disabled. Reading mode retains
attachment access. The pinned backend has a third tracked patch for length-aware
URL construction and UIKit options. Source reconstruction and runtime evidence
are recorded in ../../docs/IOS-IMPLEMENTATION.md. The focused simulator checks use
synthetic PNG/text fixtures and do not claim acceptance of every media format or
live CloudKit service behavior.


## Shared confirmations and search, build 12

Confirmation text and execution are shared with Mac. The iPhone system alert
covers the existing actions, choice flags and save-failure alternatives, and
checks the requesting context before applying a response. Trash has a native
Empty Trash control. Search keeps the shared matcher/snippet code while adding
clear and empty states, a full-bar tap target and GPUI Mobile keyboard requests
from shared single-line inputs. The existing pinned icon is visible in note rows.

The focused simulator flows cover duplicate/pin/favorite, delete/restore/purge
and empty Trash, title-first case-insensitive search and backup restoration
through the new confirmation bridge. SQLite readback checks the results and
preservation of the pre-existing synthetic documents. Live services, physical
interaction and unexercised confirmation variants remain separate gates in
../../docs/IOS-IMPLEMENTATION.md.


## Native document find and Return, build 13

`UIFindInteraction` provides the system find/replace navigator around the shared
GPUI editor. A logical Rust find session keeps one UIKit session across mode
changes; a fresh context installs a new interaction. Callbacks cross the GPUI
lease boundary asynchronously and replacements validate identity, generation and
permission. Reading mode supports find navigation only.

The fourth tracked backend patch dispatches software Return through GPUI's Enter
key path, preserving the existing editor's paragraph, list, code and single-line
Submit behavior. The focused simulator flows and XML/SQLite checks are recorded
in ../../docs/IOS-IMPLEMENTATION.md; hardware/IME and live-service acceptance remain
separate from this evidence.


## Native find viewport, build 14

The iOS reveal policy keeps the current shared find match near the top of the
editor and repeats after viewport size changes. Landscape find temporarily fades
the retained native navigation bar and extends the editor above its usual safe
area; ending find or returning to portrait restores both. The native bar and back
item remain alive across mode changes. Mac reveal and the shared find/replacement
engine are unchanged.

`FindViewportUITests` covers search/replace mode switching, distant matches in a
45-paragraph note, both orientations and returning to the note list. Screenshot
readback verifies eight full 24-point current-match highlights above native find
controls. The landscape replacement editor grows from 14 to 92 points. SQLite
checks preserve all probe paragraphs and 93 prior documents/block payloads.

The build uses the formal product name/icon and is prepared locally. Publication
remains pending explicit Cloudflare authorization; no existing channel is changed.
Physical interaction, live sync and the remaining product-alignment gates are
tracked in `docs/IOS-IMPLEMENTATION.md`.


## Shared insertion catalogue, build 15

The native insertion menu derives unique items and categories from the Mac
`SLASH_ITEMS`/`GROUPS`, reuses translations and Markdown eligibility, and dispatches
the same insertion operations. Existing quick-entry identifiers remain stable.
Selection generation and edit permission are checked before dispatch. Inserted
editable content requests the software keyboard through GPUI Mobile.

The focused `InsertionUITests` flow verifies native category navigation and
callout/columns/code insertion with direct text input, rename and return. SQLite
checks exact content and parent relationships, two equal-width columns, code
newlines and preservation of all 101 prior documents/block payloads. Specialized
equation/button/reminder/countdown configuration panels and advanced container
interaction remain separate touch-adaptation gates. The signed package is local;
remote publication remains pending explicit Cloudflare authorization.


## Native object forms, build 16

`ObjectForm.swift` presents reusable UIKit text/source fields and option lists.
`More > Content settings` provides an edit list for existing inline content and
countdowns. Equations and buttons use native forms, while date-related panels
remain pending adaptation. The shared GPUI editor keeps drawing their content.

Equation changes use the same live source-write helper as the desktop field.
Button changes use the existing draft controller, validators, action setter,
confirm/cancel methods and palette definitions. Logical form identities and
selection/generation checks guard callbacks; UIKit work and callbacks are
deferred beyond GPUI leases. Unchanged option rows retain their identity and
shared validation errors expand the sheet into view.

`ObjectFormUITests` verifies equation reopen, button actions/default labels, URL
error visibility and normalization, a color choice, draft cancellation and shared
Undo. SQLite verifies the exact resulting inline XML and preservation of 105
prior documents/block payloads. This flow configures actions without executing
external links or duplicate-page behavior. The signed build remains local while
Cloudflare publication authorization is pending. Date panels, code/media
controls, physical interaction and live-service gates remain in the alignment
record.


## Native date forms, build 17

The shared native object form adds `UIDatePicker`, `UISwitch` and numeric fields.
Reminder controls call the existing live reminder transforms and advance list.
Countdown controls call the original draft/normalization/confirm and color
operations. Input length is supplied by the existing GPUI text fields. Native
date wheels use the shared time offset, displayed in the label. Color-only
countdown saves preserve the clock through the separate existing palette path;
that routing is source-reviewed rather than separately timed through the UI.

`DateFormUITests` covers whole-day/year/advance options, reopen, countdown duration
and end modes, color, cancel, visible past-end validation and a focused numeric
limit/normalization check. SQLite verifies the actual instants, duration fields,
color, future end and preservation of all 111 previous documents/block payloads.
Configuration evidence does not establish notification permission, delivery or
background scheduling. Device-zone/DST behavior, code/media controls, physical
acceptance and live-service gates remain in `docs/IOS-IMPLEMENTATION.md`. The
signed package is local; Cloudflare publication authorization remains pending.


## Native code controls, build 18

Code captions, language changes, wrapping, folding and copy route through shared
editor helpers. The native language picker uses the existing Rust language
matcher. Reading mode exposes code tools with edit-only fields disabled and
Rust-side permission checks. Native search updates in place and the code form
avoids initially opening the caption keyboard.

The focused `CodeFormUITests` flow checks insertion/input, language search,
name/wrap/fold/reopen and reading restrictions on the existing iPhone simulator.
SQLite checks the exact three-line body, caption, Rust token, wrap and preservation
of all 115 prior documents and 603 block payloads. Fold remains session view state;
clipboard output and header-entry feel remain device acceptance items. Build 18
is signed locally; the existing remote channel is unchanged pending explicit
Cloudflare publication authorization. Remaining product-alignment gates are in
`docs/IOS-IMPLEMENTATION.md`.


## Native image settings, build 19

Image settings use native description/alignment/width controls and crop gestures.
Rust retains the original caption, proportional resize, crop constraints/save,
reset and history operations. The UIKit preview passes normalized positions to
the shared crop drag controller and displays its rectangle. It does not rewrite
image bytes. Desktop image toolbars and small resize handles are hidden on iOS.

`ImageControlsUITests` checks PNG insertion, setting/reopen, crop cancel/save,
reset/Undo and crop rotation on the existing iPhone simulator. SQLite checks the
actual crop and proportional dimensions; the original PNG hash and all 116 prior
documents/604 block payloads are unchanged. The flow also exposed and fixed an
integer overflow when shared crop controls derived IDs from random block IDs.
Photos, large/EXIF/animated media, preview gallery details, VoiceOver crop controls
and device acceptance remain separate gates. Build 19 is signed locally; the
remote installation channel still awaits explicit publication authorization.


## Photo Library through GPUI Mobile, build 20

The insertion menu offers Photo Library and retains the Files image entry.
`gpui-mobile-ios-gallery.patch` adds an asynchronous PHPicker receiver to GPUI
Mobile's existing image-picker package. The app uses the shared image validator,
asset writer and insert command, with the original session lease and stale
selection/permission checks. Supported representations are preserved; the system
provides compatible JPEG for the tested HEIC sample. Owned temporary copies are
removed after import. The legacy synchronous image-picker APIs remain unchanged.

Targeted UI checks verify cancel, synthetic PNG/HEIC selection, native preview,
shared rendering and saved notes on the same iPhone simulator. Readback confirms
PNG's exact original hash, HEIC-to-JPEG compatibility, 640-by-360 dimensions and
preservation of all 119 previous documents/610 block payloads. The pinned GPUI
Mobile revision reconstructs to 180 files with five patches. iCloud-only Photos,
large/EXIF/animated media and device acceptance remain separate gates. Build 20
is signed locally; remote publication still awaits explicit authorization.


## Native date-reminder delivery, build 21

The shared workspace reminder collector supplies the existing flags, instants,
titles and body text. iOS uses a User Notifications bridge for the earliest 60
future reminders, explicit permission/status UI, schedule reconciliation and
library-checked notification taps. UTC calendar triggers preserve the stored
instant at second granularity. Countdown keeps its existing in-app bubble behavior.

Targeted checks on the existing iPhone simulator verify permission, removing a
future request by turning off Notify, real system delivery after terminating the
app, and tapping the banner to cold-open the exact note. SQLite confirms the
unchanged instants, disabled flag and preservation of 125 prior documents/620
block payloads. The status page shows the current Notification Center contents,
not a retained history after responses. Denial, clock changes, capacity and
physical delivery remain separate gates. Build 21 is signed locally; the remote
channel still awaits explicit publication authorization.
