//! Caret geometry for lines that contain right-to-left scripts. GPUI's
//! `x_for_index` and `closest_index_for_x` assume glyphs run left to right in
//! byte order; the platform returns right-to-left glyphs in visual order with
//! their byte indices running backwards, so those answers land on the wrong
//! side of a word. Lines without such scripts keep GPUI's answers.

use gpui::{LineLayout, Pixels};
use std::collections::HashMap;
use std::ops::Range;

/// Characters of right-to-left scripts (Hebrew, Arabic, Syriac, Thaana, N'Ko
/// and their presentation forms), as in GPUI's `line_layout::is_rtl`.
pub fn is_rtl(c: char) -> bool {
    matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF)
}

pub fn has_rtl(text: &str) -> bool {
    text.chars().any(is_rtl)
}

/// The line reads right to left: its first letter belongs to such a script
/// (the rule CoreText and GPUI's `WrappedLine` use for the paragraph).
pub fn rtl_line(text: &str) -> bool {
    text.chars().find(|c| c.is_alphabetic()).is_some_and(is_rtl)
}

/// One glyph cluster: bytes `ix..end`, drawn between `x0` and `x1`.
#[derive(Clone, Debug)]
struct Cluster {
    ix: usize,
    end: usize,
    x0: Pixels,
    x1: Pixels,
    rtl: bool,
}

/// The clusters drawn in `span` (one wrapped row), left to right.
fn clusters(layout: &LineLayout, text: &str, span: &Range<Pixels>) -> Vec<Cluster> {
    let glyphs: Vec<(usize, Pixels)> = layout.runs.iter().flat_map(|r| r.glyphs.iter()).map(|g| (g.index, g.position.x)).collect();
    let mut starts: Vec<usize> = glyphs.iter().map(|g| g.0).collect();
    starts.sort_unstable();
    starts.dedup();
    let end_of = |ix: usize| starts.get(starts.partition_point(|&s| s <= ix)).copied().unwrap_or(text.len());
    let mut out: Vec<Cluster> = vec![];
    for (i, &(ix, x)) in glyphs.iter().enumerate() {
        let next = glyphs.get(i + 1).map_or(layout.width, |g| g.1);
        let (x0, x1) = (x, next.max(x));
        // Marks and multi-glyph clusters join their base.
        if let Some(last) = out.last_mut().filter(|c| c.ix == ix) {
            last.x0 = last.x0.min(x0);
            last.x1 = last.x1.max(x1);
            continue;
        }
        out.push(Cluster { ix, end: end_of(ix), x0, x1, rtl: false });
    }
    out.retain(|c| c.x0 >= span.start && c.x0 < span.end);
    // A cluster reads right to left when the next character in the text sits
    // to its left (or the previous one to its right); a cluster alone in its
    // direction falls back to its own script, then to the line's.
    let at: HashMap<usize, usize> = out.iter().enumerate().map(|(k, c)| (c.ix, k)).collect();
    let ending: HashMap<usize, usize> = out.iter().enumerate().map(|(k, c)| (c.end, k)).collect();
    let line_rtl = rtl_line(text);
    for k in 0..out.len() {
        let (next, prev) = (at.get(&out[k].end).copied(), ending.get(&out[k].ix).copied());
        out[k].rtl = if next == Some(k + 1) || prev.is_some_and(|p| p + 1 == k) {
            false
        } else if next.is_some_and(|n| n + 1 == k) || prev == Some(k + 1) {
            true
        } else {
            match text[out[k].ix..].chars().next() {
                Some(c) if is_rtl(c) => true,
                Some(c) if c.is_alphabetic() => false,
                _ => line_rtl,
            }
        };
    }
    out
}

/// x of the caret before byte `index`, among the clusters in `span`: the
/// leading edge of the cluster that starts there (its right edge when it
/// reads right to left), or the trailing edge of the last one at the end.
pub fn x_for_index(layout: &LineLayout, text: &str, span: Range<Pixels>, index: usize) -> Option<Pixels> {
    let cs = clusters(layout, text, &span);
    if let Some(c) = cs.iter().find(|c| c.ix <= index && index < c.end) {
        // Inside a ligature (لا): share its width out.
        let f = (index - c.ix) as f32 / (c.end - c.ix) as f32;
        let w = c.x1 - c.x0;
        return Some(if c.rtl { c.x1 - w * f } else { c.x0 + w * f });
    }
    if let Some(c) = cs.iter().filter(|c| c.ix > index).min_by_key(|c| c.ix) {
        return Some(if c.rtl { c.x1 } else { c.x0 });
    }
    cs.iter().max_by_key(|c| c.end).map(|c| if c.rtl { c.x0 } else { c.x1 })
}

