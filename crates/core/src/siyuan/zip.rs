//! ZIP support for Markdown exports and SiYuan imports. The central
//! directory lists each entry, whose data is stored or raw deflate.
//! ZIP64 archives are outside this reader and writer's format.

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

/// A single-disk ZIP writer, retaining only the archive and its directory.
#[derive(Default)]
pub(crate) struct Writer {
    bytes: Vec<u8>,
    directory: Vec<u8>,
    count: u16,
}

impl Writer {
    pub(crate) fn store(&mut self, name: &str, data: &[u8]) -> anyhow::Result<()> {
        self.entry(name, data, data, 0)
    }

    fn entry(&mut self, name: &str, data: &[u8], payload: &[u8], method: u16) -> anyhow::Result<()> {
        anyhow::ensure!(self.count < u16::MAX - 1 && name.len() <= u16::MAX as usize, "Too many ZIP entries or file name too long");
        anyhow::ensure!([self.bytes.len(), self.directory.len(), payload.len(), data.len()]
            .into_iter().all(|n| n < u32::MAX as usize), "ZIP64 is not supported");
        let local = self.bytes.len() as u32;
        let crc = crc32(data);
        let head = |sig: u32, central: bool| {
            let mut h = sig.to_le_bytes().to_vec();
            if central { h.extend(20u16.to_le_bytes()); }
            h.extend(20u16.to_le_bytes());
            h.extend((1u16 << 11).to_le_bytes());
            h.extend(method.to_le_bytes());
            h.extend([0, 0, 33, 0]); // 1980-01-01 00:00 in DOS time.
            h.extend(crc.to_le_bytes());
            h.extend((payload.len() as u32).to_le_bytes());
            h.extend((data.len() as u32).to_le_bytes());
            h.extend((name.len() as u16).to_le_bytes());
            h.extend(0u16.to_le_bytes());
            if central { h.extend([0; 10]); h.extend(local.to_le_bytes()); }
            h.extend(name.as_bytes());
            h
        };
        let local_head = head(0x0403_4b50, false);
        let record = head(0x0201_4b50, true);
        let size = self.bytes.len() as u64 + self.directory.len() as u64
            + local_head.len() as u64 + payload.len() as u64 + record.len() as u64 + 22;
        anyhow::ensure!(size < u32::MAX as u64, "ZIP64 is not supported");
        self.bytes.extend(local_head);
        self.bytes.extend(payload);
        self.directory.extend(record);
        self.count += 1;
        Ok(())
    }

    pub(crate) fn finish(mut self) -> Vec<u8> {
        let at = self.bytes.len() as u32;
        self.bytes.extend(&self.directory);
        self.bytes.extend(0x0605_4b50u32.to_le_bytes());
        self.bytes.extend([0; 4]);
        self.bytes.extend(self.count.to_le_bytes());
        self.bytes.extend(self.count.to_le_bytes());
        self.bytes.extend((self.directory.len() as u32).to_le_bytes());
        self.bytes.extend(at.to_le_bytes());
        self.bytes.extend([0; 2]);
        self.bytes
    }
}

/// A zip of deflated files, used by the SiYuan fixtures.
pub fn pack(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = Writer::default();
    for (name, data) in files {
        let packed = miniz_oxide::deflate::compress_to_vec(data, 6);
        zip.entry(name, data, &packed, 8).expect("small ZIP fixture");
    }
    zip.finish()
}

fn crc32(data: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut table = [0; 256];
        let mut i = 0;
        while i < table.len() {
            let mut c = i as u32;
            let mut bit = 0;
            while bit < 8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
                bit += 1;
            }
            table[i] = c;
            i += 1;
        }
        table
    };
    !data.iter().fold(!0, |c, &b| TABLE[((c as u8) ^ b) as usize] ^ (c >> 8))
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
