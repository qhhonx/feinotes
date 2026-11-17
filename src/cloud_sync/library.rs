// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.
//! Publish a verified cloud selection only after its preparation succeeds.
use anyhow::{Context, Result, ensure};
use std::path::Path;

pub(crate) fn commit(root: &Path) -> Result<()> {
    let parent = root.parent().context("Missing iCloud library folder")?;
    let key = root.file_name().and_then(|key| key.to_str()).context("Missing iCloud account key")?;
    ensure!(key.len() == 64 && key.bytes().all(|byte| byte.is_ascii_hexdigit()), "Invalid iCloud account key");
    let staged = parent.join(format!(".active-account-{}", uuid::Uuid::new_v4()));
    std::fs::write(&staged, key)?;
    if let Err(error) = std::fs::rename(&staged, parent.join("active-account")) {
        let _ = std::fs::remove_file(staged);
        return Err(error.into());
    }
    match std::fs::remove_file(parent.join("use-local-library")) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
