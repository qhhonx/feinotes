//! Clipboard import: HTML (other apps, web pages) and Markdown to DocxXML
//! blocks. Reading the pasteboard itself is `clipboard.rs`.

use crate::model::*;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

pub fn html_to_blocks(html: &str) -> Vec<Block> {
    let Ok(dom) = html5ever::parse_document(RcDom::default(), Default::default()).from_utf8().read_from(&mut html.as_bytes()) else {
        return vec![];
    };
    let root = find(&dom.document, "body").unwrap_or_else(|| dom.document.clone());
    let mut out = vec![];
    container(&root, &Ctx::default(), &mut out);
    // Markdown typed into another app's rich text still converts (inline).
    if !has_class(&dom.document, "lark-doc") {
        reparse_markdown(&mut out);
    }
    out
}

fn has_class(n: &Handle, class: &str) -> bool {
    has_own_class(n, class) || n.children.borrow().iter().any(|c| has_class(c, class))
}

fn has_own_class(n: &Handle, class: &str) -> bool {
    attr(n, "class").is_some_and(|c| c.split_whitespace().any(|c| c == class))
}

/// A callout: colours snap to the nearest swatch, the emoji id
/// (`data-emoji-id`) to its character.
fn callout(n: &Handle, ctx: &Ctx) -> Block {
    let st = style(n);
    let get = |k: &str| st.iter().find(|(p, _)| p == k).and_then(|(_, v)| parse_color(v));
    let near = |c: (u8, u8, u8, f32), names: &mut dyn Iterator<Item = &str>, f: fn(&str) -> Option<crate::palette::Rgba>| {
        names
            .filter_map(|n| Some((n, f(n)?)))
            .min_by_key(|(_, p)| {
                let d = |a: u8, b: u8| (a as i32 - b as i32).pow(2);
                d(c.0, p.r) + d(c.1, p.g) + d(c.2, p.b)
            })
            .map(|(n, _)| ColorName(n.into()))
    };
    let bg = get("background-color").and_then(|c| near(c, &mut crate::palette::CALLOUT_BG_COLORS.iter().flatten().copied(), crate::palette::callout_bg));
    let border = get("border-color").and_then(|c| near(c, &mut crate::palette::HUES.iter().copied(), crate::palette::callout_border));
    let emoji = n
        .parent
        .take()
        .and_then(|w| {
            let p = w.upgrade();
            n.parent.set(Some(w));
            p
        })
        .and_then(|p| attr(&p, "data-emoji-id"))
        .and_then(|id| crate::emoji::by_id(&id))
        .unwrap_or("💡");
    let mut kids = vec![];
    container(n, ctx, &mut kids);
    if kids.is_empty() {
        kids.push(Block::text(""));
    }
    Block::new(BlockKind::Callout { emoji: emoji.into(), bg, border, text_color: None }, RichText::default()).with_children(kids)
}

/// Unmarked runs of text blocks are read as inline Markdown.
fn reparse_markdown(blocks: &mut [Block]) {
    for b in blocks {
        reparse_markdown(&mut b.children);
        if let BlockKind::Table(t) = &mut b.kind {
            for cell in t.rows.iter_mut().flatten() {
                reparse_markdown(&mut cell.blocks);
            }
        }
        if !b.kind.has_text() || matches!(b.kind, BlockKind::Code { .. }) {
            continue;
        }
        let mut out = RichText::default();
        for (r, m) in b.text.spans() {
            if *m == Marks::default() {
                out.append(&md_inline(&b.text.text[r]));
            } else {
                out.append(&b.text.slice(r));
            }
        }
        b.text = out;
    }
}

/// Content that never becomes document text.
const SKIP: &[&str] = &[
    "head", "title", "meta", "link", "style", "script", "noscript", "template", "iframe", "object", "embed", "video", "audio",
    "canvas", "svg", "math", "button", "select", "option", "textarea", "input", "map", "area", "source", "track",
];

/// Elements that break the inline flow (their content forms new paragraphs).
const BLOCK: &[&str] = &[
    "p", "div", "section", "article", "header", "footer", "main", "aside", "nav", "figure", "figcaption", "center", "address",
    "dl", "dt", "dd", "form", "fieldset", "legend", "details", "summary", "hgroup", "html", "body", "tbody", "thead", "tfoot", "tr",
    "td", "th", "caption",
];

#[derive(Clone, Default)]
struct Ctx {
    marks: Marks,
    align: Option<Align>,
}

fn tag(n: &Handle) -> Option<String> {
    match &n.data {
        NodeData::Element { name, .. } => Some(name.local.to_string().to_ascii_lowercase()),
        _ => None,
    }
}

fn attr(n: &Handle, key: &str) -> Option<String> {
    match &n.data {
        NodeData::Element { attrs, .. } => attrs.borrow().iter().find(|a| &*a.name.local == key).map(|a| a.value.to_string()),
        _ => None,
    }
}

fn find(n: &Handle, name: &str) -> Option<Handle> {
    if tag(n).as_deref() == Some(name) {
        return Some(n.clone());
    }
    n.children.borrow().iter().find_map(|c| find(c, name))
}

/// Declarations of the inline `style` attribute, lowercased.
fn style(n: &Handle) -> Vec<(String, String)> {
    attr(n, "style")
        .unwrap_or_default()
        .split(';')
        .filter_map(|d| {
            let (k, v) = d.split_once(':')?;
            Some((k.trim().to_ascii_lowercase(), v.trim().to_ascii_lowercase()))
        })
        .collect()
}

fn font_px(st: &[(String, String)]) -> Option<f32> {
    let v = &st.iter().find(|(k, _)| k == "font-size")?.1;
    if let Some(n) = v.strip_suffix("px") {
        return n.trim().parse().ok();
    }
    v.strip_suffix("pt").and_then(|n| n.trim().parse::<f32>().ok()).map(|pt| pt * 4. / 3.)
}

fn hidden(st: &[(String, String)]) -> bool {
    st.iter().any(|(k, v)| (k == "display" && v.starts_with("none")) || (k == "visibility" && v == "hidden"))
}

fn align_of(n: &Handle, st: &[(String, String)]) -> Option<Align> {
    let v = st.iter().find(|(k, _)| k == "text-align").map(|(_, v)| v.clone()).or_else(|| attr(n, "align"))?;
    match v.trim().to_ascii_lowercase().as_str() {
        "center" | "-webkit-center" => Some(Align::Center),
        "right" | "end" | "-webkit-right" => Some(Align::Right),
        "left" | "start" | "justify" => Some(Align::Left),
        _ => None,
    }
}

/// `ctx` with the marks an inline element and its style add.
fn inline_ctx(n: &Handle, t: &str, st: &[(String, String)], ctx: &Ctx) -> Ctx {
    let mut c = ctx.clone();
    let m = &mut c.marks;
    match t {
        "b" | "strong" => m.bold = true,
        "i" | "em" | "cite" | "dfn" | "var" => m.italic = true,
        "u" | "ins" => m.underline = true,
        "s" | "strike" | "del" => m.strike = true,
        "code" | "kbd" | "samp" | "tt" => m.code = true,
        "a" => {
            if let Some(h) = attr(n, "href").map(|h| h.trim().to_string()).filter(|h| link_ok(h)) {
                m.link = Some(h);
            }
        }
        _ => {}
    }
    for (k, v) in st {
        match k.as_str() {
            "font-weight" => {
                m.bold = match v.as_str() {
                    "bold" | "bolder" => true,
                    "normal" | "lighter" => false,
                    n => n.parse::<u32>().map_or(m.bold, |w| w >= 700),
                }
            }
            "font-style" => m.italic = v.starts_with("italic") || v.starts_with("oblique"),
            "text-decoration" | "text-decoration-line" => {
                m.underline |= v.contains("underline");
                m.strike |= v.contains("line-through");
            }
            // Only colors of the palette survive.
            "color" => {
                if let Some(n) = parse_color(v).and_then(palette_text) {
                    m.text_color = n;
                }
            }
            "background-color" | "background" => {
                if let Some(n) = parse_color(v).and_then(palette_bg) {
                    m.bg_color = n;
                }
            }
            _ => {}
        }
    }
    c
}

/// `#rgb`, `#rrggbb`, `rgb(…)` / `rgba(…)` → (r, g, b, alpha).
fn parse_color(v: &str) -> Option<(u8, u8, u8, f32)> {
    let v = v.trim().trim_end_matches("!important").trim();
    if let Some(h) = v.strip_prefix('#') {
        let x = |s: &str| u8::from_str_radix(s, 16).ok();
        return match h.len() {
            3 => {
                let d: Vec<u8> = h.chars().map(|c| x(&c.to_string()).map(|n| n * 17)).collect::<Option<_>>()?;
                Some((d[0], d[1], d[2], 1.))
            }
            6 => Some((x(&h[0..2])?, x(&h[2..4])?, x(&h[4..6])?, 1.)),
            _ => None,
        };
    }
    let inner = v.strip_prefix("rgba(").or_else(|| v.strip_prefix("rgb("))?.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split([',', ' ', '/']).filter(|p| !p.is_empty()).collect();
    let n = |i: usize| parts.get(i)?.trim().parse::<f32>().ok();
    Some((n(0)? as u8, n(1)? as u8, n(2)? as u8, parts.get(3).map_or(Some(1.), |_| n(3))?))
}

fn same(a: (u8, u8, u8, f32), b: (u8, u8, u8, f32)) -> bool {
    a.0 == b.0 && a.1 == b.1 && a.2 == b.2 && (a.3 - b.3).abs() < 0.02
}

/// Some(None) is the default text color.
fn palette_text(c: (u8, u8, u8, f32)) -> Option<Option<ColorName>> {
    crate::palette::FONT_COLORS
        .iter()
        .find(|(_, v)| same(c, ((v >> 16) as u8, (v >> 8) as u8, *v as u8, 1.)))
        .map(|(n, _)| n.map(|n| ColorName(n.into())))
}

fn palette_bg(c: (u8, u8, u8, f32)) -> Option<Option<ColorName>> {
    if c.3 == 0. {
        return Some(None);
    }
    crate::palette::BG_COLORS
        .iter()
        .flatten()
        .find(|n| crate::palette::bg_color(n).is_some_and(|p| same(c, (p.r, p.g, p.b, p.a))))
        .map(|n| Some(ColorName(n.to_string())))
}

