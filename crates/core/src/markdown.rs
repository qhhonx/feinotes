//! 下载为 → Markdown: DocxXML document model to CommonMark/GFM text; and a
//! Markdown file read and written as itself (`parse_file` / `write_file`).

use crate::vfs::{PathExt as _, KindExt as _};
use crate::model::*;
use std::path::{Path, PathBuf};

/// 导出 Markdown 设置.
#[derive(Default)]
pub struct Options<'a> {
    pub library_dir: Option<&'a std::path::Path>,
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
    /// With `in_place` and no `base`: local sources as absolute paths (复制为 Markdown).
    pub absolute: bool,
    /// The file's folder and its vault, when its `![[picture.png]]` embeds are
    /// read as pictures (`wiki_images`): `original` is read the same way, so an
    /// untouched embed is matched and written back as `![[…]]`.
    pub wiki: Option<(&'a Path, Option<&'a Path>)>,
    /// The folder of a library Markdown file written in place (`base` is None
    /// there): its pictures and files are written relative to it, as `base`'s
    /// are, and the original's relative sources read from it (`resolve_present`).
    pub dir: Option<&'a Path>,
    /// Writing the file itself (not a copy to paste): what a reader would
    /// lose stays (`BLANK` for an empty paragraph, `hold_end`).
    pub file: bool,
}

pub struct Export {
    pub missing: usize,
    pub text: String,
    /// Local image files to copy: (source, path relative to the .md file).
    pub files: Vec<(PathBuf, String)>,
    /// Files made for the export (a 画板's SVG and `.excalidraw`): (path
    /// relative to the .md file, contents).
    pub generated: Vec<(String, String)>,
}

#[cfg(test)]
pub fn to_markdown(doc: &Document) -> String {
    export(doc, &Options { assets: Some("assets"), ..Default::default() }).text
}

pub fn export(doc: &Document, opts: &Options) -> Export {
    let mut w = Writer { opts, files: vec![], generated: vec![], missing: 0 };
    let mut out = String::new();
    let title = doc.title.text.trim();
    if !title.is_empty() && !opts.in_place {
        out.push_str(&format!("# {}\n\n", escape(title)));
    }
    out.push_str(&w.blocks(&doc.blocks));
    let trimmed = out.trim_end().len();
    out.truncate(trimmed);
    out.push('\n');
    Export { text: out, files: w.files, generated: w.generated, missing: w.missing }
}

/// What of `blocks` a Markdown file can keep, for pasting into one: 高亮块
/// and 分栏 give way to what they hold; attachments, 画板, 数据表, 倒计时,
/// 子页面列表 and unknown blocks are left out, and so are inline 提醒,
/// buttons and file cards.
pub fn keepable(blocks: Vec<Block>) -> Vec<Block> {
    let mut out = vec![];
    for mut b in blocks {
        match &mut b.kind {
            BlockKind::Callout { .. } | BlockKind::Grid | BlockKind::Column { .. } => {
                if !b.text.text.is_empty() { out.push(Block { kind: BlockKind::Text, children: vec![], ..b.clone() }); }
                out.extend(keepable(b.children));
                continue;
            }
            BlockKind::File(_) | BlockKind::Board(_) | BlockKind::DataTable(_) | BlockKind::Countdown(_) | BlockKind::SubPageList | BlockKind::Raw { .. } => continue,
            BlockKind::Table(t) => for c in t.rows.iter_mut().flatten() { c.blocks = keepable(std::mem::take(&mut c.blocks)); },
            _ => {}
        }
        let objects: Vec<std::ops::Range<usize>> = b.text.spans()
            .filter(|(_, m)| m.inline.as_ref().is_some_and(|o| matches!(o.kind, InlineKind::Reminder(_) | InlineKind::Button(_) | InlineKind::File(_))))
            .map(|(r, _)| r).collect();
        for r in objects.into_iter().rev() { b.text.replace(r, &RichText::default()); }
        b.children = keepable(std::mem::take(&mut b.children));
        out.push(b);
    }
    out
}

