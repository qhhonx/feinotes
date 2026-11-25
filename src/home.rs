//! 主页 and a folder's page: the one-row header (`topbar`, home_topbar.rs), then one level of the
//! tree as cards or a list, in the order 显示 picks. Dressed as 所有文档: a hairline list with quiet
//! hover fills, white cards and pills, native menus.

use crate::app::{RowMenu, Workspace};
#[path = "home_preview_style.rs"]
mod bs;
#[path = "home_text.rs"]
mod text_preview;
#[path = "home_topbar.rs"]
pub(crate) mod topbar;
use crate::settings::display_title;
use crate::model::BlockKind;
use crate::search::doc_icon;
use crate::store::Entry;
use crate::editor::image::picture;
use crate::theme::{self, Theme};
use crate::trash::{empty_state, list_header_text, row_hover, row_line};
use crate::design::Popover;
use crate::ui::{ROW_RADIUS, icon, ui_text};
use gpui::{prelude::FluentBuilder, *};
use std::cell::RefCell;
use std::collections::{HashMap};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HomeTab {
    #[default]
    Recent,
    Starred,
}

/// The optional list columns (标题 is always shown).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Col {
    Modified,
    Created,
    Visited,
    Starred,
}

impl Col {
    fn label(self) -> &'static str {
        match self {
            Col::Modified => "修改时间",
            Col::Created => "创建时间",
            Col::Visited => "最近访问",
            Col::Starred => "收藏时间",
        }
    }
    /// The order this column sorts by (收藏时间 is not an order).
    fn sort(self) -> Option<HomeSort> {
        match self {
            Col::Modified => Some(HomeSort::Modified),
            Col::Created => Some(HomeSort::Created),
            Col::Visited => Some(HomeSort::Visited),
            Col::Starred => None,
        }
    }
}

/// 最近访问's columns (no 所有者); 显示设置 toggles them.
const RECENT_COLS: [Col; 3] = [Col::Modified, Col::Created, Col::Visited];
const STARRED_COLS: [Col; 4] = [Col::Modified, Col::Created, Col::Visited, Col::Starred];

/// Bounded document trees retain formatting without retaining a whole long document.
type EquationNumbers = HashMap<crate::model::BlockId, u32>;
/// `tables`: the 数据表 files the page shows (their rows drawn in the card), saved apart from the page.
struct ThumbContent { blocks: Vec<crate::model::Block>, eqnos: EquationNumbers, has_subpages: bool, tables: Vec<String> }
type Thumb = Rc<ThumbContent>;

/// The files of the 数据表 blocks in `blocks`, at any depth.
fn table_srcs(blocks: &[crate::model::Block], out: &mut Vec<String>) {
    for b in blocks {
        match &b.kind {
            BlockKind::DataTable(t) => out.push(t.src.clone()),
            BlockKind::Table(t) => for cell in t.rows.iter().flatten() { table_srcs(&cell.blocks, out) },
            _ => {}
        }
        table_srcs(&b.children, out);
    }
}

/// When the files in `srcs` (relative to the library) last changed, as one number.
fn tables_time(srcs: &[String]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = rustc_hash::FxHasher::default();
    let root = crate::store::Store::active_dir();
    for s in srcs { std::fs::metadata(root.join(s)).and_then(|m| m.modified()).ok().hash(&mut h); }
    h.finish()
}

fn has_subpage_list(blocks: &[crate::model::Block]) -> bool {
    blocks.iter().any(|b| match &b.kind {
        BlockKind::SubPageList => true,
        BlockKind::Table(t) => t.rows.iter().any(|row| row.iter().any(|cell| has_subpage_list(&cell.blocks))),
        _ => has_subpage_list(&b.children),
    })
}

fn thumb_blocks(blocks: &[crate::model::Block], budget: &mut usize, depth: usize, numbers: &EquationNumbers, eqnos: &mut EquationNumbers) -> Vec<crate::model::Block> {
    if depth > 8 { return vec![]; }
    let mut out = vec![];
    let mut sequence = 0;
    for b in blocks {
        if *budget == 0 { break; }
        *budget -= 1;
        let end = b.text.text.char_indices().nth(600).map_or(b.text.text.len(), |(i, _)| i);
        // A table owns its cell trees inside the kind, so bound those before cloning.
        let kind = match &b.kind {
            BlockKind::Table(t) => {
                let nr = t.rows.len().min(6); let nc = t.col_widths.len().min(6);
                let rows = t.rows.iter().take(nr).enumerate().map(|(r, row)| row.iter().take(nc).enumerate().map(|(c, cell)| {
                    crate::model::TableCell { blocks: thumb_blocks(&cell.blocks, budget, depth + 1, numbers, eqnos), bg: cell.bg.clone(), valign: cell.valign.clone(),
                        colspan: cell.colspan.min((nc - c) as u32), rowspan: cell.rowspan.min((nr - r) as u32) }
                }).collect()).collect();
                BlockKind::Table(crate::model::Table { col_widths: t.col_widths.iter().take(nc).copied().collect(), header_rows: t.header_rows.min(nr), header_col: t.header_col, rows })
            }
            BlockKind::Image { src, width, height, .. } => {
                let mut kind = b.kind.clone();
                // Resolve missing dimensions once per cached thumbnail, not each frame.
                let dimensions = crate::store::image_path(src).and_then(|p| image::ImageReader::open(p).ok())
                    .and_then(|r| r.with_guessed_format().ok()).and_then(|r| r.into_dimensions().ok());
                let (iw, ih) = dimensions.map(|(w, h)| (w.max(1) as f32, h.max(1) as f32)).unwrap_or((690., 460.));
                let w = width.unwrap_or_else(|| height.map(|h| h * iw / ih).unwrap_or(iw)).max(1.);
                let h = height.unwrap_or(w * ih / iw).max(1.);
                // No wider than the page: a 1536 px picture would otherwise reserve its full height below a narrower drawing.
                let (w, h) = if w > 690. { (690., h * 690. / w) } else { (w, h) };
                if let BlockKind::Image { width, height, .. } = &mut kind { *width = Some(w); *height = Some(h); }
                kind
            }
            BlockKind::Raw { tag, .. } => BlockKind::Raw { tag: tag.clone(), xml: String::new() },
            // a 数据表 (a table file's card, or one in a page): its first rows and columns as a table, header first
            BlockKind::DataTable(r) => {
                let grid = fdl_core::table::block_grid(&crate::store::Store::active_dir(), r);
                let (nr, nc) = (grid.len().min(9), grid.first().map_or(0, |h| h.len().min(5)));
                if nr == 0 || nc == 0 { BlockKind::Table(crate::model::Table { col_widths: vec![], header_rows: 0, header_col: false, rows: vec![] }) } else {
                    let cell = |s: &str| crate::model::TableCell { blocks: vec![crate::model::Block::text(s)], ..crate::model::TableCell::empty() };
                    let rows = grid.iter().take(nr).map(|row| (0..nc).map(|c| cell(row.get(c).map_or("", |s| s.as_str()))).collect()).collect();
                    BlockKind::Table(crate::model::Table { col_widths: vec![crate::model::Table::DEFAULT_WIDTH * 1.4; nc], header_rows: 1, header_col: false, rows })
                }
            }
            kind => kind.clone(),
        };
        let text = b.text.slice(0..end);
        let mut preview = crate::model::RichText::default();
        for (range, marks) in text.spans() {
            let mut marks = marks.clone();
            if marks.inline.as_ref().is_some_and(|i| i.equation().is_some()) {
                preview.push(&text.text[range], marks);
                continue;
            }
            let label = marks.inline.as_ref().map(|inline| match &inline.kind {
                crate::model::InlineKind::Equation(_) => unreachable!(),
                crate::model::InlineKind::File(file) => file.name.clone(),
                crate::model::InlineKind::Button(button) => button.text.clone(),
                crate::model::InlineKind::Reminder(reminder) => crate::editor::reminder::reminder_label(reminder, " "),
                crate::model::InlineKind::DocLink(doc) => display_title(&doc.title).to_string(),
                crate::model::InlineKind::Other { tag } => tag.clone(),
            }).map(|label| label.chars().take(160).collect::<String>());
            if let Some(inline) = &marks.inline { match &inline.kind {
                crate::model::InlineKind::Equation(_) => {},
                crate::model::InlineKind::Button(_) => marks.bold = true,
                crate::model::InlineKind::DocLink(_) | crate::model::InlineKind::File(_) => marks.link = Some(String::new()),
                _ => {},
            } }
            marks.inline = None;
            preview.push(&text.text[range].replace('\u{fffc}', label.as_deref().unwrap_or("")), marks);
        }
        let mut copy = crate::model::Block::new(kind, preview);
        if let Some(n) = numbers.get(&b.id) { eqnos.insert(copy.id, *n); }
        if let BlockKind::Ordered { seq } = &mut copy.kind { sequence = seq.unwrap_or(sequence + 1); *seq = Some(sequence); } else { sequence = 0; }
        copy.bg = b.bg.clone(); copy.align = b.align;
        copy.children = thumb_blocks(&b.children, budget, depth + 1, numbers, eqnos);
        out.push(copy);
    }
    out
}

