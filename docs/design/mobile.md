# 设计草案：移动端（iOS / iPadOS 优先）

状态：已定稿，未实现。日期 2026-11-22。决策 M1–M11 由用户确认全部按推荐（2026-11-22），验证计划（第 7 节）已获准开始。本稿用中文写，以后按仓库惯例译成英文。

同步部分以 `docs/design/sync.md`（worktree `sync-plan-8ba6b6`）为准，主页和多个顶层文件夹以 `docs/design/workspaces.md`（worktree `workspace-model-a1ccf8`）为准，本稿只写移动端需要它们提供什么。

两条贯穿全稿的原则（用户定的）：
- **尽量控制包体积、少引入额外依赖。** 验证时记录 `gpui_ios` 加进 vendor 后新增的依赖和 iOS 包的大小（见第 7 节产出）。
- **降低用户的配置成本，越简单越好。** M5 默认把库放在 iCloud 容器、没登录 iCloud 时用沙盒，用户不需要做任何选择。

## 0. 结论先行

- **推荐路线 A′：GPUI 上 iOS + 薄原生外壳。** 编辑器、首页和文档树直接复用根 crate（GPUI 绘制），平台后端用上游 Zed 官方的 `gpui_ios`，从我们 vendor 快照的**同一个 rev** 拿，不引入第二份 GPUI。系统级的东西走原生 UIKit：文件选择、分享、编辑菜单（拷贝/粘贴）、iCloud 文件协调、设置里的系统项。
- **iPad 先做，直接能编辑**，文字类 block 全部可编辑，复杂 block（画板、公式、数据表、分栏拖拽）先只读或沿用桌面交互不做专门适配。
- **文档库默认放在 App 的 iCloud 容器**（文件 App 和 Mac 访达里都显示为 iCloud 云盘下的 `Quiet.md` 文件夹），未登录 iCloud 时退回本机沙盒。
- **iOS 测试版依赖同步第一期的 1a（元数据按设备拆分、锁挪回本机）和 1c（文件监听与静默重载）**，否则 iPad 和 Mac 同时改就会撞 `index.json`。
- **最大的风险不是能不能跑起来**（同一套代码已经有人跑在 iOS 上，见 2.2），而是 `gpui_ios` 很新（2026-10-14 才合入上游）、触摸编辑的细节（选区手柄、硬件键盘快捷键、块边界的退格/回车）要我们自己补。验证计划就盯这几项。

## 1. 现状事实

| 事实 | 位置 | 影响 |
|---|---|---|
| 根 crate 约 6.8 万行，其中编辑器 `src/editor/` 约 2.4 万行、画板视图约 0.8 万行；`fdl_core` 约 4.7 万行 | `wc -l` | 路线 B 要重写的就是 `src/editor/` 这一块 |
| vendor 的 GPUI 是上游 `e2f00a41`（2026-11-15）的快照；`upstream.toml` 列了 19 个 crate，**没带 `gpui_ios`**，注释写"left out for now" | `vendor/zed/upstream.toml`、`vendor/PATCHES.md` 末尾 | 加一行就能从同一 rev 拉进来 |
| 快照里的 `gpui` 已经有触摸手势识别：`TouchEvent`、点击合成鼠标按下/抬起（现有点击和选字逻辑直接可用）、拖动转 `ScrollEvent` 并带**松手后的惯性**、长按 `LongPressEvent`、捏合缩放 | `vendor/zed/crates/gpui/src/gestures.rs` | 触摸和滚动惯性不用我们写 |
| `gpui_platform` 已经有 `target_os = "ios" => gpui_ios::current_platform` 分支，`gpui_apple`（CoreText + Metal 渲染）已经同时面向 macOS 和 iOS | `gpui_platform.rs:64`、`gpui_apple/Cargo.toml:27` | 字体排版、渲染和桌面 Mac 是同一套代码 |
| 根 crate 有 121 处 `cfg(target_os = "macos")`；macOS 专属依赖只有 `core-graphics`、`core-foundation`、`objc`（这三个在 iOS 上也能编） | `Cargo.toml:87` | 编到 iOS 的主要工作是把 macOS 分支改成 `apple`/按功能区分，工作量可控 |
| 已有 web 构建：`fdl_core::vfs` 把文件系统抽象出来，web 上是内存库；`web_time`、`PathExt` 等平台差异已经理过一遍 | `web/README.md`、`src/web.rs` | 平台差异的"缝"已经存在，iOS 照着加 |
| 编辑器的输入法接口只暴露**光标所在块**的文字（`head_text`），marked text 记在 `Editor::marked` | `src/editor/view.rs:2791` | 中文组字在块内没问题；块边界的退格、回车要另外处理（见 3.1） |
| 官网 live demo 在手机上只显示截图 `home.webp`，不跑 wasm | `web/README.md` 网站一节 | 说明 web 构建目前没为移动浏览器调过 |