/// Byte offset of the caret stop nearest to `x` in `span`.
pub fn index_for_x(layout: &LineLayout, text: &str, span: Range<Pixels>, x: Pixels) -> Option<usize> {
    let cs = clusters(layout, text, &span);
    let dist = |c: &Cluster| if x < c.x0 { c.x0 - x } else if x >= c.x1 { x - c.x1 } else { Pixels::ZERO };
    let c = cs.iter().min_by(|a, b| dist(a).partial_cmp(&dist(b)).unwrap_or(std::cmp::Ordering::Equal))?;
    let left_half = x < c.x0 + (c.x1 - c.x0) / 2.;
    Some(if left_half != c.rtl { c.ix } else { c.end })
}

/// Horizontal extents of `range` in `span`: a selection that crosses a change
/// of direction covers separate pieces of the row.
pub fn ranges_for(layout: &LineLayout, text: &str, span: Range<Pixels>, range: Range<usize>) -> Vec<(Pixels, Pixels)> {
    let mut out: Vec<(Pixels, Pixels)> = vec![];
    let mut joined = false;
    for c in clusters(layout, text, &span) {
        if c.ix >= range.start && c.ix < range.end {
            match out.last_mut() {
                Some(last) if joined => last.1 = c.x1,
                _ => out.push((c.x0, c.x1)),
            }
            joined = true;
        } else {
            joined = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{FontId, GlyphId, ShapedGlyph, ShapedRun, point, px};

    /// A layout with one 10pt glyph per (byte index, x), in the given order.
    fn layout(glyphs: &[(usize, f32)], width: f32, len: usize) -> LineLayout {
        let glyphs = glyphs.iter().map(|&(index, x)| ShapedGlyph { id: GlyphId(0), position: point(px(x), px(0.)), index, is_emoji: false }).collect();
        LineLayout { font_size: px(16.), width: px(width), ascent: px(12.), descent: px(4.), runs: vec![ShapedRun { font_id: FontId(0), glyphs }], len }
    }

    #[test]
    fn arabic_caret_runs_right_to_left() {
        // "ابت": three 2-byte letters, drawn right to left.
        let text = "ابت";
        let l = layout(&[(4, 0.), (2, 10.), (0, 20.)], 30., text.len());
        let all = px(0.)..px(30.);
        let xs: Vec<f32> = (0..=6).step_by(2).map(|i| f32::from(x_for_index(&l, text, all.clone(), i).unwrap())).collect();
        assert_eq!(xs, [30., 20., 10., 0.]);
        assert_eq!(index_for_x(&l, text, all.clone(), px(28.)), Some(0));
        assert_eq!(index_for_x(&l, text, all.clone(), px(22.)), Some(2));
        assert_eq!(index_for_x(&l, text, all.clone(), px(-5.)), Some(6));
        assert_eq!(ranges_for(&l, text, all, 2..6), [(px(0.), px(20.))]);
    }

    #[test]
    fn latin_inside_arabic() {
        // "اب ab": the Latin word sits at the left end of a right-to-left line.
        let text = "اب ab";
        let l = layout(&[(5, 0.), (6, 10.), (4, 20.), (2, 30.), (0, 40.)], 50., text.len());
        let all = px(0.)..px(50.);
        assert!(rtl_line(text));
        assert_eq!(x_for_index(&l, text, all.clone(), 0), Some(px(50.)));
        assert_eq!(x_for_index(&l, text, all.clone(), 5), Some(px(0.)));
        assert_eq!(x_for_index(&l, text, all.clone(), 6), Some(px(10.)));
        // The end of the line is after "b", at its right edge.
        assert_eq!(x_for_index(&l, text, all.clone(), 7), Some(px(20.)));
        // Selecting "ب a" covers two pieces of the row.
        assert_eq!(ranges_for(&l, text, all, 2..6), [(px(0.), px(10.)), (px(20.), px(40.))]);
    }

    #[test]
    fn row_span_limits_the_clusters() {
        // Two rows of a wrapped right-to-left line, laid end to end.
        let text = "ابتث";
        let l = layout(&[(2, 0.), (0, 10.), (6, 20.), (4, 30.)], 40., text.len());
        assert_eq!(x_for_index(&l, text, px(0.)..px(20.), 0), Some(px(20.)));
        assert_eq!(x_for_index(&l, text, px(0.)..px(20.), 4), Some(px(0.)));
        assert_eq!(x_for_index(&l, text, px(20.)..px(40.), 4), Some(px(40.)));
    }
}
