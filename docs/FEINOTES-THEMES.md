# Fei Notes theme extensions

Read the preserved upstream [theme reference](THEMES.md) first. This document describes the additional properties implemented by Fei Notes. It supplements the upstream guide rather than replacing it.

## Built-in themes and appearance

The product combines Feishu-style block writing and editing with Apple Notes-inspired visual hierarchy, layout and iCloud integration. Notes is a visual reference, not a requirement to duplicate Apple Notes commands or its entire format model. Preserve Fei Notes menus, shortcuts and block interactions when correcting theme defects.

Fei Notes offers two themes: Notes and Feishu. Older selections of Craft, Notion, Obsidian or custom themes fall back to Notes; their CSS files remain on disk. Existing Notes and Feishu selections and their CSS overrides remain supported. Theme CSS can style the document and selected shell controls; menu layout, settings layout and interaction behavior remain controlled by the existing UI.

Color properties and `--token-*` accept `light-dark(light-value, dark-value)`. Single colors retain the automatic dark-appearance conversion. `inherit` preserves the original appearance where supported.

## Additional properties

| Selector or property | Fei Notes behavior |
| --- | --- |
| `:root --list-gap` | Independent list-item spacing in the Craft base style; defaults to paragraph spacing |
| `:root --todo-gap` | Spacing between consecutive to-do items; adjacent ordinary list items use the average of both values |
| `:root --countdown-muted: true` | Uses the countdown color as a pale background while retaining document text colors |
| `:root --code-compact: true` | Reduces padding for untitled code blocks; controls remain above the block unless `--code-tools-inside` is enabled |
| `:root --compact-code-gap-after-paragraph` | Space between a paragraph and a compact untitled code block; minimum 15 pt with external controls, nonnegative with internal controls |
| `:root --code-tools-inside: true` | Keeps the fold control and a compact actions entry inside the code panel; language, wrapping and copy reuse the existing menus |
| `pre` | Adds `padding-top` and `padding-left` for compact untitled code blocks |
| `ul list-style-type` | `disc` uses level-dependent markers; `dash` uses a consistent dash |
| `hr height` | Controls the divider's occupied height |
| `table` / `td` | Adds `padding-top`, `padding-left` and `margin-top` for symmetric cell padding and spacing above tables |
| `.attachment` | Card background, border, radius and file-card width; `margin-top` / `margin-bottom` set outer spacing and `row-gap` separates adjacent files/images |
| `.sidebar` | `background-color` and ordinary-row text `color` |
| `.sidebar-section` / `.sidebar-folder` | Section-heading and folder-icon `color` |
| `.sidebar-row:hover` / `.sidebar-row.is-selected` | Row `background-color`; selected rows also accept `font-weight` |
| `.sidebar-row.is-selected.is-focused` | Note-row `background-color` while navigation has focus in an active window |
| `.sidebar-folder.is-selected.is-focused` | Focused folder `background-color` and text/icon `color` |
| `.toolbar` | Window-toolbar `background-color` |
| `.toolbar-button` | `background-color`, `border-color`, `color` and `border-radius` for toolbar buttons and search |
| `.toolbar-button:hover` / `.toolbar-button:active` | Button `background-color` |
| `.toolbar-group` | Shared capsule `background-color` and `border-color` for adjacent sharing and more actions |
| `.page.is-wide` | Optional `max-width` for the Wide reading preset; Full remains unbounded |
| `.block-icon` | Optional `color` for block-type indicators in handles and insertion menus; absent values retain the original category colors |
| `.page` | Adds `padding-left` (symmetric minimum side inset, clamped to 24–120 pt) and `padding-top` (title inset) |
| `h2 + p` | Optional `margin-top` after a second-level heading; other block transitions keep their existing spacing |
| `.title` | Adds `margin-bottom` for the gap before the first body block |

Shell properties are optional. Their borders and corner radii affect painting while preserving control sizes and hit areas. CSS does not change block structure or table editing controls. Consult `src/theme.rs` and the built-in CSS files for the implementation.

Navigation emphasis tracks the last content-region click and subsequent editor keyboard input without moving the existing keyboard focus or caret. Focused colors apply only while the window is active; editing and inactive windows retain the neutral selection. Folder and document rows share the existing tree, not an additional note-list pane. Toolbar groups paint behind existing hit areas and preserve their order and sizes. Themes without group colors retain independent control surfaces.

The Notes insets follow native Notes 4.13 screenshots captured on 2026-11-16: 28 pt minimum right inset and a 66 pt minimum left editing gutter for the existing block handle and fold control, 36 pt above the title, no additional title-to-body gap, and no extra gap after a second-level heading. The Standard column allows up to 1200 pt, Wide up to 1400 pt; explicit full-width settings and pinned outlines still use the existing geometry rules. The title hover actions sit above the title with a 4 pt gap and follow the configured top inset. Task-type indicators follow the document’s circular or square task style. Interaction surfaces and selections resolve the active theme in both appearances. These measurements include neither the native date label nor a new toolbar row.
## Native rich-text compatibility

The macOS clipboard reader prefers HTML and falls back to AppKit's RTF/RTFD decoder. Local tag/class stylesheet rules emitted by AppKit are resolved before importing run marks. No external stylesheet is loaded. Native default ink follows the destination appearance; imported highlight pairs retain their colours when legible and receive a contrasting foreground otherwise.

Imported colours outside the named palette use bounded `#rrggbb` or `#rrggbbaa` values in the existing colour-name attributes. Named colours keep their previous behaviour. HTML soft breaks remain inside their paragraph, and preformatted text imports as a code block even without a nested `code` tag.

Bullets store a per-item dash flag. Existing `<li>` elements remain round bullets; `<li list-style-type="dash">` keeps a dash through editing, SQLite materialization, cloud payloads and DocxXML round trips. Markdown writes `*` for round bullets and `-` for dashes. Older builds can read the text but ignore the new marker attribute; use an updated build on each editing device to retain that distinction.

The Notes theme keeps consecutive paragraphs and headings compact. `h2 + ul { margin-top }` controls the gap from an H2 to any list, including tasks, separately from `h2 + p`. It does not change the document title's existing spacing.

## Notes presentation refinement — 2026-11-17

Native user captures in the local visual-polish acceptance record measure the
link/checklist accent as sRGB `#fcb827` and file-card fill as `#e5e6e9`.
Notes uses that same gold for links and folder outlines, 12pt card corners,
292pt file-card width, 10pt outer media margins and 18pt between adjacent media.
File cards preserve Fei Notes icons, names, metadata and preview actions.

Compact code keeps its 3.5pt vertical padding and uses 4pt outer gaps. Fold and
actions occupy the panel's top-right corner, with 64pt reserved on the right of
code text. The actions popover retains language selection, wrapping and copy.
Folded captions start 36pt from the left, clear of the 24pt fold target; even a
single empty line keeps a 32pt minimum panel height.

Circular tasks use a 17pt control and original, taller check artwork. Mobile
circular controls scale with the rendered body size, preserving round corners
and glyph proportions rather than retaining a fixed desktop-size control.
Other task styles retain their prior artwork and geometry.

Code actions reuse the document menu's public `ui::popover`, `ui::menu_row`
and label primitives, including 13pt labels, 20pt line height and 32pt rows.
Folded arrows, captions and actions share the panel's vertical center instead
of independently positioning the arrow above the caption.

The references footer uses the active body font and caps its existing text
sizes and line heights at the document's body metrics. Notes reference titles
therefore use its 13pt body size rather than the legacy 15pt document-link size.
