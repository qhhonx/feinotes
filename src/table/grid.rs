//! 表格视图: the grid. Only the rows in sight are built each frame (`tops`
//! gives every row's place), the frozen columns stay put while the rest
//! shift by `scroll.x`, and pointer gestures (range select, fill handle,
//! column width, column / row reorder) are tracked on the view's root from
//! the body's painted bounds.

use super::style::*;
use super::*;
use crate::ui::ui_text;

/// The columns of a view as laid out: ids, widths, left edges (the gutter at 0).
pub(crate) struct Cols {
    pub ids: Vec<FieldId>,
    pub w: Vec<f32>,
    pub x: Vec<f32>,
    pub frozen: usize,
    /// Gutter plus the frozen columns.
    pub frozen_w: f32,
    /// Right edge of the last column.
    pub total: f32,
}

impl Cols {
    pub fn index(&self, f: FieldId) -> Option<usize> { self.ids.iter().position(|i| *i == f) }
}

impl TableView {
    pub(crate) fn cols(&self, t: &Table, v: &View) -> Cols {
        let fields = t.visible_fields(v);
        let ids: Vec<FieldId> = fields.iter().map(|f| f.id).collect();
        let w: Vec<f32> = ids.iter().map(|id| match &self.drag { Some(Drag::ColWidth { field, w, .. }) if field == id => *w, _ => t.width(v, *id) }).collect();
        let mut x = Vec::with_capacity(w.len());
        let mut at = GUTTER_W;
        for cw in &w { x.push(at); at += cw; }
        let frozen = v.opts.frozen.min(ids.len());
        let frozen_w = GUTTER_W + w[..frozen].iter().sum::<f32>();
        Cols { ids, w, x, frozen, frozen_w, total: at }
    }

    fn view_size(&self) -> Size<f32> {
        let b = self.body.get();
        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
        size(if w > 0. { w } else { 1200. }, if h > 0. { h } else { 800. })
    }

    /// Room left under the last row: none in a page, where the block is exactly as tall as its rows.
    fn slack(&self) -> f32 { if self.full { 8. } else { 0. } }

    /// Row `ri`'s own height (适应内容 rows differ; a group's line too).
    pub(crate) fn row_h_at(&self, ri: usize) -> f32 { match (self.tops.get(ri), self.tops.get(ri + 1)) { (Some(a), Some(b)) => b - a, _ => 0. } }

    fn content_h(&self, _row_h: f32) -> f32 { self.tops.last().copied().unwrap_or(0.) + add_row_h() }

    pub(crate) fn clamp_scroll(&mut self, cols: &Cols, row_h: f32) {
        let s = self.view_size();
        self.scroll.x = self.scroll.x.clamp(0., (cols.total + ADD_FIELD_W - s.width).max(0.));
        self.scroll.y = self.scroll.y.clamp(0., (self.content_h(row_h) + self.slack() - s.height).max(0.));
    }

    /// The selection as (first row, last row, first column, last column):
    /// rows are places in `result.records`, columns indexes into `cols`.
    pub(crate) fn sel_rect(&self, cols: &Cols) -> Option<(usize, usize, usize, usize)> {
        let s = self.sel?;
        let (r0, r1) = (*self.order.get(&s.anchor.0)?, *self.order.get(&s.head.0)?);
        let (c0, c1) = (cols.index(s.anchor.1)?, cols.index(s.head.1)?);
        Some((r0.min(r1), r0.max(r1), c0.min(c1), c0.max(c1)))
    }

    /// The cells of the selection, row by row.
    pub(crate) fn sel_cells(&self, t: &Table, cols: &Cols) -> Vec<Vec<CellId>> {
        let Some((r0, r1, c0, c1)) = self.sel_rect(cols) else { return vec![] };
        (r0..=r1).map(|r| { let rid = t.records[self.result.records[r]].id; (c0..=c1).map(|c| (rid, cols.ids[c])).collect() }).collect()
    }

    /// Scroll so that `cell` is in sight.
    pub(crate) fn reveal(&mut self, cell: CellId, cx: &mut Context<Self>) {
        let state = self.state.clone();
        let st = state.read(cx);
        let Some(t) = st.doc.base.table(self.table) else { return };
        let Some(v) = t.view(self.view) else { return };
        let cols = self.cols(t, v);
        let row_h = row_px(v.opts.row_height);
        let s = self.view_size();
        if let Some(row) = self.order.get(&cell.0).and_then(|p| self.row_of.get(*p)).filter(|r| **r != usize::MAX) {
            let (top, h) = (self.tops[*row], self.row_h_at(*row));
            if top < self.scroll.y { self.scroll.y = top; }
            if top + h > self.scroll.y + s.height { self.scroll.y = top + h - s.height; }
        }
        if let Some(c) = cols.index(cell.1).filter(|c| *c >= cols.frozen) {
            let (l, r) = (cols.x[c], cols.x[c] + cols.w[c]);
            if l - self.scroll.x < cols.frozen_w { self.scroll.x = l - cols.frozen_w; }
            if r - self.scroll.x > s.width { self.scroll.x = r - s.width; }
        }
        self.clamp_scroll(&cols, row_h);
        cx.notify();
    }

    /// Move the active cell by rows / columns (`extend`: keep the anchor, ⇧).
    pub(crate) fn step(&mut self, dr: i32, dc: i32, extend: bool, _window: &mut Window, cx: &mut Context<Self>) {
        let next = {
            let st = self.state.read(cx);
            let Some(t) = st.doc.base.table(self.table) else { return };
            let Some(v) = t.view(self.view) else { return };
            let cols = self.cols(t, v);
            let n = self.result.records.len();
            if n == 0 || cols.ids.is_empty() { return; }
            let rid_at = |p: usize| t.records[self.result.records[p]].id;
            let Some(s) = self.sel else {
                let first = (0..n).find(|p| self.row_of[*p] != usize::MAX).unwrap_or(0);
                self.sel = Some(Sel { anchor: (rid_at(first), cols.ids[0]), head: (rid_at(first), cols.ids[0]) });
                cx.notify();
                return;
            };
            let (Some(mut r), Some(c)) = (self.order.get(&s.head.0).copied(), cols.index(s.head.1)) else { return };
            let mut c = c as i32 + dc;
            let mut dr = dr;
            // Tab past the end of a row wraps to the next
            if !extend && c >= cols.ids.len() as i32 { c = 0; dr += 1; }
            if !extend && c < 0 { c = cols.ids.len() as i32 - 1; dr -= 1; }
            let c = c.clamp(0, cols.ids.len() as i32 - 1) as usize;
            // rows folded away are stepped over
            let mut left = dr.abs();
            while left > 0 {
                let mut p = r as i32 + dr.signum();
                while p >= 0 && (p as usize) < n && self.row_of[p as usize] == usize::MAX { p += dr.signum(); }
                if p < 0 || p as usize >= n { break; }
                r = p as usize;
                left -= 1;
            }
            (rid_at(r), cols.ids[c])
        };
        self.sel = Some(Sel { anchor: if extend { self.sel.map_or(next, |s| s.anchor) } else { next }, head: next });
        self.rows_sel.clear();
        self.col_sel.clear();
        self.reveal(next, cx);
    }

    /// Where a cell sits in the window (its top-left), for hanging an editor under it.
    pub(crate) fn cell_origin(&self, cell: CellId, cx: &App) -> Option<(Point<Pixels>, f32, f32)> {
        let st = self.state.read(cx);
        let t = st.doc.base.table(self.table)?;
        let v = t.view(self.view)?;
        let cols = self.cols(t, v);
        let c = cols.index(cell.1)?;
        let row = *self.row_of.get(*self.order.get(&cell.0)?)?;
        if row == usize::MAX { return None; }
        let b = self.body.get();
        let x = if c < cols.frozen { cols.x[c] } else { cols.x[c] - self.scroll.x };
        Some((point(b.origin.x + px(x), b.origin.y + px(self.tops[row] - self.scroll.y)), cols.w[c], self.row_h_at(row)))
    }

    /// The cell under a window position, clamped to the nearest record row and column (dragging past the edges).
    fn hit_cell(&self, pos: Point<Pixels>, t: &Table, cols: &Cols) -> Option<CellId> {
        let b = self.body.get();
        let (lx, ly) = (f32::from(pos.x - b.origin.x), f32::from(pos.y - b.origin.y));
        let y = (ly + self.scroll.y).max(0.);
        let mut row = self.tops.partition_point(|t| *t <= y).saturating_sub(1).min(self.result.rows.len().checked_sub(1)?);
        while !matches!(self.result.rows[row], query::Row::Record(_)) { if row + 1 < self.result.rows.len() { row += 1 } else { break } }
        let query::Row::Record(i) = self.result.rows[row] else { return None };
        let x = if lx < cols.frozen_w { lx.max(GUTTER_W) } else { lx + self.scroll.x };
        let col = (0..cols.ids.len()).find(|i| x < cols.x[*i] + cols.w[*i]).unwrap_or(cols.ids.len().checked_sub(1)?);
        Some((t.records[i].id, cols.ids[col]))
    }