pub fn link_ok(h: &str) -> bool {
    let l = h.to_ascii_lowercase();
    ["http://", "https://", "mailto:", "ftp://"].iter().any(|p| l.starts_with(p))
}

/// Relative Markdown file references, including fragments on the current page.
pub fn relative_link(s: &str) -> bool {
    !s.is_empty() && !s.contains([':', '\\']) && !s.starts_with('/') && !s.chars().any(char::is_control)
        && (s.starts_with(['.', '#']) || s.contains('/') || s.split('#').next().and_then(|p| std::path::Path::new(p).extension()).is_some())
}

fn container(n: &Handle, ctx: &Ctx, out: &mut Vec<Block>) {
    let mut para = RichText::default();
    for c in n.children.borrow().iter() {
        node(c, ctx, out, &mut para);
    }
    flush(&mut para, ctx, out);
}

fn node(n: &Handle, ctx: &Ctx, out: &mut Vec<Block>, para: &mut RichText) {
    match &n.data {
        NodeData::Text { contents } => push_text(para, &contents.borrow(), &ctx.marks),
        NodeData::Element { .. } => {
            let t = tag(n).unwrap_or_default();
            let st = style(n);
            if SKIP.contains(&t.as_str()) || hidden(&st) {
                return;
            }
            let mut bctx = ctx.clone();
            if let Some(a) = align_of(n, &st) {
                bctx.align = Some(a);
            }
            match t.as_str() {
                "br" => {
                    trim_end_spaces(para);
                    para.push("\n", ctx.marks.clone());
                }
                // Superscript / subscript text is wrapped in parentheses.
                "sup" | "sub" => {
                    let c = inline_ctx(n, &t, &st, ctx);
                    para.push("(", c.marks.clone());
                    for ch in n.children.borrow().iter() {
                        node(ch, &c, out, para);
                    }
                    para.push(")", c.marks.clone());
                }
                "img" => {
                    flush(para, ctx, out);
                    out.extend(image(n));
                }
                "hr" => {
                    flush(para, ctx, out);
                    out.push(Block::new(BlockKind::Divider, RichText::default()));
                }
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    flush(para, ctx, out);
                    let level = t[1..].parse().unwrap_or(1);
                    let mut kids = vec![];
                    container(n, &bctx, &mut kids);
                    let mut text = RichText::default();
                    for (i, k) in kids.iter().filter(|k| k.kind.has_text()).enumerate() {
                        if i > 0 {
                            text.push("\n", Marks::default());
                        }
                        text.append(&k.text);
                    }
                    if !text.is_empty() {
                        let mut b = Block::new(BlockKind::Heading { level, seq_auto: false }, text);
                        b.align = bctx.align.unwrap_or_default();
                        out.push(b);
                    }
                    out.extend(kids.into_iter().filter(|k| !k.kind.has_text()));
                }
                "ul" | "ol" | "menu" => {
                    flush(para, ctx, out);
                    list(n, &bctx, out, t == "ol");
                }
                "li" => {
                    flush(para, ctx, out);
                    out.push(item(n, &bctx, false, None));
                }
                "blockquote" => {
                    flush(para, ctx, out);
                    let mut kids = vec![];
                    container(n, &bctx, &mut kids);
                    if !kids.is_empty() {
                        out.push(Block::new(BlockKind::Quote, RichText::default()).with_children(kids));
                    }
                }
                "pre" if code_lang(n).is_some() || find(n, "code").is_some() => {
                    flush(para, ctx, out);
                    out.push(code(n));
                }
                // A `<pre>` that isn't code is text with its line breaks.
                "pre" => {
                    flush(para, ctx, out);
                    let mut text = String::new();
                    raw_text(n, &mut text);
                    for line in text.replace("\r\n", "\n").trim_end_matches('\n').split('\n') {
                        let mut b = Block::new(BlockKind::Text, RichText::default());
                        b.text.push(&line.replace('\t', " "), ctx.marks.clone());
                        b.align = bctx.align.unwrap_or_default();
                        out.push(b);
                    }
                }
                "table" => {
                    flush(para, ctx, out);
                    out.extend(table(n, &bctx));
                }
                // 高亮块: `.callout-block` carries the colours, the container
                // above it the emoji id.
                _ if has_own_class(n, "callout-block") => {
                    flush(para, ctx, out);
                    out.push(callout(n, &bctx));
                }
                _ if BLOCK.contains(&t.as_str()) => {
                    flush(para, ctx, out);
                    let before = out.len();
                    container(n, &bctx, out);
                    // Big text (a styled title) reads as a heading 1.
                    if font_px(&st).is_some_and(|px| px >= 20.) {
                        for b in &mut out[before..] {
                            if matches!(b.kind, BlockKind::Text) && b.children.is_empty() {
                                b.kind = BlockKind::Heading { level: 1, seq_auto: false };
                            }
                        }
                    }
                    // An empty paragraph is a blank line.
                    if out.len() == before && t == "p" {
                        let mut b = Block::text("");
                        b.align = bctx.align.unwrap_or_default();
                        out.push(b);
                    }
                }
                _ => {
                    let c = inline_ctx(n, &t, &st, ctx);
                    for ch in n.children.borrow().iter() {
                        node(ch, &c, out, para);
                    }
                }
            }
        }
        _ => {
            for ch in n.children.borrow().iter() {
                node(ch, ctx, out, para);
            }
        }
    }
}

/// HTML whitespace collapsing: runs become one space, none at line start.
fn push_text(para: &mut RichText, s: &str, marks: &Marks) {
    let mut t = String::with_capacity(s.len());
    let mut space = para.text.is_empty() || para.text.ends_with([' ', '\n']);
    for c in s.chars() {
        if c.is_ascii_whitespace() {
            if !space {
                t.push(' ');
                space = true;
            }
        } else {
            t.push(if c == '\u{a0}' { ' ' } else { c });
            space = false;
        }
    }
    if !t.is_empty() {
        para.push(&t, marks.clone());
    }
}

fn trim_end_spaces(t: &mut RichText) {
    let end = t.text.trim_end_matches(' ').len();
    if end < t.text.len() {
        *t = t.slice(0..end);
    }
}

fn flush(para: &mut RichText, ctx: &Ctx, out: &mut Vec<Block>) {
    let t = std::mem::take(para);
    let start = t.text.len() - t.text.trim_start_matches([' ', '\n']).len();
    let end = t.text.trim_end_matches([' ', '\n']).len();
    if end <= start {
        return;
    }
    // A `<br>` starts a new block.
    let t = t.slice(start..end);
    let mut at = 0;
    for line in t.text.split('\n') {
        let mut b = Block::new(BlockKind::Text, t.slice(at..at + line.len()));
        b.align = ctx.align.unwrap_or_default();
        out.push(b);
        at += line.len() + 1;
    }
}

fn list(n: &Handle, ctx: &Ctx, out: &mut Vec<Block>, ordered: bool) {
    let start: Option<u32> = attr(n, "start").and_then(|s| s.trim().parse().ok()).filter(|&s| s != 1);
    let mut items: Vec<Block> = vec![];
    for c in n.children.borrow().iter() {
        match tag(c).as_deref() {
            Some("li") => {
                let seq = if items.is_empty() { start } else { None };
                items.push(item(c, ctx, ordered, seq));
            }
            // A list nested straight in a list belongs to the item before it.
            Some(t @ ("ul" | "ol")) => {
                let mut kids = vec![];
                list(c, ctx, &mut kids, t == "ol");
                match items.last_mut() {
                    Some(last) => last.children.extend(kids),
                    None => items.extend(kids),
                }
            }
            Some(_) => {
                let mut kids = vec![];
                container(c, ctx, &mut kids);
                items.extend(kids);
            }
            None => {}
        }
    }
    out.extend(items);
}

/// One `<li>`: its first paragraph is the item's text, the rest children.
fn item(li: &Handle, ctx: &Ctx, ordered: bool, seq: Option<u32>) -> Block {
    let kind = match checkbox(li) {
        Some(done) => BlockKind::Todo { done },
        None if ordered => BlockKind::Ordered { seq: attr(li, "value").and_then(|v| v.trim().parse().ok()).or(seq) },
        None => BlockKind::Bullet,
    };
    let mut kids = vec![];
    container(li, ctx, &mut kids);
    let text = match kids.first() {
        Some(b) if matches!(b.kind, BlockKind::Text) && b.children.is_empty() => kids.remove(0).text,
        _ => RichText::default(),
    };
    Block::new(kind, text).with_children(kids)
}

/// `Some(checked)` when the item carries a checkbox of its own (GFM task lists).
fn checkbox(li: &Handle) -> Option<bool> {
    // `<li data-list="done">` (checked) / `"check"` (inferred, unchecked).
    match attr(li, "data-list").as_deref() {
        Some("done") => return Some(true),
        Some("check" | "undone" | "todo") => return Some(false),
        _ => {}
    }
    fn walk(n: &Handle) -> Option<bool> {
        for c in n.children.borrow().iter() {
            match tag(c).as_deref() {
                Some("ul" | "ol") => {}
                Some("input") if attr(c, "type").is_some_and(|t| t.eq_ignore_ascii_case("checkbox")) => {
                    return Some(attr(c, "checked").is_some());
                }
                Some(_) => {
                    if let Some(v) = walk(c) {
                        return Some(v);
                    }
                }
                None => {}
            }
        }
        None
    }
    walk(li)
}

fn raw_text(n: &Handle, out: &mut String) {
    match &n.data {
        NodeData::Text { contents } => out.push_str(&contents.borrow().replace('\u{a0}', " ")),
        NodeData::Element { .. } => match tag(n).as_deref() {
            Some("br") => out.push('\n'),
            Some(t) if SKIP.contains(&t) => {}
            Some(t) => {
                let block = matches!(t, "div" | "p" | "li" | "tr");
                if block && !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                for c in n.children.borrow().iter() {
                    raw_text(c, out);
                }
            }
            None => {}
        },
        _ => {
            for c in n.children.borrow().iter() {
                raw_text(c, out);
            }
        }
    }
}

