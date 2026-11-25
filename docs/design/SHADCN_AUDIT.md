# 和 shadcn/ui 逐项对照

2026-11-23 起，组件的细节完全照 shadcn/ui 还原（维护者要求）。允许保留的偏差只有三类，原因写在 [DESIGN_SYSTEM.md](DESIGN_SYSTEM.md)「和 shadcn 的偏差」一节：

- **A 边缘**：弹层、卡片、对话框用 `shadow::popover()` / `frame()` / `dialog()`，代替 `ring-1 ring-foreground/10` 加 `shadow-*`。
- **B 圆角**：R 是 12（Vega 10），档位算法照 shadcn；macOS 上画成连续圆角（补丁 0047）。
- **C GPUI 限制**。

来源：`shadcn-ui/ui` @ `bfa64ff`。
- `.cn-*` 指 `apps/v4/registry/styles/style-vega.css` 里的类（尺寸、状态）。
- 颜色看 `style-nova.css` 和 `registry/themes.ts` 的 Neutral。
- 结构看 `registry/bases/radix/ui/*.tsx`。
- 行为照 Radix。

「处理」一列：**一致**（本来就一样）、**改**（这次改成 shadcn 的样子）、**保留 A / B / C**（允许的偏差）、**新增**（shadcn 有、我们原来没有，这次补上）、**我们的**（shadcn 没有对应的东西，不算偏差）。

## Button（`src/design/button.rs`）

| 属性 | shadcn | 我们 | 处理 |
|---|---|---|---|
| 圆角 | `.cn-button` rounded-md（8） | 8 | 一致 |
| 字号 | text-sm（14） | 14 | 一致 |
| 字重 | font-medium（500） | 400 | 保留 C（GPUI 系统字体 500 与 400 一样，第 3 批另做） |
| 不可用 | `disabled:opacity-50` | 整体 50% | 一致 |
| 按下 | `active:not-aria-[haspopup]:translate-y-px` | 按下时下移 1pt（菜单触发器 `selected` 时、调用方绝对定位时不移） | 改（新增） |
| 图标颜色 | `[&_svg]` 继承 `currentColor` | 和文字同色 | 改（原来是 `muted-foreground`） |
| 图标尺寸 | `size-4`；xs / icon-xs `size-3` | 16；带文字的 Xs 12，只有图标的 Xs（icon-xs）16 | 维护者 2026-11-24：icon-xs 的 12 在侧栏、关闭 × 等处又小又细，回到应用原来的 16 |
| 焦点环 | `focus-visible:ring-3 ring-ring/50` | 无 | 保留 C（按钮不接键盘焦点，没有 focus-visible） |
| default 变体 | `.cn-button-variant-default` bg-primary，悬停 primary/80 | 同 | 一致 |
| outline | `border-border bg-background hover:bg-muted`，深色 `bg-input/30 border-input hover:bg-input/50`，`aria-expanded:bg-muted`，shadow-xs | 同 | 一致 |
| secondary | bg-secondary，悬停 secondary 混 5% foreground | 同 | 一致 |
| ghost 悬停 | `hover:bg-muted`，深色 `hover:bg-muted/50` | 同 | 改（原来是 `accent_hover`） |
| ghost 在用 | `aria-expanded:bg-muted`；按下的 Toggle `aria-pressed:bg-muted` | `selected` → `muted` | 改（原来是 `accent`） |
| ghost 在深色弹层上 | muted / muted/50（弹层 #171717 上看得见） | 悬停 白 6%（`accent_hover`），在用 `accent` #404040 | 保留 C：我们的深色弹层 #262626 就是 `muted`，muted 画上去看不见；在弹层、对话框、卡片里（`base::raised()`）提一档 |
| ghost 在侧栏行上 | SidebarMenuAction、SidebarGroupAction `hover:bg-sidebar-accent`，和行的悬停（SidebarMenuButton `hover:bg-sidebar-accent`）同色 | 侧栏行里的「…」「+」、我的文档标题的批量和「+」：悬停和在用都是 `background`（`Button::on_wash`） | 保留 C（2026-11-25）：毛玻璃侧栏上行的悬停是黑 4.5%（浅色约 234），`muted`（245）或 accent 叠上去比行亮一点、又比周围的侧栏底（约 247）暗，看不出层次、显得发灰；`background`（浅色纯白）在行上和侧栏底上都分得开 |
| destructive | bg destructive/10，悬停 /20；深色 /20、/30；字 destructive | 同 | 一致 |
| link | `text-primary underline-offset-4 hover:underline` | `ButtonVariant::Link` | 新增 |
| 尺寸 default | `.cn-button-size-default` h-9 gap-1.5 px-2.5，带图标那侧 pl-2 / pr-2 | 36、6、10，图标侧 8 | 改（图标侧原来也是 10） |
| 尺寸 sm | h-8 gap-1 px-2.5，图标侧 1.5；圆角 min(md, 10px) | 32、4、10，图标侧 6；8 | 改（图标侧） |
| 尺寸 xs | h-6 gap-1 px-2 text-xs，图标侧 1.5；圆角 min(md, 8px) | 24、4、8、12，图标侧 6；8 | 改（原来 px 6、字 14、图标 14） |
| 尺寸 lg | h-10 gap-1.5 px-2.5 | `ButtonSize::Lg` 40 | 新增 |
| icon / icon-sm / icon-xs / icon-lg | size-9 / 8 / 6 / 10 | 只给图标时方形：36 / 32 / 24 / 40 | 一致 |
| 工具栏 28 | 无（只有 24 和 32） | `ButtonSize::Tool` 28 | **待维护者决定**：应用里约 37 处默认 28 高（页头、侧栏、工具栏），改到 32 会动到多处按固定尺寸算的位置，这次没动 |

