//! 记录详情: one record's fields in a panel beside the view, after 飞书's.
//! ∧ ∨ step through the view's records; the title and every field are listed
//! as「icon + name | value」(the view's hidden fields folded under a line).
//! Text-like values are typed in place, in the panel's one input; options,
//! dates, links and files open the cell editors (`editors`); the checkbox and
//! the stars take clicks directly.

use super::style::*;
use super::*;
use crate::ui::{ui_text};

/// The「icon + name」column.
const LABEL_W: f32 = 160.;

fn label(s: &str) -> String { crate::settings::label(s) }

#[derive(Default)]
pub(crate) struct RecordUi {
    /// The input the text-like values (and the title) are typed in, made when the panel first opens.
    input: Option<Entity<TextField>>,
    /// The value being typed.
    editing: Option<CellId>,
    /// ⋯'s menu, where it hangs.
    menu: Option<Point<Pixels>>,
    /// The view's hidden fields are listed too.
    show_hidden: bool,
    _subs: Vec<Subscription>,
}

pub(crate) fn open(v: &mut TableView, rec: RecId, window: &mut Window, cx: &mut Context<TableView>) {
    v.commit_edit(true, window, cx);
    commit(v, true, window, cx);
    if v.record.input.is_none() {
        let input = cx.new(|cx| { let mut t = TextField::new("", cx); t.size = text_sm(); t.wrap = true; t });
        v.record._subs = vec![
            cx.subscribe_in(&input, window, |v, _, ev: &TextFieldEvent, window, cx| match ev {
                TextFieldEvent::Submit => commit(v, true, window, cx),
                TextFieldEvent::Cancel => commit(v, false, window, cx),
                _ => cx.notify(),
            }),
            cx.on_focus_out(&input.focus_handle(cx), window, |v, _, window, cx| commit(v, true, window, cx)),
        ];
        v.record.input = Some(input);
    }
    v.record.menu = None;
    v.expanded = Some(rec);
    cx.notify();
}

fn close(v: &mut TableView, window: &mut Window, cx: &mut Context<TableView>) {
    commit(v, true, window, cx);
    v.record.menu = None;
    v.expanded = None;
    cx.notify();
}

/// Type in a value: the input takes the cell's text. 文本 takes line breaks; the title and the rest end on Enter.
fn begin(v: &mut TableView, cell: CellId, window: &mut Window, cx: &mut Context<TableView>) {
    commit(v, true, window, cx);
    let Some(input) = v.record.input.clone() else { return };
    let got = {
        let st = v.state.read(cx);
        st.doc.base.table(v.table).and_then(|t| Some((t, t.field(cell.1)?, t.record(cell.0)?)))
            .map(|(t, f, r)| (editors::edit_text(f, r.get(f.id)), t.primary().is_some_and(|p| p.id == f.id), f.ty == FieldType::Text))
    };
    let Some((text, title, long)) = got else { return };
    input.update(cx, |t, cx| {
        t.multiline = long && !title;
        (t.size, t.line_height) = if title { (20., 28.) } else { (text_sm(), 20.) };
        t.set_text(&text, cx);
    });
    window.focus(&input.focus_handle(cx), cx);
    v.record.editing = Some(cell);
    cx.notify();
}

/// Done typing: the text becomes the cell's value (when it changed), or is dropped.
fn commit(v: &mut TableView, save: bool, window: &mut Window, cx: &mut Context<TableView>) {
    let Some((rec, fid)) = v.record.editing.take() else { return };
    let Some(input) = v.record.input.clone() else { return };
    if save {
        let (text, tid) = (input.read(cx).text.clone(), v.table);
        v.edit(cx, |d| {
            let Some((new, old)) = d.base.table(tid).and_then(|t| Some((t.field(fid)?, t.record(rec)?))).map(|(f, r)| (field::parse(f, &text).normal(), r.get(fid).clone().normal())) else { return };
            if new != old { d.set_cells(tid, vec![(rec, fid, new)]); }
        });
    }
    if input.read(cx).is_focused(window) { window.focus(&v.focus, cx); }
    cx.notify();
}

