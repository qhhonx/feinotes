//! 画板 block: a static thumbnail in the page (the scene fit into the
//! block, painted by `board::render`); 编辑 or a double click opens the
//! board in the document's place (`crate::board::BoardView`), under the
//! app's header, whose breadcrumb leads back to the document. Closing writes
//! a changed scene back as one document step (`Core::set_board`). A
//! standalone `.excalidraw` file opens straight into the board.

use super::render::R;
use super::view::{Editor, EditorEvent};
use crate::board::render::{Painter, canvas_color, paint_fit};
use crate::board::{BoardEvent, BoardView};
use crate::editor_theme::Theme;
use crate::ui::TipExt;
use crate::model::{Block, BlockId, BlockKind};
use crate::whiteboard::Board;
use gpui::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Default)]
pub struct BoardUi {
    /// The open board: its block and view.
    open: Option<(BlockId, Entity<BoardView>, Subscription)>,
    /// The open board has written a change back (later ones join its undo step).
    saved_once: bool,
    /// Thumbnail painters by block (their caches key on element versions).
    thumbs: RefCell<HashMap<BlockId, Rc<RefCell<Painter>>>>,
}

/// Room around the drawing in a thumbnail.
const PAD: f32 = 16.;

impl R<'_> {
    pub(super) fn board_block(&self, b: &Block, bd: &Arc<Board>, cx: &mut Context<Editor>) -> AnyElement {
        let id = b.id;
        let painter = self.ed.boards.thumbs.borrow_mut().entry(id).or_default().clone();
        let board = bd.clone();
        let empty = bd.elements.is_empty();
        let group = SharedString::from(format!("board-{}", id.0));
        // under the open board the thumbnail isn't seen: don't paint it every frame
        let covered = self.ed.boards.open.as_ref().is_some_and(|o| o.0 == id);
        let thumb = canvas(|_, _, _| {}, move |bounds, _, window, cx| if !covered { paint_fit(&mut painter.borrow_mut(), &board, bounds, PAD, window, cx) }).absolute().size_full();
        let open_btn = crate::design::Button::outline(("board-open", id.0))
            .sized(crate::design::ButtonSize::Tool)
            .absolute().top(px(10.)).right(px(10.))
            .invisible()
            .group_hover(group.clone(), |s| s.visible())
            .icon("whiteboard")
            .label(crate::settings::label("编辑"))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |e, _, window, cx| e.open_board(id, window, cx)));
        div()
            .id(("board", id.0))
            .group(group)
            .relative()
            .w_full()
            .h(px(bd.height))
            .rounded(px(8.))
            .border_1()
            .border_color(Theme::line_border())
            .bg(canvas_color(bd))
            .overflow_hidden()
            .child(thumb)
            .when(empty, |d| d.child(div().absolute().size_full().flex().items_center().justify_center()
                .text_size(px(13.)).text_color(Theme::text_placeholder()).child(crate::settings::label("空画板"))))
            .child(open_btn)
            .tip_above("双击打开画板", None)
            .on_click(cx.listener(move |e, ev: &ClickEvent, window, cx| {
                if ev.click_count() >= 2 { e.open_board(id, window, cx); }
            }))
            .into_any_element()
    }
}

use gpui::prelude::FluentBuilder;

impl Editor {
    pub(super) fn open_board(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(BlockKind::Board(bd)) = self.core.block(id).map(|b| b.kind.clone()) else { return };
        let view = cx.new(|cx| BoardView::new(bd, window, cx));
        #[cfg(target_family = "wasm")]
        view.update(cx, |view, _| view.export_name = crate::settings::display_title(&self.core.doc.title.text));
        let sub = cx.subscribe_in(&view, window, move |e, view, ev: &BoardEvent, window, cx| match ev {
            BoardEvent::Close => {
                let board = view.read(cx).board();
                e.close_board(Some(board), window, cx);
            }
            // saved as it's edited: the block (one undo step per session) and the file
            BoardEvent::Changed => {
                let board = view.read(cx).board();
                let Some(id) = e.boards.open.as_ref().map(|o| o.0) else { return };
                if e.core.set_board(id, Arc::unwrap_or_clone(board), e.boards.saved_once) {
                    e.boards.saved_once = true;
                    e.changed_preserving_viewport(window, cx);
                }
            }
            // the window's toast (the document's isn't drawn over the board),
            // once this editor is no longer being updated (the workspace clears its toast)
            BoardEvent::Toast(msg) => {
                let (ws, msg) = (window.root::<crate::app::Workspace>().flatten(), msg.clone());
                cx.defer(move |cx| { if let Some(ws) = ws { ws.update(cx, |ws, cx| ws.show_toast_kind(crate::editor_ui::ToastKind::Info, &msg, cx)); } });
            }
        });
        self.boards.saved_once = false;
        self.menu = None;
        self.slash = None;
        view.update(cx, |v, cx| v.focus(window, cx));
        self.boards.open = Some((id, view, sub));
        cx.notify();
    }

