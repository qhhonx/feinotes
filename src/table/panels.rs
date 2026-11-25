//! The toolbar's panels, after 飞书's: 字段配置, 筛选, 分组, 排序, 行高, 填色.
//! Each is a popup under its button; a change takes effect at once (one
//! `edit_view`, one undo step), there is no 确定.
//!
//! The pickers inside a panel (field, operator, option, date) are the view's
//! `Dropdown`. A condition's typed value is a `TextField` kept in the `Panel`,
//! one per condition in their order: `sync_inputs` after every change to the
//! conditions keeps the two lists in step.
//!
//! Where this parts from 飞书: 字段配置 shows a switch for 显示 / 隐藏 (the icon
//! set has one eye) and has 全部显示 / 全部隐藏; 分组 and 排序 levels are not
//! dragged (remove and add again); 填色's colours are the ten tag colours, in
//! a row under the rule; a 复选框 condition's value is a list of two.

use super::style::*;
use super::*;
use crate::ui::{ui_text};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind { Fields, Filter, Group, Sort, RowHeight, Color }

pub(crate) struct Panel {
    pub kind: Kind,
    /// 字段配置's search box.
    search: Option<(Entity<TextField>, Subscription)>,
    /// 筛选 / 填色: a value box per condition (per rule), in their order.
    inputs: Vec<(Entity<TextField>, Subscription)>,
    /// 填色: the rule whose colours are laid out under it.
    palette: Option<usize>,
}

/// A 字段配置 row being carried to another place: GPUI's drag payload, drawn as the row lifted.
#[derive(Clone)]
struct FieldDrag { id: FieldId, at: usize, name: String, glyph: &'static str }

impl Render for FieldDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().h(px(34.)).w(px(236.)).px(px(8.)).flex().items_center().gap(px(6.)).rounded(px(crate::design::geo().tool_r))
            .bg(crate::theme::raised(Theme::bg_body())).shadow(crate::theme::shadow_menu()).opacity(0.96)
            .font_family(crate::theme::ui_family()).text_size(px(text_sm())).text_color(Theme::text())
            .child(icon(self.glyph, 15., Theme::icon()))
            .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui_text(self.name.clone())))
    }
}

fn label(s: &str) -> String { crate::settings::label(s) }

pub(crate) fn open(v: &mut TableView, kind: Kind, pos: Point<Pixels>, window: &mut Window, cx: &mut Context<TableView>) {
    let search = (kind == Kind::Fields).then(|| {
        let input = cx.new(|cx| { let mut t = TextField::new("搜索", cx); t.size = text_sm(); t });
        let sub = cx.subscribe_in(&input, window, |v, _, ev: &TextFieldEvent, window, cx| match ev {
            TextFieldEvent::Cancel => v.close_popup(window, cx),
            _ => cx.notify(),
        });
        window.focus(&input.focus_handle(cx), cx);
        (input, sub)
    });
    v.open_popup(pos, Popup::Panel(Panel { kind, search, inputs: vec![], palette: None }), cx);
    sync_inputs(v, window, cx);
}

pub(crate) fn render(_v: &TableView, _base: &Base, t: &Table, view: &View, panel: &Panel, cx: &Context<TableView>) -> AnyElement {
    match panel.kind {
        Kind::Fields => field_list(t, view, panel, cx),
        Kind::Filter => filter_panel(t, view, panel, cx),
        Kind::Group | Kind::Sort => order_panel(t, view, panel.kind, cx),
        Kind::RowHeight => height_panel(view, cx),
        Kind::Color => color_panel(t, view, panel, cx),
    }
}

// ---- state ----

/// The table and view shown, read in a listener.
fn peek<R>(v: &TableView, cx: &App, f: impl FnOnce(&Table, &View) -> R) -> Option<R> {
    let t = v.state.read(cx).doc.base.table(v.table)?;
    Some(f(t, t.view(v.view)?))
}

fn panel_mut(v: &mut TableView) -> Option<&mut Panel> { match v.popup.as_mut() { Some((_, Popup::Panel(p))) => Some(p), _ => None } }