/// Home cards decode pictures at this share of the size they are drawn:
/// a card is small and glanced at, so a softer picture costs nothing.
const THUMB: f32 = 0.75;

/// Long edge drawn for a picture `w` wide with this width / height `ratio`.
fn thumb_long(w: f32, ratio: f32) -> f32 { (if ratio >= 1. { w } else { w / ratio }) * THUMB }

fn thumb_block(b: &crate::model::Block, depth: usize, inherited: Hsla, quote: bool, eqnos: &EquationNumbers) -> AnyElement {
    thumb_block_in(b, depth, inherited, quote, false, 0, eqnos)
}

fn thumb_block_in(b: &crate::model::Block, depth: usize, inherited: Hsla, quote: bool, cell: bool, list_depth: usize, eqnos: &EquationNumbers) -> AnyElement {
    let s = bs::SCALE;
    let metrics = bs::typography(&b.kind);
    let (font_size, line_height) = if cell && !matches!(b.kind, BlockKind::Heading { .. } | BlockKind::Code { .. }) { (14., 22.) } else { (metrics.size, metrics.line_height) };
    let done = matches!(b.kind, BlockKind::Todo { done: true });
    let color = if done { Theme::todo_done() } else { inherited };
    let mut node = div().w_full().min_w_0().flex_none().flex().flex_col()
        .text_size(px(font_size * s)).line_height(px(line_height * s)).text_color(color)
        .when(b.kind.has_text(), |d| d.min_h(px(line_height * s)));
    if let Some(bg) = b.bg.as_ref().and_then(|c| theme::block_bg(&c.0)) { node = node.rounded(px(6. * s)).bg(bs::display_fill(bg)); }
    match &b.kind {
        BlockKind::Image { src, width, height, caption, .. } => {
            let path = crate::store::image_path(src);
            let ratio = width.unwrap_or(690.).max(1.) / height.unwrap_or(460.).max(1.);
            node = match b.align { crate::model::Align::Left => node.items_start(), crate::model::Align::Center => node.items_center(), crate::model::Align::Right => node.items_end() };
            // Reserve the proportional height at the available width before loading.
            let media = div().relative().flex_none().w_full().max_w_full().overflow_hidden().when_some(*width, |d, w| d.w(px(w * s)))
                .map(|mut d| { d.style().aspect_ratio = Some(ratio); d });
            node = node.child(if let Some(path) = path {
                // The web demo fetches pictures on first use: an empty frame until then.
                if fdl_core::vfs::pending(&path) { media.into_any_element() } else {
                media.child(picture(&path, thumb_long(width.unwrap_or(690.).min(690.), ratio) * s).absolute().inset_0().size_full().min_w_0().min_h_0().rounded(px(bs::RADIUS * s)).object_fit(ObjectFit::Contain)).into_any_element() }
            } else {
                media.rounded(px(bs::RADIUS * s)).bg(bs::code_surface()).flex().items_center().justify_center().child(icon("image-broken", 24. * s, Theme::text_disabled())).into_any_element()
            });
            return node.when_some(caption.as_ref().filter(|c| !c.is_empty()), |d, caption| d.child(div().w_full().max_w_full().when_some(*width, |d, w| d.w(px(w * s))).mt(px(6. * s)).text_size(px(13. * s)).line_height(px(20. * s)).text_color(Theme::text_caption()).text_center().child(caption.clone()))).into_any_element();
        }
        BlockKind::Divider => return node.h(px(25. * s)).justify_center().child(div().h(px(s)).w_full().bg(Theme::line_divider())).into_any_element(),
        BlockKind::Grid => node = node.flex_row().gap(px(20. * s)),
        BlockKind::Column { .. } => node = node.flex_1(),
        BlockKind::Quote => node = node.relative().py(px(bs::QUOTE_PAD * s)).pl(px(bs::QUOTE_INSET * s))
            .child(div().absolute().left_0().top_0().bottom_0().w(px(bs::QUOTE_BAR * s)).rounded_full().bg(color)),
        BlockKind::Callout { emoji, bg, border, text_color } => {
            let tc = text_color.as_ref().and_then(|c| theme::text_color(&c.0)).map(bs::display_ink).unwrap_or(color);
            let mut body = div().flex_1().min_w_0().flex().flex_col();
            if depth < 8 { for (i, child) in b.children.iter().enumerate() {
                let gap = if i == 0 { 0. } else { bs::gap(&b.children[i - 1].kind).1.max(bs::gap(&child.kind).0) };
                body = body.child(div().mt(px(gap * s)).child(thumb_block_in(child, depth + 1, tc, quote, cell, list_depth, eqnos)));
            } }
            let first_line = b.children.first().map(|b| bs::typography(&b.kind).line_height).unwrap_or(theme::BODY_LINE);
            return node.relative().p(px(bs::INSET * s)).rounded(px(bs::CALLOUT_RADIUS * s)).border(px(s)).border_color(bs::border(border.as_ref().map(|c| c.0.as_str()))).bg(bs::fill(bg.as_ref().map(|c| c.0.as_str())))
                .child(div().flex().gap(px(bs::ICON_GAP * s)).child(div().size(px(bs::ICON * s)).mt(px((first_line - bs::ICON) / 2. * s)).text_size(px(20. * s)).line_height(px(22. * s)).flex_none().child(emoji.clone())).child(body)).into_any_element();
        }
        BlockKind::Code { lang, caption, wrap, .. } => {
            node = node.rounded(px(bs::RADIUS * s)).border(px(s)).border_color(Theme::line_border()).bg(bs::code_surface()).font_family(theme::MONO_FONT)
                .child(div().h(px(bs::CODE_HEADER * s)).pl(px(28. * s)).pr(px(8. * s)).rounded_t(px((bs::RADIUS - 1.) * s)).bg(bs::code_header()).flex().items_center().font_family(theme::ui_family()).text_size(px(13. * s)).text_color(Theme::text_caption()).child(caption.clone().unwrap_or_else(|| crate::settings::text("代码块").into())));
            let spans = crate::highlight::spans(lang, &b.text.text);
            let mut body = div().pt(px(14. * s)).pb(px(18. * s)).pr(px(20. * s)).flex().flex_col();
            let mut off = 0;
            for (i, line) in b.text.text.split('\n').enumerate() {
                let end = off + line.len();
                let mut cuts = vec![off, end];
                for (r, _, _) in &spans {
                    if r.start > off && r.start < end { cuts.push(r.start); }
                    if r.end > off && r.end < end { cuts.push(r.end); }
                }
                cuts.sort_unstable(); cuts.dedup();
                let runs = cuts.windows(2).map(|r| {
                    let h = spans.iter().find(|(h, _, _)| h.start <= r[0] && r[1] <= h.end);
                    let mut f = font(theme::MONO_FONT); if h.is_some_and(|(_, _, it)| *it) { f.style = FontStyle::Italic; }
                    TextRun { len: r[1] - r[0], font: f, color: h.map_or(Theme::text_code(), |(_, c, _)| *c), background_color: None, underline: None, strikethrough: None }
                }).collect::<Vec<_>>();
                body = body.child(div().flex().items_start()
                    .child(div().w(px(bs::CODE_GUTTER * s)).mr(px(bs::CODE_GAP * s)).flex_none().text_right().text_color(Theme::text_caption()).child((i + 1).to_string()))
                    .child(div().min_w_0().flex_1().overflow_hidden().when(!*wrap, |d| d.whitespace_nowrap()).child(if line.is_empty() { StyledText::new(" ") } else { StyledText::new(line.to_string()).with_runs(runs) })));
                off = end + 1;
            }
            return node.child(body).into_any_element();
        }
        BlockKind::Table(table) => {
            let widths: Vec<f32> = table.col_widths.iter().map(|w| w * s).collect();
            let mut table_el = div().grid().grid_col_widths(widths.clone()).w(px(widths.iter().sum())).max_w_full();
            for (r, c, value) in table.cells() {
                let (rs, cs) = (value.rowspan as usize, value.colspan as usize);
                let header = r < table.header_rows || (table.header_col && c == 0);
                let mut col = div().min_w_0().flex().flex_col().row_start(r as i16 + 1).row_end((r + rs) as i16 + 1).col_start(c as i16 + 1).col_end((c + cs) as i16 + 1)
                    .p(px(8. * s)).border_r(px(s)).border_b(px(s)).when(r == 0, |d| d.border_t(px(s))).when(c == 0, |d| d.border_l(px(s))).border_color(bs::table_line())
                    .when(header, |d| d.bg(Theme::table_header())).when_some(value.bg.as_ref().and_then(|c| theme::block_bg(&c.0)), |d, c| d.bg(bs::display_fill(c)))
                    .map(|d| match value.valign.as_deref() { Some("middle") => d.justify_center(), Some("bottom") => d.justify_end(), _ => d });
                for (i, child) in value.blocks.iter().enumerate() {
                    let gap = if i == 0 { 0. } else { bs::gap(&value.blocks[i - 1].kind).1.min(6.).max(bs::gap(&child.kind).0.min(8.)) };
                    col = col.child(div().mt(px(gap * s)).child(thumb_block_in(child, depth + 1, color, quote, true, 0, eqnos)));
                }
                table_el = table_el.child(col);
            }
            return node.child(table_el).into_any_element();
        }
        BlockKind::File(f) => return node.p(px(12. * s)).rounded(px(bs::RADIUS * s)).border(px(s)).border_color(Theme::line_border()).bg(bs::code_surface()).flex_row().items_center().gap(px(12. * s))
            .child(icon("file-text", 30. * s, Theme::icon())).child(div().flex_1().min_w_0().text_size(px(15. * s)).font_weight(FontWeight::MEDIUM).child(f.name.clone())).into_any_element(),
        BlockKind::Countdown(c) => {
            let left = c.remaining(crate::time::now_ms());
            let color = u32::from_str_radix(c.color.trim_start_matches('#'), 16).map(theme::c).unwrap_or(Theme::primary_500());
            let mut ink = bs::display_ink(color); ink.l = ink.l.min(0.38);
            let mut row = div().flex().justify_center().gap(px(16. * s));
            for (i, value) in [left / 86400, left / 3600 % 24, left / 60 % 60, left % 60].into_iter().enumerate() {
                row = row.child(div().flex().flex_col().items_center().child(div().size(px(52. * s)).rounded(px(8. * s)).bg(bs::highlight(color)).flex().items_center().justify_center().font_family(theme::MONO_FONT).font_weight(FontWeight::MEDIUM).text_size(px(26. * s)).text_color(ink).child(format!("{value:02}")))
                    .child(div().mt(px(4. * s)).text_size(px(14. * s)).line_height(px(20. * s)).text_color(Theme::text_caption()).child([crate::settings::text("天"), crate::settings::text("时"), crate::settings::text("分"), crate::settings::text("秒")][i])));
            }
            return node.py(px(18. * s)).rounded(px(bs::RADIUS * s)).border(px(s)).border_color(Theme::line_border()).bg(bs::code_surface()).child(row).into_any_element();
        }
        BlockKind::SubPageList => {
            node = node.p(px(8. * s)).rounded(px(bs::RADIUS * s)).border(px(s)).border_color(Theme::line_border()).bg(Theme::bg_overlay());
            if b.children.is_empty() { return node.h(px(96. * s)).justify_center().text_center().text_color(Theme::text_caption()).child(crate::settings::text("目录下暂无子页面")).into_any_element(); }
            for child in &b.children { node = node.child(div().py(px(9. * s)).px(px(6. * s)).flex().items_start().gap(px(6. * s)).child(icon("doc", 16. * s, Theme::icon())).child(div().flex_1().min_w_0().font_weight(FontWeight::MEDIUM).child(child.text.text.clone()))); }
            return node.into_any_element();
        }
        // a 画板 block: its board fit into the block's box (painters cached by board version)
        BlockKind::Board(bd) => {
            let board = bd.clone();
            return node.h(px(bd.height * s)).rounded(px(8. * s)).border(px(s)).border_color(Theme::line_border()).bg(crate::board::render::canvas_color(bd)).overflow_hidden()
                .child(canvas(|_, _, _| {}, move |bounds, _, window, cx| crate::board::render::paint_card(&board, bounds, window, cx)).size_full()).into_any_element();
        }
        BlockKind::Raw { tag, .. } => return node.p(px(8. * s)).rounded(px(8. * s)).bg(Theme::bg_overlay()).text_color(Theme::text_caption()).child(crate::localized_format!("<{tag}> 已保留的内容", "<{tag}> Preserved content")).into_any_element(),
        _ => {}
    }
    if b.kind.has_text() {
        let runs = b.text.runs.iter().map(|r| {
            let mut f = font(if r.marks.code { theme::MONO_FONT } else { theme::ui_family() });
            f.weight = if r.marks.bold { FontWeight::BOLD } else { bs::weight(&b.kind) };
            if r.marks.italic || quote { f.style = FontStyle::Italic; }
            let mut ink = if r.marks.link.is_some() { Theme::link() } else { r.marks.text_color.as_ref().and_then(|c| theme::text_color(&c.0)).map(bs::display_ink).unwrap_or(color) };
            if done { ink.a = 0.5; }
            let mark = r.marks.bg_color.as_ref().and_then(|c| theme::bg_color(&c.0)).map(bs::highlight);
            TextRun { len: r.len, font: f, color: if mark.is_some() { crate::theme::on_mark(ink) } else { ink },
                background_color: mark.or_else(|| r.marks.code.then(bs::code_surface)),
                underline: (r.marks.underline || r.marks.link.is_some()).then_some(UnderlineStyle { thickness: px(s), color: None, wavy: false }),
                strikethrough: (r.marks.strike || done).then_some(StrikethroughStyle { thickness: px(s), color: None }) }
        }).collect::<Vec<_>>();
        let marker = match b.kind {
            BlockKind::Bullet => Some(div().w(px(24. * s)).h(px(line_height * s)).pl(px(4. * s)).flex_none().flex().items_center().child(match list_depth % 3 {
                0 => div().size(px(5.5 * s)).rounded_full().bg(Theme::text_caption()),
                1 => div().size(px(5.5 * s)).rounded_full().border(px(s)).border_color(Theme::text_caption()),
                _ => div().size(px(5. * s)).bg(Theme::text_caption()),
            }).into_any_element()),
            BlockKind::Ordered { seq } => Some(div().min_w(px(18. * s)).mr(px(6. * s)).flex_none().text_color(Theme::text_caption()).child(crate::editor::core::ordered_marker(seq.unwrap_or(1), list_depth)).into_any_element()),
            BlockKind::Todo { done } => Some(div().w(px(24. * s)).h(px(line_height * s)).flex_none().flex().items_center().child(div().size(px(16. * s)).rounded(px(4. * s)).border(px(s)).border_color(if done { Theme::primary_500() } else { Theme::text_disabled() }).bg(if done { Theme::primary_500() } else { Theme::bg_body() }).flex().items_center().justify_center().when(done, |d| d.child(icon("todo-check", 12. * s, gpui::white())))).into_any_element()),
            _ => None,
        };
        let text = if b.text.runs.iter().any(|r| r.marks.inline.as_ref().is_some_and(|i| i.equation().is_some())) {
            text_preview::PreviewText { text: b.text.clone(), runs, size: font_size * s, line_height: line_height * s, align: b.align, eq_counter: eqnos.get(&b.id).copied().unwrap_or(0) }.into_any_element()
        } else { StyledText::new(b.text.text.clone()).with_runs(runs).into_any_element() };
        node = node.child(div().flex().items_start().children(marker).child(div().min_w_0().flex_1().map(|d| match b.align { crate::model::Align::Center => d.text_center(), crate::model::Align::Right => d.text_right(), _ => d }).child(text)));
    }
    if depth < 8 { for (i, child) in b.children.iter().enumerate() {
        let nested = b.kind.nests();
        let gap = if i == 0 { if nested { 8. } else { 0. } } else { bs::gap(&b.children[i - 1].kind).1.max(bs::gap(&child.kind).0) };
        node = node.child(div().min_w_0().when(!matches!(b.kind, BlockKind::Grid), |d| d.mt(px(gap * s)))
            .when(nested, |d| d.pl(px(25. * s)))
            .when(matches!(b.kind, BlockKind::Grid), |d| d.flex().w_0().map(|mut d| { d.style().flex_grow = Some(match child.kind { BlockKind::Column { ratio } => ratio, _ => 1. }); d }))
            .child(thumb_block_in(child, depth + 1, color, quote || matches!(b.kind, BlockKind::Quote), cell, list_depth + usize::from(nested), eqnos)));
    } }
    node.into_any_element()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SortDir { Asc, Desc }
impl SortDir {
    fn key(self) -> &'static str { match self { Self::Asc => "asc", Self::Desc => "desc" } }
    fn of(key: &str) -> Self { if key == "asc" { Self::Asc } else { Self::Desc } }
    fn flip(self) -> Self { match self { Self::Asc => Self::Desc, Self::Desc => Self::Asc } }
    fn label(self) -> &'static str { crate::settings::tr(if self == Self::Asc { "升序" } else { "降序" }, if self == Self::Asc { "Ascending" } else { "Descending" }) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HomeSort { Manual, Visited, Modified, Created, Title }
impl HomeSort {
    fn key(self) -> &'static str { match self { Self::Manual => "manual", Self::Visited => "visited", Self::Modified => "modified", Self::Created => "created", Self::Title => "title" } }
    fn of(key: &str) -> Self { match key { "manual" => Self::Manual, "modified" => Self::Modified, "created" => Self::Created, "title" => Self::Title, _ => Self::Visited } }
}
impl HomeSort {
    fn label(self) -> &'static str {
        match self {
            Self::Manual => crate::settings::tr("手动排序", "Manual order"),
            Self::Visited => "最近访问时间",
            Self::Modified => "最近修改时间",
            Self::Created => "创建时间",
            Self::Title => "标题",
        }
    }
    /// The direction a column sorts by the first time it is picked.
    fn default_dir(self) -> SortDir { if self == Self::Title { SortDir::Asc } else { SortDir::Desc } }
    /// 手动排序 has no direction; the others do.
    fn directional(self) -> bool { self != Self::Manual }
}

pub struct Home {
    pub tab: HomeTab,
    pub grid: bool,
    grid_scroll: UniformListScrollHandle,
    list_scroll: ScrollHandle,
    /// A folder's page has its own scroll positions, so 主页 keeps its place.
    folder_grid_scroll: UniformListScrollHandle,
    folder_list_scroll: ScrollHandle,
    grid_scrollbar: Rc<crate::design::ScrollBar>,
    list_scrollbar: Rc<crate::design::ScrollBar>,
    /// The 搜索 dialog's results (`search.rs`).
    pub(crate) search_scrollbar: Rc<crate::design::ScrollBar>,
    sort: HomeSort,
    sort_dir: SortDir,
    folders_first: bool,
    /// The header's 显示 menu (view and sort) is open.
    pub view_menu: bool,
    thumbs: RefCell<HashMap<String, (u64, Thumb, web_time::Instant)>>,
    /// Last-frame bounds of the controls the popovers hang from.
    anchors: Rc<RefCell<HashMap<&'static str, Bounds<Pixels>>>>,
    /// Folder cards and dragging into them (`home_folder.rs`).
    pub(crate) folders: crate::home_folder::HomeFolders,
    /// The level last shown, kept while the entries (`Level::key`: their
    /// `generation`) and the order stay as they were:
    /// copying and sorting a folder of 3,000 notes took most of a frame.
    level: RefCell<Option<Level>>,
    /// The list row under the pointer (its element id), so the hairlines on
    /// both sides of it hide (`ui::row_line`); the trash list shares it.
    pub(crate) list_hover: Option<SharedString>,
}

struct Level { key: u64, parent: Option<String>, order: Order, entries: Rc<Vec<Entry>> }

impl Default for Home {
    fn default() -> Self {
        Home {
            tab: HomeTab::Recent,
            grid: true,
            grid_scroll: Default::default(),
            list_scroll: Default::default(),
            folder_grid_scroll: Default::default(),
            folder_list_scroll: Default::default(),
            // both run down to the window's bottom corner
            grid_scrollbar: Rc::new(crate::design::ScrollBar::corners(0., crate::design::scroll::WINDOW_R)),
            list_scrollbar: Rc::new(crate::design::ScrollBar::corners(0., crate::design::scroll::WINDOW_R)),
            search_scrollbar: Default::default(),
            // Manual: the sidebar's order (`Meta::place`), so both lists read the same.
            sort: HomeSort::Manual,
            sort_dir: SortDir::Desc,
            folders_first: false,
            view_menu: false,
            thumbs: Default::default(),
            anchors: Default::default(),
            folders: Default::default(),
            level: Default::default(),
            list_hover: None,
        }
    }
}

impl Home {
    /// The page list's current order, for anything else that lists pages
    /// (the sidebar's tree).
    pub(crate) fn order(&self) -> Order {
        Order { field: self.sort, dir: self.sort_dir, folders_first: self.folders_first }
    }

    /// Tracks the hovered list row; true when it changed.
    pub(crate) fn hover_row(&mut self, key: &SharedString, on: bool) -> bool {
        let was = self.list_hover.clone();
        if on { self.list_hover = Some(key.clone()) } else if self.list_hover.as_ref() == Some(key) { self.list_hover = None }
        was != self.list_hover
    }

    pub(crate) fn anchor(&self, key: &str) -> Option<Bounds<Pixels>> {
        self.anchors.borrow().get(key).copied()
    }

    /// Records the element's bounds under `key` each frame.
    pub(crate) fn track(&self, key: &'static str) -> impl IntoElement {
        let rec = self.anchors.clone();
        canvas(move |b, _, _| {
            rec.borrow_mut().insert(key, b);
        }, |_, _, _, _| {})
        .absolute()
        .inset_0()
    }

    /// The list's columns: the library's (`library`: the top level is listed) or 收藏's.
    fn cols(&self, library: bool) -> Vec<Col> {
        if library { RECENT_COLS.to_vec() } else { STARRED_COLS.to_vec() }
    }

    /// The column drawn as sorted; 标题 and 手动排序 mark their own header.
    fn sort_col(&self) -> Option<Col> {
        match self.sort {
            HomeSort::Visited => Some(Col::Visited),
            HomeSort::Modified => Some(Col::Modified),
            HomeSort::Created => Some(Col::Created),
            HomeSort::Title | HomeSort::Manual => None,
        }
    }
}

/// The arrow on a sorted column: down for 降序, up for 升序. A chevron turned
/// over, rather than a second icon, so both directions share one glyph.
fn sort_glyph(dir: SortDir, size: f32) -> Svg {
    let g = icon("chevron-down", size, Theme::icon());
    if dir == SortDir::Asc { g.with_transformation(Transformation::rotate(radians(std::f32::consts::PI))) } else { g }
}

/// Sort 主页 and a folder's page. 手动排序 is the sidebar's order; the
/// automatic ones never write it, and fall back to it (then the id) for equal
/// values, so equal pages keep the order the user arranged by hand.
pub(crate) fn sort_entries(v: &mut [Entry], order: Order) {
    v.sort_by(|a, b| order.cmp(a, b));
}

/// How pages are ordered, as one value: 主页's setting, used there and by the
/// sidebar's tree so both lists always agree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Order {
    pub field: HomeSort,
    pub dir: SortDir,
    pub folders_first: bool,
}