    /// A standalone 画板: open its board at once (the file holds just the one).
    pub fn open_board_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.board_file || self.boards.open.is_some() { return }
        if let Some(id) = self.core.doc.blocks.iter().find(|b| matches!(b.kind, BlockKind::Board(_))).map(|b| b.id) {
            self.open_board(id, window, cx);
        }
    }

    /// Leaves the board, writing `board` back when it changed (one undo step);
    /// a standalone file is left altogether.
    pub(super) fn close_board(&mut self, board: Option<Arc<Board>>, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, ..)) = self.boards.open.take() else { return };
        if let Some(bd) = board && self.core.set_board(id, Arc::unwrap_or_clone(bd), self.boards.saved_once) {
            self.changed_preserving_viewport(window, cx);
        }
        if self.board_file {
            cx.emit(EditorEvent::Leave);
        } else {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    /// The open board is in the middle of something (a drag, a text): a
    /// change from outside waits.
    pub fn board_busy(&self, cx: &App) -> bool { self.boards.open.as_ref().is_some_and(|o| o.1.read(cx).busy()) }

    /// The document was replaced (a change from outside, 重新载入, a sibling
    /// window's edit): the open board follows its block, keeping where it
    /// looks. `at`: the open board's place among the old document's boards,
    /// for when the block's id changed with the reading.
    pub(super) fn follow_open_board(&mut self, at: Option<usize>, cx: &mut Context<Self>) {
        let Some((id, view, _)) = &mut self.boards.open else { return };
        let boards = self.core.units().iter().filter_map(|u| match self.core.block(u.id).map(|b| &b.kind) { Some(BlockKind::Board(bd)) => Some((u.id, bd.clone())), _ => None }).collect::<Vec<_>>();
        let found = boards.iter().find(|b| b.0 == *id).or_else(|| at.and_then(|i| boards.get(i))).cloned();
        match found {
            Some((now, bd)) => { *id = now; view.update(cx, |v, cx| v.reload(&bd, cx)); }
            // its block is gone: back to the document (a standalone file always has its board)
            None => self.boards.open = None,
        }
    }

    /// The open board's place among the document's boards.
    pub(super) fn open_board_at(&self) -> Option<usize> {
        let (id, ..) = self.boards.open.as_ref()?;
        self.core.units().iter().filter(|u| matches!(self.core.block(u.id).map(|b| &b.kind), Some(BlockKind::Board(_)))).position(|u| u.id == *id)
    }

    /// A board is open (in place of the document).
    pub fn board_open(&self) -> bool { self.boards.open.is_some() }

    /// The open board's view (the harness drops files on it).
    #[cfg(feature = "devtools")]
    pub fn board_view(&self) -> Option<Entity<BoardView>> { self.boards.open.as_ref().map(|o| o.1.clone()) }

    /// The open board, filling the note pane under the app's header.
    pub(super) fn board_el(&self) -> Option<AnyElement> {
        let (_, view, _) = self.boards.open.as_ref()?;
        Some(view.clone().into_any_element())
    }

    /// Leaves the open board for the document it's in (the header's
    /// breadcrumb); a standalone file has nothing to go back to.
    pub fn close_open_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.board_file { return }
        let Some((_, view, _)) = self.boards.open.as_ref() else { return };
        let view = view.clone();
        view.update(cx, |v, cx| v.finish_pending(window, cx));
        let board = view.read(cx).board();
        self.close_board(Some(board), window, cx);
    }

    /// The open board belongs to the document (not a standalone file).
    pub fn board_block_open(&self) -> bool { self.boards.open.is_some() && !self.board_file }
}
