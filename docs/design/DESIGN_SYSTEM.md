# Quiet.md 设计系统

状态：**2026-11-22 定稿**（用户定的）。先在数据表上落地，正在推到整个应用（进度见 [COMPONENTS.md](COMPONENTS.md)）。和代码对不上时以代码为准：token 在 `src/design/mod.rs`，组件在 `src/design/` 的各个文件，所有组件的查看面板在 `src/gallery/`。

## 一、一句话

**shadcn/ui 的 Nova 色彩 + Vega 空间，弹窗和卡片的边缘用 affectUI 的质感。**

三条原则：

1. **空间感取 Vega。** 控件 36 高，菜单项 32，弹窗内边距 16，表头 40，单元格内边距 8。不要 Maia 那样宽松（胶囊控件、圆角 18 的菜单、表头 48），也不要 affectUI 那样紧凑（28 的控件、13 号字）。
2. **圆润度暂时保持 Vega 的档位。** 控件、菜单、弹窗 8，菜单项 6，卡片 14。长期想要更圆一些：做法是**给组件提档**（像 Maia 那样，菜单从 md 提到 xl），不是调大基准值 `R`。调 `R` 只是整体按比例缩放，风格的性格不变。
3. **浮起来的东西和卡片，边缘只有一条利落的线。** shadcn 的做法是一圈 1px 边框再套一层阴影，两层叠在一起，边缘发虚。我们改用 affectUI 的做法：0.5px 的细环（用阴影画）加上离边缘较远的淡投影。控件（输入框、选择框、描边按钮）仍用 Vega 的 1px 实线，可点、可输入的边界要清楚。

颜色取 Nova 的中性灰，不用 affectUI 的黑白透明度：affectUI 整体太淡。

### 怎么来的

2026-11-21 起，先是 Nova 色彩加 Maia 空间。之后并排做了一套 affectUI 风格对比（见第六节）。2026-11-22 用户看完两套的对比：

- 整体更喜欢 shadcn，affectUI 太紧凑、太淡；
- 但 affectUI 的弹窗质感更好，原因在边缘；
- Maia 的圆角和留白太大，换成 Vega。

于是定成上面三条。

### 来源

- shadcn/ui：`shadcn-ui/ui` @ `bfa64ff`，MIT。
  - 色彩：`apps/v4/registry/themes.ts`（Neutral）、`apps/v4/registry/styles/style-nova.css`。
  - 尺寸、圆角：`style-vega.css`，并在 ui.shadcn.com/create（Vega / Neutral / Radius Default）上实测核对过。