/// Condition `i` of the panel: 筛选's own, or 填色's rule `i` (a rule holds one).
fn cond(view: &View, kind: Kind, i: usize) -> Option<&Cond> {
    if kind == Kind::Color { view.opts.colors.get(i)?.filter.conds.first() } else { view.opts.filter.conds.get(i) }
}

fn cond_mut(view: &mut View, kind: Kind, i: usize) -> Option<&mut Cond> {
    if kind == Kind::Color { view.opts.colors.get_mut(i)?.filter.conds.first_mut() } else { view.opts.filter.conds.get_mut(i) }
}

fn set_cond(v: &mut TableView, kind: Kind, i: usize, window: &mut Window, cx: &mut Context<TableView>, f: impl FnOnce(&mut Cond)) {
    v.edit_view(cx, |view| if let Some(c) = cond_mut(view, kind, i) { f(c) });
    sync_inputs(v, window, cx);
}

/// A new condition: the first field, 等于, nothing (which filters nothing yet).
fn new_cond(v: &TableView, cx: &App) -> Option<Cond> {
    peek(v, cx, |t, _| t.fields.first().map(|f| Cond { field: f.id, op: query::ops_for(f.ty)[0], value: String::new(), date: DateRef::Exact })).flatten()
}

/// Another field for a condition: its operator stays when the type has it, its value starts over.
fn set_cond_field(v: &mut TableView, kind: Kind, i: usize, fid: FieldId, window: &mut Window, cx: &mut Context<TableView>) {
    let Some(ty) = peek(v, cx, |t, _| t.field(fid).map(|f| f.ty)).flatten() else { return };
    set_cond(v, kind, i, window, cx, |c| {
        if c.field == fid { return; }
        let ops = query::ops_for(ty);
        // 复选框 always says something: 未勾选 until ticked, as 飞书's does
        *c = Cond { field: fid, op: if ops.contains(&c.op) { c.op } else { ops[0] }, value: if ty == FieldType::Checkbox { "false".into() } else { String::new() }, date: DateRef::Exact };
    });
}

fn remove_cond(v: &mut TableView, kind: Kind, i: usize, window: &mut Window, cx: &mut Context<TableView>) {
    v.edit_view(cx, |view| {
        if kind == Kind::Color { if i < view.opts.colors.len() { view.opts.colors.remove(i); } }
        else if i < view.opts.filter.conds.len() { view.opts.filter.conds.remove(i); }
    });
    if let Some(p) = panel_mut(v) {
        if i < p.inputs.len() { drop(p.inputs.remove(i)); }
        p.palette = None;
    }
    // the box that went may have held the focus
    window.focus(&v.focus, cx);
    sync_inputs(v, window, cx);
}

/// One value box per condition, each holding its condition's value.
fn sync_inputs(v: &mut TableView, window: &mut Window, cx: &mut Context<TableView>) {
    let Some(kind) = panel_mut(v).map(|p| p.kind) else { return };
    if !matches!(kind, Kind::Filter | Kind::Color) { return; }
    let want: Vec<(String, bool)> = peek(v, cx, |t, view| {
        let n = if kind == Kind::Color { view.opts.colors.len() } else { view.opts.filter.conds.len() };
        (0..n).map(|i| { let c = cond(view, kind, i); (c.map(|c| c.value.clone()).unwrap_or_default(), c.and_then(|c| t.field(c.field)).is_some_and(|f| f.ty.is_date())) }).collect()
    }).unwrap_or_default();
    let mut inputs = panel_mut(v).map(|p| std::mem::take(&mut p.inputs)).unwrap_or_default();
    inputs.truncate(want.len());
    while inputs.len() < want.len() {
        let input = cx.new(|cx| { let mut t = TextField::new("", cx); t.size = text_sm(); t });
        let sub = cx.subscribe_in(&input, window, input_event);
        inputs.push((input, sub));
    }
    for ((input, _), (text, date)) in inputs.iter().zip(&want) {
        input.update(cx, |t, cx| { t.set_placeholder(if *date { "yyyy/mm/dd" } else { "请输入" }, cx); if t.text != *text { t.set_text(text, cx); } });
    }
    if let Some(p) = panel_mut(v) { p.inputs = inputs; }
}