fn code(pre: &Handle) -> Block {
    let mut text = String::new();
    raw_text(pre, &mut text);
    let text = text.replace("\r\n", "\n");
    let text = text.strip_suffix('\n').unwrap_or(&text).to_string();
    let lang = code_lang(pre).map_or(crate::lang::from_token(""), |l| crate::lang::resolve(&l).to_string());
    Block::new(BlockKind::Code { lang, caption: None, wrap: false, show_code: false }, RichText::plain(text))
}

/// Language from `language-x` / `lang-x` / `highlight-source-x` classes or a
/// `lang` attribute on the `<pre>`, its `<code>` or a wrapping element.
fn code_lang(pre: &Handle) -> Option<String> {
    let mut nodes = vec![pre.clone()];
    nodes.extend(pre.children.borrow().iter().filter(|c| tag(c).as_deref() == Some("code")).cloned());
    if let Some(p) = pre.parent.take() {
        if let Some(p) = p.upgrade() {
            nodes.push(p);
        }
        pre.parent.set(Some(p));
    }
    for n in &nodes {
        for key in ["data-lark-language", "data-lang", "data-language"] {
            if let Some(v) = attr(n, key).filter(|v| !v.trim().is_empty()) {
                return Some(v);
            }
        }
        let classes = attr(n, "class").unwrap_or_default();
        let classes: Vec<&str> = classes.split_whitespace().collect();
        if classes.contains(&"hljs")
            && let Some(l) = classes.iter().find(|c| **c != "hljs" && !c.starts_with("hljs-"))
        {
            return Some(l.to_string());
        }
        for c in classes {
            for p in ["language-", "lang-", "highlight-source-", "data-lark-language-"] {
                if let Some(l) = c.strip_prefix(p).filter(|l| !l.is_empty()) {
                    return Some(l.to_string());
                }
            }
        }
    }
    None
}

fn image(n: &Handle) -> Option<Block> {
    let src = attr(n, "src")?.trim().to_string();
    let l = src.to_ascii_lowercase();
    let src = if l.starts_with("http://") || l.starts_with("https://") || l.starts_with("data:image/") {
        src
    } else {
        let p = src.strip_prefix("file://")?;
        percent_decode(p)
    };
    let mut b = Block::new(
        BlockKind::Image { src_kind: "src".into(), src, width: None, height: None, caption: None, crop: None },
        RichText::default(),
    );
    b.align = Align::Center;
    Some(b)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && s.is_char_boundary(i + 3)
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn width_px(n: &Handle) -> Option<f32> {
    let st = style(n);
    let v = st.iter().find(|(k, _)| k == "width").map(|(_, v)| v.clone()).or_else(|| attr(n, "width"))?;
    let v = v.trim();
    v.strip_suffix("px").unwrap_or(v).trim().parse::<f32>().ok().filter(|w| *w > 0.)
}

/// Column widths from `<col>`s, else the first row's single-column cells.
fn col_widths(table: &Handle, trs: &[(Handle, bool)], ncols: usize) -> Vec<f32> {
    let mut w: Vec<Option<f32>> = vec![None; ncols];
    let mut cols = vec![];
    for c in table.children.borrow().iter() {
        match tag(c).as_deref() {
            Some("colgroup") => cols.extend(c.children.borrow().iter().filter(|x| tag(x).as_deref() == Some("col")).cloned()),
            Some("col") => cols.push(c.clone()),
            _ => {}
        }
    }
    let mut i = 0;
    for col in &cols {
        let span: usize = attr(col, "span").and_then(|s| s.parse().ok()).unwrap_or(1);
        for _ in 0..span {
            if let Some(x) = w.get_mut(i) {
                *x = width_px(col);
            }
            i += 1;
        }
    }
    if let Some((tr, _)) = trs.first() {
        let mut c = 0;
        for td in tr.children.borrow().iter().filter(|x| matches!(tag(x).as_deref(), Some("td" | "th"))) {
            let span: usize = attr(td, "colspan").and_then(|s| s.trim().parse().ok()).unwrap_or(1).max(1);
            if span == 1
                && let Some(x) = w.get_mut(c)
                && x.is_none()
            {
                *x = width_px(td);
            }
            c += span;
        }
    }
    w.into_iter().map(|x| x.map_or(Table::DEFAULT_WIDTH, |x| x.round().max(Table::MIN_WIDTH))).collect()
}

fn table(n: &Handle, ctx: &Ctx) -> Option<Block> {
    fn rows_of(n: &Handle, out: &mut Vec<(Handle, bool)>, head: bool) {
        for c in n.children.borrow().iter() {
            match tag(c).as_deref() {
                Some("thead") => rows_of(c, out, true),
                Some("tbody" | "tfoot") => rows_of(c, out, head),
                Some("tr") => out.push((c.clone(), head)),
                _ => {}
            }
        }
    }
    let mut trs = vec![];
    rows_of(n, &mut trs, false);
    let mut rows: Vec<Vec<TableCell>> = vec![];
    let mut taken: Vec<Vec<bool>> = vec![];
    let mut all_th = vec![];
    for (r, (tr, _)) in trs.iter().enumerate() {
        if rows.len() <= r {
            rows.resize_with(r + 1, Vec::new);
            taken.resize_with(r + 1, Vec::new);
        }
        let mut c = 0;
        let mut th = true;
        for td in tr.children.borrow().iter() {
            let t = tag(td).unwrap_or_default();
            if t != "td" && t != "th" {
                continue;
            }
            th &= t == "th";
            while taken[r].get(c).copied().unwrap_or(false) {
                c += 1;
            }
            let colspan: usize = attr(td, "colspan").and_then(|v| v.trim().parse().ok()).unwrap_or(1).clamp(1, 64);
            let rowspan: usize = attr(td, "rowspan").and_then(|v| v.trim().parse().ok()).unwrap_or(1).clamp(1, trs.len() - r);
            for rr in r..r + rowspan {
                if rows.len() <= rr {
                    rows.resize_with(rr + 1, Vec::new);
                    taken.resize_with(rr + 1, Vec::new);
                }
                if taken[rr].len() < c + colspan {
                    taken[rr].resize(c + colspan, false);
                }
                if rows[rr].len() < c + colspan {
                    rows[rr].resize_with(c + colspan, TableCell::covered);
                }
                for cc in c..c + colspan {
                    taken[rr][cc] = true;
                }
            }
            let mut blocks = vec![];
            container(td, &Ctx { align: align_of(td, &style(td)).or(ctx.align), ..Ctx::default() }, &mut blocks);
            // Cells hold no nested tables.
            blocks.retain(|b| !matches!(b.kind, BlockKind::Table(_)));
            if blocks.is_empty() {
                blocks.push(Block::text(""));
            }
            rows[r][c] = TableCell { blocks, colspan: colspan as u32, rowspan: rowspan as u32, ..TableCell::empty() };
            c += colspan;
        }
        all_th.push(th && c > 0);
    }
    let ncols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if ncols == 0 {
        return None;
    }
    for (r, row) in rows.iter_mut().enumerate() {
        row.resize_with(ncols, TableCell::covered);
        for (c, cell) in row.iter_mut().enumerate() {
            if cell.is_covered() && !taken[r].get(c).copied().unwrap_or(false) {
                *cell = TableCell::empty();
            }
        }
    }
    let head = trs.iter().take_while(|(_, h)| *h).count();
    let header_rows = if head > 0 { head } else { usize::from(all_th.first().copied().unwrap_or(false)) };
    let mut t = Table { col_widths: col_widths(n, &trs, ncols), header_rows, header_col: false, rows };
    t.repair();
    Some(Block::new(BlockKind::Table(t), RichText::default()))
}

// ---- Markdown (plain-text paste, behavior-spec §2.5)

/// Parses Markdown into blocks with the same rules as the live shortcuts plus
/// fences, nesting by indentation, pipe tables, images and links.
pub fn markdown_to_blocks(text: &str, off: Off) -> Vec<Block> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.trim_end_matches('\n').split('\n').collect();
    md_blocks(&lines, off, None)
}

/// `markdown_to_blocks`, and the lines (`[start, end)` of `text` split at
/// `\n`, CRs removed) each top-level block was read from.
pub fn markdown_to_blocks_with_lines(text: &str, off: Off) -> (Vec<Block>, Vec<(usize, usize)>) {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.trim_end_matches('\n').split('\n').collect();
    let mut spans = vec![];
    let blocks = md_blocks(&lines, off, Some(&mut spans));
    (blocks, spans)
}

/// Inline rules only (pasting into a non-empty block): one block per line,
/// except a display equation, which becomes its own block.
pub fn markdown_inline_lines(text: &str) -> Vec<Block> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.trim_end_matches('\n').split('\n').collect();
    let mut out = vec![];
    let mut i = 0;
    while i < lines.len() {
        if let Some((b, n)) = display_math(&lines[i..]) {
            if i == 0 {
                out.push(Block::text(""));
            }
            out.push(b);
            i += n;
        } else {
            out.push(Block::new(BlockKind::Text, md_inline(&tabs(lines[i]))));
            i += 1;
        }
    }
    out
}

pub fn has_math(text: &str) -> bool {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.trim_end_matches('\n').split('\n').collect();
    let mut in_fence = None;
    for (i, line) in lines.iter().enumerate() {
        if let Some((open, n, _)) = in_fence {
            if let Some((close, n2, rest)) = fence(line) && close == open && n2 >= n && rest.is_empty() {
                in_fence = None;
            }
            continue;
        }
        if let Some((open, n, _)) = fence(line) {
            in_fence = Some((open, n, ""));
            continue;
        }
        let math = (line.trim_start().starts_with("$$") && display_math(&lines[i..]).is_some())
            || md_inline(&tabs(line)).runs.iter().any(|r| r.marks.inline.is_some());
        if math {
            return true;
        }
    }
    false
}

/// Runs of tabs become one space.
fn tabs(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c != '\t' {
            out.push(c);
        } else if !out.ends_with('\t') {
            out.push('\t');
        }
    }
    out.replace('\t', " ")
}

/// Rules switched off by the paste target (a list item doesn't re-detect its
/// own kind, a quote no quotes, a callout no code or dividers).
#[derive(Clone, Copy, Default)]
pub struct Off {
    pub bullet: bool,
    pub ordered: bool,
    pub todo: bool,
    pub quote: bool,
    pub code_divider: bool,
    /// Importing a whole file: blank lines only separate blocks, and a line
    /// holding just `![alt](src)` is an image.
    pub file: bool,
    /// A Markdown file edited as itself: an image keeps its alt text (as its caption).
    pub alt: bool,
}

