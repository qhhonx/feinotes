//! Native actions for the focused or selected block, shared by the toolbar and grip.
use super::{core::{self, DropPlace}, format_mobile::{SelectionStamp, control, group, submenu},
    menus::MenuShape, view::{BlockAct, Editor}};
use crate::{model::{Align, BlockId, BlockKind, ColorName}, settings::tr, theme};
use gpui::*;
use serde_json::json;

impl Editor {
    pub(crate) fn mobile_block_target(&self) -> Option<(BlockId,bool)> {
        if self.read_only || self.core.cell_sel.is_some() { return None; }
        let (id,selected) = match self.core.block_sel {
            Some(selection) if selection.anchor == selection.head => (selection.head,true),
            None if self.core.sel.is_collapsed() => (self.core.sel.head.block,false),
            _ => return None,
        };
        self.mobile_block_kind(id)?;
        Some((id,selected))
    }

    pub(crate) fn mobile_block_toolbar_target(&self) -> Option<(BlockId,bool)> {
        if self.read_only { return None; }
        self.mobile_block_target().or_else(|| self.mobile_table_block_from_cell().map(|id|(id,false)))
    }

    pub(super) fn mobile_table_block_from_cell(&self) -> Option<BlockId> {
        if self.core.in_block_mode() { return None; }
        let (table,_,_) = self.mobile_table_target()?;
        // A text range spanning outside the table has no single table target.
        if self.core.cell_sel.is_none() && self.core.cell_of(self.core.sel.anchor.block).map(|(id,_,_)|id) != Some(table) {
            return None;
        }
        self.mobile_block_kind(table)?;
        Some(table)
    }

    pub(super) fn mobile_block_kind(&self, id: BlockId) -> Option<&BlockKind> {
        self.core.kind_of(id).filter(|kind| {
            id != core::TITLE && !matches!(kind,BlockKind::Grid | BlockKind::Column { .. })
                && (matches!(kind,BlockKind::Table(_)) || self.core.cell_of(id).is_none())
        })
    }

    fn block_move_neighbor(&self, id: BlockId, down: bool) -> Option<BlockId> {
        let ids = self.core.block_sel_ids();
        let edge = if down { ids.last() } else { ids.first() }.copied().unwrap_or(id);
        let path = self.core.path(edge)?;
        let (siblings,index) = core::siblings_at(&self.core.doc.blocks,&path)?;
        let visible: std::collections::HashSet<_> = self.core.block_order(true).into_iter().collect();
        if down {
            let following = siblings.get(index + 1..)?;
            let next = following.iter().position(|b|visible.contains(&b.id))?;
            // Move past the entire next visible block, including a folded heading's section.
            let section_end = following[next+1..].iter().take_while(|b|!visible.contains(&b.id)).last();
            Some(section_end.unwrap_or(&following[next]).id)
        } else {
            siblings[..index].iter().rev().find(|b|visible.contains(&b.id)).map(|b|b.id)
        }
    }