/// Typing in a value box is the condition's value, as typed.
fn input_event(v: &mut TableView, input: &Entity<TextField>, ev: &TextFieldEvent, window: &mut Window, cx: &mut Context<TableView>) {
    match ev {
        TextFieldEvent::Cancel => v.close_popup(window, cx),
        TextFieldEvent::Changed => {
            let Some((kind, i)) = panel_mut(v).and_then(|p| Some((p.kind, p.inputs.iter().position(|(e, _)| e == input)?))) else { return };
            let text = input.read(cx).text.clone();
            if peek(v, cx, |_, view| cond(view, kind, i).is_some_and(|c| c.value != text)) == Some(true) {
                v.edit_view(cx, |view| if let Some(c) = cond_mut(view, kind, i) { c.value = text; });
            }
        }
        _ => {}
    }
}

fn sorts(view: &View, kind: Kind) -> &Vec<SortBy> { if kind == Kind::Group { &view.opts.groups } else { &view.opts.sorts } }
fn sorts_mut(view: &mut View, kind: Kind) -> &mut Vec<SortBy> { if kind == Kind::Group { &mut view.opts.groups } else { &mut view.opts.sorts } }

/// A 字段配置 row dropped on another: it takes that row's place (the title stays first).
fn move_field(v: &mut TableView, src: FieldId, dst: FieldId, cx: &mut Context<TableView>) {
    let Some(mut ids) = peek(v, cx, |t, view| t.ordered_fields(view).iter().skip(1).map(|f| f.id).collect::<Vec<_>>()) else { return };
    let (Some(a), Some(b)) = (ids.iter().position(|f| *f == src), ids.iter().position(|f| *f == dst)) else { return };
    if a == b { return; }
    let id = ids.remove(a);
    ids.insert(b, id);
    v.edit_view(cx, |view| view.opts.order = ids);
}

// ---- pickers ----

/// Where a picker opens for a click on its box.
fn below(ev: &ClickEvent) -> Point<Pixels> { let p = ev.position(); point(p.x - px(16.), p.y + px(18.)) }

/// The table's fields but `skip`, `cur` ticked.
fn pick_field(v: &mut TableView, pos: Point<Pixels>, cur: Option<FieldId>, skip: Vec<FieldId>, cx: &mut Context<TableView>, pick: impl Fn(&mut TableView, FieldId, &mut Window, &mut Context<TableView>) + 'static) {
    let Some(list) = peek(v, cx, |t, view| t.ordered_fields(view).into_iter().filter(|f| !skip.contains(&f.id)).map(|f| (f.id, f.name.clone(), f.ty)).collect::<Vec<_>>()) else { return };
    if list.is_empty() { return; }
    let items = list.iter().map(|(id, name, ty)| DropItem::new(name.clone()).glyph(field_glyph(*ty)).on(Some(*id) == cur)).collect();
    v.open_dropdown(pos, 212., items, move |v, k, window, cx| if let Some((id, ..)) = list.get(k) { pick(v, *id, window, cx) }, cx);
}

/// A select condition's options. 单选 等于 / 不等于 takes one and closes; the rest tick several and stay open.
fn pick_options(v: &mut TableView, kind: Kind, i: usize, pos: Point<Pixels>, cx: &mut Context<TableView>) {
    let Some((opts, chosen, single)) = peek(v, cx, |t, view| {
        let c = cond(view, kind, i)?;
        let f = t.field(c.field)?;
        Some((f.opts.options.iter().map(|o| (o.name.clone(), o.color)).collect::<Vec<_>>(), field::split_list(&c.value), f.ty == FieldType::SingleSelect && matches!(c.op, FilterOp::Is | FilterOp::IsNot)))
    }).flatten() else { return };
    if opts.is_empty() { return v.open_dropdown(pos, 216., vec![DropItem::new(label("没有选项"))], |_, _, _, _| {}, cx); }
    let items = opts.iter().map(|(name, color)| DropItem::new(name.clone()).tag(*color).on(chosen.contains(name))).collect();
    v.open_dropdown(pos, 216., items, move |v, k, window, cx| {
        let Some((name, _)) = opts.get(k) else { return };
        let mut now = chosen.clone();
        if single { now = if now == [name.clone()] { vec![] } else { vec![name.clone()] }; }
        else if let Some(at) = now.iter().position(|n| n == name) { now.remove(at); } else { now.push(name.clone()); }
        set_cond(v, kind, i, window, cx, |c| c.value = now.join(","));
        if !single { pick_options(v, kind, i, pos, cx); }
    }, cx);
}

