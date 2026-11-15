//! Assets compiled into the binary: Lucide icons (ISC licensed).

use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// One embedded file by its path under `assets/`.
pub fn get(path: &str) -> Option<&'static [u8]> { FILES.iter().find(|(p, _)| *p == path).map(|(_, b)| *b) }

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if path.starts_with(crate::editor::math::SVG_PREFIX) {
            return Ok(crate::editor::math::svg_asset(path).map(Cow::Owned));
        }
        Ok(get(path).map(Cow::Borrowed))
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(FILES.iter().filter(|(p, _)| p.starts_with(path)).map(|(p, _)| SharedString::from(*p)).collect())
    }
}
