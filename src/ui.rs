//! Small shared UI pieces: icons, tooltips, popover shells.

use crate::theme::{self, Theme, craft};
use crate::time::civil;
use gpui::{prelude::FluentBuilder, *};
use std::cell::RefCell;
use std::rc::Rc;

pub fn icon(name: &str, size: f32, color: Hsla) -> Svg {
    svg().path(SharedString::from(format!("icons/{name}.svg"))).size(px(size)).flex_none().text_color(color)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ToastKind {
    Success,
    Error,
    Info,
}

/// Toast: `design::Toast` 32px below the top, centred. It lasts 3s and fades over
/// its last .3s; None once it is over.
pub fn toast_el(kind: ToastKind, msg: &SharedString, t0: web_time::Instant, window: &mut Window) -> Option<AnyElement> {
    let e = t0.elapsed().as_secs_f32();
    if e >= 3. {
        return None;
    }
    let op = if e > 2.7 {
        window.request_animation_frame();
        (3. - e) / 0.3
    } else {
        1.
    };
    let tone = match kind {
        ToastKind::Success => crate::design::ToastTone::Success,
        ToastKind::Error => crate::design::ToastTone::Error,
        ToastKind::Info => crate::design::ToastTone::Info,
    };
    let vw = window.viewport_size().width;
    let card = crate::design::Toast::new(tone, crate::settings::label(msg));
    Some(
        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(point(px(0.), px(32.)))
                .child(div().w(vw).flex().justify_center().opacity(op).child(card)),
        )
        .with_priority(20)
        .into_any_element(),
    )
}



/// Column headers of the page lists: tertiary text (measured #88898B on
/// the name / last-viewed / created columns).
pub fn list_header_text() -> Hsla {
    theme::c(0x88898b)
}

/// Hairline under a list header and between rows (black 4.5%).
pub fn list_line() -> Hsla {
    theme::ca(craft::N1000, 0.045)
}

/// The quiet list hover: a rounded fill that also hides the row's
/// hairline above it.
pub fn row_hover(s: StyleRefinement) -> StyleRefinement {
    s.bg(Theme::fill_hover()).border_color(transparent_black()).rounded(px(ROW_RADIUS))
}




/// A busy mark for a button: three short bars rising and falling in turn,
/// for `t` seconds since it started. The caller asks for frames while it shows.
pub fn spinner(t: f32, color: Hsla) -> Div {
    let mut bars = div().h(px(14.)).flex().items_center().gap(px(3.));
    for i in 0..3 {
        let w = 0.5 + 0.5 * (std::f32::consts::TAU * (t * 1.25 - i as f32 * 0.18)).sin();
        bars = bars.child(div().w(px(3.)).h(px(5. + 9. * w)).rounded(px(1.5)).bg(color.opacity(0.55 + 0.45 * w)));
    }
    bars
}




/// A list row's corners under the pointer (主页, 回收站).
pub const ROW_RADIUS: f32 = 6.;


/// Tooltip bubble: shadcn's tooltip (`tip_bg`) with an arrow.
pub struct Tooltip {
    /// One or more lines.
    label: SharedString,
    /// Window bounds of the trigger, when it was registered through [`TipExt::tip`].
    anchor: Option<Bounds<Pixels>>,
    /// Where GPUI anchors the tooltip root (mouse + 1 at show time).
    origin: Point<Pixels>,
    side: TipSide,
}

thread_local! {
    /// The trigger last pressed (its bounds), whose tooltip stays hidden until the pointer leaves it.
    static TIP_PRESSED: std::cell::Cell<Option<Bounds<Pixels>>> = const { std::cell::Cell::new(None) };
}

/// Preferred side: chrome shows tooltips below, document controls above,
/// menu items to the right. Flipped when there is no room.
#[derive(Clone, Copy, PartialEq)]
pub enum TipSide {
    Below,
    Above,
    Right,
}