    // ---- editing ----

    /// Start editing a cell: text-like fields in place (`typed` replaces what
    /// is there, as in Excel and 飞书; F2, Enter or a click keep it with the
    /// caret at the end), the others in their popup.
    pub(crate) fn begin_edit(&mut self, cell: CellId, typed: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        let (ty, text) = {
            let st = self.state.read(cx);
            let Some(t) = st.doc.base.table(self.table) else { return };
            let (Some(f), Some(r)) = (t.field(cell.1), t.record(cell.0)) else { return };
            (f.ty, editors::edit_text(f, r.get(f.id)))
        };
        self.sel = Some(Sel { anchor: cell, head: cell });
        if ty.computed() { return; }
        let under = self.cell_origin(cell, cx).map(|(p, _, h)| point(p.x, p.y + px(h + 2.)));
        match ty {
            FieldType::Checkbox => { self.toggle_check(cell, cx); }
            FieldType::SingleSelect | FieldType::MultiSelect | FieldType::Person => if let Some(p) = under { editors::open_select(self, cell, p, window, cx); },
            FieldType::Date => if let Some(p) = under { editors::open_date(self, cell, p, cx); },
            FieldType::Link => if let Some(p) = under { editors::open_links(self, cell, p, window, cx); },
            FieldType::Attachment => if let Some(p) = under { self.open_popup(p, Popup::Files { rec: cell.0, field: cell.1 }, cx); },
            // the note opens beside the table (made first for an empty cell); typing over the cell doesn't
            FieldType::Note => if typed.is_none() { notes::open_cell(self, cell, under.unwrap_or_default(), window, cx); },
            FieldType::Rating => if let Some(n) = typed.and_then(|s| s.parse::<f64>().ok()) { let tid = self.table; self.edit(cx, |d| d.set_cells(tid, vec![(cell.0, cell.1, Value::Number(n))])); },
            _ => {
                let text = typed.map_or(text, str::to_string);
                // text wraps in the cell, which grows down while it is edited (`edit_overlay`), ⇧⏎ breaking
                // the line; a number stays on its line, right-aligned
                self.input.update(cx, |t, cx| { t.align = if ty.numeric() { crate::model::Align::Right } else { crate::model::Align::Left }; t.wrap = ty.textual(); t.shift_newline = ty == FieldType::Text; t.font = Some(crate::theme::DOC_UI_FONT); t.set_text(&text, cx); });
                window.focus(&self.input.focus_handle(cx), cx);
                self.editing = Some(cell);
            }
        }
        cx.notify();
    }

    pub(crate) fn commit_edit(&mut self, save: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(cell) = self.editing.take() else { return };
        if save {
            let text = self.input.read(cx).text.clone();
            let tid = self.table;
            self.edit(cx, |d| {
                let Some(f) = d.base.table(tid).and_then(|t| t.field(cell.1)) else { return };
                let v = field::parse(f, &text);
                d.set_cells(tid, vec![(cell.0, cell.1, v)]);
            });
        }
        if self.input.read(cx).is_focused(window) { window.focus(&self.focus, cx); }
        cx.notify();
    }

    pub(crate) fn toggle_check(&mut self, cell: CellId, cx: &mut Context<Self>) {
        let tid = self.table;
        self.edit(cx, |d| {
            let on = d.base.table(tid).and_then(|t| t.record(cell.0)).is_some_and(|r| matches!(r.get(cell.1), Value::Bool(true)));
            d.set_cells(tid, vec![(cell.0, cell.1, Value::Bool(!on))]);
        });
    }

    /// Empty every selected cell (Delete).
    pub(crate) fn clear_cells(&mut self, cx: &mut Context<Self>) {
        let cells: Vec<(RecId, FieldId, Value)> = {
            let st = self.state.read(cx);
            let Some(t) = st.doc.base.table(self.table) else { return };
            let Some(v) = t.view(self.view) else { return };
            let cols = self.cols(t, v);
            if !self.col_sel.is_empty() { t.records.iter().flat_map(|r| self.col_sel.iter().map(move |f| (r.id, *f, Value::Empty))).collect() }
            else { self.sel_cells(t, &cols).into_iter().flatten().map(|(r, f)| (r, f, Value::Empty)).collect() }
        };
        let tid = self.table;
        self.edit(cx, |d| d.set_cells(tid, cells));
    }

    /// Add a record at the end (or under `group`), with what the view's filter and the group imply.
    pub(crate) fn add_record(&mut self, group: Option<&query::Group>, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(true, window, cx);
        let (with, first) = {
            let st = self.state.read(cx);
            let Some(t) = st.doc.base.table(self.table) else { return };
            let Some(v) = t.view(self.view) else { return };
            (query::defaults_for(t, v, group), t.visible_fields(v).first().map(|f| f.id))
        };
        let tid = self.table;
        let ids = self.edit(cx, |d| d.add_records(tid, None, 1, &with));
        if let (Some(id), Some(f)) = (ids.first(), first) {
            self.sel = Some(Sel { anchor: (*id, f), head: (*id, f) });
            self.rows_sel.clear();
            self.col_sel.clear();
            self.sync_now(cx);
            self.reveal((*id, f), cx);
        }
    }

    pub(crate) fn toggle_fold(&mut self, key: &str, cx: &mut Context<Self>) {
        let key = key.to_string();
        self.edit_view(cx, |v| if !v.opts.folded.remove(&key) { v.opts.folded.insert(key); });
    }
}

/// Pointer moved anywhere over the view: carry on the gesture in progress.
pub(crate) fn mouse_move(v: &mut TableView, ev: &MouseMoveEvent, window: &mut Window, cx: &mut Context<TableView>) {
    if v.drag.is_none() { return; }
    if ev.pressed_button != Some(MouseButton::Left) { mouse_up_inner(v, window, cx); return; }
    let state = v.state.clone();
    let st = state.read(cx);
    let Some(t) = st.doc.base.table(v.table) else { return };
    let Some(view) = t.view(v.view) else { return };
    let cols = v.cols(t, view);
    let (px_, py_) = (f32::from(ev.position.x), f32::from(ev.position.y));
    let body = v.body.get();
    let result = v.result.clone();
    let mut drag = v.drag.take().unwrap();
    match &mut drag {
        Drag::ColWidth { x0, w0, w, .. } => *w = (*w0 + px_ - *x0).max(MIN_WIDTH),
        Drag::Select => {
            if let Some(cell) = v.hit_cell(ev.position, t, &cols) && let Some(s) = v.sel.as_mut() { s.head = cell; }
            // near the edges the grid scrolls along
            let (top, bottom) = (f32::from(body.origin.y), f32::from(body.origin.y + body.size.height));
            if py_ < top + 12. { v.scroll.y -= 16.; } else if py_ > bottom - 12. { v.scroll.y += 16.; }
            let (left, right) = (f32::from(body.origin.x) + cols.frozen_w, f32::from(body.origin.x + body.size.width));
            if px_ > right - 12. { v.scroll.x += 16.; } else if px_ < left + 12. && px_ > left - 40. { v.scroll.x -= 16.; }
            v.clamp_scroll(&cols, row_px(view.opts.row_height));
        }
        Drag::ColMove { x0, x, to, .. } => {
            *x = px_;
            if (px_ - *x0).abs() > 4. || to.is_some() {
                let lx = px_ - f32::from(body.origin.x);
                let cx_ = if lx < cols.frozen_w { lx } else { lx + v.scroll.x };
                // the gap nearest the pointer, never before the primary column
                let gap = (1..=cols.ids.len()).min_by(|a, b| {
                    let edge = |i: usize| if i == cols.ids.len() { cols.total } else { cols.x[i] };
                    (edge(*a) - cx_).abs().total_cmp(&(edge(*b) - cx_).abs())
                }).unwrap_or(1);
                *to = Some(gap);
            }
        }
        Drag::Thumb { across, p0, s0, per } => {
            if *across { v.scroll.x = *s0 + (px_ - *p0) * *per; } else { v.scroll.y = *s0 + (py_ - *p0) * *per; }
            v.clamp_scroll(&cols, row_px(view.opts.row_height));
        }
        Drag::ColSelect { from } => {
            let lx = px_ - f32::from(body.origin.x);
            let cx_ = if lx < cols.frozen_w { lx } else { lx + v.scroll.x };
            let under = (0..cols.ids.len()).find(|i| cx_ < cols.x[*i] + cols.w[*i]).unwrap_or(cols.ids.len().saturating_sub(1));
            if let Some(a) = cols.index(*from) { v.col_sel = cols.ids[a.min(under)..=a.max(under)].to_vec(); }
        }
        Drag::RowMove { ids, y, to, moved, y0, off } => {
            *y = py_;
            if (py_ - *y0).abs() > 4. { *moved = true; }
            // the dragged rows' first top, in rows of the list without them: the nearest place is the gap's
            let first = py_ - f32::from(body.origin.y) + v.scroll.y - *off;
            // the rows left before the gap: those whose middle is above the dragged rows' first top
            let (mut y, mut n) = (0f32, 0usize);
            for (p, i) in result.records.iter().enumerate() {
                if ids.contains(&t.records[*i].id) { continue }
                let h = v.row_of.get(p).copied().filter(|r| *r != usize::MAX).map_or(0., |r| v.row_h_at(r));
                if y + h / 2. > first { break }
                y += h;
                n += 1;
            }
            *to = n;
        }
    }
    v.drag = Some(drag);
    cx.notify();
}