impl Order {
    pub(crate) fn cmp(&self, a: &Entry, b: &Entry) -> std::cmp::Ordering {
        // 多个顶层文件夹: their rows keep folders.json's order; only what is in a top-level folder is sorted.
        let row = |e: &Entry| crate::shared_store::top_of_row(&e.id).is_some();
        if row(a) || row(b) {
            return row(b).cmp(&row(a)).then_with(|| (a.meta.place(), &a.id).cmp(&(b.meta.place(), &b.id)));
        }
        if self.field == HomeSort::Manual {
            return (a.meta.place(), &a.id).cmp(&(b.meta.place(), &b.id));
        }
        let asc = |o: std::cmp::Ordering| if self.dir == SortDir::Asc { o } else { o.reverse() };
        let order = match self.field {
            HomeSort::Title => asc(a.title.cmp(&b.title)),
            HomeSort::Visited => asc(visited(a).cmp(&visited(b))),
            HomeSort::Modified => asc(a.meta.modified.cmp(&b.meta.modified)),
            HomeSort::Created => asc(a.meta.created.cmp(&b.meta.created)),
            HomeSort::Manual => std::cmp::Ordering::Equal,
        };
        let group = if self.folders_first { b.meta.folder.is_some().cmp(&a.meta.folder.is_some()) } else { std::cmp::Ordering::Equal };
        group.then(order).then_with(|| a.meta.place().cmp(&b.meta.place())).then_with(|| a.id.cmp(&b.id))
    }
}

