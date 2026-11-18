# 网页版（官网的实时笔记本）

Quiet.md 本体编译成 wasm32，跑在浏览器里，给官网的「示例笔记本」用（`site/public/notebooks/`）。
它和桌面版是同一份代码，靠 GPUI 的网页后端（`vendor/zed/crates/gpui_web`，单线程、稳定版工具链）运行。

## 和桌面版的区别

- **文件**：文档库在内存里（`fdl_core::vfs`）。桌面版 `vfs` 就是 `std::fs`；网页版启动时把一个库包解进内存，
  改动只留在当前标签页，刷新复原。路径查询用 `PathExt`（`p.fs_exists()` 等），因为 `Path` 自己的方法总是问真实文件系统，
  而且 wasm32-unknown-unknown 上 `Path::is_absolute()` 恒为 false（用 `fs_is_absolute()`）。
- **时间**：`web_time::{Instant, SystemTime}`（桌面上就是 std 的），`std::time::Instant::now()` 在浏览器里会 panic。
  同理不要直接用 `std::env::temp_dir()`、`std::process::id()`，用 `vfs::temp_dir()`、`vfs::process_id()`。
- **字体**：浏览器里读不到系统字体，`fonts/` 里的 Inter 和 JetBrains Mono（OFL）随页面下载，中文借浏览器的字体
  （`CanvasFontFallback::EmojiAndCjk`）。`.SystemUIFont` 指向 Inter（vendor 补丁 0029）。
- **图片**：库包不含 `.assets/`，图片放在库包旁边，第一次显示时才下载（`vfs::add_lazy`，下载中按「加载中」显示）。
- **快捷键**：在 Mac 浏览器里按 macOS 的规则（⌘），其它系统按 PC 的规则（`shortcuts::mac()`）。
- 没有的：全局快捷键、自动更新、打印、系统通知、快速查看、在访达中显示。入口在 `src/web.rs`。

## 构建

需要：`rustup target add wasm32-unknown-unknown`、与 Cargo.lock 一致的 wasm-bindgen-cli
（`cargo install wasm-bindgen-cli --version <版本> --root ~/.local/wasm-bindgen-<版本>`）、binaryen（`wasm-opt`）。

```sh
web/build.sh site/public/demo          # 正式包：profile web + wasm-opt -Oz（几分钟）
FAST=1 web/build.sh <目录>              # 调试：release profile、不跑 wasm-opt
```

`site/public/demo/` 是构建产物，不进仓库，随官网一起部署（`cd site && npx wrangler@4 deploy`）。
压缩后传输约 6 MB wasm + 2 MB 字体。

## 示例文档库

人物资料在 `marketing/mock-users/<名字>/`（私有）。每个库的流程：

1. 图片压成 1400px JPEG（`sips`），用资料目录里的 `build_library.sh`（`FLAT=1`）导入成文档库；
2. 用 devtools 版打开一次生成 `.quiet/index.json`，再 `web/curate.py <库> "<顶层名>" …` 定侧栏顺序；
3. 偏好里关掉 `auto_update` 和 `quick_hotkey`；
4. `web/bundle.py <库> site/public/demo/libraries/<名字>.qmdb`（同时写出 `libraries/<名字>/.assets/`）。

页面用 `/demo/?lib=<名字>` 打开某个库。新加一个人：生成库包，`site/public/notebooks/<名字>/`
放 `index.html`（照现有的抄）和 `home.webp` 截图（devtools 版脚本截 1280×800），在 `notebooks/data.js` 加一项，
首页照 Maya 那一节（`section.notebook`）加一节。

## 官网页面

- `site/public/index.html`：首页。首屏是原来的标题、介绍和下载；接着是「不是截图，是真的 Quiet.md」的引导，然后先韩梅梅、再 Maya，各一段文案加她的窗口。人物页 `site/public/notebooks/<名字>/index.html` 用同一套样式，只放这一个人。
- 窗口照真实的 macOS 26 窗口做（量过系统截图：连续圆角，曲线长 23pt、看上去约等于 16pt 的圆角，红绿灯直径 14pt、间距 23pt、圆心在 (26.75, 23.75)）。红绿灯由网页画，应用在网页版里像 macOS 一样留出 92pt（`theme::TRAFFIC_LIGHTS_W`）。
- 桌面浏览器里窗口运行真实的应用（`/demo/?lib=<名字>`）。打开页面时什么都不加载，窗口进入视野才加载；同一时间只运行一个，离开视野的会在另一个启动时被释放（滚回来会重新启动）。窗口上盖着一层透明遮罩，滚轮仍然滚页面；点一下就移开，这一下会转交给应用；鼠标移出窗口后遮罩回来。手机上换成 `home.webp` 截图。
- 窗口边缘可以拖（`site/public/window-resize.js`，样式也在里面）：窗口外侧有白色把手，左右改宽度、窗口保持居中，下边改高度，下面两个角同时改；双击把手恢复。最小 560×420。窗口第一次进入视野时会自己缩窄再弹回一次，下方有一行提示，有人拖过一次后（记在 localStorage 的 `demo-resized`）就不再出现。手机上都没有。
- 顶部 GitHub 按钮显示 star 数：页面里写着 1.9k，加载后从 GitHub API 取实时值，在浏览器里缓存一小时。
- 页尾有第二组下载按钮（`section.cta`）。
- 人物资料（5 种语言）在 `site/public/notebooks/data.js`。
