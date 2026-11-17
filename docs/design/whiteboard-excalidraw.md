# 画板：按 Excalidraw 源码移植

状态：已实现（2026-11-14；2026-11-15 样式改为完全照 Excalidraw）。取代 `feat/whiteboard` 上参照飞书的第一版（只是尝试，从未发版）。本文记录做成了什么、和上游的差别、还缺什么；细节以代码和各文件开头的注释为准。

## 已定的事

- 交互、数据模型、算法以 Excalidraw 源码为准，逐模块移植。
- 样式 100% 照 Excalidraw 原样，不跟随应用 `Theme`（2026-11-15 定，取代先前「用 Theme、不画手绘」的决定）：画布用移植的 rough.js 按元素的 `roughness` / `seed` 画手绘线条；字体用 Excalidraw 自带字体；深色模式用上游的反色滤镜；工具栏、面板、菜单、选框手柄照上游 CSS 数值和上游图标。
- 不参考 tldraw（专有授权）。
- 存储用 Excalidraw 元素 JSON；旧画板数据不迁移（2026-11-14 定）。
- 模型、几何、算法、撤销放 `crates/core/src/board/`（不依赖 GPUI）；工具状态机、渲染、外壳放根 crate `src/board/`（`pub use` 了 core 的模块，`crate::board::scene` 等指向 core）。

## 上游基线

- `github.com/excalidraw/excalidraw` @ `3a317b55f508815d10a4c12acca99df30f026521`（2026-11-14，包版本 0.18.0），MIT。整个移植钉住这个提交；以后升级按 diff 逐文件跟。
- 每个移植出来的 Rust 文件开头注明对应上游文件和与上游的差别；THIRD_PARTY.md「移植的源码」列了 Excalidraw、rough.js 4.6.4 及其依赖（hachure-fill、points-on-curve、points-on-path、path-data-parser）、perfect-freehand、fractional-indexing、nanoid。

## 渲染

画板内容先生成**显示列表**（core `display.rs`，纯数据：路径、填充、描边、文字、图片），由后端画出来：

1. **GPUI 后端**（全平台，`src/board/render.rs`）：显示列表 → `PathBuilder` / 文字 / 图片。按元素缓存显示列表（`version` / `versionNonce` 变了才重建）和按缩放档（四分之一倍频程）缓存的三角化路径；场景不变（平移、缩放、悬停）时复用上次的绘制顺序和外框，只做外框裁剪。GPUI 不能旋转文字和图片、只能按轴对齐矩形裁剪：旋转的文字画成 SVG 小图，旋转 / 翻转 / 裁剪 / 分区裁剪的图片在 CPU 上重采样，分区裁剪和箭头标签的镂空按三角形切路径（`render_geom.rs`）。
2. **vello_cpu 后端**（三个平台相同，`src/board/render_vello.rs`）：可见元素 ≥ 800 时切过去，< 500 时切回 GPUI（迟滞）；缩略图从 64 个可见元素起。画进像素图、转成 BGRA 拷进三帧轮换的 `PixelFrame`，用 `Window::paint_pixels` 当图片精灵显示（vendor/PATCHES.md #36）。文字由 GPUI 文字系统排版，画字形轮廓（CoreText / DirectWrite / swash，#37；彩色 emoji 没有轮廓，不画）。场景、视图、主题都没变时直接复用上一帧。

阈值和压测数字见 tools/perf/board-results.md。选框、手柄、吸附线、文字编辑框、工具栏、面板一律 GPUI 原生元素，叠在内容之上（`overlay.rs`，对应上游 `interactiveScene`）。

手绘线条：`crates/core/src/board/rough/` 是 roughjs 4.6.4 的逐行移植（生成器、渲染器、全部填充方式、带种子的随机数，取随机数的顺序和 roughjs 一致），`shape.rs` 移植上游 `shape.ts` 的 `generateRoughOptions` 等。`crates/core/tests/board_rough.rs` 用真 roughjs 生成的 86 组结果（`fixtures/board/rough.json`，由同目录 `rough_gen.cjs` 生成）逐个比对到 1e-6。