## Popover（`src/design/menu.rs`）

| 属性 | shadcn | 我们 | 处理 |
|---|---|---|---|
| 底色、字色 | bg-popover text-popover-foreground | `popover` / `foreground` | 一致（深色底色见颜色一节） |
| 圆角 | `.cn-popover-content` rounded-md | 8 | 一致 |
| 内边距、间距 | p-4 gap-4 | `.padded()`：16、16 | 一致 |
| 字号 | text-sm | 14 | 一致 |
| 边缘 | ring-1 ring-foreground/10 shadow-md | `shadow::popover()` | 保留 A |
| 离触发器 | `sideOffset = 4` | 调用方定位 | 一致（调用方多用 4） |
| PopoverHeader / Title / Description | flex-col gap-1；font-medium；muted | 没有对应组件，调用方自己排 | 我们的（没用到） |

## DropdownMenu（`src/design/menu.rs`）

| 属性 | shadcn | 我们 | 处理 |
|---|---|---|---|
| 内容圆角、内边距 | `.cn-dropdown-menu-content` rounded-md p-1 | 8、4 | 一致 |
| 最小宽度 | min-w-32（128） | `.menu()` 128 | 改（新增） |
| 子菜单内容 | `.cn-dropdown-menu-sub-content` min-w-[96px] rounded-md p-1 shadow-lg | 子菜单也是 `Popover::menu()` | 一致；阴影保留 A |
| item 尺寸 | `.cn-dropdown-menu-item` gap-2 rounded-sm px-2 py-1.5 text-sm | 行高 32、圆角 6、左右 8、间距 8 | 一致 |
| item 高亮 | `focus:bg-accent focus:text-accent-foreground`，`focus:**:text-accent-foreground` | 亮的行 `accent` 底，文字、图标、快捷键、说明都变 `accent-foreground` | 改（原来文字不变色） |
| item 图标 | `[&_svg]:size-4`，颜色继承 | 16，和文字同色 | 改（原来 `muted-foreground`） |
| item 不可用 | `data-disabled:opacity-50`，无指针事件 | 整行 50%，不亮 | 改（原来只把字和图标调淡） |
| destructive | `text-destructive`，高亮 `bg-destructive/10`（深色 /20），图标 destructive | 同 | 一致 |
| inset | `data-inset:pl-8` | `.inset()`：留 16 + 间距 8，等于 32 | 一致 |
| checkbox / radio item | `pr-8 pl-2`，指示 `absolute right-2`，CheckIcon `size-4` | 末尾 16 宽的位，✓ 16，和文字同色 | 改（✓ 原来 14） |
| label | `.cn-dropdown-menu-label` px-2 py-1.5 text-xs font-medium muted | 28 高、左 12（含菜单 p-1）、12 号 muted | 一致；字重保留 C |
| separator | `.cn-dropdown-menu-separator` bg-border -mx-1 my-1 h-px | 0.5pt，左右各收进 12，上下 4，`border` | 改（维护者 2026-11-24：细线、不顶到头） |
| shortcut | `.cn-dropdown-menu-shortcut` ml-auto text-xs tracking-widest muted，高亮时 accent-foreground | ml-auto、12 号、每个字后加 1.2pt 间距（0.1em），高亮时 `accent-foreground` | 改（原来没有字距、不变色）；字距用分字排开模拟，保留 C（GPUI 没有 letter-spacing） |
| sub-trigger | `.cn-dropdown-menu-sub-trigger`：`data-open:bg-accent text-accent-foreground`，chevron size-4 ml-auto 继承颜色 | `MenuItem::open(true)` 一直亮；chevron 16，和文字同色 | 改（新增 `.open()`；chevron 原来 14、muted） |
| 高亮规则 | Radix：同一菜单最多一行亮；指针离开所有行时都不亮；键盘移动时键盘行亮 | 同 | 改（原来指针离开后会回到 `selected` 行） |
| 子菜单和兄弟行 | Radix：指针移到兄弟行，子菜单关上 | 应用里本来如此；组件面板的演示改成这样 | 改（面板） |
| 两行（说明） | 无 | `.description()` | 我们的 |