pub(crate) fn mouse_up(v: &mut TableView, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<TableView>) { mouse_up_inner(v, window, cx); }

fn mouse_up_inner(v: &mut TableView, window: &mut Window, cx: &mut Context<TableView>) {
    let Some(drag) = v.drag.take() else { return };
    let tid = v.table;
    match drag {
        Drag::ColWidth { field, w, w0, .. } => if (w - w0).abs() >= 1. { v.edit_view(cx, |view| { view.opts.widths.insert(field, w.round()); }); },
        Drag::Select | Drag::Thumb { .. } => {}
        Drag::ColMove { field, to: Some(to), x, off, .. } => {
            let order = {
                let st = v.state.read(cx);
                let (Some(t), Some(view)) = (st.doc.base.table(tid), st.doc.base.table(tid).and_then(|t| t.view(v.view))) else { return };
                let shown: Vec<FieldId> = t.visible_fields(view).iter().map(|f| f.id).collect();
                // from where it was let go to its place (a left in the list's own coordinates)
                let at = shown.iter().position(|f| *f == field).map(|d| if to > d { to - 1 } else { to }).unwrap_or(0);
                let ghost = x - f32::from(v.body.get().origin.x) - off + if at >= view.opts.frozen { v.scroll.x } else { 0. };
                v.slide.borrow_mut().cols.insert(field, crate::sidebar_drag::Spring::new([ghost]));
                let all: Vec<FieldId> = t.ordered_fields(view).iter().map(|f| f.id).collect();
                // before which field (of all, hidden ones too) the dragged one lands
                let before = shown.get(to).copied();
                let mut order: Vec<FieldId> = all.into_iter().filter(|f| *f != field).collect();
                let at = before.and_then(|b| order.iter().position(|f| *f == b)).unwrap_or(order.len()).max(1);
                order.insert(at.min(order.len()), field);
                order
            };
            v.edit_view(cx, |view| view.opts.order = order);
        }
        Drag::ColMove { field, rename, .. } => {
            // a click, not a drag: the column alone picked; a second click on it edits its name
            if rename { v.begin_rename(Rename::Field(field), window, cx); return; }
            v.col_sel = vec![field];
            v.sel = None;
            v.rows_sel.clear();
        }
        Drag::ColSelect { .. } => {}
        Drag::RowMove { ids, to, moved, y, off, .. } => if moved {
            // the dragged rows glide in from where they were let go
            {
                let first = y - f32::from(v.body.get().origin.y) + v.scroll.y - off;
                let mut fx = v.slide.borrow_mut();
                let mut placed: Vec<(usize, RecId)> = ids.iter().filter_map(|r| v.order.get(r).map(|p| (*p, *r))).collect();
                placed.sort();
                let mut y = first;
                for (p, r) in placed { fx.rows.insert(r, crate::sidebar_drag::Spring::new([y])); y += v.row_of.get(p).copied().filter(|r| *r != usize::MAX).map_or(0., |ri| v.row_h_at(ri)); }
            }
            let target = {
                let st = v.state.read(cx);
                let Some(t) = st.doc.base.table(tid) else { return };
                // the record at the gap's place among the rows left → its row in the table's own order (past
                // the last of them, just after it)
                let gone: HashSet<RecId> = ids.iter().copied().collect();
                let rest: Vec<usize> = v.result.records.iter().copied().filter(|i| !gone.contains(&t.records[*i].id)).collect();
                rest.get(to).copied().unwrap_or_else(|| rest.last().map_or(t.records.len(), |i| i + 1))
            };
            v.edit(cx, |d| d.move_records(tid, &ids, target));
        },
    }
    cx.notify();
}