深色模式：`color.rs` 的 `apply_dark_mode_filter` 移植上游 `applyDarkModeFilter`（`invert(93%) hue-rotate(180deg)` 的逐色等价运算），画布底色和元素颜色都经过它；上游不滤的（便签阴影等）走 `display::theme_color`。

## 存储

- **文档里**：`<whiteboard type="excalidraw" height="…">` 的正文是场景 JSON：`{"elements":[…],"files":{…}}`。图片文件放 `.assets`，`files` 里只记 `id → 路径 + mime`，不内嵌 dataURL。写出稳定：元素按 `index`（分数索引）排序，每个元素一行，键按上游 `types.ts` 的声明顺序，未知键原样保留在末尾。`all-blocks.xml` 里有规范写法，保证原样往返。格式见 docs/FORMAT.md。
- **独立画板文件**：`.excalidraw`，官方 JSON（`type`、`version`、`source`、`elements`、`appState`、`files`），可用 excalidraw.com 打开。`DocKind::Board` 对应扩展名 `excalidraw`。旧版本跳过不认识的扩展名，不需要加 `store::FORMAT`。
- `appState` 只存跟随文件的部分（`viewBackgroundColor`、`gridSize`、`gridStep`、`gridModeEnabled`），视口、选中、工具等不存（与上游 `cleanAppStateForExport` 一致）。
- 读入一律经 `restore.rs`（移植 `data/restore.ts`）：补默认值、修旧字段、修绑定、修分数索引。PNG / SVG 里内嵌的场景也能读（`encode.rs`）。

## 和文档的集成

- **block**：`BlockKind::Board(Arc<Board>)`。正文里显示静态缩略图（`src/editor/board.rs`，按元素版本缓存，画板打开时不画）；双击或点「编辑」打开画板：画板取代文档占满笔记区，上面仍是应用的顶栏，面包屑末尾是「文档名 › 画板」，点文档名回到文档，侧栏照常可用；斜杠菜单和「+」菜单的「画板」插入一个空画板并直接打开。主页卡片同样画出缩略图。独立 `.excalidraw` 文件直接打开成画板，顶栏和笔记一样；侧栏、文件夹页的「新建 › 画板」新建这种文件。
- **撤销**：画板内部用移植的 `store.ts` + `delta.ts` 增量撤销（`history.rs`）。画板改动边编辑边写回 block（`Core::set_board`），一次打开期间的所有改动在文档里合成一步撤销。纯视口变化（缩放、平移）不经过 store，不做场景快照和比对。
- **fuzzer**：随机操作「替换某个 Board block 的场景」，不变量里有「Board 场景存盘再读回不变」。
- **搜索**：`plain_text` 收录文字元素的 `originalText` 和 frame 名称。
- **导出**：Markdown 导出（带附件时）写一张 SVG（`export_svg.rs`，移植 `staticSvgScene` 的结构）并附 `.excalidraw`；HTML / PDF / 打印内嵌 SVG。
- **复制粘贴**：画板内用上游剪贴板 JSON（`excalidraw/clipboard`），能和 excalidraw.com 互相粘贴；也能粘贴文字、HTML、图片和内嵌场景的 PNG / SVG。「复制为 PNG / SVG」把场景一起嵌进去（上游的剪贴板复制不嵌），粘回来还是元素。

## 文件对照

目标路径相对 `crates/core/src/board/`（core）或 `src/board/`（app）。上游文件名省略 `packages/`。

### core

