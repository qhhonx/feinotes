//! The panel's pages: each one's live pieces (可操作), then still states to compare (状态对照).
//! The 数据表's pieces are laid out as the table lays them out, with made-up rows.

use super::*;
use crate::design::{Badge, Button, ButtonSize, Card, Dialog, Field, Popover, Item, Kbd, Label, MenuItem, MenuLabel, MenuSeparator, Radio, Segment, Segmented, Separator, Switch, Textarea, Toast, ToastTone, geo};

type Cx<'a> = Context<'a, Gallery>;

const TYPES: [(&str, &str); 6] = [("ft-text", "文本"), ("ft-select", "单选"), ("ft-multi", "多选"), ("ft-number", "数字"), ("ft-date", "日期"), ("ft-check", "复选框")];
const OPTIONS: [(&str, u8); 5] = [("Next", 9), ("Reading", 0), ("Finished", 1), ("Set aside", 3), ("Abandoned", 4)];
const OPS: [&str; 6] = ["是", "不是", "包含", "不包含", "为空", "不为空"];
const FIELDS: [(&str, &str); 5] = [("ft-text", "Title"), ("ft-text", "Author"), ("ft-select", "Status"), ("ft-number", "Pages"), ("star", "Rating")];

/// A checkbox to click.
fn check(k: K, cx: &mut Cx, on: bool) -> Stateful<Div> { div().id(k).cursor_pointer().child(checkbox(on)).on_click(tap(cx, move |g, _, _| g.flip(k))) }
/// A switch to click.
fn toggle(k: K, cx: &mut Cx, on: bool, enabled: bool) -> Stateful<Div> {
    div().id(k).when(enabled, |d| d.cursor_pointer().on_click(tap(cx, move |g, _, _| g.flip(k)))).child(switch(on, enabled))
}
/// A segmented control whose segments pick.
fn segs(g: &Gallery, k: K, opts: &[&'static str], cx: &mut Cx) -> Segmented {
    let mut s = Segmented::new();
    for (i, o) in opts.iter().enumerate() { s = s.child(Segment::new((k.0, k.1 * 16 + i), o.to_string()).selected(g.at(k) == i).on_click(tap(cx, move |g, _, _| { g.pick.insert(k, i); }))); }
    s
}
/// Option colours to pick from, `n` of them.
fn swatches(g: &Gallery, k: K, n: u8, size: f32, cx: &mut Cx) -> Div {
    let mut d = div().flex().flex_wrap().gap(px(8.));
    for s in 0..n {
        let (bg, fg) = tag_colors(s);
        let on = g.at(k) == s as usize;
        d = d.child(swatch((k.0, k.1 * 32 + s as usize), bg, size, on).flex().items_center().justify_center().when(on, |d| d.child(icon("check", 12., fg)))
            .on_click(tap(cx, move |g, _, _| { g.pick.insert(k, s as usize); })));
    }
    d
}
fn chevron(open: bool) -> Svg { let i = icon("chevron-down", 14., color::muted_foreground()); if open { i.with_transformation(Transformation::rotate(percentage(0.5))) } else { i } }
/// A menu of `items` (glyph, name), the picked one ticked: picking sets `k` and closes.
fn choices(g: &Gallery, k: K, items: &[(&'static str, &'static str)], w: f32, cx: &mut Cx) -> Popover {
    let mut m = Popover::new(("menu", k.1)).menu().occlude().w(px(w));
    for (i, (glyph, name)) in items.iter().enumerate() {
        m = m.child(MenuItem::new((k.0, 100 + i)).icon(glyph.to_string()).label(name.to_string()).checked(g.at(k) == i)
            .on_click(tap(cx, move |g, _, _| { g.pick.insert(k, i); g.open = None; })));
    }
    m
}
/// A small select of `items`, the picked one shown.
fn small_select(g: &Gallery, k: K, items: &'static [&'static str], w: f32, cx: &mut Cx) -> Div {
    let list: Vec<(&str, &str)> = items.iter().map(|s| ("", *s)).collect();
    let menu = choices(g, k, &list, w.max(140.), cx);
    drop(g, k, select_box(k, items[g.at(k).min(items.len() - 1)]).open(g.open == Some(k)).w(px(w)), menu, cx)
}
/// A field picker (glyph and name), `w` wide.
fn field_select(g: &Gallery, k: K, w: f32, cx: &mut Cx) -> Div {
    let (glyph, name) = FIELDS[g.at(k) % FIELDS.len()];
    let menu = choices(g, k, &FIELDS, 200., cx);
    drop(g, k, pick_box(k, div().flex().items_center().gap(px(6.)).child(icon(glyph, 14., color::muted_foreground())).child(t(name))).open(g.open == Some(k)).w(px(w)), menu, cx)
}
/// Shuts the gallery's submenu when the pointer reaches a sibling row (Radix).
fn shut(cx: &mut Cx) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    cx.listener(|g: &mut Gallery, on: &bool, _, cx| if *on && g.is(("sub", 0)) { g.on.remove(&("sub", 0)); cx.notify(); })
}
fn two(live: Div, still: Div) -> Div { div().flex().flex_col().gap(px(32.)).child(group("可操作", live)).child(group("状态对照", still)) }

// ---- 按钮 ----

pub fn buttons(g: &mut Gallery, _w: &mut Window, cx: &mut Cx) -> Div {
    let gg = geo();
    let n = g.at(("tool", 0));
    let variants = |sz: ButtonSize, tag: &'static str, cx: &mut Cx| div().flex().items_center().gap(px(8.))
        .child(Button::primary((tag, 0usize)).sized(sz).label("确定").on_click(says(cx, "确定")))
        .child(Button::outline((tag, 1usize)).sized(sz).label("取消").on_click(says(cx, "取消")))
        .child(Button::secondary((tag, 2usize)).sized(sz).label("次要").on_click(says(cx, "次要")))
        .child(Button::ghost((tag, 3usize)).sized(sz).icon("plus").label("添加").on_click(says(cx, "添加")))
        .child(Button::destructive((tag, 4usize)).sized(sz).label("删除").on_click(says(cx, "删除")))
        .child(Button::link((tag, 5usize)).sized(sz).label("了解更多").on_click(says(cx, "了解更多")));
    let live = div().flex().flex_col().gap(px(20.))
        .child(item("Lg 40 高（主要 / 描边 / 次要 / 文字 / 危险 / 链接）", variants(ButtonSize::Lg, "bl", cx)))
        .child(item(&format!("默认 {} 高", gg.button_h), variants(ButtonSize::Default, "bd", cx)))
        .child(item(&format!("Sm {} 高", gg.button_sm_h), variants(ButtonSize::Sm, "bs", cx)))
        .child(item("Xs 24 高（text-xs，图标 12）", variants(ButtonSize::Xs, "bxs", cx)))
        .child(item("图标按钮 · icon-lg 40 / icon 36 / icon-sm 32 / 工具栏 28 / icon-xs 24", div().flex().items_center().gap(px(8.))
            .children([ButtonSize::Lg, ButtonSize::Default, ButtonSize::Sm, ButtonSize::Tool, ButtonSize::Xs].into_iter().enumerate().map(|(i, sz)| Button::outline(("bis", i)).sized(sz).icon("plus").on_click(says(cx, "图标按钮"))))))
        .child(row()
            .child(item(&format!("图标按钮 · 工具栏 {} 高", gg.tool_h), div().flex().gap(px(4.)).child(Button::ghost("bi0").icon("undo").on_click(says(cx, "撤销"))).child(Button::ghost("bi1").icon("redo").on_click(says(cx, "重做")))
                .child(Button::ghost("bi2").icon("search").on_click(says(cx, "搜索"))).child(Button::ghost("bi3").icon("ellipsis").on_click(says(cx, "更多")))))
            .child(item("小图标按钮 24（翻页、✕）", div().flex().gap(px(4.)).child(Button::ghost("bx-p").icon("chevron-left").sized(ButtonSize::Xs).on_click(says(cx, "上一个")))
                .child(Button::ghost("bx-n").icon("chevron-right").sized(ButtonSize::Xs).on_click(says(cx, "下一个"))).child(close_btn("bx-x").on_click(says(cx, "关闭")))))
            .child(item("工具按钮（点一下加一）", tool_btn("b-t", "text-search", n).on_click(tap(cx, |g, _, _| { let n = g.at(("tool", 0)); g.pick.insert(("tool", 0), (n + 1) % 4); }))))
            .child(item("在用 / 不在用（点一下切换）", Button::ghost("b-sel").icon("view-settings").label("字段配置").selected(g.is(("bsel", 0))).on_click(tap(cx, |g, _, _| g.flip(("bsel", 0))))))
            .child(item("带下拉", Button::outline("bdd").label("排序").icon_end("chevron-down").on_click(says(cx, "排序")))));
    let ghost_hover = if crate::theme::dark() { color::muted_half() } else { color::muted() };
    let still = row()
        .child(item("悬停（主要 / 描边 / 次要 / 文字 / 危险）", div().flex().gap(px(8.)).child(Button::primary("bh0").label("确定").bg(color::primary_hover()))
            .child(Button::outline("bh1").label("取消").bg(if crate::theme::dark() { color::field_hover() } else { color::muted() })).child(Button::secondary("bh6").label("次要").bg(color::secondary_hover()))
            .child(Button::ghost("bh2").icon("plus").label("添加").bg(ghost_hover)).child(Button::destructive("bh7").label("删除").bg(color::destructive_hover()))))
        .child(item("文字按钮 · 悬停 / 在用（Toggle 按下、菜单展开）", div().flex().gap(px(8.)).child(Button::ghost("bh4").icon("view-settings").label("字段配置").bg(ghost_hover))
            .child(Button::ghost("bh5").icon("view-settings").label("字段配置").selected(true)).child(Button::outline("bh8").label("排序").icon_end("chevron-down").selected(true))))
        .child(item("弹层上的文字按钮 · 悬停 / 在用（深色提一档）", Popover::new("bpop").padded().flex_row().child(Button::ghost("bp0").icon("view-settings").label("悬停")
            .bg(if crate::theme::dark() { color::accent_hover() } else { color::muted() })).child(Button::ghost("bp1").icon("view-settings").label("在用").selected(true))))
        .child(item("不可用", div().flex().gap(px(8.)).child(Button::primary("bx0").label("确定").disabled(true)).child(Button::outline("bx1").label("取消").disabled(true)).child(Button::ghost("bx2").icon("plus").label("添加").disabled(true))))
        .child(item("工具按钮 · 在用", tool_btn("s-t", "sort-down", 2)));
    two(live, still)
}

