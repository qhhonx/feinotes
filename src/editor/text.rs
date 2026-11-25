//! Custom GPUI element that lays out and paints one block's rich text, its
//! selection, caret and IME marked range. Layout results are written into a
//! shared cache that the editor uses for hit-testing and caret navigation.

use crate::editor::bidi;
use crate::editor::core::Pos;
use crate::editor::math;
use crate::model::{Align, BlockId, Marks, RichText};
use crate::editor_theme::{self as theme, Theme, craft};
use gpui::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

/// One visual (wrapped) row of a block.
#[derive(Clone, Debug)]
pub struct Row {
    /// Byte range in the block text.
    pub range: Range<usize>,
    /// Index of the hard line (split on `\n`).
    pub hard: usize,
    /// Top of the row relative to the element origin.
    pub y: Pixels,
    /// Row height: the line height, or more when an equation sticks out.
    pub h: Pixels,
    /// Offset of the text line box (`line_height` tall) inside the row.
    pub base_dy: Pixels,
    /// x of the row start inside the unwrapped hard-line layout.
    pub x0: Pixels,
    /// x of the row end there.
    pub x1: Pixels,
    /// Horizontal offset from alignment.
    pub align_dx: Pixels,
}

pub struct TextLayout {
    pub bounds: Bounds<Pixels>,
    pub lines: Vec<(usize, WrappedLine)>,
    pub rows: Vec<Row>,
    pub line_height: Pixels,
    /// Laid out as code (monospace).
    pub mono: bool,
    /// Equations: byte offset and window bounds of the view (the clickable,
    /// highlighted box).
    pub eqs: Vec<(usize, Bounds<Pixels>)>,
    /// 按钮: byte offset and window bounds of the button box.
    pub buttons: Vec<(usize, Bounds<Pixels>)>,
}

impl TextLayout {
    /// Wrapped rows per hard line.
    pub fn rows_per_line(&self) -> Vec<usize> {
        let mut v = vec![0; self.rows.last().map_or(0, |r| r.hard + 1)];
        for r in &self.rows {
            v[r.hard] += 1;
        }
        v
    }

    fn row_for(&self, index: usize) -> usize {
        // Prefer the row that starts at `index` (caret after a wrap goes down).
        for (i, r) in self.rows.iter().enumerate() {
            let last_of_hard = self.rows.get(i + 1).is_none_or(|n| n.hard != r.hard);
            if index >= r.range.start && (index < r.range.end || (last_of_hard && index == r.range.end)) {
                return i;
            }
        }
        self.rows.len().saturating_sub(1)
    }

    /// Caret position (top-left of the caret) relative to window.
    pub fn position_for(&self, index: usize) -> Point<Pixels> {
        self.position_in_row(self.row_for(index), index)
    }

    /// Caret position at a wrap boundary, drawn at the end of the row above
    /// instead of the start of the row below.
    pub fn position_at_row_end(&self, index: usize) -> Point<Pixels> {
        self.position_in_row(self.row_ending_at(index).unwrap_or_else(|| self.row_for(index)), index)
    }

    fn position_in_row(&self, ri: usize, index: usize) -> Point<Pixels> {
        let Some(r) = self.rows.get(ri) else { return self.bounds.origin };
        let x = self.x_in_row(r, index) - r.x0 + r.align_dx;
        point(self.bounds.left() + x, self.bounds.top() + r.y + r.base_dy)
    }

    /// x of the caret before block offset `index` in the hard-line layout of
    /// row `r`. Right-to-left glyphs come in visual order (see `bidi`).
    fn x_in_row(&self, r: &Row, index: usize) -> Pixels {
        let (start, line) = &self.lines[r.hard];
        let i = index.saturating_sub(*start);
        let l = &line.unwrapped_layout;
        if bidi::has_rtl(&line.text) && let Some(x) = bidi::x_for_index(l, &line.text, r.x0..r.x1, i) {
            return x;
        }
        l.x_for_index(i)
    }

    /// Row `ri` is followed by another row of the same hard line.
    pub fn row_wraps(&self, ri: usize) -> bool {
        matches!((self.rows.get(ri), self.rows.get(ri + 1)), (Some(r), Some(n)) if r.hard == n.hard)
    }

    /// The row that ends at `index`: a wrap boundary is the end of one row and
    /// the start of the next, so the caret decides which row it is drawn in.
    fn row_ending_at(&self, index: usize) -> Option<usize> {
        (0..self.rows.len()).find(|&ri| self.row_wraps(ri) && self.rows[ri].range.end == index)
    }

    /// Row the caret is drawn in; `at_row_end` when it was placed at the end
    /// of a wrapped row.
    pub fn row_index_for_caret(&self, index: usize, at_row_end: bool) -> usize {
        match at_row_end.then(|| self.row_ending_at(index)).flatten() {
            Some(ri) => ri,
            None => self.row_for(index),
        }
    }

    /// `index` starts a wrapped row, so the caret may move to the end of the
    /// row above instead.
    pub fn starts_row(&self, index: usize) -> bool {
        (1..self.rows.len()).any(|ri| self.rows[ri].range.start == index && self.rows[ri - 1].hard == self.rows[ri].hard)
    }

    /// The equation whose view contains a window-space point.
    pub fn equation_at(&self, p: Point<Pixels>) -> Option<usize> {
        self.eqs.iter().find(|(_, b)| b.contains(&p)).map(|(ix, _)| *ix)
    }

    pub fn equation_bounds(&self, ix: usize) -> Option<Bounds<Pixels>> {
        self.eqs.iter().find(|(i, _)| *i == ix).map(|(_, b)| *b)
    }

    pub fn button_at(&self, p: Point<Pixels>) -> Option<usize> {
        self.buttons.iter().find(|(_, b)| b.contains(&p)).map(|(ix, _)| *ix)
    }

    pub fn button_bounds(&self, ix: usize) -> Option<Bounds<Pixels>> {
        self.buttons.iter().find(|(i, _)| *i == ix).map(|(_, b)| *b)
    }

    /// Closest text index for a window-space point.
    pub fn index_for(&self, p: Point<Pixels>) -> usize {
        self.caret_for(p).0
    }

    /// Closest text index for a window-space point, and whether it is the end
    /// of the row (the caret is drawn there, not at the next row's start).
    pub fn caret_for(&self, p: Point<Pixels>) -> (usize, bool) {
        if self.rows.is_empty() {
            return (0, false);
        }
        let local_y = p.y - self.bounds.top();
        let ri = if local_y < px(0.) {
            0
        } else {
            self.rows
                .iter()
                .position(|r| local_y < r.y + r.h)
                .unwrap_or(self.rows.len() - 1)
        };
        self.caret_in_row(ri, p.x)
    }

    pub fn index_in_row(&self, ri: usize, window_x: Pixels) -> usize {
        self.caret_in_row(ri, window_x).0
    }

    /// Closest text index in row `ri`, and whether it sits at the row's end.
    /// Both a CJK character and a space wrap at a glyph boundary, so the end
    /// of a row is a real caret stop of its own.
    pub fn caret_in_row(&self, ri: usize, window_x: Pixels) -> (usize, bool) {
        let r = &self.rows[ri];
        let (start, line) = &self.lines[r.hard];
        let x = window_x - self.bounds.left() - r.align_dx + r.x0;
        let l = &line.unwrapped_layout;
        let rtl = if bidi::has_rtl(&line.text) { bidi::index_for_x(l, &line.text, r.x0..r.x1, x) } else { None };
        let ix = (start + rtl.unwrap_or_else(|| l.closest_index_for_x(x.max(px(0.))))).clamp(r.range.start, r.range.end);
        (ix, self.row_wraps(ri) && ix == r.range.end)
    }

    /// Rectangles covering `range` (window coords), one per row.
    pub fn rects_for(&self, range: Range<usize>) -> Vec<Bounds<Pixels>> {
        let mut out = vec![];
        for (ri, r) in self.rows.iter().enumerate() {
            let a = range.start.max(r.range.start);
            let b = range.end.min(r.range.end);
            let last_of_hard = self.rows.get(ri + 1).is_none_or(|n| n.hard != r.hard);
            // An empty row fully inside the range still shows a sliver.
            if a > b || (a == b && !(range.start <= r.range.start && range.end > r.range.end)) {
                continue;
            }
            let (start, line) = &self.lines[r.hard];
            let top = self.bounds.top() + r.y + r.base_dy;
            let rect = |xa: Pixels, xb: Pixels| {
                let (xa, xb) = (xa - r.x0 + r.align_dx, xb - r.x0 + r.align_dx);
                Bounds::from_corners(point(self.bounds.left() + xa, top), point(self.bounds.left() + xb, top + self.line_height))
            };
            // Right-to-left text: the range may cover several pieces of the
            // row, and the line ends on its left.
            if bidi::has_rtl(&line.text) {
                let l = &line.unwrapped_layout;
                let mut parts = bidi::ranges_for(l, &line.text, r.x0..r.x1, a - start..b - start);
                if parts.is_empty() {
                    let x = self.x_in_row(r, a);
                    parts.push((x, x));
                }
                if range.end > r.range.end && last_of_hard {
                    if bidi::rtl_line(&line.text) {
                        let first = parts.iter_mut().min_by(|p, q| f32::from(p.0).total_cmp(&f32::from(q.0))).unwrap();
                        first.0 -= px(5.);
                    } else {
                        let last = parts.iter_mut().max_by(|p, q| f32::from(p.1).total_cmp(&f32::from(q.1))).unwrap();
                        last.1 += px(5.);
                    }
                }
                out.extend(parts.into_iter().map(|(xa, xb)| rect(xa, xb)));
                continue;
            }
            let xa = line.unwrapped_layout.x_for_index(a - start);
            let mut xb = line.unwrapped_layout.x_for_index(b - start);
            if range.end > r.range.end && last_of_hard {
                xb += px(5.);
            }
            out.push(rect(xa, xb));
        }
        out
    }
}

/// Each wrapped row's first byte and left x in the hard line's layout. The
/// leftmost glyph of a right-to-left row is its last character, so there the
/// row starts at the smallest byte offset drawn in it.
fn row_starts(line: &WrappedLine) -> Vec<(usize, Pixels)> {
    let layout = &line.unwrapped_layout;
    let mut out = vec![(0, px(0.))];
    for b in line.wrap_boundaries() {
        let g = &layout.runs[b.run_ix].glyphs[b.glyph_ix];
        out.push((g.index, g.position.x));
    }
    if out.len() > 1 && bidi::has_rtl(&line.text) {
        let mut wraps = line.wrap_boundaries().iter().peekable();
        let mut row = 0;
        for (run_ix, run) in layout.runs.iter().enumerate() {
            for (glyph_ix, g) in run.glyphs.iter().enumerate() {
                if wraps.peek().is_some_and(|b| b.run_ix == run_ix && b.glyph_ix == glyph_ix) {
                    wraps.next();
                    row += 1;
                }
                if row > 0 {
                    out[row].0 = out[row].0.min(g.index);
                }
            }
        }
    }
    out
}

