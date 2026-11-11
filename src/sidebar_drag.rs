use crate::app::Workspace;
use crate::store::Entry;
use crate::theme::{self, Theme};
use crate::ui::{icon, ui_text};
use gpui::{prelude::FluentBuilder, *};
use std::{cell::Cell, collections::{HashMap, HashSet, VecDeque}, rc::Rc, time::Instant};

const PITCH: f32 = 31.;
/// Whether clicking a folder open or shut animates (the drawer below): 外观 ›
/// 动画 › 丰富. Otherwise rows appear and go in the same frame. Lifting and
/// dropping an open folder keep their drawers either way.
fn animate_folding() -> bool { crate::settings::motion_high() }
const INDENT: f32 = 18.;

#[derive(Clone)]
struct Row { entry: Entry, depth: usize }

/// Seconds a dragged row must rest on a collapsed folder before it opens.
const DWELL: f32 = 0.45;

/// A damped spring per axis; the layout uses (x, y, width), the drop wash (x, y, width, height).
#[derive(Clone, Copy)]
pub(crate) struct Spring<const N: usize = 3> { pub p: [f32; N], pub v: [f32; N], pub target: [f32; N] }
impl<const N: usize> Spring<N> {
    pub(crate) fn new(p: [f32; N]) -> Self { Self { p, v: [0.; N], target: p } }
    pub(crate) fn step(&mut self, dt: f32) -> bool {
        let mut moving = false;
        for i in 0..N {
            // Substeps keep the same damped spring stable at 60 and 120 Hz.
            for _ in 0..4 {
                self.v[i] += (420. * (self.target[i] - self.p[i]) - 31. * self.v[i]) * dt / 4.;
                self.p[i] += self.v[i] * dt / 4.;
            }
            if (self.p[i] - self.target[i]).abs() < 0.05 && self.v[i].abs() < 0.1 { self.p[i] = self.target[i]; self.v[i] = 0.; }
            else { moving = true; }
        }
        moving
    }
}

/// A scalar spring for opacities and scales: `k` stiffness, `c` damping
/// (critical at 2√k; below that it overshoots, which is what the pops want).
#[derive(Clone, Copy)]
pub(crate) struct Fade { pub p: f32, pub v: f32, pub to: f32, k: f32, c: f32 }
impl Fade {
    pub(crate) const fn new(p: f32, k: f32, c: f32) -> Self { Self { p, v: 0., to: p, k, c } }
    pub(crate) fn from(mut self, p: f32, to: f32) -> Self { self.p = p; self.to = to; self.v = 0.; self }
    pub(crate) fn step(&mut self, dt: f32) -> bool {
        if self.p == self.to && self.v == 0. { return false; }
        for _ in 0..4 {
            self.v += (self.k * (self.to - self.p) - self.c * self.v) * dt / 4.;
            self.p += self.v * dt / 4.;
        }
        if (self.p - self.to).abs() < 0.002 && self.v.abs() < 0.02 { self.p = self.to; self.v = 0.; false } else { true }
    }
    pub(crate) fn unit(&self) -> f32 { self.p.clamp(0., 1.) }
}

const SHOW: Fade = Fade::new(1., 260., 30.);
const HOVER_IN: (f32, f32) = (1100., 66.);
const HOVER_OUT: (f32, f32) = (300., 35.);
/// The row the pointer just left for another one: gone in about 80 ms.
const HOVER_SWITCH: (f32, f32) = (2500., 100.);
const QUICK: Fade = Fade::new(0., 520., 46.);
const VANISH: Fade = Fade::new(0., 900., 60.);

/// Everything a row animates besides its place: entering, hover wash, the glow
/// after landing, the icon's bounce and the card it sinks back from.
#[derive(Clone, Copy)]
struct RowFx { show: Fade, hover: Fade, glow: Fade, pop: Fade, lift: Fade }
impl RowFx {
    fn new() -> Self {
        Self { show: SHOW, hover: Fade::new(0., HOVER_IN.0, HOVER_IN.1), glow: Fade::new(0., 22., 9.4),
            pop: Fade::new(1., 600., 15.), lift: Fade::new(0., 170., 26.) }
    }
    fn step(&mut self, dt: f32) -> bool {
        let mut m = false;
        for f in [&mut self.show, &mut self.hover, &mut self.glow, &mut self.pop, &mut self.lift] { m |= f.step(dt); }
        m
    }
}

/// A row that just left the list some other way than its folder closing (it was
/// deleted, or lifted with a dragged folder): it fades out where it stood.
struct Leaving { row: Row, pos: Spring, alpha: Fade }

/// A folder opening or closing, as one drawer: a clip under the folder's row
/// that grows or shrinks, its rows already in their final places inside, so
/// the edge reveals them in reading order and nothing ever overlaps. The rows
/// below ride on the edge. `folder: None` is 我的文档 itself unfolding;
/// `anchor`: a lifted folder has no row on screen, so its drawer closes at
/// that row index instead (the slot the row left).
///
/// Timed, not sprung: the edge only travels the part of the drawer that can
/// be seen (the rest appears or goes at once), in a time that grows with that
/// distance — a couple of rows in 0.15 s, a screenful in 0.25 s — on an
/// ease-out, so it starts at once and stops without a crawl. Closing takes
/// four fifths of that. Reversing starts a new run from where the edge is.
struct Drawer { folder: Option<String>, rows: Vec<Row>, opening: bool, anchor: Option<usize>, h: f32, from: f32, t: f32, dur: Option<f32>, done: bool }
impl Drawer {
    fn new(folder: Option<String>, rows: Vec<Row>, opening: bool, anchor: Option<usize>, h: f32) -> Self {
        Self { folder, rows, opening, anchor, h, from: h, t: 0., dur: None, done: false }
    }
    /// One frame: `span` is how far the edge can travel on screen, `full` the drawer's whole height.
    fn step(&mut self, dt: f32, span: f32, full: f32) {
        let span = span.clamp(PITCH.min(full), full.max(0.));
        let goal = if self.opening { span } else { 0. };
        let dur = *self.dur.get_or_insert_with(|| {
            self.from = self.h.min(span);
            let open = (0.13 + 0.012 * (goal - self.from).abs() / PITCH).clamp(0.15, 0.25);
            if self.opening { open } else { open * 0.8 }
        });
        self.t += dt;
        let x = (self.t / dur).min(1.);
        self.h = self.from + (goal - self.from) * (1. - (1. - x).powi(3));
        if x >= 1. { self.done = true; if self.opening { self.h = full; } }
    }
}

