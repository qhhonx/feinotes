//! 主页's folders: a glass folder with its documents tucked inside as paper,
//! and dragging a document card into one.
//!
//! The folder is painted, not laid out: the papers tilt and fan, which GPUI
//! elements cannot, so back plate, papers and the frosted front are paths in
//! two canvases (papers below, glass above), with the labels as ordinary
//! elements between and over them. Every motion is a spring
//! (`sidebar_drag::{Fade, Spring}`): hover lifts the papers out one after
//! another while the front leans back; a dragged card turns into a paper,
//! the folder opens its mouth under it, and a drop slides it in behind the
//! glass while the folder dips.
use crate::app::{RowMenu, Workspace};
use crate::settings::display_title;
use crate::sidebar_drag::{Fade, Spring};
use crate::store::Entry;
use crate::theme::{self, Theme, craft, ca};
use crate::ui::{TipExt, icon, ui_text};
use gpui::{prelude::FluentBuilder, *};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Instant};

/// The tab's share of a folder card's height: the folder's body is a document
/// card's size, the tab rises above it (`home.rs` lowers the cards beside it).
pub(crate) const TAB: f32 = 0.06;
/// Papers peeking out of a folder.
const PAPERS: usize = 3;
/// The carried paper: the size a paper has inside a folder, from the moment it
/// is picked up until it is let go (same proportions as a card).
const MINI_W: f32 = 110.;
const MINI_H: f32 = MINI_W * 264. / 236.;

#[derive(Default)]
pub(crate) struct HomeFolders {
    fx: HashMap<String, FolderFx>,
    hovered: Option<String>,
    pub(crate) drag: Option<HomeDrag>,
    /// Files dragged in from outside.
    pub(crate) file_drop: Option<FileDrop>,
    /// Last-frame window bounds of the folder and document cards, by entry id.
    bounds: Rc<RefCell<HashMap<String, Bounds<Pixels>>>>,
}

struct FolderFx {
    /// Hover: the papers rise, the front leans back.
    open: Fade,
    /// A card held over it: the papers part and the front drops further.
    mouth: Fade,
    /// Each paper's rise; the back ones are softer, so they follow the front.
    papers: [Fade; PAPERS],
    /// Kicked on press and on a drop: the front dips and springs back.
    dip: Fade,
    /// A dropped document sliding in (card-local x, y, w, h).
    arrival: Option<(String, Spring<4>)>,
    /// The paper kept in front: the last one dropped in.
    front: Option<String>,
    last: Option<Instant>,
}

impl FolderFx {
    fn new() -> Self {
        Self {
            open: Fade::new(0., 260., 21.),
            mouth: Fade::new(0., 300., 23.),
            papers: [Fade::new(0., 320., 21.), Fade::new(0., 230., 18.), Fade::new(0., 170., 15.5)],
            dip: Fade::new(0., 520., 15.),
            arrival: None,
            front: None,
            last: None,
        }
    }
}

pub(crate) struct HomeDrag {
    id: String,
    start: Point<Pixels>,
    at: Point<Pixels>,
    started: bool,
    /// Where on the card it was picked up (0–1), kept on the carried paper.
    grab: (f32, f32),
    /// The card's bounds when picked up; a cancelled drag flies back there.
    home: Bounds<Pixels>,
    /// The carried paper in window coordinates (x, y, w, h).
    rect: Spring<4>,
    /// The folder under the pointer: a drop puts the document inside.
    over: Option<String>,
    returning: bool,
    last: Option<Instant>,
}

/// What lies on a paper: drawn lines, a real title over drawn lines, or real content.
#[derive(Clone, Copy, PartialEq)]
enum Face { Sketch, Titled, Content }

/// One paper's pose in card coordinates: centre, size and tilt (radians).
#[derive(Clone, Copy)]
struct Paper { cx: f32, cy: f32, w: f32, h: f32, rot: f32, seed: u64 }

/// Everything both canvases need for one frame.
#[derive(Clone)]
struct Pose {
    w: f32,
    h: f32,
    lift: f32,
    open: f32,
    /// Front's top edge: left (higher) and right, and how far its top corners reach in
    /// (negative: out, the flap tipping toward the viewer as it opens).
    yl: f32,
    yr: f32,
    inset: f32,
    papers: Vec<Paper>,
}

fn seed(id: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = rustc_hash::FxHasher::default();
    id.hash(&mut h);
    h.finish()
}

/// A rounded rectangle turned by `rot` about its centre (card-local `c`, painted at `o`).
fn rounded(o: Point<Pixels>, c: (f32, f32), w: f32, h: f32, r: f32, rot: f32) -> Option<Path<Pixels>> {
    let mut pb = PathBuilder::fill();
    append_rounded(&mut pb, o, c, w, h, r, rot);
    pb.build().ok()
}

fn append_rounded(pb: &mut PathBuilder, o: Point<Pixels>, c: (f32, f32), w: f32, h: f32, r: f32, rot: f32) {
    let (s, k) = rot.sin_cos();
    let r = r.min(w / 2.).min(h / 2.);
    let mut first = true;
    for (ox, oy, start) in [(w / 2. - r, -h / 2. + r, -90f32), (w / 2. - r, h / 2. - r, 0.), (-w / 2. + r, h / 2. - r, 90.), (-w / 2. + r, -h / 2. + r, 180.)] {
        for i in 0..=6 {
            let a = (start + 15. * i as f32).to_radians();
            let (x, y) = (ox + r * a.cos(), oy + r * a.sin());
            let p = o + point(px(c.0 + x * k - y * s), px(c.1 + x * s + y * k));
            if first { pb.move_to(p); first = false; } else { pb.line_to(p); }
        }
    }
    pb.close();
}

impl Paper {
    /// A point on the paper, from its own top-left corner.
    fn at(&self, x: f32, y: f32) -> (f32, f32) {
        let (s, k) = self.rot.sin_cos();
        let (lx, ly) = (x - self.w / 2., y - self.h / 2.);
        (self.cx + lx * k - ly * s, self.cy + lx * s + ly * k)
    }