/// A value was clicked at `at`: edit it the way its type is edited.
fn activate(v: &mut TableView, cell: CellId, at: Point<Pixels>, window: &mut Window, cx: &mut Context<TableView>) {
    commit(v, true, window, cx);
    let Some(ty) = v.state.read(cx).doc.base.table(v.table).and_then(|t| t.field(cell.1)).map(|f| f.ty) else { return };
    if ty.computed() { return; }
    let under = point(at.x - px(16.), at.y + px(16.));
    match ty {
        FieldType::Checkbox => v.toggle_check(cell, cx),
        FieldType::SingleSelect | FieldType::MultiSelect | FieldType::Person => editors::open_select(v, cell, under, window, cx),
        FieldType::Date => editors::open_date(v, cell, under, cx),
        FieldType::Link => editors::open_links(v, cell, under, window, cx),
        FieldType::Attachment => v.open_popup(under, Popup::Files { rec: cell.0, field: cell.1 }, cx),
        // the stars take their own clicks
        FieldType::Rating => {}
        _ => begin(v, cell, window, cx),
    }
}

/// What an empty value says in its place.
fn placeholder(ty: FieldType) -> &'static str {
    match ty {
        FieldType::SingleSelect | FieldType::MultiSelect => "请选择选项",
        FieldType::Person => "请选择人员",
        FieldType::Date => "请选择日期",
        FieldType::Link => "请选择记录",
        FieldType::Attachment => "添加附件",
        _ if ty.computed() => "空",
        _ => "请输入",
    }
}