fn visited(e: &Entry) -> u64 {
    if e.meta.opened > 0 { e.meta.opened } else { e.meta.modified }
}

fn cell_text(e: &Entry, c: Col) -> String {
    match c {
        Col::Modified => crate::ui::format_visit(e.meta.modified),
        Col::Created => crate::ui::format_visit(e.meta.created),
        Col::Visited => crate::ui::format_visit(visited(e)),
        Col::Starred => crate::ui::format_visit(e.meta.starred),
    }
}

/// The title column grows three times as fast as the others.
fn weight3(mut d: Div) -> Div {
    d.style().flex_grow = Some(3.);
    d
}

/// A document's icon: its emoji, or the monochrome page glyph.
fn entry_icon(e: &Entry) -> AnyElement {
    if e.meta.folder.is_some() && e.note.is_none() { return icon("folder", 22., Theme::icon()).mr(px(13.)).into_any_element(); }
    match &e.icon {
        _ if e.meta.named_by_file() => crate::search::doc_icon_named(crate::search::file_glyph(&e.meta)).mr(px(12.)).into_any_element(),
        Some(em) => div().size(px(24.)).mr(px(12.)).flex_none().flex().items_center().justify_center().text_size(px(18.)).child(em.clone()).into_any_element(),
        None => doc_icon().mr(px(12.)).into_any_element(),
    }
}

