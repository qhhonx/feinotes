//! Cell editors that open as a popup under the cell: the option list of
//! 单选 / 多选 / 人员 (search, create, recolour, rename, delete), the month
//! calendar of 日期, the record list of 关联, and 附件's file list. Text-like
//! cells are edited in place (`grid::begin_edit`).

use super::style::*;
use super::*;
use crate::ui::{ui_text};

/// The text a cell's in-place editor starts with.
pub(crate) fn edit_text(f: &Field, v: &Value) -> String {
    match v {
        Value::Number(n) if f.ty == FieldType::Progress || (f.ty == FieldType::Number && f.opts.style == NumberStyle::Percent) => field::plain_number((n * 100. * 1e9).round() / 1e9),
        Value::Number(n) => field::plain_number(*n),
        Value::Date(ms) => field::iso(*ms, f.opts.time),
        Value::Text(s) => s.clone(),
        _ => String::new(),
    }
}

pub(crate) fn open_select(v: &mut TableView, cell: CellId, pos: Point<Pixels>, window: &mut Window, cx: &mut Context<TableView>) {
    v.aux.update(cx, |t, cx| { t.size = text_sm(); t.line_height = 20.; t.set_placeholder(crate::settings::label("查找或创建选项"), cx); t.set_text("", cx); });
    window.focus(&v.aux.focus_handle(cx), cx);
    v.rename = None;
    v.open_popup(pos, Popup::Select { rec: cell.0, field: cell.1, editing: None }, cx);
}

pub(crate) fn open_date(v: &mut TableView, cell: CellId, pos: Point<Pixels>, cx: &mut Context<TableView>) {
    let cur = { let st = v.state.read(cx); st.doc.base.table(v.table).and_then(|t| t.record(cell.0)).and_then(|r| match r.get(cell.1) { Value::Date(d) => Some(*d), _ => None }) };
    let (y, m, ..) = field::date_parts(cur.unwrap_or_else(field::today));
    v.open_popup(pos, Popup::Date { rec: cell.0, field: cell.1, year: y, month: m }, cx);
}

pub(crate) fn open_links(v: &mut TableView, cell: CellId, pos: Point<Pixels>, window: &mut Window, cx: &mut Context<TableView>) {
    v.aux.update(cx, |t, cx| { t.size = text_sm(); t.line_height = 20.; t.set_placeholder(crate::settings::label("搜索记录"), cx); t.set_text("", cx); });
    window.focus(&v.aux.focus_handle(cx), cx);
    v.rename = None;
    v.open_popup(pos, Popup::Links { rec: cell.0, field: cell.1 }, cx);
}

/// Keys in the popup's search box.
pub(crate) fn aux_event(v: &mut TableView, ev: TextFieldEvent, window: &mut Window, cx: &mut Context<TableView>) {
    match (&v.popup, ev) {
        (Some((_, Popup::Select { rec, field, editing: None })), TextFieldEvent::Submit) => {
            let (rec, fid, name) = (*rec, *field, v.aux.read(cx).text.trim().to_string());
            if name.is_empty() { return; }
            // Enter picks the option of that name, making it first if need be
            let id = ensure_option(v, fid, &name, cx);
            if let Some(id) = id { pick_option(v, rec, fid, id, window, cx); }
            v.aux.update(cx, |t, cx| t.set_text("", cx));
        }
        (Some((_, Popup::Select { field, editing: Some(opt), .. })), TextFieldEvent::Submit) => {
            let (fid, opt, name) = (*field, *opt, v.aux.read(cx).text.trim().to_string());
            edit_option(v, fid, cx, |o| { if !name.is_empty() && let Some(o) = o.iter_mut().find(|o| o.id == opt) { o.name = name; } });
            back_to_options(v, window, cx);
        }
        (Some((_, Popup::Select { editing: Some(_), .. })), TextFieldEvent::Cancel) => back_to_options(v, window, cx),
        (Some(_), TextFieldEvent::Cancel) => v.close_popup(window, cx),
        _ => cx.notify(),
    }
}