fn indent_of(line: &str) -> usize {
    let mut n = 0;
    for c in line.chars() {
        match c {
            ' ' => n += 1,
            '\t' => n += 4 - n % 4,
            _ => break,
        }
    }
    n
}

/// `line` without its first `n` columns of indentation.
fn dedent(line: &str, n: usize) -> &str {
    let mut col = 0;
    for (i, c) in line.char_indices() {
        if col >= n {
            return &line[i..];
        }
        match c {
            ' ' => col += 1,
            '\t' => col += 4 - col % 4,
            _ => return &line[i..],
        }
    }
    ""
}

enum Marker {
    Bullet,
    Ordered(u32),
}

/// A list item line: (marker, content column, content).
fn list_item(line: &str) -> Option<(Marker, usize, &str)> {
    let ind = indent_of(line);
    let rest = line.trim_start_matches([' ', '\t']);
    let skip = line.len() - rest.len();
    // Full-width as typed with a Chinese input method: `－ ` / `＊ ` bullets, `1．` like `1。`.
    let (marker, mlen, tight) = if let Some(c) = rest.chars().next().filter(|c| matches!(c, '-' | '*' | '+' | '－' | '＊')) {
        (Marker::Bullet, c.len_utf8(), false)
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || digits > 9 {
            return None;
        }
        let tail = &rest[digits..];
        let d = tail.chars().next().filter(|c| matches!(c, '.' | ')' | '。' | '．'))?;
        // `1。有序` needs no space, but `2024。` or `3．14` starts no list.
        let next = tail[d.len_utf8()..].chars().next();
        let tight = matches!(d, '。' | '．') && digits <= 2 && next.is_some_and(|c| !c.is_whitespace() && !c.is_ascii_digit());
        (Marker::Ordered(rest[..digits].parse().ok()?), digits + d.len_utf8(), tight)
    };
    let after = &rest[mlen..];
    let content = if tight { after } else { after.strip_prefix([' ', '\t'])? };
    // `***` / `- - -` style rules are dividers, not items.
    if divider(rest) {
        return None;
    }
    let _ = skip;
    Some((marker, ind + mlen + usize::from(!tight), content))
}

fn divider(line: &str) -> bool {
    let t: String = line.trim().chars().filter(|c| *c != ' ').collect();
    matches!(t.as_str(), "---" | "***" | "___") || (t.len() > 3 && t.len() < 6 && t.chars().all(|c| c == '-'))
}

fn fence(line: &str) -> Option<(char, usize, &str)> {
    let t = line.trim_start();
    let c = t.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let n = t.chars().take_while(|&x| x == c).count();
    (n >= 3).then(|| (c, n, t[n..].trim()))
}

fn display_math(lines: &[&str]) -> Option<(Block, usize)> {
    let first = lines.first()?.trim();
    if !first.starts_with("$$") {
        return None;
    }
    let single = first.len() > 4 && first.ends_with("$$");
    let (mut src, end) = if single {
        (first[2..first.len() - 2].to_string(), 1)
    } else {
        let first = first[2..].to_string();
        let end = lines.iter().skip(1).position(|l| {
            let l = l.trim();
            l == "$$" || (l.len() > 4 && l.ends_with("$$"))
        })? + 1;
        let mut parts = vec![first];
        parts.extend(lines[1..end].iter().map(|l| tabs(l)));
        let closing = lines[end - 1].trim();
        let last = if closing == "$$" { String::new() } else { closing.strip_suffix("$$").unwrap_or(closing).to_string() };
        *parts.last_mut().unwrap() = last;
        (parts.join("\n"), end + 1)
    };
    src = markdown_equation_source(src.trim());
    if src.is_empty() {
        return None;
    }
    let mut text = RichText::default();
    text.push(crate::editor::core::EQUATION, Marks { inline: Some(crate::docxml::to_inline(InlineKind::Equation(src.to_string()))), ..Marks::default() });
    let mut b = Block::new(BlockKind::Text, text);
    b.align = Align::Center;
    Some((b, end))
}

fn todo_prefix(s: &str) -> Option<(bool, &str)> {
    for (p, done) in [("[ ] ", false), ("[] ", false), ("【】 ", false), ("[x] ", true), ("[X] ", true), ("【x】 ", true)] {
        if let Some(r) = s.strip_prefix(p) {
            return Some((done, r));
        }
    }
    None
}

fn md_blocks(lines: &[&str], off: Off, mut spans: Option<&mut Vec<(usize, usize)>>) -> Vec<Block> {
    let mut out: Vec<Block> = vec![];
    let mut i = 0;
    // Ordered items continue the previous item's numbering.
    let mut last_list = false;
    // Each pass adds at most one block: (blocks, line) when the pass began.
    let mut mark = (0, 0);
    loop {
        if let Some(s) = spans.as_deref_mut() && out.len() > mark.0 { s.push((mark.1, i)); }
        mark = (out.len(), i);
        if i >= lines.len() { break; }
        let line = lines[i];
        // Blank lines are empty paragraphs (none before the first block).
        if line.trim().is_empty() {
            if !out.is_empty() && !off.file {
                out.push(Block::text(""));
            }
            last_list = false;
            i += 1;
            continue;
        }
        let ind = indent_of(line);
        let body = line.trim();
        // An empty paragraph, as Markdown files are written (`markdown::BLANK`).
        if body == crate::markdown::BLANK {
            out.push(Block::text(""));
            last_list = false;
            i += 1;
            continue;
        }
        if let Some((b, n)) = display_math(&lines[i..]) {
            out.push(b);
            i += n;
            last_list = false;
            continue;
        }
        // Fenced code (closed or running to the end).
        if let Some((fc, n, info)) = fence(line).filter(|_| !off.code_divider) {
            let mut code = vec![];
            let mut j = i + 1;
            while j < lines.len() {
                if let Some((c2, n2, rest)) = fence(lines[j])
                    && c2 == fc
                    && n2 >= n
                    && rest.is_empty()
                {
                    break;
                }
                code.push(dedent(lines[j], ind));
                j += 1;
            }
            let lang = crate::lang::resolve(info.split_whitespace().next().unwrap_or("")).to_string();
            out.push(Block::new(BlockKind::Code { lang, caption: None, wrap: false, show_code: false }, RichText::plain(code.join("\n"))));
            i = j + 1;
            last_list = false;
            continue;
        }
        if let Some((marker, col, content)) = list_item(line).filter(|(m, _, c)| {
            let todo = todo_prefix(c).is_some();
            !(todo && off.todo || !todo && matches!(m, Marker::Bullet) && off.bullet || !todo && matches!(m, Marker::Ordered(_)) && off.ordered)
        }) {
            // The item's own lines: anything indented past its marker.
            let mut j = i + 1;
            let mut sub: Vec<&str> = vec![];
            while j < lines.len() {
                let l = lines[j];
                if l.trim().is_empty() {
                    // A blank line ends the item unless indented content follows.
                    if lines.get(j + 1).is_some_and(|n| !n.trim().is_empty() && indent_of(n) > ind) {
                        j += 1;
                        continue;
                    }
                    break;
                }
                if indent_of(l) <= ind {
                    break;
                }
                sub.push(dedent(l, col.min(indent_of(l))));
                j += 1;
            }
            let (kind, text) = match todo_prefix(content) {
                Some((done, rest)) => (BlockKind::Todo { done }, rest),
                None => match marker {
                    Marker::Bullet => (BlockKind::Bullet, content),
                    Marker::Ordered(n) => {
                        let cont = last_list && matches!(out.last().map(|b| &b.kind), Some(BlockKind::Ordered { .. }));
                        (BlockKind::Ordered { seq: (!cont && n != 1).then_some(n) }, content)
                    }
                },
            };
            out.push(Block::new(kind, md_inline(text.trim())).with_children(md_blocks(&sub, off, None)));
            last_list = true;
            i = j;
            continue;
        }
        last_list = false;
        if let Some((done, rest)) = todo_prefix(body).filter(|_| !off.todo) {
            out.push(Block::new(BlockKind::Todo { done }, md_inline(rest.trim())));
            i += 1;
            continue;
        }
        // `＃` as typed with a Chinese input method counts too.
        let hashes = body.chars().take_while(|&c| c == '#' || c == '＃').count();
        let hlen: usize = body.chars().take(hashes).map(char::len_utf8).sum();
        if (1..=9).contains(&hashes) && body[hlen..].starts_with([' ', '\t']) {
            let t = body[hlen..].trim().trim_end_matches(['#', '＃']).trim_end();
            out.push(Block::new(BlockKind::Heading { level: hashes as u8, seq_auto: false }, md_inline(t)));
            i += 1;
            continue;
        }
        if divider(body) && !off.code_divider {
            out.push(Block::new(BlockKind::Divider, RichText::default()));
            i += 1;
            continue;
        }
        if quote_line(body).is_some() && !off.quote {
            let mut sub = vec![];
            while i < lines.len() {
                let Some(r) = quote_line(lines[i].trim()) else { break };
                sub.push(r);
                i += 1;
            }
            // Not in a Markdown file edited as itself: saving would rewrite `[!NOTE]`.
            if let Some(b) = (!off.alt).then(|| md_callout(&sub, off)).flatten() { out.push(b); continue; }
            let kids = md_blocks(&sub, Off { quote: true, ..off }, None);
            if !kids.is_empty() {
                out.push(Block::new(BlockKind::Quote, RichText::default()).with_children(kids));
            }
            continue;
        }
        if let Some((t, n)) = pipe_table(&lines[i..]) {
            out.push(t);
            i += n;
            continue;
        }
        if off.file && let Some((alt, src, width)) = image_line(body).map(|(a, s)| (md_inline(a).text, s.to_string(), None)).or_else(|| html_image_line(body)) {
            let caption = (off.alt && !alt.is_empty()).then_some(alt);
            let mut b = Block::new(
                BlockKind::Image { src_kind: "src".into(), src, width, height: None, caption, crop: None },
                RichText::default(),
            );
            b.align = crate::model::Align::Center;
            out.push(b);
            i += 1;
            continue;
        }
        // Leading spaces stay literal.
        out.push(Block::new(BlockKind::Text, md_inline(&tabs(line.trim_end()))));
        i += 1;
    }
    out
}