/// Radix's offset with shadcn's 10pt arrow (`size-2.5`, sticking out 5).
const TIP_GAP: f32 = 10.;
const TIP_ARROW: f32 = 5.;
const TIP_PAD: f32 = 2. * crate::design::G.tip_py;
const TIP_LINE: f32 = 16.;
/// shadcn's tooltip: `foreground` under `background` text (inverted in the dark), `rounded-md`,
/// px-3 py-1.5, text-xs.
fn tip_bg() -> Hsla { crate::design::color::foreground() }

impl Render for Tooltip {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if self.anchor.is_some() && TIP_PRESSED.get() == self.anchor { return div().into_any_element(); }
        let lines: Vec<SharedString> = self.label.split('\n').map(|l| SharedString::from(l.to_string())).collect();
        let g = crate::design::geo();
        let bubble = div()
            .px(px(g.tip_px))
            .py(px(g.tip_py))
            .rounded(px(g.tip_r))
            .bg(tip_bg())
            .text_color(crate::design::color::background())
            .text_size(px(g.text_xs))
            .line_height(px(TIP_LINE))
            .font_family(theme::ui_family())
            .whitespace_nowrap()
            .flex()
            .flex_col()
            .children(lines.iter().map(|l| ui_text(l.clone())));
        let Some(anchor) = self.anchor else {
            return div().pl(px(8.)).pt(px(10.)).child(bubble).into_any_element();
        };
        // 10px from the trigger with an arrow pointing at it, on the preferred
        // side unless it doesn't fit, and kept inside the window.
        let mut style = window.text_style();
        style.font_family = theme::ui_family().into();
        let text_w = lines
            .iter()
            .map(|l| {
                let runs = ui_text(l.clone()).runs(&style);
                window.text_system().shape_line(l.clone(), px(12.), &runs, None).width
            })
            .fold(px(0.), |a, b| a.max(b));
        let w = text_w + px(2. * g.tip_px);
        let h = px(TIP_PAD + TIP_LINE * lines.len() as f32);
        let vp = window.viewport_size();
        let gap = px(TIP_GAP);
        let s = px(TIP_ARROW);
        // (bubble origin, arrow base centre, arrow direction)
        let (x, y, arrow): (Pixels, Pixels, Box<dyn Fn(&mut PathBuilder)>) = if self.side == TipSide::Right
            && (anchor.right() + gap + w <= vp.width || anchor.left() - gap - w < px(0.))
        {
            let x = anchor.right() + gap;
            let cy = anchor.center().y;
            let y = (cy - h / 2.).min(vp.height - h).max(px(0.));
            let tip_y = cy.max(y + px(10.)).min(y + h - px(10.));
            (x, y, Box::new(move |p: &mut PathBuilder| {
                p.move_to(point(x, tip_y - s));
                p.line_to(point(x - s * 0.88, tip_y - px(0.8)));
                p.line_to(point(x - s * 0.88, tip_y + px(0.8)));
                p.line_to(point(x, tip_y + s));
            }))
        } else if self.side == TipSide::Right {
            let x = anchor.left() - gap - w;
            let cy = anchor.center().y;
            let y = (cy - h / 2.).min(vp.height - h).max(px(0.));
            let tip_y = cy.max(y + px(10.)).min(y + h - px(10.));
            let bx = x + w;
            (x, y, Box::new(move |p: &mut PathBuilder| {
                p.move_to(point(bx, tip_y - s));
                p.line_to(point(bx + s * 0.88, tip_y - px(0.8)));
                p.line_to(point(bx + s * 0.88, tip_y + px(0.8)));
                p.line_to(point(bx, tip_y + s));
            }))
        } else {
            let cx = anchor.center().x;
            let x = (cx - w / 2.).min(vp.width - w).max(px(0.));
            let fits_below = anchor.bottom() + gap + h <= vp.height;
            let fits_above = anchor.top() - gap - h >= px(0.);
            let below = if self.side == TipSide::Above { !fits_above } else { fits_below };
            let y = if below { anchor.bottom() + gap } else { anchor.top() - gap - h };
            let tip_x = cx.max(x + px(10.)).min(x + w - px(10.));
            let (base, apex) = if below { (y, y - s) } else { (y + h, y + h + s) };
            (x, y, Box::new(move |p: &mut PathBuilder| {
                p.move_to(point(tip_x - s, base));
                p.line_to(point(tip_x - px(0.8), apex + (base - apex) * 0.12));
                p.line_to(point(tip_x + px(0.8), apex + (base - apex) * 0.12));
                p.line_to(point(tip_x + s, base));
            }))
        };
        let o = self.origin;
        let arrow = canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let mut p = PathBuilder::fill();
                arrow(&mut p);
                p.close();
                if let Ok(path) = p.build() {
                    window.paint_path(path, tip_bg());
                }
            },
        )
        .absolute()
        .size(px(1.));
        div()
            .size(px(1.))
            .child(div().absolute().left(x - o.x).top(y - o.y).child(bubble))
            .child(arrow)
            .into_any_element()
    }
}

