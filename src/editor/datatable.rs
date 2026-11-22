//! 数据表 block: the table drawn in the page itself (`crate::table::TableView`,
//! editable where it is, as tall as its rows), and 全屏 over
//! the whole note pane the way a 画板 opens. The block only names its file;
//! the view reads and writes it (`crate::table::BaseState`). A standalone
//! `.sqlite` file opens straight into the full view.

use super::render::R;
use super::view::{Editor, EditorEvent};
use crate::editor_theme::Theme;
use crate::model::{Block, BlockId, BlockKind};
use crate::table::{BaseState, TableEvent, TableRef, TableView};
use gpui::*;
use std::collections::HashMap;

#[derive(Default)]
pub struct DataTablesUi {
    /// A view per 数据表 block of the page.
    views: HashMap<BlockId, (Entity<TableView>, Subscription)>,
    /// The block shown over the whole pane.
    open: Option<BlockId>,
}

impl R<'_> {
    pub(super) fn datatable_block(&self, b: &Block, t: &TableRef, cx: &mut Context<Editor>) -> AnyElement {
        let id = b.id;
        let frame = || div().id(("datatable", id.0)).relative().w_full();
        // the drag ghost, and the frame before the view exists
        let Some(view) = self.ed.datatables.views.get(&id).map(|v| v.0.clone()).filter(|_| !self.ghost) else {
            return frame().h(px(t.height.min(160.))).border_1().border_color(Theme::line_border()).flex().items_center().justify_center()
                .text_size(px(13.)).text_color(Theme::text_placeholder()).child(crate::settings::label("数据表")).into_any_element();
        };
        let covered = self.ed.datatables.open == Some(id);
        // the view is as tall as its rows (`TableView::render`); while it is full-pane, a gap of its height stays
        let h = view.read(cx).height;
        frame()
            // clicks inside belong to the table, not to the page's caret
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .when(covered, |d| d.h(px(h)))
            .when(!covered, |d| d.child(view))
            .into_any_element()
    }
}

use gpui::prelude::FluentBuilder;

impl Editor {
    /// A view for every 数据表 block of the page; views of blocks that are gone are dropped.
    pub(super) fn sync_tables(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !crate::DATATABLE || self.read_only || self.preview { return; }
        let mut blocks: Vec<(BlockId, TableRef)> = vec![];
        self.core.doc.for_each_block(|b| if let BlockKind::DataTable(t) = &b.kind { blocks.push((b.id, t.clone())) });
        if blocks.is_empty() && self.datatables.views.is_empty() { return; }
        self.datatables.views.retain(|id, _| blocks.iter().any(|b| b.0 == *id));
        if self.datatables.open.is_some_and(|id| !self.datatables.views.contains_key(&id) && !blocks.iter().any(|b| b.0 == id)) { self.datatables.open = None; }
        let root = self.library_root.clone();
        for (id, t) in blocks {
            if self.datatables.views.contains_key(&id) { continue; }
            let state = BaseState::open(&root.join(&t.src), cx);
            let standalone = self.table_file;
            let view = cx.new(|cx| TableView::new(state, &t, standalone, standalone, window, cx));
            let sub = cx.subscribe_in(&view, window, move |e, view, ev: &TableEvent, window, cx| match ev {
                TableEvent::Expand => e.open_table(id, window, cx),
                TableEvent::Close => e.close_table(window, cx),
                // which table and view shows: the block remembers
                TableEvent::Shown => {
                    let Some(BlockKind::DataTable(old)) = e.core.block(id).map(|b| b.kind.clone()) else { return };
                    let (table, shown) = view.read(cx).shown(cx);
                    let new = TableRef { src: old.src.clone(), table, view: shown, height: view.read(cx).height };
                    if !e.table_file && e.core.set_table_ref(id, new) { e.changed_preserving_viewport(window, cx); }
                }
            });
            self.datatables.views.insert(id, (view, sub));
        }
    }

    /// A standalone 数据表: its one table fills the pane at once.
    pub fn open_table_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.table_file || self.datatables.open.is_some() { return }
        self.sync_tables(window, cx);
        if let Some(id) = self.core.doc.blocks.iter().find(|b| matches!(b.kind, BlockKind::DataTable(_))).map(|b| b.id) { self.open_table(id, window, cx); }
    }

    pub(super) fn open_table(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_tables(window, cx);
        let Some(view) = self.datatables.views.get(&id).map(|v| v.0.clone()) else { return };
        self.menu = None;
        self.slash = None;
        self.datatables.open = Some(id);
        view.update(cx, |v, cx| { v.full = true; v.focus(window, cx); cx.notify(); });
        cx.notify();
    }

    /// Leave the full view for the page (a standalone file is left altogether).
    fn close_table(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.datatables.open.take() else { return };
        if let Some(view) = self.datatables.views.get(&id).map(|v| v.0.clone()) { view.update(cx, |v, cx| { v.finish(window, cx); v.full = v.standalone; cx.notify(); }); }
        if self.table_file { cx.emit(EditorEvent::Leave); } else { window.focus(&self.focus, cx); }
        cx.notify();
    }

    /// The header's breadcrumb: back from a block's full view to its page.
    pub fn close_open_table(&mut self, window: &mut Window, cx: &mut Context<Self>) { if !self.table_file { self.close_table(window, cx); } }

    pub fn table_open(&self) -> bool { self.datatables.open.is_some() }

    /// The open table belongs to the page (not a standalone file).
    pub fn table_block_open(&self) -> bool { self.datatables.open.is_some() && !self.table_file }

    pub(super) fn table_el(&self) -> Option<AnyElement> {
        let view = self.datatables.views.get(&self.datatables.open?)?.0.clone();
        Some(view.into_any_element())
    }

    /// A table in the page holds the keyboard: the page's own shortcuts stand aside.
    pub(super) fn tables_focused(&self, window: &Window, cx: &App) -> bool {
        self.datatables.views.values().any(|v| v.0.focus_handle(cx).contains_focused(window, cx))
    }

    /// A standalone table's file moved (its page was renamed).
    pub fn table_file_at(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        if !self.table_file { return }
        for (view, _) in self.datatables.views.values() { let state = view.read(cx).state.clone(); state.update(cx, |s, cx| s.moved(path, cx)); }
    }

}
