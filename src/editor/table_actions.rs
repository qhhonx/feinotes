//! Table commands shared by the desktop toolbar and touch controls.
use super::view::Editor;
use crate::model::{BlockId, ColorName};
use gpui::*;

pub(super) enum TableAction {
    Insert { table: BlockId, cols: bool, at: usize },
    Merge,
    Unmerge,
    DeleteSelected,
    Background(Option<ColorName>),
    Vertical(Option<&'static str>),
    #[cfg(target_os = "ios")]
    Width { table: BlockId, col: usize, width: f32 },
    #[cfg(target_os = "ios")]
    Move(usize),
}

impl Editor {
    pub(super) fn table_action(&mut self, action: TableAction, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only { return; }
        let preserve = matches!(&action,TableAction::Background(_) | TableAction::Vertical(_));
        self.menu = None;
        match action {
            TableAction::Insert { table,cols,at } => {
                self.core.block_sel = None;
                self.core.table_insert(table,cols,at);
                self.tables.insert = None;
            }
            TableAction::Merge => self.core.table_merge(),
            TableAction::Unmerge => self.core.table_unmerge(),
            TableAction::DeleteSelected => self.core.table_delete_selected(),
            TableAction::Background(value) => self.core.table_set_bg(value),
            TableAction::Vertical(value) => self.core.table_set_valign(value),
            #[cfg(target_os = "ios")]
            TableAction::Width { table,col,width } => { self.core.table_set_width(table,col,width); }
            #[cfg(target_os = "ios")]
            TableAction::Move(to) => { self.core.table_move_selected(to); }
        }
        crate::gpui_compat::focus(window,&self.focus,cx);
        if preserve { self.changed_preserving_viewport(window,cx); } else { self.changed(window,cx); }
    }
}