/// `![alt](src "title")` filling the whole line: its alt and src.
fn image_line(s: &str) -> Option<(&str, &str)> {
    let rest = s.strip_prefix("![")?;
    let (alt, tail) = rest.split_once("](")?;
    let inner = tail.strip_suffix(')')?;
    let src = inner.split_once([' ', '\t']).map_or(inner, |(a, _)| a);
    let src = src.trim_start_matches('<').trim_end_matches('>');
    (!src.is_empty() && !inner.contains(')')).then_some((alt, src))
}

/// `<img src="…" alt="…" width="…">` filling the whole line, as GitHub
/// READMEs and some editors write a picture: its alt, src and width in px
/// (`auto`, percentages and the height are left to the picture itself).
fn html_image_line(s: &str) -> Option<(String, String, Option<f32>)> {
    if !s.get(..4)?.eq_ignore_ascii_case("<img") || !s.ends_with('>') { return None; }
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).from_utf8().read_from(&mut s.as_bytes()).ok()?;
    let body = find(&dom.document, "body")?;
    let kids = body.children.borrow();
    let [img] = &kids[..] else { return None };
    if tag(img).as_deref() != Some("img") { return None; }
    let src = attr(img, "src")?.trim().to_string();
    (!src.is_empty()).then(|| (attr(img, "alt").unwrap_or_default().trim().to_string(), src, width_px(img)))
}

/// A quote that is really a callout: GitHub's / Obsidian's `[!NOTE] Title`
/// first line (`+` / `-` folding ignored), or the `💡 text` our own Markdown
/// export writes. Unknown `[!TYPE]`s still become a callout, with 💡.
fn md_callout(lines: &[&str], off: Off) -> Option<Block> {
    let first = lines.first()?.trim_start();
    let (emoji, hue, title, rest) = if let Some(r) = first.strip_prefix("[!") {
        let (ty, after) = r.split_once(']')?;
        if ty.is_empty() || !ty.chars().all(|c| c.is_ascii_alphabetic() || c == '-') { return None; }
        let (emoji, hue) = match ty.to_ascii_lowercase().as_str() {
            "note" | "info" | "todo" => ("ℹ️", "blue"),
            "abstract" | "summary" | "tldr" | "example" => ("📝", "purple"),
            "tip" | "hint" => ("💡", "green"),
            "success" | "check" | "done" => ("✅", "green"),
            "important" => ("❗", "purple"),
            "question" | "help" | "faq" => ("❓", "yellow"),
            "warning" | "attention" => ("⚠️", "yellow"),
            "caution" | "danger" | "error" | "failure" | "fail" | "missing" | "bug" => ("🚫", "red"),
            "quote" | "cite" => ("💬", "gray"),
            _ => ("💡", "orange"),
        };
        let title = after.trim_start_matches(['+', '-']).trim();
        (emoji.to_string(), hue, (!title.is_empty()).then(|| title.to_string()), lines[1..].to_vec())
    } else {
        // Our export: `> 💡 body`, first line only. Colours aren't exported: a new callout's.
        let (e, body) = first.split_once(' ')?;
        if !crate::emoji::TSV.lines().any(|l| l.split('\t').nth(2) == Some(e)) { return None; }
        let mut rest = lines.to_vec();
        rest[0] = body;
        (e.to_string(), "orange", None, rest)
    };
    let mut kids = md_blocks(&rest, Off { quote: true, ..off }, None);
    if let Some(t) = title { kids.insert(0, Block::new(BlockKind::Text, md_inline(&format!("**{t}**")))); }
    if kids.is_empty() { kids.push(Block::text("")); }
    let kind = BlockKind::Callout { emoji, bg: Some(ColorName(format!("light-{hue}"))), border: Some(ColorName(hue.into())), text_color: None };
    Some(Block::new(kind, RichText::default()).with_children(kids))
}

fn quote_line(s: &str) -> Option<&str> {
    if s == ">" {
        return Some("");
    }
    if let Some(r) = s.strip_prefix(['>', '\u{ff1e}']) {
        return Some(r.strip_prefix(' ').unwrap_or(r));
    }
    s.strip_prefix("》 ")
}

fn table_cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix(['|', '\u{ff5c}']).unwrap_or(t);
    let t = t.strip_suffix(['|', '\u{ff5c}']).filter(|x| !x.ends_with('\\')).unwrap_or(t);
    let mut cells = vec![];
    let mut cur = String::new();
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') || chars.peek() == Some(&'\u{ff5c}') => {
                cur.push('|');
                chars.next();
            }
            '|' | '\u{ff5c}' => cells.push(std::mem::take(&mut cur).trim().to_string()),
            _ => cur.push(c),
        }
    }
    cells.push(cur.trim().to_string());
    cells
}

/// GFM pipe table at the start of `lines`: (table, lines used).
fn pipe_table(lines: &[&str]) -> Option<(Block, usize)> {
    let head = lines.first()?;
    let sep = lines.get(1)?;
    if !head.contains(['|', '\u{ff5c}']) {
        return None;
    }
    let seps = table_cells(sep);
    let ok = |c: &String| {
        let c = c.trim_matches([':', '\u{ff1a}']);
        !c.is_empty() && c.chars().all(|x| x == '-' || x == '\u{ff0d}')
    };
    if !seps.iter().all(ok) || !sep.contains(['-', '\u{ff0d}']) {
        return None;
    }
    let header = table_cells(head);
    let ncols = header.len();
    if ncols != seps.len() {
        return None;
    }
    let aligns: Vec<Align> = seps
        .iter()
        .map(|c| match (c.starts_with([':', '\u{ff1a}']), c.ends_with([':', '\u{ff1a}'])) {
            (true, true) => Align::Center,
            (false, true) => Align::Right,
            _ => Align::Left,
        })
        .collect();
    let mut n = 2;
    let mut rows = vec![header];
    while let Some(l) = lines.get(n) {
        if l.trim().is_empty() || !l.contains(['|', '\u{ff5c}']) {
            break;
        }
        rows.push(table_cells(l));
        n += 1;
    }
    let rows: Vec<Vec<TableCell>> = rows
        .into_iter()
        .map(|r| {
            (0..ncols)
                .map(|c| {
                    let mut b = Block::new(BlockKind::Text, md_inline(r.get(c).map_or("", String::as_str)));
                    b.align = aligns[c];
                    TableCell { blocks: vec![b], ..TableCell::empty() }
                })
                .collect()
        })
        .collect();
    let col_widths = pipe_table_widths(&rows, ncols);
    let mut t = Table { col_widths, header_rows: 1, header_col: false, rows };
    t.repair();
    Some((Block::new(BlockKind::Table(t), RichText::default()), n))
}

/// Initial Markdown widths; saved DocxXML widths and manual resizing stay intact.
pub(crate) fn pipe_table_widths(rows: &[Vec<TableCell>], ncols: usize) -> Vec<f32> {
    use unicode_width::UnicodeWidthStr;
    let mut widths = vec![Table::MIN_WIDTH; ncols];
    for row in rows {
        for (c, cell) in row.iter().enumerate() {
            for b in &cell.blocks {
                let text = b.text.plain_text();
                let units = text.lines().map(UnicodeWidthStr::width).max().unwrap_or(0);
                // Approximate body text plus cell padding; prose wraps at 480px.
                widths[c] = widths[c].max((units as f32 * 8. + 24.).clamp(Table::MIN_WIDTH, 480.));
            }
        }
    }
    // Fit an ordinary text column when possible. Wide tables still scroll,
    // with at least the default width per column available on average.
    let total: f32 = widths.iter().sum();
    let budget = 720f32.max(ncols as f32 * Table::DEFAULT_WIDTH);
    if total > budget {
        let min = ncols as f32 * Table::MIN_WIDTH;
        let scale = (budget - min) / (total - min);
        widths.iter_mut().for_each(|w| *w = (Table::MIN_WIDTH + (*w - Table::MIN_WIDTH) * scale).floor());
    }
    widths
}

fn is_boundary(c: Option<char>) -> bool {
    c.is_none_or(|c| c.is_whitespace() || "，。；：！？、）】》”’".contains(c))
}

fn math_boundary(c: Option<char>) -> bool {
    c.is_none_or(|c| c.is_whitespace() || c.is_ascii_punctuation() || "，。；：！？、（）【】《》“”‘’".contains(c))
}

pub fn markdown_equation_source(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        if bytes[i] == b'\\' && bytes.get(i + 1) == Some(&b'\\') && bytes.get(i + 2).is_some_and(u8::is_ascii_alphabetic) {
            out.push('\\');
            i += 2;
        } else {
            let c = src[i..].chars().next().unwrap();
            out.push(c);
            i += c.len_utf8();
        }
    }
    out
}

pub fn is_numeric_math(src: &str) -> bool {
    src.contains('\\') || src.contains('=') || src.contains('+') || src.contains('/') || src.contains("×") || src.contains("÷")
}

/// Inline Markdown: code spans, equations, links, bare URLs and the emphasis
/// rules of the live shortcuts (`~x~` is underline), and paired `<u>…</u>`
/// (how Markdown export writes underline; no other HTML). Paste turns escapes,
/// images and `<…>` autolinks off.
/// A numeric character reference at the start of `s` (`&#32;`, `&#xA0;`):
/// the character and the reference's length.
pub(crate) fn char_ref(s: &str) -> Option<(char, usize)> {
    let body = s.strip_prefix("&#")?;
    let (hex, digits) = match body.strip_prefix(['x', 'X']) { Some(h) => (true, h), None => (false, body) };
    let n = digits.bytes().take_while(|b| if hex { b.is_ascii_hexdigit() } else { b.is_ascii_digit() }).count();
    if n == 0 || n > if hex { 6 } else { 7 } || digits.as_bytes().get(n) != Some(&b';') { return None; }
    let ch = char::from_u32(u32::from_str_radix(&digits[..n], if hex { 16 } else { 10 }).ok()?).filter(|&c| c != '\0')?;
    Some((ch, s.len() - digits.len() + n + 1))
}

