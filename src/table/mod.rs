//! 数据表 (多维表格) view, after 飞书's: tabs for the file's tables and the
//! table's views, a toolbar (字段配置 / 筛选 / 分组 / 排序 / 行高 / 填色), and
//! the view itself (`grid`, one 表格 per file), with 记录详情 beside it.
//! The engine is `fdl_core::table` (re-exported here); this module is the
//! GPUI side.
//!
//! `BaseState` is one open file: its `Doc` (data + undo), shared by every
//! view showing that file (a block in a page, the same table full-pane) and
//! saved a moment after each change. `TableView` is one view onto it, drawn
//! in a page's block or over the whole note pane (`full`), as the editor asks.
//!
//! Every change goes `TableView::edit` → `Doc` (ops, one undo step) →
//! `BaseState` notifies its views and schedules the file write.

pub mod style;

mod cell;
mod editors;
mod fields;
mod grid;
mod keys;
mod menus;
mod panels;
mod record;

pub use fdl_core::table::*;

/// A new file's one blank table, it and its columns named in the interface language.
pub fn blank_base() -> Base { Base::blank_named(field::now(), crate::settings::text("未命名数据表"), crate::settings::text("文本")) }
pub use fdl_core::table::View;

use crate::feishu_theme::Theme;
use crate::input::{TextField, TextFieldEvent};
use gpui::prelude::FluentBuilder;
use gpui::*;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;
use style::*;

/// The open files, by path: views of one file share its state.
#[derive(Default)]
struct Hub(HashMap<PathBuf, WeakEntity<BaseState>>);
impl Global for Hub {}

pub struct BaseState {
    pub doc: Doc,
    pub path: PathBuf,
    /// The file could not be read (or written): shown in place of the table.
    pub error: Option<SharedString>,
    /// The file as we last read or wrote it (`sqlite::stamp`).
    stamp: Option<u64>,
    save: Option<Task<()>>,
    /// A 版本历史 snapshot: shown, never written (an edit changes a copy and is gone).
    pub read_only: bool,
}

impl BaseState {
    /// The state of the file at `path`, read now if no view has it open.
    pub fn open(path: &Path, cx: &mut App) -> Entity<BaseState> {
        if let Some(s) = cx.default_global::<Hub>().0.get(path).and_then(|w| w.upgrade()) { return s; }
        let path = path.to_path_buf();
        let s = cx.new(|cx| {
            // a version kept under .quiet/history is history: look, don't touch
            let read_only = path.components().any(|c| c.as_os_str() == ".quiet") && path.components().any(|c| c.as_os_str() == "history");
            let mut s = BaseState { doc: Doc::default(), path: path.clone(), error: None, stamp: None, save: None, read_only };
            s.read();
            cx.on_release(|s: &mut BaseState, _| s.write()).detach();
            s
        });
        let hub = cx.default_global::<Hub>();
        hub.0.retain(|_, w| w.upgrade().is_some());
        hub.0.insert(path, s.downgrade());
        s
    }

    #[cfg(not(target_family = "wasm"))]
    fn read(&mut self) {
        match sqlite::load(&self.path) {
            Ok(base) => { self.doc.replace(base); self.error = None; self.stamp = sqlite::stamp(&self.path); }
            Err(e) => self.error = Some(format!("{}：{e}", crate::settings::label("无法读取数据表")).into()),
        }
    }
    #[cfg(target_family = "wasm")]
    fn read(&mut self) { self.doc.replace(sample::demo(field::now())); }