## Input / Field、Select trigger、Textarea、Label（`src/design/field.rs`）

| 属性 | shadcn | 我们 | 处理 |
|---|---|---|---|
| 高、圆角、左右 | `.cn-input` h-9 rounded-md px-2.5 py-1 | 36、8、10 | 一致 |
| 边、阴影 | border-input，shadow-xs | 1px `input`，`shadow::xs()` | 一致 |
| 底色（浅色） | bg-transparent | 白 | 保留 C（GPUI 把阴影画在元素底下，透明底会透出 shadow-xs） |
| 底色（深色） | `dark:bg-input/30` | 同 | 一致；深色去掉 shadow-xs 也属 C（同上） |
| 焦点 | `focus-visible:border-ring ring-3 ring-ring/50` | `focused`：`ring` 边加 3pt `ring/50` 光晕 | 一致 |
| 出错 | `aria-invalid:border-destructive ring-3 ring-destructive/20`，深色边 /50、光晕 /40 | `.invalid(true)` | 新增 |
| 不可用 | `disabled:opacity-50` | 整体 50% | 改（原来换底色和字色） |
| 占位 | `placeholder:text-muted-foreground` | `color::placeholder()` = muted-foreground | 一致 |
| select 尺寸 | `.cn-select-trigger` h-9 / h-8，gap-1.5，py-2 pr-2 pl-2.5 | 36 / 32，6，右 8、左 10 | 改（右边原来 10） |
| select 箭头 | `.cn-select-trigger-icon` size-4 muted | 16 muted | 改（原来 14） |
| select 悬停 | `dark:hover:bg-input/50`，浅色无 | 同 | 一致 |
| select 展开 | 无变化（焦点进了列表，触发器没有 ring） | `.open()` 不改外观 | 改（原来展开时画焦点环） |
| textarea | `.cn-textarea` rounded-md px-2.5 py-2 min-h-16 | 8、10 / 8、最小 64 | 改（上下原来 10，没有最小高度） |
| textarea 不可用、出错 | opacity-50；aria-invalid 同 input | `.disabled()`、`.invalid()` | 新增 |
| Label | `.cn-label` text-sm leading-none font-medium，不可用 opacity-50 | 14、行高 14、`.disabled()` | 改（行高原来 20）；字重保留 C |
| 说明文字 | `.cn-field-description` text-sm muted leading-normal | `Label::muted()`：14、行高 21 | 改（原来 12 号） |

## Checkbox、Radio、Switch（`src/design/control.rs`）

| 属性 | shadcn | 我们 | 处理 |
|---|---|---|---|
| checkbox | `.cn-checkbox` size-4 rounded-[4px] border-input shadow-xs，深色 input/30 | 16、4、同 | 一致 |
| checkbox 勾上 | bg-primary border-primary，指示 `[&>svg]:size-3.5` | primary，✓ 14 | 改（✓ 原来 12） |
| checkbox 半选 | radix 基础组件没有 | 实心加横杠 | 我们的 |
| radio | `.cn-radio-group-item` size-4 rounded-full border-input，深色 input/30，**无 shadow** | 同 | 改（原来有 shadow-xs） |
| radio 选中 | bg-primary border-primary，中心 `bg-primary-foreground size-2` | primary 实心，8pt `primary-foreground` 点 | 改（原来是 primary 圈加 primary 点） |
| switch 尺寸 | `.cn-switch` h-[18.4px] w-[32px]，透明 1px 边，thumb size-4 | 32 × 18.4，thumb 16，左右 1 | 改（原来高 18） |
| switch 阴影 | shadow-xs | `shadow::xs()` | 改（原来没有） |
| switch 颜色 | 开 primary，关 input（深色 input/80）；thumb background，深色关 foreground、开 primary-foreground | 同 | 一致 |
| 不可用 | opacity-50 | 50% | 一致 |
| switch sm | 24 × 14，thumb 12 | 没有 | 没用到，未加 |

