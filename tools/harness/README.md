# 脚本驱动（harness）

用 `FDL_SCRIPT` 回放一份逐行脚本来操作窗口，并截图。整个过程不碰用户的文档库：每次运行都先把 `lib_dir` 复制一份到 `work_dir/lib`，再用 `QUIET_MD_HOME` 指向这份副本。

```sh
cargo build --release --features devtools   # 正式构建不带脚本驱动
python3 tools/harness/run.py tools/harness/examples/scroll.txt            # 默认用 lib_clean，结果在 /tmp/fdl-run
python3 tools/harness/run.py my.txt /path/to/lib /tmp/my-run              # 自定义文档库和输出目录
FDL_BIN=/other/quiet_md python3 tools/harness/run.py my.txt
```

- 截图：`<work_dir>/<name>.png`。`shot` 是离屏渲染，没有系统玻璃和桌面，透明的侧栏铺在窗口底色（和全屏时一样）上；要拍到真实的玻璃用 `deskshot`。和改动前（侧栏为黑色）的二进制做 `compare.py` 时，侧栏区域会有差异。
- 日志：`<work_dir>/log`。性能数字用 `grep -a bench log` 查看。
- `lib_clean/` 是一个审查用的文档库，含一篇「全部 Block 样式审查」和几张示例图片。
- `winid.swift` 列出屏幕上 quiet_md 窗口的 window id（`swift winid.swift`），用来配合系统截图。

## 截图回归

```sh
python3 tools/harness/compare.py tools/harness/examples/menus.txt <旧版二进制> target/release/quiet_md
```

- 用同一份脚本分别跑两个版本，逐张逐像素比较截图。有差异时输出差异像素数和范围，并把差异图写到 `/tmp/fdl-compare/diff-<名字>.png`。
- 只移动代码、不改外观的重构（拆模块、换 token、共用组件），应当每张都是 `identical`。
- `examples/menus.txt` 依次打开「…」菜单和页宽子菜单、编辑模式菜单、斜杠菜单、块菜单、侧栏右键菜单、帮助菜单。
- `python3 tools/harness/table_resize.py` 在临时文档库中验证调整表格列宽时页面位置不跳动，覆盖屏幕外光标、单元格内光标、无变化拖拽、保存和撤销重做；需要 Pillow。
- `python3 tools/harness/table_paste.py` 用模拟剪贴板验证 Markdown 表格按内容分配初始列宽，覆盖中文、粘贴、文件导入、保存重开和撤销重做，并确认已有列宽保留。
- `python3 tools/harness/checkbox_toggle.py` 在临时文档库中验证点击任务框只切换完成状态，保留光标、选区和滚动位置，覆盖两种主题、嵌套、提示块、表格、保存和撤销重做；需要 Pillow。`CHECKBOX_CASES` 可指定逗号分隔的场景名。
- `python3 tools/harness/scroll_intent.py` 验证标题折叠/展开保留视口和文字选区、隐藏光标迁到标题后仍可见、输入/方向键仍跟随光标，以及分栏调整与两种主题代码换行的保存和位置保持；需要 Pillow。`SCROLL_CASES` 可指定逗号分隔的场景名。
- `python3 tools/harness/pointer_properties.py` 逐项验证图片对齐、附件卡片/预览、提示块颜色/图标/默认设置、倒计时、日期提醒和单元格背景/垂直对齐，核对视口、编辑位置、保存及撤销重做；需要 Pillow。`PROPERTY_CASES` 可指定逗号分隔的场景名；单独运行撤销场景时，会先验证对应基础操作确实生效。
- `python3 tools/harness/clipboard_images.py` 用模拟剪贴板验证 TIFF 截图、图片附带文字、多图、光标处插入、保存、撤销重做，以及纯文本和富文本粘贴；需要 Pillow。
- `python3 tools/harness/paste_remote_images.py` 用本机 HTTP 服务和模拟剪贴板验证从网页粘贴的网络图片下载后变成文档库自己的图片（#220），覆盖撤销重做、关闭「自动下载网络图片」时不联网、已过期链接保持原样；需要 Pillow。
- 比较时光标不闪烁（`FDL_NOBLINK`）。脚本开头要先把指针移到空白处，否则真实鼠标的位置会以 hover 的形式出现在截图里。
- 旧版二进制可以这样编：`git worktree add /tmp/old <提交>`，在那里用 `CARGO_TARGET_DIR=<本仓库>/target cargo build --release --features devtools`，再把二进制复制出来。

## 脚本命令

每行一条命令；`#` 开头的是注释。完整说明见 `src/devtools.rs` 顶部。