/// The selection: drawn under its row, moving with it (folders opening, rows
/// making room in a drag). A click moves it to the new row at once; with
/// 动画 › 丰富 it glides there instead.
struct Pill { id: String, pos: Spring, travel: bool }

pub(crate) struct Drag {
    id: String,
    start: Point<Pixels>,
    at: Point<Pixels>,
    offset: Point<Pixels>,
    started: bool,
    excluded: HashSet<String>,
    index: usize,
    depth: usize,
    initial_depth: usize,
    parent: Option<String>,
    original_index: usize,
    original_depth: usize,
    hover: Option<(String, Instant)>,
    ghost: Spring,
    lift: Fade,
}

#[derive(Clone)]
struct PendingMove { id: String, parent: Option<String>, before: Option<String>, history: u64 }

#[derive(Default)]
pub(crate) struct SidebarDrag {
    pub active: Option<Drag>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    springs: HashMap<String, Spring>,
    last_frame: Option<Instant>,
    pending: VecDeque<PendingMove>,
    saving: bool,
    fx: HashMap<String, RowFx>,
    shown: Vec<Row>,
    leaving: HashMap<String, Leaving>,
    hovered: Option<String>,
    primed: bool,
    pill: Option<Pill>,
    zone: Option<(Spring<4>, Fade)>,
    slot: Option<(Spring, Fade)>,
    drawers: Vec<Drawer>,
    /// The folders open last frame, to notice one opening or closing.
    was_open: HashSet<String>,
    unfold: bool,
    /// A dropped folder whose rows should open back out under it.
    reopen: Option<String>,
    /// The rename field fading in over its row.
    rename_fx: Option<(String, Fade)>,
    /// The press that is about to click a row landed on a folder's icon
    /// (its chevron), and whether ⌥ was down: that folds the whole subtree.
    pub icon_press: bool,
    pub icon_alt: bool,
    /// Folders with something in them (they get a chevron), rebuilt each frame.
    parents: HashSet<String>,
}

impl SidebarDrag {
    /// How far a row's hover has faded in (0–1): drives its wash and its buttons.
    pub(crate) fn reveal(&self, id: &str) -> f32 { self.fx.get(id).map_or(0., |f| f.hover.unit()) }
    /// The folder icon's scale (rests at 1, bounces when kicked).
    pub(crate) fn pop(&self, id: &str) -> f32 { self.fx.get(id).map_or(1., |f| f.pop.p.max(0.5)) }
    pub(crate) fn has_children(&self, id: &str) -> bool { self.parents.contains(id) }
    /// How far the rename field on `id` has faded in (0–1).
    pub(crate) fn rename_in(&self, id: &str) -> f32 { self.rename_fx.as_ref().filter(|(r, _)| r == id).map_or(1., |(_, f)| f.unit()) }
    pub(crate) fn kick(&mut self, id: &str, v: f32) { if let Some(f) = self.fx.get_mut(id) { f.pop.v += v; } }
    /// Forget every row so the next frame opens the whole list as one drawer (unfolding 我的文档).
    pub(crate) fn reset(&mut self) {
        self.springs.clear(); self.fx.clear(); self.shown.clear(); self.leaving.clear(); self.pill = None; self.drawers.clear();
        self.unfold = true;
    }
}

/// A row's face without its controls: the drag ghost and rows on their way out.
fn row_face(e: &Entry, open: bool) -> Div {
    div().h(px(crate::sidebar::ROW_H)).w_full().pl(px(6.)).flex().items_center()
        .child(div().size(px(20.)).flex_none().flex().items_center().justify_center().map(|d| match &e.icon {
            _ if e.meta.folder.is_some() => d.child(icon(if open { "folder-open" } else { "folder" }, crate::sidebar::ROW_ICON, crate::sidebar::folder_color())),
            _ if e.meta.markdown => d.child(icon("doc-md", crate::sidebar::ROW_ICON, Theme::icon())),
            Some(em) => d.text_size(px(13.)).line_height(px(20.)).child(em.clone()),
            None => d.child(icon("doc", crate::sidebar::ROW_ICON, Theme::icon())),
        }))
        .child(div().pl(px(9.)).flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(px(13.))
            .child(ui_text(crate::settings::display_title(&e.title))))
}

fn faded(shadows: Vec<BoxShadow>, a: f32) -> Vec<BoxShadow> {
    shadows.into_iter().map(|mut s| { s.color = s.color.opacity(a); s }).collect()
}

impl Workspace {
    fn drag_rows(&self) -> Vec<Row> {
        crate::tree::Tree::new(&self.entries).rows(|id| self.expanded.contains(id)).into_iter()
            .filter(|(e, _)| self.folder_drag.active.as_ref().is_none_or(|d| !d.started || !d.excluded.contains(&e.id)))
            .map(|(e, depth)| Row { entry: e.clone(), depth }).collect()
    }

    pub(crate) fn arm_folder_drag(&mut self, id: &str, at: Point<Pixels>, cx: &mut Context<Self>) {
        let rows = self.drag_rows();
        let Some(index) = rows.iter().position(|r| r.entry.id == id) else { return };
        let Some(bounds) = self.folder_drag.bounds.get() else { return };
        let Some(s) = self.folder_drag.springs.get(id) else { return };
        let origin = bounds.origin + point(px(s.p[0]), px(s.p[1]));
        self.folder_drag.active = Some(Drag {
            id: id.to_string(), start: at, at, offset: at - origin, started: false,
            excluded: crate::tree::Tree::new(&self.entries).subtree(id).into_iter().collect(),
            index, depth: rows[index].depth, initial_depth: rows[index].depth, parent: rows[index].entry.meta.parent.clone(),
            original_index: index, original_depth: rows[index].depth, hover: None,
            ghost: Spring::new([f32::from(origin.x), f32::from(origin.y), s.p[2]]), lift: Fade::new(0., 520., 24.),
        });
        cx.notify();
    }