## Tabs（`Segmented` / `Segment`）

| 属性 | shadcn | 我们 | 处理 |
|---|---|---|---|
| 列表 | `.cn-tabs-list` bg-muted rounded-lg p-[3px] h-9 | muted、10、3、36 | 改（高原来 32） |
| 触发器尺寸 | `.cn-tabs-trigger` h-[calc(100%-1px)] rounded-md px-2 py-1 gap-1.5，透明 1px 边 | 高 29、8、左右 8、6 | 改（原来铺满、圆角 7、左右 10） |
| 触发器文字 | text-foreground/60，深色 muted-foreground，悬停 foreground | 同 | 改（浅色原来 muted-foreground） |
| 选中 | bg-background text-foreground shadow-sm；深色 `border-input bg-input/30` | 同 | 改（深色原来是抬高的实色底、无边） |
| 不可用 | opacity-50 | `Segment::disabled()` | 新增 |
| 圆角嵌套 | 外 10、内 8（shadcn 不按「外 − 内边距」算） | 8 | 改（原来按 10 − 3 = 7） |

## Badge、Kbd（`src/design/badge.rs`）

| 属性 | shadcn | 我们 | 处理 |
|---|---|---|---|
| 尺寸 | `.cn-badge` h-5 gap-1 rounded-4xl px-2 py-0.5 text-xs，透明 1px 边 | 20、4、胶囊、8、12 | 改（间距原来 2，没有透明边） |
| 字重 | font-medium | 400 | 保留 C |
| 变体 | default / secondary / outline / destructive / ghost / link | `Badge::new`（secondary）、`primary`、`outline`、`destructive` | 新增三种（ghost、link 只在 `<a>` 上有区别，没加） |
| secondary 字色 | secondary-foreground | `color::secondary_foreground()` | 改（原来 foreground） |
| 末尾图标 | `has-data-[icon=inline-end]:pr-1.5`，`[&>svg]:size-3` 继承颜色 | `.closable()`：右 6，× 12，和文字同色 | 改（原来右 3、× 放在 14 的格里、muted） |
| 选项颜色 | 无 | `chip` | 我们的 |
| Kbd | `.cn-kbd` bg-muted muted h-5 min-w-5 gap-1 rounded-sm px-1 text-xs | 同 | 一致；字重保留 C |
| KbdGroup | gap-1 | 4；连按之间加 → | 一致；→ 是我们的 |

## Dialog、AlertDialog（`src/design/dialog.rs`）

| 属性 | shadcn | 我们 | 处理 |
|---|---|---|---|
| 内容 | `.cn-dialog-content` bg-popover gap-6 rounded-xl p-6 text-sm，sm:max-w-md | popover、24、14、24、14 号，默认宽 448 | 改（新增默认宽度） |
| 边缘 | ring-1 ring-foreground/10 | `shadow::dialog()` | 保留 A |
| 遮罩 | `.cn-dialog-overlay` bg-black/10，backdrop-blur-xs | 黑 10%，没有模糊 | 改（深色原来 40%）；模糊保留 C（GPUI 没有背景模糊） |
| header | `.cn-dialog-header` gap-2 | 8 | 一致 |
| title | `.cn-dialog-title` leading-none font-medium（字号继承 text-sm） | 14、行高 14 | 改（原来 16 / 20）；字重保留 C |
| description | text-sm muted | 同 | 一致 |
| footer | flex-row justify-end gap-2 | 调用方排，统一 gap 8 靠右 | 一致 |
| 关闭 × | `showCloseButton = true`：Button ghost icon-sm（32，圆角 min(md,10)），`absolute top-4 right-4`，XIcon 继承颜色 | `.closable()`：`Button::ghost` Sm 32，距上、右 16，× 16 用 foreground | 改（原来 28、距边 20、muted、悬停 accent_hover） |
| 哪些带 × | Dialog 默认带；AlertDialog 不带；CommandDialog 默认 `showCloseButton = false` | 移动到、导出 Markdown、文档信息、倒计时带；确认框、强制更新（AlertDialog）不带；搜索（CommandDialog）不带 | 一致（`Dialog::alert` 忽略 `closable`） |
| AlertDialog 内容 | `.cn-alert-dialog-content` gap-6 rounded-xl p-6，sm:max-w-lg | `Dialog::alert`：同 Dialog，默认宽 512 | 新增 |
| AlertDialog header | `.cn-alert-dialog-header` gap-1.5，sm 以上靠左 | 6，靠左 | 新增 |
| AlertDialog title | `.cn-alert-dialog-title` text-lg font-medium | 18、行高 28 | 新增；字重保留 C |
| AlertDialog footer | flex-col-reverse gap-2 sm:flex-row sm:justify-end；Cancel（outline）在前，Action（default）在后 | 调用方：取消（outline）在前、动作在后，靠右 | 一致 |