/// Tooltip anchored on the element rather than the mouse.
pub trait TipExt: Sized {
    /// Window chrome: below, shortcut in full-width parens ("展开面板（⌘ + ]）").
    fn tip(self, text: &str, keys: Option<&str>) -> Self {
        let text = crate::settings::label(text);
        let label = match keys {
            Some(k) => format!("{text}（{k}）"),
            None => text.to_string(),
        };
        self.tip_label(label, TipSide::Below)
    }
    /// Document controls: above, in the editor's shortcut format
    /// ("粗体 (⌘ + B)"), optionally with a "Markdown: …" second line.
    fn tip_above(self, text: &str, keys: Option<&str>) -> Self {
        self.tip_label(doc_tip(text, keys, None), TipSide::Above)
    }
    fn tip_md(self, text: &str, keys: Option<&str>, md: Option<&str>) -> Self {
        self.tip_label(doc_tip(text, keys, md), TipSide::Above)
    }
    /// Menu items: to the right.
    fn tip_right(self, text: &str, keys: Option<&str>) -> Self {
        self.tip_label(doc_tip(text, keys, None), TipSide::Right)
    }
    fn tip_label(self, label: String, side: TipSide) -> Self;
}

/// Window chrome above the page's top line works like a native title bar: a
/// press drags the window, a double press zooms it (per the system setting).
/// Controls on it opt out with `no_drag`, so double-clicking them doesn't zoom.
pub trait TitleBarExt: InteractiveElement + Sized {
    fn title_bar(self) -> Self {
        // Keep the workspace's focus handler from preventing the native caption action.
        if cfg!(target_os = "windows") { return self.occlude().window_control_area(WindowControlArea::Drag); }
        self.on_mouse_down(MouseButton::Left, |ev, window, _| match ev.click_count {
            1 => window.start_window_move(),
            2 => window.titlebar_double_click(),
            _ => {}
        })
    }
    fn no_drag(self) -> Self {
        let element = if cfg!(target_os = "windows") { self.occlude() } else { self };
        element.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    }
}

impl<E: InteractiveElement> TitleBarExt for E {}

pub(crate) const WINDOW_CONTROLS_W: f32 = if cfg!(any(target_os = "windows", target_os = "linux")) { 163. } else { 0. };
pub(crate) fn header_right_inset() -> f32 {
    if cfg!(any(target_os = "windows", target_os = "linux")) { WINDOW_CONTROLS_W } else { (crate::app::HEADER_H - crate::app::HEADER_ACTIONS_H) / 2. }
}

