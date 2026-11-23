mod bidi;
mod board;
#[cfg(target_os = "ios")]
mod block_mobile;
mod datatable;
mod button;
mod caption;
mod code;
mod catalogue;
mod countdown;
mod cover;
mod ctxmenu;
pub use fdl_core::editor::core;
mod drag;
pub mod emoji;
mod emoji_picker;
mod file;
mod find;
#[cfg(target_os = "ios")]
mod find_mobile;
mod gap;
mod grid;
pub mod image;
pub mod keymap;
mod link;
pub mod links;
pub mod math;
pub mod menus;
mod overlays;
mod numbering;
mod reading;
mod scrollbar;
pub mod reminder;
mod render;
pub(crate) use render::equation_numbers;
mod subpages;
#[cfg(target_os = "ios")]
pub(crate) mod touch;
pub mod table;
mod table_actions;
#[cfg(target_os = "ios")]
mod table_mobile;
#[cfg(target_os = "ios")]
mod format_mobile;
#[cfg(target_os = "ios")]
mod media_mobile;
#[cfg(target_os = "ios")]
mod object_mobile;
pub use fdl_core::editor::table_ops;
pub mod text;
pub mod view;