    pub(crate) fn move_folder_drag(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.folder_drag.active.as_mut() else { return };
        d.at = at;
        if !d.started && (f32::from(at.x - d.start.x).hypot(f32::from(at.y - d.start.y)) >= 4.) {
            d.started = true; d.lift.to = 1.;
            self.folder_drag.icon_press = false;
            self.folder_drag.icon_alt = false;
            self.row_menu = None; self.create_menu = None;
            window.focus(&self.focus);
        }
        self.project_folder_drop();
        cx.notify();
    }

    fn project_folder_drop(&mut self) {
        let Some(bounds) = self.folder_drag.bounds.get() else { return };
        let rows = self.drag_rows();
        let Some(d) = self.folder_drag.active.as_mut().filter(|d| d.started) else { return };
        let y = f32::from(d.at.y - d.offset.y - bounds.top()) + 15.;
        let desired = (d.initial_depth as i32 + (f32::from(d.at.x - d.start.x) / INDENT).round() as i32).max(0) as usize;
        // Hit the visible folder row with the pointer, including collapsed folders.
        // Accept immediately; the dwell timer only controls automatic expansion.
        let pointer_y = f32::from(d.at.y - bounds.top());
        let folder = rows.iter().enumerate().find(|(i, r)| {
            let top = self.folder_drag.springs.get(&r.entry.id).map_or(
                (*i + usize::from(*i >= d.index)) as f32 * PITCH, |s| s.p[1]);
            r.entry.meta.folder.is_some() && pointer_y > top + 5. && pointer_y < top + 26.
                && f32::from(d.at.x - bounds.left()) >= r.depth as f32 * INDENT
        });
        if let Some((i, r)) = folder {
            if d.hover.as_ref().is_none_or(|(id, _)| *id != r.entry.id) {
                d.hover = Some((r.entry.id.clone(), Instant::now()));
                d.initial_depth = r.depth + 1; d.start.x = d.at.x;
                // The folder notices the row: a small bounce as its lid opens.
                if let Some(f) = self.folder_drag.fx.get_mut(&r.entry.id) { f.pop.v += 3.2; }
            }
            d.index = i + 1;
            d.depth = r.depth + 1;
            d.parent = Some(r.entry.id.clone());
            if d.hover.as_ref().is_some_and(|(_, t)| t.elapsed().as_secs_f32() >= DWELL) && self.expanded.insert(r.entry.id.clone())
                && let Some(f) = self.folder_drag.fx.get_mut(&r.entry.id) { f.pop.v += 4.; }
            return;
        }
        let slot_y = d.index as f32 * PITCH;
        let in_slot = y >= slot_y && y <= slot_y + PITCH;
        d.hover = None;
        let index = if in_slot { d.index } else {
            rows.iter().enumerate().filter(|(i, _)| y > (*i + usize::from(*i >= d.index)) as f32 * PITCH + 15.).count()
        };
        let prev = index.checked_sub(1).and_then(|i| rows.get(i));
        let max = prev.map_or(0, |r| r.depth + usize::from(r.entry.meta.folder.is_some()));
        let min = rows.get(index).map_or(0, |r| r.depth);
        let depth = desired.clamp(min.min(max), max);
        let parent = if depth == 0 { None } else { rows[..index].iter().rev().find(|r| r.depth + 1 == depth && r.entry.meta.folder.is_some()).map(|r| r.entry.id.clone()) };
        d.index = index; d.depth = depth; d.parent = parent;
    }

    pub(crate) fn folder_drop_target(&self, id: &str) -> bool {
        self.folder_drag.active.as_ref().is_some_and(|d| {
            let b = self.list_scroll.bounds();
            d.started && d.parent.as_deref() == Some(id)
                && d.at.x >= b.left() && d.at.x <= b.right() && d.at.y >= b.top() && d.at.y <= b.bottom()
        })
    }

    pub(crate) fn end_folder_drag(&mut self, cancel: bool, cx: &mut Context<Self>) {
        let rows = self.drag_rows();
        let Some(d) = self.folder_drag.active.as_mut() else { return };
        if !d.started { self.folder_drag.active = None; cx.notify(); return; }
        let b = self.list_scroll.bounds();
        let cancel = cancel || d.at.x < b.left() || d.at.x > b.right() || d.at.y < b.top() || d.at.y > b.bottom();
        let before = rows.get(d.index).filter(|r| r.entry.meta.parent == d.parent).map(|r| r.entry.id.clone());
        let mut movement = PendingMove { id: d.id.clone(), parent: d.parent.clone(), before, history: 0 };
        let depth = if cancel { d.original_depth } else { d.depth };
        let index = if cancel { d.original_index } else { d.index };
        // Drop the overlay immediately: normal row color and hit testing resume this frame.
        // Seed the real row at the pointer so it settles without a second visual swap.
        let bounds = self.folder_drag.bounds.get();
        // The card keeps its lifted size and shadow and sinks back from there.
        let lift = d.lift.unit();
        let mut spring = d.ghost;
        if let Some(b) = bounds { spring.p[0] -= f32::from(b.left()); spring.p[1] -= f32::from(b.top()); }
        spring.p[0] -= 3. * lift; spring.p[1] -= lift; spring.p[2] += 6. * lift;
        spring.target = [depth as f32 * INDENT, index as f32 * PITCH, spring.p[2]];
        self.folder_drag.springs.insert(movement.id.clone(), spring);
        let fx = self.folder_drag.fx.entry(movement.id.clone()).or_insert_with(RowFx::new);
        fx.show = SHOW;
        fx.lift = fx.lift.from(lift, 0.);
        if !cancel { fx.glow = fx.glow.from(1., 0.); }
        if !cancel && let Some(parent) = &movement.parent { self.folder_drag.kick(parent, 4.5); }
        self.folder_drag.active = None;
        // The card covers the slot as it lands; the slot goes at once instead of stacking under it.
        self.folder_drag.slot = None;
        // A folder carried open comes back open: its rows open out under it.
        if self.expanded.contains(&movement.id) { self.folder_drag.reopen = Some(movement.id.clone()); }
        if !cancel {
            if let Some(parent) = &movement.parent { self.expanded.insert(parent.clone()); }
            let file_before = self.entries.clone();
            self.apply_tree_move(&movement);
            movement.history = self.remember_tree_move(file_before, crate::settings::tr("拖拽移动", "Drag move"));
            self.folder_drag.pending.push_back(movement);
            self.save_next_tree_move(cx);
            self.sync_sub_pages(cx);
        }
        cx.notify();
    }