## 2. 参考

### 2.1 上游 `gpui_ios`（读的是上游 main `d31d1074`，2026-11-22）

约 3700 行，作者是 Zed 团队成员。提交历史：2026-10-14 首次合入（#63068），之后一个多月里加了触控板滚动、Universal Links、屏幕圆角和边缘返回手势、清理代码，共 11 个提交。

| 能力 | 状态 | 说明 |
|---|---|---|
| 生命周期 | 有 | UIScene：连接/断开、进入前后台、激活/失活；`applicationDidReceiveMemoryWarning`、`WillTerminate` |
| Metal 渲染 | 有 | `CAMetalLayer` + `CADisplayLink`，走 `gpui_apple` 共享渲染器 |
| 触摸 | 有 | 原始 `UITouch` 转 `TouchEvent`，由 gpui 的手势识别器处理（点击、拖动、惯性、长按） |
| 触控板 / 鼠标滚轮 | 有 | `UIPanGestureRecognizer`（间接指针） |
| 输入法 | 有，较完整 | 自己实现 `UITextInput`：`setMarkedText:selectedRange:`、`unmarkText`、`firstRectForRange`（候选框定位）、`caretRectForPosition`、`selectionRectsForRange`、tokenizer、书写方向；有原生协议冒烟测试 |
| 键盘避让 | 有，交给 App | 监听键盘 frame，把键盘高度放进窗口的 safe area 底部；向下拖动收键盘 |
| 安全区 | 有 | 刘海、Home 指示条 |
| 边缘返回手势 | 有 | `on_back_gesture` |
| 剪贴板 | **只有纯文本** | 富文本 HTML 和图片要补 |
| 文件选择 / 另存 | **未实现** | `prompt_for_paths` 直接返回 "not yet implemented"；要原生 `UIDocumentPickerViewController` |
| 硬件键盘快捷键 | **没有** | 没有 `pressesBegan`/`UIKeyCommand`，不产生 `KeyDownEvent`。外接键盘打字走 `insertText` 能用，但 ⌘B、方向键、Tab 等不会到达 GPUI 的 action 系统。iPad 先做的话这是必补项 |
| 系统选区手柄 / 放大镜 / 编辑菜单 | **没有** | 没接 `UITextInteraction`、`UIEditMenuInteraction` |
| Apple Pencil 悬停、指针悬停 | 没有 | |

公开使用上游 `gpui_ios` 的 App：没找到。

### 2.2 feinotes（飞记）：同一套代码已经跑在 iOS 上

feinotes 是 Quiet.md 的 fork（`qhhonx/feinotes`，最近同步到上游 11-17），已经在 TestFlight 上同时支持 iPhone 和 iPad。读的是它 main `78a2c6c`：

- 做法和 A′ 一样：根 crate 整个编成 staticlib，**编辑区全部由 GPUI 绘制**；Swift/UIKit 外壳约 7200 行，负责导航、笔记列表、iPad 三栏、编辑工具栏、查找（UIFindInteraction）、设置、各种表单、打印、提醒。整个 App 只有一个 GPUI 视图，在各页面之间挪。
- 根 crate 里加了 `src/mobile*.rs`、`src/ipad*.rs` 约 2400 行，`cfg(target_os = "ios")` 约 900 处。
- 平台后端用的不是上游 `gpui_ios`，而是 Longbridge 的 `gpui-mobile`（锁 commit，打了 9 个补丁：iOS 17 字体、剪贴板、URL、**软键盘回车映射成 Enter action**、相册、文件选择器、emoji 边界……），GPUI 本身另锁 `gpui-pre` 0.3.10。**代价是 iOS 和桌面用两份 GPUI**，各打各的补丁。我们用上游 `gpui_ios` 正好避开这一点。
- 触摸选择（他们的 build 8）：长按用 Core 的分词选中一个词，44 pt 的端点手柄由 GPUI 自己画，拖动扩展 Core 的选区；拷贝粘贴菜单用 `UIEditMenuInteraction`，调用现有编辑器 action。**没有另写一套选区引擎**。这是我们可以直接参照的思路。
- 存储换成了 SQLite，同步用 Loro CRDT + CloudKit。这和我们"文件夹是唯一真相"的方向不同，不照搬（sync.md 已定）。
- 授权：上游部分 MIT，他们自己写的模块是 PolyForm Noncommercial。**只参考思路，不拷代码。**