// ---- 输入 ----

pub fn fields(g: &mut Gallery, w: &mut Window, cx: &mut Cx) -> Div {
    let gg = geo();
    let k = ("type", 0);
    let (glyph, name) = TYPES[g.at(k)];
    let trigger = Field::select(k).open(g.open == Some(k)).w(px(220.)).child(icon(glyph, 16., color::muted_foreground())).child(t(name));
    let menu = choices(g, k, &TYPES, 220., cx);
    let typed = g.inputs.name.read(cx).text.chars().count();
    let area = text_in(&g.inputs.area, Textarea::new(box_id(&g.inputs.area)).focused(typing(&g.inputs.area, w, cx)), cx).w(px(300.)).h(px(76.));
    let lab = |s: &str, el: AnyElement| div().flex().flex_col().gap(px(gg.label_gap)).child(Label::new(s.to_string())).child(el);
    let live = row()
        .child(item(&format!("Field（{typed} 个字）"), field_in(&g.inputs.name, w, cx).w(px(220.))))
        .child(item(&format!("Field · Sm {} 高（搜索框）", gg.field_sm_h), small_in(&g.inputs.search, "search", w, cx).w(px(220.))))
        .child(item("Select", drop(g, k, trigger, menu, cx)))
        .child(item("小选择框（字段）", field_select(g, ("fld", 0), 148., cx)))
        .child(item("小选择框（值）", small_select(g, ("op", 0), &OPS, 108., cx)))
        .child(item("Textarea（公式）", area))
        .child(item("Label + Field", lab("文档标题", field_in(&g.inputs.rename, w, cx).into_any_element()).w(px(220.))));
    let still = row()
        .child(item("空", Field::new("f0").w(px(220.)).child(faint("请输入标题"))))
        .child(item("有值", Field::new("f1").w(px(220.)).child(t("Reading log"))))
        .child(item("输入中", Field::new("f2").focused(true).w(px(220.)).child(t("Reading log")).child(div().w(px(1.)).h(px(16.)).bg(color::foreground()))))
        .child(item("不可用", Field::new("f3").disabled(true).w(px(220.)).child("Reading log")))
        .child(item("出错（aria-invalid）", Field::new("f4").invalid(true).w(px(220.)).child(t("Reading log"))))
        .child(item("Select · 不可用", Field::select("f7").disabled(true).w(px(220.)).child(t("文本"))))
        .child(item("Textarea · 输入中 / 不可用", div().flex().gap(px(8.)).child(Textarea::new("ta1").focused(true).w(px(160.)).child(t("=SUM(A)"))).child(Textarea::new("ta2").disabled(true).w(px(160.)).child(t("=SUM(A)")))))
        .child(item("Label · 说明文字", div().flex().flex_col().gap(px(gg.label_gap)).child(Label::new("文档标题")).child(Field::new("f8").w(px(220.)).child(t("周报"))).child(Label::new("在侧栏和标签页里显示").muted())))
        .child(item("Select · 展开", Field::select("f6").open(true).w(px(220.)).child(icon("ft-text", 16., color::muted_foreground())).child(t("文本"))))
        .child(item("小选择框（选项）", pick_box("s-pt", div().flex().items_center().gap(px(4.)).child(tag("Reading", 0)).child(tag("Next", 9))).w(px(200.))));
    two(live, still)
}

// ---- 选择控件 ----

pub fn toggles(g: &mut Gallery, _w: &mut Window, cx: &mut Cx) -> Div {
    let live = row()
        .child(item("Checkbox", div().flex().gap(px(16.)).child(check(("chk", 0), cx, g.is(("chk", 0)))).child(check(("chk", 1), cx, g.is(("chk", 1))))))
        .child(item("带文字", div().id("chk-l").flex().items_center().gap(px(8.)).cursor_pointer().on_click(tap(cx, |g, _, _| g.flip(("chk", 2)))).child(checkbox(g.is(("chk", 2)))).child(t("允许多选"))))
        .child(item("Switch", div().flex().gap(px(16.)).child(toggle(("sw", 0), cx, g.is(("sw", 0)), true)).child(toggle(("sw", 1), cx, g.is(("sw", 1)), true))))
        .child(item("Segmented（两段）", segs(g, ("seg", 0), &["A → Z", "Z → A"], cx).w(px(200.))))
        .child(item("Segmented（四段）", segs(g, ("seg", 1), &["低", "中等", "高", "超高"], cx).w(px(260.))))
        .child(item("Segmented（图标）", Segmented::new()
            .child(Segment::new("seg-ic-0", "").icon("view-grid").selected(g.at(("seg-ic", 0)) == 0).on_click(tap(cx, |g, _, _| { g.pick.insert(("seg-ic", 0), 0); })))
            .child(Segment::new("seg-ic-1", "").icon("view-list").selected(g.at(("seg-ic", 0)) == 1).on_click(tap(cx, |g, _, _| { g.pick.insert(("seg-ic", 0), 1); })))))
        .child(item("Radio", div().flex().gap(px(16.)).children((0..2).map(|i| div().id(("rd", i)).flex().items_center().gap(px(8.)).cursor_pointer()
            .on_click(tap(cx, move |g, _, _| { g.pick.insert(("rd", 0), i); })).child(Radio::new(g.at(("rd", 0)) == i)).child(t(["仅文本", "所有内容"][i]))))))
        .child(item("颜色（选项、填色）", swatches(g, ("sw-c", 0), 10, 22., cx).w(px(200.))));
    let still = row()
        .child(item("Checkbox · 关 / 开 / 部分 / 不可用", div().flex().gap(px(16.)).child(checkbox(false)).child(checkbox(true)).child(checkbox(false).partial(true)).child(checkbox(true).disabled(true))))
        .child(item("Radio · 关 / 开 / 不可用", div().flex().gap(px(16.)).child(Radio::new(false)).child(Radio::new(true)).child(Radio::new(true).disabled(true))))
        .child(item("Switch · 关 / 开 / 不可用", div().flex().gap(px(16.)).child(switch(false, true)).child(switch(true, true)).child(switch(true, false))))
        .child(item("Segmented · 一段不可用", Segmented::new().w(px(260.)).child(Segment::new("sgd0", "低").selected(true)).child(Segment::new("sgd1", "中等")).child(Segment::new("sgd2", "高").disabled(true))));
    two(live, still)
}

// ---- 标签 ----