/// A single-line field's horizontal scroll: the caret stays 8pt inside the
/// visible box, so a long URL opens on its tail and moving the caret reveals
/// the part of the text around it. `prev` keeps the view steady while typing.
pub fn hscroll(text_w: Pixels, caret_x: Pixels, view_w: Pixels, prev: Pixels) -> Pixels {
    let pad = px(8.);
    let max = (text_w + pad - view_w).max(px(0.));
    let mut s = prev.min(max);
    if caret_x - s > view_w - pad {
        s = caret_x - (view_w - pad);
    } else if caret_x - s < pad {
        s = caret_x - pad;
    }
    s.max(px(0.)).min(max)
}

pub type LayoutCache = Rc<RefCell<HashMap<BlockId, TextLayout>>>;

#[derive(Clone)]
pub struct TextSpec {
    pub size: f32,
    pub line_height: f32,
    pub weight: FontWeight,
    pub color: Hsla,
    pub mono: bool,
    pub strike_all: bool,
    pub wrap: bool,
    /// CSS letter-spacing in px (headings use -0.02em).
    pub letter_spacing: f32,
    /// Text colours and highlights at .7 opacity (inside a quote:
    /// `.docx-quote_container-block .textHighlight { opacity: .7 }`).
    pub dim_marks: bool,
    /// Links are underlined while the caret is in their block.
    pub link_underline: bool,
    /// Document text: drawn in the theme's body font (fields keep the UI font).
    pub doc_font: bool,
    /// A family of its own (headings, the title), else the body's.
    pub font: Option<&'static str>,
    /// Everything italic (a theme's quotes).
    pub italic: bool,
    /// Fonts by run instead (board text: Excalidraw's fonts, `board::fonts::runs`).
    pub font_runs: Option<FontRuns>,
    /// Every line's ascent and descent in px instead of its fonts' (board
    /// text: the board's own, so the baseline sits where the board draws it).
    pub metrics: Option<(f32, f32)>,
    /// Rows cut where these say instead of at the wrap width (board text:
    /// where the board wraps it).
    pub breaks: Option<Breaks>,
    /// Drawn this far from where layout puts it: less than a device pixel,
    /// which layout rounds positions to (board text: where the board's is).
    pub shift: Point<Pixels>,
}

/// A hard line's rows as byte ranges; what lies between two rows (spaces
/// dropped at a soft break) takes no room.
pub type Breaks = Rc<dyn Fn(&str) -> Vec<Range<usize>>>;

/// Cuts a span into runs by font (byte lengths).
pub type FontRuns = Rc<dyn Fn(&str) -> Vec<(usize, Font)>>;

impl TextSpec {
    fn shifted(&self, b: Bounds<Pixels>) -> Bounds<Pixels> { Bounds::new(b.origin + self.shift, b.size) }

    /// The page body: 15pt, 1.25 × the natural line height.
    pub fn body() -> Self {
        TextSpec {
            size: theme::body_size(),
            line_height: theme::body_line(),
            weight: FontWeight::NORMAL,
            color: Theme::text(),
            mono: false,
            strike_all: false,
            wrap: true,
            letter_spacing: 0.,
            dim_marks: false,
            link_underline: false,
            doc_font: true,
            font: None,
            italic: false,
            font_runs: None,
            metrics: None,
            breaks: None,
            shift: Point::default(),
        }
    }
}

pub struct RichTextElement {
    pub id: BlockId,
    pub text: RichText,
    pub spec: TextSpec,
    pub align: Align,
    pub placeholder: Option<SharedString>,
    pub placeholder_color: Option<Hsla>,
    /// Horizontal nudge for the placeholder glyphs only (caret stays put).
    pub placeholder_dx: f32,
    pub selection: Option<Range<usize>>,
    pub caret: Option<usize>,
    /// The caret sits at the end of a wrapped row: draw it there rather than
    /// at the start of the row below (both are the same text offset).
    pub caret_row_end: bool,
    pub marked: Option<Range<usize>>,
    pub cache: LayoutCache,
    pub show_caret: bool,
    pub highlight: Option<Box<dyn Fn(&str) -> Vec<crate::highlight::Span>>>,
    /// 查找和替换 hits as (range, is_current), tinted over the glyphs.
    pub found: Vec<(Range<usize>, bool)>,
    /// Equation numbers shown before this block (`katexEqnNo`).
    pub eq_counter: u32,
    /// The button whose 按钮设置 panel is open (`.editing`), shown with
    /// the panel's unsaved values.
    pub button_edit: Option<(usize, crate::model::InlineButton)>,
    /// Single-line fields keep their horizontal scroll here, so a long URL
    /// can be read and edited instead of being clipped on the right.
    pub scroll: Option<Rc<Cell<Pixels>>>,
    /// Unwrapped text as wide as its longest line (at least the parent), for
    /// a scroller around it: a code block that doesn't wrap (#161).
    pub fit_content: bool,
}

pub struct Prepared {
    lines: Vec<(usize, WrappedLine)>,
    wrap_width: Option<Pixels>,
    line_height: Pixels,
    /// (height, base_dy) of every row, in order.
    rows: Vec<(Pixels, Pixels)>,
    height: Pixels,
    /// The longest line, the width of a `fit_content` element.
    width: Pixels,
    eqs: Vec<Rc<EqGeom>>,
}

/// The equation box is `vertical-align: middle`: its centre sits this
/// far (in em of the paragraph font) above the baseline.
const EQ_MIDDLE: f32 = 0.375;

/// An equation's box inside its line. Coordinates are relative to the
/// box's left edge and vertical centre.
pub struct EqGeom {
    /// Byte offset of its U+FFFC.
    ix: usize,
    display: bool,
    /// Advance in the line (`.editor-kit-equation-block`, padding 0 1px).
    adv: f32,
    /// Layout height (a `transform: scale` shrink keeps it).
    box_h: f32,
    /// The view: x, y, w, h (radius 4, hover / selected backgrounds).
    view: [f32; 4],
    /// Empty or invalid: `.placeholder` background.
    placeholder: bool,
    /// Buttons sit relative to the top of the line box instead of the
    /// equation middle: the row extents they need there (top, bottom).
    ext: Option<[f32; 2]>,
    body: EqBody,
}

enum EqBody {
    /// Baseline origins of the formula and its `\tag`.
    Math { math: Rc<math::Math>, at: [f32; 2], tag_at: Option<[f32; 2]> },
    /// 添加 LaTeX 公式 / 无效公式 (with the warning icon's top-left).
    Text { line: ShapedLine, at: [f32; 2], icon: Option<[f32; 2]> },
    /// 按钮: `view` is the button box; the label is painted in a `bh` tall
    /// line box `pad` from its left edge. `wrap_h`: the wrapper's height
    /// (selected background).
    Button { line: ShapedLine, font: Font, color: String, disabled: bool, editing: bool, bh: f32, wrap_h: f32, pad: f32 },
    /// 文本视图 attachment: the type icon box (x, y, size) and the name
    /// painted from `text_x` in the row's line box.
    File { line: ShapedLine, font: Font, kind: crate::editor::file::Kind, icon: [f32; 3], text_x: f32 },
    /// An @文档 link: page icon (or the page's emoji) and title.
    Doc { line: ShapedLine, font: Font, emoji: Option<ShapedLine>, deleted: bool, icon: [f32; 3], text_x: f32 },
    /// 日期提醒: the clock icon box (x, y, size), filled when it notifies,
    /// and the date text from `text_x`.
    Reminder { line: ShapedLine, icon: [f32; 3], icon_color: Hsla, filled: bool, text_x: f32 },
}

enum ObjSrc {
    Eq(String),
    Btn(crate::model::InlineButton, bool),
    File(crate::model::FileBlock),
    Rem(crate::model::InlineReminder),
    Doc(crate::model::InlineDocLink),
}

thread_local! {
    /// Live titles and icons of the library's documents, for @文档 links
    /// (set by the editor from the workspace).
    pub static DOC_TITLES: std::cell::RefCell<std::collections::HashMap<String, (String, Option<String>)>> = Default::default();
}

/// `.embed-inline-link.mention-doc`: the 1em page icon and the live title
/// (weight 500) in the link colour, padding 3px 2px, radius 4.
fn doc_geom(ix: usize, l: &crate::model::InlineDocLink, spec: &TextSpec, row_w: f32, window: &mut Window) -> EqGeom {
    let (fs, lh) = (spec.size, spec.line_height);
    let live = DOC_TITLES.with(|t| t.borrow().get(&l.id).cloned());
    let deleted = live.is_none();
    let (title, emoji) = match live {
        Some((t, e)) => (crate::settings::display_title(&t).to_string(), e),
        None => (crate::settings::text("此文档已被删除").to_string(), None),
    };
    let font = Font {
        family: theme::DOC_UI_FONT.into(),
        features: FontFeatures::default(),
        fallbacks: Some(FontFallbacks::from_fonts(vec!["PingFang SC".into(), "Apple Color Emoji".into()])),
        weight: if spec.weight.0 > 500. { spec.weight } else { FontWeight(500.) },
        style: FontStyle::Normal,
    };
    let name: String = title.chars().take(200).collect();
    let color = if deleted { Theme::text_disabled() } else { Theme::link() };
    let run = TextRun { len: name.len(), font: font.clone(), color, background_color: None, underline: None, strikethrough: None };
    let line = window.text_system().shape_line(name.into(), px(fs), &[run], None);
    let emoji = emoji.map(|e| {
        let run = TextRun { len: e.len(), font: font.clone(), color, background_color: None, underline: None, strikethrough: None };
        window.text_system().shape_line(e.into(), px(fs), &[run], None)
    });
    let s = fs.round();
    // An emoji can be wider than the em box (Segoe UI Emoji on Windows): keep the title clear of it.
    let iw = emoji.as_ref().map_or(s, |e| f32::from(e.width).max(s));
    let text_x = 2. + iw + 2.;
    let w = (text_x + f32::from(line.width) + 2.).min(row_w.max(text_x + 2.));
    EqGeom {
        ix,
        display: false,
        adv: w,
        box_h: lh,
        view: [0., 0., w, lh],
        placeholder: false,
        ext: Some([0., lh]),
        body: EqBody::Doc { line, font, emoji, deleted, icon: [2., (lh - s) / 2., s], text_x },
    }
}

