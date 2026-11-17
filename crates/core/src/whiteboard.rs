//! 画板 as a block and as a file: an Excalidraw scene (`board::*`) inside a
//! document (`<whiteboard type="excalidraw" height="…">` holding the compact
//! scene JSON) or a standalone `.excalidraw` file (the official JSON). Reading
//! goes through `board::restore`, writing through `board::json`; deleted
//! elements are left out, as upstream's export does. See
//! docs/design/whiteboard-excalidraw.md (存储).

use crate::board::export_svg::{SvgExportOpts, export_to_svg};
use crate::board::json::{serialize_as_json, serialize_scene};
use crate::board::restore::{ExportedAppState, RestoreOpts, load_file, load_scene};
use crate::board::text_measure::{Font, Measure};
use crate::board::types::{Element, FileData};
use std::collections::BTreeMap;

/// A new block's height in the page, px.
pub const DEFAULT_HEIGHT: f32 = 360.;
/// What `.excalidraw` files we write say made them (`source`).
pub const FILE_SOURCE: &str = "https://quiet.md";

/// One scene. Documents keep only `elements` and `files`; a `.excalidraw`
/// file also keeps `app` (canvas color, grid). `height` is the block's
/// height in the page (files ignore it).
#[derive(Clone, Debug, PartialEq)]
pub struct Board {
    /// Non-deleted, in z-order (fractional-index order).
    pub elements: Vec<Element>,
    pub files: BTreeMap<String, FileData>,
    pub app: ExportedAppState,
    pub height: f32,
}

impl Default for Board {
    fn default() -> Self { Board { elements: vec![], files: BTreeMap::new(), app: ExportedAppState::default(), height: DEFAULT_HEIGHT } }
}

impl Board {
    fn new(elements: Vec<Element>, files: BTreeMap<String, FileData>, app: ExportedAppState, height: f32) -> Board {
        Board { elements: elements.into_iter().filter(|e| !e.is_deleted).collect(), files, app, height }
    }

    /// The document form (`{"elements":[…],"files":{…}}`); None when it isn't
    /// scene JSON (the element then stays as it came).
    pub fn from_scene(json: &str, height: f32) -> Option<Board> {
        let d = load_scene(json, &RestoreOpts::load()).ok()?;
        Some(Board::new(d.elements, d.files, ExportedAppState::default(), height))
    }

    pub fn scene_json(&self) -> String { serialize_scene(&self.elements, &self.files) }

    /// A `.excalidraw` file.
    pub fn from_file(json: &str) -> anyhow::Result<Board> {
        let d = load_file(json, &RestoreOpts::load()).map_err(anyhow::Error::msg)?;
        Ok(Board::new(d.elements, d.files, d.app_state, DEFAULT_HEIGHT))
    }

    pub fn file_json(&self) -> String { serialize_as_json(&self.elements, &self.app, &self.files, FILE_SOURCE) + "\n" }

    /// `getSceneVersion` plus the count: changes whenever an element does.
    /// Caches (thumbnails, `hash_block`) key on it.
    pub fn version(&self) -> u64 {
        self.elements.iter().fold(self.elements.len() as u64, |n, e| n.wrapping_mul(31).wrapping_add(e.version as u64 ^ e.version_nonce as u64))
    }

    /// What search sees: text elements' original text and frame names.
    pub fn plain_text(&self) -> String {
        let mut out: Vec<&str> = vec![];
        for e in &self.elements {
            if let Some(t) = e.text() && !t.original_text.trim().is_empty() { out.push(&t.original_text); }
            if let Some(n) = e.frame_name().filter(|n| !n.trim().is_empty()) { out.push(n); }
        }
        out.join("\n")
    }

    /// A standalone SVG (Markdown / HTML export): `href` turns an image file
    /// into what its `<image href>` says (None: left out).
    pub fn svg(&self, m: &dyn Measure, href: &dyn Fn(&FileData) -> Option<String>) -> String {
        let opts = SvgExportOpts { background: self.app.view_background_color.clone(), image_href: Some(href), ..Default::default() };
        export_to_svg(&self.elements, &self.files, m, &opts)
    }

    /// `svg` with images read from the library at `root` (or their data URL).
    pub fn svg_in(&self, root: &std::path::Path) -> String {
        use base64::Engine;
        let href = |f: &FileData| -> Option<String> {
            if let Some(u) = &f.data_url { return Some(u.clone()); }
            let p = crate::store::export_path_in(root, f.path.as_deref()?)?;
            let bytes = crate::store::read_bounded(&p, crate::store::EXPORT_FILE_LIMIT).ok()?;
            Some(format!("data:{};base64,{}", f.mime_type, base64::engine::general_purpose::STANDARD.encode(bytes)))
        };
        self.svg(&ApproxMeasure, &href)
    }
}

/// A `.excalidraw` file as a document: titled by its file name, holding
/// the one board.
pub fn parse_file(text: &str, title: &str) -> anyhow::Result<crate::model::Document> {
    let bd = if text.trim().is_empty() { Board::default() } else { Board::from_file(text)? };
    Ok(file_doc(bd, title))
}

pub fn file_doc(bd: Board, title: &str) -> crate::model::Document {
    use crate::model::*;
    Document { title: RichText::plain(title), blocks: vec![Block::new(BlockKind::Board(std::sync::Arc::new(bd)), RichText::default())], ..Default::default() }
}

/// The board of a document opened from a `.excalidraw` file (its first
/// top-level one).
pub fn board_of(doc: &crate::model::Document) -> Option<&Board> {
    doc.blocks.iter().find_map(|b| if let crate::model::BlockKind::Board(bd) = &b.kind { Some(&**bd) } else { None })
}

/// `parse_file` reversed: the official JSON.
pub fn write_file(doc: &crate::model::Document) -> String { board_of(doc).cloned().unwrap_or_default().file_json() }

/// Text widths without a font system (export): wide characters one em,
/// others 0.55 em. The app measures with GPUI instead.
pub struct ApproxMeasure;
impl Measure for ApproxMeasure {
    fn line_width(&self, text: &str, font: Font) -> f64 {
        text.chars().map(|c| if unicode_width::UnicodeWidthChar::width(c).unwrap_or(1) >= 2 { 1. } else { 0.55 }).sum::<f64>() * font.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &str = include_str!("../tests/fixtures/board/all-elements.excalidraw");

    #[test]
    fn file_and_scene_forms_round_trip() {
        let b = Board::from_file(FIXTURE).unwrap();
        assert!(!b.elements.is_empty());
        assert_eq!(Board::from_file(&b.file_json()).unwrap(), b);
        let s = Board::from_scene(&b.scene_json(), 300.).unwrap();
        assert_eq!((&s.elements, &s.files, s.height), (&b.elements, &b.files, 300.));
        assert_eq!(s.scene_json(), b.scene_json());
        let text = b.plain_text();
        assert!(text.contains("Hello world") && text.contains("Group A"), "{text}");
        assert!(Board::from_scene("[1]", 1.).is_none() && Board::from_scene("nope", 1.).is_none());
        assert!(b.svg_in(std::path::Path::new("/nonexistent")).starts_with("<svg"));
    }
}