pub fn badges(g: &mut Gallery, _w: &mut Window, cx: &mut Cx) -> Div {
    let mut tags = div().flex().flex_wrap().gap(px(6.)).w(px(320.));
    for (slot, name) in ["蓝", "绿", "黄", "橙", "红", "粉", "紫", "青", "天蓝", "灰"].iter().enumerate() {
        let k = ("tag", slot);
        let s = (slot + g.at(k)) % 20;
        tags = tags.child(div().id(k).cursor_pointer().child(tag(name, s as u8)).on_click(tap(cx, move |g, _, _| { let n = g.at(k); g.pick.insert(k, n + 10); })));
    }
    const CHIPS: [&str; 3] = ["Status 是 Reading", "Pages > 300", "Author 不为空"];
    let mut chips = div().flex().flex_wrap().gap(px(6.)).w(px(360.));
    for &i in g.rows["chips"].iter() {
        chips = chips.child(div().id(("cx", i)).cursor_pointer().child(Badge::new(CHIPS[i]).closable())
            .on_click(tap(cx, move |g, _, cx| { g.rows.get_mut("chips").unwrap().retain(|&c| c != i); g.say("去掉了一个筛选条件", cx); })));
    }
    if g.rows["chips"].is_empty() { chips = chips.child(ghost_btn("cx-reset", "undo", "恢复").on_click(tap(cx, |g, _, _| { g.rows.insert("chips", vec![0, 1, 2]); }))); }
    let live = row().child(item("Badge · 选项颜色（点一下换成深一档）", tags)).child(item("Badge · 可删除（点一下去掉）", chips));
    let still = row()
        .child(item("Badge · secondary / default / outline / destructive", div().flex().gap(px(8.)).child(Badge::new("草稿")).child(Badge::primary("新")).child(Badge::outline("只读")).child(Badge::destructive("过期"))))
        .child(item("关联 / 人员", div().flex().gap(px(8.)).child(link_chip("Objectivity")).child(person("Okafor", 0)).child(person("Lin", 4))))
        .child(item("Kbd", div().flex().gap(px(4.)).child(Kbd::new("⌘")).child(Kbd::new("⇧")).child(Kbd::new("F"))))
        .child(item("KbdGroup · 连按", crate::design::KbdGroup::new(vec![vec!["⌘".into(), "K".into()], vec!["⌘".into(), "S".into()]])));
    div().flex().flex_col().gap(px(32.)).child(group("可操作", live)).child(group("只读", still))
}

// ---- 菜单 ----

/// The 字段菜单's lines: glyph, label, danger; "" a rule.
const FIELD_MENU: [(&str, &str, bool); 11] = [("pen-line", "修改字段/列", false), ("copy", "复制字段/列", false), ("indent-decrease", "向左插入字段/列", false), ("indent-increase", "向右插入字段/列", false), ("", "", false),
    ("sort-down", "按 A 到 Z 排序", false), ("text-search", "按此字段筛选", false), ("view-list", "按此字段分组", false), ("", "", false), ("eye", "隐藏字段", false), ("bin", "删除字段", true)];

/// A menu line that says what it was and closes its menu.
fn act(id: impl Into<ElementId>, label: &'static str, cx: &mut Cx) -> MenuItem {
    MenuItem::new(id).label(label).on_click(tap(cx, move |g, _, cx| { g.open = None; g.say(format!("点了「{label}」"), cx); }))
}

fn field_menu(id: &'static str, hovered: Option<usize>, cx: &mut Cx) -> Popover {
    let mut m = Popover::new(id).menu().occlude().w(px(232.));
    for (i, (glyph, label, danger)) in FIELD_MENU.iter().enumerate() {
        if label.is_empty() { m = m.child(menu_rule()); continue; }
        m = m.child(act((id, i), label, cx).icon(glyph.to_string()).destructive(*danger).selected(hovered == Some(i)));
    }
    m
}

/// The 单选's option list: a search that narrows it, the options, 新建选项 with what was typed.
fn option_list(g: &Gallery, k: K, w: &Window, cx: &mut Cx) -> Popover {
    let q = g.inputs.menu_search.read(cx).text.clone();
    let mut m = Popover::new(("opts", k.1)).occlude().w(px(248.)).py(px(6.)).flex().flex_col()
        .child(div().px(px(8.)).pb(px(6.)).child(small_in(&g.inputs.menu_search, "search", w, cx)));
    for (i, (name, slot)) in OPTIONS.iter().enumerate() {
        if !q.is_empty() && !name.to_lowercase().contains(&q.to_lowercase()) { continue; }
        m = m.child(list_row((k.0, 200 + i)).child(div().flex_1().flex().child(tag(name, *slot))).when(g.at(k) == i, |d| d.child(icon("check", 14., color::foreground())))
            .on_click(tap(cx, move |g, _, _| { g.pick.insert(k, i); g.open = None; })));
    }
    if !q.is_empty() && !OPTIONS.iter().any(|o| o.0.eq_ignore_ascii_case(&q)) {
        m = m.child(list_row("ol-new").child(icon("plus", 14., color::muted_foreground())).child(div().text_color(color::muted_foreground()).child("新建选项")).child(tag(&q, 4))
            .on_click(tap(cx, move |g, _, cx| { g.open = None; g.say("新建了一个选项", cx); })));
    }
    m
}

pub fn menus(g: &mut Gallery, w: &mut Window, cx: &mut Cx) -> Div {
    let kind = |k: &str| img(format!("illus/kinds/{k}.svg")).flex_none().size(px(18.));
    // the app's menus
    let doc = Popover::new("am-doc").menu().occlude().w(px(220.))
        .child(act("d1", "页面宽度", cx).icon("page-width").submenu(true))
        .child(act("d2", "查找和替换", cx).icon("search").shortcut("⌘F"))
        .child(MenuSeparator::new())
        .child(MenuItem::new("d3").icon("pin").label("置顶").checked(g.is(("pin", 0))).on_click(tap(cx, |g, _, _| { g.flip(("pin", 0)); g.open = None; })))
        .child(act("d5", "创建副本", cx).icon("copy").shortcut("⌘D"))
        .child(MenuSeparator::new())
        .child(act("d6", "下载为", cx).icon("download").submenu(true))
        .child(MenuItem::new("d7").icon("info").label("文档信息").disabled(true))
        .child(MenuSeparator::new())
        .child(act("d8", "删除", cx).icon("bin").destructive(true));
    let new = Popover::new("am-new").menu().occlude().w(px(200.))
        .child(act("n1", "文档", cx).leading(kind("doc"))).child(act("n2", "画板", cx).leading(kind("board"))).child(act("n3", "多维表格", cx).leading(kind("datatable")))
        .child(MenuSeparator::new()).child(act("n4", "文件夹", cx).icon("folder"));
    let mut sort = Popover::new("am-sort").menu().occlude().w(px(200.)).child(MenuLabel::new("排序方式"));
    for (i, s) in ["最近查看", "最近修改", "名称"].into_iter().enumerate() {
        sort = sort.child(MenuItem::new(("s", i)).label(s).checked(g.at(("sortby", 0)) == i).on_click(tap(cx, move |g, _, _| { g.pick.insert(("sortby", 0), i); })));
    }
    let sort = sort.child(MenuSeparator::new()).child(MenuItem::new("s4").label("文件夹优先").checked(g.is(("sort", 3))).on_click(tap(cx, |g, _, _| g.flip(("sort", 3)))));
    let mut modes = Popover::new("am-modes").menu().occlude().w(px(260.));
    for (i, (glyph, s, d)) in [("pen-line", "编辑", "随时修改文档内容"), ("eye", "阅读", "只看，不会误改")].into_iter().enumerate() {
        modes = modes.child(MenuItem::new(("md", i)).icon(glyph).label(s).description(d).checked(g.at(("mode", 0)) == i).on_click(tap(cx, move |g, _, _| { g.pick.insert(("mode", 0), i); g.open = None; })));
    }
    let mode = ["编辑", "阅读"][g.at(("mode", 0))];
    let app = row()
        .child(item("「…」菜单（子菜单、快捷键、勾选、不可用、危险）", drop(g, ("doc", 0), Button::ghost("doc-b").icon("ellipsis"), doc, cx)))
        .child(item("新建菜单（插画图标）", drop(g, ("new", 0), Button::outline("new-b").icon("plus").label("新建").icon_end("chevron-down"), new, cx)))
        .child(item("排序菜单（标题、勾选，选了不收起）", drop(g, ("sortm", 0), Button::ghost("sort-b").icon("sort-down").label("排序"), sort, cx)))
        .child(item("模式菜单（两行）", drop(g, ("modem", 0), Button::ghost("mode-b").label(mode).icon_end("chevron-down"), modes, cx)));
    // the 数据表's
    let k = ("opt", 0);
    let (name, slot) = OPTIONS[g.at(k)];
    let fm = field_menu("fm", None, cx);
    let opts = option_list(g, k, w, cx);
    let more = Popover::new("more").menu().occlude().w(px(220.)).child(menu_title("视图"))
        .child(act("mm1", "导出为 CSV", cx).icon("export")).child(act("mm2", "复制为 Markdown 表格", cx).icon("copy"));
    let table = row()
        .child(item("字段菜单", drop(g, ("fm", 0), Button::outline("fm-b").label("字段菜单").icon_end("chevron-down"), fm, cx)))
        .child(item("单选的选项列表（可以搜）", drop(g, k, pick_box("opt-b", div().flex().child(tag(name, slot))).open(g.open == Some(k)).w(px(200.)), opts, cx)))
        .child(item("下拉选择", small_select(g, ("op", 1), &OPS, 140., cx)))
        .child(item("「…」菜单", drop(g, ("more", 0), Button::ghost("more-b").icon("ellipsis"), more, cx)));
    let mut dropl = Popover::new("g-dl").menu().w(px(160.));
    for (i, s) in OPS.iter().enumerate() { dropl = dropl.child(list_row(("sdl", i)).selected(i == 4).child(div().flex_1().text_color(color::foreground()).child(s.to_string())).when(i == 2, |d| d.child(icon("check", 14., color::foreground())))); }
    // a submenu, as Radix: open on its row, shut when the pointer reaches a sibling; its row lit while it is open
    let sub_k = ("sub", 0);
    let sub_open = g.is(sub_k);
    let mut sub = Popover::new("s-sub").menu().w(px(160.));
    for (i, s) in ["标准", "较宽", "全宽"].into_iter().enumerate() {
        sub = sub.child(MenuItem::new(("sw", i)).label(s).checked(g.at(("width", 0)) == i).on_click(tap(cx, move |g, _, cx| { g.pick.insert(("width", 0), i); g.on.remove(&("sub", 0)); g.say(format!("页面宽度：{s}"), cx); })));
    }
    let doc_live = Popover::new("s-doc").menu().w(px(220.))
        .child(MenuItem::new("sd1").icon("page-width").label("页面宽度").submenu(true).open(sub_open)
            .on_hover(cx.listener(move |g, on: &bool, _, cx| if *on && !g.is(sub_k) { g.on.insert(sub_k); cx.notify(); }))
            .on_click(tap(cx, move |g, _, _| g.flip(sub_k)))
            .when(sub_open, |d| d.child(div().absolute().left_full().top(px(-crate::design::menu::PAD)).child(deferred(anchored().snap_to_window_with_margin(px(8.)).child(div().pl(px(4.)).child(sub))).with_priority(1)))))
        .child(MenuItem::new("sd2").icon("search").label("查找和替换").shortcut("⌘F").on_hover(shut(cx)).on_click(says(cx, "查找和替换")))
        .child(MenuItem::new("sd5").icon("copy").label("创建副本").shortcut("⌘D").on_hover(shut(cx)).on_click(says(cx, "创建副本")))
        .child(MenuItem::new("sd7").icon("info").label("文档信息").disabled(true).on_hover(shut(cx)))
        .child(MenuItem::new("sd8").icon("bin").label("删除").destructive(true).on_hover(shut(cx)).on_click(says(cx, "删除")));
    let lit = Popover::new("s-lit").menu().w(px(220.)).child(MenuLabel::new("我的账户"))
        .child(MenuItem::new("sl1").icon("pen-line").label("重命名").shortcut("⌘R").selected(true))
        .child(MenuItem::new("sl2").inset().label("缩进的一项"))
        .child(MenuItem::new("sl3").label("显示网格").checked(true))
        .child(MenuSeparator::new())
        .child(MenuItem::new("sl4").icon("bin").label("删除").destructive(true).selected(false));
    let lit_d = Popover::new("s-litd").menu().w(px(220.))
        .child(MenuItem::new("sd-1").icon("copy").label("创建副本").shortcut("⌘D"))
        .child(MenuItem::new("sd-2").icon("bin").label("删除").shortcut("⌫").destructive(true).selected(true));
    let still = row().child(item("键盘行亮着（标题、快捷键、缩进、勾选）", lit)).child(item("危险项亮着", lit_d)).child(item("子菜单（悬停「页面宽度」打开，点一下收起）：开着时父行一直亮着；指针移到兄弟行上子菜单收起（Radix）", doc_live).max_w(px(300.))).child(item("字段菜单 · 第二项悬停", field_menu("sfm", Some(1), cx))).child(item("下拉选择 · 第五项悬停", dropl));
    div().flex().flex_col().gap(px(32.)).child(group("可操作 · 应用", app)).child(group("可操作 · 数据表", table)).child(group("子菜单和状态对照", still))
}

