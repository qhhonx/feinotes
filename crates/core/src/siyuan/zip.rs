//! Unpacking a `.sy.zip` (思源's 导出 › SiYuan .sy.zip) without a zip crate:
//! the central directory at the end lists each entry, whose data is stored
//! or raw deflate (`miniz_oxide`). No Zip64: an export that large is refused.

use std::path::{Component, Path, PathBuf};

fn u16_at(b: &[u8], at: usize) -> Option<usize> { Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?) as usize) }
fn u32_at(b: &[u8], at: usize) -> Option<usize> { Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?) as usize) }

/// The files in `zip`, as (relative path, contents). Entries that would
/// land outside the folder (`..`, absolute paths) are skipped.
pub fn files(zip: &[u8]) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
    // End of central directory: 22 bytes plus a comment of up to 64 KiB.
    let low = zip.len().saturating_sub(22 + 65_535);
    let end = (low..zip.len().saturating_sub(21)).rev().find(|&i| zip[i..i + 4] == [0x50, 0x4b, 0x05, 0x06]).ok_or("不是 zip 文件")?;
    let (count, mut at) = (u16_at(zip, end + 10).ok_or("zip 文件已损坏")?, u32_at(zip, end + 16).ok_or("zip 文件已损坏")?);
    if count == 0xffff || at == 0xffff_ffff { return Err("不支持 Zip64 格式的压缩包".into()) }
    let mut out = vec![];
    for _ in 0..count {
        let head = zip.get(at..at + 46).ok_or("zip 文件已损坏")?;
        if head[..4] != [0x50, 0x4b, 0x01, 0x02] { return Err("zip 文件已损坏".into()) }
        let method = u16_at(head, 10).unwrap_or(0);
        let (size, full) = (u32_at(head, 20).unwrap_or(0), u32_at(head, 24).unwrap_or(0));
        let (name_len, extra, comment, local) = (u16_at(head, 28).unwrap_or(0), u16_at(head, 30).unwrap_or(0), u16_at(head, 32).unwrap_or(0), u32_at(head, 42).unwrap_or(0));
        let raw = zip.get(at + 46..at + 46 + name_len).ok_or("zip 文件已损坏")?;
        at += 46 + name_len + extra + comment;
        if size == 0xffff_ffff || full == 0xffff_ffff { return Err("不支持 Zip64 格式的压缩包".into()) }
        // 思源 writes UTF-8 names (flag bit 11); anything else is read the same, lossily.
        let name = String::from_utf8_lossy(raw).replace('\\', "/");
        if name.ends_with('/') { continue }
        let rel = PathBuf::from(&name);
        if !rel.components().all(|c| matches!(c, Component::Normal(_))) { continue }
        let lh = zip.get(local..local + 30).ok_or("zip 文件已损坏")?;
        if lh[..4] != [0x50, 0x4b, 0x03, 0x04] { return Err("zip 文件已损坏".into()) }
        let start = local + 30 + u16_at(lh, 26).unwrap_or(0) + u16_at(lh, 28).unwrap_or(0);
        let data = zip.get(start..start + size).ok_or("zip 文件已损坏")?;
        let bytes = match method {
            0 => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, full.max(1)).map_err(|e| format!("解压失败：{e:?}"))?,
            m => return Err(format!("不支持的压缩方式 {m}")),
        };
        out.push((rel, bytes));
    }
    Ok(out)
}

/// `files` written out under `dir`.
pub fn unpack(zip: &[u8], dir: &Path) -> anyhow::Result<()> {
    for (rel, bytes) in files(zip).map_err(anyhow::Error::msg)? {
        let to = dir.join(rel);
        if let Some(p) = to.parent() { crate::vfs::create_dir_all(p)?; }
        crate::vfs::write(to, bytes)?;
    }
    Ok(())
}

/// A zip of `files` (deflated), for tests and fixtures.
pub fn pack(files: &[(&str, &[u8])]) -> Vec<u8> {
    let (mut out, mut dir) = (vec![], vec![]);
    for (name, data) in files {
        let packed = miniz_oxide::deflate::compress_to_vec(data, 6);
        let crc = crc32(data);
        let local = out.len() as u32;
        let head = |sig: u32, central: bool| {
            let mut h = sig.to_le_bytes().to_vec();
            if central { h.extend(20u16.to_le_bytes()); }
            h.extend(20u16.to_le_bytes());
            h.extend((1u16 << 11).to_le_bytes());
            h.extend(8u16.to_le_bytes());
            h.extend([0; 4]);
            h.extend(crc.to_le_bytes());
            h.extend((packed.len() as u32).to_le_bytes());
            h.extend((data.len() as u32).to_le_bytes());
            h.extend((name.len() as u16).to_le_bytes());
            h.extend(0u16.to_le_bytes());
            // Comment length, disk, internal and external attributes, then where the entry starts.
            if central { h.extend([0; 10]); h.extend(local.to_le_bytes()); }
            h.extend(name.as_bytes());
            h
        };
        out.extend(head(0x0403_4b50, false));
        out.extend(&packed);
        dir.extend(head(0x0201_4b50, true));
    }
    let at = out.len() as u32;
    out.extend(&dir);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0; 4]);
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((dir.len() as u32).to_le_bytes());
    out.extend(at.to_le_bytes());
    out.extend([0; 2]);
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 { c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 }; }
    }
    !c
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_packed_zip_unpacks_to_the_same_files_and_skips_escapes() {
        let zip = super::pack(&[("笔记本/a.sy", b"{\"x\":1}"), ("笔记本/assets/b.png", &[0u8; 300]), ("../evil", b"x")]);
        let got = super::files(&zip).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].0, std::path::PathBuf::from("笔记本/a.sy"));
        assert_eq!(got[0].1, b"{\"x\":1}");
        assert_eq!(got[1].1, vec![0u8; 300]);
        assert!(super::files(b"not a zip").is_err());
        assert_eq!(super::crc32(b"123456789"), 0xcbf4_3926);
    }
}
