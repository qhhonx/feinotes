# 组件库：盘点与替换进度

状态：2026-11-22 开始。设计系统见 [DESIGN_SYSTEM.md](DESIGN_SYSTEM.md)，组件代码在 `src/design/`，所有组件都在组件库面板里（`src/gallery/`，`scripts/components.sh` 单独打开，可交互，可切浅色、深色、浅深对照）。

## 一、盘点：数据表以外最高频的界面写法

2026-11-22 在迁移前的代码上用 grep 统计（`src/` 下除 `src/table/` 以外的 `.rs` 文件）。「次数」是匹配到的调用次数，「文件」是出现在几个文件里。同一个组件常常有好几种旧写法，表里合在一起算。

| # | 组件 | 旧写法 | 次数 | 文件 | 新组件 |
|---|---|---|---|---|---|
| 1 | 菜单外壳 | `ui::popover()`、`editor_ui::menu()`、`editor_ui::popover()`，加上调用方自己写的 `.py(px(MENU_PAD_Y))` 和 `MENU_*` 常量 | 55 次外壳，`MENU_*` 常量 100 处 | 25 / 23 | `Popover`（`.menu()`、`.padded()`） |
| 2 | 菜单项 | `ui::menu_row(id, group)` + `menu_label` + `menu_glyph`，或调用方手拼 `icon` + 文字 | 23 + 31 | 15 | `MenuItem`（图标、文字、说明、快捷键、勾选、子菜单、危险、不可用） |
| 3 | 悬停底色的可点区域（手写的图标按钮、工具按钮） | `.hover(\|s\| s.bg(Theme::fill_hover()))` 加 `.rounded(..).cursor_pointer()` | 45 | 21 | `Button::ghost`（`ButtonSize::Tool / Xs`，图标按钮就是不给 `label`） |
| 4 | 按钮 | `ui::primary_button`、`ui::secondary_button`、`settings::button`、各处手写的按钮 | 41 | 11 | `Button::primary / outline / secondary / ghost / destructive` |
| 5 | 提示（tooltip） | `.tip()`、`tip_above`、`tip_md`、`tip_right` | 84 | 22 | 调用不变（组件都实现了 `TipExt`）；气泡 `ui::Tooltip` 改成 shadcn 的样子（token `tip_r`、`tip_px`、`tip_py`） |
| 6 | 对话框 | `ui::dialog`、`ui::scrim`、各处 `rounded(16)` + `shadow_dialog` | 24 | 12 | `Dialog`（标题、说明、正文、底部按钮）、`scrim()` |
| 7 | 输入框 | `TextField` 外面手写的框（圆角、描边、聚焦色） | 22 处 `TextField` | 13 | `Field`（`FieldSize::Default / Sm`、`focused`、`disabled`）、`Textarea`、`Label` |
| 8 | 分隔线 | `div().h(px(1.)).bg(Theme::line_border())` 等 | 20 | 15 | 菜单里用 `MenuSeparator`，页面和卡片里用 `Separator` |
| 9 | 头部图标按钮、关闭按钮 | `ui::header_icon`、`ui::close_button` | 13 | 9 | `Button::ghost(..).icon(..)` |
| 10 | 开关 | `settings::switch`、`docmenu` 里手写的开关、`editor/overlays.rs` 的 `switch` | 12 | 6 | `Switch` |
| 11 | 复选框 | `ui::checkbox` | 9 | 5 | `Checkbox` |
| 12 | 分段控件 | 设置页、搜索里手写的分段 | 10 | 3 | `Segmented` + `Segment` |
| 13 | 下拉按钮 | `settings::popup_button` | 5 | 1 | `Field::select` |

另外几个数字，说明旧体系散在哪里：`Theme::fill_hover()` 82 处、`theme::shadow_button/menu/panel/dialog` 48 处。

## 二、组件库（`src/design/`）

