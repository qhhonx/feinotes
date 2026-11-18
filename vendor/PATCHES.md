# vendor 补丁

GPUI 来自 [Zed 仓库](https://github.com/zed-industries/zed)：`vendor/zed` 是 Zed 某个固定提交的快照加上本项目的补丁序列。快照的提交和要复制的 crate 写在 `vendor/zed/upstream.toml`；补丁在 `vendor/zed/patches`，按编号依次打；`vendor/zed/crates` 是打完补丁的结果，应用通过 `Cargo.toml` 里的路径依赖使用它。补丁处的代码都带 `quiet.md:` 注释。

不用 git 依赖直接指向 Zed 仓库：整个仓库太大，会拖慢 CI 和贡献者的首次构建。

## 目录

| 路径 | 内容 |
|---|---|
| `vendor/zed/upstream.toml` | Zed 仓库地址、提交、要复制的 crate、丢掉的目录（`examples`、`benches`），以及 Zed 用 git 依赖、我们改用 crates.io 版本的包 |
| `vendor/zed/patches/NNNN-名字.patch` | 一个补丁一个文件。开头是说明（改了什么、为什么），后面是 `git diff`，路径相对 Zed 仓库根目录 |
| `vendor/zed/crates/…` | 生成的结果，目录结构和 Zed 仓库相同（`crates/gpui`、`crates/refineable/derive_refineable`…） |
| `vendor/zed/crates/*/Cargo.toml.orig` | 打完补丁的上游 manifest（补丁改的是这一份） |
| `vendor/zed/crates/*/Cargo.toml` | 由 `.orig` 生成的独立 manifest：展开 Zed workspace 继承的字段和依赖，去掉 dev-dependencies、没带的 Zed crate 及引用它们的 feature，git 依赖换成 crates.io 版本，`[lints]` 换成 Zed 的规则 |
| `vendor/zed/LICENSE-APACHE` | Zed 的许可（这些 crate 都是 Apache-2.0） |
| `tools/vendor-sync` | 同步工具 |

这些 crate 不是本项目 workspace 的成员（根 `Cargo.toml` 的 `exclude`），所以 `cargo test --workspace`、clippy 不检查它们。`vendor/zed/crates` 不要直接改了不管：改完要存成补丁（见下面），`check` 会报出没存的改动。

## 同步工具

```sh
V="cargo run -q --manifest-path tools/vendor-sync/Cargo.toml --"
$V check                      # vendor/zed/crates 是否等于 上游快照 + 全部补丁
$V save <名字> -m "<说明>"     # 把 vendor/zed/crates 里的改动存成新补丁（编号排在最后）
$V save <已有名字>             # 重新生成这个补丁（说明不变，挪到序列末尾）
$V sync                       # 按 upstream.toml 重建 vendor/zed/crates
$V sync --rev <提交|main>     # 换上游提交：写回 upstream.toml，打不上的补丁会逐个报出
$V sync --rev main --keep-going   # 打不上的跳过，其余照常写出
$V refresh                    # 用当前快照重写所有补丁的 diff（只更新行号和上下文）
```

工具在 `~/.cache/quiet-md/zed`（或 `QUIET_MD_ZED_DIR`）放一份不带文件内容的 Zed 克隆，只稀疏检出需要的 crate；中间结果在 `target/vendor-sync`。补丁按 `git apply --3way` 依次打，每个补丁一次提交。

新增补丁的做法：直接改 `vendor/zed/crates` 里的文件（manifest 改 `Cargo.toml.orig`），`$V save 名字 -m "说明"`，再 `$V sync` 重新生成 `Cargo.toml`，最后 `$V check`。一次只存一组相关改动：`save` 会把 `vendor/zed/crates` 里所有没存的改动收进同一个补丁。

升级上游：

1. `$V sync --rev main --keep-going`，记下没打上的补丁；
2. 对每个没打上的补丁，按它开头的说明在 `vendor/zed/crates` 里重新改，`$V save <它的名字>`（每次只修一个）；上游已经有等价实现的，删掉补丁文件并在下面的表里写明；
3. `$V refresh`、`$V check`，然后 `cargo build`，修应用这边的编译错误；
4. 按下面的「验证」截图对比、跑测试，更新本文件。

## 补丁

快照：Zed `e2f00a41ac67`（2026-11-15）。「旧 #」是 0.2.2 时代本文件表格里的编号。

| 补丁 | 旧 # | 文件 | 改动与用途 |
|---|---|---|---|
| 0001 glyph-bounds-at-size | 2 | `gpui_apple/text_system.rs` | 字形边界用和光栅化相同字号的 CTFont 量。系统字体随字号切换光学尺寸（SF Text → SF Display），原来大标题字形左侧会被切掉一条 |
| 0002 katex-fonts | 3 | 同上 | 名字以 `KaTeX_` 开头的字体没有 `m` 字形也允许加载（Size1–4、AMS、Caligraphic、Script） |
| 0003 fallback-font-identity | 14 | 同上 | 一个 PostScript 名对应多个 FontId，CoreText 回退来的字体按字形数挑。系统回退的苹方（PingFangUI.ttc）和 PingFang.ttc 都叫 `PingFangSC-Regular`，字形表不同；原来按名字查到另一份，中文画成错字。测试在 `editor::text::tests::cjk_fallback_glyphs` |
| 0004 synthetic-italic | 12 | 同上 | 合成斜体：要斜体而字体（含回退字体）没有斜体字面时换成「倾斜孪生」FontId，光栅化时错切 14°（WebKit 的角度），字形边界相应加宽。中文斜体像浏览器一样倾斜 |
| 0005 grid-col-widths | 6 | `gpui/style.rs`、`styled.rs`、`taffy.rs` | `grid_col_widths(Vec<f32>)`：以像素给出网格列宽，覆盖 `grid_cols`。表格列宽 |
| 0006 paint-rows | 5 | `gpui/text_system/line.rs` | `WrappedLine::paint_rows` / `paint_background_rows`：按给出的每行顶部位置绘制。行内有公式或按钮时各行高度不同 |
| 0007 list-scroll-estimates | 7 | `gpui/elements/list.rs` | `ListState::estimated_scroll_metrics` / `scroll_to_estimated_offset`：没量过的行按平均行高估算。长文档的滚动条和拖动滚动条 |
| 0008 path-prompt-extensions | 8 | `gpui/platform.rs`、`gpui_macos/platform.rs` | `PathPromptOptions.extensions`：限制 NSOpenPanel 可选的文件类型。导入对话框只列出 Markdown / 文本和 .xml |
| 0009 offscreen-capture | 10 | `gpui/window.rs`、`platform.rs`、`gpui_macos/window.rs`、`gpui_apple/metal_renderer.rs` | 上游的离屏渲染（`Window::render_to_image`）原来只在 `test-support` 下编译，改为总是编译；新增 `Window::render_offscreen`（跑下一帧回调、绘制、离屏渲染、不上屏）和 `Window::needs_draw`。脚本截图，窗口被遮挡或锁屏时也能用（devtools） |
| 0010 replace-root-type | 26 | `gpui/window.rs` | `Window::replace_root` 同时更新窗口句柄记录的根视图类型。首次启动的引导页走完后在同一窗口换成主界面，之后 `downcast::<Workspace>()` 才找得到 |
| 0011 window-min-size | 27 | `gpui/window.rs`、`platform.rs`、`gpui_macos/window.rs`、`gpui_windows/window.rs`、`events.rs` | `Window::set_window_min_size`：创建后改最小内容尺寸（macOS `setContentMinSize`，Windows 改 `WM_GETMINMAXINFO` 用的 `min_size`）。最小宽度随侧栏开合和拖宽变化 |
| 0012 present-in-transaction | 28 | `gpui/window.rs`、`platform.rs`、`gpui_macos/window.rs` | `Window::present_next_frame_in_transaction`：下一帧走 `PlatformWindow::draw_in_transaction`，macOS 上这一帧 `presentsWithTransaction`，和渲染期间改的原生视图同一个 Core Animation 事务上屏。侧栏玻璃变化的那一帧和画面同时出现 |
| 0013 hollow-images | 24 | `gpui/assets.rs`、`window.rs` | `RenderImage::hollow`（同 id、同尺寸、不带像素的空壳）、`is_hollow`、`with_scale_factor`；`paint_image` 遇到 atlas 里没有的空壳时跳过、记下并请求下一帧，由 `Window::take_missing_images` 取走。已上传的文档图片不在内存里留像素（docs/MEMORY.md） |
| 0014 load-visible-only | 20 | `gpui/elements/img.rs`、`image_cache.rs` | `ImageCache::peek`（不触发加载、不算使用）；`Img::load_visible_only`：给了宽高的图片到在可见区域内绘制时才解码。卡片裁剪范围外的图片不预先解码 |
| 0015 hide-cursor | 新 | `gpui/app.rs` | `App::hide_cursor_until_mouse_moves`。上游删了 `CursorStyle::None`，改为自己在打字时隐藏指针（`CursorHideMode`）；应用把 `CursorHideMode` 设为 `Never`，在原来用 `CursorStyle::None` 的地方（编辑器和输入框打字、看图界面空闲）调用它 |
| 0016 font-smoothing | 1 | `gpui_apple/text_system.rs` | 所有颜色的文字都按最深一级打开字体平滑（stem darkening），`glyph_dilation_for_color` 恒为 0。上游按文字颜色选平滑程度（深色文字不平滑、浅色文字填色更浅）；应用的文字是对照原生 AppKit 调的（关掉平滑笔画少约 15% 墨量），保持升级前的样子。用户关了 AppleFontSmoothing 时不平滑 |
| 0017 fallback-by-name | 新 | `gpui_apple/open_type.rs` | `FontFallbacks` 里的回退字体按名字建描述符（`CTFontDescriptorCreateWithNameAndSize`，0.2 的做法）。上游按「家族名 + 字重/倾斜」建，`PingFang SC` 会落到 PingFang.ttc 里的家族（49533 个字形），而不是系统 UI 用的那份（47098 个），轮廓略有不同，中文整体淡约 2% |
| 0018 clipboard-text-and-image | 22 | `gpui_macos/pasteboard.rs` | 读剪贴板时同时返回文字和一种图片（PNG 优先于 TIFF），跳过空图片数据。截图工具同时给文字和图片时，正文可以取图片、输入框仍读文字。测试在 `pasteboard::tests`（私有剪贴板） |
| 0019 korean-word-wrap | 23 | `gpui/text_system/line_wrapper.rs` | 上游换行改用 UAX #14（icu_segmenter），它允许在任意两个谚文音节之间断行；去掉谚文和字母连成的词内部的断行机会（`Quiet.md를` 也不拆）。韩文按词（어절）换行，超出一行的长词仍按字符断开。`test_break_indices_quiet_md` 同时覆盖中文避头尾和阿拉伯文、希伯来文整词 |
| 0020 rtl-wrap | 30 | `gpui/text_system/line_layout.rs` | 含从右到左文字的段落需要折行时（`wrap_bidi`）：按阅读顺序取每个字符的宽度定折行点，每行单独交给平台排版，再首尾相接拼成一个布局，每行第一个字形处放折行点，行尾空格不参与排版。原来按显示顺序切行，阿拉伯文多行时行序颠倒 |
| 0021 rtl-align | 32 | `gpui/text_system/line.rs` | 折成多行、第一个字母属于从右到左文字的段落，默认左对齐改成右对齐；单行、显式对齐和 `paint_rows` 不变 |
| 0022 single-axis-scroll | 17 | `gpui/elements/div.rs` | 只能在一个方向滚动的容器只接收偏向这个方向的滚动。竖着滚过宽表格时表格不再跟着左右滑；在表格上横向滑动仍能横滚。上游的 `restrict_scroll_to_axis` 要逐个开启，而且只对跟踪了滚动句柄的容器锁定触控板手势，不等价 |
| 0023 windows-clipboard | 16 | `gpui_windows/clipboard.rs` | 上游已经一次读出文字、一种图片（含 `CF_DIB`）和复制的文件。在此之上：有 PNG 时优先取 PNG，否则把位图转成 PNG（文档要存它，BMP 很大）；上游的 DIB 转换对 V4/V5 头也多加 12 字节掩码，图像错位，改用我们的转换；复制的文件全是图片时读成图片，否则在没有文字时把路径按行粘贴 |
| 0024 windows-device-lost | 25 | `gpui_windows/directx_renderer.rs`、`events.rs`、`platform.rs`、`window.rs` | 上游已重写设备丢失恢复（资源用 Option、错误向上传、resize 失败时作废设备、atlas 查不到纹理返回 None），但恢复失败时 `panic!`。改为：平台层记日志、下一次 vsync 再试；窗口渲染器恢复失败时留着新设备、下一次绘制再试（期间不绘制，按窗口当前大小建交换链）；恢复后重新设置亚克力模糊（驱动重启后会丢；先关再开，同样的设置可能被忽略） |
| 0025 windows-shader-path | 15 | `gpui_windows/directx_renderer.rs` | 调试构建编译着色器时路径按宽字符串传给 `D3DCompileFromFile`（原来 `to_str().unwrap()`） |
| 0026 linux-appearance-reborrow | 18 | `gpui_linux/wayland/client.rs`、`x11/client.rs` | 外观变化时先复制窗口句柄、释放 client 的借用，再调窗口回调。回调会查询平台、更新菜单，原来重复借用 panic |
| 0027 wgpu-30-crates-io | 新 | `gpui_wgpu/wgpu_context.rs`、`wgpu_renderer.rs` | Zed 用 gfx-rs/wgpu 的一个 git 提交，我们用 crates.io 的 30.0.2（`upstream.toml` 的 `[git-to-crates-io]`）。两处 API 差异：`DeviceDescriptor` 还没有 `default_queue`，`has_srgb_suffix` 在 30.0.2 里叫 `is_srgb`。只影响 Linux |
| 0028 keybinding-load-unmapped | 新 | `gpui/keymap/binding.rs` | `KeyBinding::load_unmapped`：不需要 `App`，按给定系统解析按键，不按键盘布局映射（0.2 的 `load` 配 `DummyKeyboardMapper`）。上游的 `load` 要 `&App` 并用真实的键盘映射，Windows 上会改写按键；应用的 Windows / Linux 键位和用户自定义快捷键照旧不映射 |
| 0029 web-system-font | 新 | `gpui_web/platform.rs` | `WebPlatform::new_with_system_font`：网页版 `.SystemUIFont` 对应的字族（上游写死 IBM Plex Sans）。网页版加载 Inter（`src/web.rs`） |
| 0030 web-diagonal-cursors | 新 | `gpui_web/platform.rs` | 两个斜向缩放光标对调：`ResizeUpLeftDownRight` 是 CSS 的 `nwse-resize`。网页版图片四角的光标原来方向相反 |
| 0031 board-paths | 34、35 | `gpui/path_builder.rs`、`window.rs` | `Path::transformed`（等比缩放加平移，对单调二次曲线是精确的）、`empty`、`is_empty`、`from_polygons`；`PathBuilder::build_polygons`（轮廓按容差展平成多边形，保留每条轮廓）；`Window::with_element_opacity` 改为 `pub`。画板按元素缓存路径、随视图平移缩放复用；框架裁剪和箭头标签挖空在展平的轮廓上做（`src/board/render_geom.rs`），上游的 `Path` 不再保留轮廓顺序，也没有三角形 |
| 0032 cosmic-bundled-fonts | 43、38 | `gpui_wgpu/cosmic_text_system.rs` | `add_fonts` 加入的字形替换系统里同字族、同样式的已有字形（Fedora 的 COLRv1 Noto Color Emoji 让位给内置的 CBDT 版）；Xiaolai 没有 `m` 字形也加载。Linux 和网页 |
| 0033 glyph-outline | 37、38 | `gpui/platform.rs`、`text_system.rs`、`gpui_apple/text_system.rs`、`gpui_windows/direct_write.rs`（加 `Win32_Graphics_Direct2D_Common`）、`gpui_wgpu/cosmic_text_system.rs`、`gpui_web/text_system.rs` | `WindowTextSystem::glyph_outline` 和 `GlyphOutlineCmd`：字形轮廓（字体单位、y 向上）。macOS 用 font-kit（合成斜体同样错切），Windows 用 DirectWrite `GetGlyphRunOutline`，Linux 和网页用 swash；网页的画布回退字体没有。画板的 CPU 渲染（vello）和旋转文字用它。macOS 加载没有 `m` 字形的 Xiaolai |
| 0034 image-into-frames | 新 | `gpui/assets.rs` | `RenderImage::into_frames`：取回图片的帧。画板的 vello 帧每次画面变化新建一张图片，上一张 `drop_image` 后把它的缓冲取回，下一帧 vello 直接画进去（`src/board/render_vello.rs`），省掉每帧新分配和拷贝 12 MB |

## 没有再带的旧补丁

| 旧 # | 原来的改动 | 现在 |
|---|---|---|
| 4 | 中文避头尾 | 上游换行改用 UAX #14，闭合标点不在行首、开标点不在行尾已经是默认规则（`test_break_indices_quiet_md`） |
| 9 | `mouseDownCanMoveWindow` 返回 NO，延后执行 `start_window_move` | 上游有 `WindowOptions::app_owns_titlebar_drag`（整个视图算应用自己的标题栏，AppKit 不拖动也不等双击判定）和 `Window::start_window_move`；应用的主窗口和快速笔记窗口开启它 |
| 11 | `DispatchEventResult` 改为 `pub` | 上游已是 `pub` |
| 13 | 文字场景排序边界包含对齐后的字形 | 上游的绘制层边界改为由层内实际绘制的图元求出，不再依赖调用方给的边界 |
| 19 | 路径纹理按需创建、按批次范围 | 上游的路径改为解析式分块（tile）直接画进目标纹理，没有中间纹理和 MSAA 纹理；深度纹理在 Apple GPU 上是 Memoryless |
| 21 | 删除图片时先移除 tile 索引 | 上游的通用 atlas 先从 `tiles_by_key` 移除再释放 |
| 29 | 阴影按模糊范围判断是否在裁剪区外 | 上游的阴影图元边界已含 3 倍模糊半径 |
| 31 | 希伯来、阿拉伯等字母算单词字符 | UAX #14 把这些字母归为 AL，词内不断行 |
| 33 | `CAMetalLayer` 最多 2 张 drawable | 没有带。上游改用 `CAMetalDisplayLink` 提前分发 drawable，限为 2 张有掉帧风险；脚本场景下实测内存占用没有差别（升级前后都约 95 MB）。以后要省这 16 MB 时在真窗口上测帧率再定 |
| 36 | `PixelFrame`、`Window::paint_pixels`：CPU 画好的帧直接写进图集块 | 画板的 vello 帧改为普通的 `RenderImage`，每次画面变化新建一张、上一张在下次显示时 `drop_image`（`src/board/render_vello.rs`） |
| 39 | `layout_line` 合并 run 时比较字体 | 上游已比较字体 |
| 40 | Metal 路径光栅化的顶点直接写进 instance buffer | 上游的路径改为分块绘制，没有这一步 |
| 41 | 有 `active` 样式的元素总是监听松开 | 上游已无条件注册松开监听 |
| 42 | 视图注册 `rightMouseDragged:`、`otherMouseDragged:` | 上游已注册 |
| —（Windows `VK_PROCESSKEY`） | 输入法正在处理的按键始终交给 IME | 上游的 `VK_PROCESSKEY` 解析不出按键，不再作为 GPUI 按键派发，直接交给 `TranslateMessage` 和 IME。需在 Windows 上用小狼毫确认退格、回车不会处理两次 |

## 升级后和 0.2.2 不同、没有改回去的地方

- 像素对齐：上游把元素边界（taffy 布局后的绝对坐标）、边框粗细、裁剪区对齐到设备像素，字形纵向不再分子像素位置、横向取最近的 1/4 像素（原来向下取）。截图对比里文字、图标边缘有亚像素差异；高度不是整设备像素的块（比如含行内公式的段落）下面的内容可能整体差一个设备像素。
- 静止指针下的 hover：上游在内容变化后重新计算 hover，打开搜索框后，被遮罩盖住的搜索按钮不再保持 hover 背景和提示（原来要等鼠标移动才更新）。
- `uniform_list` 的内边距：0.2.2 先按内边距缩小内容区，摆放子项和算内容高度时又各加一次（子项下移一个上内边距，可滚动范围多出上下内边距之和），上游修掉了。应用照旧的位置改：主页卡片网格 `pt(14)`/`pb(16)` → `pt(28)`/`pb(32)`；目录列表能滚动时 `pb(22)` → `pb(44)`（列表放得下时 0.2.2 还能多滚 22pt，这个不再复现）。
- 窗口原点在半点上时，上游创建窗口会多出 1pt 高；应用打开主窗口和快速笔记窗口时把原点取整。

## 验证

升级上游后按这一节逐项验证。每次运行都用临时文档库（`QUIET_MD_HOME`），不碰真实剪贴板，不弹系统对话框，不抢焦点。

```sh
cargo build --release --features devtools            # 新版；旧版按 tools/harness/README.md 另编一份
FDL_BIN=target/release/quiet_md FDL_REF_BIN=<旧版> python3 tools/harness/gpui_patches.py
for f in table_paste table_resize clipboard_images checkbox_toggle scroll_intent pointer_properties; do
  FDL_BIN=target/release/quiet_md python3 tools/harness/$f.py; done
scripts/check.sh
```

- `tools/harness/gpui_patches.py`：对每个补丁影响的行为跑一个场景。新旧两个版本各跑一遍，比较「排版」：文档区每一行文字的上下左右位置要在 3 个设备像素以内，墨量（暗度之和）要在 4% 以内。所以亚像素差异不算失败，换行、对齐、行高、滚动位置的变化会算。
  - 有的场景还带不需要旧版的绝对检查：斜体确实倾斜、表格列宽是 85/300/150、滚动条跟着拖动、图片真的画出来了、自定义快捷键生效、⌘Q 前输入的内容已保存。
  - 内存在运行时每 0.1 秒采一次 RSS。
- 6 个 harness 脚本（README 里列的那些）是功能回归，它们不依赖旧版。
- 截图逐像素对比（`compare.py`）和旧补丁 #13、#29 的菜单、阴影检查，用的是升级时临时写的巡检脚本（文档、菜单、斜杠菜单、搜索、主页、设置、滚动、窄窗口、深色 + 中文），没有留在仓库里。
- `gpui` 自己的单元测试（`test_break_indices_quiet_md` 等）需要 Zed 仓库里的字体、示例图片和 `util_macros`，在 vendor 副本里编译不了。`gpui_macos` 的测试可以跑：把 `vendor/zed/crates` 复制出去，给 `gpui_macos/Cargo.toml` 加一个空的 `[workspace]`，再 `cargo test --features test-support --lib pasteboard`。
- Windows 和 Linux 在 macOS 上能做类型检查：
  - Windows：`cargo check --target x86_64-pc-windows-msvc`，需要一个假的 `rc.exe` / `llvm-rc`，只写空的输出文件。
  - Linux：`--target x86_64-unknown-linux-gnu`，需要能编 C 的交叉编译器包装，并设 `RUST_FONTCONFIG_DLOPEN=on`。

### 逐项结果（2026-11-15，macOS arm64，旧版 = main `a15b5a7`）

汇总：40 项。macOS 上通过 28 项（含 0028 的 macOS 部分），部分验证 2 项（0011 macOS、旧 #9），未验证 3 项（0008、0012、0015，需要在真窗口里手动看），等 sida-windows 5 项（0011 Windows、0023、0024、0025、`VK_PROCESSKEY`），只验证了能编译、等 CI 2 项（0026、0027）。没有失败项。

「旧版 / 新版」都是实测结果。「场景」指 `gpui_patches.py` 里的场景名。

| 项 | 不打补丁会怎样 | 怎么验证 | 结果 |
|---|---|---|---|
| 0001 glyph-bounds-at-size | 大标题字形左边被切掉一条 | 场景 `title`：标题 + 一级、二级标题（`Wj`、`fjord` 等左侧外伸的字形），窗口宽 1280/1000/760 | 通过：3 个宽度的排版都和旧版一致 |
| 0002 katex-fonts | 花体、黑板粗体、大号括号回退成别的字体 | 场景 `katex`：`\mathcal`、`\mathscr`、`\mathbb`、`\mathfrak`、`\left( \right)`、`\Bigg\{` 等，3 个宽度 | 通过 |
| 0003 fallback-font-identity | 中文画成错字 | 场景 `cjk`（3 个宽度）；`check.sh` 里的 `editor::text::tests::cjk_fallback_glyphs` | 通过 |
| 0004 synthetic-italic | 中文斜体画成正体 | 场景 `italic`：比较同一行中文的斜体和正体，斜体上半部的墨迹要偏右；3 个宽度和旧版比 | 通过：新旧版都倾斜，排版一致 |
| 0005 grid-col-widths | 表格忽略存下的列宽 | 场景 `table`：列宽 85/300/150，量竖线间距；3 个宽度 | 通过：间距在 ±2pt 内，新旧版一致 |
| 0006 paint-rows | 含行内公式的行和其它行一样高，公式压住上下行 | 场景 `rows`：两处行内公式（分式、根号）的长段落，3 个宽度 | 通过 |
| 0007 list-scroll-estimates | 长文档滚动条大小和位置不对，拖动跳动 | 场景 `scrollbar`：400 段长短不一的文档，滚动后把滑块从 y=68 拖下 280pt，记下滑块位置和高度 | 通过：新旧版都是 (52, 32) → (340, 32) → 滚动后 (344, 32)，拖完后页面内容一致 |
| 0008 path-prompt-extensions | 导入对话框列出所有文件 | 没有实测：脚本用 `pick` 绕过系统面板，规则也不允许弹真面板。只核对了代码：导入时传了扩展名，补丁设置 `allowedContentTypes` | 未验证（要在预览版里手动打开「导入」看一眼） |
| 0009 offscreen-capture | 窗口被挡住或锁屏时脚本截不了图 | 所有场景和 harness 脚本的截图都走 `render_offscreen`，窗口在屏幕外或被遮挡 | 通过（几百张截图） |
| 0010 replace-root-type | 首次启动走完引导页后找不到主窗口：⌘Q 不保存、Finder 打开的文件没地方去 | 场景 `first-launch`：空文档库加 `QUIET_MD_ONBOARDING`，按 8 次回车走完引导，新建文档、输入、马上 ⌘Q；`app::quit` 用 `downcast::<Workspace>()` 找窗口保存 | 通过：新旧版都换成主界面，文档已保存 |
| 0011 window-min-size（macOS） | 侧栏开合、拖宽后窗口最小宽度不变 | 想直接读 `contentMinSize`：辅助功能接口不提供 GPUI 窗口的尺寸（-25205），lldb 附加要系统授权，都放弃了。脚本里开合侧栏，代码路径能跑通 | 部分：没量到最小尺寸本身 |
| 0011 window-min-size（Windows） | 同上，`WM_GETMINMAXINFO` | — | 等 sida-windows |
| 0012 present-in-transaction | 侧栏玻璃比画面早或晚一帧变化（闪一下） | 只能看真窗口上屏的那一帧，离屏截图拍不到。脚本里开合侧栏不报错 | 未验证（要在真窗口里用肉眼或录屏看侧栏开合、拖宽） |
| 0013 hollow-images | 文档图片的像素一直留在内存里 | 场景 `images`：30 张 2400×1600 的图片，滚到底再滚回顶，采 RSS，确认图片画出来了 | 通过：峰值 新 366–378 / 旧 365–377 MB，稳定后 新 300–306 / 旧 299 MB（各跑 3 次） |
| 0014 load-visible-only | 主页卡片裁剪范围外的图片也解码 | 场景 `home-images`：12 篇文档，每篇 4 张大图（卡片里只看得到前两张），打开主页、滚到底 | 通过：峰值 新 135–145 / 旧 142–150 MB，稳定后 新 129–130 / 旧 134–135 MB，卡片画面一致 |
| 0015 hide-cursor | 编辑器、输入框打字时指针不隐藏 | 窗口在屏幕外时观察不到，`CGCursorIsVisible` 已不可用（结果不可信）。harness 里打字不报错 | 未验证（要在真窗口里打字看指针） |
| 0016 font-smoothing | 深色文字不加粗、浅色文字更淡，全文比升级前细 | 场景 `cjk`（浅色）和 `cjk-dark`（深色主题）：比较墨量 | 通过：正文每行墨量和旧版差 ≤0.5% |
| 0017 fallback-by-name | 中文整体淡约 2% | 场景 `cjk`：每行墨量阈值 4%，实测正文差 ≤0.5% | 通过 |
| 0018 clipboard-text-and-image | 截图工具给的「图片 + 文字」只读到文字，图片贴不进来 | `gpui_macos` 的 `pasteboard::tests`（私有剪贴板，9 项，其中 3 项是本补丁的）；harness `clipboard_images.py` 走的是模拟剪贴板，只测应用这一侧 | 通过：9/9 |
| 0019 korean-word-wrap | 韩文在音节之间断行 | 场景 `korean`：3 个宽度和旧版比（旧版按词换行） | 通过（上游测试在 vendor 里编译不了，见上） |
| 0020 rtl-wrap | 阿拉伯文多行时行序颠倒、词被拆开 | 场景 `rtl`：阿拉伯文、希伯来文长段落，3 个宽度 | 通过 |
| 0021 rtl-align | 多行的从右到左摘要靠左 | 场景 `home-rtl`：主页卡片上的阿拉伯文、希伯来文摘要 | 通过 |
| 0022 single-axis-scroll | 竖向滚轮（带一点横向漂移）经过宽表格时，表格也跟着左右滑 | 场景 `wide-table`：10 列 × 220pt 的表格。在表格上竖向滚动并带 4pt 横向分量，再横向滚动 ±300/120 | 通过：竖向滚动只滚页面，横向滚动只滚表格，3 张截图新旧版一致 |
| 0023 windows-clipboard | 位图存成 BMP、V4/V5 头图像错位、复制的图片文件贴不进 | — | 等 sida-windows |
| 0024 windows-device-lost | 驱动重启、睡眠唤醒后 panic | — | 等 sida-windows |
| 0025 windows-shader-path | 中文路径下调试构建崩溃 | — | 等 sida-windows |
| 0026 linux-appearance-reborrow | 切换明暗时 panic | 在 macOS 上交叉跑 clippy | 只验证了能编译；等 CI |
| 0027 wgpu-30-crates-io | 用 crates.io 的 wgpu 编译不过 | 同上 | 只验证了能编译；等 CI |
| 0028 keybinding-load-unmapped | 自定义快捷键、Windows/Linux 键位被键盘映射改写 | 场景 `shortcuts`：`.quiet/shortcuts.json` 把加粗设成 `cmd-shift-k`，全选后按它，检查保存的 `<b>` | macOS 通过（新旧版都生效）；Windows 等 sida-windows |
| 旧 #4 中文避头尾 | 闭合标点出现在行首 | 场景 `kinsoku`：各种中文标点密集的段落，3 个宽度和旧版比 | 通过 |
| 旧 #31 RTL 整词 | 阿拉伯文、希伯来文在词中断行 | 场景 `rtl` | 通过 |
| 旧 #9 标题栏拖动 | 标题栏上的按钮点不到，或拖动、双击不灵 | harness 的点击（搜索、「…」、侧栏开关、主页）都正常；真实指针拖动窗口、双击缩放要真鼠标，脚本做不到 | 部分：按钮通过，拖动和双击未验证 |
| 旧 #11 `DispatchEventResult` 公开 | 编译不过 | 编译 | 通过 |
| 旧 #13 文字层边界 | 部分字形被裁掉或不画 | 所有文字场景的排版一致；升级时的截图巡检 | 通过 |
| 旧 #19 路径纹理 | 路径（卡片圆角、复选框等）占用大块纹理 | 主页、表格、复选框截图一致（场景 `home-images`、harness `checkbox_toggle`）；内存见 0013/0014 | 通过 |
| 旧 #21 atlas 删除 | 删除图片 tile 时崩溃 | 场景 `images`：滚过 30 张图再滚回，空壳图片丢掉又重新上传 | 通过：没有崩溃，图片都画出来了 |
| 旧 #29 阴影裁剪 | 贴边的阴影被裁掉 | 升级时的截图巡检：菜单、斜杠菜单、弹出框贴边时的阴影和旧版一致 | 通过 |
| 旧 #33 2 张 drawable | 多占一张 drawable（2560×1600 时 16 MB） | 没有移植。实测新旧版的 RSS 都在 2 张和 3 张 drawable 之间跳（相差正好 16 MB，比如无图文档 110/126 MB） | 通过：新旧版分布相同 |
| Windows `VK_PROCESSKEY` | 输入法处理中的退格、回车被处理两次 | — | 等 sida-windows（用小狼毫测） |
| `uniform_list` 内边距 | 主页网格滚到底、长目录滚到底时，底部留白比升级前少 16 / 22pt | 场景 `home-images`（滚到底）和 `catalogue`（80 个标题，滚到底） | 通过：修正后和旧版一致 |

## 没带的上游 crate

`gpui_ios` 暂不复制：`upstream.toml` 的 `crates` 里加上即可。`gpui_web` 已带上，网页版（官网的实时笔记本）用它，见 `web/README.md`。

## 上游带来的、暂未启用的能力

- 无障碍（AccessKit）：各平台的适配器已接好，元素用 `.id(...).role(...)` 标出角色、自定义元素实现 `a11y_role` 等才会暴露给读屏软件（见 `gpui/src/_accessibility.rs`）。应用现在没有标注。
- 编译时间和体积（macOS arm64，从零构建，2026-11-15）：`release` 66.8 秒 → 45.3 秒，二进制 36.6 MB → 35.1 MB；`dist`（LTO）210 秒 → 161 秒，24.97 MB → 24.27 MB。脚本场景的内存占用不变（约 95 MB）。

## 删掉的 vendor

- `vendor/taffy`（0.9.0 加「超出限制」阶段修复）：上游 GPUI 用 taffy 0.15，已包含这个修复（它就是从 0.13 移植的）。
- `vendor/xattr`（Linux `ENOATTR`）：新的依赖树里已经没有 xattr。

## Fei Notes additions

Patch `0035-feinotes-universal-links` ports the previous product `application:continueUserActivity:restorationHandler:` delegate callback to `gpui_macos`. Browsing activities enter the same URL handler as explicit URL opens, preserving CloudKit invitation handling. The old `vendor/gpui` layout is retired with upstream.
