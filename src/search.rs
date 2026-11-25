//! 搜索 dialog: a 540-wide white dialog with the input row, 最近浏览 / hit
//! rows and a gray footer. Locally there is one owner and no sessions or
//! spaces, so each row's extra line is just 所有者 + the opened / modified
//! times. The design's `Dialog` shell, quiet row fills, monochrome page glyphs.

use crate::app::Workspace;
use crate::settings::display_title;
use crate::store::Entry;
use crate::design::{Button, ButtonSize, Dialog, Separator, scrim};
use crate::theme::{self, Theme};
use crate::ui::{ROW_RADIUS, format_visit, icon, ui_runs, ui_text};
use gpui::{prelude::FluentBuilder, *};
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

/// How many documents 最近浏览 lists.
const RECENT: usize = 20;
/// Body text kept for a hit's two-line summary.
const SUMMARY_CHARS: usize = 160;
/// The summary joins its blocks with three no-break spaces.
const SUMMARY_SEP: &str = "\u{a0}\u{a0}\u{a0}";
/// Text column of a row: 540 − list padding 6 / 6 (the right one holds the
/// scrollbar) − row padding 12 / 8 − icon 24 + 10.
const INFO_W: f32 = 474.;
/// A hit row without a summary: padding 8 / 8, title 20, extra line 2 + 20.
const ROW_H: f32 = 58.;

/// Case-insensitive match position in `hay`, in `hay`'s own byte offsets.
pub(crate) fn find_ci(hay: &str, needle_lower: &str) -> Option<Range<usize>> {
    if needle_lower.is_empty() {
        return None;
    }
    find_folded(hay, &hay.to_lowercase(), needle_lower)
}

/// [`find_ci`] with `hay.to_lowercase()` already at hand.
fn find_folded(hay: &str, lower: &str, needle_lower: &str) -> Option<Range<usize>> {
    if lower.len() == hay.len() {
        let i = lower.find(needle_lower)?;
        (hay.is_char_boundary(i) && hay.is_char_boundary(i + needle_lower.len())).then(|| i..i + needle_lower.len())
    } else {
        let i = hay.find(needle_lower)?;
        Some(i..i + needle_lower.len())
    }
}

/// Every match of `q` in `hay`.
fn find_all(hay: &str, q: &str) -> Vec<Range<usize>> {
    let mut out = vec![];
    let mut at = 0;
    while let Some(m) = find_ci(&hay[at..], q) {
        out.push(at + m.start..at + m.end);
        at += m.end;
    }
    out
}

/// Body text from the block holding the first match, with about 16
/// characters of context before it and the following blocks after it.
pub(crate) fn snippet(body: &str, q: &str) -> Option<(String, Vec<Range<usize>>)> {
    if q.is_empty() {
        return None;
    }
    snippet_folded(body, &body.to_lowercase(), q)
}

/// [`snippet`] with `body.to_lowercase()` already at hand.
fn snippet_folded(body: &str, lower: &str, q: &str) -> Option<(String, Vec<Range<usize>>)> {
    let text = snippet_text(body, lower, q)?;
    let hits = find_all(&text, q);
    Some((text, hits))
}