// ---- 对话框 ----

/// The field form's body: title, type (its list unfolds in place), options, 允许多选.
fn form_body(g: &Gallery, w: &Window, cx: &mut Cx) -> Div {
    let gg = geo();
    let lab = |s: &str, el: AnyElement| div().flex().flex_col().gap(px(gg.label_gap)).child(field_label(s.to_string())).child(el);
    let k = ("type", 1);
    let unfolded = g.is(("types-open", 0));
    let (glyph, name) = TYPES[g.at(k)];
    let mut types = inset_list("ft-list").child(menu_title("基础"));
    for (i, (glyph, name)) in TYPES.iter().enumerate() {
        types = types.child(menu_row(("ftl", i)).icon(glyph.to_string()).label(name.to_string()).checked(g.at(k) == i)
            .on_click(tap(cx, move |g, _, _| { g.pick.insert(k, i); g.on.remove(&("types-open", 0)); })));
    }
    let trigger = Field::select("fo-t").open(unfolded).child(icon(glyph, 16., color::muted_foreground())).child(t(name)).on_click(tap(cx, |g, _, _| g.flip(("types-open", 0))));
    let mut opts = div().flex().flex_col().gap(px(6.));
    for (n, &slot) in g.rows["opts"].iter().enumerate() {
        let label = OPTIONS.iter().find(|o| o.1 as usize == slot).map_or("新选项", |o| o.0);
        opts = opts.child(div().flex().items_center().gap(px(8.))
            .child(swatch(("fo-c", n), tag_colors(slot as u8).0, 20., false).on_click(tap(cx, move |g, _, _| { let v = g.rows.get_mut("opts").unwrap(); v[n] = (v[n] + 1) % 10; })))
            .child(input_box(("fo-i", n)).flex_1().child(t(label)))
            .child(close_btn(("fo-x", n)).on_click(tap(cx, move |g, _, _| { g.rows.get_mut("opts").unwrap().remove(n); }))));
    }
    let opts = opts.child(div().flex().child(ghost_btn("fo-add", "plus", "添加选项").on_click(tap(cx, |g, _, _| { let v = g.rows.get_mut("opts").unwrap(); let next = (v.last().copied().unwrap_or(0) + 1) % 10; v.push(next); }))));
    div().flex().flex_col().gap(px(gg.popover_gap))
        .child(lab("标题", field_in(&g.inputs.title, w, cx).into_any_element()))
        .child(lab("字段类型", div().flex().flex_col().gap(px(6.)).child(trigger).when(unfolded, |d| d.child(types)).into_any_element()))
        .when(matches!(g.at(k), 1 | 2), |d| d.child(lab("选项内容", opts.into_any_element())))
        .child(div().flex().items_center().justify_between().child(t("允许多选")).child(toggle(("multi", 0), cx, g.is(("multi", 0)), true)))
}
fn form_footer(cx: &mut Cx) -> Div {
    div().flex().justify_end().gap(px(8.))
        .child(plain_btn("fo-c", "取消").on_click(tap(cx, |g, _, cx| { g.dialog = None; g.say("取消了", cx); })))
        .child(primary_btn("fo-o", "确定").on_click(tap(cx, |g, _, cx| { g.dialog = None; let s = g.inputs.title.read(cx).text.clone(); g.say(format!("保存了字段「{s}」"), cx); })))
}

/// The open dialog over a scrim: a click on the scrim, 取消, 确定 or Esc closes it.
pub fn dialog(g: &mut Gallery, which: &'static str, w: &mut Window, cx: &mut Cx) -> Div {
    let close = |id: &'static str, label: &'static str, cx: &mut Cx| Button::outline(id).label(label).on_click(tap(cx, |g, _, _| g.dialog = None));
    let card = match which {
        "delete" => Dialog::alert("dg1").occlude().title("删除「周报」？").description("文档会移到回收站，30 天内可以恢复。")
            .footer(div().flex().justify_end().gap(px(8.)).child(close("dg1-n", "取消", cx)).child(Button::destructive("dg1-y").label("删除").on_click(tap(cx, |g, _, cx| { g.dialog = None; g.say("删了（假的）", cx); })))),
        "rename" => Dialog::new("dg2").occlude().title("重命名").closable(tap(cx, |g, _, _| g.dialog = None)).child(field_in(&g.inputs.rename, w, cx))
            .footer(div().flex().justify_end().gap(px(8.)).child(close("dg2-n", "取消", cx)).child(Button::primary("dg2-y").label("确定").on_click(tap(cx, |g, _, cx| { g.dialog = None; let s = g.inputs.rename.read(cx).text.clone(); g.say(format!("改名为「{s}」"), cx); })))),
        _ => Dialog::new("dg3").occlude().w(px(400.)).title("新增字段").closable(tap(cx, |g, _, _| g.dialog = None)).child(form_body(g, w, cx)).footer(form_footer(cx)),
    };
    div().absolute().inset_0().flex().items_center().justify_center()
        .child(div().id("scrim").absolute().inset_0().bg(color::overlay()).occlude().on_click(tap(cx, |g, _, _| g.dialog = None)))
        .child(card)
}

