# Fei Notes theme extensions

Read the preserved upstream [theme reference](THEMES.md) first. This document describes the additional properties implemented by Fei Notes. It supplements the upstream guide rather than replacing it.

## Built-in themes and appearance

Fei Notes offers two themes: Notes and Feishu. Older selections of Craft, Notion, Obsidian or custom themes fall back to Notes; their CSS files remain on disk. Existing Notes and Feishu selections and their CSS overrides remain supported. Theme CSS can style the document and selected shell controls; menu layout, settings layout and interaction behavior remain controlled by the existing UI.

Color properties and `--token-*` accept `light-dark(light-value, dark-value)`. Single colors retain the automatic dark-appearance conversion. `inherit` preserves the original appearance where supported.

## Additional properties

| Selector or property | Fei Notes behavior |
| --- | --- |
| `:root --list-gap` | Independent list-item spacing in the Craft base style; defaults to paragraph spacing |
| `:root --todo-gap` | Spacing between consecutive to-do items; adjacent ordinary list items use the average of both values |
| `:root --countdown-muted: true` | Uses the countdown color as a pale background while retaining document text colors |
| `:root --code-compact: true` | Reduces padding for untitled code blocks; controls remain above the block during hover and editing |
| `:root --compact-code-gap-after-paragraph` | Space between a paragraph and a compact untitled code block; minimum 15 pt |
| `pre` | Adds `padding-top` and `padding-left` for compact untitled code blocks |
| `ul list-style-type` | `disc` uses level-dependent markers; `dash` uses a consistent dash |
| `hr height` | Controls the divider's occupied height |
| `table` / `td` | Adds `padding-top`, `padding-left` and `margin-top` for symmetric cell padding and spacing above tables |
| `.attachment` | `background-color` and `border-color` for attachment cards, preview bars and audio controls |
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
| `.page` | Adds `padding-left` (symmetric minimum side inset, clamped to 24–120 pt) and `padding-top` (title inset) |
| `h2 + p` | Optional `margin-top` after a second-level heading; other block transitions keep their existing spacing |
| `.title` | Adds `margin-bottom` for the gap before the first body block |

Shell properties are optional. Their borders and corner radii affect painting while preserving control sizes and hit areas. CSS does not change block structure or table editing controls. Consult `src/theme.rs` and the built-in CSS files for the implementation.

Navigation emphasis tracks the last content-region click and subsequent editor keyboard input without moving the existing keyboard focus or caret. Focused colors apply only while the window is active; editing and inactive windows retain the neutral selection. Folder and document rows share the existing tree, not an additional note-list pane. Toolbar groups paint behind existing hit areas and preserve their order and sizes. Themes without group colors retain independent control surfaces.

The Notes insets follow native Notes 4.13 screenshots captured on 2026-11-16: 28 pt minimum side inset, 36 pt above the title, no additional title-to-body gap, and no extra gap after a second-level heading. The Standard column allows up to 1200 pt, Wide up to 1400 pt; explicit full-width settings and pinned outlines still use the existing geometry rules. These measurements include neither the native date label nor a new toolbar row.