## 3. 三条路线

### 3.1 A：GPUI 上 iOS

把 `gpui_ios` 加进 vendor，根 crate 编成 `aarch64-apple-ios` 的 staticlib，Xcode 工程只放 `main` 和 Info.plist、entitlements。

要我们做的事（不含同步）：

| 项 | 内容 | 估计 |
|---|---|---|
| 编译通 | vendor 加 `gpui_ios`；121 处 macOS 分支按功能重新归类（Apple 共有 / 仅 macOS / 仅桌面）；关掉全局热键、自动更新、托盘、打印、Quick Look、显示于访达等桌面专属功能；字体枚举换成 iOS 可用的 | 3–5 天 |
| 输入适配 | 软键盘回车 → 分块（现在 `insertText("\n")` 会进 `type_text`）；块首退格 → 合并上一块（`gpui_ios` 在可编辑范围起点直接丢掉退格）；中文组字在块边界、表格单元格里的表现 | 2–4 天 |
| 硬件键盘 | 给 `gpui_ios` 补 `pressesBegan/Ended` → `KeyDownEvent`，作为 vendor 补丁（也可以提给上游） | 2–3 天 |
| 触摸选区 | 长按选词、双端手柄拖动、`UIEditMenuInteraction` 剪切/拷贝/粘贴/全选，调用现有 action | 4–6 天 |
| 布局 | iPad：现有双栏布局基本可用，侧栏可收起；应用 safe area 和键盘高度；放大触控目标（菜单行、块手柄）。iPhone：单栏，侧栏变成全屏列表 | iPad 3–5 天，iPhone 另 1–2 周 |
| 剪贴板 | HTML 和图片读写（复用现有 HTML 粘贴路径） | 1–2 天 |
| 原生外壳 | 文件选择（导入、插图片/附件）、分享、相册、iCloud 容器初始化与文件协调 | 1 周 |
| 工程与分发 | XcodeGen 工程、`build-rust.sh`、签名、TestFlight 流水线、隐私清单 | 3–5 天 |

合计 iPad 可编辑测试版约 **5–7 周**（一个人加 agent），iPhone 再加 1–2 周。

- 复用率：编辑引擎 100%，编辑器视图和渲染约 90%+，界面整体约 80%。
- 体验上限：和桌面完全一致，所有 block 第一天就能显示；但系统级手感（选区放大镜、系统文字服务、听写、Writing Tools、VoiceOver）要逐项补，有些（Writing Tools、系统放大镜）可能永远做不到原生的程度。
- 风险：`gpui_ios` 只有一个多月历史，可能有我们第一个踩到的坑；绘制全部自管，长文档的电量和发热要量。
- 维护成本：最低。桌面新功能自动到移动端，只需偶尔加触摸适配。vendor 升级时 `gpui_ios` 跟着一起升。

### 3.2 B：原生 SwiftUI/UIKit + `fdl_core`

UniFFI 把 `fdl_core` 的模型、DocxXML、store、Markdown、`editor::Core` 导出给 Swift，界面全部原生，编辑器用 TextKit 2 重写。

