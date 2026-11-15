# vendor 补丁

`vendor/gpui` 是 crates.io 的 gpui 0.2.2，`vendor/taffy` 是 taffy 0.9.0，`vendor/xattr` 是 xattr 0.2.3，都打了本地补丁。项目通过 `Cargo.toml` 的 `[patch.crates-io]` 使用这些目录。补丁处都带 `quiet.md:` 注释；完整列表以下表和 diff 为准。

和上游对比：

```sh
R=~/.cargo/registry/src/index.crates.io-*/
diff -ru $R/gpui-0.2.2/src vendor/gpui/src
diff -ru $R/taffy-0.9.0/src vendor/taffy/src
```

升级上游时，按下表逐条重新打补丁，并在补丁处保留注释。

## GPUI 0.2.2

- `platform/windows/events.rs`: `VK_PROCESSKEY` 始终交给 IME，不在组合结束后重新派发成编辑快捷键，避免小狼毫退格、回车被处理两次。

| # | 文件 | 改动 | 用途 |
|---|---|---|---|
| 1 | `platform/mac/text_system.rs` | 光栅化时显式打开字体平滑（`set_allows_font_smoothing(true)`、`set_should_smooth_fonts(true)`） | 与原生 AppKit / Catalyst 文字一致（笔画墨量 5.73 vs 5.74 / 像素宽）；关闭时少约 15% |
| 2 | `platform/mac/text_system.rs` | 字形边界改用和光栅化相同字号的 CTFont 来量 | 系统字体会随字号切换光学尺寸（SF Text → SF Display），原来大标题字形左侧会被切掉一条 |
| 3 | `platform/mac/text_system.rs` | 名字以 `KaTeX_` 开头的字体，即使没有 `m` 字形也允许加载 | KaTeX 的符号字体（Size1–4、AMS、Caligraphic、Script）没有小写字母，原来会被拒绝加载 |
| 4 | `text_system/line_wrapper.rs`、`line_layout.rs` | 中文避头尾：`is_no_break_before`（闭合标点不在行首）、`is_no_break_after`（开标点不在行尾），两处换行算法都使用 | 正文、搜索摘要的中文换行符合中文避头尾规则 |
| 5 | `text_system/line.rs` | `paint_rows` / `paint_background_rows`：按显式给出的每行顶部位置绘制 | 行内含公式或按钮时各行高度不同 |
| 6 | `style.rs`、`styled.rs`、`taffy.rs` | `grid_col_widths(Vec<f32>)`：以像素给出网格列宽，覆盖 `grid_cols`；`repeat` 的字面量补上类型 | 表格列宽 |
| 7 | `elements/list.rs` | `ListState::estimated_scroll_metrics` / `scroll_to_estimated_offset`：未量过的行按平均行高估算 | 长文档的滚动条和拖动滚动条 |
| 8 | `platform.rs`、`platform/mac/platform.rs` | `PathPromptOptions.extensions`：限制 NSOpenPanel 可选的文件类型 | 导入对话框只列出 Markdown / 文本和 .xml 文件 |
| 9 | `platform/mac/window.rs` | `mouseDownCanMoveWindow` 返回 NO，新增 `start_window_move`（延后执行 `performWindowDragWithEvent`） | 透明标题栏不再自己拖动窗口，也不再盖住应用按钮；应用自己标出可拖动区域 |
| 10 | `platform/mac/metal_renderer.rs`、`window.rs`、`platform.rs` | `render_to_bgra` / `Window::render_offscreen`：把场景渲染到离屏纹理再读回；`Window::needs_draw` | 脚本截图和录屏，窗口被遮挡或锁屏时也能用（devtools） |
| 11 | `window.rs` | `DispatchEventResult` 改为 `pub`（`Window::dispatch_event` 的返回类型） | devtools 回放事件时调用 `dispatch_event` |
| 12 | `platform/mac/text_system.rs` | 合成斜体：要斜体而字体没有斜体字面时（含 CoreText 回退到的字体），换成一个「倾斜孪生」FontId，光栅化时错切 14°（WebKit 的合成倾斜角），字形边界相应加宽 | 中文斜体像浏览器一样倾斜；原来苹方没有斜体，中文斜体显示为正体 |
| 13 | `text_system/line.rs` | 文字与背景装饰的场景排序边界包含居中、右对齐后的字形位置及超宽文字 | 图片描述不再穿透浮动工具栏，避免工具栏看起来透明 |
| 14 | `platform/mac/text_system.rs`、`platform/mac.rs`、`platform.rs` | `font_ids_by_postscript_name` 一个名字对应多个 FontId，CoreText 回退来的字体按字形数挑出同一份字体；新增 `check_fallback_glyphs` 供测试调用（`editor::text::tests`） | 系统回退用的苹方（PingFangUI.ttc）和下载的 PingFang.ttc 都叫 `PingFangSC-Regular`，字形表不同；原来按名字查到另一份，编辑器里的中文画成错字 |
| 15 | `platform/windows/directx_renderer.rs` | 调试构建编译着色器时，路径按宽字符串传给 `D3DCompileFromFile`（原来 `to_str().unwrap()`） | 仓库放在含中文等非 ASCII 字符的目录下也能跑调试构建 |
| 16 | `platform/windows/clipboard.rs` | 读剪贴板时文字、图片、复制的文件一起读（原来只取第一个认得的格式）；新增位图（`CF_DIB`，转成 PNG）；复制的图片文件读成图片，其他文件的路径按行分开（原来连成一串） | 截图工具放进剪贴板的位图能粘贴，不再贴出截图文件的路径；从资源管理器复制的图片能粘贴 |
| 17 | `elements/div.rs` | `paint_scroll_listener`：只在一个方向上可滚动的容器，只接收偏向这个方向的滚动（横向量大于竖向量才横滚，反之才竖滚）。原来竖向滚轮在只能横滚的容器上会被转成横滚，触控板竖滑带的少量横向分量也会照单全收；原有的方向锁只在两个方向都有量时才生效，单向容器另一方向恒为 0，所以从不触发 | 竖着滚动经过宽表格时，表格不再跟着左右滑动；在表格上横向滑动仍然能横滚 |

