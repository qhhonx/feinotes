//! Reading GitHub's tarball (`git archive`, gzipped) without a tar crate:
//! gzip's header is skipped by hand and the deflate stream inflated with
//! `miniz_oxide`; tar is 512-byte headers each followed by the file, with
//! long names in the ustar `prefix` field or a pax `path` record.

/// The gzip stream's contents (RFC 1952: a header, raw deflate, a trailer).
pub fn gunzip(gz: &[u8]) -> Result<Vec<u8>, String> {
    if gz.len() < 18 || gz[0] != 0x1f || gz[1] != 0x8b || gz[2] != 8 { return Err("not gzip".into()) }
    let flags = gz[3];
    let mut at = 10;
    if flags & 4 != 0 {
        let n = *gz.get(at).ok_or("short gzip")? as usize | (*gz.get(at + 1).ok_or("short gzip")? as usize) << 8;
        at += 2 + n;
    }
    // Name, then comment: each ends at a zero byte.
    for bit in [8, 16] {
        if flags & bit != 0 { at += gz.get(at..).and_then(|r| r.iter().position(|&b| b == 0)).ok_or("short gzip")? + 1 }
    }
    if flags & 2 != 0 { at += 2 }
    miniz_oxide::inflate::decompress_to_vec(gz.get(at..).ok_or("short gzip")?).map_err(|e| format!("inflate: {e:?}"))
}

/// The regular files in a tar archive, with the archive's top folder
/// (`<owner>-<repo>-<sha>/`) taken off their paths.
pub fn files(tar: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut out = vec![];
    let mut at = 0;
    // A pax `path` (or a GNU long name) for the entry that follows.
    let mut long: Option<String> = None;
    while at + 512 <= tar.len() {
        let h = &tar[at..at + 512];
        if h.iter().all(|&b| b == 0) { break }
        let size = octal(&h[124..136]).ok_or("bad tar size")? as usize;
        let data = tar.get(at + 512..at + 512 + size).ok_or("short tar")?;
        at += 512 + size.div_ceil(512) * 512;
        match h[156] {
            b'x' => { if let Some(p) = pax_path(data) { long = Some(p) } }
            b'L' => long = Some(text(data)),
            // A global pax header (git puts the commit id there).
            b'g' => {}
            b'0' | 0 => {
                let path = long.take().unwrap_or_else(|| {
                    let (name, prefix) = (text(&h[0..100]), text(&h[345..500]));
                    if &h[257..262] == b"ustar" && !prefix.is_empty() { format!("{prefix}/{name}") } else { name }
                });
                // Off with the top folder; nothing is outside it.
                if let Some((_, rest)) = path.split_once('/') && !rest.is_empty() { out.push((rest.to_string(), data.to_vec())) }
            }
            // Folders, links, anything else: not a file to sync.
            _ => long = None,
        }
    }
    Ok(out)
}

/// A zero- or space-ended octal field.
fn octal(field: &[u8]) -> Option<u64> {
    let s = std::str::from_utf8(field).ok()?.trim_matches(|c: char| c == '\0' || c == ' ');
    if s.is_empty() { Some(0) } else { u64::from_str_radix(s, 8).ok() }
}

/// A zero-ended name field.
fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]).into_owned()
}

/// The `path` record of a pax header: lines of `<length> <key>=<value>\n`.
fn pax_path(data: &[u8]) -> Option<String> {
    let mut at = 0;
    while at < data.len() {
        let sp = at + data[at..].iter().position(|&b| b == b' ')?;
        let len: usize = std::str::from_utf8(&data[at..sp]).ok()?.parse().ok()?;
        let rec = data.get(sp + 1..at + len)?.strip_suffix(b"\n")?;
        if let Some(v) = rec.strip_prefix(b"path=") { return String::from_utf8(v.to_vec()).ok() }
        at += len;
    }
    None
}

/// A tar archive of `files` under `top/`, long names through pax, the way
/// `git archive` writes them; and gzip of that. For tests.
#[cfg(test)]
pub fn build(top: &str, files: &[(&str, &[u8])]) -> Vec<u8> {
    fn header(name: &str, size: usize, kind: u8, prefix: &str) -> [u8; 512] {
        let mut h = [0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[100..107].copy_from_slice(b"0000644");
        h[124..135].copy_from_slice(format!("{size:011o}").as_bytes());
        h[156] = kind;
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        h[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
        h[148..156].copy_from_slice(b"        ");
        let sum: u32 = h.iter().map(|&b| b as u32).sum();
        h[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
        h
    }
    fn push(out: &mut Vec<u8>, data: &[u8]) {
        out.extend_from_slice(data);
        out.resize(out.len().div_ceil(512) * 512, 0);
    }
    let mut out = vec![];
    let comment = b"52 comment=0123456789012345678901234567890123456789\n";
    push(&mut out, &header("pax_global_header", comment.len(), b'g', ""));
    push(&mut out, comment);
    push(&mut out, &header(&format!("{top}/"), 0, b'5', ""));
    for (path, data) in files {
        let full = format!("{top}/{path}");
        // ustar: split at a slash into prefix and name, when one fits.
        let cut = full.match_indices('/').map(|(i, _)| i).find(|&i| i <= 155 && full.len() - i - 1 <= 100);
        if full.len() > 100 && let Some(cut) = cut {
            push(&mut out, &header(&full[cut + 1..], data.len(), b'0', &full[..cut]));
        } else if full.len() > 100 {
            let rec = format!(" path={full}\n");
            let mut n = rec.len() + 1;
            while format!("{n}{rec}").len() != n { n += 1 }
            let rec = format!("{n}{rec}");
            push(&mut out, &header("pax", rec.len(), b'x', ""));
            push(&mut out, rec.as_bytes());
            push(&mut out, &header("long", data.len(), b'0', ""));
        } else {
            push(&mut out, &header(&full, data.len(), b'0', ""));
        }
        push(&mut out, data);
    }
    out.extend_from_slice(&[0; 1024]);
    out
}

#[cfg(test)]
pub fn gzip(data: &[u8]) -> Vec<u8> {
    // A name field, so the header's optional parts are read past too.
    let mut out = vec![0x1f, 0x8b, 8, 8, 0, 0, 0, 0, 0, 3];
    out.extend_from_slice(b"repo.tar\0");
    out.extend(miniz_oxide::deflate::compress_to_vec(data, 6));
    out.extend_from_slice(&[0; 8]);
    out
}