    /// Write what changed, now.
    #[cfg(not(target_family = "wasm"))]
    pub fn write(&mut self) {
        self.save = None;
        if self.read_only || self.doc.dirty.is_clean() || self.path.as_os_str().is_empty() { self.doc.dirty = Dirty::default(); return; }
        // never write a new file where one was: it moved or was deleted under us
        if !self.path.exists() { log::error!("数据表文件不在原处，未写入 {}", self.path.display()); return; }
        match sqlite::save(&self.path, &self.doc.base, &self.doc.dirty) {
            Ok(()) => { self.doc.dirty = Dirty::default(); self.stamp = sqlite::stamp(&self.path); }
            Err(e) => log::error!("数据表保存失败 {}: {e}", self.path.display()),
        }
    }
    #[cfg(target_family = "wasm")]
    pub fn write(&mut self) { self.save = None; self.doc.dirty = Dirty::default(); }

    /// Change the base; views redraw and the file is written shortly after.
    pub fn edit<R>(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Doc) -> R) -> R {
        if self.read_only { let mut scratch = self.doc.clone(); return f(&mut scratch); }
        let r = f(&mut self.doc);
        if !self.doc.dirty.is_clean() && self.save.is_none() {
            self.save = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(400)).await;
                let _ = this.update(cx, |s, _| s.write());
            }));
        }
        cx.notify();
        r
    }

    /// Another program (or another copy of the app) wrote the file: read it
    /// again, unless we hold changes of our own.
    #[cfg(not(target_family = "wasm"))]
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.path.as_os_str().is_empty() || !self.doc.dirty.is_clean() { return; }
        let now = sqlite::stamp(&self.path);
        if now.is_some() && now != self.stamp { self.read(); cx.notify(); }
    }
    #[cfg(target_family = "wasm")]
    pub fn refresh(&mut self, _cx: &mut Context<Self>) {}

    /// The file moved (its page was renamed): later writes follow it.
    pub fn moved(&mut self, to: &Path, cx: &mut App) {
        // a version being looked at is not the file, wherever the file goes
        if self.path == to || self.read_only { return; }
        let hub = cx.default_global::<Hub>();
        if let Some(w) = hub.0.remove(&self.path) { hub.0.insert(to.to_path_buf(), w); }
        self.path = to.to_path_buf();
    }
}

/// The file at `path` was replaced (a version restored): an open view reads it again.
pub fn reload(path: &Path, cx: &mut App) {
    if let Some(s) = cx.default_global::<Hub>().0.get(path).and_then(|w| w.upgrade()) { s.update(cx, |s, cx| s.refresh(cx)); }
}

/// Write every open file's pending changes (the app is closing or saving all).
pub fn flush_all(cx: &mut App) {
    let open: Vec<Entity<BaseState>> = cx.default_global::<Hub>().0.values().filter_map(|w| w.upgrade()).collect();
    for s in open { s.update(cx, |s, _| s.write()); }
}

pub enum TableEvent {
    /// Leave the full-pane view (back to the page, or out of the file).
    Close,
    /// Open this view over the whole pane.
    Expand,
    /// The table or view shown changed: the block remembers which.
    Shown,
}

/// A cell: record and field.
pub type CellId = (RecId, FieldId);

/// The selected range: from the cell first picked to the one last reached.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sel {
    pub anchor: CellId,
    pub head: CellId,
}

pub(crate) enum Drag {
    /// A column's right edge: pointer x and width at the start, the width now.
    ColWidth { field: FieldId, x0: f32, w0: f32, w: f32 },
    /// Extending the selection.
    Select,
    /// The fill handle: from the selection to this record's row.
    Fill(RecId),
    /// A header pressed, then (past a few pixels) dragged: `to` is the gap
    /// it would drop into, as a column index. Released unmoved, it was a click.
    ColMove { field: FieldId, x0: f32, x: f32, to: Option<usize> },
    /// Row handles dragged: lands before the record at place `to` of the view's order.
    RowMove { ids: Vec<RecId>, to: usize, y0: f32, y: f32, moved: bool },
}