| 命令 | 作用 |
|---|---|
| `wait <ms>` | 等待，期间自己绘制帧（锁屏时也能出图） |
| `key <按键>` | 发送按键，如 `key cmd-b`、`key escape` |
| `type <文字>` | 输入文字（行尾空格有效） |
| `move x y` / `click x y` / `dclick x y` / `rclick x y` / `drag x1 y1 x2 y2` | 鼠标操作（窗口坐标） |
| `down x y` / `dragto x y` / `dragby dx dy` / `up x y` | 分步拖动：按下、拖到某点、从上一个拖动点挪一步、松开；`bench 30 dragby 4 0` 测拖拽每一步的耗时 |
| `mdown x y` / `mdragto x y` / `mup x y` | 中键（按下滚轮）：按下、拖到某点、松开 |
| `wheel x y dy [dx]` | 一次滚轮事件；可选的 `dx` 是横向分量（模拟触控板），正数露出右边的内容 |
| `shot <名字>` / `burst <名字> n` | 截图；连拍 n 张，间隔约 8ms |
| `mark <文字>` / `commit <文字>` | 输入法组合文字 / 提交 |
| `clip html|text|image <文件>` / `clip clear` | 设置模拟剪贴板的内容（图片可追加多张，文字和 HTML 可并存），之后的 `key cmd-v` 粘贴它；`clear` 清空模拟内容 |
| `pick <路径>[|<路径>…]` | 预先回答下一次打开 / 保存对话框，不弹出系统面板 |
| `resize w h` | 调整窗口大小 |
| `place x y` | 把窗口左上角放到主屏幕的 (x, y)，浮在其它窗口上（配合 `FDL_NOPARK` 拍桌面截图） |
| `deskshot <名字> [边距]` | 连同窗口下面的内容截图，四周多留「边距」pt（默认 48）。窗口下面先用 `shared-mock/guides/backdrop.swift` 铺一张壁纸，见 `shared-mock/guides/shots.sh` |
| `bench <n> <命令>` | 把一条命令跑 n 次，每次同步画一帧，输出耗时 |
| `record <文件.mp4>` / `record stop` / `glide` / `scroll` / `caption` | 录制演示视频 |
| `filedrag x y <路径>[\|<路径>…]` / `filedrop x y` / `fileleave` | 模拟从访达拖文件进窗口：拖到 (x, y)、在 (x, y) 松手、移出窗口 |
| `activate` | 让窗口成为 key window。**会抢焦点，一般不要用** |
| `quit` | 退出（`run.py` 会自动在末尾追加） |

环境变量：

- `FDL_SHOTS`：截图目录（`run.py` 已设好）；
- `FDL_TRACE`：输出事件跟踪；
- `FDL_IN_WINDOW_PROMPTS`：用窗口内的 GPUI 对话框回放确认按钮。
- `FDL_NOPARK`：不把窗口挪到屏幕外；
- `FDL_WINDOW_CAPTURE`：改用窗口服务器截图，截图里带系统画的红绿灯；
- `FDL_NOBLINK`：光标不闪烁（`compare.py` 已设好）。
- `FDL_RECORD_FPS`：录屏帧率，默认 30。

## 使用规则

脚本跑在用户正在使用的电脑上，必须遵守以下规则：

- 不用真实的 ⌘C / ⌘V 系统剪贴板；`key cmd-v` 只在 `clip` 之后使用。
- 不抢焦点：不用 `activate`。窗口默认被挪到屏幕外渲染。
- 不点：
  - 设置 › 文档库的「在访达中显示」按钮，主页和文件夹页「…」菜单里的「在访达中打开」；
  - 链接、「网页搜索」、打开链接类按钮（会启动浏览器）；
  - 打印（会弹出系统打印面板）；
  - 快速查看 / `qlmanage -p`。
- 不用 `open` 命令。
- 需要选文件时，用 `pick a|b` 代替系统对话框。
- 不要拿用户的真实文档库当 `lib_dir`。确实需要时只读复制一份，产物里也不要提交用户的内容。

## Fei Notes SQLite regression

`python3 tools/harness/sqlite_history.py` checks first-launch creation, synthetic IME marked text (not persisted before commit), read-only history preview, returning to the live document, and restoring a snapshot into SQLite. Build with `cargo build --features cloudkit-sync,devtools`; `FDL_CLOUD_BIN` selects another binary and `FDL_CLOUD_DYLD_PATH` selects the matching Swift bridge directory. The test creates a fresh `/private/tmp/quiet-sync-qa-history-*` local-only library, uses the existing offscreen driver and never selects an iCloud account or uses the system clipboard.

`python3 tools/harness/sqlite_board.py` uses the same binary overrides and isolated local-only startup to draw a shape, edit board text, paste a synthetic image through the fake clipboard, and verify the persisted scene and image ownership in SQLite. It reopens the note read-only and checks that attempted board edits leave its payload unchanged. Artifacts stay in `/private/tmp/quiet-sync-qa-board-*`.

The table-paste and scroll-intent tests locate visible documents in both the old flat library and FORMAT 4, excluding hidden migration backups.

`python3 tools/harness/sharing_ui.py` checks the contextual shared-note list,
creation and member-management flow with a fresh synthetic library and an
explicit devtools-only offline invitation transport. It injects initial-upload
and invitation failures, checks retry identity and original-note preservation,
then exercises permission changes, confirmed removal and read-only invitation
acceptance. It also verifies collapsible shared navigation, same-window opening,
participant leave, owner deletion and retry after a recoverable deletion failure.
Shared Documents reuses the existing Home document-list renderer. The shared
section starts collapsed above My Documents; captures also check the linked
mail-check invitation controls.
Captures include narrow/dark member management. No live CloudKit,
real notes, system clipboard or foreground activation is used. The existing
`FDL_CLOUD_BIN` and `FDL_CLOUD_DYLD_PATH` overrides apply.

`python3 tools/harness/startup.py` verifies the Mac startup boundary with newly
generated offline fixtures: verified cache, delayed background restore, local
editing retained after a late reply, and explicit local-only preference. It
also exercises the existing New Document shortcut before discovery completes.
An unavailable-account fixture must reject an unverified cache; a fresh install
with a delayed account check must retain a note entered before it completes.
Build with `cloudkit-sync,devtools` and set `FDL_CLOUD_BIN` and
`FDL_CLOUD_DYLD_PATH`; no real account or existing library is used. Account and
device-marker mismatch checks also run in the cloud configuration unit tests.