- 要重写的是 `src/editor/` 的 2.4 万行对应的功能：块树渲染（嵌套列表、引用、提示块、分栏、表格、代码块高亮、公式、图片裁切、文件卡片、倒计时、按钮、提醒）、选区跨块、拖拽、斜杠菜单、行内对象（U+FFFC）。TextKit 2 适合"一篇连续文字"，对**块树 + 表格 + 分栏**要自己做 `NSTextLayoutFragment`/附件视图，Apple 备忘录级别的效果要大量工作。保守估计做到桌面七成功能要 **4–6 个月**，之后每个桌面新功能都要做两遍。
- 格式一致性：写盘全部走 `fdl_core`，所以文件往返有保证（`all-blocks.xml` 照样逐字节往返）；但**显示和编辑行为**无法保证一致，需要另建一套跨端对照测试。
- `editor::Core` 的 API 以"块 id + 字节偏移"表达选区，要和 TextKit 的 UTF-16 `NSTextRange` 双向映射，每个结构操作后要重建 text storage 的对应段，这层胶水本身就是新的 bug 来源。
- 体验上限：最高（系统选区、放大镜、Writing Tools、听写、VoiceOver、Stage Manager 全部原生）。
- 结论：只有在 A′ 被验证不可行，或者将来要做一个和桌面很不一样的"移动优先"产品时才值得。

### 3.3 C：web 构建装进 WKWebView

- 现成的：`gpui_web` 已有 IME 镜像（隐藏 textarea + composition 事件）、触摸指针、单线程运行；官网 demo 能用。
- 问题：
  - 文件：web 版的库在内存里（`vfs`），装进 WKWebView 后每次读写都要过 JS ↔ Swift 消息桥，iCloud 文件协调、占位文件都在原生侧，等于把文件层再写一遍。
  - 文字：浏览器里走 canvas 文字（`canvas_text.rs`），中文靠浏览器回退字体，排版和桌面 CoreText 不一致。
  - 性能：wasm 单线程，图片解码在主线程；压缩后 6 MB wasm；iOS 上 WKWebView 的 WebGPU/WebGL 和内存上限都更紧。
  - 输入法：隐藏 textarea 方案在 iOS Safari 上候选框定位、组字中断、键盘弹收都是已知的麻烦区，网站在手机上目前直接放截图。
- 结论：不推荐作为产品路线；只适合"官网手机上也能点一点"的展示用途。

### 3.4 混合方案

| 方案 | 说明 | 评价 |
|---|---|---|
| **A′：GPUI 为主 + 薄原生外壳（推荐）** | 首页、侧栏、文档树、编辑器都用 GPUI（就是桌面那套）；原生只做系统对接：文件选择、分享、编辑菜单、iCloud 协调、外部打开 | 改动最小，和桌面保持一致；以后觉得某个页面手感不够，再逐个换成原生 |
| A″：原生导航 + GPUI 只做文档视图 | feinotes 的做法：列表、设置、表单全部原生，整个 App 一个 GPUI 视图挪来挪去 | 手感更"iOS"，但列表、设置、各种表单都要用 Swift 再写一遍（他们 7200 行），桌面改了这些页面移动端要跟着改。可以作为 A′ 之后逐步演进的方向，不建议一开始就做 |
| B′：原生 + GPUI 只渲染复杂 block | 文字用 TextKit，表格/画板嵌 GPUI | 两套文字系统混排，选区跨越边界极难，不考虑 |

### 3.5 对比表

| | A′ GPUI + 薄外壳 | A″ 原生导航 + GPUI 文档 | B 全原生 + fdl_core | C WKWebView |
|---|---|---|---|---|
| 复用率 | 引擎 100%，界面约 80% | 引擎 100%，编辑器 90%，其他页面 0% | 引擎 100%，编辑器 0% | 引擎和界面约 95%，文件层要重做 |
| 体验上限 | 和桌面一致；系统手感逐项补 | 导航原生，文档区同 A′ | 最高 | 最低 |
| iPad 可编辑版工作量 | 5–7 周 | 8–12 周 | 4–6 个月 | 3–4 周到能用，打磨无底 |
| 主要风险 | `gpui_ios` 新；触摸编辑细节自补 | 同 A′，外加两套 UI 同步 | TextKit 2 做块树；行为不一致 | 输入法、性能、文件桥 |
| 维护成本 | 低，桌面功能自动到达 | 中 | 高，功能做两遍 | 中，浏览器差异 |
| 和桌面的格式一致性 | 天然一致（同一份代码） | 天然一致 | 文件一致，行为要另测 | 天然一致 |