/// What floats over the view. One at a time, with at most one `Dropdown` on top of it.
pub(crate) enum Popup {
    FieldMenu(FieldId),
    FieldForm(fields::FieldForm),
    CellMenu(RecId),
    Select { rec: RecId, field: FieldId, editing: Option<OptId> },
    Date { rec: RecId, field: FieldId, year: i64, month: u32 },
    Links { rec: RecId, field: FieldId },
    Files { rec: RecId, field: FieldId },
    Panel(panels::Panel),
    /// The view list under a table-in-a-page's view name.
    Stat(FieldId),
    More,
}

/// A plain list of choices on top of a popup (a form's select boxes).
pub(crate) struct Dropdown {
    pub pos: Point<Pixels>,
    pub width: f32,
    pub items: Vec<DropItem>,
    pub pick: Rc<dyn Fn(&mut TableView, usize, &mut Window, &mut Context<TableView>)>,
}

pub(crate) struct DropItem {
    pub label: SharedString,
    pub glyph: Option<&'static str>,
    pub on: bool,
    /// A coloured tag instead of plain text (select options).
    pub tag: Option<u8>,
}

impl DropItem {
    pub fn new(label: impl Into<SharedString>) -> Self { DropItem { label: label.into(), glyph: None, on: false, tag: None } }
    pub fn on(mut self, on: bool) -> Self { self.on = on; self }
    pub fn glyph(mut self, g: &'static str) -> Self { self.glyph = Some(g); self }
    pub fn tag(mut self, slot: u8) -> Self { self.tag = Some(slot); self }
}

/// Renaming in place: a view tab, a table tab.
pub(crate) enum Rename { Table(TableId) }

pub struct TableView {
    pub state: Entity<BaseState>,
    pub(crate) focus: FocusHandle,
    pub table: TableId,
    pub view: ViewId,
    /// Over the whole note pane (not a block in a page).
    pub full: bool,
    /// A table file's own view: there is no page to go back to.
    pub standalone: bool,
    /// The block's height in a page.
    pub height: f32,
    /// The view's rows, computed against `Doc::rev`.
    pub(crate) result: Rc<query::ViewResult>,
    /// Top of each row of `result.rows`, and the total height.
    pub(crate) tops: Rc<Vec<f32>>,
    /// Each shown record's place in `result.records`, and that place's row in `result.rows` (`usize::MAX`: folded away).
    pub(crate) order: Rc<HashMap<RecId, usize>>,
    pub(crate) row_of: Rc<Vec<usize>>,
    result_key: (u64, TableId, ViewId, i64),
    pub(crate) sel: Option<Sel>,
    pub(crate) rows_sel: HashSet<RecId>,
    /// Selected columns (header clicks; ⇧ extends, ⌘ adds or takes away), in the order picked.
    pub(crate) col_sel: Vec<FieldId>,
    pub(crate) editing: Option<CellId>,
    pub(crate) input: Entity<TextField>,
    /// A second field for popups: option search, renames.
    pub(crate) aux: Entity<TextField>,
    pub(crate) scroll: Point<f32>,
    /// The grid body's bounds in the window, as last painted.
    pub(crate) body: Rc<Cell<Bounds<Pixels>>>,
    pub(crate) drag: Option<Drag>,
    pub(crate) popup: Option<(Point<Pixels>, Popup)>,
    pub(crate) dropdown: Option<Dropdown>,
    pub(crate) rename: Option<Rename>,
    /// 记录详情.
    pub(crate) expanded: Option<RecId>,
    pub(crate) record: record::RecordUi,
    /// 表内搜索: the text, its matches, the current one.
    pub(crate) find: Option<Find>,
    pub(crate) find_input: Entity<TextField>,
    /// The column whose right edge is under the pointer (its width can be dragged there).
    pub(crate) grip_hover: Option<FieldId>,
    _refresh: Task<()>,
    _subs: Vec<Subscription>,
}

pub(crate) struct Find {
    pub hits: Vec<CellId>,
    pub at: usize,
}

impl EventEmitter<TableEvent> for TableView {}

impl Focusable for TableView {
    fn focus_handle(&self, _: &App) -> FocusHandle { self.focus.clone() }
}

impl TableView {
    pub fn new(state: Entity<BaseState>, shown: &TableRef, full: bool, standalone: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = |hint: &str, cx: &mut Context<Self>| cx.new(|cx| { let mut t = TextField::new(crate::settings::label(hint), cx); t.size = TEXT; t });
        let input = field("", cx);
        input.update(cx, |t, _| t.tab_events = true);
        let aux = field("", cx);
        let find_input = field("查找", cx);
        let mut subs = vec![
            cx.observe(&state, |_, _, cx| cx.notify()),
            cx.subscribe_in(&input, window, |v, _, ev: &TextFieldEvent, window, cx| match ev {
                TextFieldEvent::Submit => { v.commit_edit(true, window, cx); v.step(1, 0, false, window, cx); }
                TextFieldEvent::Tab => { v.commit_edit(true, window, cx); v.step(0, 1, false, window, cx); }
                TextFieldEvent::ShiftTab => { v.commit_edit(true, window, cx); v.step(0, -1, false, window, cx); }
                TextFieldEvent::Cancel => v.commit_edit(false, window, cx),
                _ => {}
            }),
            cx.subscribe_in(&aux, window, |v, _, ev: &TextFieldEvent, window, cx| v.aux_event(*ev, window, cx)),
            cx.subscribe_in(&find_input, window, |v, _, ev: &TextFieldEvent, window, cx| match ev {
                TextFieldEvent::Changed => v.run_find(cx),
                TextFieldEvent::Submit | TextFieldEvent::Down => v.find_step(1, cx),
                TextFieldEvent::ShiftSubmit | TextFieldEvent::Up => v.find_step(-1, cx),
                TextFieldEvent::Cancel => v.close_find(window, cx),
                _ => {}
            }),
        ];
        subs.push(cx.on_focus_out(&input.focus_handle(cx), window, |v, _, window, cx| if v.editing.is_some() { v.commit_edit(true, window, cx); }));
        // a click away from the rename box keeps the name, as Finder does
        subs.push(cx.on_focus_out(&aux.focus_handle(cx), window, |v, _, _, cx| v.finish_rename(true, cx)));
        // another program may write the file: look again every couple of seconds while shown
        let refresh = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            if this.update(cx, |v, cx| v.state.update(cx, |s, cx| s.refresh(cx))).is_err() { break; }
        });
        let (table, view) = {
            let base = &state.read(cx).doc.base;
            fdl_core::table::shown(base, shown).map(|(t, v)| (t.id, v.id)).unwrap_or((0, 0))
        };
        TableView {
            state, focus: cx.focus_handle(), table, view, full, standalone, height: shown.height,
            result: Default::default(), tops: Default::default(), order: Default::default(), row_of: Default::default(), result_key: (u64::MAX, 0, 0, 0),
            sel: None, rows_sel: HashSet::new(), col_sel: vec![], editing: None, input, aux, scroll: point(0., 0.),
            body: Rc::new(Cell::new(Bounds::default())), drag: None, popup: None, dropdown: None, rename: None,
            expanded: None, record: Default::default(), find: None, find_input,
            grip_hover: None, _refresh: refresh, _subs: subs,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) { window.focus(&self.focus, cx); }


