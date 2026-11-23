//! 右键菜单: what a right-click in the page offers for 正文 (a selection, or
//! the caret) and for 表格 cells.
//!
//! Every row dispatches the action the keyboard runs — never a second
//! implementation — so a click and a shortcut cannot drift apart. The few
//! items with no action (table rows and columns, 转换为) call the same core
//! operations the toolbars do.
//!
//! Three groups at most, destructive items last in red, no shortcut hints
//! (Apple HIG: a context menu *is* the shortcut; the menu bar carries the
//! key equivalents).

use super::keymap as k;
use super::menus::{TURN_ITEMS, TurnItem};
use super::overlays::{MENU_W, at, row, row_tinted, separator_h};
use super::table::{CellSel, CellSelKind};
use super::view::{BlockAct, CtxKind, CtxSub, Editor, Menu};
use crate::model::BlockKind;
use crate::editor_ui::{MENU_PAD_Y, MENU_ROW_H, menu};
use crate::feishu_theme::Theme;
use gpui::{prelude::FluentBuilder, *};

/// A separator: a 1px rule in 4px margins.
const SEP_H: f32 = 9.;
/// How far a submenu sits right of its menu.
const SUB_DX: f32 = 4.;

/// The menu being built: rows and group rules, one child at a time.
struct Panel {
    div: Stateful<Div>,
}

impl Panel {
    fn new() -> Self {
        Self { div: menu().id("ctx-menu").occlude().w(px(MENU_W)).py(px(MENU_PAD_Y)).flex().flex_col() }
    }

    fn push(mut self, el: impl IntoElement) -> Self {
        self.div = self.div.child(el);
        self
    }

    fn rule(mut self) -> Self {
        self.div = self.div.child(separator_h());
        self
    }

    fn when(self, on: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if on { f(self) } else { self }
    }
}

/// A submenu panel, hung at `top` beside the main one.
fn sub_menu(id: &'static str, top: f32) -> Stateful<Div> {
    menu()
        .id(id)
        .occlude()
        .absolute()
        .left(px(MENU_W + SUB_DX))
        .top(px(top))
        .w(px(MENU_W))
        .py(px(MENU_PAD_Y))
        .flex()
        .flex_col()
}

fn item(id: impl Into<ElementId>, icon_name: &str, label: impl Into<SharedString>) -> Stateful<Div> {
    row(id, icon_name, label, None, false)
}

/// A destructive row: red label and glyph.
fn danger_row(id: impl Into<ElementId>, icon_name: &str, label: impl Into<SharedString>) -> Stateful<Div> {
    row_tinted(id, icon_name, label, None, false, Theme::danger(), Theme::danger())
}

/// A click: close the menu, then run `a` exactly as its shortcut would.
fn fire<A: Action + Clone + 'static>(cx: &Context<Editor>, a: A) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |e: &mut Editor, _: &MouseDownEvent, window, cx| {
        cx.stop_propagation();
        e.menu = None;
        window.dispatch_action(Box::new(a.clone()), cx);
        cx.notify();
    })
}

/// A click that calls the editor directly, for the items with no action.
fn act<F>(cx: &Context<Editor>, f: F) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static
where
    F: Fn(&mut Editor, &mut Window, &mut Context<Editor>) + 'static,
{
    cx.listener(move |e: &mut Editor, _: &MouseDownEvent, window, cx| {
        cx.stop_propagation();
        e.menu = None;
        f(e, window, cx);
        cx.notify();
    })
}

/// Hovering a row opens (`Some`) or closes (`None`) its submenu.
fn hover(s: Option<CtxSub>, cx: &Context<Editor>) -> impl Fn(&MouseMoveEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |e: &mut Editor, _: &MouseMoveEvent, _, cx| {
        if let Some(Menu::Ctx { sub, .. }) = &mut e.menu
            && *sub != s
        {
            *sub = s;
            cx.notify();
        }
    })
}

/// A row that runs an action.
fn fire_row<A: Action + Clone + 'static>(cx: &Context<Editor>, id: impl Into<ElementId>, icon_name: &str, label: impl Into<SharedString>, a: A) -> Stateful<Div> {
    item(id, icon_name, label).on_mouse_move(hover(None, cx)).on_mouse_down(MouseButton::Left, fire(cx, a))
}