pub(crate) fn render(v: &TableView, base: &Base, t: &Table, view: &View, window: &mut Window, cx: &Context<TableView>) -> AnyElement {
    let cols = v.cols(t, view);
    let mut fields = t.visible_fields(view);
    let row_h = row_px(view.opts.row_height);
    let (sx, sy) = (v.scroll.x, v.scroll.y);
    // a column being dragged (as rows are): laid out at the gap it would land in, its place there
    // left empty, itself riding the pointer over the rest (`ghost`, from the list's left)
    let col_lift = match &v.drag { Some(Drag::ColMove { field, to: Some(to), x, off, .. }) => cols.index(*field).map(|d| (d, *to, x - f32::from(v.body.get().origin.x) - off)), _ => None };
    let was_x: HashMap<FieldId, f32> = cols.ids.iter().copied().zip(cols.x.iter().copied()).collect();
    let (cols, dragged_col, ghost) = match col_lift {
        Some((d, to, ghost)) if cols.ids.len() > 1 => {
            let n = (if to > d { to - 1 } else { to }).clamp(1, cols.ids.len() - 1);
            let f = fields.remove(d);
            fields.insert(n, f);
            (relaid(&cols, d, n), Some(n), ghost)
        }
        _ => (cols, None, 0.),
    };
    // columns making room slide on springs (the rows'), each cell of theirs `col_dx` off its place
    let mut fx = v.slide.borrow_mut();
    let now = web_time::Instant::now();
    let dt = fx.last.replace(now).map_or(1. / 120., |l| now.duration_since(l).as_secs_f32().min(0.032));
    let mut sliding = false;
    let mut col_dx = vec![0f32; cols.ids.len()];
    for (c, fid) in cols.ids.iter().enumerate() {
        if Some(c) == dragged_col { continue }
        let target = cols.x[c];
        let s = match fx.cols.get_mut(fid) {
            Some(s) => s,
            None if dragged_col.is_some() => fx.cols.entry(*fid).or_insert(crate::sidebar_drag::Spring::new([was_x.get(fid).copied().unwrap_or(target)])),
            None => continue,
        };
        s.target = [target];
        sliding |= s.step(dt);
        col_dx[c] = s.p[0] - target;
    }
    if dragged_col.is_none() { fx.cols.retain(|_, s| s.p != s.target || s.v[0] != 0.); }
    let vs = v.view_size();
    let sel = v.sel_rect(&cols);
    let manual = view.opts.sorts.is_empty() && view.opts.groups.is_empty();
    let all_sel = !v.result.records.is_empty() && v.rows_sel.len() == v.result.records.len();
    let today = field::today();
    let hits: HashSet<CellId> = v.find.as_ref().map(|f| f.hits.iter().copied().collect()).unwrap_or_default();
    let hit_now = v.find.as_ref().and_then(|f| f.hits.get(f.at).copied());
    let _ = window;

    let inline = !v.full;
    // ---- header ----
    // the header's fill stops with its last cell (the + column), never runs to the edge
    let hdr_bg = header_bg();
    let hdr_cell = |ci: usize| {
        let f = fields[ci];
        let fid = f.id;
        let (w, on) = (cols.w[ci], v.col_sel.contains(&fid));
        let sorted = view.opts.sorts.iter().find(|s| s.field == fid).map(|s| s.desc);
        // its left on screen, from the list's left: where a drag of it takes hold
        let col_left = cols.x[ci] - if ci >= cols.frozen { sx } else { 0. };
        let renaming = matches!(v.rename, Some(Rename::Field(r)) if r == fid);
        div().id(("tb-hdr", fid)).group("tb-hdr").relative().flex_none().w(px(w)).h_full().flex().items_center().gap(px(4.)).px(px(cell_pad_x()))
            .border_r_1().border_color(line())
            .when(on, |d| d.bg(sel_header()))
            // as Supabase's head: the name, then its type in muted words (a button: the field's settings,
            // its type, options, format); renaming, the name edited in place as a cell is, the head framed
            .child(if renaming {
                div().flex_1().min_w_0().child(v.aux.clone()).on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).into_any_element()
            } else {
                div().flex_1().min_w_0().flex().items_center().gap(px(6.))
                    .child(div().flex_shrink_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().font_weight(FontWeight::MEDIUM).text_color(header_text()).child(ui_text(f.name.clone())))
                    .child(div().id(("tb-hdr-type", fid)).flex_none().px(px(4.)).mx(px(-4.)).rounded(px(crate::design::geo().tool_r)).cursor_pointer()
                        .text_size(px(text_xs())).text_color(Theme::text_placeholder()).hover(|s| s.bg(Theme::fill_hover_strong()).text_color(Theme::text_caption()))
                        .whitespace_nowrap().child(crate::settings::label(f.ty.label()))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |v, ev: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            window.focus(&v.focus, cx);
                            v.col_sel = vec![fid];
                            v.sel = None;
                            menus::edit_field(v, fid, point(ev.position.x - px(12.), ev.position.y + px(16.)), window, cx);
                        })))
                    .into_any_element()
            })
            .when_some(sorted, |d, desc| d.child(icon(if desc { "caret-down" } else { "caret-up" }, 12., Theme::primary())))
            .child(div().id(("tb-hdr-menu", fid)).relative().flex_none().flex().items_center().justify_center().size(px(18.)).rounded(px(4.)).cursor_pointer()
                .child(v.anchor(format!("tb-hdr-menu-{fid}")))
                .hover(|s| s.bg(Theme::fill_hover_strong())).when(!on, |d| d.invisible().group_hover("tb-hdr", |s| s.visible()))
                .child(icon("chevron-down", 13., Theme::text_caption()))
                .on_mouse_down(MouseButton::Left, cx.listener(move |v, _: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    v.commit_edit(true, window, cx);
                    if !v.col_sel.contains(&fid) { v.col_sel = vec![fid]; }
                    v.sel = None;
                    // hung from the ▾ (its right edge), not the pointer
                    v.hang_popup(&format!("tb-hdr-menu-{fid}"), crate::design::Align::End, |v, pos| v.open_popup(pos, Popup::FieldMenu(fid), cx));
                })))
            .on_mouse_down(MouseButton::Left, cx.listener(move |v, ev: &MouseDownEvent, window, cx| {
                v.commit_edit(true, window, cx);
                window.focus(&v.focus, cx);
                // a double click edits the name in place (the type's icon opens the field's settings)
                // (the name's box keeps the focus: the press must not reach the view's own track_focus)
                if ev.click_count >= 2 { v.drag = None; v.col_sel = vec![fid]; v.begin_rename(Rename::Field(fid), window, cx); window.prevent_default(); return; }
                // ⇧: every column from the last picked one to this; ⌘: this one in or out
                if ev.modifiers.shift || ev.modifiers.platform {
                    v.sel = None;
                    v.rows_sel.clear();
                    if ev.modifiers.platform { if let Some(i) = v.col_sel.iter().position(|f| *f == fid) { v.col_sel.remove(i); } else { v.col_sel.push(fid); } }
                    else if let Some(from) = v.col_sel.last().copied() {
                        let order = { let st = v.state.read(cx); st.doc.base.table(v.table).and_then(|t| t.view(v.view).map(|view| t.visible_fields(view).iter().map(|f| f.id).collect::<Vec<_>>())).unwrap_or_default() };
                        if let (Some(a), Some(b)) = (order.iter().position(|f| *f == from), order.iter().position(|f| *f == fid)) {
                            for f in &order[a.min(b)..=a.max(b)] { if !v.col_sel.contains(f) { v.col_sel.push(*f); } }
                        }
                    } else { v.col_sel = vec![fid]; }
                    cx.notify();
                    return;
                }
                // a picked header drags its column along; another starts picking columns, as cells are picked
                let x = f32::from(ev.position.x);
                if v.col_sel.contains(&fid) {
                    v.drag = Some(Drag::ColMove { field: fid, x0: x, x, off: x - f32::from(v.body.get().origin.x) - col_left, to: None, rename: v.col_sel == [fid] });
                } else {
                    v.col_sel = vec![fid];
                    v.sel = None;
                    v.rows_sel.clear();
                    v.drag = Some(Drag::ColSelect { from: fid });
                }
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Right, cx.listener(move |v, ev: &MouseDownEvent, window, cx| {
                v.commit_edit(true, window, cx);
                if !v.col_sel.contains(&fid) { v.col_sel = vec![fid]; }
                v.sel = None;
                v.open_popup(ev.position, Popup::FieldMenu(fid), cx);
            }))
            .when(renaming, |d| d.child(div().absolute().inset_0().border(px(RING_W)).border_color(ring())
                .on_mouse_down_out(cx.listener(|v, _, _, cx| v.finish_rename(true, cx)))))
            // the right edge drags the width
            .child(div().id(("tb-grip", fid)).absolute().top_0().bottom_0().right(px(-4.)).w(px(8.)).flex().justify_center().cursor(CursorStyle::ResizeLeftRight)
                // the line it lights is the whole column's (`col_edges`), not a stub in the header
                .on_hover(cx.listener(move |v, on: &bool, _, cx| { let g = on.then_some(fid); if v.grip_hover != g && (*on || v.grip_hover == Some(fid)) { v.grip_hover = g; cx.notify(); } }))
                .on_mouse_down(MouseButton::Left, cx.listener(move |v, ev: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    v.drag = Some(Drag::ColWidth { field: fid, x0: f32::from(ev.position.x), w0: w, w });
                    cx.notify();
                })))
    };
    let mut hdr_frozen = div().flex_none().flex().w(px(cols.frozen_w)).h_full().border_r_1().border_color(frozen_line())
        // the header's checkbox stays (all the rows, or none), over the rows' own
        .child(div().id("tb-hdr-check").relative().flex_none().w(px(GUTTER_W)).h_full().border_r_1().border_color(line()).cursor_pointer()
            .child(div().absolute().left(px(CHECK_X)).top_0().bottom_0().w(px(NUM_W)).flex().items_center().justify_center().child(checkbox(all_sel)))
            .on_click(cx.listener(|v, _, _, cx| {
                let ids: Vec<RecId> = { let st = v.state.read(cx); st.doc.base.table(v.table).map(|t| v.result.records.iter().map(|i| t.records[*i].id).collect()).unwrap_or_default() };
                if v.rows_sel.len() == ids.len() { v.rows_sel.clear(); } else { v.rows_sel = ids.into_iter().collect(); }
                v.sel = None;
                cx.notify();
            })));
    // a column's place while it is dragged: empty, the rest sliding round it
    let hole = |ci: usize| div().flex_none().w(px(cols.w[ci])).h_full().bg(Theme::fill_hover()).border_r_1().border_b_1().border_color(line()).into_any_element();
    let placed_hdr = |ci: usize| if Some(ci) == dragged_col { hole(ci) } else { hdr_cell(ci).left(px(col_dx[ci])).into_any_element() };
    for ci in 0..cols.frozen { hdr_frozen = hdr_frozen.child(placed_hdr(ci)); }
    let mut hdr_scroll = div().flex().h_full().ml(px(-sx));
    for ci in cols.frozen..fields.len() { hdr_scroll = hdr_scroll.child(placed_hdr(ci)); }
    hdr_scroll = hdr_scroll.child(div().id("tb-add-field").flex_none().w(px(ADD_FIELD_W)).h_full().flex().items_center().justify_center()
        .cursor_pointer().hover(|s| s.bg(Theme::fill_hover())).child(icon("plus", 15., Theme::icon()))
        // a 文本 column at once, its name ready to type; its type's icon changes the type
        .on_click(cx.listener(|v, _, window, cx| v.add_text_field(window, cx))));
    // in a page the fill follows the frame's top corners
    let header = div().relative().flex_none().flex().h(px(header_h())).bg(hdr_bg).border_b_1().border_color(header_line()).when(inline, |d| d.rounded_t(px(frame_r())))
        .child(hdr_frozen).child(div().flex_1().min_w_0().h_full().overflow_hidden().child(hdr_scroll))
        .children(dragged_col.map(|c| ghost_col(hdr_cell(c).into_any_element(), ghost, hdr_bg)));

    // ---- rows in sight ----
    let first = v.tops.partition_point(|y| *y + group_h().max(row_h) <= sy).min(v.result.rows.len());
    let last = v.tops.partition_point(|y| *y < sy + vs.height).min(v.result.rows.len());
    let mut body = div().id("tb-body").relative().flex_1().min_h_0().overflow_hidden()
        .child({ let b = v.body.clone(); canvas(move |bounds, _, _| b.set(bounds), |_, _, _, _| {}).absolute().size_full() });
    let rec_pos = |i: usize| v.order.get(&t.records[i].id).copied();
    // rows being dragged: each left row's place counting the gap, the dragged ones over the rest at the pointer
    // rows being dragged (only in the table's own order: no groups, a row is its record's place):
    // the dragged ones, their places in order, the gap's place among the rest, the first one's top on screen
    let lift = match &v.drag {
        Some(Drag::RowMove { ids, to, y, off, moved: true, .. }) => {
            let set: HashSet<RecId> = ids.iter().copied().collect();
            let mut at: Vec<usize> = ids.iter().filter_map(|r| v.order.get(r).copied()).collect();
            at.sort_unstable();
            // the rows' own heights (适应内容 rows differ): the dragged ones stacked, and the rest from the top
            let h_at = |p: usize| v.row_of.get(p).copied().filter(|r| *r != usize::MAX).map_or(row_h, |r| v.row_h_at(r));
            let mut lifted_tops = vec![0f32];
            for p in &at { lifted_tops.push(lifted_tops.last().unwrap() + h_at(*p)); }
            let mut rest_tops = vec![0f32];
            for p in 0..v.result.records.len() { if at.binary_search(&p).is_err() { rest_tops.push(rest_tops.last().unwrap() + h_at(p)); } }
            Some((set, at, *to, y - f32::from(v.body.get().origin.y) - off, lifted_tops, rest_tops))
        }
        _ => None,
    };
    let mut lifted: Vec<AnyElement> = vec![];
    // rows slide to their places on springs too (`fx`, above)
    let n_rows = v.result.rows.len();
    let near = |y: f32| y - sy > -2. * row_h && y - sy < vs.height + row_h;
    // the rows to lay out: those in sight, wherever they are going or coming from. Only these get
    // springs, so a long table costs no more than a short one.
    let visit: Vec<usize> = match &lift {
        Some((set, at, _, ghost, lifted_tops, _)) => {
            // a row moves by at most the dragged ones' count: the window in sight, widened by that
            let k = at.len();
            let (lo, hi) = (v.tops.partition_point(|y| *y < sy).saturating_sub(k + 1), (v.tops.partition_point(|y| *y < sy + vs.height) + k + 1).min(n_rows));
            let mut r: Vec<usize> = (lo..hi).filter(|ri| !matches!(&v.result.rows[*ri], query::Row::Record(i) if set.contains(&t.records[*i].id))).collect();
            // the dragged ones in sight, riding the pointer
            r.extend(at.iter().enumerate().filter(|(j, _)| { let y = ghost + lifted_tops[*j]; y + (lifted_tops[*j + 1] - lifted_tops[*j]) > 0. && y < vs.height }).filter_map(|(_, p)| v.row_of.get(*p).copied().filter(|r| *r != usize::MAX)));
            r
        }
        None => {
            let mut r: Vec<usize> = (first..last).collect();
            // still settling: those a spring still carries, from wherever they are
            r.extend(fx.rows.keys().filter_map(|id| v.order.get(id).and_then(|p| v.row_of.get(*p)).copied()).filter(|ri| *ri != usize::MAX && !(first..last).contains(ri)));
            r
        }
    };
    match &lift {
        Some((_, _, to, _, lifted_tops, rest_tops)) => {
            let y = rest_tops[(*to).min(rest_tops.len() - 1)];
            let gap = fx.gap.get_or_insert_with(|| crate::sidebar_drag::Spring::new([y]));
            gap.target = [y];
            sliding |= gap.step(dt);
            let gap_top = gap.p[0] - sy;
            body = body.child(div().absolute().left_0().w(px((cols.total - sx).max(cols.frozen_w))).top(px(gap_top)).h(px(*lifted_tops.last().unwrap())).bg(Theme::fill_hover()).border_b_1().border_color(line()));
        }
        None => fx.gap = None,
    }
    for ri in visit {
        let mut top = v.tops[ri] - sy;
        // the place it shows at, numbered so while rows are dragged
        let mut place = None;
        if let (Some((set, at, to, ghost, lifted_tops, rest_tops)), query::Row::Record(i)) = (&lift, &v.result.rows[ri]) {
            let rid = t.records[*i].id;
            let pos = v.order.get(&rid).copied().unwrap_or(ri);
            if set.contains(&rid) {
                let j = at.partition_point(|p| *p < pos);
                top = ghost + lifted_tops[j];
                place = Some(to + j);
            } else {
                // its place among the rows left (those dragged from above it are gone), the gap's rows after it
                let left = pos - at.partition_point(|p| *p < pos);
                let shown = left + if left >= *to { at.len() } else { 0 };
                let target = rest_tops[left.min(rest_tops.len() - 1)] + if left >= *to { *lifted_tops.last().unwrap() } else { 0. };
                let s = match fx.rows.get_mut(&rid) {
                    Some(s) => s,
                    // in sight before or after: on a spring from where it was; else straight to its place
                    None if near(target) || near(v.tops[ri]) => fx.rows.entry(rid).or_insert(crate::sidebar_drag::Spring::new([if near(v.tops[ri]) { v.tops[ri] } else { target }])),
                    None => continue,
                };
                s.target = [target];
                sliding |= s.step(dt);
                top = s.p[0] - sy;
                place = Some(shown);
            }
        } else if let (None, query::Row::Record(i)) = (&lift, &v.result.rows[ri]) && let Some(s) = fx.rows.get_mut(&t.records[*i].id) {
            // dropped, or making room a moment ago: on to its place in the list
            s.target = [v.tops[ri]];
            sliding |= s.step(dt);
            top = s.p[0] - sy;
        }
        let row_h = v.row_h_at(ri);
        if top + row_h < 0. || top > vs.height { continue }
        match &v.result.rows[ri] {
            query::Row::Group(g) => body = body.child(group_row(v, t, g, top, cols.frozen_w, cx)),
            query::Row::Record(i) => {
                let r = &t.records[*i];
                let rid = r.id;
                let pos = rec_pos(*i).unwrap_or(0);
                let number = place.unwrap_or(pos) + 1;
                let checked = v.rows_sel.contains(&rid);
                let tint = query::row_color(base, t, r, view, today);
                let dragging = matches!(&v.drag, Some(Drag::RowMove { ids, moved: true, .. }) if ids.contains(&rid));
                let gutter = div().id(("tb-gutter", rid as usize)).relative().flex_none().w(px(GUTTER_W)).h_full().flex().items_center().justify_center()
                    .border_r_1().border_b_1().border_color(line())
                    .when(checked, |d| d.bg(sel_bg()))
                    .when(manual, |d| d.child(div().id(("tb-grab", rid as usize)).absolute().left(px(GRAB_X)).top_0().bottom_0().w(px(CHECK_X - GRAB_X)).flex().items_center().justify_center()
                        .invisible().group_hover("tb-row", |s| s.visible()).cursor(CursorStyle::OpenHand)
                        .child(icon("grip-vertical", 13., Theme::text_placeholder()))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |v, ev: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            v.commit_edit(true, window, cx);
                            let ids: Vec<RecId> = if v.rows_sel.contains(&rid) { v.rows_sel.iter().copied().collect() } else { vec![rid] };
                            let y = f32::from(ev.position.y);
                            // the rows ride the pointer stacked in their order, this one where it was pressed
                            let above: f32 = ids.iter().filter_map(|r| v.order.get(r).copied()).filter(|p| *p < pos).map(|p| v.row_of.get(p).copied().filter(|r| *r != usize::MAX).map_or(0., |r| v.row_h_at(r))).sum();
                            let off = y - f32::from(v.body.get().origin.y) + v.scroll.y - top - v.scroll.y + above;
                            v.drag = Some(Drag::RowMove { ids, to: pos, y0: y, y, off, moved: false });
                        }))))
                    // its number at rest, across the whole start; hovered, the handle, the checkbox and 展开 (a ticked one keeps its box)
                    .when(!checked, |d| d.child(div().absolute().left_0().right_0().top_0().bottom_0().flex().items_center().justify_center().group_hover("tb-row", |s| s.invisible())
                        .text_size(px(12.)).text_color(Theme::text_caption()).child(format!("{number}"))))
                    .child(div().id(("tb-check", rid as usize)).absolute().left(px(CHECK_X)).top_0().bottom_0().w(px(NUM_W)).flex().items_center().justify_center().cursor_pointer()
                        .when(!checked, |d| d.invisible().group_hover("tb-row", |s| s.visible())).child(checkbox(checked))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |v, ev: &MouseDownEvent, window, cx| { cx.stop_propagation(); v.tick_row(rid, ev.modifiers.shift, window, cx); })))

                    // 展开: the record, in 记录详情 (as Supabase's row ⤢; 飞书 hangs a 查看 on the title cell, over its text)
                    .child(div().absolute().left(px(EXPAND_X)).top_0().bottom_0().flex().items_center().invisible().group_hover("tb-row", |s| s.visible())
                        .child({ use crate::ui::TipExt; div().id(("tb-open", rid as usize)).size(px(NUM_W)).rounded(px(crate::design::geo().tool_r)).flex().items_center().justify_center().cursor_pointer()
                            .hover(|s| s.bg(Theme::fill_hover_strong())).child(icon("expand", 13., Theme::icon())).tip(&crate::settings::label("查看详情"), None) }
                            // a toggle: this record's panel open, the same press shuts it
                            .on_mouse_down(MouseButton::Left, cx.listener(move |v, _, window, cx| { cx.stop_propagation(); v.commit_edit(true, window, cx); if v.expanded == Some(rid) { record::close(v, window, cx) } else { record::open(v, rid, window, cx) } }))));
                let cell = |ci: usize| {
                    let f = fields[ci];
                    let id: CellId = (rid, f.id);
                    let in_sel = sel.is_some_and(|(r0, r1, c0, c1)| pos >= r0 && pos <= r1 && ci >= c0 && ci <= c1);
                    let multi = sel.is_some_and(|(r0, r1, c0, c1)| r0 != r1 || c0 != c1);
                    let editing = v.editing == Some(id);
                    let found = hits.contains(&id);
                    // a link's cell: ⌘-click opens it (in the browser 设置 picks), as a link in a page does
                    let link = match r.get(f.id) { Value::Text(s) if matches!(f.ty, FieldType::Url | FieldType::Email) && !s.trim().is_empty() => Some(href(f.ty, s.trim())), _ => None };
                    // text is edited in the cell grown to fit it (`edit_overlay`), the cell left blank under it
                    let content: AnyElement = if editing && f.ty.textual() { div().into_any_element() } else if editing { div().w_full().child(v.input.clone()).into_any_element() } else { cell::content(v, base, t, f, r, row_h, cx) };
                    div().id(("tb-cell", (rid as usize) << 12 | (f.id as usize & 0xfff))).relative().flex_none().w(px(cols.w[ci])).h_full().flex().items_start().px(px(cell_pad_x())).overflow_hidden()
                        .border_b_1().border_r_1().border_color(line())
                        .when(v.col_sel.contains(&f.id), |d| d.bg(sel_bg()))
                        .when(found, |d| d.bg(if hit_now == Some(id) { found_current() } else { found_bg() }))
                        // a range is filled, all of it, and not framed (`cursor_ring`)
                        .when(in_sel && multi, |d| d.bg(range_bg()))
                        .when(editing, |d| d.bg(Theme::bg_body()))
                        .child(div().w_full().h(px(row_h.min(one_line_h()) - 1.)).flex().items_center().when(row_h > one_line_h(), |d| d.h_full().items_start().py(px(10.)))
                            // a taller row shows whole lines only, the rest cut at a line's end (not halfway through the next)
                            .child(div().w_full().when(row_h > one_line_h(), |d| d.max_h(px(((row_h - 20.) / 20.).floor() * 20.)).overflow_hidden()).child(content)))
                        .when(link.is_some(), |d| { use crate::ui::TipExt; d.tip("打开链接", Some(&crate::settings::label("⌘ + 点击"))) })
                        .on_mouse_down(MouseButton::Left, cx.listener(move |v, ev: &MouseDownEvent, window, cx| {
                            if ev.modifiers.platform && let Some(url) = &link { crate::editor::open_link(cx, url); return; }
                            if v.editing == Some(id) { return; }
                            v.commit_edit(true, window, cx);
                            window.focus(&v.focus, cx);
                            let was = v.sel.is_some_and(|s| s.anchor == id && s.head == id);
                            v.rows_sel.clear();
                            v.col_sel.clear();
                            if ev.modifiers.shift && let Some(s) = v.sel.as_mut() { s.head = id; }
                            else { v.sel = Some(Sel { anchor: id, head: id }); }
                            // a second click on the active cell opens its editor, as a double click does
                            if ev.click_count >= 2 || (was && !ev.modifiers.shift) { v.begin_edit(id, None, window, cx); }
                            else { v.drag = Some(Drag::Select); }
                            // focus is set here (the grid, or the cell editor): the view's own track_focus must not take
                            // it back as the press bubbles up (on Windows the editor lost its keys that way)
                            window.prevent_default();
                            cx.notify();
                        }))
                        .on_mouse_down(MouseButton::Right, cx.listener(move |v, ev: &MouseDownEvent, window, cx| {
                            v.commit_edit(true, window, cx);
                            if !v.rows_sel.contains(&rid) {
                                v.rows_sel.clear();
                                let inside = { let st = v.state.read(cx); st.doc.base.table(v.table).and_then(|t| t.view(v.view).map(|view| v.cols(t, view))).and_then(|cols| v.sel_rect(&cols).zip(v.order.get(&rid).copied()).zip(cols.index(id.1)))
                                    .is_some_and(|(((r0, r1, c0, c1), p), c)| p >= r0 && p <= r1 && c >= c0 && c <= c1) };
                                if !inside { v.sel = Some(Sel { anchor: id, head: id }); }
                            }
                            v.col_sel.clear();
                            v.open_popup(ev.position, Popup::CellMenu(rid), cx);
                        }))
                };
                let mut fz = div().flex_none().flex().w(px(cols.frozen_w)).h_full().border_r_1().border_color(frozen_line()).child(gutter);
                let placed = |ci: usize| if Some(ci) == dragged_col { hole(ci) } else { cell(ci).left(px(col_dx[ci])).into_any_element() };
                for ci in 0..cols.frozen { fz = fz.child(placed(ci)); }
                let mut sc = div().flex().h_full().ml(px(-sx));
                for ci in cols.frozen..fields.len() { sc = sc.child(placed(ci)); }
                let row = div().group("tb-row").absolute().left_0().w(px((cols.total - sx).max(cols.frozen_w))).top(px(top)).h(px(row_h)).flex()
                    .when_some(tint, |d, slot| d.bg(row_tint(slot)))
                    .when(checked, |d| d.bg(sel_bg()))
                    // lifted: over the rest, a card riding the pointer
                    .when(dragging, |d| d.bg(Theme::bg_body()).opacity(0.92).shadow(crate::design::shadow::popover()))
                    .child(fz).child(div().flex_1().min_w_0().h_full().overflow_hidden().child(sc))
                    .children(dragged_col.map(|c| ghost_col(cell(c).into_any_element(), ghost, Theme::bg_body())));
                if dragging { lifted.push(row.into_any_element()); } else { body = body.child(row); }
            }
        }
    }
    // the add-record line, after the last row
    let end = v.tops.last().copied().unwrap_or(0.) - sy;
    if end < vs.height {
        // 飞书: the last field's right edge runs on down through the add-record line, and that line spans the whole width
        let last = cols.total - sx;
        if last > cols.frozen_w { body = body.child(div().absolute().top(px(end)).left(px(last - 1.)).w(px(1.)).h(px(add_row_h())).bg(line())); }
        body = body.child(div().id("tb-add-row").absolute().left_0().right_0().top(px(end)).h(px(add_row_h())).flex().items_center().cursor_pointer()
            .when(!inline, |d| d.border_b_1().border_color(line())).when(inline, |d| d.rounded_b(px(frame_r()))).hover(|s| s.bg(Theme::fill_hover()))
            .child(div().flex_none().w(px(GUTTER_W)).flex().items_center().justify_center().child(icon("plus", 15., Theme::text_placeholder())))
            .on_click(cx.listener(|v, _, window, cx| v.add_record(None, window, cx))));
    }
    body = body.children(lifted);
    // settled rows let go of their springs; while any moves, another frame
    if lift.is_none() { fx.rows.retain(|_, s| s.p != s.target || s.v[0] != 0.); }
    if !sliding && lift.is_none() && dragged_col.is_none() { fx.last = None; }
    drop(fx);
    if sliding { window.request_animation_frame(); }
    let empty = v.result.records.is_empty() && !t.records.is_empty();

    let max_x = (cols.total + ADD_FIELD_W - vs.width).max(0.);
    let max_y = (v.content_h(row_h) + v.slack() - vs.height).max(0.);
    div().id("tb-grid").relative().flex_1().min_h_0().flex().flex_col()
        // in a page, a frame around the header and the rows only
        .child(div().relative().flex_1().min_h_0().flex().flex_col().when(inline, |d| d.rounded(px(frame_r())).bg(Theme::bg_body()).shadow(crate::design::shadow::frame()))
            .child(header)
            .child(body.when(empty, |d| d.child(div().absolute().left_0().right_0().top(px(48.)).flex().justify_center().text_color(Theme::text_placeholder()).child(crate::settings::label("没有符合筛选条件的记录")))))
            // the frame only while the grid has the keys (or a cell, menu or list of its own is open):
            // with the record panel, a note or another pane in front, no cell is about to take them
            .children(cursor_ring(v, &cols).filter(|_| v.focus.is_focused(window) || v.editing.is_some() || v.popup.is_some() || v.dropdown.is_some()))
            // scrolled sideways: the frozen columns cast a shadow over the ones passing under them
            .when(sx > 0.5, |d| d.child(div().absolute().top_0().bottom_0().left(px(cols.frozen_w)).right_0().overflow_hidden()
                // cast by a block just inside the edge, only what falls past it shown
                .child(div().absolute().top_0().bottom_0().left(px(-24.)).w(px(24.)).bg(Theme::bg_body()).shadow(crate::design::shadow::sticky()))))
            .children(col_edge(v, &cols))
            .children(col_grips(v, &cols, &fields, cx))
            .children(edit_overlay(v, &cols, &fields))
            .children(scrollbars(v, max_x, max_y, vs, cols.frozen_w, inline, cx)))
        .on_scroll(cx.listener(move |v, ev: &ScrollEvent, _, cx| {
            let d = ev.delta.pixel_delta(px(20.));
            let (mut dx, mut dy) = (f32::from(d.x), f32::from(d.y));
            if ev.modifiers.shift && dx == 0. { (dx, dy) = (dy, 0.); }
            let (nx, ny) = ((v.scroll.x - dx).clamp(0., max_x), (v.scroll.y - dy).clamp(0., max_y));
            if (nx, ny) == (v.scroll.x, v.scroll.y) { return; }
            // took the wheel: the page under a block stays where it is
            v.scroll = point(nx, ny);
            cx.stop_propagation();
            cx.notify();
        }))
        .into_any_element()
}