pub fn dialogs(g: &mut Gallery, w: &mut Window, cx: &mut Cx) -> Div {
    let open = |b: Button, which: &'static str, cx: &mut Cx| b.on_click(tap(cx, move |g, _, _| g.dialog = Some(which)));
    let live = row()
        .child(item("确认（危险）", open(Button::destructive("o1").label("删除文档…"), "delete", cx)))
        .child(item("表单（重命名）", open(Button::outline("o2").label("重命名…"), "rename", cx)))
        .child(item("数据表 · 新增字段", open(Button::primary("o3").label("新增字段…"), "field", cx)))
        // one input draws in one place: while a dialog holds the form, it is not drawn here too
        .child(item("表单弹窗本身（类型能展开、选项能加减、色块点一下换）", if g.dialog.is_some() { small("（在弹窗里）").into_any_element() } else {
            Popover::new("form").padded().w(px(340.)).child(form_body(g, w, cx)).child(form_footer(cx)).into_any_element() }));
    let still = row()
        .child(item("AlertDialog · 确认（不带 ×）", Dialog::alert("s-dg1").title("删除「周报」？").description("文档会移到回收站，30 天内可以恢复。")
            .footer(div().flex().justify_end().gap(px(8.)).child(Button::outline("s-dg1-n").label("取消")).child(Button::destructive("s-dg1-y").label("删除")))))
        .child(item("Dialog · 表单（输入中，带 ×）", Dialog::new("s-dg2").w(px(420.)).title("重命名").closable(|_, _, _| {}).child(Field::new("s-dg2-f").focused(true).child(t("周报 · 第 47 周")))
            .footer(div().flex().justify_end().gap(px(8.)).child(Button::outline("s-dg2-n").label("取消")).child(Button::primary("s-dg2-y").label("确定")))));
    two(live, still)
}

// ---- 卡片 ----

pub fn cards(g: &mut Gallery, _w: &mut Window, cx: &mut Cx) -> Div {
    let line = |k: K, title: &'static str, about: &'static str, cx: &mut Cx| Item::new(title).description(Some(about))
        .child(div().id(k).cursor_pointer().on_click(tap(cx, move |g, _, _| g.flip(k))).child(Switch::new(g.is(k))));
    let card = Card::new("card-live").w(px(420.))
        .child(line(("card-sw", 0), "拼写检查", "输入时在拼错的词下画波浪线", cx)).child(Separator::new())
        .child(line(("card-sw", 1), "自动配对括号", "输入左括号时补上右括号", cx));
    let toasts = div().flex().gap(px(8.))
        .child(Button::outline("toast-ok").label("成功").on_click(says(cx, "已复制为 Markdown")))
        .child(Button::outline("toast-err").label("出错").on_click(says(cx, "导入未能完成")));
    let live = row().child(item("Card + Item（设置分组，开关能点）", card)).child(item("Toast（在面板底部提示）", toasts));
    let still = row()
        .child(item("Separator · 横 / 竖", div().w(px(200.)).flex().flex_col().gap(px(12.)).child(t("上")).child(Separator::new()).child(div().h(px(20.)).flex().gap(px(12.)).child(t("左")).child(Separator::new().vertical()).child(t("右")))))
        .child(item("Toast · 成功 / 出错 / 提示", div().flex().flex_col().gap(px(8.)).child(Toast::new(ToastTone::Success, "已复制为 Markdown"))
            .child(Toast::new(ToastTone::Error, "导入未能完成")).child(Toast::new(ToastTone::Info, "正在导入…"))));
    two(live, still)
}

// ---- 面板 ----

pub fn panels(g: &mut Gallery, w: &mut Window, cx: &mut Cx) -> Div {
    let gg = geo();
    let mut filter = Popover::new("p-filter").padded().gap(px(gg.panel_gap)).w(px(560.))
        .child(panel_title("设置筛选条件"))
        .child(div().flex().items_center().gap(px(8.)).text_size(px(text_sm())).text_color(color::muted_foreground()).child("符合以下").child(small_select(g, ("all", 0), &["所有", "任一"], 76., cx)).child("条件"));
    for (n, &c) in g.rows["filter"].clone().iter().enumerate() {
        let value = if n == 1 && c == 1 { small_in(&g.inputs.value, "", w, cx).flex_1().into_any_element() } else { pick_box(("pf-v", c), div().flex().gap(px(4.)).child(tag("Reading", 0)).child(tag("Next", 9))).flex_1().into_any_element() };
        filter = filter.child(div().flex().items_center().gap(px(8.)).child(field_select(g, ("pf-f", c), 148., cx)).child(small_select(g, ("pf-o", c), &OPS, 108., cx))
            .child(div().flex_1().flex().child(value)).child(close_btn(("pf-x", c)).on_click(tap(cx, move |g, _, _| { g.rows.get_mut("filter").unwrap().retain(|&x| x != c); }))));
    }
    let filter = filter.child(div().flex().child(ghost_btn("pf-add", "plus", "添加条件").on_click(tap(cx, |g, _, _| { let v = g.rows.get_mut("filter").unwrap(); let n = v.iter().max().map_or(0, |m| m + 1); v.push(n); g.pick.insert(("pf-f", n), 2); }))));
    let mut sort = Popover::new("p-sort").padded().gap(px(gg.panel_gap)).w(px(400.)).child(panel_title("设置排序条件"));
    for &c in g.rows["sort"].clone().iter() {
        sort = sort.child(div().flex().items_center().gap(px(8.)).child(field_select(g, ("ps-f", c), 140., cx)).child(segs(g, ("ps-s", c), &["A → Z", "Z → A"], cx).flex_1())
            .child(close_btn(("ps-x", c)).on_click(tap(cx, move |g, _, _| { g.rows.get_mut("sort").unwrap().retain(|&x| x != c); }))));
    }
    let sort = sort.child(div().flex().child(ghost_btn("ps-add", "plus", "添加排序").on_click(tap(cx, |g, _, _| { let v = g.rows.get_mut("sort").unwrap(); let n = v.iter().max().map_or(0, |m| m + 1); v.push(n); }))));
    let mut list = Popover::new("p-fl").menu().w(px(280.))
        .child(div().h(px(36.)).px(px(12.)).mb(px(4.)).flex().items_center().gap(px(6.)).border_b_1().border_color(color::border()).child(icon("search", 14., color::placeholder())).child(faint("搜索字段")));
    for (i, (glyph, name)) in FIELDS.iter().enumerate() {
        let first = i == 0;
        let shown = first || g.is(("fl", i));
        let fg = if shown { color::foreground() } else { color::placeholder() };
        list = list.child(list_row(("pl", i)).h(px(34.)).gap(px(6.))
            .when(!first, |d| d.child(icon("grip-vertical", 14., color::placeholder())))
            .child(icon(glyph, 15., if shown { color::muted_foreground() } else { color::placeholder() }))
            .child(div().text_color(fg).child(name.to_string()))
            .when(first, |d| d.child(icon("ft-lock", 14., color::muted_foreground())))
            .child(div().flex_1())
            .when(!first, |d| d.child(toggle(("fl", i), cx, shown, true))));
        if first { list = list.child(menu_rule()); }
    }
    let list = list.child(menu_rule()).child(div().px(px(6.)).flex().gap(px(4.))
            .child(ghost_btn("pl-a", "", "全部显示").on_click(tap(cx, |g, _, _| for i in 1..5 { g.on.insert(("fl", i)); })))
            .child(ghost_btn("pl-n", "", "全部隐藏").on_click(tap(cx, |g, _, _| for i in 1..5 { g.on.remove(&("fl", i)); }))))
        .child(menu_rule()).child(menu_item("pl-add", "plus", "新增字段", false).on_click(says(cx, "新增字段")));
    let pk = g.at(("pk", 0));
    let color_panel = Popover::new("p-color").padded().gap(px(gg.panel_gap)).w(px(560.))
        .child(panel_title("设置填色条件"))
        .child(div().flex().items_center().gap(px(8.)).child(swatch("pk-s", tag_colors(pk as u8).0, gg.field_sm_h, true)).child(field_select(g, ("pk-f", 0), 120., cx)).child(small_select(g, ("pk-o", 0), &OPS, 108., cx))
            .child(pick_box("pk-v", div().flex().child(tag("Set aside", 3))).flex_1()).child(close_btn("pk-x").on_click(says(cx, "删除条件"))))
        .child(swatches(g, ("pk", 0), 10, 22., cx))
        .child(div().flex().child(ghost_btn("pk-add", "plus", "添加条件").on_click(says(cx, "添加条件"))));
    let live = row().child(item("筛选（条件能加能删，字段、运算符能选，值能打字）", filter)).child(item("排序", sort)).child(item("字段配置（开关显示隐藏）", list)).child(item("填色（点色块换颜色）", color_panel));
    div().flex().flex_col().gap(px(32.)).child(group("可操作", live))
}

