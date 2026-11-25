//! Serialize the live board for a browser download.

use super::{Board, BoardEvent, BoardView, types::FileData};
use gpui::Context;
use std::sync::Arc;

#[derive(Clone, Copy)]
pub(super) enum Format { Svg, Excalidraw }

impl Format {
    pub fn label(self) -> &'static str {
        use crate::settings::tr;
        match self {
            Self::Svg => tr("下载 SVG", "Download SVG"),
            Self::Excalidraw => tr("下载 Excalidraw", "Download Excalidraw"),
        }
    }

    pub fn save(self, view: &BoardView, board: Arc<Board>, read: impl Fn(&FileData) -> Option<String>, cx: &mut Context<BoardView>) {
        let mut board = Arc::unwrap_or_clone(board);
        for file in board.files.values_mut() {
            let Some(uri) = read(file) else {
                cx.emit(BoardEvent::Toast(crate::settings::tr(
                    "部分画板图片还无法读取，请在图片加载后重试。",
                    "Some board images could not be read. Try again after they load.",
                ).into()));
                return;
            };
            file.data_url = Some(uri);
            file.path = None;
        }
        let (ext, mime, content) = match self {
            Self::Svg => ("svg", super::image::mime::SVG, board.svg(&*view.m, &|file| file.data_url.clone())),
            Self::Excalidraw => ("excalidraw", super::image::mime::EXCALIDRAW, board.file_json()),
        };
        let name = fdl_core::markdown::bundle_name(&view.export_name);
        if crate::web_download::download_file(&format!("{name}.{ext}"), content.as_bytes(), mime).is_err() {
            cx.emit(BoardEvent::Toast(crate::settings::tr("下载失败，请重试。", "The download failed. Try again.").into()));
        }
    }
}
