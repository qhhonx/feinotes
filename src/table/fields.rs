//! 新增 / 修改字段: the field form, after 飞书's (REPORT.md §B). It holds a
//! draft (name, type, settings) with its own inputs; 确定 writes it to the
//! document as one `add_field` / `change_field`, anything else drops it.

use super::style::*;
use super::*;
use crate::ui::{ui_text};

fn label(s: &str) -> String { crate::settings::label(s) }

const W: f32 = 340.;
const PAD: f32 = 16.;

/// An input of the form, with what listens to it.
type Input = (Entity<TextField>, Subscription);

pub(crate) struct FieldForm {
    /// The field being edited (None: a new one), as it was when the form opened.
    orig: Option<Field>,
    /// Where a new field goes in the table's own order.
    at: Option<usize>,
    /// A new field sits beside this one in the view: (field, after it).
    beside: Option<(FieldId, bool)>,
    /// The table's first field: only types a title can be.
    primary: bool,
    ty: FieldType,
    /// The draft's settings. Options made here have id 0 until 确定; `formula`,
    /// `prefix` and a typed `default` live in their inputs.
    opts: FieldOpts,
    name: Input,
    formula: Input,
    prefix: Input,
    default: Input,
    /// One name input per option of `opts.options`, in step with it.
    rows: Vec<Input>,
    types_open: bool,
    /// The option whose colours are shown.
    palette: Option<usize>,
    /// The function whose description is shown (index into `formula::functions`).
    hint: Option<usize>,
    /// The form's bounds as last painted (dropdowns hang from its left edge).
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

fn form(v: &mut TableView) -> Option<&mut FieldForm> { match v.popup.as_mut() { Some((_, Popup::FieldForm(f))) => Some(f), _ => None } }

/// Change the open form's draft.
fn with(v: &mut TableView, cx: &mut Context<TableView>, f: impl FnOnce(&mut FieldForm)) { if let Some(form) = form(v) { f(form); } cx.notify(); }

/// A single-line input: Enter is 确定 (in an option's name: the next option), Esc is 取消.
fn input(hint: &str, text: &str, window: &mut Window, cx: &mut Context<TableView>) -> Input {
    let e = cx.new(|cx| { let mut t = TextField::new(hint.to_string(), cx); t.size = text_sm(); t.set_text(text, cx); t });
    let sub = cx.subscribe_in(&e, window, |v, e, ev: &TextFieldEvent, window, cx| match ev {
        TextFieldEvent::Submit => if form(v).is_some_and(|f| f.rows.iter().any(|r| r.0 == *e)) { add_option(v, window, cx) } else { confirm(v, window, cx) },
        TextFieldEvent::Cancel => v.close_popup(window, cx),
        _ => cx.notify(),
    });
    (e, sub)
}

/// What the table's first field may be (`Doc::change_field` holds the same line).
fn title_ok(ty: FieldType) -> bool {
    ty.textual() || matches!(ty, FieldType::Number | FieldType::Date | FieldType::Formula | FieldType::AutoNumber | FieldType::Currency | FieldType::Phone | FieldType::Email)
}

/// Types whose 默认值 is typed as text.
fn typed_default(ty: FieldType) -> bool { ty.textual() || matches!(ty, FieldType::Number | FieldType::Currency) }

impl FieldForm {
    pub fn add(_v: &mut TableView, at: Option<usize>, window: &mut Window, cx: &mut Context<TableView>) -> Self { Self::new(None, false, at, window, cx) }

    pub fn edit(v: &mut TableView, field: FieldId, window: &mut Window, cx: &mut Context<TableView>) -> Option<Self> {
        let (f, primary) = { let t = v.state.read(cx).doc.base.table(v.table)?; (t.field(field)?.clone(), t.fields.first().is_some_and(|p| p.id == field)) };
        Some(Self::new(Some(f), primary, None, window, cx))
    }

    pub fn beside(mut self, field: FieldId, after: bool) -> Self { self.beside = Some((field, after)); self }