## 4. 推荐和理由

推荐 **A′**，理由按权重：

1. **维护成本是决定性的。** 项目还在快速迭代（数据表、画板、CLI 都在推进），任何"功能做两遍"的路线都会让移动端永远落后。
2. **可行性已经有旁证。** feinotes 用一个更旧、更不正式的 iOS 后端，把同一套编辑器跑到了 TestFlight，拼音输入、软键盘、查找都已适配。
3. **我们的条件比 feinotes 更好。** 上游官方 `gpui_ios` 和我们 vendor 的 GPUI 是同一个仓库的同一个 rev，触摸手势、惯性滚动已经在 `gpui` 里，不用维护两份 GPUI。
4. 走不通时的退路清楚：A′ 的原生外壳可以逐步加厚成 A″；`fdl_core` 本来就不依赖 GPUI，B 随时可以起步。

## 5. 第一版范围

### 5.1 iPad 先，还是 iPhone 先

**推荐 iPad 先。**

- 现有双栏布局在 iPad 横屏上基本可以直接用，iPhone 要重新设计单栏导航；
- iPad 用户常配外接键盘，桌面的快捷键和编辑习惯能平移过去（前提是补上硬件键盘事件）；
- 屏幕大，触控目标的问题少，第一版能更快到"真的能用"。

iPhone 放在第二个里程碑：单栏（首页 → 文档），侧栏变全屏列表，编辑工具栏放在键盘上方。

### 5.2 只读还是直接能编辑

**推荐直接能编辑。** 走 A′ 时编辑能力是现成的，只读版本几乎省不了工作量（滚动、选择、拷贝、链接都要做），反而价值低很多。为了控制风险，分两级：

| 级别 | block | 移动端处理 |
|---|---|---|
| 可编辑 | 正文、标题、各类列表、待办、引用、提示块、代码块、分割线、链接、行内样式、@ 提及、图片（插入、查看、删除） | 第一版完整支持，含中文输入、触摸选区、拷贝粘贴 |
| 可显示、编辑沿用桌面交互 | 表格、分栏、公式、文件卡片、倒计时、按钮、提醒、目录 | 显示正常；编辑能用但不专门做触摸优化，问题记下来逐个修 |
| 只读 | 画板、数据表（数据表本来就还没放出） | 显示缩略图/静态内容，点开可以缩放平移查看；编辑放后续版本 |

斜杠菜单保留（外接键盘和软键盘都能输 `/`），另在键盘上方加一条插入/格式工具栏。

### 5.3 不在第一版

Git 同步、导入导出（Obsidian/Markdown 导入除外的其他格式）、打印、版本历史抽屉的恢复操作（可看不可恢复，或者整个推迟）、全局快捷键、主题编辑、多窗口（iPad 多场景）、Apple Pencil。

## 6. 和同步、文档库位置、分发的关系

### 6.1 和同步的关系

iOS 本身不需要新的同步机制：**iCloud 云盘把文件夹同步过来，App 只是又一个"设备"**。所以移动端完全建立在 sync.md 的第一期上：

| sync.md 的步骤 | iOS 是否依赖 | 说明 |
|---|---|---|
| 1a FORMAT 5：元数据按设备拆分、锁和缩略图挪回本机、设备 id | **必须** | 否则 iPad 和 Mac 同时改，`index.json` 一定冲突 |
| 1b 三方合并 + 冲突副本 | 强烈建议 | 没有它，两边改同一篇时只能"另存一份" |
| 1c 文件监听 + 静默重载 + 基准快照 | **必须** | iOS 上 App 在后台时文件会被改，回到前台要能发现 |
| 1d iCloud：`NSFileCoordinator` 写入、`NSFileVersion` 冲突、占位文件下载 | **必须**，而且 iOS 上更严格 | iOS 上访问 iCloud 容器**必须**经过文件协调；占位文件要主动 `startDownloadingUbiquitousItem` |
| 1e Git | 不依赖 | iOS 第一版不做 Git（sync.md D1 已经说 iOS 再换实现） |

需要和同步会话对齐的三点，**均已写进 sync.md**（worktree `sync-plan-8ba6b6`，2026-11-22）：

