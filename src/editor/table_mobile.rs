//! Native controls dispatch the same table operations as the desktop editor.
use super::{format_mobile::{SelectionStamp,group,palette,submenu}, table_ops::{CellSel, CellSelKind}, table_actions::TableAction, view::{BlockAct, Editor}};
use crate::{model::{BlockId, ColorName}, settings::tr};
use gpui::*;
pub(super) struct MenuStamp { stamp: SelectionStamp, table: BlockId }

impl Editor {
    #[cfg(feature="mobile-qa")]
    pub(crate) fn mobile_scroll_probe(&self) -> String {
        let top=self.list.logical_scroll_top();
        let tables=self.tables.hscroll.borrow().iter().map(|(id,(scroll,_))| {
            let rect=scroll.bounds();
            serde_json::json!({"id":id.0,"x":f32::from(scroll.offset().x),"rect":[f32::from(rect.origin.x),f32::from(rect.origin.y),f32::from(rect.size.width),f32::from(rect.size.height)]})
        }).collect::<Vec<_>>();
        let bounds=self.block_bounds.borrow();
        let mut blocks=vec![];
        self.core.doc.for_each_block(|b| { if let Some(rect)=bounds.get(&b.id) { blocks.push(serde_json::json!({"kind":format!("{:?}",b.kind),"text":b.text.text.chars().take(80).collect::<String>(),"rect":[f32::from(rect.origin.x),f32::from(rect.origin.y),f32::from(rect.size.width),f32::from(rect.size.height)]})); } });
        serde_json::json!({"page":[top.item_ix as f32,f32::from(top.offset_in_item)],"tables":tables,"blocks":blocks,"catalog":matches!(self.menu,Some(super::view::Menu::Add { .. })) || self.slash.is_some(),"catalog_items":self.slash_results().len(),"catalog_labels":self.slash_results().iter().map(|i|crate::settings::text(super::menus::SLASH_ITEMS[*i].label)).collect::<Vec<_>>(),"body_size":crate::editor_theme::body_size(),"code_size":crate::editor_theme::code_size(),"code_line":crate::editor_theme::code_line()}).to_string()
    }

    pub(crate) fn mobile_table_target(&self) -> Option<(BlockId,usize,usize)> {
        if let Some(s) = self.core.cell_sel { return self.core.table(s.table).map(|_| (s.table,s.anchor.0,s.anchor.1)); }
        let caret = self.core.cell_of(self.core.sel.head.block);
        if let Some(s) = self.core.block_sel {
            if s.anchor != s.head || self.core.table(s.head).is_none() { return None; }
            return Some(caret.filter(|(id,_,_)|*id == s.head).unwrap_or((s.head,0,0)));
        }
        caret
    }

    pub(super) fn invalidate_table_controls(&mut self) {
        self.table_menu = None;
        self.format_menu = None;
        unsafe { fei_ios_dismiss_table_controls(); }
        super::media_mobile::dismiss_link_form();
        super::object_mobile::dismiss_form();
    }