pub(crate) fn window_controls(window: &Window) -> Option<Div> {
    if !cfg!(any(target_os = "windows", target_os = "linux")) { return None; }
    // Windows must receive mouse-down to arm its native caption buttons.
    let button = |id: &'static str, area: WindowControlArea, label: &'static str| {
        div().id(id).size(px(crate::app::HEADER_ACTIONS_H)).mx(px(7.)).rounded(px(8.)).flex_none().flex().items_center().justify_center().cursor_pointer()
            .occlude()
            .when(cfg!(target_os = "linux"), |d| d.no_drag())
            .when(cfg!(target_os = "windows"), |d| d.window_control_area(area))
            .when(cfg!(target_os = "linux"), |d| d.on_click(move |_, window, cx| match area {
                WindowControlArea::Min => window.minimize_window(),
                WindowControlArea::Max => window.zoom_window(),
                WindowControlArea::Close => window.dispatch_action(Box::new(crate::app::CloseWindow), cx),
                WindowControlArea::Drag => {},
            })).hover(|s| s.bg(Theme::fill_hover())).active(|s| s.bg(Theme::fill_pressed())).tip(label, None)
    };
    let maximized = window.is_maximized();
    // Native hit-test areas preserve Snap Layouts and the normal WM_CLOSE save guard.
    Some(div().absolute().top_0().right_0().h(px(crate::app::HEADER_H)).flex().items_center().title_bar()
        .child(div().mx(px(12.)).w(px(1.)).h(px(18.)).bg(Theme::line_divider()))
        .child(button("window-minimize", WindowControlArea::Min, crate::settings::tr("最小化", "Minimize"))
            .child(icon("minus", 18., Theme::icon())))
        .child(button("window-maximize", WindowControlArea::Max, if maximized { crate::settings::tr("还原", "Restore") } else { crate::settings::tr("最大化", "Maximize") }).child(icon(if maximized { "copy" } else { "expand" }, 18., Theme::icon())))
        .child(button("window-close", WindowControlArea::Close, crate::settings::tr("关闭", "Close"))
            .child(icon("x", 18., Theme::icon()))))
}

/// "名称 (键 + 键)" plus an optional "Markdown: …" line.
pub fn doc_tip(text: &str, keys: Option<&str>, md: Option<&str>) -> String {
    let text = crate::settings::text(text);
    let mut s = match keys {
        Some(k) => format!("{text} ({k})"),
        None => text.to_string(),
    };
    if let Some(m) = md {
        s.push_str("\nMarkdown: ");
        s.push_str(crate::settings::text(m));
    }
    s
}

impl TipExt for Stateful<Div> {
    fn tip_label(self, label: String, side: TipSide) -> Self {
        let bounds = Rc::new(std::cell::Cell::new(None::<Bounds<Pixels>>));
        let record = bounds.clone();
        let label = SharedString::from(label);
        // Radix closes a trigger's tooltip on pointerdown and keeps it closed until the pointer
        // leaves, so a menu opened by the press isn't covered by it. GPUI only drops it on the
        // press and shows it again on the next move: `TIP_PRESSED` holds it back until the pointer
        // is out. The element is rebuilt every frame, so the press is kept by the trigger's bounds.
        let press = bounds.clone();
        self.child(canvas(move |b, window, _| {
            record.set(Some(b));
            if TIP_PRESSED.get() == Some(b) && !b.contains(&window.mouse_position()) { TIP_PRESSED.set(None); }
        }, |_, _, _, _| {}).absolute().inset_0())
            .capture_any_mouse_down(move |_, _, _| TIP_PRESSED.set(press.get()))
            // shadcn's TooltipProvider: `delayDuration = 0`
            .tooltip_show_delay(std::time::Duration::ZERO).tooltip(move |window, cx| {
            let origin = window.mouse_position() + point(px(1.), px(1.));
            let (label, anchor) = (label.clone(), bounds.get());
            cx.new(|_| Tooltip { label, anchor, origin, side }).into()
        })
    }
}

/// Human-friendly modification time ("今天 14:03", "昨天", "10月3日").
pub fn format_time(secs: u64) -> String {
    let now = crate::time::now();
    let (y, mo, d, h, mi) = civil(secs);
    let (ny, nmo, nd, _, _) = civil(now);
    let (yy, ymo, yd, _, _) = civil(now.saturating_sub(86_400));
    if (y, mo, d) == (ny, nmo, nd) {
        crate::localized_format!("今天 {h:02}:{mi:02}", "Today {h:02}:{mi:02}")
    } else if (y, mo, d) == (yy, ymo, yd) {
        crate::localized_format!("昨天 {h:02}:{mi:02}", "Yesterday {h:02}:{mi:02}")
    } else if y == ny {
        crate::localized_format!("{mo}月{d}日", "{mo:02}-{d:02}")
    } else {
        crate::localized_format!("{y}年{mo}月{d}日", "{y}-{mo:02}-{d:02}")
    }
}

