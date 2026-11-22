//! Base64 images inside Markdown (#133).
//!
//! A Markdown file can carry its pictures inline, as
//! `![alt](data:image/png;base64,…)`. Everything else in the editor reads
//! images from disk, so opening such a file writes each payload into the
//! library's `assets/` folder and points the block there. The Markdown file
//! keeps its base64 until the document is saved; from then on the image is an
//! ordinary asset, exported like a pasted one.
//!
//! The payload name is a hash of the bytes, so opening the same file twice
//! reuses the file already written instead of piling up copies.

use crate::vfs::PathExt as _;
use std::path::Path;

use base64::Engine;

use crate::model::{Block, BlockKind};

/// Longest payload that is written out; one image pasted by hand is capped at
/// the same 20 MB (`src/editor/image.rs`).
const MAX_BYTES: u64 = 20 * 1024 * 1024;

/// Writes every inline image under `root/assets` and rewrites its `src` to the
/// library-relative path. Returns how many were written. A payload that is too
/// big, isn't an image this app can show, or doesn't decode is left alone — the
/// block then shows `图片不可用` and the file keeps its own bytes.
pub fn absorb(blocks: &mut [Block], root: &Path) -> usize {
    let mut n = 0;
    for b in blocks.iter_mut() {
        n += absorb(&mut b.children, root);
        if let BlockKind::Table(t) = &mut b.kind {
            for cell in t.rows.iter_mut().flatten() {
                n += absorb(&mut cell.blocks, root);
            }
        }
        let BlockKind::Image { src_kind, src, .. } = &mut b.kind else { continue };
        let Some((bytes, ext)) = inline_image(src) else { continue };
        let name = format!("b64-{:016x}.{ext}", hash(&bytes));
        // The library's asset folder for its layout (`.assets`, or `assets` before real folders).
        let dir = crate::store::Store::assets_in(root);
        let path = dir.join(&name);
        if !path.fs_is_file() && (crate::vfs::create_dir_all(&dir).is_err() || crate::vfs::write(&path, &bytes).is_err()) {
            continue;
        }
        *src = format!("{}/{name}", dir.file_name().unwrap_or_default().to_string_lossy());
        *src_kind = "src".into();
        n += 1;
    }
    n
}

/// The bytes and extension of a `data:image/…;base64,…` source.
fn inline_image(src: &str) -> Option<(Vec<u8>, &'static str)> {
    let data = src.strip_prefix("data:image/")?;
    let (head, payload) = data.split_once(',')?;
    if !head.ends_with(";base64") {
        return None;
    }
    let mime = head.trim_end_matches(";base64");
    let payload = payload.trim();
    // Base64 is 4 chars per 3 bytes; refuse before decoding what can't fit.
    if payload.len() as u64 / 4 * 3 > MAX_BYTES {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(payload).ok()?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_BYTES {
        return None;
    }
    // The declared type only names the file; the bytes decide whether it is an
    // image this app shows. SVG stays out, as it does everywhere else.
    let ext = sniff(&bytes).or(match mime {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/bmp" => Some("bmp"),
        _ => None,
    })?;
    Some((bytes, ext))
}

/// The extension of an image by its magic bytes; None for anything else.
fn sniff(b: &[u8]) -> Option<&'static str> {
    match b {
        [0x89, b'P', b'N', b'G', ..] => Some("png"),
        [0xff, 0xd8, 0xff, ..] => Some("jpg"),
        [b'G', b'I', b'F', b'8', ..] => Some("gif"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("webp"),
        [b'B', b'M', ..] => Some("bmp"),
        _ => None,
    }
}

/// FNV-1a, 64 bit: a short, stable name for the payload. A collision only
/// costs a wrong picture, so the bar is "never happens by accident".
fn hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^ (bytes.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(payload: &[u8]) -> String {
        format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(payload))
    }

    fn image_block(src: &str) -> Block {
        Block::new(
            BlockKind::Image { src_kind: "src".into(), src: src.into(), width: None, height: None, caption: None, crop: None },
            Default::default(),
        )
    }

    fn src_of(b: &Block) -> &str {
        match &b.kind {
            BlockKind::Image { src, .. } => src,
            _ => "",
        }
    }

    #[test]
    fn inline_images_become_assets_once() {
        let root = crate::vfs::temp_dir().join(format!("quiet-b64-{}", uuid::Uuid::new_v4()));
        let bytes = b"\x89PNG\r\n\x1a\npayload";
        let mut blocks = vec![image_block(&png(bytes))];

        assert_eq!(absorb(&mut blocks, &root), 1);
        let src = src_of(&blocks[0]).to_string();
        assert!(src.starts_with(".assets/b64-") && src.ends_with(".png"), "{src}");
        assert_eq!(crate::vfs::read(root.join(&src)).unwrap(), bytes);

        // Opening the file again reuses the same asset instead of a new copy.
        let mut again = vec![image_block(&png(bytes))];
        assert_eq!(absorb(&mut again, &root), 1);
        assert_eq!(src_of(&again[0]), src);
        assert_eq!(crate::vfs::read_dir(root.join(".assets")).unwrap().count(), 1);
        crate::vfs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn what_is_not_a_picture_stays_inline() {
        let root = crate::vfs::temp_dir().join(format!("quiet-b64-{}", uuid::Uuid::new_v4()));
        crate::vfs::create_dir_all(root.join("assets")).unwrap();
        let cases = [
            "data:image/svg+xml;base64,PHN2Zz4=".to_string(),
            "data:image/png;base64,<not base64>".to_string(),
            "data:image/png;base64,aGVsbG8=".to_string(),
            "data:image/png,notbase64".to_string(),
            "https://example.com/a.png".to_string(),
            "assets/pasted.png".to_string(),
        ];
        let mut blocks: Vec<Block> = cases.iter().map(|c| image_block(c)).collect();
        assert_eq!(absorb(&mut blocks, &root), 0);
        for (b, c) in blocks.iter().zip(&cases) {
            assert_eq!(src_of(b), c);
        }
        crate::vfs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn images_inside_a_table_are_absorbed() {
        let root = crate::vfs::temp_dir().join(format!("quiet-b64-{}", uuid::Uuid::new_v4()));
        crate::vfs::create_dir_all(root.join("assets")).unwrap();
        let mut cell = crate::model::TableCell::empty();
        cell.blocks = vec![image_block(&png(b"\x89PNG\r\n\x1a\nin a cell"))];
        let table = crate::model::Table { col_widths: vec![100.], header_rows: 0, header_col: false, rows: vec![vec![cell]], ..Default::default() };
        let mut blocks = vec![Block::new(BlockKind::Table(table), Default::default())];
        assert_eq!(absorb(&mut blocks, &root), 1);
        crate::vfs::remove_dir_all(&root).unwrap();
    }
}