// ---- 工具栏 ----

pub fn bars(g: &mut Gallery, _w: &mut Window, cx: &mut Cx) -> Div {
    const TOOLS: [(&str, usize); 6] = [("view-settings", 1), ("text-search", 2), ("view-list", 1), ("sort-down", 2), ("list", 1), ("palette", 1)];
    let mut tools = div().flex().items_center().gap(px(4.)).child(Button::ghost("tb-a").icon("plus").on_click(says(cx, "添加记录")));
    for (i, (glyph, n)) in TOOLS.iter().enumerate() {
        let k = ("tb", i);
        tools = tools.child(tool_btn(k, glyph, if g.is(k) { *n } else { 0 }).on_click(tap(cx, move |g, _, _| g.flip(k))));
    }
    let bar = div().w(px(620.)).h(px(44.)).px(px(12.)).flex().items_center().justify_between().border_b_1().border_color(line())
        .child(tools)
        .child(div().flex().items_center().gap(px(4.)).child(Button::ghost("tb-u").icon("undo").on_click(says(cx, "撤销"))).child(Button::ghost("tb-r").icon("redo").on_click(says(cx, "重做")))
            .child(Button::ghost("tb-f").icon("search").on_click(tap(cx, |g, _, _| { g.on.remove(&("find-off", 0)); }))).child(Button::ghost("tb-m").icon("ellipsis").on_click(says(cx, "更多"))));
    let selbar = if g.is(("sel-off", 0)) { ghost_btn("sb-back", "undo", "再选 3 条").on_click(tap(cx, |g, _, _| { g.on.remove(&("sel-off", 0)); })).into_any_element() } else {
        Popover::new("selbar").h(px(44.)).px(px(12.)).flex().flex_row().items_center().gap(px(8.))
            .child(t("已选 3 条记录")).child(div().w(px(1.)).h(px(16.)).bg(color::border()))
            .child(ghost_btn("sb-1", "copy", "创建副本").on_click(says(cx, "创建副本"))).child(ghost_btn("sb-2", "bin", "删除").on_click(says(cx, "删除")))
            .child(ghost_btn("sb-3", "x", "取消选择").on_click(tap(cx, |g, _, _| { g.on.insert(("sel-off", 0)); }))).into_any_element()
    };
    let at = g.at(("found", 0));
    let find = if g.is(("find-off", 0)) { small("关掉了（点工具栏的搜索再打开）").into_any_element() } else {
        Popover::new("find").h(px(40.)).w(px(320.)).px(px(10.)).flex().flex_row().items_center().gap(px(6.))
            .child(icon("search", 14., color::placeholder())).child(text_in(&g.inputs.find, div().flex_1().min_w_0().flex().items_center(), cx)).child(small(&format!("{}/7", at + 1)))
            .child(Button::ghost("fb-p").icon("chevron-left").sized(ButtonSize::Xs).on_click(tap(cx, |g, _, _| { let n = g.at(("found", 0)); g.pick.insert(("found", 0), (n + 6) % 7); })))
            .child(Button::ghost("fb-n").icon("chevron-right").sized(ButtonSize::Xs).on_click(tap(cx, |g, _, _| { let n = g.at(("found", 0)); g.pick.insert(("found", 0), (n + 1) % 7); })))
            .child(Button::ghost("fb-x").icon("x").sized(ButtonSize::Xs).on_click(tap(cx, |g, _, _| { g.on.insert(("find-off", 0)); }))).into_any_element()
    };
    let live = row().child(item("工具栏（工具点一下切换在用）", bar)).child(item("勾选记录后的操作条", selbar)).child(item("表内搜索（能打字，上一个 / 下一个）", find));
    div().flex().flex_col().gap(px(32.)).child(group("可操作", live))
}

// ---- 表格 ----

const BOOKS: [(&str, &str, u8, &str, usize, f32); 4] = [("Spectacular Bodies", "Martin Kemp", 9, "Next", 4, 0.0), ("Books of the Body", "Andrea Carlino", 0, "Reading", 3, 0.45),
    ("Objectivity", "Lorraine Daston", 1, "Finished", 5, 1.0), ("The Body Emblazoned", "Jonathan Sawday", 3, "Set aside", 2, 0.2)];

pub fn grid(g: &mut Gallery, _w: &mut Window, cx: &mut Cx) -> Div {
    let gg = geo();
    let (rh, hh) = (gg.row_medium, gg.table_head);
    let cols = [("ft-text", "Title", 190.), ("ft-text", "Author", 150.), ("ft-select", "Status", 110.), ("star", "Rating", 100.), ("ft-number", "Progress", 110.)];
    let width: f32 = GUTTER_W + cols.iter().map(|c| c.2).sum::<f32>();
    let recs = g.rows["grid"].clone();
    let n = recs.len();
    let all = n > 0 && (0..n).all(|r| g.is(("rowchk", r)));
    let (r0, r1) = (g.cell.0.min(g.anchor.0), g.cell.0.max(g.anchor.0));
    let (c0, c1) = (g.cell.1.min(g.anchor.1), g.cell.1.max(g.anchor.1));
    let col_sel = g.pick.get(&("col", 0)).copied();
    let mut head = div().flex().h(px(hh)).bg(header_bg()).border_b_1().border_color(line())
        .child(div().id("all").w(px(GUTTER_W)).h_full().flex().items_center().justify_center().border_r_1().border_color(line()).cursor_pointer().child(checkbox(all))
            .on_click(tap(cx, move |g, _, _| for r in 0..n { if all { g.on.remove(&("rowchk", r)); } else { g.on.insert(("rowchk", r)); } })));
    for (i, (glyph, name, w)) in cols.iter().enumerate() {
        let on = col_sel == Some(i + 1);
        head = head.child(div().id(("hd", i)).w(px(*w)).h_full().px(px(cell_pad_x())).flex().items_center().gap(px(4.)).border_r_1().border_color(line()).cursor_pointer()
            .when(on, |d| d.bg(sel_header())).hover(|s| s.bg(color::accent()))
            .on_click(tap(cx, move |g, _, _| { if g.pick.get(&("col", 0)) == Some(&(i + 1)) { g.pick.remove(&("col", 0)); } else { g.pick.insert(("col", 0), i + 1); } }))
            .child(icon(glyph, 16., color::placeholder())).child(div().flex_1().text_size(px(text_sm())).text_color(header_text()).child(name.to_string()))
            .when(on, |d| d.child(icon("chevron-down", 13., color::muted_foreground()))));
    }
    let folded = g.is(("fold", 0));
    let mut body = div().flex().flex_col();
    for (r, &b) in recs.iter().enumerate() {
        if folded && BOOKS[b % 4].3 == "Reading" { continue; }
        let (title, author, slot, status, stars, prog) = BOOKS[b % 4];
        let checked = g.is(("rowchk", r));
        let stars = div().flex().gap(px(2.)).children((1..=5usize).map(|s| icon("star", 14., if s <= stars { star() } else { color::border() })));
        let bar = div().flex().items_center().gap(px(6.)).w_full().child(div().flex_1().h(px(6.)).rounded(px(3.)).bg(color::muted()).child(div().h_full().rounded(px(3.)).bg(color::primary()).w(relative(prog))))
            .child(small(&format!("{}%", (prog * 100.) as u32)));
        let vals: [AnyElement; 5] = [t(title).into_any_element(), t(author).into_any_element(), tag(status, slot).into_any_element(), stars.into_any_element(), bar.into_any_element()];
        let mut line_ = div().flex().h(px(rh)).border_b_1().border_color(line()).when(b == 3, |d| d.bg(row_tint(2)))
            .child(div().id(("gut", r)).w(px(GUTTER_W)).h_full().flex().items_center().justify_center().border_r_1().border_color(line()).cursor_pointer().when(checked, |d| d.bg(sel_bg()))
                .on_click(tap(cx, move |g, _, _| g.flip(("rowchk", r))))
                .child(if checked { checkbox(true).into_any_element() } else { small(&format!("{}", r + 1)).into_any_element() }));
        for (c, v) in vals.into_iter().enumerate() {
            let in_range = (r0..=r1).contains(&r) && (c0..=c1).contains(&c);
            let picked = in_range || checked || col_sel == Some(c + 1);
            line_ = line_.child(div().id(("c", r * 8 + c)).relative().w(px(cols[c].2)).h_full().px(px(cell_pad_x())).flex().items_center().border_r_1().border_color(line()).overflow_hidden()
                .when(picked, |d| d.bg(sel_bg()))
                .on_click(cx.listener(move |g, e: &ClickEvent, _, cx| { g.cell = (r, c); if !e.modifiers().shift { g.anchor = (r, c); } g.pick.remove(&("col", 0)); cx.notify(); }))
                .child(v)
                .when(g.cell == (r, c), |d| d.child(div().absolute().inset_0().border_2().border_color(ring()))));
        }
        body = body.child(line_);
    }
    let add = div().id("add-rec").flex().h(px(gg.add_row_h)).items_center().gap(px(6.)).pl(px(GUTTER_W + cell_pad_x())).text_size(px(text_sm())).text_color(color::muted_foreground()).cursor_pointer().hover(|s| s.bg(color::accent()))
        .on_click(tap(cx, |g, _, _| { let v = g.rows.get_mut("grid").unwrap(); let next = v.len(); v.push(next); }))
        .child(icon("plus", 14., color::muted_foreground())).child("添加记录");
    let footer = div().flex().h(px(gg.footer_h)).items_center().justify_between().px(px(cell_pad_x())).border_t_1().border_color(line()).child(small(&format!("{n} 条记录"))).child(small("平均值 1,240"));
    let table = div().w(px(width)).rounded(px(frame_r())).shadow(crate::design::shadow::frame()).overflow_hidden().bg(color::background())
        .child(head).child(body).child(add).child(footer);
    let groupbar = div().id("group").w(px(width)).h(px(gg.group_h)).px(px(12.)).flex().items_center().gap(px(6.)).rounded(px(gg.tool_r)).bg(color::muted()).cursor_pointer()
        .on_click(tap(cx, |g, _, _| g.flip(("fold", 0))))
        .child(icon("chevron-down", 14., color::muted_foreground()).with_transformation(Transformation::rotate(percentage(if folded { -0.25 } else { 0. })))).child(tag("Reading", 0)).child(small("点一下收起 / 展开 Reading 组"));
    let live = row().child(item(&format!("表头 {hh} · 行高 {rh} · 点格子选中，⇧ 点框选，点表头选列，点行号勾选，第 4 行填色"), div().flex().flex_col().gap(px(12.)).child(groupbar).child(table)));
    div().flex().flex_col().gap(px(32.)).child(group("可操作", live))
}