    fn new(orig: Option<Field>, primary: bool, at: Option<usize>, window: &mut Window, cx: &mut Context<TableView>) -> Self {
        let (title, ty, opts) = match &orig { Some(f) => (f.name.clone(), f.ty, f.opts.clone()), None => (String::new(), FieldType::Text, field::default_opts(FieldType::Text)) };
        let name = input("请输入字段标题", &title, window, cx);
        let formula = input("请输入公式", &opts.formula, window, cx);
        formula.0.update(cx, |t, _| { t.multiline = true; t.wrap = true; t.mono = true; });
        let prefix = input("编号前缀，例如 NO.", &opts.prefix, window, cx);
        let default = input("请输入默认值", if typed_default(ty) { &opts.default } else { "" }, window, cx);
        if orig.is_some() { name.0.update(cx, |t, cx| t.select_all_text(cx)); }
        window.focus(&name.0.focus_handle(cx), cx);
        let mut f = FieldForm { orig, at, beside: None, primary, ty, opts, name, formula, prefix, default, rows: vec![], types_open: false, palette: None, hint: None, bounds: Default::default() };
        f.set_rows(window, cx);
        f
    }

    /// An input for each option of the draft.
    fn set_rows(&mut self, window: &mut Window, cx: &mut Context<TableView>) {
        self.rows = self.opts.options.iter().map(|o| input("请输入选项", &o.name, window, cx)).collect();
    }

    /// Another type: its default settings, keeping what still applies. Back
    /// to the field's own type: the settings it had.
    fn retype(&mut self, ty: FieldType, window: &mut Window, cx: &mut Context<TableView>) {
        self.types_open = false;
        if ty == self.ty { return; }
        let (old, was) = (std::mem::replace(&mut self.opts, field::default_opts(ty)), self.ty);
        if let Some(o) = self.orig.as_ref().filter(|o| o.ty == ty) { self.opts = o.opts.clone(); }
        // 单选 ↔ 多选 ↔ 人员 keep the options as edited
        if ty.select() && was.select() { self.opts.options = old.options; } else { self.set_rows(window, cx); }
        if ty.is_date() && was.is_date() { self.opts.date = old.date; self.opts.time = old.time; }
        if matches!(ty, FieldType::Number | FieldType::Formula) && matches!(was, FieldType::Number | FieldType::Formula) { self.opts.precision = old.precision; self.opts.style = old.style; }
        self.ty = ty;
        self.palette = None;
        self.hint = None;
    }

    /// Why 确定 can't be pressed yet.
    fn problem(&self, base: &Base, t: &Table, cx: &App) -> Option<String> {
        match self.ty {
            FieldType::Formula => {
                let src = &self.formula.0.read(cx).text;
                if src.trim().is_empty() { Some(label("请输入公式")) } else { formula::compile(src, t).err() }
            }
            FieldType::Lookup => {
                let other = self.via_table(base, t);
                if other.zip(self.opts.target).is_some_and(|(o, id)| o.field(id).is_some()) { None } else { Some(label("请选择关联字段和要引用的字段")) }
            }
            _ => None,
        }
    }

    /// 查找引用: the table its 关联 field points to.
    fn via_table<'a>(&self, base: &'a Base, t: &Table) -> Option<&'a Table> {
        let via = self.opts.via.and_then(|id| t.field(id)).filter(|f| f.ty == FieldType::Link)?;
        base.table(via.opts.link_table?)
    }

    /// The draft as a field's name and settings (new options still have id 0).
    fn draft(&self, t: &Table, cx: &App) -> (String, FieldOpts) {
        let text = |i: &Input| i.0.read(cx).text.trim().to_string();
        let (ty, mut opts) = (self.ty, self.opts.clone());
        for (o, r) in opts.options.iter_mut().zip(&self.rows) { o.name = text(r); }
        opts.options.retain(|o| !o.name.is_empty());
        if !ty.select() { opts.options.clear(); }
        opts.formula = if ty == FieldType::Formula { text(&self.formula) } else { String::new() };
        opts.prefix = if ty == FieldType::AutoNumber { self.prefix.0.read(cx).text.clone() } else { String::new() };
        opts.default = if typed_default(ty) { text(&self.default) } else if ty == FieldType::Date && opts.default == "now" { "now".into() } else { String::new() };
        if ty == FieldType::Link {
            opts.link_table.get_or_insert(t.id);
            // a mirror is only made with a new field (`Doc::add_field`)
            if opts.back == Some(0) && self.orig.is_some() { opts.back = None; }
        }
        let name = text(&self.name);
        (if name.is_empty() { label(ty.label()) } else { name }, opts)
    }
}

