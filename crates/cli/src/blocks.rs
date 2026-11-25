//! Blocks as the CLI names them. A ref is a block's position, 1-based and
//! dotted through children (`#3.1`: the first child of the third block),
//! with a fingerprint of the block's kind and text (`#3.1~k7f2`). A ref
//! checks itself: the block at that position must still have that
//! fingerprint, else the one block in the document that has it is taken
//! (it moved), else the ref is stale. So a ref read before someone else
//! edited the document still finds its block or fails; it never edits
//! another one. Inside one `quiet run` a block also has an id (`b17`) that
//! stays put however the document is edited. Table cells have no refs.

use fdl_core::model::{Block, BlockId, BlockKind, Document, InlineDocLink, InlineKind, Marks, RichText};
use serde_json::{Value, json};

use crate::fail::{R, fail};

pub struct Item<'a> {
    pub path: Vec<usize>,
    pub block: &'a Block,
}

/// Every block in document order, children after their parent.
pub fn items(doc: &Document) -> Vec<Item<'_>> {
    fn walk<'a>(list: &'a [Block], at: &mut Vec<usize>, out: &mut Vec<Item<'a>>) {
        for (i, b) in list.iter().enumerate() {
            at.push(i);
            out.push(Item { path: at.clone(), block: b });
            walk(&b.children, at, out);
            at.pop();
        }
    }
    let mut out = vec![];
    walk(&doc.blocks, &mut vec![], &mut out);
    out
}

pub fn ref_of(path: &[usize]) -> String {
    format!("#{}", path.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>().join("."))
}

pub fn id_of(id: BlockId) -> String { format!("b{}", id.0) }

/// Four base-32 characters from the block's kind and its own text, inline
/// objects (mentions, equations) included.
pub fn fingerprint(b: &Block) -> String {
    let inline: String = b.text.runs.iter().filter_map(|r| r.marks.inline.as_ref()).map(|i| i.xml.as_str()).collect();
    let d = sha1_smol::Sha1::from(format!("{}\0{}\0{inline}", kind_name(&b.kind), text_of(b))).digest().bytes();
    const A: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
    let n = u32::from_be_bytes([d[0], d[1], d[2], d[3]]);
    (0..4).map(|i| A[((n >> (i * 5)) & 31) as usize] as char).collect()
}

/// The self-checking ref of a listed block: `#3.1~k7f2`.
pub fn pin(it: &Item) -> String { format!("{}~{}", ref_of(&it.path), fingerprint(it.block)) }

/// The self-checking ref of the block `id`, if it is in the document.
pub fn pin_of(doc: &Document, id: BlockId) -> Option<String> { items(doc).iter().find(|it| it.block.id == id).map(pin) }

pub fn path_of(doc: &Document, id: BlockId) -> Option<Vec<usize>> {
    items(doc).into_iter().find(|it| it.block.id == id).map(|it| it.path)
}

pub fn find(doc: &Document, id: BlockId) -> Option<&Block> {
    items(doc).into_iter().find(|it| it.block.id == id).map(|it| it.block)
}

pub fn kind_name(k: &BlockKind) -> String {
    match k {
        BlockKind::Text => "text".into(),
        BlockKind::Heading { level, .. } => format!("h{level}"),
        BlockKind::Bullet { .. } => "bullet".into(),
        BlockKind::Ordered { .. } => "ordered".into(),
        BlockKind::Todo { .. } => "todo".into(),
        BlockKind::Code { .. } => "code".into(),
        BlockKind::Quote => "quote".into(),
        BlockKind::Callout { .. } => "callout".into(),
        BlockKind::Divider => "divider".into(),
        BlockKind::Image { .. } => "image".into(),
        BlockKind::Table(_) => "table".into(),
        BlockKind::Grid => "grid".into(),
        BlockKind::Column { .. } => "column".into(),
        BlockKind::SubPageList => "subpages".into(),
        BlockKind::Countdown(_) => "countdown".into(),
        BlockKind::File(_) => "file".into(),
        BlockKind::Board(_) => "board".into(),
        BlockKind::DataTable(_) => "datatable".into(),
        BlockKind::Raw { tag, .. } => format!("raw:{tag}"),
    }
}

