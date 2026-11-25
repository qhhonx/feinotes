//! Portable Markdown and its referenced assets as a ZIP archive.

use super::{Options, bundle_name, export};
use crate::model::Document;
use anyhow::Result;
use crate::siyuan::zip::Writer;
use std::path::Path;

pub struct Archive {
    pub filename: String,
    pub bytes: Vec<u8>,
    pub missing: usize,
}

/// Plan links and member names together. The caller supplies an already
/// scoped asset lookup in `opts` and reads just the sources it authorized.
pub fn pack(doc: &Document, title: &str, opts: Options<'_>, mut read: impl FnMut(&Path) -> Result<Vec<u8>>) -> Result<Archive> {
    let name = bundle_name(title);
    let assets = format!("{name}.assets");
    let out = export(doc, &Options { assets: Some(&assets), ..opts });
    let mut zip = Writer::default();
    zip.store(&format!("{name}.md"), out.text.as_bytes())?;
    for (path, content) in out.generated {
        zip.store(&path, content.as_bytes())?;
    }
    for (source, path) in out.files {
        zip.store(&path, &read(&source)?)?;
    }
    Ok(Archive { filename: format!("{name}.zip"), bytes: zip.finish(), missing: out.missing })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn archive_reuses_members_and_leaves_missing_sources_out_of_links() {
        let doc = crate::docxml::parse(r#"<title>Bundle</title>
<img src="one/Cover.png" caption="first"/>
<img src="two/cover.png" caption="second"/>
<img src="one/Cover.png" caption="same file again"/>
<img src="missing.png" caption="unavailable"/>"#).unwrap();
        let resolve = |src: &str| (src != "missing.png").then(|| PathBuf::from(src));
        let archive = pack(&doc, "CON", Options { archive_asset: Some(&resolve), ..Default::default() }, |source| {
            Ok(source.to_string_lossy().as_bytes().to_vec())
        }).unwrap();
        assert_eq!(archive.filename, "_CON.zip");
        assert_eq!(archive.missing, 1);
        let zipped = crate::siyuan::zip::files(&archive.bytes).unwrap();
        assert_eq!(zipped.len(), 3);
        let member = |name: &str| zipped.iter().find(|(path, _)| path == Path::new(name)).unwrap().1.as_slice();
        let body = std::str::from_utf8(member("_CON.md")).unwrap();
        assert_eq!(body.matches("_CON.assets/files/Cover.png").count(), 2);
        assert!(body.contains("_CON.assets/files/cover-2.png"));
        assert!(body.contains("unavailable") && !body.contains("missing.png"));
        assert_eq!(member("_CON.assets/files/cover-2.png"), b"two/cover.png");
    }
}