    /// The sheet, its hairline edge, a soft shadow and, unless real content
    /// lies on it, lines of "text" (below a real title when `titled`).
    fn paint(&self, o: Point<Pixels>, face: Face, window: &mut Window) {
        let b = Bounds::new(o + point(px(self.cx - self.w / 2.), px(self.cy - self.h / 2.)), size(px(self.w), px(self.h)));
        window.paint_shadows(b, Corners::all(px(10.)), &[BoxShadow { color: theme::rgbaf(0, 0, 0, if theme::dark() { 0.3 } else { 0.07 }), offset: point(px(0.), px(3.)), blur_radius: px(10.), spread_radius: px(0.) }]);
        if let Some(p) = rounded(o, (self.cx, self.cy), self.w + 1., self.h + 1., 10.5, self.rot) { window.paint_path(p, ca(craft::N1000, 0.08)); }
        if let Some(p) = rounded(o, (self.cx, self.cy), self.w, self.h, 10., self.rot) { window.paint_path(p, theme::raised(Theme::bg_body())); }
        let mut rng = self.seed;
        let mut next = || { rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17; (rng % 1000) as f32 / 1000. };
        if face == Face::Content { return; }
        let pad = 12.;
        let mut y = if face == Face::Titled { 36. } else {
            // An untitled paper still carries a heading.
            let len = (self.w - 2. * pad) * (0.42 + 0.3 * next());
            if let Some(p) = rounded(o, self.at(pad + len / 2., 16.), len, 4., 2., self.rot) { window.paint_path(p, ca(craft::N900, 0.55)); }
            30.
        };
        let mut pb = PathBuilder::fill();
        let mut has_lines = false;
        while y < self.h - 14. {
            let full = self.w - 2. * pad;
            let len = if next() < 0.2 { full * (0.3 + 0.3 * next()) } else { full * (0.72 + 0.28 * next()) };
            append_rounded(&mut pb, o, self.at(pad + len / 2., y), len, 3., 1.5, self.rot);
            has_lines = true;
            y += 8.5;
        }
        if has_lines && let Ok(p) = pb.build() { window.paint_path(p, ca(craft::N1000, 0.10)); }
    }
}

/// The folder's back: a plate with a tab on its top-left.
fn back_plate(o: Point<Pixels>, p: &Pose, window: &mut Window) {
    let (w, h, r) = (p.w, p.h, 18.);
    let top = TAB * h + p.lift;
    let tab_top = p.lift;
    let tab = 0.40 * w;
    let pt = |x: f32, y: f32| o + point(px(x), px(y));
    let mut pb = PathBuilder::fill();
    pb.move_to(pt(0., h - r + p.lift));
    pb.line_to(pt(0., tab_top + r));
    pb.curve_to(pt(r, tab_top), pt(0., tab_top));
    pb.line_to(pt(tab - 22., tab_top));
    pb.cubic_bezier_to(pt(tab + 14., top), pt(tab - 6., tab_top), pt(tab - 2., top));
    pb.line_to(pt(w - r, top));
    pb.curve_to(pt(w, top + r), pt(w, top));
    pb.line_to(pt(w, h - r + p.lift));
    pb.curve_to(pt(w - r, h + p.lift), pt(w, h + p.lift));
    pb.line_to(pt(r, h + p.lift));
    pb.curve_to(pt(0., h - r + p.lift), pt(0., h + p.lift));
    pb.close();
    if let Ok(path) = pb.build() {
        window.paint_path(path, linear_gradient(180., linear_color_stop(ca(craft::N1000, 0.11), 0.), linear_color_stop(ca(craft::N1000, 0.07), 1.)));
    }
}

/// The frosted front: a sheet whose top steps down from a raised left part,
/// milky at the top and nearly opaque where the name sits, with a lit edge.
fn glass(o: Point<Pixels>, p: &Pose, window: &mut Window) {
    let (w, h, r) = (p.w, p.h + p.lift, 18.);
    let (yl, yr, i) = (p.yl + p.lift, p.yr + p.lift, p.inset);
    let step = 0.44 * w;
    let pt = |x: f32, y: f32| o + point(px(x), px(y));
    let edge = |pb: &mut PathBuilder| {
        pb.move_to(pt(i, yl + r));
        pb.curve_to(pt(i + r, yl), pt(i, yl));
        pb.line_to(pt(step - 16., yl));
        pb.cubic_bezier_to(pt(step + 26., yr), pt(step + 6., yl), pt(step + 6., yr));
        pb.line_to(pt(w - i - r, yr));
        pb.curve_to(pt(w - i, yr + r), pt(w - i, yr));
    };
    // The whole folder's shadow sits under the front.
    let lift = -p.lift / 4.;
    window.paint_shadows(Bounds::new(pt(6., yr + 10.), size(px(w - 12.), px(h - yr - 10.))), Corners::all(px(r)),
        &[BoxShadow { color: theme::rgbaf(0, 0, 0, if theme::dark() { 0.35 } else { 0.06 + 0.04 * lift }), offset: point(px(0.), px(8. + 6. * lift)), blur_radius: px(22. + 10. * lift), spread_radius: px(-2.) }]);
    let mut pb = PathBuilder::fill();
    edge(&mut pb);
    pb.line_to(pt(w, h - r));
    pb.curve_to(pt(w - r, h), pt(w, h));
    pb.line_to(pt(r, h));
    pb.curve_to(pt(0., h - r), pt(0., h));
    pb.close();
    let face = theme::raised(Theme::bg_body());
    if let Ok(path) = pb.build() {
        // Milky where the papers show through, all but opaque by the name; a faint
        // cool tint keeps the glass apart from the white papers behind it.
        window.paint_path(path.clone(), linear_gradient(180., linear_color_stop(face.opacity(0.78), 0.), linear_color_stop(face.opacity(0.97), 0.42)));
        window.paint_path(path, linear_gradient(180., linear_color_stop(ca(craft::N1000, 0.012), 0.), linear_color_stop(ca(craft::N1000, 0.035), 1.)));
    }
    let mut hair = PathBuilder::stroke(px(1.));
    edge(&mut hair);
    hair.line_to(pt(w, h - r));
    hair.curve_to(pt(w - r, h), pt(w, h));
    hair.line_to(pt(r, h));
    hair.curve_to(pt(0., h - r), pt(0., h));
    hair.close();
    if let Ok(path) = hair.build() { window.paint_path(path, ca(craft::N1000, 0.07)); }
    // The lit top edge: brighter as the front leans into the light.
    let mut lit = PathBuilder::stroke(px(1.5));
    edge(&mut lit);
    if let Ok(path) = lit.build() { window.paint_path(path, theme::rgbaf(255, 255, 255, if theme::dark() { 0.18 } else { 0.85 + 0.15 * p.open.clamp(0., 1.) })); }
}