fn back_to_options(v: &mut TableView, window: &mut Window, cx: &mut Context<TableView>) {
    if let Some((_, Popup::Select { editing, .. })) = v.popup.as_mut() { *editing = None; }
    v.aux.update(cx, |t, cx| { t.size = text_sm(); t.line_height = 20.; t.set_placeholder(crate::settings::label("查找或创建选项"), cx); t.set_text("", cx); });
    window.focus(&v.aux.focus_handle(cx), cx);
    cx.notify();
}

/// Change a select field's options (one undo step).
pub(crate) fn edit_option(v: &mut TableView, fid: FieldId, cx: &mut Context<TableView>, f: impl FnOnce(&mut Vec<SelectOption>)) {
    let tid = v.table;
    v.edit(cx, |d| {
        let Some(mut field) = d.base.table(tid).and_then(|t| t.field(fid)).cloned() else { return };
        f(&mut field.opts.options);
        d.change_field(tid, field);
    });
}

/// The id of the option named `name`, added when the field has none such.
pub(crate) fn ensure_option(v: &mut TableView, fid: FieldId, name: &str, cx: &mut Context<TableView>) -> Option<OptId> {
    let tid = v.table;
    v.edit(cx, |d| {
        let f = d.base.table(tid)?.field(fid)?;
        if let Some(o) = f.option_named(name) { return Some(o.id); }
        let mut f = f.clone();
        let id = d.base.new_id();
        let color = (f.opts.options.len() % 10) as u8;
        f.opts.options.push(SelectOption { id, name: name.to_string(), color });
        d.change_field(tid, f);
        Some(id)
    })
}

/// A click on an option: 单选 takes it and closes (again: clears it); 多选 / 人员 toggle it and stay open.
pub(crate) fn pick_option(v: &mut TableView, rec: RecId, fid: FieldId, opt: OptId, window: &mut Window, cx: &mut Context<TableView>) {
    let tid = v.table;
    let single = v.edit(cx, |d| {
        let Some(t) = d.base.table(tid) else { return true };
        let (Some(f), Some(r)) = (t.field(fid), t.record(rec)) else { return true };
        let mut ids = match r.get(fid) { Value::Select(ids) => ids.clone(), _ => vec![] };
        let single = f.ty == FieldType::SingleSelect;
        if single { ids = if ids == [opt] { vec![] } else { vec![opt] }; }
        else if let Some(i) = ids.iter().position(|i| *i == opt) { ids.remove(i); } else { ids.push(opt); }
        d.set_cells(tid, vec![(rec, fid, Value::Select(ids))]);
        single
    });
    if single { v.close_popup(window, cx); }
}

fn close_on_out<E: InteractiveElement>(d: E, cx: &Context<TableView>) -> E {
    d.occlude().on_mouse_down_out(cx.listener(|v, _, window, cx| if v.dropdown.is_none() { v.close_popup(window, cx) }))
}

