//! The system's trash (废纸篓, the Recycle Bin), for files in 本机文件夹:
//! they are the user's, so they go where the Finder would put them.

use std::path::Path;

#[cfg(target_os = "macos")]
pub fn trash(path: &Path) -> anyhow::Result<()> {
    // NSFileManager, not the default (asking the Finder over AppleScript,
    // which needs the Automation permission and plays the trash sound).
    use trash::macos::{DeleteMethod, TrashContextExtMacos};
    let mut ctx = trash::TrashContext::default();
    ctx.set_delete_method(DeleteMethod::NsFileManager);
    Ok(ctx.delete(path)?)
}

#[cfg(not(any(target_os = "macos", target_os = "ios", target_family = "wasm")))]
pub fn trash(path: &Path) -> anyhow::Result<()> { Ok(trash::delete(path)?) }

#[cfg(any(target_os = "ios", target_family = "wasm"))]
pub fn trash(_: &Path) -> anyhow::Result<()> { anyhow::bail!("unsupported") }