fn add_option(v: &mut TableView, window: &mut Window, cx: &mut Context<TableView>) {
    let row = input("请输入选项", "", window, cx);
    window.focus(&row.0.focus_handle(cx), cx);
    with(v, cx, |f| {
        let color = (f.opts.options.len() % 10) as u8;
        f.opts.options.push(SelectOption { id: 0, name: String::new(), color });
        f.rows.push(row);
        f.palette = None;
    });
}

/// 确定: write the draft (one undo step; a place in the view's order is a second).
fn confirm(v: &mut TableView, window: &mut Window, cx: &mut Context<TableView>) {
    let (tid, vid) = (v.table, v.view);
    let (orig, at, beside, ty, name, mut opts) = {
        let st = v.state.read(cx);
        let (Some(t), Some((_, Popup::FieldForm(f)))) = (st.doc.base.table(tid), v.popup.as_ref()) else { return };
        if f.problem(&st.doc.base, t, cx).is_some() { return; }
        let (name, opts) = f.draft(t, cx);
        (f.orig.clone(), f.at, f.beside, f.ty, name, opts)
    };
    // left blank, a new field is called by its type in the interface language
    let name = if name.trim().is_empty() && orig.is_none() { crate::settings::text(ty.label()).to_string() } else { name };
    match orig {
        Some(old) => {
            v.edit(cx, |d| {
                for o in &mut opts.options { if o.id == 0 { o.id = d.base.new_id(); } }
                d.change_field(tid, Field { id: old.id, name, ty, opts, desc: old.desc })
            });
        }
        None => {
            let id = v.edit(cx, |d| {
                for o in &mut opts.options { if o.id == 0 { o.id = d.base.new_id(); } }
                d.add_field(tid, at, &name, ty, opts)
            });
            if let (Some(id), Some((by, after))) = (id, beside) {
                let order = {
                    let st = v.state.read(cx);
                    st.doc.base.table(tid).and_then(|t| t.view(vid).map(|view| {
                        let mut ids: Vec<FieldId> = t.ordered_fields(view).iter().map(|f| f.id).filter(|f| *f != id).collect();
                        // never before the primary field
                        let to = ids.iter().position(|f| *f == by).map_or(ids.len(), |p| p + after as usize).max(1).min(ids.len());
                        ids.insert(to, id);
                        ids
                    }))
                };
                if let Some(order) = order { v.edit_view(cx, |view| view.opts.order = order); }
            }
        }
    }
    v.close_popup(window, cx);
}

// ---- pieces ----

fn caption(s: &str) -> crate::design::Label { field_label(label(s)) }

/// A labelled control.
fn block(name: &str, el: impl IntoElement) -> Div { div().flex_none().flex().flex_col().gap(px(8.)).child(caption(name)).child(el) }

fn text_input(i: &Input) -> crate::design::Field { field_box(("tb-ff-input", i.0.entity_id())).child(div().flex_1().min_w_0().flex().child(i.0.clone())) }


/// A setting that is on or off: its name, then the switch.
fn switch_row(id: &'static str, name: &str, on: bool, enabled: bool, cx: &Context<TableView>, set: impl Fn(&mut FieldForm, bool) + 'static) -> Div {
    div().flex_none().child(div().id(id).h(px(24.)).flex().items_center().gap(px(8.))
        .child(div().flex_1().min_w_0().text_color(if enabled { Theme::text() } else { Theme::text_disabled() }).child(label(name)))
        .child(switch(on, enabled))
        .when(enabled, |d| d.cursor_pointer().on_click(cx.listener(move |v, _, _, cx| with(v, cx, |f| set(f, !on))))))
}