| 文件 | 组件 | 对应 shadcn |
|---|---|---|
| `mod.rs` | token：`color`、`chip`、`geo()`（`G`）、`shadow`、`R`、`text`、`radius` | 主题变量、Vega 尺寸 |
| `base.rs` | `under()`（组件样式垫在调用方样式下面）、`element!` 宏、`icon()` | — |
| `button.rs` | `Button` + `ButtonVariant`（Primary / Outline / Secondary / Ghost / Destructive / Link）+ `ButtonSize`（Lg 40 / Default 36 / Sm 32 / Tool 28 / Xs 24）；`icon`、`icon_end`、`label`、`selected`、`disabled`、`variant` | Button |
| `card.rs` | `Card`、`Item`、`Separator`、`Toast` + `ToastTone` | Card、Item、Separator、Sonner |
| `menu.rs` | `Popover`（`.menu()`、`.padded()`）、`MenuItem`、`MenuLabel`、`MenuSeparator`；常量 `menu::PAD`、`ROW_H`、`SEP_H`、`LABEL_H` 给算子菜单位置用 ；`MenuItem::switch(on)`（带开关的行：点了只切换，菜单留着，Esc 和点外面才关）；`hang`（按钮菜单挂在量到的按钮下 4、`Align::Start / End`、放不下翻上去）、`room` + `Popover::fit`（放不下时限高、滚动） | DropdownMenu、Popover |
| `scroll.rs` | `ScrollBar`（`render(&handle)`，`rounded(r)`：框铺满一个圆角 r 的面时，上下让出 `ends_for(r)`，圆头不碰圆角）、`scroll::bar(key, &handle, r)`（给任何 `overflow_y_scroll` 框挂上，放在框的最后一个子元素）、`Fade`、`THUMB` / `PAD` / `thumb_color`（页面和宽表格自己的滚动条也用） | ScrollArea、ScrollBar |
| `field.rs` | `Field`（`Field::select`、`Field::select_value`，`FieldSize`）、`Textarea`、`Label` | Input、SelectTrigger、Textarea、Label |
| `control.rs` | `Checkbox`（含半选）、`Radio`、`Switch`（`.keyed(id)`：切换时滑过去）、`Segmented` + `Segment`（可带图标） | Checkbox、RadioGroupItem、Switch、Tabs |
| `badge.rs` | `Badge`（`new` / `primary` / `outline` / `destructive`、`chip(i)`、`colored`、`closable`）、`Kbd`、`KbdGroup` | Badge、Kbd、KbdGroup |
| `dialog.rs` | `Dialog`（`title`、`description`、`footer`、`closable`、`keys`；`Dialog::alert`）、`DialogFocus`、`scrim()` | Dialog、AlertDialog |
| `toolbar.rs` | `Toggle`（`ToggleSize`）、`ButtonGroup`、`ButtonGroupSeparator`、`Toolbar`、`block_handle()`；常量 `TOOLBAR_H`、`TOOLBAR_PAD`、`TOOLBAR_GAP`、`toolbar::w::*` 和 `toolbar_layout()` 给算下拉位置用 | Toggle、ToggleGroup、ButtonGroup（Toolbar 是这几样拼的） |

### 公开 API

全部是纯 builder：状态（选中、勾选、展开、聚焦）由调用方传进来，组件不持有状态、不读主应用的 `App` / `Workspace`，在任何窗口里都能直接构造。交互靠调用方挂 `.on_click()` 等处理函数、改自己的状态后重画。