    /// Put `id` last in `folder` (a drop on 主页's folder card), saved like a sidebar drop.
    pub(crate) fn move_into_folder(&mut self, id: &str, folder: &str, cx: &mut Context<Self>) {
        let mut movement = PendingMove { id: id.to_string(), parent: Some(folder.to_string()), before: None, history: 0 };
        let file_before = self.entries.clone();
        self.apply_tree_move(&movement);
        movement.history = self.remember_tree_move(file_before, crate::settings::tr("拖拽移动", "Drag move"));
        self.folder_drag.pending.push_back(movement);
        self.save_next_tree_move(cx);
        self.sync_sub_pages(cx);
        cx.notify();
    }


    pub(crate) fn reapply_pending_tree_moves(&mut self) {
        for movement in self.folder_drag.pending.clone() { self.apply_tree_move(&movement); }
    }

    fn apply_tree_move(&mut self, movement: &PendingMove) {
        let tree = crate::tree::Tree::new(&self.entries);
        let mut siblings: Vec<String> = tree.children(movement.parent.as_deref()).iter()
            .filter(|e| e.id != movement.id).map(|e| e.id.clone()).collect();
        let pos = movement.before.as_ref().and_then(|id| siblings.iter().position(|key| key == id)).unwrap_or(siblings.len());
        siblings.insert(pos, movement.id.clone());
        for e in &mut self.entries {
            if e.id == movement.id { e.meta.parent = movement.parent.clone(); }
            if let Some(i) = siblings.iter().position(|id| *id == e.id) {
                if movement.parent.is_none() { e.meta.root_order = Some(i as u64 + 1); }
                else { e.meta.order = i as u64 + 1; e.meta.root_order = None; }
            }
        }
    }

    pub(crate) fn tree_writes_pending(&self) -> bool { self.folder_drag.saving || !self.folder_drag.pending.is_empty() }

    fn save_next_tree_move(&mut self, cx: &mut Context<Self>) {
        if self.folder_drag.saving { return; }
        let Some(movement) = self.folder_drag.pending.front().cloned() else { return };
        self.folder_drag.saving = true;
        let history = movement.history;
        let mut writer = self.store.tree_writer();
        let task = cx.background_executor().spawn(async move {
            let journal = writer.track_file_changes();
            let result = writer.move_in_tree(&movement.id, movement.parent.as_deref(), movement.before.as_deref());
            let restored = if result.is_err() { Some(writer.list()) } else { None };
            (result, restored, journal.take())
        });
        cx.spawn(async move |this, cx| {
            let (result, restored, changes) = task.await;
            let _ = this.update(cx, |w, cx| {
                w.folder_drag.pending.pop_front();
                w.folder_drag.saving = false;
                if let Err(ref e) = result {
                    w.forget_file_change(history);
                    // Restore only hierarchy, preserving edits made while the write was pending.
                    if let Some(restored) = restored {
                        for entry in &mut w.entries {
                            if let Some(saved) = restored.iter().find(|e| e.id == entry.id) {
                                entry.meta.parent = saved.meta.parent.clone();
                                entry.meta.order = saved.meta.order;
                                entry.meta.root_order = saved.meta.root_order;
                            }
                        }
                    }
                    for movement in w.folder_drag.pending.clone() { w.apply_tree_move(&movement); }
                    w.sync_sub_pages(cx);
                    w.show_toast_kind(crate::ui::ToastKind::Error, &e.to_string(), cx);
                }
                if result.is_ok() { w.confirm_tree_history(history, changes); }
                w.save_next_tree_move(cx);
                cx.notify();
            });
        }).detach();
    }

    /// The row the selection pill sits on: the open doc, or the open folder.
    fn sidebar_pick(&self) -> Option<String> {
        match self.view {
            crate::app::View::Doc => self.current.clone(),
            crate::app::View::Folder => self.folder_current.clone(),
            _ => None,
        }
    }