## Card、Item、Separator（`src/design/card.rs`）

| 属性 | shadcn | 我们 | 处理 |
|---|---|---|---|
| card 底色、圆角 | `.cn-card` bg-card rounded-xl text-sm | card、14、14 号 | 改（新增 text-sm） |
| card 边缘 | ring-1 ring-foreground/10 shadow-xs | `shadow::frame()` | 保留 A |
| card 内边距 | py-6 gap-6（`--card-spacing`），CardHeader / Content / Footer px-6 | 不加：设置分组里的 Item 自己带 px-4 py-3.5，卡片铺满 | **拿不准**：shadcn 也允许 `className="p-0"` 覆盖；我们的 Card 只当外壳用，没有 CardHeader 等，见报告 |
| item 尺寸 | `.cn-item-size-default` gap-3.5 px-4 py-3.5 | 14、16、14 | 改（原来间距 24、上下 10、最小高 48） |
| item 内容 | `.cn-item-content` gap-1 | 4 | 改（原来 2） |
| item 标题 | `.cn-item-title` text-sm leading-snug font-medium | 14、行高 19.25 | 改（行高原来 20）；字重保留 C |
| item 说明 | `.cn-item-description` text-sm leading-normal muted | 14、行高 21、muted | 改（原来 12 号） |
| item 圆角、边 | rounded-md，透明 1px 边 | 无（卡片里的行，靠 Separator 分开） | 一致（default 变体边是透明的） |
| separator | `.cn-separator` bg-border h-px / w-px | 同 | 一致 |

## Sonner（`Toast`，`src/design/card.rs`）

| 属性 | shadcn | 我们 | 处理 |
|---|---|---|---|
| 圆角 | `.cn-toast` rounded-2xl（18） | 18（`toast_r` = R + 8） | 改（原来 10） |
| 底色、字色 | `--normal-bg: popover`，`--normal-text: popover-foreground` | 同 | 一致 |
| 边缘 | `--normal-border: border` 1px，Sonner 自带 `0 4px 12px #0000001a` | `shadow::popover()` | 保留 A |
| 内边距、间距、字号、宽 | Sonner：padding 16，gap 6，13px，宽 356 | 16、6、13、356 | 改（原来上下 12、间距 8、14 号、自适应宽） |
| 图标 | Lucide CircleCheck / Info / OctagonX，size-4，继承文字颜色（没开 richColors，出错也不是红色） | 同（`circle-check`、`info`、`octagon-x`） | 改（原来是实心圆里挖出 ✓ / ×，出错是红色） |

## Tooltip（`src/ui.rs` 的 `Tooltip`）

| 属性 | shadcn | 我们 | 处理 |
|---|---|---|---|
| 颜色 | `bg-foreground text-background` | 同 | 一致 |
| 尺寸 | `.cn-tooltip-content` rounded-md px-3 py-1.5 text-xs gap-1.5 | 8、12、6、12 号 | 一致 |
| 箭头 | `.cn-tooltip-arrow` size-2.5 旋转 45°、rounded-[2px]，伸出约 5 | 宽 10、伸出 5 的三角，尖端略平 | 一致 |
| 离触发器 | Radix：sideOffset 0 加箭头高（10） | 10 | 改（原来 8） |
| 延迟 | TooltipProvider `delayDuration = 0` | 0（`tooltip_show_delay`） | 改（原来 GPUI 默认 500ms） |
| 最大宽度 | max-w-xs，自动换行 | 不自动换行，调用方用 `\n` 分行 | 保留 C（气泡在 GPUI 的 tooltip 视图里按行量宽定位，没有自动换行的测量） |

## 颜色（`src/design/mod.rs` 的 `color`）