impl Workspace {
    fn folder_papers(&self, id: &str, front: Option<&String>) -> (Vec<Entry>, usize) {
        let n = self.entries.iter().filter(|e| e.meta.parent.as_deref() == Some(id)).count();
        // Papers stand for documents. A folder drawn as a paper reads as a note
        // with the folder's name on it, so only documents are painted — but a
        // folder usually holds folders for a while before a document appears,
        // and its documents usually sit deeper, so look all the way down and
        // paint the newest ones (#123).
        let mut kids: Vec<Entry> = vec![];
        self.documents_below(id, &mut kids, 0);
        kids.sort_by(|a, b| {
            let lead = |e: &Entry| front.is_some_and(|f| *f == e.id);
            lead(b).cmp(&lead(a)).then(b.meta.opened.max(b.meta.modified).cmp(&a.meta.opened.max(a.meta.modified)))
        });
        kids.truncate(PAPERS);
        (kids, n)
    }

    /// Every document anywhere under `id`, newest first once sorted by the caller.
    fn documents_below(&self, id: &str, out: &mut Vec<Entry>, depth: usize) {
        // A folder that contains itself would otherwise recurse forever.
        if depth > 32 {
            return;
        }
        for e in self.entries.iter().filter(|e| e.meta.parent.as_deref() == Some(id)) {
            if e.meta.folder.is_some() {
                self.documents_below(&e.id, out, depth + 1);
            } else {
                out.push(e.clone());
            }
        }
    }