| 组件 | 构造 | 变体 / 尺寸 | 状态和插槽 | 可挂的处理 |
|---|---|---|---|---|
| `Button` | `Button::new(id, ButtonVariant)`，或 `primary / outline / secondary / ghost / destructive / link(id)` | `ButtonVariant::{Primary, Outline, Secondary, Ghost, Destructive, Link}`；`.sized(ButtonSize::{Lg 40, Default 36, Sm 32, Tool 28, Xs 24})`（`ghost` 默认 `Tool`；Xs 是 12 号字、12 的图标）；`.variant(v)` 换掉别处建好的按钮的变体 | `.label(s)`、`.icon(name)`（只有图标时是方形按钮；图标和文字同色）、`.editor_icon(name)`（编辑器图标集的同名图标）、`.icon_end(name)`（有图标、没文字、带尾部箭头时是菜单触发器，不是方形）、`.selected(bool)`（在用：shadcn 的 `aria-expanded` / Toggle 的按下，`muted` 底；ghost 悬停也是 `muted`，深色 `muted/50`；在深色弹层、对话框、卡片上提一档：悬停白 6%、在用 `accent`）、`.disabled(bool)`（整体 50%）；按下时下移 1pt；`.child()` 加在图标和文字前面（要自定颜色的图标、记位置的 canvas） | `Styled`、`InteractiveElement`、`StatefulInteractiveElement`（`.on_click`）、`TipExt`（`.tip`）、`ParentElement` |
| `Popover` | `Popover::new(id)` | `.menu()`（一列菜单项，上下 `PAD`）、`.padded()`（表单弹窗：内边距 16、间距 16） | 子元素随意（`ParentElement`） | 同上，另有 `.on_mouse_down_out` 等 |
| `MenuItem` | `MenuItem::new(id)` | — | `.label(s)`、`.icon(name)`、`.leading(el)`、`.description(s)`（两行）、`.shortcut(s)`、`.checked(bool)`（单选标记，关时留位）、`.submenu(bool)`、`.inset()`、`.editor_icon(name)`（编辑器图标集里的同名图标）、`.selected(bool)`（键盘当前行）、`.open(bool)`（子菜单开着：一直亮着）、`.destructive(bool)`、`.disabled(bool)`（整行 50%）；亮的行文字、图标、快捷键都是 `accent-foreground`，见下面「菜单的高亮」；`.child()` 加在文字后面 | 同 `Button` |
| `MenuLabel` | `MenuLabel::new(s)` | — | — | `Styled` |
| `MenuSeparator` | `MenuSeparator::new()` | — | — | — |
| `Field` | `Field::new(id)`；`Field::select(id)`（带箭头）；`Field::select_value(id, s)`（小号、显示一个值） | `.sized(FieldSize::{Default 36, Sm 32})` | `.focused(bool)`（输入中）、`.open(bool)`（选择框展开；照 shadcn 不改外观）、`.invalid(bool)`（出错：`destructive` 边和光晕）、`.disabled(bool)`（整体 50%）；`.child()` 放输入控件（`crate::input::TextField` 实体）或文字 | 同 `Button` |
| `Textarea` | `Textarea::new(id)` | — | `.focused(bool)`、`.invalid(bool)`、`.disabled(bool)`；`.child()` | 同 `Button` |
| `Label` | `Label::new(s)` | `.muted()`（说明文字，shadcn 的 FieldDescription：14 号 muted） | `.disabled(bool)` | — |
| `Checkbox` | `Checkbox::new(checked)` | — | `.partial(bool)`（部分选中：实心加横杠）、`.disabled(bool)` | `Styled`（点击由外层可点元素接） |
| `Radio` | `Radio::new(picked)` | — | `.disabled(bool)` | `Styled`（同上） |
| `Switch` | `Switch::new(on)` | — | `.disabled(bool)` | `Styled`（同上） |
| `Segmented` / `Segment` | `Segmented::new().child(Segment::new(id, label).selected(bool))`；`Segmented::of(id, &[..], picked)` | — | `Segment::selected(bool)`、`Segment::disabled(bool)`、`Segment::icon(name)`（文字为空时是图标分段，如宫格 / 列表切换） | `Segment`：同 `Button` |
| `Badge` | `Badge::new(s)`（secondary）、`Badge::primary / outline / destructive(s)`、`Badge::chip(s, i)`（第 i 种选项色，9 为灰）、`Badge::colored(s, (底, 字))` | — | `.closable()`（末尾 ×） | `Styled` |
| `Kbd` | `Kbd::new(s)` | — | — | — |
| `KbdGroup` | `KbdGroup::new(strokes)`（每一下按键是一组键名，如 `shortcuts::strokes(key)`） | — | — | — |
| `Dialog` | `Dialog::new(id)`；`Dialog::alert(id)`（AlertDialog：确认类问题，不带 ×，默认宽 512，18 号标题） | — | `.title(s)`、`.description(s)`、`.footer(el)`；`.child()` 是正文；`.closable(on_close)`（右上角 ×：ghost 32，距上右 16；shadcn 的 Dialog 默认带）；`.keys(&DialogFocus, esc, enter)`（接管键盘：Esc 取消，↩ 执行默认按钮） | 同 `Button` |
| `DialogFocus` | `DialogFocus::new(cx)`，存在持有对话框状态的视图里 | — | 每次绘制时 `.sync(open, window, cx)`：打开时把焦点从原来的地方（多半是编辑器）拿到对话框，关上时还回去 | — |
| `scrim()` | 函数，返回铺满父元素、居中的遮罩 `Div` | — | — | — |
| `Card` | `Card::new(id)` | — | `.child()`（默认一列） | 同 `Button`，另有 `ParentElement` |
| `Item` | `Item::new(title)` | — | `.description(Option<s>)`；`.child()` 是右边的控件 | `Styled`、`ParentElement` |
| `Separator` | `Separator::new()` | `.vertical()` | — | `Styled` |
| `Toast` | `Toast::new(ToastTone::{Success, Error, Info}, s)`（Sonner：宽 356、圆角 18、13 号字、Lucide 的线框图标） | — | — | `Styled` |
| `Toggle` | `Toggle::new(id)`（默认 sm） | `.sized(ToggleSize::{Default 36, Sm 32, Lg 40})`、`.outline()` | `.icon(name)` / `.editor_icon(name)`、`.label(s)`、`.pressed(bool)`（`aria-pressed`：`muted` 底，深色弹层上提一档）、`.disabled(bool)` | 同 `Button` |
| `ButtonGroup` | `ButtonGroup::new().child(button)…` | — | 子元素紧挨着，里侧圆角变直角、里侧左边框去掉；`.separator(ButtonGroupSeparator::new())` | `Styled` |
| `ButtonGroupSeparator` | `ButtonGroupSeparator::new()` | — | 1px `input` 竖线，和这一行一样高，上下各缩 1 | `Styled` |
| `Toolbar` | `Toolbar::new(id)` | — | `.child()` 是一排工具（Toggle、ghost `Button`、`ButtonGroup`、分隔线）；`.overlay(el)` / `.overlays(..)` 挂下拉（绝对定位，从工具条左上角算） | `Styled`、`InteractiveElement`、`StatefulInteractiveElement`、`ParentElement` |
| `block_handle(key, glyph, w, open)` | 函数：编辑器的块手柄 | 42 宽（类型图标 + ⋮⋮），没有 glyph 时是 28 宽的 + | `open`：菜单开着 | 返回 `Stateful<Div>`，调用方挂按下 |
| `icon(name, size, color)` | 两套图标里任取一个：外壳图标集没有的名字自动去编辑器图标集找，`MenuItem::icon` 等组件里的图标同样适用 | — | — | — |

