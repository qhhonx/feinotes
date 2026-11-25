> This inventory describes upstream Quiet.md assets. Fei Notes replaces the interface shell icons with Lucide or original artwork. See [THIRD_PARTY.md](../THIRD_PARTY.md) for the product inventory and licenses.

# Icon inventory

Where every interface icon comes from, for the licence summary in [THIRD_PARTY.md](../THIRD_PARTY.md). Each SVG also names its source in a comment on its first line, except the Lucide and project-drawn editor icons, which follow the conventions below.

## `assets/icons/`: 71 Nucleo icons

Nucleo UI Outline 18, UI Fill 18 and UI Essential Outline 18, under the project’s commercial Nucleo licence (up to 100 icons in an open-source project). 71 files draw on 63 distinct Nucleo icons: several names reuse one icon (for example `code` and `braces` are both Code, `doc` and `file-text` both FileContent). They are excluded from the MIT licence and may not be extracted into an icon set.

Checked 2026-11-24 against the official packages `nucleo-ui-outline-18` 1.5.0 and `nucleo-ui-essential-outline-18` 1.1.8 by comparing the drawn shapes.

| File | Set | Nucleo icon | Check |
|---|---|---|---|
| `arrow-left.svg` | UI Outline 18 | ArrowLeft | identical to the package |
| `batch.svg` | UI Outline 18 | CheckList | identical to the package |
| `bin.svg` | UI Outline 18 | Trash 2 | Trash 2 as exported from the Nucleo app; matches an earlier Nucleo export exactly; the 1.5.0 package draws Trash2 slightly differently. |
| `bold.svg` | UI Outline 18 | TextBold | identical to the package |
| `board.svg` | UI Outline 18 | Palette2 | Taken from the 1.5.0 package as is; the glyph of a board (`.excalidraw`) file. |
| `braces.svg` | UI Outline 18 | Code | identical to the package |
| `check.svg` | UI Outline 18 | Check | identical to the package |
| `chevron-down.svg` | UI Outline 18 | ChevronDown | identical to the package |
| `chevron-left.svg` | UI Outline 18 | ChevronLeft | Same polyline as ChevronLeft, points listed in reverse order. |
| `chevron-right.svg` | UI Outline 18 | ChevronRight | identical to the package |
| `circle-filled.svg` | UI Outline 18 | ShapeCircle | identical to the package |
| `code.svg` | UI Outline 18 | Code | identical to the package |
| `copy.svg` | UI Outline 18 | Copy | identical to the package |
| `divider.svg` | UI Outline 18 | Minus | identical to the package |
| `doc.svg` | UI Outline 18 | FileContent | identical to the package |
| `doc-md.svg` | UI Outline 18 | FileContent | FileContent with MD lettering added by the project. |
| `ellipsis.svg` | UI Outline 18 | Dots | identical to the package |
| `expand.svg` | UI Outline 18 | Expand | Expand, the same four corners in absolute path notation. |
| `export.svg` | UI Outline 18 | FileExport | identical to the package |
| `eye.svg` | UI Outline 18 | Eye | identical to the package |
| `eye-open.svg` | UI Outline 18 | Eye | identical to the package |
| `file-text.svg` | UI Outline 18 | FileContent | identical to the package |
| `folder-input.svg` | UI Outline 18 | FolderDownload | identical to the package |
| `folder-plus.svg` | UI Outline 18 | FolderPlus | identical to the package (taken from `nucleo-ui-outline-18` 1.5.0, 2026-11-25) |
| `heading-1.svg` | UI Outline 18 | Heading1 | identical to the package |
| `heading-2.svg` | UI Outline 18 | Heading2 | identical to the package |
| `heading-3.svg` | UI Outline 18 | Heading3 | identical to the package |
| `home.svg` | UI Outline 18 | House | identical to the package |
| `image-broken.svg` | UI Outline 18 | Image | identical to the package |
| `indent-decrease.svg` | UI Outline 18 | TextOutdentLeft | identical to the package |
| `indent-increase.svg` | UI Outline 18 | TextOutdentRight | identical to the package |
| `info.svg` | UI Outline 18 | CircleInfo | identical to the package |
| `info-mark.svg` | UI Outline 18 | CircleInfo | CircleInfo without its circle: the “i” drawn on top of `circle-filled`. |
| `italic.svg` | UI Outline 18 | TextItalic | identical to the package |
| `keyboard.svg` | UI Outline 18 | Keyboard | identical to the package |
| `library.svg` | UI Outline 18 | Books | identical to the package |
| `list.svg` | UI Outline 18 | BulletList | identical to the package |
| `list-ordered.svg` | UI Outline 18 | OrderedList | identical to the package |
| `list-todo.svg` | UI Outline 18 | ListTodo | identical to the package |
| `minus.svg` | UI Outline 18 | Minus | Minus, the same line written as a path. |
| `page-width.svg` | UI Outline 18 | ArrowsExpandX | identical to the package |
| `palette.svg` | UI Outline 18 | ColorPalette | identical to the package |
| `panel-left-close.svg` | UI Outline 18 | SidebarLeftHide | identical to the package |
| `panel-left-open.svg` | UI Outline 18 | SidebarLeftShow | identical to the package |
| `pen-line.svg` | UI Outline 18 | Pen | identical to the package |
| `pin.svg` | UI Outline 18 | PinTack | identical to the package |
| `pin-filled.svg` | UI Fill 18 | PinTack | identical to the package |
| `pin-off.svg` | UI Outline 18 | PinTackSlash | identical to the package |
| `plus.svg` | UI Outline 18 | Plus | identical to the package |
| `printer.svg` | UI Outline 18 | Print | identical to the package |
| `quote.svg` | UI Outline 18 | Quote | identical to the package |
| `restore.svg` | UI Outline 18 | ArrowRotateAnticlockwise | identical to the package |
| `search.svg` | UI Outline 18 | Magnifier | identical to the package |
| `search-clear.svg` | UI Outline 18 | CircleXmark | identical to the package |
| `settings-gear.svg` | UI Outline 18 | Gear | identical to the package |
| `sigma.svg` | UI Outline 18 | MathFunction | identical to the package |
| `sort-down.svg` | UI Outline 18 | SortNumDescending | identical to the package |
| `square-pen.svg` | UI Outline 18 | EditSquare | identical to the package |
| `star.svg` | UI Outline 18 | Star | identical to the package |
| `star-off.svg` | UI Outline 18 | StarSlash | identical to the package |
| `storage.svg` | UI Outline 18 | HardDrive | identical to the package |
| `strikethrough.svg` | UI Outline 18 | TextStrikethrough | identical to the package |
| `text-search.svg` | UI Outline 18 | TextASparkle | identical to the package |
| `todo-check.svg` | UI Outline 18 | Check | identical to the package |
| `tree-triangle.svg` | UI Fill 18 | CaretRight | Fill 18 CaretRight; not checked against a package (no UI Fill 18 package was available); the header and the comma path style match the other Nucleo 1.5.1 files. |
| `underline.svg` | UI Outline 18 | TextUnderline | identical to the package |
| `view-grid.svg` | UI Outline 18 | Grid2 | identical to the package |
| `view-list.svg` | UI Outline 18 | BulletList | identical to the package |
| `view-settings.svg` | UI Outline 18 | Sliders | identical to the package |
| `whiteboard.svg` | UI Essential Outline 18 | Shapes | identical to the package |
| `x.svg` | UI Outline 18 | Xmark | identical to the package |