/// A select box whose choices drop down under the pointer, the form's width.
fn select(id: &'static str, cur: impl Into<SharedString>, items: Vec<(String, Option<&'static str>)>, on: Option<usize>, cx: &Context<TableView>, pick: impl Fn(&mut FieldForm, usize) + 'static) -> crate::design::Field {
    let pick: Rc<dyn Fn(&mut FieldForm, usize)> = Rc::new(pick);
    select_box(id, cur).on_click(cx.listener(move |v, ev: &ClickEvent, _, cx| {
        let Some(f) = form(v) else { return };
        let pos = point(f.bounds.get().left() + px(PAD), ev.position().y + px(20.));
        let list = items.iter().enumerate().map(|(i, (name, glyph))| { let it = DropItem::new(name.clone()).on(on == Some(i)); match glyph { Some(g) => it.glyph(g), None => it } }).collect();
        let pick = pick.clone();
        v.open_dropdown(pos, W - 2. * PAD, list, move |v, i, _, cx| with(v, cx, |f| pick(f, i)), cx);
    }))
}

/// 数字格式, 飞书's list; `auto` leads with "as typed".
fn number_formats(auto: bool) -> Vec<(String, Option<u8>, NumberStyle)> {
    let mut out = vec![];
    if auto { out.push((label("默认"), None, NumberStyle::Plain)); }
    out.push((label("整数"), Some(0), NumberStyle::Plain));
    out.push((label("千分位"), Some(0), NumberStyle::Thousands));
    out.push((label("千分位（小数点）"), Some(2), NumberStyle::Thousands));
    for p in 1..=9u8 { out.push((format!("{} {p} {}", label("保留"), label("位小数")), Some(p), NumberStyle::Plain)); }
    out.push((label("百分比"), Some(0), NumberStyle::Percent));
    out.push((label("百分比（小数点）"), Some(2), NumberStyle::Percent));
    out
}

fn number_format(name: &str, o: &FieldOpts, cx: &Context<TableView>) -> Div {
    let fmts = number_formats(true);
    // a precision the list doesn't have shows as the style's nearest entry
    let at = fmts.iter().position(|f| f.1 == o.precision && f.2 == o.style)
        .or_else(|| fmts.iter().position(|f| f.2 == o.style && f.1.is_some() && (f.1 == Some(0)) == (o.precision.unwrap_or(0) == 0)));
    let cur = at.map(|i| fmts[i].0.clone()).unwrap_or_default();
    let set: Vec<(Option<u8>, NumberStyle)> = fmts.iter().map(|f| (f.1, f.2)).collect();
    block(name, select("tb-ff-numfmt", cur, fmts.into_iter().map(|f| (f.0, None)).collect(), at, cx, move |f, i| { f.opts.precision = set[i].0; f.opts.style = set[i].1; }))
}

/// 小数位数: 整数, 1 位 … `max` 位, each with a sample.
fn decimals(o: &FieldOpts, max: u8, sample: impl Fn(u8) -> String, cx: &Context<TableView>) -> Div {
    let name = |p: u8| { let n = if p == 0 { label("整数") } else { format!("{p} {}", label("位")) }; let s = sample(p); if s.is_empty() { n } else { format!("{n}　{s}") } };
    let cur = o.precision.unwrap_or(0).min(max);
    block("小数位数", select("tb-ff-decimals", name(cur), (0..=max).map(|p| (name(p), None)).collect(), Some(cur as usize), cx, |f, i| f.opts.precision = Some(i as u8)))
}