impl Workspace {
    /// In 批量管理 a click checks or unchecks; otherwise it opens.
    fn home_click(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.batch.is_none() { return self.open(id, window, cx) }
        if !self.toggle_check(id) {
            self.show_toast_kind(crate::ui::ToastKind::Error, &crate::localized_format!("一次最多选择 {} 项", "Select up to {} items at a time", crate::sidebar::BATCH_LIMIT), cx);
        }
        cx.notify();
    }

    /// The row's / card's checkbox while 批量管理 is on.
    fn home_check(&self, e: &Entry) -> Option<crate::design::Checkbox> {
        use crate::sidebar::{BATCH_LIMIT, Check, checkbox};
        let sel = self.batch.as_ref()?;
        let on = sel.contains(&e.id);
        Some(checkbox(if on { Check::On } else { Check::Off }, !on && sel.len() >= BATCH_LIMIT))
    }

    pub(crate) fn home_entries(&self) -> Rc<Vec<Entry>> {
        match self.home.tab {
            // 我的文档 is the library's top level.
            HomeTab::Recent => self.level_entries(None),
            HomeTab::Starred => Rc::new(self.sorted_entries(self.entries.iter().filter(|e| e.meta.folder.is_none() && e.meta.starred > 0).cloned().collect())),
        }
    }

    /// One level of the tree as 主页 shows it (a folder's page uses it too).
    /// Cards show no body, links or reminders: the entries come without them (`tree::face`).
    pub(crate) fn level_entries(&self, parent: Option<&str>) -> Rc<Vec<Entry>> {
        let (key, order) = (self.entries.generation(), self.home.order());
        let mut level = self.home.level.borrow_mut();
        if let Some(l) = level.as_ref().filter(|l| l.key == key && l.order == order && l.parent.as_deref() == parent) { return l.entries.clone(); }
        // A folder's 正文 is the folder's own card, not one inside it.
        let notes: std::collections::HashSet<&str> = self.entries.iter().filter_map(|e| e.note.as_deref()).collect();
        let entries = Rc::new(self.sorted_entries(self.entries.iter().filter(|e| e.meta.parent.as_deref() == parent && !notes.contains(e.id.as_str())).map(crate::tree::face).collect()));
        *level = Some(Level { key, parent: parent.map(str::to_string), order, entries: entries.clone() });
        entries
    }

    pub(crate) fn prepare_manual_order(&mut self, cx: &mut Context<Self>) -> bool {
        if self.tree_writes_pending() { return false; }
        let previous = self.store.track_file_changes();
        match self.store.initialize_manual_order() {
            Err(e) => { self.show_toast_kind(crate::ui::ToastKind::Error, &e.to_string(), cx); return false; }
            // Already in order: nothing written, the rows stand.
            Ok(false) => return true,
            Ok(true) => self.entries.set(self.store.list()),
        }
        self.remember_files(previous, crate::settings::tr("初始化手动排序", "Initialize manual order"));
        true
    }

    pub(crate) fn manual_home_sort(&self) -> bool { self.home.sort == HomeSort::Manual && self.lists_library() }
    /// 主页 shows its 我的文档 tab: the top level under 主页, every top-level folder a card.
    pub(crate) fn lists_library(&self) -> bool { self.home.tab == HomeTab::Recent }

    /// A list header: clicking it sorts by that column; clicking the sorted
    /// one flips the direction. Headers without an order are not clickable.
    fn sort_head(&self, id: SharedString, label: &'static str, sorted: bool, on: Option<HomeSort>, cx: &mut Context<Self>) -> Stateful<Div> {
        let dir = self.home.sort_dir;
        div()
            .id(id)
            .relative()
            .px(px(4.))
            .flex()
            .items_center()
            .gap(px(4.))
            .rounded(px(ROW_RADIUS))
            .text_size(px(13.))
            .line_height(px(24.))
            .text_color(if sorted { Theme::text() } else { list_header_text() })
            .when(sorted, |d| d.font_weight(FontWeight::SEMIBOLD))
            .when(on.is_some(), |d| d.cursor_pointer().hover(|s| s.bg(Theme::fill_hover())))
            .child(crate::settings::label(label))
            .when(sorted, |d| d.child(sort_glyph(dir, 12.)))
            .when_some(on, |d, field| d.on_click(cx.listener(move |w, _, _, cx| w.sort_by(field, cx))))
    }