/// What `set --kind` and `setKind` turn a text block into.
pub fn parse_kind(s: &str) -> Option<BlockKind> {
    Some(match s {
        "text" | "p" => BlockKind::Text,
        "bullet" => BlockKind::Bullet { dash: false },
        "ordered" => BlockKind::Ordered { seq: None },
        "todo" => BlockKind::Todo { done: false },
        "quote" => BlockKind::Quote,
        "code" => BlockKind::Code { lang: String::new(), caption: None, wrap: false, show_code: false },
        h if h.len() == 2 && h.starts_with('h') => match h[1..].parse::<u8>() { Ok(l @ 1..=9) => BlockKind::Heading { level: l, seq_auto: false }, _ => return None },
        _ => return None,
    })
}

/// A block's text as a reader sees it; a table as its rows, cells joined by ` | `.
pub fn text_of(b: &Block) -> String {
    match &b.kind {
        BlockKind::Table(t) => t.rows.iter().map(|row| row.iter().filter(|c| !c.is_covered()).map(|c| c.blocks.iter().map(|b| shown(&b.text)).collect::<Vec<_>>().join(" ")).collect::<Vec<_>>().join(" | ")).collect::<Vec<_>>().join("\n"),
        BlockKind::Image { caption, src, .. } => caption.clone().unwrap_or_else(|| src.clone()),
        BlockKind::File(f) => f.name.clone(),
        BlockKind::Board(bd) => bd.plain_text(),
        BlockKind::DataTable(t) => t.src.clone(),
        _ => shown(&b.text),
    }
}

/// Text as the page shows it: a mention as the title of what it points at.
fn shown(t: &RichText) -> String {
    if !t.text.contains('\u{fffc}') { return t.text.clone(); }
    let mut out = String::new();
    for (r, m) in t.spans() {
        match m.inline.as_ref().and_then(|i| i.doc_link()) {
            Some(l) => out.push_str(&l.title),
            None => { let mut one = RichText::default(); one.push(&t.text[r], m.clone()); out.push_str(&one.plain_text()); }
        }
    }
    out
}

/// `[[target]]` links (a Markdown file's links to other pages): target and the line.
pub fn wiki_links(doc: &Document) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![];
    for it in items(doc) {
        for (_, m) in it.block.text.spans() {
            if let Some(w) = m.link.as_deref().and_then(|l| l.strip_prefix("quiet-wiki:")) {
                let target = w.split('|').next().unwrap_or(w).split('#').next().unwrap_or(w).trim().to_string();
                if !out.iter().any(|(t, _)| t == &target) { out.push((target, text_of(it.block))); }
            }
        }
    }
    out
}

pub fn heading_level(b: &Block) -> Option<u8> { if let BlockKind::Heading { level, .. } = b.kind { Some(level) } else { None } }

pub fn block_json(it: &Item) -> Value {
    let b = it.block;
    let mut v = json!({ "ref": pin(it), "id": id_of(b.id), "kind": kind_name(&b.kind), "depth": it.path.len() - 1, "text": text_of(b) });
    match &b.kind {
        BlockKind::Heading { level, .. } => { v["level"] = json!(level); }
        BlockKind::Todo { done } => { v["done"] = json!(done); }
        BlockKind::Code { lang, .. } if !lang.is_empty() => { v["lang"] = json!(lang); }
        BlockKind::Image { src, .. } => { v["src"] = json!(src); }
        BlockKind::DataTable(t) => { v["src"] = json!(t.src); }
        _ => {}
    }
    if !b.children.is_empty() { v["children"] = json!(b.children.len()); }
    v
}