/// 货币, by symbol (the symbol is all a field keeps).
const CURRENCIES: &[(&str, &str, &str)] = &[
    ("CNY", "¥", "人民币"), ("USD", "$", "美元"), ("EUR", "€", "欧元"), ("GBP", "£", "英镑"), ("HKD", "HK$", "港元"), ("AED", "dh", "阿联酋迪拉姆"),
    ("AUD", "A$", "澳大利亚元"), ("BRL", "R$", "巴西雷亚尔"), ("CAD", "C$", "加拿大元"), ("CHF", "CHF", "瑞士法郎"), ("INR", "₹", "印度卢比"), ("KRW", "₩", "韩元"), ("RUB", "₽", "俄罗斯卢布"),
];

fn date_format(o: &FieldOpts, cx: &Context<TableView>) -> Div {
    let time = o.time;
    let name = move |d: DateFmt| if time { format!("{} 14:00", d.sample()) } else { d.sample().to_string() };
    block("日期格式", select("tb-ff-datefmt", name(o.date), DateFmt::ALL.iter().map(|d| (name(*d), None)).collect(), DateFmt::ALL.iter().position(|d| *d == o.date), cx, |f, i| f.opts.date = DateFmt::ALL[i]))
}

/// 字段类型: the current type; the grouped list opens under it.
fn type_box(f: &FieldForm, cx: &Context<TableView>) -> Div {
    let open = f.types_open;
    let head = crate::design::Field::new("tb-ff-type").focused(open).cursor_pointer()
        .child(icon(field_glyph(f.ty), 15., Theme::icon()))
        .child(div().flex_1().min_w_0().child(label(f.ty.label())))
        .child(icon(if open { "chevron-up" } else { "chevron-down" }, 13., Theme::icon()))
        .on_click(cx.listener(|v, _, _, cx| with(v, cx, |f| f.types_open = !f.types_open)));
    let mut out = div().flex().flex_col().gap(px(4.)).child(head);
    if open {
        let mut list = inset_list("tb-ff-types").max_h(px(300.)).overflow_y_scroll();
        for g in [FieldGroup::General, FieldGroup::Business, FieldGroup::Advanced] {
            list = list.child(menu_title(label(g.label())));
            // a file is one table: 关联 and 查找引用 (which reach into another) are not offered, only kept where a file has them
            let other_table = |ty: FieldType| matches!(ty, FieldType::Link | FieldType::Lookup) && ty != f.ty;
            for (i, ty) in FieldType::ALL.iter().copied().enumerate().filter(|(_, ty)| ty.group() == g && !other_table(*ty)) {
                let ok = !f.primary || title_ok(ty);
                list = list.child(menu_row(("tb-ff-ty", i))
                    .child(icon(field_glyph(ty), 15., if ok { Theme::icon() } else { Theme::text_disabled() }))
                    .child(div().flex_1().min_w_0().text_color(if ok { Theme::text() } else { Theme::text_disabled() }).child(label(ty.label())))
                    .when(ty == f.ty, |d| d.child(icon("check", 14., Theme::primary())))
                    .when(ok, |d| d.cursor_pointer().hover(|s| s.bg(Theme::fill_hover())).on_click(cx.listener(move |v, _, window, cx| { if let Some(f) = form(v) { f.retype(ty, window, cx); } cx.notify(); }))));
            }
        }
        out = out.child(list);
    }
    block("字段类型", out)
}