/// `.docx-reminder-inline-component`: margin-left .25em, padding
/// .125em .25em .125em 1.125em with the .875em clock as its background
/// (at 0 56%), in the status colour.
fn reminder_geom(ix: usize, r: &crate::model::InlineReminder, spec: &TextSpec, window: &mut Window) -> EqGeom {
    let (fs, lh) = (spec.size, spec.line_height);
    let font = Font {
        family: theme::DOC_UI_FONT.into(),
        features: FontFeatures::default(),
        fallbacks: Some(FontFallbacks::from_fonts(vec!["PingFang SC".into(), "Apple Color Emoji".into()])),
        weight: spec.weight,
        style: FontStyle::Normal,
    };
    let now = crate::time::now_ms() as i64;
    let text = crate::editor::reminder::chip_text(r, now);
    let (color, icon_color) = crate::editor::reminder::colors(r.expire, now);
    let run = TextRun { len: text.len(), font, color, background_color: None, underline: None, strikethrough: None };
    let line = window.text_system().shape_line(text.into(), px(fs), &[run], None);
    let m = 0.25 * fs;
    let text_x = m + 1.125 * fs;
    let w = text_x + f32::from(line.width) + 0.25 * fs;
    // The padding box: the font's content area (≈1.4em) and .25em of padding.
    let bh = (1.65 * fs).min(lh);
    let top = (lh - bh) / 2.;
    let s = 0.875 * fs;
    EqGeom {
        ix,
        display: false,
        adv: w,
        box_h: lh,
        view: [m, top, w - m, bh],
        placeholder: false,
        ext: Some([0., lh]),
        body: EqBody::Reminder { line, icon: [m, top + (bh - s) * 0.56, s], icon_color, filled: r.should_notify, text_x },
    }
}

/// `.embed-inline-link.file-block`: the type icon (1.15em, raised 2px)
/// and the name in the link colour, padding 0 2px.
fn file_geom(ix: usize, f: &crate::model::FileBlock, spec: &TextSpec, row_w: f32, window: &mut Window) -> EqGeom {
    let (fs, lh) = (spec.size, spec.line_height);
    let font = Font {
        family: theme::DOC_UI_FONT.into(),
        features: FontFeatures::default(),
        fallbacks: Some(FontFallbacks::from_fonts(vec!["PingFang SC".into(), "Apple Color Emoji".into()])),
        weight: spec.weight,
        style: FontStyle::Normal,
    };
    let name: String = f.name.chars().take(200).collect();
    let run = TextRun { len: name.len(), font: font.clone(), color: Theme::link(), background_color: None, underline: None, strikethrough: None };
    let line = window.text_system().shape_line(name.into(), px(fs), &[run], None);
    let s = (fs * 1.15).round();
    let text_x = 2. + s + 4.;
    let w = (text_x + f32::from(line.width) + 2.).min(row_w.max(text_x + 2.));
    EqGeom {
        ix,
        display: false,
        adv: w,
        box_h: lh,
        view: [0., 0., w, lh],
        placeholder: false,
        ext: Some([0., lh]),
        body: EqBody::File { line, font, kind: crate::editor::file::kind_of(&f.name), icon: [2., (lh - s) / 2. - 1., s], text_x },
    }
}

/// `.docx-button-component__wrapper { margin: 2px 4px }` around
/// `.docx-button-component { padding: 0 8px }`.
fn button_geom(ix: usize, b: &crate::model::InlineButton, editing: bool, spec: &TextSpec, row_w: f32, window: &mut Window) -> EqGeom {
    let (fs, bh, weight, heading) = crate::editor::button::metrics(spec.size, spec.weight);
    let lh = spec.line_height;
    let wrap_lh = if heading { bh } else { lh };
    let font = Font {
        family: theme::DOC_UI_FONT.into(),
        features: FontFeatures::default(),
        fallbacks: Some(FontFallbacks::from_fonts(vec!["PingFang SC".into(), "Apple Color Emoji".into()])),
        weight,
        style: FontStyle::Normal,
    };
    let label: String = b.text.chars().take(200).collect();
    let pal = crate::editor::button::palette(&b.color);
    let run = TextRun { len: label.len(), font: font.clone(), color: pal.text, background_color: None, underline: None, strikethrough: None };
    let line = window.text_system().shape_line(label.into(), px(fs), &[run], None);
    let w = (f32::from(line.width) + 16.).min((row_w - 8.).max(16.));
    let box_top = (lh - wrap_lh) / 2.;
    let wrap_h = bh.max(wrap_lh);
    EqGeom {
        ix,
        display: false,
        adv: w + 8.,
        box_h: wrap_h + 4.,
        view: [4., box_top, w, bh],
        placeholder: false,
        ext: Some([(box_top - 2.).min(0.), (box_top + wrap_h + 2.).max(lh)]),
        body: EqBody::Button {
            line,
            font,
            color: b.color.clone(),
            disabled: b.action == crate::model::ButtonAction::FollowPage,
            editing,
            bh,
            wrap_h,
            pad: 8.,
        },
    }
}

fn eq_geom(ix: usize, src: &str, fs: f32, row_w: f32, color: Hsla, eqno: u32, window: &mut Window) -> EqGeom {
    let display = math::is_block(src);
    let parsed = if src.trim().is_empty() { None } else { Some(math::layout(src, display, math::FONT_PX, color, eqno, window)) };
    let Some(Some(m)) = parsed else {
        // `.equation-block-placeholder` / `-error-placeholder`: 14px/20px,
        // padding 3px 8px; the error text has margin-left 4 after the icon.
        let error = parsed.is_some();
        let (label, tc) = if error { (crate::settings::text("无效公式"), Theme::text()) } else { (crate::settings::text("添加 LaTeX 公式"), Theme::text_caption()) };
        let font = Font {
            family: theme::DOC_UI_FONT.into(),
            features: FontFeatures::default(),
            fallbacks: Some(FontFallbacks::from_fonts(vec!["PingFang SC".into()])),
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
        };
        let run = TextRun { len: label.len(), font, color: tc, background_color: None, underline: None, strikethrough: None };
        let line = window.text_system().shape_line(label.into(), px(14.), &[run], None);
        let (asc, desc) = (f32::from(line.ascent), f32::from(line.descent));
        let icon_w = if error { 14. + 4. } else { 0. };
        let view_w = 2. + 8. + icon_w + f32::from(line.width) + 8. + 2.;
        let top = -13.;
        let baseline = top + 3. + (20. - asc - desc) / 2. + asc;
        return EqGeom {
            ix,
            display,
            adv: (view_w + 2.).min(row_w),
            box_h: 26.,
            view: [1., top, view_w, 26.],
            placeholder: true,
            ext: None,
            body: EqBody::Text { line, at: [1. + 2. + 8. + icon_w, baseline], icon: error.then_some([1. + 2. + 8., top + 3. + 3.]) },
        };
    };
    if display {
        // `.katex-display { margin: 1em 0 }` at the view's 15px, content padding-bottom 3.
        let tag_w = m.tag.as_ref().map_or(0., |t| t.width);
        let r = row_w - 4.;
        let a = m.width + tag_w;
        let squeeze = m.tag.is_some() && r < a + tag_w;
        let d = (r / (a.max(m.width) + 1.)).min(1.);
        let view_h = 15. + m.ascent + m.depth + 15. + 3.;
        let top = -view_h / 2.;
        if d >= 1. && !squeeze {
            let base = top + 15. + m.ascent;
            return EqGeom {
                ix,
                display,
                adv: row_w,
                box_h: view_h,
                view: [1., top, row_w - 2., view_h],
                placeholder: false,
                ext: None,
                body: EqBody::Math {
                    at: [3. + (row_w - 6. - m.width) / 2., base],
                    tag_at: m.tag.as_ref().map(|t| [row_w - 3. - t.width, base]),
                    math: m,
                },
            };
        }
        // `.overflow-numbering`: the tag follows the formula and the view
        // shrinks to fit, scaled about its left centre.
        let sm = if d < 1. { math::layout(src, true, math::FONT_PX * d, color, eqno, window).unwrap_or(m.clone()) } else { m.clone() };
        let view_w = a + 4.;
        let base = (top + 15. + m.ascent) * d;
        return EqGeom {
            ix,
            display,
            adv: row_w,
            box_h: view_h,
            view: [1., top * d, view_w * d, view_h * d],
            placeholder: false,
            ext: None,
            body: EqBody::Math { at: [1. + 2. * d, base], tag_at: sm.tag.as_ref().map(|_| [1. + 2. * d + sm.width, base]), math: sm },
        };
    }
    // Inline: `.editor-kit-equation-view { padding: 0 2px; min-width: 16px }`;
    // the katex strut is at least 16.48 / 6.59 around the baseline, plus
    // the content's 3px bottom padding.
    let asc = m.ascent.max(16.48);
    let desc = m.depth.max(6.59);
    let view_w = (m.width + 4.).max(16.);
    let view_h = asc + desc + 3.;
    let top = -view_h / 2.;
    let d = ((row_w - 4.) / (m.width + 1.)).min(1.);
    let _ = fs;
    let (math, d) = if d < 1. {
        (math::layout(src, false, math::FONT_PX * d, color, eqno, window).unwrap_or(m), d)
    } else {
        (m, 1.)
    };
    EqGeom {
        ix,
        display,
        adv: (view_w + 2.).min(row_w),
        box_h: view_h,
        view: [1., top * d, view_w * d, view_h * d],
        placeholder: false,
        ext: None,
        body: EqBody::Math { at: [1. + 2. * d, (top + asc) * d], tag_at: None, math },
    }
}

fn font_for(spec: &TextSpec, m: &Marks) -> Font {
    let family: SharedString = if spec.mono || m.code { theme::mono_font().into() } else if let Some(f) = spec.font { f.into() } else if spec.doc_font { theme::body_font().into() } else { theme::ui_family().into() };
    Font {
        family,
        features: FontFeatures::default(),
        fallbacks: Some(FontFallbacks::from_fonts(vec!["PingFang SC".into(), "Apple Color Emoji".into()])),
        weight: if m.bold { bold_weight() } else { spec.weight },
        style: if m.italic || spec.italic { FontStyle::Italic } else { FontStyle::Normal },
    }
}

/// `strong { font-weight }` (700).
fn bold_weight() -> FontWeight {
    if crate::settings::feishu() { FontWeight::BOLD } else { FontWeight(theme::style().num("strong.weight")) }
}

fn span_color(spec: &TextSpec, m: &Marks) -> Hsla {
    if m.link.is_some() {
        return Theme::link();
    }
    let st = theme::style();
    // `code`, then `strong`, then `em` (a bold italic run takes the bold colour).
    let own = if spec.mono { None } else if m.code { st.color("code.color") } else if m.bold { st.color("strong.color") } else if m.italic { st.color("em.color") } else { None };
    let c = match m.text_color.as_ref().and_then(|c| theme::text_color(&c.0)) {
        Some(c) if spec.dim_marks => c.opacity(0.7),
        Some(c) => c,
        None => own.unwrap_or(spec.color),
    };
    // Strikethrough: colour blended 35% toward the background.
    if m.strike && !crate::settings::feishu() { c.opacity(c.a * 0.65) } else { c }
}

use crate::editor_ui::{cjk_punct_font, is_cjk_punct};

