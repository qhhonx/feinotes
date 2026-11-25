//! Keys on the grid (as in 飞书 and any spreadsheet), copy / paste as
//! tab-separated text (so cells go to and from Excel), and CSV in and out.

use super::*;
use std::ops::Range;

pub(crate) fn key_down(v: &mut TableView, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<TableView>) {
    // text fields (a cell being edited, a popup's search box) take their own keys
    if !v.focus.is_focused(window) { return; }
    let m = &ev.keystroke.modifiers;
    let key = ev.keystroke.key.as_str();
    let cmd = if cfg!(target_os = "macos") { m.platform } else { m.control };
    // typing in a note in 记录详情: the keys are the note's
    if notes::typing(v, window, cx) { return; }
    if header_key(v, key, m.shift, window, cx) { cx.stop_propagation(); return; }
    let mut used = true;
    match key {
        "z" if cmd && m.shift => v.redo(cx),
        "z" if cmd => v.undo(cx),
        "y" if cmd => v.redo(cx),
        "f" if cmd => v.open_find(window, cx),
        "escape" if v.popup.is_some() || v.dropdown.is_some() => v.close_popup(window, cx),
        "escape" if v.find.is_some() => v.close_find(window, cx),
        "escape" if v.expanded.is_some() => { notes::close(v, cx); v.expanded = None; cx.notify(); }
        "escape" if v.sel.is_some() || !v.rows_sel.is_empty() || !v.col_sel.is_empty() => { v.clear_selection(); cx.notify(); }
        "c" if cmd => copy(v, false, cx),
        "x" if cmd => { copy(v, false, cx); v.clear_cells(cx); }
        "v" if cmd => paste(v, cx),
        "a" if cmd => select_all(v, cx),
        "up" => v.step(-1, 0, m.shift, window, cx),
        "down" => v.step(1, 0, m.shift, window, cx),
        "left" => v.step(0, -1, m.shift, window, cx),
        "right" => v.step(0, 1, m.shift, window, cx),
        "tab" => v.step(0, if m.shift { -1 } else { 1 }, false, window, cx),
        "enter" => v.step(if m.shift { -1 } else { 1 }, 0, false, window, cx),
        "backspace" | "delete" => v.clear_cells(cx),
        "f2" => if let Some(s) = v.sel { v.begin_edit(s.head, None, window, cx); },
        "space" => if let Some(s) = v.sel {
            let check = { let st = v.state.read(cx); st.doc.base.table(v.table).and_then(|t| t.field(s.head.1)).is_some_and(|f| f.ty == FieldType::Checkbox) };
            if check { v.toggle_check(s.head, cx); } else { record::open(v, s.head.0, window, cx); }
        },
        // typed text comes through the grid's input handler (below), so an input
        // method composes from the first key and Windows' VK_PACKET text arrives too
        _ => used = false,
    }
    if used { cx.stop_propagation(); }
}

/// While a cell is selected (not edited) the grid takes text input itself:
/// typing starts editing the cell with what was typed in place of what is
/// there, and an input method's first composition moves into the cell editor.
impl EntityInputHandler for TableView {
    fn text_for_range(&mut self, _: Range<usize>, _: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> { Some(String::new()) }
    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> { Some(UTF16Selection { range: 0..0, reversed: false }) }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> { None }
    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {}
    fn replace_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.sel else { return };
        if self.editing.is_some() || text.is_empty() || text.chars().any(char::is_control) { return; }
        self.begin_edit(s.head, Some(text), window, cx);
    }
    fn replace_and_mark_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, new_sel: Option<Range<usize>>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.sel else { return };
        if self.editing.is_some() || text.is_empty() { return; }
        self.begin_edit(s.head, Some(""), window, cx);
        if self.editing.is_some() { self.input.update(cx, |t, cx| t.replace_and_mark_text_in_range(None, text, new_sel, window, cx)); }
    }
    fn bounds_for_range(&mut self, _: Range<usize>, _: Bounds<Pixels>, _: &mut Window, cx: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        let (p, w, h) = self.cell_origin(self.sel?.head, cx)?;
        Some(Bounds::new(p, size(px(w), px(h))))
    }
    fn character_index_for_point(&mut self, _: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> { None }
}

