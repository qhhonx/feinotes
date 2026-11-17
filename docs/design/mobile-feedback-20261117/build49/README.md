# Mobile feedback build 49

The user's build-48 feedback continues the mobile comparison. This record covers the nine new findings without treating them as an exhaustive product checklist. Private user screenshots stay outside repository evidence. All implementation screenshots and linked-document chains use the isolated QA library and synthetic content.

## Changes

| Area | Implementation |
| --- | --- |
| Selection readability | Keep the original table and multiple-selection model. Selected batch rows use a neutral system fill and an accent circle/check, rather than applying the document-focus accent to the whole row. Preserve normal text and summary colors. Row background configuration is managed explicitly, so UIKit does not replace it with its default selected fill. |
| Home and tabs | The page title is **主页 / Home**. **我的文档 / My documents**, **收藏 / Favorites** and **共享文档 / Shared documents** are same-level native segments. Remove the additional shared-document navigation row. Segment widths follow their content. iPad retains the corresponding sidebar scopes. |
| Collapsing title | Use the navigation controller's real large title and identify the table as its content scroll view. The list extends below the bar, using native adjusted insets. Remove the large UILabel from the table header. The title moves into the bar when scrolling. |
| Trash return | Capture the originating view, library, settings page/parent, folder, tab and query before opening Trash. Settings → Documents → Trash returns to Documents. The existing confirmation and removal commands remain unchanged. |
| Document links | Mobile editor link events retain a library-bound source stack, including folder, tab and query. A → B → C returns C → B → A. Reopening uses existing save, scroll and selection restoration. List/sidebar navigation starts a new document route. Repeated visits have distinct route identities. iPad's focus control supplements the system back button instead of replacing it. Folder mentions continue to enter the existing folder hierarchy, and do not create document-return frames. |
| Native pop completion | Ignore duplicate completion callbacks for the same pending native pop until the source route has changed. QA waits for the native state acknowledgment before its next action. Cancelled transitions do not invent a return destination. |
| Theme containers | Mobile Notes and Feishu use the common parsed-theme renderer. Desktop Feishu keeps its existing rendering path. Compact unnamed code blocks no longer reserve a 48-point blank header; controls retain their touch area beside the text. Code and table text honor dynamic type and mobile overrides. Table scrollbar padding is 12 points rather than a 44-point button target. |
| iPad focus gestures | Refresh the three-column reading guard when entering/leaving focused content and when split display mode changes. A removed sidebar must not leave a transparent view intercepting table/code gestures. |
| Preview close | Standalone Quick Look wrappers use the native Close item (×), keeping the existing share control and dismissal behavior. |
| Format and Insert | Both entry points open the original inline Slash/Add catalog, including Countdown and the complete supported content set. Remove the intermediate text toolbar from these entry points. The catalog uses readable touch rows, fits the live editor viewport and remains scrollable. Existing text-selection formatting actions remain available. |
| Compose color | Explicitly use the active theme accent on Home, the iPhone document reader and the iPad detail Compose item. |

## Theme audit

The bounded matrix is Notes and Feishu, light and dark, iPhone and iPad. Synthetic scenes cover title/body hierarchy, headings, bold/italic/underline/strike, inline code, single/multiline code, nested lists, checked/unchecked tasks, quote, callout, columns and tables. Code typography and actual single-line block height are asserted. Screenshots verify clipping, contrast, padding and wrapping. Existing horizontal-container and vertical-page gesture regressions are rerun separately.

`.mobile-code` and `.mobile-table` may override font size and line height, alongside the existing mobile page/title selectors. Defaults apply readable mobile floors before system text scaling. These are rendering metrics and do not change the stored document or the cloud protocol.

## Evidence and boundaries

The final [verification record](verification.json) contains ten passed current UI selectors, 241 passed host tests (four ignored), strict Clippy and the final IPA signature/hash checks. Notes/Feishu light/dark checks passed on both simulator device classes. The focused iPad overflow-table regression passed after refreshing the reading guard. Earlier six-column fixtures fitted the expanded iPad and were superseded by a ten-column overflow fixture. The first iPhone theme run was superseded after resetting its intentionally restored document scroll before measurement. Physical-device touch/keyboard acceptance and live cross-account sharing remain separate. No Core structural operation, library migration or sharing protocol was introduced here. The normal save-failure path remains authoritative when a requested return cannot save the current document.

The candidate is **0.1.9 (49)**. Build 48 remains immutable. No TestFlight upload is part of this feedback checkout.

## UIKit references

The [native navigation-bar guide](https://developer.apple.com/documentation/uikit/customizing-your-app-s-navigation-bar) describes large titles transitioning into the bar with scroll content. The [table-cell background API](https://developer.apple.com/documentation/uikit/uitableviewcell/automaticallyupdatesbackgroundconfiguration) explains disabling automatic updates when applying custom state configurations; [backgroundConfiguration](https://developer.apple.com/documentation/uikit/uitableviewcell/backgroundconfiguration-24e8e) also documents its mutual exclusion with selected background views.


## Synthetic evidence

The capture names and test results are recorded in `verification.json`. Raw
screenshots and simulator result bundles are local QA artifacts and are not
included in this source handoff. These results describe mobile build 49 before
integration with the current Mac product branch.
