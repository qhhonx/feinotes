//! 下载为 → Markdown: DocxXML document model to CommonMark/GFM text; and a
//! Markdown file read and written as itself (`parse_file` / `write_file`).

use crate::model::*;
use std::path::{Path, PathBuf};

/// 导出 Markdown 设置.
#[derive(Default)]
pub struct Options<'a> {
    pub library_root: Option<&'a std::path::Path>,
    /// Titles of the document's child pages (what 子页面列表 shows).
    pub sub_pages: &'a [String],
    /// 所有内容: images are kept, local ones copied into this folder next to
    /// the .md file. None is 仅文本（不含图片和附件）.
    pub assets: Option<&'a str>,
    /// Writing a Markdown file as itself: no title line, and images and file
    /// links keep their sources (made relative to `base` when under it).
    pub in_place: bool,
    /// The folder of a Markdown file outside the library; None: one in the library.
    pub base: Option<&'a Path>,
    /// In place, outside the library: the file's name (no extension). The
    /// library's own images and attachments it uses are copied beside it,
    /// into `<name>.assets/`.
    pub name: Option<&'a str>,
    /// Copies already made: library file → path relative to the .md file.
    pub copied: Option<&'a std::collections::HashMap<PathBuf, String>>,
    /// The file as it is on disk: blocks that did not change are written
    /// back exactly as they are there (with the blank lines between them).
    pub original: Option<&'a str>,
}

pub struct Export {
    pub missing: usize,
    pub text: String,
    /// Local image files to copy: (source, path relative to the .md file).
    pub files: Vec<(PathBuf, String)>,
}

#[cfg(test)]
pub fn to_markdown(doc: &Document) -> String {
    export(doc, &Options { assets: Some("assets"), ..Default::default() }).text
}

pub fn export(doc: &Document, opts: &Options) -> Export {
    let mut w = Writer { opts, files: vec![], missing: 0 };
    let mut out = String::new();
    let title = doc.title.text.trim();
    if !title.is_empty() && !opts.in_place {
        out.push_str(&format!("# {}\n\n", escape(title)));
    }
    out.push_str(&w.blocks(&doc.blocks));
    let trimmed = out.trim_end().len();
    out.truncate(trimmed);
    out.push('\n');
    Export { text: out, files: w.files, missing: w.missing }
}

/// One top-level block as written, and the separator before it.
struct Piece {
    sep: &'static str,
    text: String,
}

struct Writer<'a> {
    missing: usize,
    opts: &'a Options<'a>,
    files: Vec<(PathBuf, String)>,
}