/// A family the platform has: a missing one resolves to GPUI's last-resort
/// face in its regular style, so quotes in bold or italic text lost theirs (#419).
#[cfg(target_vendor = "apple")]
const QUOTE_FONT: &str = "Helvetica Neue";
#[cfg(target_os = "windows")]
const QUOTE_FONT: &str = "Segoe UI";
#[cfg(not(any(target_vendor = "apple", target_os = "windows")))]
const QUOTE_FONT: &str = crate::theme::SYSTEM_FONT;

/// Shaped in place of an equation's U+FFFC: same UTF-8 length, a glyph of
/// the text font (so the line's ascent is unchanged).
const EQ_STAND_IN: &str = "\u{2013}";

pub struct PlacedEq {
    /// Box left edge, vertical centre.
    origin: Point<Pixels>,
    view: Bounds<Pixels>,
    hitbox: Hitbox,
    geom: Rc<EqGeom>,
}

/// What prepaint resolved: equations and the field's horizontal scroll.
pub struct Prepaint {
    placed: Vec<PlacedEq>,
    sx: Pixels,
}

thread_local! {
    /// The equation under a pressed mouse button (`:active`).
    static EQ_PRESSED: std::cell::Cell<Option<(BlockId, usize)>> = const { std::cell::Cell::new(None) };
}

fn is_quote(ch: char) -> bool {
    matches!(ch, '\u{2018}' | '\u{2019}' | '\u{201C}' | '\u{201D}')
}

/// #419: the curly quotes that belong to Chinese text, by byte offset: those
/// right next to a Han character or Chinese punctuation, and the other one of
/// their pair (`用“Quiet.md”打开`: both). They take the Chinese face, the rest
/// stay Latin. A quote with no pair (an apostrophe) goes by its own neighbours.
fn cjk_quotes(text: &str) -> Vec<usize> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let chinese = |j: Option<usize>| j.and_then(|j| chars.get(j)).is_some_and(|&(_, ch)| is_cjk(ch) || is_cjk_punct(ch));
    let mut on: Vec<bool> = (0..chars.len()).map(|i| is_quote(chars[i].1) && (chinese(i.checked_sub(1)) || chinese(Some(i + 1)))).collect();
    let (mut double, mut single) = (vec![], vec![]);
    for (i, &(_, ch)) in chars.iter().enumerate() {
        let open = match ch {
            '\u{201C}' => { double.push(i); continue }
            '\u{2018}' => { single.push(i); continue }
            '\u{201D}' => double.pop(),
            '\u{2019}' => single.pop(),
            _ => continue,
        };
        if let Some(o) = open {
            let both = on[o] || on[i];
            (on[o], on[i]) = (both, both);
        }
    }
    chars.iter().zip(on).filter(|(_, on)| *on).map(|(&(at, _), _)| at).collect()
}

pub fn build_runs(
    text: &RichText,
    spec: &TextSpec,
    marked: Option<&Range<usize>>,
    highlight: Option<&Vec<crate::highlight::Span>>,
) -> Vec<TextRun> {
    let mut runs = vec![];
    let chinese_quotes = if text.text.chars().any(is_quote) { cjk_quotes(&text.text) } else { vec![] };
    for (range, m) in text.spans() {
        // Split by the marked (IME) range and syntax highlight boundaries.
        let mut cuts = vec![range.start, range.end];
        if let Some(mr) = marked {
            cuts.push(mr.start.clamp(range.start, range.end));
            cuts.push(mr.end.clamp(range.start, range.end));
        }
        if let Some(h) = highlight {
            for (r, _, _) in h {
                cuts.push(r.start.clamp(range.start, range.end));
                cuts.push(r.end.clamp(range.start, range.end));
            }
        }
        // Curly quotes render in a Latin-width face (QUOTE_FONT) rather
        // than the body face's full-width ones, but in Chinese text in the
        // Chinese one (`cjk_quotes`).
        for (i, ch) in text.text[range.clone()].char_indices() {
            if is_quote(ch) || is_cjk_punct(ch) {
                cuts.push(range.start + i);
                cuts.push(range.start + i + ch.len_utf8());
            }
        }
        cuts.sort();
        cuts.dedup();
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a >= b {
                continue;
            }
            let mut color = span_color(spec, m);
            let mut hl_italic = false;
            if let Some(h) = highlight
                && let Some((_, hc, it)) = h.iter().find(|(r, _, _)| r.start <= a && b <= r.end)
            {
                color = *hc;
                hl_italic = *it;
            }
            let in_marked = marked.is_some_and(|mr| mr.start <= a && b <= mr.end && mr.start < mr.end);
            // `a { text-decoration: underline }`: links stay underlined, not
            // only while being edited.
            let link_line = m.link.is_some() && !m.underline && !in_marked && !crate::settings::feishu() && theme::style().flag("a.underline");
            let underline = if link_line {
                Some(UnderlineStyle { thickness: px(1.), color: Some(theme::style().color("a.underline-color").unwrap_or(color)), wavy: false })
            } else if in_marked || m.underline || (spec.link_underline && m.link.is_some()) {
                Some(UnderlineStyle { thickness: px(1.), color: Some(color), wavy: false })
            } else {
                None
            };
            let mut font = font_for(spec, m);
            if !spec.mono && !m.code {
                // A family the reader picked keeps the ones it draws itself.
                let own = crate::fonts::own_glyphs(&font.family);
                if text.text[a..b].chars().all(is_quote) {
                    // Cut one by one, so `a` is the quote.
                    // In Chinese text the Chinese face's; without one (not
                    // installed here), as in Latin text.
                    match (chinese_quotes.binary_search(&a).is_ok(), own.cjk_punct, cjk_punct_font()) {
                        (true, true, _) => {}
                        (true, false, Some(f)) => font.family = f.into(),
                        _ if !own.quotes => font.family = QUOTE_FONT.into(),
                        _ => {}
                    }
                } else if !own.cjk_punct && text.text[a..b].chars().all(is_cjk_punct) && let Some(f) = cjk_punct_font() {
                    font.family = f.into();
                }
            }
            if hl_italic {
                font.style = FontStyle::Italic;
            }
            if m.code && !spec.mono {
                font.weight = if m.bold { bold_weight() } else { FontWeight::NORMAL };
            }
            let background_color = m.bg_color.as_ref().and_then(|c| theme::bg_color(&c.0)).map(|c| if spec.dim_marks { c.opacity(0.7) } else { c });
            let fonts = match &spec.font_runs { Some(f) => f(&text.text[a..b]), None => vec![(b - a, font)] };
            for (len, font) in fonts {
                runs.push(TextRun {
                    len,
                    font,
                    color: if background_color.is_some() { crate::theme::on_mark(color) } else { color },
                    background_color,
                    underline,
                    // Painted by `RichTextElement::paint`, see `paint_strikes`.
                    strikethrough: None,
                });
            }
        }
    }
    runs
}