| 上游 | 文件 |
|---|---|
| `math/src/*`（`pca` 除外） | `math.rs` |
| `common/src/constants.ts`（画板用到的部分） | `consts.rs` |
| `common/src/random.ts`（+ roughjs `Random`、nanoid 字母表） | `random.rs` |
| `common/src/colors.ts` 的解析部分（tinycolor 的用法） | `color.rs` |
| `element/src/types.ts` `typeChecks.ts` | `types.rs` |
| `element/src/newElement.ts` | `new_element.rs` |
| `element/src/mutateElement.ts` | `mutate.rs` |
| `element/src/Scene.ts` `sortElements.ts` | `scene.rs` |
| `element/src/fractionalIndex.ts` + `fractional-indexing` | `fractional_index.rs` |
| `element/src/utils.ts`（非几何部分）`comparisons.ts` | `utils.rs` |
| `element/src/utils.ts`（线段 / 曲线拆分） | `deconstruct.rs` |
| `element/src/bounds.ts` | `bounds.rs` |
| `element/src/collision.ts` `distance.ts` | `collision.rs` |
| `element/src/shape.ts`（含箭头头、画笔轮廓） | `shape.rs` |
| rough.js 4.6.4 + hachure-fill、points-on-curve、points-on-path、path-data-parser | `rough/` |
| perfect-freehand | `freehand.rs` |
| `element/src/renderElement.ts` + `staticScene` 的元素遍历 | `display.rs` |
| `scene/export.ts` `renderer/staticSvgScene.ts` | `export_svg.rs` |
| `element/src/transform.ts` | `transform.rs` |
| `element/src/dragElements.ts` | `drag.rs` |
| `element/src/resizeElements.ts` + `actionFlip` 的核心 | `resize.rs` |
| `element/src/transformHandles.ts` `resizeTest.ts` | `handles.rs` |
| `element/src/sizeHelpers.ts` | `size_helpers.rs` |
| `element/src/selection.ts` `showSelectedShapeActions.ts` | `selection.rs` |
| `element/src/groups.ts` | `groups.rs` |
| `element/src/zindex.ts` | `zindex.rs` |
| `element/src/duplicate.ts` | `duplicate.rs` |
| `element/src/align.ts` `distribute.ts` `positionElementsOnGrid.ts` | `align.rs` |
| `element/src/linearElementEditor.ts`（编辑部分）`arrows/` | `linear.rs` |
| `element/src/binding.ts` | `binding.rs` |
| `element/src/arrowEndpointText.ts` | `arrow_text.rs` |
| `element/src/elbowArrow.ts` `common/src/binary-heap.ts` | `elbow.rs` |
| `element/src/heading.ts` | `heading.rs` |
| `element/src/textElement.ts` | `text.rs` |
| `element/src/textWrapping.ts` | `text_wrapping.rs` |
| `element/src/textMeasurements.ts` + `font-metadata.ts` 的度量 | `text_measure.rs` |
| `element/src/listMarker.ts` | `list_marker.rs` |
| `wysiwyg/textWysiwyg.tsx` 的非 DOM 逻辑 | `text_edit.rs` |
| `element/src/frame.ts` | `frame.rs` |
| `element/src/image.ts` | `image.rs` |
| `element/src/cropElement.ts` | `crop.rs` |
| `element/src/stickyNote.ts` | `sticky.rs` |
| `element/src/flowchart.ts` | `flowchart.rs` |
| `element/src/delta.ts` `store.ts` `excalidraw/history.ts` | `history.rs` |
| `excalidraw/appState.ts` | `app_state.rs` |
| `excalidraw/viewport.ts` `scene/normalize.ts` | `viewport.rs` |
| `excalidraw/snapping.ts` | `snapping.rs` |
| `excalidraw/lasso/` | `lasso.rs` |
| `excalidraw/eraser/` | `eraser.rs` |
| `excalidraw/clipboard.ts` + `App.clipboard.ts` 的数据部分 | `clipboard.rs` |
| `data/restore.ts` `data/blob.ts`（读文件） | `restore.rs` |
| `data/json.ts` | `json.rs` |
| `data/encode.ts` `data/image.ts`（PNG `tEXt`、SVG 载荷） | `encode.rs` |
| `excalidraw/actions/*`（每个 action 一个函数） | `actions.rs` + `actions/`（align、canvas、delete、duplicate、flip、frame、group、lock、properties、select、styles、text、zindex） |

文档侧：`crates/core/src/whiteboard.rs`（`Board`：block 和文件的读写、SVG）。

### app