pub fn md_inline(s: &str) -> RichText {
    let mut out = RichText::default();
    let mut plain = String::new();
    let flush = |plain: &mut String, out: &mut RichText| {
        if !plain.is_empty() {
            out.push(plain, Marks::default());
            plain.clear();
        }
    };
    let mut i = 0;
    while i < s.len() {
        let rest = &s[i..];
        let c = rest.chars().next().unwrap();
        let prev = s[..i].chars().next_back();
        // `\*`, `\_`, `\<` …: the character itself (CommonMark), which is how
        // Markdown export writes them.
        if c == '\\' && let Some(next) = rest[1..].chars().next().filter(char::is_ascii_punctuation) {
            plain.push(next);
            i += 1 + next.len_utf8();
            continue;
        }
        // `<br>`, a line break inside the block, and `&#32;`, a character
        // (a space a line would lose), as Markdown files are written.
        if c == '<' && let Some(t) = ["<br>", "<br/>", "<br />"].into_iter().find(|t| rest.get(..t.len()).is_some_and(|r| r.eq_ignore_ascii_case(t))) {
            plain.push('\n');
            i += t.len();
            continue;
        }
        if c == '&' && let Some((ch, n)) = char_ref(rest) {
            plain.push(ch);
            i += n;
            continue;
        }
        // Code span.
        if c == '`' {
            let n = rest.bytes().take_while(|&b| b == b'`').count();
            let delim = &rest[..n];
            if let Some(end) = find_run(&rest[n..], delim, b'`') {
                let inner = &rest[n..n + end];
                let inner = if inner.len() > 2 && inner.starts_with(' ') && inner.ends_with(' ') { &inner[1..inner.len() - 1] } else { inner };
                if !inner.is_empty() {
                    flush(&mut plain, &mut out);
                    out.push(inner, Marks { code: true, ..Marks::default() });
                    i += n + end + n;
                    continue;
                }
            }
            plain.push_str(delim);
            i += n;
            continue;
        }
        // Equations: `$$x$$` and `$x$` with whitespace/line boundaries.
        if c == '$' && math_boundary(prev) {
            let mut done = false;
            for delim in ["$$", "$"] {
                if !rest.starts_with(delim) || rest[delim.len()..].starts_with('$') {
                    continue;
                }
                let body = &rest[delim.len()..];
                let Some(end) = body.find(delim) else { continue };
                let inner = &body[..end];
                let after = body[end + delim.len()..].chars().next();
                // A lone number after `$` is money; operators make it math.
                let digit = delim == "$" && inner.starts_with(|c: char| c.is_ascii_digit()) && !is_numeric_math(inner);
                if digit || inner.trim().is_empty() || inner.starts_with(' ') || !math_boundary(after) || after == Some('$') {
                    continue;
                }
                flush(&mut plain, &mut out);
                out.push(crate::editor::core::EQUATION, Marks { inline: Some(crate::docxml::to_inline(InlineKind::Equation(markdown_equation_source(inner)))), ..Marks::default() });
                i += delim.len() * 2 + end;
                done = true;
                break;
            }
            if done {
                continue;
            }
        }
        if let Some(body) = rest.strip_prefix("![[") && let Some((inner, _)) = body.split_once("]]")
            && !inner.contains(['[', ']', '\n']) {
            let end = inner.len() + 5;
            plain.push_str(&rest[..end]);
            i += end;
            continue;
        }
        if c == '[' && prev != Some('!') && let Some((inside, _)) = rest.strip_prefix("[[").and_then(|s| s.split_once("]]"))
            && !inside.contains(['[', ']', '\n']) {
            let (target, label) = inside.split_once('|').unwrap_or((inside, inside));
            if !target.trim().is_empty() && !label.is_empty() {
                flush(&mut plain, &mut out);
                out.push(label, Marks { link: Some(format!("quiet-wiki:{inside}")), ..Default::default() });
                i += inside.len() + 4;
                continue;
            }
        }
        // Links `[text](url)`.
        if c == '!' && rest.starts_with("![") && let Some((_, _, used)) = link_at(&rest[1..]) {
            plain.push_str(&rest[..used + 1]);
            i += used + 1;
            continue;
        }
        if c == '[' {
            let open = 0;
            if let Some((label, url, used)) = link_at(&rest[open..]) {
                flush(&mut plain, &mut out);
                let mut t = md_inline(label);
                if t.is_empty() {
                    t = RichText::plain(url);
                }
                let len = t.len();
                t.update_marks(0..len, |m| m.link = Some(url.to_string()));
                out.append(&t);
                i += open + used;
                continue;
            }
        }
        if c == '\u{3010}' && let Some((label, url, used)) = cjk_link_at(rest) {
            flush(&mut plain, &mut out);
            let mut t = md_inline(label);
            if t.is_empty() {
                t = RichText::plain(url);
            }
            let len = t.len();
            t.update_marks(0..len, |m| m.link = Some(url.to_string()));
            out.append(&t);
            i += used;
            continue;
        }
        // Bare URLs (they get links when drawn).
        if (c == 'h' || c == 'w') && !prev.is_some_and(|p| p.is_alphanumeric()) {
            let l = rest.get(..8).unwrap_or(rest).to_ascii_lowercase();
            if l.starts_with("https://") || l.starts_with("http://") || l.starts_with("www.") {
                let end = rest.find(|c: char| c.is_whitespace() || "<>\"'`".contains(c) || !c.is_ascii()).unwrap_or(rest.len());
                let url = rest[..end].trim_end_matches(|c: char| ".,;:!?)]}*_~".contains(c));
                let host = url.splitn(2, "://").last().unwrap_or("");
                if host.len() > 3 && host.contains('.') {
                    flush(&mut plain, &mut out);
                    let href = if l.starts_with("www.") { format!("https://{url}") } else { url.to_string() };
                    out.push(url, Marks { link: Some(href), ..Marks::default() });
                    i += url.len();
                    continue;
                }
            }
        }
        if let Some(body) = rest.strip_prefix("<u>") && let Some(end) = find_u_close(body).filter(|&e| e > 0) {
            flush(&mut plain, &mut out);
            let mut t = md_inline(&body[..end]);
            let len = t.len();
            t.update_marks(0..len, |m| m.underline = true);
            out.append(&t);
            i += 3 + end + 4;
            continue;
        }
        // `＊强调＊` / `＊＊加粗＊＊`: only a closed pair is emphasis; a lone `＊` stays itself.
        if c == '＊' {
            let n = rest.chars().take_while(|&x| x == '＊').count();
            let run = &rest[..n * 3];
            if n <= 3 && let Some(end) = find_wide_run(&rest[run.len()..], run) {
                let inner = &rest[run.len()..run.len() + end];
                if !inner.starts_with(char::is_whitespace) && !inner.ends_with(char::is_whitespace) {
                    flush(&mut plain, &mut out);
                    let mut t = md_inline(inner);
                    let len = t.len();
                    t.update_marks(0..len, |m| { m.bold |= n != 1; m.italic |= n != 2; });
                    out.append(&t);
                    i += run.len() * 2 + end;
                    continue;
                }
            }
            plain.push_str(run);
            i += run.len();
            continue;
        }
        // Emphasis.
        if matches!(c, '*' | '_' | '~') {
            let n = rest.bytes().take_while(|&b| b == c as u8).count();
            let run = &rest[..n];
            let rule: Option<fn(&mut Marks)> = match (c, n) {
                ('*' | '_', 3) => Some(|m| {
                    m.bold = true;
                    m.italic = true
                }),
                ('*' | '_', 2) => Some(|m| m.bold = true),
                ('*' | '_', 1) => Some(|m| m.italic = true),
                ('~', 2) => Some(|m| m.strike = true),
                ('~', 1) => Some(|m| m.underline = true),
                _ => None,
            };
            let bounded = c == '_' || (c == '~' && n == 1);
            if let Some(set) = rule
                && (!bounded || is_boundary(prev))
                && let Some(end) = find_run(&rest[n..], run, c as u8)
            {
                let inner = &rest[n..n + end];
                let after = rest[n + end + n..].chars().next();
                if !inner.starts_with(char::is_whitespace) && !inner.ends_with(char::is_whitespace) && (!bounded || is_boundary(after)) {
                    flush(&mut plain, &mut out);
                    let mut t = md_inline(inner);
                    let len = t.len();
                    t.update_marks(0..len, set);
                    out.append(&t);
                    i += n + end + n;
                    continue;
                }
            }
            plain.push_str(run);
            i += n;
            continue;
        }
        plain.push(c);
        i += c.len_utf8();
    }
    flush(&mut plain, &mut out);
    out
}

/// Offset of the `</u>` closing an opened `<u>`: nested pairs count, escapes
/// and code spans are skipped (a tag inside code is text).
fn find_u_close(s: &str) -> Option<usize> {
    let mut depth = 0;
    let mut i = 0;
    while i < s.len() {
        let rest = &s[i..];
        if rest.starts_with('\\') && rest[1..].starts_with(|c: char| c.is_ascii_punctuation()) { i += 2; continue; }
        if rest.starts_with('`') {
            let n = rest.bytes().take_while(|&b| b == b'`').count();
            i += find_run(&rest[n..], &rest[..n], b'`').map_or(n, |end| n + end + n);
            continue;
        }
        if rest.starts_with("<u>") { depth += 1; i += 3; continue; }
        if rest.starts_with("</u>") {
            if depth == 0 { return Some(i); }
            depth -= 1;
            i += 4;
            continue;
        }
        i += rest.chars().next().unwrap().len_utf8();
    }
    None
}

/// `find_run` for a run of `＊` (not part of a longer one), non-empty match only.
fn find_wide_run(s: &str, delim: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(k) = s[from..].find(delim) {
        let at = from + k;
        if at > 0 && !s[..at].ends_with('＊') && !s[at + delim.len()..].starts_with('＊') { return Some(at); }
        from = at + delim.len();
    }
    None
}

/// Offset in `s` of a delimiter run exactly equal to `delim` (not part of a
/// longer run of `ch`), skipping a non-empty match only.
fn find_run(s: &str, delim: &str, ch: u8) -> Option<usize> {
    let b = s.as_bytes();
    let mut from = 0;
    while let Some(k) = s[from..].find(delim) {
        let at = from + k;
        let before_ok = at == 0 || b[at - 1] != ch;
        let after_ok = b.get(at + delim.len()) != Some(&ch);
        if at > 0 && before_ok && after_ok {
            return Some(at);
        }
        from = at + delim.len().max(1);
        // Skip the rest of a longer run.
        while b.get(from) == Some(&ch) {
            from += 1;
        }
    }
    None
}