    /// Pick a sort field. Picking the current one flips 升序 / 降序; picking
    /// 手动排序 freezes the order shown right now, so later edits cannot
    /// reshuffle it.
    fn sort_by(&mut self, field: HomeSort, cx: &mut Context<Self>) {
        if field == HomeSort::Manual {
            if self.home.sort != HomeSort::Manual && !self.prepare_manual_order(cx) { return; }
            self.home.sort = HomeSort::Manual;
        } else if self.home.sort == field {
            self.home.sort_dir = self.home.sort_dir.flip();
        } else {
            self.home.sort = field;
            self.home.sort_dir = field.default_dir();
        }
        self.save_home();
        self.home.grid_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.home.folder_grid_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.home.list_scroll.set_offset(point(px(0.), px(0.)));
        self.home.folder_list_scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    /// Mix folders and documents unless the user explicitly groups folders first.
    fn sorted_entries(&self, mut v: Vec<Entry>) -> Vec<Entry> {
        sort_entries(&mut v, self.home.order());
        v
    }

    fn thumb(&self, e: &Entry) -> Thumb {
        use std::hash::{Hash, Hasher};
        let mut stamp = rustc_hash::FxHasher::default();
        e.meta.modified.hash(&mut stamp);
        crate::settings::feishu().hash(&mut stamp);
        // a 数据表's rows live in its file, saved apart from the index: its time is the card's too
        let file_time = (e.meta.kind == crate::store::DocKind::Table).then(|| self.store.file_path(&e.id).and_then(|p| std::fs::metadata(p).ok()?.modified().ok())).flatten();
        file_time.hash(&mut stamp);
        // and a page's embedded tables: a row changed there redraws the card
        let cached_tables = self.home.thumbs.borrow().get(&e.id).map(|(_, t, _)| tables_time(&t.tables)).unwrap_or(0);
        let mut page_stamp = stamp.clone();
        cached_tables.hash(&mut stamp);
        let base_stamp = stamp.finish();
        // If the preview contains no sibling subpages, modified + settings uniquely
        // identifies it without rebuilding the whole tree on every frame.
        if let Some((m, t, used)) = self.home.thumbs.borrow_mut().get_mut(&e.id)
            && !t.has_subpages && *m == base_stamp { *used = web_time::Instant::now(); return t.clone(); }
        // A child-page list shows the other pages in the document's folder, as in
        // the open document (`sync_sub_pages`).
        let tree = crate::tree::Tree::new(&self.entries);
        // A document with children (a folder's 正文) lists them.
        let level = if e.note.is_some() { Some(e.id.as_str()) } else { e.meta.parent.as_deref() };
        let children: Vec<&Entry> = tree.children(level).iter().copied().filter(|p| p.id != e.id).collect();
        let mut full_stamp = rustc_hash::FxHasher::default();
        e.meta.modified.hash(&mut full_stamp);
        file_time.hash(&mut full_stamp);
        crate::settings::feishu().hash(&mut full_stamp);
        for child in &children { (&child.id, &child.title, &child.icon).hash(&mut full_stamp); }
        let mut children_stamp = full_stamp.clone();
        cached_tables.hash(&mut full_stamp);
        let full_stamp = full_stamp.finish();
        if let Some((m, t, used)) = self.home.thumbs.borrow_mut().get_mut(&e.id)
            && *m == full_stamp { *used = web_time::Instant::now(); return t.clone(); }
        // The derived rows exist only in this preview tree and never enter the editor/store.
        fn subpages(blocks: &mut [crate::model::Block], entries: &[&Entry], budget: &mut usize) {
            for b in blocks {
                if matches!(b.kind, BlockKind::SubPageList) {
                    b.children = entries.iter().take(*budget).map(|e| crate::model::Block::text(&format!("{}{}", e.icon.as_ref().map(|i| format!("{i} ")).unwrap_or_default(), display_title(&e.title)))).collect();
                    *budget -= b.children.len();
                } else {
                    subpages(&mut b.children, entries, budget);
                    if let BlockKind::Table(t) = &mut b.kind { for row in &mut t.rows { for cell in row { subpages(&mut cell.blocks, entries, budget); } } }
                }
            }
        }
        let mut budget = 32;
        let mut eqnos = HashMap::new();
        let doc = self.store.load(e.note.as_deref().unwrap_or(&e.id));
        let loaded = doc.as_ref().map(|doc| {
            let numbers = crate::editor::equation_numbers(doc);
            thumb_blocks(&doc.blocks, &mut budget, 0, &numbers, &mut eqnos)
        });
        let read = loaded.is_ok();
        let mut blocks = loaded.unwrap_or_default();
        let has_subpages = has_subpage_list(&blocks);
        if has_subpages { subpages(&mut blocks, &children, &mut budget); }
        let mut tables = vec![];
        if let Ok(doc) = &doc { table_srcs(&doc.blocks, &mut tables); }
        // stamped with the tables just found, so the next frame finds it
        let tt = tables_time(&tables);
        let stamp = if has_subpages { tt.hash(&mut children_stamp); children_stamp.finish() } else { tt.hash(&mut page_stamp); page_stamp.finish() };
        let t: Thumb = Rc::new(ThumbContent { blocks, eqnos, has_subpages, tables });
        // A failed read (mid-write by another process, still in iCloud) is not
        // remembered: the card would stay blank until the document changes.
        if !read { return t; }
        let mut thumbs = self.home.thumbs.borrow_mut();
        // Keep recent previews warm without retaining every document ever visited.
        if thumbs.len() >= 128 && !thumbs.contains_key(&e.id) {
            let oldest = thumbs.iter().min_by_key(|(_, (_, _, used))| *used).map(|(id, _)| id.clone());
            if let Some(id) = oldest { thumbs.remove(&id); }
        }
        thumbs.insert(e.id.clone(), (stamp, t.clone(), web_time::Instant::now()));
        t
    }

    fn open_row_menu(&mut self, id: &str, key: &str, cx: &mut Context<Self>) {
        let b = self.more_bounds.borrow().get(key).copied();
        let Some(b) = b else { return };
        let (pos, corner) = crate::design::hang(b, crate::design::Align::End, None);
        self.row_menu = Some(RowMenu { id: id.to_string(), pos, corner, sub: false, batch: false, from: Some(key.to_string()) });
        cx.notify();
    }

    pub(crate) fn home_more(&self, e: &Entry, key: String, cx: &mut Context<Self>) -> crate::design::Button {
        let id = e.id.clone();
        let (rec, k2) = (self.more_bounds.clone(), key.clone());
        let open = self.row_menu.as_ref().is_some_and(|m| m.id == e.id && m.from.as_deref() == Some(key.as_str()));
        crate::design::Button::ghost(SharedString::from(key.clone()))
            .sized(crate::design::ButtonSize::Xs)
            .relative()
            .icon("ellipsis")
            .selected(open)
            .child(canvas(move |b, _, _| {
                rec.borrow_mut().insert(key.clone(), b);
            }, |_, _, _, _| {}).absolute().inset_0())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |w, _, _, cx| {
                cx.stop_propagation();
                w.open_row_menu(&id, &k2, cx);
            }))
    }

    /// A newly opened folder page starts at its top.
    pub(crate) fn reset_folder_scroll(&self) {
        self.home.folder_grid_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.home.folder_list_scroll.set_offset(point(px(0.), px(0.)));
    }

    /// 主页's view and sort order are remembered (`settings.json`).
    /// Whether 主页 lists by hand (手动排序) under these settings, as `load_home` reads them.
    pub(crate) fn sorts_by_hand(s: &crate::store::Settings) -> bool { s.home_sort.as_deref().map(HomeSort::of).unwrap_or(HomeSort::Manual) == HomeSort::Manual }

    pub(crate) fn load_home(&mut self) {
        let s = self.store.settings();
        self.home.grid = !s.home_list;
        self.home.folders_first = s.home_folders_first;
        self.home.sort = s.home_sort.as_deref().map(HomeSort::of).unwrap_or(HomeSort::Manual);
        // Settings from before directions existed keep the order they used to show.
        self.home.sort_dir = s.home_sort_dir.as_deref().map(SortDir::of).unwrap_or_else(|| self.home.sort.default_dir());
        self.store.set_place_by_hand(self.home.sort == HomeSort::Manual);
    }

    fn save_home(&mut self) {
        let mut s = self.store.settings();
        s.home_folders_first = self.home.folders_first;
        (s.home_list, s.home_sort) = (!self.home.grid, Some(self.home.sort.key().to_string()));
        s.home_sort_dir = Some(self.home.sort_dir.key().to_string());
        self.store.set_place_by_hand(self.home.sort == HomeSort::Manual);
        if let Err(e) = self.store.save_settings(&s) { eprintln!("无法保存主页视图：{e}"); }
    }

    /// 主页: the one-row header (`topbar`), then the top level in the view chosen.
    pub(crate) fn render_home(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let header = self.topbar(None, window, cx);
        // No top-level folder open (2.8): nothing to list or sort, a way to open one.
        if self.store.shown_tops().is_empty() {
            return div().flex_1().min_w_0().h_full().flex().flex_col().child(header).child(self.empty_main(cx)).into_any_element();
        }
        let entries = self.home_entries();
        let body = if entries.is_empty() {
            empty_state("illus/empty-list.svg", crate::settings::text("当前列表暂无内容")).into_any_element()
        } else if self.home.grid { self.render_home_grid(&entries, false, window, cx) } else { self.render_home_list(&entries, false, cx) };
        div().flex_1().min_w_0().h_full().flex().flex_col().child(header).child(div().h(px(8.)).flex_none()).child(body).into_any_element()
    }

    pub(crate) fn render_home_list(&self, entries: &[Entry], in_folder: bool, cx: &mut Context<Self>) -> AnyElement {
        let list_scroll = if in_folder { &self.home.folder_list_scroll } else { &self.home.list_scroll };
        let cols = self.home.cols(self.lists_library());
        let sort = self.home.sort_col();
        // The title is sized at about three times a column.
        let title_cell = || div().flex_grow_1().flex_shrink_1().flex_basis(relative(0.)).min_w(px(200.));
        let col_cell = || div().flex_grow_1().flex_shrink_1().flex_basis(relative(0.)).max_w(px(240.)).min_w(px(120.));
        // The list: 13pt tertiary headers over a hairline; the
        // sorted column in the text colour, semibold, with its arrow.
        // A row is lit while hovered or holding its menu; the lines touching it hide.
        let lit = |i: usize| entries.get(i).is_some_and(|e| {
            self.home.list_hover.as_ref().is_some_and(|h| *h == format!("home-row-{}", e.id))
                || self.row_menu.as_ref().is_some_and(|m| m.id == e.id)
        });
        let mut header = div()
            .h(px(33.))
            .mt(px(10.))
            .flex_none()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(row_line(lit(0)))
            .child(
                weight3(title_cell()).pl(px(20.)).child(
                    self.sort_head(SharedString::from("home-col-title"), "标题", self.home.sort == HomeSort::Title, Some(HomeSort::Title), cx),
                ),
            );
        for c in &cols {
            let sorted = sort == Some(*c);
            header = header.child(
                col_cell().pl(px(20.)).child(
                    self.sort_head(SharedString::from(format!("home-col-{}", c.label())), c.label(), sorted, c.sort(), cx),
                ),
            );
        }
        header = header.child(div().w(px(60.)).flex_none());
        let mut rows = div().id("home-rows").flex_1().min_h_0().overflow_y_scroll().track_scroll(list_scroll).px(px(24.)).pb(px(24.));
        if entries.is_empty() {
            rows = rows.child(div().pt(px(72.)).child(empty_state("illus/empty-list.svg", crate::settings::text("当前列表暂无内容"))));
        }
        for (i, e) in entries.iter().enumerate() {
            let (id, id2) = (e.id.clone(), e.id.clone());
            let menu_on = self.row_menu.as_ref().is_some_and(|m| m.id == e.id);
            let check = self.home_check(e);
            let key = SharedString::from(format!("home-row-{}", e.id));
            let mut row = div()
                .id(key.clone())
                .h(px(56.))
                .flex()
                .items_center()
                .when(i > 0, |d| d.border_t_1().border_color(row_line(lit(i - 1) || lit(i))))
                .cursor_pointer()
                .hover(row_hover)
                .on_hover(cx.listener(move |w, on: &bool, _, cx| if w.home.hover_row(&key, *on) { cx.notify() }))
                .when(menu_on, |d| d.bg(Theme::fill_hover()).rounded(px(ROW_RADIUS)))
                .on_click(cx.listener(move |w, _, window, cx| w.home_click(&id, window, cx)))
                .when(check.is_none(), |d| d.on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |w, ev: &MouseDownEvent, _, cx| {
                        w.row_menu = Some(RowMenu { id: id2.clone(), pos: ev.position, corner: Anchor::TopLeft, sub: false, batch: false, from: None });
                        cx.notify();
                    }),
                ))
                .child(
                    weight3(title_cell())
                        .pl(px(24.))
                        .pr(px(16.))
                        .flex()
                        .items_center()
                        .when_some(check, |d, c| d.child(c.mr(px(12.))))
                        .child(entry_icon(e))
                        .child(div().min_w_0().text_size(px(15.)).line_height(px(22.)).overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui_text(display_title(&e.title))))
                        .when(e.meta.pinned, |d| d.child(icon("pin-filled", 14., Theme::pinned()).ml(px(6.)))),
                );
            for c in &cols {
                row = row.child(col_cell().pl(px(24.)).text_size(px(13.)).text_color(Theme::text_caption()).whitespace_nowrap().overflow_hidden().child(cell_text(e, *c)));
            }
            row = row.child(div().w(px(60.)).flex_none().flex().justify_end().pr(px(12.)).when(self.batch.is_none(), |d| d.child(self.home_more(e, format!("home-more-{}", e.id), cx))));
            rows = rows.child(row);
        }
        div().flex_1().min_h_0().flex().flex_col().child(header.mx(px(24.)))
            .child(div().relative().flex_1().min_h_0().flex().flex_col().child(rows).child(self.home.list_scrollbar.render(list_scroll))).into_any_element()
    }

    pub(crate) fn render_home_grid(&self, entries: &Rc<Vec<Entry>>, in_folder: bool, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let grid_scroll = if in_folder { &self.home.folder_grid_scroll } else { &self.home.grid_scroll };
        let main_w = f32::from(window.viewport_size().width) - if self.sidebar_open { self.sidebar_w(window) } else { 0. };
        let avail = (main_w - 48.).max(1.);
        let entries = entries.clone();
        let mixed = entries.iter().any(|e| e.meta.folder.is_some() && e.note.is_none());
        // Folders spill past their cell (shadow, tab, the front flaring open), so
        // a level holding them gets more air between cells.
        let gap = if mixed { 24. } else { 16. };
        let wide_cols = (((avail + gap) / (200. + gap)).floor() as usize).max(1);
        let n = if wide_cols >= 4 { (((avail + gap) / (240. + gap)).floor() as usize).max(4) } else { (((avail + gap) / (160. + gap)).floor() as usize).clamp(1, 3) };
        // A document card is always 5:6 at its column's width; folders stand
        // taller by their tab so the pages line up beside them.
        let col_w = (avail - gap * (n - 1) as f32) / n as f32;
        let doc_h = col_w * (6. / 5.);
        let card_h = if mixed { doc_h / (1. - crate::home_folder::TAB) } else { doc_h };
        let rows = entries.len().div_ceil(n);
        let grid = uniform_list("home-grid-area", rows, cx.processor(move |workspace, range: std::ops::Range<usize>, window, cx| {
            range.map(|row| {
                let start = row * n;
                let end = (start + n).min(entries.len());
                div().w_full().h(px(card_h + gap)).px(px(24.)).pb(px(gap))
                    .grid().grid_cols(n as u16).gap(px(gap))
                    .children(entries[start..end].iter().map(|e| {
                        // A folder with a 正文 is a document with child pages: a document's card (#357).
                        if e.meta.folder.is_some() && e.note.is_none() { workspace.render_folder_card(e, col_w, card_h, window, cx) }
                        else if mixed { div().w_full().min_w_0().h(px(card_h)).flex().flex_col().justify_end()
                            .child(workspace.render_home_card(e, doc_h, cx)).into_any_element() }
                        else { workspace.render_home_card(e, card_h, cx) }
                    }))
            }).collect()
        }))
        // The last row's own 16 plus 32 more: 48 below the cards at the end,
        // clear of the window's rounded corner. (32: GPUI 0.2's uniform_list
        // counted its padding twice, and the cards end where that put them.)
        // The top padding sits inside the list's clip, so a folder's papers can
        // rise above the first row without being cut off — and that clip line is
        // where a row scrolling out of sight stops. It used to sit 4pt higher,
        // the list lifted into the tab row above it, so a scrolling row drew over
        // 我的文档 / 收藏 and the buttons beside them (#238). 28 under that lift is
        // 24 without it: the cards stay exactly where they were.
        .flex_1().min_h_0().pt(px(24.)).pb(px(32.)).track_scroll(grid_scroll);
        let scroll = grid_scroll.0.borrow().base_handle.clone();
        div().relative().flex_1().min_h_0().flex().flex_col()
            .on_mouse_move(cx.listener(|w, ev: &MouseMoveEvent, _, cx| w.track_home_press(ev, cx)))
            .child(grid).child(self.home.grid_scrollbar.render(&scroll)).into_any_element()
    }

    /// A document's first page in miniature: a card's face, and the papers
    /// in a folder that show one (`home_folder.rs`). `pad`: its margin.
    pub(crate) fn thumb_page(&self, e: &Entry, pad: f32) -> Div {
        let thumb = self.thumb(e);
        self.thumb_page_with(e, &thumb, pad)
    }

    fn thumb_page_with(&self, e: &Entry, thumb: &Thumb, pad: f32) -> Div {
        if thumb.blocks.is_empty() && e.title.trim().is_empty() {
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .child(img(crate::trash::illus_for("illus/blank-doc.svg")).size(px(64.)))
                    .child(div().mt(px(4.)).text_size(px(14.)).line_height(px(22.)).text_color(Theme::text_placeholder()).child(crate::settings::text("空白文档")))
            } else {
                let mut p = div().size_full().p(px(pad)).overflow_hidden().flex().flex_col();
                p = p.child(div().flex_none().flex().items_start().gap(px(6.)).mb(px(8.))
                    .when_some(e.icon.as_ref(), |d, emoji| d.child(div().flex_none().text_size(px(14.)).line_height(px(20.)).child(emoji.clone())))
                    .child(div().min_w_0().text_size(px(14.)).line_height(px(20.)).font_weight(FontWeight::BOLD).child(e.title.clone())));
                // A 画板: its board, drawn to fit the rest of the card.
                if e.meta.kind == crate::store::DocKind::Board {
                    let board = thumb.blocks.iter().find_map(|b| match &b.kind { BlockKind::Board(bd) => Some(bd.clone()), _ => None }).unwrap_or_default();
                    return p.child(canvas(|_, _, _| {}, move |bounds, _, window, cx| crate::board::render::paint_card(&board, bounds, window, cx)).flex_1().w_full());
                }
                for (i, block) in thumb.blocks.iter().enumerate() {
                    let (mt, _) = bs::gap(&block.kind);
                    let prev = if i == 0 { 0. } else { bs::gap(&thumb.blocks[i - 1].kind).1 };
                    p = p.child(div().flex_none().mt(px(mt.max(prev) * bs::SCALE)).child(thumb_block(block, 0, Theme::text(), false, &thumb.eqnos)));
                }
                p
            }
    }

    fn render_home_card(&self, e: &Entry, card_h: f32, cx: &mut Context<Self>) -> AnyElement {
            const RING: f32 = 3.;
            let id = e.id.clone();
            let id2 = e.id.clone();
            let thumb = self.thumb(e);
            let page = self.thumb_page_with(e, &thumb, 18.);
            let radius = 32.;
            let menu_on = self.row_menu.as_ref().is_some_and(|m| m.id == e.id);
            let check = self.home_check(e);
            let arm = e.id.clone();
            // Carried away as a paper: its place stays as an empty slot of the
            // card's shape (no page, no cover), so nothing half-shows through.
            if self.home_dragging(&e.id) {
                return div().id(SharedString::from(format!("home-card-{}", e.id))).w_full().min_w_0().h(px(card_h)).relative()
                    .child(self.record_home_card(&e.id))
                    .child(crate::ui::squircle_surface(radius, Theme::fill_hover()).size_full())
                    .into_any_element();
            }
                div()
                    .id(SharedString::from(format!("home-card-{}", e.id)))
                    .when(check.is_none(), |d| d.on_mouse_down(MouseButton::Left, cx.listener(move |w, ev: &MouseDownEvent, _, cx| w.arm_home_drag(&arm, ev.position, cx))))
                    .child(self.record_home_card(&e.id))
                    .w_full().min_w_0().h(px(card_h)).relative().group("home-card")
                    .flex_none()
                    // A loose sheet on the home page: squircle corners, a 0.5pt
                    // hairline and the faintest lift; hover darkens the hairline.
                    .cursor_pointer()
                    // Inset as the folder card's shadow box is (6 a side, 10 from the top).
                    .child(div().absolute().left(px(6.)).right(px(6.)).top(px(10.)).bottom_0().rounded(px(radius * 0.6)).shadow(theme::shadow_sheet()))
                    .on_click(cx.listener(move |w, _, window, cx| w.home_click(&id, window, cx)))
                    .when(check.is_none(), |d| d.on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |w, ev: &MouseDownEvent, _, cx| {
                            w.row_menu = Some(RowMenu { id: id2.clone(), pos: ev.position, corner: Anchor::TopLeft, sub: false, batch: false, from: None });
                            cx.notify();
                        }),
                    ))
                    .child(crate::ui::squircle_surface(radius, Theme::bg_body()).size_full().overflow_hidden().child(page.relative()))
                    .child(crate::ui::squircle_outline(radius, 0.5, Theme::line_sheet()))
                    // Hovered, or its menu open: a soft grey ring around the card.
                    .child(div().absolute().left(px(-RING)).right(px(-RING)).top(px(-RING)).bottom(px(-RING))
                        .when(!menu_on, |d| d.opacity(0.).group_hover("home-card", |s| s.opacity(1.)))
                        .child(crate::ui::squircle_outline(radius + RING, RING, Theme::ring_sheet())))
                    .when_some(check, |d, c| d.child(div().absolute().top(px(12.)).left(px(12.)).child(c)))
                    .when(self.batch.is_none(), |d| d.child(
                        div().absolute().top(px(8.)).right(px(8.))
                            .when(!menu_on, |d| d.opacity(0.).group_hover("home-card", |s| s.opacity(1.)))
                            .child(self.home_more(e, format!("home-gmore-{}", e.id), cx).rounded_full().bg(Theme::bg_overlay())),
                    ))
            .into_any_element()
    }

    /// 显示设置 and the sort dropdown: the menu panel; the sort item is a
    /// native menu row.
    pub(crate) fn render_home_menus(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out = vec![];
        if !matches!(self.view, crate::app::View::Home | crate::app::View::Folder) {
            return out;
        }
        let place = |pos: Point<Pixels>, corner: Anchor, menu: Popover| {
            deferred(anchored().anchor(corner).position(pos).snap_to_window_with_margin(px(8.)).child(menu.fit(crate::design::room(pos, corner)))).with_priority(2).into_any_element()
        };
        if self.home.view_menu && let Some(b) = self.home.anchor("home-view") {
            let (pos, corner) = crate::design::hang(b, crate::design::Align::End, None);
            out.push(place(pos, corner, self.view_menu(cx)));
        }
        out
    }
}