pub(crate) fn render(v: &TableView, base: &Base, t: &Table, view: &View, r: &Record, window: &mut Window, cx: &Context<TableView>) -> AnyElement {
    let rid = r.id;
    // a table in a page opens the record as a dialog over the window (飞书 docx), not beside the table
    let modal = !v.full;
    let label_w = if modal { 240. } else { LABEL_W };
    let ui = &v.record;
    // the neighbours in the view's order (a record the filter leaves out has none)
    let place = v.order.get(&rid).copied();
    let id_at = |p: Option<usize>| p.and_then(|p| v.result.records.get(p)).and_then(|i| t.records.get(*i)).map(|r| r.id);
    let (prev, next) = (id_at(place.and_then(|p| p.checked_sub(1))), id_at(place.map(|p| p + 1)));
    let button = |id: &'static str, glyph: Svg, on: bool| div().id(id).flex_none().size(px(28.)).rounded(px(crate::design::geo().tool_r)).flex().items_center().justify_center()
        .when(on, |d| d.cursor_pointer().hover(|s| s.bg(Theme::fill_hover()))).child(glyph);
    let nav = |id: &'static str, up: bool, to: Option<RecId>| {
        let glyph = icon("chevron-down", 16., if to.is_some() { Theme::icon() } else { Theme::text_disabled() });
        button(id, if up { glyph.with_transformation(Transformation::rotate(percentage(0.5))) } else { glyph }, to.is_some())
            .on_click(cx.listener(move |v, _, window, cx| if let Some(to) = to { open(v, to, window, cx) }))
    };
    let head = div().flex_none().h(px(if modal { 64. } else { 48. })).px(px(if modal { 24. } else { 12. })).flex().items_center().gap(px(4.)).border_b_1().border_color(Theme::line_divider())
        .child(nav("tb-rec-prev", true, prev))
        .child(nav("tb-rec-next", false, next))
        .when_some(place, |d, p| d.child(div().pl(px(4.)).text_size(px(12.)).text_color(Theme::text_placeholder()).child(format!("{}/{}", p + 1, v.result.records.len()))))
        .child(div().flex_1())
        .child(button("tb-rec-more", icon("ellipsis", 16., Theme::icon()), true).on_click(cx.listener(|v, ev: &ClickEvent, window, cx| {
            commit(v, true, window, cx);
            let p = ev.position();
            v.record.menu = Some(point(p.x - px(150.), p.y + px(18.)));
            cx.notify();
        })))
        .child(button("tb-rec-close", icon("x", 16., Theme::icon()), true).on_click(cx.listener(|v, _, window, cx| close(v, window, cx))));

    // a value's box: a hairline while it is typed in, a tint under the pointer when a click would edit it
    let value_box = |id: ElementId, editing: bool, clickable: bool| div().id(id).flex_1().min_w_0().min_h(px(crate::design::geo().field_h)).px(px(8.)).py(px(6.)).flex().items_center().rounded(px(crate::design::geo().tool_r)).border_1()
        .border_color(if editing { Theme::primary() } else { transparent_black() })
        .when(clickable && !editing, |d| d.cursor_pointer().hover(|s| s.bg(Theme::fill_hover())));
    let input = |d: Stateful<Div>| d.children(ui.input.clone().map(|i| div().flex_1().min_w_0().child(i)));
    let empty = |text: &str| div().text_color(Theme::text_placeholder()).child(label(text));

    let primary = t.primary();
    let title = t.title(r);
    let title_cell = primary.map(|f| (rid, f.id));
    let title_editing = title_cell.is_some() && ui.editing == title_cell;
    let title_el = div().px(px(16.)).pt(px(if modal { 24. } else { 16. })).pb(px(if modal { 16. } else { 8. })).flex().text_size(px(if modal { 24. } else { 20. })).line_height(px(if modal { 32. } else { 28. })).font_weight(if modal { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM })
        .child(value_box("tb-rec-title".into(), title_editing, primary.is_some_and(|f| !f.ty.computed()))
            .map(|d| if title_editing { input(d) } else if title.is_empty() { d.child(empty("未命名记录")) } else { d.child(div().w_full().child(SharedString::from(title))) })
            .when_some(title_cell.filter(|_| !title_editing), |d, cell| d.on_click(cx.listener(move |v, ev: &ClickEvent, window, cx| activate(v, cell, ev.position(), window, cx)))));

    let row = |f: &Field| {
        let (fid, val) = (f.id, r.get(f.id));
        let cell: CellId = (rid, fid);
        let editing = ui.editing == Some(cell);
        let content = match f.ty {
            _ if editing => None,
            FieldType::Checkbox => Some(checkbox(matches!(val, Value::Bool(true))).into_any_element()),
            FieldType::Rating => {
                let n = val.as_number().unwrap_or(0.);
                Some(div().flex().items_center().gap(px(2.)).children((1..=f.stars()).map(|s| div().id(("tb-rec-star", (fid as usize) << 4 | s as usize)).cursor_pointer()
                    .child(icon("star", 16., if (s as f64) <= n { star() } else { crate::design::color::border() }))
                    .on_click(cx.listener(move |v, _, window, cx| {
                        commit(v, true, window, cx);
                        let tid = v.table;
                        // the same star again clears it
                        v.edit(cx, |d| d.set_cells(tid, vec![(rid, fid, if s as f64 == n { Value::Empty } else { Value::Number(s as f64) })]));
                    })))).into_any_element())
            }
            _ if val.is_empty() => Some(empty(placeholder(f.ty)).into_any_element()),
            // what the table works out is read, not edited: a shade lighter
            _ if f.ty.computed() => Some(div().w_full().opacity(0.7).child(value(base, f, val)).into_any_element()),
            FieldType::Progress => Some(div().w_full().max_w(px(260.)).child(value(base, f, val)).into_any_element()),
            _ => Some(value(base, f, val)),
        };
        div().w_full().px(px(16.)).flex().items_start().gap(px(8.))
            .child(div().flex_none().w(px(label_w)).h(px(36.)).pl(px(8.)).flex().items_center().gap(px(6.))
                .child(icon(field_glyph(f.ty), 15., Theme::icon()))
                .child(div().flex_1().min_w_0().text_color(Theme::text_caption()).overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui_text(f.name.clone()))))
            .child(value_box(("tb-rec-val", fid).into(), editing, !f.ty.computed() && f.ty != FieldType::Rating)
                .map(|d| match content { Some(c) => d.child(c), None => input(d) })
                .when(!editing && !f.ty.computed(), |d| d.on_click(cx.listener(move |v, ev: &ClickEvent, window, cx| activate(v, cell, ev.position(), window, cx)))))
    };

    let fields = t.ordered_fields(view);
    let (shown, hidden): (Vec<&Field>, Vec<&Field>) = fields.into_iter().skip(1).partition(|f| !view.opts.hidden.contains(&f.id));
    let mut list = div().w_full().flex().flex_col().gap(px(2.)).child(title_el).children(shown.into_iter().map(row));
    if !hidden.is_empty() {
        list = list.child(div().px(px(16.)).pt(px(8.)).flex().child(div().id("tb-rec-hidden").h(px(28.)).px(px(8.)).flex().items_center().gap(px(4.)).rounded(px(crate::design::geo().tool_r)).cursor_pointer()
            .text_size(px(12.)).text_color(Theme::text_caption()).hover(|s| s.bg(Theme::fill_hover()))
            .child(icon(if ui.show_hidden { "triangle-down" } else { "triangle-right" }, 10., Theme::text_caption()))
            .child(format!("{} {}", hidden.len(), label("个隐藏字段")))
            .on_click(cx.listener(|v, _, _, cx| { v.record.show_hidden = !v.record.show_hidden; cx.notify(); }))));
        if ui.show_hidden { list = list.children(hidden.into_iter().map(row)); }
    }
    list = list
        .child(div().px(px(16.)).pt(px(8.)).flex().child(div().id("tb-rec-add").h(px(32.)).px(px(8.)).flex().items_center().gap(px(6.)).rounded(px(crate::design::geo().tool_r)).cursor_pointer()
            .text_color(Theme::text_caption()).hover(|s| s.bg(Theme::fill_hover()))
            .child(icon("plus", 14., Theme::icon())).child(label("新增字段"))
            .on_click(cx.listener(|v, ev: &ClickEvent, window, cx| {
                commit(v, true, window, cx);
                let p = ev.position();
                menus::add_field(v, None, point(p.x - px(20.), p.y + px(20.)), window, cx);
            }))))
        .child(div().px(px(24.)).pt(px(16.)).pb(px(24.)).text_size(px(12.)).line_height(px(18.)).text_color(Theme::text_placeholder())
            .child(format!("{} {}", label("创建于"), field::format_date(r.created, DateFmt::Dash, true)))
            .child(format!("{} {}", label("最后更新于"), field::format_date(r.modified, DateFmt::Dash, true))));

    let menu = ui.menu.map(|pos| deferred(anchored().position(pos).snap_to_window_with_margin(px(8.)).child(
        crate::design::Popover::new("tb-rec-menu").occlude().w(px(180.)).py(px(4.)).on_mouse_down_out(cx.listener(|v, _, _, cx| { v.record.menu = None; cx.notify(); }))
            .child(menu_item("tb-rec-dup", "copy", label("创建副本"), false).on_click(cx.listener(move |v, _, _, cx| {
                v.record.menu = None;
                let tid = v.table;
                v.edit(cx, |d| d.duplicate_records(tid, &[rid]));
            })))
            .child(menu_item("tb-rec-del", "bin", label("删除记录"), true).on_click(cx.listener(move |v, _, window, cx| {
                close(v, window, cx);
                let tid = v.table;
                v.edit(cx, |d| d.remove_records(tid, &[rid]));
                v.rows_sel.remove(&rid);
                if v.sel.is_some_and(|s| s.anchor.0 == rid || s.head.0 == rid) { v.sel = None; }
            }))))).with_priority(5));

    if modal {
        // 飞书's record dialog: the page dimmed, the record centred (781×675 in a 1161×795 window, measured)
        let vp = window.viewport_size();
        let (w, h) = ((f32::from(vp.width) * 0.86).min(1200.), (f32::from(vp.height) * 0.86).min(900.));
        return deferred(anchored().position(point(px(0.), px(0.))).child(
            crate::design::scrim().id("tb-record-scrim").occlude().relative().w(vp.width).h(vp.height)
                .on_mouse_down(MouseButton::Left, cx.listener(|v, _, window, cx| close(v, window, cx)))
                .child(crate::design::Dialog::new("tb-record").w(px(w)).h(px(h)).p_0().gap_0().overflow_hidden().text_size(px(text_sm()))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(head)
                    .child(div().id("tb-rec-body").flex_1().min_h_0().overflow_y_scroll().px(px(40.)).pb(px(24.)).child(list))
                    .children(menu)))).with_priority(4).into_any_element();
    }
    div().id("tb-record").flex_none().w(px(RECORD_W)).h_full().flex().flex_col().bg(Theme::bg_body()).border_l_1().border_color(line())
        .child(head)
        .child(div().id("tb-rec-body").flex_1().min_h_0().overflow_y_scroll().child(list))
        .children(menu)
        .into_any_element()
}

/// A value in 记录详情: `cell::value_view`, flush left.
fn value(base: &Base, f: &Field, v: &Value) -> AnyElement {
    match v {
        Value::Bool(b) => checkbox(*b).into_any_element(),
        Value::Number(n) if !matches!(f.ty, FieldType::Rating | FieldType::Progress) => div().w_full().line_height(px(20.)).child(ui_text(field::format_number(*n, f))).into_any_element(),
        _ => cell::value_view(base, f, v, true),
    }
}