fn select_all(v: &mut TableView, cx: &mut Context<TableView>) {
    let st = v.state.read(cx);
    let Some(t) = st.doc.base.table(v.table) else { return };
    let Some(view) = t.view(v.view) else { return };
    let cols = v.cols(t, view);
    let (Some(first), Some(last)) = (v.result.records.first(), v.result.records.last()) else { return };
    let (Some(c0), Some(c1)) = (cols.ids.first(), cols.ids.last()) else { return };
    v.sel = Some(Sel { anchor: (t.records[*first].id, *c0), head: (t.records[*last].id, *c1) });
    v.rows_sel.clear();
    v.col_sel.clear();
    cx.notify();
}

fn write_clipboard(text: String, cx: &mut App) {
    // scripted runs keep off the system clipboard
    let mut fake = crate::clipboard::FAKE_CLIPBOARD.lock().unwrap();
    match fake.as_mut() {
        Some(f) => { f.text = text; f.html = None; }
        None => cx.write_to_clipboard(ClipboardItem::new_string(text)),
    }
}

fn read_clipboard(cx: &mut App) -> Option<String> {
    let fake = crate::clipboard::FAKE_CLIPBOARD.lock().unwrap().clone();
    match fake { Some(f) => Some(f.text), None => cx.read_from_clipboard().and_then(|i| i.text()) }.filter(|t| !t.is_empty())
}

/// Copy the selection (ticked rows, a column, or the cell range) as
/// tab-separated text; `markdown`: the whole view as a Markdown table.
pub(crate) fn copy(v: &mut TableView, markdown: bool, cx: &mut Context<TableView>) {
    let text = {
        let st = v.state.read(cx);
        let base = &st.doc.base;
        let Some(t) = base.table(v.table) else { return };
        let Some(view) = t.view(v.view) else { return };
        if markdown { csv::to_markdown(base, t, view, &v.result.records) } else {
            let cols = v.cols(t, view);
            let show = |r: RecId, f: FieldId| t.record(r).zip(t.field(f)).map(|(r, f)| field::display_in(base, f, r.get(f.id))).unwrap_or_default();
            let grid: Vec<Vec<String>> = if !v.rows_sel.is_empty() {
                v.result.records.iter().map(|i| t.records[*i].id).filter(|r| v.rows_sel.contains(r)).map(|r| cols.ids.iter().map(|f| show(r, *f)).collect()).collect()
            } else if !v.col_sel.is_empty() {
                // the selected columns in the view's order
                let picked: Vec<FieldId> = cols.ids.iter().copied().filter(|f| v.col_sel.contains(f)).collect();
                v.result.records.iter().map(|i| picked.iter().map(|f| show(t.records[*i].id, *f)).collect()).collect()
            } else {
                v.sel_cells(t, &cols).into_iter().map(|row| row.into_iter().map(|(r, f)| show(r, f)).collect()).collect()
            };
            if grid.is_empty() { return; }
            csv::write(&grid, '\t').trim_end_matches('\n').to_string()
        }
    };
    write_clipboard(text, cx);
}