| token | shadcn Neutral | 我们 | 处理 |
|---|---|---|---|
| foreground、muted-foreground、primary、secondary、muted、border、input、ring、destructive（浅、深） | themes.ts | 同 | 一致 |
| popover / card（深色） | oklch(0.205) #171717 | #262626 | 保留 C：我们深色页面 #1c1c1c 比它还亮 |
| accent（深色） | oklch(0.269) #262626 | #404040 | 保留 C：画在 #262626 弹层上 |
| accent_hover | 无 | 深色白 6%，只用在深色弹层上的 ghost 悬停 | 保留 C（同 ghost 那一行） |
| secondary-foreground | oklch(0.205) / oklch(0.985) | `secondary_foreground()` | 新增 |
| 遮罩 | black/10 | 黑 10% | 改（深色原来 40%） |

## Toggle、ToggleGroup、ButtonGroup、Toolbar（`src/design/toolbar.rs`，2026-11-23 第 1d 批）

| 属性 | shadcn | 我们 | 处理 |
|---|---|---|---|
| toggle 尺寸 | `.cn-toggle-size-default` h-9 min-w-9 px-2.5；sm h-8 min-w-8 px-2.5；lg h-10 min-w-10 | `ToggleSize::{Default, Sm, Lg}`：36 / 32 / 40，左右 10；只有图标时 36 宽 | 新增 |
| toggle 带图标和文字 | `has-data-[icon=inline-start]:pl-2`（sm pl-1.5），gap-1 | 图标侧 8（sm 6），间距 4 | 新增 |
| toggle 圆角、字号 | `.cn-toggle` rounded-md text-sm | 8、14 | 新增 |
| toggle 字重 | font-medium | 400 | 保留 C |
| toggle 悬停 | `hover:bg-muted hover:text-foreground` | `muted`；深色弹层上 `accent_hover` | 新增；弹层上提一档保留 C（同 1c 的 ghost） |
| toggle 按下 | `aria-pressed:bg-muted` | `.pressed(true)`：`muted`；深色弹层上 `accent` | 新增；同上 |
| toggle 图标 | `[&_svg]:size-4`，继承颜色 | 16，foreground | 新增 |
| toggle outline | `border-input hover:bg-muted border bg-transparent shadow-xs` | 1px `input`、shadow-xs；浅色铺 `background` | 新增；浅色铺底保留 C（GPUI 把阴影画在元素底下，同输入框） |
| toggle 不可用 | `disabled:opacity-50` | 50% | 新增 |
| ToggleGroup 间距 | `spacing`（默认 2 = 8；文字格式示例用 1 = 4） | 工具条里 4 | 新增 |
| ButtonGroup | 无间距；`[&>*:not(:first-child)]:rounded-l-none border-l-0`，`[&>*:not(:last-child)]:rounded-r-none` | `ButtonGroup`：同 | 新增 |
| ButtonGroupSeparator | `.cn-button-group-separator` bg-input，self-stretch，竖的上下 my-px | 1px `input`，self-stretch，上下各 1 | 新增 |
| Toolbar 外壳 | 无此组件；按 Popover：bg-popover rounded-md p-1，ring + shadow-md | `Toolbar`：popover 底、8、内边距 4、高 40；`shadow::popover()` | 我们的（拼法）；边缘保留 A |
| 工具条里的按下、展开 | Toggle `aria-pressed`、Button `aria-expanded`：bg-muted，无阴影 | 同；原来按下的工具是 `accent` 底，下拉箭头开着时翻成朝上 | 改（箭头不再翻转：shadcn 的触发器箭头不变） |
| 工具条按钮尺寸 | Toggle sm 36 × 32；Button icon-sm 32；带箭头的 sm 按钮 pl-1.5 pr-1.5 | 同（下拉触发器 48、分体 A▾ 64） | 改（原来 24 高的 Xs 按钮、18 的图标、间距 8、内边距 8） |
| 下拉箭头 | lucide ChevronDown size-4，继承颜色 | `chevron-down` 16，foreground | 改（原来 12 的 caret，muted） |
| 块手柄 | `ButtonGroup` + `Button variant="outline" size="icon-xs"`（24 方形，圆角 min(md, 8)，图标 size-3） | 一个 outline 按钮（维护者 2026-11-24：不画两段之间的接缝）：1px 边、`background` 底、shadow-xs、圆角 8、图标 12；悬停 outline 的悬停色；菜单开着 `muted` | 改（原来 1px `line_border`、圆角 6、悬停和开着都是 `handle_active`、图标 14） |
| 块手柄尺寸 | 24 × 48（两个 icon-xs） | 26 高，42 宽（24 + 18），空行 + 28 宽 | 保留 C：编辑器按这些固定尺寸摆手柄（和首行垂直居中、表格手柄在列选择条那一行） |
| 块手柄悬停 | 每个按钮各自悬停 | 两半一起亮 | 我们的：整块是一个控件（按下就是拖动或开菜单），不分左右 |
| 字数小块 | 无 | outline xs 按钮的外观（1px `border`、shadow-xs、8、text-xs muted、`background` 底） | 我们的；改（原来 `button_border`、`shadow_button`） |

