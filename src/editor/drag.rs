//! Moving blocks by their handle: press the handle,
//! move past 2px and a card of the blocks follows the pointer. Where they
//! land a grey slot their size opens, as in the sidebar: the blocks between
//! slide aside on the sidebar's springs, drawn moved rather than laid out
//! again, and once let go the blocks glide from the card into it.

use super::core::{BlockSel, DropPlace, Seg, block_at, siblings_at};
use super::render::{Env, Item, R, TOP, flattens, margins, visible_with_gaps};
use super::view::Editor;
use crate::model::*;
use crate::feishu_theme::{self as theme, Theme};
use crate::sidebar_drag::{Fade, QUICK, Spring};
use gpui::{prelude::FluentBuilder, *};
use rustc_hash::FxHashMap;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;
use web_time::Instant;

/// Nesting deeper than this is refused (`pE=16`).
const MAX_DEPTH: usize = 16;
/// Children sit this far right of their parent.
const INDENT: f32 = 25.;
const FLASH: Duration = Duration::from_millis(2000);
/// The slot and the card over it reach this far past the blocks either side.
const PAD: f32 = 4.;
/// A taller card is cut here (the slot keeps the blocks' height).
const CARD_MAX_H: f32 = 360.;

/// What moved blocks are drawn in: the page's rows, or the blocks of one
/// block drawn inside it (a quote, callout or column, a list nested there).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Flow {
    Rows,
    In(BlockId),
}

/// `at..end` of a flow: indices of `Editor::items`, or of the block's children.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Run {
    flow: Flow,
    at: usize,
    end: usize,
}

#[derive(Clone, Debug)]
struct Land {
    key: (BlockId, DropPlace),
    run: Run,
    /// The slot as drawn over the page (window coordinates). None when it
    /// opens in a container the blocks don't come from: it is laid out
    /// there, growing the container.
    rect: Option<Bounds<Pixels>>,
    /// Then the blocks after `index` of a flow are laid out that much lower:
    /// the container's, and those after it and after each one it is in.
    grow: Vec<(Flow, usize, f32)>,
    /// The blocks that grow, by how much.
    grown: Vec<(BlockId, f32)>,
}

/// The springs of a block drag: the blocks making room and those gliding in
/// after the drop, and the slot drawn over the page.
#[derive(Default)]
pub(super) struct DragFx {
    /// How far each block is drawn from where it was before the drag (x, y),
    /// the frame it was last drawn in, and whether it is a dropped block
    /// gliding in.
    moves: FxHashMap<BlockId, (Spring<2>, u64, bool)>,
    slot: Option<(Spring<4>, Fade)>,
    /// The slot laid out in a container: fading in, and where it was drawn.
    spacer: Option<(Fade, Rc<Cell<Option<Bounds<Pixels>>>>)>,
    /// A block's top last frame, to carry the slot along when the page scrolls.
    anchor: Option<(BlockId, Pixels)>,
    frame: u64,
    last: Option<Instant>,
    dt: f32,
    moving: bool,
}

thread_local! {
    /// How far the blocks being prepainted are drawn from where they were
    /// before the drag; `record_bounds` takes it back off, so drops are
    /// found against that layout while the blocks slide.
    static DRAWN: Cell<(f32, f32)> = const { Cell::new((0., 0.)) };
}

/// Bounds as laid out before the drag.
pub(super) fn undrawn(b: Bounds<Pixels>) -> Bounds<Pixels> {
    let (x, y) = DRAWN.with(Cell::get);
    Bounds { origin: point(b.origin.x - px(x), b.origin.y - px(y)), size: b.size }
}

/// Draws its child `by` from where it was before the drag, of which `laid`
/// (down) is already in the layout. Nothing is laid out again.
pub(super) struct Shift {
    child: AnyElement,
    by: (f32, f32),
    laid: f32,
}

impl Shift {
    pub(super) fn wrap(child: impl IntoElement, by: Option<((f32, f32), f32)>) -> AnyElement {
        match by {
            Some((by, laid)) => Shift { child: child.into_any_element(), by, laid }.into_any_element(),
            None => child.into_any_element(),
        }
    }
}

impl Element for Shift {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        let (x, y) = DRAWN.with(Cell::get);
        DRAWN.with(|d| d.set((x + self.by.0, y + self.by.1)));
        window.with_element_offset(point(px(self.by.0), px(self.by.1 - self.laid)), |window| self.child.prepaint(window, cx));
        DRAWN.with(|d| d.set((x, y)));
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        self.child.paint(window, cx);
    }
}

/// A block's place among the page's rows (its child indices).
fn row_path(p: &[Seg]) -> Option<Vec<usize>> {
    p.iter().map(|s| if let Seg::Child(i) = s { Some(*i) } else { None }).collect()
}

/// Rows come in document order, so their paths are sorted: the first row
/// not before `p`.
fn rows_before(items: &[Item], p: &[usize]) -> usize {
    items.partition_point(|it| match it {
        Item::Header => true,
        Item::Block { path, .. } => path.as_slice() < p,
        Item::Below => false,
    })
}

/// The first row after `p` and the rows under it.
fn rows_after(items: &[Item], p: &[usize]) -> usize {
    items.partition_point(|it| match it {
        Item::Header => true,
        Item::Block { path, .. } => path.as_slice() < p || path.starts_with(p),
        Item::Below => false,
    })
}