/// 单选 / 多选's options, 人员's members: colour, name, remove; 添加 under them.
fn options(f: &FieldForm, cx: &Context<TableView>) -> Div {
    let person = f.ty == FieldType::Person;
    let mut list = div().id("tb-ff-opts").max_h(px(228.)).overflow_y_scroll().flex().flex_col().gap(px(6.));
    for (i, (o, row)) in f.opts.options.iter().zip(&f.rows).enumerate() {
        let (bg, fg) = tag_colors(o.color);
        list = list.child(div().flex_none().flex().items_center().gap(px(6.))
            .when(!person, |d| d.child(div().id(("tb-ff-opt-color", i)).flex_none().size(px(18.)).rounded_full().bg(bg).flex().items_center().justify_center().cursor_pointer()
                .child(icon("triangle-down", 8., fg))
                .on_click(cx.listener(move |v, _, _, cx| with(v, cx, |f| f.palette = if f.palette == Some(i) { None } else { Some(i) })))))
            .child(text_input(row).flex_1().min_w_0())
            .child(div().id(("tb-ff-opt-del", i)).flex_none().size(px(24.)).rounded(px(crate::design::geo().tool_r)).flex().items_center().justify_center().cursor_pointer().hover(|s| s.bg(Theme::fill_hover()))
                .child(icon("x", 13., Theme::icon()))
                .on_click(cx.listener(move |v, _, _, cx| with(v, cx, |f| if i < f.rows.len() && i < f.opts.options.len() { f.opts.options.remove(i); drop(f.rows.remove(i)); f.palette = None; })))));
        if f.palette == Some(i) && !person {
            let mut swatches = div().flex_none().p(px(8.)).rounded(px(crate::design::geo().menu_row_r)).bg(Theme::bg_body()).border_1().border_color(Theme::line_border()).flex().flex_wrap().gap(px(6.));
            for slot in 0..SLOTS as u8 {
                let (bg, fg) = tag_colors(slot);
                swatches = swatches.child(div().id(("tb-ff-swatch", slot as usize)).size(px(20.)).rounded(px(crate::design::geo().tool_r)).bg(bg).flex().items_center().justify_center().cursor_pointer()
                    .when(o.color == slot, |d| d.child(icon("check", 12., fg)))
                    .on_click(cx.listener(move |v, _, _, cx| with(v, cx, |f| { if let Some(o) = f.opts.options.get_mut(i) { o.color = slot; } f.palette = None; }))));
            }
            list = list.child(swatches);
        }
    }
    let add = div().id("tb-ff-opt-add").flex_none().h(px(28.)).px(px(8.)).flex().items_center().gap(px(4.)).rounded(px(crate::design::geo().tool_r)).cursor_pointer().text_color(Theme::primary()).hover(|s| s.bg(Theme::fill_hover()))
        .child(icon("plus", 13., Theme::primary())).child(label(if person { "添加成员" } else { "添加选项" }))
        .on_click(cx.listener(|v, _, window, cx| add_option(v, window, cx)));
    block(if person { "成员" } else { "选项内容" }, div().p(px(8.)).rounded(px(crate::design::geo().menu_row_r)).bg(Theme::primary_fill_1()).flex().flex_col().gap(px(6.))
        .when(!f.rows.is_empty(), |d| d.child(list))
        .child(div().flex().child(add)))
}

