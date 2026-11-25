//! 组件库查看面板: every component of the design system (`crate::design`, docs/design/COMPONENTS.md),
//! live, one page each, like Storybook: the pages down the left, the open page's pieces on the right —
//! first the ones to use (they click, toggle, type, open and close), then still states (hovered, open,
//! disabled) to compare against. The bar on top switches 浅色 / 深色; 浅深对照 lays the page out
//! light and dark side by side.
//!
//! Its own launch mode, not a window of the app: `quiet_md --components` (or `QUIET_MD_COMPONENTS=1`,
//! which UI scripts use; scripts/components.sh builds and opens it) opens this window alone and
//! never opens, reads or sets up a library.

mod pages;

use crate::design::{Button, ButtonSize, Segment, Segmented, Toast, ToastTone, color, icon};
use crate::input::{TextField, TextFieldEvent};
use crate::table::style::*;
use gpui::prelude::FluentBuilder;
use gpui::*;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};

/// Is this launch the panel's?
pub fn wanted() -> bool { std::env::args().any(|a| a == "--components") || std::env::var_os("QUIET_MD_COMPONENTS").is_some() }

/// The panel's launch: field keys, Chinese labels and the bundled theme (never the library's
/// preferences), then its window.
pub fn run() {
    let app = gpui_platform::application().with_assets(crate::assets::Assets);
    app.run(|cx: &mut App| {
        cx.set_cursor_hide_mode(CursorHideMode::Never);
        crate::input::bind_keys(cx);
        crate::settings::show_lang(crate::settings::Lang::Zh);
        crate::themes::load(&crate::settings::prefs().theme_id(), false, || cx.text_system().all_font_names());
        open(cx);
        cx.on_window_closed(|cx, _| if cx.windows().is_empty() { cx.quit() }).detach();
        if !crate::devtools::scripted() { cx.activate(true); }
    });
}

fn open(cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1360.), px(900.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions { title: Some("Quiet.md 组件库".into()), ..Default::default() }),
        window_min_size: Some(size(px(720.), px(480.))),
        focus: !crate::devtools::scripted(),
        ..Default::default()
    };
    let _ = cx.open_window(options, |window, cx| { crate::devtools::start(window, cx); cx.new(|cx| Gallery::new(window, cx)) });
}