pub struct BlockDrag {
    /// The moved blocks, topmost only, in document order.
    pub ids: Vec<BlockId>,
    /// A multi-block selection the press started on (the handle menu narrows
    /// the selection to its block; dragging moves them all).
    sel: Option<BlockSel>,
    from: Point<Pixels>,
    at: Point<Pixels>,
    pub started: bool,
    drop: Option<BlockDrop>,
    scrolled: Option<Instant>,
    /// Size of the moved blocks on the page when the drag started.
    size: Size<Pixels>,
    /// The pointer from the card's top-left, as pressed: the card keeps it.
    grab: Point<Pixels>,
    /// The handle pressed, and where it sits on the card.
    handle: (BlockId, Point<Pixels>),
    /// The window was active at the press (losing focus cancels).
    active: Option<bool>,
    /// Where the blocks are taken from (None: from several containers), and
    /// how far the blocks after them close up: their height and the margin
    /// above them.
    from_run: Option<Run>,
    pitch: f32,
    land: Option<Land>,
}

#[derive(Clone, Debug)]
struct BlockDrop {
    target: BlockId,
    place: DropPlace,
    /// Side placement creates or extends an image row instead of a vertical move.
    side: Option<bool>,
    /// The 2px line (window coordinates): where dropped files land, and
    /// where an image goes beside another.
    line: Bounds<Pixels>,
    /// The container the blocks land in, tinted (as laid out before the drag).
    area: Option<(BlockId, Bounds<Pixels>)>,
}

/// A drop found by walking the page under the pointer.
struct Hit {
    target: BlockId,
    place: DropPlace,
    /// Right edge of the container (None: the target's own).
    right: Option<Pixels>,
}

struct Walk<'a> {
    bb: &'a HashMap<BlockId, Bounds<Pixels>>,
    ids: &'a HashSet<BlockId>,
    p: Point<Pixels>,
}

impl Walk<'_> {
    /// The block's rows: its own and, when its children are rows of their
    /// own, theirs.
    fn span(&self, b: &Block) -> Option<Bounds<Pixels>> {
        let own = self.bb.get(&b.id).copied();
        if !flattens(b) {
            return own;
        }
        visible_with_gaps(&b.children, TOP)
            .into_iter()
            .filter_map(|(i, _)| self.span(&b.children[i]))
            .fold(own, |a, s| Some(a.map_or(s, |a| a.union(&s))))
    }

    fn nestable(b: &Block) -> bool {
        match b.kind {
            BlockKind::Bullet | BlockKind::Ordered { .. } | BlockKind::Todo { .. } => !b.folded,
            BlockKind::Text | BlockKind::Heading { .. } => !b.children.is_empty() && !b.folded,
            _ => false,
        }
    }

    fn level(&self, list: &[Block], right: Option<Pixels>) -> Option<Hit> {
        let (x, y) = (self.p.x, self.p.y);
        let vis: Vec<(&Block, Bounds<Pixels>)> =
            visible_with_gaps(list, TOP).into_iter().filter_map(|(i, _)| Some((&list[i], self.span(&list[i])?))).collect();
        let i = vis.iter().position(|(_, s)| s.bottom() >= y).unwrap_or(vis.len().checked_sub(1)?);
        let (b, span) = vis[i];
        let dragged = self.ids.contains(&b.id);
        if dragged && y >= span.top() {
            return None;
        }
        let own = *self.bb.get(&b.id)?;
        let hit = |target: BlockId, place| Some(Hit { target, place, right });
        let inside = y >= own.top() && y <= own.bottom();
        match &b.kind {
            BlockKind::Grid if inside => {
                let cols: Vec<(&Block, Bounds<Pixels>)> = b.children.iter().filter_map(|c| Some((c, *self.bb.get(&c.id)?))).collect();
                let (col, cb) = cols
                    .iter()
                    .min_by(|a, b| dist_x(x, &a.1).total_cmp(&dist_x(x, &b.1)))
                    .copied()?;
                return self.level(&col.children, Some(cb.right()));
            }
            BlockKind::Callout { .. } | BlockKind::Quote if inside => {
                let kids = visible_with_gaps(&b.children, TOP);
                let first = kids.first().and_then(|(i, _)| self.span(&b.children[*i]));
                let last = kids.last().and_then(|(i, _)| self.span(&b.children[*i]));
                if let (Some(f), Some(l)) = (first, last)
                    && y >= f.top()
                    && y <= l.bottom()
                {
                    let inset = if matches!(b.kind, BlockKind::Quote) { 0. } else { 16. };
                    return self.level(&b.children, Some(own.right() - px(inset)));
                }
                // The padding above and below the children: beside the box.
                return if y < own.center().y { self.before(&vis, i, right) } else { hit(b.id, DropPlace::After) };
            }
            _ => {}
        }
        if flattens(b) && y > own.bottom() {
            let r = self.level(&b.children, right)?;
            // Left of a last child: out to after its parent.
            let last = visible_with_gaps(&b.children, TOP).last().map(|(i, _)| b.children[*i].id);
            if r.place == DropPlace::After
                && Some(r.target) == last
                && self.bb.get(&r.target).is_some_and(|t| x < t.left() - px(5.))
            {
                return hit(b.id, DropPlace::After);
            }
            return Some(r);
        }
        if y < own.center().y {
            return self.before(&vis, i, right);
        }
        if dragged {
            return None;
        }
        if flattens(b) || (Self::nestable(b) && x - own.left() > px(36.)) {
            return hit(b.id, DropPlace::FirstChild);
        }
        hit(b.id, DropPlace::After)
    }

    /// Above block `i`: after the one before it when that has no rows below
    /// it (nesting under it when far enough right), else before `i`.
    fn before(&self, vis: &[(&Block, Bounds<Pixels>)], i: usize, right: Option<Pixels>) -> Option<Hit> {
        let b = vis[i].0;
        let hit = |target: BlockId, place| Some(Hit { target, place, right });
        match i.checked_sub(1).map(|j| vis[j].0) {
            Some(prev) if !flattens(prev) => {
                if self.ids.contains(&prev.id) {
                    return if self.ids.contains(&b.id) { None } else { hit(b.id, DropPlace::Before) };
                }
                let left = self.bb.get(&prev.id)?.left();
                if Self::nestable(prev) && self.p.x - left > px(36.) {
                    hit(prev.id, DropPlace::FirstChild)
                } else {
                    hit(prev.id, DropPlace::After)
                }
            }
            _ if self.ids.contains(&b.id) => None,
            _ => hit(b.id, DropPlace::Before),
        }
    }
}

