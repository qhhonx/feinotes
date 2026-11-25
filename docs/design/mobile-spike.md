# 移动端验证记录（第 1–2 天）

日期 2026-11-22。对应 [mobile.md](mobile.md) 第 7 节的第 1–2 天。环境：Xcode 27.2，iPad Pro 11-inch (M5) 模拟器，iPadOS 26.5，Rust 1.99.1，debug 构建。测试文档库放在临时目录（用 `QUIET_MD_HOME` 指定）：内置指南用 `shared-mock/guides/build.py` 生成；长文档用 `tools/perf/gen_library.py` 生成（100 万字合订本）。截图在 [mobile-spike/](mobile-spike/)。

**结论：M1（GPUI 上 iOS + 薄原生外壳）可以确认。** 整个应用不改架构就能在 iPad 上跑起来，编辑、中文输入、自动保存都可用。跑的过程中暴露了三个平台层问题，都已经用 vendor 补丁修好，每个补丁几十行。剩下要做的是第 3 节的适配清单，都在 mobile.md 3.1 的估算范围内，没有发现需要换路线的阻塞。

## 1. 第 1 天：跑起来

| 步骤 | 结果 |
|---|---|
| `gpui_ios` 按 vendor 同一个 rev（`e2f00a41`）加入 | 通过。新增依赖只有一个第三方 crate：`objc2-ui-kit`，另外就是 `gpui_ios` 本身；桌面和 web 构建不编它 |
| 上游示例跑在 iPad 模拟器上 | 通过，点击计数正常（`01-hello.png`） |
| 整个应用编成 `aarch64-apple-ios-sim` | 通过。spike 时只有 `trash` crate 编不过（它不支持 iOS）；工作区的 #368 已经去掉了这个依赖 |
| 应用在 iPad 上打开内置指南 | 通过。首页卡片、侧栏、文档、表格、代码高亮、图片、画板缩略图都正常（`02-folder-home.png`、`03-document.png`） |

为了跑起来改的地方（都在本分支上，带说明）：

| 位置 | 改动 | 原因 |
|---|---|---|
| `src/big_alloc.rs` | `MAP_ANON` 按 `target_vendor = "apple"` 取值 | 原来只有 macOS 用 Apple 的值，iOS 落到 Linux 的 `0x20`，所有 ≥1 MiB 的分配都失败，一启动就崩 |
| `src/theme.rs` | 系统字体和等宽字体按 Apple 平台取（`.SystemUIFont`） | 原来 iOS 落到 Linux 分支，用的是 Noto Sans |
| `src/shortcuts.rs` | iOS 用 Mac 的快捷键表（⌘） | iPad 外接键盘用 ⌘ |
| `src/editor/view.rs`、`src/input.rs` | 实现 `text_length_utf16` | `gpui_ios` 要靠这个值判断输入框能不能编辑，不实现就不弹键盘 |
| `packaging/ios/Info.plist` | 加 `UIDeviceFamily` [1, 2] | 不加就以 iPhone 兼容模式（320×480 放大）跑在 iPad 上 |
| vendor 补丁 0039 `ios-font-traits` | 读字体特征时缺失的值用默认值 | iOS 上 Helvetica、Arial 等字体缺少字体特征数值（重量、倾斜等），font-kit 直接 unwrap，结果找不到字体，排版时也会崩 |
| vendor 补丁 0040 `ios-key-events` | 外接键盘按键（`pressesBegan`）、软键盘回车、块首退格都转成 GPUI 按键事件 | 见第 2 节 |
| vendor 补丁 0041 `ios-scene-window` | 窗口大小交给 scene 决定 | iPadOS 26 的 App 是可调整大小的窗口；`gpui_ios` 把窗口设成整屏，内容画出了窗口，旋转后也只显示一部分 |

## 2. 第 2 天：风险点

| 检查 | 结果 | 说明 |
|---|---|---|
| 中文输入法（拼音） | **通过** | 组字中的文字直接显示在正文里并带下划线，候选栏正常，上屏正确，上屏后有联想词（`04-pinyin-marked.png`、`05-pinyin-committed.png`）。外接键盘时候选框的位置还没测 |
| 软键盘回车 | 原来不通过，**加补丁 0040 后通过** | 原来 `insertText("\n")` 被吞掉；现在会分块，空块里显示占位提示（`06-return-splits.png`） |
| 块首退格 | 原来不通过，**加补丁 0040 后通过** | 原来 `gpui_ios` 在可编辑范围的起点直接丢弃退格；现在能和上一块合并 |
| 块内退格、点按定位光标 | 通过 | 光标落点准确 |
| 外接键盘快捷键 | 原来不通过，**加补丁 0040 后通过** | ⌘A 全选，格式工具栏随之弹出（`07-cmd-a-toolbar.png`）；⌘Z 撤销、⌘⇧Z 重做都正常 |
| 触摸点格式工具栏 | 通过 | 点「加粗」后，文件里写成了 `<b>你好</b>`，撤销、重做后也正确 |
| 自动保存 | 通过 | 改动写回 DocxXML，重启后还在 |
| 滚动和惯性 | 通过 | 甩动后继续滑行 2 秒以上。100 万字的文档打开、甩动都流畅（`10-million-char-doc.png`），但这是模拟器加 debug 构建，帧率要在真机上量 |
| 键盘避让 | **不通过（预期内）** | 软键盘挡住了光标所在行（`08-keyboard-covers-caret.png`）。`gpui_ios` 已经把键盘高度放进 safe area 底部，应用要读这个值，并把光标滚到可见处 |
| 顶部安全区 | **不通过（预期内）** | 状态栏压在侧栏收起按钮和标题栏上，应用要读 safe area 顶部 |
| 长按、双击选词 | **不通过（预期内）** | 没反应。要自己做触摸选区：长按选词、两端手柄、`UIEditMenuInteraction`（mobile.md 3.1 估 4–6 天） |
| 旋转和窗口 | 原来不通过，**加补丁 0041 后通过** | 横屏时窗口 806×834，内容正好铺满（`09-landscape-window.png`）。用手拖动改变窗口大小还没测 |
| 旧文档库迁移 | 通过 | 旧格式的库在 iOS 上自动迁成真实文件夹（FORMAT 4） |
| 内存 | 待真机 | 模拟器里打开长文档后 RSS 约 375 MB（debug 构建；模拟器进程跑在 macOS 上，数字不代表真机） |