// ---- parts ----

fn shell(id: &'static str, w: f32, cx: &Context<TableView>) -> crate::design::Popover {
    crate::design::Popover::new(id).occlude().w(px(w)).flex().flex_col().text_size(px(text_sm()))
        .on_mouse_down_out(cx.listener(|v, _, window, cx| if v.dropdown.is_none() { v.close_popup(window, cx) }))
}

fn title(s: &str) -> crate::design::Label { panel_title(label(s)) }

/// A field in a `pick_box`: its type's glyph and its name (a field since deleted: pick another).
fn field_body(f: Option<&Field>) -> Div {
    let row = div().flex().items_center().gap(px(6.));
    let Some(f) = f else { return row.text_color(Theme::text_placeholder()).child(label("选择字段")) };
    row.child(icon(field_glyph(f.ty), 14., Theme::icon()))
        .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(Theme::text()).child(ui_text(f.name.clone())))
}

fn x_btn(id: impl Into<ElementId>) -> crate::design::Button { close_btn(id) }

/// A text button (`+ 添加条件`, 全部折叠).
fn text_btn(id: impl Into<ElementId>, glyph: &str, text: &str) -> crate::design::Button { ghost_btn(id, glyph, label(text)) }

/// 飞书 spells the comparisons out.
fn op_label(op: FilterOp) -> String {
    label(match op { FilterOp::Gt => "大于", FilterOp::Ge => "大于或等于", FilterOp::Lt => "小于", FilterOp::Le => "小于或等于", op => op.label() })
}

/// 分组 / 排序's two directions, worded by what the field holds.
fn order_words(ty: Option<FieldType>) -> (&'static str, &'static str) {
    match ty {
        Some(t) if t.select() => ("选项顺序", "选项倒序"),
        Some(t) if t.is_date() || t.numeric() => ("0 → 9", "9 → 0"),
        Some(FieldType::Checkbox) => ("顺序", "倒序"),
        _ => ("A → Z", "Z → A"),
    }
}