/// Thin scroll thumbs along the right and bottom edges of the body, each dragged to scroll (a band
/// wider than the thumb takes the press). `quiet`: only while the pointer is over the block (a table in a page).
fn scrollbars(v: &TableView, max_x: f32, max_y: f32, vs: Size<f32>, frozen_w: f32, _quiet: bool, cx: &Context<TableView>) -> Vec<AnyElement> {
    use crate::design::scroll::{BAR, MIN, PAD, THUMB, thumb_color};
    let mut out = vec![];
    // the thumbs' places (from the grid's top left), and a band as wide as the shared bar takes the press
    let down = (max_y > 0.).then(|| {
        let track = vs.height;
        let len = (track * track / (track + max_y)).max(MIN);
        (header_h() + (track - len) * (v.scroll.y / max_y), len, max_y / (track - len).max(1.))
    });
    let across = (max_x > 0.).then(|| {
        let track = vs.width - frozen_w;
        let len = (track * track / (track + max_x)).max(MIN);
        (frozen_w + (track - len) * (v.scroll.x / max_x), len, max_x / (track - len).max(1.))
    });
    if let Some((top, len, per)) = down {
        out.push(div().id("tb-thumb-y").absolute().right_0().top(px(top)).w(px(BAR)).h(px(len))
            .on_mouse_down(MouseButton::Left, cx.listener(move |v, ev: &MouseDownEvent, _, cx| { cx.stop_propagation(); v.drag = Some(Drag::Thumb { across: false, p0: f32::from(ev.position.y), s0: v.scroll.y, per }); cx.notify(); }))
            .into_any_element());
    }
    if let Some((left, len, per)) = across {
        out.push(div().id("tb-thumb-x").absolute().bottom_0().left(px(left)).h(px(BAR)).w(px(len))
            .on_mouse_down(MouseButton::Left, cx.listener(move |v, ev: &MouseDownEvent, _, cx| { cx.stop_propagation(); v.drag = Some(Drag::Thumb { across: true, p0: f32::from(ev.position.x), s0: v.scroll.x, per }); cx.notify(); }))
            .into_any_element());
    }
    // drawn as the shared ScrollArea's (`design::scroll`): its width, inset and colour, shown while the
    // pointer is over the grid, a thumb is held or it scrolls, fading out after
    let fades = v.bar_fades.clone();
    let (sx, sy) = (v.scroll.x, v.scroll.y);
    let held = matches!(v.drag, Some(Drag::Thumb { .. }));
    out.push(canvas(|_, _, _| {}, move |b, _, window, cx| {
        let over = b.contains(&window.mouse_position()) || held;
        if let Some((top, len, _)) = down {
            let a = fades[0].step(over, sy, window, cx);
            if a > 0. { window.paint_quad(fill(Bounds::new(point(b.right() - px(PAD + THUMB), b.top() + px(top)), size(px(THUMB), px(len))), thumb_color(a)).corner_radii(px(THUMB / 2.))); }
        }
        if let Some((left, len, _)) = across {
            let a = fades[1].step(over, sx, window, cx);
            if a > 0. { window.paint_quad(fill(Bounds::new(point(b.left() + px(left), b.bottom() - px(PAD + THUMB)), size(px(len), px(THUMB))), thumb_color(a)).corner_radii(px(THUMB / 2.))); }
        }
    }).absolute().inset_0().into_any_element());
    out
}