impl Element for RichTextElement {
    type RequestLayoutState = Rc<RefCell<Option<Prepared>>>;
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        _cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let state: Rc<RefCell<Option<Prepared>>> = Rc::new(RefCell::new(None));
        let font_size = px(self.spec.size);
        let line_height = px(self.spec.line_height);
        // Lines' ascent and descent when fixed: Linux fallback fonts change during IME
        // composition, so the title keeps its font's; board text takes the board's (`TextSpec::metrics`).
        let metrics = if cfg!(target_os = "linux") && self.id == crate::editor::core::TITLE {
            let ts = window.text_system();
            let id = ts.resolve_font(&font_for(&self.spec, &Marks::default()));
            // GPUI Linux font metrics use a signed descent; shaped line layouts use a positive distance.
            Some((ts.ascent(id, font_size), px(f32::from(ts.descent(id, font_size)).abs())))
        } else { self.spec.metrics.map(|(a, d)| (px(a), px(d))) };
        let mut eq_src: Vec<(usize, ObjSrc)> = vec![];
        let (text, runs) = if self.text.is_empty() {
            match &self.placeholder {
                Some(ph) => {
                    let mut spec = self.spec.clone();
                    spec.color = self.placeholder_color.unwrap_or(Theme::block_placeholder());
                    let rt = RichText::plain(ph.to_string());
                    let runs = build_runs(&rt, &spec, None, None);
                    (ph.clone(), runs)
                }
                None => (SharedString::default(), vec![]),
            }
        } else {
            let hl = self.highlight.as_ref().map(|f| f(&self.text.text));
            let mut runs = build_runs(&self.text, &self.spec, self.marked.as_ref(), hl.as_ref());
            let mut text = self.text.text.clone();
            for (r, m) in self.text.spans() {
                let Some(obj) = m.inline.as_ref() else { continue };
                if let Some(src) = obj.equation() {
                    for (i, _) in self.text.text[r.clone()].char_indices() {
                        eq_src.push((r.start + i, ObjSrc::Eq(src.to_string())));
                    }
                } else if let Some(f) = obj.file() {
                    for (i, _) in self.text.text[r.clone()].char_indices() {
                        eq_src.push((r.start + i, ObjSrc::File(f.clone())));
                    }
                } else if let Some(l) = obj.doc_link() {
                    for (i, _) in self.text.text[r.clone()].char_indices() {
                        eq_src.push((r.start + i, ObjSrc::Doc(l.clone())));
                    }
                } else if let Some(&rm) = obj.reminder() {
                    for (i, _) in self.text.text[r.clone()].char_indices() {
                        eq_src.push((r.start + i, ObjSrc::Rem(rm)));
                    }
                } else if let Some(b) = obj.button() {
                    for (i, _) in self.text.text[r.clone()].char_indices() {
                        let ix = r.start + i;
                        match &self.button_edit {
                            Some((e, pb)) if *e == ix => eq_src.push((ix, ObjSrc::Btn(pb.clone(), true))),
                            _ => eq_src.push((ix, ObjSrc::Btn(b.clone(), false))),
                        }
                    }
                }
            }
            if !eq_src.is_empty() {
                // Shape each equation as an invisible stand-in of the same
                // UTF-8 length, widened to the equation's box afterwards.
                for (ix, _) in &eq_src {
                    text.replace_range(*ix..*ix + 3, EQ_STAND_IN);
                }
                let mut off = 0;
                for run in &mut runs {
                    if eq_src.iter().any(|(ix, _)| *ix >= off && *ix < off + run.len) {
                        run.color = transparent_black();
                        run.background_color = None;
                        run.underline = None;
                    }
                    off += run.len;
                }
            }
            (SharedString::from(text), runs)
        };
        let wrap = self.spec.wrap;
        let spacing = px(self.spec.letter_spacing);
        let fs = self.spec.size;
        let color = self.spec.color;
        let eq_counter = self.eq_counter;
        let spec = self.spec.clone();
        let breaks = self.spec.breaks.clone();
        let mut style = Style::default();
        let fit = self.fit_content && !wrap;
        if fit { style.min_size.width = relative(1.).into(); style.flex_shrink = 0.; } else { style.size.width = relative(1.).into() }
        let st = state.clone();
        let layout_id = window.request_measured_layout(style, move |known, available, window, _cx| {
            let wrap_width = if wrap {
                known.width.or(match available.width {
                    AvailableSpace::Definite(x) => Some(x),
                    _ => None,
                })
            } else {
                None
            };
            if let Some(p) = st.borrow().as_ref()
                && p.wrap_width == wrap_width
            {
                return size(known.width.unwrap_or(if fit { p.width } else { px(0.) }), p.height);
            }
            let custom = spacing != px(0.) || !eq_src.is_empty() || breaks.is_some();
            let mut lines = window
                .text_system()
                .shape_text(text.clone(), font_size, &runs, if custom { None } else { wrap_width }, None)
                .unwrap_or_default();
            let row_w = wrap_width.or(known.width).map_or(f32::INFINITY, f32::from);
            let mut eqno = eq_counter;
            let eqs: Vec<Rc<EqGeom>> = eq_src
                .iter()
                .map(|(ix, src)| match src {
                    ObjSrc::Eq(src) => {
                        let g = Rc::new(eq_geom(*ix, src, fs, row_w, color, eqno, window));
                        eqno += math::numbers_in(src);
                        g
                    }
                    ObjSrc::Btn(b, editing) => Rc::new(button_geom(*ix, b, *editing, &spec, row_w, window)),
                    ObjSrc::File(f) => Rc::new(file_geom(*ix, f, &spec, row_w, window)),
                    ObjSrc::Rem(r) => Rc::new(reminder_geom(*ix, r, &spec, window)),
                    ObjSrc::Doc(l) => Rc::new(doc_geom(*ix, l, &spec, row_w, window)),
                })
                .collect();
            let mut start = 0;
            let mut out = vec![];
            let mut rows = vec![];
            let mut h = px(0.);
            let mut w = px(0.);
            for mut l in lines.drain(..) {
                let len = l.len();
                let line_eqs: Vec<&EqGeom> = eqs.iter().map(|e| &**e).filter(|e| e.ix >= start && e.ix < start + len).collect();
                if let Some(b) = &breaks {
                    let rows = b(&l.text);
                    break_line(&mut l, &rows);
                } else if custom {
                    adjust_line(&mut l, spacing, &line_eqs, start, wrap_width);
                }
                if let Some((ascent, descent)) = metrics {
                    let src = &l.unwrapped_layout;
                    let layout = LineLayout { font_size: src.font_size, width: src.width, ascent, descent, runs: src.runs.clone(), len: src.len };
                    let wrapped = WrappedLineLayout { unwrapped_layout: std::sync::Arc::new(layout), wrap_boundaries: l.wrap_boundaries().into(), wrap_width: l.wrap_width };
                    let target: &mut std::sync::Arc<WrappedLineLayout> = &mut l;
                    *target = std::sync::Arc::new(wrapped);
                }
                // Row heights: the line box grows to contain equation boxes.
                let layout = &l.unwrapped_layout;
                let base = (line_height - layout.ascent - layout.descent) / 2. + layout.ascent;
                let mut cuts: Vec<usize> = row_starts(&l).into_iter().map(|r| r.0).collect();
                cuts.push(len + 1);
                for c in cuts.windows(2) {
                    let (mut top, mut bottom) = (px(0.), line_height);
                    for e in line_eqs.iter().filter(|e| e.ix - start >= c[0] && e.ix - start < c[1]) {
                        if let Some([t, b]) = e.ext {
                            top = top.min(px(t));
                            bottom = bottom.max(px(b));
                            continue;
                        }
                        let mid = base - px(EQ_MIDDLE * fs);
                        top = top.min(mid - px(e.box_h / 2.));
                        bottom = bottom.max(mid + px(e.box_h / 2.));
                    }
                    rows.push((bottom - top, -top));
                    h += bottom - top;
                }
                w = w.max(l.width());
                out.push((start, l));
                start += len + 1;
            }
            if out.is_empty() {
                h = line_height;
            }
            *st.borrow_mut() = Some(Prepared { lines: out, wrap_width, line_height, rows, height: h, width: w, eqs });
            size(known.width.unwrap_or(w), h)
        });
        (layout_id, state)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let bounds = self.spec.shifted(bounds);
        let Some(p) = state.borrow_mut().take() else { return Prepaint { placed: vec![], sx: px(0.) } };
        // Single-line fields scroll horizontally; wrapped ones never do.
        let sx = match &self.scroll {
            Some(cell) if !self.spec.wrap && p.lines.len() == 1 => {
                let (start, line) = &p.lines[0];
                let i = self.caret.unwrap_or(0).saturating_sub(*start);
                let l = &line.unwrapped_layout;
                let rtl = if bidi::has_rtl(&line.text) { bidi::x_for_index(l, &line.text, px(0.)..l.width, i) } else { None };
                let caret_x = rtl.unwrap_or_else(|| l.x_for_index(i));
                let s = hscroll(line.unwrapped_layout.width, caret_x, bounds.size.width, cell.get());
                cell.set(s);
                s
            }
            Some(cell) => {
                cell.set(px(0.));
                px(0.)
            }
            None => px(0.),
        };
        // Text coordinates start `sx` to the left of the visible box; the
        // cached layout then answers hit-testing and IME in window coords.
        let text_origin = point(bounds.left() - sx, bounds.top());
        let lh = p.line_height;
        let mut rows = vec![];
        let mut y = px(0.);
        let showing_placeholder = self.text.is_empty();
        let mut heights = p.rows.iter();
        for (hi, (start, line)) in p.lines.iter().enumerate() {
            let layout = &line.unwrapped_layout;
            let mut bounds_ix = row_starts(line);
            bounds_ix.push((line.len(), layout.width));
            // A line that reads right to left keeps the default (left)
            // alignment against the right edge, as the platform text views do.
            let align = if self.align == Align::Left && bidi::rtl_line(&line.text) { Align::Right } else { self.align };
            for w in bounds_ix.windows(2) {
                let (a, xa) = w[0];
                let (b, xb) = w[1];
                let width = xb - xa;
                let align_dx = match align {
                    Align::Left => px(0.),
                    Align::Center => ((bounds.size.width - width) / 2.).max(px(0.)),
                    Align::Right => (bounds.size.width - width).max(px(0.)),
                };
                let (ra, rb) = if showing_placeholder { (0, 0) } else { (start + a, start + b) };
                let (h, base_dy) = heights.next().copied().unwrap_or((lh, px(0.)));
                rows.push(Row { range: ra..rb, hard: hi, y, h, base_dy, x0: xa, x1: xb, align_dx });
                y += h;
            }
        }
        let mut layout = TextLayout { bounds: Bounds::new(text_origin, bounds.size), lines: p.lines, rows, line_height: lh, mono: self.spec.mono, eqs: vec![], buttons: vec![] };
        let mut placed = vec![];
        for e in p.eqs {
            let ri = layout.row_for(e.ix);
            let Some(r) = layout.rows.get(ri) else { continue };
            let l = &layout.lines[r.hard].1.unwrapped_layout;
            let x = layout.x_in_row(r, e.ix) - r.x0 + r.align_dx;
            let base = (lh - l.ascent - l.descent) / 2. + l.ascent;
            let line_top = bounds.top() + r.y + r.base_dy;
            let origin = point(text_origin.x + x, if e.ext.is_some() { line_top } else { line_top + base - px(EQ_MIDDLE * self.spec.size) });
            let [vx, vy, vw, vh] = e.view;
            let view = Bounds::new(point(origin.x + px(vx), origin.y + px(vy)), size(px(vw), px(vh)));
            if e.ext.is_some() {
                layout.buttons.push((e.ix, view));
            } else {
                layout.eqs.push((e.ix, view));
            }
            let hitbox = window.insert_hitbox(view, HitboxBehavior::Normal);
            placed.push(PlacedEq { origin, view, hitbox, geom: e });
        }
        self.cache.borrow_mut().insert(self.id, layout);
        Prepaint { placed, sx }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _state: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let bounds = self.spec.shifted(bounds);
        let cache = self.cache.borrow();
        let Some(layout) = cache.get(&self.id) else { return };
        let lh = layout.line_height;
        // Per hard line: one that reads right to left sits against the right
        // edge (see `prepaint`).
        let gpui_align = |line: &WrappedLine| match self.align {
            Align::Left if bidi::rtl_line(&line.text) => TextAlign::Right,
            Align::Left => TextAlign::Left,
            Align::Center => TextAlign::Center,
            Align::Right => TextAlign::Right,
        };
        // Inline code chips behind the text: a grey chip with a 1pt rim,
        // radius 3, 4.5pt beyond the glyphs.
        if !self.text.is_empty() && !self.spec.mono {
            for (range, m) in self.text.spans() {
                if !m.code {
                    continue;
                }
                for r in layout.rects_for(range) {
                    let inset = if crate::settings::feishu() { px(3.) } else { ((r.size.height - px(self.spec.size * 1.32)) / 2.).max(px(0.)) };
                    let (pad, radius) = (theme::style().num("code.pad"), theme::style().num("code.radius"));
                    let r = Bounds::from_corners(point(r.left() - px(pad), r.top() + inset), point(r.right() + px(pad), r.bottom() - inset));
                    window.paint_quad(
                        quad(r, px(radius), if crate::settings::feishu() { Theme::bg_overlay() } else { Theme::code_chip() }, px(1.), if crate::settings::feishu() { Theme::line_border() } else { Theme::code_chip_border() }, BorderStyle::Solid),
                    );
                }
            }
        }
        // Text backgrounds sit below selection; glyphs are painted above both.
        let dx = if self.text.is_empty() { px(self.placeholder_dx) } else { px(0.) };
        let sx = prepaint.sx;
        let o = point(bounds.left() + dx - sx, bounds.top());
        for (hi, (_, line)) in layout.lines.iter().enumerate() {
            let tops: Vec<Pixels> = layout.rows.iter().filter(|r| r.hard == hi).map(|r| r.y + r.base_dy).collect();
            let _ = line.paint_background_rows(o, lh, &tops, gpui_align(line), Some(bounds), window, cx);
        }
        if let Some(sel) = &self.selection {
            let rects = if self.text.is_empty() {
                vec![Bounds::new(bounds.origin, size(px(5.), lh))]
            } else {
                // Equations show their own selected background instead
                // (`.equation-leaf ::selection` is transparent).
                let mut parts = vec![];
                let mut a = sel.start;
                let mut objs: Vec<usize> = layout.eqs.iter().chain(&layout.buttons).map(|(ix, _)| *ix).collect();
                objs.sort();
                for ix in objs.iter().filter(|ix| sel.start <= **ix && **ix < sel.end) {
                    if a < *ix {
                        parts.push(a..*ix);
                    }
                    a = ix + 3;
                }
                if a < sel.end || parts.is_empty() && a == sel.start {
                    parts.push(a..sel.end);
                }
                parts.into_iter().flat_map(|r| layout.rects_for(r)).collect()
            };
            for r in rects {
                window.paint_quad(fill(r, Theme::selection()));
            }
        }
        for eq in prepaint.placed.iter().filter(|e| e.geom.ext.is_none()) {
            let ix = eq.geom.ix;
            let selected = self.selection.as_ref().is_some_and(|s| s.start <= ix && ix + 3 <= s.end);
            let pressed = EQ_PRESSED.get() == Some((self.id, ix));
            let hovered = eq.hitbox.is_hovered(window);
            let ph = eq.geom.placeholder;
            let bg = if selected {
                Some(Theme::block_selected())
            } else if pressed {
                Some(if ph { theme::c(craft::N400) } else { Theme::fill_pressed() })
            } else if hovered {
                Some(if ph { theme::c(craft::N350) } else { Theme::fill_hover_strong() })
            } else if ph {
                Some(theme::c(craft::N100))
            } else {
                None
            };
            if let Some(bg) = bg {
                window.paint_quad(fill(eq.view, bg).corner_radii(px(4.)));
            }
        }
        for (hi, (_, line)) in layout.lines.iter().enumerate() {
            let tops: Vec<Pixels> = layout.rows.iter().filter(|r| r.hard == hi).map(|r| r.y + r.base_dy).collect();
            let _ = line.paint_rows(o, lh, &tops, gpui_align(line), Some(bounds), window, cx);
        }
        for eq in prepaint.placed.iter() {
            let o = eq.origin;
            match &eq.geom.body {
                EqBody::Math { math, at, tag_at } => {
                    math.paint(point(o.x + px(at[0]), o.y + px(at[1])), window, cx);
                    if let (Some(tag), Some(t)) = (&math.tag, tag_at) {
                        tag.paint(point(o.x + px(t[0]), o.y + px(t[1])), window, cx);
                    }
                }
                EqBody::Button { .. } => self.paint_button(eq, window, cx),
                EqBody::File { .. } => self.paint_file(eq, window, cx),
                EqBody::Doc { .. } => self.paint_doc(eq, window, cx),
                EqBody::Reminder { .. } => self.paint_reminder(eq, window, cx),
                EqBody::Text { line, at, icon } => {
                    let top = o.y + px(at[1]) - line.ascent;
                    let _ = line.paint(point(o.x + px(at[0]), top), line.ascent + line.descent, TextAlign::Left, None, window, cx);
                    if let Some(i) = icon {
                        let b = Bounds::new(point(o.x + px(i[0]), o.y + px(i[1])), size(px(14.), px(14.)));
                        let _ = window.paint_svg(
                            b,
                            "editor-icons/equation-error.svg".into(),
                            None,
                            TransformationMatrix::unit(),
                            theme::c(0xff9200),
                            cx,
                        );
                    }
                }
            }
        }
        self.listen_equations(&prepaint.placed, window);
        if !self.text.is_empty() {
            self.paint_strikes(layout, window);
        }
        // Find marks sit over the text box of each hit.
        let box_h = (px(self.spec.size * 1.4)).min(lh);
        for (range, current) in &self.found {
            // Find hits in marker yellow; the current one in orange.
            let color = if *current { theme::ca(0xff9200, 0.45) } else { theme::ca(0xfff100, 0.35) };
            for r in layout.rects_for(range.clone()) {
                let top = r.top() + (lh - box_h) / 2.;
                window.paint_quad(fill(Bounds::new(point(r.left(), top), size(r.size.width, box_h)), color));
            }
        }
        if self.show_caret
            && let Some(c) = self.caret
        {
            let p = if self.text.is_empty() {
                let dx = match self.align {
                    Align::Left => px(0.),
                    Align::Center => bounds.size.width / 2.,
                    Align::Right => bounds.size.width - px(2.),
                };
                point(bounds.left() + dx, bounds.top())
            } else if self.caret_row_end {
                layout.position_at_row_end(c)
            } else {
                layout.position_for(c)
            };
            // The caret: the accent, 2pt wide, the font's ascent + descent tall.
            let font_h = px(self.spec.size * 1.18);
            let top = p.y + (lh - font_h) / 2.;
            window.paint_quad(fill(Bounds::new(point(p.x, top), size(px(2.), font_h)), Theme::caret()).corner_radii(px(1.)));
        }
    }
}

