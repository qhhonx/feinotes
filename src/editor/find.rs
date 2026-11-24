//! 查找和替换 (⌘F / ⌘⇧H): the floating box at the top right of the page,
//! match search over every visible text line and the replace commands.

use super::core::{Core, EditKind, Pos, Selection, TITLE};
use super::view::Editor;
use crate::input::TextField;
use crate::model::*;
use crate::editor_theme::Theme;
use crate::editor_ui::{TipExt, TipSide};
use crate::design::{Button, ButtonSize, ButtonVariant, Field, FieldSize, Popover};
use gpui::{prelude::FluentBuilder, *};
use regex::Regex;
use std::ops::Range;

pub struct Find {
    /// 替换 tab selected.
    pub replace: bool,
    /// `.*` on: the query is a regular expression (remembered in the preferences).
    pub regex: bool,
    /// The regular expression does not compile.
    pub invalid: bool,
    /// Collecting stopped at `MAX_HITS`.
    pub capped: bool,
    /// Hits in document order.
    pub matches: Vec<(BlockId, Range<usize>)>,
    pub current: Option<usize>,
    #[cfg(target_os = "ios")]
    pub native_serial: u64,
    #[cfg(target_os = "ios")]
    pub native_published: Vec<u8>,
    #[cfg(target_os = "ios")]
    pub native_viewport: Option<Size<Pixels>>,
}

/// Hits beyond this are not collected, so a one-letter pattern over a book stays responsive.
pub const MAX_HITS: usize = 10_000;

/// The query as a case-insensitive regular expression where `^` and `$`
/// also match at line breaks; None for an empty query.
pub fn compile(q: &str) -> Result<Option<Regex>, regex::Error> {
    if q.is_empty() {
        return Ok(None);
    }
    regex::RegexBuilder::new(q).case_insensitive(true).multi_line(true).size_limit(1 << 22).build().map(Some)
}

/// Non-empty, non-overlapping hits of `re` within one line's text that keep
/// clear of inline objects, at most `limit`. Empty matches (`^`, `a*`) are skipped.
pub fn find_re(text: &RichText, re: &Regex, limit: usize) -> Vec<Range<usize>> {
    let raw: Vec<Range<usize>> = text.spans().filter(|(_, m)| m.inline.is_some()).map(|(r, _)| r).collect();
    re.find_iter(&text.text)
        .map(|m| m.range())
        .filter(|r| !r.is_empty() && !raw.iter().any(|o| o.start < r.end && r.start < o.end))
        .take(limit)
        .collect()
}

/// The replacement for each hit in `hits` (sorted, as `find_re` gave them):
/// `with` with `$1` / `${name}` filled from the hit's groups.
pub fn expand_all(text: &str, re: &Regex, hits: &[Range<usize>], with: &str) -> Vec<String> {
    let mut caps = re.captures_iter(text).peekable();
    hits.iter()
        .map(|r| {
            while caps.peek().is_some_and(|c| c.get(0).is_some_and(|m| m.start() < r.start)) {
                caps.next();
            }
            let mut out = String::new();
            match caps.peek() {
                Some(c) if c.get(0).map(|m| m.range()) == Some(r.clone()) => c.expand(with, &mut out),
                _ => out.push_str(with),
            }
            out
        })
        .collect()
}

/// Replace `r` with plain `with` in the style of the replaced text's first run.
pub fn replace_in(t: &mut RichText, r: Range<usize>, with: &str, plain: bool) {
    let mut marks = t.slice(r.clone()).runs.first().map(|run| run.marks.clone()).unwrap_or_default();
    marks.inline = None;
    if plain {
        marks = Marks::default();
    }
    let mut ins = RichText::default();
    ins.push(with, marks);
    t.replace(r, &ins);
}

/// Case-insensitive, non-overlapping hits of `q` in `text`, skipping inline objects.
pub fn find_in(text: &RichText, q: &str) -> Vec<Range<usize>> {
    let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
    let q: Vec<char> = q.chars().map(fold).collect();
    if q.is_empty() {
        return vec![];
    }
    let chars: Vec<(usize, char)> = text.text.char_indices().collect();
    let raw: Vec<Range<usize>> = text.spans().filter(|(_, m)| m.inline.is_some()).map(|(r, _)| r).collect();
    let mut out = vec![];
    let mut i = 0;
    while i + q.len() <= chars.len() {
        if chars[i..i + q.len()].iter().zip(&q).all(|((_, c), k)| fold(*c) == *k) {
            let start = chars[i].0;
            let end = chars.get(i + q.len()).map_or(text.len(), |(o, _)| *o);
            if !raw.iter().any(|r| r.start < end && start < r.end) {
                out.push(start..end);
                i += q.len();
                continue;
            }
        }
        i += 1;
    }
    out
}