/// A 分组 header line: fold triangle, the field's name, the group's value, its count.
fn group_row(v: &TableView, t: &Table, g: &query::Group, top: f32, frozen_w: f32, cx: &Context<TableView>) -> AnyElement {
    let key = g.key.clone();
    let f = t.field(g.field);
    let label: AnyElement = match (&g.value, f) {
        (Value::Select(ids), Some(f)) if !ids.is_empty() => div().flex().items_center().gap(px(4.))
            .children(ids.iter().filter_map(|id| f.option(*id)).map(|o| if f.ty == FieldType::Person { person(&o.name, o.color).into_any_element() } else { tag(&o.name, o.color).into_any_element() })).into_any_element(),
        _ if g.label.is_empty() => div().text_color(Theme::text_placeholder()).child(crate::settings::label("空")).into_any_element(),
        _ => div().text_color(Theme::text()).child(ui_text(g.label.clone())).into_any_element(),
    };
    let group = g.clone();
    let _ = v;
    div().id(SharedString::from(format!("tb-group-{}", g.key))).group("tb-group").absolute().left_0().right_0().top(px(top)).h(px(group_h())).flex().items_center().gap(px(6.))
        .pl(px(12. + g.level as f32 * GROUP_INDENT)).bg(if g.level == 0 { header_bg() } else { Theme::bg_body() }).border_b_1().border_color(line()).cursor_pointer()
        .child(div().flex_none().size(px(16.)).flex().items_center().justify_center().child(icon(if g.folded { "triangle-right" } else { "triangle-down" }, 10., Theme::text_caption())))
        .child(div().flex_none().text_size(px(12.)).text_color(Theme::text_caption()).child(ui_text(f.map(|f| f.name.clone()).unwrap_or_default())))
        .child(label)
        .child(div().flex_none().text_size(px(12.)).text_color(Theme::text_placeholder()).child(format!("{}", g.count)))
        .child(div().id(SharedString::from(format!("tb-group-add-{}", g.key))).flex_none().size(px(22.)).rounded(px(4.)).flex().items_center().justify_center()
            .invisible().group_hover("tb-group", |s| s.visible()).hover(|s| s.bg(Theme::fill_hover_strong())).child(icon("plus", 13., Theme::icon()))
            .on_mouse_down(MouseButton::Left, cx.listener(move |v, _, window, cx| { cx.stop_propagation(); v.add_record(Some(&group), window, cx); })))
        .child(div().w(px(frozen_w.min(0.))))
        .on_click(cx.listener(move |v, _, _, cx| v.toggle_fold(&key, cx)))
        .into_any_element()
}