菜单位置计算用的常量：`design::menu::{PAD, ROW_H, SEP_H, LABEL_H}`。

### 工具条

shadcn 没有 Toolbar，这里按它已有的零件拼（2026-11-23，维护者要求把编辑器工具条和块手柄纳入组件库）：

- **外壳**：`Toolbar` = `Popover` 的底色和边缘（`shadow::popover()`），内边距 4（Popover 的 p-1），工具之间 4（shadcn 文字格式示例的 `ToggleGroup spacing={1}`），高 40（工具都是 sm 32）。
- **工具**：开 / 关的（粗体、斜体、对齐、裁剪、视图）用 `Toggle`，36 × 32；一次性动作（链接以外的：下载、合并、删除、添加描述）用 ghost `Button` sm 方形 32；带下拉的用 ghost `Button` sm 加 `icon_end("chevron-down")`，48 宽，下拉开着时 `selected`（`aria-expanded`）；分体按钮（A▾）用 `ButtonGroup` 包两个 32 方形 ghost 按钮；组和组之间用 `ButtonGroupSeparator`。链接按钮不会「开着」，但为了和旁边的记号一样宽，也用 `Toggle`。
- **不要阴影、不要胶囊**：按下、展开、选中一律 `muted` 底（深色弹层上 `accent`），圆角跟按钮（8）。
- **算位置**：下拉要贴在按钮下面，按钮的起点用 `toolbar_layout(&widths)` 算（`widths` 用 `toolbar::w::{TOGGLE, BUTTON, DROPDOWN, SPLIT, SEPARATOR}`），同时得到整条宽度；下拉放在 `top = TOOLBAR_H + 4`。编辑器里的 `toolbar_widths()`、`table_toolbar_widths()` 是现成的例子。
- **块手柄**：`block_handle()`，shadcn 的 `ButtonGroup` 加两个 `Button variant="outline" size="icon-xs"`：1px 边、`background` 底、shadow-xs，指针在上面时两半一起变 outline 的悬停色，菜单开着时 `muted`；类型图标保留各类型的颜色，⋮⋮ 用 `muted-foreground`，图标 12。尺寸保持 26 高、42 / 28 宽（偏差 C）。
- 用到的地方：选中文字工具条、表格选区工具条（`overlays.rs`），高亮块悬停条（`callout_bar`），图片工具条（`image.rs`），附件工具条和行内附件的悬停卡片（`file.rs`），块手柄（`overlays.rs` 的 `handle`）。