impl RichTextElement {
    fn paint_button(&self, eq: &PlacedEq, window: &mut Window, cx: &mut App) {
        let EqBody::Button { line, font, color, disabled, editing, bh, wrap_h, pad } = &eq.geom.body else { return };
        let ix = eq.geom.ix;
        let v = eq.view;
        if self.selection.as_ref().is_some_and(|s| s.start <= ix && ix + 3 <= s.end) {
            // `.docx-button-component__wrapper--selected`, behind the button.
            window.paint_quad(fill(Bounds::new(v.origin, size(v.size.width, px(*wrap_h))), Theme::selection()));
        }
        let pal = crate::editor::button::palette(color);
        let pressed = EQ_PRESSED.get() == Some((self.id, ix));
        let hovered = eq.hitbox.is_hovered(window);
        let (bg, tc) = if *disabled {
            (if color.starts_with("deep") { pal.disabled_bg } else { pal.bg }, Theme::text_disabled())
        } else if pressed {
            (pal.active_bg, pal.active_text)
        } else if hovered {
            (pal.hover_bg, pal.hover_text)
        } else {
            (pal.bg, pal.text)
        };
        let lifted = !*disabled && (pressed || hovered || *editing);
        let shadows = if lifted { crate::editor::button::shadow_s4() } else { crate::editor::button::shadow_s1() };
        // Content stays crisp: buttons in the text round at 5.
        let radius = px(5.);
        window.paint_drop_shadows(v, Corners::all(radius), &shadows);
        window.paint_quad(fill(v, bg).corner_radii(radius));
        // A done to-do: `.todo-line-through .docx-button-component` draws
        // a 1.6px `--icon-n2` line at 55% behind the label; `opacity: .7`.
        let dim = if self.spec.strike_all { 0.7 } else { 1. };
        if self.spec.strike_all {
            let y = v.top() + (v.size.height - px(1.6)) * 0.55;
            window.paint_quad(fill(Bounds::new(point(v.left(), y), size(v.size.width, px(1.6))), theme::c(craft::N600)));
        }
        let run = TextRun { len: line.text.len(), font: font.clone(), color: tc.opacity(dim), background_color: None, underline: None, strikethrough: None };
        let line = window.text_system().shape_line(line.text.clone(), line.font_size, &[run], None);
        let clip = Bounds::new(point(v.left() + px(*pad), v.top()), size((v.size.width - px(pad * 2.)).max(px(0.)), v.size.height));
        window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
            let _ = line.paint(point(v.left() + px(*pad), v.top()), px(*bh), TextAlign::Left, None, window, cx);
        });
    }

    fn paint_file(&self, eq: &PlacedEq, window: &mut Window, cx: &mut App) {
        let EqBody::File { line, font, kind, icon, text_x } = &eq.geom.body else { return };
        let ix = eq.geom.ix;
        let v = eq.view;
        if self.selection.as_ref().is_some_and(|s| s.start <= ix && ix + 3 <= s.end) {
            window.paint_quad(fill(v, Theme::selection()));
        }
        let o = eq.origin;
        let ib = Bounds::new(point(o.x + px(icon[0]), o.y + px(icon[1])), size(px(icon[2]), px(icon[2])));
        crate::editor::file::paint_type_icon(*kind, ib, window, cx);
        let color = if eq.hitbox.is_hovered(window) { Theme::link_hover() } else { Theme::link() };
        let run = TextRun { len: line.text.len(), font: font.clone(), color, background_color: None, underline: None, strikethrough: None };
        let line = window.text_system().shape_line(line.text.clone(), line.font_size, &[run], None);
        let clip = Bounds::new(point(o.x + px(*text_x), v.top()), size((v.size.width - px(text_x + 2.)).max(px(0.)), v.size.height));
        window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
            let _ = line.paint(point(o.x + px(*text_x), v.top()), v.size.height, TextAlign::Left, None, window, cx);
        });
    }

    fn paint_doc(&self, eq: &PlacedEq, window: &mut Window, cx: &mut App) {
        let EqBody::Doc { line, font, emoji, deleted, icon, text_x } = &eq.geom.body else { return };
        let ix = eq.geom.ix;
        let v = eq.view;
        let hovered = !deleted && eq.hitbox.is_hovered(window);
        if self.selection.as_ref().is_some_and(|s| s.start <= ix && ix + 3 <= s.end) {
            window.paint_quad(fill(v, Theme::selection()));
        } else if hovered {
            let a = if EQ_PRESSED.get() == Some((self.id, ix)) { 0.2 } else { 0.1 };
            window.paint_quad(fill(v, Theme::primary_500().opacity(a)).corner_radii(px(4.)));
        }
        let o = eq.origin;
        let ib = Bounds::new(point(o.x + px(icon[0]), o.y + px(icon[1])), size(px(icon[2]), px(icon[2])));
        match emoji {
            Some(e) => {
                let _ = e.paint(point(ib.origin.x, v.top()), v.size.height, TextAlign::Left, None, window, cx);
            }
            None => {
                let c = if *deleted { Theme::text_disabled() } else { Theme::primary_500() };
                let _ = window.paint_svg(ib, "editor-icons/doc-blue.svg".into(), None, TransformationMatrix::unit(), c, cx);
                let _ = window.paint_svg(ib, "editor-icons/doc-blue-lines.svg".into(), None, TransformationMatrix::unit(), Theme::bg_body(), cx);
            }
        }
        let color = if *deleted { Theme::text_disabled() } else if hovered { Theme::link_hover() } else { Theme::link() };
        let run = TextRun { len: line.text.len(), font: font.clone(), color, background_color: None, underline: None, strikethrough: None };
        let line = window.text_system().shape_line(line.text.clone(), line.font_size, &[run], None);
        let clip = Bounds::new(point(o.x + px(*text_x), v.top()), size((v.size.width - px(text_x + 2.)).max(px(0.)), v.size.height));
        window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
            let _ = line.paint(point(o.x + px(*text_x), v.top()), v.size.height, TextAlign::Left, None, window, cx);
        });
    }

    fn paint_reminder(&self, eq: &PlacedEq, window: &mut Window, cx: &mut App) {
        let EqBody::Reminder { line, icon, icon_color, filled, text_x } = &eq.geom.body else { return };
        let ix = eq.geom.ix;
        let v = eq.view;
        if self.selection.as_ref().is_some_and(|s| s.start <= ix && ix + 3 <= s.end) {
            window.paint_quad(fill(v, Theme::selection()));
        }
        let o = eq.origin;
        let ib = Bounds::new(point(o.x + px(icon[0]), o.y + px(icon[1])), size(px(icon[2]), px(icon[2])));
        let path = if *filled { "editor-icons/reminder-filled.svg" } else { "editor-icons/reminder.svg" };
        let _ = window.paint_svg(ib, path.into(), None, TransformationMatrix::unit(), *icon_color, cx);
        let lh = px(self.spec.line_height);
        let _ = line.paint(point(o.x + px(*text_x), o.y), lh, TextAlign::Left, None, window, cx);
    }

    /// Pointer cursor, hover and `:active` repaints for equation views.
    fn listen_equations(&self, placed: &[PlacedEq], window: &mut Window) {
        if placed.is_empty() {
            return;
        }
        let id = self.id;
        let hits: Vec<(Hitbox, usize, bool)> =
            placed.iter().map(|e| (e.hitbox.clone(), e.geom.ix, e.hitbox.is_hovered(window))).collect();
        for (h, _, _) in &hits {
            window.set_cursor_style(CursorStyle::PointingHand, h);
        }
        let hs = hits.clone();
        window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, _| {
            if phase == DispatchPhase::Bubble && hs.iter().any(|(h, _, was)| h.is_hovered(window) != *was) {
                window.refresh();
            }
        });
        let hs = hits.clone();
        window.on_mouse_event(move |ev: &MouseDownEvent, phase, window, _| {
            if phase == DispatchPhase::Capture
                && ev.button == MouseButton::Left
                && let Some((_, ix, _)) = hs.iter().find(|(h, _, _)| h.is_hovered(window))
            {
                EQ_PRESSED.set(Some((id, *ix)));
                window.refresh();
            }
        });
        window.on_mouse_event(move |_: &MouseUpEvent, phase, window, _| {
            if phase == DispatchPhase::Capture && EQ_PRESSED.get().is_some_and(|(b, _)| b == id) {
                EQ_PRESSED.set(None);
                window.refresh();
            }
        });
    }

    /// GPUI's own strikethrough sits at ~45% of the ascent, right on the
    /// middle stroke of most CJK glyphs, and drops glyphs after a font
    /// fallback switch. Draw it like the browser does instead: through the
    /// middle of the glyph box (done to-dos: 1.6px at 56%, per `.todo-line-through`).
    fn paint_strikes(&self, layout: &TextLayout, window: &mut Window) {
        let font_h = self.spec.size * 1.3;
        let (thickness, at) = if self.spec.strike_all { (1.6, 0.56) } else { (1., 0.5) };
        for (range, m) in self.text.spans() {
            if !(m.strike || self.spec.strike_all) || m.inline.is_some() {
                continue;
            }
            let color = span_color(&self.spec, m);
            for r in layout.rects_for(range) {
                let top = r.top() + (r.size.height - px(font_h)) / 2. + px(font_h * at - thickness / 2.);
                window.paint_quad(fill(Bounds::new(point(r.left(), top), size(r.size.width, px(thickness))), color));
            }
        }
    }
}