/// The first hit of `q` below the title, on a line the page shows: where a
/// 搜索 result found in the body opens (#64).
pub fn first_body_hit(core: &Core, q: &str) -> Option<(BlockId, Range<usize>)> {
    core.units().iter().filter(|u| u.text && u.id != TITLE).find_map(|u| Some((u.id, find_in(core.text_of(u.id)?, q).into_iter().next()?)))
}

const BOX_W: f32 = 404.;
const BTN_W: f32 = 80.;

impl Editor {
    pub fn open_find(&mut self, replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        // Seed the query with a single-line selection, like the browser find bar.
        let (a, b) = self.core.ordered();
        if !self.core.in_block_mode() && a.block == b.block && a.offset < b.offset {
            let s = self.core.text_of(a.block).and_then(|t| t.text.get(a.offset..b.offset)).unwrap_or("").to_string();
            if !s.contains('\n') {
                self.find_field.update(cx, |f, cx| f.set_text(&s, cx));
            }
        }
        #[cfg(target_os = "ios")]
        if let Some(find) = &self.find { super::find_mobile::dismiss(find.native_serial); }
        let regex = !cfg!(target_os = "ios") && crate::settings::prefs().find_regex;
        self.find = Some(Find { replace, regex, invalid: false, capped: false, matches: vec![], current: None,
            #[cfg(target_os = "ios")] native_serial: super::find_mobile::next_serial(),
            #[cfg(target_os = "ios")] native_published: vec![],
            #[cfg(target_os = "ios")] native_viewport: None,
        });
        self.slash = None;
        self.menu = None;
        // 13pt inputs (the fields are made with the 14pt default).
        for f in [&self.find_field, &self.replace_field] {
            f.update(cx, |f, _| {
                f.size = 13.;
                f.line_height = 20.;
            });
        }
        self.refresh_find(true, cx);
        let field = self.find_field.clone();
        #[cfg(not(target_os = "ios"))]
        crate::gpui_compat::focus(window, &field.focus_handle(cx), cx);
        field.update(cx, |f, cx| f.select_all_text(cx));
        self.scroll_to_current(window);
        #[cfg(target_os = "ios")]
        self.present_native_find(cx);
        cx.notify();
    }

