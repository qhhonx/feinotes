//! File types and parsing shared by browser pickers and drops.

use crate::model::Document;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind { Markdown, DocxXml }

impl Kind {
    pub fn of(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;
        if ext.eq_ignore_ascii_case("xml") { return Some(Self::DocxXml); }
        ["md", "markdown", "mdown", "mark", "txt"].iter().any(|kind| ext.eq_ignore_ascii_case(kind)).then_some(Self::Markdown)
    }
}

/// Keep the file's contents behind the library and attachment policies of
/// the importing workspace.
pub fn parse_docxxml(text: &str) -> anyhow::Result<Document> {
    crate::docxml::parse(text.trim_start_matches('\u{feff}'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_file_types() {
        for path in ["Readme.md", "Alpha.MDOWN", "folder/Plan.mark", "root/sub/title.txt", "Notes.MARKDOWN"] {
            assert_eq!(Kind::of(Path::new(path)), Some(Kind::Markdown));
        }
        assert_eq!(Kind::of(Path::new("folder/Document.XML")), Some(Kind::DocxXml));
        for path in ["Note.docx", "Artifact.csv", "Diagram.png", "NoExtension", "Backups.txt.zip"] {
            assert_eq!(Kind::of(Path::new(path)), None);
        }
    }
}