impl Writer<'_> {
    /// Blocks separated by blank lines; consecutive list items stay tight.
    fn blocks(&mut self, bs: &[Block]) -> String {
        self.pieces(bs).into_iter().flat_map(|p| [p.sep.to_string(), p.text]).collect()
    }

    /// `blocks`, one piece per block written (empty ones are left out).
    fn pieces(&mut self, bs: &[Block]) -> Vec<Piece> {
        let mut out: Vec<Piece> = vec![];
        let mut prev_list = false;
        let mut prev: Option<&BlockKind> = None;
        let mut seq = 0u32;
        for b in bs.iter() {
            let list = b.kind.is_list();
            // Different kinds of list are separate lists.
            // An item numbered on its own starts a new list (read back, a tight one would continue the last).
            let same = prev.is_some_and(|p| std::mem::discriminant(p) == std::mem::discriminant(&b.kind)) && !matches!(b.kind, BlockKind::Ordered { seq: Some(_) });
            prev = Some(&b.kind);
            match &b.kind {
                BlockKind::Ordered { seq: Some(n) } => seq = *n,
                BlockKind::Ordered { seq: None } => seq = if prev_list && seq > 0 { seq + 1 } else { 1 },
                _ => seq = 0,
            }
            let s = self.block(b, seq);
            if s.is_empty() && !list {
                continue;
            }
            let sep = if out.is_empty() { "" } else if list && prev_list && same { "\n" } else { "\n\n" };
            out.push(Piece { sep, text: s });
            prev_list = list;
        }
        out
    }

    fn block(&mut self, b: &Block, seq: u32) -> String {
        let text = inline(&b.text, &mut |f| self.file_link(f));
        let nested = |w: &mut Self, head: String| -> String {
            if b.children.is_empty() {
                return head;
            }
            format!("{head}\n{}", indent(&w.blocks(&b.children), "    "))
        };
        match &b.kind {
            BlockKind::Text => nested(self, text),
            BlockKind::Heading { level, .. } => nested(self, format!("{} {text}", "#".repeat((*level).clamp(1, 6) as usize))),
            BlockKind::Bullet => nested(self, format!("- {text}")),
            BlockKind::Ordered { .. } => nested(self, format!("{seq}. {text}")),
            BlockKind::Todo { done } => nested(self, format!("- [{}] {text}", if *done { "x" } else { " " })),
            BlockKind::Code { lang, .. } => {
                let lang = if lang.eq_ignore_ascii_case("plain text") { String::new() } else { lang.to_lowercase().replace(' ', "") };
                let fence = if b.text.text.contains("```") { "````" } else { "```" };
                format!("{fence}{lang}\n{}\n{fence}", b.text.text)
            }
            BlockKind::Quote => indent(&self.blocks(&b.children), "> "),
            BlockKind::Callout { emoji, .. } => {
                let body = self.blocks(&b.children);
                indent(&format!("{emoji} {body}"), "> ")
            }
            BlockKind::Divider => "---".into(),
            BlockKind::Image { src, caption, .. } if self.opts.in_place => {
                format!("![{}]({})", escape(caption.as_deref().unwrap_or_default()), link_target(&self.keep(src, true)))
            }
            BlockKind::Image { src, caption, .. } => match self.opts.assets {
                Some(dir) => {
                    let alt = caption.clone().unwrap_or_default();
                    format!("![{}]({})", escape(&alt), self.image(src, dir).replace(' ', "%20"))
                }
                None => String::new(),
            },
            BlockKind::Table(t) => table(t, &mut |f| self.file_link(f)),
            BlockKind::Grid | BlockKind::Column { .. } => self.blocks(&b.children),
            // An add-on block: Markdown has nothing for it.
            BlockKind::Countdown(_) => String::new(),
            BlockKind::File(f) => self.file_link(f).map(|url| format!("[{}]({url})", escape(&f.name))).unwrap_or_else(|| escape(&f.name)),
            BlockKind::SubPageList => {
                self.opts.sub_pages.iter().map(|t| format!("- {}", escape(&crate::model::display_title(t)))).collect::<Vec<_>>().join("\n")
            }
            BlockKind::Raw { xml, .. } => match crate::docxml::raw_summary(xml) {
                (text, Some(link)) if crate::paste::link_ok(&link) => format!("[{}]({})", escape(if text.is_empty() { &link } else { &text }), link.replace(' ', "%20")),
                (text, _) => escape(&text).replace('\n', "<br>"),
            },
        }
    }

    fn file_link(&mut self, file: &FileBlock) -> Option<String> {
        if self.opts.in_place {
            return (!file.src.is_empty()).then(|| link_target(&self.keep(&file.src, false)));
        }
        let dir = self.opts.assets.filter(|_| file.src_kind == "path")?;
        let link = self.image(&file.src, dir);
        (!link.is_empty()).then(|| link.replace(' ', "%20").replace(')', "%29").replace('(', "%28"))
    }

    /// A source as the file being written should say it: relative to the
    /// file's folder when under it; a library asset (`assets/…`) stays
    /// relative in the library and becomes absolute outside it.
    fn keep(&mut self, src: &str, image: bool) -> String {
        if src.contains("://") && !src.starts_with("file://") || src.starts_with("data:") {
            return src.to_string();
        }
        let Some(base) = self.opts.base else { return src.to_string() };
        let raw = Path::new(src.strip_prefix("file://").unwrap_or(src));
        let root = self.opts.library_root.map(Path::to_path_buf).unwrap_or_else(crate::store::Store::root_dir);
        // Relative sources left after `parse_file` are the library's (`assets/…`).
        let abs = if raw.is_absolute() { raw.to_path_buf() } else { root.join(raw) };
        if let Ok(rel) = abs.strip_prefix(base) {
            return rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
        }
        // The library's own image or attachment: a copy beside the file.
        let assets = root.join("assets");
        match self.opts.name.filter(|_| abs.starts_with(&assets) && abs.is_file()) {
            Some(name) => self.beside(&abs, base, name, image),
            None => abs.to_string_lossy().into_owned(),
        }
    }

    /// Where a library file goes beside a 本机文件: `<name>.assets/<name>-<n>.<ext>`
    /// for an image (pasted ones are named by a uuid), the attachment's own
    /// name otherwise; never over a file already there.
    fn beside(&mut self, src: &Path, base: &Path, name: &str, image: bool) -> String {
        if let Some(rel) = self.opts.copied.and_then(|c| c.get(src)).or_else(|| self.files.iter().find(|(p, _)| p == src).map(|(_, r)| r)) {
            return rel.clone();
        }
        let dir = format!("{}.assets", name.replace('/', "_"));
        let ext = src.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
        // A library image is `<uuid>` when pasted (no name), `<8 hex>-<name>` when it came from a file.
        let hex = |s: &str| s.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
        let own = match stem.split_at_checked(9) {
            Some((head, rest)) if head.ends_with('-') && hex(head) && !rest.is_empty() && !(stem.len() == 36 && hex(&stem)) => Some(rest.to_string()),
            _ if image => None,
            _ => Some(stem.clone()),
        };
        let taken = |rel: &str, files: &[(PathBuf, String)]| base.join(rel).exists() || files.iter().any(|(_, r)| r == rel);
        let mut n = 1;
        let rel = loop {
            let file = match (&own, n) {
                (None, _) => format!("{name}-{n}{ext}"),
                (Some(own), 1) => format!("{own}{ext}"),
                (Some(own), _) => format!("{own}-{n}{ext}"),
            };
            let rel = format!("{dir}/{file}");
            if !taken(&rel, &self.files) { break rel; }
            n += 1;
        };
        self.files.push((src.to_path_buf(), rel.clone()));
        rel
    }

    /// The link for an image: remote ones stay remote, local files get a
    /// copy in the assets folder (same name, numbered on clashes).
    fn image(&mut self, src: &str, dir: &str) -> String {
        if src.starts_with("http://") || src.starts_with("https://") {
            return src.to_string();
        }
        let Some(path) = crate::store::export_path_in(self.opts.library_root.unwrap_or(&crate::store::Store::root_dir()), src) else { self.missing += 1; return String::new() };
        if let Some((_, rel)) = self.files.iter().find(|(p, _)| *p == path) {
            return rel.clone();
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("image").to_string();
        let ext = path.extension().and_then(|s| s.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
        let mut rel = format!("{dir}/{stem}{ext}");
        let mut n = 1;
        while self.files.iter().any(|(_, r)| *r == rel) {
            n += 1;
            rel = format!("{dir}/{stem}-{n}{ext}");
        }
        self.files.push((path, rel.clone()));
        rel
    }
}

/// A link target that survives the parser: spaces and parentheses escaped.
fn link_target(s: &str) -> String {
    s.replace(' ', "%20").replace('(', "%28").replace(')', "%29")
}

/// Splits off a leading YAML front matter block (`---` … `---`), kept
/// verbatim and written back unchanged.
pub fn front_matter(text: &str) -> (&str, &str) {
    let Some(rest) = text.strip_prefix("---\n").or_else(|| text.strip_prefix("---\r\n")) else { return ("", text) };
    let mut at = text.len() - rest.len();
    for line in rest.split_inclusive('\n') {
        at += line.len();
        if line.trim_end() == "---" {
            let tail = &text[at..];
            let blank = tail.len() - tail.trim_start_matches(['\n', '\r']).len();
            return (&text[..at + blank], &text[at + blank..]);
        }
    }
    ("", text)
}

/// `%20` and friends in a link.
pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len()
            && let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// Relative and `file://` image sources made absolute against `base`.
pub fn resolve_images(blocks: &mut [Block], base: &Path) {
    for b in blocks.iter_mut() {
        resolve_images(&mut b.children, base);
        if let BlockKind::Table(t) = &mut b.kind {
            for cell in t.rows.iter_mut().flatten() {
                resolve_images(&mut cell.blocks, base);
            }
        }
        let BlockKind::Image { src, .. } = &mut b.kind else { continue };
        if src.contains("://") && !src.starts_with("file://") || src.starts_with("data:") {
            continue;
        }
        let path = match src.strip_prefix("file://") {
            Some(p) => PathBuf::from(percent_decode(p)),
            None => base.join(percent_decode(src)),
        };
        *src = path.to_string_lossy().to_string();
    }
}

/// A Markdown file as a page: `title` is its name (a Markdown file has no
/// title of its own), the body every line after the front matter. `base`:
/// the folder of a file outside the library, which relative images resolve
/// against.
pub fn parse_file(text: &str, title: &str, base: Option<&Path>) -> Document {
    let text = text.trim_start_matches('\u{feff}');
    let (_, body) = front_matter(text);
    let mut blocks = crate::paste::markdown_to_blocks(body, crate::paste::Off { file: true, alt: true, ..Default::default() });
    if let Some(dir) = base {
        resolve_images(&mut blocks, dir);
    }
    if blocks.is_empty() {
        blocks.push(Block::text(""));
    }
    Document { title: RichText::plain(title), icon: None, cover: None, blocks }
}

/// `doc` written back as a Markdown file, after the file's `front` matter.
pub fn write_file(doc: &Document, front: &str, base: Option<&Path>) -> String {
    write_file_with(doc, front, &Options { in_place: true, base, ..Default::default() }).text
}

/// `write_file` with the copies to make beside a 本机文件 (`Export::files`,
/// relative to the file's folder).
pub fn write_file_with(doc: &Document, front: &str, opts: &Options) -> Export {
    let opts = Options { in_place: true, library_root: opts.library_root, base: opts.base, name: opts.name, copied: opts.copied, original: opts.original, ..Default::default() };
    let mut w = Writer { opts: &opts, files: vec![], missing: 0 };
    let pieces = w.pieces(&doc.blocks);
    let canonical = joined(&pieces);
    // Kept lines must still read back as the document; if a kept spacing
    // changes what a neighbour means, try again more strictly, then give up
    // on keeping anything.
    let body = match opts.original {
        Some(original) => [false, true].into_iter().map(|strict| preserve(original, &pieces, &opts, strict))
            .find(|body| reads_as(body, &canonical, &opts)).unwrap_or(canonical),
        None => canonical,
    };
    // A file opening with a `---` line would read back as front matter.
    let misread = front.is_empty() && !front_matter(&body).0.is_empty();
    let body = match body.strip_prefix("---\n").filter(|_| misread) { Some(rest) => format!("***\n{rest}"), None => body };
    let text = if body.trim().is_empty() { front.to_string() } else { format!("{front}{body}") };
    Export { text, files: w.files, missing: 0 }
}

/// The body of `original` (a whole file) rewritten to `pieces`: a piece
/// that matches one of the file's blocks (same text when written) is that
/// block's lines as they are; between two such neighbours the file's own
/// blank lines stay too. Only changed blocks take the written form.
fn preserve(original: &str, pieces: &[Piece], opts: &Options, strict: bool) -> String {
    let (_, body) = front_matter(original.trim_start_matches('\u{feff}'));
    let crlf = body.contains("\r\n");
    let norm = body.replace("\r\n", "\n");
    let (mut blocks, spans) = crate::paste::markdown_to_blocks_with_lines(&norm, crate::paste::Off { file: true, alt: true, ..Default::default() });
    if blocks.len() != spans.len() { return joined(pieces); }
    if let Some(dir) = opts.base { resolve_images(&mut blocks, dir); }
    // The file's blocks as written now, each with the lines it came from.
    let mut old: Vec<(String, (usize, usize))> = vec![];
    {
        let mut w = Writer { opts, files: vec![], missing: 0 };
        let mut prev_list = false;
        let mut seq = 0u32;
        // `pieces` drops empty blocks; walk the same way to keep each span.
        for (b, span) in blocks.iter().zip(&spans) {
            let list = b.kind.is_list();
            match &b.kind {
                BlockKind::Ordered { seq: Some(n) } => seq = *n,
                BlockKind::Ordered { seq: None } => seq = if prev_list && seq > 0 { seq + 1 } else { 1 },
                _ => seq = 0,
            }
            let s = w.block(b, seq);
            if s.is_empty() && !list { continue; }
            old.push((s, *span));
            prev_list = list;
        }
    }
    let lines: Vec<&str> = norm.split('\n').collect();
    let text_of = |(a, b): (usize, usize)| lines[a..b].join("\n");
    // Lines `[a, b)` as the text between two blocks (each line plus its newline).
    let gap = |a: usize, b: usize| -> String { std::iter::once("\n".to_string()).chain(lines[a..b].iter().map(|l| format!("{l}\n"))).collect() };
    let matched = match_pieces(&old.iter().map(|o| o.0.as_str()).collect::<Vec<_>>(), &pieces.iter().map(|p| p.text.as_str()).collect::<Vec<_>>());
    // Where each piece sits in the file: its block, or for an edited one the
    // block it replaced (the next unmatched one), so the spacing around an
    // edit stays the file's too.
    let used: std::collections::HashSet<usize> = matched.iter().flatten().copied().collect();
    let mut at: Vec<Option<usize>> = vec![None; pieces.len()];
    for j in 0..pieces.len() {
        at[j] = matched[j].or_else(|| {
            let next = if j == 0 { 0 } else { at[j - 1]? + 1 };
            (next < old.len() && !used.contains(&next)).then_some(next)
        });
    }
    let mut out = String::new();
    for (j, p) in pieces.iter().enumerate() {
        let m = matched[j];
        match (j, at[j]) {
            (0, Some(0)) => out.push_str(&lines[..old[0].1.0].iter().map(|l| format!("{l}\n")).collect::<String>()),
            (0, _) => {}
            (_, Some(k)) if k > 0 && at[j - 1] == Some(k - 1) => {
                // The file's spacing, when it means the same: tight (one
                // line break) where the written form is tight, loose (a blank
                // line or more) where it is loose. Otherwise `- a` and a new
                // paragraph `b` would join, or one list read back as two.
                let g = gap(old[k - 1].1.1, old[k].1.0);
                let (lines, tight) = (g.matches('\n').count(), p.sep == "\n");
                let edited = m.is_none() || matched[j - 1].is_none();
                let keep = if strict { (lines == 1) == tight } else { !edited || (!tight && lines >= 2) };
                out.push_str(if keep { &g } else { p.sep });
            }
            _ => out.push_str(p.sep),
        }
        match m {
            Some(k) => out.push_str(&text_of(old[k].1)),
            None => out.push_str(&p.text),
        }
    }
    // The file's own ending when its last block is still last.
    match (at.last().copied().flatten(), old.last()) {
        (Some(k), Some((_, (_, end)))) if k + 1 == old.len() => {
            let tail = lines[*end..].join("\n");
            out.push('\n');
            out.push_str(&tail);
            if !out.ends_with('\n') { out.push('\n'); }
        }
        _ if out.is_empty() => {}
        _ => {
            let n = out.trim_end().len();
            out.truncate(n);
            out.push('\n');
        }
    }
    if crlf { out = out.replace('\n', "\r\n"); }
    out
}

/// Pieces joined as written, ending in one line break.
fn joined(pieces: &[Piece]) -> String {
    pieces.iter().flat_map(|p| [p.sep, p.text.as_str()]).collect::<String>().trim_end().to_string() + "\n"
}

/// Whether `body` reads back as the blocks that were written as `canonical`.
fn reads_as(body: &str, canonical: &str, opts: &Options) -> bool {
    let mut blocks = crate::paste::markdown_to_blocks(body, crate::paste::Off { file: true, alt: true, ..Default::default() });
    if let Some(dir) = opts.base { resolve_images(&mut blocks, dir); }
    let mut w = Writer { opts, files: vec![], missing: 0 };
    joined(&w.pieces(&blocks)) == canonical
}

/// For each new piece, the old one it is (longest common subsequence of
/// equal texts). The unchanged start and end are matched first, so a
/// typical edit leaves a small middle; a huge middle is left unmatched.
fn match_pieces(old: &[&str], new: &[&str]) -> Vec<Option<usize>> {
    let mut out = vec![None; new.len()];
    let mut a = 0;
    while a < old.len() && a < new.len() && old[a] == new[a] { out[a] = Some(a); a += 1; }
    let (mut eo, mut en) = (old.len(), new.len());
    while eo > a && en > a && old[eo - 1] == new[en - 1] { eo -= 1; en -= 1; out[en] = Some(eo); }
    let (o, n) = (&old[a..eo], &new[a..en]);
    if o.is_empty() || n.is_empty() || o.len() * n.len() > 4_000_000 { return out; }
    // lcs[i][j]: the longest common subsequence of o[i..] and n[j..].
    let w = n.len() + 1;
    let mut lcs = vec![0u32; (o.len() + 1) * w];
    for i in (0..o.len()).rev() {
        for j in (0..n.len()).rev() {
            lcs[i * w + j] = if o[i] == n[j] { lcs[(i + 1) * w + j + 1] + 1 } else { lcs[(i + 1) * w + j].max(lcs[i * w + j + 1]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < o.len() && j < n.len() {
        if o[i] == n[j] { out[a + j] = Some(a + i); i += 1; j += 1; }
        else if lcs[(i + 1) * w + j] >= lcs[i * w + j + 1] { i += 1; }
        else { j += 1; }
    }
    out
}

fn table(t: &Table, files: &mut impl FnMut(&FileBlock) -> Option<String>) -> String {
    let cols = t.rows.iter().map(|r| r.len()).max().unwrap_or(0).max(1);
    let mut cell = |c: &TableCell| {
        let parts: Vec<String> = c.blocks.iter().map(|b| inline(&b.text, files)).filter(|s| !s.is_empty()).collect();
        parts.join("<br>").replace('|', "\\|")
    };
    let mut row = |r: &Vec<TableCell>| {
        let mut cells: Vec<String> = r.iter().map(&mut cell).collect();
        cells.resize(cols, String::new());
        format!("| {} |", cells.join(" | "))
    };
    let mut lines = vec![];
    let mut rows = t.rows.iter();
    match rows.next() {
        Some(r) => lines.push(row(r)),
        None => lines.push(row(&vec![])),
    }
    lines.push(format!("|{}", " --- |".repeat(cols)));
    lines.extend(rows.map(row));
    lines.join("\n")
}

fn indent(s: &str, prefix: &str) -> String {
    s.lines()
        .map(|l| if l.is_empty() && prefix.trim().is_empty() { String::new() } else { format!("{prefix}{l}").trim_end().to_string() })
        .collect::<Vec<_>>()
        .join("\n")
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '*' | '_' | '`' | '[' | ']' | '<' | '~') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[derive(PartialEq, Clone, Default)]
struct Md {
    bold: bool,
    italic: bool,
    strike: bool,
    underline: bool,
    code: bool,
    link: Option<String>,
}

fn inline(t: &RichText, files: &mut impl FnMut(&FileBlock) -> Option<String>) -> String {
    // Merge runs that only differ in marks Markdown can't express (colors).
    let mut runs: Vec<(Md, String)> = vec![];
    for (r, m) in t.spans() {
        let s = match m.inline.as_ref().and_then(Inline::equation) {
            Some(src) => t.text[r].chars().map(|_| format!("${src}$")).collect(),
            None => match &m.inline {
                Some(Inline { kind: InlineKind::File(f), .. }) => t.text[r].chars().map(|_| f.name.clone()).collect(),
                Some(obj) => {
                    // An 打开超链接 button exports as a link.
                    let (text, link) = crate::docxml::raw_summary(&obj.xml);
                    let s = match link.filter(|l| matches!(obj.kind, InlineKind::Button(_)) && crate::paste::link_ok(l)) {
                        Some(l) => format!("[{text}]({l})"),
                        None => text,
                    };
                    t.text[r].chars().map(|_| s.clone()).collect()
                }
                None => t.text[r].to_string(),
            },
        };
        let file_link = m.inline.as_ref().and_then(|obj| match &obj.kind { InlineKind::File(f) => files(f), _ => None });
        let md = Md {
            bold: m.bold,
            italic: m.italic,
            strike: m.strike,
            underline: m.underline,
            code: m.code,
            link: file_link.or_else(|| m.link.clone().filter(|url| crate::paste::link_ok(url))),
        };
        match runs.last_mut() {
            Some((last, text)) if *last == md => text.push_str(&s),
            _ => runs.push((md, s)),
        }
    }
    let mut out = String::new();
    for (m, s) in runs {
        if s.is_empty() {
            continue;
        }
        // Emphasis markers must hug the text: keep outer spaces outside.
        let body = s.trim();
        let lead = &s[..s.len() - s.trim_start().len()];
        let trail = &s[s.trim_end().len()..];
        if body.is_empty() {
            out.push_str(&s);
            continue;
        }
        let mut x = if m.code {
            let tick = if body.contains('`') { "``" } else { "`" };
            format!("{tick}{body}{tick}")
        } else {
            escape(body)
        };
        if m.underline {
            x = format!("<u>{x}</u>");
        }
        if m.strike {
            x = format!("~~{x}~~");
        }
        if m.italic {
            x = format!("*{x}*");
        }
        if m.bold {
            x = format!("**{x}**");
        }
        if let Some(url) = &m.link {
            x = format!("[{x}]({})", url.replace(' ', "%20").replace(')', "%29"));
        }
        out.push_str(lead);
        out.push_str(&x);
        out.push_str(trail);
    }
    out.replace('\n', "<br>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_attachments_export_links_and_copies_including_tables() {
        let root = std::env::temp_dir().join(format!("quiet-export-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("assets/report (1).pdf"), b"pdf").unwrap();
        let file = FileBlock { src_kind: "path".into(), src: "assets/report (1).pdf".into(), name: "Report [1]".into(), view: FileView::Card };
        let mut text = RichText::plain("before ");
        text.push("\u{fffc}", Marks { inline: Some(crate::docxml::to_inline(InlineKind::File(file))), ..Default::default() });
        text.push(" after", Marks::default());
        let mut table = Table { rows: vec![vec![TableCell::empty()]], ..Default::default() };
        table.rows[0][0].blocks = vec![Block::new(BlockKind::Text, text.clone())];
        let doc = Document { blocks: vec![Block::new(BlockKind::Text, text), Block::new(BlockKind::Table(table), RichText::default())], ..Default::default() };
        let out = export(&doc, &Options { assets: Some("out.assets"), library_root: Some(&root), ..Default::default() });
        assert_eq!(out.files.len(), 1);
        assert_eq!(out.missing, 0);
        assert_eq!(out.text.matches("[Report \\[1\\]](out.assets/report%20%281%29.pdf)").count(), 2);
        let plain = export(&doc, &Options { library_root: Some(&root), ..Default::default() });
        assert!(plain.files.is_empty());
        assert!(!plain.text.contains("out.assets"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn exports_common_blocks() {
        let mut bold = RichText::plain("a ");
        bold.push("bold ", Marks { bold: true, ..Default::default() });
        bold.push("link", Marks { link: Some("https://x.y".into()), ..Default::default() });
        let doc = Document {
            title: RichText::plain("标题"),
            blocks: vec![
                Block::new(BlockKind::Heading { level: 2, seq_auto: false }, RichText::plain("小节")),
                Block::new(BlockKind::Text, bold),
                Block::new(BlockKind::Ordered { seq: None }, RichText::plain("one"))
                    .with_children(vec![Block::new(BlockKind::Bullet, RichText::plain("sub"))]),
                Block::new(BlockKind::Ordered { seq: None }, RichText::plain("two")),
                Block::new(BlockKind::Todo { done: true }, RichText::plain("done")),
                Block::new(BlockKind::Code { lang: "Rust".into(), caption: None, wrap: false }, RichText::plain("fn main() {}")),
                Block::new(BlockKind::Quote, RichText::default()).with_children(vec![Block::text("q")]),
                Block::new(BlockKind::Divider, RichText::default()),
            ],
            ..Default::default()
        };
        let md = to_markdown(&doc);
        assert_eq!(
            md,
            "# 标题\n\n## 小节\n\na **bold** [link](https://x.y)\n\n1. one\n    - sub\n2. two\n\n- [x] done\n\n```rust\nfn main() {}\n```\n\n> q\n\n---\n"
        );
    }

    /// What the editor writes back reads back the same, front matter and
    /// relative images included.
    #[test]
    fn markdown_files_round_trip() {
        let src = "---\ntitle: x\n---\n\n# Heading\n\nSome **bold**, *italic*, `code` and [a link](https://x.y).\n\n- one\n    - nested\n- two\n\n1. first\n2. second\n\n- [ ] todo\n- [x] done\n\n> quoted\n\n```rust\nfn main() {}\n```\n\n![alt](img/a%20b.png)\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\n---\n";
        let base = Path::new(if cfg!(windows) { "C:/notes" } else { "/notes" });
        let doc = parse_file(src, "page", Some(base));
        assert_eq!(doc.title.text, "page");
        assert!(doc.blocks.iter().any(|b| matches!(&b.kind, BlockKind::Image { src, .. } if Path::new(src) == base.join("img").join("a b.png"))));
        let out = write_file(&doc, front_matter(src).0, Some(base));
        assert_eq!(out, src);
        assert_eq!(write_file(&parse_file(&out, "page", Some(base)), front_matter(&out).0, Some(base)), out);
    }

    fn preserved(original: &str, edit: impl FnOnce(&mut Document)) -> String {
        let base = Path::new(if cfg!(windows) { "C:/notes" } else { "/notes" });
        let mut doc = parse_file(original, "t", Some(base));
        edit(&mut doc);
        let out = write_file_with(&doc, front_matter(original).0, &Options { base: Some(base), original: Some(original), ..Default::default() }).text;
        // Whatever was kept, the file reads back as the document.
        let canon = |d: &Document| write_file(d, "", Some(base));
        assert_eq!(canon(&parse_file(&out, "t", Some(base))), canon(&doc), "{out}");
        out
    }

    /// Only what changed is rewritten: other blocks keep their own syntax
    /// and the blank lines around them.
    #[test]
    fn saving_keeps_untouched_lines() {
        let src = "---\na: 1\n---\n* apple\n+ pear\n\n\nTitle\n\nfirst<br>second\n__bold__ text\n\n1. one\n1. two\n\n```\ncode\n```\n";
        // Nothing changed: byte for byte.
        assert_eq!(preserved(src, |_| {}), src);
        // One paragraph edited: only its line changes.
        let out = preserved(src, |d| {
            let b = d.blocks.iter_mut().find(|b| b.text.text == "Title").unwrap();
            b.text = RichText::plain("New title");
        });
        assert_eq!(out, src.replace("Title", "New title"));
        // A block added at the end and one removed in the middle.
        let out = preserved(src, |d| {
            d.blocks.retain(|b| b.text.text != "Title");
            d.blocks.push(Block::text("added"));
        });
        assert_eq!(out, "---\na: 1\n---\n* apple\n+ pear\n\nfirst<br>second\n__bold__ text\n\n1. one\n1. two\n\n```\ncode\n```\n\nadded\n");
        // Moving a block keeps its text.
        let out = preserved(src, |d| {
            let i = d.blocks.iter().position(|b| b.text.text == "Title").unwrap();
            let b = d.blocks.remove(i);
            d.blocks.insert(0, b);
        });
        assert!(out.starts_with("---\na: 1\n---\nTitle\n\n* apple\n+ pear\n"), "{out}");
        assert!(out.contains("first<br>second\n__bold__ text"));
        // A list item turned into a paragraph gets its blank line.
        let out = preserved("- a\n- b\n", |d| d.blocks[1].kind = BlockKind::Text);
        assert_eq!(out, "- a\n\nb\n");
        // CRLF files stay CRLF.
        let crlf = src.replace('\n', "\r\n");
        assert_eq!(preserved(&crlf, |_| {}), crlf);
    }

    /// Random files and random edits: the saved file always reads back as
    /// the edited document (`preserved` checks), and an untouched one is unchanged.
    #[test]
    fn random_edits_keep_files_readable() {
        let parts = ["* star", "- dash", "+ plus", "1. one", "3) three", "- [ ] todo", "- [x] done", "# Head", "Setext\n===", "para <b>html</b>", "__under__ bold",
            "> quote\n> more", "```\nfence\n\nblank\n```", "---", "| a | b |\n| - | - |\n| 1 | 2 |", "![alt](x.png)", "text\nlazy"];
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut rand = |n: usize| { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; (seed % n as u64) as usize };
        for _ in 0..400 {
            let mut src = String::new();
            for _ in 0..1 + rand(8) {
                src.push_str(parts[rand(parts.len())]);
                src.push_str(["\n", "\n\n", "\n\n\n"][rand(3)]);
            }
            assert_eq!(preserved(&src, |_| {}).trim_end(), src.trim_end(), "untouched");
            preserved(&src, |d| {
                for _ in 0..1 + rand(3) {
                    let n = d.blocks.len();
                    match rand(4) {
                        0 if n > 1 => { d.blocks.remove(rand(n)); }
                        1 => d.blocks.insert(rand(n + 1), Block::text("new")),
                        2 => { let i = rand(n); if d.blocks[i].kind.has_text() { d.blocks[i].text = RichText::plain("edited"); } }
                        _ => { let i = rand(n); if d.blocks[i].kind.has_text() && !matches!(d.blocks[i].kind, BlockKind::Code { .. }) && d.blocks[i].children.is_empty() && d.blocks[i].text.text.trim() == d.blocks[i].text.text { d.blocks[i].kind = [BlockKind::Text, BlockKind::Bullet, BlockKind::Ordered { seq: None }][rand(3)].clone(); } }
                    }
                }
            });
        }
    }

    #[test]
    fn front_matter_needs_a_closing_line() {
        assert_eq!(front_matter("---\na: 1\n---\n\nbody"), ("---\na: 1\n---\n\n", "body"));
        assert_eq!(front_matter("---\nno end"), ("", "---\nno end"));
        assert_eq!(front_matter("body"), ("", "body"));
    }

    #[test]
    fn exports_tables() {
        let cell = |s: &str| TableCell { blocks: vec![Block::text(s)], ..TableCell::empty() };
        let t = Table { header_rows: 1, rows: vec![vec![cell("a"), cell("b|c")], vec![cell("1")]], ..Default::default() };
        assert_eq!(table(&t, &mut |_| None), "| a | b\\|c |\n| --- | --- |\n| 1 |  |");
    }
}
