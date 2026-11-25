<h1 align="center">Quiet.md</h1>

<p align="center">
  <b>Fast, quiet notes that live on your computer.</b><br>
  A native notes app built with Rust + GPUI: real folders, Markdown that formats itself, and a whiteboard inside your notes. Free and open source.
</p>

<p align="center">
  English · <a href="README.zh-CN.md">简体中文</a> · <a href="README.zh-TW.md">繁體中文</a> · <a href="README.ja.md">日本語</a> · <a href="README.ko.md">한국어</a> · <a href="README.es.md">Español</a> · <a href="README.fr.md">Français</a>
</p>

<p align="center">
  <a href="https://github.com/s1dashu/quiet-md/releases/latest"><img src="https://img.shields.io/github/v/release/s1dashu/quiet-md?label=release&color=2f6feb" alt="Latest release"></a>
  <a href="https://github.com/s1dashu/quiet-md/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/s1dashu/quiet-md/ci.yml?branch=main&label=CI" alt="CI status"></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey" alt="macOS, Windows and Linux">
  <img src="https://img.shields.io/badge/built%20with-Rust%20%2B%20GPUI-dea584?logo=rust" alt="Built with Rust and GPUI">
  <a href="LICENSE"><img src="https://img.shields.io/github/license/s1dashu/quiet-md?color=green" alt="MIT license"></a>
  <img src="https://img.shields.io/badge/status-beta-orange" alt="Beta">
</p>

<p align="center">
  <a href="https://download.quiet.md/latest/quiet-md-macos-arm64.dmg"><img src="docs/images/download-macos.svg" height="56" alt="Download for macOS"></a>
  &nbsp;
  <a href="https://download.quiet.md/latest/quiet-md-windows-x64-setup.exe"><img src="docs/images/download-windows.svg" height="56" alt="Download for Windows"></a>
  &nbsp;
  <a href="https://download.quiet.md/latest/quiet-md-x86_64.AppImage"><img src="docs/images/download-linux.svg" height="56" alt="Download for Linux"></a>
</p>

<p align="center">
  <a href="https://quiet.md/?lang=en"><b>Website</b></a> ·
  <a href="#download"><b>Download</b></a> ·
  <a href="#features">Features</a> ·
  <a href="#performance">Performance</a> ·
  <a href="CONTRIBUTING.md">Contributing</a>
</p>

<p align="center">
  <img src="docs/images/readme-home.webp" width="100%" alt="Quiet.md on a pixel-art meadow: the sidebar lists folders, and Home shows each folder as a stack of note cards with pictures.">
</p>