/// A condition's controls: field, operator, value by the field's type. 筛选's lines and 填色's rules are both this.
fn cond_controls(t: &Table, kind: Kind, i: usize, c: &Cond, panel: &Panel, field_w: f32, cx: &Context<TableView>) -> Div {
    let f = t.field(c.field);
    let row = div().flex_1().min_w_0().flex().items_center().gap(px(8.))
        .child(pick_box(("tb-pc-field", i), field_body(f)).flex_none().w(px(field_w)).on_click(cx.listener(move |v, ev: &ClickEvent, _, cx| {
            let cur = peek(v, cx, |_, view| cond(view, kind, i).map(|c| c.field)).flatten();
            pick_field(v, below(ev), cur, vec![], cx, move |v, fid, window, cx| set_cond_field(v, kind, i, fid, window, cx));
        })));
    let Some(f) = f else { return row };
    let typed = |row: Div| row.child(input_box(("tb-pc-input", i)).flex_1().min_w_0().children(panel.inputs.get(i).map(|(e, _)| div().flex_1().min_w_0().child(e.clone()))));
    if f.ty == FieldType::Checkbox {
        let on = !field::parse(f, &c.value).is_empty();
        return row.child(select_box(("tb-pc-check", i), label(if on { "已勾选" } else { "未勾选" })).flex_1().min_w_0().on_click(cx.listener(move |v, ev: &ClickEvent, _, cx| {
            let items = vec![DropItem::new(label("已勾选")).on(on), DropItem::new(label("未勾选")).on(!on)];
            v.open_dropdown(below(ev), 140., items, move |v, k, window, cx| set_cond(v, kind, i, window, cx, |c| { c.op = FilterOp::Is; c.value = if k == 0 { "true" } else { "false" }.into(); }), cx);
        })));
    }
    let row = row.child(select_box(("tb-pc-op", i), op_label(c.op)).flex_none().w(px(108.)).on_click(cx.listener(move |v, ev: &ClickEvent, _, cx| {
        let Some((ops, cur)) = peek(v, cx, |t, view| { let c = cond(view, kind, i)?; Some((query::ops_for(t.field(c.field)?.ty), c.op)) }).flatten() else { return };
        let items = ops.iter().map(|o| DropItem::new(op_label(*o)).on(*o == cur)).collect();
        v.open_dropdown(below(ev), 140., items, move |v, k, window, cx| if let Some(op) = ops.get(k).copied() {
            set_cond(v, kind, i, window, cx, |c| {
                c.op = op;
                // 早于 / 晚于 take a day, not a span
                if matches!(op, FilterOp::Before | FilterOp::After) && !DateRef::ALL[..4].contains(&c.date) { c.date = DateRef::Exact; c.value.clear(); }
            });
        }, cx);
    })));
    if !c.op.needs_value() { return row; }
    if f.ty.select() {
        let names = field::split_list(&c.value);
        let mut body = div().flex().items_center().gap(px(4.));
        if names.is_empty() { body = body.text_color(Theme::text_placeholder()).child(label("请选择选项")); }
        for n in &names { body = body.child(tag(n, f.option_named(n).map_or(9, |o| o.color))); }
        return row.child(pick_box(("tb-pc-opts", i), body).flex_1().min_w_0().on_click(cx.listener(move |v, ev: &ClickEvent, _, cx| pick_options(v, kind, i, below(ev), cx))));
    }
    if f.ty.is_date() {
        let (cur, exact, near) = (c.date, c.date == DateRef::Exact, matches!(c.op, FilterOp::Before | FilterOp::After));
        let row = row.child(select_box(("tb-pc-date", i), label(cur.label())).map(|d| if exact { d.flex_none().w(px(104.)) } else { d.flex_1().min_w_0() }).on_click(cx.listener(move |v, ev: &ClickEvent, _, cx| {
            let refs: &'static [DateRef] = if near { &DateRef::ALL[..4] } else { DateRef::ALL };
            let items = refs.iter().map(|r| DropItem::new(label(r.label())).on(*r == cur)).collect();
            v.open_dropdown(below(ev), 160., items, move |v, k, window, cx| if let Some(r) = refs.get(k).copied() { set_cond(v, kind, i, window, cx, |c| c.date = r) }, cx);
        })));
        return if exact { typed(row) } else { row };
    }
    typed(row)
}

// ---- 字段配置 ----