### `table::style` 里名字的变化

和 bitable 里的旧 `style.rs` 相比，下面这些名字有变化，接 `table::style` 的代码照着改：

| 旧 | 现在 |
|---|---|
| `popover()`（返回 `Div`） | 删掉，改用 `design::Popover::new(id)`（`.menu()` / `.padded()`） |
| `input_box()`、`field_box()` | 要传 id：`input_box(id)`、`field_box(id)`，返回 `design::Field` |
| `field_focused()` | 删掉，改用 `Field::new(id).focused(true)` |
| `select_trigger(id, open)` | 删掉，改用 `Field::select(id).open(open)`；不需要箭头时用 `Field::new(id).focused(open)` |
| `area_box()` | 删掉，改用 `design::Textarea::new(id)` |
| `segmented(id, opts, picked)` | 删掉，改用 `design::Segmented::of(id, opts, picked)` |
| `menu_item_hovered(..)` | 删掉，改用 `menu_item(..).selected(true)` |
| `chip_x(id, label)` | 删掉，改用 `design::Badge::new(label).closable()` |
| `kbd(s)` | 删掉，改用 `design::Kbd::new(s)` |
| `danger_btn`、`icon_btn` | 删掉，改用 `Button::destructive(id).label(..)`、`Button::ghost(id).icon(..)` |
| `icon_btn(..).size(px(24.))` 这类写法 | 改成 `.sized(ButtonSize::Xs)`：`.size()` 是 GPUI 设宽高的方法 |
| 其余（`primary_btn`、`plain_btn`、`ghost_btn`、`tool_btn`、`close_btn`、`menu_item`、`menu_row`、`list_row`、`menu_rule`、`menu_title`、`checkbox`、`switch`、`seg_track`、`segment`、`pick_box`、`select_box`、`field_label`、`panel_title`、`tag`、`link_chip`） | 名字和参数不变，返回值从 `Div` / `Stateful<Div>` 变成对应的 design 组件。链式调用照旧可用 |
| `design::Look`、`with_look`、`set_look`、`affect()`、`color::text()` / `strong()` / `surface()` / `border_strong()` / `check_edge()`、`shadow::card()` | 随 affectUI 风格一起删掉；`text` 和 `strong` 改用 `color::foreground()` |

### 菜单的高亮

对齐 Radix：同一个菜单里任何时候最多亮一行。

- `Popover` 就是一个菜单：它里面的 `MenuItem`（不算更里层的 `Popover` 里的）在 prepaint 时登记自己的位置和 `selected`，`Popover` 在 paint 前定出亮哪一行，行只在被选中时画 `accent` 底。状态按 `Popover` 的 id 存在 GPUI 的元素状态里，菜单关掉就丢掉。
- 悬停和 `selected`（键盘当前行）谁后变谁说了算：指针移到一行上、在一行上移动、或离开所有行，指针说了算；`selected` 换了一行，键盘说了算。**指针离开所有行时一行都不亮**（Radix 的规则；键盘再动一下，键盘行才重新亮）。
- 亮的行文字、图标、快捷键、说明都换成 `accent-foreground`。行在排版时读上一帧定下的亮行；定下的亮行变了，就再画一帧。
- 子菜单开着的那一行用 `.open(true)`：不管指针在哪都亮着（shadcn 的 `data-open:bg-accent`）。照 Radix，指针移到同一菜单的兄弟行时，调用方把子菜单关上。
- 指针在不可用的行上时谁都不亮。不在任何 `Popover` 里的 `MenuItem`（对话框里嵌的列表）照旧：悬停、`selected` 或 `open` 就亮。
- 2026-11-23 起，表示「子菜单开着」的调用点从 `.selected` 改成了 `.open`：`docmenu.rs` 的 `menu_item`、`sidebar.rs` 的「下载」、`editor/overlays.rs` 的 `row`（块菜单的缩进、颜色、在下方添加）和斜杠菜单里带子面板的行（`.open(sub && n == selected)`）、组件面板的子菜单演示。

### 对话框的键盘

编辑器有焦点时，GPUI 先派发编辑器自己的 Esc、↩ 绑定，窗口上的按键处理轮不到。所以对话框打开时用 `DialogFocus` 把焦点移到对话框上（key context `Dialog`），背后的编辑器收不到按键（shadcn 的焦点圈定）；关上时还给原来的焦点（如果期间别的地方拿走了焦点，就不还）。