> [!NOTE]
> Quiet.md is in **beta** and changing quickly. If you hit a bug or have an idea, please [open an issue](https://github.com/s1dashu/quiet-md/issues) — or better, send a pull request.

## Why Quiet.md

**📁 Your notes are just files.** No account, no cloud, and nothing goes over the network by default, apart from the optional update check, the `quiet` command line tool's anonymous daily usage counts (`quiet telemetry off`), and font downloads you cause: the handwriting font for Chinese, Japanese and Korean the first time a whiteboard needs it, and a downloadable document font when you pick one. Every folder in the sidebar is a real folder on your disk, and every note is a file named after its title: `.md` for Markdown, `.xml` for rich documents. Sync them with iCloud, Git or anything you like. If you ever stop using Quiet.md, your files stay right where they are.

**⚡ Native, and fast.** The whole interface is drawn on the GPU by Rust and [GPUI](https://www.gpui.rs/). No Electron, no WebView. In a one-million-character note, typing takes about 5 ms a frame, and a 120 Hz display allows 8.3.

**✍️ Write and draw in one place.** Type `#`, `-`, `[]` or `>` and Markdown formats itself. When words are not enough, put a whiteboard right in the note. Rich notes are stored in the Feishu (Lark) DocxXML format, so pasting from Feishu or a web page keeps its structure.

## Features

### Write

<p align="center">
  <img src="docs/images/readme-write.webp" width="100%" alt="A long note open in Quiet.md, with headings, inline math and a table of ion concentrations.">
</p>

Headings, lists, to-dos, quotes, callouts, columns, tables with merged cells, code with syntax highlighting, LaTeX math, images, files and covers. Markdown shortcuts work as you type, and the `/` menu has everything else. Mention another note to link it; the other note lists its backlinks.

### Draw

<p align="center">
  <img src="docs/images/readme-board.webp" width="100%" alt="A board in Quiet.md: pictures and maps laid out with handwritten labels, boxes and connecting arrows.">
</p>

Boards are a whiteboard ported from Excalidraw: shapes, arrows that stay attached, freehand, text, sticky notes, frames and pictures. Put one inside a note, or keep it as an `.excalidraw` file that excalidraw.com opens. Zoom with a trackpad pinch; a board with 10,000 shapes stays smooth.

### Organize in real folders

Home shows each folder as a stack of its pages. Hold a note over a folder and it opens for you; folders go inside folders, and moves can be undone. Pin and favorite notes, search every title and line with `⌘ J`, and find and replace within a note, with regular expressions if you like. Deleted notes wait in the Trash. Markdown files and folders elsewhere on your computer can be opened under **Files on this computer** and edited where they are.

### Bring your notes

<p align="center">
  <img src="docs/images/readme-import.webp" width="100%" alt="The first-launch screen of Quiet.md, offering to import an Obsidian vault it found.">
</p>

On first launch, Quiet.md looks for your Obsidian, Logseq, iA Writer and Typora libraries and copies them in, leaving the originals alone. Obsidian `[[wikilinks]]` keep working: click one to open the note it points to. Later, drag Markdown files and folders onto Home or straight into a sidebar folder. Notes go out again as Markdown or PDF, or straight to the printer.

### Work with agents

The `quiet` command line tool lets an AI agent (Claude Code, Codex, or any MCP client) read, search and edit the same library you write in. It finds blocks by outline, edits through the same engine as the app, rewrites links when notes move, adds rows to data tables and draws on boards, and refuses to overwrite what you changed since it read. A note you have open takes in the agent's edit as it lands, every save keeps the previous text as a version, and `quiet run` takes a whole JavaScript script as one change that is written in full or not at all. `quiet skill install` gives Claude Code and Codex a skill, so they reach for `quiet` whenever you ask about your notes; clients without a shell use `quiet mcp`. `quiet` counts which of its commands run each day, under a random id and never with your notes, and sends the counts once a day; `quiet telemetry off`, `DO_NOT_TRACK=1` or turning off the app's automatic update check stops it.

`quiet` ships inside the app. On macOS, put it on your PATH with `ln -s /Applications/quiet.md.app/Contents/MacOS/quiet /usr/local/bin/quiet`; on Windows it sits beside `quiet_md.exe` in the install folder. See [docs/design/CLI.md](docs/design/CLI.md).

### And more

- **Version history**: every note keeps snapshots as it saves (20 by default, up to 30). Open one to see what changed in place, restore it in one click, or give it a name.
- **Quick notes**: on macOS, a global shortcut (`⌥ Space` by default) opens a small note window over any app.
- **Your look**: four built-in styles — Craft, Feishu, Notion and Obsidian — each with a dark look worked out from its colours, or your own CSS ([docs/THEMES.md](docs/THEMES.md)). Choose the interface and document fonts.
- **Your keys**: press `⌘ /` to see every shortcut, and rebind any of them. On Windows and Linux, Ctrl stands in for ⌘ and Alt for ⌥.
- **Your language**: English, Simplified Chinese, Traditional Chinese, Japanese, Korean, Spanish, French, Arabic and Turkish. Arabic text joins its letters and runs right to left.
- **Housekeeping**: Settings › Documents finds images and files no note uses any more and moves them out once you confirm.
- **Updates**: new versions install from inside the app after their signature is verified. Automatic checks can be turned off.

## Download

<p align="center">
  <a href="https://download.quiet.md/latest/quiet-md-macos-arm64.dmg"><img src="docs/images/download-macos.svg" height="56" alt="Download for macOS"></a>
  &nbsp;
  <a href="https://download.quiet.md/latest/quiet-md-windows-x64-setup.exe"><img src="docs/images/download-windows.svg" height="56" alt="Download for Windows"></a>
  &nbsp;
  <a href="https://download.quiet.md/latest/quiet-md-x86_64.AppImage"><img src="docs/images/download-linux.svg" height="56" alt="Download for Linux"></a>
</p>

| Platform | Package | Requirements |
|---|---|---|
| **macOS** (Apple Silicon) | [quiet-md-macos-arm64.dmg](https://download.quiet.md/latest/quiet-md-macos-arm64.dmg) | macOS 13 or later |
| **Windows** (x64) | [quiet-md-windows-x64-setup.exe](https://download.quiet.md/latest/quiet-md-windows-x64-setup.exe) | Windows 10 version 1809 or later |
| **Linux** (x86_64, experimental) | [quiet-md-x86_64.AppImage](https://download.quiet.md/latest/quiet-md-x86_64.AppImage) | glibc 2.35 or later (Ubuntu 22.04 and newer) |
| **macOS** (Intel) | [quiet-md-macos-x86_64.dmg](https://download.quiet.md/latest/quiet-md-macos-x86_64.dmg) | macOS 13 or later |

- **macOS**: open the `.dmg` and drag `quiet.md.app` onto the Applications shortcut beside it, then open it from Applications. The app and the disk image are signed and notarized by Apple. The `.zip` on the Releases page holds the same app; it is what in-app updates download.
- **Windows**: run the installer. By default it installs for the current user only, and uninstalling never deletes your notes. If SmartScreen appears on first launch, click **More info → Run anyway**.
- **Linux** (experimental): `chmod +x quiet-md-*.AppImage` and run it; without FUSE, add `--appimage-extract-and-run`. AppImage 0.1.10 and later support in-app updates; older Linux versions need a one-time manual install. Interactive testing so far covers Omarchy on Wayland; see the [Linux notes](CONTRIBUTING.md#linux-experimental).

You can also download from [Quiet.md](https://quiet.md/?lang=en). Every release and its SHA-256 checksums are on [GitHub Releases](https://github.com/s1dashu/quiet-md/releases).

On first launch, Quiet.md asks for the interface language, where to keep your library and the quick-note shortcut, and offers to copy in notes from Obsidian, Logseq, iA Writer or Typora. An empty library gets a **Getting started** guide — a ten-minute tour of the main features. Existing notes are never overwritten.

## Performance

Frame processing time over 100 consecutive operations in a mixed document of **1 million characters** and 549 chapters:

| Operation | P50 | P95 |
|---|---:|---:|
| ⌨️ Typing | **5.06 ms** | 5.78 ms |
| ⌫ Backspace | **4.99 ms** | 5.55 ms |
| 🖱️ Scrolling | **2.08 ms** | 4.14 ms |

A 120 Hz display gives each frame 8.3 ms. Even at P95 on a million-character document, there is headroom left.

| | |
|---|---|
| macOS download | ~18 MB |
| Windows installer | ~14 MB |
| Runtime | No WebView, no Electron, no background services |

<sub>Apple M5 Pro / 48 GiB, macOS 26.5, release build. Methodology and scripts to reproduce: [docs/PERFORMANCE.md](docs/PERFORMANCE.md).</sub>

## Contributors

Thanks to everyone who has helped build Quiet.md. Issues and pull requests are welcome: building, testing and code conventions are in [CONTRIBUTING.md](CONTRIBUTING.md).

[![Contributors](https://raw.githubusercontent.com/s1dashu/quiet-md/contributors-assets/docs/images/contributors.svg?v=1)](https://github.com/s1dashu/quiet-md/graphs/contributors)

## Recent updates

- **0.1.14** (Nov 22): find and replace with regular expressions. Obsidian `[[wikilinks]]` open the note they point to. A Turkish interface, and Arabic laid out right to left. Choose the interface and document fonts, and clear out images and files no note uses. Search keeps up with large libraries, and deleting many notes is quicker. A setting for whether Backspace returns to the title; on Windows and Linux, Copy as Markdown is now Ctrl+Shift+M.
- **0.1.13** (Nov 17): boards — a whiteboard ported from Excalidraw, as a block in a note or a standalone `.excalidraw` file, fast with 10,000 shapes. Version history for every note (… › Version history). French and Arabic interfaces. Copy as Markdown, copy a file's path, and switch the library folder in Settings. Pasted web images are saved into the library. Less memory on picture-heavy pages and smoother sidebar scrolling.
- **0.1.12** (Nov 14): drag files from Finder, or a file from Files on this computer, straight into a sidebar folder or between rows; the whole page lights up as a drop target. Redesigned Home cards with squircle corners, an Open in Finder menu, breadcrumbs that start at My documents, a minimum window width that keeps room for the note, and a solid sidebar in full screen. Fixes a hang when creating a document in a folder.
- **0.1.11** (Nov 13): the library is now real folders, with files named by title; older libraries move over automatically, with a backup. A new first launch that sets the language, library location and quick-note shortcut and imports notes from Obsidian and other Markdown apps. Japanese, Spanish and Korean interfaces; display math in Markdown; documents reopen where you left off; faster launch and lower memory use.
- **0.1.10** (Nov 10): choose a document type from the New menu, scroll wide tables horizontally, preserve Markdown column alignment, and keep the caret visible in long fields.
- **0.1.9** (Nov 8): fix Cmd+Q on macOS, unresponsive window controls on Windows, and text selection hidden by highlight colors.
- **0.1.8** (Nov 9): edit local Markdown files in place, import folders while keeping `.md`, and organize notes in nested folders with drag and undo. Lower image and preview memory use; steadier table resizing and refined menus. First launch chooses a supported system language. Experimental Linux AppImage packaging.
- **0.1.7** (Nov 6): safer multi-window saving, conflict recovery and disk writes; restricted export file access; Markdown exports keep inline attachments. Fixed Chinese input deletion with Weasel on Windows.
- **0.1.6** (Nov 6): smoother window switching and typing, wider tables, regular saves during continuous typing, and Option+Space for global quick notes on macOS. Windows adds image paste, PDF export and printing, with corrected word deletion.
- **0.1.5** (Nov 5): the app is now called Quiet.md; your library and notes stay where they were.
  - Manage several documents at once from Home: duplicate, move or delete them together.
  - Empty the Trash in one step; arrow keys move through the insert menu's icons; deleting every document no longer creates an empty one.
  - After an in-app update, the sidebar shows once what changed in the new version.
  - Windows: shortcuts and the built-in guide read Ctrl / Alt, and a failed launch explains itself and leaves a log.
- **0.1.4** (Nov 5):
  - In-app updates: when a new version is out, an **Update** button appears at the top of the sidebar. The download is verified against its signature, and a restart finishes the install; open documents are saved first. Automatic checks can be turned off in **Settings › General**. A check only fetches a version manifest and sends no data. Version 0.1.3 and earlier need a one-time manual download of 0.1.4.
  - Dark mode: choose System, Light or Dark in **Settings › Appearance**. Built-in and custom themes all have a dark look; export and printing stay light.
- **0.1.3** (Nov 4): sidebar text is readable again when macOS is in dark mode.
- **0.1.2** (Nov 4): the standard macOS menus and shortcuts (Hide, Minimize, Zoom, Full Screen); clicking the Dock icon reopens a window after the last one was closed. Fixed some Chinese characters rendering with the wrong glyphs.
- **0.1.1** (Nov 3): first Windows x64 release, installed per user, keeping your notes on uninstall. Polished the Chinese and English interface and the built-in guide.

Full notes for each version are on [Releases](https://github.com/s1dashu/quiet-md/releases).

## License

The code is licensed under [MIT](LICENSE). Third-party code, fonts and assets are listed in [THIRD_PARTY.md](THIRD_PARTY.md).

The following are not covered by the MIT license and should be replaced in forks:

- The Nucleo icons in `assets/icons/` (69) and `assets/editor-icons/` (24) are under a commercial Nucleo license. Do not extract or redistribute them on their own.
- The name "Quiet.md" and the app icon.
