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
        let frozen = v.opts.frozen.clamp(1, ids.len().max(1)).min(ids.len());
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
            let top = self.tops[*row];
            if top < self.scroll.y { self.scroll.y = top; }
            if top + row_h > self.scroll.y + s.height { self.scroll.y = top + row_h - s.height; }
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
        Some((point(b.origin.x + px(x), b.origin.y + px(self.tops[row] - self.scroll.y)), cols.w[c], row_px(v.opts.row_height)))
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
            FieldType::Rating => if let Some(n) = typed.and_then(|s| s.parse::<f64>().ok()) { let tid = self.table; self.edit(cx, |d| d.set_cells(tid, vec![(cell.0, cell.1, Value::Number(n))])); },
            _ => {
                let text = typed.map_or(text, str::to_string);
                self.input.update(cx, |t, cx| { t.align = if ty.numeric() { crate::model::Align::Right } else { crate::model::Align::Left }; t.set_text(&text, cx); });
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
pub(crate) fn mouse_move(v: &mut TableView, ev: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<TableView>) {
    if v.drag.is_none() { return; }
    if ev.pressed_button != Some(MouseButton::Left) { mouse_up_inner(v, cx); return; }
    let state = v.state.clone();
    let st = state.read(cx);
    let Some(t) = st.doc.base.table(v.table) else { return };
    let Some(view) = t.view(v.view) else { return };
    let cols = v.cols(t, view);
    let (px_, py_) = (f32::from(ev.position.x), f32::from(ev.position.y));
    let body = v.body.get();
    let (tops, result) = (v.tops.clone(), v.result.clone());
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
        Drag::Fill(to) => { if let Some(cell) = v.hit_cell(ev.position, t, &cols) { *to = cell.0; } }
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
        Drag::RowMove { y, to, moved, y0, .. } => {
            *y = py_;
            if (py_ - *y0).abs() > 4. { *moved = true; }
            let ly = py_ - f32::from(body.origin.y) + v.scroll.y;
            let row = tops.partition_point(|t| *t + row_px(view.opts.row_height) / 2. <= ly).min(result.rows.len());
            // rows before `row` in the list that are records → its place in the record order
            *to = result.rows[..row].iter().filter(|r| matches!(r, query::Row::Record(_))).count();
        }
    }
    v.drag = Some(drag);
    cx.notify();
}

pub(crate) fn mouse_up(v: &mut TableView, _: &MouseUpEvent, _window: &mut Window, cx: &mut Context<TableView>) { mouse_up_inner(v, cx); }