1. sync.md 的 1d 现在只写了"macOS 上写入改为经过 `NSFileCoordinator`"，建议改成"**Apple 平台**"，vfs 里那层实现 macOS 和 iOS 共用。
2. sync.md D12 推荐"第一期任意文件夹，做 iOS 时再加 App 容器，先验证 Developer ID 能否用容器"。本稿推荐的做法**不需要 Mac 端有容器权限**（见 6.2），这个验证可以从阻塞项里拿掉，但要验证"Mac 上没装带容器权限的 App 时，容器文件夹是否照样同步到 Mac"。
3. iOS 上 App 随时可能被挂起或杀掉：保存的 500 ms 防抖要在 `sceneWillResignActive`/进入后台时立刻 flush，并申请后台时间。

### 6.2 文档库放在哪

| 选项 | 优点 | 缺点 |
|---|---|---|
| **App 的 iCloud 容器**（`iCloud~com~s1dashu~quietmd/Documents`，设为公开） | 文件 App 和 Mac 访达里都显示为 iCloud 云盘下的 `Quiet.md` 文件夹；系统负责同步；Mac 端是非沙盒 App，直接按路径 `~/Library/Mobile Documents/iCloud~com~s1dashu~quietmd/Documents` 打开就行，不需要 Mac 端加权限 | 需要 iOS App 有 iCloud Documents 权限（我们的团队可以开）；容器只在登录 iCloud 时可用 |
| App 沙盒（"我的 iPad 上"） | 最简单，离线可靠 | 不能和 Mac 同步，只能靠导入导出 |
| 用户在文件 App 里选的任意文件夹（安全作用域书签） | 能直接打开 Mac 上已有的 iCloud 云盘库、第三方网盘 | 书签可能失效；第三方 File Provider 的行为不可控；每次访问要 `startAccessingSecurityScopedResource` |

**推荐：默认 iCloud 容器；未登录 iCloud 时用沙盒；"打开其他文件夹"放到第二版。** Mac 端配合：设置里加"移到 iCloud 云盘（和 iPhone/iPad 共用）"，把库移动到容器路径；新建库时也给这个选项。这和 sync.md D12"第一期任意文件夹"不冲突，容器只是一个推荐位置。

和顶层文件夹模型（`docs/design/workspaces.md` 第 4 节，worktree `workspace-model-a1ccf8`）的对应关系：

- 主页是本机的一组顶层文件夹，每台设备各一份，不同步，每个顶层文件夹一个 `Store`。所有顶层文件夹地位平等（workspaces.md 决策 W4）。**iOS 第一版的主页固定只有一个顶层文件夹**：iCloud 容器里的那个文件夹，没登录 iCloud 时用沙盒里的。iOS 上不提供移除，因为第一版没有别的顶层文件夹可以加。这是第一版的产品范围，不代表这个顶层文件夹有特殊地位。直接用 core 里同一个主页门面（`workspace.rs`，不依赖 GPUI），不另写单库代码。
- 第二版"打开其他文件夹"就是往主页里加一个顶层文件夹。`folders.json` 的每一项预留 `bookmark` 字段，存安全作用域书签；书签只在本机有效，正好对应顶层文件夹列表不同步。
- 全局设置（偏好、主题、快捷键、界面状态）不放在任何文件夹里，存在本机，iOS 上就是 App 沙盒。跨设备同步设置由 sync.md 的 D5 决定。
- 桌面上指向其他顶层文件夹的跨文件夹链接，在 iOS 上会变成断链。所以顶层文件夹方案第一步不允许新建跨文件夹链接；以后允许时也只写相对路径，并给出提示。断链分两种状态（顶层文件夹方案已采纳）：目标解析后不落在任何已打开的文件夹里，提示"这篇文档在没有打开的文件夹里"；落在已打开的文件夹里、但文件不存在，才提示"已删除或移走"。
- `folders.json` 每项是 `{name, path, bookmark, added}`。iOS 上只认 `bookmark`，每次启动重新解析；书签失效就把这个顶层文件夹当成不可用，侧栏显示灰色，并提供"重新查找"。
- workspaces.md 决策 W17：引导页里"新建文件夹"在 Mac 上的默认位置，推荐登录 iCloud 时用这个容器。**它和本稿 6.2 的推荐都取决于第 7 节第 3 天的容器验证**；验证通过之前，默认位置维持 `~/Documents/Quiet.md`。