    /// `w`: the grid's column width, this frame's (the recorded bounds are last
    /// frame's, stale for a frame after the sidebar opens or closes).
    pub(crate) fn render_folder_card(&mut self, e: &Entry, w: f32, card_h: f32, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let id = e.id.clone();
        let h = card_h;
        let hot = self.home.folders.hovered.as_ref() == Some(&id) && self.home.folders.drag.is_none();
        let over = self.home.folders.drag.as_ref().is_some_and(|d| d.started && !d.returning && d.over.as_ref() == Some(&id)) || self.file_drop_over(&id);
        let front = self.home.folders.fx.get(&id).and_then(|f| f.front.clone());
        let (kids, count) = self.folder_papers(&id, front.as_ref());

        // ---- Springs.
        let fx = self.home.folders.fx.entry(id.clone()).or_insert_with(FolderFx::new);
        let now = Instant::now();
        let dt = fx.last.replace(now).map_or(1. / 120., |t| now.duration_since(t).as_secs_f32().min(0.032));
        fx.open.to = if hot || over { 1. } else { 0. };
        fx.mouth.to = if over { 1. } else { 0. };
        for p in &mut fx.papers { p.to = fx.open.to + 0.35 * fx.mouth.to; }
        let mut moving = fx.open.step(dt) | fx.mouth.step(dt) | fx.dip.step(dt);
        for p in &mut fx.papers { moving |= p.step(dt); }
        let (o, m, dip) = (fx.open.p, fx.mouth.p, fx.dip.p);
        let lift = -5. * o - 2. * m + 3. * dip;
        let pose_of = |i: usize, r: f32| -> Paper {
            // (dx, top, tilt°) at rest; how far hover moves them; how far a held card parts them.
            let (rest, rise, part) = [
                // Kept inside the card: tilted corners stay clear of the neighbours.
                ((0.0, 0.16, 0.0), (0.0, -0.07, 0.0), (0.0, -0.03, 0.0)),
                ((0.03, 0.135, 2.5), (0.03, -0.055, 2.0), (0.01, -0.02, 1.5)),
                ((-0.035, 0.115, -3.0), (-0.03, -0.05, -2.5), (-0.01, -0.02, -1.5)),
            ][i];
            let (pw, ph) = (0.80 * w, 0.64 * h);
            let dx = (rest.0 + rise.0 * r + part.0 * m) * w;
            let top = (rest.1 + rise.1 * r + part.1 * m) * h + lift;
            Paper { cx: w / 2. + dx, cy: top + ph / 2., w: pw, h: ph, rot: (rest.2 + rise.2 * r + part.2 * m).to_radians(), seed: 0 }
        };
        let mut papers: Vec<Paper> = kids.iter().enumerate().map(|(i, k)| Paper { seed: seed(&k.id), ..pose_of(i, fx.papers[i].p) }).collect();
        if let Some((aid, s)) = &mut fx.arrival {
            let t = pose_of(0, fx.papers[0].p);
            s.target = [t.cx - t.w / 2., t.cy - t.h / 2., t.w, t.h];
            let still = !s.step(dt);
            moving |= !still;
            if let Some(first) = papers.first_mut().filter(|_| kids.first().is_some_and(|k| &k.id == aid)) {
                let [x, y, pw, ph] = s.p;
                *first = Paper { cx: x + pw / 2., cy: y + ph / 2., w: pw, h: ph, rot: 0., seed: first.seed };
            }
            if still { fx.arrival = None; }
        }
        if moving { window.request_animation_frame(); }
        let lean = 0.03 * h * o + 0.045 * h * m + 0.05 * h * dip;
        let pose = Pose { w, h, lift, open: o, yl: 0.49 * h + lean, yr: 0.54 * h + lean, inset: -(3. * o + 7. * m), papers };

        // ---- Elements: back canvas, the front paper's title, the glass, the labels.
        let record = self.home.folders.bounds.clone();
        let rid = id.clone();
        // The front paper is never tilted, so it carries the document's real first page.
        let front_face = if kids.first().is_some_and(|k| k.meta.folder.is_none()) { Face::Content } else { Face::Titled };
        let back = { let pose = pose.clone(); canvas(move |b, _, _| { record.borrow_mut().insert(rid.clone(), b); }, move |b, _, window, _| {
            back_plate(b.origin, &pose, window);
            for (i, p) in pose.papers.iter().enumerate().rev() { p.paint(b.origin, if i == 0 { front_face } else { Face::Sketch }, window); }
        }).absolute().inset_0() };
        let title = pose.papers.first().zip(kids.first()).map(|(p, k)| if front_face == Face::Content {
            div().absolute().left(px(p.cx - p.w / 2.)).top(px(p.cy - p.h / 2.)).w(px(p.w)).h(px(p.h)).rounded(px(10.)).overflow_hidden()
                .child(self.thumb_page(k, 12.))
        } else {
            div().absolute().left(px(p.cx - p.w / 2. + 12.)).top(px(p.cy - p.h / 2. + 10.)).w(px(p.w - 24.))
                .flex().items_center().gap(px(4.)).overflow_hidden()
                .text_size(px(11.)).line_height(px(16.)).font_weight(FontWeight::SEMIBOLD).text_color(Theme::text())
                .when_some(k.icon.clone(), |d, em| d.child(div().flex_none().child(em)))
                .when(k.meta.folder.is_some(), |d| d.child(icon("folder", 12., Theme::icon())))
                .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui_text(display_title(&k.title))))
        });
        let front_glass = { let pose = pose.clone(); canvas(|_, _, _| {}, move |b, _, window, _| glass(b.origin, &pose, window)).absolute().inset_0() };
        let menu_on = self.row_menu.as_ref().is_some_and(|r| r.id == id);
        // The buttons come with hover only, not under a carried card.
        let reveal = if menu_on { 1. } else { (o - m).clamp(0., 1.) };
        let (open_id, menu_id, new_id) = (id.clone(), id.clone(), id.clone());
        let hover_id = id.clone();
        let label = div().absolute().left(px(16.)).right(px(14.)).bottom(px(14. - lift)).flex().items_end().gap(px(8.))
            .child(div().flex_1().min_w_0().flex().flex_col()
                .child(div().text_size(px(15.)).line_height(px(22.)).font_weight(FontWeight::SEMIBOLD).text_color(Theme::text())
                    .overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui_text(display_title(&e.title))))
                .child(div().text_size(px(12.)).line_height(px(18.)).text_color(Theme::text_caption())
                    .child(if count == 1 { crate::settings::tr("1 项", "1 item").into() } else { crate::localized_format!("{} 项", "{} items", count) })))
            .when(self.batch.is_none(), |d| d
                .child(div().id(SharedString::from(format!("home-fnew-{id}"))).flex_none().size(px(28.)).rounded_full().flex().items_center().justify_center()
                    .bg(theme::raised(Theme::bg_body())).shadow(theme::shadow_button()).opacity(reveal).cursor_pointer()
                    .hover(|s| s.bg(Theme::fill_hover_strong()))
                    .tip(crate::settings::tr("新建文档", "New document"), None)
                    .child(icon("plus", 15., Theme::icon()).with_transformation(Transformation::scale(size(0.7 + 0.3 * reveal, 0.7 + 0.3 * reveal))))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |w, _, window, cx| { cx.stop_propagation(); w.new_child(&new_id, window, cx); })))
                .child(self.home_more(e, format!("home-fmore-{id}"), cx).size(px(28.)).rounded_full().bg(theme::raised(Theme::bg_body())).shadow(theme::shadow_button()).opacity(reveal)));
        div().id(SharedString::from(format!("home-folder-{id}"))).relative().w_full().min_w_0().h(px(h)).cursor_pointer()
            .when(self.batch.is_some(), |d| d.opacity(0.45))
            .on_hover(cx.listener(move |w, on: &bool, _, cx| {
                if *on { w.home.folders.hovered = Some(hover_id.clone()); }
                else if w.home.folders.hovered.as_ref() == Some(&hover_id) { w.home.folders.hovered = None; }
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Left, cx.listener({ let id = id.clone(); move |w, _, _, cx| {
                // A press sinks the front a little; the release springs it back.
                if let Some(f) = w.home.folders.fx.get_mut(&id) { f.dip.v += 2.2; }
                cx.notify();
            }}))
            .when(self.batch.is_none(), |d| d
                .on_click(cx.listener(move |w, _, window, cx| { w.try_open(&open_id, window, cx); }))
                .on_mouse_down(MouseButton::Right, cx.listener(move |w, ev: &MouseDownEvent, _, cx| {
                    w.row_menu = Some(RowMenu { id: menu_id.clone(), pos: ev.position, corner: Corner::TopLeft, sub: false, batch: false });
                    cx.notify();
                })))
            .child(back).children(title).child(front_glass).child(label)
            .into_any_element()
    }

    /// The card's bounds are kept for picking it up (and for the flight back).
    pub(crate) fn record_home_card(&self, id: &str) -> impl IntoElement {
        let record = self.home.folders.bounds.clone();
        let id = id.to_string();
        canvas(move |b, _, _| { record.borrow_mut().insert(id.clone(), b); }, |_, _, _, _| {}).absolute().inset_0()
    }

    /// A document card picked up on Home or a folder page: it is carried as a
    /// small page skeleton and can be dropped into a folder (cards are not
    /// reordered by dragging).
    pub(crate) fn arm_home_drag(&mut self, id: &str, at: Point<Pixels>, _cx: &mut Context<Self>) {
        let Some(home) = self.home.folders.bounds.borrow().get(id).copied() else { return };
        let grab = (((at.x - home.left()) / home.size.width).clamp(0., 1.), ((at.y - home.top()) / home.size.height).clamp(0., 1.));
        let r = [f32::from(home.left()), f32::from(home.top()), f32::from(home.size.width), f32::from(home.size.height)];
        self.home.folders.drag = Some(HomeDrag { id: id.to_string(), start: at, at, started: false, grab, home, rect: Spring::new(r), over: None, returning: false, last: None });
    }

    /// Before the drag starts the pointer is tracked by the home body.
    pub(crate) fn track_home_press(&mut self, ev: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(d) = self.home.folders.drag.as_mut().filter(|d| !d.started) else { return };
        if ev.pressed_button != Some(MouseButton::Left) { self.home.folders.drag = None; return; }
        d.at = ev.position;
        if f32::from(d.at.x - d.start.x).hypot(f32::from(d.at.y - d.start.y)) >= 5. {
            d.started = true;
            self.row_menu = None;
            cx.notify();
        }
    }

    pub(crate) fn home_dragging(&self, id: &str) -> bool {
        self.home.folders.drag.as_ref().is_some_and(|d| d.started && d.id == id)
    }

    fn move_home_drag(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        if !self.home.folders.drag.as_ref().is_some_and(|d| d.started && !d.returning) { return; }
        // Only folders on the page now: bounds linger for cards of pages left behind.
        let shown = if self.view == crate::app::View::Folder { self.level_entries(self.folder_current.as_deref()) } else { self.home_entries() };
        let over = shown.iter().filter(|e| e.meta.folder.is_some())
            .find(|e| self.home.folders.bounds.borrow().get(&e.id).is_some_and(|b| b.contains(&at)))
            .map(|e| e.id.clone());
        let Some(d) = self.home.folders.drag.as_mut() else { return };
        d.at = at;
        if over != d.over {
            // The folder notices: a small dip as its mouth opens.
            if let Some(f) = over.as_ref().and_then(|k| self.home.folders.fx.get_mut(k)) { f.dip.v -= 1.6; }
            d.over = over;
        }
        cx.notify();
    }

    pub(crate) fn has_home_drag(&self) -> bool { self.home.folders.drag.is_some() }

    pub(crate) fn cancel_home_drag(&mut self, cx: &mut Context<Self>) {
        self.home.folders.drag = None;
        cx.notify();
    }

    fn end_home_drag(&mut self, cx: &mut Context<Self>) {
        let Some(d) = self.home.folders.drag.as_mut().filter(|d| d.started && !d.returning) else { self.home.folders.drag = None; return };
        let target = d.over.clone().and_then(|f| self.home.folders.bounds.borrow().get(&f).copied().map(|b| (f, b)));
        let Some((folder, b)) = target else {
            // Not over a folder: the paper flies back and becomes the card again.
            d.returning = true;
            d.rect.target = [f32::from(d.home.left()), f32::from(d.home.top()), f32::from(d.home.size.width), f32::from(d.home.size.height)];
            cx.notify();
            return;
        };
        let id = d.id.clone();
        // Hand the paper to the folder where it is; it slides in behind the glass.
        let [x, y, w, h] = d.rect.p;
        let local = [x - f32::from(b.left()), y - f32::from(b.top()), w, h];
        self.home.folders.drag = None;
        let fx = self.home.folders.fx.entry(folder.clone()).or_insert_with(FolderFx::new);
        fx.arrival = Some((id.clone(), Spring::new(local)));
        fx.front = Some(id.clone());
        fx.dip.v += 5.5;
        self.move_into_folder(&id, &folder, cx);
    }

    pub(crate) fn render_home_drag_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !matches!(self.view, crate::app::View::Home | crate::app::View::Folder) { self.home.folders.drag = None; return None; }
        let folder_bounds = self.home.folders.drag.as_ref().and_then(|d| d.over.clone()).and_then(|f| self.home.folders.bounds.borrow().get(&f).copied());
        let d = self.home.folders.drag.as_mut().filter(|d| d.started)?;
        let now = Instant::now();
        let dt = d.last.replace(now).map_or(1. / 120., |t| now.duration_since(t).as_secs_f32().min(0.032));
        if !d.returning {
            // The size a paper has inside a folder, from pickup to drop.
            let (mut x, mut y) = (f32::from(d.at.x) - d.grab.0 * MINI_W, f32::from(d.at.y) - d.grab.1 * MINI_H);
            if let Some(b) = folder_bounds {
                // Drawn toward the folder's mouth, held just above the glass.
                let (fx, fy) = (f32::from(b.center().x) - MINI_W / 2., f32::from(b.top()) + 0.56 * f32::from(b.size.height) - MINI_H);
                x += (fx - x) * 0.45; y += (fy - y) * 0.45;
            }
            d.rect.target = [x, y, MINI_W, MINI_H];
        }
        let moving = d.rect.step(dt);
        if d.returning && !moving { self.home.folders.drag = None; cx.notify(); return None; }
        window.request_animation_frame();
        let [x, y, w, h] = d.rect.p;
        let e = self.entries.iter().find(|e| e.id == d.id)?.clone();
        // The carried card is a page's skeleton (a heading bar and lines of text),
        // painted straight onto the GPU: nothing is laid out while it moves.
        let paper = Paper { cx: w / 2., cy: h / 2., w, h, rot: 0., seed: seed(&e.id) };
        let lines = canvas(|_, _, _| {}, move |b, _, window, _| paper.paint(b.origin, Face::Sketch, window)).absolute().inset_0();
        let layer = div().id("home-drag-overlay").absolute().inset_0().occlude().cursor(CursorStyle::ClosedHand)
            .on_mouse_move(cx.listener(|w, ev: &MouseMoveEvent, _, cx| {
                if ev.pressed_button != Some(MouseButton::Left) { w.cancel_home_drag(cx); return; }
                w.move_home_drag(ev.position, cx);
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|w, ev: &MouseUpEvent, window, cx| {
                let p = ev.position;
                let size = window.viewport_size();
                if p.x < px(0.) || p.y < px(0.) || p.x >= size.width || p.y >= size.height { w.cancel_home_drag(cx); return; }
                w.move_home_drag(p, cx);
                w.end_home_drag(cx);
            }))
            .child(div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h))
                .shadow(theme::shadow_panel()).rounded(px(10.))
                .child(lines));
        Some(deferred(layer).with_priority(5).into_any_element())
    }
}