/// Paste text at the selection: a grid goes in from its top-left cell
/// (adding rows past the end); one value fills every selected cell.
pub(crate) fn paste(v: &mut TableView, cx: &mut Context<TableView>) {
    let Some(text) = read_clipboard(cx) else { return };
    let grid = csv::parse_clipboard(&text);
    if grid.is_empty() { return; }
    let plan = {
        let st = v.state.read(cx);
        let Some(t) = st.doc.base.table(v.table) else { return };
        let Some(view) = t.view(v.view) else { return };
        let cols = v.cols(t, view);
        let Some((r0, r1, c0, c1)) = v.sel_rect(&cols) else { return };
        let rid = |p: usize| t.records[v.result.records[p]].id;
        if grid.len() == 1 && grid[0].len() == 1 && (r0 != r1 || c0 != c1) {
            // one value over a range: repeat it
            let line: Vec<String> = vec![grid[0][0].clone(); c1 - c0 + 1];
            ((r0..=r1).map(rid).collect::<Vec<_>>(), cols.ids[c0..=c1].to_vec(), vec![line; r1 - r0 + 1])
        } else {
            ((r0..v.result.records.len()).map(rid).collect(), cols.ids[c0..].to_vec(), grid)
        }
    };
    let tid = v.table;
    v.edit(cx, |d| d.paste(tid, &plan.0, &plan.1, &plan.2));
}

pub(crate) fn export_csv(v: &mut TableView, window: &mut Window, cx: &mut Context<TableView>) {
    let (name, text) = {
        let st = v.state.read(cx);
        let base = &st.doc.base;
        let Some(t) = base.table(v.table) else { return };
        let Some(view) = t.view(v.view) else { return };
        (format!("{}.csv", t.name), csv::write(&csv::export(base, t, view, &v.result.records), ','))
    };
    let picked = crate::devtools::take_pick().and_then(|p| p.into_iter().next());
    let dir = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Downloads")).unwrap_or_default();
    let prompt = picked.is_none().then(|| cx.prompt_for_new_path(&dir, Some(&name)));
    cx.spawn_in(window, async move |_, _| {
        let path = match prompt { Some(p) => p.await.ok().and_then(Result::ok).flatten(), None => picked };
        // a BOM, so Excel reads the UTF-8 as such
        if let Some(path) = path { let _ = std::fs::write(path, format!("\u{feff}{text}")); }
    }).detach();
}

/// Keys on the headers, which are picked as cells are: ↑ from the first row picks the column's
/// header; there ←/→ pick the next (⇧: as well), ↓ goes back to the first row, ⏎ / F2 edit the
/// name. False: the key is the grid's as usual.
fn header_key(v: &mut TableView, key: &str, shift: bool, window: &mut Window, cx: &mut Context<TableView>) -> bool {
    let (ids, first) = {
        let st = v.state.read(cx);
        let Some(t) = st.doc.base.table(v.table) else { return false };
        let Some(view) = t.view(v.view) else { return false };
        let ids: Vec<FieldId> = t.visible_fields(view).iter().map(|f| f.id).collect();
        let first = (0..v.result.records.len()).find(|p| v.row_of.get(*p).is_some_and(|r| *r != usize::MAX)).map(|p| t.records[v.result.records[p]].id);
        (ids, first)
    };
    // in the grid: ↑ on the first row climbs into the header
    if let Some(s) = v.sel {
        if key == "up" && !shift && s.anchor == s.head && Some(s.head.0) == first {
            v.col_sel = vec![s.head.1];
            v.sel = None;
            cx.notify();
            return true;
        }
        return false;
    }
    let (Some(&last), true) = (v.col_sel.last(), v.rows_sel.is_empty()) else { return false };
    let Some(at) = ids.iter().position(|f| *f == last) else { return false };
    match key {
        "enter" | "f2" if v.col_sel.len() == 1 => v.begin_rename(Rename::Field(last), window, cx),
        "left" | "right" => {
            let to = if key == "left" { at.saturating_sub(1) } else { (at + 1).min(ids.len() - 1) };
            if shift { if !v.col_sel.contains(&ids[to]) { v.col_sel.push(ids[to]); } } else { v.col_sel = vec![ids[to]]; }
            cx.notify();
        }
        "down" => {
            let Some(r) = first else { return true };
            v.col_sel.clear();
            v.sel = Some(Sel { anchor: (r, last), head: (r, last) });
            cx.notify();
        }
        "up" => {}
        _ => return false,
    }
    true
}