fn mouse_up_inner(v: &mut TableView, cx: &mut Context<TableView>) {
    let Some(drag) = v.drag.take() else { return };
    let tid = v.table;
    match drag {
        Drag::ColWidth { field, w, w0, .. } => if (w - w0).abs() >= 1. { v.edit_view(cx, |view| { view.opts.widths.insert(field, w.round()); }); },
        Drag::Select => {}
        Drag::Fill(to) => {
            let plan = {
                let st = v.state.read(cx);
                let (Some(t), Some(view)) = (st.doc.base.table(tid), st.doc.base.table(tid).and_then(|t| t.view(v.view))) else { return };
                let cols = v.cols(t, view);
                let (Some((r0, r1, c0, c1)), Some(end)) = (v.sel_rect(&cols), v.order.get(&to).copied()) else { return };
                let rid = |p: usize| t.records[v.result.records[p]].id;
                let src: Vec<RecId> = (r0..=r1).map(rid).collect();
                let dst: Vec<RecId> = if end > r1 { (r1 + 1..=end).map(rid).collect() } else if end < r0 { (end..r0).rev().map(rid).collect() } else { vec![] };
                let fields: Vec<FieldId> = cols.ids[c0..=c1].to_vec();
                let head = if end > r1 || end < r0 { rid(end) } else { rid(r1) };
                (src, dst, fields, head)
            };
            let (src, dst, fields, head) = plan;
            if !dst.is_empty() {
                // filling upwards repeats the source from its last row
                let src = if v.order.get(&dst[0]) < v.order.get(&src[0]) { src.into_iter().rev().collect() } else { src };
                v.edit(cx, |d| d.fill(tid, &src, &dst, &fields));
                if let Some(s) = v.sel.as_mut() {
                    let (a, h) = (v.order.get(&s.anchor.0).copied().unwrap_or(0), v.order.get(&head).copied().unwrap_or(0));
                    let first = v.order.get(&s.head.0).copied().unwrap_or(0).min(a);
                    if h < first { s.anchor.0 = head; } else { s.head.0 = head; }
                }
            }
        }
        Drag::ColMove { field, to: Some(to), .. } => {
            let order = {
                let st = v.state.read(cx);
                let (Some(t), Some(view)) = (st.doc.base.table(tid), st.doc.base.table(tid).and_then(|t| t.view(v.view))) else { return };
                let shown: Vec<FieldId> = t.visible_fields(view).iter().map(|f| f.id).collect();
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
        Drag::ColMove { field, .. } => {
            // a click, not a drag: select the column
            v.col_sel = vec![field];
            v.sel = None;
            v.rows_sel.clear();
        }
        Drag::RowMove { ids, to, moved, .. } => if moved {
            let target = {
                let st = v.state.read(cx);
                let Some(t) = st.doc.base.table(tid) else { return };
                // the record now at that place in the view → its row in the table's own order
                v.result.records.get(to).copied().unwrap_or(t.records.len())
            };
            v.edit(cx, |d| d.move_records(tid, &ids, target));
        },
    }
    cx.notify();
}

pub(crate) fn render(v: &TableView, base: &Base, t: &Table, view: &View, window: &mut Window, cx: &Context<TableView>) -> AnyElement {
    let cols = v.cols(t, view);
    let fields = t.visible_fields(view);
    let row_h = row_px(view.opts.row_height);
    let (sx, sy) = (v.scroll.x, v.scroll.y);
    let vs = v.view_size();
    let sel = v.sel_rect(&cols);
    let fill_to = match &v.drag { Some(Drag::Fill(r)) => v.order.get(r).copied(), _ => None };
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
        div().id(("tb-hdr", fid)).group("tb-hdr").relative().flex_none().w(px(w)).h_full().flex().items_center().gap(px(4.)).px(px(cell_pad_x()))
            .border_r_1().border_color(line())
            .when(on, |d| d.bg(sel_header()))
            .child(icon(field_glyph(f.ty), 16., Theme::text_placeholder()))
            // shadcn's table head: `foreground` names by a `muted-foreground` glyph
            .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(header_text()).child(ui_text(f.name.clone())))
            .when_some(sorted, |d, desc| d.child(icon(if desc { "caret-down" } else { "caret-up" }, 12., Theme::primary())))
            .child(div().id(("tb-hdr-menu", fid)).flex_none().flex().items_center().justify_center().size(px(18.)).rounded(px(4.)).cursor_pointer()
                .hover(|s| s.bg(Theme::fill_hover_strong())).when(!on, |d| d.invisible().group_hover("tb-hdr", |s| s.visible()))
                .child(icon("chevron-down", 13., Theme::text_caption()))
                .on_mouse_down(MouseButton::Left, cx.listener(move |v, ev: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    v.commit_edit(true, window, cx);
                    if !v.col_sel.contains(&fid) { v.col_sel = vec![fid]; }
                    v.sel = None;
                    v.open_popup(point(ev.position.x - px(w - 24.).min(px(200.)), ev.position.y + px(14.)), Popup::FieldMenu(fid), cx);
                })))
            .on_mouse_down(MouseButton::Left, cx.listener(move |v, ev: &MouseDownEvent, window, cx| {
                v.commit_edit(true, window, cx);
                window.focus(&v.focus, cx);
                if ev.click_count >= 2 { menus::edit_field(v, fid, ev.position, window, cx); return; }
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
                let x = f32::from(ev.position.x);
                v.drag = Some(Drag::ColMove { field: fid, x0: x, x, to: None });
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Right, cx.listener(move |v, ev: &MouseDownEvent, window, cx| {
                v.commit_edit(true, window, cx);
                if !v.col_sel.contains(&fid) { v.col_sel = vec![fid]; }
                v.sel = None;
                v.open_popup(ev.position, Popup::FieldMenu(fid), cx);
            }))
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
        .child(div().id("tb-hdr-check").group("tb-hdr-all").flex_none().w(px(GUTTER_W)).h_full().flex().items_center().justify_center().border_r_1().border_color(line()).cursor_pointer()
            .child(div().when(!all_sel, |d| d.invisible().group_hover("tb-hdr-all", |s| s.visible())).child(checkbox(all_sel)))
            .on_click(cx.listener(|v, _, _, cx| {
                let ids: Vec<RecId> = { let st = v.state.read(cx); st.doc.base.table(v.table).map(|t| v.result.records.iter().map(|i| t.records[*i].id).collect()).unwrap_or_default() };
                if v.rows_sel.len() == ids.len() { v.rows_sel.clear(); } else { v.rows_sel = ids.into_iter().collect(); }
                v.sel = None;
                cx.notify();
            })));
    for ci in 0..cols.frozen { hdr_frozen = hdr_frozen.child(hdr_cell(ci)); }
    let mut hdr_scroll = div().flex().h_full().ml(px(-sx));
    for ci in cols.frozen..fields.len() { hdr_scroll = hdr_scroll.child(hdr_cell(ci)); }
    hdr_scroll = hdr_scroll.child(div().id("tb-add-field").flex_none().w(px(ADD_FIELD_W)).h_full().flex().items_center().justify_center()
        .cursor_pointer().hover(|s| s.bg(Theme::fill_hover())).child(icon("plus", 15., Theme::icon()))
        .on_click(cx.listener(|v, ev: &ClickEvent, window, cx| { let p = ev.position(); menus::add_field(v, None, point(p.x - px(160.), p.y + px(16.)), window, cx); })));
    // in a page the fill follows the frame's top corners
    let header = div().flex_none().flex().h(px(header_h())).bg(hdr_bg).border_b_1().border_color(line()).when(inline, |d| d.rounded_t(px(frame_r())))
        .child(hdr_frozen).child(div().flex_1().min_w_0().h_full().overflow_hidden().child(hdr_scroll));

    // ---- rows in sight ----
    let first = v.tops.partition_point(|y| *y + group_h().max(row_h) <= sy).min(v.result.rows.len());
    let last = v.tops.partition_point(|y| *y < sy + vs.height).min(v.result.rows.len());
    let mut body = div().id("tb-body").relative().flex_1().min_h_0().overflow_hidden()
        .child({ let b = v.body.clone(); canvas(move |bounds, _, _| b.set(bounds), |_, _, _, _| {}).absolute().size_full() });
    let rec_pos = |i: usize| v.order.get(&t.records[i].id).copied();
    for ri in first..last {
        let top = v.tops[ri] - sy;
        match &v.result.rows[ri] {
            query::Row::Group(g) => body = body.child(group_row(v, t, g, top, cols.frozen_w, cx)),
            query::Row::Record(i) => {
                let r = &t.records[*i];
                let rid = r.id;
                let pos = rec_pos(*i).unwrap_or(0);
                let checked = v.rows_sel.contains(&rid);
                let tint = query::row_color(base, t, r, view, today);
                let in_fill = fill_to.zip(sel).is_some_and(|(to, (r0, r1, _, _))| (pos > r1 && pos <= to) || (pos < r0 && pos >= to));
                let dragging = matches!(&v.drag, Some(Drag::RowMove { ids, moved: true, .. }) if ids.contains(&rid));
                let gutter = div().id(("tb-gutter", rid as usize)).relative().flex_none().w(px(GUTTER_W)).h_full().flex().items_center().justify_center()
                    .border_r_1().border_b_1().border_color(line())
                    .when(checked, |d| d.bg(sel_bg()))
                    .child(div().when(checked, |d| d.invisible()).group_hover("tb-row", |s| s.invisible()).text_size(px(12.)).text_color(Theme::text_placeholder()).child(format!("{}", pos + 1)))
                    .when(manual, |d| d.child(div().id(("tb-grab", rid as usize)).absolute().left(px(2.)).top_0().bottom_0().w(px(16.)).flex().items_center().justify_center()
                        .invisible().group_hover("tb-row", |s| s.visible()).cursor(CursorStyle::OpenHand)
                        .child(icon("grip-vertical", 14., Theme::text_placeholder()))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |v, ev: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            v.commit_edit(true, window, cx);
                            let ids: Vec<RecId> = if v.rows_sel.contains(&rid) { v.rows_sel.iter().copied().collect() } else { vec![rid] };
                            let y = f32::from(ev.position.y);
                            v.drag = Some(Drag::RowMove { ids, to: 0, y0: y, y, moved: false });
                        }))))
                    .child(div().id(("tb-check", rid as usize)).absolute().left(px(20.)).top_0().bottom_0().w(px(18.)).flex().items_center().justify_center().cursor_pointer()
                        .when(!checked, |d| d.invisible().group_hover("tb-row", |s| s.visible())).child(checkbox(checked))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |v, ev: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            v.commit_edit(true, window, cx);
                            // ⇧: everything from the last ticked row to this one
                            if ev.modifiers.shift && let (Some(to), Some(from)) = (v.order.get(&rid).copied(), v.rows_sel.iter().filter_map(|r| v.order.get(r).copied()).max()) {
                                let ids: Vec<RecId> = { let st = v.state.read(cx); st.doc.base.table(v.table).map(|t| (from.min(to)..=from.max(to)).map(|p| t.records[v.result.records[p]].id).collect()).unwrap_or_default() };
                                v.rows_sel.extend(ids);
                            } else if !v.rows_sel.remove(&rid) { v.rows_sel.insert(rid); }
                            v.sel = None;
                            v.col_sel.clear();
                            cx.notify();
                        })))