fn dist_x(x: Pixels, b: &Bounds<Pixels>) -> f32 {
    if x < b.left() {
        f32::from(b.left() - x)
    } else if x > b.right() {
        f32::from(x - b.right())
    } else {
        0.
    }
}

impl Editor {
    pub(super) fn is_dragged(&self, id: BlockId) -> bool {
        self.block_drag.as_ref().is_some_and(|d| d.started && d.ids.contains(&id))
    }

    /// A press on a block handle: the drag starts once the pointer moves.
    pub(super) fn press_handle(&mut self, id: BlockId, at: Point<Pixels>) {
        if matches!(self.core.kind_of(id), None | Some(BlockKind::Grid | BlockKind::Column { .. })) {
            return;
        }
        let multi = self.core.block_sel_ids();
        let (ids, sel) = if multi.len() > 1 && multi.contains(&id) {
            (multi, self.core.block_sel)
        } else {
            // A folded heading carries its hidden section.
            let keep = self.core.block_sel;
            self.core.select_block(id);
            let ids = self.core.block_sel_ids();
            self.core.block_sel = keep;
            (ids, None)
        };
        let bb = self.block_bounds.borrow();
        let empty = HashSet::new();
        let w = Walk { bb: &bb, ids: &empty, p: at };
        let index = super::core::block_index(&self.core.doc);
        let union = ids_union(&w, &index, &ids);
        drop(bb);
        let size = union.map_or(size(px(0.), px(0.)), |b| b.size);
        // The card starts as the blocks' slot: where they are, PAD wider.
        let card = union.map_or(at, |b| point(b.left() - px(PAD), b.top()));
        let handle = self.handle_rect(id, false).map_or(point(px(-48. - PAD), px(0.)), |h| h.origin - card);
        let run = self.run_of(&ids);
        // The margin that goes with them: below them (above the block
        // after), or above them when they end their list.
        let gap = match run {
            Some(Run { flow: Flow::Rows, at, end }) => [end, at].iter().find_map(|i| match self.items.get(*i) { Some(Item::Block { gap, .. }) => Some(*gap), _ => None }).unwrap_or(0.),
            Some(Run { flow: Flow::In(p), at, end }) => self.core.block(p).map_or(0., |p| {
                let gaps = visible_with_gaps(&p.children, TOP);
                gaps.iter().find(|(i, _)| *i >= end).or(gaps.iter().find(|(i, _)| *i == at)).map_or(0., |(_, g)| *g)
            }),
            None => 0.,
        };
        let pitch = f32::from(size.height) + gap;
        self.block_drag = Some(BlockDrag { ids, sel, from: at, at, started: false, drop: None, scrolled: None, size, grab: at - card, handle: (id, handle), active: None, from_run: run, pitch, land: None });
    }

    pub(super) fn block_drag_move(&mut self, at: Point<Pixels>, pressed: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !pressed {
            return self.end_block_drag(None, window, cx);
        }
        let Some(d) = &mut self.block_drag else { return };
        d.at = at;
        if !d.started {
            let m = d.at - d.from;
            if f32::from(m.x).hypot(f32::from(m.y)) <= 2. {
                return;
            }
            d.started = true;
            if let Some(sel) = d.sel {
                self.core.block_sel = Some(sel);
            }
            self.menu = None;
            self.slash = None;
            self.toolbar_hidden = true;
        }
        self.update_block_drop();
        cx.notify();
    }

    pub(super) fn end_block_drag(&mut self, at: Option<Point<Pixels>>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.block_drag.take() else { return };
        if !d.started {
            return;
        }
        self.toolbar_hidden = false;
        let m = at.unwrap_or(d.at) - d.from;
        // A press that hardly moved is a click.
        if f32::from(m.x).hypot(f32::from(m.y)) > 10.
            && let Some(drop) = d.drop
            && match drop.side {
                Some(before) => self.core.move_image_beside(d.ids[0], drop.target, before),
                None => self.core.move_blocks(&d.ids, drop.target, drop.place),
            }
        {
            self.core.clear_block_sel();
            // The page is laid out anew as it was drawn; the blocks glide
            // from the card into their slot.
            let fx = self.drag_fx.get_mut();
            let slot = fx.slot.as_ref().filter(|(_, a)| a.to > 0.).map(|(s, _)| point(px(s.p[0]), px(s.p[1])))
                .or_else(|| fx.spacer.as_ref().and_then(|(_, r)| r.get()).map(|r| r.origin));
            fx.moves.clear();
            fx.slot = None;
            if let Some(slot) = slot.filter(|_| drop.side.is_none()) {
                let card = d.at - d.grab;
                let by = [f32::from(card.x - slot.x), f32::from(card.y - slot.y)];
                for id in &d.ids {
                    fx.moves.insert(*id, (Spring { p: by, v: [0.; 2], target: [0.; 2] }, fx.frame, true));
                }
            }
            self.drop_flash = Some((d.ids, Instant::now()));
            self.changed(window, cx);
        }
        cx.notify();
    }