    pub(crate) fn present_block_controls(&mut self, id: BlockId, cx: &mut Context<Self>) {
        if self.read_only || self.marked.is_some() || !crate::cloud_sync::shared::can_edit_in(&self.library_root,cx) { return; }
        let Some(kind) = self.mobile_block_kind(id) else { return };
        let shape = MenuShape::of(kind);
        let table = matches!(kind,BlockKind::Table(_));
        // A sheet opened from a cell must leave the real selection available
        // for cancellation, keyboard restoration, and the cell tools.
        if !table || self.mobile_table_block_from_cell() != Some(id) {
            self.core.select_block(id);
        }
        self.menu = None;
        self.slash = None;
        self.table_menu = None;
        self.dismiss_touch_menu();
        let action = |key: &str,title: &str,enabled: bool| {
            let mut row = control(key,title,false);
            row["enabled"] = json!(enabled);
            row["dismiss"] = json!(true);
            row
        };
        let turn_title = tr("转换类型","Convert block");
        let turn: Vec<_> = shape.turn_items().into_iter().filter(|item| !self.markdown || item.label != "高亮块")
            .map(|item|action(&format!("turn:{}",item.label),crate::settings::text(item.label),true)).collect();
        let mut edit = Vec::new();
        if !turn.is_empty() { edit.push(submenu("convert",turn_title,"types")); }
        let styled = shape == MenuShape::Text && !self.markdown;
        if styled {
            edit.push(submenu("align",tr("对齐","Alignment"),"alignment"));
            edit.push(submenu("colors",tr("颜色","Color"),"colors"));
        }
        if table { edit.push(action("table",tr("表格操作","Table controls"),true)); }
        edit.push(action("add-below",tr("在下方添加","Add below"),true));
        let mut delete = action("delete",if table { tr("删除整张表格","Delete entire table") } else { tr("删除块","Delete block") },true);
        delete["destructive"] = json!(true);
        let align = self.core.block(id).map(|b|b.align).unwrap_or(Align::Left);
        let alignment = [("left",tr("左对齐","Align left"),Align::Left),("center",tr("居中对齐","Align center"),Align::Center),
            ("right",tr("右对齐","Align right"),Align::Right)].into_iter().map(|(key,title,value)| {
            let mut row = action(key,title,true);
            row["checked"] = json!(align == value);
            row
        }).collect();
        let block_bg = self.core.block(id).and_then(|b|b.bg.as_ref().map(|color|theme::bg_canon(&color.0)));
        let mut colors = super::format_mobile::palette("block-color:",&theme::BG_COLORS,block_bg,theme::block_bg);
        for row in &mut colors { row["dismiss"] = json!(true); }
        let stamp = SelectionStamp::capture(self,"block");
        let request = json!({"serial":stamp.serial,"namespace":"block","command":"block-action",
            "title":if table { tr("整表块操作","Table block actions") } else { tr("块操作","Block actions") },"done":tr("完成","Done"),"context":"",
            "sections":[group("edit","",false,edit),
                group("position",tr("位置","Position"),false,vec![
                    action("up",tr("上移","Move up"),self.block_move_neighbor(id,false).is_some()),
                    action("down",tr("下移","Move down"),self.block_move_neighbor(id,true).is_some()),
                    action("indent",tr("增加缩进","Indent"),self.core.can_indent(id)),
                    action("outdent",tr("减少缩进","Outdent"),self.core.can_outdent(id))]),
                group("clipboard","",false,vec![
                    action("cut",if table { tr("剪切整张表格","Cut entire table") } else { tr("剪切块","Cut block") },true),
                    action("copy",if table { tr("复制整张表格","Copy entire table") } else { tr("复制块","Copy block") },true),delete]),
                group("types",turn_title,true,turn),group("alignment",tr("对齐","Alignment"),true,alignment),
                group("colors",tr("块背景色","Block background color"),true,colors)]});
        self.format_menu = Some(stamp);
        super::format_mobile::show_format_request(&request);
        cx.notify();
    }

    pub(crate) fn mobile_block_action(&mut self, serial: u64, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.format_menu.as_ref().is_some_and(|stamp|stamp.matches(self,serial,"block"))
            || !crate::cloud_sync::shared::can_edit_in(&self.library_root,cx) { return; }
        self.format_menu = None;
        let Some((id,selected)) = self.mobile_block_toolbar_target() else { return };
        if !selected && self.mobile_table_block_from_cell() != Some(id) { return; }
        let action = match name {
            "cut" => BlockAct::Cut, "copy" => BlockAct::Copy, "delete" => BlockAct::Delete,
            "indent" if self.core.can_indent(id) => BlockAct::Indent,
            "outdent" if self.core.can_outdent(id) => BlockAct::Outdent,
            "left" => BlockAct::Align(Align::Left), "center" => BlockAct::Align(Align::Center), "right" => BlockAct::Align(Align::Right),
            "up" | "down" => {
                let down = name == "down";
                let Some(target) = self.block_move_neighbor(id,down) else { return };
                let ids = if selected { self.core.block_sel_ids() } else { vec![id] };
                if self.core.move_blocks(&ids,target,if down { DropPlace::After } else { DropPlace::Before }) {
                    self.changed(window,cx);
                }
                return;
            }
            "add-below" => {
                self.present_insert_controls_below(cx);
                return;
            }
            "table" if matches!(self.core.kind_of(id),Some(BlockKind::Table(_))) => { self.present_table_controls(cx); return; }
            name if name.starts_with("block-color:") => {
                let Some(color) = theme::BG_COLORS.iter().copied().find(|c|Some(c.unwrap_or("none")) == name.strip_prefix("block-color:")) else { return };
                if !self.markdown && self.mobile_block_kind(id).is_some_and(|kind|MenuShape::of(kind) == MenuShape::Text) {
                    self.set_block_bg(id,color.map(|c|ColorName(c.into())),window,cx);
                }
                return;
            }
            _ => {
                let Some(kind) = self.mobile_block_kind(id) else { return };
                let Some(item) = MenuShape::of(kind).turn_items().into_iter().find(|item|Some(item.label)==name.strip_prefix("turn:")) else { return };
                if self.markdown && item.label == "高亮块" { return; }
                BlockAct::Turn((item.kind)())
            }
        };
        if !selected && matches!(action,BlockAct::Copy | BlockAct::Indent | BlockAct::Outdent) {
            let cells = self.core.cell_sel;
            let copied = matches!(action,BlockAct::Copy);
            self.apply_block_menu_action(id,action,cx);
            self.core.clear_block_sel();
            self.core.cell_sel = cells;
            if copied { cx.notify(); } else { self.changed(window,cx); }
        } else {
            self.block_menu_action(id,action,window,cx);
        }
    }
}
