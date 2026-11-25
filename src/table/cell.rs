//! What a cell shows, by field type: tags, people, stars, a progress bar,
//! link chips, plain text. `value_view` is the still picture (cards, 记录详情
//! reuse it); `content` adds what can be clicked in the grid itself.

use super::style::*;
use super::*;
use crate::ui::ui_text;

/// One line of text that ends in an ellipsis, or (in a tall row) wraps.
fn text_line(s: String, color: Hsla, wrap: bool) -> Div {
    if wrap { div().w_full().text_color(color).line_height(px(20.)).overflow_hidden().child(SharedString::from(s)) }
    // the in-place editor's line height (`TextField`: 20), so typing and the finished cell put the text at the same place
    else { div().w_full().text_color(color).line_height(px(20.)).overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui_text(s)) }
}

/// A value as shown, without anything to click. `wrap`: there is room for more than one line.
pub(crate) fn value_view(base: &Base, f: &Field, v: &Value, wrap: bool) -> AnyElement {
    let chips = || div().w_full().flex().items_center().gap(px(4.)).overflow_hidden().when(wrap, |d| d.flex_wrap().gap_y(px(4.)));
    match v {
        Value::Empty => div().into_any_element(),
        Value::Error(e) => text_line(e.clone(), Theme::danger(), false).into_any_element(),
        Value::Bool(b) => div().w_full().flex().justify_center().child(checkbox(*b)).into_any_element(),
        Value::Select(ids) => chips().children(ids.iter().filter_map(|id| f.option(*id)).map(|o| if f.ty == FieldType::Person { person(&o.name, o.color).into_any_element() } else { tag(&o.name, o.color).into_any_element() })).into_any_element(),
        Value::Links(ids) => {
            let other = f.opts.link_table.and_then(|t| base.table(t));
            chips().children(ids.iter().map(|id| {
                let title = other.and_then(|t| t.record(*id).map(|r| t.title(r))).unwrap_or_default();
                link_chip(if title.is_empty() { crate::settings::text("未命名记录") } else { &title })
            })).into_any_element()
        }
        Value::Files(fs) => chips().children(fs.iter().map(|a| div().flex_none().h(px(22.)).max_w(px(160.)).px(px(6.)).rounded(px(4.)).border_1().border_color(line()).flex().items_center().gap(px(4.))
            .child(icon("paperclip", 12., Theme::text_caption()))
            .child(div().text_size(px(12.)).overflow_hidden().whitespace_nowrap().text_ellipsis().child(ui_text(a.name.clone()))))).into_any_element(),
        Value::Number(n) if f.ty == FieldType::Rating => div().flex().items_center().gap(px(2.)).children((1..=f.stars()).map(|s| icon("star", 15., if (s as f64) <= *n { star() } else { crate::design::color::border() }))).into_any_element(),
        Value::Number(n) if f.ty == FieldType::Progress => div().w_full().flex().items_center().gap(px(8.))
            .child(div().flex_1().min_w(px(20.)).h(px(6.)).rounded(px(3.)).bg(crate::design::color::muted()).child(div().h_full().rounded(px(3.)).bg(Theme::primary()).w(relative(n.clamp(0., 1.) as f32))))
            .child(div().flex_none().text_color(Theme::text()).child(field::format_number(*n, f))).into_any_element(),
        Value::Number(n) => div().w_full().flex().justify_end().text_color(Theme::text()).line_height(px(20.)).whitespace_nowrap().child(ui_text(field::format_number(*n, f))).into_any_element(),
        Value::Text(s) if f.ty == FieldType::Url || f.ty == FieldType::Email => text_line(s.clone(), Theme::link(), false).into_any_element(),
        Value::Text(s) => text_line(s.clone(), Theme::text(), wrap).into_any_element(),
        Value::Date(_) | Value::List(_) => text_line(field::display_in(base, f, v), Theme::text(), wrap).into_any_element(),
    }
}

/// A grid cell's content: `value_view`, with the checkbox and the stars taking clicks.
pub(crate) fn content(_v: &TableView, base: &Base, _t: &Table, f: &Field, r: &Record, row_h: f32, cx: &Context<TableView>) -> AnyElement {
    let val = r.get(f.id);
    let cell: CellId = (r.id, f.id);
    match f.ty {
        FieldType::Checkbox => div().w_full().flex().justify_center().child(div().id(("tb-chk", r.id as usize)).cursor_pointer().p(px(4.)).child(checkbox(matches!(val, Value::Bool(true))))
            .on_mouse_down(MouseButton::Left, cx.listener(move |v, _, window, cx| {
                cx.stop_propagation();
                v.commit_edit(true, window, cx);
                v.sel = Some(Sel { anchor: cell, head: cell });
                v.toggle_check(cell, cx);
            }))).into_any_element(),
        FieldType::Rating => {
            let n = val.as_number().unwrap_or(0.);
            div().group("tb-stars").flex().items_center().gap(px(2.)).children((1..=f.stars()).map(|s| {
                let on = (s as f64) <= n;
                div().id(("tb-star", (r.id as usize) << 4 | s as usize)).cursor_pointer().when(!on, |d| d.invisible().group_hover("tb-row", |s| s.visible()))
                    .child(icon("star", 15., if on { star() } else { crate::design::color::border() }))
                    .on_mouse_down(MouseButton::Left, cx.listener(move |v, _, window, cx| {
                        cx.stop_propagation();
                        v.commit_edit(true, window, cx);
                        v.sel = Some(Sel { anchor: cell, head: cell });
                        let tid = v.table;
                        // the same star again clears it
                        v.edit(cx, |d| d.set_cells(tid, vec![(cell.0, cell.1, if s as f64 == n { Value::Empty } else { Value::Number(s as f64) })]));
                    }))
            })).into_any_element()
        }
        _ => value_view(base, f, val, row_h > one_line_h()),
    }
}