    /// Recomputes the drop under the pointer from last frame's layout, and
    /// the slot it opens.
    fn update_block_drop(&mut self) {
        let Some(d) = &self.block_drag else { return };
        if !d.started {
            return;
        }
        // The card finds the drop: the left of the blocks on it (for the
        // column and nesting) at its middle.
        let card = d.at - d.grab;
        let p = point(card.x + px(PAD), card.y + d.size.height.min(px(CARD_MAX_H)) / 2.);
        // Over the slot, it stays: the blocks around it moved for it.
        let slot = d.land.as_ref().and_then(|l| l.rect.or_else(|| self.drag_fx.borrow().spacer.as_ref().and_then(|(_, r)| r.get())));
        if d.drop.is_some() && slot.is_some_and(|r| r.contains(&p)) {
            return;
        }
        let drop = self.find_drop(p, &d.ids);
        let key = drop.as_ref().filter(|d| d.side.is_none()).map(|d| (d.target, d.place));
        let land = match (key, &d.land) {
            (Some(k), Some(l)) if l.key == k => Some(l.clone()),
            (Some((target, place)), _) => self.landing(target, place).map(|run| Land { key: (target, place), run, rect: None, grow: vec![], grown: vec![] }),
            _ => None,
        };
        let land = land.map(|mut l| {
            let spacer = matches!(l.run.flow, Flow::In(_)) && d.from_run.is_none_or(|f| f.flow != l.run.flow);
            l.rect = None;
            (l.grow, l.grown) = (vec![], vec![]);
            match (spacer, l.run.flow, &drop) {
                (true, Flow::In(p), _) => (l.grow, l.grown) = self.growth(p, d.pitch),
                (false, _, Some(drop)) => l.rect = Some(self.slot_rect(d, drop, l.run)),
                _ => {}
            }
            l
        });
        if let Some(d) = &mut self.block_drag {
            d.drop = drop;
            d.land = land;
        }
    }

    /// The slot drawn over the page: at the drop's line, as wide as the
    /// place the blocks land in and as tall as they are. Moving down, the
    /// blocks above it moved up by the blocks' height.
    fn slot_rect(&self, d: &BlockDrag, drop: &BlockDrop, run: Run) -> Bounds<Pixels> {
        let h = d.size.height;
        let gap = px(d.pitch) - h;
        let l = drop.line;
        let left = l.left() + if drop.place == DropPlace::FirstChild { px(INDENT) } else { px(0.) };
        // The bottom of what is above the slot.
        let above = if drop.place == DropPlace::Before { l.top() - gap } else { l.top() };
        let down = d.from_run.is_some_and(|f| f.flow == run.flow && run.at >= f.end);
        let top = if down { above - h } else { above + gap };
        Bounds { origin: point(left - px(PAD), top), size: size((l.right() - left).max(px(INDENT * 2.)) + px(2. * PAD), h) }
    }

    /// The slot's size, which the card over it shares: as wide as where the
    /// blocks land (or came from) and PAD either side, as tall as they are.
    fn drag_box(&self, d: &BlockDrag) -> Size<Pixels> {
        let w = match &d.land {
            Some(Land { rect: Some(r), .. }) => Some(r.size.width),
            Some(_) => self.drag_fx.borrow().spacer.as_ref().and_then(|(_, r)| r.get()).map(|r| r.size.width),
            None => None,
        };
        size(w.unwrap_or(d.size.width + px(2. * PAD)), d.size.height)
    }

    /// The flow block `id` is drawn in, and its index there.
    fn place_of(&self, id: BlockId) -> Option<(Flow, usize)> {
        let path = self.core.path(id)?;
        if let Some(rows) = row_path(&path) {
            let ix = rows_before(&self.items, &rows);
            if matches!(self.items.get(ix), Some(Item::Block { path, .. }) if *path == rows) {
                return Some((Flow::Rows, ix));
            }
        }
        let Seg::Child(i) = *path.last()? else { return None };
        Some((Flow::In(self.core.parent_of(id)?.id), i))
    }

    /// Where the moved blocks are drawn: a run of rows (with the rows under
    /// them), or of one container's blocks.
    fn run_of(&self, ids: &[BlockId]) -> Option<Run> {
        let places: Vec<(Flow, usize)> = ids.iter().map(|id| self.place_of(*id)).collect::<Option<_>>()?;
        let (flow, at) = *places.first()?;
        if places.iter().any(|(f, _)| *f != flow) {
            return None;
        }
        let end = match flow {
            Flow::Rows => rows_after(&self.items, &row_path(&self.core.path(*ids.last()?)?)?),
            Flow::In(_) => places.last()?.1 + 1,
        };
        Some(Run { flow, at, end })
    }

    /// Where blocks dropped `place` of `target` are drawn.
    fn landing(&self, target: BlockId, place: DropPlace) -> Option<Run> {
        let (flow, i) = self.place_of(target)?;
        let at = match (flow, place) {
            // Under a row that has (or will have) rows under it.
            (Flow::Rows, DropPlace::FirstChild) if self.core.kind_of(target)?.nests() => i + 1,
            (_, DropPlace::FirstChild) => return Some(Run { flow: Flow::In(target), at: 0, end: 0 }),
            (Flow::Rows, DropPlace::After) => rows_after(&self.items, &row_path(&self.core.path(target)?)?),
            (_, DropPlace::After) => i + 1,
            (_, DropPlace::Before) => i,
        };
        Some(Run { flow, at, end: at })
    }

    /// A slot `h` tall laid out in `p`: what grows and moves down with it,
    /// up to the page's rows. A grid grows only by what its column now
    /// passes its tallest by.
    fn growth(&self, p: BlockId, h: f32) -> (Vec<(Flow, usize, f32)>, Vec<(BlockId, f32)>) {
        let (mut grow, mut grown) = (vec![], vec![(p, h)]);
        let (mut cur, mut g) = (p, h);
        let bb = self.block_bounds.borrow();
        while let Some((flow, i)) = self.place_of(cur) {
            if let Flow::In(grid) = flow
                && let Some(gb) = self.core.block(grid).filter(|b| matches!(b.kind, BlockKind::Grid))
            {
                // Each column's blocks reach this far down.
                let reach = |c: &Block| visible_with_gaps(&c.children, TOP).last().and_then(|(k, _)| bb.get(&c.children[*k].id)).map(|b| f32::from(b.bottom()));
                let cols: Vec<_> = gb.children.iter().filter_map(|c| Some((c.id, bb.get(&c.id)?.left(), reach(c)?))).collect();
                let side = cols.windows(2).all(|w| w[0].1 != w[1].1);
                if side && let Some(&(_, _, mine)) = cols.iter().find(|c| c.0 == cur) {
                    let tallest = cols.iter().map(|c| c.2).fold(f32::MIN, f32::max);
                    g = (mine + g - tallest).max(0.);
                }
                grown.retain(|(id, _)| *id != cur);
                grown.extend(gb.children.iter().map(|c| (c.id, g)));
                grown.push((grid, g));
                cur = grid;
                continue;
            }
            grow.push((flow, i, g));
            let Flow::In(up) = flow else { break };
            grown.push((up, g));
            cur = up;
        }
        (grow, grown)
    }