// ---- 记录详情 ----

pub fn record(g: &mut Gallery, w: &mut Window, cx: &mut Cx) -> Div {
    let gg = geo();
    let at = g.at(("rec", 0));
    let (title, author, ..) = BOOKS[at];
    let prop = |glyph: &str, name: &str, value: AnyElement| div().flex().items_start().gap(px(12.))
        .child(div().w(px(120.)).h(px(gg.field_h)).flex().items_center().gap(px(6.)).child(icon(glyph, 14., color::muted_foreground())).child(div().text_size(px(text_sm())).text_color(color::muted_foreground()).child(name.to_string())))
        .child(div().flex_1().child(value));
    let hov = |id: &'static str| div().id(id).w_full().min_h(px(gg.field_h)).px(px(8.)).flex().items_center().rounded(px(gg.tool_r)).cursor_pointer().hover(|s| s.bg(color::accent()));
    let sk = ("rst", 0);
    let (sname, sslot) = OPTIONS[g.at(sk)];
    let status_menu = option_list(g, sk, w, cx);
    let status = drop(g, sk, hov("r-st").child(tag(sname, sslot)), status_menu, cx);
    let owned = g.is(("own", 0));
    let pages = if typing(&g.inputs.pages, w, cx) { field_in(&g.inputs.pages, w, cx).into_any_element() } else {
        text_in(&g.inputs.pages, div().w_full().h(px(gg.field_h)).px(px(8.)).flex().items_center().rounded(px(gg.tool_r)).hover(|s| s.bg(color::accent())), cx).into_any_element() };
    let panel = Popover::new("rec").w(px(520.)).flex().flex_col()
        .child(div().h(px(48.)).px(px(12.)).flex().items_center().gap(px(4.)).border_b_1().border_color(color::border())
            .child(div().id("r-up").size(px(gg.tool_h)).flex().items_center().justify_center().rounded(px(gg.tool_r)).cursor_pointer().hover(|s| s.bg(color::accent()))
                .child(chevron(true).size(px(16.))).on_click(tap(cx, |g, _, _| { let n = g.at(("rec", 0)); g.pick.insert(("rec", 0), (n + 3) % 4); })))
            .child(Button::ghost("r-dn").icon("chevron-down").on_click(tap(cx, |g, _, _| { let n = g.at(("rec", 0)); g.pick.insert(("rec", 0), (n + 1) % 4); })))
            .child(small(&format!("{}/4", at + 1))).child(div().flex_1()).child(Button::ghost("r-more").icon("ellipsis").on_click(says(cx, "更多"))).child(Button::ghost("r-x").icon("x").on_click(says(cx, "关闭"))))
        .child(div().p(px(20.)).flex().flex_col().gap(px(4.))
            .child(div().pb(px(12.)).text_size(px(20.)).text_color(color::foreground()).child(title))
            .child(prop("ft-text", "Author", hov("r-au").child(t(author)).on_click(says(cx, "Author")).into_any_element()))
            .child(prop("ft-select", "Status", status.into_any_element()))
            .child(prop("ft-number", "Pages", pages))
            .child(prop("ft-date", "Started", hov("r-sd").child(t("2026/11/02")).on_click(says(cx, "Started")).into_any_element()))
            .child(prop("ft-check", "Owned", hov("r-ow").child(checkbox(owned)).on_click(tap(cx, |g, _, _| g.flip(("own", 0)))).into_any_element()))
            .child(prop("ft-person", "Lent to", hov("r-lt").child(person("Okafor", 0)).on_click(says(cx, "Lent to")).into_any_element()))
            .child(div().pt(px(8.)).flex().gap(px(4.)).child(ghost_btn("r-hid", "view-settings", "1 个隐藏字段").on_click(says(cx, "1 个隐藏字段"))).child(ghost_btn("r-add", "plus", "新增字段").on_click(says(cx, "新增字段")))));
    let live = row().child(item("记录详情（上下翻记录，Status 能选，Pages 能改，Owned 能勾）", panel));
    div().flex().flex_col().gap(px(32.)).child(group("可操作", live))
}

// ---- 单元格编辑器 ----

pub fn editors(g: &mut Gallery, w: &mut Window, cx: &mut Cx) -> Div {
    let month = g.at(("month", 0)) as i32; // 0-based, 2026
    // the 1st's weekday (Monday 0) and the month's length, 2026
    let (first, days) = { let m = month + 1; let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4]; let y = if m < 3 { 2025 } else { 2026 };
        let dow = (y + y / 4 - y / 100 + y / 400 + t[(m - 1) as usize] + 1) % 7; ((dow + 6) % 7, [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][(m - 1) as usize]) };
    let picked = g.at(("day", 0)) as i32;
    let mut cal = div().px(px(12.)).flex().flex_col().gap(px(2.))
        .child(div().flex().children(["一", "二", "三", "四", "五", "六", "日"].into_iter().map(|d| div().size(px(32.)).flex().items_center().justify_center().text_size(px(text_xs())).text_color(color::placeholder()).child(d))));
    for week in 0..((first + days + 6) / 7) {
        let mut r = div().flex();
        for wd in 0..7 {
            let d = week * 7 + wd - first + 1;
            if d < 1 || d > days { r = r.child(div().size(px(32.))); continue; }
            let (today, on) = (month == 10 && d == 22, d == picked);
            r = r.child(div().size(px(32.)).flex().items_center().justify_center().child(div().id(("d", d as usize)).size(px(28.)).rounded_full().flex().items_center().justify_center().text_size(px(text_sm())).cursor_pointer()
                .when(on, |x| x.bg(color::primary()).text_color(color::primary_foreground()))
                .when(today && !on, |x| x.border_1().border_color(color::ring()).text_color(color::foreground()))
                .when(!on && !today, |x| x.text_color(color::foreground()).hover(|s| s.bg(color::accent())))
                .on_click(tap(cx, move |g, _, _| { g.pick.insert(("day", 0), d as usize); }))
                .child(format!("{d}"))));
        }
        cal = cal.child(r);
    }
    let date = Popover::new("date").w(px(248.)).py(px(10.)).flex().flex_col()
        .child(div().px(px(12.)).pb(px(6.)).flex().items_center().child(div().flex_1().text_size(px(text_sm())).text_color(color::foreground()).child(format!("2026年{}月", month + 1)))
            .child(Button::ghost("d-p").icon("chevron-left").sized(ButtonSize::Xs).on_click(tap(cx, |g, _, _| { let m = g.at(("month", 0)); g.pick.insert(("month", 0), (m + 11) % 12); })))
            .child(Button::ghost("d-n").icon("chevron-right").sized(ButtonSize::Xs).on_click(tap(cx, |g, _, _| { let m = g.at(("month", 0)); g.pick.insert(("month", 0), (m + 1) % 12); }))))
        .child(cal).child(menu_rule())
        .child(div().px(px(12.)).flex().justify_between()
            .child(ghost_btn("d-t", "", "今天").on_click(tap(cx, |g, _, _| { g.pick.insert(("month", 0), 10); g.pick.insert(("day", 0), 22); })))
            .child(ghost_btn("d-c", "", "清除").on_click(tap(cx, |g, _, _| { g.pick.insert(("day", 0), 0); }))));
    let q = g.inputs.links.read(cx).text.to_lowercase();
    let mut links = Popover::new("links").w(px(300.)).py(px(6.)).flex().flex_col()
        .child(div().px(px(12.)).pb(px(6.)).child(small("关联到「Authors」")))
        .child(div().px(px(8.)).pb(px(6.)).child(small_in(&g.inputs.links, "search", w, cx)));
    for (i, name) in ["Martin Kemp", "Andrea Carlino", "Lorraine Daston", "Jonathan Sawday"].iter().enumerate() {
        if !q.is_empty() && !name.to_lowercase().contains(&q) { continue; }
        links = links.child(list_row(("lk", i)).h(px(34.)).child(checkbox(g.is(("lk", i)))).child(t(name)).on_click(tap(cx, move |g, _, _| g.flip(("lk", i)))));
    }
    let edit = Popover::new("edit").w(px(260.)).py(px(6.)).flex().flex_col()
        .child(div().px(px(12.)).pb(px(4.)).child(small("选项名称")))
        .child(div().px(px(12.)).child(small_in(&g.inputs.option, "", w, cx)))
        .child(div().px(px(12.)).pt(px(10.)).child(small("颜色")))
        .child(div().px(px(12.)).py(px(8.)).child(swatches(g, ("es", 0), 20, 20., cx)))
        .child(menu_rule()).child(menu_item("e-del", "bin", "删除选项", true).on_click(says(cx, "删除选项")));
    let name = g.inputs.option.read(cx).text.clone();
    let live = row()
        .child(item("日期（点日期选中，翻月，今天 / 清除）", date))
        .child(item("关联记录（能搜，点行勾选）", links))
        .child(item("编辑选项（改名、换色）", div().flex().flex_col().gap(px(10.)).child(edit).child(div().flex().items_center().gap(px(6.)).child(small("效果：")).child(tag(if name.is_empty() { " " } else { &name }, g.at(("es", 0)) as u8)))));
    div().flex().flex_col().gap(px(32.)).child(group("可操作", live))
}