    pub(crate) fn present_table_controls(&mut self, cx: &mut Context<Self>) {
        let Some((id,row,col)) = self.mobile_table_target() else { self.invalidate_table_controls(); return };
        let Some(table) = self.core.table(id) else { return };
        if table.rows.is_empty() || table.ncols() == 0 { return; }
        let stamp = SelectionStamp::capture(self,"table");
        let serial = stamp.serial;
        self.table_menu = Some(MenuStamp { stamp,table: id });
        self.format_menu = None;
        let mut sections = Vec::new();
        let mut section = |title: &str, actions: Vec<(&str,&str,bool,bool)>| {
            sections.push(serde_json::json!({"title": title,"actions": actions.into_iter().map(|(key,title,enabled,checked)|
                serde_json::json!({"key": key,"title": title,"enabled": enabled,"checked": checked,"destructive": key.starts_with("delete-")})).collect::<Vec<_>>() }));
        };
        section(tr("选择","Selection"),vec![
            ("select-cell",tr("选择单元格","Select cell"),true,false),
            ("select-row",tr("选择当前行","Select row"),true,self.core.cell_sel.is_some_and(|s| s.kind == CellSelKind::Rows && s.anchor.0 == s.head.0)),
            ("select-column",tr("选择当前列","Select column"),true,self.core.cell_sel.is_some_and(|s| s.kind == CellSelKind::Cols && s.anchor.1 == s.head.1)),
            ("select-table",tr("选择整个表格","Select entire table"),true,self.core.cell_range().is_some_and(|(_,r)|r.top == 0 && r.left == 0 && r.bottom+1 == table.rows.len() && r.right+1 == table.ncols())),
            ("extend-right",tr("向右扩选单元格","Extend selection right"),self.core.cell_sel.map_or(col,|s| s.head.1)+1 < table.ncols(),false),
            ("extend-down",tr("向下扩选单元格","Extend selection down"),self.core.cell_sel.map_or(row,|s| s.head.0)+1 < table.rows.len(),false),
        ]);
        let editable = !self.read_only && crate::cloud_sync::shared::can_edit_in(&self.library_root,cx);
        if editable {
            section(tr("插入","Insert"),vec![
                ("row-before",tr("在上方插入行","Insert row above"),true,false),("row-after",tr("在下方插入行","Insert row below"),true,false),
                ("column-before",tr("在左侧插入列","Insert column left"),true,false),("column-after",tr("在右侧插入列","Insert column right"),true,false),
            ]);
            let cell = self.core.first_selected_cell().or_else(|| table.rows.get(row).and_then(|r| r.get(col)));
            let merged = cell.is_some_and(|c| c.colspan > 1 || c.rowspan > 1) && self.core.selected_cells().len() <= 1;
            section(tr("单元格","Cells"),vec![
                ("merge",tr("合并单元格","Merge cells"),self.core.selected_cells().len() >= 2,false),
                ("split",tr("拆分单元格","Split cell"),merged,false),
                ("vertical-top",tr("顶部对齐","Align top"),true,cell.is_some_and(|c| c.valign.is_none())),
                ("vertical-middle",tr("垂直居中","Align middle"),true,cell.is_some_and(|c| c.valign.as_deref() == Some("middle"))),
                ("vertical-bottom",tr("底部对齐","Align bottom"),true,cell.is_some_and(|c| c.valign.as_deref() == Some("bottom"))),
            ]);
            section(tr("表格样式","Table style"),vec![
                ("header-row",tr("标题行","Header row"),true,table.header_rows > 0),("header-column",tr("标题列","Header column"),true,table.header_col),
                ("distribute",tr("均分列宽","Distribute columns"),true,false),
                ("width-narrow",tr("当前列更窄","Narrow current column"),table.col_widths.get(col).is_some_and(|w| *w > crate::model::Table::MIN_WIDTH),false),
                ("width-wide",tr("当前列更宽","Widen current column"),true,false),
            ]);
            section(tr("排列","Arrange"),vec![
                ("row-up",tr("当前行上移","Move row up"),row > 0,false),("row-down",tr("当前行下移","Move row down"),row+1 < table.rows.len(),false),
                ("column-left",tr("当前列左移","Move column left"),col > 0,false),("column-right",tr("当前列右移","Move column right"),col+1 < table.ncols(),false),
            ]);
            section(tr("删除","Delete"),vec![
                ("delete-row",tr("删除选中行","Delete selected rows"),true,false),("delete-column",tr("删除选中列","Delete selected columns"),true,false),
                ("delete-table",tr("删除表格","Delete table"),true,false),
            ]);
            let background = cell.and_then(|c|c.bg.as_ref()).map(|c|crate::theme::bg_canon(&c.0));
            sections.insert(2,group("cell-appearance",tr("文字与颜色","Text and colors"),false,vec![
                submenu("cell-text",tr("文字样式","Text style"),"styles"),submenu("cell-paragraph",tr("段落样式","Paragraph style"),"paragraph"),
                submenu("background-palette",tr("单元格背景色","Cell background"),"cell-colors"),
            ]));
            sections.push(group("cell-colors",tr("单元格背景色","Cell background"),true,palette("background-color:",&crate::theme::CELL_BG_COLORS,background,crate::theme::bg_swatch)));
            sections.extend(self.native_format_sections("format:",true));
        }
        let request = serde_json::json!({"serial":serial,"title":tr("表格操作","Table controls"),"done":tr("完成","Done"),
            "context": format!("{} × {} · {} {}, {} {}",table.rows.len(),table.ncols(),tr("行","row"),row+1,tr("列","column"),col+1),"sections":sections});
        self.dismiss_touch_menu();
        if let Ok(json) = std::ffi::CString::new(request.to_string()) { unsafe { fei_ios_present_table_controls(json.as_ptr()); } }
    }