The 17 project-drawn icons in this folder (MIT) are listed in THIRD_PARTY.md; each opens with a comment saying so.

## `assets/editor-icons/`: 174 icons

73 come from [Lucide](https://lucide.dev) (ISC, see `assets/editor-icons/LICENSE-lucide`): 67 unchanged and 6 adapted or taken from an earlier Lucide version. The other 101 were drawn by the project on the same 24 px grid with a 2 px stroke and are distributed under MIT. Checked 2026-11-24 against the shapes in `lucide-react` (several versions up to 1.70).

| File | Lucide icon |
|---|---|
| `alarm-clock.svg` | alarm-clock |
| `baseline.svg` | baseline |
| `braces.svg` | braces |
| `calendar.svg` | calendar |
| `check.svg` | check |
| `chevron-down.svg` | chevron-down |
| `chevron-left.svg` | chevron-left |
| `chevron-right.svg` | chevron-right |
| `circle-check.svg` | circle-check |
| `circle-help.svg` | circle-help |
| `circle-play.svg` | circle-play (an earlier circle-play) |
| `columns-2.svg` | columns-2 |
| `crop.svg` | crop |
| `download.svg` | download |
| `ellipsis.svg` | ellipsis |
| `emoji-foods.svg` | apple (an earlier apple) |
| `emoji-frequent.svg` | clock-9 |
| `emoji-objects.svg` | lightbulb (lightbulb with a filament added) |
| `export.svg` | log-out |
| `eye-open.svg` | eye |
| `file-text.svg` | file-text |
| `fit-page.svg` | maximize |
| `folder-input.svg` | folder-input |
| `globe.svg` | globe |
| `heading.svg` | heading |
| `history.svg` | history |
| `image.svg` | image |
| `image-reset.svg` | rotate-ccw |
| `import.svg` | import |
| `indent-decrease.svg` | list-indent-decrease |
| `indent-increase.svg` | list-indent-increase |
| `info.svg` | info |
| `layout-grid.svg` | layout-grid |
| `library.svg` | library |
| `link.svg` | link |
| `list-todo.svg` | list-todo |
| `list-tree.svg` | list-tree |
| `maximize.svg` | maximize |
| `minus.svg` | minus |
| `move.svg` | move |
| `octagon-x.svg` | octagon-x |
| `panel-left-close.svg` | panel-left-close |
| `panel-left-open.svg` | panel-left-open |
| `paperclip.svg` | paperclip |
| `pause-filled.svg` | pause |
| `pen-line.svg` | pen-line |
| `pin.svg` | pin |
| `pin-filled.svg` | pin |
| `pin-off.svg` | pin-off |
| `printer.svg` | printer |
| `redo.svg` | redo-2 |
| `restore.svg` | undo-2 |
| `rotate-ccw-square.svg` | rotate-ccw-square |
| `search.svg` | search |
| `shuffle.svg` | shuffle |
| `sigma.svg` | sigma |
| `smile.svg` | face-slightly-smiling |
| `smile-plus.svg` | smile-plus |
| `square-pen.svg` | square-pen |
| `square-plus.svg` | square-plus (an earlier square-plus) |
| `star.svg` | star |
| `star-off.svg` | star-off |
| `text-search.svg` | text-search |
| `timer.svg` | timer |
| `trash-2.svg` | trash-2 |
| `undo.svg` | undo-2 |
| `unlink.svg` | unlink |
| `upload.svg` | upload |
| `user.svg` | user |
| `wrap-text.svg` | text-wrap |
| `x.svg` | x |
| `zoom-in.svg` | zoom-in (an earlier zoom-in) |
| `zoom-out.svg` | zoom-out (an earlier zoom-out) |

Drawn by the project (101): `align-center`, `align-left`, `align-right`, `batch`, `bin`, `bold`, `button`, `button-link`, `callout`, `caption`, `caret-down`, `caret-up`, `catalogue-collapse`, `catalogue-list`, `circle-filled`, `code`, `code-fold-down`, `code-fold-right`, `column-add`, `copy`, `distribute-cols`, `divider`, `doc`, `doc-blue`, `doc-blue-lines`, `doc-info`, `emoji-activity`, `emoji-nature`, `emoji-people`, `emoji-places`, `emoji-symbols`, `equation-error`, `eye`, `file-fold`, `file-g-audio`, `file-g-code`, `file-g-excel`, `file-g-image`, `file-g-pdf`, `file-g-ppt`, `file-g-text`, `file-g-video`, `file-g-word`, `file-g-zip`, `file-page`, `grip-vertical`, `header-col`, `header-row`, `heading-1`, `heading-2`, `heading-3`, `heading-4`, `heading-5`, `heading-6`, `home`, `home-filled`, `image-broken`, `indent-align`, `info-mark`, `italic`, `keyboard`, `list`, `list-ordered`, `merge-cells`, `one-to-one`, `page-width`, `palette`, `play-filled`, `plus`, `quote`, `reminder`, `reminder-filled`, `replace`, `row-add`, `scissors`, `search-clear`, `sort-down`, `spinner`, `split-cells`, `storage`, `strikethrough`, `swatch-none`, `table`, `todo`, `todo-check`, `tree-triangle`, `triangle-down`, `triangle-right`, `type`, `underline`, `valign-bottom`, `valign-middle`, `valign-top`, `view-card`, `view-grid`, `view-list`, `view-preview`, `view-settings`, `view-text`, `volume-2`, `volume-x`.