### 用法约定

- 组件是 GPUI 的 `RenderOnce` 结构体。调用方照常链式调用 `.w()`、`.on_click()`、`.tip()`、`.when()`；渲染时组件把自己的样式垫在下面，调用方写的尺寸、颜色优先。
- 悬停的样式由组件决定。要表达状态用 `selected`、`disabled`、`destructive`、`checked`，不要自己写 `.hover()`。
- 方法名避开 GPUI 的 `Styled`：尺寸档位用 `.sized(ButtonSize::Sm)`，不是 `.size(..)`（那是 GPUI 设宽高的方法）。
- 文案由调用方先本地化（`crate::settings::label` / `tr`），组件不再翻译一次。
- 菜单的位置计算用 `design::menu` 的常量。新外壳没有 1px 边框（边缘是阴影画的 0.5px 环），所以旧代码里为边框加的 `1.`、`2.` 要去掉。
- 数值只取 `design` 的 token，不写十六进制颜色。

### 和设计系统文档的出入

- 删掉了对比用的 affectUI 风格（`Look::Affect`、`QUIET_MD_LOOK`）：每个 token 和组件不再按风格分支。组件库面板的「浅深对照」用 `theme::with_dark` 把同一页按浅色、深色并排画。
- 2026-11-23 逐项照 shadcn 还原（[SHADCN_AUDIT.md](SHADCN_AUDIT.md)）：ghost 按钮悬停和在用改回 `muted`（深色弹层上提一档）；按钮图标和文字同色，新增 `Link` 变体和 `Lg` 尺寸，Xs 改成 12 号字和 12 的图标；菜单项亮起时文字变 `accent-foreground`、不可用整行 50%、新增 `.open()`；Dialog 的 × 改成 32、距边 16，新增 `Dialog::alert`；分段控件 36 高；Toast 照 Sonner；Item 照 shadcn 的间距；提示气泡立即出现；遮罩深色也是黑 10%；斜杠菜单不再给滚动条常驻留空。
- 2026-11-23 打磨：新增 token `color::accent_hover`（ghost 按钮悬停，深色白 6%）；`Dialog::closable`、`Dialog::keys`、`DialogFocus`；组件库面板自己的外壳（左侧导航、顶部浅色 / 深色 / 浅深对照、点击提示）换成 `Button::ghost`、`Segmented`、`Toast`。
- 新增 token：`button_sm_h`、`dialog_r`（R+4）、`dialog_pad` / `dialog_gap`（24，Vega 的 `p-6 gap-6`）、`shadow::sm()`（选中的分段）、`shadow::dialog()`、`color::secondary_hover`、`destructive_hover`、`disabled`、`overlay`、`background`；2026-11-22 全应用替换时又加了 `card_r`（R+4）、`toast_r` / `toast_px` / `toast_py`（Sonner：`rounded-lg`、p-4 左右、上下 12）、`tip_r` / `tip_px` / `tip_py`（tooltip：`rounded-md`、px-3 py-1.5）。
- `MenuItem` 的文字统一用 `foreground`、图标 `muted-foreground`；危险项文字和图标都是 `destructive`，悬停 `destructive/10`。

## 三、替换进度

| 类 | 状态 | 范围 |
|---|---|---|
| 数据表 | ✅ | `table::style` 改成转发到组件库（`menu_item`、`primary_btn`、`input_box` 等保留名字，返回新组件）；表格专用的部分（网格尺寸、选项颜色 `tag_colors`、选区、`swatch`、`person`、`inset_list`）留在 `style.rs`。所有 `popover()` 换成 `Popover` |
| 1. 菜单（应用外壳） | ✅ | 「…」文档菜单和它的子菜单（`docmenu.rs`）、侧栏右键菜单和批量菜单、下载子菜单（`sidebar.rs`）、新建菜单和文件夹右键（`folders.rs`）、本机文件 / 挂载文件夹右键（`local_files.rs`、`local_folders.rs`）、导入和「…」位置菜单（`transfer.rs`）、主页排序（`home.rs`）、回收站右键（`trash.rs`）、编辑 / 阅读模式（`modes.rs`）、引导页语言（`onboarding.rs`）、设置里的主题 / 偏好 / 字体 / 语言下拉（`settings.rs`）。删掉了 `ui::menu_label`、`ui::menu_glyph`、`theme::shadow_s4_down` |