// ---- Files dragged in from the Finder

/// A drag of files from outside (the Finder) over 主页 or a folder's page:
/// the list dims behind a receiving tray, a glass folder whose papers (one
/// per file, up to three) rise out of it, follow the pointer and part as it
/// comes near; let go, they slide in, the folder dips, and the documents are
/// imported where it says. Held over a folder card, that folder opens its
/// mouth instead and the tray steps back.
pub(crate) struct FileDrop {
    paths: Vec<std::path::PathBuf>,
    md: usize,
    xml: usize,
    dirs: usize,
    other: usize,
    at: Point<Pixels>,
    /// The folder card under the pointer.
    over: Option<String>,
    /// The pane's bounds last frame: anywhere in it takes the drop.
    zone: Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// Where it was let go, which the ripple spreads from.
    drop_at: Point<Pixels>,
    inside: bool,
    show: Fade,
    veil: Fade,
    arm: Fade,
    dip: Fade,
    land: Fade,
    shake: Fade,
    rise: [Fade; PAPERS],
    /// The pointer's place over the tray (−1…1 each way), which the papers lean toward.
    tilt: Spring<2>,
    /// Let go: where the files went (None: the top of 我的文档).
    landed: Option<Option<String>>,
    leaving: bool,
    born: Instant,
    last: Option<Instant>,
}

/// Tray illustration: the glass folder's box.
const ART_W: f32 = 196.;
const ART_H: f32 = 168.;

impl FileDrop {
    fn new(paths: Vec<std::path::PathBuf>) -> Self {
        let (mut md, mut xml, mut dirs, mut other) = (0, 0, 0, 0);
        for p in &paths {
            if p.is_dir() { dirs += 1; }
            else if crate::local_files::is_markdown_path(p) { md += 1; }
            else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xml")) { xml += 1; }
            else { other += 1; }
        }
        Self {
            paths, md, xml, dirs, other, at: point(px(0.), px(0.)), over: None, zone: Default::default(), inside: false, drop_at: point(px(0.), px(0.)),
            show: Fade::new(0., 240., 21.), veil: Fade::new(0., 160., 25.), arm: Fade::new(0., 300., 23.),
            dip: Fade::new(0., 520., 15.), land: Fade::new(0., 150., 21.), shake: Fade::new(0., 700., 9.),
            // Each paper surfaces a little after the one in front.
            rise: [Fade::new(0., 320., 21.), Fade::new(0., 210., 17.), Fade::new(0., 140., 14.)],
            tilt: Spring::new([0., 0.]), landed: None, leaving: false, born: Instant::now(), last: None,
        }
    }