## 滚动条（`src/design/scroll.rs`：菜单、面板、弹出层，页面和宽表格同一个样子）

| 属性 | shadcn | 我们 | 处理 |
|---|---|---|---|
| 占位 | ScrollArea 的滚动条浮在内容上，不占宽度，会压住行的右端 | 框内容超高时，右边让出一条通道（`ScrollBar::room`：滑块 1 + 7，再离内容 4），显示、藏起都留着，内容不左右跳；定宽的菜单整体加宽这一条，行的宽度不变；本来就离右边够远的（侧栏 14、设置页 32）不加 | 有意不同：用户 2026-11-25「滚动条会和菜单里面的元素重叠……hover 区域被挡住了一部分，这个不对」 |
| 出现 | 内容超高才有；`type="hover"`：指针在框上、滚动中显示，离开 600ms（`scrollHideDelay`）后藏 | 同，拖动中也显示 | 一致 |
| 显隐 | 直接切换 | 150ms 淡入淡出 | 有意不同（总控 2026-11-25 要的淡入淡出） |
| 宽度 | 条 w-2.5（10），p-px，透明 border-l：滑块 7 | 滑块 7，条 10 接指针 | 一致；shadcn 没有悬停加粗，也不加 |
| 留白 | p-px：四周 1，上下贴到框的两端，圆角容器靠裁切 | 侧边 1；上下各让出整个圆角再加 1（`ends_for(r) = r + 1`，方框 1），按所在面的实际圆角算（菜单 10、弹窗 16；顶端和底端可以不同，`ScrollBar::corners`），滑块到头也只走在直边上；贴着窗口角的框（文档页、设置页、主页列表、版本历史、记录面板）底端按窗口圆角让（`WINDOW_R`：macOS 18，Windows 8） | 有意不同：用户 2026-11-25「滚动条只能从直线的部分开始」。之前的 `ends_for` 只让圆头不出界（圆角 12 时 4.5），仍然伸进弧线 |
| 颜色 | thumb `bg-border`，rounded-full | 同（`thumb_color`） | 一致；页面、宽表格原来的 22% 黑和悬停加深去掉 |
| 悬停 / 拖动 | 没有变化（thumb 一直是 `bg-border`） | 悬停变深一档，按住拖动最深（浅色 `border` → `ring`，深色前景色 10% → 22% → 35%），150ms 过渡；拖动中指针移出滑块也保持最深，松手才回 | 有意不同：照浏览器滚动条（用户 2026-11-25：「拖住滚动的时候，它颜色会变深一点」） |
| 限高 | — | 按钮菜单上下都放不下时 `room` 限高、`Popover::fit` 滚动 | 新增 |
| 子面板 | — | 贴菜单右边（`pos.x + SLASH_W`，滚动时加上通道） | 改 |
| 内容高度 | — | 滚动条的 canvas 放在框中间、零大小：GPUI 把子元素的范围当内容高度，放在框顶会多算一个上内边距，放得下的菜单也能滚 4pt（侧栏右键菜单里铺满的量宽 canvas 同理多 8pt，已改成中间一条线） | 修 |

参考实现怎么做（2026-11-25 查证，按用户的规则取舍）：

