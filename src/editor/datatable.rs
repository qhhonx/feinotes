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
    /// A side being dragged: the block, where the press was, the width then, which side (+1 right, −1 left), the width now.
    resize: Option<(BlockId, f32, f32, f32, f32)>,
    /// The page was swapped under a standalone file's full view (版本历史): open its table again.
    reopen: bool,
}

/// The narrowest a table in a page can be dragged.
const MIN_W: f32 = 320.;

impl R<'_> {
    pub(super) fn datatable_block(&self, b: &Block, t: &TableRef, cx: &mut Context<Editor>) -> AnyElement {
        let id = b.id;
        let frame = || div().id(("datatable", id.0)).relative().w_full();
        // the drag ghost, and the frame before the view exists
        let Some(view) = self.ed.datatables.views.get(&id).map(|v| v.0.clone()).filter(|_| !self.ghost) else {
            return frame().h(px(t.height.min(160.))).border_1().border_color(Theme::line_border()).flex().items_center().justify_center()
                .text_size(px(13.)).text_color(Theme::text_placeholder())
                .child(crate::settings::label("数据表")).into_any_element();
        };
        let covered = self.ed.datatables.open == Some(id);
        // the view is as tall as its rows (`TableView::render`); while it is full-pane, a gap of its height stays
        let h = view.read(cx).height;
        // as wide as the text column, or dragged wider (into the side margins, evenly) or narrower, centred on it
        let content = self.ed.content_width();
        let (bl, br) = self.ed.bleed();
        let max = content + 2. * bl.min(br);
        let dragging = self.ed.datatables.resize.filter(|r| r.0 == id).map(|r| r.4);
        let w = dragging.or(t.width).map(|w| w.clamp(MIN_W.min(content), max.max(content))).unwrap_or(content);
        // a side to drag: a bar shows under the pointer and while dragged
        let side = |dir: f32| { let g = if dir < 0. { "dt-side-l" } else { "dt-side-r" }; div().id(("datatable-side", (id.0 as usize) * 2 + (dir > 0.) as usize)).absolute().top_0().bottom_0().w(px(12.)).flex().justify_center()
            .when(dir < 0., |d| d.left(px(-14.))).when(dir > 0., |d| d.right(px(-14.)))
            .group(g).cursor(CursorStyle::ResizeLeftRight)
            .child(div().w(px(3.)).h_full().rounded_full().when(dragging.is_some(), |d| d.bg(Theme::primary())).when(dragging.is_none(), |d| d.group_hover(g, |s| s.bg(Theme::primary()))))
            .on_mouse_down(MouseButton::Left, cx.listener(move |e, ev: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                // a double click puts it back to the text column's width
                if ev.click_count >= 2 { e.set_table_width(id, None, window, cx); return; }
                e.datatables.resize = Some((id, f32::from(ev.position.x), w, dir, w));
                cx.notify();
            })) };
        frame()
            .w(px(w)).ml(px((content - w) / 2.))
            // clicks inside belong to the table, not to the page's caret
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .when(covered, |d| d.h(px(h)))
            .when(!covered, |d| d.child(view).child(side(-1.)).child(side(1.)))
            // the drag follows the pointer anywhere in the window and ends on release
            .when(dragging.is_some(), |d| {
                let (a, b) = (cx.weak_entity(), cx.weak_entity());
                d.child(canvas(|_, _, _| {}, move |_, _, window, _| {
                    window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| if phase == DispatchPhase::Capture { let _ = a.update(cx, |e, cx| e.drag_table_width(f32::from(ev.position.x), (MIN_W.min(content), max.max(content)), cx)); });
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| if phase == DispatchPhase::Capture { let _ = b.update(cx, |e, cx| e.end_table_width(content, window, cx)); });
                }).absolute().size_full())
            })
            .into_any_element()
    }
}

use gpui::prelude::FluentBuilder;

impl Editor {
    /// A view for every 数据表 block of the page; views of blocks that are gone are dropped.
    pub(super) fn sync_tables(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // a build without 数据表: no view ever opens its file; the block stays a placeholder
        let mut blocks: Vec<(BlockId, TableRef)> = vec![];
        self.core.doc.for_each_block(|b| if let BlockKind::DataTable(t) = &b.kind { blocks.push((b.id, t.clone())) });
        if blocks.is_empty() && self.datatables.views.is_empty() { return; }
        self.datatables.views.retain(|id, _| blocks.iter().any(|b| b.0 == *id));
        let lost = self.datatables.open.is_some_and(|id| !self.datatables.views.contains_key(&id) && !blocks.iter().any(|b| b.0 == id)) || std::mem::take(&mut self.datatables.reopen);
        if lost { self.datatables.open = None; }
        // a standalone file's page was replaced (a version previewed, or back): its table fills the pane again
        if lost && self.table_file && let Some(&(id, _)) = blocks.first() { cx.defer_in(window, move |e, window, cx| e.open_table(id, window, cx)); }
        let root = crate::store::Store::active_dir();
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
                    let new = TableRef { src: old.src.clone(), table, view: shown, height: view.read(cx).height, width: old.width };
                    if !e.table_file && e.core.set_table_ref(id, new) { e.changed_preserving_viewport(window, cx); }
                }
                // 笔记 cells: the workspace opens, lists and keeps the notes
                TableEvent::OpenNote(doc) => cx.emit(EditorEvent::OpenDoc(doc.clone())),
                TableEvent::LibraryChanged => cx.emit(EditorEvent::LibraryChanged),
                TableEvent::NoteSaved(n) => cx.emit(EditorEvent::NoteSaved(n.clone())),
            });
            self.datatables.views.insert(id, (view, sub));
        }
    }

    fn drag_table_width(&mut self, x: f32, (lo, hi): (f32, f32), cx: &mut Context<Self>) {
        let Some(r) = self.datatables.resize.as_mut() else { return };
        // both sides move: the table stays centred on the text column
        r.4 = (r.2 + 2. * r.3 * (x - r.1)).clamp(lo, hi);
        cx.notify();
    }

    fn end_table_width(&mut self, content: f32, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, _, w0, _, w)) = self.datatables.resize.take() else { return };
        if (w - w0).abs() < 1. { cx.notify(); return; }
        // back within a few points of the column: the column's width again
        self.set_table_width(id, ((w - content).abs() > 4.).then_some(w.round()), window, cx);
    }

    /// A 数据表 block's width (None: the text column's), as one undo step.
    fn set_table_width(&mut self, id: BlockId, width: Option<f32>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(BlockKind::DataTable(old)) = self.core.block(id).map(|b| b.kind.clone()) else { return };
        if self.core.set_table_ref(id, TableRef { width, ..old }) { self.changed(window, cx); }
        cx.notify();
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

    /// Keep what the page's tables have in progress (a cell being edited): the page is being saved or left.
    pub fn commit_tables(&mut self, cx: &mut Context<Self>) {
        for (view, _) in self.datatables.views.values() { view.update(cx, |v, cx| v.commit_pending(cx)); }
    }

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

    /// The page is about to be replaced by another copy of itself (a version previewed, or back to
    /// now): its views go, so the next ones read the file the new page points at.
    pub fn reset_tables(&mut self) {
        self.datatables.views.clear();
        if self.datatables.open.take().is_some() { self.datatables.reopen = true; }
    }

    /// A standalone table's file moved (its page was renamed).
    pub fn table_file_at(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        if !self.table_file { return }
        for (view, _) in self.datatables.views.values() { let state = view.read(cx).state.clone(); state.update(cx, |s, cx| s.moved(path, cx)); }
    }

}