    /// How far the block at `ix` of `flow` is drawn from where it was before
    /// the drag, making room, and how much of that is in its layout.
    fn room(&self, flow: Flow, ix: usize) -> (f32, f32) {
        let Some(d) = self.block_drag.as_ref().filter(|d| d.started) else { return (0., 0.) };
        let Some(land) = &d.land else { return (0., 0.) };
        let (h, l) = (d.pitch, land.run);
        let (mut want, mut laid) = (0., 0.);
        match d.from_run {
            Some(f) if f.flow == flow && l.flow == flow => {
                if l.at >= f.end && ix >= f.end && ix < l.at {
                    want = -h;
                } else if l.at <= f.at && ix >= l.at && ix < f.at {
                    want = h;
                }
            }
            f => {
                // Taken from the page's rows: those below close up.
                if f.is_some_and(|f| f.flow == flow && flow == Flow::Rows && ix >= f.end) {
                    want -= h;
                }
                if l.flow == flow && ix >= l.at {
                    want += h;
                    if land.rect.is_none() {
                        laid += h;
                    }
                }
            }
        }
        for &(gf, gi, g) in &land.grow {
            if gf == flow && ix > gi {
                want += g;
                laid += g;
            }
        }
        (want, laid)
    }

    /// Whether block `id` is a dropped block gliding into its place.
    pub(super) fn gliding(&self, id: BlockId) -> bool {
        self.drag_fx.borrow().moves.get(&id).is_some_and(|m| m.2)
    }

    /// How the block at `ix` of `flow` is drawn moved this frame (stepping
    /// its spring): None where it is drawn as laid out. `id` is the block,
    /// or the dropped block it glides in with.
    pub(super) fn drawn_shift(&self, flow: Flow, ix: usize, id: BlockId) -> Option<((f32, f32), f32)> {
        let dragging = self.block_drag.as_ref().is_some_and(|d| d.started);
        let mut fx = self.drag_fx.borrow_mut();
        let fx = &mut *fx;
        if fx.moves.is_empty() && !dragging {
            return None;
        }
        let (want, laid) = self.room(flow, ix);
        let (frame, dt) = (fx.frame, fx.dt);
        if !dragging && !fx.moves.contains_key(&id) {
            return None;
        }
        let m = fx.moves.entry(id).or_insert((Spring::new([0., want]), frame, false));
        if !m.2 {
            m.0.target = [0., want];
            // Off screen last frame: in place at once.
            if m.1 + 1 < frame {
                m.0 = Spring::new([0., want]);
            }
        }
        let mut moving = false;
        if m.1 != frame {
            m.1 = frame;
            moving = m.0.step(dt);
        }
        let p = m.0.p;
        fx.moving |= moving;
        (p != [0.; 2] || laid != 0.).then_some(((p[0], p[1]), if m.2 { 0. } else { laid }))
    }

    /// Where the slot opens in `flow` when it is laid out there: before the
    /// first block from this index on.
    pub(super) fn drag_slot_at(&self, flow: Flow) -> Option<usize> {
        let l = self.block_drag.as_ref().filter(|d| d.started)?.land.as_ref()?;
        (l.run.flow == flow && l.rect.is_none()).then_some(l.run.at)
    }