- **shadcn / Radix ScrollArea**：滚动条 `position: absolute`，`top: 0; right: 0; bottom: <corner>`，浮在 viewport 上，不占宽度，两端也不让圆角（[Radix scroll-area.tsx](https://github.com/radix-ui/primitives/blob/main/packages/react/scroll-area/src/scroll-area.tsx)）。`type` 有 `hover`（默认）、`scroll`、`auto`、`always`，`scrollHideDelay` 默认 600。shadcn 的条是 `w-2.5 p-px border-l border-l-transparent`，滑块 `rounded-full bg-border`（[scroll-area.tsx](https://github.com/shadcn-ui/ui/blob/main/apps/v4/registry/new-york-v4/ui/scroll-area.tsx)）。shadcn 的菜单反而不用 ScrollArea：DropdownMenuContent 是 `max-h-(--radix-dropdown-menu-content-available-height) overflow-y-auto … p-1`（[dropdown-menu.tsx](https://github.com/shadcn-ui/ui/blob/main/apps/v4/registry/new-york-v4/ui/dropdown-menu.tsx)），Select 用上下滚动按钮，Command 列表也是原生 `overflow-y-auto`。
- **macOS NSScroller**：两种样式，overlay（10.7 起）浮在内容上，legacy 占宽度（[NSScroller.Style](https://developer.apple.com/documentation/appkit/nsscroller/style)），跟着系统「显示滚动条」设置走（[preferredScrollerStyle](https://developer.apple.com/documentation/appkit/nsscroller/preferredscrollerstyle)）。`contentInsets` 只让出边框和「非 overlay 的滚动条」（[contentInsets](https://developer.apple.com/documentation/appkit/nsscrollview/contentinsets)）。`scrollerInsets` 把滚动条从框边往里收（[scrollerInsets](https://developer.apple.com/documentation/appkit/nsscrollview/scrollerinsets)），这是 AppKit 让两端离开圆角的办法。没找到 AppKit 专门处理圆角容器两端的文档。
- **浏览器**：`scrollbar-gutter: stable` 只对 classic 滚动条留槽，不管是否溢出都留；overlay 滚动条浮在内容上，这个属性无效（[MDN scrollbar-gutter](https://developer.mozilla.org/en-US/docs/Web/CSS/scrollbar-gutter)）。圆角容器里 `overflow` 不是 visible 时按弯曲的 padding 边裁切，滚动条两头被圆角切掉，规范建议给内容加 padding（[CSS Backgrounds 3 §corner-clipping](https://drafts.csswg.org/css-backgrounds-3/#corner-clipping)）。
- **GPUI Kit（gpui-component）**：条宽 16，滑块 6、悬停或拖动 8，四周 inset 4（两端也各让 4），最短 48；`ScrollbarMode` 有 `Scrolling`（默认）、`Hover`、`Always`；浮在 viewport 上不占位。菜单 `p_1` 加 `vertical_scrollbar`，可滚时不给右边加内边距，所以默认也压住悬停底色（[scrollbar.rs](https://github.com/longbridge/gpui-component/blob/9d70dc57d7152442855e0d4064e518d9dc238554/crates/base/src/scrollbar.rs)，[popup_menu.rs](https://github.com/longbridge/gpui-component/blob/9d70dc57d7152442855e0d4064e518d9dc238554/crates/component/src/menu/popup_menu.rs)）。

结论：四家都没有让滚动条避开行的悬停底色，shadcn 和 GPUI Kit 都直接叠在行上。用户的规则（不盖可交互内容、只走直边）和这些参考有冲突，我们照用户的规则：占位学 legacy 滚动条和 `scrollbar-gutter`，只在溢出时留，留了就不随显隐变；两端学 `scrollerInsets` / GPUI Kit 的 inset，让出整个圆角。样子（7 宽、`bg-border`、hover 类型、600ms）仍照 shadcn。

## 面包屑（文档页顶栏 `src/app.rs`，文件夹页 `src/folders.rs`）

| 属性 | shadcn | 我们 | 处理 |
|---|---|---|---|
| 链接 | BreadcrumbLink：`muted-foreground`，hover `foreground` | 同（原来悬停变蓝 `primary_500`） | 改 |
| 当前页 | BreadcrumbPage：`foreground`、常规字重 | `muted-foreground`、常规字重，和前面几段一样，不响应点击 | 有意不同：用户 2026-11-25 要求整条路径同色同字重（「这里不用加黑吧，感觉有点不太统一」）；原来是加粗黑色 |
| 分隔符 | `>` 图标，随列表 `muted-foreground` | 同（原来是图标色，比文字深） | 改 |

## 菜单里的开关行（`MenuItem::switch`）

| 属性 | shadcn / Radix | 我们 | 处理 |
|---|---|---|---|
| 切换后 | DropdownMenuCheckboxItem 默认选中后关菜单，`onSelect` 里 `preventDefault` 才留住 | 固定留住：点行或开关只切换，Esc、点外面才关 | 按留住做（用户 2026-11-25：「开关开启或者关闭完之后，能不能不要自动关闭菜单」，和页宽选择一致） |
| 开关动画 | Switch 的 thumb `transition-transform`，track `transition-all` | `Switch::keyed`：150ms ease-out 滑动，轨道和滑块颜色同步过渡 | 一致 |
| Esc | 关菜单 | 关菜单；编辑器把 Esc 绑成自己的动作，在窗口根部用 `capture_action` 先截住 | 一致（原来编辑器有焦点时 Esc 被拿去选块，菜单关不掉） |