### 6.3 分发

- 签名团队用现有的 `9UXM7M6CX5`。Bundle id 建议和 Mac 共用 `com.s1dashu.quietmd`，以后可以做 Mac App Store 的通用购买；如果担心和现有 Developer ID 版冲突，iOS 用 `com.s1dashu.quietmd.ios` 也行（见决策 M8）。`CFBundleDisplayName` 定为「Quiet.md」，和 Mac 的 `scripts/release.sh` 保持一致。iCloud 容器在文件 App 和访达里的名字、主页里这个顶层文件夹的显示名（workspaces.md W20），都取自这个值。
- 要在开发者后台建：App ID（iCloud Documents 能力）、iCloud 容器 `iCloud.com.s1dashu.quietmd`、App Store Connect 的 App 记录。
- 先内部测试（TestFlight 内部测试员，不需要审核），稳定后开外部测试（要过 Beta 审核）。
- 工程：`packaging/ios/`，XcodeGen 生成工程，`build-rust.sh` 编 `aarch64-apple-ios` 和 `aarch64-apple-ios-sim` 的 staticlib；最低系统版本建议 iOS 17（feinotes 同样是 17，`gpui_ios` 代码里也照顾到了 15/16）。
- 要准备：隐私清单 `PrivacyInfo.xcprivacy`、出口合规（不含自定义加密）、App 图标各尺寸、截图。
- 开源：iOS 工程放在本仓库。签名、证书、描述文件不进仓库，写进 `CLAUDE.local.md`。

## 7. 验证计划（1–3 天）

> 第 1–2 天已完成（2026-11-22），M1 确认可行。结果、截图、新增的三个 vendor 补丁和更新后的适配清单见 [mobile-spike.md](mobile-spike.md)。

目标：用最小代价回答"A′ 走不走得通"。每一项都有明确的通过标准；任何一项失败，就停下来重新评估路线。全部在本 worktree 的分支上做，用临时文档库（`QUIET_MD_HOME`），不影响 main，结束时写一份结果记录。

### 第 1 天：跑起来

1. 用 `tools/vendor-sync` 把 `crates/gpui_ios` 按同一 rev `e2f00a41` 加进 vendor，补齐它要的 workspace 依赖（`objc2-ui-kit`、`objc2-quartz-core` 等）。
   - 通过：`cargo build -p gpui_ios --target aarch64-apple-ios-sim` 成功；上游示例在 iPad 模拟器上能显示、点击计数会变。
2. 根 crate 编成 iOS staticlib：先粗暴地把桌面专属模块 cfg 掉，入口打开一个窗口，指向用内置指南初始化的临时库。
   - 通过：iPad 模拟器里能看到首页和侧栏，点开一篇指南能显示，所有 block 类型渲染不崩。

### 第 2 天：最大的几个风险

| 检查 | 方法 | 通过标准 |
|---|---|---|
| 中文输入法 | 模拟器加简体拼音键盘，软键盘输入"你好世界"，组字中改选候选、删除、在列表项和表格单元格里输入 | 候选框贴着光标；上屏正确；组字中途切块不丢字、不重复 |
| 回车和退格 | 段落中回车、空列表项回车、块首退格 | 能分块/退出列表/合并块；不行就记下要补的适配点（预期要改） |
| 触摸与滚动 | 长文档（`tools/perf/gen_library.py` 生成）拖动、甩动、点按定位光标 | 有惯性；点按落点准确；滚动 60fps 以上没有明显卡顿 |
| 键盘避让 | 在文末输入 | 光标不被键盘挡住（预期要在根 crate 里应用 safe area，量一下改动） |
| 选择 | 长按、双击选词，拖动 | 记录现状，评估自画手柄的工作量 |
| 硬件键盘 | 模拟器接 Mac 键盘按 ⌘B、方向键 | 预期不通；确认缺口确实只在 `gpui_ios` 的按键事件，试写 `pressesBegan` 补丁看能否打通 |
| 旋转和分屏 | 横竖屏切换、Split View 1/3 宽 | 布局不崩，窄宽度下可用 |

### 第 3 天：真机和 iCloud（可选，看前两天结果）