/// GPUI has no letter-spacing: shift the glyphs of an unwrapped line the way
/// CSS does (the spacing goes after every character), widen each equation's
/// stand-in glyph to the equation's advance, and redo the wrapping on the
/// adjusted positions.
fn adjust_line(line: &mut WrappedLine, ls: Pixels, eqs: &[&EqGeom], start: usize, wrap_width: Option<Pixels>) {
    let text = line.text.clone();
    let src = line.unwrapped_layout.clone();
    let x_at = |ix: usize| {
        src.runs.iter().flat_map(|r| r.glyphs.iter()).find(|g| g.index >= ix).map_or(src.width, |g| g.position.x)
    };
    // Extra width added after each byte offset.
    let mut shift = vec![px(0.); text.len() + 1];
    let mut n = 0;
    let mut acc = px(0.);
    for (i, ch) in text.char_indices() {
        shift[i] = acc;
        n += 1;
        acc += ls;
        if let Some(e) = eqs.iter().find(|e| e.ix - start == i) {
            acc += px(e.adv) - (x_at(i + ch.len_utf8()) - x_at(i));
        }
    }
    shift[text.len()] = acc;
    let _ = n;
    let runs = src
        .runs
        .iter()
        .map(|r| ShapedRun {
            font_id: r.font_id,
            glyphs: r
                .glyphs
                .iter()
                .map(|g| {
                    let mut g = g.clone();
                    g.position.x += shift[g.index.min(text.len())];
                    g
                })
                .collect(),
        })
        .collect();
    let layout = LineLayout {
        font_size: src.font_size,
        width: src.width + acc,
        ascent: src.ascent,
        descent: src.descent,
        runs,
        len: src.len,
    };
    let marks: Vec<(usize, bool)> = eqs.iter().map(|e| (e.ix - start, e.display)).collect();
    let wraps = wrap_width.map(|w| wrap_spaced(&text, &layout, w, &marks)).unwrap_or_default();
    let wrapped: &mut std::sync::Arc<WrappedLineLayout> = line;
    *wrapped = std::sync::Arc::new(WrappedLineLayout {
        unwrapped_layout: std::sync::Arc::new(layout),
        wrap_boundaries: wraps.into(),
        wrap_width,
    });
}

/// Cuts a line into `rows` (`break_rows`).
fn break_line(line: &mut WrappedLine, rows: &[Range<usize>]) {
    let (layout, wraps) = break_rows(&line.unwrapped_layout, rows);
    let wrapped: &mut std::sync::Arc<WrappedLineLayout> = line;
    *wrapped = std::sync::Arc::new(WrappedLineLayout { unwrapped_layout: std::sync::Arc::new(layout), wrap_boundaries: wraps.into(), wrap_width: None });
}

/// A line cut into `rows`: wrap boundaries where each starts, and what lies
/// between two rows moved to the end of the first with no width (as the
/// board drops it), so rows align as the board draws them.
fn break_rows(src: &LineLayout, rows: &[Range<usize>]) -> (LineLayout, Vec<WrapBoundary>) {
    let x_at = |ix: usize| src.runs.iter().flat_map(|r| r.glyphs.iter()).find(|g| g.index >= ix).map_or(src.width, |g| g.position.x);
    // (gap, width dropped before it)
    let mut gaps = vec![];
    let mut dropped = px(0.);
    for w in rows.windows(2) {
        let gap = w[0].end..w[1].start;
        gaps.push((gap.clone(), dropped));
        dropped += x_at(gap.end) - x_at(gap.start);
    }
    let moved = |ix: usize, x: Pixels| match gaps.iter().rev().find(|(g, _)| g.start <= ix) {
        Some((g, d)) if ix < g.end => x_at(g.start) - *d,
        Some((g, d)) => x - *d - (x_at(g.end) - x_at(g.start)),
        None => x,
    };
    let runs: Vec<ShapedRun> = src
        .runs
        .iter()
        .map(|r| ShapedRun { font_id: r.font_id, glyphs: r.glyphs.iter().map(|g| { let mut g = g.clone(); g.position.x = moved(g.index, g.position.x); g }).collect() })
        .collect();
    let mut wraps = vec![];
    for r in rows.iter().skip(1) {
        let at = runs.iter().enumerate().flat_map(|(ri, run)| run.glyphs.iter().enumerate().map(move |(gi, g)| (ri, gi, g.index))).find(|g| g.2 >= r.start);
        if let Some((run_ix, glyph_ix, _)) = at { wraps.push(WrapBoundary { run_ix, glyph_ix }); }
    }
    (LineLayout { font_size: src.font_size, width: src.width - dropped, ascent: src.ascent, descent: src.descent, runs, len: src.len }, wraps)
}

fn is_cjk(ch: char) -> bool {
    (('\u{2E80}'..='\u{9FFF}').contains(&ch) || ('\u{F900}'..='\u{FFEF}').contains(&ch)) && !is_hangul(ch)
}

/// Korean puts spaces between words, so Hangul wraps by word (어절) like Latin.
fn is_hangul(ch: char) -> bool {
    matches!(ch, '\u{1100}'..='\u{11FF}' | '\u{3130}'..='\u{318F}' | '\u{A960}'..='\u{A97F}' | '\u{AC00}'..='\u{D7FF}' | '\u{FFA0}'..='\u{FFDC}')
}

/// A line may break before `ch`: after a space, or next to CJK ideographs
/// and kana, keeping closing punctuation with what precedes it.
fn can_break(prev: char, ch: char) -> bool {
    if ch.is_whitespace() { false } else if prev.is_whitespace() { true } else { (is_cjk(ch) || is_cjk(prev)) && !no_break_before(ch) && !no_break_after(prev) }
}

fn no_break_before(ch: char) -> bool {
    "，。、；：！？）」』】》〉’”…·,.;:!?)]}%".contains(ch)
}

fn no_break_after(ch: char) -> bool {
    "（「『【《〈‘“([{".contains(ch)
}

/// Greedy line breaking close to the browser's: after spaces and around CJK
/// characters, keeping closing punctuation with what precedes it. Equations
/// (`eqs`: offset, display) can break on both sides; display ones always do.
pub(crate) fn wrap_spaced(text: &str, layout: &LineLayout, width: Pixels, eqs: &[(usize, bool)]) -> Vec<WrapBoundary> {
    let glyphs: Vec<(usize, usize, usize, Pixels)> = layout
        .runs
        .iter()
        .enumerate()
        .flat_map(|(ri, r)| r.glyphs.iter().enumerate().map(move |(gi, g)| (ri, gi, g.index, g.position.x)))
        .collect();
    let mut out = vec![];
    let mut row_start = 0;
    let mut last_break: Option<usize> = None;
    for k in 0..glyphs.len() {
        let ix = glyphs[k].2;
        let ch = text[ix..].chars().next().unwrap_or(' ');
        if k > row_start && glyphs[k - 1].2 != ix {
            let prev = text[..ix].chars().next_back().unwrap_or(' ');
            let prev_ix = glyphs[k - 1].2;
            let eq_here = eqs.iter().find(|e| e.0 == ix);
            let eq_prev = eqs.iter().find(|e| e.0 == prev_ix);
            if eq_here.is_some_and(|e| e.1) || eq_prev.is_some_and(|e| e.1) {
                out.push(WrapBoundary { run_ix: glyphs[k].0, glyph_ix: glyphs[k].1 });
                row_start = k;
                last_break = None;
                continue;
            }
            let breakable = if eq_here.is_some() || eq_prev.is_some() {
                !ch.is_whitespace() && !no_break_before(ch)
            } else {
                can_break(prev, ch)
            };
            if breakable {
                last_break = Some(k);
            }
        }
        let end = glyphs.get(k + 1).map(|g| g.3).unwrap_or(layout.width);
        if k > row_start && !ch.is_whitespace() && end - glyphs[row_start].3 > width {
            let b = last_break.filter(|&b| b > row_start).unwrap_or(k);
            out.push(WrapBoundary { run_ix: glyphs[b].0, glyph_ix: glyphs[b].1 });
            row_start = b;
            last_break = None;
        }
    }
    out
}