/// 复制为 Markdown (#60): `blocks` (and the title, for a whole document) as
/// Markdown text. Equations stay LaTeX; local images and attachments are
/// linked by their absolute paths, nothing is copied.
pub fn clipboard(title: Option<&str>, blocks: &[Block], library_dir: &Path) -> String {
    let opts = Options { in_place: true, absolute: true, library_dir: Some(library_dir), ..Default::default() };
    let mut w = Writer { opts: &opts, files: vec![], generated: vec![], missing: 0 };
    let mut out = title.map(str::trim).filter(|t| !t.is_empty()).map(|t| format!("# {}\n\n", escape(t))).unwrap_or_default();
    out.push_str(&w.blocks(blocks));
    let trimmed = out.trim_end().len();
    out.truncate(trimmed);
    out.push('\n');
    out
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
    generated: Vec<(String, String)>,
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
        let text = if !self.opts.file { text } else if b.kind == BlockKind::Text && b.text.is_empty() { BLANK.into() } else if text == BLANK { format!("\\{BLANK}") } else { hold_end(text) };
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
            // A width Markdown has no room for: an `<img>` line, as GitHub writes one (and as it is read).
            BlockKind::Image { src, caption, width: Some(w), .. } if self.opts.in_place => {
                let attr = |s: &str| s.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;");
                let alt = caption.as_deref().filter(|c| !c.is_empty()).map(|c| format!(" alt=\"{}\"", attr(c))).unwrap_or_default();
                format!("<img src=\"{}\"{alt} width=\"{}\">", attr(&self.keep(src, true)), w.round())
            }
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
            // A 画板 with 所有内容: its SVG, linking to its `.excalidraw`;
            // otherwise its words.
            BlockKind::Board(bd) => match self.opts.assets.filter(|_| !self.opts.in_place) {
                Some(dir) => {
                    let n = self.generated.len() / 2 + 1;
                    let (svg, file) = (format!("{dir}/board-{n}.svg"), format!("{dir}/board-{n}.excalidraw"));
                    let root = self.opts.library_dir.map(Path::to_path_buf).unwrap_or_else(crate::store::Store::active_dir);
                    self.generated.push((svg.clone(), bd.svg_in(&root)));
                    self.generated.push((file.clone(), bd.file_json()));
                    format!("[![]({})]({})", link_target(&svg), link_target(&file))
                }
                None => bd.plain_text().lines().map(escape).collect::<Vec<_>>().join("<br>"),
            },
            // A 数据表: the shown view as a Markdown table.
            BlockKind::DataTable(t) => crate::table::block_markdown(&self.opts.library_dir.map(Path::to_path_buf).unwrap_or_else(crate::store::Store::active_dir), t),
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
    /// file's folder (`base`, or `dir` in the library), with `/` and `../` as
    /// needed, so it still works when the folder moves; a library asset used by
    /// a file outside the library is copied beside it. Absolute only where no
    /// relative path reaches (another drive) or when asked to (`absolute`).
    fn keep(&mut self, src: &str, image: bool) -> String {
        if src.contains("://") && !src.starts_with("file://") || src.starts_with("data:") {
            return src.to_string();
        }
        if self.opts.base.is_none() && self.opts.dir.is_none() && !self.opts.absolute { return src.to_string() }
        let raw = Path::new(src.strip_prefix("file://").unwrap_or(src));
        let root = self.opts.library_dir.map(Path::to_path_buf).unwrap_or_else(crate::store::Store::active_dir);
        // Relative sources left after `parse_file` are the library's (`.assets/…`).
        let abs = if raw.fs_is_absolute() { raw.to_path_buf() } else { root.join(raw) };
        let Some(base) = self.opts.base.or(self.opts.dir) else { return abs.to_string_lossy().into_owned() };
        if let Ok(rel) = abs.strip_prefix(base) {
            return rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
        }
        // The library's own image or attachment: a copy beside the file.
        let assets = crate::store::Store::assets_in(&root);
        match self.opts.name.filter(|_| self.opts.base.is_some() && abs.starts_with(&assets) && abs.fs_is_file()) {
            Some(name) => self.beside(&abs, base, name, image),
            None => crate::paths::portable(&abs, base).unwrap_or_else(|| abs.to_string_lossy().into_owned()),
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
        let taken = |rel: &str, files: &[(PathBuf, String)]| base.join(rel).fs_exists() || files.iter().any(|(_, r)| r == rel);
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
    /// copy in the assets folder (same name, numbered on clashes). An inline
    /// base64 image that couldn't be written out (too big, not a picture)
    /// stays inline, so saving never drops it (#133).
    fn image(&mut self, src: &str, dir: &str) -> String {
        if src.starts_with("http://") || src.starts_with("https://") || src.starts_with("data:image/") {
            return src.to_string();
        }
        let Some(path) = crate::store::export_path_in(self.opts.library_dir.unwrap_or(&crate::store::Store::active_dir()), src) else { self.missing += 1; return String::new() };
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
/// `b` (or a block in it) is a picture whose relative source is not beside the
/// file (`resolve_present` left it) but is the library's own.
fn stale(b: &Block, opts: &Options) -> bool {
    let root = opts.library_dir.map(Path::to_path_buf).unwrap_or_else(crate::store::Store::active_dir);
    let here = |src: &str| !src.contains("://") && !src.starts_with("data:") && !Path::new(src).fs_is_absolute() && crate::paths::resolve(&percent_decode(src), &root).fs_is_file();
    matches!(&b.kind, BlockKind::Image { src, .. } if here(src)) || b.children.iter().any(|c| stale(c, opts))
        || matches!(&b.kind, BlockKind::Table(t) if t.rows.iter().flatten().any(|c| c.blocks.iter().any(|c| stale(c, opts))))
}

/// `resolve_images` for a library Markdown file: a relative source is read
/// from the file's folder when the file is there; otherwise it stays as it
/// was, the library's own (`.assets/…`, as older builds wrote it).
pub fn resolve_present(blocks: &mut [Block], dir: &Path) {
    for b in blocks.iter_mut() {
        resolve_present(&mut b.children, dir);
        if let BlockKind::Table(t) = &mut b.kind {
            for cell in t.rows.iter_mut().flatten() { resolve_present(&mut cell.blocks, dir); }
        }
        let BlockKind::Image { src, .. } = &mut b.kind else { continue };
        if src.contains("://") || src.starts_with("data:") || Path::new(src.as_str()).fs_is_absolute() { continue }
        let path = crate::paths::lexical(&dir.join(percent_decode(src)));
        if path.fs_is_file() { *src = path.to_string_lossy().into_owned(); }
    }
}

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

/// When front matter (`front_matter`'s first part) says its page was made
/// and last changed, in unix seconds, for an import to keep. Keys are read
/// in any case, with `-`, `_` and spaces ignored, the first that parses
/// winning: made from `created`, `createdAt`, `date created`,
/// `creation_date`, then `date` (Obsidian, Jekyll, Hugo); changed from
/// `updated`, `modified`, `lastmod`, `updatedAt`, `last_modified`,
/// `last_modified_at`, `date modified`, `date updated`.
pub fn front_matter_times(front: &str) -> (Option<u64>, Option<u64>) {
    const MADE: [&str; 5] = ["created", "createdat", "datecreated", "creationdate", "date"];
    const CHANGED: [&str; 8] = ["updated", "modified", "lastmod", "updatedat", "lastmodified", "lastmodifiedat", "datemodified", "dateupdated"];
    // Top-level `key: value` lines; a value in quotes may hold ` #`.
    let fields: Vec<(String, &str)> = front.lines().skip(1).filter(|l| !l.starts_with([' ', '\t', '#', '-'])).filter_map(|l| {
        let (k, v) = l.split_once(':')?;
        let key: String = k.trim().trim_matches(['"', '\'']).chars().filter(|c| !matches!(c, '-' | '_' | ' ')).flat_map(char::to_lowercase).collect();
        let v = v.trim();
        let v = if v.starts_with(['"', '\'']) { v } else { v.split(" #").next().unwrap_or(v).trim() };
        Some((key, v))
    }).collect();
    let first = |keys: &[&str]| keys.iter().find_map(|k| fields.iter().filter(|(f, _)| f == k).find_map(|(_, v)| crate::time::parse_date(v)));
    (first(&MADE), first(&CHANGED))
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

/// The picture formats the editor shows (`src/editor/image.rs`).
const PICTURES: [&str; 6] = ["jpeg", "jpg", "png", "gif", "webp", "bmp"];

/// On import: a paragraph that is just `![[name.png]]` (or `![[name.png|300]]`)
/// — how Obsidian and its exports embed a picture — as that picture. Looked up
/// the way Obsidian does: beside the page, in the vault's attachment folder,
/// at the vault's top, then anywhere in the vault by name, the file with the
/// shortest path (several that short: none, it can't be told which).
/// The vault is the nearest folder above the page holding `.obsidian`, else
/// `root` (the folder being imported). What can't be found or shown
/// (`![[page#heading]]`, a missing file, an svg) stays the text it was.
pub fn wiki_images(blocks: &mut [Block], page_dir: &Path, root: Option<&Path>) {
    let vault = page_dir.ancestors().find(|d| d.join(".obsidian").fs_is_dir()).or(root);
    let attachments = vault.and_then(|v| {
        let app: serde_json::Value = serde_json::from_slice(&crate::vfs::read(v.join(".obsidian/app.json")).ok()?).ok()?;
        let f = app.get("attachmentFolderPath")?.as_str()?.trim_end_matches('/');
        Some(match f.strip_prefix("./") { Some(r) => page_dir.join(r), None if f == "." => page_dir.to_path_buf(), None => v.join(f.trim_start_matches('/')) })
    });
    let mut by_name: Option<std::sync::Arc<Names>> = None;
    wiki_walk(blocks, &mut |name| {
        let p = Path::new(name);
        let found = [Some(page_dir), attachments.as_deref(), vault].into_iter().flatten().map(|d| d.join(p)).find(|p| p.fs_is_file());
        found.or_else(|| {
            let v = vault?;
            let key = p.file_name()?.to_string_lossy().to_lowercase();
            let names = by_name.get_or_insert_with(|| vault_names(v));
            // A name the folders a person sees have nothing for is looked for
            // in the hidden ones (#227); one they do have stays decided by
            // them, so a copy in `.cache` cannot make it ambiguous.
            match names.get(&key) {
                Some(all) => shortest(all),
                None => shortest(hidden_names(v).get(&key)?),
            }
        })
    });
}

/// Of files with one name, the one with the fewest folders above it; none on a tie.
fn shortest(all: &[PathBuf]) -> Option<PathBuf> {
    let depth = |p: &PathBuf| p.components().count();
    let min = all.iter().map(depth).min()?;
    let mut at = all.iter().filter(|p| depth(p) == min);
    match (at.next(), at.next()) { (Some(one), None) => Some(one.clone()), _ => None }
}

fn wiki_walk(blocks: &mut [Block], find: &mut dyn FnMut(&str) -> Option<PathBuf>) {
    for b in blocks.iter_mut() {
        wiki_walk(&mut b.children, find);
        if let BlockKind::Table(t) = &mut b.kind {
            for cell in t.rows.iter_mut().flatten() { wiki_walk(&mut cell.blocks, find); }
        }
        if !matches!(b.kind, BlockKind::Text) { continue; }
        let Some(inner) = b.text.text.trim().strip_prefix("![[").and_then(|r| r.strip_suffix("]]")) else { continue };
        let (name, size) = inner.split_once('|').map_or((inner, None), |(n, s)| (n, Some(s)));
        // `![[page#heading]]` embeds part of a page, not a file.
        let name = name.trim();
        if name.is_empty() || name.contains(['#', '^']) || name.contains("]]") { continue; }
        let ext = Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
        if !PICTURES.contains(&ext.as_str()) { continue; }
        let Some(path) = find(name) else { continue };
        // `|300` or `|300x200`: the width.
        let width = size.and_then(|s| s.trim().split('x').next()?.trim().parse::<f32>().ok()).filter(|w| *w > 0.);
        b.kind = BlockKind::Image { src_kind: "src".into(), src: path.to_string_lossy().into_owned(), width, height: None, caption: None, crop: None };
        b.text = RichText::default();
    }
}

/// A `quiet-wiki:` link (`[[page#heading|alias]]` as read) split into the
/// page and the heading: the last of nested `#a#b`, none for a `#^block`.
pub fn wiki_parts(raw: &str) -> (&str, &str) {
    let target = raw.split('|').next().unwrap_or(raw);
    let (name, anchor) = target.split_once('#').unwrap_or((target, ""));
    let heading = anchor.rsplit('#').next().unwrap_or("").trim();
    (name.trim(), if heading.starts_with('^') { "" } else { heading })
}

/// The vault `[[page]]` links in the file `from` are looked up in: the nearest
/// folder holding `.obsidian`, else the mounted folder it is in, else (in the
/// library) the top folder it is in — where an imported vault lands — else
/// the folder it is in.
pub fn wiki_vault(from: &Path, library: Option<&Path>, mounts: &[PathBuf]) -> PathBuf {
    let dir = from.parent().unwrap_or(from);
    if let Some(v) = dir.ancestors().find(|d| d.join(".obsidian").fs_is_dir()) { return v.to_path_buf(); }
    if let Some(m) = mounts.iter().filter(|m| from.starts_with(m)).max_by_key(|m| m.components().count()) { return m.clone(); }
    if let Some(lib) = library && let Ok(rel) = from.strip_prefix(lib) {
        let mut parts = rel.components();
        return match (parts.next(), parts.next()) { (Some(top), Some(_)) => lib.join(top), _ => lib.to_path_buf() };
    }
    dir.to_path_buf()
}

/// The file `[[name]]` in `from` means, the way Obsidian resolves it: `name`
/// or `name.md`, any file in `vault` whose path ends with it, letter case
/// aside — one in the page's own folder first, then the shortest path; a name
/// starting `./` or `../` is a path from the page's folder. `#heading` and
/// `|alias` are already off (`wiki_parts`); an empty name is the page itself.
/// Only files found in the vault's listing count, so the path has their real
/// letter case (a case-blind disk would open `top.md` for `Top.md`).
pub fn resolve_wiki(name: &str, from: &Path, vault: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let name = name.trim().trim_start_matches('/');
    if name.is_empty() { return Some(from.to_path_buf()); }
    let dir = from.parent()?;
    let mut wanted = vec![format!("{name}.md")];
    if Path::new(name).extension().is_some() { wanted.push(name.to_string()); }
    let norm = |p: &Path| {
        let mut out: Vec<String> = vec![];
        for c in p.components() {
            match c {
                Component::CurDir => {}
                Component::ParentDir => { out.pop(); }
                c => out.push(c.as_os_str().to_string_lossy().to_lowercase()),
            }
        }
        out
    };
    for hidden in [false, true] {
        let names = walk_names(vault, hidden);
        for w in &wanted {
            // A title the library had to rename on import (`:` and the like)
            // is found by the name it was given.
            let mut keys = vec![w.clone()];
            let base = Path::new(w).file_name()?.to_string_lossy().into_owned();
            let stem = base.strip_suffix(".md").unwrap_or(&base);
            let safe = format!("{}{}", crate::store::file_name_for(stem), if base.ends_with(".md") { ".md" } else { "" });
            if safe != base { keys.push(Path::new(w).with_file_name(safe).to_string_lossy().into_owned()); }
            for key in keys {
                let relative = key.starts_with("./") || key.starts_with("../");
                let tail = if relative { norm(&dir.join(&key)) } else { norm(Path::new(&key)) };
                let Some(last) = tail.last() else { continue };
                let found = names.get(last).into_iter().flatten().filter(|p| {
                    let rel = if relative { norm(p) } else { norm(p.strip_prefix(vault).unwrap_or(p)) };
                    if relative { rel == tail } else { rel.len() >= tail.len() && rel[rel.len() - tail.len()..] == tail[..] }
                });
                if let Some(p) = found.min_by_key(|p| (p.parent() != Some(dir), p.components().count(), p.as_os_str().len(), p.as_path())) { return Some(p.clone()); }
            }
        }
    }
    None
}

type Names = std::collections::HashMap<String, Vec<PathBuf>>;

/// Every file in `vault` by lower-case name (folders starting with `.` skipped;
/// at most 50 000 entries). Kept a minute: a folder import asks once per page.
fn vault_names(vault: &Path) -> std::sync::Arc<Names> {
    static LAST: std::sync::Mutex<Option<(PathBuf, web_time::Instant, std::sync::Arc<Names>)>> = std::sync::Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((v, t, n)) = &*last && v == vault && t.elapsed().as_secs() < 60 { return n.clone(); }
    let n = std::sync::Arc::new(walk_names(vault, false));
    *last = Some((vault.to_path_buf(), web_time::Instant::now(), n.clone()));
    n
}

/// The same index for the folders the walk above does not go into (#227): a
/// vault may keep every picture in `.attachments` and configure nothing, and
/// `![[name.png]]` still means that file. Asked only for a name the visible
/// folders have nothing for — never to decide one they do have — so a copy in
/// `.cache` or `.stversions` cannot turn a picture that resolved into an
/// ambiguous one. Its own 50 000-entry cap. Kept a minute, like the walk above.
fn hidden_names(vault: &Path) -> std::sync::Arc<Names> {
    static LAST: std::sync::Mutex<Option<(PathBuf, web_time::Instant, std::sync::Arc<Names>)>> = std::sync::Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((v, t, n)) = &*last && v == vault && t.elapsed().as_secs() < 60 { return n.clone(); }
    let n = std::sync::Arc::new(walk_names(vault, true));
    *last = Some((vault.to_path_buf(), web_time::Instant::now(), n.clone()));
    n
}

/// Folders that hold an app's own files, never a vault's attachments: the
/// editor's configuration and local trash, a repository's, this app's library.
const INTERNAL_FOLDERS: [&str; 5] = [".obsidian", ".trash", ".git", ".quiet", ".assets"];

/// `vault` by name. `hidden`: go into the dot-folders the first pass skips,
/// an app's own ones aside. A file whose own name starts with `.` is not a
/// picture anyone wrote, in either pass.
fn walk_names(vault: &Path, hidden: bool) -> Names {
    // `inside`: this folder sits below a hidden one, so what it holds is that
    // folder's content and is walked whatever its own name.
    let (mut out, mut stack, mut seen) = (Names::new(), vec![(vault.to_path_buf(), false)], 0);
    while let Some((d, inside)) = stack.pop() {
        let Ok(rd) = crate::vfs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            seen += 1;
            if seen > 50_000 { return out; }
            let name = e.file_name().to_string_lossy().into_owned();
            let dotted = name.starts_with('.');
            let Ok(kind) = e.file_type() else { continue };
            if kind.fs_is_dir() {
                let enter = inside || if dotted { hidden && !INTERNAL_FOLDERS.contains(&name.to_ascii_lowercase().as_str()) } else { !hidden };
                if enter { stack.push((e.path(), inside || dotted)); }
            } else if kind.fs_is_file() && !dotted {
                out.entry(name.to_lowercase()).or_default().push(e.path());
            }
        }
    }
    out
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
    let opts = Options { in_place: true, file: true, library_dir: opts.library_dir, base: opts.base, name: opts.name, copied: opts.copied, original: opts.original, wiki: opts.wiki, dir: opts.dir, ..Default::default() };
    let mut w = Writer { opts: &opts, files: vec![], generated: vec![], missing: 0 };
    let mut pieces = w.pieces(&doc.blocks);
    // Empty lines at the very end show nothing (the editor adds one for a click below the text).
    while pieces.last().is_some_and(|p| p.text == BLANK) { pieces.pop(); }
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
    Export { text, files: w.files, generated: vec![], missing: 0 }
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
    if let Some(dir) = opts.dir { resolve_present(&mut blocks, dir); }
    if let Some((dir, vault)) = opts.wiki { wiki_images(&mut blocks, dir, vault); }
    // The file's blocks as written now, each with the lines it came from.
    let mut old: Vec<(String, (usize, usize))> = vec![];
    {
        let mut w = Writer { opts, files: vec![], generated: vec![], missing: 0 };
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
            // A picture found only from the library (the file moved, or an older build
            // wrote `.assets/…`) is written again, relative to the file: never kept as it was.
            let s = if opts.dir.is_some() && stale(b, opts) { format!("{s}\u{0}") } else { s };
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
    if let Some(dir) = opts.dir { resolve_present(&mut blocks, dir); }
    if let Some((dir, vault)) = opts.wiki { wiki_images(&mut blocks, dir, vault); }
    let mut w = Writer { opts, files: vec![], generated: vec![], missing: 0 };
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
    // A column's alignment is its cells' most common one (ties go to the
    // header's), so a row added since keeps the column as it was.
    let sep = |c: usize| {
        let of = |r: &Vec<TableCell>| r.get(c).and_then(|x| x.blocks.first()).map(|b| b.align);
        let n = |a| t.rows.iter().filter(|r| of(r) == Some(a)).count();
        let head = t.rows.first().and_then(of).unwrap_or_default();
        let a = [Align::Left, Align::Center, Align::Right].into_iter().fold(head, |best, a| if n(a) > n(best) { a } else { best });
        match a {
            Align::Left => " --- |",
            Align::Center => " :---: |",
            Align::Right => " ---: |",
        }
    };
    let mut lines = vec![];
    let mut rows = t.rows.iter();
    match rows.next() {
        Some(r) => lines.push(row(r)),
        None => lines.push(row(&vec![])),
    }
    lines.push(format!("|{}", (0..cols).map(sep).collect::<String>()));
    lines.extend(rows.map(row));
    lines.join("\n")
}

fn indent(s: &str, prefix: &str) -> String {
    s.lines()
        .map(|l| if l.is_empty() && prefix.trim().is_empty() { String::new() } else { format!("{prefix}{l}").trim_end().to_string() })
        .collect::<Vec<_>>()
        .join("\n")
}

/// An empty paragraph in a Markdown file: a line a reader would skip shows
/// as a blank line everywhere (Obsidian, Typora, GitHub) written this way.
pub const BLANK: &str = "&nbsp;";

/// `s` with its last character as a character reference when it is a space
/// (a no-break space, a tab): a reader drops the end of a line's spaces, and
/// a line of only spaces is a blank one.
fn hold_end(s: String) -> String {
    match s.chars().next_back().filter(|c| c.is_whitespace()) {
        Some(c) => format!("{}&#{};", &s[..s.len() - c.len_utf8()], c as u32),
        None => s,
    }
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.char_indices() {
        if matches!(c, '\\' | '*' | '_' | '`' | '[' | ']' | '<' | '~') || c == '&' && crate::paste::char_ref(&s[i..]).is_some() {
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
    /// An equation: its own run, written as it is (escaping would change the LaTeX).
    math: bool,
}

fn inline(t: &RichText, files: &mut impl FnMut(&FileBlock) -> Option<String>) -> String {
    // Merge runs that only differ in marks Markdown can't express (colors).
    let mut runs: Vec<(Md, String)> = vec![];
    let mut display_math = false;
    for (r, m) in t.spans() {
        let s = match m.inline.as_ref().and_then(Inline::equation) {
            Some(src) if src.contains('\n') => {
                display_math = true;
                t.text[r].chars().map(|_| format!("$$\n{src}\n$$")).collect()
            }
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
            link: file_link.or_else(|| m.link.clone().filter(|url| crate::paste::link_ok(url) || crate::paste::relative_link(url) || url.starts_with("quiet-wiki:"))),
            math: m.inline.as_ref().and_then(Inline::equation).is_some(),
        };
        match runs.last_mut() {
            Some((last, text)) if *last == md && !md.math => text.push_str(&s),
            _ => runs.push((md, s)),
        }
    }
    let mut out = String::new();
    for (m, s) in runs {
        if s.is_empty() {
            continue;
        }
        if let Some(raw) = m.link.as_deref().and_then(|l| l.strip_prefix("quiet-wiki:")) {
            let (target, label) = raw.split_once('|').unwrap_or((raw, raw));
            if s == label { out.push_str(&format!("[[{raw}]]")); }
            else { out.push_str(&format!("[[{target}|{s}]]")); }
            continue;
        }
        // Emphasis markers must hug the text: keep outer spaces outside.
        let body = s.trim();
        let lead = &s[..s.len() - s.trim_start().len()];
        let trail = &s[s.trim_end().len()..];
        if m.math {
            out.push_str(&s);
            continue;
        }
        if body.is_empty() {
            out.push_str(&s);
            continue;
        }
        let mut x = if m.code {
            let tick = if body.contains('`') { "``" } else { "`" };
            format!("{tick}{body}{tick}")
        } else {
            static EMBED: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"!\[\[[^\[\]\r\n]+\]\]|!\[[^\[\]\r\n]*\]\([^()\r\n]+\)").unwrap());
            let mut escaped = String::new();
            let mut end = 0;
            for span in EMBED.find_iter(body) {
                escaped.push_str(&escape(&body[end..span.start()]));
                escaped.push_str(span.as_str());
                end = span.end();
            }
            escaped.push_str(&escape(&body[end..]));
            escaped
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
    if display_math {
        return out.replace("$$<br>", "$$\n").replace("<br>$$", "\n$$");
    }
    out.replace('\n', "<br>")
}

#[cfg(test)]
mod tests {
    /// What a page holds, as typed: an empty paragraph, a row of them, one of
    /// only spaces, spaces at the start, in the middle and at the end, a line
    /// break (and a blank line) inside a paragraph.
    #[test]
    fn blank_lines_and_spaces_read_back_as_typed() {
        use super::*;
        let doc = |t: &[&str]| Document { blocks: t.iter().map(|t| Block::text(t)).collect(), ..Default::default() };
        let texts = |d: &Document| d.blocks.iter().map(|b| b.text.text.clone()).collect::<Vec<_>>();
        let page = ["", "a", "", "", "", "   ", "   lead", "mid   space   ", "\u{a0}", "\t", "x\ny", "x\n\ny", "x   \ny", "&nbsp;", "&#32; and &#x20;", "<br>", "z"];
        let md = write_file(&doc(&page), "", None);
        assert_eq!(md, "&nbsp;\n\na\n\n&nbsp;\n\n&nbsp;\n\n&nbsp;\n\n  &#32;\n\n   lead\n\nmid   space  &#32;\n\n&#160;\n\n&#9;\n\nx<br>y\n\nx<br><br>y\n\nx   <br>y\n\n\\&nbsp;\n\n\\&#32; and \\&#x20;\n\n\\<br>\n\nz\n");
        assert_eq!(texts(&parse_file(&md, "t", None)), page);
        // Written again (kept lines and all) it is the same file.
        assert_eq!(write_file_with(&parse_file(&md, "t", None), "", &Options { original: Some(&md), ..Default::default() }).text, md);
        // Empty lines at the very end are not written: they show nothing.
        assert_eq!(write_file(&doc(&["a", "", "b", "", ""]), "", None), "a\n\n&nbsp;\n\nb\n");
        assert_eq!(write_file(&doc(&["", ""]), "", None), "");
        // Inside a quote too.
        let mut q = Block::new(BlockKind::Quote, RichText::default());
        q.children = doc(&["a", "", "b  "]).blocks;
        let md = write_file(&Document { blocks: vec![q.clone()], ..Default::default() }, "", None);
        assert_eq!(parse_file(&md, "t", None).blocks[0].children.iter().map(|b| b.text.text.as_str()).collect::<Vec<_>>(), ["a", "", "b  "], "{md}");
        // Another editor's spacer lines are empty paragraphs too; a paste, a copy are as before.
        assert_eq!(texts(&parse_file("a\n\n&nbsp;\n\nb\n", "t", None)), ["a", "", "b"]);
        assert_eq!(clipboard(None, &doc(&["a", "", "b "]).blocks, Path::new("/")), "a\n\nb\n");
    }
    #[test]
    fn front_matter_says_when_a_page_was_made_and_changed() {
        use super::front_matter_times as times;
        let t = |s| crate::time::parse_date(s);
        // Obsidian and Hugo spellings, any case; the first that parses wins.
        assert_eq!(times("---\ncreated: 2024-03-05\nupdated: 2024-03-06T10:00:00Z\n---\n"), (t("2024-03-05"), t("2024-03-06T10:00:00Z")));
        assert_eq!(times("---\ntitle: x\ndate: 2024-03-05T14:30:00+08:00\nlastmod: 2024-04-01\n---\n"), (t("2024-03-05T14:30:00+08:00"), t("2024-04-01")));
        assert_eq!(times("---\nCreatedAt: \"2024-03-05\"\nUpdated_At: '2024-03-07'\n---\n"), (t("2024-03-05"), t("2024-03-07")));
        assert_eq!(times("---\ndate created: 2024-03-05\ndate modified: 2024-03-08 # by Linter\n---\n"), (t("2024-03-05"), t("2024-03-08")));
        assert_eq!(times("---\nlast_modified_at: 2024-03-09\n---\n"), (None, t("2024-03-09")));
        // `created` before `date`, whichever comes first in the file.
        assert_eq!(times("---\ndate: 2020-01-01\ncreated: 2024-03-05\n---\n").0, t("2024-03-05"));
        // One that does not parse gives way to the next, or to nothing.
        assert_eq!(times("---\ncreated: last spring\ndate: 2024-03-05\n---\n").0, t("2024-03-05"));
        assert_eq!(times("---\ncreated: last spring\nmodified: {{date}}\n---\n"), (None, None));
        // Nested keys, list items and comments are not the page's.
        assert_eq!(times("---\nmeta:\n  created: 2024-03-05\n# date: 2024-03-05\ntags:\n- date: 2024-03-05\n---\n"), (None, None));
        assert_eq!(times(""), (None, None));
    }

    use super::*;

    #[test]
    fn inline_attachments_export_links_and_copies_including_tables() {
        let root = crate::vfs::temp_dir().join(format!("quiet-export-{}", uuid::Uuid::new_v4()));
        crate::vfs::create_dir_all(root.join("assets")).unwrap();
        crate::vfs::write(root.join("assets/report (1).pdf"), b"pdf").unwrap();
        let file = FileBlock { src_kind: "path".into(), src: "assets/report (1).pdf".into(), name: "Report [1]".into(), view: FileView::Card };
        let mut text = RichText::plain("before ");
        text.push("\u{fffc}", Marks { inline: Some(crate::docxml::to_inline(InlineKind::File(file))), ..Default::default() });
        text.push(" after", Marks::default());
        let mut table = Table { col_widths: vec![], header_rows: 0, header_col: false, rows: vec![vec![TableCell::empty()]] };
        table.rows[0][0].blocks = vec![Block::new(BlockKind::Text, text.clone())];
        let doc = Document { blocks: vec![Block::new(BlockKind::Text, text), Block::new(BlockKind::Table(table), RichText::default())], ..Default::default() };
        let out = export(&doc, &Options { assets: Some("out.assets"), library_dir: Some(&root), ..Default::default() });
        assert_eq!(out.files.len(), 1);
        assert_eq!(out.missing, 0);
        assert_eq!(out.text.matches("[Report \\[1\\]](out.assets/report%20%281%29.pdf)").count(), 2);
        let plain = export(&doc, &Options { library_dir: Some(&root), ..Default::default() });
        assert!(plain.files.is_empty());
        assert!(!plain.text.contains("out.assets"));
        crate::vfs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn inline_equations_are_not_escaped() {
        let text = "a $X_t + \\alpha*b*$ z\n";
        let mut out = text.to_string();
        for _ in 0..3 { out = write_file(&parse_file(&out, "t", None), "", None); }
        assert_eq!(out, text);
    }

    #[test]
    fn clipboard_keeps_structure_equations_and_local_paths() {
        let root = crate::vfs::temp_dir().join("quiet-clip-root");
        let mut eq = RichText::plain("e: ");
        eq.push("\u{fffc}", Marks { inline: Some(crate::docxml::to_inline(InlineKind::Equation("X_t".into()))), ..Default::default() });
        let blocks = vec![
            Block::new(BlockKind::Heading { level: 2, seq_auto: false }, RichText::plain("小节")),
            Block::new(BlockKind::Todo { done: true }, RichText::plain("done")),
            Block::new(BlockKind::Text, eq),
            Block::new(BlockKind::Image { src_kind: "path".into(), src: ".assets/a b.png".into(), width: None, height: None, caption: None, crop: None }, RichText::default()),
        ];
        let md = clipboard(Some("T"), &blocks, &root);
        let img = root.join(".assets/a b.png").to_string_lossy().replace(' ', "%20");
        assert!(md.starts_with("# T\n\n## 小节\n\n- [x] done\n\ne: $X_t$\n\n![]("), "{md}");
        assert!(md.contains(&img), "{md}");
        assert!(!clipboard(None, &blocks[..1], &root).contains("# T"));
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
                Block::new(BlockKind::Code { lang: "Rust".into(), caption: None, wrap: false, show_code: false }, RichText::plain("fn main() {}")),
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

    /// A ```mermaid fence is a Mermaid code block (drawn as a diagram); either
    /// view saves back as the same fence.
    #[test]
    fn mermaid_fences_round_trip() {
        let src = "---\ntitle: x\n---\n\n```mermaid\nflowchart LR\n    A[开始] --> B{判断}\n```\n";
        let mut doc = parse_file(src, "page", None);
        let [b] = doc.blocks.as_mut_slice() else { panic!("{:?}", doc.blocks) };
        assert!(matches!(&b.kind, BlockKind::Code { lang, show_code: false, .. } if lang == "Mermaid"), "{:?}", b.kind);
        assert_eq!(write_file(&doc, front_matter(src).0, None), src);
        if let BlockKind::Code { show_code, .. } = &mut doc.blocks[0].kind { *show_code = true; }
        assert_eq!(write_file(&doc, front_matter(src).0, None), src);
        let doc = crate::docxml::parse(&crate::docxml::serialize(&doc)).unwrap();
        assert!(matches!(&doc.blocks[0].kind, BlockKind::Code { lang, show_code: true, .. } if lang == "Mermaid"));
    }

    /// Underline is written `<u>…</u>` and reads back as underline, nested in
    /// the other marks too; a `<u>` that was text (written `\<u>`) stays text.
    #[test]
    fn underline_round_trips() {
        let base = Path::new(if cfg!(windows) { "C:/notes" } else { "/notes" });
        let src = "Plain <u>under</u>, **<u>bold</u>**, *<u>it</u>*, ***<u>both</u>***, [<u>link</u>](https://x.y) and <u>a</u> **<u>b</u>** [<u>c</u>](https://x.y).\n\nOld \\<u>text\\</u> and `<u>code</u>`.\n\n```\n<u>fence</u>\n```\n";
        let doc = parse_file(src, "page", Some(base));
        let t = &doc.blocks[0].text;
        let under: String = t.spans().filter(|(_, m)| m.underline).map(|(r, _)| &t.text[r]).collect();
        assert_eq!(under, "underbolditbothlinkabc");
        assert!(t.spans().any(|(r, m)| m.underline && m.bold && m.italic && &t.text[r] == "both"));
        assert!(t.spans().any(|(r, m)| m.underline && m.link.as_deref() == Some("https://x.y") && &t.text[r] == "link"));
        let t = &doc.blocks[1].text;
        assert!(t.text.contains("<u>text</u>") && t.text.contains("<u>code</u>") && t.spans().all(|(_, m)| !m.underline), "{t:?}");
        assert!(doc.blocks[2].text.text.contains("<u>fence</u>"));
        // Saved as is, then edited (written fresh): the same text either way, and again after reopening.
        assert_eq!(write_file(&doc, front_matter(src).0, Some(base)), src);
        let fresh = to_markdown(&Document { blocks: doc.blocks.clone(), ..Default::default() });
        assert!(fresh.starts_with(src.lines().next().unwrap()) && fresh.contains("Old \\<u>text\\</u>"), "{fresh}");
        let again = to_markdown(&Document { blocks: parse_file(&fresh, "page", Some(base)).blocks, ..Default::default() });
        assert_eq!(again, fresh);
        // Copy as Markdown writes the same.
        let copied = clipboard(None, &doc.blocks[..1], base);
        assert_eq!(copied.trim_end(), src.lines().next().unwrap());
        // DocxXML keeps it.
        assert!(crate::docxml::serialize(&doc).contains("<u>under</u>"));
        // Marks inside one `<u>` (Typora, Obsidian) read the same; saving writes them run by run.
        let doc = parse_file("<u>a **b** [c](https://x.y)</u>\n", "page", None);
        let t = &doc.blocks[0].text;
        assert!(t.spans().all(|(_, m)| m.underline) && t.spans().any(|(r, m)| m.bold && &t.text[r] == "b"), "{t:?}");
        assert_eq!(to_markdown(&Document { blocks: doc.blocks.clone(), ..Default::default() }), "<u>a</u> **<u>b</u>** [<u>c</u>](https://x.y)\n");
    }

    /// Pasting into a Markdown file keeps what it can hold and nothing it can't.
    #[test]
    fn keepable_leaves_out_what_markdown_drops() {
        let xml = "<title>T</title>\n<p>交付 <time expire-time=\"1761200000000\" notify-time=\"1761199700000\" should-notify=\"false\">x</time> 前</p>\n<callout emoji=\"💡\">\n  <p>提示</p>\n</callout>\n<grid>\n  <column width-ratio=\"0.5\">\n    <p>左</p>\n  </column>\n  <column width-ratio=\"0.5\">\n    <blockquote>\n      <p>右引</p>\n    </blockquote>\n  </column>\n</grid>\n<countdown duration=\"60000\" start-time=\"0\" notify=\"false\" color=\"blue\" timing=\"false\"/>\n<p>公式 <latex>x^2</latex></p>\n";
        let doc = crate::docxml::parse(xml).unwrap();
        assert!(doc.blocks.iter().any(|b| matches!(b.kind, BlockKind::Countdown(_))), "{:?}", doc.blocks);
        let kept = keepable(doc.blocks);
        let out = crate::docxml::serialize(&Document { blocks: kept.clone(), ..Default::default() });
        assert!(!out.contains("<time") && !out.contains("<callout") && !out.contains("<grid") && !out.contains("<countdown"), "{out}");
        assert!(out.contains("<p>交付  前</p>") && out.contains("<p>提示</p>") && out.contains("<p>左</p>") && out.contains("<blockquote>") && out.contains("<latex>x^2</latex>"), "{out}");
        // Saved as Markdown and read back, nothing more goes missing.
        let md = write_file(&Document { blocks: kept.clone(), ..Default::default() }, "", None);
        assert_eq!(crate::docxml::serialize(&parse_file(&md, "", None)).matches("<p>").count(), out.matches("<p>").count(), "{md}");
    }

    #[test]
    fn display_math_round_trips() {
        let src = "---\ntitle: x\n---\n\n$$\nV &= I R \\\\\nQ_c &= 2I - I^2\n$$\n";
        let base = Path::new(if cfg!(windows) { "C:/notes" } else { "/notes" });
        let doc = parse_file(src, "page", Some(base));
        assert_eq!(write_file(&doc, front_matter(src).0, Some(base)), src);
    }

    #[test]
    fn display_math_export_keeps_surrounding_text() {
        let mut text = RichText::plain("before ");
        text.push(crate::editor::core::EQUATION, Marks { inline: Some(crate::docxml::to_inline(InlineKind::Equation("V = I R\nQ = I^2".into()))), ..Default::default() });
        text.push(" after", Marks::default());
        let doc = Document { blocks: vec![Block::new(BlockKind::Text, text)], ..Default::default() };
        assert_eq!(to_markdown(&doc), "before $$\nV = I R\nQ = I^2\n$$ after\n");
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

    /// An `<img>` line is a picture like `![](…)`: relative paths resolve
    /// beside the file, and saving keeps the tag as written.
    #[test]
    fn html_images() {
        let src = "a\n\n<img src=\"img/a%20b.png\" width=\"240\">\n\n<img src=\"https://file.oss/image.png\" alt=\"\" width=\"auto\" height=\"auto\" >\n";
        let base = Path::new(if cfg!(windows) { "C:/notes" } else { "/notes" });
        let imgs: Vec<_> = parse_file(src, "t", Some(base)).blocks.into_iter().filter_map(|b| match b.kind {
            BlockKind::Image { src, width, .. } => Some((PathBuf::from(src), width)),
            _ => None,
        }).collect();
        assert_eq!(imgs, [(base.join("img").join("a b.png"), Some(240.)), (PathBuf::from("https://file.oss/image.png"), None)]);
        assert_eq!(preserved(src, |_| {}), src);
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
        let t = Table { col_widths: vec![], header_rows: 1, header_col: false, rows: vec![vec![cell("a"), cell("b|c")], vec![cell("1")]] };
        assert_eq!(table(&t, &mut |_| None), "| a | b\\|c |\n| --- | --- |\n| 1 |  |");
        // Column alignment reads back from the delimiter row.
        let src = "| a | b | c |\n| :--- | :---: | ---: |\n| 1 | 2 | 3 |";
        let doc = parse_file(src, "t", None);
        assert_eq!(write_file(&doc, "", None).trim_end(), "| a | b | c |\n| --- | :---: | ---: |\n| 1 | 2 | 3 |");
    }

    /// `![[a.png]]`, as Obsidian and its exports write it: beside the page,
    /// in the vault's attachment folder, or by a name only one file has.
    #[test]
    fn wiki_embeds_become_pictures_when_the_file_is_there() {
        let root = crate::vfs::temp_dir().join(format!("fdl-embed-{}", uuid::Uuid::new_v4()));
        let notes = root.join("notes");
        for d in ["notes", ".obsidian", "files", "deep/er"] { crate::vfs::create_dir_all(root.join(d)).unwrap(); }
        crate::vfs::write(root.join(".obsidian/app.json"), r#"{"attachmentFolderPath":"files"}"#).unwrap();
        for f in ["notes/p 1.png", "files/att.png", "deep/er/far.png", "drawing.svg", "notes/twice.png", "files/twice.png"] { crate::vfs::write(root.join(f), b"png").unwrap(); }
        let src = "4、\n\n![[p 1.png]]\n\n![[p 1.png|300x200]]\n\n![[att.png]]\n\n![[far.png]]\n\n![[missing.png]]\n\n![[page#heading]]\n\n![[drawing.svg]]\n\n![[deep/er/far.png]]\n";
        let mut doc = parse_file(src, "page", Some(&notes));
        wiki_images(&mut doc.blocks, &notes, None);
        let pics: Vec<(PathBuf, Option<f32>)> = doc.blocks.iter().filter_map(|b| match &b.kind {
            BlockKind::Image { src, width, .. } => Some((PathBuf::from(src), *width)),
            _ => None,
        }).collect();
        let p1 = notes.join("p 1.png");
        assert_eq!(pics, vec![(p1.clone(), None), (p1, Some(300.)), (root.join("files/att.png"), None), (root.join("deep/er/far.png"), None), (root.join("deep/er/far.png"), None)]);
        let texts: Vec<&str> = doc.blocks.iter().map(|b| b.text.text.as_str()).filter(|t| !t.is_empty()).collect();
        assert_eq!(texts, vec!["4、", "![[missing.png]]", "![[page#heading]]", "![[drawing.svg]]"]);
        // Imported as a DocxXML page (not edited as Markdown) too.
        let mut blocks = crate::paste::markdown_to_blocks("![[att.png]]\n", crate::paste::Off { file: true, ..Default::default() });
        wiki_images(&mut blocks, &notes, None);
        assert!(matches!(&blocks[0].kind, BlockKind::Image { src, .. } if Path::new(src) == root.join("files/att.png")), "{blocks:?}");
        let _ = crate::vfs::remove_dir_all(&root);
    }

    /// A name several files have is the one with the shortest path, as in
    /// Obsidian; two equally short stay the text they were.
    #[test]
    fn wiki_embeds_take_the_shortest_path_of_a_shared_name() {
        let root = crate::vfs::temp_dir().join(format!("fdl-shortest-{}", uuid::Uuid::new_v4()));
        let notes = root.join("notes");
        for d in ["notes", ".obsidian", "a/b/c", "d", "e"] { crate::vfs::create_dir_all(root.join(d)).unwrap(); }
        for f in ["a/b/c/pic.png", "d/pic.png", "d/tie.png", "e/tie.png"] { crate::vfs::write(root.join(f), b"png").unwrap(); }
        let mut doc = parse_file("![[pic.png]]\n\n![[tie.png]]\n", "page", Some(&notes));
        wiki_images(&mut doc.blocks, &notes, None);
        assert!(matches!(&doc.blocks[0].kind, BlockKind::Image { src, .. } if Path::new(src) == root.join("d/pic.png")), "{:?}", doc.blocks[0]);
        assert_eq!(doc.blocks[1].text.text, "![[tie.png]]");
        let _ = crate::vfs::remove_dir_all(&root);
    }

    /// Read with `wiki` (as a library document is), the embeds are pictures;
    /// written back with the file as `original`, they keep `![[…]]`, also when
    /// a block beside them changed. Another picture in one's place is `![](…)`.
    #[test]
    fn wiki_embeds_round_trip_in_place() {
        let root = crate::vfs::temp_dir().join(format!("fdl-embed-rt-{}", uuid::Uuid::new_v4()));
        let notes = root.join("notes");
        for d in ["notes", ".obsidian", "files"] { crate::vfs::create_dir_all(root.join(d)).unwrap(); }
        crate::vfs::write(root.join(".obsidian/app.json"), r#"{"attachmentFolderPath":"files"}"#).unwrap();
        crate::vfs::write(root.join("files/a.png"), b"png").unwrap();
        let src = "intro\n\n![[a.png|300]]\n\n![[a.png]]\n\n![[Note]]\n\nend\n";
        let opts = |o| Options { wiki: Some((notes.as_path(), None)), original: Some(o), dir: Some(&notes), ..Default::default() };
        // As `Store::parse_doc` reads a library file.
        let read = |text: &str| { let mut d = parse_file(text, "t", None); resolve_present(&mut d.blocks, &notes); wiki_images(&mut d.blocks, &notes, None); d };
        let mut doc = read(src);
        assert_eq!(doc.blocks.iter().filter(|b| matches!(b.kind, BlockKind::Image { .. })).count(), 2);
        assert_eq!(write_file_with(&doc, "", &opts(src)).text, src);
        doc.blocks[0] = Block::text("first");
        let out = write_file_with(&doc, "", &opts(src)).text;
        assert_eq!(out, src.replace("intro", "first"));
        // Read again, it is the same document; and stable from then on.
        assert_eq!(write_file_with(&read(&out), "", &opts(&out)).text, out);
        // Another picture in its place is written the way Markdown writes one,
        // relative to the file's folder (`../`, `/`), never as an absolute path.
        crate::vfs::write(root.join("files/b.png"), b"png").unwrap();
        if let BlockKind::Image { src, .. } = &mut doc.blocks[2].kind { *src = root.join("files").join("b.png").to_string_lossy().into_owned(); }
        let out = write_file_with(&doc, "", &opts(src)).text;
        assert_eq!(out, src.replace("intro", "first").replace("![[a.png]]", "![](../files/b.png)"));
        assert_relative(&out);
        // Read again (relative to the file's folder), it is the same picture.
        let again = read(&out);
        assert!(matches!(&again.blocks[2].kind, BlockKind::Image { src, .. } if Path::new(src) == root.join("files").join("b.png")), "{:?}", again.blocks[2]);
        // A width Markdown can't hold is an `<img>` line, which reads back with it.
        if let BlockKind::Image { width, .. } = &mut doc.blocks[1].kind { *width = Some(120.); }
        let out = write_file_with(&doc, "", &opts(src)).text;
        assert!(out.contains("<img src=\"../files/a.png\" width=\"120\">"), "{out}");
        assert_relative(&out);
        let a = root.join("files").join("a.png");
        assert!(read(&out).blocks.iter().any(|b| matches!(&b.kind, BlockKind::Image { src, width: Some(w), .. } if Path::new(src) == a && *w == 120.)));
        // Written fresh (an import into the library): the width as `<img>`, the rest as `![](…)`, both relative.
        let out = write_file_with(&read(src), "", &Options { dir: Some(&notes), ..Default::default() }).text;
        assert!(out.contains("<img src=\"../files/a.png\" width=\"300\">") && out.contains("![](../files/a.png)"), "{out}");
        assert_relative(&out);
        let _ = crate::vfs::remove_dir_all(&root);
    }

    /// No picture in `out` is an absolute path: no drive (`C:`), no leading `/` or `\\`.
    fn assert_relative(out: &str) {
        let srcs = out.lines().filter_map(|l| l.split_once("](").map(|(_, r)| r.trim_end_matches(')')).or_else(|| l.split_once("src=\"").map(|(_, r)| r.split('"').next().unwrap())));
        for s in srcs {
            let drive = s.as_bytes().get(1) == Some(&b':') && s.as_bytes()[0].is_ascii_alphabetic();
            assert!(!drive && !s.starts_with('/') && !s.starts_with('\\') && !s.contains('\\'), "{s} in {out}");
        }
    }

    /// Written in place, a picture is relative to the Markdown file's folder:
    /// one below it as `pics/a.png`, one beside a parent as `../files/b.png`,
    /// on any platform with `/`. A local file (`base`) the same way.
    #[test]
    fn in_place_pictures_are_relative_to_the_file() {
        let root = crate::vfs::temp_dir().join(format!("fdl-relative-{}", uuid::Uuid::new_v4()));
        let notes = root.join("notes");
        for d in ["notes/pics", "files", "far/away"] { crate::vfs::create_dir_all(root.join(d)).unwrap(); }
        for f in ["notes/pics/a.png", "files/b.png", "far/away/c.png"] { crate::vfs::write(root.join(f), b"png").unwrap(); }
        let pic = |p: PathBuf, w: Option<f32>| Block::new(BlockKind::Image { src_kind: "src".into(), src: p.to_string_lossy().into_owned(), width: w, height: None, caption: None, crop: None }, RichText::default());
        let doc = Document { blocks: vec![pic(notes.join("pics").join("a.png"), None), pic(root.join("files").join("b.png"), Some(50.)), pic(root.join("far").join("away").join("c.png"), None)], ..Default::default() };
        let want = "![](pics/a.png)\n\n<img src=\"../files/b.png\" width=\"50\">\n\n![](../far/away/c.png)\n";
        assert_eq!(write_file_with(&doc, "", &Options { dir: Some(&notes), ..Default::default() }).text, want);
        assert_eq!(write_file(&doc, "", Some(&notes)), want);
        assert_relative(want);
        // Read back from the file's folder, each is the picture it was.
        let mut back = parse_file(want, "t", None);
        resolve_present(&mut back.blocks, &notes);
        let srcs: Vec<_> = back.blocks.iter().filter_map(|b| match &b.kind { BlockKind::Image { src, .. } => Some(PathBuf::from(src)), _ => None }).collect();
        assert_eq!(srcs, [notes.join("pics/a.png"), root.join("files/b.png"), root.join("far/away/c.png")]);
        // A local file's `../` source, read with `resolve_images`, is written back as it was.
        let mut local = parse_file("![](../files/b.png)\n", "t", Some(&notes));
        local.blocks[0].align = Align::Left;
        assert_eq!(write_file(&local, "", Some(&notes)), "![](../files/b.png)\n");
        assert!(srcs.iter().all(|p| p.fs_is_file()));
        let _ = crate::vfs::remove_dir_all(&root);
    }

    /// A vault may keep its pictures in a hidden folder (`.attachments`) and
    /// configure nothing, so the walk has to go on into it — #227. A name the
    /// folders a person sees already have keeps resolving to theirs, and the
    /// library's own folders are still not the vault's.
    #[test]
    fn wiki_embeds_reach_pictures_in_a_hidden_attachment_folder() {
        let root = crate::vfs::temp_dir().join(format!("fdl-hidden-{}", uuid::Uuid::new_v4()));
        let notes = root.join("notes");
        for d in ["notes", ".obsidian", ".attachments/sub", "normal", ".cache", ".backups", ".quiet", ".git"] {
            crate::vfs::create_dir_all(root.join(d)).unwrap();
        }
        for f in [".attachments/hidden.png", ".attachments/sub/deep.png", "normal/visible.png", ".cache/visible.png",
            ".cache/twice.png", ".backups/twice.png", ".quiet/quiet.png", ".git/ignored.png", ".obsidian/app.png"] {
            crate::vfs::write(root.join(f), b"png").unwrap();
        }
        let src = "![[hidden.png]]\n\n![[deep.png]]\n\n![[visible.png]]\n\n![[twice.png]]\n\n![[quiet.png]]\n\n![[ignored.png]]\n\n![[app.png]]\n";
        let mut doc = parse_file(src, "page", Some(&notes));
        wiki_images(&mut doc.blocks, &notes, None);
        let pics: Vec<PathBuf> = doc.blocks.iter().filter_map(|b| match &b.kind {
            BlockKind::Image { src, .. } => Some(PathBuf::from(src)),
            _ => None,
        }).collect();
        // `visible.png` is the visible one, though `.cache` has another; `twice.png`
        // is only in hidden folders, in two of them, so it is as unresolved as a
        // name nobody has.
        assert_eq!(pics, vec![root.join(".attachments/hidden.png"), root.join(".attachments/sub/deep.png"), root.join("normal/visible.png")]);
        let texts: Vec<&str> = doc.blocks.iter().map(|b| b.text.text.as_str()).filter(|t| !t.is_empty()).collect();
        assert_eq!(texts, vec!["![[twice.png]]", "![[quiet.png]]", "![[ignored.png]]", "![[app.png]]"]);
        let _ = crate::vfs::remove_dir_all(&root);
    }

    /// Obsidian's `[[page]]`, `[[page|alias]]` and `[[page#heading]]` read as
    /// links showing the alias (or the page), and are written back as they
    /// were — in paragraphs, lists, quotes and table cells.
    #[test]
    fn wiki_links_read_as_links_and_write_back() {
        let src = "See [[Page]], [[Other Note|the alias]] and [[Deep/Path/Note#Some Heading]].\n\n- [[a#^block]] and `[[code]]`\n\n> [[Quoted]]\n\n| a | b |\n| --- | --- |\n| [[Page\\|alias]] | [[Plain]] |\n\n![[embed]] text\n";
        let doc = parse_file(src, "page", None);
        let links: Vec<(String, String)> = doc.blocks.iter().flat_map(|b| {
            let mut texts = vec![&b.text];
            texts.extend(b.children.iter().map(|c| &c.text));
            if let BlockKind::Table(t) = &b.kind { texts.extend(t.rows.iter().flatten().flat_map(|c| c.blocks.iter().map(|b| &b.text))); }
            texts.into_iter().flat_map(|t| t.spans().filter_map(|(r, m)| Some((t.text[r].to_string(), m.link.clone()?))).collect::<Vec<_>>()).collect::<Vec<_>>()
        }).collect();
        assert_eq!(links, [("Page", "Page"), ("the alias", "Other Note|the alias"), ("Deep/Path/Note#Some Heading", "Deep/Path/Note#Some Heading"), ("a#^block", "a#^block"), ("Quoted", "Quoted"), ("alias", "Page|alias"), ("Plain", "Plain")]
            .map(|(t, l)| (t.to_string(), format!("quiet-wiki:{l}"))));
        assert_eq!(write_file(&doc, "", None), src);
        // Re-labelled in the editor: the target stays, the new text is the alias.
        let mut doc = parse_file("[[Page]] and [[Note|old]]\n", "page", None);
        let t = &mut doc.blocks[0].text;
        let text = t.text.replacen("old", "new label", 1);
        let marks: Vec<_> = t.spans().map(|(r, m)| (r, m.clone())).collect();
        let mut fresh = RichText::plain("");
        for (r, m) in marks { let s = &t.text[r]; fresh.push(if s == "old" { "new label" } else { s }, m); }
        assert_eq!(fresh.text, text);
        doc.blocks[0].text = fresh;
        assert_eq!(write_file(&doc, "", None), "[[Page]] and [[Note|new label]]\n");
        // A DocxXML page keeps the link too.
        let xml = crate::docxml::serialize(&parse_file("[[Page#H|alias]]\n", "x", None));
        let back = crate::docxml::parse(&xml).unwrap();
        assert_eq!(to_markdown(&back), "# x\n\n[[Page#H|alias]]\n");
    }

    #[test]
    fn wiki_parts_split_page_heading_and_alias() {
        assert_eq!(wiki_parts("Page"), ("Page", ""));
        assert_eq!(wiki_parts("Page|alias"), ("Page", ""));
        assert_eq!(wiki_parts("Page#Heading|alias"), ("Page", "Heading"));
        assert_eq!(wiki_parts("Page#Outer#Inner"), ("Page", "Inner"));
        assert_eq!(wiki_parts("Page#^block1"), ("Page", ""));
        assert_eq!(wiki_parts("#Here"), ("", "Here"));
        assert_eq!(wiki_parts("a/b/Page "), ("a/b/Page", ""));
    }

    /// Obsidian's lookup: letter case aside, the path may be left out, a
    /// name in the page's own folder wins, then the shortest path; a partial
    /// path picks among same names; a missing page is nothing.
    #[test]
    fn wiki_links_resolve_like_obsidian() {
        let v = crate::vfs::temp_dir().join(format!("fdl-wiki-{}", uuid::Uuid::new_v4()));
        for f in ["Top.md", "notes/Page.md", "notes/deep/Page.md", "other/Same.md", "notes/Same.md", "x/y/Same.md", "a/b/Only Here.md",
            "Note v1.2.md", "files/report.pdf", "Ratio：Odds.md", ".trash/Gone.md", "notes/Here.md", ".attachments/hidden.pdf"] {
            let p = v.join(f);
            crate::vfs::create_dir_all(p.parent().unwrap()).unwrap();
            crate::vfs::write(&p, b"x").unwrap();
        }
        let from = v.join("notes/Here.md");
        let r = |name: &str| resolve_wiki(name, &from, &v).map(|p| p.strip_prefix(&v).unwrap().to_string_lossy().replace('\\', "/"));
        assert_eq!(r("Top").as_deref(), Some("Top.md"));
        assert_eq!(r("top").as_deref(), Some("Top.md"));
        assert_eq!(r("Page").as_deref(), Some("notes/Page.md"));
        assert_eq!(r("deep/Page").as_deref(), Some("notes/deep/Page.md"));
        assert_eq!(r("Same").as_deref(), Some("notes/Same.md"));
        assert_eq!(r("y/same").as_deref(), Some("x/y/Same.md"));
        assert_eq!(r("only here").as_deref(), Some("a/b/Only Here.md"));
        assert_eq!(r("/a/b/Only Here").as_deref(), Some("a/b/Only Here.md"));
        assert_eq!(r("Note v1.2").as_deref(), Some("Note v1.2.md"));
        assert_eq!(r("report.pdf").as_deref(), Some("files/report.pdf"));
        assert_eq!(r("Ratio:Odds").as_deref(), Some("Ratio：Odds.md"));
        assert_eq!(r("").as_deref(), Some("notes/Here.md"));
        assert_eq!(r("Missing"), None);
        assert_eq!(r("Gone"), None);
        assert_eq!(r("z/Same"), None);
        assert_eq!(r("../Top").as_deref(), Some("Top.md"));
        assert_eq!(r("./deep/page").as_deref(), Some("notes/deep/Page.md"));
        assert_eq!(r("../Page"), None);
        assert_eq!(r("hidden.pdf").as_deref(), Some(".attachments/hidden.pdf"));
        // From elsewhere the shortest path wins.
        let from = v.join("a/b/Only Here.md");
        assert_eq!(resolve_wiki("Same", &from, &v), Some(v.join("notes/Same.md")));
        assert_eq!(resolve_wiki("Page", &from, &v), Some(v.join("notes/Page.md")));
        let _ = crate::vfs::remove_dir_all(&v);
    }

    /// Where links are looked up: the vault, a mounted folder, an imported
    /// vault's top folder in the library, else the page's folder.
    #[test]
    fn wiki_vault_is_the_nearest_sensible_root() {
        let t = crate::vfs::temp_dir().join(format!("fdl-vault-{}", uuid::Uuid::new_v4()));
        for d in ["obs/.obsidian", "obs/sub", "lib/Vault/notes", "mnt/a", "loose"] { crate::vfs::create_dir_all(t.join(d)).unwrap(); }
        let lib = t.join("lib");
        let mounts = [t.join("mnt")];
        assert_eq!(wiki_vault(&t.join("obs/sub/x.md"), Some(&lib), &mounts), t.join("obs"));
        assert_eq!(wiki_vault(&t.join("mnt/a/x.md"), Some(&lib), &mounts), t.join("mnt"));
        assert_eq!(wiki_vault(&t.join("lib/Vault/notes/x.md"), Some(&lib), &mounts), t.join("lib/Vault"));
        assert_eq!(wiki_vault(&t.join("lib/x.md"), Some(&lib), &mounts), lib);
        assert_eq!(wiki_vault(&t.join("loose/x.md"), Some(&lib), &mounts), t.join("loose"));
        let _ = crate::vfs::remove_dir_all(&t);
    }
}