/// 公式: the source, whether it compiles, and what can go in it (fields, functions).
fn formula_blocks(f: &FieldForm, t: &Table, problem: &Option<String>, cx: &Context<TableView>) -> Vec<Div> {
    let entity = f.formula.0.clone();
    let append = move |v: &mut TableView, s: String, window: &mut Window, cx: &mut Context<TableView>| {
        let Some(e) = form(v).map(|f| f.formula.0.clone()) else { return };
        e.update(cx, |t, cx| { let all = format!("{}{s}", t.text); t.set_text(&all, cx); });
        window.focus(&e.focus_handle(cx), cx);
        cx.notify();
    };
    let focus = entity.clone();
    let editor = crate::design::Textarea::new("tb-ff-formula").min_h(px(80.)).flex().items_start().cursor(CursorStyle::IBeam)
        .child(entity)
        .on_click(cx.listener(move |_, _, window, cx| window.focus(&focus.focus_handle(cx), cx)));
    let status = match problem {
        Some(e) => div().flex_none().text_size(px(12.)).text_color(Theme::danger()).child(ui_text(e.clone())),
        None => div().flex_none().text_size(px(12.)).text_color(Theme::text_caption()).child(label("公式有效")),
    };
    let fns = formula::functions();
    let head = |s: String| div().flex_none().h(px(26.)).px(px(8.)).flex().items_center().text_size(px(12.)).text_color(Theme::text_caption()).child(s);
    let mut list = inset_list("tb-ff-ref").max_h(px(180.)).overflow_y_scroll();
    list = list.child(head(label("字段引用")));
    for (i, fld) in t.fields.iter().enumerate().filter(|(_, fld)| f.orig.as_ref().is_none_or(|o| o.id != fld.id)) {
        let (name, add) = (fld.name.clone(), append);
        list = list.child(div().id(("tb-ff-ref-field", i)).flex_none().h(px(28.)).mx(px(4.)).px(px(6.)).flex().items_center().gap(px(6.)).rounded(px(crate::design::geo().menu_row_r)).cursor_pointer().hover(|s| s.bg(Theme::fill_hover()))
            .child(icon(field_glyph(fld.ty), 14., Theme::icon()))
            .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui_text(fld.name.clone())))
            .on_click(cx.listener(move |v, _, window, cx| add(v, format!("{{{name}}}"), window, cx))));
    }
    let mut group = "";
    for (i, d) in fns.iter().enumerate() {
        if d.group != group { group = d.group; list = list.child(head(format!("{}{}", label(d.group), label("函数")))); }
        let add = append;
        list = list.child(div().id(("tb-ff-ref-fn", i)).flex_none().h(px(28.)).mx(px(4.)).px(px(6.)).flex().items_center().gap(px(8.)).rounded(px(crate::design::geo().menu_row_r)).cursor_pointer().hover(|s| s.bg(Theme::fill_hover()))
            .when(f.hint == Some(i), |x| x.bg(Theme::primary_fill_1()))
            .child(div().flex_none().font_family(crate::feishu_theme::MONO_FONT).text_size(px(12.)).child(d.name))
            .child(div().flex_1().min_w_0().text_size(px(12.)).text_color(Theme::text_caption()).overflow_hidden().whitespace_nowrap().text_ellipsis().child(d.sig))
            .on_click(cx.listener(move |v, _, window, cx| { if let Some(f) = form(v) { f.hint = Some(i); } add(v, format!("{}(", fns[i].name), window, cx); })));
    }
    let mut out = vec![block("公式内容", div().flex().flex_col().gap(px(6.)).child(editor).child(status))];
    let hint = f.hint.and_then(|i| fns.get(i)).map(|d| div().flex_none().flex().flex_col().gap(px(2.)).text_size(px(12.))
        .child(div().font_family(crate::feishu_theme::MONO_FONT).child(d.sig))
        .child(div().text_color(Theme::text_caption()).child(label(d.desc))));
    out.push(block("字段与函数", div().flex().flex_col().gap(px(6.)).child(list).children(hint)));
    out.push(number_format("字段格式", &f.opts, cx));
    out
}