/// 单选 / 多选 / 人员's option list, or (after an option's ⋯) that option's name, colours and 删除.
pub(crate) fn select_popup(v: &TableView, t: &Table, rec: RecId, fid: FieldId, editing: Option<OptId>, cx: &Context<TableView>) -> AnyElement {
    let (Some(f), Some(r)) = (t.field(fid), t.record(rec)) else { return div().into_any_element() };
    let chosen: Vec<OptId> = match r.get(fid) { Value::Select(ids) => ids.clone(), _ => vec![] };
    let person = f.ty == FieldType::Person;
    let shell = close_on_out(crate::design::Popover::new("tb-select").w(px(260.)).py(px(6.)), cx);
    if let Some(opt) = editing.and_then(|id| f.option(id)) {
        let oid = opt.id;
        let mut swatches = div().px(px(12.)).py(px(8.)).flex().flex_wrap().gap(px(8.));
        for slot in 0..SLOTS as u8 {
            let (bg, fg) = tag_colors(slot);
            swatches = swatches.child(div().id(("tb-swatch", slot as usize)).size(px(20.)).rounded(px(4.)).bg(bg).flex().items_center().justify_center().cursor_pointer()
                .when(opt.color == slot, |d| d.child(icon("check", 12., fg)))
                .on_click(cx.listener(move |v, _, _, cx| edit_option(v, fid, cx, |o| if let Some(o) = o.iter_mut().find(|o| o.id == oid) { o.color = slot; }))));
        }
        return shell
            .child(div().px(px(12.)).pb(px(4.)).text_size(px(12.)).text_color(Theme::text_caption()).child(crate::settings::label("选项名称")))
            .child(div().px(px(12.)).child(input_box("tb-opt-name").child(div().flex_1().child(v.aux.clone()))))
            .when(!person, |d| d.child(div().px(px(12.)).pt(px(10.)).text_size(px(12.)).text_color(Theme::text_caption()).child(crate::settings::label("颜色"))).child(swatches))
            .child(menu_rule())
            .child(menu_item("tb-opt-del", "bin", crate::settings::label("删除选项"), true).on_click(cx.listener(move |v, _, window, cx| {
                edit_option(v, fid, cx, |o| o.retain(|o| o.id != oid));
                back_to_options(v, window, cx);
            })))
            .into_any_element();
    }
    let query = v.aux.read(cx).text.trim().to_lowercase();
    let shown: Vec<&SelectOption> = f.opts.options.iter().filter(|o| query.is_empty() || o.name.to_lowercase().contains(&query)).collect();
    let exact = f.opts.options.iter().any(|o| o.name.to_lowercase() == query);
    let mut list = crate::design::scroll::make_room(div().id("tb-select-list").max_h(px(280.)).overflow_y_scroll().track_scroll(&crate::design::scroll::handle("tb-select-list")).flex().flex_col(), "tb-select-list", crate::design::menu::PAD);
    for o in shown {
        let (oid, on, name) = (o.id, chosen.contains(&o.id), o.name.clone());
        list = list.child(div().id(("tb-opt", oid)).group("tb-opt").flex_none().h(px(32.)).mx(px(4.)).px(px(8.)).flex().items_center().gap(px(8.)).rounded(px(crate::design::geo().menu_row_r)).cursor_pointer()
            .hover(|s| s.bg(Theme::fill_hover()))
            .child(div().flex_1().min_w_0().flex().child(if person { super::style::person(&o.name, o.color).into_any_element() } else { tag(&o.name, o.color).into_any_element() }))
            .when(on, |d| d.child(icon("check", 14., crate::design::color::foreground())))
            .child(div().id(("tb-opt-more", oid)).flex_none().size(px(22.)).rounded(px(4.)).flex().items_center().justify_center().invisible().group_hover("tb-opt", |s| s.visible())
                .hover(|s| s.bg(Theme::fill_hover_strong())).child(icon("ellipsis", 14., Theme::icon()))
                .on_mouse_down(MouseButton::Left, cx.listener(move |v, _, window, cx| {
                    cx.stop_propagation();
                    if let Some((_, Popup::Select { editing, .. })) = v.popup.as_mut() { *editing = Some(oid); }
                    v.aux.update(cx, |t, cx| { t.size = text_sm(); t.line_height = 20.; t.set_placeholder("", cx); t.set_text(&name, cx); t.select_all_text(cx); });
                    window.focus(&v.aux.focus_handle(cx), cx);
                    cx.notify();
                })))
            .on_click(cx.listener(move |v, _, window, cx| pick_option(v, rec, fid, oid, window, cx))));
    }
    if !query.is_empty() && !exact {
        let name = v.aux.read(cx).text.trim().to_string();
        let slot = (f.opts.options.len() % 10) as u8;
        list = list.child(list_row("tb-opt-new")
            .child(icon("plus", 14., Theme::icon())).child(div().flex_none().text_color(Theme::text_caption()).child(crate::settings::label(if person { "添加人员" } else { "新建选项" })))
            .child(if person { super::style::person(&name, slot).into_any_element() } else { tag(&name, slot).into_any_element() })
            .on_click(cx.listener(move |v, _, window, cx| {
                if let Some(id) = ensure_option(v, fid, &name, cx) { pick_option(v, rec, fid, id, window, cx); }
                v.aux.update(cx, |t, cx| t.set_text("", cx));
            })));
    } else if f.opts.options.is_empty() {
        list = list.child(div().px(px(12.)).py(px(10.)).text_color(Theme::text_placeholder()).child(crate::settings::label(if person { "输入名字后回车添加" } else { "输入名称后回车新建选项" })));
    }
    shell
        .child(div().px(px(8.)).pb(px(6.)).child(input_box("tb-opt-search").child(icon("search", 14., Theme::text_placeholder())).gap(px(6.)).child(div().flex_1().child(v.aux.clone()))))
        .child(list.child(crate::design::scroll::bar("tb-select-list", &crate::design::scroll::handle("tb-select-list"), 0.)))
        .into_any_element()
}

