//! Quiet.md's document engine, free of any UI: the block model, DocxXML
//! reading and writing, the editing core with undo, the library on disk,
//! and Markdown / HTML import plus Markdown export.

/// Quiet.md version of the shared document engine.
pub const UPSTREAM_VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod board;
pub mod changes;
pub mod data_images;
pub mod diff;
pub mod docxml;
pub mod editor;
pub mod emoji;
pub mod lang;
pub mod markdown;
pub mod merge;
pub mod merge_file;
pub mod model;
pub mod palette;
pub mod paths;
pub mod paste;
pub mod remote;
pub mod siyuan;
pub mod spacing;
pub mod store;
pub mod sync_base;
pub mod table;
pub mod time;
pub mod versions;
pub mod vfs;
pub mod whiteboard;
pub mod home;