/// A piece of state: its name and, for pieces in rows, which one.
pub(crate) type K = (&'static str, usize);

/// The pages: name, what is on it, how to draw it.
type Page = fn(&mut Gallery, &mut Window, &mut Context<Gallery>) -> Div;
const PAGES: [(&str, &str, Page); 13] = [
    ("按钮", "Button：六种变体、五种尺寸、图标按钮、工具按钮", pages::buttons),
    ("输入", "Field、Select、Textarea、Label", pages::fields),
    ("选择控件", "Checkbox、Radio、Switch、Segmented、颜色", pages::toggles),
    ("标签", "Badge、Kbd、KbdGroup、关联、人员", pages::badges),
    ("菜单", "Popover、MenuItem、MenuLabel、MenuSeparator；应用和数据表的菜单", pages::menus),
    ("对话框", "Dialog；数据表的新增 / 修改字段", pages::dialogs),
    ("卡片", "Card、Item、Separator、Toast", pages::cards),
    ("面板", "数据表的筛选、排序、字段配置、填色", pages::panels),
    ("工具栏", "视图工具栏、勾选操作条、表内搜索", pages::bars),
    ("表格", "表头、行、勾选、选区、填色、分组、添加记录、统计", pages::grid),
    ("记录详情", "展开一条记录", pages::record),
    ("单元格编辑器", "日期、关联、编辑选项", pages::editors),
    ("编辑器工具条", "Toggle、ButtonGroup、Toolbar；选中文字工具条、块手柄", pages::toolbars),
];

/// The text inputs the pages type into.
pub(crate) struct Inputs {
    pub name: Entity<TextField>, pub search: Entity<TextField>, pub area: Entity<TextField>, pub menu_search: Entity<TextField>,
    pub title: Entity<TextField>, pub value: Entity<TextField>, pub find: Entity<TextField>, pub pages: Entity<TextField>,
    pub links: Entity<TextField>, pub option: Entity<TextField>, pub rename: Entity<TextField>,
}
impl Inputs {
    fn all(&self) -> [&Entity<TextField>; 11] { [&self.name, &self.search, &self.area, &self.menu_search, &self.title, &self.value, &self.find, &self.pages, &self.links, &self.option, &self.rename] }
}

pub struct Gallery {
    focus: FocusHandle,
    page: usize,
    /// 浅深对照: the page light and dark side by side.
    pair: bool,
    pub(crate) clicks: usize,
    toast: Option<(String, usize)>,
    /// Switched on (checkboxes, switches, rows picked…).
    pub(crate) on: HashSet<K>,
    /// Picked of several (segments, selects, swatches, days…).
    pub(crate) pick: HashMap<K, usize>,
    /// The open menu or dropdown.
    pub(crate) open: Option<K>,
    /// The open dialog.
    pub(crate) dialog: Option<&'static str>,
    pub(crate) inputs: Inputs,
    /// The grid's active cell and range anchor (row, column).
    pub(crate) cell: (usize, usize),
    pub(crate) anchor: (usize, usize),
    /// Rows a list holds (filter conditions, sort lines, form options, grid records…).
    pub(crate) rows: HashMap<&'static str, Vec<usize>>,
}

impl Gallery {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = |hint: &str, text: &str, cx: &mut Context<Self>| {
            let e = cx.new(|cx| { let mut t = TextField::new(hint.to_string(), cx); t.size = text_sm(); t.set_text(text, cx); t });
            cx.subscribe_in(&e, window, |g: &mut Gallery, _, ev: &TextFieldEvent, _, cx| { if let TextFieldEvent::Cancel = ev { g.open = None; g.dialog = None; } cx.notify(); }).detach();
            e
        };
        let inputs = Inputs {
            name: field("请输入标题", "", cx), search: field("查找或创建选项", "", cx), area: field("", "IF([Pages] > 300, \"long\", \"short\")", cx),
            menu_search: field("查找或创建选项", "", cx), title: field("字段名称", "Status", cx), value: field("值", "300", cx), find: field("搜索", "body", cx),
            pages: field("", "336", cx), links: field("搜索记录", "", cx), option: field("选项名称", "Reading", cx), rename: field("文档标题", "周报 · 第 47 周", cx),
        };
        inputs.area.update(cx, |t, _| { t.multiline = true; t.wrap = true; });
        let on = [("chk", 1), ("sw", 1), ("own", 0), ("lk", 0), ("lk", 2), ("fl", 1), ("fl", 2), ("fl", 4), ("rowchk", 0), ("sort", 0), ("sort", 3)].into_iter().collect();
        let pick = [(("seg", 0), 0), (("seg", 1), 1), (("sw-c", 0), 2), (("type", 0), 1), (("type", 1), 1), (("opt", 0), 1), (("op", 0), 2), (("day", 0), 12), (("month", 0), 10),
            (("es", 0), 0), (("pk", 0), 2), (("rec", 0), 1), (("pf-f", 0), 2), (("pf-f", 1), 3), (("pf-o", 1), 1), (("ps-f", 1), 1), (("ps-s", 0), 1), (("fld", 0), 2), (("mode", 0), 0)].into_iter().collect();
        let rows = [("filter", vec![0, 1]), ("sort", vec![0, 1]), ("opts", vec![9, 0]), ("chips", vec![0, 1, 2]), ("grid", vec![0, 1, 2, 3])].into_iter().collect();
        Gallery { focus: cx.focus_handle(), page: 0, pair: false, clicks: 0, toast: None, on, pick, open: None, dialog: None, inputs, cell: (1, 0), anchor: (1, 1), rows }
    }

    /// Show `msg` at the bottom for a moment (what a click did).
    pub(crate) fn say(&mut self, msg: impl Into<String>, cx: &mut Context<Self>) {
        self.clicks += 1;
        let n = self.clicks;
        self.toast = Some((msg.into(), n));
        cx.spawn(async move |g, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(1800)).await;
            let _ = g.update(cx, |g, cx| if g.toast.as_ref().is_some_and(|t| t.1 == n) { g.toast = None; cx.notify(); });
        }).detach();
        cx.notify();
    }
    pub(crate) fn is(&self, k: K) -> bool { self.on.contains(&k) }
    pub(crate) fn flip(&mut self, k: K) { if !self.on.remove(&k) { self.on.insert(k); } }
    pub(crate) fn at(&self, k: K) -> usize { self.pick.get(&k).copied().unwrap_or(0) }
}

// ---- helpers the pages share ----

/// A click handler that runs `f` and redraws.
pub(crate) fn tap<E: 'static>(cx: &mut Context<Gallery>, f: impl Fn(&mut Gallery, &mut Window, &mut Context<Gallery>) + 'static) -> impl Fn(&E, &mut Window, &mut App) + 'static {
    cx.listener(move |g, _: &E, window, cx| { f(g, window, cx); cx.notify(); })
}
/// A click handler that only says what was clicked.
pub(crate) fn says<E: 'static>(cx: &mut Context<Gallery>, msg: &'static str) -> impl Fn(&E, &mut Window, &mut App) + 'static {
    cx.listener(move |g, _: &E, _, cx| g.say(format!("点了「{msg}」"), cx))
}