/// Header status time ("刚刚", "5 分钟前", then like `format_time`).
pub fn format_ago(secs: u64) -> String {
    let ago = crate::time::now().saturating_sub(secs);
    if ago < 60 {
        crate::settings::text("刚刚").into()
    } else if ago < 3600 {
        crate::localized_format!("{} 分钟前", "{} min ago", ago / 60)
    } else {
        format_time(secs)
    }
}

/// Full date ("2025年12月2日"), as in 文档信息.
pub fn format_date(secs: u64) -> String {
    let (y, mo, d, _, _) = civil(secs);
    crate::localized_format!("{y}年{mo}月{d}日", "{y}-{mo:02}-{d:02}")
}

/// Relative time for 文档信息 ("几秒前", "3 分钟前", "2 小时前", then the date).
pub fn format_relative(secs: u64) -> String {
    let ago = crate::time::now().saturating_sub(secs);
    match ago {
        0..60 => crate::settings::text("几秒前").into(),
        60..3600 => crate::localized_format!("{} 分钟前", "{} min ago", ago / 60),
        3600..86_400 => crate::localized_count!(ago / 3600, "{} 小时前", "{} hour ago", "{} hours ago", ago / 3600),
        _ => format_date(secs),
    }
}

/// Search result times (`space.date_format.*`): "刚刚", "N 分钟前",
/// "今天 / 昨天 / 前天 HH:mm", "M月D日 HH:mm", across years "YYYY年M月D日".
pub fn format_visit(secs: u64) -> String {
    let now = crate::time::now();
    let ago = now.saturating_sub(secs);
    if ago < 60 {
        return crate::settings::tr("刚刚", "just now").into();
    }
    if ago < 3600 {
        return crate::localized_format!("{} 分钟前", "{} min ago", ago / 60);
    }
    let (y, mo, d, h, mi) = civil(secs);
    let day = |back: u64| {
        let (y, mo, d, _, _) = civil(now.saturating_sub(back * 86_400));
        (y, mo, d)
    };
    let names = [crate::settings::tr("今天", "Today"), crate::settings::tr("昨天", "Yesterday"), crate::settings::tr("前天", "Two days ago")];
    if let Some(name) = (0..3).find(|&b| day(b) == (y, mo, d)).map(|b| names[b as usize]) {
        format!("{name} {h:02}:{mi:02}")
    } else if y == civil(now).0 {
        crate::localized_format!("{mo}月{d}日 {h:02}:{mi:02}", "{mo:02}-{d:02} {h:02}:{mi:02}")
    } else {
        crate::localized_format!("{y}年{mo}月{d}日", "{y}-{mo:02}-{d:02}")
    }
}

/// Full-width CJK punctuation (、。〈〉《》「」『』【】〔〕（）：，… in U+3001–301B / U+FF01–FF65).
pub fn is_cjk_punct(ch: char) -> bool {
    matches!(ch, '\u{3001}'..='\u{3003}' | '\u{3008}'..='\u{3011}' | '\u{3014}'..='\u{301B}'
        | '\u{FF01}'..='\u{FF0F}' | '\u{FF1A}'..='\u{FF20}' | '\u{FF3B}'..='\u{FF40}' | '\u{FF5B}'..='\u{FF65}')
}

/// Font for full-width punctuation, and curly quotes in Chinese text. When
/// CoreText falls back from the system UI font it squeezes these (an opening
/// paren after a CJK character loses ~5px). Browsers draw them full width
/// with PingFang SC. Each system's own Chinese sans, which has bold: a
/// missing family would draw every weight and style in one regular face
/// (`cjk_punct_font`).
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub const CJK_PUNCT_FONT: &str = "PingFang SC";
#[cfg(target_os = "windows")]
pub const CJK_PUNCT_FONT: &str = "Microsoft YaHei";
#[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "windows")))]
pub const CJK_PUNCT_FONT: &str = "Noto Sans CJK SC";