/// The block an anchor names: `b17` (this run's id), `#3.1~k7f2` / `#3.1`
/// / `3.1` (a position, checked against its fingerprint when it has one),
/// or a heading's text (exact, else the one heading containing it).
pub fn resolve(doc: &Document, anchor: &str) -> R<BlockId> {
    let a = anchor.trim();
    let all = items(doc);
    if let Some(n) = a.strip_prefix('b').and_then(|n| n.parse::<u64>().ok()) {
        return all.iter().find(|it| it.block.id.0 == n).map(|it| it.block.id)
            .ok_or_else(|| fail("stale_ref", format!("block {a} is not in this document (ids last only for one `quiet run`)")).hint("look the block up again with blocks() or outline()"));
    }
    let bare = a.trim_start_matches('#');
    let (at, fp) = match bare.split_once('~') { Some((p, f)) => (p, Some(f)), None => (bare, None) };
    if !at.is_empty() && at.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
        let path: Vec<usize> = at.split('.').map(|p| p.parse::<usize>().unwrap_or(0)).collect();
        if path.contains(&0) { return Err(fail("bad_ref", format!("{a}: refs count from 1")).hint("#1 is the first block")); }
        let path: Vec<usize> = path.iter().map(|i| i - 1).collect();
        let here = all.iter().find(|it| it.path == path);
        let Some(fp) = fp else {
            return here.map(|it| it.block.id)
                .ok_or_else(|| fail("stale_ref", format!("no block at {a} ({} top-level blocks)", doc.blocks.len())).hint("the document may have changed; run `quiet outline` (or blocks()) for current refs"));
        };
        if let Some(it) = here.filter(|it| fingerprint(it.block) == fp) { return Ok(it.block.id); }
        // It moved: the one block with that fingerprint.
        let same: Vec<&Item> = all.iter().filter(|it| fingerprint(it.block) == fp).collect();
        if same.len() == 1 { return Ok(same[0].block.id); }
        let stale = fail("stale_ref", match (same.len(), here) {
            (0, Some(it)) => format!("the block at #{at} is no longer the one you read (now: {} {:?})", kind_name(&it.block.kind), clip(&text_of(it.block), 60)),
            (0, None) => format!("no block at #{at} any more, and none like the one you read"),
            (n, _) => format!("{n} blocks match {a}, none at #{at}"),
        });
        return Err(stale.hint("the document changed since you read it: run `quiet outline` or `quiet blocks` again for current refs"));
    }
    let heads: Vec<&Item> = all.iter().filter(|it| heading_level(it.block).is_some()).collect();
    if let Some(it) = heads.iter().find(|it| text_of(it.block).trim() == bare) { return Ok(it.block.id); }
    let low = bare.to_lowercase();
    let hits: Vec<&&Item> = heads.iter().filter(|it| text_of(it.block).to_lowercase().contains(&low)).collect();
    match hits.len() {
        1 => Ok(hits[0].block.id),
        0 => Err(fail("bad_ref", format!("no block or heading matches {a:?}")).hint("use a ref like #3 or #2.1 from `quiet outline`, or a heading's text")),
        _ => Err(fail("ambiguous_ref", format!("{} headings contain {a:?}: {}", hits.len(), hits.iter().map(|it| format!("{} {}", pin(it), text_of(it.block))).collect::<Vec<_>>().join("; "))).hint("use one of the refs")),
    }
}

pub fn clip(s: &str, n: usize) -> String { let s = s.replace('\n', " "); if s.chars().count() > n { format!("{}…", s.chars().take(n).collect::<String>()) } else { s } }

/// A heading with the blocks after it up to the next heading at its level
/// or above, among its siblings; any other block alone.
pub fn section(doc: &Document, id: BlockId) -> Vec<BlockId> {
    let Some(path) = path_of(doc, id) else { return vec![id] };
    let mut list = &doc.blocks;
    for &i in &path[..path.len() - 1] { list = &list[i].children; }
    let i = path[path.len() - 1];
    let Some(level) = heading_level(&list[i]) else { return vec![id] };
    let mut out = vec![id];
    for b in &list[i + 1..] {
        if heading_level(b).is_some_and(|l| l <= level) { break; }
        out.push(b.id);
    }
    out
}

/// Markdown read in as blocks (blank lines separate paragraphs, as in a file).
pub fn from_markdown(md: &str) -> Vec<Block> {
    let off = fdl_core::paste::Off { file: true, ..Default::default() };
    fdl_core::paste::markdown_to_blocks(md, off)
}

/// One line of Markdown as rich text (inline styles and links).
pub fn inline_text(s: &str, code: bool) -> RichText { if code { RichText::plain(s) } else { fdl_core::paste::md_inline(s) } }