1. 用团队 `9UXM7M6CX5` 签名装到真机 iPad，测内存、电量、发热，以及真机拼音和手写输入。
2. 建 iCloud 容器，iOS 侧写一个测试文件夹，看：
   - Mac 访达里 iCloud 云盘下是否出现 `Quiet.md` 文件夹（**Mac 上不装任何带容器权限的 App 时**）；
   - Mac 上的非沙盒进程按路径读写，iPad 侧能否收到变化，文件协调和 `NSFileVersion` 冲突是否照常；
   - 开着"优化 Mac 存储"时，容器里的文件会不会变成占位文件。
   - 初步证据（2026-11-22，本机实测）：`~/Library/Mobile Documents/` 下有只在 iPhone 上装过的 App 的容器，比如 `iCloud~com~lamborghini~connectedcar`、`iCloud~host~exp~Exponent`，这些 App 在 Mac 上都没装，容器却照样出现了。非沙盒进程调用 `FileManager.displayName(atPath:)` 和 URL 的 `localizedName`，返回的都是 App 名。说明第一项（容器出现在 Mac 上）和顶层文件夹方案 W20（顶层文件夹的显示名就用系统给容器的显示名）大概率成立，还要验证双向写入。另外，在 iOS 模拟器里也要调一次 `localizedName`，确认返回的是 App 名。
   - 三项和 sync.md D12 的实测清单相同，做一次两边都用。不通过就退回 D12 的备选：桌面端加 iCloud 权限，并验证 Developer ID 签名能否使用。
   - 通过：两边互相看到对方的修改。这一条决定本稿 6.2 的推荐能否成立，也决定顶层文件夹方案的 W17（Mac 上新建文件夹默认放进容器）。即使前两天就决定了路线，这一条也值得单独做。

### 产出

一份 `docs/design/mobile-spike.md`：每项结果、截图、要补的适配点清单、更新后的工作量估计，以及新增依赖清单和 iOS 包大小（staticlib 和 .app，release 构建）。spike 的代码不合并，正式实现时按决策重新做。

## 8. 待拍板的决策点

| # | 问题 | 选项 | 推荐 |
|---|---|---|---|
| M1 | 技术路线 | A′ GPUI + 薄外壳 / A″ 原生导航 + GPUI 文档 / B 全原生 / C WKWebView | **A′**，以后按需加厚成 A″ |
| M2 | iOS 平台后端 | 上游 `gpui_ios`（同一 rev vendor） / Longbridge `gpui-mobile` | **上游 `gpui_ios`**，不维护两份 GPUI；缺的能力作为 vendor 补丁，合适的提给上游 |
| M3 | 先做哪个设备 | iPad 先 / iPhone 先 / 同时 | **iPad 先**，iPhone 第二个里程碑 |
| M4 | 第一版能力 | 只读 / 直接能编辑 | **直接能编辑**，按 5.2 分三级 |
| M5 | 文档库位置 | iCloud 容器 / 沙盒 / 用户选文件夹 | **默认 iCloud 容器，未登录用沙盒**，用户选文件夹放第二版 |
| M6 | 和同步的先后 | 等 sync 1a+1c(+1d) 做完再发测试版 / 先发不同步的单机版 | **开发并行，TestFlight 等 1a+1c+1d 的 Apple 部分**；在那之前只做内部单机测试 |
| M7 | 硬件键盘快捷键 | 第一版必须 / 以后 | **第一版必须**（iPad 先做的前提） |
| M8 | Bundle id | 和 Mac 共用 `com.s1dashu.quietmd` / 单独 `…quietmd.ios` | **共用**，以后方便通用购买；要先确认对现有 Developer ID 版和自动更新没有影响。显示名两端都用「Quiet.md」 |
| M9 | 最低系统 | iOS 17 / iOS 18 | **iOS 17**，spike 时确认 `gpui_ios` 没用到 18 才有的接口 |
| M10 | iOS 工程放哪 | 本仓库 `packaging/ios/` / 单独私有仓库 | **本仓库**，签名材料不进仓库 |
| M11 | 是否先跑 1–3 天验证 | 先验证 / 直接开工 | **先验证**，第 1–2 天的结果足以决定 M1 |