    fn importable(&self) -> usize { self.md + self.xml + self.dirs }

    /// The pointer went somewhere else that takes the drop (the sidebar).
    pub(crate) fn leave(&mut self) { if self.landed.is_none() { self.leaving = true; } }

    /// "2 个 Markdown 文件 · 1 个文件夹".
    fn summary(&self) -> String {
        let mut parts = vec![];
        if self.md > 0 { parts.push(crate::localized_format!("{} 个 Markdown 文件", "{} Markdown files", self.md)); }
        if self.xml > 0 { parts.push(crate::localized_format!("{} 个 DocxXML 文件", "{} DocxXML files", self.xml)); }
        if self.dirs > 0 { parts.push(crate::localized_format!("{} 个文件夹", "{} folders", self.dirs)); }
        if self.other > 0 && !parts.is_empty() { parts.push(crate::localized_format!("{} 个不支持的文件会被跳过", "{} unsupported skipped", self.other)); }
        parts.join(" · ")
    }

    /// The front paper's tag: what kind of file leads.
    fn tag(&self) -> Option<&'static str> {
        if self.md > 0 { Some("MD") } else if self.xml > 0 { Some("XML") } else { None }
    }
}

impl Workspace {
    /// Files dragged over 主页 or a folder's page (the pointer, the folder
    /// card under it, whether it is over the tray).
    pub(crate) fn file_drag_move(&mut self, ev: &DragMoveEvent<ExternalPaths>, cx: &mut Context<Self>) {
        // Every listener hears every move: off the page (over the sidebar) it steps back.
        if !ev.bounds.contains(&ev.event.position) {
            if let Some(d) = self.home.folders.file_drop.as_mut() { d.leave(); cx.notify(); }
            return;
        }
        let paths = ev.drag(cx).paths().to_vec();
        self.file_drag_at(ev.event.position, paths, cx);
    }

    /// `file_drag_move` from a position and the dragged paths (scripted runs too).
    pub(crate) fn file_drag_at(&mut self, at: Point<Pixels>, paths: Vec<std::path::PathBuf>, cx: &mut Context<Self>) {
        if !matches!(self.view, crate::app::View::Home | crate::app::View::Folder) { return; }
        let fresh = self.home.folders.file_drop.as_ref().is_none_or(|d| d.landed.is_some());
        if fresh { self.home.folders.file_drop = Some(FileDrop::new(paths)); }
        let shown = if self.view == crate::app::View::Folder { self.level_entries(self.folder_current.as_deref()) } else { self.home_entries() };
        let over = shown.iter().filter(|e| e.meta.folder.is_some())
            .find(|e| self.home.folders.bounds.borrow().get(&e.id).is_some_and(|b| b.contains(&at)))
            .map(|e| e.id.clone());
        let Some(d) = self.home.folders.file_drop.as_mut() else { return };
        d.leaving = false;
        if over != d.over {
            // The folder notices, as it does for a dragged card.
            if let Some(f) = over.as_ref().and_then(|k| self.home.folders.fx.get_mut(k)) { f.dip.v -= 1.6; }
            d.over = over;
        }
        d.at = at;
        d.inside = d.zone.get().is_some_and(|z| z.contains(&at));
        cx.notify();
    }

    /// Let go over the list: import into the folder under the pointer, the
    /// folder page shown, or the top of 我的文档; nothing to import, a shake.
    pub(crate) fn file_dropped(&mut self, paths: &ExternalPaths, window: &mut Window, cx: &mut Context<Self>) {
        self.file_drop_paths(paths.paths().to_vec(), window, cx);
    }

    pub(crate) fn file_drop_paths(&mut self, paths: Vec<std::path::PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.view, crate::app::View::Home | crate::app::View::Folder) { return; }
        if self.home.folders.file_drop.is_none() { self.home.folders.file_drop = Some(FileDrop::new(paths)); }
        let page = if self.view == crate::app::View::Folder { self.folder_current.clone() } else { None };
        let Some(d) = self.home.folders.file_drop.as_mut() else { return };
        if d.importable() == 0 {
            d.shake.v += 34.;
            d.leaving = true;
            cx.notify();
            return;
        }
        let target = d.over.clone().or(page);
        d.landed = Some(target.clone());
        d.drop_at = d.at;
        d.land.to = 1.;
        d.dip.v += 5.5;
        if let Some(f) = d.over.as_ref().and_then(|k| self.home.folders.fx.get_mut(k)) { f.dip.v += 5.5; }
        let (dirs, files): (Vec<_>, Vec<_>) = d.paths.iter().cloned().partition(|p| p.is_dir());
        let files: Vec<_> = files.into_iter().filter(|p| crate::local_files::is_markdown_path(p) || p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xml"))).collect();
        if !files.is_empty() { self.import_paths(&files, target.clone(), false, window, cx); }
        for dir in &dirs { self.import_folder(dir, target.clone(), false, window, cx); }
        cx.notify();
    }

    pub(crate) fn file_drop_over(&self, id: &str) -> bool {
        self.home.folders.file_drop.as_ref().is_some_and(|d| d.landed.is_none() && !d.leaving && d.importable() > 0 && d.over.as_deref() == Some(id))
    }

