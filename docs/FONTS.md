# Downloadable fonts

Some fonts are not in the packages. `src/font_downloads.rs` fetches them when they are first needed, from `https://download.quiet.md/fonts/`. `packaging/fonts.json` lists them; the app compiles it in.

- **Xiaolai**, the handwriting face boards draw Chinese, Japanese and Korean with (14 MB). It is fetched the first time a board needs it: an Excalifont text with CJK characters Excalifont lacks, or a copy as SVG / PNG of such text. Until then boards draw CJK with the system fonts. A note at the foot of the board shows the download's progress, then says it is ready (`src/board/font_hint.rs`). A failed download says so once per run, with Retry.
- **Document fonts** in Settings › Appearance › Document font, under the system's fonts: 霞鹜文楷 (LXGW WenKai, Regular and Medium), 朱雀仿宋 (Zhuque Fangsong) and Xiaolai. Each shows its size; picking one downloads it with progress in the list, then makes it the document font. A failed download leaves the font in use and says why. A downloaded font has a delete button. Deleting the document font in use goes back to the theme's. Deleting Xiaolai leaves the boards open now as they are; boards fetch it again when they need it.
- Nothing is fetched before it is needed, and nothing is sent besides the request for the file.

## The font lists

Settings › Appearance › Document font and Interface font share one list (`settings::font_choices`): the reset, 收藏 (starred, shared by both lists) and 最近使用 (the last 5 picked for that list), then a section per class: 黑体, 宋体, 楷体·手写, 圆体, 等宽, 西文衬线, 西文无衬线, 其他. The CJK sections come first in Chinese, Japanese and Korean interfaces, the Latin ones otherwise; within a section, faces that suit the interface language lead (full simplified Chinese for 简体中文). Symbol faces and faces made for another script are folded under 其他语言和符号. `src/font_classes.rs` reads what each installed family is from its file (fontdb's scan, about 0.2 s off the main thread when the list first opens): coverage of 通用规范汉字表's first-level characters, monospace from `post` or the ASCII advances, serif or sans from OS/2 `sFamilyClass` and PANOSE, else from the names; `KNOWN` fixes common system families those get wrong. Each row draws a sample in its face; hovering or arrowing to a row shows the page in it after a short pause, Return or a click saves it, Esc or a click outside puts the saved font back.

## Trust and cache

- Each file's name carries the first 12 hex digits of its SHA-256; the catalog holds the full hash and size, and a file that does not match is discarded. Files are fetched with the same `curl` call as update packages (HTTPS only including redirects, the system's proxy settings).
- Verified files are kept in the app's data folder (`~/Library/Application Support/quiet.md/fonts` on macOS, `%LOCALAPPDATA%\quiet.md\fonts` on Windows, `~/.local/share/quiet.md/fonts` on Linux), never in the library. The board and the document fonts share it. Files are checked again when loaded; a cached file that no longer verifies is deleted.
- At launch a downloaded document font is read from the cache. If the cache was deleted or damaged it is not fetched again on its own: the default face stands in, the preference stays, and the Document font row says it can be downloaded again (as for an uninstalled system font).
- A failed fetch is silent and tried again only when needed, after 15 seconds, doubling up to an hour. A copy as picture waits up to 8 seconds for Xiaolai, then uses the system fonts and says so.
- Exports (Markdown, HTML, PDF, print) use their own stylesheet and never followed the document font; a downloaded one changes nothing there. Board SVG / PNG copies embed Xiaolai when it is here.

## Trial fonts

Entries marked `trial` are being tried out. Release builds leave them out of the list (`TRIAL_FONTS` in `src/font_downloads.rs`); preview builds (`scripts/preview.sh`) and `QUIET_MD_TRIAL_FONTS=1` list them. Chinese fonts in the catalog cover simplified Chinese in full (see below). `"group"` is `cjk`, `latin` (labelled 西文; CJK falls back to the system font) or `mono` (等宽).

## Adding or updating a font

1. Add or edit its entry in `packaging/fonts.json`: `id`, `family` (as the font's name table says), `zh` / `en` names, `group`, `trial`, provenance (`version`, `license`, `home`, `license_from`) and `files` with `stem` and `from` (a URL, `URL#member` of a zip, or `repo:<path>`).
2. `python3 tools/fonts/stage.py --fill` builds the files from upstream and writes `name`, `sha256` and `size`. With fontTools installed it also checks the family name.
3. A Chinese font (`"group": "cjk"`) must cover all 3500 first-level characters of 通用规范汉字表: `stage.py` refuses it otherwise (with fontTools installed; `python3 tools/fonts/coverage.py <file>` shows the coverage and the missing characters).
4. A font offered to everyone (not `trial`) goes in THIRD_PARTY.md.

## Release order

`scripts/publish-downloads.sh --fonts` builds the files with `tools/fonts/stage.py --official` and uploads each one and its license (`fonts/<id>-LICENSE.txt`) unless already served; `DRY_RUN=1` prints the uploads. `FONTS='id …'` publishes just those (trial fonts included). Publishing packages runs this step first: copies of a new version must find their fonts.

## Testing

- `QUIET_MD_FONT_URL` points at another folder (http allowed; a compiled-in Xiaolai is then ignored), `QUIET_MD_FONT_DIR` replaces the cache folder. `--features embed-xiaolai` (on with `devtools`) compiles Xiaolai in, for tests and harness screenshots.
- `python3 tools/fonts/stage.py --serve 8766` builds every file into `/tmp/quiet-md-fonts` and serves it, for a build started with `QUIET_MD_FONT_URL=http://127.0.0.1:8766/`.
- `python3 tools/harness/board_font.py` and `python3 tools/harness/doc_fonts.py` check the board and document fonts end to end; `doc_fonts.py --compare` shoots an article in every catalog font.