fn field_list(t: &Table, view: &View, panel: &Panel, cx: &Context<TableView>) -> AnyElement {
    let query = panel.search.as_ref().map(|(e, _)| e.read(cx).text.trim().to_lowercase()).unwrap_or_default();
    let primary = t.fields.first().map(|f| f.id);
    let mut list = div().id("tb-pl-list").max_h(px(420.)).overflow_y_scroll().flex().flex_col();
    let mut any = false;
    for (at, f) in t.ordered_fields(view).into_iter().enumerate() {
        if !query.is_empty() && !f.name.to_lowercase().contains(&query) { continue; }
        any = true;
        let (fid, first, hidden) = (f.id, Some(f.id) == primary, view.opts.hidden.contains(&f.id));
        let row = div().id(("tb-pl", fid)).flex_none().h(px(34.)).mx(px(4.)).px(px(8.)).flex().items_center().gap(px(6.)).rounded(px(crate::design::geo().menu_row_r)).hover(|s| s.bg(Theme::fill_hover()))
            .when(!first, |d| d.child(icon("grip-vertical", 14., Theme::text_placeholder())))
            .child(icon(field_glyph(f.ty), 15., if hidden { Theme::text_disabled() } else { Theme::icon() }))
            .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(if hidden { Theme::text_disabled() } else { Theme::text() }).child(ui_text(f.name.clone())))
            // the title column stays, and stays first
            .when(first, |d| d.child(icon("ft-lock", 14., Theme::text_caption())))
            .child(div().flex_1());
        if first { list = list.child(row).when(query.is_empty(), |d| d.child(menu_rule())); continue; }
        list = list.child(row
            .child(div().id(("tb-pl-show", fid)).flex_none().h(px(24.)).px(px(4.)).flex().items_center().cursor_pointer().child(switch(!hidden, true))
                .on_click(cx.listener(move |v, _, _, cx| {
                    v.edit_view(cx, |view| if !view.opts.hidden.remove(&fid) { view.opts.hidden.insert(fid); });
                    v.col_sel.retain(|f| *f != fid);
                })))
            .on_drag(FieldDrag { id: fid, at, name: f.name.clone(), glyph: field_glyph(f.ty) }, |d, _, _, cx| cx.new(|_| d.clone()))
            // the line it would land on: under a row below where it came from, over one above
            .drag_over::<FieldDrag>(move |s, d, _, _| if d.id == fid { s } else if d.at < at { s.border_b_2().border_color(Theme::primary()) } else { s.border_t_2().border_color(Theme::primary()) })
            .on_drop(cx.listener(move |v, d: &FieldDrag, _, cx| move_field(v, d.id, fid, cx))));
    }
    if !any { list = list.child(div().px(px(12.)).py(px(10.)).text_color(Theme::text_placeholder()).child(label("无结果"))); }
    let every = |id: &'static str, text: &str, hide: bool| text_btn(id, "", text).on_click(cx.listener(move |v, _, _, cx| {
        let ids: Vec<FieldId> = if hide { peek(v, cx, |t, _| t.fields.iter().skip(1).map(|f| f.id).collect()).unwrap_or_default() } else { vec![] };
        v.edit_view(cx, |view| view.opts.hidden = ids.into_iter().collect());
        if hide { v.col_sel.clear(); }
    }));
    shell("tb-panel-fields", 280., cx).py(px(6.))
        .children(panel.search.as_ref().map(|(e, _)| div().flex_none().h(px(36.)).px(px(12.)).mb(px(4.)).flex().items_center().gap(px(6.)).border_b_1().border_color(Theme::line_divider())
            .child(icon("search", 14., Theme::text_placeholder())).child(div().flex_1().min_w_0().child(e.clone()))))
        .child(list)
        .child(menu_rule())
        .child(div().flex_none().px(px(6.)).flex().items_center().gap(px(4.)).child(every("tb-pl-all", "全部显示", false)).child(every("tb-pl-none", "全部隐藏", true)))
        .child(menu_rule())
        .child(menu_item("tb-pl-add", "plus", label("新增字段"), false).on_click(cx.listener(|v, _, window, cx| {
            let Some((pos, _)) = v.popup.take() else { return };
            let form = fields::FieldForm::add(v, None, window, cx);
            v.open_popup(pos, Popup::FieldForm(form), cx);
        })))
        .into_any_element()
}

// ---- 筛选 ----

/// A condition panel with nothing in it yet: only its title and an add button, so narrow.
const EMPTY_W: f32 = 280.;

fn filter_panel(t: &Table, view: &View, panel: &Panel, cx: &Context<TableView>) -> AnyElement {
    let f = &view.opts.filter;
    let mut p = shell("tb-panel-filter", if f.conds.is_empty() { EMPTY_W } else { 560. }, cx).p(px(crate::design::geo().popover_pad)).gap(px(crate::design::geo().panel_gap)).child(title("设置筛选条件"));
    if f.conds.len() >= 2 {
        let all = f.all;
        p = p.child(div().flex().items_center().gap(px(8.)).text_color(Theme::text_caption())
            .child(label("符合以下"))
            .child(select_box("tb-pf-all", label(if all { "所有" } else { "任一" })).flex_none().w(px(76.)).on_click(cx.listener(move |v, ev: &ClickEvent, _, cx| {
                let items = vec![DropItem::new(label("所有")).on(all), DropItem::new(label("任一")).on(!all)];
                v.open_dropdown(below(ev), 96., items, |v, k, _, cx| v.edit_view(cx, |view| view.opts.filter.all = k == 0), cx);
            })))
            .child(label("条件")));
    }
    for (i, c) in f.conds.iter().enumerate() {
        p = p.child(div().flex().items_center().gap(px(8.))
            .child(cond_controls(t, Kind::Filter, i, c, panel, 148., cx))
            .child(x_btn(("tb-pf-del", i)).on_click(cx.listener(move |v, _, window, cx| remove_cond(v, Kind::Filter, i, window, cx)))));
    }
    p.child(div().flex().child(text_btn("tb-pf-add", "plus", "添加条件").on_click(cx.listener(|v, _, window, cx| {
        let Some(c) = new_cond(v, cx) else { return };
        v.edit_view(cx, |view| view.opts.filter.conds.push(c));
        sync_inputs(v, window, cx);
        // the title is text: its value is typed next
        if let Some(input) = panel_mut(v).and_then(|p| p.inputs.last()).map(|(e, _)| e.clone()) { window.focus(&input.focus_handle(cx), cx); }
    })))).into_any_element()
}