    pub(crate) fn render_folder_rows(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let now = Instant::now();
        let dt = self.folder_drag.last_frame.replace(now).map_or(1. / 120., |last| now.duration_since(last).as_secs_f32().min(0.032));
        self.project_folder_drop();
        let rows = self.drag_rows();
        let width = self.sidebar_w(window) - 2. * crate::sidebar::SB_X;
        let active = self.folder_drag.active.as_ref().filter(|d| d.started);
        let slot = active.map(|d| (d.index, d.depth));
        let dragged = active.map(|d| d.id.clone());
        let dwell = active.and_then(|d| d.hover.as_ref()).filter(|(id, _)| !self.expanded.contains(id))
            .map(|(id, t)| (id.clone(), (t.elapsed().as_secs_f32() / DWELL).min(1.)));
        let receiving = rows.iter().position(|r| self.folder_drop_target(&r.entry.id));
        let receiving_end = receiving.map(|start| rows.iter().enumerate().skip(start + 1)
            .find(|(_, r)| r.depth <= rows[start].depth).map_or(rows.len(), |(i, _)| i));
        let pick = self.sidebar_pick();
        let mut moving = false;

        let renaming = self.rename.as_ref().map(|r| r.id.clone());
        self.folder_drag.parents = self.entries.iter().filter_map(|e| e.meta.parent.clone()).collect();
        let high = crate::settings::motion_high();
        // The visible list's bottom edge, in this list's coordinates (last frame's layout).
        let vis_bottom = self.folder_drag.bounds.get().map_or(f32::MAX, |b| f32::from(self.list_scroll.bounds().bottom() - b.top()));
        // ---- Step every spring; nothing below this block mutates.
        let fd = &mut self.folder_drag;
        let index: HashMap<&str, usize> = rows.iter().enumerate().map(|(i, r)| (r.entry.id.as_str(), i)).collect();
        let targets: Vec<[f32; 3]> = rows.iter().enumerate().map(|(i, r)| {
            let y = (i + usize::from(slot.is_some_and(|(index, _)| i >= index))) as f32 * PITCH;
            let x = r.depth as f32 * INDENT;
            let inset = if receiving.zip(receiving_end).is_some_and(|(start, end)| i >= start && i < end) { 8. } else { 0. };
            [x, y, width - x - inset]
        }).collect();
        // The rows under row `i` that open and close with it.
        let under = |rows: &[Row], i: usize| -> Vec<Row> {
            rows[i + 1..].iter().take_while(|r| r.depth > rows[i].depth).cloned().collect()
        };
        let primed = std::mem::replace(&mut fd.primed, true);
        let open_now: HashSet<String> = self.expanded.clone();
        let mut started = false;
        // Rows that appear at once, without fading in (folding is instant).
        let mut instant: HashSet<String> = HashSet::new();
        if std::mem::take(&mut fd.unfold) && !rows.is_empty() {
            if animate_folding() { fd.drawers.push(Drawer::new(None, vec![], true, None, 0.)); }
            else { instant.extend(rows.iter().map(|r| r.entry.id.clone())); }
            started = true;
        }
        if primed {
            for f in open_now.difference(&fd.was_open) {
                let Some(&i) = index.get(f.as_str()) else { continue };
                if under(&rows, i).is_empty() { continue; }
                if !animate_folding() {
                    instant.extend(under(&rows, i).into_iter().map(|r| r.entry.id));
                    started = true;
                    continue;
                }
                // Reopened while closing: carry on from the current height.
                let h = fd.drawers.iter().position(|d| d.folder.as_ref() == Some(f)).map_or(0., |k| fd.drawers.remove(k).h);
                fd.drawers.push(Drawer::new(Some(f.clone()), vec![], true, None, h));
                started = true;
            }
            for f in fd.was_open.difference(&open_now) {
                if !index.contains_key(f.as_str()) { continue; }
                let Some(i) = fd.shown.iter().position(|r| &r.entry.id == f) else { continue };
                let gone = under(&fd.shown, i);
                if gone.is_empty() { continue; }
                let full = gone.len() as f32 * PITCH;
                let h = fd.drawers.iter().position(|d| d.folder.as_ref() == Some(f)).map_or(full, |k| fd.drawers.remove(k).h);
                // A drawer inside one that is closing goes with it.
                let inside: HashSet<&str> = gone.iter().map(|r| r.entry.id.as_str()).collect();
                fd.drawers.retain(|d| d.folder.as_deref().is_none_or(|x| !inside.contains(x)));
                for r in &gone { fd.springs.remove(&r.entry.id); fd.fx.remove(&r.entry.id); }
                started = true;
                if !animate_folding() { continue; }
                fd.drawers.push(Drawer::new(Some(f.clone()), gone, false, None, h));
            }
        }
        fd.was_open = open_now;
        // Lifting an open folder closes its rows into the place it left ...
        if let Some(id) = &dragged && let Some(i) = fd.shown.iter().position(|r| &r.entry.id == id) && !index.contains_key(id.as_str()) {
            let gone = under(&fd.shown, i);
            if !gone.is_empty() && !gone.iter().any(|r| index.contains_key(r.entry.id.as_str())) {
                for r in &gone { fd.springs.remove(&r.entry.id); fd.fx.remove(&r.entry.id); }
                // Still opening when it was picked up: close from where it got to.
                let full = fd.drawers.iter().position(|d| d.folder.as_ref() == Some(id)).map_or(gone.len() as f32 * PITCH, |k| fd.drawers.remove(k).h);
                fd.drawers.push(Drawer::new(Some(id.clone()), gone, false, Some(i), full));
                started = true;
            }
        }
        // ... and dropping it opens them back out under it.
        if let Some(id) = fd.reopen.take() && let Some(&i) = index.get(id.as_str()) && !under(&rows, i).is_empty() {
            // One drawer per folder: the one it closed into (if still running) gives way.
            fd.drawers.retain(|d| d.folder.as_ref() != Some(&id));
            fd.drawers.push(Drawer::new(Some(id), vec![], true, None, 0.));
            started = true;
        }
        // Rows that vanished without their folder closing fade out where they were.
        let present: HashSet<&str> = rows.iter().map(|r| r.entry.id.as_str()).collect();
        for r in std::mem::take(&mut fd.shown) {
            if present.contains(r.entry.id.as_str()) || dragged.as_ref() == Some(&r.entry.id) { continue; }
            let Some(pos) = fd.springs.remove(&r.entry.id) else { continue };
            let a = fd.fx.remove(&r.entry.id).map_or(1., |f| f.show.unit());
            fd.leaving.insert(r.entry.id.clone(), Leaving { row: r, pos, alpha: VANISH.from(a, 0.) });
        }
        // Opening drawers hold the rows under their folder, refreshed as those change.
        for d in &mut fd.drawers {
            if !d.opening { continue; }
            d.rows = match &d.folder { None => rows.clone(), Some(f) => index.get(f.as_str()).map_or(vec![], |&i| under(&rows, i)) };
        }
        let in_drawer: HashSet<String> = fd.drawers.iter().filter(|d| d.opening).flat_map(|d| d.rows.iter().map(|r| r.entry.id.clone())).collect();
        for (i, r) in rows.iter().enumerate() {
            let target = targets[i];
            let id = &r.entry.id;
            if !fd.springs.contains_key(id) {
                let mut s = Spring::new(target);
                let mut fx = RowFx::new();
                if let Some(l) = fd.leaving.remove(id) {
                    // Came back before it finished leaving: carry on from where it is.
                    s = l.pos; fx.show = SHOW.from(l.alpha.unit(), 1.);
                } else if primed && !in_drawer.contains(id) && !instant.contains(id) {
                    // A row that simply appears (a new page) settles in from just above.
                    s.p[1] = target[1] - 6.;
                    fx.show = SHOW.from(0., 1.);
                }
                fd.springs.insert(id.clone(), s);
                fd.fx.insert(id.clone(), fx);
            }
            let fx = fd.fx.entry(id.clone()).or_insert_with(RowFx::new);
            // Hover: one row at a time. Sweeping the pointer down the list must not
            // leave a trail, so a row the pointer moved on from clears at once (丰富:
            // in a few frames); only leaving the list lets the last wash linger.
            let hot = dragged.is_none() && fd.hovered.as_ref() == Some(id);
            let elsewhere = dragged.is_none() && fd.hovered.as_ref().is_some_and(|h| h != id && present.contains(h.as_str()));
            fx.hover.to = if hot { 1. } else { 0. };
            (fx.hover.k, fx.hover.c) = if hot { HOVER_IN } else if elsewhere { HOVER_SWITCH } else { HOVER_OUT };
            if !high && fx.hover.p != fx.hover.to { fx.hover.p = fx.hover.to; fx.hover.v = 0.; }
            let spring = fd.springs.get_mut(id).expect("inserted above");
            spring.target = target;
            // A drawer starting moves the rows below it itself: their places
            // jump to the new layout and the drawer's edge carries them there.
            if started && fx.lift.p < 0.005 { spring.p[1] = target[1]; spring.v[1] = 0.; }
            moving |= spring.step(dt);
            moving |= fx.step(dt);
        }
        fd.shown = rows.clone();
        match (&renaming, &mut fd.rename_fx) {
            (Some(id), Some((r, _))) if r == id => {}
            (Some(id), _) => fd.rename_fx = Some((id.clone(), Fade::new(0., 900., 60.).from(0., 1.))),
            (None, _) => fd.rename_fx = None,
        }
        if let Some((_, f)) = &mut fd.rename_fx { moving |= f.step(dt); }
        // Rows on their way out fade where they stood, then go.
        fd.leaving.retain(|_, l| {
            moving |= l.pos.step(dt);
            let fading = l.alpha.step(dt);
            moving |= fading;
            fading && l.alpha.p > 0.01
        });
        // An opening drawer whose folder left the list (lifted, deleted) has nothing to hold.
        fd.drawers.retain(|d| !(d.opening && d.folder.as_ref().is_some_and(|f| !index.contains_key(f.as_str()))));
        // Where every row is drawn this frame: its spring, moved by the drawers
        // above it, and clipped by the drawers it sits in. A drawer hangs from its
        // folder's place in the layout, not from where the row is drawn: a folder
        // just dropped is still sinking into that place.
        let base_y = |i: usize| targets[i][1];
        let mut shift = vec![0f32; rows.len()];
        let mut clip: Vec<Option<(f32, f32)>> = vec![None; rows.len()];
        let mut reveal = vec![1f32; rows.len()];
        let mut closing: Vec<(Row, [f32; 3], (f32, f32), f32)> = vec![];
        // Outer drawers first, so an inner one's edge rides on the outer's.
        let mut order: Vec<usize> = (0..fd.drawers.len()).collect();
        order.sort_by_key(|&k| if fd.drawers[k].anchor.is_some() { 0 } else { fd.drawers[k].folder.as_ref().and_then(|f| index.get(f.as_str())).map_or(0, |&i| rows[i].depth + 1) });
        for k in order {
            let d = &mut fd.drawers[k];
            let full = d.rows.len() as f32 * PITCH;
            if let Some(at) = d.anchor {
                // Closing at the slot a lifted folder left (one row tall, so the edge starts below it).
                let top = (at as f32 + 1.) * PITCH + shift.get(at).copied().unwrap_or(0.);
                d.step(dt, vis_bottom - top, full);
                moving = true;
                let edge = (top, top + d.h.max(0.));
                for (n, r) in d.rows.iter().enumerate() {
                    let x = r.depth as f32 * INDENT;
                    let local = ((d.h - n as f32 * PITCH) / (PITCH * 0.7)).clamp(0., 1.);
                    closing.push((r.clone(), [x, top + n as f32 * PITCH, width - x], edge, local * local * (3. - 2. * local)));
                }
                for s in shift.iter_mut().skip(at) { *s += d.h; }
                continue;
            }
            let fi = match &d.folder { None => None, Some(f) => match index.get(f.as_str()) { Some(&i) => Some(i), None => { d.done = true; continue } } };
            let top = fi.map_or(0., |i| base_y(i) + shift[i] + PITCH);
            d.step(dt, vis_bottom - top, full);
            moving = true;
            let edge = (top, top + d.h.max(0.));
            if d.opening {
                let first = fi.map_or(0, |i| i + 1);
                let end = first + d.rows.len();
                for i in first..end.min(rows.len()) {
                    let (lo, hi) = clip[i].unwrap_or((f32::MIN, f32::MAX));
                    clip[i] = Some((lo.max(edge.0), hi.min(edge.1)));
                    // Each row is fully in by the time the edge is most of the way past it.
                    let local = ((d.h - (i - first) as f32 * PITCH) / (PITCH * 0.7)).clamp(0., 1.);
                    reveal[i] = reveal[i].min(local * local * (3. - 2. * local));
                }
                for s in shift.iter_mut().skip(end) { *s += d.h - full; }
            } else {
                let fi = fi.unwrap_or(0);
                for (n, r) in d.rows.iter().enumerate() {
                    let x = r.depth as f32 * INDENT;
                    let local = ((d.h - n as f32 * PITCH) / (PITCH * 0.7)).clamp(0., 1.);
                    closing.push((r.clone(), [x, top + n as f32 * PITCH, width - x], edge, local * local * (3. - 2. * local)));
                }
                for s in shift.iter_mut().skip(fi + 1) { *s += d.h; }
            }
        }
        let extra: f32 = fd.drawers.iter().map(|d| if d.opening { d.h - d.rows.len() as f32 * PITCH } else { d.h }).sum();
        fd.drawers.retain(|d| !d.done);
        let drawn: Vec<[f32; 3]> = (0..rows.len()).map(|i| { let p = fd.springs[&rows[i].entry.id].p; [p[0], p[1] + shift[i], p[2]] }).collect();
        // The receiving folder's wash springs to its subtree's bounds and fades in and out.
        let zone = receiving.zip(receiving_end).map(|(start, end)| {
            let top = drawn[start][1];
            let mut bottom = (start..end).map(|i| drawn[i][1] + crate::sidebar::ROW_H).fold(top, f32::max);
            if let Some((index, _)) = slot && index > start && index <= end { bottom = bottom.max(index as f32 * PITCH + crate::sidebar::ROW_H); }
            let x = rows[start].depth as f32 * INDENT;
            [x - 3., top - 3., width - x + 3., bottom - top + 6.]
        });
        match (zone, &mut fd.zone) {
            (Some(r), Some((s, a))) => { if a.p < 0.02 { *s = Spring::new(r); } s.target = r; a.to = 1.; }
            (Some(r), None) => fd.zone = Some((Spring::new(r), QUICK.from(0., 1.))),
            (None, Some((_, a))) => a.to = 0.,
            (None, None) => {}
        }
        if let Some((s, a)) = &mut fd.zone {
            let m = s.step(dt) | a.step(dt);
            moving |= m;
            if !m && a.to == 0. { fd.zone = None; }
        }
        // The slot the row will land in.
        let slot_target = slot.map(|(index, depth)| [depth as f32 * INDENT, index as f32 * PITCH, width - depth as f32 * INDENT - if receiving.is_some() { 8. } else { 0. }]);
        match (slot_target, &mut fd.slot) {
            (Some(t), Some((s, a))) => { if a.p < 0.02 { *s = Spring::new(t); } s.target = t; a.to = 1.; }
            (Some(t), None) => fd.slot = Some((Spring::new(t), QUICK.from(0., 1.))),
            (None, Some((_, a))) => a.to = 0.,
            (None, None) => {}
        }
        if let Some((s, a)) = &mut fd.slot {
            let m = s.step(dt) | a.step(dt);
            moving |= m;
            if !m && a.to == 0. { fd.slot = None; }
        }
        // The selection sits on its row's drawn place; hidden or lifted rows have none.
        let picked = pick.filter(|id| dragged.as_ref() != Some(id)).and_then(|id| index.get(id.as_str()).map(|&i| (id, drawn[i])));
        match (picked, &mut fd.pill) {
            (Some((id, p)), Some(pill)) => {
                if pill.id != id && high { pill.travel = true; }
                pill.id = id; pill.pos.target = p;
                if !pill.travel { pill.pos.p = p; pill.pos.v = [0.; 3]; }
            }
            (Some((id, p)), None) => fd.pill = Some(Pill { id, pos: Spring::new(p), travel: false }),
            (None, _) => fd.pill = None,
        }
        if let Some(pill) = &mut fd.pill && pill.travel { pill.travel = pill.pos.step(dt); moving |= pill.travel; }
        if let Some(d) = fd.active.as_mut().filter(|d| d.started) {
            moving |= d.lift.step(dt);
            // The lifted row follows the pointer exactly; only layout and landing spring.
            d.ghost.p = [f32::from(d.at.x - d.offset.x), f32::from(d.at.y - d.offset.y), slot_target.map_or(d.ghost.p[2], |t| t[2])];
            moving |= d.hover.is_some() && dwell.is_some();
        }

        // ---- Build the elements, back to front.
        let fd = &self.folder_drag;
        let row_h = px(crate::sidebar::ROW_H);
        let radius = px(crate::sidebar::ROW_R);
        // Clip `el` (drawn at x, y, w) to a drawer's band, or draw it as is.
        let clipped = |el: Stateful<Div>, x: f32, y: f32, w: f32, band: Option<(f32, f32)>| -> AnyElement {
            match band {
                None => el.absolute().left(px(x)).top(px(y)).w(px(w)).into_any_element(),
                Some((lo, hi)) => div().absolute().left(px(x)).top(px(lo)).w(px(w)).h(px((hi - lo).max(0.))).overflow_hidden()
                    .child(el.absolute().left_0().top(px(y - lo)).w(px(w))).into_any_element(),
            }
        };
        let mut container = div().relative().w_full().h(px(((rows.len() + usize::from(slot.is_some())) as f32 * PITCH + extra).max(0.)))
            .child({ let b = fd.bounds.clone(); canvas(move |bounds, _, _| b.set(Some(bounds)), |_, _, _, _| {}).absolute().inset_0() });
        for l in fd.leaving.values() {
            let [x, y, w] = l.pos.p;
            container = container.child(div().absolute().left(px(x)).top(px(y)).w(px(w)).opacity(l.alpha.unit())
                .text_color(crate::sidebar::text_color()).child(row_face(&l.row.entry, false)));
        }
        for (r, [x, y, w], band, a) in &closing {
            if band.1 - band.0 < 0.5 { continue; }
            let el = div().id(SharedString::from(format!("fclose-{}", r.entry.id))).h(row_h).opacity(*a).text_color(crate::sidebar::text_color())
                .child(row_face(&r.entry, self.expanded.contains(&r.entry.id)));
            container = container.child(clipped(el, *x, *y - 4. * (1. - a), *w, Some(*band)));
        }
        if let Some((s, a)) = &fd.zone {
            let [x, y, w, h] = s.p;
            let a = a.unit();
            container = container.child(div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h)).rounded(px(crate::sidebar::ROW_R + 3.))
                .bg(Theme::sidebar_drop().opacity(a)));
        }
        if let Some((id, t)) = &dwell && let Some(&i) = index.get(id.as_str()) {
            // Pressure building on a closed folder: a wash sweeps across it until it opens.
            let e = 1. - (1. - t) * (1. - t);
            let [x, y, w] = drawn[i];
            container = container.child(div().absolute().left(px(x)).top(px(y)).w(px(w * e)).h(row_h).rounded(radius).bg(Theme::sidebar_drop_slot()));
        }
        if let Some(pill) = &fd.pill {
            let [x, y, w] = pill.pos.p;
            let i = index.get(pill.id.as_str()).copied();
            let show = fd.fx.get(&pill.id).map_or(1., |f| f.show.unit()) * i.map_or(1., |i| reveal[i]);
            let el = div().id("sidebar-pill").h(row_h).rounded(radius).bg(crate::sidebar::selection_fill().opacity(show));
            container = container.child(clipped(el, x, y, w, i.and_then(|i| clip[i]).filter(|_| !pill.travel)));
        }
        if let Some((s, a)) = &fd.slot {
            let [x, y, w] = s.p;
            container = container.child(div().absolute().left(px(x)).top(px(y)).w(px(w)).h(row_h).rounded(radius).bg(Theme::sidebar_drop_slot().opacity(a.unit())));
        }
        let batch = self.batch.clone();
        // Landing cards last, so a dropped folder sinks over the rows opening under it.
        let mut order: Vec<usize> = (0..rows.len()).collect();
        order.sort_by_key(|&i| fd.fx[&rows[i].entry.id].lift.p > 0.005);
        for i in order {
            let r = &rows[i];
            let id = r.entry.id.clone();
            let [x, y, w] = drawn[i];
            let fx = fd.fx[&id];
            let quiet = fd.pill.as_ref().is_some_and(|p| p.id == id) || batch.as_ref().is_some_and(|b| b.contains(&id)) || self.folder_drop_target(&id);
            let hover = if quiet { 0. } else { fx.hover.unit() };
            let lift = fx.lift.unit();
            // A landing card is never clipped: it is still above the list.
            let band = clip[i].filter(|_| lift < 0.005);
            if band.is_some_and(|(lo, hi)| hi - lo < 0.5 || y >= hi || y + PITCH <= lo) { continue; }
            let a = reveal[i];
            let hid = id.clone();
            let el = div().id(SharedString::from(format!("frow-{id}"))).h(row_h)
                .opacity(fx.show.unit() * a)
                .on_hover(cx.listener(move |w, on: &bool, _, cx| {
                    if *on { w.folder_drag.hovered = Some(hid.clone()); }
                    else if w.folder_drag.hovered.as_ref() == Some(&hid) { w.folder_drag.hovered = None; }
                    cx.notify();
                }))
                .when(lift > 0.005, |d| d.child(div().absolute().inset_0().rounded(radius)
                    .bg(theme::raised(Theme::bg_body()).opacity(lift)).shadow(faded(theme::shadow_panel(), lift))))
                .when(fx.glow.unit() > 0.005, |d| d.child(div().absolute().inset_0().rounded(radius).bg(Theme::sidebar_landed().opacity(fx.glow.unit()))))
                .when(hover > 0.005, |d| d.child(div().absolute().inset_0().rounded(radius).bg(crate::sidebar::hover().opacity(hover))))
                .child(self.doc_row(&r.entry, "doc", 0, r.entry.meta.folder.is_some(), cx));
            // Revealed rows settle the last few pixels as they fade in.
            container = container.child(clipped(el, x, y - 4. * (1. - a), w, band));
        }
        if let Some(d) = self.folder_drag.active.as_ref().filter(|d| d.started) {
            let b = self.list_scroll.bounds();
            let delta = if d.at.y < b.top() + px(24.) { 1. } else if d.at.y > b.bottom() - px(24.) { -1. } else { 0. };
            if delta != 0. && d.at.x >= b.left() && d.at.x <= b.right() {
                let mut offset = self.list_scroll.offset(); offset.y += px(delta * dt * 420.); self.list_scroll.set_offset(offset); moving = true;
            }
        }
        if moving { window.request_animation_frame(); }
        container.into_any_element()
    }

    /// A click on a row. A folder opens its page and unfolds, like a page does;
    /// its chevron (or clicking the folder already shown) only folds and unfolds
    /// it, and ⌥ on the chevron does the same to every folder inside.
    fn sidebar_click(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let on_icon = std::mem::take(&mut self.folder_drag.icon_press) && self.folder_drag.has_children(id);
        let all = std::mem::take(&mut self.folder_drag.icon_alt);
        if self.entries.iter().any(|e| e.id == id && e.meta.folder.is_some()) {
            let shown = self.view == crate::app::View::Folder && self.folder_current.as_deref() == Some(id);
            if on_icon || shown {
                let opening = !self.expanded.contains(id);
                let folders: Vec<String> = if on_icon && all {
                    crate::tree::Tree::new(&self.entries).subtree(id).into_iter()
                        .filter(|k| self.entries.iter().any(|e| &e.id == k && e.meta.folder.is_some())).collect()
                } else { vec![id.to_string()] };
                for f in folders { if opening { self.expanded.insert(f); } else { self.expanded.remove(&f); } }
            } else if !self.open_folder(id, window, cx) { return; }
            self.folder_drag.kick(id, 3.5);
            cx.notify();
        } else { self.open(id, window, cx); }
    }

    pub(crate) fn track_sidebar_press(&mut self, ev: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.folder_drag.active.as_ref().is_some_and(|d| !d.started) { return; }
        if ev.pressed_button == Some(MouseButton::Left) { self.move_folder_drag(ev.position, window, cx); }
        else { self.folder_drag.active = None; }
    }

    pub(crate) fn finish_sidebar_click(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let click = self.folder_drag.active.as_ref().filter(|d| !d.started).map(|d| d.id.clone());
        if let Some(id) = click { self.folder_drag.active = None; self.sidebar_click(&id, window, cx); }
    }

    pub(crate) fn render_folder_drag_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let d = self.folder_drag.active.as_ref().filter(|d| d.started)?;
        let mut layer = div().id("folder-drag-overlay").absolute().inset_0().occlude()
            .on_mouse_move(cx.listener(|w, ev: &MouseMoveEvent, window, cx| {
                if ev.pressed_button != Some(MouseButton::Left) { w.end_folder_drag(true, cx); return; }
                w.move_folder_drag(ev.position, window, cx);
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|w, _, window, cx| {
                let click = w.folder_drag.active.as_ref().filter(|d| !d.started).map(|d| d.id.clone());
                if let Some(id) = click { w.folder_drag.active = None; w.sidebar_click(&id, window, cx); }
                else { w.end_folder_drag(false, cx); }
            }));
        if d.started && let Some(e) = self.entries.iter().find(|e| e.id == d.id) {
            // Lifted: a touch wider and higher than the row, with a shadow that grows in.
            let [x, y, width] = d.ghost.p;
            let l = d.lift.p.max(0.);
            let inside = d.excluded.len().saturating_sub(1);
            layer = layer.child(div().absolute().left(px(x - 3. * l)).top(px(y - l)).w(px(width + 6. * l)).h(px(crate::sidebar::ROW_H)).pr(px(6.)).rounded(px(crate::sidebar::ROW_R))
                .bg(theme::raised(Theme::bg_body())).shadow(faded(theme::shadow_panel(), l.min(1.))).flex().items_center().text_color(Theme::text())
                .child(row_face(e, self.expanded.contains(&e.id)).flex_1().min_w_0().pl(px(6. + 3. * l)))
                .when(inside > 0, |g| g.child(div().flex_none().h(px(18.)).min_w(px(18.)).px(px(5.)).rounded(px(9.)).flex().items_center().justify_center()
                    .bg(Theme::sidebar_drop_slot()).text_size(px(11.)).text_color(crate::sidebar::text_color()).font_weight(FontWeight::MEDIUM)
                    .opacity(d.lift.unit()).child(inside.to_string()))));
        }
        Some(deferred(layer).with_priority(5).into_any_element())
    }
}
