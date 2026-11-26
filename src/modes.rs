//! The header's 编辑 / 阅读 mode switch, the 快捷键 drawer (⌘ + /) and the
//! "?" help button at the bottom right of the page that opens it.

use crate::app::Workspace;
use crate::theme::Theme;
use crate::design::{MenuItem, Popover};
use crate::ui::{TipExt, TitleBarExt, ToastKind};
use gpui::{prelude::FluentBuilder, *};

actions!(workspace, [EditMode, ReadMode, ToggleShortcuts]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-alt-e", EditMode, None),
        KeyBinding::new("ctrl-alt-v", ReadMode, None),
        KeyBinding::new("cmd-/", ToggleShortcuts, None),
        KeyBinding::new("cmd-alt-n", crate::app::NewDoc, None),
    ]);
}

/// The mode button: 85×32, 16px left of "…" (whose right edge is
/// `MENU_RIGHT` from the window's).
const DRAWER_W: f32 = 314.;

/// 快捷键 rows: icon, description, keys; `None` separates groups.
type Row = Option<(&'static str, &'static str, &'static str)>;

const STANDARD: &[Row] = &[
    // 复制, 剪切, 粘贴 and 全选 are the live rows above (with the user's keys), not repeated here.
    Some(("keyboard", "阅读模式切回编辑", "E")), Some(("keyboard", "逐字扩选", "⇧← / ⇧→")), Some(("keyboard", "逐行扩选", "⇧↑ / ⇧↓")),
    Some(("keyboard", "选到行首 / 行尾", "⌘⇧← / ⌘⇧→")), Some(("keyboard", "选到文档开头 / 结尾", "⌘⇧↑ / ⌘⇧↓")),
];
const MARKDOWN: &[Row] = &[
    Some(("heading-1", "一级标题", "# 空格")),
    Some(("heading-2", "二级标题", "## 空格")),
    Some(("heading-3", "三级标题", "### 空格")),
    Some(("list-ordered", "有序列表", "1. 空格")),
    Some(("list", "无序列表", "- 空格 或 * 空格")),
    Some(("list-todo", "任务列表", "[] 空格")),
    Some(("indent-increase", "增加缩进", "Tab")),
    Some(("indent-decrease", "减少缩进", "Shift + Tab")),
    Some(("quote", "引用", "> 空格")),
    Some(("code", "代码", "`代码`")),
    Some(("braces", "代码块", "``` 空格 或 ``` 代码语言 空格")),
    None,
    Some(("bold", "加粗", "**文字** 空格")),
    Some(("italic", "斜体", "*文字* 空格")),
    Some(("underline", "下划线", "~文字~ 空格")),
    Some(("strikethrough", "删除线", "~~文字~~ 空格")),
    Some(("sigma", "公式", "$$文字$$")),
    Some(("divider", "分隔线", "--- 或 ***")),
];

fn rows(list: &[Row]) -> Vec<AnyElement> {
    list.iter()
        .map(|r| match r {
            None => div().h(px(12.)).into_any_element(),
            Some((ic, desc, keys)) => row(crate::settings::text(desc), (*ic != "keyboard").then_some(*ic), seq(crate::shortcuts::keys(crate::settings::text(keys)))),
        })
        .collect()
}

/// One line: shadcn's small `Item` without its frame — an optional muted glyph, the action in
/// `text-sm`, its keys on the right.
fn row(title: impl Into<SharedString>, glyph: Option<&'static str>, keys: AnyElement) -> AnyElement {
    let g = crate::design::geo();
    div().min_h(px(32.)).py(px(4.)).flex().items_center().gap(px(g.menu_gap))
        .children(glyph.map(|ic| div().flex_none().child(crate::design::icon(ic, 16., crate::design::color::muted_foreground()))))
        .child(div().flex_1().min_w_0().text_size(px(g.text)).line_height(px(crate::design::text::SM_LINE)).text_color(crate::design::color::foreground()).child(title.into()))
        .child(div().flex_none().max_w(px(150.)).flex().flex_wrap().justify_end().items_center().gap(px(4.)).child(keys))
        .into_any_element()
}

/// A sequence (`# 空格`, `- 空格 或 * 空格`, `⌘C / ⌘X / ⌘V`): each way to type it a `Kbd`, the
/// language's "or" (as the translated string has it) or the / between. The 键盘快捷键 rows
/// go without the keyboard glyph, as the ones above them.
fn seq(keys: &'static str) -> AnyElement {
    const OR: &[&str] = &[" / ", " 或 ", " or ", " o ", " ou ", " または ", " 또는 ", " veya ", " أو "];
    let sep = OR.iter().find(|o| keys.contains(**o)).copied();
    let parts: Vec<&str> = match sep { Some(o) => keys.split(o).collect(), None => vec![keys] };
    let mut d = div().flex().flex_wrap().justify_end().items_center().gap(px(4.));
    for (i, k) in parts.into_iter().enumerate() {
        if i > 0 { d = d.child(div().text_size(px(crate::design::geo().text_xs)).text_color(crate::design::color::muted_foreground()).child(sep.unwrap_or("").trim())); }
        d = d.child(crate::design::Kbd::new(k));
    }
    d.into_any_element()
}

fn caps(key: &str) -> AnyElement {
    if key.is_empty() { return div().text_size(px(crate::design::geo().text_xs)).text_color(crate::design::color::muted_foreground()).child(crate::shortcuts::pretty(key)).into_any_element() }
    crate::design::KbdGroup::new(crate::shortcuts::strokes(key)).into_any_element()
}

/// A section's heading: a group label (`text-xs`, `muted-foreground`, medium), as a menu's.
fn section(title: &'static str) -> Div {
    let g = crate::design::geo();
    div().pb(px(4.)).text_size(px(g.text_xs)).line_height(px(crate::design::text::XS_LINE)).font_weight(FontWeight::SEMIBOLD).text_color(crate::design::color::muted_foreground()).child(title)
}

impl Workspace {
    /// The open document is in 阅读模式.
    pub fn reading(&self) -> bool {
        self.current.as_deref().is_some_and(|id| self.reading_document(id))
    }

    pub(crate) fn reading_document(&self, id: &str) -> bool {
        if self.is_shared() { return self.store.settings().shared_reading.contains(id); }
        self.entries.iter().find(|e| e.id == id).is_some_and(|e| e.meta.reading)
    }

    pub fn set_mode(&mut self, reading: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.set_mode_with_focus(reading, true, window, cx);
    }

    #[cfg(target_os = "ios")]
    pub(crate) fn set_mode_from_navigation(&mut self, reading: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.set_mode_with_focus(reading, false, window, cx);
    }

    fn set_mode_with_focus(&mut self, reading: bool, focus_input: bool, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if self.is_shared() && (crate::cloud_sync::shared::editing_paused(&self.store, cx)
            || !crate::cloud_sync::shared::access(&self.store, cx).is_some_and(|access| access.can_write())) { return; }
        self.mode_menu = false;
        cx.notify();
        let Some(id) = self.current.clone() else { return };
        if self.reading() == reading {
            return;
        }
        // The snapshot reading mode keeps must be what is on disk.
        if !self.save_before(cx, move |w, window, cx| w.set_mode_with_focus(reading, focus_input, window, cx)) {
            return;
        }
        if self.is_shared() {
            let mut settings = self.store.settings();
            if reading { settings.shared_reading.insert(id); }
            else { settings.shared_reading.remove(&id); }
            if self.store.save_settings(&settings).is_err() {
                self.show_toast_kind(ToastKind::Error, crate::settings::tr("无法保存阅读设置", "Could not save reading preference"), cx);
                return;
            }
        } else {
            if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) { e.meta.reading = reading; }
            let _ = self.store.update_meta(&id, |m| m.reading = reading);
        }
        if let Some(ed) = &self.editor {
            ed.update(cx, |e, cx| e.set_read_only(reading, cx));
            if focus_input {
                crate::gpui_compat::focus(window, &ed.read(cx).focus_handle(cx), cx);
                #[cfg(target_os = "ios")]
                if !reading { gpui_mobile::show_keyboard(); }
            }
        }
        self.show_toast_kind(ToastKind::Info, if reading { crate::settings::text("已切换为阅读模式") } else { crate::settings::text("已切换为编辑模式") }, cx);
    }

    pub fn set_info_row(&mut self, info: crate::store::InfoRow) {
        let Some(id) = self.current.clone() else { return };
        if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
            e.meta.info = info;
        }
        let _ = self.store.update_meta(&id, |m| m.info = info);
    }

    pub(crate) fn document_mode_button(&self, window: &Window, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        if !crate::FEI_NOTES || self.current.is_none() || self.preview.is_some() { return None; }
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "cloudkit-sync"))]
        if self.is_shared() && (crate::cloud_sync::shared::editing_paused(&self.store, cx)
            || !crate::cloud_sync::shared::access(&self.store, cx).is_some_and(|access| access.can_write())) { return None; }
        let bounds = self.mode_btn_bounds.clone();
        let (glyph, text) = if self.reading() { ("book-open", crate::settings::tr("阅读", "Read")) }
            else { ("pen-line", crate::settings::tr("编辑", "Edit")) };
        let labels = self.header_action_labels(window);
        let size = px(crate::app::HEADER_ACTIONS_H);
        Some(crate::ui::grouped_toolbar_control(div().id("doc-mode").no_drag().relative().flex_none()
            .h(size).min_w(size).flex().items_center().justify_center().gap(px(5.)).cursor_pointer(),
            8., Theme::fill_hover, Some(Theme::fill_pressed))
            .text_size(px(14.)).line_height(px(20.)).font_weight(gpui::FontWeight::NORMAL)
            .child(crate::ui::toolbar_icon(glyph, 16.))
            .when(labels, |d| d.px(px(7.)).child(text)
                .child(crate::ui::icon("chevron-down", 12., Theme::icon())))
            .when(!labels && !self.mode_menu, |d| d.tip(text, None))
            .child(canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {}).absolute().inset_0())
            .on_click(cx.listener(|w, _, _, cx| {
                w.doc_menu = false;
                w.mode_menu = !w.mode_menu;
                cx.notify();
            })))
    }

    /// The mode menu: a native menu with two-line rows (name over a
    /// description); hovering paints the accent, the current mode carries a check.
    pub(crate) fn render_mode_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.mode_menu || self.current.is_none() {
            return None;
        }
        let at = self.mode_btn_bounds.get()?;
        let reading = self.reading();
        let item = |id: &'static str, ic: &'static str, title: &'static str, desc: &'static str, selected: bool, to_reading: bool| {
            MenuItem::new(id).icon(ic).label(crate::settings::label(title)).description(crate::settings::label(desc)).checked(selected)
                .on_click(cx.listener(move |w, _, window, cx| w.set_mode(to_reading, window, cx)))
        };
        let btn = self.mode_btn_bounds.clone();
        let menu = Popover::new("doc-mode-menu")
            .menu()
            .w(px(200.))
            .on_mouse_down_out(cx.listener(move |w, ev: &MouseDownEvent, _, cx| {
                // The button toggles the menu itself.
                if btn.get().is_none_or(|b| !b.contains(&ev.position)) {
                    w.mode_menu = false;
                    cx.notify();
                }
            }))
            .child(item("mode-edit", "pen-line", crate::settings::text("编辑"), crate::settings::text("可编辑文档"), !reading, false))
            .child(item("mode-read", "book-open", crate::settings::text("阅读"), crate::settings::tr("仅查看文档", "View this document"), reading, true));
        Some(deferred(anchored().position(point(at.left(), at.bottom() + px(4.))).snap_to_window_with_margin(px(8.)).child(menu)).with_priority(2).into_any_element())
    }

    /// The 快捷键 drawer to the right of the page, below the top bar: shadcn's `Sheet` (side
    /// right) — `background`, a `border` on its open side, the title in `font-semibold` beside a
    /// ghost ✕, then the keys in labelled groups split by a `Separator`.
    pub(crate) fn render_shortcuts(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.shortcuts {
            return None;
        }
        let g = crate::design::geo();
        Some(
            div()
                .id("shortcuts")
                .flex_none()
                .w(px(DRAWER_W))
                .h_full()
                .flex()
                .flex_col()
                .border_l_1()
                .border_color(crate::design::color::border())
                .bg(crate::design::color::background())
                .child(
                    div()
                        .flex_none()
                        .h(px(52.))
                        .pl(px(16.))
                        .pr(px(10.))
                        .flex()
                        .items_center()
                        .child(div().flex_1().text_size(px(16.)).line_height(px(24.)).font_weight(FontWeight::SEMIBOLD).text_color(crate::design::color::foreground()).child(crate::settings::tr("快捷键", "Keyboard shortcuts")))
                        .child(crate::design::Button::ghost("shortcuts-close").sized(crate::design::ButtonSize::Sm).icon("x").on_click(cx.listener(|w, _, _, cx| {
                            w.shortcuts = false;
                            cx.notify();
                        }))),
                )
                .child(
                    div()
                        .id("shortcuts-body")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .pt(px(4.))
                        .px(px(16.))
                        .pb(px(24.))
                        .child(section(crate::settings::text("键盘快捷键")))
                        .children(crate::shortcuts::rows(cx).into_iter().map(|(_, title, key)| row(title, None, caps(&key))))
                        .children(rows(STANDARD))
                        .child(crate::design::Separator::new().my(px(16.)))
                        .child(section("Markdown"))
                        .child(
                            div()
                                .pb(px(8.))
                                .text_size(px(g.text_xs))
                                .line_height(px(crate::design::text::XS_LINE + 2.))
                                .text_color(crate::design::color::muted_foreground())
                                .child(crate::settings::text("Markdown 是一种快速输入带格式文本的方式，让你书写文档时行云流水，一气呵成，例如输入#空格，即可输入一级标题。")),
                        )
                        .children(rows(MARKDOWN)),
                )
                .into_any_element(),
        )
    }

}