    /// The names of the table and view shown (what the block saves).
    pub fn shown(&self, cx: &App) -> (Option<String>, Option<String>) {
        let base = &self.state.read(cx).doc.base;
        let t = base.table(self.table);
        (t.map(|t| t.name.clone()), t.and_then(|t| t.view(self.view)).map(|v| v.name.clone()))
    }

    /// Run an edit on the document (one undo step per `Doc` call inside `f`).
    pub(crate) fn edit<R>(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Doc) -> R) -> R {
        self.state.update(cx, |s, cx| s.edit(cx, f))
    }

    /// Change the shown view's settings.
    pub(crate) fn edit_view(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut View)) {
        let (t, v) = (self.table, self.view);
        self.edit(cx, |d| d.edit_view(t, v, f));
    }

    /// The table and view shown still exist (a table deleted elsewhere, a
    /// file read again): fall back to the first, and bring the cached rows
    /// up to date.
    fn sync(&mut self, cx: &mut Context<Self>) {
        let state = self.state.clone();
        let st = state.read(cx);
        let base = &st.doc.base;
        let before = (self.table, self.view);
        if base.table(self.table).is_none() { self.table = base.tables.first().map_or(0, |t| t.id); }
        let Some(t) = base.table(self.table) else { return };
        if t.view(self.view).is_none() { self.view = t.views.first().map_or(0, |v| v.id); }
        let Some(v) = t.view(self.view) else { return };
        let today = field::today();
        let key = (st.doc.rev, self.table, self.view, today);
        if key != self.result_key {
            let result = query::run(base, t, v);
            let mut tops = Vec::with_capacity(result.rows.len() + 1);
            let (mut y, h) = (0., v.opts.row_height.px());
            for r in &result.rows {
                tops.push(y);
                y += match r { query::Row::Group(_) => GROUP_H, query::Row::Record(_) => h };
            }
            tops.push(y);
            let order: HashMap<RecId, usize> = result.records.iter().enumerate().map(|(p, i)| (t.records[*i].id, p)).collect();
            let mut row_of = vec![usize::MAX; result.records.len()];
            for (ri, r) in result.rows.iter().enumerate() { if let query::Row::Record(i) = r && let Some(p) = order.get(&t.records[*i].id) { row_of[*p] = ri; } }
            self.order = Rc::new(order);
            self.row_of = Rc::new(row_of);
            self.result = Rc::new(result);
            self.tops = Rc::new(tops);
            self.result_key = key;
            // what is selected may be gone
            let alive = |c: &CellId| t.record(c.0).is_some() && t.field(c.1).is_some();
            if self.sel.is_some_and(|s| !alive(&s.anchor) || !alive(&s.head)) { self.sel = None; }
            self.rows_sel.retain(|r| t.record(*r).is_some());
            self.col_sel.retain(|f| t.field(*f).is_some());
            if self.expanded.is_some_and(|r| t.record(r).is_none()) { self.expanded = None; }
        }
        if before != (self.table, self.view) { cx.emit(TableEvent::Shown); }
    }

    /// `sync` outside a frame (after an edit whose result is needed at once).
    pub(crate) fn sync_now(&mut self, cx: &mut Context<Self>) { self.sync(cx); }

    pub(crate) fn clear_selection(&mut self) {
        self.sel = None;
        self.rows_sel.clear();
        self.col_sel.clear();
        self.editing = None;
    }

    pub(crate) fn open_popup(&mut self, pos: Point<Pixels>, popup: Popup, cx: &mut Context<Self>) {
        self.dropdown = None;
        self.popup = Some((pos, popup));
        cx.notify();
    }

    pub(crate) fn close_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.popup.take().is_some() || self.dropdown.take().is_some() {
            if !self.input.read(cx).is_focused(window) { window.focus(&self.focus, cx); }
            cx.notify();
        }
    }

    pub(crate) fn open_dropdown(&mut self, pos: Point<Pixels>, width: f32, items: Vec<DropItem>, pick: impl Fn(&mut TableView, usize, &mut Window, &mut Context<TableView>) + 'static, cx: &mut Context<Self>) {
        self.dropdown = Some(Dropdown { pos, width, items, pick: Rc::new(pick) });
        cx.notify();
    }

    pub(crate) fn undo(&mut self, cx: &mut Context<Self>) { self.edit(cx, |d| d.undo()); }
    pub(crate) fn redo(&mut self, cx: &mut Context<Self>) { self.edit(cx, |d| d.redo()); }

    /// Keep a cell being edited and a name being typed, without a window at hand: the page is about
    /// to be left or saved, and the view may be dropped with no focus change to commit them.
    pub fn commit_pending(&mut self, cx: &mut Context<Self>) {
        if let Some(cell) = self.editing.take() {
            let text = self.input.read(cx).text.clone();
            let tid = self.table;
            self.edit(cx, |d| {
                let Some(f) = d.base.table(tid).and_then(|t| t.field(cell.1)) else { return };
                let v = field::parse(f, &text);
                d.set_cells(tid, vec![(cell.0, cell.1, v)]);
            });
        }
        self.finish_rename(true, cx);
    }

    /// Finish what is in progress and write the file (the page is closing).
    pub fn finish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(true, window, cx);
        self.state.update(cx, |s, _| s.write());
    }

    // ---- 表内搜索 ----

    pub(crate) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.is_none() { self.find = Some(Find { hits: vec![], at: 0 }); }
        self.find_input.update(cx, |t, cx| t.select_all_text(cx));
        window.focus(&self.find_input.focus_handle(cx), cx);
        self.run_find(cx);
    }

    pub(crate) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(crate) fn run_find(&mut self, cx: &mut Context<Self>) {
        if self.find.is_none() { return; }
        let needle = self.find_input.read(cx).text.clone();
        let hits: Vec<CellId> = {
            let base = &self.state.read(cx).doc.base;
            let Some(t) = base.table(self.table) else { return };
            let Some(v) = t.view(self.view) else { return };
            let fields: Vec<FieldId> = t.visible_fields(v).iter().map(|f| f.id).collect();
            if needle.is_empty() { vec![] } else { query::find(base, t, &self.result.records, &fields, &needle).into_iter().map(|(i, f)| (t.records[i].id, f)).collect() }
        };
        let at = self.find.as_ref().map_or(0, |f| f.at).min(hits.len().saturating_sub(1));
        self.find = Some(Find { hits, at });
        self.find_step(0, cx);
    }

    pub(crate) fn find_step(&mut self, by: i32, cx: &mut Context<Self>) {
        let Some(f) = self.find.as_mut() else { return };
        if !f.hits.is_empty() {
            f.at = (f.at as i32 + by).rem_euclid(f.hits.len() as i32) as usize;
            let c = f.hits[f.at];
            self.sel = Some(Sel { anchor: c, head: c });
            self.reveal(c, cx);
        }
        cx.notify();
    }

    fn aux_event(&mut self, ev: TextFieldEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.rename.is_some() {
            match ev {
                TextFieldEvent::Submit => { self.finish_rename(true, cx); window.focus(&self.focus, cx); }
                TextFieldEvent::Cancel => { self.finish_rename(false, cx); window.focus(&self.focus, cx); }
                _ => {}
            }
            cx.notify();
            return;
        }
        editors::aux_event(self, ev, window, cx);
    }

    /// End renaming: keep the name typed (unless it is blank) or drop it.
    pub(crate) fn finish_rename(&mut self, save: bool, cx: &mut Context<Self>) {
        let Some(r) = self.rename.take() else { return };
        let name = self.aux.read(cx).text.trim().to_string();
        if save && !name.is_empty() {
            match r {
                Rename::Table(t) => { self.edit(cx, |d| d.rename_table(t, &name)); }
            }
            cx.emit(TableEvent::Shown);
        }
        cx.notify();
    }

    pub(crate) fn begin_rename(&mut self, r: Rename, window: &mut Window, cx: &mut Context<Self>) {
        let name = {
            let base = &self.state.read(cx).doc.base;
            match r {
                Rename::Table(t) => base.table(t).map(|t| t.name.clone()),
            }
        };
        let Some(name) = name else { return };
        self.popup = None;
        // the name at the size it shows in, all of it selected
        self.aux.update(cx, |t, cx| { t.size = 14.; t.line_height = 20.; t.set_placeholder("", cx); t.set_text(&name, cx); t.select_all_text(cx); });
        window.focus(&self.aux.focus_handle(cx), cx);
        self.rename = Some(r);
        cx.notify();
    }
}