| 18 | `platform/linux/wayland/client.rs`、`platform/linux/x11/client.rs` | 外观变化时先复制窗口句柄，释放 client 的 RefCell 借用后再触发窗口回调 | 避免回调重新访问平台、更新菜单时触发重复借用 panic |
| 19 | `platform/mac/metal_renderer.rs`、`platform/mac/shaders.metal`、`platform/windows/directx_renderer.rs` | 画路径用的两张纹理（中间纹理和 4× MSAA 纹理）改为某一帧有路径时才创建，后续无路径帧距最后使用超过 1 秒时释放；窗口尺寸变化时丢掉，下次用到再建。Apple GPU 上 MSAA 纹理用 `Memoryless`（采样只在片上 tile 内存里，渲染过程里清空、只存 resolve 结果）。mac 上纹理不再按整窗建，而是按每批路径覆盖的范围（`fit_path_textures`，按 128 px 增长、不超过窗口）：路径平移到纹理原点光栅化，`path_sprite_vertex` 多收一个 `texture_rect`（批次在屏幕上的原点和纹理尺寸），`path_sprite_fragment` 按像素直接 `read` 纹素 | 原来每个窗口常驻这两张纹理，2560×1600 时约 80MB；路径只用于连续圆角、文件夹图标、进度波浪和提示箭头，编辑器里平时一条都不画。主页和文件夹页一直有路径，整窗中间纹理会常驻约 16MB，按批次范围后只剩一两张卡片大小。抗锯齿不变；渐变填充按平移后的坐标计算，边缘个别像素可能差 1/255 |
| 20 | `elements/img.rs`、`elements/image_cache.rs` | 图片缓存增加不触发加载的 `peek`；显式尺寸图片可选择延迟到可见区域绘制时才解码 | 卡片裁剪范围之外的图片不再预先解码；原始像素、尺寸和动图播放保持不变 |
| 21 | `platform/mac/metal_atlas.rs` | 删除图片时立即移除 tile 索引，再递减所属纹理的引用计数 | 避免共享纹理仍被使用时残留旧索引，导致重复删除或复用失效 tile |
| 22 | `platform/mac/platform.rs` | 读取剪贴板时同时保留文字和一种图片格式（优先 PNG），跳过空图片数据 | 截图工具同时提供文字和图片时，正文可选择图片，文本输入框仍可读取文字 |
| 23 | `text_system/line_wrapper.rs` | `is_word_char` 把韩文（谚文字母、兼容字母、音节及半角谚文）算作单词字符；上游测试原本断言它不是 | 韩文用空格分词，界面文字按词（어절）换行，不再在词中间断开；超出一行的长词仍按字符断开 |
| 24 | `assets.rs`、`window.rs` | `RenderImage::with_scale_factor`；`RenderImage::hollow` 生成同 id、同尺寸、不带像素的空壳；`paint_image` 遇到 atlas 里没有的空壳时跳过、记下并请求下一帧，由 `Window::take_missing_images` 取走 | 文档图片按显示尺寸解码后，布局仍按原图尺寸；已上传到 atlas 的图片不再在内存里留一份像素（docs/MEMORY.md） |
| 25 | `platform/windows/directx_renderer.rs`、`directx_atlas.rs`、`platform.rs`、`events.rs`、`window.rs` | GPU 设备丢失时：resize 失败后 render target 为 `None`，绘制前先重做（原来 `pre_draw` 直接 unwrap 崩溃，旧 back buffer 还会被二次释放）；渲染器恢复失败可重试，已释放的设备和资源只释放一次，期间不绘制、不 resize，恢复时按窗口当前大小建交换链，下一帧再试；atlas 查不到纹理时跳过这批 sprite；重建 DirectX 设备失败时不再把丢失的设备当新设备发给窗口，下一次 vsync 重试；恢复后重新设置窗口的亚克力模糊（驱动重启后会丢，侧栏后面的窗口不再被模糊） | 显卡驱动重启、睡眠唤醒、驱动更新后应用不崩溃，恢复后正常显示 |
| 26 | `window.rs` | `Window::replace_root` 同时更新窗口句柄记录的根视图类型（`handle.state_type` 和 `App::window_handles` 里那一份） | 首次启动的引导页走完后，在同一个窗口里换成主界面；原来换了根视图后 `downcast::<新类型>()` 仍然失败，按主界面找窗口的地方找不到它 |