pub(crate) fn t(s: &str) -> Div { div().text_size(px(text_sm())).text_color(color::foreground()).whitespace_nowrap().child(s.to_string()) }
pub(crate) fn faint(s: &str) -> Div { div().text_size(px(text_sm())).text_color(color::placeholder()).whitespace_nowrap().child(s.to_string()) }
pub(crate) fn small(s: &str) -> Div { div().text_size(px(text_xs())).text_color(color::muted_foreground()).whitespace_nowrap().child(s.to_string()) }
/// A labelled piece: the thing, its caption under it.
pub(crate) fn item(caption: &str, el: impl IntoElement) -> Div {
    div().flex().flex_col().items_start().gap(px(8.)).child(el).child(div().text_size(px(11.)).text_color(color::muted_foreground()).child(caption.to_string()))
}
pub(crate) fn row() -> Div { div().flex().flex_wrap().items_start().gap(px(24.)) }
/// A group of a page: 可操作 or 状态对照, then its pieces.
pub(crate) fn group(name: &str, body: Div) -> Div {
    div().flex().flex_col().gap(px(14.)).child(div().text_size(px(12.)).text_color(color::muted_foreground()).child(name.to_string())).child(body)
}

thread_local! {
    /// Drawing 浅深对照's second copy: an input draws in one place only, so there it shows its text.
    static MIRROR: Cell<bool> = const { Cell::new(false) };
}

/// Whether `e` has the keyboard.
pub(crate) fn typing(e: &Entity<TextField>, window: &Window, cx: &App) -> bool { !MIRROR.with(|m| m.get()) && e.read(cx).is_focused(window) }
/// An id for the box round `e`.
pub(crate) fn box_id(e: &Entity<TextField>) -> (&'static str, usize) { ("in", e.entity_id().as_u64() as usize) }
/// The text input `e` in `base` (a `Field` or `Textarea`): clicking the box types in it.
pub(crate) fn text_in<B: ParentElement + InteractiveElement + Styled>(e: &Entity<TextField>, base: B, cx: &App) -> B {
    if MIRROR.with(|m| m.get()) { return base.child(t(&e.read(cx).text)); }
    let h = e.read(cx).focus_handle(cx);
    base.cursor_text().on_mouse_down(MouseButton::Left, move |_, window, cx| window.focus(&h, cx)).child(div().flex_1().min_w_0().overflow_hidden().child(e.clone()))
}
/// A form-height input.
pub(crate) fn field_in(e: &Entity<TextField>, window: &Window, cx: &App) -> crate::design::Field {
    text_in(e, field_box(box_id(e)).focused(typing(e, window, cx)), cx)
}
/// A small input (menus, panels), `glyph` before it.
pub(crate) fn small_in(e: &Entity<TextField>, glyph: &str, window: &Window, cx: &App) -> crate::design::Field {
    let base = input_box(box_id(e)).focused(typing(e, window, cx)).gap(px(6.)).when(!glyph.is_empty(), |d| d.child(icon(glyph, 14., color::placeholder())));
    text_in(e, base, cx)
}

/// `trigger` opening `menu` under it while `k` is the open one: a click on it opens or closes, a pick
/// in the menu or a click anywhere else closes.
pub(crate) fn drop<T: StatefulInteractiveElement + IntoElement + 'static>(g: &Gallery, k: K, trigger: T, menu: impl IntoElement, cx: &mut Context<Gallery>) -> Div {
    let open = g.open == Some(k);
    div().relative().child(trigger.on_click(tap(cx, move |g, _, _| g.open = if g.open == Some(k) { None } else { Some(k) })))
        .when(open, |d| d.child(div().absolute().left_0().top_full().child(deferred(anchored().snap_to_window_with_margin(px(8.)).child(div().pt(px(4.)).child(menu))).with_priority(2))))
}

/// Lays its child out and paints it in one look (`theme::with_dark`): components build themselves
/// when laid out, after the column that holds them was built.
struct Scheme { dark: bool, child: AnyElement }