/// `[label](url "title")` at the start of `s`: (label, url, bytes used).
fn link_at(s: &str) -> Option<(&str, &str, usize)> {
    let b = s.as_bytes();
    if b.first() != Some(&b'[') {
        return None;
    }
    let mut depth = 0;
    let mut close = None;
    for (k, &c) in b.iter().enumerate() {
        match c {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(k);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let rest = &s[close + 1..];
    let inner = rest.strip_prefix('(')?;
    let end = inner.find(')')?;
    let target = inner[..end].trim();
    let url = if let Some(inner) = target.strip_prefix('<').and_then(|s| s.split_once('>')) { inner.0 } else { target.split_whitespace().next().unwrap_or("").trim_matches(['<', '>']) };
    if url.is_empty() || !(link_ok(url) || relative_link(url) || url.starts_with("www.")) {
        return None;
    }
    Some((&s[1..close], url, close + 1 + 1 + end + 1))
}

/// `【label】(url)` or `【label】（url）` at the start of `s`.
fn cjk_link_at(s: &str) -> Option<(&str, &str, usize)> {
    let (label, tail) = s.strip_prefix('\u{3010}')?.split_once('\u{3011}')?;
    let (inner, close_ch) = tail
        .strip_prefix('(')
        .map(|inner| (inner, ')'))
        .or_else(|| tail.strip_prefix('\u{ff08}').map(|inner| (inner, '\u{ff09}')))?;
    let end = inner.find(close_ch)?;
    let url = inner[..end].trim();
    let used = s.len() - inner.len() + end + close_ch.len_utf8();
    (!url.is_empty() && (link_ok(url) || url.starts_with("www."))).then_some((label, url, used))
}

#[cfg(test)]
mod tests {
    use super::html_to_blocks;
    use crate::docxml;
    use crate::model::{Document, RichText};

    fn xml(html: &str) -> String {
        docxml::serialize(&Document { blocks: html_to_blocks(html), ..Default::default() })
    }

    #[test]
    fn blocks_and_marks() {
        let x = xml(
            "<meta charset='utf-8'><h2>Title</h2><p>a <b>bold</b> and <a href=\"https://x.y/\">link</a></p>\
             <ul><li>one<ul><li>sub</li></ul></li><li><input type=checkbox checked> done</li></ul>\
             <ol start=3><li>three</li></ol><blockquote><p>q</p></blockquote><hr><pre><code class=\"language-rust\">fn x() {}\n</code></pre>",
        );
        assert!(x.contains("<h2>Title</h2>"), "{x}");
        assert!(x.contains("<p>a <b>bold</b> and <a href=\"https://x.y/\">link</a></p>"), "{x}");
        assert!(x.contains("<li>one"), "{x}");
        assert!(x.contains("sub"), "{x}");
        assert!(x.contains("<checkbox done=\"true\">done</checkbox>"), "{x}");
        assert!(x.contains("<li seq=\"3\">three</li>"), "{x}");
        assert!(x.contains("<blockquote>"), "{x}");
        assert!(x.contains("<hr/>"), "{x}");
        assert!(x.contains("lang=\"rust\""), "{x}");
        assert!(x.contains("<code>fn x() {}</code></pre>"), "{x}");
    }

    #[test]
    fn feishu_todo_and_callout() {
        let x = xml("<ul class=\"list-done1\"><li data-list=\"done\"><div>t</div></li></ul>\
             <div class=\"zoneType-calloutBlock\"><div class=\"callout-container\" data-emoji-id=\"chestnut\">\
             <div class=\"callout-block\" style=\"background-color:rgb(255,245,235);border-color:rgb(254,212,164);\"><div>c</div></div></div></div>");
        assert!(x.contains("<checkbox done=\"true\">t</checkbox>"), "{x}");
        assert!(x.contains("<callout emoji=\"🌰\" background-color=\"light-orange\" border-color=\"orange\">"), "{x}");
    }

    #[test]
    fn google_docs_wrapper_is_not_bold() {
        let x = xml("<b style=\"font-weight:normal;\" id=\"docs-internal-guid-1\"><p dir=\"ltr\"><span style=\"font-weight:700\">B</span><span> plain</span></p></b>");
        assert!(x.contains("<p><b>B</b> plain</p>"), "{x}");
    }

    #[test]
    fn whitespace_and_breaks() {
        let x = xml("<div>\n  line one <br>\n  line   two\n</div><p></p><div>x</div>");
        assert!(x.contains("<p>line one</p>\n<p>line two</p>\n<p></p>\n<p>x</p>"), "{x}");
    }

    #[test]
    fn markdown_callouts() {
        let md = |s: &str| docxml::serialize(&Document { blocks: super::markdown_to_blocks(s, Default::default()), ..Default::default() });
        let x = md("> [!WARNING]\n> Pass a pointer.\n");
        assert!(x.contains("<callout emoji=\"⚠️\" background-color=\"light-yellow\" border-color=\"yellow\">\n  <p>Pass a pointer.</p>\n</callout>"), "{x}");
        let x = md("> [!tip]- Folded title\n> body\n");
        assert!(x.contains("emoji=\"💡\"") && x.contains("<p><b>Folded title</b></p>\n  <p>body</p>"), "{x}");
        // What our own export writes comes back a callout.
        let x = md("> 🌰 nut\n");
        assert!(x.contains("<callout emoji=\"🌰\"") && x.contains("<p>nut</p>"), "{x}");
        // Plain quotes, and `[!x]` that is not a marker, stay quotes.
        for q in ["> just a quote\n", "> [!] x\n", "> [!a b] x\n", "> :) hi\n"] { assert!(md(q).contains("<blockquote>"), "{q}: {}", md(q)); }
        // A Markdown file edited as itself keeps its text.
        let off = super::Off { file: true, alt: true, ..Default::default() };
        let x = docxml::serialize(&Document { blocks: super::markdown_to_blocks("> [!NOTE]\n> x\n", off), ..Default::default() });
        assert!(x.contains("<blockquote>"), "{x}");
    }

    #[test]
    fn markdown() {
        let x = docxml::serialize(&Document {
            title: RichText::default(),
            blocks: super::markdown_to_blocks(
                "# H1\n\nSome **bold** and *it* and ~u~ and `c` [l](https://a.b/c)\n- a\n  - b\n- [x] t\n3. three\n4. four\n> q\n\n---\n```rust\nfn main() {}\n```\n| A | B |\n|---|:-:|\n| 1 | 2 |\n",
                Default::default(),
            ),
            ..Default::default()
        });
        assert!(x.contains("<h1>H1</h1>"), "{x}");
        assert!(x.contains("<b>bold</b>"), "{x}");
        assert!(x.contains("<em>it</em>"), "{x}");
        assert!(x.contains("<u>u</u>"), "{x}");
        assert!(x.contains("<code>c</code>"), "{x}");
        assert!(x.contains("<a href=\"https://a.b/c\">l</a>"), "{x}");
        assert!(x.contains("<li>a"), "{x}");
        assert!(x.contains("<checkbox done=\"true\">t</checkbox>"), "{x}");
        assert!(x.contains("<li seq=\"3\">three</li>"), "{x}");
        assert!(x.contains("<li>four</li>"), "{x}");
        assert!(x.contains("<blockquote>"), "{x}");
        assert!(x.contains("<hr/>"), "{x}");
        assert!(x.contains("fn main() {}"), "{x}");
        assert!(x.contains("<table"), "{x}");
    }

    #[test]
    fn markdown_underline_tags() {
        let under = |s: &str| { let t = super::md_inline(s); t.spans().filter(|(_, m)| m.underline).map(|(r, _)| t.text[r].to_string()).collect::<Vec<_>>().join("|") };
        assert_eq!(under("a <u>b</u> c"), "b");
        assert_eq!(under("<u>a <u>b</u> c</u>"), "a b c");
        assert_eq!(under("<u>a `</u>` b</u>"), "a |</u>| b");
        // Unpaired, empty, escaped or other tags stay text.
        for s in ["a <u>b", "b</u> c", "<u></u>", "\\<u>b\\</u>", "<U>b</U>", "<b>x</b>", "`<u>b</u>`"] {
            assert_eq!(under(s), "", "{s}");
        }
        assert_eq!(super::md_inline("a <u>b").text, "a <u>b");
        assert_eq!(super::md_inline("\\<u>b\\</u>").text, "<u>b</u>");
        // Opening a Markdown file and pasting read it the same.
        for off in [super::Off::default(), super::Off { file: true, ..Default::default() }] {
            let b = super::markdown_to_blocks("# <u>t</u>\n\n- <u>x</u>\n", off);
            assert!(b.iter().all(|b| b.text.spans().all(|(_, m)| m.underline)), "{b:?}");
        }
    }

    #[test]
    fn markdown_display_math() {
        for (src, equation) in [
            ("$$V = I R$$", "V = I R"),
            ("$$\nV &= I R \\\\\nQ_c &= 2I\n$$", "V &= I R \\\\\nQ_c &= 2I"),
        ] {
            let blocks = super::markdown_to_blocks(src, Default::default());
            assert_eq!(blocks.len(), 1, "{src}: {blocks:?}");
            assert_eq!(blocks[0].text.text, crate::editor::core::EQUATION);
            let inline = blocks[0].text.spans().next().unwrap().1.inline.clone().unwrap();
            assert_eq!(inline.equation().unwrap(), equation);
        }
    }

    #[test]
    fn detects_math() {
        assert!(super::has_math("$$\nx\n$$"));
        assert!(super::has_math("$$x$$"));
        assert!(super::has_math("存储。$1{,}000 / 30 = 33{,}000{,}000$。"));
        assert!(!super::has_math("$$\nx"));
        assert!(!super::has_math("costs $5"));
        assert!(!super::has_math("```\n$1 / 2 = 3$\n```"));
    }

    #[test]
    fn math_detection_ignores_unclosed_fences() {
        assert!(super::has_math("```rust\n$1 / 2 = 3$\n```\n$1 / 2 = 3$"));
        assert!(!super::has_math("```math\n$1 / 2 = 3$\n```math"));
    }

    #[test]
    fn markdown_math_from_chat_latex() {
        let t = super::md_inline("存储。$10 亿页 \\\\times 500k = 500 TB$。");
        let equations: Vec<_> = t.spans().filter_map(|(_, m)| m.inline.as_ref().and_then(|i| i.equation().map(str::to_string))).collect();
        assert_eq!(equations, vec!["10 亿页 \\times 500k = 500 TB"]);
    }

    #[test]
    fn emphasis_after_chinese_punctuation() {
        let t = super::md_inline("中文，_强调_，后续");
        assert_eq!(t.text, "中文，强调，后续");
        assert!(t.spans().any(|(_, m)| m.italic), "{t:?}");
    }

    #[test]
    fn math_delimiters_and_latex_line_breaks() {
        let t = super::md_inline("Total: $x / 2$; tax($7$) not math, matrix $a \\\\ b$ is.");
        let equations: Vec<_> = t.spans().filter_map(|(_, m)| m.inline.as_ref().and_then(|i| i.equation().map(str::to_string))).collect();
        assert_eq!(equations, vec!["x / 2", "a \\\\ b"]);
    }

    #[test]
    fn markdown_html_images() {
        use crate::model::BlockKind;
        let off = super::Off { file: true, alt: true, ..Default::default() };
        let img = |line: &str| match super::markdown_to_blocks(line, off).as_slice() {
            [b] => match &b.kind { BlockKind::Image { src, width, caption, .. } => Some((src.clone(), *width, caption.clone())), _ => None },
            _ => None,
        };
        assert_eq!(img(r#"<img src="https://file.oss/image.png" alt="" width="auto" height="auto" >"#), Some(("https://file.oss/image.png".into(), None, None)));
        assert_eq!(img(r#"<IMG SRC='img/a b.png' alt="Cat" width="300" />"#), Some(("img/a b.png".into(), Some(300.), Some("Cat".into()))));
        assert_eq!(img(r#"<img src="a.png" style="width: 120px" height="50%">"#), Some(("a.png".into(), Some(120.), None)));
        assert_eq!(img(r#"<img src="https://x.y/a.png?a=1&amp;b=2">"#).map(|i| i.0), Some("https://x.y/a.png?a=1&b=2".into()));
        // Anything else on the line, or no src: not a picture.
        for line in [r#"<img src="a.png"> and text"#, r#"<img alt="x">"#, r#"<img src="">"#, "<imgx src=\"a.png\">", r#"<img src="a.png"><img src="b.png">"#] {
            assert_eq!(img(line), None, "{line}");
        }
        // Pasting keeps it as text, as with `![](…)`.
        assert!(matches!(super::markdown_to_blocks(r#"<img src="a.png">"#, super::Off::default())[0].kind, BlockKind::Text));
    }

    #[test]
    fn markdown_literals() {
        let t = super::md_inline("a_b_c snake_case_name ** x** costs $5 and $6");
        assert!(t.runs.iter().all(|r| !r.marks.bold && !r.marks.italic), "{:?}", t);
    }

    fn markdown_table(text: &str) -> crate::model::Table {
        let mut blocks = super::markdown_to_blocks(text, super::Off::default());
        let crate::model::BlockKind::Table(t) = blocks.remove(0).kind else { panic!("expected table") };
        t
    }

    #[test]
    fn markdown_table_widths_follow_content() {
        let t = markdown_table("| ID | Status | Description |\n| --: | :--: | --- |\n| 1 | OK | A longer description should get more room than a short identifier or status. |\n| 2 | Done | Short |\n");
        assert!(t.col_widths[0] < t.col_widths[1], "{:?}", t.col_widths);
        assert!(t.col_widths[1] < 200. && t.col_widths[2] > 200., "{:?}", t.col_widths);
        assert!(t.col_widths.iter().sum::<f32>() <= 720.);
        assert_eq!(t.header_rows, 1);
        assert_eq!(t.rows[1][0].blocks[0].align, crate::model::Align::Right);
        assert_eq!(t.rows[1][1].blocks[0].align, crate::model::Align::Center);
        assert_eq!(t.rows[1][2].blocks[0].text.text, "A longer description should get more room than a short identifier or status.");
        let doc = Document { blocks: vec![crate::model::Block::new(crate::model::BlockKind::Table(t), RichText::default())], ..Default::default() };
        let saved = docxml::serialize(&doc);
        assert_eq!(docxml::serialize(&docxml::parse(&saved).unwrap()), saved);
    }

    #[test]
    fn markdown_table_widths_use_visible_unicode_text() {
        let t = markdown_table("| aaaaaa | 中文内容测试 |\n| --- | --- |\n| aaaaaa | 中文内容测试 |\n");
        assert!(t.col_widths[1] > t.col_widths[0], "{:?}", t.col_widths);
        let plain = markdown_table("| Label | Label |\n| --- | --- |\n| Resume | Resume |\n");
        let styled = markdown_table(&format!("| Label | Label |\n| --- | --- |\n| **Resume** | [Resume](https://example.com/{}) |\n", "long-path/".repeat(100)));
        assert_eq!(styled.col_widths, plain.col_widths);
        let accents = markdown_table("| éééééé | e\u{301}e\u{301}e\u{301}e\u{301}e\u{301}e\u{301} |\n| --- | --- |\n");
        assert_eq!(accents.col_widths[0], accents.col_widths[1]);
        let emoji = markdown_table("| 😀😀😀😀 | 👨‍👩‍👧‍👦👨‍👩‍👧‍👦👨‍👩‍👧‍👦👨‍👩‍👧‍👦 |\n| --- | --- |\n");
        assert_eq!(emoji.col_widths[0], emoji.col_widths[1]);
    }

    #[test]
    fn markdown_table_widths_are_bounded() {
        use crate::model::Table;
        let empty = markdown_table("| | |\n| --- | --- |\n| |\n");
        assert_eq!(empty.col_widths, vec![Table::MIN_WIDTH; 2]);
        assert_eq!(empty.rows[1].len(), 2);
        let long = "description ".repeat(1000);
        let t = markdown_table(&format!("| x |\n| --- |\n| {long} |\n"));
        assert!(t.col_widths[0] > 200. && t.col_widths[0] <= 480.);
        for n in [3, 20] {
            let head = vec!["x"; n].join("|");
            let sep = vec!["---"; n].join("|");
            let body = vec![long.as_str(); n].join("|");
            let t = markdown_table(&format!("|{head}|\n|{sep}|\n|{body}|\n"));
            assert!(t.col_widths.iter().all(|w| w.is_finite() && *w >= Table::MIN_WIDTH && *w <= 480.));
            assert!(t.col_widths.iter().sum::<f32>() <= 720f32.max(n as f32 * Table::DEFAULT_WIDTH));
        }
    }

    #[test]
    fn markdown_chinese_punctuation() {
        let src = "＃ 一级标题\n－ 无序列表\n1．有序列表\n＞ 引用\n｜列一｜列二｜\n｜：－：｜：－：｜\n\
                   中文，＊强调＊，后续\n＊＊加粗＊＊\n【链接】（https://example.com）\n＊ 项目";
        let blocks = super::markdown_to_blocks(src, Default::default());
        let kind = |i: usize| &blocks[i].kind;
        assert!(matches!(kind(0), crate::model::BlockKind::Heading { level: 1, .. }), "{blocks:?}");
        assert!(matches!(kind(1), crate::model::BlockKind::Bullet), "{blocks:?}");
        assert!(matches!(kind(2), crate::model::BlockKind::Ordered { .. }), "{blocks:?}");
        assert!(matches!(kind(3), crate::model::BlockKind::Quote), "{blocks:?}");
        assert!(matches!(kind(4), crate::model::BlockKind::Table(_)), "{blocks:?}");
        assert!(matches!(kind(5), crate::model::BlockKind::Text), "{blocks:?}");
        let text = |i: usize| blocks[i].text.text.as_str();
        assert_eq!(text(5), "中文，强调，后续");
        assert_eq!(text(6), "加粗");
        assert!(blocks[7].text.runs.iter().any(|r| r.marks.link.is_some()), "{:?}", blocks[7].text);
        assert!(matches!(kind(8), crate::model::BlockKind::Bullet), "{blocks:?}");
    }

    #[test]
    fn markdown_chinese_punctuation_literals() {
        let blocks = super::markdown_to_blocks("——破折号\n——\n《书名》【正文】（注释）", Default::default());
        assert_eq!(blocks[0].text.text, "——破折号");
        assert!(matches!(blocks[1].kind, crate::model::BlockKind::Text), "{blocks:?}");
        assert_eq!(blocks[2].text.text, "《书名》【正文】（注释）");

        let code = super::markdown_to_blocks("```\n＃ ＊x＊ ｜【a】（b）｜\n```\n`＃ ＊x＊ 【a】（b）`", Default::default());
        assert!(matches!(code[0].kind, crate::model::BlockKind::Code { .. }), "{code:?}");
        assert_eq!(code[0].text.text, "＃ ＊x＊ ｜【a】（b）｜");
        assert_eq!(code[1].text.text, "＃ ＊x＊ 【a】（b）");
        assert!(code[1].text.runs[0].marks.code, "{:?}", code[1].text.runs[0]);

        // Full-width characters that aren't syntax are kept as typed.
        let prose = super::markdown_to_blocks("价格＊2\n选项一｜选项二\n2024。今天\n3．14 是圆周率\n—— 鲁迅\n＃标题没有空格", Default::default());
        let texts: Vec<&str> = prose.iter().map(|b| b.text.text.as_str()).collect();
        assert_eq!(texts, ["价格＊2", "选项一｜选项二", "2024。今天", "3．14 是圆周率", "—— 鲁迅", "＃标题没有空格"], "{prose:?}");
        assert!(prose.iter().all(|b| matches!(b.kind, crate::model::BlockKind::Text)), "{prose:?}");

        let escaped = super::md_inline("\\＊不是");
        assert_eq!(escaped.text, "\\＊不是", "{escaped:?}");
    }

    #[test]
    fn tables() {
        let x = xml("<table><tr><th>a</th><th>b</th></tr><tr><td colspan=2>c</td></tr></table>");
        assert!(x.contains("<table"), "{x}");
        assert!(x.contains("colspan=\"2\""), "{x}");
    }
}