/// The selection's frame: one ring (border-button-active) round the whole range, and, when the range
/// is more than one cell, the same ring on the active cell, laid where a one-cell selection's would be:
/// on the range's edge the two are one line, not two of different widths. BoardUI's grid rings only the active cell;
/// the frame round the range is the spreadsheet convention, in its colour.
fn cursor_ring(v: &TableView, cols: &Cols) -> Option<AnyElement> {
    let s = v.sel?;
    let (r0, r1, c0, c1) = v.sel_rect(cols)?;
    let row = |pos: usize| v.row_of.get(pos).copied().filter(|r| *r != usize::MAX);
    let (top, bottom) = (row(r0)?, row(r1)?);
    let scrolled = |ci: usize| ci >= cols.frozen;
    let left = |ci: usize| cols.x[ci] - if scrolled(ci) { v.scroll.x } else { 0. };
    // a scrolled column passes under the frozen ones, a row under the header: the ring is cut there too
    let (cx0, cy0) = (if scrolled(c0) { cols.frozen_w - 2. } else { 0. }, header_h() - 2.);
    let ring_at = |x0: f32, x1: f32, y0: f32, y1: f32, w: f32| div().absolute().left(px(x0 - w - cx0)).top(px(y0 - w - cy0)).w(px(x1 - x0 + 2. * w - 1.)).h(px(y1 - y0 + 2. * w - 1.))
        .border(px(w)).border_color(ring());
    let y = |r: usize| header_h() + v.tops[r] - v.scroll.y;
    // a range is shown by its fill alone; only a cell picked by itself is framed
    if r0 != r1 || c0 != c1 || s.anchor != s.head { return None }
    Some(div().absolute().left(px(cx0)).top(px(cy0)).right_0().bottom_0().overflow_hidden()
        .child(ring_at(left(c0), left(c1) + cols.w[c1], y(top), y(bottom) + v.row_h_at(bottom), RING_W)).into_any_element())
}