/// The chosen type's settings.
fn settings(f: &FieldForm, base: &Base, t: &Table, problem: &Option<String>, cx: &Context<TableView>) -> Vec<Div> {
    use FieldType::*;
    let o = &f.opts;
    let default = || block("默认值", text_input(&f.default));
    match f.ty {
        Text | Phone | Email | Url | Location | Barcode => vec![default()],
        Attachment | Checkbox => vec![],
        SingleSelect | MultiSelect | Person => vec![options(f, cx)],
        Number => vec![number_format("数字格式", o, cx), default()],
        Currency => {
            let cur = CURRENCIES.iter().position(|c| c.1 == o.currency);
            let name = |c: &(&str, &str, &str)| format!("{} - {} {}", c.0, c.1, label(c.2));
            let symbol = if o.currency.is_empty() { "¥".to_string() } else { o.currency.clone() };
            let sample = symbol.clone();
            vec![
                block("货币", select("tb-ff-currency", cur.map_or(symbol, |i| name(&CURRENCIES[i])), CURRENCIES.iter().map(|c| (name(c), None)).collect(), cur, cx, |f, i| f.opts.currency = CURRENCIES[i].1.to_string())),
                decimals(o, 4, move |p| format!("{sample}1,000{}{}", if p > 0 { "." } else { "" }, "0".repeat(p as usize)), cx),
                default(),
            ]
        }
        Progress => vec![decimals(o, 2, |p| format!("50{}{}%", if p > 0 { "." } else { "" }, "0".repeat(p as usize)), cx)],
        Rating => {
            let max = if o.max == 0 { 5 } else { o.max.min(10) };
            vec![block("最大分值", select("tb-ff-rating", format!("{max}"), (1..=10).map(|n| (format!("{n}"), None)).collect(), Some(max as usize - 1), cx, |f, i| f.opts.max = i as u8 + 1))]
        }
        Date => vec![
            date_format(o, cx),
            switch_row("tb-ff-time", "显示时间", o.time, true, cx, |f, on| f.opts.time = on),
            switch_row("tb-ff-now", "新记录默认填入创建日期", o.default == "now", true, cx, |f, on| f.opts.default = if on { "now".into() } else { String::new() }),
        ],
        CreatedTime | ModifiedTime => vec![date_format(o, cx), switch_row("tb-ff-time", "显示时间", o.time, true, cx, |f, on| f.opts.time = on)],
        AutoNumber => vec![block("编号前缀", text_input(&f.prefix))],
        Formula => formula_blocks(f, t, problem, cx),
        // a file is one table now: 关联 and 查找引用 from an older file keep working, their settings shown, not changed
        Link => {
            let name = o.link_table.and_then(|id| base.table(id)).map(|x| x.name.clone()).unwrap_or_else(|| label("已删除的数据表"));
            vec![block("关联的数据表", fixed(name)), block("允许关联多条记录", fixed(label(if o.single { "否" } else { "是" }))), legacy_note()]
        }
        Lookup => {
            let via = o.via.and_then(|id| t.field(id));
            let target = via.and_then(|v| v.opts.link_table).and_then(|id| base.table(id)).zip(o.target).and_then(|(x, id)| x.field(id));
            let name = |f: Option<&Field>| f.map_or_else(|| label("已删除的字段"), |f| f.name.clone());
            vec![block("关联字段", fixed(name(via))), block("要引用的字段", fixed(name(target))), block("计算方式", fixed(label(o.rollup.label()))), legacy_note()]
        }
    }
}

/// A setting that can no longer be changed, as text in a quiet box.
fn fixed(value: String) -> crate::design::Field {
    field_box(ElementId::Name(value.clone().into())).px(px(10.)).text_color(Theme::text_caption()).overflow_hidden().whitespace_nowrap().text_ellipsis().child(value)
}

fn legacy_note() -> Div {
    div().flex_none().text_size(px(12.)).line_height(px(18.)).text_color(Theme::text_placeholder()).child(label("这个字段来自有多张数据表的旧文件：它照常显示，设置不能再修改。"))
}

pub(crate) fn render(_v: &TableView, base: &Base, t: &Table, form: &FieldForm, cx: &Context<TableView>) -> AnyElement {
    let problem = form.problem(base, t, cx);
    let bounds = form.bounds.clone();
    let body = div().id("tb-ff-body").max_h(px(560.)).overflow_y_scroll().px(px(PAD)).pb(px(4.)).flex().flex_col().gap(px(crate::design::geo().popover_gap))
        .child(block("标题", text_input(&form.name)))
        .child(type_box(form, cx))
        .children(settings(form, base, t, &problem, cx));
    let ok = problem.is_none();
    crate::design::Popover::new("tb-field-form").occlude().relative().w(px(W)).pt(px(PAD)).flex().flex_col().text_size(px(text_sm()))
        .on_mouse_down_out(cx.listener(|v, _, window, cx| if v.dropdown.is_none() { v.close_popup(window, cx) }))
        .child(canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {}).absolute().size_full())
        .child(body)
        .child(div().flex_none().p(px(PAD)).flex().justify_end().gap(px(8.))
            .child(plain_btn("tb-ff-cancel", label("取消")).on_click(cx.listener(|v, _, window, cx| v.close_popup(window, cx))))
            .child(primary_btn("tb-ff-ok", label("确定")).when(!ok, |d| d.opacity(0.5).cursor_default()).when(ok, |d| d.on_click(cx.listener(|v, _, window, cx| confirm(v, window, cx))))))
        .into_any_element()
}