/// A row that runs editor code.
fn act_row<F>(cx: &Context<Editor>, id: impl Into<ElementId>, icon_name: &str, label: impl Into<SharedString>, f: F) -> Stateful<Div>
where
    F: Fn(&mut Editor, &mut Window, &mut Context<Editor>) + 'static,
{
    item(id, icon_name, label).on_mouse_move(hover(None, cx)).on_mouse_down(MouseButton::Left, act(cx, f))
}

/// A destructive row that runs editor code.
fn danger_act<F>(cx: &Context<Editor>, id: impl Into<ElementId>, icon_name: &str, label: impl Into<SharedString>, f: F) -> Stateful<Div>
where
    F: Fn(&mut Editor, &mut Window, &mut Context<Editor>) + 'static,
{
    danger_row(id, icon_name, label).on_mouse_move(hover(None, cx)).on_mouse_down(MouseButton::Left, act(cx, f))
}

/// A row that opens a submenu.
fn sub_row(cx: &Context<Editor>, id: impl Into<ElementId>, icon_name: &str, label: impl Into<SharedString>, s: CtxSub, open: bool) -> Stateful<Div> {
    row(id, icon_name, label, Some("›"), open).on_mouse_move(hover(Some(s), cx))
}

/// 转换为's rows: 正文 through 标题 3, lists, 引用, 代码块 (by kind, not label).
fn turn_items() -> impl Iterator<Item = (usize, &'static TurnItem)> {
    TURN_ITEMS.iter().enumerate().filter(|(_, t)| !t.action.is_empty() && !matches!((t.kind)(), BlockKind::Heading { level: 4.., .. }))
}