/// Every block, cells of tables included, for `f` to change.
pub fn each_block(list: &mut [Block], f: &mut dyn FnMut(&mut Block)) {
    for b in list {
        f(b);
        if let BlockKind::Table(t) = &mut b.kind { for cell in t.rows.iter_mut().flatten() { each_block(&mut cell.blocks, f); } }
        each_block(&mut b.children, f);
    }
}

fn mention(id: String, title: String) -> Marks {
    Marks { inline: Some(fdl_core::docxml::to_inline(InlineKind::DocLink(InlineDocLink { id, title }))), ..Default::default() }
}

/// `[[target]]` (which Markdown reads as a `quiet-wiki:` link) and links to
/// files in the library as mentions of those documents; `find` names the
/// document a target is (id, title). Wiki targets it does not know are
/// returned; other links stay links.
pub fn link_mentions(list: &mut [Block], convert: bool, find: &dyn Fn(&str) -> Option<(String, String)>) -> Vec<String> {
    let mut missing = vec![];
    each_block(list, &mut |b| {
        if matches!(b.kind, BlockKind::Code { .. }) || !b.text.runs.iter().any(|r| r.marks.link.is_some()) { return; }
        let mut out = RichText::default();
        let mut last: Option<&str> = None;
        for (r, m) in b.text.spans() {
            let seg = &b.text.text[r];
            let target = m.link.as_deref().and_then(|l| match l.strip_prefix("quiet-wiki:") {
                Some(w) => Some((w.split('|').next().unwrap_or(w).trim().to_string(), true)),
                None if !l.contains("://") && !l.starts_with(['#', '?']) && !l.starts_with("mailto:") => Some((fdl_core::markdown::percent_decode(l.trim_start_matches("./")), false)),
                None => None,
            });
            let Some((t, wiki)) = target else { last = None; out.push(seg, m.clone()); continue };
            match find(&t) {
                // Only checked: kept as the link it is.
                Some(_) if !convert => out.push(seg, m.clone()),
                // One mention for a link however its text is styled.
                Some(_) if last == m.link.as_deref() => {}
                Some((id, title)) => out.push("\u{fffc}", mention(id, title)),
                None => { if wiki && !missing.contains(&t) { missing.push(t); } out.push(seg, m.clone()); }
            }
            last = m.link.as_deref();
        }
        b.text = out;
    });
    missing
}

/// Mentions as numbered placeholders, `name` saying what each points at
/// (None: its title); `wiki_back` turns the Markdown written from them into
/// `[[name]]`, so what is read out can be written back with its links (and
/// the Markdown writer escapes nothing in the names).
pub fn mentions_as_wiki(list: &mut [Block], name: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let mut names = vec![];
    each_block(list, &mut |b| {
        if !b.text.text.contains('\u{fffc}') { return; }
        let mut out = RichText::default();
        for (r, m) in b.text.spans() {
            match m.inline.as_ref().and_then(|i| i.doc_link()) {
                Some(l) => for _ in b.text.text[r].chars() {
                    out.push(&format!("\u{e000}{}\u{e001}", names.len()), Marks::default());
                    names.push(name(&l.id).unwrap_or_else(|| l.title.clone()));
                },
                None => out.push(&b.text.text[r], m.clone()),
            }
        }
        b.text = out;
    });
    names
}

pub fn wiki_back(md: &str, names: &[String]) -> String {
    if names.is_empty() { return md.to_string(); }
    let mut out = String::with_capacity(md.len());
    let mut rest = md;
    while let Some(a) = rest.find('\u{e000}') {
        out.push_str(&rest[..a]);
        let after = &rest[a + '\u{e000}'.len_utf8()..];
        match after.find('\u{e001}').and_then(|e| after[..e].parse::<usize>().ok().map(|k| (k, e))) {
            Some((k, e)) if k < names.len() => { out.push_str(&format!("[[{}]]", names[k])); rest = &after[e + '\u{e001}'.len_utf8()..]; }
            _ => { out.push('\u{e000}'); rest = after; }
        }
    }
    out.push_str(rest);
    out
}
