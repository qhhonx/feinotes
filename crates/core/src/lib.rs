//! Quiet.md's document engine, free of any UI: the block model, DocxXML
//! reading and writing, the editing core with undo, the library on disk,
//! and Markdown / HTML import plus Markdown export.

pub mod changes;
pub mod data_images;
pub mod diff;
pub mod docxml;
pub mod editor;
pub mod emoji;
pub mod lang;
pub mod markdown;
pub mod model;
pub mod palette;
pub mod paste;
pub mod store;
pub mod time;
pub mod versions;