/// Pos helper for callers that hit-test across blocks.
pub fn hit_test(cache: &LayoutCache, p: Point<Pixels>, candidates: &[BlockId]) -> Option<(Pos, bool)> {
    let cache = cache.borrow();
    let mut best: Option<(f32, f32, BlockId)> = None;
    for id in candidates {
        let Some(l) = cache.get(id) else { continue };
        let b = l.bounds;
        let dy = if p.y < b.top() {
            f32::from(b.top() - p.y)
        } else if p.y > b.bottom() {
            f32::from(p.y - b.bottom())
        } else {
            0.
        };
        let dx = if p.x < b.left() {
            f32::from(b.left() - p.x)
        } else if p.x > b.right() {
            f32::from(p.x - b.right())
        } else {
            0.
        };
        let better = match best {
            None => true,
            Some((by, bx, _)) => dy < by || (dy == by && dx < bx),
        };
        if better {
            best = Some((dy, dx, *id));
        }
    }
    let (_, _, id) = best?;
    let l = cache.get(&id)?;
    let (offset, at_row_end) = l.caret_for(p);
    Some((Pos::new(id, offset), at_row_end))
}

#[cfg(test)]
mod wrap_tests {
    use super::can_break;

    fn breaks(text: &str) -> Vec<usize> {
        let chars: Vec<char> = text.chars().collect();
        (1..chars.len()).filter(|&i| can_break(chars[i - 1], chars[i])).collect()
    }

    #[test]
    fn hangul_wraps_by_word_and_chinese_by_character() {
        assert_eq!(breaks("한국어 문서를 편집합니다"), [4, 8], "only where a word starts");
        assert_eq!(breaks("Quiet.md를 써요"), [10], "a particle stays on its word");
        assert_eq!(breaks("ㅋㅋㅋ 좋아요"), [4]);
        assert_eq!(breaks("你好世界"), [1, 2, 3]);
        assert_eq!(breaks("中文。한국어"), [1, 3], "Chinese still breaks, but not before closing punctuation");
        assert_eq!(breaks("Hello world"), [6]);
    }
}

#[cfg(test)]
mod quote_tests {
    use super::cjk_quotes;

    /// The quotes `cjk_quotes` gives the Chinese face, as the text with those
    /// marked `^` (the others `.`): everything else drops out.
    fn marks(text: &str) -> String {
        let on = cjk_quotes(text);
        text.char_indices().filter(|(_, ch)| super::is_quote(*ch)).map(|(at, _)| if on.contains(&at) { '^' } else { '.' }).collect()
    }

    #[test]
    fn quotes_in_chinese_text_take_the_chinese_face() {
        // Next to a Han character, on either side, or Chinese punctuation.
        assert_eq!(marks("他说“好”。"), "^^");
        assert_eq!(marks("说：“Hello world”"), "^^", "a pair goes together: the closing one has only Latin around it");
        assert_eq!(marks("用“Quiet.md”打开"), "^^", "English inside, Chinese outside");
        assert_eq!(marks("“好”"), "^^", "the start and end of a paragraph");
        assert_eq!(marks("“Hello”你好"), "^^", "one side is enough");
        // Nested: each pair by its own neighbours.
        assert_eq!(marks("他说：“她说‘好’。”"), "^^^^");
        assert_eq!(marks("他说：“‘Hi’ there”"), "^..^", "the inner pair has only quotes and Latin around it");
        // Latin on both sides stays Latin.
        assert_eq!(marks("He said “hello” to ‘me’."), "....");
        assert_eq!(marks("“Hello”"), "..");
        assert_eq!(marks("中文 “Hello” 中文"), "..", "a space between is not next to");
        // An apostrophe has no pair: its own neighbours decide.
        assert_eq!(marks("it’s 中文"), ".");
        assert_eq!(marks("中’s"), "^");
        assert_eq!(marks("‘it’s’ 好"), "...", "the apostrophe closes the pair; the last one is alone");
        // Unpaired openings and closings.
        assert_eq!(marks("“好"), "^");
        assert_eq!(marks("Hi” 好”"), ".^", "unpaired closings go by their own neighbours");
        assert_eq!(marks("plain text"), "");
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    /// Rows cut where the board cuts them: the spaces dropped at a break
    /// take no room, so a row is as wide as the board's line.
    #[test]
    fn rows_break_where_given() {
        use gpui::{FontId, GlyphId, LineLayout, ShapedGlyph, ShapedRun, WrapBoundary, point, px};
        // "ab  cd e": 10pt a glyph
        let text = "ab  cd e";
        let glyphs = text.char_indices().map(|(i, _)| ShapedGlyph { id: GlyphId(i as u32), position: point(px(i as f32 * 10.), px(0.)), index: i, is_emoji: false }).collect();
        let src = LineLayout { font_size: px(10.), width: px(80.), ascent: px(8.), descent: px(2.), runs: vec![ShapedRun { font_id: FontId(0), glyphs }], len: text.len() };
        let (l, wraps) = super::break_rows(&src, &[0..2, 4..6, 7..8]);
        assert_eq!(wraps, vec![WrapBoundary { run_ix: 0, glyph_ix: 4 }, WrapBoundary { run_ix: 0, glyph_ix: 7 }]);
        let xs: Vec<f32> = l.runs[0].glyphs.iter().map(|g| f32::from(g.position.x)).collect();
        // the two spaces sit at the end of "ab", "cd" right after them, then " " at its end, "e"
        assert_eq!(xs, vec![0., 10., 20., 20., 20., 30., 40., 40.]);
        assert_eq!(l.width, px(50.));
        // rows as wide as the board's lines: "ab", "cd", "e"
        let ends = [px(20.), px(40.), l.width];
        let starts: Vec<f32> = std::iter::once(0.).chain(wraps.iter().map(|w| xs[w.glyph_ix])).collect();
        assert_eq!(starts.iter().zip(ends).map(|(a, b)| f32::from(b) - a).collect::<Vec<_>>(), vec![20., 20., 10.]);
    }

    /// Editor text shapes CJK through CoreText's PingFang fallback, while CJK
    /// punctuation loads the PingFang SC family (`CJK_PUNCT_FONT`): both must
    /// keep drawing the right glyphs.
    ///
    /// CoreText's fallback copy of PingFang SC shares its PostScript name with
    /// the PingFang SC family but not its glyph table (vendor patch
    /// fallback-font-identity). Shapes the text in the system font before and
    /// after loading that family, and checks every glyph against its face.
    #[test]
    fn cjk_fallback_glyphs() {
        use gpui::{FontFallbacks, FontRun, PlatformTextSystem, px};
        let text = "周末规划一背景本季度目标";
        let fonts = gpui_apple::AppleTextSystem::new();
        let mut ui = gpui::font(".SystemUIFont");
        ui.fallbacks = Some(FontFallbacks::from_fonts(vec!["PingFang SC".into()]));
        let font_id = fonts.font_id(&ui).unwrap();
        for pass in 0..2 {
            if pass == 1 { fonts.font_id(&gpui::font("PingFang SC")).unwrap(); }
            let layout = fonts.layout_line(text, px(16.), &[FontRun { font_id, len: text.len() }]);
            for run in &layout.runs {
                for g in &run.glyphs {
                    let ch = text[g.index..].chars().next().unwrap_or_default();
                    assert_eq!(fonts.glyph_for_char(run.font_id, ch), Some(g.id), "{ch:?} drawn from the wrong face (pass {pass})");
                }
            }
        }
    }

    /// The face CoreText falls back to for a CJK character is kept and given
    /// to later lines (vendor patch reuse-coretext-fonts). Those lines
    /// must come out exactly as CoreText's own fallback has them on a fresh
    /// text system: the same glyphs, places and outlines (bold staying bold
    /// after regular text kept its faces), across runs of alternating sizes.
    #[test]
    fn kept_cjk_fallbacks_shape_like_coretext() {
        use gpui::{FontRun, FontWeight, PlatformTextSystem, px};
        let text = "他说“好”。用 Quiet.md 打开（文件），再按「保存」：完成！";
        let split = text.char_indices().nth(9).unwrap().0;
        let shape = |fonts: &gpui_apple::AppleTextSystem, weight, parts: usize| {
            let id = fonts.font_id(&gpui::Font { weight, ..gpui::font(".SystemUIFont") }).unwrap();
            let runs = if parts == 1 { vec![FontRun { font_id: id, len: text.len() }] } else { vec![FontRun { font_id: id, len: split }, FontRun { font_id: id, len: text.len() - split }] };
            let layout = fonts.layout_line(text, px(16.), &runs);
            layout.runs.iter().flat_map(|r| r.glyphs.iter().map(move |g| (g.id, g.index, g.position, fonts.glyph_outline(r.font_id, g.id)))).collect::<Vec<_>>()
        };
        let kept = gpui_apple::AppleTextSystem::new();
        for weight in [FontWeight::NORMAL, FontWeight::BOLD] {
            for parts in [1, 2] {
                let fresh = shape(&gpui_apple::AppleTextSystem::new(), weight, parts);
                for pass in 0..2 {
                    assert!(shape(&kept, weight, parts) == fresh, "{weight:?}, {parts} runs, pass {pass}");
                }
            }
        }
    }

    /// #419: curly quotes are drawn in `QUOTE_FONT` whatever the run's weight
    /// and style, so it must be a family the platform has: a missing one
    /// resolves every weight and style to one regular fallback face.
    #[test]
    fn quote_font_keeps_bold_and_italic() {
        use gpui::{FontStyle, FontWeight};
        let ts = gpui::TextSystem::new(std::sync::Arc::new(gpui_apple::AppleTextSystem::new()));
        let id = |family: &str, weight, style| ts.resolve_font(&gpui::Font { weight, style, ..gpui::font(family.to_string()) });
        for family in [super::QUOTE_FONT, "No Such Family"] {
            let regular = id(family, FontWeight::NORMAL, FontStyle::Normal);
            let differ = regular != id(family, FontWeight::BOLD, FontStyle::Normal) && regular != id(family, FontWeight::NORMAL, FontStyle::Italic);
            assert_eq!(differ, family == super::QUOTE_FONT, "{family}");
        }
    }

    #[test]
    fn field_scroll() {
        use super::hscroll;
        use gpui::px;
        let w = px(100.);
        // Short text never scrolls; a caret at the end shows the tail.
        assert_eq!(hscroll(px(60.), px(60.), w, px(0.)), px(0.));
        assert_eq!(hscroll(px(300.), px(300.), w, px(0.)), px(208.));
        // Moving the caret back reveals the text around it.
        assert_eq!(hscroll(px(300.), px(0.), w, px(208.)), px(0.));
        assert_eq!(hscroll(px(300.), px(150.), w, px(0.)), px(58.));
        // Shorter text clamps the old offset.
        assert_eq!(hscroll(px(120.), px(120.), w, px(208.)), px(28.));
    }
}