| 2. 全应用（2026-11-22，分支 `claude/swap-all-components-da81f8`） | ✅ | **编辑器菜单**：斜杠、块、转换、工具栏下拉、链接、@ 提及、编号（`overlays.rs`、`link.rs`、`links.rs`、`numbering.rs`），位置计算改用 `menu::{PAD, ROW_H, SEP_H, LABEL_H}`；**按钮**：页头图标按钮（`ui::header_icon` 现在返回 `Button::ghost`）、侧栏和主页的工具按钮、批量工具栏、文件夹卡片上的浮动按钮、各处手写的悬停图标按钮；**对话框**：确认、移动到、导出 Markdown、文档信息、搜索、强制更新、倒计时、表格记录详情（`Dialog` + `scrim()`）；**开关 / 复选框 / 分段 / 单选**：设置、文档菜单、批量勾选（含半选）、主页标签和宫格 / 列表切换、导出范围、倒计时；**输入框**：移动到、查找替换、链接、表情、封面、按钮块设置、提醒、倒计时、设置里的快捷键录制（`Field` + `KbdGroup`）；**卡片**：设置分组、引导页、更新日志（`Card` + `Item` + `Separator`）；**浮层外壳**：版本历史、查找栏、图片 / 附件工具栏、封面和表情面板、画板主菜单和右键菜单（`Popover`）；**提示气泡和 toast**：`ui::Tooltip` 改成 shadcn 的样子，toast 用 `Toast`。删掉了 `ui::popover`、`ui::menu_row`、`ui::MENU_*`、`ui::primary_button`、`ui::secondary_button`、`ui::close_button`、`ui::dialog`、`ui::scrim`、`ui::checkbox`、`ui::pill`、`ui::continuous_surface`、`editor_ui` 里的菜单转发 |

### 还没换的

- **画板工具栏、属性面板、颜色选择器、帮助对话框外壳**（`board/chrome/`）：照搬 Excalidraw 的样式和图标，工具按钮带编号角标、色块和字号滑块，组件库里没有对应的东西；只换了它的菜单和确认按钮。要不要整体换成 design 风格待定。
- **压在影片背景上的引导页按钮**（跳过、上一步、全选）：深色半透明白字，组件的悬停底色在草地画面上看不清；背景是平的时候已经用 design 按钮。
- **图片大图查看器的深色控制条**：压在黑色遮罩上的白色图标，ghost 的 `muted` 在上面看不见。
- **代码块头上的语言 / 复制 / 换行**（`render.rs`）：在块里面、跟文档主题走（文档字体、主题色），不是浮起来的工具条。
- **倒计时块的设置 / 颜色按钮**（`countdown.rs`）、**斜杠菜单和块菜单的图标格子**（`overlays.rs` 的 `tool`）：在菜单或块里面，不是工具条。
- **表格的行 / 列选择条**（`table.rs`）：贴着表格边的细条，带插入点，不是按钮。
- **选中文字时的字数小块**：2026-11-23 起照 outline xs 按钮的外观画（1px `border`、shadow-xs、圆角 8、12 号 muted 字），但它不是按钮，没有套组件。
- **侧栏的「更新」药丸**：带下载进度的波浪填充。
- **没有对应组件、各只有一两处的**：提醒和倒计时的小日历、表情格子、封面面板和查找框顶部带下划线的页签、引用列表的页签、颜色色块、尺寸选择器格子、倒计时的两张选项卡片。
- **列表行**（侧栏树、主页列表、回收站、搜索结果、设置导航、版本历史行）：按约定保持列表行的样子，只把颜色换成 token。
- **对话框的键盘**：导出 Markdown、文档信息已用 `DialogFocus`（Esc、↩ 不再落到编辑器上）；确认框用它自己的 `confirm_focus`（key context `Confirm`，`ConfirmOk` / `ConfirmCancel` 动作）。没套的：移动到（打开时焦点就在它的搜索框里，Esc 由搜索框收）、搜索（同样有自己的输入框）、强制更新（窗口在 capture 阶段吞掉所有按键）、倒计时（编辑器里，已有自己的 `CountdownDialog` 焦点）、数据表记录详情（有自己的头部和关闭逻辑，没动）。