    pub(crate) fn mobile_table_action(&mut self, serial: u64, action: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(stamp) = self.table_menu.take() else { return };
        let valid = stamp.stamp.matches(self,serial,"table");
        let selection_only = matches!(action,"select-cell"|"select-row"|"select-column"|"select-table"|"extend-right"|"extend-down");
        let known = selection_only || matches!(action,"row-before"|"row-after"|"column-before"|"column-after"|"merge"|"split"|"delete-row"|"delete-column"|"delete-table"
            |"vertical-top"|"vertical-middle"|"vertical-bottom"|"width-narrow"|"width-wide"|"row-up"|"row-down"|"column-left"|"column-right"|"header-row"|"header-column"|"distribute")
            || action.starts_with("format:") || action.strip_prefix("background-color:").is_some_and(|v|crate::theme::CELL_BG_COLORS.contains(&if v == "none" { None } else { Some(v) }));
        if !valid || !known || (!selection_only && (self.read_only || !crate::cloud_sync::shared::can_edit_in(&self.library_root,cx))) { self.invalidate_table_controls(); return; }
        let Some((table,row,col)) = self.mobile_table_target().filter(|t| t.0 == stamp.table) else { self.invalidate_table_controls(); return };
        let Some(t) = self.core.table(table) else { return };
        let (nr,nc) = (t.rows.len(),t.ncols());
        let span = t.rows.get(row).and_then(|r| r.get(col)).map(|c| (c.rowspan as usize,c.colspan as usize)).unwrap_or((1,1));
        let width = t.col_widths.get(col).copied().unwrap_or(120.);
        let old = self.core.cell_sel;
        let cell = CellSel { table,anchor:(row,col),head:(row,col),kind:CellSelKind::Range };
        let mut select = cell;
        match action {
            "select-cell" => {},
            "select-row" | "delete-row" | "row-up" | "row-down" => { select = old.unwrap_or(cell); select.kind = CellSelKind::Rows; },
            "select-column" | "delete-column" | "column-left" | "column-right" => { select = old.unwrap_or(cell); select.kind = CellSelKind::Cols; },
            "select-table" | "delete-table" => { select.head = (nr-1,nc-1); select.kind = CellSelKind::Rows; },
            "extend-right" | "extend-down" => {
                select = old.unwrap_or(cell); select.kind = CellSelKind::Range;
                if action == "extend-right" { select.head.1 = (select.head.1+1).min(nc-1); }
                else { select.head.0 = (select.head.0+1).min(nr-1); }
            }
            _ => { select = old.unwrap_or(cell); }
        }
        self.core.select_cells(select);
        if let Some(name) = action.strip_prefix("format:") {
            if self.apply_native_format(name,false,window,cx) { self.present_table_controls(cx); } else { self.invalidate_table_controls(); }
            return;
        }
        let command = match action {
            "row-before" => Some(TableAction::Insert { table,cols:false,at:row }),
            "row-after" => Some(TableAction::Insert { table,cols:false,at:(row+span.0).min(nr) }),
            "column-before" => Some(TableAction::Insert { table,cols:true,at:col }),
            "column-after" => Some(TableAction::Insert { table,cols:true,at:(col+span.1).min(nc) }),
            "merge" => Some(TableAction::Merge), "split" => Some(TableAction::Unmerge),
            "delete-row" | "delete-column" | "delete-table" => Some(TableAction::DeleteSelected),
            "vertical-top" => Some(TableAction::Vertical(None)),"vertical-middle" => Some(TableAction::Vertical(Some("middle"))),"vertical-bottom" => Some(TableAction::Vertical(Some("bottom"))),
            value if value.starts_with("background-color:") => Some(TableAction::Background(value.strip_prefix("background-color:").filter(|v|*v != "none").map(|v|ColorName(v.into())))),
            "width-narrow" | "width-wide" => Some(TableAction::Width { table,col,width:width+if action == "width-narrow" { -32. } else { 32. } }),
            "row-up" | "column-left" => Some(TableAction::Move(if action == "row-up" { row.saturating_sub(1) } else { col.saturating_sub(1) })),
            "row-down" | "column-right" => { let r = select.raw(self.core.table(table).unwrap()); Some(TableAction::Move(if action == "row-down" { (r.bottom+2).min(nr) } else { (r.right+2).min(nc) })) },
            _ => None,
        };
        if let Some(command) = command { self.table_action(command,window,cx); }
        else if let Some(command) = match action { "header-row"=>Some(BlockAct::HeaderRow),"header-column"=>Some(BlockAct::HeaderCol),"distribute"=>Some(BlockAct::DistributeCols),_=>None } {
            self.block_menu_action(table,command,window,cx);
        } else { cx.notify(); }
        self.present_table_controls(cx);
    }
}

unsafe extern "C" {
    fn fei_ios_present_table_controls(json: *const std::ffi::c_char);
    fn fei_ios_dismiss_table_controls();
}