    /// The veil and the tray, over the page while files are dragged in.
    pub(crate) fn render_file_drop(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !matches!(self.view, crate::app::View::Home | crate::app::View::Folder) { self.home.folders.file_drop = None; return None; }
        let dragging = cx.has_active_drag() || crate::devtools::file_drag();
        let target_title = |w: &Self, id: Option<&String>| id.and_then(|id| w.entries.iter().find(|e| &e.id == id)).map(|e| display_title(&e.title))
            .unwrap_or_else(|| crate::settings::tr("我的文档", "My documents").to_string());
        let page = if self.view == crate::app::View::Folder { self.folder_current.clone() } else { None };
        let d = self.home.folders.file_drop.as_ref()?;
        let place = target_title(self, d.landed.clone().flatten().as_ref().or(d.over.as_ref()).or(page.as_ref()));
        let d = self.home.folders.file_drop.as_mut()?;
        // The drag went away without a drop here (left the window, or let go elsewhere).
        if !dragging && d.landed.is_none() { d.leaving = true; }
        let now = Instant::now();
        let dt = d.last.replace(now).map_or(1. / 120., |t| now.duration_since(t).as_secs_f32().min(0.032));
        let reject = d.importable() == 0;
        // Over a folder card (and after a drop into one) the folder answers; the tray steps back.
        let over = d.over.is_some() && !reject && d.landed.as_ref().is_none_or(|t| t.is_some() && *t == d.over);
        d.show.to = if d.leaving { 0. } else { 1. };
        d.veil.to = if d.leaving { 0. } else if over { 0.12 } else { 1. };
        d.arm.to = if !reject && !d.leaving && (d.inside || d.landed.is_some()) { 1. } else { 0. };
        for (i, r) in d.rise.iter_mut().enumerate() { r.to = if d.leaving && d.landed.is_none() { 0. } else { 1. - 0.12 * i as f32 }; }
        let zone = d.zone.get();
        d.tilt.target = match zone.filter(|_| d.inside && d.landed.is_none()) {
            Some(z) => [((d.at.x - z.center().x) / (z.size.width / 2.)).clamp(-1., 1.), ((d.at.y - z.center().y) / (z.size.height / 2.)).clamp(-1., 1.)],
            None => [0., 0.],
        };
        for f in [&mut d.show, &mut d.veil, &mut d.arm, &mut d.dip, &mut d.land, &mut d.shake] { f.step(dt); }
        d.tilt.step(dt);
        for r in &mut d.rise { r.step(dt); }
        // Landed and settled: the tray goes.
        if d.landed.is_some() && d.land.p > 0.97 && !d.leaving { d.leaving = true; }
        if d.leaving && d.show.p < 0.01 && d.veil.p < 0.01 { self.home.folders.file_drop = None; cx.notify(); return None; }
        // The tray breathes while it waits, so it draws every frame while shown.
        window.request_animation_frame();

        // ---- The illustration's pose.
        // `lit`: the pane's glow, full while the pointer is over it. The glass folder
        // opens only a little with it, so it keeps its shape.
        let (show, lit, land, dip) = (d.show.unit(), d.arm.p, d.land.p, d.dip.p);
        let arm = 0.3 * lit;
        let t = d.born.elapsed().as_secs_f32();
        let breath = 0.5 + 0.5 * (t * std::f32::consts::TAU / 1.8).sin();
        let shake = d.shake.p * (t * 46.).sin() * 0.35;
        let n = d.importable().clamp(1, PAPERS);
        let (w, h) = (ART_W, ART_H);
        let (o, m) = (arm.max(0.35 * show), arm);
        let lift = -5. * o - 2. * m + 3. * dip + 6. * land;
        let [tx, ty] = d.tilt.p;
        let pose_of = |i: usize, r: f32| -> Paper {
            let (rest, rise, part) = [
                ((0.0, 0.16, 0.0), (0.0, -0.07, 0.0), (0.0, -0.03, 0.0)),
                ((0.03, 0.135, 2.5), (0.03, -0.055, 2.0), (0.06, -0.02, 4.0)),
                ((-0.035, 0.115, -3.0), (-0.03, -0.05, -2.5), (-0.06, -0.02, -4.0)),
            ][i];
            let (pw, ph) = (0.62 * w, 0.62 * h);
            // Out of the folder by `r`, higher while armed, sunk back in by `land`.
            let up = r * (1.6 + 1.1 * arm) - 2.4 * land;
            let dx = (rest.0 + rise.0 * up + part.0 * m) * w + tx * (5. + 3. * i as f32) * (1. - land);
            let top = (rest.1 + rise.1 * up + part.1 * m) * h + lift + ty * 3. * (1. - land);
            Paper { cx: w / 2. + dx, cy: top + ph / 2., w: pw, h: ph, rot: (rest.2 + rise.2 * up + part.2 * m + 2.5 * tx * (1. - land)).to_radians(), seed: 7 + i as u64 }
        };
        let papers: Vec<Paper> = (0..n).map(|i| pose_of(i, d.rise[i].p)).collect();
        let lean = 0.03 * h * o + 0.045 * h * m + 0.05 * h * dip;
        let pose = Pose { w, h, lift, open: o, yl: 0.49 * h + lean, yr: 0.54 * h + lean, inset: -(3. * o + 7. * m), papers };
        let tag = d.tag();
        // The front paper's name: the first file of the kind it shows.
        let first = d.paths.iter().find(|p| match tag { Some("MD") => crate::local_files::is_markdown_path(p), Some(_) => p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xml")), None => p.is_dir() })
            .or(d.paths.first()).and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let summary = d.summary();
        let record = d.zone.clone();
        let (at, drop_at, landed, veil_p, shake_x) = (d.at, d.drop_at, d.landed.is_some(), d.veil.p, shake * 18.);

        // ---- Elements.
        let accent = Theme::primary();
        let quiet = if reject { 1. } else { 0. };
        let (title, sub) = if reject {
            (crate::settings::tr("这些文件不能导入", "These files can't be imported").to_string(), crate::settings::tr("支持 Markdown、DocxXML 文件和文件夹", "Markdown, DocxXML files and folders are supported").to_string())
        } else if d.landed.is_some() {
            (crate::localized_format!("已导入到「{}」", "Imported into “{}”", place), summary)
        } else if over {
            (crate::localized_format!("松手放入「{}」", "Release to put it in “{}”", place), summary)
        } else {
            (crate::localized_format!("松手导入到「{}」", "Release to import into “{}”", place), summary)
        };
        let back = { let pose = pose.clone(); canvas(|_, _, _| {}, move |b, _, window, _| {
            back_plate(b.origin, &pose, window);
            // The front paper is the first file: its kind and name in place of a heading.
            for (i, p) in pose.papers.iter().enumerate().rev() { p.paint(b.origin, if i == 0 { Face::Titled } else { Face::Sketch }, window); }
        }).absolute().inset_0() };
        let chip = pose.papers.first().copied().map(|p| div().absolute().left(px(p.cx - p.w / 2. + 10.)).top(px(p.cy - p.h / 2. + 9.)).w(px(p.w - 20.))
            .flex().items_center().gap(px(5.)).overflow_hidden()
            .map(|d| match tag {
                Some(t) => d.child(div().flex_none().px(px(4.)).h(px(14.)).rounded(px(4.)).bg(accent).flex().items_center()
                    .text_size(px(8.5)).line_height(px(12.)).font_weight(FontWeight::BOLD).text_color(gpui::white()).child(t)),
                None => d.child(icon("folder", 12., Theme::icon())),
            })
            .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(px(10.)).line_height(px(14.))
                .font_weight(FontWeight::SEMIBOLD).text_color(Theme::text()).child(ui_text(first.clone()))));
        let front = { let pose = pose.clone(); canvas(|_, _, _| {}, move |b, _, window, _| glass(b.origin, &pose, window)).absolute().inset_0() };
        let grow = 0.9 + 0.1 * show + 0.05 * lit;
        let art = div().relative().flex_none().w(px(w)).h(px(h)).ml(px(shake_x)).opacity(1. - 0.35 * quiet)
            .child(back).children(chip).child(front);
        // The middle: the glass folder on a halo of the accent, the line under it. No box:
        // the whole pane is the place to drop.
        let middle = div().relative().flex().flex_col().items_center().gap(px(6.))
            .opacity(show * (0.08 + 0.92 * ((veil_p - 0.12) / 0.88).clamp(0., 1.)).min(show))
            .child(div().relative().flex_none().w(px(w * grow)).h(px(h * grow)).flex().items_center().justify_center()
                .child(div().absolute().left(px(w * grow / 2. - 40.)).top(px(h * grow / 2. - 30.)).size(px(80.)).rounded_full()
                    .shadow(vec![BoxShadow { color: if reject { ca(craft::N1000, 0.06) } else { accent.opacity(0.10 + 0.10 * lit + 0.06 * breath) }, offset: point(px(0.), px(0.)), blur_radius: px(90.), spread_radius: px(40. + 24. * lit) }]))
                .child(art))
            .child(div().mt(px(14.)).text_size(px(18.)).line_height(px(26.)).font_weight(FontWeight::SEMIBOLD)
                .text_color(if reject { Theme::text_caption() } else { Theme::text() }).child(ui_text(title)))
            .child(div().text_size(px(13.)).line_height(px(20.)).text_color(Theme::text_caption()).child(ui_text(sub)));
        // The pane's edge: a glow breathing in from every side, a dashed line settling
        // onto it, a halo under the pointer, and on a drop a
        // ripple from where it landed while the edge flares.
        let flare = if landed { (land * std::f32::consts::PI).sin().max(0.) } else { 0. };
        let edge = canvas(move |b, _, _| record.set(Some(b)), move |b, _, window, _| window.with_content_mask(Some(ContentMask { bounds: b }), |window| {
            let tint = if reject { ca(craft::N1000, 1.) } else { accent };
            let clear = tint.opacity(0.);
            let (x0, y0, wd, ht) = (f32::from(b.origin.x), f32::from(b.origin.y), f32::from(b.size.width), f32::from(b.size.height));
            let rect = |x: f32, y: f32, w: f32, h: f32| Bounds::new(point(px(x), px(y)), size(px(w.max(0.)), px(h.max(0.))));
            // A dashed edge, set in from the pane: the place to let go. It settles
            // from further inside onto its place as it shows.
            let inset = 22. + 30. * (1. - show);
            let r: f32 = 88.;
            let ring = rect(x0 + inset + shake_x * 0.4, y0 + inset, wd - 2. * inset, ht - 2. * inset);
            // The glow starts at the dashed line and fades inward: the blurred shadow of a
            // frame just outside it, seen only inside. Blur keeps the falloff continuous
            // and rounds the corners.
            let depth = (46. + 34. * lit + 30. * flare) * show;
            let a = if reject { 0.10 } else { (0.16 + 0.06 * breath + 0.16 * lit + 0.2 * flare) * show };
            let (l, t, rw, rh, big) = (f32::from(ring.left()), f32::from(ring.top()), f32::from(ring.size.width), f32::from(ring.size.height), 400.);
            let glow = [BoxShadow { color: tint.opacity(a), offset: point(px(0.), px(0.)), blur_radius: px(depth), spread_radius: px(0.) }];
            // Masks are only rectangles, so the ring's squircle inside (see `ui::squircle_path`)
            // is cut in 1px rows through the corner bands (the dashed line covers the steps);
            // one rect between.
            let rr = r.min(rw / 2.).min(rh / 2.).max(0.);
            let n = rr.ceil() as usize;
            let rows = (0..n).flat_map(|i| {
                let (y, hh) = (i as f32, (rr - i as f32).min(1.));
                let dx = rr - (rr.powi(4) - (rr - y - hh / 2.).powi(4)).max(0.).powf(0.25);
                [rect(l + dx, t + y, rw - 2. * dx, hh), rect(l + dx, t + rh - y - hh, rw - 2. * dx, hh)]
            }).chain([rect(l, t + rr, rw, rh - 2. * rr)]);
            for clip in rows {
                window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
                    // Each strip reaches a point inside, or GPUI culls a shadow wholly outside the clip.
                    for side in [rect(l - big, t - big, rw + 2. * big, big + 1.), rect(l - big, t + rh - 1., rw + 2. * big, big + 1.), rect(l - big, t, big + 1., rh), rect(l + rw - 1., t, big + 1., rh)] {
                        window.paint_shadows(side, px(0.).into(), &glow);
                    }
                });
            }
            // The halo under the pointer.
            if !reject && !landed {
                let c = point(at.x.max(b.left()).min(b.right()), at.y.max(b.top()).min(b.bottom()));
                window.paint_shadows(Bounds::centered_at(c, size(px(40.), px(40.))), px(20.).into(),
                    &[BoxShadow { color: accent.opacity(0.12 * lit * show), offset: point(px(0.), px(0.)), blur_radius: px(120.), spread_radius: px(70.) }]);
            }
            let ring_a = if reject { 0.22 } else { (0.40 + 0.12 * breath + 0.35 * lit + 0.2 * flare).min(1.) } * show;
            if let Some(p) = crate::ui::squircle_path(ring, r, 1., PathBuilder::stroke(px(2.)).dash_array(&[px(6.), px(5.)])) { window.paint_path(p, tint.opacity(ring_a)); }
            // The ripple from the drop.
            if landed {
                let far = wd.hypot(ht);
                for (k, delay) in [(0, 0.), (1, 0.18)] {
                    let q = ((land - delay) / (1. - delay)).clamp(0., 1.);
                    if q <= 0. { continue; }
                    let e = 1. - (1. - q).powi(3);
                    let rad = 30. + e * far * 0.8;
                    let c = point(drop_at.x, drop_at.y);
                    window.paint_quad(quad(Bounds::centered_at(c, size(px(2. * rad), px(2. * rad))), px(rad), clear, px(3. - k as f32), accent.opacity(0.5 * (1. - q)), BorderStyle::Solid));
                }
            }
        })).absolute().inset_0();
        let veil = theme::raised(Theme::bg_body()).opacity(0.86 * d.veil.p.clamp(0., 1.));
        Some(div().absolute().inset_0().bg(veil).child(edge).flex().items_center().justify_center().child(middle).into_any_element())
    }
}