    /// The slot laid out in a container: the margin above, then grey, as
    /// tall as the moved blocks.
    pub(super) fn slot_el(&self) -> AnyElement {
        let Some(d) = self.block_drag.as_ref() else { return div().into_any_element() };
        let mut fx = self.drag_fx.borrow_mut();
        let (a, at) = fx.spacer.get_or_insert_with(|| (QUICK.from(0., 1.), Rc::new(Cell::new(None)))).clone();
        let h = d.size.height;
        div()
            .pt(px(d.pitch) - h)
            .child(
                div().h(h).relative().child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(-PAD))
                        .right(px(-PAD))
                        .rounded(px(crate::sidebar::ROW_R))
                        .bg(crate::theme::Theme::sidebar_drop_slot().opacity(a.unit()))
                        .child(canvas(move |b, _, _| at.set(Some(b)), |_, _, _, _| {}).size_full()),
                ),
            )
            .into_any_element()
    }

    /// Steps the slot's springs once a frame, before the page is drawn, and
    /// lets settled blocks go.
    pub(super) fn tick_drag_fx(&mut self, window: &mut Window) {
        let started = self.block_drag.as_ref().filter(|d| d.started);
        // The slot over the page; with no drop, the place the blocks left.
        let target = started.and_then(|d| match &d.land {
            Some(l) => l.rect,
            None if d.drop.is_none() => {
                let bb = self.block_bounds.borrow();
                let w = Walk { bb: &bb, ids: &HashSet::new(), p: d.at };
                let index: rustc_hash::FxHashMap<BlockId, &Block> = d.ids.iter().filter_map(|id| Some((*id, self.core.block(*id)?))).collect();
                ids_union(&w, &index, &d.ids).map(|b| Bounds { origin: point(b.left() - px(PAD), b.top()), size: size(b.size.width + px(2. * PAD), b.size.height) })
            }
            None => None,
        });
        let spacer = started.and_then(|d| d.land.as_ref()).is_some_and(|l| l.rect.is_none());
        let anchor = started.and_then(|d| d.drop.as_ref().map(|x| x.target).or(d.ids.first().copied()))
            .and_then(|id| Some((id, self.block_bounds.borrow().get(&id)?.top())));
        let fx = self.drag_fx.get_mut();
        let now = Instant::now();
        fx.dt = fx.last.replace(now).map_or(1. / 120., |l| now.duration_since(l).as_secs_f32().min(0.032));
        fx.frame += 1;
        let frame = fx.frame;
        let dt = fx.dt;
        // The page scrolled: the slot goes with it.
        if let (Some((id, top)), Some((was, then)), Some((s, _))) = (anchor, fx.anchor, &mut fx.slot)
            && id == was
        {
            s.p[1] += f32::from(top - then);
        }
        fx.anchor = anchor;
        let t = target.map(|r| [f32::from(r.left()), f32::from(r.top()), f32::from(r.size.width), f32::from(r.size.height)]);
        match (t, &mut fx.slot) {
            (Some(t), Some((s, a))) => {
                if a.p < 0.02 {
                    *s = Spring::new(t);
                }
                // Bounds come back rounded to the pixel as blocks slide: a
                // move under a pixel is that, not a new place.
                if t.iter().zip(s.target).any(|(a, b)| (a - b).abs() >= 1.) {
                    s.target = t;
                }
                a.to = 1.;
            }
            (Some(t), None) => fx.slot = Some((Spring::new(t), QUICK.from(0., 1.))),
            (None, Some((_, a))) => a.to = 0.,
            (None, None) => {}
        }
        let mut moving = std::mem::take(&mut fx.moving);
        if let Some((s, a)) = &mut fx.slot {
            let m = s.step(dt) | a.step(dt);
            moving |= m;
            if !m && a.to == 0. {
                fx.slot = None;
            }
        }
        if !spacer {
            fx.spacer = None;
        } else if let Some((a, _)) = &mut fx.spacer {
            moving |= a.step(dt);
        }
        let dragging = started.is_some();
        fx.moves.retain(|_, (s, seen, _)| *seen + 1 >= frame && (dragging || s.p != [0.; 2] || s.v != [0.; 2]));
        moving |= fx.moves.values().any(|(s, _, _)| s.p != s.target || s.v != [0.; 2]);
        if moving {
            window.request_animation_frame();
        } else if !dragging {
            fx.last = None;
        }
    }

    /// Whether blocks are moving or settling: only then are they drawn moved.
    pub(super) fn drag_fx_live(&self) -> bool {
        self.block_drag.as_ref().is_some_and(|d| d.started) || !self.drag_fx.borrow().moves.is_empty()
    }

    /// Asks for another frame while a block drawn this frame still moves.
    pub(super) fn drag_fx_moving(&self) -> bool {
        self.drag_fx.borrow().moving
    }

    /// Where dropped files land: the block-drag drop under `p` (target,
    /// place and the 2px line).
    pub(super) fn file_drop_at(&self, p: Point<Pixels>) -> Option<(BlockId, DropPlace, Bounds<Pixels>)> {
        self.find_drop(p, &[]).map(|d| (d.target, d.place, d.line))
    }

    fn find_drop(&self, p: Point<Pixels>, ids: &[BlockId]) -> Option<BlockDrop> {
        if !self.list.viewport_bounds().contains(&p) {
            return None;
        }
        // Reserve the middle of each side for horizontal drops; top/bottom still reorder.
        if let [image] = ids {
            for (&target, bounds) in self.img.boxes.borrow().iter() {
                if !self.core.can_move_image_beside(*image, target) { continue; }
                let edge = bounds.size.width * 0.25;
                let central_y = p.y > bounds.top() + bounds.size.height * 0.2
                    && p.y < bounds.bottom() - bounds.size.height * 0.2;
                let left = p.x >= bounds.left() - px(24.) && p.x <= bounds.left() + edge;
                let right = p.x >= bounds.right() - edge && p.x <= bounds.right() + px(24.);
                if central_y && (left || right) {
                    let x = if left { bounds.left() - px(4.) } else { bounds.right() + px(2.) };
                    let line = Bounds::from_corners(point(x, bounds.top()), point(x + px(2.), bounds.bottom()));
                    return Some(BlockDrop { target, place: DropPlace::After, side: Some(left), line, area: None });
                }
            }
        }
        let laid = self.block_bounds.borrow();
        // A slot laid out in a container grows it and those it is in: find
        // the drop against them as they were.
        let grown = self.block_drag.as_ref().and_then(|d| d.land.as_ref()).map_or(&[][..], |l| &l.grown);
        let mut was;
        let bb = if grown.is_empty() {
            &*laid
        } else {
            was = laid.clone();
            for (id, g) in grown {
                if let Some(b) = was.get_mut(id) {
                    b.size.height -= px(*g);
                }
            }
            &was
        };
        let set: HashSet<BlockId> = ids.iter().copied().collect();
        let w = Walk { bb, ids: &set, p };
        // Only the top-level blocks on screen (and one either side) can be
        // under the pointer: a long page has tens of thousands.
        let blocks = &self.core.doc.blocks;
        let top = self.rendered_top.get().map_or(0..blocks.len(), |(a, z)| {
            let z = (z + 2).min(blocks.len());
            a.saturating_sub(1).min(z)..z
        });
        let hit = w.level(&blocks[top], None)?;
        let core = &self.core;
        // Where the blocks land: the parent they join.
        let parent = match hit.place {
            DropPlace::FirstChild => Some(hit.target),
            _ => core.parent_of(hit.target).map(|b| b.id),
        };
        let chain: Vec<&Block> = match parent {
            Some(par) => {
                let path = core.path(par)?;
                (1..=path.len()).filter_map(|n| block_at(&core.doc.blocks, &path[..n])).collect()
            }
            None => vec![],
        };
        if chain.len() + 1 > MAX_DEPTH {
            return None;
        }
        let in_quote = chain.iter().any(|b| matches!(b.kind, BlockKind::Quote));
        let in_callout = chain.iter().any(|b| matches!(b.kind, BlockKind::Callout { .. }));
        for id in ids {
            let ok = match core.kind_of(*id)? {
                BlockKind::Grid => chain.is_empty(),
                BlockKind::Quote => !in_quote,
                BlockKind::Callout { .. } | BlockKind::Table(_) => !in_quote && !in_callout,
                _ => true,
            };
            if !ok {
                return None;
            }
        }

        let t = core.block(hit.target)?;
        let own = *bb.get(&hit.target)?;
        let span = w.span(t)?;
        let right = hit.right.unwrap_or(own.right());
        let top = match hit.place {
            DropPlace::Before => own.top(),
            DropPlace::After => span.bottom(),
            DropPlace::FirstChild => own.bottom(),
        };
        let line = Bounds::from_corners(point(own.left(), top), point(right.max(own.left() + px(INDENT * 2.)), top + px(2.)));
        let area = chain
            .last()
            .filter(|b| !matches!(b.kind, BlockKind::Column { .. } | BlockKind::Callout { .. } | BlockKind::Grid))
            .and_then(|b| Some((b.id, w.span(b)?)));
        Some(BlockDrop { target: hit.target, place: hit.place, side: None, line, area })
    }

    /// Within 20px of the page's top or bottom edge the page scrolls 6px
    /// every 10ms; the drop follows the page.
    pub(super) fn block_autoscroll(&mut self, window: &mut Window) {
        let active = window.is_window_active();
        if let Some(d) = &mut self.block_drag
            && *d.active.get_or_insert(active)
            && !active
        {
            // Leaving the window drops nothing.
            self.block_drag = None;
            self.toolbar_hidden = false;
            return;
        }
        self.update_block_drop();
        let v = self.list.viewport_bounds();
        let Some(d) = &mut self.block_drag else { return };
        let dir = if !d.started {
            0.
        } else if d.at.y < v.top() + px(20.) {
            -1.
        } else if d.at.y > v.bottom() - px(20.) {
            1.
        } else {
            0.
        };
        if dir == 0. {
            d.scrolled = None;
            return;
        }
        let now = Instant::now();
        let dt = d.scrolled.map_or(0., |t| (now - t).as_secs_f32().min(0.05));
        d.scrolled = Some(now);
        self.list.scroll_by(px(dir * 600. * dt));
        window.request_animation_frame();
    }

    /// Moved blocks flash in the accent for 2s, as light as the
    /// selected-block tint.
    pub(super) fn drop_flash_el(&self, id: BlockId) -> Option<Div> {
        let (ids, t0) = self.drop_flash.as_ref()?;
        if !ids.contains(&id) {
            return None;
        }
        let k = t0.elapsed().as_secs_f32() / FLASH.as_secs_f32();
        let a = if k < 0.2 { k / 0.2 } else if k < 0.8 { 1. } else { ((1. - k) / 0.2).max(0.) };
        Some(div().absolute().inset_0().rounded(px(3.)).bg(Theme::block_selected().opacity(a)))
    }

    /// Ends the flash once it has run (and keeps frames coming until then).
    pub(super) fn tick_drop_flash(&mut self, window: &mut Window) {
        if let Some((_, t0)) = &self.drop_flash {
            if t0.elapsed() >= FLASH {
                self.drop_flash = None;
            }
            window.request_animation_frame();
        }
    }

    /// The layer over the page while blocks move: the grabbing cursor, the
    /// slot, and the card following the pointer.
    pub(super) fn block_drag_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let d = self.block_drag.as_ref().filter(|d| d.started)?;
        let o = self.content_origin.get();
        let mut layer = div().id("block-drag-overlay").absolute().inset_0().occlude().cursor(CursorStyle::ClosedHand);
        let rect = |b: Bounds<Pixels>| div().absolute().left(b.left() - o.x).top(b.top() - o.y).w(b.size.width).h(b.size.height);
        if let Some(drop) = &d.drop {
            if let Some((id, a)) = drop.area {
                // As drawn: moved with the blocks around it, around the slot.
                let (t, b) = self.painted_span(id);
                let mut a = Bounds::from_corners(point(a.left(), a.top() + px(t)), point(a.right(), a.bottom() + px(b)));
                if let Some(r) = d.land.as_ref().and_then(|l| l.rect) {
                    a = a.union(&r);
                }
                layer = layer.child(rect(a).rounded(px(3.)).bg(Theme::primary().opacity(25. / 255.)));
            }
            // Beside an image: the 2pt accent line, its ends rounded.
            if drop.side.is_some() {
                layer = layer.child(rect(drop.line).rounded(px(1.)).bg(Theme::primary()));
            }
        }
        if let Some((s, a)) = &self.drag_fx.borrow().slot {
            let [x, y, w, h] = s.p;
            layer = layer.child(
                rect(Bounds { origin: point(px(x), px(y)), size: size(px(w), px(h)) })
                    .rounded(px(crate::sidebar::ROW_R))
                    .bg(crate::theme::Theme::sidebar_drop_slot().opacity(a.unit())),
            );
        }
        // The card keeps the pointer where it grabbed the handle.
        let card = d.at - d.grab - o;
        Some(deferred(layer.child(self.drag_ghost(d, cx)?.left(card.x).top(card.y))).with_priority(4).into_any_element())
    }

    /// How far the top and the bottom of block `id` (with any rows under it)
    /// are drawn from where they were before the drag.
    fn painted_span(&self, id: BlockId) -> (f32, f32) {
        let Some((flow, ix)) = self.place_of(id) else { return (0., 0.) };
        let grown = self.block_drag.as_ref().and_then(|d| d.land.as_ref()).and_then(|l| l.grown.iter().find(|g| g.0 == id)).map_or(0., |g| g.1);
        let last = match flow {
            Flow::Rows => self.core.path(id).and_then(|p| row_path(&p)).map_or(ix, |p| rows_after(&self.items, &p).saturating_sub(1).max(ix)),
            Flow::In(_) => ix,
        };
        (self.room(flow, ix).0, self.room(flow, last).0 + grown)
    }

    /// The moved blocks, drawn again on a card the slot's size and shape,
    /// lifted (the page's colour and a soft shadow), with the handle beside
    /// them as on the page. A tall card is cut at `CARD_MAX_H`.
    fn drag_ghost(&self, d: &BlockDrag, cx: &mut Context<Self>) -> Option<Div> {
        let b = self.drag_box(d);
        let w = f32::from(b.width) - 2. * PAD;
        if d.size.width <= px(0.) {
            return None;
        }
        let r = R {
            ed: self,
            units: Rc::new(vec![]),
            eqnos: self.frame.eqnos.clone(),
            focused: false,
            selected: Rc::new(HashSet::new()),
            targets: RefCell::new(vec![]),
            flat: None,
            cache: Rc::new(RefCell::new(HashMap::new())),
            bounds: Rc::new(RefCell::new(HashMap::new())),
            ghost: true,
        };
        let core = &self.core;
        let mut col = div().flex().flex_col().pl(px(PAD)).w(b.width);
        let mut prev_mb: Option<f32> = None;
        for id in &d.ids {
            let Some(path) = core.path(*id) else { continue };
            let Some((list, ix)) = siblings_at(&core.doc.blocks, &path) else { continue };
            let b = &list[ix];
            let depth = path.len() - 1;
            let env = Env { depth, root: depth == 0, width: w, ..TOP };
            let (mt, mb) = margins(b, env);
            col = col.child(div().w(px(w)).mt(px(prev_mb.map_or(0., |p| p.max(mt)))).child(r.block(b, list, ix, env, cx)));
            prev_mb = Some(mb);
        }
        let h = b.height.min(px(CARD_MAX_H));
        let card = div()
            .absolute()
            .size_full()
            .overflow_hidden()
            .rounded(px(crate::sidebar::ROW_R))
            .bg(crate::theme::raised(Theme::bg_body()))
            .shadow(theme::shadow_panel())
            .child(col);
        let (id, at) = d.handle;
        Some(
            div()
                .absolute()
                .w(b.width)
                .h(h)
                .child(card)
                .children(self.handle_face(id).map(|f| div().absolute().left(at.x).top(at.y).child(f))),
        )
    }
}