/// 日期's month calendar (weeks start on Monday): today ringed, the cell's day filled.
pub(crate) fn date_popup(_v: &TableView, t: &Table, rec: RecId, fid: FieldId, year: i64, month: u32, cx: &Context<TableView>) -> AnyElement {
    let cur = t.record(rec).and_then(|r| match r.get(fid) { Value::Date(d) => Some(*d), _ => None });
    let cur_day = cur.map(|d| { let (y, m, d, ..) = field::date_parts(d); (y, m, d) });
    let time = cur.map(|d| { let (.., h, mi) = field::date_parts(d); (h, mi) }).unwrap_or((0, 0));
    let (ty, tm, td, ..) = field::date_parts(field::today());
    let nav = |id: &'static str, glyph: &'static str, by: i32| mini_btn(id, glyph)
        .on_click(cx.listener(move |v, _, _, cx| {
            if let Some((_, Popup::Date { year, month, .. })) = v.popup.as_mut() {
                let m = *month as i32 - 1 + by;
                *year += m.div_euclid(12) as i64;
                *month = m.rem_euclid(12) as u32 + 1;
            }
            cx.notify();
        }));
    let set = move |v: &mut TableView, value: Value, window: &mut Window, cx: &mut Context<TableView>| {
        let tid = v.table;
        v.edit(cx, |d| d.set_cells(tid, vec![(rec, fid, value)]));
        v.close_popup(window, cx);
    };
    let first = crate::time::weekday(year, month, 1) as i64;
    let days = crate::time::days_in_month(year, month) as i64;
    let mut grid = div().px(px(12.)).flex().flex_col().gap(px(2.));
    grid = grid.child(div().flex().children(["一", "二", "三", "四", "五", "六", "日"].into_iter().map(|w| div().size(px(32.)).flex().items_center().justify_center().text_size(px(12.)).text_color(Theme::text_placeholder()).child(w))));
    for week in 0..((first + days + 6) / 7) {
        let mut row = div().flex();
        for wd in 0..7 {
            let d = week * 7 + wd - first + 1;
            if d < 1 || d > days { row = row.child(div().size(px(32.))); continue; }
            let d = d as u32;
            let (today, on) = ((year, month, d) == (ty, tm, td), cur_day == Some((year, month, d)));
            row = row.child(div().id(("tb-day", d as usize)).size(px(32.)).flex().items_center().justify_center().cursor_pointer()
                .child(div().size(px(28.)).rounded_full().flex().items_center().justify_center()
                    .when(on, |x| x.bg(crate::design::color::primary()).text_color(crate::design::color::primary_foreground()))
                    .when(today && !on, |x| x.border_1().border_color(crate::design::color::foreground()).text_color(crate::design::color::foreground()))
                    .when(!on, |x| x.hover(|s| s.bg(Theme::fill_hover())))
                    .child(format!("{d}")))
                .on_click(cx.listener(move |v, _, window, cx| set(v, Value::Date(field::date_ms(year, month, d, time.0, time.1)), window, cx))));
        }
        grid = grid.child(row);
    }
    close_on_out(crate::design::Popover::new("tb-date").w(px(248.)).py(px(10.)), cx)
        .child(div().px(px(12.)).pb(px(6.)).flex().items_center()
            .child(div().flex_1().text_size(px(14.)).child(format!("{year}年{month}月")))
            .child(nav("tb-date-prev", "chevron-left", -1)).child(nav("tb-date-next", "chevron-right", 1)))
        .child(grid)
        .child(menu_rule())
        .child(div().px(px(12.)).flex().items_center().justify_between()
            .child(crate::design::Button::ghost("tb-date-today").sized(crate::design::ButtonSize::Xs).label(crate::settings::label("今天")).on_click(cx.listener(move |v, _, window, cx| set(v, Value::Date(field::today()), window, cx))))
            .child(crate::design::Button::ghost("tb-date-clear").sized(crate::design::ButtonSize::Xs).label(crate::settings::label("清除")).on_click(cx.listener(move |v, _, window, cx| set(v, Value::Empty, window, cx)))))
        .into_any_element()
}