// ---- 分组 / 排序 ----

/// 分组 nests this deep.
const GROUP_LEVELS: usize = 3;

fn order_panel(t: &Table, view: &View, kind: Kind, cx: &Context<TableView>) -> AnyElement {
    let (list, group) = (sorts(view, kind), kind == Kind::Group);
    let mut p = shell(if group { "tb-panel-group" } else { "tb-panel-sort" }, if list.is_empty() { EMPTY_W } else { 400. }, cx).p(px(crate::design::geo().popover_pad)).gap(px(crate::design::geo().panel_gap)).child(title(if group { "设置分组条件" } else { "设置排序条件" }));
    for (i, s) in list.iter().enumerate() {
        let f = t.field(s.field);
        let (asc, desc) = order_words(f.map(|f| f.ty));
        let seg = |k: usize, text: &str, on: bool| segment(("tb-po-seg", i * 2 + k), on, label(text))
            .on_click(cx.listener(move |v, _, _, cx| v.edit_view(cx, |view| if let Some(s) = sorts_mut(view, kind).get_mut(i) { s.desc = k == 1; })));
        p = p.child(div().flex().items_center().gap(px(8.))
            .child(pick_box(("tb-po-field", i), field_body(f)).flex_none().w(px(140.)).on_click(cx.listener(move |v, ev: &ClickEvent, _, cx| {
                // a field sorts (groups) once
                let Some((cur, used)) = peek(v, cx, |_, view| { let l = sorts(view, kind); (l.get(i).map(|s| s.field), l.iter().enumerate().filter(|(k, _)| *k != i).map(|(_, s)| s.field).collect::<Vec<_>>()) }) else { return };
                pick_field(v, below(ev), cur, used, cx, move |v, fid, _, cx| v.edit_view(cx, |view| {
                    if let Some(s) = sorts_mut(view, kind).get_mut(i) { s.field = fid; }
                    if kind == Kind::Group { view.opts.folded.clear(); }
                }));
            })))
            .child(seg_track().flex_1().min_w_0().child(seg(0, asc, !s.desc)).child(seg(1, desc, s.desc)))
            .child(x_btn(("tb-po-del", i)).on_click(cx.listener(move |v, _, _, cx| v.edit_view(cx, |view| {
                let l = sorts_mut(view, kind);
                if i < l.len() { l.remove(i); }
                if kind == Kind::Group { view.opts.folded.clear(); }
            })))));
    }
    // 飞书's last line is an empty picker: choosing a field adds the next level
    if t.fields.iter().any(|f| !list.iter().any(|s| s.field == f.id)) && (!group || list.len() < GROUP_LEVELS) {
        p = p.child(div().flex().child(pick_box("tb-po-add", div().text_color(Theme::text_placeholder()).child(label("选择条件"))).flex_none().w(px(140.)).on_click(cx.listener(move |v, ev: &ClickEvent, _, cx| {
            let used = peek(v, cx, |_, view| sorts(view, kind).iter().map(|s| s.field).collect::<Vec<_>>()).unwrap_or_default();
            pick_field(v, below(ev), None, used, cx, move |v, fid, _, cx| v.edit_view(cx, |view| {
                let l = sorts_mut(view, kind);
                if !l.iter().any(|s| s.field == fid) && (kind != Kind::Group || l.len() < GROUP_LEVELS) { l.push(SortBy { field: fid, desc: false }); }
            }));
        }))));
    }
    if group && !list.is_empty() {
        let fold = |id: &'static str, text: &str, shut: bool| text_btn(id, "", text).on_click(cx.listener(move |v, _, _, cx| {
            // what is under a folded group is not listed: the headers in sight are the ones to shut
            let keys: Vec<String> = if shut { v.result.rows.iter().filter_map(|r| match r { query::Row::Group(g) => Some(g.key.clone()), _ => None }).collect() } else { vec![] };
            v.edit_view(cx, |view| if shut { view.opts.folded.extend(keys) } else { view.opts.folded.clear() });
        }));
        p = p.child(div().flex_none().h(px(1.)).bg(Theme::line_divider()))
            .child(div().flex().items_center().gap(px(4.)).child(fold("tb-po-fold", "全部折叠", true)).child(fold("tb-po-unfold", "全部展开", false)));
    }
    p.into_any_element()
}