/// `CJK_PUNCT_FONT`, unless this system was found not to have it
/// (`fonts::check_cjk_punct`): then that text keeps its own family, as
/// before there was one.
pub fn cjk_punct_font() -> Option<&'static str> { crate::fonts::cjk_punct_installed().then_some(CJK_PUNCT_FONT) }

/// Single-line UI text that keeps full-width punctuation at full width,
/// with an optional colored range (search hits). Inherits the parent text
/// style; honours `text_ellipsis()` against the final width (StyledText
/// caches its first, unconstrained measurement when nowrap).
pub struct UiText {
    text: SharedString,
    hit: Option<(std::ops::Range<usize>, Hsla)>,
}

pub fn ui_text(text: impl Into<SharedString>) -> UiText {
    UiText { text: text.into(), hit: None }
}

pub struct UiTextLayout {
    runs: Vec<TextRun>,
    font_size: Pixels,
    line_height: Pixels,
    ellipsis: Option<SharedString>,
    full: Rc<RefCell<Option<ShapedLine>>>,
}

impl UiText {
    pub fn hit(mut self, range: Option<std::ops::Range<usize>>, color: Hsla) -> Self {
        self.hit = range.map(|r| (r, color));
        self
    }

    fn runs(&self, style: &TextStyle) -> Vec<TextRun> {
        match &self.hit {
            Some((r, c)) => ui_runs(&self.text, style, std::slice::from_ref(r), *c),
            None => ui_runs(&self.text, style, &[], style.color),
        }
    }
}

/// Text runs that keep full-width punctuation at full width (see
/// [`CJK_PUNCT_FONT`]), with `hits` drawn in `hit_color`.
pub fn ui_runs(text: &str, style: &TextStyle, hits: &[std::ops::Range<usize>], hit_color: Hsla) -> Vec<TextRun> {
    let mut cuts = vec![0, text.len()];
    for (i, ch) in text.char_indices() {
        if is_cjk_punct(ch) {
            cuts.extend([i, i + ch.len_utf8()]);
        }
    }
    for r in hits {
        cuts.extend([r.start.min(text.len()), r.end.min(text.len())]);
    }
    cuts.sort();
    cuts.dedup();
    cuts.windows(2)
        .map(|w| {
            let mut run = style.to_run(w[1] - w[0]);
            if text[w[0]..w[1]].chars().all(is_cjk_punct) && !crate::fonts::own_glyphs(&run.font.family).cjk_punct && let Some(f) = cjk_punct_font() {
                run.font.family = f.into();
            }
            if hits.iter().any(|r| r.start <= w[0] && w[1] <= r.end) {
                run.color = hit_color;
            }
            run
        })
        .collect()
}

/// Longest prefix of `line` that fits `width` together with `suffix`.
fn truncated(line: &ShapedLine, runs: &[TextRun], suffix: &str, font_size: Pixels, width: Pixels, window: &mut Window) -> ShapedLine {
    let text = line.text.clone();
    let mut suffix_runs: Vec<TextRun> = runs.last().cloned().into_iter().collect();
    if let Some(r) = suffix_runs.first_mut() {
        r.len = suffix.len();
        r.font.family = theme::ui_family().into();
    }
    let suffix_w = window.text_system().shape_line(suffix.to_string().into(), font_size, &suffix_runs, None).width;
    let mut cut = 0;
    for (i, ch) in text.char_indices() {
        let end = i + ch.len_utf8();
        if line.x_for_index(end) + suffix_w > width {
            break;
        }
        cut = end;
    }
    let mut out = vec![];
    let mut at = 0;
    for r in runs {
        if at >= cut {
            break;
        }
        let mut r = r.clone();
        r.len = r.len.min(cut - at);
        at += r.len;
        out.push(r);
    }
    out.extend(suffix_runs);
    let s: SharedString = format!("{}{}", &text[..cut], suffix).into();
    window.text_system().shape_line(s, font_size, &out, None)
}