    pub fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(f) = self.find.take() else { return };
        #[cfg(target_os = "ios")]
        super::find_mobile::dismiss(f.native_serial);
        // Leave the current hit selected.
        if let Some((id, r)) = f.current.and_then(|i| f.matches.get(i).cloned()) {
            self.core.block_sel = None;
            self.core.sel = Selection { anchor: Pos::new(id, r.start), head: Pos::new(id, r.end) };
            self.toolbar_hidden = true;
        }
        crate::gpui_compat::focus(window, &self.focus, cx);
        cx.notify();
    }

    /// Select the first body hit of `q` and bring it into view (centred like
    /// a find hit), instead of the remembered reading position; false when
    /// the page shows none. 搜索 opens a body hit this way (#64).
    pub fn reveal_body_hit(&mut self, q: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.board_file || self.table_file { return false; }
        let Some((id, r)) = first_body_hit(&self.core, q) else { return false };
        self.core.block_sel = None;
        self.core.cell_sel = None;
        self.core.sel = Selection { anchor: Pos::new(id, r.start), head: Pos::new(id, r.end) };
        self.toolbar_hidden = true;
        self.restore_top = None;
        self.reveal = Some((super::render::Reveal::Find(Pos::new(id, r.start)), 0));
        window.refresh();
        cx.notify();
        true
    }

    /// Recompute hits. `from_caret` picks the first hit at or after the caret,
    /// otherwise the current hit stays where it was in the document.
    pub fn refresh_find(&mut self, from_caret: bool, cx: &App) {
        let Some(old) = self.find.take() else { return };
        let q = self.find_field.read(cx).text.clone();
        let units = self.core.units();
        let re = if old.regex { compile(&q) } else { Ok(None) };
        let invalid = re.is_err();
        let re = re.ok().flatten();
        let mut matches = vec![];
        for u in units.iter().filter(|u| u.text) {
            if matches.len() > MAX_HITS {
                break;
            }
            if let Some(t) = self.core.text_of(u.id) {
                let hits = match &re {
                    Some(re) => find_re(t, re, MAX_HITS + 1 - matches.len()),
                    None if old.regex => vec![],
                    None => find_in(t, &q),
                };
                matches.extend(hits.into_iter().map(|r| (u.id, r)));
            }
        }
        let capped = matches.len() > MAX_HITS;
        matches.truncate(MAX_HITS);
        let order = |id: BlockId| units.iter().position(|u| u.id == id).unwrap_or(usize::MAX);
        let anchor = if from_caret {
            let h = self.core.ordered().0;
            Some((order(h.block), h.offset))
        } else {
            old.current.and_then(|i| old.matches.get(i)).map(|(id, r)| (order(*id), r.start))
        };
        let current = (!matches.is_empty()).then(|| {
            anchor
                .and_then(|a| matches.iter().position(|(id, r)| (order(*id), r.start) >= a))
                .unwrap_or(0)
        });
        self.find = Some(Find { replace: old.replace, regex: old.regex, invalid, capped, matches, current,
            #[cfg(target_os = "ios")] native_serial: old.native_serial,
            #[cfg(target_os = "ios")] native_published: old.native_published,
            #[cfg(target_os = "ios")] native_viewport: old.native_viewport,
        });
    }

    /// Hits inside one text line, for painting.
    pub fn find_hits(&self, id: BlockId) -> Vec<(Range<usize>, bool)> {
        let Some(f) = &self.find else { return vec![] };
        f.matches
            .iter()
            .enumerate()
            .filter(|(_, (b, _))| *b == id)
            .map(|(i, (_, r))| (r.clone(), f.current == Some(i)))
            .collect()
    }

    pub(super) fn step_find(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(f) = &mut self.find else { return };
        let n = f.matches.len();
        if n == 0 {
            return;
        }
        f.current = Some(match f.current {
            Some(i) if forward => (i + 1) % n,
            Some(i) => (i + n - 1) % n,
            None => 0,
        });
        self.scroll_to_current(window);
        cx.notify();
    }

    /// Bring the current hit into view (centered when it was off screen).
    pub(super) fn scroll_to_current(&mut self, window: &mut Window) {
        let Some((id, r)) = self.find.as_ref().and_then(|f| f.matches.get(f.current?).cloned()) else { return };
        self.reveal = Some((super::render::Reveal::Find(Pos::new(id, r.start)), 0));
        window.refresh();
    }

    fn replace_range(&mut self, id: BlockId, r: Range<usize>, with: &str) {
        let Some(t) = self.core.text_mut(id) else { return };
        replace_in(t, r, with, id == TITLE);
    }

    /// The compiled query while `.*` is on.
    fn find_regex(&self, cx: &App) -> Option<Regex> {
        self.find.as_ref().filter(|f| f.regex)?;
        compile(&self.find_field.read(cx).text).ok().flatten()
    }

    /// What replaces each hit of block `id`: the field's text, with groups filled in regex mode.
    fn replacements(&self, id: BlockId, hits: &[Range<usize>], with: &str, cx: &App) -> Vec<String> {
        match (self.find_regex(cx), self.core.text_of(id)) {
            (Some(re), Some(t)) => expand_all(&t.text, &re, hits, with),
            _ => hits.iter().map(|_| with.to_string()).collect(),
        }
    }

    fn toggle_regex(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(f) = &mut self.find else { return };
        f.regex = !f.regex;
        let mut p = crate::settings::prefs();
        p.find_regex = f.regex;
        let _ = crate::settings::save(&p);
        self.refresh_find(true, cx);
        self.scroll_to_current(window);
        let field = self.find_field.clone();
        window.focus(&field.focus_handle(cx), cx);
        cx.notify();
    }

    fn after_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.core.fix_selection();
        if self.core.dirty {
            self.core.dirty = false;
            cx.emit(super::view::EditorEvent::Changed);
        }
        self.refresh_find(false, cx);
        self.scroll_to_current(window);
        cx.notify();
    }

    pub(super) fn replace_one(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, r)) = self.find.as_ref().and_then(|f| f.matches.get(f.current?).cloned()) else { return };
        let with = self.replace_field.read(cx).text.clone();
        let with = self.replacements(id, std::slice::from_ref(&r), &with, cx).pop().unwrap_or(with);
        self.core.checkpoint(EditKind::Other);
        self.replace_range(id, r.clone(), &with);
        // Keep "current" on the hit that followed the replaced one.
        if let Some(f) = &mut self.find {
            let next = f.current.map_or(0, |i| i + 1);
            f.current = f.matches.get(next).map(|_| next).or(f.current);
            let delta = with.len() as isize - r.len() as isize;
            for (b, m) in f.matches.iter_mut() {
                if *b == id && m.start >= r.end {
                    *m = (m.start as isize + delta) as usize..(m.end as isize + delta) as usize;
                }
            }
        }
        self.after_replace(window, cx);
    }

    pub(super) fn replace_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(f) = &self.find else { return };
        if f.matches.is_empty() {
            return;
        }
        let matches = f.matches.clone();
        let with = self.replace_field.read(cx).text.clone();
        // Fill in every hit's groups before the text changes.
        let mut edits = vec![];
        for chunk in matches.chunk_by(|a, b| a.0 == b.0) {
            let hits: Vec<Range<usize>> = chunk.iter().map(|(_, r)| r.clone()).collect();
            let texts = self.replacements(chunk[0].0, &hits, &with, cx);
            edits.extend(hits.into_iter().zip(texts).map(|(r, s)| (chunk[0].0, r, s)));
        }
        self.core.checkpoint(EditKind::Other);
        for (id, r, s) in edits.into_iter().rev() {
            self.replace_range(id, r, &s);
        }
        self.after_replace(window, cx);
    }

    pub(super) fn on_find_event(&mut self, ev: &crate::input::TextFieldEvent, window: &mut Window, cx: &mut Context<Self>) {
        use crate::input::TextFieldEvent as E;
        match ev {
            E::Changed => {
                self.refresh_find(true, cx);
                self.scroll_to_current(window);
                cx.notify();
            }
            E::Submit => self.step_find(true, window, cx),
            E::ShiftSubmit => self.step_find(false, window, cx),
            E::Cancel => self.close_find(window, cx),
            _ => {}
        }
    }

    pub(super) fn find_box(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if cfg!(target_os = "ios") { return None; }
        let f = self.find.as_ref()?;
        let n = f.matches.len();
        let more = if f.capped { "+" } else { "" };
        let counter = match f.current {
            _ if f.invalid => crate::settings::text("正则有误").to_string(),
            Some(i) if n > 0 => format!("{} / {n}{more}", i + 1),
            _ => "0 / 0".to_string(),
        };
        let (invalid, regex) = (f.invalid, f.regex);
        let replace = f.replace;
        // Tabs: the chosen one semibold in the accent over a 2pt accent bar.
        let tab = |id: &'static str, label: &'static str, active: bool, to_replace: bool| {
            div()
                .id(id)
                .relative()
                .h_full()
                .flex()
                .items_center()
                .cursor_pointer()
                .text_size(px(15.))
                .text_color(if active { Theme::primary_500() } else { Theme::text_caption() })
                .when(active, |d| d.font_weight(FontWeight::SEMIBOLD))
                .when(!active, |d| d.hover(|s| s.text_color(Theme::text())))
                .child(label)
                .when(active, |d| {
                    d.child(div().absolute().bottom_0().left_0().right_0().h(px(2.)).rounded(px(1.)).bg(Theme::primary()))
                })
                .on_click(cx.listener(move |e, _, window, cx| {
                    if let Some(f) = &mut e.find {
                        f.replace = to_replace;
                    }
                    let field = e.find_field.clone();
                    crate::gpui_compat::focus(window, &field.focus_handle(cx), cx);
                    cx.notify();
                }))
        };
        // The input: a small field.
        let input = |id: &'static str, field: &Entity<TextField>, suffix: Option<String>, toggle: Option<AnyElement>| {
            let focused = field.read(cx).is_focused(window);
            Field::new(id)
                .sized(FieldSize::Sm)
                .focused(focused)
                .gap(px(8.))
                .child(field.clone())
                .children(suffix.map(|s| {
                    let color = if invalid { Theme::danger() } else { Theme::text_caption() };
                    div().flex_none().text_size(px(12.)).line_height(px(20.)).text_color(color).child(s)
                }))
                .children(toggle)
        };
        // `.*`: regular expressions, accent-filled while on.
        let regex_toggle = Button::ghost("find-regex")
            .sized(ButtonSize::Xs)
            .selected(regex)
            .mr(px(-6.))
            .w(px(24.))
            .font_weight(FontWeight::SEMIBOLD)
            .label(".*")
            .tip_label(crate::settings::text("正则表达式").into(), TipSide::Above)
            .on_click(cx.listener(|e, _, window, cx| e.toggle_regex(window, cx)))
            .into_any_element();
        let label = |s: &'static str| div().text_size(px(13.)).line_height(px(20.)).text_color(Theme::text()).child(s);
        let enabled = n > 0;
        // Primary or outline, small; disabled with nothing found.
        let button = |id: &'static str, text: &'static str, primary: bool| {
            Button::new(id, if primary { ButtonVariant::Primary } else { ButtonVariant::Outline }).sized(ButtonSize::Sm).w(px(BTN_W)).disabled(!enabled).label(text)
        };
        let prev = button("find-prev", crate::settings::text("上一处"), false)
            .tip_label("Shift + Enter".into(), TipSide::Above)
            .on_click(cx.listener(|e, _, window, cx| e.step_find(false, window, cx)));
        let next = button("find-next", crate::settings::text("下一处"), false)
            .tip_label("Enter".into(), TipSide::Above)
            .on_click(cx.listener(|e, _, window, cx| e.step_find(true, window, cx)));
        let body = div()
            .px(px(20.))
            .pb(px(20.))
            .flex()
            .flex_col()
            .child(
                div()
                    .mt(px(16.))
                    .flex()
                    .flex_col()
                    .when(replace, |d| d.child(label(crate::settings::text("查找"))))
                    .child(div().when(replace, |d| d.mt(px(6.))).child(input("find-field", &self.find_field, Some(counter), Some(regex_toggle)))),
            )
            .when(replace, |d| {
                d.child(
                    div()
                        .mt(px(14.))
                        .flex()
                        .flex_col()
                        .child(label(crate::settings::text("替换为")))
                        .child(div().mt(px(6.)).child(input("replace-field", &self.replace_field, None, None))),
                )
            })
            .child(
                div()
                    .mt(px(20.))
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .when(replace, |d| {
                        d.child(
                            button("find-replace-all", crate::settings::text("全部替换"), true)
                                .on_click(cx.listener(|e, _, window, cx| e.replace_all(window, cx))),
                        )
                        .child(
                            button("find-replace", crate::settings::text("替换"), false)
                                .on_click(cx.listener(|e, _, window, cx| e.replace_one(window, cx))),
                        )
                    })
                    .child(prev)
                    .child(next),
            );
        // The popover.
        let el = Popover::new("find-box")
            .absolute()
            .top(px(8.))
            .right(px(16.))
            .w(px(BOX_W))
            .cursor(CursorStyle::Arrow)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .relative()
                    .h(px(44.))
                    .px(px(20.))
                    .flex()
                    .gap(px(24.))
                    .border_b_1()
                    .border_color(Theme::line_divider())
                    .child(tab("find-tab", crate::settings::text("查找"), !replace, false))
                    .child(tab("replace-tab", crate::settings::text("替换"), replace, true))
                    .child(
                        Button::ghost("find-close")
                            .sized(ButtonSize::Xs)
                            .icon("x")
                            .absolute()
                            .top(px(10.))
                            .right(px(10.))
                            .on_click(cx.listener(|e, _, window, cx| e.close_find(window, cx))),
                    ),
            )
            .child(body);
        Some(el.into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::{compile, expand_all, find_in, find_re, first_body_hit, replace_in, MAX_HITS};
    use crate::model::*;

    fn re(q: &str) -> regex::Regex { compile(q).unwrap().unwrap() }

    #[test]
    fn regex_groups_fill_the_replacement() {
        let t = RichText::plain("2026-11-21 and 1999-01-02");
        let r = re(r"(\d{4})-(?P<m>\d\d)-(\d\d)");
        let hits = find_re(&t, &r, MAX_HITS);
        assert_eq!(hits, vec![0..10, 15..25]);
        assert_eq!(expand_all(&t.text, &r, &hits, "$3/${m}/$1 $$"), vec!["21/11/2026 $", "02/01/1999 $"]);
        // Only some of the hits (replace one): the others are skipped over.
        assert_eq!(expand_all(&t.text, &r, &hits[1..], "${m}"), vec!["01"]);
        // Case-insensitive like the plain search, `(?-i)` opts out.
        assert_eq!(find_re(&RichText::plain("Ab ab"), &re("ab"), 9), vec![0..2, 3..5]);
        assert_eq!(find_re(&RichText::plain("Ab ab"), &re("(?-i)ab"), 9), vec![3..5]);
    }

    #[test]
    fn bad_and_empty_patterns() {
        assert!(compile("(").is_err());
        assert!(compile("[a-").is_err());
        assert!(compile("").unwrap().is_none());
        // Empty matches are skipped and never loop.
        let t = RichText::plain("baaa\nb");
        assert!(find_re(&t, &re("^"), MAX_HITS).is_empty());
        assert!(find_re(&t, &re("$"), MAX_HITS).is_empty());
        assert_eq!(find_re(&t, &re("a*"), MAX_HITS), vec![1..4]);
        // `^` and `$` see the line breaks of a code block, never another block.
        assert_eq!(find_re(&t, &re("^b"), MAX_HITS), vec![0..1, 5..6]);
        // The cap.
        assert_eq!(find_re(&RichText::plain("x".repeat(50)), &re("x"), 7).len(), 7);
    }

    #[test]
    fn regex_skips_inline_objects_and_keeps_styles() {
        let mut t = RichText::default();
        let bold = Marks { bold: true, ..Default::default() };
        t.push("foo", bold.clone());
        t.push("bar ", Marks::default());
        t.push("\u{fffc}", Marks { inline: Some(Inline { kind: InlineKind::Other { tag: "x".into() }, xml: "<x/>".into() }), ..Default::default() });
        t.push("baz", Marks::default());
        let r = re(r"(fo+)(bar)|.\w+");
        let hits = find_re(&t, &r, MAX_HITS);
        assert_eq!(hits, vec![0..6], "a hit over the inline object is dropped");
        let with = expand_all(&t.text, &r, &hits, "$2$1");
        // Across bold and plain: the replacement takes the first run's style.
        replace_in(&mut t, hits[0].clone(), &with[0], false);
        assert!(t.text.starts_with("barfoo "));
        assert_eq!(t.slice(0..6).runs.len(), 1);
        assert!(t.slice(0..6).runs[0].marks.bold);
        assert!(!t.slice(6..7).runs[0].marks.bold);
        // The title stays plain.
        replace_in(&mut t, 0..3, "x", true);
        assert!(!t.slice(0..1).runs[0].marks.bold);
    }

    #[test]
    fn find_ignores_case_and_inline_objects() {
        let mut t = RichText::plain("Hello hello ");
        t.push("\u{fffc}", Marks { inline: Some(Inline { kind: InlineKind::Other { tag: "x".into() }, xml: "<x/>".into() }), ..Default::default() });
        t.push("中文中文", Marks::default());
        assert_eq!(find_in(&t, "HELLO"), vec![0..5, 6..11]);
        assert_eq!(find_in(&t, "中文"), vec![15..21, 21..27]);
        assert!(find_in(&t, "\u{fffc}").is_empty());
        assert_eq!(find_in(&RichText::plain("aaa"), "aa"), vec![0..2]);
    }

    #[test]
    fn body_hit_skips_the_title_and_folded_lines() {
        let doc = crate::docxml::parse("<title>Apple notes</title>\n<h2 folded=\"true\">Fold</h2>\n<p>hidden APPLE</p>\n<h2>Open</h2>\n<p>a <b>red</b> Apple, an apple</p>\n").unwrap();
        let core = crate::editor::core::Core::new(doc);
        let (id, r) = first_body_hit(&core, "apple").unwrap();
        assert_eq!(&core.text_of(id).unwrap().text[r], "Apple");
        assert_eq!(core.text_of(id).unwrap().text, "a red Apple, an apple");
        // Only the title or a folded section holds it: no body hit.
        assert!(first_body_hit(&core, "notes").is_none());
        assert!(first_body_hit(&core, "hidden").is_none());
        assert!(first_body_hit(&core, "").is_none());
    }
}