/// The list a block sits in, and its index there.
fn ids_union(w: &Walk, index: &rustc_hash::FxHashMap<BlockId, &Block>, ids: &[BlockId]) -> Option<Bounds<Pixels>> {
    ids.iter().filter_map(|id| w.span(index.get(id)?)).reduce(|a, b| a.union(&b))
}

impl R<'_> {
    /// A table on the drag card: cells and borders only (the page's table
    /// keeps its own bar and scroll state).
    pub(super) fn ghost_table(&self, b: &Block, t: &Table, cx: &mut Context<Editor>) -> AnyElement {
        let env = Env { cell: true, root: false, ..TOP };
        let widths = self.ed.table_widths(b.id, t);
        let n = widths.len();
        let mut grid = div().grid().grid_col_widths(widths.clone()).w(px(widths.iter().sum()));
        for (r, c, cell) in t.cells() {
            let (rs, cs) = (cell.rowspan.max(1) as usize, cell.colspan.max(1) as usize);
            let header = r < t.header_rows || (t.header_col && c == 0);
            let bg = cell.bg.as_ref().and_then(|x| theme::block_bg(&x.0)).or(header.then(Theme::table_header));
            grid = grid.child(
                div()
                    .row_start(r as i16 + 1)
                    .row_end((r + rs) as i16 + 1)
                    .col_start(c as i16 + 1)
                    .col_end((c + cs) as i16 + 1)
                    .w(px(widths[c..(c + cs).min(n)].iter().sum()))
                    .p(px(8.))
                    .pb(px(9.))
                    .when(r == 0, |d| d.border_t_1())
                    .when(c == 0, |d| d.border_l_1())
                    .border_r_1()
                    .border_b_1()
                    .border_color(Theme::line_border())
                    .when_some(bg, |d, c| d.bg(c))
                    .child(self.blocks(&cell.blocks, env, cx)),
            );
        }
        div().child(grid).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{Item, Seg, row_path, rows_after, rows_before};

    #[test]
    fn rows_are_found_by_their_paths_in_document_order() {
        let row = |p: &[usize]| Item::Block { path: p.to_vec(), gap: 0. };
        let items = vec![Item::Header, row(&[0]), row(&[1]), row(&[1, 0]), row(&[1, 0, 0]), row(&[1, 1]), row(&[2]), Item::Below];
        assert_eq!(rows_before(&items, &[1]), 2);
        assert_eq!(rows_before(&items, &[1, 1]), 5);
        // A row with rows under it ends after the last of them.
        assert_eq!(rows_after(&items, &[1]), 6);
        assert_eq!(rows_after(&items, &[1, 0]), 5);
        assert_eq!(rows_after(&items, &[2]), 7);
        assert_eq!(rows_before(&items, &[3]), 7);
        assert_eq!(row_path(&[Seg::Child(1), Seg::Child(0)]), Some(vec![1, 0]));
        assert_eq!(row_path(&[Seg::Child(1), Seg::Cell(0, 0, 0)]), None);
    }
}