/// A text cell being edited, in place: the cell itself, its width the column's, grown down over the
/// rows below as the text needs (to `EDIT_MAX_H`, then scrolling), and back to its row's height once
/// saved. Framed as the active cell is.
fn edit_overlay(v: &TableView, cols: &Cols, fields: &[&Field]) -> Option<AnyElement> {
    let cell = v.editing?;
    let ci = cols.index(cell.1)?;
    if !fields.get(ci).is_some_and(|f| f.ty.textual()) { return None }
    let row = *v.row_of.get(*v.order.get(&cell.0)?)?;
    if row == usize::MAX { return None }
    let row_h = v.row_h_at(row);
    let scrolled = ci >= cols.frozen;
    let x = cols.x[ci] - if scrolled { v.scroll.x } else { 0. };
    if scrolled && x + cols.w[ci] <= cols.frozen_w { return None }
    let y = header_h() + v.tops[row] - v.scroll.y;
    // the frame on the same lines as the picked cell's (`cursor_ring`): each side over the grid line and one point out
    Some(div().id("tb-cell-editor").absolute().left(px(x - RING_W)).top(px(y - RING_W)).w(px(cols.w[ci] + 2. * RING_W - 1.)).min_h(px(row_h + 2. * RING_W - 1.)).max_h(px(EDIT_MAX_H))
        .overflow_y_scroll().bg(Theme::bg_body()).border(px(RING_W)).border_color(ring())
        // the text exactly where the cell shows it: centred in a one-line row, 10 from the top of a taller one
        .pl(px(cell_pad_x())).pr(px(cell_pad_x() - 1.)).py(px(if row_h > one_line_h() { 10. } else { (row_h - 21.) / 2. })).child(v.input.clone())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .into_any_element())
}
const EDIT_MAX_H: f32 = 240.;

/// The picked cell's frame: 2pt, laid on the grid lines round it.
const RING_W: f32 = 2.;

/// Below the header, every column edge drags its width too, down to the last row: the line it
/// lights (`col_edge`) is the whole edge, so the whole edge takes the drag.
fn col_grips(v: &TableView, cols: &Cols, fields: &[&Field], cx: &Context<TableView>) -> Vec<AnyElement> {
    let end = header_h() + v.tops.last().copied().unwrap_or(0.) - v.scroll.y;
    // a range being dragged passes over the edges without lighting them
    if end <= header_h() || v.drag.as_ref().is_some_and(|d| !matches!(d, Drag::ColWidth { .. })) { return vec![] }
    let mut out = vec![];
    for (ci, f) in fields.iter().enumerate() {
        let scrolled = ci >= cols.frozen;
        let edge = cols.x[ci] + cols.w[ci] - if scrolled { v.scroll.x } else { 0. };
        if scrolled && edge < cols.frozen_w + 4. { continue }
        let (fid, w) = (f.id, cols.w[ci]);
        out.push(div().id(("tb-grip-body", fid)).absolute().top(px(header_h())).h(px(end - header_h())).left(px(edge - 4.)).w(px(8.)).cursor(CursorStyle::ResizeLeftRight)
            .on_hover(cx.listener(move |v, on: &bool, _, cx| { let g = on.then_some(fid); if v.grip_hover != g && (*on || v.grip_hover == Some(fid)) { v.grip_hover = g; cx.notify(); } }))
            .on_mouse_down(MouseButton::Left, cx.listener(move |v, ev: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                v.commit_edit(true, window, cx);
                v.drag = Some(Drag::ColWidth { field: fid, x0: f32::from(ev.position.x), w0: w, w });
                cx.notify();
            }))
            .into_any_element());
    }
    out
}

/// A link cell's address as a browser takes it: a mail address as `mailto:`, a bare host over https.
fn href(ty: FieldType, s: &str) -> String {
    if ty == FieldType::Email { if s.starts_with("mailto:") { s.to_string() } else { format!("mailto:{s}") } }
    else if s.contains("://") || s.starts_with("mailto:") { s.to_string() } else { format!("https://{s}") }
}

/// 适应内容: each row's height in `result`'s rows (a group's is left 0), as tall as its tallest text
/// needs wrapped at its column's width: 20 a line and 10 above and below, at least one line's row,
/// at most `FIT_LINES` lines. A cell's lines are kept by its text and width (`cache`), so only cells
/// whose text or column changed are measured again.
pub(crate) fn fit_heights(cache: &mut HashMap<CellId, (u64, u32, u8)>, base: &Base, t: &Table, view: &View, result: &query::ViewResult, cx: &App) -> Vec<f32> {
    use std::hash::{Hash, Hasher};
    let fields: Vec<(&Field, f32)> = t.visible_fields(view).into_iter().filter(|f| f.ty.textual()).map(|f| (f, (t.width(view, f.id) - 2. * cell_pad_x()).max(8.))).collect();
    let mut wrap = cx.text_system().line_wrapper(font(crate::theme::DOC_UI_FONT), px(text_sm()));
    let one = row_px(RowHeight::Fit);
    result.rows.iter().map(|r| {
        let query::Row::Record(i) = r else { return 0. };
        let rec = &t.records[*i];
        let mut lines = 1u8;
        for (f, w) in &fields {
            let text = field::display_in(base, f, rec.get(f.id));
            if text.is_empty() { continue }
            let mut h = std::collections::hash_map::DefaultHasher::new();
            text.hash(&mut h);
            let (hash, wb) = (h.finish(), w.to_bits());
            let n = match cache.get(&(rec.id, f.id)) {
                Some((th, cw, n)) if *th == hash && *cw == wb => *n,
                _ => {
                    let mut n = 0usize;
                    for para in text.split('\n') {
                        n += 1 + wrap.wrap_line(&[LineFragment::text(para)], px(*w), IndentAdjustment::NoIndent).count();
                        if n >= FIT_LINES { break }
                    }
                    let n = n.min(FIT_LINES) as u8;
                    cache.insert((rec.id, f.id), (hash, wb, n));
                    n
                }
            };
            lines = lines.max(n);
        }
        if lines <= 1 { one } else { (lines as f32 * 20. + 20.).max(one) }
    }).collect()
}
/// 适应内容's tallest row, in lines (the rest of a longer text is in its cell's editor and the record).
const FIT_LINES: usize = 8;

/// `cols` with column `d` moved to place `n`, the lefts laid out again.
fn relaid(cols: &Cols, d: usize, n: usize) -> Cols {
    let (mut ids, mut w) = (cols.ids.clone(), cols.w.clone());
    let (id, cw) = (ids.remove(d), w.remove(d));
    ids.insert(n, id);
    w.insert(n, cw);
    let mut x = Vec::with_capacity(w.len());
    let mut at = GUTTER_W;
    for cw in &w { x.push(at); at += cw; }
    let frozen_w = GUTTER_W + w[..cols.frozen].iter().sum::<f32>();
    Cols { ids, w, x, frozen: cols.frozen, frozen_w, total: at }
}

/// A dragged column's cell (or head), over the rest at `left`: edged on both sides, a little see-through.
fn ghost_col(cell: AnyElement, left: f32, bg: Hsla) -> AnyElement {
    div().absolute().top_0().h_full().left(px(left)).bg(bg).opacity(0.92).border_l_1().border_color(line()).child(cell).into_any_element()
}

/// Where a row's checkbox sits in its controls (and the header's, above them).
const CHECK_X: f32 = 14.;
/// The slot its number (or checkbox) takes.
const NUM_W: f32 = 20.;
/// The row's handle, first, then the checkbox, then 展开 (each `NUM_W` across): `GUTTER_W` in all.
const GRAB_X: f32 = 2.;
const EXPAND_X: f32 = CHECK_X + NUM_W;

impl TableView {
    /// A row in or out of the ticked ones; ⇧, everything from the last ticked row to this one.
    pub(crate) fn tick_row(&mut self, rid: RecId, shift: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(true, window, cx);
        window.focus(&self.focus, cx);
        if shift && let (Some(to), Some(from)) = (self.order.get(&rid).copied(), self.rows_sel.iter().filter_map(|r| self.order.get(r).copied()).max()) {
            let ids: Vec<RecId> = { let st = self.state.read(cx); st.doc.base.table(self.table).map(|t| (from.min(to)..=from.max(to)).map(|p| t.records[self.result.records[p]].id).collect()).unwrap_or_default() };
            self.rows_sel.extend(ids);
        } else if !self.rows_sel.remove(&rid) { self.rows_sel.insert(rid); }
        self.sel = None;
        self.col_sel.clear();
        cx.notify();
    }
}

/// The column edge being dragged (or under the pointer, ready to drag): lit from the header to the last row,
/// not past it (the add-record line and the blank under it have no columns). On the picked cell's frame's
/// own lines (over the grid line and one point out), so where both show the edge covers the frame exactly.
fn col_edge(v: &TableView, cols: &Cols) -> Option<AnyElement> {
    let fid = match v.drag { Some(Drag::ColWidth { field, .. }) => field, Some(_) => return None, None => v.grip_hover? };
    let ci = cols.index(fid)?;
    let scrolled = ci >= cols.frozen;
    let edge = cols.x[ci] + cols.w[ci] - if scrolled { v.scroll.x } else { 0. };
    if scrolled && edge < cols.frozen_w { return None }
    let end = header_h() + v.tops.last().copied().unwrap_or(0.) - v.scroll.y;
    Some(div().absolute().top_0().h(px(end.max(header_h()))).left(px(edge - 1.)).w(px(RING_W)).bg(ring()).into_any_element())
}