顺带发现的问题：

- 打开文档时编辑器会自动获得焦点，在 iPad 上就是一打开文档就弹软键盘。触摸设备上应该等用户点进正文才弹。
- 有些交互要靠鼠标悬停，比如块左侧的手柄（指南里写的就是 "Hover to the left of a line"），触摸设备上没法触发，要给触摸单独设计入口。
- 日志里有「全局快捷键注册失败」，iOS 上应该直接不注册。
- 桌面端原有的问题，和 iOS 无关：引导页播放动画时，「跳过」「上一步」按钮重复设置了 `.hover()`。debug 构建里会触发 GPUI 的断言而崩溃，release 构建不受影响（`src/onboarding.rs:715` 附近）。这次 spike 先用生成好的文档库绕开了引导页。

## 3. 适配清单（更新后的估算）

| 项 | 状态 | 估计 |
|---|---|---|
| 编译通 | 已完成（spike 里的改动正式实现时可以保留） | — |
| 平台层补丁（字体、按键、窗口） | 已完成 0039–0041 | 正式化 1 天：外接键盘的方向键、Tab、Esc、⌃ 组合键补测，⌥ 组合输入的字符补测 |
| 回车、退格 | 已完成（0040） | — |
| 安全区、键盘避让、打开文档不弹键盘 | 已完成（`11-safe-area.png`、`12-keyboard-avoidance.png`） | — |
| 触摸选区和编辑菜单 | 长按选词、按住拖动扩展已完成（`13-long-press-word.png`、`14-long-press-drag.png`）；两端手柄和系统编辑菜单（`UIEditMenuInteraction`）待做。实测键盘快捷栏的拷贝按钮不会调用 `copy:`，要从编辑菜单接 | 3–4 天 |
| 悬停类交互的触摸入口（块手柄、行内菜单） | 待做 | 3–5 天 |
| 剪贴板 HTML 和图片 | 待做 | 1–2 天 |
| 原生外壳（文件选择、分享、相册、iCloud 容器） | 待做 | 1 周 |
| 工程和分发（XcodeGen、签名、TestFlight） | 待做 | 3–5 天 |

iPad 可编辑测试版的总估计维持 **5–7 周**；因为编译和平台层比预期顺，偏向下限。

## 4. 包体积和依赖

- 新增依赖：只有 `objc2-ui-kit` 一个第三方 crate（另有 `gpui_ios` 本身）。
- 体积：见下（`--profile dist`，`aarch64-apple-ios`，fat LTO、去符号）。macOS 版作参照：二进制 35.1 MB，zip 24.3 MB（vendor/PATCHES.md）。

| | iOS（本次） | macOS（参照） |
|---|---|---|
| 可执行文件 | 31.9 MB | 35.1 MB |
| zip -9 压缩后 | 14.9 MB（只压了可执行文件） | 24.3 MB（整个 .app） |

iOS 版比 macOS 版略小：iOS 上不编 Mac 专属的模块。App Store 实际的下载大小还要算上图标和 Info.plist，并且是按设备裁剪后再压缩的，大概在 15 MB 左右。

## 5. 第 3 天（按需）

在真机 iPad 上测帧率、内存、发热，以及真机的拼音和手写输入；再测 iCloud 容器（mobile.md 7 第 3 天的三项，和 sync.md D12 共用）。M1 不依赖这两项，可以等进入实现阶段、开始签名真机包时一起做。

## 6. 复现

```sh
rustup target add aarch64-apple-ios-sim aarch64-apple-ios
python3 shared-mock/guides/build.py <空临时目录>
QUIET_MD_HOME=<那个目录> packaging/ios/simulator.sh   # 编译、打包、装到已启动的模拟器并运行，控制台输出在 target/ios/console.log
```

构建入口：`packaging/ios/simulator.sh` 和 `packaging/ios/Info.plist`。这不是发版用的流程：没有 Xcode 工程，只做临时签名。上游示例当时单独建了一个 crate 来跑，没有保留在仓库里。