impl Render for TableView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync(cx);
        let state = self.state.clone();
        let st = state.read(cx);
        let root = div().id("table-view").key_context("Table").track_focus(&self.focus).relative().w_full().when(self.full, |d| d.h_full()).flex().flex_col()
            .bg(Theme::bg_body()).font_family(crate::theme::DOC_UI_FONT).text_size(px(TEXT)).text_color(Theme::text())
            .on_key_down(cx.listener(keys::key_down))
            // a selected cell takes typed text (keys.rs, EntityInputHandler)
            .when(self.sel.is_some() && self.editing.is_none() && self.focus.is_focused(window), |d| {
                let (focus, entity) = (self.focus.clone(), cx.entity());
                d.child(canvas(|_, _, _| {}, move |bounds, _, window, cx| window.handle_input(&focus, ElementInputHandler::new(bounds, entity), cx)).absolute().size_0())
            })
            // a gesture in progress follows the pointer wherever it goes in the window
            .when(self.drag.is_some(), |d| {
                let (a, b) = (cx.weak_entity(), cx.weak_entity());
                d.child(canvas(|_, _, _| {}, move |_, _, window, _| {
                    window.on_mouse_event(move |ev: &MouseMoveEvent, phase, window, cx| if phase == DispatchPhase::Capture { let _ = a.update(cx, |v, cx| grid::mouse_move(v, ev, window, cx)); });
                    window.on_mouse_event(move |ev: &MouseUpEvent, phase, window, cx| if phase == DispatchPhase::Capture { let _ = b.update(cx, |v, cx| grid::mouse_up(v, ev, window, cx)); });
                }).absolute().size_full())
            });
        if let Some(e) = &st.error {
            return root.items_center().justify_center().child(div().text_color(Theme::text_caption()).child(e.clone())).into_any_element();
        }
        let base = &st.doc.base;
        let (Some(t), Some(v)) = (base.table(self.table), base.table(self.table).and_then(|t| t.view(self.view))) else {
            return root.into_any_element();
        };
        // In a page the block is as tall as its rows (up to a cap): no scroll beyond what overflows the cap.
        // A row added (and revealed) while the block was still the old height left the rows scrolled up,
        // a blank under them, once the block grew.
        if !self.full {
            let rows = self.tops.last().copied().unwrap_or(0.) + ADD_ROW_H;
            self.scroll.y = self.scroll.y.min((rows - rows.min(INLINE_MAX_ROWS_H)).max(0.));
        }
        // one view, a 表格 (a file's old 看板 or other kinds show as one too)
        let main = grid::render(self, base, t, v, window, cx);
        let side = self.expanded.and_then(|r| t.record(r)).map(|r| record::render(self, base, t, v, r, window, cx));
        let body = div().flex_1().min_h_0().flex()
            .child(div().relative().flex_1().min_w_0().h_full().flex().flex_col().child(main).children(menus::find_bar(self, cx)).children(menus::selection_bar(self, t, cx)))
            .children(side);
        // In a page, as 飞书 docx embeds a base: one quiet header line (the table's
        // name; its tools only while the pointer is over the table), no frame
        // around the block, and as tall as its rows. Full pane: the whole chrome.
        let root = if self.full {
            root.child(menus::bar(self, t, v, cx)).child(body)
        } else {
            let rows = self.tops.last().copied().unwrap_or(0.) + ADD_ROW_H;
            let h = INLINE_HEAD_H + HEADER_H + rows.min(INLINE_MAX_ROWS_H) + FOOTER_H + 2.;
            self.height = h;
            root.group("tb-block").h(px(h))
                .child(menus::inline_header(self, t, v, cx))
                .child(body)
        };
        root
            .children(menus::popup(self, base, t, v, window, cx))
            .children(menus::dropdown(self, cx))
            .into_any_element()
    }
}