#[cfg(test)]
mod sort_tests {
    // Not `use super::*`: it would bring in gpui's `test` attribute macro.
    use super::{HomeSort, Order, SortDir, sort_entries};
    use crate::store::Entry;

    fn page(id: &str, title: &str, modified: u64, created: u64, order: u64, folder: bool) -> Entry {
        Entry {
            id: id.into(),
            meta: crate::store::Meta {
                modified,
                created,
                order,
                // A page inside a folder: its hand order is `order`, not
                // `top_order` (which is only for the top level).
                parent: Some("p".into()),
                folder: folder.then(|| format!("{id}-dir")),
                ..Default::default()
            },
            title: title.into(),
            icon: None,
            body: "".into(),
            reminders: vec![],
            links: vec![],
            note: None,
            top: String::new(),
        }
    }

    fn ids(v: &[Entry]) -> Vec<&str> { v.iter().map(|e| e.id.as_str()).collect() }

    #[test]
    fn directions_apply_to_every_field() {
        let mut v = vec![
            page("a", "Banana", 20, 200, 3, false),
            page("b", "Apple", 10, 100, 2, false),
            page("c", "Cherry", 30, 300, 1, false),
        ];
        sort_entries(&mut v, Order { field: HomeSort::Modified, dir: SortDir::Desc, folders_first: false });
        assert_eq!(ids(&v), ["c", "a", "b"]);
        sort_entries(&mut v, Order { field: HomeSort::Modified, dir: SortDir::Asc, folders_first: false });
        assert_eq!(ids(&v), ["b", "a", "c"]);
        sort_entries(&mut v, Order { field: HomeSort::Created, dir: SortDir::Desc, folders_first: false });
        assert_eq!(ids(&v), ["c", "a", "b"]);
        sort_entries(&mut v, Order { field: HomeSort::Title, dir: SortDir::Asc, folders_first: false });
        assert_eq!(ids(&v), ["b", "a", "c"]);
        sort_entries(&mut v, Order { field: HomeSort::Title, dir: SortDir::Desc, folders_first: false });
        assert_eq!(ids(&v), ["c", "a", "b"]);
    }

