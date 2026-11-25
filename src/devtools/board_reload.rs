//! Assertions for tools/harness/board_reload.py: a 画板 written by another
//! program (an agent) while it is open, half written, and changed on both sides.
use super::*;
use crate::app::{SaveStatus, Workspace};
use crate::board::BoardView;
use crate::whiteboard::Board;

fn view(ws: &Workspace, cx: &App) -> Entity<BoardView> { ws.editor.as_ref().unwrap().read(cx).board_view().expect("a board open") }
fn ids(v: &BoardView) -> Vec<String> { v.scene.non_deleted_elements().map(|e| e.id.clone()).collect() }
fn file(ws: &Workspace) -> std::path::PathBuf { ws.store.file_path(ws.current.as_ref().unwrap()).unwrap() }
/// The board on disk plus a copy of the fixture's first shape named `id`, moved right.
fn with_shape(ws: &Workspace, id: &str) -> String {
    let mut bd = Board::from_file(&std::fs::read_to_string(file(ws)).unwrap()).unwrap();
    let fixture = Board::from_file(include_str!("../../crates/core/tests/fixtures/board/all-elements.excalidraw")).unwrap();
    let mut e = fixture.elements.into_iter().find(|e| e.text().is_none() && e.image().is_none()).unwrap();
    e.id = id.into();
    e.x += 40. * bd.elements.len() as f64;
    bd.elements.push(e);
    bd.file_json()
}

pub(super) fn step(phase: &str, ws: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    match phase {
        "open" => ws.create_board(None, window, cx),
        "opened" => {
            let v = view(ws, cx);
            assert!(ids(v.read(cx)).is_empty());
            // somewhere other than where it opens, to see it kept
            v.update(cx, |v, cx| v.set_viewport(crate::board::viewport::Viewport { scroll_x: 123., scroll_y: -45., zoom: 1.5 }, cx));
            // the agent's first shape
            std::fs::write(file(ws), with_shape(ws, "agent-1")).unwrap();
        }
        "added" => {
            let v = view(ws, cx).read(cx);
            assert_eq!(ids(v), ["agent-1"], "the shape written outside is on the board");
            let vp = v.viewport();
            assert_eq!((vp.scroll_x, vp.scroll_y, vp.zoom), (123., -45., 1.5), "the viewport stays");
            assert!(!ws.unsaved && !matches!(ws.status, SaveStatus::Failed(_)));
            // Caught half written: nothing changes, nothing is cleared, it is read again until whole.
            let whole = with_shape(ws, "agent-2");
            std::fs::write(file(ws), &whole[..whole.len() / 2]).unwrap();
            std::fs::write(std::env::temp_dir().join("quiet-board-reload-whole"), whole).unwrap();
        }
        "half" => {
            assert_eq!(ids(view(ws, cx).read(cx)), ["agent-1"], "a half-written file leaves the board as it was");
            let whole = std::env::temp_dir().join("quiet-board-reload-whole");
            std::fs::write(file(ws), std::fs::read(&whole).unwrap()).unwrap();
            let _ = std::fs::remove_file(whole);
        }
        "whole" => {
            assert_eq!(ids(view(ws, cx).read(cx)), ["agent-1", "agent-2"]);
            // Changed outside and here at once: the save refuses to write over the outside version,
            // and the board keeps the edit made here (as a document does).
            let theirs = with_shape(ws, "agent-3");
            std::fs::write(file(ws), &theirs).unwrap();
            std::fs::write(std::env::temp_dir().join("quiet-board-reload-theirs"), theirs).unwrap();
            view(ws, cx).update(cx, |v, cx| {
                let mut els = v.scene.elements_including_deleted().to_vec();
                els.retain(|e| e.id != "agent-1");
                v.scene.replace_all_elements(els);
                v.commit(Some(crate::board::history::CaptureUpdateAction::Immediately), cx);
            });
        }
        "conflict" => {
            let theirs = std::env::temp_dir().join("quiet-board-reload-theirs");
            assert_eq!(std::fs::read_to_string(file(ws)).unwrap(), std::fs::read_to_string(&theirs).unwrap(), "the outside version is not written over");
            let _ = std::fs::remove_file(theirs);
            assert_eq!(ids(view(ws, cx).read(cx)), ["agent-2"], "the edit made here stays on the board");
            assert!(ws.unsaved && matches!(ws.status, SaveStatus::Failed(_)), "{:?}", ws.status);
            // 重新载入: the version on disk, on the open board.
            ws.resolve_save_failed(false, window, cx);
        }
        "reloaded" => {
            assert_eq!(ids(view(ws, cx).read(cx)), ["agent-1", "agent-2", "agent-3"]);
            assert!(!ws.unsaved);
        }
        _ => panic!("unknown board-reload check {phase}"),
    }
    eprintln!("board-reload {phase}: PASS");
}