impl Element for Scheme {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> { None }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> { None }
    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        (crate::theme::with_dark(self.dark, || self.child.request_layout(window, cx)), ())
    }
    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        crate::theme::with_dark(self.dark, || { self.child.prepaint(window, cx); });
    }
    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        crate::theme::with_dark(self.dark, || self.child.paint(window, cx));
    }
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // the inputs' colours follow the theme
        for e in self.inputs.all() { e.update(cx, |t, _| { t.color = color::foreground(); t.placeholder_color = color::placeholder(); }); }
        let dark = crate::theme::dark();
        fn refresh(window: &mut Window, cx: &mut App) { crate::themes::touch(); window.refresh(); cx.refresh_windows(); }
        let look = if self.pair { 2 } else if dark { 1 } else { 0 };
        let seg = |id: &'static str, s: &'static str, i: usize| Segment::new(id, s).selected(look == i);
        let bar = div().flex_none().h(px(52.)).pl(px(84.)).pr(px(16.)).flex().items_center().justify_between().border_b_1().border_color(color::border())
            .child(div().flex().items_baseline().gap(px(10.))
                .child(div().text_size(px(15.)).text_color(color::foreground()).child("组件库"))
                .child(div().text_size(px(12.)).text_color(color::muted_foreground()).child(format!("点了 {} 次", self.clicks))))
            .child(Segmented::new()
                .child(seg("light", "浅色", 0).on_click(cx.listener(|g, _, window, cx| { g.pair = false; crate::theme::set_dark(false); refresh(window, cx); })))
                .child(seg("dark", "深色", 1).on_click(cx.listener(|g, _, window, cx| { g.pair = false; crate::theme::set_dark(true); refresh(window, cx); })))
                .child(seg("pair", "浅深对照", 2).on_click(cx.listener(|g, _, window, cx| { g.pair = true; refresh(window, cx); }))));
        let mut nav = div().id("nav").w(px(200.)).flex_none().h_full().overflow_y_scroll().py(px(12.)).px(px(8.)).flex().flex_col().gap(px(2.)).border_r_1().border_color(color::border());
        for (i, (name, _, _)) in PAGES.iter().enumerate() {
            nav = nav.child(Button::ghost(("page", i)).sized(ButtonSize::Sm).w_full().justify_start().label(*name).selected(i == self.page)
                .on_click(cx.listener(move |g, _, _, cx| { g.page = i; g.open = None; cx.notify(); })));
        }
        let (name, note, page) = PAGES[self.page];
        let body = if self.pair {
            let col = |g: &mut Gallery, dark: bool, window: &mut Window, cx: &mut Context<Gallery>| {
                MIRROR.with(|m| m.set(dark));
                let c = crate::theme::with_dark(dark, || div().id(if dark { "dark-col" } else { "light-col" }).flex_1().min_w_0().p(px(24.)).bg(color::background()).flex().flex_col().gap(px(16.))
                    .child(div().text_size(px(13.)).text_color(color::muted_foreground()).child(if dark { "深色" } else { "浅色" }))
                    .child(page(g, window, cx)).into_any_element());
                MIRROR.with(|m| m.set(false));
                Scheme { dark, child: c }
            };
            let light = col(self, false, window, cx);
            let dark = col(self, true, window, cx);
            div().flex().child(light).child(dark)
        } else { div().px(px(32.)).flex().child(page(self, window, cx).flex_1().min_w_0()) };
        let content = div().id("content").flex_1().min_w_0().h_full().overflow_y_scroll().child(div().pt(px(24.)).pb(px(80.)).flex().flex_col().gap(px(24.))
            .child(div().px(px(32.)).flex().flex_col().gap(px(4.))
                .child(div().text_size(px(20.)).text_color(color::foreground()).child(name))
                .child(div().text_size(px(13.)).text_color(color::muted_foreground()).child(note)))
            .child(body));
        let toast = self.toast.as_ref().map(|(s, _)| div().absolute().bottom(px(24.)).left_0().right_0().flex().justify_center().child(Toast::new(ToastTone::Success, s.clone())));
        div().track_focus(&self.focus).key_context("Gallery").relative().size_full().flex().flex_col().bg(color::background()).font_family(crate::theme::ui_family()).text_color(color::foreground())
            .on_key_down(cx.listener(|g, e: &KeyDownEvent, _, cx| if e.keystroke.key == "escape" { g.open = None; g.dialog = None; cx.notify(); }))
            .child(bar)
            .child(div().flex_1().min_h_0().flex().child(nav).child(content))
            // a click outside an open menu closes it (the menu draws above this, deferred)
            .when(self.open.is_some(), |d| d.child(div().id("backdrop").absolute().inset_0().occlude().on_mouse_down(MouseButton::Left, cx.listener(|g, _, _, cx| { g.open = None; cx.notify(); }))))
            .when_some(self.dialog, |d, which| d.child(pages::dialog(self, which, window, cx)))
            .children(toast)
    }
}