;
                let cell = |ci: usize| {
                    let f = fields[ci];
                    let id: CellId = (rid, f.id);
                    let in_sel = sel.is_some_and(|(r0, r1, c0, c1)| pos >= r0 && pos <= r1 && ci >= c0 && ci <= c1);
                    let multi = sel.is_some_and(|(r0, r1, c0, c1)| r0 != r1 || c0 != c1);
                    let editing = v.editing == Some(id);
                    let corner = sel.is_some_and(|(_, r1, _, c1)| pos == r1 && ci == c1) && !editing && v.drag.as_ref().is_none_or(|d| matches!(d, Drag::Fill(_)));
                    let filling = in_fill && sel.is_some_and(|(_, _, c0, c1)| ci >= c0 && ci <= c1);
                    let found = hits.contains(&id);
                    let content: AnyElement = if editing { div().w_full().child(v.input.clone()).into_any_element() } else { cell::content(v, base, t, f, r, row_h, cx) };
                    div().id(("tb-cell", (rid as usize) << 12 | (f.id as usize & 0xfff))).relative().flex_none().w(px(cols.w[ci])).h_full().flex().items_start().px(px(cell_pad_x())).overflow_hidden()
                        .border_b_1().border_r_1().border_color(line())
                        .when(v.col_sel.contains(&f.id), |d| d.bg(sel_bg()))
                        .when(found, |d| d.bg(if hit_now == Some(id) { found_current() } else { found_bg() }))
                        // the range is filled except its active cell, which the ring marks inside the frame (as Excel)
                        .when((in_sel && multi && v.sel.is_none_or(|s| s.head != id)) || filling, |d| d.bg(range_bg()))
                        .when(editing, |d| d.bg(Theme::bg_body()))
                        .child(div().w_full().h(px(row_h.min(one_line_h()) - 1.)).flex().items_center().when(row_h > one_line_h(), |d| d.h_full().items_start().py(px(10.))).child(content))
                        // the title cell offers 查看 (the record) while the row is hovered, as 飞书
                        .when(ci == 0 && !editing, |d| d.child(div().absolute().right(px(6.)).top_0().h(px(row_h.min(one_line_h()))).flex().items_center().invisible().group_hover("tb-row", |s| s.visible())
                            .child(div().id(("tb-open", rid as usize)).h(px(22.)).px(px(6.)).flex().items_center().gap(px(4.)).rounded(px(crate::design::geo().tool_r)).border_1().border_color(Theme::line_border()).bg(Theme::bg_body()).shadow(crate::design::shadow::xs()).cursor_pointer()
                                .hover(|s| s.bg(Theme::fill_hover())).child(icon("expand", 12., Theme::icon())).child(div().text_size(px(12.)).text_color(Theme::text()).child(crate::settings::label("查看")))
                                .on_mouse_down(MouseButton::Left, cx.listener(move |v, _, window, cx| { cx.stop_propagation(); v.commit_edit(true, window, cx); record::open(v, rid, window, cx); })))))
                        .when(corner, |d| d.child(div().id(("tb-fill", rid as usize)).absolute().right(px(-1.)).bottom(px(-1.)).size(px(7.)).bg(ring()).border_1().border_color(Theme::bg_body())
                            .cursor(CursorStyle::Crosshair)
                            .on_mouse_down(MouseButton::Left, cx.listener(move |v, _, _, cx| { cx.stop_propagation(); v.drag = Some(Drag::Fill(rid)); cx.notify(); }))))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |v, ev: &MouseDownEvent, window, cx| {
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
                for ci in 0..cols.frozen { fz = fz.child(cell(ci)); }
                let mut sc = div().flex().h_full().ml(px(-sx));
                for ci in cols.frozen..fields.len() { sc = sc.child(cell(ci)); }
                body = body.child(div().group("tb-row").absolute().left_0().w(px((cols.total - sx).max(cols.frozen_w))).top(px(top)).h(px(row_h)).flex()
                    .when_some(tint, |d, slot| d.bg(row_tint(slot)))
                    .when(checked, |d| d.bg(sel_bg()))
                    .when(dragging, |d| d.opacity(0.4))
                    .child(fz).child(div().flex_1().min_w_0().h_full().overflow_hidden().child(sc)));
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
    // gesture feedback: where a dragged column or row will land, a width being dragged
    match &v.drag {
        Some(Drag::ColMove { to: Some(to), .. }) => {
            let edge = if *to >= cols.ids.len() { cols.total } else { cols.x[*to] };
            let x = if *to <= cols.frozen { edge } else { edge - sx };
            if *to <= cols.frozen || x >= cols.frozen_w { body = body.child(div().absolute().top_0().bottom_0().left(px(x - 1.)).w(px(2.)).bg(Theme::primary())); }
        }
        Some(Drag::RowMove { to, moved: true, .. }) => {
            let row = v.result.records.get(*to).and_then(|i| rec_pos(*i)).and_then(|p| v.row_of.get(p)).copied().filter(|r| *r != usize::MAX);
            let y = row.map_or(v.tops.last().copied().unwrap_or(0.), |r| v.tops[r]) - sy;
            body = body.child(div().absolute().left_0().right_0().top(px(y - 1.)).h(px(2.)).bg(Theme::primary()));
        }
        _ => {}
    }
    let empty = v.result.records.is_empty() && !t.records.is_empty();

    // ---- 统计 ----
    let stat_cell = |ci: usize| {
        let f = fields[ci];
        let fid = f.id;
        let text = view.opts.stats.get(&fid).map(|s| format!("{} {}", crate::settings::label(s.label()), query::stat(base, t, f, &v.result.records, *s)));
        div().id(("tb-stat", fid)).group("tb-stat").flex_none().w(px(cols.w[ci])).h_full().flex().items_center().justify_end().px(px(cell_pad_x())).cursor_pointer()
            .hover(|s| s.bg(Theme::fill_hover())).text_size(px(12.)).text_color(Theme::text_caption()).overflow_hidden().whitespace_nowrap()
            .map(|d| match text {
                Some(t) => d.child(ui_text(t)),
                None => d.child(div().invisible().group_hover("tb-stat", |s| s.visible()).flex().items_center().gap(px(2.)).child(crate::settings::label("统计")).child(icon("chevron-down", 11., Theme::text_caption()))),
            })
            .on_click(cx.listener(move |v, ev: &ClickEvent, _, cx| { let p = ev.position(); v.open_popup(point(p.x - px(80.), p.y - px(12.)), Popup::Stat(fid), cx); }))
    };
    let count = v.result.records.len();
    let mut ft_frozen = div().flex_none().flex().w(px(cols.frozen_w)).h_full().when(!inline, |d| d.border_r_1().border_color(frozen_line()))
        .child(div().flex_none().w(px(GUTTER_W)).h_full());
    for ci in 0..cols.frozen {
        ft_frozen = ft_frozen.child(if ci == 0 && !view.opts.stats.contains_key(&fields[0].id) {
            // 飞书: the count at the right of the title column, its menu the summary choices
            let fid = fields[0].id;
            div().flex_none().w(px(cols.w[0])).h_full().flex().items_center().justify_end().px(px(cell_pad_x()))
                .child(div().id("tb-count").h(px(24.)).px(px(4.)).flex().items_center().gap(px(4.)).rounded(px(4.)).cursor_pointer().hover(|s| s.bg(Theme::fill_hover()))
                    .text_size(px(12.)).text_color(Theme::text_caption())
                    .child(format!("{count} {}", crate::settings::label("条记录"))).child(icon("chevron-down", 11., Theme::text_caption()))
                    .on_click(cx.listener(move |v, ev: &ClickEvent, _, cx| { let p = ev.position(); v.open_popup(point(p.x - px(80.), p.y - px(12.)), Popup::Stat(fid), cx); })))
                .into_any_element()
        } else { stat_cell(ci).into_any_element() });
    }
    let mut ft_scroll = div().flex().h_full().ml(px(-sx));
    for ci in cols.frozen..fields.len() { ft_scroll = ft_scroll.child(stat_cell(ci)); }
    // in a page the count sits under the frame, unboxed (飞书's embedded base)
    let footer = div().flex_none().flex().h(px(footer_h())).when(!inline, |d| d.border_t_1().border_color(line())).bg(Theme::bg_body())
        .child(ft_frozen).child(div().flex_1().min_w_0().h_full().overflow_hidden().child(ft_scroll));

    let max_x = (cols.total + ADD_FIELD_W - vs.width).max(0.);
    let max_y = (v.content_h(row_h) + v.slack() - vs.height).max(0.);
    div().id("tb-grid").relative().flex_1().min_h_0().flex().flex_col()
        // in a page, a frame around the header and the rows only
        .child(div().relative().flex_1().min_h_0().flex().flex_col().when(inline, |d| d.rounded(px(frame_r())).bg(Theme::bg_body()).shadow(crate::design::shadow::frame()))
            .child(header)
            .child(body.when(empty, |d| d.child(div().absolute().left_0().right_0().top(px(48.)).flex().justify_center().text_color(Theme::text_placeholder()).child(crate::settings::label("没有符合筛选条件的记录")))))
            .children(cursor_ring(v, &cols, row_h))
            .children(col_edge(v, &cols))
            .children(scrollbars(v, max_x, max_y, vs, cols.frozen_w, inline)))
        .child(footer)
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

/// Thin scroll thumbs along the right and bottom edges of the body.
/// `quiet`: only while the pointer is over the block (a table in a page).
fn scrollbars(v: &TableView, max_x: f32, max_y: f32, vs: Size<f32>, frozen_w: f32, quiet: bool) -> Vec<AnyElement> {
    let mut out = vec![];
    if max_y > 0. {
        let track = vs.height;
        let len = (track * track / (track + max_y)).max(28.);
        let top = header_h() + (track - len) * (v.scroll.y / max_y);
        out.push(div().absolute().right(px(2.)).top(px(top)).w(px(6.)).h(px(len)).rounded(px(3.)).bg(Theme::scrollbar_thumb()).when(quiet, |d| d.invisible().group_hover("tb-block", |s| s.visible())).into_any_element());
    }
    if max_x > 0. {
        let track = vs.width - frozen_w;
        let len = (track * track / (track + max_x)).max(28.);
        let left = frozen_w + (track - len) * (v.scroll.x / max_x);
        out.push(div().absolute().bottom(px(if quiet { 2. } else { footer_h() + 2. })).left(px(left)).h(px(6.)).w(px(len)).rounded(px(3.)).bg(Theme::scrollbar_thumb()).when(quiet, |d| d.invisible().group_hover("tb-block", |s| s.visible())).into_any_element());
    }
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
/// is more than one cell, a thinner one on the active cell. BoardUI's grid rings only the active cell;
/// the frame round the range is the spreadsheet convention, in its colour.
fn cursor_ring(v: &TableView, cols: &Cols, row_h: f32) -> Option<AnyElement> {
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
    let mut clip = div().absolute().left(px(cx0)).top(px(cy0)).right_0().bottom_0().overflow_hidden()
        .child(ring_at(left(c0), left(c1) + cols.w[c1], y(top), y(bottom) + row_h, 2.));
    if r0 != r1 || c0 != c1 {
        let (hr, hc) = (v.order.get(&s.head.0).and_then(|p| row(*p))?, cols.index(s.head.1)?);
        clip = clip.child(ring_at(left(hc) + 1., left(hc) + cols.w[hc] - 1., y(hr) + 1., y(hr) + row_h - 1., 1.));
    }
    Some(clip.into_any_element())
}

/// The column edge being dragged (or under the pointer, ready to drag): lit from the header to the last row,
/// not past it (the add-record line and the blank under it have no columns).
fn col_edge(v: &TableView, cols: &Cols) -> Option<AnyElement> {
    let fid = match v.drag { Some(Drag::ColWidth { field, .. }) => field, _ => v.grip_hover? };
    let ci = cols.index(fid)?;
    let scrolled = ci >= cols.frozen;
    let edge = cols.x[ci] + cols.w[ci] - if scrolled { v.scroll.x } else { 0. };
    if scrolled && edge < cols.frozen_w { return None }
    let end = header_h() + v.tops.last().copied().unwrap_or(0.) - v.scroll.y;
    Some(div().absolute().top_0().h(px(end.max(header_h()))).left(px(edge - 1.5)).w(px(2.)).bg(Theme::primary()).into_any_element())
}