impl Element for UiText {
    type RequestLayoutState = UiTextLayout;
    type PrepaintState = Option<ShapedLine>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        _: &mut App,
    ) -> (LayoutId, UiTextLayout) {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = style.line_height.to_pixels(font_size.into(), window.rem_size());
        let runs = if self.text.is_empty() { vec![] } else { self.runs(&style) };
        let ellipsis = style.text_overflow.as_ref().map(|(TextOverflow::Truncate(s) | TextOverflow::TruncateStart(s) | TextOverflow::TruncateMiddle(s))| s.clone());
        let full = Rc::new(RefCell::new(None));
        let (text, r, f, clip) = (self.text.clone(), runs.clone(), full.clone(), ellipsis.is_some());
        let id = window.request_measured_layout(Style::default(), move |known, avail, window, _| {
            let mut slot = f.borrow_mut();
            let line = slot.get_or_insert_with(|| window.text_system().shape_line(text.clone(), font_size, &r, None));
            let mut w = line.width.ceil();
            if clip {
                let limit = known.width.or(match avail.width {
                    AvailableSpace::Definite(x) => Some(x),
                    _ => None,
                });
                if let Some(l) = limit {
                    w = w.min(l);
                }
            }
            size(w, line_height)
        });
        (id, UiTextLayout { runs, font_size, line_height, ellipsis, full })
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut UiTextLayout,
        window: &mut Window,
        _: &mut App,
    ) -> Option<ShapedLine> {
        let full = layout.full.borrow().clone()?;
        match &layout.ellipsis {
            Some(suffix) if full.width > bounds.size.width + px(0.5) => {
                Some(truncated(&full, &layout.runs, suffix, layout.font_size, bounds.size.width, window))
            }
            _ => Some(full),
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut UiTextLayout,
        line: &mut Option<ShapedLine>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(line) = line {
            let _ = line.paint(bounds.origin, layout.line_height, TextAlign::Left, None, window, cx);
        }
    }
}

/// [`UiText`] that wraps: a paragraph keeping full-width punctuation at full
/// width, in the inherited text style.
pub fn ui_para(text: impl Into<SharedString>) -> UiPara {
    UiPara { text: text.into(), inner: None }
}

pub struct UiPara {
    text: SharedString,
    inner: Option<StyledText>,
}

impl Element for UiPara {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = window.text_style();
        let runs = ui_runs(&self.text, &style, &[], style.color);
        self.inner.insert(StyledText::new(self.text.clone()).with_runs(runs)).request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(t) = &mut self.inner {
            t.prepaint(id, inspector_id, bounds, layout, window, cx)
        }
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut (),
        state: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(t) = &mut self.inner {
            t.paint(id, inspector_id, bounds, layout, state, window, cx)
        }
    }
}

/// CSS `corner-shape: squircle` (superellipse |x|⁴ + |y|⁴ = 1 in each `r`
/// box): the outline of `b` inset by `i`, clockwise from the top edge.
fn squircle_points(b: Bounds<Pixels>, radius: f32, i: f32) -> Vec<Point<Pixels>> {
    let (x, y) = (f32::from(b.left()) + i, f32::from(b.top()) + i);
    let (w, h) = (f32::from(b.size.width) - 2. * i, f32::from(b.size.height) - 2. * i);
    let r = (radius - i).max(0.).min(w / 2.).min(h / 2.);
    const N: usize = 24;
    let mut out = Vec::with_capacity(4 * (N + 1));
    // Anchor centres with the directions to the corner's start and end edges.
    for (cx, cy, ax, ay, bx, by) in [(x + w - r, y + r, 0., -1., 1., 0.), (x + w - r, y + h - r, 1., 0., 0., 1.), (x + r, y + h - r, 0., 1., -1., 0.), (x + r, y + r, -1., 0., 0., -1.)] {
        for k in 0..=N {
            let t = std::f32::consts::FRAC_PI_2 * k as f32 / N as f32;
            let (u, v) = (t.cos().max(0.).sqrt() * r, t.sin().max(0.).sqrt() * r);
            out.push(point(px(cx + u * ax + v * bx), px(cy + u * ay + v * by)));
        }
    }
    out
}