| 上游 | 文件 |
|---|---|
| `components/App.tsx`（状态、提交循环） | `mod.rs`（`BoardView`） |
| `App.tsx` 的指针分发、`App.pan` `App.wheel` `App.viewport` | `canvas.rs` |
| `App.selectionTool` `App.selection`、拖动 / 缩放 / 旋转手柄 | `tools/selection.rs` |
| `App.drawshape` `App.toolDrag`（矩形、菱形、椭圆、分区、便签） | `tools/shape.rs` |
| 线 / 箭头（含多点）、线性编辑 | `tools/linear.rs` |
| 画笔、文字、橡皮、套索、图片、裁剪、快捷创建 | `tools/freedraw.rs` `text.rs` `eraser.rs` `lasso.rs` `image.rs` `crop.rs` `flowchart.rs` |
| `actions/shortcuts.ts` `manager.tsx` 的 `keyTest`、`onKeyDown` | `keys.rs` |
| 工具与 action 的枚举（`TOOL_TYPE`、`actions/*`） | `action.rs` |
| action 执行、撤销重做、剪贴板 | `run.rs` |
| `renderer/interactiveScene.ts` `renderSnaps.ts` | `overlay.rs` |
| `wysiwyg/textWysiwyg.tsx` | `text_editor.rs` |
| `renderFrameNames`（改分区名） | `frame_name.rs` |
| `renderer/staticScene.ts`（GPUI 后端）/ vello 后端 | `render.rs` `render_geom.rs` / `render_vello.rs` |
| `Toolbar` `Actions`（属性面板）`ColorPicker` `MainMenu` `ContextMenu` 缩放控件 `HelpDialog`，`theme.scss` 等的数值 | `chrome/*.rs`，数值在 `chrome/style.rs` |

文档侧：`src/editor/board.rs`（block 缩略图、打开 / 关闭画板、面包屑返回）。

## 文字

- 字体族号照存（`fontFamily` 1–10），用 Excalidraw 自带字体画和量：`assets/fonts/excalidraw/` 有 Excalifont、Virgil、Nunito、Lilita One、Cascadia、Comic Shanns、Assistant 和中文的小赖（Xiaolai，zlib 压缩，第一次遇到需要它的文字时才解压注册），由上游的 woff2 分片合并成 TTF。授权见 THIRD_PARTY.md。
- 回退照上游 CSS：`src/board/fonts.rs` 把每行切成字体段，主字体没有的中日韩字符用小赖，其余交给系统字体（emoji 等）。基线按上游 `getVerticalOffset` 和各字体度量（`text_measure.rs`）。
- Liberation Sans（9）没带（上游带的版本是 GPL 加字体例外），用等宽度的 Arial；Helvetica（2）用系统字体。
- 测量：core 定义 `Measure` trait，app 用 GPUI `TextSystem::layout_line` 实现；core 测试用上游测试的 mock（每个 UTF-16 单元 10px）。
- 换行、容器内文字、自动尺寸、便签字号自适应照 `textWrapping.ts` / `textElement.ts` / `stickyNote.ts`。

## 界面外壳

- 位置照 Excalidraw：顶部居中工具栏（锁定、平移 H、选择 V/1、矩形 R/2、菱形 D/3、椭圆 O/4、箭头 A/5、线 L/6、画笔 P/7、文字 T/8、便签 N、橡皮 E/0、图片 9、分区 F、套索），左侧属性面板（选中或有工具时出现），左下缩放和撤销，右下帮助，右键菜单。快捷键照上游 `shortcuts.ts`；画板打开时画板内优先，⌘W / ⌘Q 等窗口级不拦。
- 视觉：`chrome/style.rs` 抄录上游 `theme.scss` / `variables.module.scss` / `styles.scss` 的浅色、深色数值（颜色、阴影、圆角、按钮尺寸、间距、字号），界面字体 Assistant。主菜单（含画布背景）、取色器（常用色、场景里最常用的自定义色、色板及其快捷键、色阶、十六进制）、字体列表（快速搜索、悬停预览、方向键选择）、右键菜单（含文本绑定、复制为文本、箭头绑定和中点吸附开关）、提示行、更多工具下拉、清空画布确认框照上游组件。文字的行高照浏览器给 Assistant 算的 `line-height: normal`（`style::lh`）。属性面板有「线条风格」（roughness）。
- 图标：`assets/icons/excalidraw/` 96 个，取自上游 `icons.tsx`（MIT，多数来自 tabler-icons），每个文件开头注明来源。画板界面不用 Nucleo 图标。
- 文案：中文原文为翻译键；其他语言表缺的显示英文。