    #[test]
    fn manual_order_is_the_sidebar_order() {
        let mut v = vec![
            page("a", "A", 10, 10, 2, false),
            page("b", "B", 99, 99, 1, false),
            // Never placed by hand: above the placed ones, newest first.
            page("c", "C", 50, 50, 0, false),
        ];
        sort_entries(&mut v, Order { field: HomeSort::Manual, dir: SortDir::Desc, folders_first: false });
        assert_eq!(ids(&v), ["c", "b", "a"]);
    }

    #[test]
    fn equal_values_keep_the_hand_order() {
        // Same modification time: the hand order decides, in either direction.
        let mut v = vec![page("a", "A", 10, 10, 2, false), page("b", "B", 10, 10, 1, false)];
        sort_entries(&mut v, Order { field: HomeSort::Modified, dir: SortDir::Desc, folders_first: false });
        assert_eq!(ids(&v), ["b", "a"]);
        sort_entries(&mut v, Order { field: HomeSort::Modified, dir: SortDir::Asc, folders_first: false });
        assert_eq!(ids(&v), ["b", "a"]);
        // Title has no eponymous column, but sorts the same way.
        let mut v = vec![page("a", "Same", 10, 10, 2, false), page("b", "Same", 20, 20, 1, false)];
        sort_entries(&mut v, Order { field: HomeSort::Title, dir: SortDir::Asc, folders_first: false });
        assert_eq!(ids(&v), ["b", "a"]);
    }

    #[test]
    fn folders_first_groups_only_when_asked() {
        let mut v = vec![page("d", "Doc", 30, 30, 2, false), page("f", "Folder", 20, 20, 3, true)];
        sort_entries(&mut v, Order { field: HomeSort::Modified, dir: SortDir::Desc, folders_first: false });
        assert_eq!(ids(&v), ["d", "f"]);
        sort_entries(&mut v, Order { field: HomeSort::Modified, dir: SortDir::Desc, folders_first: true });
        assert_eq!(ids(&v), ["f", "d"]);
        // 手动排序 never groups: it is the order the sidebar shows.
        sort_entries(&mut v, Order { field: HomeSort::Manual, dir: SortDir::Desc, folders_first: true });
        assert_eq!(ids(&v), ["d", "f"]);
    }

    #[test]
    fn top_rows_keep_their_order() {
        // 多个顶层文件夹: the rows stay in folders.json's order (`top_order`) whatever the sort.
        let row = |key: &str, title: &str, at: u64| {
            let mut e = page(&crate::shared_store::top_row(key), title, 0, 0, 0, true);
            (e.meta.parent, e.meta.top_order) = (None, Some(at));
            e
        };
        let mut v = vec![row("b", "Alpha", 2), row("a", "Zulu", 1), page("x", "Middle", 5, 5, 0, false)];
        for field in [HomeSort::Title, HomeSort::Modified, HomeSort::Manual] {
            for dir in [SortDir::Asc, SortDir::Desc] {
                sort_entries(&mut v, Order { field, dir, folders_first: false });
                assert_eq!(ids(&v), ["top:a", "top:b", "x"]);
            }
        }
    }
}
