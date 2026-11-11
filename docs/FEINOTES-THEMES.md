# Fei Notes theme extensions

Read the preserved upstream [theme reference](THEMES.md) first. This document describes the additional properties implemented by Fei Notes. It supplements the upstream guide rather than replacing it.

## Built-in themes and appearance

Fei Notes adds the Notes theme after Feishu, Craft, Notion and Obsidian. Existing selections remain unchanged. Theme CSS can style the document and selected shell controls; menu layout, settings layout and interaction behavior remain controlled by the existing UI.

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
| `.sidebar-row:hover` / `.sidebar-row.is-selected` | Row `background-color` |
| `.toolbar` | Window-toolbar `background-color` |
| `.toolbar-button` | `background-color`, `border-color`, `color` and `border-radius` for toolbar buttons and search |
| `.toolbar-button:hover` / `.toolbar-button:active` | Button `background-color` |

Shell properties are optional. Their borders and corner radii affect painting while preserving control sizes and hit areas. CSS does not change block structure or table editing controls. Consult `src/theme.rs` and the built-in CSS files for the implementation.