| 27 | `platform.rs`、`platform/mac/window.rs`、`platform/windows/window.rs`、`window.rs` | `Window::set_window_min_size`：创建后改窗口最小内容尺寸（macOS `setContentMinSize`，Windows 改 `WM_GETMINMAXINFO` 用的 `min_size`；其他平台不变） | 最小宽度 = 文档区最小宽度 + 侧栏宽度，随侧栏开合和拖宽变化 |
| 28 | `platform.rs`、`platform/mac/window.rs`、`window.rs` | `Window::present_next_frame_in_transaction`：下一帧走 `PlatformWindow::draw_in_transaction`，macOS 上这一帧 `presentsWithTransaction`，和渲染期间改的原生视图在同一个 Core Animation 事务里上屏；其他平台照常 `draw` | 侧栏玻璃（`NSGlassEffectView`）大小或显隐变化的那一帧和 GPUI 画面同时出现；原来画面异步上屏、晚一个刷新，开关侧栏时玻璃先没（先出来），看着像顿了一下 |
| 29 | `scene.rs` | `insert_primitive` 判断阴影是否落在裁剪区外时，按着色器实际画到的范围（四周各多 3 倍模糊半径）算，原来只看阴影本身的矩形 | 阴影的模糊部分落进裁剪区、矩形本身在区外时不再被丢掉（主页拖入文件时，按连续圆角逐行裁剪的内侧光晕） |
| 30 | `text_system/line_layout.rs` | `layout_wrapped_line`：文字含从右到左的字符（希伯来、阿拉伯等）且需要折行时，改走 `wrap_bidi`：按阅读顺序取每个字符的宽度来定折行点，每行单独交给平台排版（行内由平台重排），再把各行首尾相接拼成一个布局，折行点放在每行第一个字形上；每行末尾的空格不参与排版 | 整段一次排版后字形是显示顺序，原来按显示顺序切行，阿拉伯文多行时行序颠倒、从词中间断开；其他文字不走这条路 |
| 31 | `text_system/line_wrapper.rs` | `is_word_char` 把希伯来、阿拉伯、叙利亚、它拿、N'Ko 字母及阿拉伯/希伯来表现形式算作单词字符，并加了测试 | 这些文字用空格分词，按词换行，不再在词中间断开 |
| 32 | `text_system/line.rs` | `WrappedLine::paint` / `paint_background`：折成多行、且第一个字母属于从右到左文字的段落，默认的左对齐改成右对齐（`rtl_align`）；单行文字、显式指定的对齐和编辑器用的 `paint_rows` 不变 | 阿拉伯文等的多行说明文字贴右边排，符合这些文字的阅读习惯 |
| 33 | `platform/mac/metal_renderer.rs` | `CAMetalLayer` 的 `maximumDrawableCount` 从 3 改为 2 | 每张 drawable 是一整窗 BGRA（2560×1600 时 16MB），第三张只在帧堆积时才会建出来并一直留着。我们每帧的 GPU 时间只有一两毫秒，滚动文件夹页时 `nextDrawable` 等待最长 0.17ms，和 3 张时一样 |

## taffy 0.9.0

| 文件 | 改动 | 用途 |
|---|---|---|
| `compute/grid/track_sizing.rs` | 「超出限制」阶段只对受影响的轨道分配空间，增长上限只看 `fit-content()` 参数。这是从 taffy 0.13 移植的修复 | 跨多个 auto 行的合并单元格在 min-content 约束下原来不会撑高这些行 |

## xattr 0.2.3

| 文件 | 改动 | 用途 |
|---|---|---|
| `src/sys/mod.rs` | Linux/Android 下 `ENOATTR` 映射为 `::libc::ENODATA` | 解决现代 libc crate 在 Linux GNU 下没有 `ENOATTR` 常量导致的编译错误 |

## 删掉的上游文件

`gpui/examples/image/black-cat-typing.gif`（4.3 MB）只在 `examples/gif_viewer.rs` 和 `examples/opacity.rs` 里用到，这两个示例不参与本项目的构建；为了减小仓库体积删掉了。