- affectUI：[flohoeller/affectui](https://github.com/flohoeller/affectui) @ `e627e5b` 的 clean 风格（`components/*-clean.tsx`），MIT。只取了弹窗和卡片的阴影。
- 两者都只搬数值和样式配方，不搬代码，登记在 THIRD_PARTY.md。组件是给 GPUI 写的，用到哪个组件再迁哪个。
- 不参考 BoardUI（Pro 授权，不适合开源仓库）。GPUI Kit（longbridge/gpui-component，Apache-2.0）可作实现参考（在 GPUI 里怎么做菜单的键盘导航、焦点、对话框），不整段搬代码；真有改写过来的，登记在 THIRD_PARTY.md。
- **2026-11-23 起，细节完全照 shadcn 还原**（维护者要求）。逐个组件、逐个属性的对照在 [SHADCN_AUDIT.md](SHADCN_AUDIT.md)；允许保留的偏差只有下面「和 shadcn 的偏差」一节列的三类。

## 二、Token（`src/design/mod.rs`）

有四组：`color`、`chip`、`geo()`、`shadow`，外加一个基准圆角 `R`。

### 颜色 `color`

shadcn Neutral 主题的语义色，浅色、深色各一套，用 `theme::dark()` 选。

| token | 浅色 | 深色 | 用在哪 |
|---|---|---|---|
| `foreground` | #0a0a0a | #fafafa | 正文、表头 |
| `muted_foreground` | #737373 | #a1a1a1 | 说明、图标、行号、占位 |
| `popover` / `card` | #ffffff | #262626 | 菜单、弹窗 |
| `primary` | #171717 | #e5e5e5 | 主按钮、勾选的复选框、开着的开关 |
| `muted` | #f5f5f5 | #262626 | 选中的行、列、格子 |
| `muted` 也用于 | | | 文字按钮的悬停（深色 `muted/50`）和在用（同 shadcn 的 Toggle） |
| `accent` | #f5f5f5 | #404040 | 亮着的菜单项；深色弹层上在用的文字按钮 |
| `accent_hover` | #f5f5f5 | 白 6% | 深色弹层、对话框、卡片上悬停的文字按钮 |
| `border` | #e5e5e5 | 白 10% | 表格线、分隔线 |
| `input` | #e5e5e5 | 白 15% | 输入框、复选框的边 |
| `ring` | #a1a1a1 | #737373 | 焦点环、选区框 |
| `destructive` | #e7000b | #ff6467 | 删除 |

和 shadcn 不同的地方只有允许的那几类，见下面「和 shadcn 的偏差」。

### 和 shadcn 的偏差

只有这三类，其余一律照 shadcn（逐项见 [SHADCN_AUDIT.md](SHADCN_AUDIT.md)）。

**A. 边缘。** 弹层、卡片、对话框用 `shadow::popover()` / `frame()` / `dialog()`：0.5px 细环加投影，代替 shadcn 的 `ring-1 ring-foreground/10` 加 `shadow-*`。原因是维护者 2026-11-22 定的：shadcn 的边框和阴影两层叠在一起，边缘发虚。Toast 的 1px 边加 Sonner 的阴影也换成 `popover()`。控件（输入框、描边按钮、复选框）仍是 Vega 的 1px 实线。

**B. 圆角。** 基准 R 是 12（Vega 是 10），档位的算法照 shadcn。macOS 上的圆角画成连续圆角（GPUI 补丁 0047，只改了 Metal 着色器），Windows 和 wgpu 仍是普通圆弧。以后「提档」由维护者决定。

**C. GPUI 限制逼出来的。**

- **深色弹层 #262626、弹层上的 `accent` #404040。** shadcn 的深色页面是 #0a0a0a、弹层 #171717；我们的深色页面是 #1c1c1c，比它的弹层还亮，弹层只能升一档。画在弹层上的 `accent` 跟着升一档（shadcn 官网自己的深色 accent）。
- **深色弹层、对话框、卡片上的文字按钮提一档。** shadcn 用 `muted`（悬停 `muted/50`）。我们的深色弹层本身就是 `muted` 的颜色，画上去看不见，所以在这些面上（`base::raised()`）悬停用白 6%（`accent_hover`）、在用用 `accent`。浅色和页面上照 shadcn。
- **浅色输入框铺白底。** shadcn 是透明。GPUI 会把阴影画在元素底下，透明底会透出 shadow-xs 的灰。深色输入框是半透明的 `input/30`，同理去掉 shadow-xs（描边按钮、复选框、开关、outline 的 Toggle 也一样）。
- **块手柄 26 高、42 / 28 宽。** shadcn 的组合（`ButtonGroup` 加两个 outline `icon-xs`）是 24 × 48；我们合成一个按钮，中间不画接缝（维护者 2026-11-24：它是一个控件）。编辑器按固定尺寸摆手柄（和块的首行垂直居中、表格的手柄放在列选择条那一行），尺寸不动，外观照 shadcn。
- **500 字重。** shadcn 的按钮、标签、标题、菜单标签是 font-medium。GPUI 用系统字体时 500 和 400 一样，第 3 批另做（打包有中等字重的界面字体）。
- **菜单快捷键的字距**（`tracking-widest`）：GPUI 没有 letter-spacing，把字符拆开、每个后面空 0.1em 来模拟。
- **对话框遮罩的模糊**（`backdrop-blur-xs`）：GPUI 画不了背景模糊，只有黑 10%。
- **提示气泡不自动换行**（`max-w-xs`）：气泡按行量宽来定位，调用方用 `\n` 分行。
- **按钮、开关等没有 `focus-visible` 焦点环**：这些控件不接键盘焦点。

- **只有图标的 xs 按钮，图标 16。** shadcn 的 icon-xs 是 `size-3`（12）。侧栏行上的 …、新建、关闭 × 这些地方原来都是 15–16，缩到 12 又小又细（维护者 2026-11-24）。带文字的 xs 仍是 12。

**待维护者决定（不在上面三类里，这次没改）：** 工具栏按钮 28 高（`ButtonSize::Tool`）。shadcn 只有 24（icon-xs）和 32（icon-sm）。应用里约 37 处默认用它，改高会动到多处按固定尺寸算的位置。

### 选项颜色 `chip`

单选和多选的选项颜色，shadcn 没有，是我们自己定的：

- 浅色：Tailwind 的 200 作底、800 作字。
- 深色：950 的 60% 作底、400 或 500 作字。
- 九种加灰：蓝、绿、黄、橙、红、粉、紫、青、天蓝（`chip::ALL`），外加灰（`chip::neutral()`：`secondary` 底、`muted-foreground` 字）。
- 旧文件里第 11–20 号颜色仍是飞书的深色一行。

### 圆角 `R` 和档位

和 shadcn 一样，所有圆角都由一个基准值推出来：`design::R = 12`（shadcn Vega 是 10，`--radius: 0.625rem`；2026-11-23 先试 14，定在 12）。

| 档位 | 算法 | 值 | 用在哪 |
|---|---|---|---|
| sm | R − 4 | 8 | 菜单项 |
| md | R − 2 | 10 | 按钮、输入框、选择框、多行输入框、菜单、弹窗、工具栏按钮 |
| lg | R | 12 | 分段控件的外框（里面的段 10，照 shadcn 不按「外 − 内边距」算） |
| xl | R + 4 | 16 | 卡片（页面里的表格外框）、对话框 |
| 2xl | R + 8 | 20 | Toast（Sonner 的 `rounded-2xl`） |
| 固定 | — | 4 | 复选框 |
| 胶囊 | `radius::PILL` | — | 选项标签、开关 |

- **想更圆：改档位，不改 `R`。** `design::G` 表里，每个圆角都写成 `R - 2.` 这样的形式。要让菜单更圆，就把它的 `R - 2.` 换成 `R + 2.` 或 `R + 4.`。
- **连续圆角（macOS）。** 补丁 0047 把每个圆角画成 Apple 式的连续圆角：半径 r 的角从离角 1.528 r 处开始，是一段超椭圆，在对角线上和半径 r 的圆弧相交；放不下时（胶囊）退回圆弧。Windows（DirectX）和 Linux / 网页（wgpu）的着色器没改，同样的半径画成圆弧。
- **嵌套的圆角要配套：里层 = 外层 − 中间的内边距。** 例如菜单提到 14、内边距 4，菜单项就用 10。否则角上的缝隙宽窄不一。

### 尺寸 `geo()`

所有尺寸都从 `geo()` 取，不在组件里现写。

| | 值 |
|---|---|
| 字号 | 14（`text-sm`，行高 20）；小字 12（`text-xs`，行高 16） |
| 按钮 | Lg 40、默认 36、Sm 32、Xs 24（12 号字、12 的图标）；左右 10（Xs 8），图标那一侧 8（Sm、Xs 6）；工具栏 28 |
| 输入框、选择框 | 高 36，左右 10；面板里的小号高 32 |
| 多行输入框 | 内边距 10 |
| 菜单 | 内边距 4；菜单项高 32、左右 8、图标和文字间距 8；分隔线通栏 |
| 弹窗 | 内边距 16，字段之间 16，筛选和排序的行之间 8，标签在控件上方 8 |
| 选项标签 | 高 20，左右 8 |
| 复选框、开关 | 16；32×18.4，圆点 16 |
| 分段控件 | 高 36，内边距 3；每段高 29、左右 8、圆角 8 |
| 对话框 | 默认宽 448（AlertDialog 512），内边距 24，关闭 × 32、距上右 16 |
| Toast | 宽 356，内边距 16，间距 6，13 号字，圆角 18 |
| 提示气泡 | 离触发器 10，立即出现（`delayDuration = 0`） |
| 表格 | 表头 40，单元格左右 8，行高 36（低）或 44（中等），添加记录 36，统计栏 40，分组标题 36 |

### 阴影 `shadow`

| token | 构成 | 用在哪 |
|---|---|---|
| `xs()` | 0 1 2 黑 5%；深色模式下去掉 | 输入框、描边按钮（配 1px `input` 边） |
| `popover()` | 0.5px 环（黑 8%）+ 0 8 24 −6 黑 14% + 0 2 6 −2 黑 6%；深色：白 14% 的环 + 0 12 32 −8 黑 60% | 菜单、下拉、面板、单元格编辑器 |
| `frame()` | 0.5px 环（黑 10%）+ 0 1 1 −1 黑 10% + 0 1 2 黑 5%；深色：白 12% 的环 + 0 1 2 黑 40% | 卡片：页面里的表格外框 |
| `ring(c, w)` | 宽 `w` 的环 | 弹窗里嵌的列表（0.5px `border`） |

- 卡片的环用 10%，和 Vega 卡片的 `ring-foreground/10` 一样浓，只是画成 0.5px。卡片不浮起，所以投影只有 1–2px。
- 边缘为什么不照 shadcn：见上面「和 shadcn 的偏差」A。
- **GPUI 的限制：** 阴影画在元素底下，整块都会被盖住。所以用阴影画边缘的元素，自己必须有不透明的底色（卡片要 `bg(Theme::bg_body())`）。半透明的东西（比如颜色块）只能用 1px 边框。

### 字重

shadcn 的表头、标签、按钮是 500。GPUI 用系统字体时，500 和 400 渲染出来一样，所以目前都是常规字重（偏差 C）。要做到需要打包一款有中等字重的界面字体，第 3 批做。

## 三、组件

组件库在 `src/design/`，清单、用法约定和替换进度见 [COMPONENTS.md](COMPONENTS.md)。下表是数据表 `src/table/style.rs` 里的名字（现在都转发到组件库）和它们的要点。

| 组件 | 对应 shadcn | 要点 |
|---|---|---|
| `primary_btn` | Button default | `primary` 底，悬停 `primary/80`，36 高、圆角 8 |
| `plain_btn` | Button outline | `border` 细边加 `xs`；深色 `input` 边、`input/30` 底 |
| `ghost_btn` / `icon_btn` / `tool_btn` | Button ghost | 无底，悬停和在用 `muted`（深色悬停 `muted/50`；深色弹层上提一档）；工具栏 28 高 |
| `danger_btn` | Button destructive | `destructive` 的浅底和字 |
| `field_box` / `input_box` | Input | `input` 细边，36 / 32 高，圆角 8；输入中是 `ring` 边加 3pt 的 `ring/50` 光晕 |
| `area_box` | Textarea | 圆角 8，内边距 10 |
| `select_trigger` / `select_box` / `pick_box` | SelectTrigger | 输入框的外观，展开时和输入中一样 |
| `field_label` | Label | 14，`foreground`，在控件上方 8 |
| `checkbox` | Checkbox | 16，圆角 4，勾上是 `primary` |
| `switch` | Switch | 32×18，开 `primary`、关 `input` |
| `segmented` / `seg_track` / `segment` | Tabs | `muted` 底，36 高、圆角 10、内边距 3；选中的一段白底加 shadow-sm（深色 `input/30` 加 `input` 边）、圆角 8 |
| `swatch` | — | 颜色块；选中时 2pt 的环，用边框画（半透明） |
| `tag` / `link_chip` / `person` | Badge | 高 20，胶囊，12 号字 |
| `popover` | DropdownMenuContent / PopoverContent | `popover` 底，圆角 8，`shadow::popover()`，无描边 |
| `menu_row` / `menu_item` / `list_row` / `menu_title` / `menu_rule` | DropdownMenuItem / Label / Separator | 32 高、圆角 6；亮的行 `accent` 底、`accent-foreground` 字；不可用整行 50%；分隔线 `border`，0.5pt，左右各收进 12（和行里的图标、文字对齐；维护者 2026-11-24 定的） |
| 编辑器的浮动工具条、块手柄 | Toggle、ButtonGroup、Popover 拼成（`design::toolbar`） | 工具条 popover 底、内边距 4、工具 32 高、间距 4；按下 / 展开 `muted`，不加阴影；块手柄是一个 outline xs 按钮，类型图标和 ⋮⋮ 之间不画线 |
| `inset_list` | — | 弹窗里嵌的列表：菜单的外观、0.5px 环，不浮起 |
| `panel_title` / `close_btn` / `kbd` / `chip_x` | — | 面板标题、关闭按钮、快捷键、选项上的 × |

数据表网格本身（shadcn 只有静态表格）：

- 表头不铺底色，字用 `foreground`。
- 选中的格子和行用 `muted`。
- 选区外面一圈 2pt 的 `ring` 框；选区里的当前格子不铺底，描 1pt 细框（和 Excel 一样）。
- 嵌在页面里时，整张表是一张卡片：圆角 14，`shadow::frame()`，没有边框。

## 四、怎么用、怎么扩

- 界面只用 `src/design/` 的组件和 token（数据表里也可以用 `style.rs` 的转发名）。不写十六进制颜色，不现场写圆角、边框和阴影。
- **缺一个组件：**
  1. 先在 shadcn 源码里找对应组件，颜色看 `style-nova.css`，尺寸和圆角看 `style-vega.css`。
  2. 浮层和卡片的边缘用 `shadow::popover()` / `shadow::frame()`，不照抄 shadcn 的 `ring-1` 加 `shadow-md`。
  3. 写成 `src/design/` 里的一个组件（`RenderOnce` 结构体），加进组件库面板（`src/gallery/pages.rs`，至少一个能操作的实例），登记到 COMPONENTS.md。
- **缺一个 token：** 从 shadcn 的主题和比例尺里取，加进 `src/design/mod.rs`。新圆角写成 `R ± n` 的形式。
- **组件库面板：** `scripts/components.sh` 编译并单独打开（就是 `quiet_md --components`，UI 脚本里用 `QUIET_MD_COMPONENTS=1`），不开主窗口、不碰文档库。左边是组件目录，右边是当前组件：先是能操作的实例（能点、能切换、能打字、菜单能开能选、弹窗能开关），再是悬停态、展开态、不可用等静态对照。顶部切换浅色、深色、浅深对照。改了 token 或组件，先在这里看一遍浅色和深色。

## 五、接下来

1. ~~把 `style.rs` 里的函数整理成正式组件，移到 `src/design/`~~（2026-11-22 完成）。
2. 应用里其他部分按模块迁移：应用外壳的菜单已换完，剩下的顺序见 COMPONENTS.md。
3. 字体：评估打包一款有中等字重的界面字体，补上 shadcn 的 500 字重。
4. 圆角提档：等整体迁移完，再按第二节的做法试更圆的一版。

## 六、对比用的 affectUI 风格（代码已删除）

定稿前代码里并排做过一整套 affectUI 风格（`design::Look::Affect`、`QUIET_MD_LOOK=affect`），用来逐个组件对比。2026-11-22 建组件库时删掉了：token 和组件不再按风格分支，组件陈列（现在的组件库面板）改成浅色、深色对照。两套当时的对比留在下表。

| | 定稿（shadcn） | affectUI（Clean） |
|---|---|---|
| 正文 | 14 | 13 |
| 按钮 | 高 36，圆角 8 | 高 28，圆角 6 |
| 输入框 | 36 / 32，圆角 8 | 32（表单）/ 28（面板），圆角 8 / 6 |
| 控件边缘 | 1px 实线 `input` 加 shadow-xs | 0.5px 环（黑 8%）加很淡的投影 |
| 弹窗、卡片边缘 | 同右（从 affectUI 取来的） | 0.5px 环加投影 |
| 颜色 | 中性灰阶的实色 | 黑白加透明度：字 88% / 61% / 45% / 30%，底 3–6% |
| 菜单 | 圆角 8，行高 32、圆角 6 | 圆角 12，行高 32、圆角 8，分隔线两端缩进 6 |
| 弹窗 | 内边距 16，间距 16 | 内边距 14，间距 12 |
| 选项标签 | Tailwind 200 底 / 800 字，胶囊 | 色相 12% 底 / 深一档的字，圆角 5 |
| 焦点、选区 | 灰色 `ring` | 蓝色（`rgba(0,96,226,.7)`） |
| 表格 | 表头 40，行高 36 / 44 | 表头 32，行高 32 / 40 |

深色模式下 affectUI 的浮层是 #1b1b1e，和我们的页面一个颜色，所以升到 #232326。