impl Editor {
    /// The right-click menu at `pos` (page coordinates).
    pub(super) fn ctx_menu(&self, pos: Point<Pixels>, kind: CtxKind, sub: Option<CtxSub>, cx: &mut Context<Self>) -> Option<AnyElement> {
        let mut p = Panel::new();
        // The caret menu's rows, top down: 粘贴 / 粘贴为纯文本 / 全选, a rule,
        // 插入, a rule, 转换为, 对齐, 上移块, 下移块. Each submenu hangs at its
        // own row, so these follow the same constants the panel is built from.
        let (insert_y, turn_y, align_y) = (MENU_PAD_Y + 3. * MENU_ROW_H + SEP_H, MENU_PAD_Y + 4. * MENU_ROW_H + 2. * SEP_H, MENU_PAD_Y + 5. * MENU_ROW_H + 2. * SEP_H);

        match kind {
            CtxKind::Selection => {
                p = p
                    .push(fire_row(cx, "ctx-cut", "scissors", crate::settings::text("剪切"), k::Cut))
                    .push(fire_row(cx, "ctx-copy", "copy", crate::settings::text("复制"), k::Copy))
                    .push(fire_row(cx, "ctx-paste", "import", crate::settings::text("粘贴"), k::Paste))
                    .push(fire_row(cx, "ctx-paste-plain", "baseline", crate::settings::text("粘贴为纯文本"), k::PastePlain))
                    .rule()
                    .push(fire_row(cx, "ctx-md", "export", crate::settings::text("复制为 Markdown"), k::CopyMarkdown))
                    .push(fire_row(cx, "ctx-all", "batch", crate::settings::text("全选"), k::SelectAll))
                    .rule()
                    .push(fire_row(cx, "ctx-link", "link", crate::settings::text("插入链接"), k::Link))
                    .push(fire_row(cx, "ctx-code", "code", crate::settings::text("行内代码"), k::InlineCode))
                    .push(
                        danger_row("ctx-delete", "trash-2", crate::settings::text("删除"))
                            .on_mouse_move(hover(None, cx))
                            .on_mouse_down(MouseButton::Left, fire(cx, k::Delete)),
                    );
            }
            CtxKind::ReadOnly { sel } => {
                if sel {
                    p = p
                        .push(fire_row(cx, "ctx-copy", "copy", crate::settings::text("复制"), k::Copy))
                        .push(fire_row(cx, "ctx-md", "export", crate::settings::text("复制为 Markdown"), k::CopyMarkdown));
                }
                p = p.push(fire_row(cx, "ctx-all", "batch", crate::settings::text("全选"), k::SelectAll));
            }
            CtxKind::Caret | CtxKind::Title => {
                p = p
                    .push(fire_row(cx, "ctx-paste", "import", crate::settings::text("粘贴"), k::Paste))
                    .push(fire_row(cx, "ctx-paste-plain", "baseline", crate::settings::text("粘贴为纯文本"), k::PastePlain))
                    .push(fire_row(cx, "ctx-all", "batch", crate::settings::text("全选"), k::SelectAll))
                    .rule()
                    .push(sub_row(cx, "ctx-insert", "plus", crate::settings::text("插入"), CtxSub::Insert, sub == Some(CtxSub::Insert)));
                if kind == CtxKind::Caret {
                    p = p
                        .rule()
                        // 转换为 / 对齐 / the two block moves: what a caret can do
                        // to its own paragraph.
                        .push(sub_row(cx, "ctx-turn", "type", crate::settings::text("转换为"), CtxSub::Turn, sub == Some(CtxSub::Turn)))
                        .when(!self.markdown, |p| p.push(sub_row(cx, "ctx-align", "align-left", crate::settings::text("对齐"), CtxSub::Align, sub == Some(CtxSub::Align))))
                        .push(fire_row(cx, "ctx-up", "caret-up", crate::settings::text("上移块"), k::MoveBlockUp))
                        .push(fire_row(cx, "ctx-down", "caret-down", crate::settings::text("下移块"), k::MoveBlockDown));
                }
            }
            CtxKind::Cell { block, row, col } => {
                // 合并 needs the cell selection the user made: right-clicking
                // inside it leaves it alone, so it is still there to merge.
                // Markdown tables have no merged cells.
                let merge = !self.markdown && self.core.cell_range().is_some_and(|(t, r)| {
                    t == block && r.contains(row, col) && self.core.selected_cells().len() >= 2
                });
                let split = self
                    .core
                    .table(block)
                    .and_then(|t| t.rows.get(row).and_then(|r| r.get(col)))
                    .is_some_and(|c| !self.markdown && (c.colspan > 1 || c.rowspan > 1));
                p = p
                    .push(act_row(cx, "ctx-row-above", "row-add", crate::settings::text("上方插入行"), move |e, window, cx| {
                        e.core.table_insert(block, false, row);
                        e.changed(window, cx);
                    }))
                    .push(act_row(cx, "ctx-row-below", "row-add", crate::settings::text("下方插入行"), move |e, window, cx| {
                        e.core.table_insert(block, false, row + 1);
                        e.changed(window, cx);
                    }))
                    .push(act_row(cx, "ctx-col-left", "column-add", crate::settings::text("左侧插入列"), move |e, window, cx| {
                        e.core.table_insert(block, true, col);
                        e.changed(window, cx);
                    }))
                    .push(act_row(cx, "ctx-col-right", "column-add", crate::settings::text("右侧插入列"), move |e, window, cx| {
                        e.core.table_insert(block, true, col + 1);
                        e.changed(window, cx);
                    }))
                    .rule();
                if merge {
                    p = p.push(act_row(cx, "ctx-merge", "merge-cells", crate::settings::text("合并单元格"), move |e, window, cx| {
                        e.core.table_merge();
                        e.changed(window, cx);
                    }));
                }
                if split {
                    p = p.push(act_row(cx, "ctx-split", "split-cells", crate::settings::text("拆分单元格"), move |e, window, cx| {
                        let s = CellSel { table: block, anchor: (row, col), head: (row, col), kind: CellSelKind::Range };
                        e.core.select_cells(s);
                        e.core.table_unmerge();
                        e.changed(window, cx);
                    }));
                }
                p = p.push(act_row(cx, "ctx-clear", "minus", crate::settings::text("清空内容"), move |e, window, cx| {
                    // The core clears the cell selection, so select this one
                    // cell, clear it and put the old selection back.
                    let (sel, cells, blocks) = (e.core.sel, e.core.cell_sel, e.core.block_sel);
                    let s = CellSel { table: block, anchor: (row, col), head: (row, col), kind: CellSelKind::Range };
                    e.core.select_cells(s);
                    e.core.table_clear_cells();
                    (e.core.sel, e.core.cell_sel, e.core.block_sel) = (sel, cells, blocks);
                    e.changed(window, cx);
                }));
                p = p
                    .rule()
                    .push(danger_act(cx, "ctx-del-row", "trash-2", crate::settings::text("删除行"), move |e, window, cx| {
                        let s = CellSel { table: block, anchor: (row, 0), head: (row, 0), kind: CellSelKind::Rows };
                        e.core.select_cells(s);
                        e.core.table_delete_selected();
                        e.changed(window, cx);
                    }))
                    .push(danger_act(cx, "ctx-del-col", "trash-2", crate::settings::text("删除列"), move |e, window, cx| {
                        let s = CellSel { table: block, anchor: (0, col), head: (0, col), kind: CellSelKind::Cols };
                        e.core.select_cells(s);
                        e.core.table_delete_selected();
                        e.changed(window, cx);
                    }))
                    .push(danger_act(cx, "ctx-del-table", "trash-2", crate::settings::text("删除表格"), move |e, window, cx| {
                        e.block_menu_action(block, BlockAct::Delete, window, cx);
                    }));
            }
        }

        // The open submenu, hung beside the row that opened it.
        let sub_el: Option<AnyElement> = match (kind, sub) {
            (CtxKind::Caret | CtxKind::Title, Some(CtxSub::Insert)) => Some(
                sub_menu("ctx-insert-sub", insert_y)
                    .child(fire_row(cx, "ctx-ins-link", "link", crate::settings::text("链接"), k::Link))
                    .child(fire_row(cx, "ctx-ins-table", "table", crate::settings::text("表格"), k::InsertTable))
                    .child(fire_row(cx, "ctx-ins-divider", "divider", crate::settings::text("分隔线"), k::InsertDivider))
                    // A Markdown file can't keep a reminder.
                    .when(!self.markdown, |m| m.child(fire_row(cx, "ctx-ins-reminder", "reminder", crate::settings::text("提醒"), k::InsertReminder)))
                    .into_any_element(),
            ),
            (CtxKind::Caret, Some(CtxSub::Turn)) => {
                // 正文 through 标题 3, lists, 引用, 代码块: 标题 4–9 and 高亮块
                // stay in the handle menu, which can hold a whole grid.
                let mut m = sub_menu("ctx-turn-sub", turn_y);
                for (i, it) in turn_items() {
                    let kind = (it.kind)();
                    m = m.child(act_row(cx, ("ctx-turn", i), it.icon, crate::settings::text(it.label), move |e, window, cx| {
                        e.turn(kind.clone(), false, window, cx);
                    }));
                }
                Some(m.into_any_element())
            }
            (CtxKind::Caret, Some(CtxSub::Align)) => Some(
                sub_menu("ctx-align-sub", align_y)
                    .child(fire_row(cx, "ctx-al-left", "align-left", crate::settings::text("左对齐"), k::AlignLeft))
                    .child(fire_row(cx, "ctx-al-center", "align-center", crate::settings::text("居中对齐"), k::AlignCenter))
                    .child(fire_row(cx, "ctx-al-right", "align-right", crate::settings::text("右对齐"), k::AlignRight))
                    .into_any_element(),
            ),
            _ => None,
        };

        let el = div()
            .relative()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(p.div)
            .children(sub_el);
        Some(at(pos, Anchor::TopLeft, el))
    }
}

#[cfg(test)]
mod tests {
    use super::{BlockKind, turn_items};

    #[test]
    fn turn_into_leaves_deep_headings_and_callouts_to_the_handle_menu() {
        let kinds: Vec<BlockKind> = turn_items().map(|(_, t)| (t.kind)()).collect();
        assert!(kinds.iter().any(|k| matches!(k, BlockKind::Heading { level: 3, .. })));
        assert!(!kinds.iter().any(|k| matches!(k, BlockKind::Heading { level: 4.., .. } | BlockKind::Callout { .. })));
        assert!(kinds.iter().any(|k| matches!(k, BlockKind::Code { .. })) && kinds.contains(&BlockKind::Quote));
    }
}