// ---- 编辑器工具条 ----

/// A toolbar as the editor's selection toolbar lays it out: T▾ | ≡▾ | six marks, A▾ | 更多.
fn text_toolbar(id: &'static str, pressed: [bool; 6], open: Option<usize>, live: Option<&Gallery>, cx: &mut Cx) -> crate::design::Toolbar {
    use crate::design::{ButtonGroup, ButtonGroupSeparator, Toggle, Toolbar};
    const MARKS: [&str; 6] = ["bold", "strikethrough", "italic", "underline", "link", "code"];
    let trigger = |k: usize, ic: &'static str| Button::ghost((id, k)).sized(ButtonSize::Sm).editor_icon(ic).icon_end("chevron-down").selected(open == Some(k));
    let turn = if live.is_some() { trigger(0, "type").on_click(tap(cx, |g, _, _| g.flip(("tb-turn", 0)))) } else { trigger(0, "type") };
    let mut bar = Toolbar::new(id).child(turn).child(ButtonGroupSeparator::new()).child(trigger(1, "align-left")).child(ButtonGroupSeparator::new());
    for (i, ic) in MARKS.iter().enumerate() {
        let k = ("tbm", i);
        let t = Toggle::new((id, 10 + i)).editor_icon(*ic).pressed(match live { Some(g) => g.is(k), None => pressed[i] });
        bar = bar.child(if live.is_some() && i != 4 { t.on_click(tap(cx, move |g, _, _| g.flip(k))) } else { t });
    }
    // the A shows the last colours (content): the editor's default, the light-yellow highlight
    let hl = crate::editor_theme::bg_color("light-yellow").unwrap_or(color::muted());
    let a = div().size(px(20.)).rounded(px(2.)).bg(hl).flex().items_center().justify_center().text_size(px(16.)).text_color(crate::theme::on_mark(color::foreground())).child("A");
    bar.child(ButtonGroup::new().child(Button::ghost((id, 20usize)).sized(ButtonSize::Sm).child(a)).child(Button::ghost((id, 21usize)).sized(ButtonSize::Sm).icon("chevron-down").selected(open == Some(2))))
        .child(ButtonGroupSeparator::new())
        .child(Button::ghost((id, 22usize)).sized(ButtonSize::Sm).editor_icon("layout-grid").selected(open == Some(3)))
}

pub fn toolbars(g: &mut Gallery, _w: &mut Window, cx: &mut Cx) -> Div {
    use crate::design::toolbar::block_handle;
    use crate::design::{ButtonGroup, Toggle, ToggleSize};
    let open = g.is(("tb-turn", 0)).then_some(0);
    let turn = text_toolbar("tbl", [false; 6], open, Some(g), cx);
    let turn = if open.is_some() {
        turn.overlay(div().absolute().top(px(44.)).left(px(4.)).child(Popover::new("tbl-menu").menu().w(px(200.))
            .child(MenuItem::new("tbl-m0").editor_icon("type").label("正文").checked(true).on_click(tap(cx, |g, _, _| g.flip(("tb-turn", 0)))))
            .child(MenuItem::new("tbl-m1").editor_icon("heading-1").label("一级标题").checked(false).on_click(tap(cx, |g, _, _| g.flip(("tb-turn", 0)))))))
    } else { turn };
    let toggles = div().flex().items_center().gap(px(4.)).children(["bold", "italic", "underline"].iter().enumerate().map(|(i, ic)| {
        let k = ("tgl", i);
        Toggle::new(k).editor_icon(*ic).pressed(g.is(k)).on_click(tap(cx, move |g, _, _| g.flip(k)))
    }));
    let handles = div().flex().items_center().gap(px(16.))
        .child(block_handle(900, Some(crate::editor_ui::icon("list-ordered", 12., crate::editor::overlays::kind_color(&crate::model::BlockKind::Bullet { dash: false }))), 42., g.is(("hdl", 0))).on_click(tap(cx, |g, _, _| g.flip(("hdl", 0)))))
        .child(block_handle(901, None, 28., g.is(("hdl", 1))).on_click(tap(cx, |g, _, _| g.flip(("hdl", 1)))));
    let live = row()
        .child(item("Toolbar（选中文字工具条的排法；T▾ 点一下展开，记号点一下切换）", div().pb(px(if open.is_some() { 96. } else { 0. })).child(turn)))
        .child(item("Toggle（sm）", toggles))
        .child(item("块手柄（点一下 = 菜单开着）", handles));
    let t = |id: &'static str| Toggle::new(id).editor_icon("bold");
    let still = row()
        .child(item("Toggle sm · 关 / 开 / 不可用 / 开且不可用", div().flex().gap(px(8.)).child(t("ts0")).child(t("ts1").pressed(true)).child(t("ts2").disabled(true)).child(t("ts3").pressed(true).disabled(true))))
        .child(item("Toggle default / lg · 带文字", div().flex().items_center().gap(px(8.)).child(t("td0").sized(ToggleSize::Default).pressed(true)).child(t("tl0").sized(ToggleSize::Lg)).child(Toggle::new("tlb").editor_icon("italic").label("斜体").sized(ToggleSize::Default))))
        .child(item("Toggle outline · 关 / 开", div().flex().gap(px(8.)).child(t("to0").outline()).child(t("to1").outline().pressed(true))))
        .child(item("ButtonGroup · outline / ghost 分体按钮", div().flex().gap(px(16.))
            .child(ButtonGroup::new().child(Button::outline("bg0").sized(ButtonSize::Sm).label("合并")).child(Button::outline("bg1").sized(ButtonSize::Sm).icon("chevron-down")))
            .child(ButtonGroup::new().child(Button::ghost("bg2").sized(ButtonSize::Sm).label("应用")).child(Button::ghost("bg3").sized(ButtonSize::Sm).icon("chevron-down").selected(true)))))
        .child(item("工具条 · 粗体开着", text_toolbar("tbs0", [true, false, false, false, false, false], None, None, cx)))
        .child(item("工具条 · T▾ 展开", text_toolbar("tbs1", [false; 6], Some(0), None, cx)))
        .child(item("工具条 · A▾ 展开", text_toolbar("tbs2", [false; 6], Some(2), None, cx)))
        .child(item("块手柄 · 普通 / 菜单开着 / 空行 + / + 菜单开着", div().flex().items_center().gap(px(16.))
            .child(block_handle(910, Some(crate::editor_ui::icon("list", 12., crate::editor::overlays::kind_color(&crate::model::BlockKind::Bullet { dash: false }))), 42., false))
            .child(block_handle(911, Some(crate::editor_ui::icon("list", 12., crate::editor::overlays::kind_color(&crate::model::BlockKind::Bullet { dash: false }))), 42., true))
            .child(block_handle(912, None, 28., false)).child(block_handle(913, None, 28., true))));
    two(live, still)
}