/// 关联's record list: the other table's records by title, ticked when linked.
pub(crate) fn links_popup(v: &TableView, base: &Base, t: &Table, rec: RecId, fid: FieldId, cx: &Context<TableView>) -> AnyElement {
    let (Some(f), Some(r)) = (t.field(fid), t.record(rec)) else { return div().into_any_element() };
    let Some(other) = f.opts.link_table.and_then(|id| base.table(id)) else { return div().into_any_element() };
    let linked: Vec<RecId> = match r.get(fid) { Value::Links(l) => l.clone(), _ => vec![] };
    let query = v.aux.read(cx).text.trim().to_lowercase();
    let single = f.opts.single;
    let mut list = crate::design::scroll::make_room(div().id("tb-links-list").max_h(px(320.)).overflow_y_scroll().track_scroll(&crate::design::scroll::handle("tb-links-list")).flex().flex_col(), "tb-links-list", crate::design::menu::PAD);
    let mut shown = 0;
    for o in &other.records {
        let title = other.title(o);
        if !query.is_empty() && !title.to_lowercase().contains(&query) { continue; }
        // a table can be long: the search narrows it
        shown += 1;
        if shown > 200 { break; }
        let (oid, on) = (o.id, linked.contains(&o.id));
        list = list.child(list_row(("tb-link", oid as usize))
            .child(checkbox(on))
            .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().when(title.is_empty(), |d| d.text_color(Theme::text_placeholder()))
                .child(ui_text(if title.is_empty() { crate::settings::label("未命名记录") } else { title })))
            .on_click(cx.listener(move |v, _, window, cx| {
                let tid = v.table;
                v.edit(cx, |d| {
                    let Some(r) = d.base.table(tid).and_then(|t| t.record(rec)) else { return };
                    let mut l = match r.get(fid) { Value::Links(l) => l.clone(), _ => vec![] };
                    if let Some(i) = l.iter().position(|x| *x == oid) { l.remove(i); } else if single { l = vec![oid]; } else { l.push(oid); }
                    d.set_cells(tid, vec![(rec, fid, Value::Links(l))]);
                });
                if single { v.close_popup(window, cx); }
            })));
    }
    if shown == 0 { list = list.child(div().px(px(12.)).py(px(10.)).text_color(Theme::text_placeholder()).child(crate::settings::label("没有记录"))); }
    close_on_out(crate::design::Popover::new("tb-links").w(px(320.)).py(px(6.)), cx)
        .child(div().px(px(12.)).pb(px(6.)).text_size(px(12.)).text_color(Theme::text_caption()).child(format!("{}「{}」", crate::settings::label("关联到"), other.name)))
        .child(div().px(px(8.)).pb(px(6.)).child(input_box("tb-link-search").child(icon("search", 14., Theme::text_placeholder())).gap(px(6.)).child(div().flex_1().child(v.aux.clone()))))
        .child(list.child(crate::design::scroll::bar("tb-links-list", &crate::design::scroll::handle("tb-links-list"), 0.)))
        .into_any_element()
}