## 与上游的差别

有意的取舍，原因写在对应文件开头：

- **随机数**：roughjs 在种子为 0 和点状填充时用 `Math.random()`，我们用固定种子流，画面稳定。
- **便签**：边缘画成轮廓内 0.5px 描边（上游是裁剪的 1px）。
- **深色模式**：SVG 图片不反色（图片路径不能逐像素滤）；SVG 导出始终是浅色。
- **字体**：Liberation Sans 用 Arial 代替；导出 SVG 内嵌 TTF 子集（上游是 woff2）；复制为 PNG 时中文字体可能取苹方。
- **界面**：没有协作、库、AI、社交链接、统计、禅模式 / 只读模式、小地图（所以左下是「− 缩放 +」，即上游关掉小地图时的样子，不是缩放菜单）、笔模式、激光笔、嵌入网页、油漆桶、命令面板 / 画布内搜索；主菜单没有打开 / 保存 / 导出 / 查找 / 主题；取色器没有吸管、方向键 / Tab 导航和「拖到常用色」；字体列表没有拖到常用位；没有画笔压感开关和链接按钮（链接弹窗、元素上的链接图标）。工具栏的提示是我们画的黑底提示（上游用浏览器原生 `title`）。画板放在应用顶栏下的笔记区里，所以左上角的 ☰ 下面没有文件名，回文档靠顶栏面包屑；没有选中时按 Esc 也回到文档。中文里「复制」已表示 Copy，所以 Duplicate 用「创建副本」；「线条」已表示 Line，所以 Hachure 用「斜线」。
- **分区**：分区的边框宽度和圆角用场景单位（上游在交互画布上除以缩放）；分区里的文字只按分区外框裁剪。分区名和上游一样按屏幕固定 14px 画在画布之上（`frame_name.rs`），缩略图和导出按场景单位画。
- **撤销**：delta 记录按元素 id 排序再应用（上游按插入顺序）；值做深比较（上游比一层引用）；JSON 里 `null` 和缺省是同一个值（`history.rs`）。
- **剪贴板**：「复制为 PNG / SVG」嵌入场景；粘贴不认识的 SVG 代码时当文字（上游插成 SVG 图片，我们的图片不显示 SVG）。不支持表格转图表、Mermaid（`clipboard.rs`）。
- **画笔**：等宽笔迹用 perfect-freehand 代替 `@excalidraw/laser-pointer`（`shape.rs`）。
- **状态**：`appState` 里协作、对话框、导出对话框、主题、笔输入等字段不建模；`boxSelectionMode` 固定为默认值（`app_state.rs`）。
- **vello 后端**：不画彩色 emoji。

## 还没做

- 上游的库面板、协作、元素链接（属性面板和链接弹窗）、嵌入网页（`embeddable`）、油漆桶、手绘转图形（`convertToShape`）、激光笔、Mermaid / 文字转图表、命令面板、搜索、小地图、统计面板、导出对话框。
- 把画板剪贴板 JSON 粘贴到正文里变成 Board block。
- Windows / Linux 的 vello 后端（需要 DirectX / Vulkan 纹理路径），以及 GPUI 不能做的旋转文字裁剪。

## 验收

- **算法**：上游 `packages/math/tests`、`packages/element/tests` 和 `excalidraw/tests` 里与已移植模块对应的测试翻成 Rust（`crates/core/tests/board_*.rs`、`board_transform/`），输入和期望值照抄。每个测试文件开头列出跳过的用例和原因（多为 React / DOM 交互、间谍计数、未移植的功能）。
- **格式**：`crates/core/tests/fixtures/board/` 的 `.excalidraw` 读入 → 写出 → 读回一致；`all-blocks.xml` 往返；指南库往返。
- **常规**：`scripts/check.sh` 零警告；`FDL_FUZZ_SEEDS=2000 cargo test --release random_edits`。
- **性能**：tools/perf/board-results.md（`board_gen.py` 场景，1 万个图形约 3.25 万元素：全景平移每帧约 12ms，100% 平移约 3ms，⌘= / ⌘- 处理按键 0.02ms）。