pub(crate) fn squircle_path(b: Bounds<Pixels>, radius: f32, i: f32, mut pb: PathBuilder) -> Option<Path> {
    // lyon panics on a collapsed outline (a box not laid out yet).
    if f32::from(b.size.width) <= 2. * i + 1. || f32::from(b.size.height) <= 2. * i + 1. || radius <= i { return None }
    let pts = squircle_points(b, radius, i);
    pb.move_to(pts[0]);
    for p in &pts[1..] { pb.line_to(*p); }
    pb.close();
    pb.build().ok()
}

/// A squircle-cornered fill (see `squircle_points`).
pub fn squircle_surface(radius: f32, color: Hsla) -> Div {
    div().relative().child(canvas(|b, _, _| b, move |_, b, window, _| {
        if let Some(p) = squircle_path(b, radius, 0., PathBuilder::fill()) { window.paint_path(p, color); }
    }).absolute().inset_0())
}

/// A `width` line along the inside of the squircle edge.
pub fn squircle_outline(radius: f32, width: f32, color: Hsla) -> impl IntoElement {
    canvas(|b, _, _| b, move |_, b, window, _| {
        if let Some(p) = squircle_path(b, radius, width / 2., PathBuilder::stroke(px(width))) { window.paint_path(p, color); }
    }).absolute().inset_0()
}

/// A capsule filled to `fraction` from the left, its edge a moving wave (the
/// update pill's download progress). Redraws every frame while shown.
pub fn wave_fill(fraction: f32, color: Hsla) -> impl IntoElement {
    static START: std::sync::OnceLock<web_time::Instant> = std::sync::OnceLock::new();
    let t = START.get_or_init(web_time::Instant::now).elapsed().as_secs_f32();
    canvas(|bounds, _, _| bounds, move |_, b, window, _| {
        window.request_animation_frame();
        let f = fraction.clamp(0., 1.);
        if f <= 0. { return }
        let (x, y) = (f32::from(b.left()), f32::from(b.top()));
        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
        let r = h / 2.;
        // The capsule's left and right walls at height v (0 = top).
        let wall = |v: f32| (r * r - (v - r).powi(2)).max(0.).sqrt();
        // A 2pt swell, one crest per pill height, drifting downwards; flat when full.
        let amp = if f >= 1. { 0. } else { 2. };
        let edge = |v: f32| (x + f * w + amp * (std::f32::consts::TAU * v / h - t * 5.).sin()).clamp(x + r - wall(v), x + w - r + wall(v));
        const N: usize = 24;
        let mut path = PathBuilder::fill();
        path.move_to(point(px(edge(0.)), px(y)));
        for i in 1..=N { let v = h * i as f32 / N as f32; path.line_to(point(px(edge(v)), px(y + v))); }
        for i in (0..=N).rev() { let v = h * i as f32 / N as f32; path.line_to(point(px(x + r - wall(v)), px(y + v))); }
        path.close();
        if let Ok(path) = path.build() { window.paint_path(path, color); }
    }).absolute().inset_0()
}


/// A path as people read it: the home folder as "~".
pub(crate) fn tilde(p: &std::path::Path) -> String {
    match dirs::home_dir().and_then(|h| p.strip_prefix(&h).ok().map(|r| r.to_path_buf())) {
        Some(r) => format!("~/{}", r.display()),
        None => p.display().to_string(),
    }
}

/// The breadcrumbs' size in every page header (a document's, a folder's, 主页's): shadcn's
/// Breadcrumb, text-sm.
pub const CRUMB_SIZE: f32 = 14.;

/// shadcn's Breadcrumb (the user's call, 2026-11-25): the crumbs before the page, their separators
/// and the "…" in `muted-foreground` (BreadcrumbLink), `foreground` under the pointer; the current
/// page in `foreground` (BreadcrumbPage), the same weight.
pub fn crumb_ink() -> Hsla { crate::design::color::muted_foreground() }
pub fn crumb_page() -> Hsla { crate::design::color::foreground() }

/// A page header's icon button (search, new, more): `Button::ghost`, header-tall.
pub fn header_icon(id: &'static str, name: &'static str) -> crate::design::Button {
    crate::design::Button::ghost(id).icon(name).size(px(crate::app::HEADER_ACTIONS_H)).no_drag()
}

