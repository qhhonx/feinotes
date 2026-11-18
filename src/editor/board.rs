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
        painter.borrow_mut().library_root = Some(self.ed.library_root.clone());
        let board = bd.clone();
        let empty = bd.elements.is_empty();
        let mobile = cfg!(target_os = "ios");
        let group = SharedString::from(format!("board-{}", id.0));
        // under the open board the thumbnail isn't seen: don't paint it every frame
        let covered = self.ed.boards.open.as_ref().is_some_and(|o| o.0 == id);
        let thumb = canvas(|_, _, _| {}, move |bounds, _, window, cx| if !covered { paint_fit(&mut painter.borrow_mut(), &board, bounds, PAD, window, cx) }).absolute().size_full();
        let chip = div()
            .h(px(if mobile { 24. } else { 28. })).px(px(if mobile { 6. } else { 10. }))
            .flex().items_center().gap(px(4.))
            .rounded(px(6.))
            .bg(crate::theme::raised(Theme::bg_body()))
            .border_1().border_color(Theme::line_border())
            .shadow(crate::theme::shadow_button())
            .text_size(px(13.)).text_color(Theme::text())
            .hover(|s| s.bg(Theme::fill_hover()))
            .child(crate::ui::icon("whiteboard", 14., Theme::icon()))
            .child(crate::settings::label("编辑"));
        // Keep the original touch area while giving the visible chip less padding.
        let open_btn = if mobile {
            div().h(px(crate::mobile::control(28.))).px(px(4.)).flex().items_center().child(chip)
        } else { chip };
        let open_btn = open_btn
            .id(("board-open", id.0))
            .absolute().top(px(10.)).right(px(10.))
            .cursor_pointer()
            .when(!mobile, |d| d.invisible().group_hover(group.clone(), |s| s.visible()))
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
            .when(!mobile || self.ed.can_open_board(cx), |d| d.child(open_btn))
            .when(!mobile, |d| d.tip_above("双击打开画板", None).on_click(cx.listener(move |e, ev: &ClickEvent, window, cx| {
                if ev.click_count() >= 2 { e.open_board(id, window, cx); }
            })))
            .into_any_element()
    }
}

use gpui::prelude::FluentBuilder;

impl Editor {
    pub(super) fn discard_open_board(&mut self) { self.boards.open = None; }

    fn can_open_board(&self, _cx: &App) -> bool {
        if self.read_only || self.preview { return false; }
        #[cfg(all(target_os = "ios", feature = "cloudkit-sync"))]
        return crate::cloud_sync::shared::can_edit_in(&self.library_root, _cx);
        #[cfg(not(all(target_os = "ios", feature = "cloudkit-sync")))]
        true
    }

    pub(super) fn open_board(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(BlockKind::Board(bd)) = self.core.block(id).map(|b| b.kind.clone()) else { return };
        if self.board_open() || !self.can_open_board(cx) { return; }
        #[cfg(target_os = "ios")]
        {
            self.dismiss_touch_menu();
            gpui_mobile::hide_keyboard();
        }
        let view = cx.new(|cx| BoardView::new(bd, self.library_root.clone(), window, cx));
        let sub = cx.subscribe_in(&view, window, move |e, view, ev: &BoardEvent, window, cx| {
            if !e.boards.open.as_ref().is_some_and(|(_, open, _)| open.entity_id() == view.entity_id()) { return; }
            match ev {
                BoardEvent::Close => e.close_board(Some(view.read(cx).board()), window, cx),
                // Saved as it's edited: one document undo step per board session.
                BoardEvent::Changed if e.can_open_board(cx) => {
                    if e.core.set_board(id, Arc::unwrap_or_clone(view.read(cx).board()), e.boards.saved_once) {
                        e.boards.saved_once = true;
                        e.changed_preserving_viewport(window, cx);
                    }
                }
                BoardEvent::Changed => {}
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
        if let Some(bd) = board && self.can_open_board(cx) && self.core.set_board(id, Arc::unwrap_or_clone(bd), self.boards.saved_once) {
            self.changed_preserving_viewport(window, cx);
        }
        #[cfg(target_os = "ios")]
        gpui_mobile::hide_keyboard();
        if self.board_file {
            cx.emit(EditorEvent::Leave);
        } else {
            crate::gpui_compat::focus(window, &self.focus, cx);
        }
        cx.notify();
    }

    /// A board is open (in place of the document).
    pub fn board_open(&self) -> bool { self.boards.open.is_some() }

    #[cfg(target_os = "ios")]
    pub(crate) fn close_mobile_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((_, view, _)) = &self.boards.open {
            view.update(cx, |view, cx| view.close(window, cx));
        }
    }

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

#[cfg(target_os = "ios")]
impl Editor {
    #[cfg(feature = "mobile-qa")]
    pub(crate) fn mobile_board_probe(&self) -> Option<String> {
        let (id, ..) = self.boards.open.as_ref()?;
        let BlockKind::Board(board) = &self.core.block(*id)?.kind else { return None; };
        let elements: Vec<_> = board.elements.iter().map(|e| (&e.id, e.ty().as_str(), e.x, e.y, e.width, e.height)).collect();
        Some(serde_json::json!({"elements":elements}).to_string())
    }
}