// ---- 行高 ----

fn height_panel(view: &View, cx: &Context<TableView>) -> AnyElement {
    let cur = view.opts.row_height;
    let mut p = shell("tb-panel-height", 140., cx).py(px(8.)).child(div().px(px(12.)).pb(px(4.)).child(title("设置行高")));
    for (i, h) in RowHeight::ALL.iter().copied().enumerate() {
        p = p.child(menu_item(("tb-ph", i), "", label(h.label()), false).when(h == cur, |d| d.text_color(Theme::primary()).child(icon("check", 14., Theme::primary())))
            .on_click(cx.listener(move |v, _, window, cx| { v.edit_view(cx, |view| view.opts.row_height = h); v.close_popup(window, cx); })));
    }
    p.into_any_element()
}

// ---- 填色 ----

fn color_panel(t: &Table, view: &View, panel: &Panel, cx: &Context<TableView>) -> AnyElement {
    let mut p = shell("tb-panel-color", if view.opts.colors.is_empty() { EMPTY_W } else { 560. }, cx).p(px(crate::design::geo().popover_pad)).gap(px(crate::design::geo().panel_gap)).child(title("设置填色条件"));
    for (i, rule) in view.opts.colors.iter().enumerate() {
        let cur = rule.color % 10;
        p = p.child(div().flex().items_center().gap(px(8.))
            .child(swatch(("tb-pk-swatch", i), tag_colors(cur).0, crate::design::geo().field_sm_h, panel.palette == Some(i))
                .on_click(cx.listener(move |v, _, _, cx| {
                    if let Some(p) = panel_mut(v) { p.palette = if p.palette == Some(i) { None } else { Some(i) }; }
                    cx.notify();
                })))
            .child(match rule.filter.conds.first() { Some(c) => cond_controls(t, Kind::Color, i, c, panel, 120., cx), None => div().flex_1() })
            .child(x_btn(("tb-pk-del", i)).on_click(cx.listener(move |v, _, window, cx| remove_cond(v, Kind::Color, i, window, cx)))));
        if panel.palette != Some(i) { continue; }
        let mut row = div().flex().items_center().gap(px(8.));
        for slot in 0..10u8 {
            let (bg, fg) = tag_colors(slot);
            row = row.child(swatch(("tb-pk-color", slot as usize), bg, 22., slot == cur).flex().items_center().justify_center()
                .when(slot == cur, |d| d.child(icon("check", 12., fg)))
                .on_click(cx.listener(move |v, _, _, cx| {
                    v.edit_view(cx, |view| if let Some(r) = view.opts.colors.get_mut(i) { r.color = slot; });
                    if let Some(p) = panel_mut(v) { p.palette = None; }
                    cx.notify();
                })));
        }
        p = p.child(row);
    }
    p.child(div().flex().child(text_btn("tb-pk-add", "plus", "添加条件").on_click(cx.listener(|v, _, window, cx| {
        let Some(c) = new_cond(v, cx) else { return };
        v.edit_view(cx, |view| {
            // 飞书 starts on yellow; then whichever colour no rule has yet
            let color = (0..10u8).map(|k| (k + 2) % 10).find(|s| !view.opts.colors.iter().any(|r| r.color == *s)).unwrap_or(2);
            view.opts.colors.push(ColorRule { filter: Filter { all: true, conds: vec![c] }, color });
        });
        sync_inputs(v, window, cx);
        if let Some(input) = panel_mut(v).and_then(|p| p.inputs.last()).map(|(e, _)| e.clone()) { window.focus(&input.focus_handle(cx), cx); }
    })))).into_any_element()
}