/// [`snippet_folded`]'s text without the hits.
fn snippet_text(body: &str, lower: &str, q: &str) -> Option<String> {
    if q.is_empty() {
        return None;
    }
    let m = find_folded(body, lower, q)?;
    let line_start = body[..m.start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let before: Vec<(usize, char)> = body[line_start..m.start].char_indices().collect();
    let cut = if before.len() > 16 { line_start + before[before.len() - 16].0 } else { line_start };
    let mut text = String::from(if cut > line_start { "…" } else { "" });
    let mut chars = 0;
    for line in body[cut..].lines().map(str::trim_end).filter(|l| !l.trim().is_empty()) {
        if !text.is_empty() && text != "…" {
            text.push_str(SUMMARY_SEP);
        }
        for ch in line.chars() {
            text.push(ch);
            chars += 1;
            if chars >= SUMMARY_CHARS {
                break;
            }
        }
        if chars >= SUMMARY_CHARS {
            break;
        }
    }
    Some(text)
}

/// The summary as two lines at most, ending in "…" when cut. GPUI's own
/// `line_clamp` truncates at twice the width, ignoring wrap leftovers.
pub(crate) fn summary_text(text: String, mut hits: Vec<Range<usize>>, width: f32, window: &Window) -> StyledText {
    let style = summary_style(window);
    let lines = |s: &str| summary_lines(s, &style, &[], width, window);
    let text = if lines(&text) <= 2 {
        text
    } else {
        let ends: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
        let (mut lo, mut hi) = (0, ends.len());
        while lo + 1 < hi {
            let mid = (lo + hi) / 2;
            if lines(&format!("{}…", &text[..ends[mid]])) <= 2 { lo = mid } else { hi = mid }
        }
        let cut = ends[lo];
        hits.retain(|r| r.end <= cut);
        format!("{}…", &text[..cut])
    };
    let runs = ui_runs(&text, &style, &hits, crate::design::color::foreground());
    StyledText::new(text).with_runs(runs)
}

fn summary_style(window: &Window) -> TextStyle {
    let mut style = window.text_style();
    style.font_family = theme::ui_family().into();
    style.font_size = px(12.).into();
    style.color = Theme::text_caption();
    style
}

/// How many lines `s`, with `hits` in the link colour, wraps to in a summary `width` wide.
fn summary_lines(s: &str, style: &TextStyle, hits: &[Range<usize>], width: f32, window: &Window) -> usize {
    window
        .text_system()
        .shape_text(s.to_string().into(), px(12.), &ui_runs(s, style, hits, crate::design::color::foreground()), Some(px(width)), None)
        .map(|ls| ls.iter().map(|l| l.wrap_boundaries().len() + 1).sum::<usize>())
        .unwrap_or(1)
}

/// A library document's title and body, lowercased once (`None` when that
/// changes nothing): lowercasing all of them on every keystroke, several
/// times a frame, made typing in 搜索 lag on big libraries (#332).
struct Folded {
    id: String,
    title: String,
    body: Arc<str>,
    title_l: Option<String>,
    body_l: Option<Box<str>>,
}

impl Folded {
    fn new(e: &Entry) -> Self {
        let title_l = e.title.to_lowercase();
        let body_l = e.body.to_lowercase();
        Folded {
            id: e.id.clone(),
            title: e.title.clone(),
            body: e.body.clone(),
            title_l: (title_l != e.title).then_some(title_l),
            body_l: (*body_l != *e.body).then(|| body_l.into()),
        }
    }
    fn title_l(&self) -> &str { self.title_l.as_deref().unwrap_or(&self.title) }
    fn body_l(&self) -> &str { self.body_l.as_deref().unwrap_or(&self.body) }
    /// Still describes `e`'s text.
    fn holds(&self, e: &Entry) -> bool {
        self.id == e.id && self.title == e.title && (Arc::ptr_eq(&self.body, &e.body) || self.body == e.body)
    }
}

/// The search index: [`Folded`] copies aligned with `Workspace::entries`,
/// checked against them on each use (they are replaced in many places), and
/// the last query's hits with their row heights.
#[derive(Default)]
pub(crate) struct SearchCache {
    docs: Vec<Folded>,
    /// Bumped whenever `docs` changes; older hits are stale.
    generation: u64,
    hits: Option<Hits>,
    /// Advances of printable ASCII in the summary font (界面字体), for [`min_width`].
    ascii: Option<(&'static str, [f32; 128])>,
}

struct Hits {
    generation: u64,
    q: String,
    /// Indices into the entries: title hits, then body hits, each in list order.
    rows: Vec<usize>,
    /// Each row's height in the 界面字体 it was measured in on first render.
    heights: Option<(&'static str, Vec<f32>)>,
}

impl SearchCache {
    fn sync(&mut self, entries: &[Entry]) {
        if self.docs.len() == entries.len() && self.docs.iter().zip(entries).all(|(d, e)| d.holds(e)) {
            // Same text in a new allocation: keep the new one so the next check is a pointer compare.
            for (d, e) in self.docs.iter_mut().zip(entries) { d.body = e.body.clone(); }
            return;
        }
        let mut old: HashMap<String, Folded> = std::mem::take(&mut self.docs).into_iter().map(|d| (d.id.clone(), d)).collect();
        self.docs = entries
            .iter()
            .map(|e| match old.remove(&e.id) {
                Some(d) if d.holds(e) => Folded { body: e.body.clone(), ..d },
                _ => Folded::new(e),
            })
            .collect();
        self.generation += 1;
    }

    /// The hits for `q` (lowercase, not empty) among `entries`.
    fn rows(&mut self, entries: &[Entry], q: &str) -> &mut Hits {
        self.sync(entries);
        if !matches!(&self.hits, Some(h) if h.generation == self.generation && h.q == q) {
            let (mut rows, mut body) = (vec![], vec![]);
            for (i, d) in self.docs.iter().enumerate() {
                // A folder with a 正文 is found as that document (#357), not twice.
                if entries[i].note.is_some() { continue; }
                if d.title_l().contains(q) {
                    rows.push(i)
                } else if d.body_l().contains(q) {
                    body.push(i)
                }
            }
            rows.extend(body);
            self.hits = Some(Hits { generation: self.generation, q: q.into(), rows, heights: None });
        }
        self.hits.as_mut().unwrap()
    }
}

/// A lower bound for the summary width of `s`: ASCII at 90% of its advance
/// (cached in `ascii`), CJK ideographs, kana and Hangul at 80% of the em,
/// the rest 0.
fn min_width(ascii: &mut Option<(&'static str, [f32; 128])>, s: &str, style: &TextStyle, window: &Window) -> f32 {
    let family = theme::ui_family();
    if ascii.as_ref().is_none_or(|(f, _)| *f != family) {
        let ts = window.text_system();
        let font = ts.resolve_font(&style.font());
        *ascii = Some((family, std::array::from_fn(|c| match c as u8 {
            b' '..=b'~' => ts.advance(font, px(12.), c as u8 as char).map(|s| f32::from(s.width)).unwrap_or(0.),
            _ => 0.,
        })));
    }
    let adv = &ascii.as_ref().unwrap().1;
    s.chars()
        .map(|c| match c as u32 {
            c @ 0..128 => adv[c as usize] * 0.9,
            0x3040..=0x30ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xac00..=0xd7a3 => 12. * 0.8,
            _ => 0.,
        })
        .sum()
}

/// `e` is a result for `q` through its body, not its title (see `SearchCache::rows`).
fn body_hit(e: &Entry, q: &str) -> bool { !q.is_empty() && !e.title.to_lowercase().contains(q) }

/// The glyph of a file titled by its name: Markdown, a 画板; else a page.
pub(crate) fn file_glyph(m: &crate::store::Meta) -> &'static str {
    match m.kind {
        crate::store::DocKind::Board => "board",
        crate::store::DocKind::Table => "doc-table",
        _ if m.markdown => "doc-md",
        _ => "doc",
    }
}

/// A result's icon: the page's emoji, or the page glyph.
pub(crate) fn entry_icon(e: &Entry) -> Div {
    match &e.icon {
        _ if e.meta.named_by_file() => doc_icon_named(file_glyph(&e.meta)),
        Some(em) => div().flex_none().size(px(24.)).mr(px(10.)).flex().items_center().justify_center().text_size(px(20.)).child(em.clone()),
        None => doc_icon(),
    }
}

/// A page without an emoji: the monochrome page glyph (the sidebar's), in
/// a 24 slot.
pub(crate) fn doc_icon() -> Div { doc_icon_named("doc") }

/// A page glyph by name (`doc`, `doc-md` for a Markdown file), in a 24 slot.
pub(crate) fn doc_icon_named(name: &str) -> Div {
    div()
        .flex_none()
        .size(px(24.))
        .mr(px(10.))
        .flex()
        .items_center()
        .justify_center()
        .child(icon(name, 18., Theme::icon()))
}

/// One `·`-separated part of a row's extra line.
fn extra_segment(glyph: Option<&'static str>, text: String) -> Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .children(glyph.map(|g| icon(g, 12., Theme::icon()).mr(px(4.))))
        .child(ui_text(text))
}

/// A result's extra line: 所有者 and the opened / modified times.
pub(crate) fn extra_line(e: &Entry) -> Div {
    let mut extra = vec![extra_segment(None, crate::settings::tr("所有者：我", "Owner: me").into())];
    if e.meta.opened > 0 {
        extra.push(extra_segment(Some("eye"), crate::localized_format!("你在 {}打开过", "Opened {}", format_visit(e.meta.opened))));
    }
    // a row with no modified time (a folder never written) says nothing rather than 1970
    if e.meta.modified > 0 {
        extra.push(extra_segment(Some("pen-line"), crate::localized_format!("我在 {}修改过", "Modified {}", format_visit(e.meta.modified))));
    }
    let last = extra.len() - 1;
    let extra = extra.into_iter().enumerate().flat_map(|(i, seg)| {
        let seg = if i == last { seg.flex_shrink_1().min_w_0().overflow_hidden() } else { seg };
        [(i > 0).then(|| div().flex_none().child(" · ")), Some(seg)].into_iter().flatten()
    });
    div()
        .mt(px(2.))
        .h(px(20.))
        .flex()
        .items_center()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(px(12.))
        .line_height(px(20.))
        .text_color(Theme::text_caption())
        .children(extra)
}

/// A result's title (13pt semibold, hits in the link colour), summary and extra line.
pub(crate) fn result_info(e: &Entry, q: &str, summary: Option<(String, Vec<Range<usize>>)>, width: f32, window: &Window) -> Div {
    let title = display_title(&e.title);
    let title_hit = find_ci(&title, q);
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(
            div()
                .text_size(px(13.))
                .line_height(px(20.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(Theme::text())
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(ui_text(title).hit(title_hit, crate::design::color::foreground())),
        )
        .children(summary.map(|(text, hits)| div().text_size(px(12.)).line_height(px(20.)).child(summary_text(text, hits, width, window))))
        .child(extra_line(e))
}

impl Workspace {
    pub(crate) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.still_loading(cx) { return }
        self.search_open = true;
        self.search_sel = Some(0);
        self.row_menu = None;
        self.doc_menu = false;
        self.doc_sub = None;
        let search = self.search.clone();
        search.update(cx, |s, cx| {
            // The dialog's query reads at body size, above the 13pt rows.
            s.size = 15.;
            s.line_height = 22.;
            s.set_text("", cx)
        });
        window.focus(&search.focus_handle(cx), cx);
        self.search_scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    pub(crate) fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.search_open {
            return;
        }
        self.search_open = false;
        if let Some(ed) = &self.editor {
            { let f = ed.read(cx).focus.clone(); window.focus(&f, cx); }
        }
        cx.notify();
    }

    /// First result is selected; an empty result list has no selection.
    pub(crate) fn default_search_sel(&self, cx: &App) -> Option<usize> {
        (!self.search_results(cx).is_empty()).then_some(0)
    }

    pub(crate) fn pick_search(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let q = self.query(cx);
        let Some((id, body_hit)) = self.search_results(cx).get(row).map(|e| (e.id.clone(), body_hit(e, &q))) else { return };
        self.close_search(window, cx);
        self.open(&id, window, cx);
        if let Some(ed) = &self.editor {
            // Found in the body: open at that hit, selected (#64). A title hit
            // opens where the document was left, as before.
            if body_hit && self.current.as_deref() == Some(id.as_str()) {
                ed.update(cx, |e, cx| e.reveal_body_hit(&q, window, cx));
            }
            { let f = ed.read(cx).focus.clone(); window.focus(&f, cx); }
        }
    }

    pub(crate) fn move_search_sel(&mut self, delta: isize, cx: &mut Context<Self>) {
        let n = self.search_results(cx).len();
        if n == 0 { self.search_sel = None; return; }
        let next = match self.search_sel {
            Some(i) => (i as isize + delta).rem_euclid(n as isize) as usize,
            None if delta > 0 => 0,
            None => n - 1,
        };
        self.search_sel = Some(next);
        let title = usize::from(self.query(cx).is_empty());
        self.search_scroll.scroll_to_item(next + title);
        cx.notify();
    }

    /// Title hits first, then body hits; with no query, 最近浏览.
    pub fn search_results(&self, cx: &App) -> Vec<&Entry> {
        self.search_rows(&self.query(cx)).into_iter().map(|i| &self.entries[i]).collect()
    }

    /// [`Self::search_results`] as indices into `entries`.
    fn search_rows(&self, q: &str) -> Vec<usize> {
        if q.is_empty() {
            // An attachment opened in its own window belongs to its page, not to 最近浏览.
            let mut recent: Vec<usize> = (0..self.entries.len()).filter(|&i| self.entries[i].note.is_none() && !self.entries[i].meta.attachment()).collect();
            recent.sort_by_key(|&i| std::cmp::Reverse(self.entries[i].meta.opened.max(self.entries[i].meta.modified)));
            recent.truncate(RECENT);
            return recent;
        }
        self.search_cache.borrow_mut().rows(&self.entries, q).rows.clone()
    }

    /// Each hit row's height for `q` (not empty): the summary's line count
    /// decides it, so only rows near the view need building.
    fn search_heights(&self, q: &str, window: &Window) -> Vec<f32> {
        let mut cache = self.search_cache.borrow_mut();
        let cache = &mut *cache;
        if let Some((font, h)) = &cache.rows(&self.entries, q).heights && *font == theme::ui_family() {
            return h.clone();
        }
        let SearchCache { docs, hits, ascii, .. } = cache;
        let hits = hits.as_mut().unwrap();
        let style = summary_style(window);
        let mut heights = Vec::with_capacity(hits.rows.len());
        for &i in &hits.rows {
            let d = &docs[i];
            heights.push(match snippet_text(&d.body, d.body_l(), q) {
                None => ROW_H,
                // Clearly wider than a line: two lines, cut by `summary_text` if longer.
                Some(text) if min_width(ascii, &text, &style, window) > INFO_W + 12. => ROW_H + 40.,
                // Else shaped as `summary_text` and the row will.
                Some(text) if summary_lines(&text, &style, &[], INFO_W, window) > 2 => ROW_H + 40.,
                Some(text) => {
                    let hits = find_all(&text, q);
                    ROW_H + 20. * summary_lines(&text, &style, &hits, INFO_W, window) as f32
                }
            });
        }
        hits.heights = Some((theme::ui_family(), heights.clone()));
        heights
    }

    /// Result `ix`, the entry at `i`.
    fn search_row(&self, i: usize, ix: usize, q: &str, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let e = &self.entries[i];
        let summary = match self.search_cache.borrow().docs.get(i) {
            Some(d) if d.holds(e) => snippet_folded(&d.body, d.body_l(), q),
            _ => snippet(&e.body, q),
        };
        // Result indices match keyboard selection.
        let row = ix;
        div()
            .id(("search-hit", ix))
            .flex_none()
            .pt(px(8.))
            .pr(px(8.))
            .pb(px(8.))
            .pl(px(12.))
            .flex()
            .items_center()
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .when(self.search_sel == Some(row), |d| d.bg(Theme::fill_hover()))
            .on_hover(cx.listener(move |w, hovered: &bool, _, cx| {
                if *hovered && w.search_sel != Some(row) {
                    w.search_sel = Some(row);
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |w, _, window, cx| w.pick_search(row, window, cx)))
            .child(entry_icon(e))
            .child(result_info(e, q, summary, INFO_W, window))
    }

    pub(crate) fn render_search(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.search_open {
            return None;
        }
        let q = self.query(cx);
        let has_text = !self.search.read(cx).text.is_empty();
        let results = self.search_rows(&q);
        let vh = f32::from(window.viewport_size().height);
        let dialog_h = 630f32.min(vh - 120.).max(200.);

        let input = div()
            .flex_none()
            .h(px(46.))
            .pl(px(12.))
            .pr(px(8.))
            .flex()
            .items_center()
            .child(
                // The glyph's ink fills 14.6 of the 16px box; Lucide's fills 20/24.
                div()
                    .relative()
                    .flex_none()
                    .size(px(16.))
                    .ml(px(4.))
                    .mr(px(10.))
                    .child(icon("search", 17.5, Theme::icon()).absolute().top(px(-0.75)).left(px(-0.75))),
            )
            .child(div().flex_1().min_w_0().pl(px(2.)).flex().child(self.search.clone()))
            .when(has_text, |d| {
                d.child(Button::ghost("search-clear").icon("search-clear").sized(ButtonSize::Xs).ml(px(8.)).on_click(cx.listener(
                    |w, _, window, cx| {
                        w.search.update(cx, |s, cx| s.set_text("", cx));
                        window.focus(&w.search.focus_handle(cx), cx);
                        w.search_sel = Some(0);
                        w.search_scroll.set_offset(point(px(0.), px(0.)));
                        cx.notify();
                    },
                )))
            })
            ;

        let mut list = div()
            .id("search-results")
            .flex_1()
            .min_h_0()
            .pt(px(4.))
            .px(px(6.))
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .track_scroll(&self.search_scroll);
        // the rows keep 6 from the side: while the list scrolls, the scrollbar's lane beside them
        list = self.home.search_scrollbar.make_room(list, 6.);
        if q.is_empty() && !results.is_empty() {
            list = list.child(
                div()
                    .flex_none()
                    .h(px(36.))
                    .pt(px(2.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .text_size(px(12.))
                    .line_height(px(18.))
                    .text_color(Theme::text_caption())
                    .child(crate::settings::tr("最近浏览", "Recently viewed")),
            );
        }
        if results.is_empty() && !q.is_empty() {
            list = list.child(
                div()
                    .flex_none()
                    .m(px(8.))
                    .px(px(10.))
                    .text_size(px(12.))
                    .line_height(px(18.))
                    .text_color(Theme::text_caption())
                    .child(crate::settings::text("没有找到匹配的结果，尝试其他关键词进行搜索")),
            );
        }
        if q.is_empty() {
            for (ix, &i) in results.iter().enumerate() {
                list = list.child(self.search_row(i, ix, &q, window, cx));
            }
        } else {
            // Hits can number thousands: rows away from the view and the
            // selection (which a keyboard move scrolls to this frame) are
            // spacers of the row's height, so the scrollbar and scroll
            // positions stay the same.
            let heights = self.search_heights(&q, window);
            let view = dialog_h - 46. - 1. - 36.;
            let top = -f32::from(self.search_scroll.offset().y);
            let (mut y, mut sel) = (4., None);
            if let Some(s) = self.search_sel.filter(|&s| s < heights.len()) {
                let (y0, y1) = (4. + heights[..s].iter().sum::<f32>(), 4. + heights[..=s].iter().sum::<f32>());
                // Off screen, it is scrolled to the top or bottom edge.
                if y0 < top || y1 > top + view { sel = Some((y0 - view, y1 + view)) }
            }
            for (ix, (&i, &h)) in results.iter().zip(&heights).enumerate() {
                let near = |(a, b): (f32, f32)| y + h > a && y < b;
                list = if near((top - 100., top + view + 100.)) || sel.is_some_and(near) {
                    list.child(self.search_row(i, ix, &q, window, cx).into_any_element())
                } else {
                    list.child(div().flex_none().h(px(h)).into_any_element())
                };
                y += h;
            }
        }

        let footer = div()
            .flex_none()
            .h(px(36.))
            .pl(px(14.))
            .pr(px(8.))
            .flex()
            .items_center()
            // GPUI clips children to the dialog's rect, not its rounded corners.
            .rounded_b(px(crate::design::geo().dialog_r))
            .border_t_1()
            .border_color(Theme::line_divider())
            .bg(Theme::bg_overlay())
            .text_size(px(12.))
            .line_height(px(18.))
            .text_color(Theme::text_caption())
            .whitespace_nowrap()
            .overflow_hidden()
            .child(ui_text(crate::settings::tr("支持 ↑↓ 键选择、Enter 键打开", "Use ↑↓ to select, Enter to open")));

        // The dialog's shell only: the input row, list and footer run edge to edge.
        let modal = Dialog::new("search-dialog")
            .w(px(540.))
            .h(px(dialog_h))
            .p_0()
            .gap_0()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(input)
            .child(Separator::new())
            // The thumb sits in the list's right padding (`make_room`), over no row.
            .child(div().flex_1().min_h_0().relative().flex().flex_col().child(list).child(self.home.search_scrollbar.render(&self.search_scroll)))
            .child(footer);

        Some(
            deferred(
                scrim()
                    .id("search-mask")
                    .on_mouse_down(MouseButton::Left, cx.listener(|w, _, window, cx| w.close_search(window, cx)))
                    .child(modal),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{SearchCache, body_hit, snippet};
    use crate::store::Entry;

    fn entry(id: &str, title: &str, body: &str) -> Entry {
        Entry { id: id.into(), meta: Default::default(), title: title.into(), icon: None, body: body.into(), reminders: vec![], links: vec![], note: None, top: String::new() }
    }

    /// The pre-#332 ranking: lowercase each title, then each body.
    fn reference(entries: &[Entry], q: &str) -> Vec<usize> {
        let mut hits: Vec<(u8, usize)> = entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                if e.title.to_lowercase().contains(q) { Some((0, i)) } else if e.body.to_lowercase().contains(q) { Some((1, i)) } else { None }
            })
            .collect();
        hits.sort_by_key(|(rank, _)| *rank);
        hits.into_iter().map(|(_, i)| i).collect()
    }

    #[test]
    fn hits_rank_titles_first_in_list_order() {
        let entries = vec![
            entry("a", "Notes", "the Apple pie"),
            entry("b", "APPLE harvest", ""),
            entry("c", "Ärger", "ÄPFEL und apple"),
            entry("d", "plain", "nothing"),
            entry("e", "Apple again", "apple"),
            entry("f", "苹果", "Apple 苹果"),
        ];
        let mut cache = SearchCache::default();
        for q in ["apple", "äp", "ä", "苹果", "e", "zzz"] {
            assert_eq!(cache.rows(&entries, q).rows, reference(&entries, q), "{q}");
        }
        assert_eq!(cache.rows(&entries, "apple").rows, [1, 4, 0, 2, 5]);
    }

    #[test]
    fn hits_follow_library_changes() {
        let mut entries = vec![entry("a", "One", "alpha"), entry("b", "Two", "beta")];
        let mut cache = SearchCache::default();
        assert_eq!(cache.rows(&entries, "beta").rows, [1]);
        cache.rows(&entries, "beta").heights = Some(("", vec![58.]));
        let generation = cache.generation;
        // The same text in new allocations (a fresh `Store::list`) keeps the hits.
        let fresh: Vec<Entry> = entries.iter().map(|e| entry(&e.id, &e.title, e.body.as_ref())).collect();
        assert!(cache.rows(&fresh, "beta").heights.is_some());
        assert_eq!(cache.generation, generation);
        // An edited body, a renamed title, an added and a removed document each drop them.
        entries[0].body = "alpha beta".into();
        assert_eq!(cache.rows(&entries, "beta").rows, [0, 1]);
        assert!(cache.rows(&entries, "beta").heights.is_none());
        entries[1].title = "Beta".into();
        assert_eq!(cache.rows(&entries, "beta").rows, [1, 0]);
        entries.insert(0, entry("c", "Gamma", "BETA"));
        assert_eq!(cache.rows(&entries, "beta").rows, [2, 0, 1]);
        entries.remove(2);
        assert_eq!(cache.rows(&entries, "beta").rows, [0, 1]);
        assert_eq!(cache.rows(&entries, "gamma").rows, [0]);
        // Reordered (sorted by modification) entries map to their new places.
        entries.reverse();
        assert_eq!(cache.rows(&entries, "gamma").rows, [1]);
        assert_eq!(cache.rows(&[], "gamma").rows, Vec::<usize>::new());
    }

    #[test]
    fn body_hits_open_at_the_hit() {
        // `rows` puts a document whose title holds the query with the title hits.
        assert!(!body_hit(&entry("a", "APPLE harvest", "apple"), "apple"));
        assert!(body_hit(&entry("b", "Notes", "the Apple pie"), "apple"));
        assert!(!body_hit(&entry("c", "Notes", "x"), ""));
    }

    #[test]
    fn snippet_context() {
        let (s, r) = snippet("first line\nsome long prefix text before the Needle here\nnext", "needle").unwrap();
        assert_eq!(&s[r[0].clone()], "Needle");
        assert!(s.starts_with('…'));
        assert!(s.ends_with("\u{a0}\u{a0}\u{a0}next"));
        let (s, r) = snippet("中文内容测试内容", "内容").unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(&s[r[1].clone()], "内容");
        assert!(snippet("abc", "x").is_none());
    }
}