/// 附件's list: each file (click opens it), remove, and 添加附件.
pub(crate) fn files_popup(_v: &TableView, t: &Table, rec: RecId, fid: FieldId, cx: &Context<TableView>) -> AnyElement {
    let Some(r) = t.record(rec) else { return div().into_any_element() };
    let files: Vec<Attachment> = match r.get(fid) { Value::Files(f) => f.clone(), _ => vec![] };
    let mut list = div().flex().flex_col();
    for (i, a) in files.iter().enumerate() {
        let path = a.path.clone();
        list = list.child(div().id(("tb-file", i)).group("tb-file").h(px(34.)).mx(px(4.)).px(px(8.)).flex().items_center().gap(px(8.)).rounded(px(crate::design::geo().menu_row_r)).cursor_pointer().hover(|s| s.bg(Theme::fill_hover()))
            .child(icon("paperclip", 14., Theme::icon()))
            .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui_text(a.name.clone())))
            .child(div().id(("tb-file-del", i)).size(px(22.)).rounded(px(4.)).flex().items_center().justify_center().invisible().group_hover("tb-file", |s| s.visible())
                .hover(|s| s.bg(Theme::fill_hover_strong())).child(icon("x", 13., Theme::icon()))
                .on_mouse_down(MouseButton::Left, cx.listener(move |v, _, _, cx| {
                    cx.stop_propagation();
                    let tid = v.table;
                    v.edit(cx, |d| {
                        let Some(r) = d.base.table(tid).and_then(|t| t.record(rec)) else { return };
                        let mut fs = match r.get(fid) { Value::Files(f) => f.clone(), _ => vec![] };
                        if i < fs.len() { fs.remove(i); }
                        d.set_cells(tid, vec![(rec, fid, Value::Files(fs))]);
                    });
                })))
            .on_click(cx.listener(move |_, _, _, cx| {
                let abs = crate::store::Store::active_dir().join(&path);
                cx.open_url(&format!("file://{}", abs.display()));
            })));
    }
    close_on_out(crate::design::Popover::new("tb-files").w(px(300.)).py(px(6.)), cx)
        .when(files.is_empty(), |d| d.child(div().px(px(12.)).py(px(8.)).text_color(Theme::text_placeholder()).child(crate::settings::label("还没有附件"))))
        .child(list)
        .child(menu_rule())
        .child(menu_item("tb-file-add", "plus", crate::settings::label("添加附件"), false).on_click(cx.listener(move |v, _, window, cx| add_files(v, rec, fid, window, cx))))
        .into_any_element()
}

/// Pick files and copy them into the library's assets, then append them to the cell.
fn add_files(_v: &mut TableView, rec: RecId, fid: FieldId, window: &mut Window, cx: &mut Context<TableView>) {
    let picked = crate::devtools::take_pick();
    let prompt = picked.is_none().then(|| cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some(crate::settings::text("添加").into()), extensions: None }));
    cx.spawn_in(window, async move |this, cx| {
        let paths = match prompt { Some(p) => p.await.ok().and_then(Result::ok).flatten(), None => picked };
        let Some(paths) = paths else { return };
        let _ = this.update(cx, |v, cx| {
            let root = crate::store::Store::active_dir();
            let mut added = vec![];
            for p in paths {
                let Some(name) = p.file_name().map(|n| n.to_string_lossy().to_string()) else { continue };
                let dir = crate::store::Store::assets_dir().join("files").join(uuid::Uuid::new_v4().to_string());
                if fdl_core::vfs::create_dir_all(&dir).is_err() || fdl_core::vfs::copy(&p, dir.join(&name)).is_err() { continue; }
                let rel = dir.join(&name).strip_prefix(&root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| dir.join(&name).to_string_lossy().to_string());
                added.push(Attachment { path: rel, name });
            }
            if added.is_empty() { return; }
            let tid = v.table;
            v.edit(cx, |d| {
                let Some(r) = d.base.table(tid).and_then(|t| t.record(rec)) else { return };
                let mut fs = match r.get(fid) { Value::Files(f) => f.clone(), _ => vec![] };
                fs.extend(added);
                d.set_cells(tid, vec![(rec, fid, Value::Files(fs))]);
            });
        });
    }).detach();
}
